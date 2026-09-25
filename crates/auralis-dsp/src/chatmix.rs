use serde::{Deserialize, Serialize};

/// Constant-power crossfade for game vs chat audio.
/// balance: -1.0 (100% Game), 0.0 (50/50 equal power), 1.0 (100% Chat)
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ChatMixConfig {
    pub balance: f32,
}

impl Default for ChatMixConfig {
    fn default() -> Self {
        Self { balance: 0.0 }
    }
}

pub struct ChatMixState {
    current_game_gain: f32,
    current_chat_gain: f32,
    target_game_gain: f32,
    target_chat_gain: f32,
    slew_rate: f32,
}

impl Default for ChatMixState {
    fn default() -> Self {
        let (game, chat) = Self::calculate_gains(0.0);
        Self {
            current_game_gain: game,
            current_chat_gain: chat,
            target_game_gain: game,
            target_chat_gain: chat,
            // Slew rate limits how fast volume changes per sample (prevents zipper noise)
            // 48kHz: change 1.0 full scale over ~20ms (480 * 2 samples)
            slew_rate: 1.0 / 960.0, 
        }
    }
}

impl ChatMixState {
    pub fn new(balance: f32, sample_rate: f32) -> Self {
        let (game, chat) = Self::calculate_gains(balance);
        let slew_time_sec = 0.02; // 20ms transition
        Self {
            current_game_gain: game,
            current_chat_gain: chat,
            target_game_gain: game,
            target_chat_gain: chat,
            slew_rate: 1.0 / (sample_rate * slew_time_sec),
        }
    }

    pub fn set_balance(&mut self, balance: f32) {
        let (game, chat) = Self::calculate_gains(balance);
        self.target_game_gain = game;
        self.target_chat_gain = chat;
    }

    /// Process a single stereo frame (L, R) for both game and chat.
    /// Returns the mixed output (L, R).
    #[inline(always)]
    pub fn process(&mut self, game_frame: (f32, f32), chat_frame: (f32, f32)) -> (f32, f32) {
        // Interpolate gains towards targets to prevent pops/zipper noise
        self.current_game_gain = Self::approach(self.current_game_gain, self.target_game_gain, self.slew_rate);
        self.current_chat_gain = Self::approach(self.current_chat_gain, self.target_chat_gain, self.slew_rate);

        let out_l = game_frame.0 * self.current_game_gain + chat_frame.0 * self.current_chat_gain;
        let out_r = game_frame.1 * self.current_game_gain + chat_frame.1 * self.current_chat_gain;
        
        (out_l, out_r)
    }

    /// Equal-power calculation: sin/cos curve based on theta.
    #[inline(always)]
    fn calculate_gains(balance: f32) -> (f32, f32) {
        let clamped = balance.clamp(-1.0, 1.0);
        // Map [-1.0, 1.0] to [0, pi/2]
        let theta = std::f32::consts::FRAC_PI_4 * (clamped + 1.0);
        let game_gain = theta.cos();
        let chat_gain = theta.sin();
        (game_gain, chat_gain)
    }

    #[inline(always)]
    fn approach(current: f32, target: f32, step: f32) -> f32 {
        if current < target {
            (current + step).min(target)
        } else if current > target {
            (current - step).max(target)
        } else {
            current
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chatmix_constant_power() {
        // cos^2(theta) + sin^2(theta) must equal 1.0 across all balance settings
        for i in -100..=100 {
            let balance = i as f32 / 100.0;
            let (game, chat) = ChatMixState::calculate_gains(balance);
            let power = game * game + chat * chat;
            assert!((power - 1.0).abs() < 1e-5, "Constant power violated at balance {}: power={}", balance, power);
        }
    }

    #[test]
    fn test_chatmix_center_balance() {
        let (game, chat) = ChatMixState::calculate_gains(0.0);
        // At 0.0, both should be ~0.7071 (-3dB)
        assert!((game - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-4);
        assert!((chat - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-4);
    }

    #[test]
    fn test_chatmix_extremes() {
        // -1.0 is full game (game = 1.0, chat = 0.0)
        let (game_full, chat_full_g) = ChatMixState::calculate_gains(-1.0);
        assert!((game_full - 1.0).abs() < 1e-5);
        assert!(chat_full_g.abs() < 1e-5);

        // 1.0 is full chat (game = 0.0, chat = 1.0)
        let (game_full_c, chat_full) = ChatMixState::calculate_gains(1.0);
        assert!(game_full_c.abs() < 1e-5);
        assert!((chat_full - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_chatmix_slew_rate_smoothing() {
        let mut state = ChatMixState::new(0.0, 48000.0);
        state.set_balance(1.0); // Abrupt shift to full chat

        let game_frame = (1.0, 1.0);
        let chat_frame = (0.0, 0.0);

        // First frame should not immediately drop to zero game sound due to slew rate
        let (l, _) = state.process(game_frame, chat_frame);
        assert!(l > 0.65, "Output dropped too fast without smoothing: {}", l);
    }
}

