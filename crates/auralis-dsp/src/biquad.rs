use std::sync::atomic::{AtomicPtr, Ordering};
use auralis_core::types::FilterType;
use serde::{Deserialize, Serialize};

/// Biquad coefficients: b0, b1, b2, a1, a2
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BiquadCoefficients {
    pub b0: f32,
    pub b1: f32,
    pub b2: f32,
    pub a1: f32,
    pub a2: f32,
}

impl Default for BiquadCoefficients {
    fn default() -> Self {
        // Identity filter (passes audio unchanged)
        Self {
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
        }
    }
}

impl BiquadCoefficients {
    /// Calculate coefficients for a given filter type.
    pub fn calculate(filter: FilterType, sample_rate: f32, freq: f32, q: f32, gain_db: f32) -> Self {
        // Prevent denormals / div by zero
        let q = q.max(0.01);
        let freq = freq.clamp(20.0, sample_rate / 2.0 * 0.99); // max at nyquist
        
        let a = 10.0f32.powf(gain_db / 40.0);
        let w0 = 2.0 * std::f32::consts::PI * freq / sample_rate;
        let alpha = w0.sin() / (2.0 * q);
        let cos_w0 = w0.cos();

        match filter {
            FilterType::Peaking => {
                let a_factor = alpha * a;
                let a_div = alpha / a;
                let b0 = 1.0 + a_factor;
                let b1 = -2.0 * cos_w0;
                let b2 = 1.0 - a_factor;
                let a0 = 1.0 + a_div;
                let a1 = -2.0 * cos_w0;
                let a2 = 1.0 - a_div;
                Self::normalize(b0, b1, b2, a0, a1, a2)
            }
            FilterType::HighPass => {
                let b0 = (1.0 + cos_w0) / 2.0;
                let b1 = -(1.0 + cos_w0);
                let b2 = (1.0 + cos_w0) / 2.0;
                let a0 = 1.0 + alpha;
                let a1 = -2.0 * cos_w0;
                let a2 = 1.0 - alpha;
                Self::normalize(b0, b1, b2, a0, a1, a2)
            }
            FilterType::LowPass => {
                let b0 = (1.0 - cos_w0) / 2.0;
                let b1 = 1.0 - cos_w0;
                let b2 = (1.0 - cos_w0) / 2.0;
                let a0 = 1.0 + alpha;
                let a1 = -2.0 * cos_w0;
                let a2 = 1.0 - alpha;
                Self::normalize(b0, b1, b2, a0, a1, a2)
            }
            FilterType::LowShelf => {
                let sqrt_a = a.sqrt();
                let b0 = a * ((a + 1.0) - (a - 1.0) * cos_w0 + 2.0 * sqrt_a * alpha);
                let b1 = 2.0 * a * ((a - 1.0) - (a + 1.0) * cos_w0);
                let b2 = a * ((a + 1.0) - (a - 1.0) * cos_w0 - 2.0 * sqrt_a * alpha);
                let a0 = (a + 1.0) + (a - 1.0) * cos_w0 + 2.0 * sqrt_a * alpha;
                let a1 = -2.0 * ((a - 1.0) + (a + 1.0) * cos_w0);
                let a2 = (a + 1.0) + (a - 1.0) * cos_w0 - 2.0 * sqrt_a * alpha;
                Self::normalize(b0, b1, b2, a0, a1, a2)
            }
            FilterType::HighShelf => {
                let sqrt_a = a.sqrt();
                let b0 = a * ((a + 1.0) + (a - 1.0) * cos_w0 + 2.0 * sqrt_a * alpha);
                let b1 = -2.0 * a * ((a - 1.0) + (a + 1.0) * cos_w0);
                let b2 = a * ((a + 1.0) + (a - 1.0) * cos_w0 - 2.0 * sqrt_a * alpha);
                let a0 = (a + 1.0) - (a - 1.0) * cos_w0 + 2.0 * sqrt_a * alpha;
                let a1 = 2.0 * ((a - 1.0) - (a + 1.0) * cos_w0);
                let a2 = (a + 1.0) - (a - 1.0) * cos_w0 - 2.0 * sqrt_a * alpha;
                Self::normalize(b0, b1, b2, a0, a1, a2)
            }
        }
    }

    #[inline(always)]
    fn normalize(b0: f32, b1: f32, b2: f32, a0: f32, a1: f32, a2: f32) -> Self {
        let inv_a0 = 1.0 / a0;
        Self {
            b0: b0 * inv_a0,
            b1: b1 * inv_a0,
            b2: b2 * inv_a0,
            a1: a1 * inv_a0,
            a2: a2 * inv_a0,
        }
    }
}

