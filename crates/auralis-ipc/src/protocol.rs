use serde::{Deserialize, Serialize};
use auralis_core::types::{ChannelId, FilterType, AudioEndpointInfo};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppAudioSession {
    pub process_id: u32,
    pub process_name: String,
    pub display_name: String,
    pub current_channel: ChannelId,
    pub peak_volume: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IpcRequest {
    GetRenderDevices,
    SelectOutputDevice(String),
    SelectInputDevice(String),
    GetAudioSessions,
    SetAppChannel {
        process_name: String,
        channel: ChannelId,
    },
    SetChatMixBalance(f32),
    SetChannelVolume {
        channel: ChannelId,
        gain_db: f32,
    },
    SetChannelMute {
        channel: ChannelId,
        muted: bool,
    },
    SetEqBand {
        channel: ChannelId,
        band_idx: usize,
        filter_type: FilterType,
        freq: f32,
        q: f32,
        gain_db: f32,
    },
    GetEngineStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IpcResponse {
    Ok,
    Error(String),
    RenderDevices(Vec<AudioEndpointInfo>),
    AudioSessions(Vec<AppAudioSession>),
    Status {
        is_running: bool,
        cpu_usage_pct: f32,
        ram_usage_mb: f32,
        buffer_size_ms: u32,
    },
}
