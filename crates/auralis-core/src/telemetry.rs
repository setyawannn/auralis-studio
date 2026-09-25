use bytemuck::Zeroable;
use std::sync::atomic::{AtomicU32, Ordering};

/// Magic number used to verify the shared memory initialization.
pub const TELEMETRY_MAGIC: u32 = 0xA0BA0BA0;

/// Lock-free structure shared via Windows File Mapping between UI and Engine.
/// Peak levels are represented as f32 bits (via `f32::to_bits` and `f32::from_bits`)
/// to allow atomic reads/writes without mutexes on the audio thread.
#[repr(C)]
pub struct SharedAudioTelemetry {
    pub magic: u32,
    // Store levels as atomic u32 (f32 bits)
    game_peak_l: AtomicU32,
    game_peak_r: AtomicU32,
    chat_peak_l: AtomicU32,
    chat_peak_r: AtomicU32,
    mic_peak: AtomicU32,
    master_peak_l: AtomicU32,
    master_peak_r: AtomicU32,
    is_running: AtomicU32, // 0 = stopped, 1 = running

    // Real-time Controls (UI writes, Engine reads)
    game_volume: AtomicU32,
    chat_volume: AtomicU32,
    master_volume: AtomicU32,
    mic_volume: AtomicU32,
    mic_muted: AtomicU32,
    chatmix_balance: AtomicU32,
    target_device_idx: AtomicU32,
    target_device_changed: AtomicU32,
    target_input_device_idx: AtomicU32,
    target_input_device_changed: AtomicU32,

    // App Routing (UI -> Engine)
    app_names: [[u8; 32]; 16],
    app_is_chat: [AtomicU32; 16],
    app_is_muted: [AtomicU32; 16],
}

// Ensure the struct can be safely casted to bytes
unsafe impl Zeroable for SharedAudioTelemetry {}

impl SharedAudioTelemetry {
    pub fn new() -> Self {
        Self {
            magic: TELEMETRY_MAGIC,
            game_peak_l: AtomicU32::new(0),
            game_peak_r: AtomicU32::new(0),
            chat_peak_l: AtomicU32::new(0),
            chat_peak_r: AtomicU32::new(0),
            mic_peak: AtomicU32::new(0),
            master_peak_l: AtomicU32::new(0),
            master_peak_r: AtomicU32::new(0),
            is_running: AtomicU32::new(0),
            game_volume: AtomicU32::new(1.0f32.to_bits()),
            chat_volume: AtomicU32::new(1.0f32.to_bits()),
            master_volume: AtomicU32::new(0.9f32.to_bits()),
            mic_volume: AtomicU32::new(1.0f32.to_bits()),
            mic_muted: AtomicU32::new(0),
            chatmix_balance: AtomicU32::new(0.0f32.to_bits()),
            target_device_idx: AtomicU32::new(0),
            target_device_changed: AtomicU32::new(0),
            target_input_device_idx: AtomicU32::new(0),
            target_input_device_changed: AtomicU32::new(0),
            app_names: [[0; 32]; 16],
            app_is_chat: core::array::from_fn(|_| AtomicU32::new(0)),
            app_is_muted: core::array::from_fn(|_| AtomicU32::new(0)),
        }
    }

    #[inline(always)]
    fn f32_to_bits(val: f32) -> u32 {
        val.to_bits()
    }

    #[inline(always)]
    fn bits_to_f32(bits: u32) -> f32 {
        f32::from_bits(bits)
    }

    // --- Writers (Called by Engine audio thread) ---

    #[inline(always)]
    pub fn write_game_peak(&self, left: f32, right: f32) {
        self.game_peak_l.store(Self::f32_to_bits(left), Ordering::Relaxed);
        self.game_peak_r.store(Self::f32_to_bits(right), Ordering::Relaxed);
    }

