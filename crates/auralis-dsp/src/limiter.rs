use auralis_core::constants::LIMITER_THRESHOLD_DB;

/// Soft Limiter using tanh curve for gentle saturation.
/// Prevents harsh digital clipping by smoothing out peaks near 0 dBFS.
pub struct SoftLimiter {
    threshold_linear: f32,
    makeup_gain: f32,
}

impl Default for SoftLimiter {
    fn default() -> Self {
        Self::new(LIMITER_THRESHOLD_DB)
    }
}

impl SoftLimiter {
    pub fn new(threshold_db: f32) -> Self {
        // Convert threshold dB to linear scale
        let threshold_linear = 10.0f32.powf(threshold_db / 20.0);
        
        Self {
            threshold_linear,
            makeup_gain: 1.0, // Can add auto-makeup gain if needed later
        }
    }

    /// Process a stereo frame through the soft limiter.
    #[inline(always)]
    pub fn process(&mut self, frame: (f32, f32)) -> (f32, f32) {
        let (l, r) = frame;
        
        // Use an approximate tanh or algebraic soft clipper for performance
        // f(x) = x / (1 + (x/t)^2)^0.5
        // or faster polynomial if needed. 
        // Here we use a piecewise approach for efficiency: linear below threshold, soft clip above.
        
        (self.soft_clip(l), self.soft_clip(r))
    }

    #[inline(always)]
    fn soft_clip(&self, sample: f32) -> f32 {
        let abs_s = sample.abs();
        
        if abs_s <= self.threshold_linear {
            // Below threshold: linear pass-through
            sample * self.makeup_gain
        } else {
            // Above threshold: gentle compression using tanh approximation
            let overdrive = abs_s - self.threshold_linear;
            
            // Limit the overdrive so it asymptotes at 1.0 (0 dBFS)
            let headroom = 1.0 - self.threshold_linear;
            if headroom <= 0.0 {
                return sample.signum() * self.threshold_linear;
            }
            
            // Scaled tanh
            let compressed = headroom * (overdrive / headroom).tanh();
            sample.signum() * (self.threshold_linear + compressed) * self.makeup_gain
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_limiter_passthrough_below_threshold() {
        let mut limiter = SoftLimiter::new(-0.5);
        let sample = 0.2f32; // well below threshold
        let (out_l, out_r) = limiter.process((sample, -sample));
        assert!((out_l - sample).abs() < 1e-6);
        assert!((out_r - (-sample)).abs() < 1e-6);
    }

    #[test]
    fn test_limiter_never_exceeds_unity() {
        let mut limiter = SoftLimiter::new(-0.5);
        // Test extreme inputs up to +20 dBFS (amplitude 10.0)
        let extreme_inputs = [1.0, 1.5, 2.0, 5.0, 10.0, 50.0];
        for &input in &extreme_inputs {
            let (out_pos, _) = limiter.process((input, 0.0));
            let (out_neg, _) = limiter.process((-input, 0.0));

            assert!(out_pos <= 1.0, "Limiter allowed signal to exceed 1.0: {}", out_pos);
            assert!(out_neg >= -1.0, "Limiter allowed signal to exceed -1.0: {}", out_neg);
            assert!(!out_pos.is_nan());
            assert!(!out_neg.is_nan());
        }
    }
}

