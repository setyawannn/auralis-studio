/// Standard processing sample rate (48 kHz) for all internal DSP pipelines.
pub const SAMPLE_RATE: u32 = 48_000;

/// Number of frames per internal processing buffer (10ms buffer size).
/// 48,000 Hz * 0.010 s = 480 frames.
pub const FRAMES_PER_BUFFER: usize = 480;

/// Peak limiter threshold in dBFS (e.g. -0.5 dB).
pub const LIMITER_THRESHOLD_DB: f32 = -0.5;

/// Number of EQ bands available per channel.
pub const EQ_BAND_COUNT: usize = 10;