    #[inline(always)]
    pub fn write_chat_peak(&self, left: f32, right: f32) {
        self.chat_peak_l.store(Self::f32_to_bits(left), Ordering::Relaxed);
        self.chat_peak_r.store(Self::f32_to_bits(right), Ordering::Relaxed);
    }

    #[inline(always)]
    pub fn write_mic_peak(&self, mono: f32) {
        self.mic_peak.store(Self::f32_to_bits(mono), Ordering::Relaxed);
    }

    #[inline(always)]
    pub fn write_master_peak(&self, left: f32, right: f32) {
        self.master_peak_l.store(Self::f32_to_bits(left), Ordering::Relaxed);
        self.master_peak_r.store(Self::f32_to_bits(right), Ordering::Relaxed);
    }
    
    pub fn set_running(&self, running: bool) {
        self.is_running.store(if running { 1 } else { 0 }, Ordering::Release);
    }

    // --- Readers (Called by UI render thread at 60 FPS) ---

    #[inline(always)]
    pub fn read_game_peak(&self) -> (f32, f32) {
        (
            Self::bits_to_f32(self.game_peak_l.load(Ordering::Relaxed)),
            Self::bits_to_f32(self.game_peak_r.load(Ordering::Relaxed)),
        )
    }

    #[inline(always)]
    pub fn read_chat_peak(&self) -> (f32, f32) {
        (
            Self::bits_to_f32(self.chat_peak_l.load(Ordering::Relaxed)),
            Self::bits_to_f32(self.chat_peak_r.load(Ordering::Relaxed)),
        )
    }

    #[inline(always)]
    pub fn read_mic_peak(&self) -> f32 {
        Self::bits_to_f32(self.mic_peak.load(Ordering::Relaxed))
    }

    #[inline(always)]
    pub fn read_master_peak(&self) -> (f32, f32) {
        (
            Self::bits_to_f32(self.master_peak_l.load(Ordering::Relaxed)),
            Self::bits_to_f32(self.master_peak_r.load(Ordering::Relaxed)),
        )
    }
    
    pub fn is_running(&self) -> bool {
        self.is_running.load(Ordering::Acquire) != 0
    }

    // --- Control Getters & Setters ---

    #[inline(always)]
    pub fn write_game_volume(&self, val: f32) {
        self.game_volume.store(Self::f32_to_bits(val), Ordering::Relaxed);
    }

    #[inline(always)]
    pub fn read_game_volume(&self) -> f32 {
        Self::bits_to_f32(self.game_volume.load(Ordering::Relaxed))
    }

    #[inline(always)]
    pub fn write_chat_volume(&self, val: f32) {
        self.chat_volume.store(Self::f32_to_bits(val), Ordering::Relaxed);
    }

    #[inline(always)]
    pub fn read_chat_volume(&self) -> f32 {
        Self::bits_to_f32(self.chat_volume.load(Ordering::Relaxed))
    }

    #[inline(always)]
    pub fn write_master_volume(&self, val: f32) {
        self.master_volume.store(Self::f32_to_bits(val), Ordering::Relaxed);
    }

    #[inline(always)]
    pub fn read_master_volume(&self) -> f32 {
        Self::bits_to_f32(self.master_volume.load(Ordering::Relaxed))
    }

    #[inline(always)]
    pub fn write_chatmix_balance(&self, val: f32) {
        self.chatmix_balance.store(Self::f32_to_bits(val), Ordering::Relaxed);
    }

    #[inline(always)]
    pub fn read_chatmix_balance(&self) -> f32 {
        Self::bits_to_f32(self.chatmix_balance.load(Ordering::Relaxed))
    }

    #[inline(always)]
    pub fn write_mic_volume(&self, val: f32) {
        self.mic_volume.store(Self::f32_to_bits(val), Ordering::Relaxed);
    }

    #[inline(always)]
    pub fn read_mic_volume(&self) -> f32 {
        Self::bits_to_f32(self.mic_volume.load(Ordering::Relaxed))
    }

