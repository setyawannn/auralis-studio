use windows_sys::Win32::System::Memory::{
    CreateFileMappingW, MapViewOfFile, UnmapViewOfFile, OpenFileMappingW,
    FILE_MAP_ALL_ACCESS, PAGE_READWRITE, MEMORY_MAPPED_VIEW_ADDRESS,
};
use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, HANDLE, INVALID_HANDLE_VALUE};
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::ptr::NonNull;
use auralis_core::telemetry::{SharedAudioTelemetry, TELEMETRY_MAGIC};

pub const SHM_NAME: &str = r"Local\AuralisAudioState";

fn to_wstring(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}

pub struct SharedMemory {
    handle: HANDLE,
    ptr: NonNull<SharedAudioTelemetry>,
    #[allow(dead_code)]
    is_owner: bool,
}

impl SharedMemory {
    /// Create new shared memory (used by Engine)
    pub fn create(name: &str) -> Result<Self, u32> {
        let name_w = to_wstring(name);
        let size = std::mem::size_of::<SharedAudioTelemetry>() as u32;

        let handle = unsafe {
            CreateFileMappingW(
                INVALID_HANDLE_VALUE,
                std::ptr::null(),
                PAGE_READWRITE,
                0,
                size,
                name_w.as_ptr(),
            )
        };

        if handle.is_null() || handle == INVALID_HANDLE_VALUE {
            return Err(unsafe { GetLastError() });
        }

        let view = unsafe { MapViewOfFile(handle, FILE_MAP_ALL_ACCESS, 0, 0, size as usize) };
        if view.Value.is_null() {
            unsafe { CloseHandle(handle) };
            return Err(unsafe { GetLastError() });
        }

        let telemetry_ptr = NonNull::new(view.Value as *mut SharedAudioTelemetry).unwrap();
        
        // Initialize if we are the creator
        unsafe {
            std::ptr::write(telemetry_ptr.as_ptr(), SharedAudioTelemetry::new());
        }

        Ok(Self { handle, ptr: telemetry_ptr, is_owner: true })
    }

    /// Open existing or create shared memory if not yet created (bidirectional UI <-> Engine resilience)
    pub fn open_or_create(name: &str) -> Result<Self, u32> {
        let name_w = to_wstring(name);
        let size = std::mem::size_of::<SharedAudioTelemetry>() as u32;

        let handle = unsafe {
            CreateFileMappingW(
                INVALID_HANDLE_VALUE,
                std::ptr::null(),
                PAGE_READWRITE,
                0,
                size,
                name_w.as_ptr(),
            )
        };

        if handle.is_null() || handle == INVALID_HANDLE_VALUE {
            return Err(unsafe { GetLastError() });
        }

        let already_exists = unsafe { GetLastError() } == windows_sys::Win32::Foundation::ERROR_ALREADY_EXISTS;

        let view = unsafe { MapViewOfFile(handle, FILE_MAP_ALL_ACCESS, 0, 0, size as usize) };
        if view.Value.is_null() {
            unsafe { CloseHandle(handle) };
            return Err(unsafe { GetLastError() });
        }

        let telemetry_ptr = NonNull::new(view.Value as *mut SharedAudioTelemetry).unwrap();

        if !already_exists {
            unsafe {
                std::ptr::write(telemetry_ptr.as_ptr(), SharedAudioTelemetry::new());
            }
        }

        Ok(Self { handle, ptr: telemetry_ptr, is_owner: !already_exists })
    }

    /// Open existing shared memory (used by UI)
    pub fn open(name: &str) -> Result<Self, u32> {
        let name_w = to_wstring(name);
        let size = std::mem::size_of::<SharedAudioTelemetry>() as u32;

        let handle = unsafe { OpenFileMappingW(FILE_MAP_ALL_ACCESS, 0, name_w.as_ptr()) };
        if handle.is_null() {
            return Err(unsafe { GetLastError() });
        }

        let view = unsafe { MapViewOfFile(handle, FILE_MAP_ALL_ACCESS, 0, 0, size as usize) };
        if view.Value.is_null() {
            unsafe { CloseHandle(handle) };
            return Err(unsafe { GetLastError() });
        }

        let telemetry_ptr = NonNull::new(view.Value as *mut SharedAudioTelemetry).unwrap();

        // Verify magic
        unsafe {
            let magic = (*telemetry_ptr.as_ptr()).magic;
            if magic != TELEMETRY_MAGIC {
                UnmapViewOfFile(view);
                CloseHandle(handle);
                return Err(0); // Invalid format
            }
        }

        Ok(Self { handle, ptr: telemetry_ptr, is_owner: false })
    }

    pub fn get(&self) -> &SharedAudioTelemetry {
        unsafe { self.ptr.as_ref() }
    }

    #[allow(clippy::mut_from_ref)]
    pub fn get_mut(&self) -> &mut SharedAudioTelemetry {
        unsafe { &mut *self.ptr.as_ptr() }
    }
}

impl Drop for SharedMemory {
    fn drop(&mut self) {
        unsafe {
            let view = MEMORY_MAPPED_VIEW_ADDRESS { Value: self.ptr.as_ptr() as *mut _ };
            UnmapViewOfFile(view);
            if !self.handle.is_null() && self.handle != INVALID_HANDLE_VALUE {
                CloseHandle(self.handle);
            }
        }
    }
}

// Allow sending pointer between threads
unsafe impl Send for SharedMemory {}
unsafe impl Sync for SharedMemory {}
