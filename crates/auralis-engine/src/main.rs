use auralis_ipc::shared_memory::SharedMemory;
use auralis_wasapi::device::AudioDeviceManager;
use auralis_wasapi::stream::WasapiRenderStream;
use crate::pipeline::AudioEnginePipeline;
use std::sync::Arc;
use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};

pub mod config;
pub mod pipeline;

fn main() {
    println!("Auralis Engine Daemon Started.");

    // 1. Inisialisasi COM (wajib untuk WASAPI)
    unsafe {
        if let Err(e) = CoInitializeEx(None, COINIT_MULTITHREADED).ok() {
            eprintln!("Failed to initialize COM: {:?}", e);
            return;
        }
    }

    // 2. Inisialisasi Shared Memory untuk komunikasi VU meter & slider dengan UI
    let shm = Arc::new(match SharedMemory::open_or_create(auralis_ipc::shared_memory::SHM_NAME) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to open or create shared memory: {}", e);
            return;
        }
    });
    
    // 3. Enumerasi Device Audio
    let device_manager = AudioDeviceManager::new().expect("Failed to create Device Manager");
    let (virtual_game_device, virtual_name) = device_manager
        .find_virtual_game_device()
        .expect("Failed to find virtual game capture device");
    let (physical_output_device, physical_name) = device_manager
        .find_physical_output_device(None)
        .expect("Failed to find physical output device");

    println!("==================================================");
    println!(">>> Virtual Audio Input (Source): {}", virtual_name);
    println!(">>> Physical Audio Output (Target): {}", physical_name);
    println!("==================================================");
    
    // 4. Buat Lock-Free SPSC Ring Buffers (Kapasitas ~1 detik audio stereo pada 48kHz = 48000 * 2 = 96000 float)
    let (game_producer, game_consumer) = rtrb::RingBuffer::<f32>::new(96000);

    // 5. Inisialisasi WASAPI Streams
    use auralis_wasapi::stream::WasapiCaptureStream;
    let mut game_capture_stream = WasapiCaptureStream::new_loopback(&virtual_game_device)
        .expect("Failed to initialize loopback capture stream");

    // Inisialisasi Input Microphone Fisik (jika tersedia)
    let physical_mic_devices = device_manager.enumerate_physical_capture_devices().unwrap_or_default();
    let mut current_mic_stream: Option<WasapiCaptureStream> = None;
    if let Some(mic_info) = physical_mic_devices.first() {
        if let Ok(mic_dev) = device_manager.get_device_by_id(&mic_info.id) {
            if let Ok(mut stream) = WasapiCaptureStream::new(&mic_dev) {
                let mic_pipeline = MicCapturePipeline { shm: shm.clone() };
                if stream.start(mic_pipeline).is_ok() {
                    println!(">>> Physical Microphone Input (Source): {}", mic_info.name);
                    current_mic_stream = Some(stream);
                }
            }
        }
    }

    // 6. Inisialisasi Pipelines dengan Shared Memory
    let mut pipeline = AudioEnginePipeline::new(shm.clone());
    pipeline.game_in = Some(game_consumer); // Pasang consumer ke render pipeline
    let pipeline_shared = crate::pipeline::SharedAudioPipeline(Arc::new(std::sync::Mutex::new(pipeline)));
    
    let capture_pipeline = crate::pipeline::CaptureIngestionPipeline::new(game_producer); // Pasang producer ke capture pipeline

    // 7. Mulai Real-time Event Loops
    println!("Starting WASAPI real-time audio loops...");
    game_capture_stream.start(capture_pipeline).expect("Failed to start capture stream");

    let mut current_render_stream = Some(
        WasapiRenderStream::new(&physical_output_device)
            .expect("Failed to initialize physical render stream")
    );
    current_render_stream.as_mut().unwrap().start(pipeline_shared.clone()).expect("Failed to start render stream");

    // 8. Start Smart App Audio Manager Thread (Routing & Volume)
    let shm_manager = shm.clone();
    std::thread::spawn(move || {
        unsafe {
            let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_MULTITHREADED);
        }
        let dev_mgr = auralis_wasapi::device::AudioDeviceManager::new().expect("Failed to create Device Manager");
        loop {
            std::thread::sleep(std::time::Duration::from_millis(32)); // ~30 fps
            let telemetry = shm_manager.get();
            let game_vol = telemetry.read_game_volume();
            let chat_vol = telemetry.read_chat_volume();
            let mix = telemetry.read_chatmix_balance(); // -1.0 (Game) to 1.0 (Chat)

            let game_mul = if mix > 0.0 { 1.0 - mix } else { 1.0 };
            let chat_mul = if mix < 0.0 { 1.0 + mix } else { 1.0 };

            let mut max_game_peak = 0.0f32;
            let mut max_chat_peak = 0.0f32;

            use windows::Win32::Media::Audio::{IAudioSessionControl2, ISimpleAudioVolume};
            use windows::Win32::Media::Audio::Endpoints::IAudioMeterInformation;
            use windows_core::Interface;

            if let Ok(devices) = dev_mgr.enumerate_render_devices() {
                for dev_info in devices {
                    if let Ok(imm_dev) = dev_mgr.get_device_by_id(&dev_info.id) {
                        if let Ok(session_manager) = unsafe { imm_dev.Activate::<windows::Win32::Media::Audio::IAudioSessionManager2>(windows::Win32::System::Com::CLSCTX_ALL, None) } {
                            if let Ok(enumerator) = unsafe { session_manager.GetSessionEnumerator() } {
                                if let Ok(count) = unsafe { enumerator.GetCount() } {
                                    for i in 0..count {
                                        if let Ok(control) = unsafe { enumerator.GetSession(i) } {
                                            if let Ok(control2) = control.cast::<IAudioSessionControl2>() {
                                                let pid = match unsafe { control2.GetProcessId() } {
                                                    Ok(p) if p != 0 => p,
                                                    _ => continue,
                                                };

                                                let process_name = auralis_wasapi::sessions::AudioSessionTracker::get_process_name(pid)
                                                    .unwrap_or_default();
                                                
                                                let display_name = unsafe { control.GetDisplayName() }
                                                    .map(|pwstr| unsafe { pwstr.to_string().unwrap_or_default() })
                                                    .unwrap_or_default();
                                                    
                                                let final_display = if !display_name.is_empty() {
                                                    display_name
                                                } else if !process_name.is_empty() {
                                                    process_name.clone()
                                                } else {
                                                    continue;
                                                };

                                                let lower = final_display.to_lowercase();
                                                let proc_lower = process_name.to_lowercase();
                                                if lower.contains("audiodg") || lower.contains("auralis") || lower.contains("antigravity")
                                                    || proc_lower.contains("audiodg") || proc_lower.contains("auralis") || proc_lower.contains("antigravity") {
                                                    continue;
                                                }

                                                let is_chat = telemetry.get_app_routing(&final_display)
                                                    .or_else(|| telemetry.get_app_routing(&process_name))
                                                    .unwrap_or_else(|| {
                                                        proc_lower.contains("discord") || proc_lower.contains("teamspeak") || proc_lower.contains("skype") || proc_lower.contains("zoom")
                                                    });

                                                let is_app_muted = telemetry.get_app_muted(&final_display) || telemetry.get_app_muted(&process_name);

                                                let target_vol = if is_chat { chat_vol * chat_mul } else { game_vol * game_mul };

                                                if let Ok(simple_vol) = control.cast::<ISimpleAudioVolume>() {
                                                    unsafe {
                                                        let _ = simple_vol.SetMute(is_app_muted, std::ptr::null());
                                                        let _ = simple_vol.SetMasterVolume(target_vol, std::ptr::null());
                                                    }
                                                }

                                                if let Ok(meter) = control.cast::<IAudioMeterInformation>() {
                                                    if let Ok(peak) = unsafe { meter.GetPeakValue() } {
                                                        // Kalikan dengan target_vol agar grafik turun saat slider volume diturunkan
                                                        let effective_peak = if is_app_muted { 0.0 } else { peak * target_vol };
                                                        if is_chat {
                                                            if effective_peak > max_chat_peak { max_chat_peak = effective_peak; }
                                                        } else {
                                                            if effective_peak > max_game_peak { max_game_peak = effective_peak; }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Cut-off noise floor agar tidak ada bar yang menyala saat hening
            if max_game_peak < 0.003 { max_game_peak = 0.0; }
            if max_chat_peak < 0.003 { max_chat_peak = 0.0; }

            telemetry.write_game_peak(max_game_peak, max_game_peak);
            telemetry.write_chat_peak(max_chat_peak, max_chat_peak);
        }
    });

    // Loop pemantauan kontrol: deteksi jika pengguna mengganti speaker/headphone atau microphone di UI
    loop {
        std::thread::sleep(std::time::Duration::from_millis(100));

        // 1. Output Device Switching
        if let Some(target_idx) = shm.get().check_target_device_change() {
            let current_phys_devices = device_manager.enumerate_physical_devices().unwrap_or_default();
            if let Some(target_info) = current_phys_devices.get(target_idx as usize) {
                println!(">>> Switching physical audio output to: {}", target_info.name);
                if let Ok(new_dev) = device_manager.get_device_by_id(&target_info.id) {
                    if let Ok(mut new_stream) = WasapiRenderStream::new(&new_dev) {
                        let mut success = false;
                        if new_stream.start(pipeline_shared.clone()).is_ok() {
                            println!(">>> Output switched successfully to: {}", target_info.name);
                            success = true;
                        } else {
                            println!(">>> Failed to start new stream to: {}", target_info.name);
                        }

                        if success {
                            if let Some(old_stream) = current_render_stream.take() {
                                old_stream.stop();
                            }
                            current_render_stream = Some(new_stream);
                        }
                    }
                }
            }
        }

        // 2. Microphone Input Device Switching
        if let Some(target_input_idx) = shm.get().check_target_input_change() {
            let current_phys_mics = device_manager.enumerate_physical_capture_devices().unwrap_or_default();
            if let Some(target_info) = current_phys_mics.get(target_input_idx as usize) {
                println!(">>> Switching physical microphone input to: {}", target_info.name);
                if let Ok(new_mic_dev) = device_manager.get_device_by_id(&target_info.id) {
                    if let Ok(mut new_mic_stream) = WasapiCaptureStream::new(&new_mic_dev) {
                        let mic_pipe = MicCapturePipeline { shm: shm.clone() };
                        if new_mic_stream.start(mic_pipe).is_ok() {
                            if let Some(old_mic) = current_mic_stream.take() {
                                old_mic.stop();
                            }
                            current_mic_stream = Some(new_mic_stream);
                            println!(">>> Microphone input switched successfully to: {}", target_info.name);
                        }
                    }
                }
            }
        }
    }
}

/// Callback capture stream untuk Microphone fisik
struct MicCapturePipeline {
    shm: Arc<SharedMemory>,
}

impl auralis_wasapi::stream::AudioCaptureCallback for MicCapturePipeline {
    fn process_capture(&mut self, buffer: &[f32]) {
        let telemetry = self.shm.get();
        let mic_vol = telemetry.read_mic_volume();
        let mut peak = 0.0f32;
        for &sample in buffer {
            let abs = sample.abs() * mic_vol;
            if abs > peak {
                peak = abs;
            }
        }
        if peak < 0.003 {
            peak = 0.0;
        }
        telemetry.write_mic_peak(peak);
    }
}