    #[inline(always)]
    pub fn write_mic_muted(&self, muted: bool) {
        self.mic_muted.store(if muted { 1 } else { 0 }, Ordering::Relaxed);
    }

    #[inline(always)]
    pub fn read_mic_muted(&self) -> bool {
        self.mic_muted.load(Ordering::Relaxed) != 0
    }

    pub fn set_target_device(&self, idx: u32) {
        self.target_device_idx.store(idx, Ordering::Release);
        self.target_device_changed.store(1, Ordering::Release);
    }

    pub fn check_target_device_change(&self) -> Option<u32> {
        if self.target_device_changed.swap(0, Ordering::AcqRel) == 1 {
            Some(self.target_device_idx.load(Ordering::Acquire))
        } else {
            None
        }
    }

    pub fn set_target_input_device(&self, idx: u32) {
        self.target_input_device_idx.store(idx, Ordering::Release);
        self.target_input_device_changed.store(1, Ordering::Release);
    }

    pub fn check_target_input_change(&self) -> Option<u32> {
        if self.target_input_device_changed.swap(0, Ordering::AcqRel) == 1 {
            Some(self.target_input_device_idx.load(Ordering::Acquire))
        } else {
            None
        }
    }

    fn normalize_name(name: &str) -> [u8; 32] {
        let mut buf = [0u8; 32];
        let lower = name.trim().to_ascii_lowercase();
        let stripped = lower.strip_suffix(".exe").unwrap_or(&lower);
        let bytes = stripped.as_bytes();
        let len = std::cmp::min(32, bytes.len());
        buf[..len].copy_from_slice(&bytes[..len]);
        buf
    }

    pub fn set_app_routing(&mut self, name: &str, is_chat: bool) {
        let norm = Self::normalize_name(name);
        if norm[0] == 0 {
            return;
        }
        let mut slot = 16;
        for i in 0..16 {
            if self.app_names[i] == norm {
                self.app_is_chat[i].store(if is_chat { 1 } else { 0 }, Ordering::Release);
                return;
            }
            if self.app_names[i][0] == 0 && slot == 16 {
                slot = i;
            }
        }
        if slot < 16 {
            self.app_names[slot] = norm;
            self.app_is_chat[slot].store(if is_chat { 1 } else { 0 }, Ordering::Release);
        }
    }

    pub fn get_app_routing(&self, name: &str) -> Option<bool> {
        let norm = Self::normalize_name(name);
        if norm[0] == 0 {
            return None;
        }
        for i in 0..16 {
            if self.app_names[i] == norm {
                return Some(self.app_is_chat[i].load(Ordering::Acquire) == 1);
            }
        }
        None
    }

    pub fn set_app_muted(&mut self, name: &str, is_muted: bool) {
        let norm = Self::normalize_name(name);
        if norm[0] == 0 {
            return;
        }
        let mut slot = 16;
        for i in 0..16 {
            if self.app_names[i] == norm {
                self.app_is_muted[i].store(if is_muted { 1 } else { 0 }, Ordering::Release);
                return;
            }
            if self.app_names[i][0] == 0 && slot == 16 {
                slot = i;
            }
        }
        if slot < 16 {
            self.app_names[slot] = norm;
            self.app_is_muted[slot].store(if is_muted { 1 } else { 0 }, Ordering::Release);
        }
    }

    pub fn toggle_app_muted(&mut self, name: &str) -> bool {
        let current = self.get_app_muted(name);
        let next = !current;
        self.set_app_muted(name, next);
        next
    }

    pub fn get_app_muted(&self, name: &str) -> bool {
        let norm = Self::normalize_name(name);
        if norm[0] == 0 {
            return false;
        }
        for i in 0..16 {
            if self.app_names[i] == norm {
                return self.app_is_muted[i].load(Ordering::Acquire) == 1;
            }
        }
        false
    }
}
