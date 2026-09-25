use windows_sys::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe,
    PIPE_READMODE_MESSAGE, PIPE_TYPE_MESSAGE, PIPE_WAIT,
};
use windows_sys::Win32::Storage::FileSystem::{
    ReadFile, WriteFile, PIPE_ACCESS_DUPLEX,
};
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, HANDLE, INVALID_HANDLE_VALUE, ERROR_PIPE_CONNECTED,
};
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;

pub const PIPE_NAME: &str = r"\\.\pipe\auralis-control";

fn to_wstring(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}

pub struct NamedPipeServer {
    handle: HANDLE,
}

impl NamedPipeServer {
    pub fn new(name: &str) -> Result<Self, u32> {
        let name_w = to_wstring(name);
        let handle = unsafe {
            CreateNamedPipeW(
                name_w.as_ptr(),
                PIPE_ACCESS_DUPLEX,
                PIPE_TYPE_MESSAGE | PIPE_READMODE_MESSAGE | PIPE_WAIT,
                1, // max instances
                4096, // out buffer
                4096, // in buffer
                0, // default time out
                std::ptr::null(), // security attributes
            )
        };

        if handle == INVALID_HANDLE_VALUE {
            return Err(unsafe { GetLastError() });
        }

        Ok(Self { handle })
    }

    pub fn wait_for_client(&self) -> Result<(), u32> {
        let connected = unsafe { ConnectNamedPipe(self.handle, std::ptr::null_mut()) };
        if connected == 0 {
            let err = unsafe { GetLastError() };
            if err == ERROR_PIPE_CONNECTED {
                return Ok(());
            }
            return Err(err);
        }
        Ok(())
    }

    pub fn read(&self, buffer: &mut [u8]) -> Result<usize, u32> {
        let mut bytes_read = 0;
        let success = unsafe {
            ReadFile(
                self.handle,
                buffer.as_mut_ptr() as *mut _,
                buffer.len() as u32,
                &mut bytes_read,
                std::ptr::null_mut(),
            )
        };

        if success == 0 {
            Err(unsafe { GetLastError() })
        } else {
            Ok(bytes_read as usize)
        }
    }

    pub fn write(&self, buffer: &[u8]) -> Result<usize, u32> {
        let mut bytes_written = 0;
        let success = unsafe {
            WriteFile(
                self.handle,
                buffer.as_ptr() as *const _,
                buffer.len() as u32,
                &mut bytes_written,
                std::ptr::null_mut(),
            )
        };

        if success == 0 {
            Err(unsafe { GetLastError() })
        } else {
            Ok(bytes_written as usize)
        }
    }
}

impl Drop for NamedPipeServer {
    fn drop(&mut self) {
        if self.handle != INVALID_HANDLE_VALUE {
            unsafe {
                DisconnectNamedPipe(self.handle);
                CloseHandle(self.handle);
            }
        }
    }
}
