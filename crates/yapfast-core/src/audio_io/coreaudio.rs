//! Sound through CoreAudio, macOS's own: an output unit for the speakers and
//! one for the microphone, each taking and giving frames on the device's clock
//! through a ring in `native/coreaudio.c`.
//!
//! A device is named as [`crate::voice`] lists it, which on macOS is the
//! device's CoreAudio name, since WebRTC gives its macOS devices no identifier
//! of their own. `None` is the system's default. CoreAudio names a stream by
//! the process rather than per stream, so the `name` Yapfast passes each one
//! goes to the log instead of to the sound card.

use std::ffi::CString;
use std::os::raw::{c_char, c_int, c_void};
use std::ptr;
use std::time::Duration;

use anyhow::{Result, anyhow};

use super::Format;

/// Why a call into the unit failed, from the reason it wrote.
fn error(what: &str, message: &[u8; REASON]) -> anyhow::Error {
    let end = message
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(message.len());
    anyhow!("{what}: {}", String::from_utf8_lossy(&message[..end]))
}

/// The longest reason the C side writes, plus the byte it can't.
const REASON: usize = 256;

/// How many frames `duration` is, which is what a ring is measured in.
fn frames(format: Format, duration: Duration) -> u32 {
    (format.bytes(duration) / format.frame_bytes())
        .try_into()
        .unwrap_or(u32::MAX)
}

fn reason() -> [u8; REASON] {
    [0; REASON]
}

/// The microphone or speaker a stream for `id` opens. CoreAudio's names don't
/// change when a device comes back, so the identifier is the answer; the
/// default is left empty, since a stream with none follows the system anyway.
#[cfg(feature = "discord")]
pub fn endpoint(id: Option<&str>, _: bool) -> String {
    let Some(id) = id else { return String::new() };
    id.strip_prefix(crate::voice::NAMED).unwrap_or(id).to_owned()
}

fn cstr(text: Option<&str>) -> Result<Option<CString>> {
    text.map(CString::new)
        .transpose()
        .map_err(|_| anyhow!("a device's name has a zero in it"))
}

unsafe extern "C" {
    fn yap_output_open(
        device: *const c_char,
        rate: u32,
        channels: u32,
        float: c_int,
        queue_us: u32,
        err: *mut c_char,
        err_length: usize,
    ) -> *mut c_void;
    fn yap_output_write(
        output: *mut c_void,
        data: *const u8,
        bytes: usize,
        err: *mut c_char,
        err_length: usize,
    ) -> c_int;
    fn yap_output_drain(output: *mut c_void, err: *mut c_char, err_length: usize) -> c_int;
    fn yap_output_flush(output: *mut c_void);
    fn yap_output_latency_us(output: *mut c_void) -> u64;
    fn yap_output_free(output: *mut c_void);

    fn yap_input_open(
        device: *const c_char,
        rate: u32,
        channels: u32,
        float: c_int,
        fragment_us: u32,
        err: *mut c_char,
        err_length: usize,
    ) -> *mut c_void;
    fn yap_input_read(
        input: *mut c_void,
        data: *mut u8,
        bytes: usize,
        err: *mut c_char,
        err_length: usize,
    ) -> c_int;
    fn yap_input_free(input: *mut c_void);
}

/// Speakers Yapfast plays to. The unit runs from the moment it opens, taking
/// what's queued and staying quiet when there's nothing, so a sound starts on
/// the sample rather than after a start-up.
pub struct Output {
    handle: *mut c_void,
}

// The unit's own thread takes and gives frames; Yapfast reaches the stream from
// one thread of its own at a time, which is what the ring is for.
unsafe impl Send for Output {}

impl Output {
    pub fn open(
        speaker: Option<&str>,
        name: &'static str,
        format: Format,
        queue: Option<Duration>,
    ) -> Result<Self> {
        let device = cstr(speaker)?;
        let mut message = reason();
        // CoreAudio has no per-stream name; say which stream a failure is about.
        let handle = unsafe {
            yap_output_open(
                device.as_ref().map_or(ptr::null(), |name| name.as_ptr()),
                format.rate,
                u32::from(format.channels),
                c_int::from(format.float),
                // Zero lets the unit hold a quarter second.
                queue.map_or(0, |queue| frames(format, queue)),
                message.as_mut_ptr().cast(),
                REASON,
            )
        };
        if handle.is_null() {
            return Err(error(name, &message));
        }
        Ok(Self { handle })
    }

    pub fn write(&mut self, bytes: &[u8]) -> Result<()> {
        let mut message = reason();
        let result = unsafe {
            yap_output_write(
                self.handle,
                bytes.as_ptr(),
                bytes.len(),
                message.as_mut_ptr().cast(),
                REASON,
            )
        };
        if result != 0 {
            return Err(error("could not queue sound", &message));
        }
        Ok(())
    }

    pub fn drain(&mut self) -> Result<()> {
        let mut message = reason();
        let result = unsafe { yap_output_drain(self.handle, message.as_mut_ptr().cast(), REASON) };
        if result != 0 {
            return Err(error("could not wait for the sound to finish", &message));
        }
        Ok(())
    }

    pub fn flush(&mut self) {
        unsafe { yap_output_flush(self.handle) };
    }

    pub fn latency(&self) -> Duration {
        let microseconds = unsafe { yap_output_latency_us(self.handle) };
        Duration::from_micros(microseconds)
    }
}

impl Drop for Output {
    fn drop(&mut self) {
        unsafe { yap_output_free(self.handle) };
    }
}

/// A microphone Yapfast records.
pub struct Input {
    handle: *mut c_void,
}

// As for [`Output`].
unsafe impl Send for Input {}

impl Input {
    pub fn open(
        microphone: Option<&str>,
        name: &'static str,
        format: Format,
        fragment: Duration,
    ) -> Result<Self> {
        let device = cstr(microphone)?;
        let mut message = reason();
        let handle = unsafe {
            yap_input_open(
                device.as_ref().map_or(ptr::null(), |name| name.as_ptr()),
                format.rate,
                u32::from(format.channels),
                c_int::from(format.float),
                frames(format, fragment),
                message.as_mut_ptr().cast(),
                REASON,
            )
        };
        if handle.is_null() {
            return Err(error(name, &message));
        }
        Ok(Self { handle })
    }

    pub fn read(&mut self, bytes: &mut [u8]) -> Result<()> {
        let mut message = reason();
        let result = unsafe {
            yap_input_read(
                self.handle,
                bytes.as_mut_ptr(),
                bytes.len(),
                message.as_mut_ptr().cast(),
                REASON,
            )
        };
        if result != 0 {
            return Err(error("could not hear the microphone", &message));
        }
        Ok(())
    }
}

impl Drop for Input {
    fn drop(&mut self) {
        unsafe { yap_input_free(self.handle) };
    }
}

/// Hearing what the other applications play needs a stream of the desktop's
/// sound, which macOS only hands out with screen recording, and then only for
/// a chosen window or display rather than for the machine. Yapfast shares a
/// Go Live stream's sound on Linux and Windows for now.
#[cfg(feature = "discord")]
pub fn others(
    _: fn(&str) -> bool,
    _: super::Hear,
    _: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> Result<std::thread::JoinHandle<()>> {
    Err(anyhow!(
        "Yapfast shares other applications' sound on Linux and Windows for now."
    ))
}
