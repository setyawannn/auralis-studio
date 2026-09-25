use auralis_dsp::chatmix::ChatMixState;
use auralis_dsp::limiter::SoftLimiter;
use auralis_wasapi::stream::AudioRenderCallback;
use rtrb::Consumer;
use std::sync::Arc;

pub struct AudioEnginePipeline {
    // SPSC Consumers dari capture endpoints (virtual)
    pub game_in: Option<Consumer<f32>>,
    pub chat_in: Option<Consumer<f32>>,
    pub mic_in: Option<Consumer<f32>>,

    // DSP States (dieksekusi lokal di thread WASAPI, bebas alokasi dinamis)
    pub chatmix: ChatMixState,
    pub limiter: SoftLimiter,
    
    // Shared Memory untuk kontrol volume real-time & visualisasi VU meter (60 FPS atomic)
    pub shm: Arc<auralis_ipc::shared_memory::SharedMemory>,
}

impl AudioEnginePipeline {
    pub fn new(shm: Arc<auralis_ipc::shared_memory::SharedMemory>) -> Self {
        Self {
            game_in: None,
            chat_in: None,
            mic_in: None,
            chatmix: ChatMixState::default(),
            limiter: SoftLimiter::default(),
            shm,
        }
    }
}

impl AudioRenderCallback for AudioEnginePipeline {
    #[inline(always)]
    fn process_render(&mut self, buffer: &mut [f32]) {
        // Buffer berisi [L, R, L, R, ...]
        let frames = buffer.len() / 2;
        
        let mut master_peak_l = 0.0f32;
        let mut master_peak_r = 0.0f32;

        let telemetry = self.shm.get();

        // Ambil kontrol real-time terkini dari UI (Lock-Free atomic reads)
        let master_vol = telemetry.read_master_volume();

        for i in 0..frames {
            // -- Tahap 1: Baca dari SPSC Capture Ring Buffer (Virtual Audio Cable) --
            // Aplikasi sudah diskalakan volumenya secara spesifik via Windows WASAPI ISimpleAudioVolume
            let mut mixed_l = 0.0;
            let mut mixed_r = 0.0;
            
            if let Some(ref mut consumer) = self.game_in {
                if let Ok(l) = consumer.pop() { mixed_l += l; }
                if let Ok(r) = consumer.pop() { mixed_r += r; }
            }

            if let Some(ref mut consumer) = self.chat_in {
                if let Ok(l) = consumer.pop() { mixed_l += l; }
                if let Ok(r) = consumer.pop() { mixed_r += r; }
            }

            // -- Tahap 3: Master Volume & Soft Limiter --
            let (final_l, final_r) = self.limiter.process((mixed_l * master_vol, mixed_r * master_vol));

            // Simpan ke output WASAPI buffer fisik
            buffer[i * 2] = final_l;
            buffer[i * 2 + 1] = final_r;

            // Deteksi Puncak Master
            if final_l.abs() > master_peak_l { master_peak_l = final_l.abs(); }
            if final_r.abs() > master_peak_r { master_peak_r = final_r.abs(); }
        }

        // -- Tahap 4: Publikasi State ke UI via Lock-free Shared Memory --
        telemetry.write_master_peak(master_peak_l, master_peak_r);
    }
}

use std::sync::Mutex;

#[derive(Clone)]
pub struct SharedAudioPipeline(pub Arc<Mutex<AudioEnginePipeline>>);

impl AudioRenderCallback for SharedAudioPipeline {
    #[inline(always)]
    fn process_render(&mut self, buffer: &mut [f32]) {
        if let Ok(mut pipe) = self.0.try_lock() {
            pipe.process_render(buffer);
        } else {
            buffer.fill(0.0);
        }
    }
}

use auralis_wasapi::stream::AudioCaptureCallback;
use rtrb::Producer;

/// Menangkap audio dari Virtual Device dan memasukkannya ke Ring Buffer secara lock-free
pub struct CaptureIngestionPipeline {
    producer: Producer<f32>,
}

impl CaptureIngestionPipeline {
    pub fn new(producer: Producer<f32>) -> Self {
        Self { producer }
    }
}

impl AudioCaptureCallback for CaptureIngestionPipeline {
    #[inline(always)]
    fn process_capture(&mut self, buffer: &[f32]) {
        // Tulis secepat mungkin ke dalam SPSC lock-free buffer
        // Jika buffer penuh (sangat jarang terjadi jika render thread sehat), kita lewati (drop frame)
        // untuk menjaga konsistensi sistem audio real-time.
        for &sample in buffer {
            let _ = self.producer.push(sample);
        }
    }
}
