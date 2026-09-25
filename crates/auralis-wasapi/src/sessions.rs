use windows::Win32::Media::Audio::{
    IAudioSessionManager2, IAudioSessionControl2, IMMDevice,
};
use windows::Win32::System::Com::CLSCTX_ALL;
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_FORMAT,
};
use windows::Win32::Foundation::CloseHandle;
use windows::core::{Result, Interface};
use auralis_core::types::ChannelId;
use auralis_ipc::protocol::AppAudioSession;
use std::path::Path;

const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;

pub struct AudioSessionTracker;

impl AudioSessionTracker {
    /// Mengambil seluruh sesi audio aktif dari SEMUA endpoint output (CABLE, Default, Speakers, dll)
    pub fn get_all_active_sessions(manager: &crate::device::AudioDeviceManager) -> Vec<AppAudioSession> {
        let mut all_sessions = Vec::new();
        let mut seen_names = std::collections::HashSet::new();

        if let Ok(devices) = manager.enumerate_render_devices() {
            for dev_info in devices {
                if let Ok(imm_dev) = manager.get_device_by_id(&dev_info.id) {
                    if let Ok(sessions) = Self::get_active_sessions(&imm_dev) {
                        for s in sessions {
                            let lower = s.process_name.to_lowercase();
                            // Abaikan audiodg / auralis internal / antigravity
                            if lower.contains("audiodg") || lower.contains("auralis") || lower.contains("antigravity") {
                                continue;
                            }
                            // Deduplikasi berdasarkan nama aplikasi
                            if seen_names.insert(lower) {
                                all_sessions.push(s);
                            }
                        }
                    }
                }
            }
        }

        all_sessions
    }

    /// Mengambil seluruh sesi audio aplikasi yang sedang aktif di Windows untuk suatu perangkat
    pub fn get_active_sessions(device: &IMMDevice) -> Result<Vec<AppAudioSession>> {
        let mut app_sessions = Vec::new();

        unsafe {
            let session_manager: IAudioSessionManager2 = device.Activate(CLSCTX_ALL, None)?;
            let enumerator = session_manager.GetSessionEnumerator()?;
            let count = enumerator.GetCount()?;

            for i in 0..count {
                if let Ok(control) = enumerator.GetSession(i) {
                    if let Ok(control2) = control.cast::<IAudioSessionControl2>() {
                        // Cek apakah sesi ini bukan system sound internal
                        if control2.IsSystemSoundsSession().is_ok() {
                            // Abaikan atau proses jika perlu
                        }

                        if let Ok(pid) = control2.GetProcessId() {
                            if pid == 0 {
                                continue; // Idle / system process
                            }

                            let process_name = Self::get_process_name(pid)
                                .unwrap_or_else(|| format!("Process ({})", pid));

                            let display_name = control
                                .GetDisplayName()
                                .map(|pwstr| pwstr.to_string().unwrap_or_default())
                                .unwrap_or_default();

                            let final_display = if display_name.is_empty() {
                                process_name.clone()
                            } else {
                                display_name
                            };

                            // Klasifikasikan default channel berdasarkan nama proses
                            let lower = process_name.to_lowercase();
                            let current_channel = if lower.contains("discord") || lower.contains("teamspeak") || lower.contains("skype") || lower.contains("zoom") {
                                ChannelId::Chat
                            } else {
                                ChannelId::Game
                            };

                            app_sessions.push(AppAudioSession {
                                process_id: pid,
                                process_name,
                                display_name: final_display,
                                current_channel,
                                peak_volume: 0.8,
                            });
                        }
                    }
                }
            }
        }

        Ok(app_sessions)
    }

    /// Mengambil nama executable file dari PID (misal: "chrome.exe", "Discord.exe")
    pub fn get_process_name(pid: u32) -> Option<String> {
        unsafe {
            let handle_res = OpenProcess(
                windows::Win32::System::Threading::PROCESS_ACCESS_RIGHTS(PROCESS_QUERY_LIMITED_INFORMATION),
                false,
                pid,
            );

            let handle = match handle_res {
                Ok(h) if !h.0.is_null() => h,
                _ => return None,
            };

            let mut buffer = [0u16; 1024];
            let mut size = buffer.len() as u32;

            let success = QueryFullProcessImageNameW(
                handle,
                PROCESS_NAME_FORMAT(0),
                windows::core::PWSTR(buffer.as_mut_ptr()),
                &mut size,
            );

            let _ = CloseHandle(handle);

            if success.is_ok() && size > 0 {
                let full_path = String::from_utf16_lossy(&buffer[..size as usize]);
                let file_name = Path::new(&full_path)
                    .file_name()
                    .and_then(|f| f.to_str())
                    .map(|s| s.to_string());
                return file_name;
            }

            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::AudioDeviceManager;
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};

    #[test]
    fn test_active_audio_sessions() {
        unsafe { let _ = CoInitializeEx(None, COINIT_MULTITHREADED); }
        let manager = AudioDeviceManager::new().expect("Failed to create AudioDeviceManager");
        let default_device = manager.get_default_render_device().expect("Failed to get default device");
        
        let sessions = AudioSessionTracker::get_active_sessions(&default_device)
            .expect("Failed to get active sessions");

        println!("\n=== DETECTED RUNNING AUDIO APPS ===");
        for s in &sessions {
            println!("- App: {} (PID: {}, Channel: {:?})", s.display_name, s.process_id, s.current_channel);
        }
        println!("===================================\n");

        let all = AudioSessionTracker::get_all_active_sessions(&manager);
        println!("\n=== ALL ACTIVE SESSIONS ACROSS ALL ENDPOINTS ===");
        for s in &all {
            println!("* All App: {} (PID: {})", s.display_name, s.process_id);
        }
        println!("================================================\n");
    }
}