/// Internal state for a biquad filter (Direct Form II Transposed).
#[derive(Debug, Clone, Copy, Default)]
pub struct BiquadState {
    pub s1: f32,
    pub s2: f32,
}

impl BiquadState {
    #[inline(always)]
    pub fn process(&mut self, sample: f32, coefs: &BiquadCoefficients) -> f32 {
        let out = coefs.b0 * sample + self.s1;
        self.s1 = coefs.b1 * sample - coefs.a1 * out + self.s2;
        self.s2 = coefs.b2 * sample - coefs.a2 * out;
        
        // Denormal prevention
        if out.abs() < 1e-15 { 0.0 } else { out }
    }
}

/// Shared Biquad config (Lock-Free pointer swapping)
pub struct AtomicBiquadBank<const BANDS: usize> {
    active_coefficients: AtomicPtr<[BiquadCoefficients; BANDS]>,
}

impl<const BANDS: usize> AtomicBiquadBank<BANDS> {
    pub fn new() -> Self {
        let initial_config = Box::new([BiquadCoefficients::default(); BANDS]);
        Self {
            active_coefficients: AtomicPtr::new(Box::into_raw(initial_config)),
        }
    }

    pub fn update(&self, new_config: Box<[BiquadCoefficients; BANDS]>) {
        let new_ptr = Box::into_raw(new_config);
        let old_ptr = self.active_coefficients.swap(new_ptr, Ordering::AcqRel);
        if !old_ptr.is_null() {
            unsafe { drop(Box::from_raw(old_ptr)); }
        }
    }

    #[inline(always)]
    pub fn read_current(&self) -> &[BiquadCoefficients; BANDS] {
        unsafe { &*self.active_coefficients.load(Ordering::Acquire) }
    }
}

impl<const BANDS: usize> Default for AtomicBiquadBank<BANDS> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const BANDS: usize> Drop for AtomicBiquadBank<BANDS> {
    fn drop(&mut self) {
        let ptr = self.active_coefficients.load(Ordering::Relaxed);
        if !ptr.is_null() {
            unsafe { drop(Box::from_raw(ptr)); }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use auralis_core::constants::SAMPLE_RATE;

    #[test]
    fn test_biquad_default_is_identity() {
        let coefs = BiquadCoefficients::default();
        let mut state = BiquadState::default();
        let sample = 0.5f32;
        let out = state.process(sample, &coefs);
        assert!((out - sample).abs() < 1e-6);
    }

    #[test]
    fn test_biquad_extreme_frequencies_stability() {
        let sample_rate = SAMPLE_RATE as f32;
        let test_frequencies = [20.0, 50.0, 1000.0, 10000.0, 20000.0];
        let test_filters = [
            FilterType::Peaking,
            FilterType::LowShelf,
            FilterType::HighShelf,
            FilterType::HighPass,
            FilterType::LowPass,
        ];

        for &filter in &test_filters {
            for &freq in &test_frequencies {
                let coefs = BiquadCoefficients::calculate(filter, sample_rate, freq, 1.0, 6.0);
                assert!(!coefs.b0.is_nan() && !coefs.b0.is_infinite());
                assert!(!coefs.b1.is_nan() && !coefs.b1.is_infinite());
                assert!(!coefs.b2.is_nan() && !coefs.b2.is_infinite());
                assert!(!coefs.a1.is_nan() && !coefs.a1.is_infinite());
                assert!(!coefs.a2.is_nan() && !coefs.a2.is_infinite());

                let mut state = BiquadState::default();
                for i in 0..100 {
                    let input = (i as f32 * 0.1).sin();
                    let out = state.process(input, &coefs);
                    assert!(!out.is_nan(), "Filter output resulted in NaN for freq {}", freq);
                    assert!(!out.is_infinite(), "Filter output resulted in Inf for freq {}", freq);
                }
            }
        }
    }

    #[test]
    fn test_atomic_biquad_bank_swap() {
        let bank = AtomicBiquadBank::<10>::new();
        let current = bank.read_current();
        assert_eq!(current[0].b0, 1.0);

        let mut new_config = Box::new([BiquadCoefficients::default(); 10]);
        new_config[0].b0 = 2.5;
        bank.update(new_config);

        let updated = bank.read_current();
        assert_eq!(updated[0].b0, 2.5);
    }
}

