use windows::Win32::Media::Audio::{
    eRender, eMultimedia, eCapture, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator,
    DEVICE_STATE_ACTIVE,
};
use windows::Win32::System::Com::{CoCreateInstance, CoTaskMemFree, CLSCTX_ALL, STGM_READ};
use windows::Win32::System::Com::StructuredStorage::{PropVariantToStringAlloc, PropVariantClear};
use windows::Win32::Foundation::PROPERTYKEY;
use windows::core::{Result, GUID};
use auralis_core::types::AudioEndpointInfo;

// PKEY_Device_FriendlyName: {a45c254e-df1c-4efd-8020-67d146a850e0}, 14
const PKEY_DEVICE_FRIENDLY_NAME: PROPERTYKEY = PROPERTYKEY {
    fmtid: GUID::from_u128(0xa45c254e_df1c_4efd_8020_67d146a850e0),
    pid: 14,
};

pub struct AudioDeviceManager {
    enumerator: IMMDeviceEnumerator,
}

impl AudioDeviceManager {
    pub fn new() -> Result<Self> {
        let enumerator: IMMDeviceEnumerator = unsafe {
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?
        };
        Ok(Self { enumerator })
    }

    /// Mendapatkan seluruh perangkat output audio aktif (Speaker, Headphone, DAC, Monitor Audio)
    pub fn enumerate_render_devices(&self) -> Result<Vec<AudioEndpointInfo>> {
        unsafe {
            let collection = self.enumerator.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)?;
            let count = collection.GetCount()?;
            let mut devices = Vec::with_capacity(count as usize);

            let default_device = self.get_default_render_device().ok();
            let default_id = default_device.and_then(|d| Self::get_device_id(&d).ok());

            for i in 0..count {
                if let Ok(device) = collection.Item(i) {
                    if let Ok(id) = Self::get_device_id(&device) {
                        let name = Self::get_device_friendly_name(&device).unwrap_or_else(|_| id.clone());
                        let is_default = default_id.as_deref() == Some(&id);

                        devices.push(AudioEndpointInfo {
                            id,
                            name,
                            is_default,
                        });
                    }
                }
            }

            Ok(devices)
        }
    }

    /// Mendapatkan seluruh perangkat output fisik nyata (mengabaikan virtual audio cable/device)
    pub fn enumerate_physical_devices(&self) -> Result<Vec<AudioEndpointInfo>> {
        let all = self.enumerate_render_devices()?;
        let mut physical = Vec::new();
        for dev in all {
            let lower = dev.name.to_lowercase();
            if lower.contains("cable") || lower.contains("sonar") || lower.contains("virtual") {
                continue;
            }
            physical.push(dev);
        }
        Ok(physical)
    }

    /// Mendapatkan seluruh perangkat input audio aktif (Microphone, Line In, dsb)
    pub fn enumerate_capture_devices(&self) -> Result<Vec<AudioEndpointInfo>> {
        unsafe {
            let collection = self.enumerator.EnumAudioEndpoints(eCapture, DEVICE_STATE_ACTIVE)?;
            let count = collection.GetCount()?;
            let mut devices = Vec::with_capacity(count as usize);

            let default_device = self.get_default_capture_device().ok();
            let default_id = default_device.and_then(|d| Self::get_device_id(&d).ok());

            for i in 0..count {
                if let Ok(device) = collection.Item(i) {
                    if let Ok(id) = Self::get_device_id(&device) {
                        let name = Self::get_device_friendly_name(&device).unwrap_or_else(|_| id.clone());
                        let is_default = default_id.as_deref() == Some(&id);

                        devices.push(AudioEndpointInfo {
                            id,
                            name,
                            is_default,
                        });
                    }
                }
            }

            Ok(devices)
        }
    }

    /// Mendapatkan seluruh perangkat input fisik (Microphone fisik nyata, abaikan virtual cable)
    pub fn enumerate_physical_capture_devices(&self) -> Result<Vec<AudioEndpointInfo>> {
        let all = self.enumerate_capture_devices()?;
        let mut physical = Vec::new();
        for dev in all {
            let lower = dev.name.to_lowercase();
            if lower.contains("cable") || lower.contains("sonar") || lower.contains("virtual") {
                continue;
            }
            physical.push(dev);
        }
        Ok(physical)
    }

    /// Mendapatkan perangkat output audio default
    pub fn get_default_render_device(&self) -> Result<IMMDevice> {
        unsafe {
            self.enumerator.GetDefaultAudioEndpoint(eRender, eMultimedia)
        }
    }

    /// Mendapatkan perangkat input audio default
    pub fn get_default_capture_device(&self) -> Result<IMMDevice> {
        unsafe {
            self.enumerator.GetDefaultAudioEndpoint(eCapture, eMultimedia)
        }
    }

    /// Mendapatkan perangkat berdasarkan Device ID string
    pub fn get_device_by_id(&self, id: &str) -> Result<IMMDevice> {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;

        let wide_id: Vec<u16> = OsStr::new(id).encode_wide().chain(std::iter::once(0)).collect();
        unsafe {
            self.enumerator.GetDevice(windows::core::PCWSTR(wide_id.as_ptr()))
        }
    }

    /// Mengambil ID perangkat
    pub fn get_device_id(device: &IMMDevice) -> Result<String> {
        unsafe {
            let id_pwstr = device.GetId()?;
            let id_string = id_pwstr.to_string()?;
            CoTaskMemFree(Some(id_pwstr.as_ptr() as *const _));
            Ok(id_string)
        }
    }

    /// Mengambil Nama Ramah Perangkat (Contoh: "Speakers (Realtek(R) Audio)", "Headphones (HyperX Cloud III)")
    pub fn get_device_friendly_name(device: &IMMDevice) -> Result<String> {
        unsafe {
            let store = device.OpenPropertyStore(STGM_READ)?;
            let mut prop_var = store.GetValue(&PKEY_DEVICE_FRIENDLY_NAME)?;
            
            let pwstr = match PropVariantToStringAlloc(&prop_var) {
                Ok(p) => p,
                Err(e) => {
                    let _ = PropVariantClear(&mut prop_var);
                    return Err(e);
                }
            };
            
            let name_string = pwstr.to_string()?;
            CoTaskMemFree(Some(pwstr.as_ptr() as *const _));
            let _ = PropVariantClear(&mut prop_var);

            Ok(name_string)
        }
    }

    /// Mencari virtual playback device (Auralis Game / CABLE Input / Sonar) sebagai sumber input loopback
    pub fn find_virtual_game_device(&self) -> Result<(IMMDevice, String)> {
        let devices = self.enumerate_render_devices()?;
        // Prioritas 1: Auralis Game (Driver resmi Auralis)
        if let Some(d) = devices.iter().find(|d| d.name.contains("Auralis Game")) {
            return Ok((self.get_device_by_id(&d.id)?, d.name.clone()));
        }
        // Prioritas 2: Virtual Audio Cable mandiri (misal VB-Cable "CABLE Input")
        if let Some(d) = devices.iter().find(|d| d.name.to_lowercase().contains("cable input")) {
            return Ok((self.get_device_by_id(&d.id)?, d.name.clone()));
        }
        // Prioritas 3: Sonar Gaming (fallback alternatif)
        if let Some(d) = devices.iter().find(|d| d.name.contains("Sonar - Gaming")) {
            return Ok((self.get_device_by_id(&d.id)?, d.name.clone()));
        }
        // Fallback: Default Render device
        let default_device = self.get_default_render_device()?;
        let name = Self::get_device_friendly_name(&default_device).unwrap_or_else(|_| "Default".to_string());
        Ok((default_device, name))
    }

    /// Mencari output fisik (Hardware speaker / headphone) untuk memutar hasil mixing DSP
    pub fn find_physical_output_device(&self, preferred_id: Option<&str>) -> Result<(IMMDevice, String)> {
        let devices = self.enumerate_render_devices()?;
        if let Some(id) = preferred_id {
            if let Some(d) = devices.iter().find(|d| d.id == id) {
                return Ok((self.get_device_by_id(&d.id)?, d.name.clone()));
            }
        }
        // Prioritas 1: Headphone / Headset hardware (misal HyperX Cloud III)
        if let Some(d) = devices.iter().find(|d| {
            let lower = d.name.to_lowercase();
            (lower.contains("headphone") || lower.contains("hyperx")) && !lower.contains("virtual")
        }) {
            return Ok((self.get_device_by_id(&d.id)?, d.name.clone()));
        }
        // Prioritas 2: Speaker atau monitor audio fisik lainnya
        if let Some(d) = devices.iter().find(|d| {
            let lower = d.name.to_lowercase();
            !lower.contains("virtual") && !lower.contains("sonar") && !lower.contains("vb-audio") && !lower.contains("auralis")
        }) {
            return Ok((self.get_device_by_id(&d.id)?, d.name.clone()));
        }
        // Fallback default
        let default_device = self.get_default_render_device()?;
        let name = Self::get_device_friendly_name(&default_device).unwrap_or_else(|_| "Default".to_string());
        Ok((default_device, name))
    }

    /// Mengambil master volume Windows (0.0 sampai 1.0)
    pub fn get_master_volume(&self, device: &IMMDevice) -> Result<f32> {
        unsafe {
            use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
            let ep_vol: IAudioEndpointVolume = device.Activate(CLSCTX_ALL, None)?;
            let level = ep_vol.GetMasterVolumeLevelScalar()?;
            Ok(level)
        }
    }

    /// Mengatur master volume Windows (0.0 sampai 1.0)
    pub fn set_master_volume(&self, device: &IMMDevice, level: f32) -> Result<()> {
        unsafe {
            use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
            let ep_vol: IAudioEndpointVolume = device.Activate(CLSCTX_ALL, None)?;
            ep_vol.SetMasterVolumeLevelScalar(level, std::ptr::null())?;
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};

    #[test]
    fn test_enumerate_render_endpoints() {
        unsafe { let _ = CoInitializeEx(None, COINIT_MULTITHREADED); }
        let manager = AudioDeviceManager::new().expect("Failed to create AudioDeviceManager");
        let devices = manager.enumerate_render_devices().expect("Failed to enumerate devices");
        
        println!("\n=== DETECTED PHYSICAL & VIRTUAL OUTPUT DEVICES ===");
        for dev in &devices {
            println!("- Name: {} (Default: {})", dev.name, dev.is_default);
        }
        println!("==================================================\n");

        assert!(!devices.is_empty(), "Should detect at least one audio output device");
    }
}

