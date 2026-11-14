//! macOS sound backend using CoreAudio / AudioToolbox (`native/macos_audio.c`).

use std::{
    ffi::{CStr, CString, c_char, c_int, c_void},
    ptr,
    time::Duration,
};

use anyhow::{Result, bail};

use super::Format;

unsafe extern "C" {
    fn yapfast_macos_raise_priority(is_playback: c_int) -> c_int;
    fn yapfast_macos_default_device_uid(input: c_int, buf: *mut c_char, cap: c_int) -> c_int;
    fn yapfast_macos_output_open(
        device_uid: *const c_char,
        rate: u32,
        channels: u16,
        is_float: c_int,
        buffer_bytes: u32,
        out_handle: *mut *mut c_void,
    ) -> c_int;
    fn yapfast_macos_output_write(handle: *mut c_void, data: *const u8, len: u32) -> c_int;
    fn yapfast_macos_output_drain(handle: *mut c_void) -> c_int;
    fn yapfast_macos_output_flush(handle: *mut c_void);
    fn yapfast_macos_output_latency_us(handle: *mut c_void) -> u64;
    fn yapfast_macos_output_close(handle: *mut c_void);

    fn yapfast_macos_input_open(
        device_uid: *const c_char,
        rate: u32,
        channels: u16,
        is_float: c_int,
        fragment_bytes: u32,
        out_handle: *mut *mut c_void,
    ) -> c_int;
    fn yapfast_macos_input_read(handle: *mut c_void, dst: *mut u8, len: u32) -> c_int;
    fn yapfast_macos_input_close(handle: *mut c_void);
}

pub fn raise_priority(priority: super::Priority) -> Result<&'static str, String> {
    let is_playback = c_int::from(matches!(priority, super::Priority::Playback));
    // SAFETY: sets QoS class on the calling thread only.
    let status = unsafe { yapfast_macos_raise_priority(is_playback) };
    if status == 0 {
        Ok("QOS_CLASS_USER_INTERACTIVE")
    } else {
        Err(format!("pthread_set_qos_class_self_np failed ({status})"))
    }
}

#[cfg(feature = "discord")]
pub fn full_speed(_: bool) {}

fn device_cstr(device: Option<&str>) -> Option<CString> {
    device
        .filter(|id| !id.is_empty() && *id != "default")
        .and_then(|id| CString::new(id).ok())
}

pub struct Output {
    handle: *mut c_void,
}

// SAFETY: macos_audio.c synchronizes all Output operations with a pthread mutex.
unsafe impl Send for Output {}

impl Output {
    pub fn open(
        speaker: Option<&str>,
        _name: &'static str,
        format: Format,
        queue: Option<Duration>,
    ) -> Result<Self> {
        let uid = device_cstr(speaker);
        let uid_ptr = uid.as_ref().map_or(ptr::null(), |s| s.as_ptr());
        let buffer_bytes = queue.map_or(0, |d| format.bytes(d).min(u32::MAX as usize) as u32);
        let mut handle = ptr::null_mut();
        // SAFETY: valid pointers and output handle slot.
        let status = unsafe {
            yapfast_macos_output_open(
                uid_ptr,
                format.rate,
                format.channels,
                c_int::from(format.float),
                buffer_bytes,
                &mut handle,
            )
        };
        if status != 0 || handle.is_null() {
            bail!("CoreAudio output failed to open (OSStatus {status})");
        }
        Ok(Self { handle })
    }

    pub fn write(&mut self, samples: &[u8]) -> Result<()> {
        let len = u32::try_from(samples.len()).unwrap_or(u32::MAX);
        // SAFETY: handle is open until Drop; samples is valid for len bytes.
        let status = unsafe { yapfast_macos_output_write(self.handle, samples.as_ptr(), len) };
        if status != 0 {
            bail!("CoreAudio output write failed ({status})");
        }
        Ok(())
    }

    pub fn drain(&mut self) -> Result<()> {
        // SAFETY: handle is open until Drop.
        let status = unsafe { yapfast_macos_output_drain(self.handle) };
        if status != 0 {
            bail!("CoreAudio output drain failed ({status})");
        }
        Ok(())
    }

    pub fn flush(&mut self) {
        // SAFETY: handle is open until Drop.
        unsafe { yapfast_macos_output_flush(self.handle) };
    }

    pub fn latency(&self) -> Duration {
        // SAFETY: handle is open until Drop.
        Duration::from_micros(unsafe { yapfast_macos_output_latency_us(self.handle) })
    }
}

impl Drop for Output {
    fn drop(&mut self) {
        // SAFETY: handle is owned exclusively by this Output.
        unsafe { yapfast_macos_output_close(self.handle) };
    }
}

pub struct Input {
    handle: *mut c_void,
}

// SAFETY: macos_audio.c synchronizes all Input operations with a pthread mutex.
unsafe impl Send for Input {}

impl Input {
    pub fn open(
        microphone: Option<&str>,
        _name: &'static str,
        format: Format,
        fragment: Duration,
    ) -> Result<Self> {
        let uid = device_cstr(microphone);
        let uid_ptr = uid.as_ref().map_or(ptr::null(), |s| s.as_ptr());
        let fragment_bytes = format.bytes(fragment).min(u32::MAX as usize) as u32;
        let mut handle = ptr::null_mut();
        // SAFETY: valid pointers and output handle slot.
        let status = unsafe {
            yapfast_macos_input_open(
                uid_ptr,
                format.rate,
                format.channels,
                c_int::from(format.float),
                fragment_bytes,
                &mut handle,
            )
        };
        if status != 0 || handle.is_null() {
            bail!("CoreAudio input failed to open (OSStatus {status})");
        }
        Ok(Self { handle })
    }

    pub fn read(&mut self, samples: &mut [u8]) -> Result<()> {
        let len = u32::try_from(samples.len()).unwrap_or(u32::MAX);
        // SAFETY: handle is open until Drop; samples is valid for len bytes.
        let status = unsafe { yapfast_macos_input_read(self.handle, samples.as_mut_ptr(), len) };
        if status != 0 {
            bail!("CoreAudio input read failed ({status})");
        }
        Ok(())
    }
}

impl Drop for Input {
    fn drop(&mut self) {
        // SAFETY: handle is owned exclusively by this Input.
        unsafe { yapfast_macos_input_close(self.handle) };
    }
}

#[cfg(feature = "discord")]
pub fn endpoint(device: Option<&str>, input: bool) -> String {
    if let Some(id) = device.filter(|id| !id.is_empty() && *id != "default") {
        return id.to_owned();
    }
    let mut buf = [0 as c_char; 256];
    // SAFETY: buf is valid for 256 bytes and NUL-terminated by macos_audio.c.
    let ok = unsafe { yapfast_macos_default_device_uid(c_int::from(input), buf.as_mut_ptr(), 256) };
    if ok == 0 {
        unsafe { CStr::from_ptr(buf.as_ptr()) }
            .to_string_lossy()
            .into_owned()
    } else {
        String::new()
    }
}

#[cfg(feature = "discord")]
pub fn others(
    _: fn(&str) -> bool,
    _: super::Hear,
    _: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> Result<std::thread::JoinHandle<()>> {
    bail!("Yapfast does not capture application audio on macOS yet")
}
