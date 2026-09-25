use windows::Win32::Media::Audio::{
    IAudioClient, IAudioRenderClient, IAudioCaptureClient, AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
    AUDCLNT_STREAMFLAGS_LOOPBACK, AUDCLNT_SHAREMODE_SHARED,
};
use windows::Win32::System::Com::{CLSCTX_ALL, CoTaskMemFree};
use windows::Win32::System::Threading::{
    CreateEventW, WaitForSingleObject, SetEvent, SetThreadPriority, GetCurrentThread,
    THREAD_PRIORITY_TIME_CRITICAL,
};
use windows::Win32::Foundation::{HANDLE, CloseHandle, INVALID_HANDLE_VALUE, WAIT_OBJECT_0};
use windows::core::{Result, Interface};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use auralis_core::constants::{SAMPLE_RATE, FRAMES_PER_BUFFER};

/// Trait untuk menyuntikkan callback DSP pemrosesan real-time (Render/Output)
pub trait AudioRenderCallback: Send + 'static {
    fn process_render(&mut self, buffer: &mut [f32]);
}

/// Trait untuk menyuntikkan callback penangkapan audio (Capture/Input dari Virtual Endpoint)
pub trait AudioCaptureCallback: Send + 'static {
    fn process_capture(&mut self, buffer: &[f32]);
}

pub struct WasapiRenderStream {
    audio_client: IAudioClient,
    render_client: IAudioRenderClient,
    event_handle: HANDLE,
    is_running: Arc<AtomicBool>,
}

impl WasapiRenderStream {
    pub fn new(device: &windows::Win32::Media::Audio::IMMDevice) -> Result<Self> {
        unsafe {
            let audio_client: IAudioClient = device.Activate(CLSCTX_ALL, None)?;

            // Ambil format audio native yang didukung endpoint hardware Windows
            let mix_format = audio_client.GetMixFormat()?;
            let sample_rate = (*mix_format).nSamplesPerSec;
            let channels = (*mix_format).nChannels;
            let bits = (*mix_format).wBitsPerSample;
            println!(
                "Render endpoint native format: {} Hz, {} ch, {} bit",
                sample_rate, channels, bits
            );

            let reference_time = (FRAMES_PER_BUFFER as i64 * 10_000_000) / (SAMPLE_RATE as i64);

            let init_res = audio_client.Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
                reference_time,
                0,
                mix_format,
                None,
            );
            
            // Bebaskan memory mix_format yang dialokasikan oleh Windows COM
            CoTaskMemFree(Some(mix_format as *const _));
            init_res?;

            let event_handle = CreateEventW(None, false, false, None)?;
            audio_client.SetEventHandle(event_handle)?;

            let render_client: IAudioRenderClient = audio_client.GetService()?;

            Ok(Self {
                audio_client,
                render_client,
                event_handle,
                is_running: Arc::new(AtomicBool::new(false)),
            })
        }
    }

    pub fn start<C: AudioRenderCallback>(&mut self, mut callback: C) -> Result<()> {
        self.is_running.store(true, Ordering::Release);
        
        let thread_audio_client = self.audio_client.clone();
        let thread_render_client = self.render_client.clone();

        let client_raw = thread_audio_client.as_raw() as usize;
        let render_raw = thread_render_client.as_raw() as usize;
        let event_raw = self.event_handle.0 as usize;
        let is_running = self.is_running.clone();

        unsafe {
            std::mem::forget(thread_audio_client);
            std::mem::forget(thread_render_client);

            let buffer_frame_count = self.audio_client.GetBufferSize()?;
            self.audio_client.Start()?;

            std::thread::spawn(move || {
                let audio_client: IAudioClient = std::mem::transmute(client_raw as *mut std::ffi::c_void);
                let render_client: IAudioRenderClient = std::mem::transmute(render_raw as *mut std::ffi::c_void);
                let event_handle = HANDLE(event_raw as *mut std::ffi::c_void);

                let _ = SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_TIME_CRITICAL);

                while is_running.load(Ordering::Acquire) {
                    let wait_res = WaitForSingleObject(event_handle, 2000);
                    if wait_res != WAIT_OBJECT_0 {
                        break;
                    }

                    let padding = match audio_client.GetCurrentPadding() {
                        Ok(p) => p,
                        Err(_) => break,
                    };

                    let frames_available = buffer_frame_count - padding;
                    if frames_available == 0 {
                        continue;
                    }

                    let data_ptr = match render_client.GetBuffer(frames_available) {
                        Ok(ptr) => ptr,
                        Err(_) => break,
                    };

                    let num_samples = (frames_available * 2) as usize;
                    let buffer_slice = std::slice::from_raw_parts_mut(data_ptr as *mut f32, num_samples);

                    callback.process_render(buffer_slice);
                    
                    let _ = render_client.ReleaseBuffer(frames_available, 0);
                }

                let _ = audio_client.Stop();
                // Biarkan Drop berjalan secara natural untuk memanggil COM Release()
            });
        }
        Ok(())
    }

    pub fn stop(&self) {
        self.is_running.store(false, Ordering::Release);
        unsafe { let _ = SetEvent(self.event_handle); }
    }
}

