/// Simple Noise Gate for Microphone processing.
pub struct NoiseGate {
    open_threshold_linear: f32,
    close_threshold_linear: f32,
    attack_coef: f32,
    release_coef: f32,
    envelope: f32,
    is_open: bool,
}

impl NoiseGate {
    pub fn new(sample_rate: f32, open_threshold_db: f32, close_threshold_db: f32, attack_ms: f32, release_ms: f32) -> Self {
        let open_threshold_linear = 10.0f32.powf(open_threshold_db / 20.0);
        let close_threshold_linear = 10.0f32.powf(close_threshold_db / 20.0);
        
        let attack_coef = (-1.0 / (attack_ms * 0.001 * sample_rate)).exp();
        let release_coef = (-1.0 / (release_ms * 0.001 * sample_rate)).exp();

        Self {
            open_threshold_linear,
            close_threshold_linear,
            attack_coef,
            release_coef,
            envelope: 0.0,
            is_open: false,
        }
    }

    #[inline(always)]
    pub fn process(&mut self, sample: f32) -> f32 {
        let abs_s = sample.abs();
        
        // Envelope follower (fast attack, slow release)
        if abs_s > self.envelope {
            self.envelope = self.attack_coef * self.envelope + (1.0 - self.attack_coef) * abs_s;
        } else {
            self.envelope = self.release_coef * self.envelope + (1.0 - self.release_coef) * abs_s;
        }

        // Hysteresis logic
        if self.envelope > self.open_threshold_linear {
            self.is_open = true;
        } else if self.envelope < self.close_threshold_linear {
            self.is_open = false;
        }

        // Apply gate
        if self.is_open {
            sample
        } else {
            // Apply slight smoothing/fade-out instead of hard zero to prevent clicks
            sample * (self.envelope / self.close_threshold_linear).min(1.0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_noise_gate_attenuates_low_noise() {
        // -40 dB open threshold, -45 dB close threshold
        let mut gate = NoiseGate::new(48000.0, -40.0, -45.0, 5.0, 50.0);
        let low_noise = 0.0001f32; // very quiet noise (-80 dB)

        for _ in 0..1000 {
            let out = gate.process(low_noise);
            assert!(out.abs() <= low_noise);
        }
        assert!(!gate.is_open);
    }

    #[test]
    fn test_noise_gate_opens_on_loud_signal() {
        let mut gate = NoiseGate::new(48000.0, -40.0, -45.0, 1.0, 50.0);
        let voice_signal = 0.5f32; // loud voice (-6 dB)

        for _ in 0..500 {
            gate.process(voice_signal);
        }
        assert!(gate.is_open);
    }
}

