use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ChannelId {
    Game,
    Chat,
    Mic,
    Master,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum FilterType {
    Peaking,
    LowShelf,
    HighShelf,
    HighPass,
    LowPass,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EqBand {
    pub filter_type: FilterType,
    pub frequency_hz: f32,
    pub q_factor: f32,
    pub gain_db: f32,
}

impl Default for EqBand {
    fn default() -> Self {
        Self {
            filter_type: FilterType::Peaking,
            frequency_hz: 1000.0,
            q_factor: 0.707, // Butterworth Q
            gain_db: 0.0,
        }
    }
}

/// DSP State container for a single audio channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelState {
    pub volume_db: f32,
    pub is_muted: bool,
    pub eq_bands: Vec<EqBand>,
}

impl Default for ChannelState {
    fn default() -> Self {
        Self {
            volume_db: 0.0,
            is_muted: false,
            eq_bands: vec![EqBand::default(); crate::constants::EQ_BAND_COUNT],
        }
    }
}

/// Global audio configuration state that gets persisted to TOML.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioConfig {
    pub game: ChannelState,
    pub chat: ChannelState,
    pub mic: ChannelState,
    pub master: ChannelState,
    
    /// Range from -1.0 (Full Game) to 1.0 (Full Chat), 0.0 is Center.
    pub chat_mix_balance: f32,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            game: ChannelState::default(),
            chat: ChannelState::default(),
            mic: ChannelState::default(),
            master: ChannelState::default(),
            chat_mix_balance: 0.0,
        }
    }
}

/// Metadata perangkat endpoint audio Windows
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioEndpointInfo {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