impl Drop for WasapiRenderStream {
    fn drop(&mut self) {
        self.stop();
        if !self.event_handle.0.is_null() && self.event_handle != INVALID_HANDLE_VALUE {
            unsafe { let _ = CloseHandle(self.event_handle); }
        }
    }
}

pub struct WasapiCaptureStream {
    audio_client: IAudioClient,
    capture_client: IAudioCaptureClient,
    event_handle: HANDLE,
    channels: u16,
    sample_rate: u32,
    is_running: Arc<AtomicBool>,
}

impl WasapiCaptureStream {
    pub fn new(device: &windows::Win32::Media::Audio::IMMDevice) -> Result<Self> {
        Self::create_internal(device, false)
    }

    /// Membuat capture stream dalam mode loopback (menangkap suara yang dimainkan di render endpoint)
    pub fn new_loopback(device: &windows::Win32::Media::Audio::IMMDevice) -> Result<Self> {
        Self::create_internal(device, true)
    }

    fn create_internal(device: &windows::Win32::Media::Audio::IMMDevice, loopback: bool) -> Result<Self> {
        unsafe {
            let audio_client: IAudioClient = device.Activate(CLSCTX_ALL, None)?;

            let mix_format = audio_client.GetMixFormat()?;
            let sample_rate = (*mix_format).nSamplesPerSec;
            let channels = (*mix_format).nChannels;
            let bits = (*mix_format).wBitsPerSample;
            println!(
                "{} endpoint native format: {} Hz, {} ch, {} bit",
                if loopback { "Loopback capture" } else { "Capture" },
                sample_rate, channels, bits
            );

            let reference_time = (FRAMES_PER_BUFFER as i64 * 10_000_000) / (SAMPLE_RATE as i64);

            let mut flags = AUDCLNT_STREAMFLAGS_EVENTCALLBACK;
            if loopback {
                flags |= AUDCLNT_STREAMFLAGS_LOOPBACK;
            }

            let init_res = audio_client.Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                flags,
                reference_time,
                0,
                mix_format,
                None,
            );
            
            CoTaskMemFree(Some(mix_format as *const _));
            init_res?;

            let event_handle = CreateEventW(None, false, false, None)?;
            audio_client.SetEventHandle(event_handle)?;

            let capture_client: IAudioCaptureClient = audio_client.GetService()?;

            Ok(Self {
                audio_client,
                capture_client,
                event_handle,
                channels,
                sample_rate,
                is_running: Arc::new(AtomicBool::new(false)),
            })
        }
    }

    pub fn start<C: AudioCaptureCallback>(&mut self, mut callback: C) -> Result<()> {
        self.is_running.store(true, Ordering::Release);
        
        let thread_audio_client = self.audio_client.clone();
        let thread_capture_client = self.capture_client.clone();

        let client_raw = thread_audio_client.as_raw() as usize;
        let capture_raw = thread_capture_client.as_raw() as usize;
        let event_raw = self.event_handle.0 as usize;
        let channels = self.channels as usize;
        let sample_rate = self.sample_rate;
        let is_running = self.is_running.clone();

        unsafe {
            std::mem::forget(thread_audio_client);
            std::mem::forget(thread_capture_client);

            self.audio_client.Start()?;

            std::thread::spawn(move || {
                let audio_client: IAudioClient = std::mem::transmute(client_raw as *mut std::ffi::c_void);
                let capture_client: IAudioCaptureClient = std::mem::transmute(capture_raw as *mut std::ffi::c_void);
                let event_handle = HANDLE(event_raw as *mut std::ffi::c_void);

                let _ = SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_TIME_CRITICAL);

                // Buffer pre-alokasi untuk downmixing ke 48kHz Stereo (tanpa alokasi memori di loop)
                let mut stereo_buf = vec![0.0f32; 8192];
                let step = if sample_rate >= 96000 { 2 } else { 1 };

                while is_running.load(Ordering::Acquire) {
                    let wait_res = WaitForSingleObject(event_handle, 50);
                    if wait_res != WAIT_OBJECT_0 {
                        // Pada WASAPI Loopback, saat tidak ada suara/audio berhenti,
                        // event tidak akan disignal (timeout). Loop harus tetap jalan menunggu audio baru.
                        continue;
                    }

                    loop {
                        let mut p_data = std::ptr::null_mut();
                        let mut num_frames_to_read = 0;
                        let mut flags = 0;
                        let mut device_position = 0;
                        let mut qpc_position = 0;

                        let hr = capture_client.GetBuffer(
                            &mut p_data,
                            &mut num_frames_to_read,
                            &mut flags,
                            Some(&mut device_position),
                            Some(&mut qpc_position),
                        );

                        if hr.is_err() || num_frames_to_read == 0 {
                            break;
                        }

                        let p_float = p_data as *const f32;
                        let mut out_idx = 0;

                        for frame_idx in (0..num_frames_to_read as usize).step_by(step) {
                            let base = frame_idx * channels;
                            let l = *p_float.add(base);
                            let r = if channels > 1 { *p_float.add(base + 1) } else { l };

                            if out_idx + 1 < stereo_buf.len() {
                                stereo_buf[out_idx] = l;
                                stereo_buf[out_idx + 1] = r;
                                out_idx += 2;
                            }
                        }

                        if out_idx > 0 {
                            callback.process_capture(&stereo_buf[..out_idx]);
                        }

                        let _ = capture_client.ReleaseBuffer(num_frames_to_read);
                    }
                }

                let _ = audio_client.Stop();
                // Biarkan Drop berjalan secara natural
            });
        }
        Ok(())
    }

    pub fn stop(&self) {
        self.is_running.store(false, Ordering::Release);
        unsafe { let _ = SetEvent(self.event_handle); }
    }
}

impl Drop for WasapiCaptureStream {
    fn drop(&mut self) {
        self.stop();
        if !self.event_handle.0.is_null() && self.event_handle != INVALID_HANDLE_VALUE {
            unsafe { let _ = CloseHandle(self.event_handle); }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::AudioDeviceManager;
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};

    #[test]
    fn test_stream_formats() {
        unsafe { let _ = CoInitializeEx(None, COINIT_MULTITHREADED); }
        let mgr = AudioDeviceManager::new().expect("device manager");
        let (virtual_dev, vname) = mgr.find_virtual_game_device().expect("virtual dev");
        let (phys_dev, pname) = mgr.find_physical_output_device(None).expect("physical dev");
        println!("\nTesting Virtual Dev: {}", vname);
        let cap = WasapiCaptureStream::new_loopback(&virtual_dev);
        println!("WasapiCaptureStream loopback result: {:?}", cap.is_ok());

        println!("Testing Physical Dev: {}", pname);
        let ren = WasapiRenderStream::new(&phys_dev);
        println!("WasapiRenderStream result: {:?}", ren.is_ok());
    }
}

