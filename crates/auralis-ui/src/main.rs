use auralis_ipc::shared_memory::SharedMemory;
use auralis_wasapi::device::AudioDeviceManager;
use auralis_wasapi::sessions::AudioSessionTracker;
use slint::{Timer, TimerMode, ModelRc, VecModel, SharedString};
use std::time::Duration;
use std::sync::{Arc, Mutex};

slint::include_modules!();

pub mod tray;

use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};

fn sync_app_models(ui: &MainWindow, list: &[AppSessionItem]) {
    let all = list.to_vec();
    let game: Vec<AppSessionItem> = list.iter().filter(|a| !a.is_chat).cloned().collect();
    let chat: Vec<AppSessionItem> = list.iter().filter(|a| a.is_chat).cloned().collect();

    ui.set_all_apps(ModelRc::new(VecModel::from(all)));
    ui.set_game_apps(ModelRc::new(VecModel::from(game)));
    ui.set_chat_apps(ModelRc::new(VecModel::from(chat)));
}

fn main() -> Result<(), slint::PlatformError> {
    // 0. Inisialisasi COM (STA) agar AudioDeviceManager dan Winit bisa berjalan harmonis di thread utama
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }

    // 1. Inisiasi Jendela Utama Slint
    let ui = MainWindow::new()?;

    // 2. Hubungkan ke Shared Memory dari Engine (atau buat jika UI start terlebih dahulu)
    let shared_mem = SharedMemory::open_or_create(auralis_ipc::shared_memory::SHM_NAME).ok();
    let shm_arc = shared_mem.map(Arc::new);

    // 3. Deteksi Perangkat Output Nyata Windows (Realtek, HyperX Cloud III, NVIDIA 24G4)
    let dev_manager = AudioDeviceManager::new().ok().map(Arc::new);
    let mut output_device_names: Vec<SharedString> = Vec::new();
    let mut default_output_idx = 0;

    if let Some(ref mgr) = dev_manager {
        if let Ok(devices) = mgr.enumerate_physical_devices() {
            let mut current_idx = 0;
            let mut matched = false;
            for dev in devices.into_iter() {
                let lower = dev.name.to_lowercase();
                if !matched && (lower.contains("hyperx") || lower.contains("headphone")) {
                    default_output_idx = current_idx;
                    matched = true;
                }
                output_device_names.push(SharedString::from(dev.name));
                current_idx += 1;
            }
        }
    }

    if output_device_names.is_empty() {
        output_device_names.push("No Output Devices Detected".into());
    }

    let out_names_for_cb = output_device_names.clone();
    ui.set_output_devices(ModelRc::new(VecModel::from(output_device_names)));
    ui.set_selected_output_index(default_output_idx);

    // 4. Deteksi Perangkat Input (Microphone) Nyata Windows
    let mut input_device_names: Vec<SharedString> = Vec::new();
    let mut default_input_idx = 0;

    if let Some(ref mgr) = dev_manager {
        if let Ok(mics) = mgr.enumerate_physical_capture_devices() {
            let mut current_idx = 0;
            let mut matched = false;
            for mic in mics.into_iter() {
                let lower = mic.name.to_lowercase();
                if !matched && (lower.contains("hyperx") || lower.contains("mic") || lower.contains("realtek")) {
                    default_input_idx = current_idx;
                    matched = true;
                }
                input_device_names.push(SharedString::from(mic.name));
                current_idx += 1;
            }
        }
    }

    if input_device_names.is_empty() {
        input_device_names.push("No Microphone Detected".into());
    }

    let in_names_for_cb = input_device_names.clone();
    ui.set_input_devices(ModelRc::new(VecModel::from(input_device_names)));
    ui.set_selected_input_index(default_input_idx);

    // Inisialisasi Master Volume dari Windows Endpoint
    let mut initial_master_vol = 0.8f32;
    if let Some(ref mgr) = dev_manager {
        if let Ok(def_dev) = mgr.get_default_render_device() {
            if let Ok(vol) = mgr.get_master_volume(&def_dev) {
                initial_master_vol = vol;
            }
        }
    }
    ui.set_master_volume(initial_master_vol);
    if let Some(ref shm) = shm_arc {
        shm.get().write_master_volume(initial_master_vol);
    }

    // Callback saat pengguna mengganti Output (Speaker / Headphone)
    let shm_output = shm_arc.clone();
    ui.on_output_device_selected(move |name: SharedString| {
        let mut selected_idx = 0;
        for (i, n) in out_names_for_cb.iter().enumerate() {
            if n == &name {
                selected_idx = i;
                break;
            }
        }
        println!("UI: Mengganti physical audio output ke index {}: {}", selected_idx, name);
        if let Some(ref shm) = shm_output {
            shm.get().set_target_device(selected_idx as u32);
        }
    });

    // Callback saat pengguna mengganti Input (Microphone)
    let shm_input = shm_arc.clone();
    ui.on_input_device_selected(move |name: SharedString| {
        let mut selected_idx = 0;
        for (i, n) in in_names_for_cb.iter().enumerate() {
            if n == &name {
                selected_idx = i;
                break;
            }
        }
        println!("UI: Mengganti physical microphone input ke index {}: {}", selected_idx, name);
        if let Some(ref shm) = shm_input {
            shm.get().set_target_input_device(selected_idx as u32);
        }
    });

    // Wire up Volume Faders & ChatMix
    let last_master_user_change = Arc::new(Mutex::new(std::time::Instant::now() - Duration::from_secs(10)));
    let last_master_change_cb = last_master_user_change.clone();
    let last_master_change_timer = last_master_user_change.clone();

    let shm_master = shm_arc.clone();
    let mgr_master = dev_manager.clone();
    ui.on_master_volume_changed(move |vol| {
        *last_master_change_cb.lock().unwrap() = std::time::Instant::now();
        if let Some(ref shm) = shm_master {
            shm.get().write_master_volume(vol);
        }
        if let Some(ref mgr) = mgr_master {
            if let Ok(def_dev) = mgr.get_default_render_device() {
                let _ = mgr.set_master_volume(&def_dev, vol);
            }
        }
    });

    let shm_game = shm_arc.clone();
    ui.on_game_volume_changed(move |vol| {
        if let Some(ref shm) = shm_game {
            shm.get().write_game_volume(vol);
        }
    });

    let shm_chat = shm_arc.clone();
    ui.on_chat_volume_changed(move |vol| {
        if let Some(ref shm) = shm_chat {
            shm.get().write_chat_volume(vol);
        }
    });

    let shm_mic = shm_arc.clone();
    ui.on_mic_volume_changed(move |vol| {
        if let Some(ref shm) = shm_mic {
            shm.get().write_mic_volume(vol);
        }
    });

    let shm_mix = shm_arc.clone();
    ui.on_chatmix_changed(move |bal| {
        if let Some(ref shm) = shm_mix {
            shm.get().write_chatmix_balance(bal);
        }
    });

    // 5. Deteksi Awal Sesi Audio Aplikasi Aktif
    let app_items_list = Arc::new(Mutex::new(Vec::<AppSessionItem>::new()));
    if let Some(ref mgr) = dev_manager {
        let initial_sessions = AudioSessionTracker::get_all_active_sessions(mgr);
        let mut items = Vec::new();
        for s in initial_sessions {
            let is_chat = s.current_channel == auralis_core::types::ChannelId::Chat;
            items.push(AppSessionItem {
                name: SharedString::from(s.display_name.clone()),
                is_chat,
                is_muted: false,
            });
            if let Some(ref shm) = shm_arc {
                shm.get_mut().set_app_routing(&s.display_name, is_chat);
            }
        }

        *app_items_list.lock().unwrap() = items.clone();
        sync_app_models(&ui, &items);
    }

    // Callback saat pengguna mengklik chip aplikasi di Master (Toggle Mute/Unmute per app)
    let app_items_mute = app_items_list.clone();
    let ui_handle_mute = ui.as_weak();
    let shm_mute = shm_arc.clone();
    ui.on_toggle_app_mute(move |name: SharedString| {
        let mut list = app_items_mute.lock().unwrap();
        if let Some(item) = list.iter_mut().find(|it| it.name == name) {
            item.is_muted = !item.is_muted;
            println!("App Audio: '{}' toggled mute -> {}", item.name, item.is_muted);
            if let Some(ref shm) = shm_mute {
                shm.get_mut().set_app_muted(item.name.as_str(), item.is_muted);
            }
            if let Some(ui) = ui_handle_mute.upgrade() {
                sync_app_models(&ui, &list);
            }
        }
    });

    // Callback saat pengguna mengklik chip aplikasi di Game atau Chat (Switch Channel Game <-> Chat)
    let app_items_switch = app_items_list.clone();
    let ui_handle_switch = ui.as_weak();
    let shm_switch = shm_arc.clone();
    ui.on_switch_app_channel(move |name: SharedString| {
        let mut list = app_items_switch.lock().unwrap();
        if let Some(item) = list.iter_mut().find(|it| it.name == name) {
            item.is_chat = !item.is_chat;
            println!("App Routing: '{}' switched to {}", item.name, if item.is_chat { "CHAT" } else { "GAME" });
            if let Some(ref shm) = shm_switch {
                shm.get_mut().set_app_routing(item.name.as_str(), item.is_chat);
            }
            if let Some(ui) = ui_handle_switch.upgrade() {
                sync_app_models(&ui, &list);
            }
        }
    });

    // 6. Setup Timer 60 FPS untuk merender VU Meter, Sync Windows Volume, dan Auto-detect Aplikasi
    let ui_weak = ui.as_weak();
    let vu_timer = Timer::default();
    let mgr_timer = dev_manager.clone();
    let shm_timer = shm_arc.clone();
    let app_items_timer = app_items_list.clone();
    let mut tick_counter: u64 = 0;

    let mut vu_game_l: f32 = 0.0;
    let mut vu_game_r: f32 = 0.0;
    let mut vu_chat_l: f32 = 0.0;
    let mut vu_chat_r: f32 = 0.0;
    let mut vu_master_l: f32 = 0.0;
    let mut vu_master_r: f32 = 0.0;
    let mut vu_mic: f32 = 0.0;
    
    vu_timer.start(TimerMode::Repeated, Duration::from_millis(16), move || {
        tick_counter = tick_counter.wrapping_add(1);

        if let Some(ui) = ui_weak.upgrade() {
            // A. Baca Peak VU Meters dari Shared Memory
            if let Some(ref shm) = shm_timer {
                let telemetry = shm.get();
                
                let (game_l, game_r) = telemetry.read_game_peak();
                let (chat_l, chat_r) = telemetry.read_chat_peak();
                let (master_l, master_r) = telemetry.read_master_peak();
                let mic_peak = telemetry.read_mic_peak();
                
                let to_level = |raw: f32, cur: &mut f32| -> f32 {
                    let target = if raw <= 0.003 {
                        0.0
                    } else {
                        let db = 20.0 * raw.log10();
                        let pct = (db + 42.0) / 42.0; // -42 dB studio dynamic range
                        pct.clamp(0.0, 1.0)
                    };
                    if target > *cur {
                        *cur = target; // Instant attack
                    } else {
                        *cur = *cur * 0.85; // Natural studio meter decay
                        if *cur < 0.01 {
                            *cur = 0.0;
                        }
                    }
                    *cur
                };

                ui.set_vu_master_l(to_level(master_l, &mut vu_master_l));
                ui.set_vu_master_r(to_level(master_r, &mut vu_master_r));
                ui.set_vu_game_l(to_level(game_l, &mut vu_game_l));
                ui.set_vu_game_r(to_level(game_r, &mut vu_game_r));
                ui.set_vu_chat_l(to_level(chat_l, &mut vu_chat_l));
                ui.set_vu_chat_r(to_level(chat_r, &mut vu_chat_r));
                ui.set_vu_mic(to_level(mic_peak, &mut vu_mic));
            }

            // B. Sinkronkan Master Volume dari Windows (setiap ~100ms / 6 ticks)
            if tick_counter % 6 == 0 {
                let user_touched = last_master_change_timer.lock().unwrap().elapsed() < Duration::from_millis(1000);
                if !user_touched {
                    if let Some(ref mgr) = mgr_timer {
                        if let Ok(def_dev) = mgr.get_default_render_device() {
                            if let Ok(win_vol) = mgr.get_master_volume(&def_dev) {
                                let cur_ui_vol = ui.get_master_volume();
                                if (win_vol - cur_ui_vol).abs() > 0.02 {
                                    ui.set_master_volume(win_vol);
                                    if let Some(ref shm) = shm_timer {
                                        shm.get().write_master_volume(win_vol);
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // C. Auto-refresh Deteksi Sesi Audio Aplikasi Aktif (setiap ~1 detik / 60 ticks)
            if tick_counter % 60 == 0 {
                if let Some(ref mgr) = mgr_timer {
                    let sessions = AudioSessionTracker::get_all_active_sessions(mgr);
                    if !sessions.is_empty() {
                        let mut list = app_items_timer.lock().unwrap();
                        let mut changed = list.len() != sessions.len();
                        if !changed {
                            for (i, s) in sessions.iter().enumerate() {
                                if list[i].name.as_str() != s.display_name {
                                    changed = true;
                                    break;
                                }
                            }
                        }
                        if changed {
                            let mut new_items = Vec::new();
                            for s in sessions {
                                let existing = list.iter().find(|it| it.name.as_str() == s.display_name);
                                let is_chat = existing
                                    .map(|it| it.is_chat)
                                    .unwrap_or(s.current_channel == auralis_core::types::ChannelId::Chat);
                                let is_muted = existing.map(|it| it.is_muted).unwrap_or(false);

                                new_items.push(AppSessionItem {
                                    name: SharedString::from(s.display_name.clone()),
                                    is_chat,
                                    is_muted,
                                });
                                if let Some(ref shm) = shm_timer {
                                    shm.get_mut().set_app_routing(&s.display_name, is_chat);
                                    shm.get_mut().set_app_muted(&s.display_name, is_muted);
                                }
                            }
                            *list = new_items.clone();
                            sync_app_models(&ui, &new_items);
                        }
                    }
                }
            }
        }
    });

    // 7. Jalankan UI Event Loop
    ui.run()
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};

    #[test]
    fn test_ui_dev_manager() {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        }
        let ui = MainWindow::new();
        println!("MainWindow::new() result: {:?}", ui.is_ok());
        let mgr = AudioDeviceManager::new();
        match mgr {
            Ok(ref m) => {
                println!("AudioDeviceManager::new() SUCCEEDED");
                let devs = m.enumerate_physical_devices();
                println!("enumerate_physical_devices: {:?}", devs);
                let mics = m.enumerate_physical_capture_devices();
                println!("enumerate_physical_capture_devices: {:?}", mics);
                let sessions = AudioSessionTracker::get_all_active_sessions(m);
                println!("sessions count: {}", sessions.len());
                for s in &sessions {
                    println!("Session: {} ({})", s.display_name, s.process_name);
                }
            }
            Err(ref e) => {
                println!("AudioDeviceManager::new() FAILED with error: {:?}", e);
            }
        }
    }
}
