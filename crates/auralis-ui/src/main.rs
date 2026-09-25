use auralis_ipc::shared_memory::SharedMemory;
use auralis_wasapi::device::AudioDeviceManager;
use auralis_wasapi::sessions::AudioSessionTracker;
use slint::{Timer, TimerMode, ModelRc, VecModel, SharedString};
use std::time::Duration;
use std::sync::{Arc, Mutex};

slint::include_modules!();

pub mod tray;

use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};

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
    let mut device_names: Vec<SharedString> = Vec::new();
    let mut default_idx = 0;

    if let Some(ref mgr) = dev_manager {
        if let Ok(devices) = mgr.enumerate_physical_devices() {
            let mut current_idx = 0;
            let mut matched = false;
            for dev in devices.into_iter() {
                let lower = dev.name.to_lowercase();
                if !matched && (lower.contains("hyperx") || lower.contains("headphone")) {
                    default_idx = current_idx;
                    matched = true;
                }
                device_names.push(SharedString::from(dev.name));
                current_idx += 1;
            }
        }
    }

    if device_names.is_empty() {
        device_names.push("No Devices Detected".into());
    }

    let dev_names_for_cb = device_names.clone();
    ui.set_output_devices(ModelRc::new(VecModel::from(device_names)));
    ui.set_selected_device_index(default_idx);

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

    // Callback saat pengguna mengganti speaker/headphone
    let shm_dev = shm_arc.clone();
    ui.on_device_selected(move |name: SharedString| {
        let mut selected_idx = 0;
        for (i, n) in dev_names_for_cb.iter().enumerate() {
            if n == &name {
                selected_idx = i;
                break;
            }
        }
        println!("UI: Mengganti physical audio output ke index {}: {}", selected_idx, name);
        if let Some(ref shm) = shm_dev {
            shm.get().set_target_device(selected_idx as u32);
        }
    });

    // Wire up Real-time Sliders (Game, Chat, Master, ChatMix) ke Shared Memory
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
        // Sinkronkan perubahan volume ke Windows Master Endpoint
        if let Some(ref mgr) = mgr_master {
            if let Ok(def_dev) = mgr.get_default_render_device() {
                let _ = mgr.set_master_volume(&def_dev, vol);
            }
        }
    });

    let shm_mix = shm_arc.clone();
    ui.on_chatmix_changed(move |bal| {
        if let Some(ref shm) = shm_mix {
            shm.get().write_chatmix_balance(bal);
        }
    });

    // 4. Deteksi Sesi Audio Aplikasi Aktif (Spotify, Chrome, Discord, Games)
    let app_items_list = Arc::new(Mutex::new(Vec::<AppSessionItem>::new()));
    if let Some(ref mgr) = dev_manager {
        let initial_sessions = AudioSessionTracker::get_all_active_sessions(mgr);
        let mut items = Vec::new();
        for s in initial_sessions {
            let is_chat = s.current_channel == auralis_core::types::ChannelId::Chat;
            items.push(AppSessionItem {
                name: SharedString::from(s.display_name.clone()),
                is_chat,
            });
            if let Some(ref shm) = shm_arc {
                shm.get_mut().set_app_routing(&s.display_name, is_chat);
            }
        }

        *app_items_list.lock().unwrap() = items.clone();
        ui.set_app_sessions(ModelRc::new(VecModel::from(items)));
    }

    // Callback saat pengguna mengubah rute aplikasi (Game vs Chat)
    let app_items_cb = app_items_list.clone();
    let ui_handle_cb = ui.as_weak();
    let shm_route = shm_arc.clone();
    ui.on_set_app_channel(move |idx, is_chat| {
        let mut list = app_items_cb.lock().unwrap();
        if let Some(item) = list.get_mut(idx as usize) {
            item.is_chat = is_chat;
            println!("App Routing: '{}' moved to {} channel", item.name, if is_chat { "CHAT" } else { "GAME" });
            if let Some(ref shm) = shm_route {
                shm.get_mut().set_app_routing(item.name.as_str(), is_chat);
            }
            if let Some(ui) = ui_handle_cb.upgrade() {
                ui.set_app_sessions(ModelRc::new(VecModel::from(list.clone())));
            }
        }
    });

    // 5. Setup Timer 60 FPS untuk merender VU Meter, Sync Windows Volume, dan Auto-detect Aplikasi
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
    
    vu_timer.start(TimerMode::Repeated, Duration::from_millis(16), move || {
        tick_counter += 1;
        if let Some(ui) = ui_weak.upgrade() {
            // A. Update VU Meter 60 FPS dengan Decay Halus
            if let Some(ref shm) = shm_timer {
                let telemetry = shm.get();
                
                let (game_l, game_r) = telemetry.read_game_peak();
                let (chat_l, chat_r) = telemetry.read_chat_peak();
                let (master_l, master_r) = telemetry.read_master_peak();
                
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

                ui.set_vu_game_l(to_level(game_l, &mut vu_game_l));
                ui.set_vu_game_r(to_level(game_r, &mut vu_game_r));
                ui.set_vu_chat_l(to_level(chat_l, &mut vu_chat_l));
                ui.set_vu_chat_r(to_level(chat_r, &mut vu_chat_r));
                ui.set_vu_master_l(to_level(master_l, &mut vu_master_l));
                ui.set_vu_master_r(to_level(master_r, &mut vu_master_r));
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
                                let is_chat = list
                                    .iter()
                                    .find(|it| it.name.as_str() == s.display_name)
                                    .map(|it| it.is_chat)
                                    .unwrap_or(s.current_channel == auralis_core::types::ChannelId::Chat);

                                new_items.push(AppSessionItem {
                                    name: SharedString::from(s.display_name.clone()),
                                    is_chat,
                                });
                                if let Some(ref shm) = shm_timer {
                                    shm.get_mut().set_app_routing(&s.display_name, is_chat);
                                }
                            }
                            *list = new_items.clone();
                            ui.set_app_sessions(ModelRc::new(VecModel::from(new_items)));
                        }
                    }
                }
            }
        }
    });

    // 6. Jalankan UI Event Loop
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

