//! Android AAudio input and output streams for [`crate::audio_io`].

use std::{
    ffi::c_void,
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};

use anyhow::{Result, anyhow, bail};
use ndk::audio::{
    AudioContentType, AudioDirection, AudioFormat, AudioInputPreset, AudioPerformanceMode,
    AudioSharingMode, AudioStream, AudioStreamBuilder, AudioStreamState, AudioUsage,
};

use super::Format;

/// Timeout for single blocking AAudio read/write operations before bubbling an
/// error up to `lasting` so disconnected headsets can reopen cleanly.
const IO_TIMEOUT_NS: i64 = 2_000_000_000;
const FLUSH_TIMEOUT_NS: i64 = 50_000_000;

/// Raises the calling thread's scheduling priority for audio work.
pub fn raise_priority(priority: super::Priority) -> Result<&'static str, String> {
    let nice = match priority {
        super::Priority::Capture => -19,
        super::Priority::Playback => -16,
    };
    // SAFETY: valid PRIO_PROCESS and PID 0 (calling thread).
    if unsafe { libc::setpriority(libc::PRIO_PROCESS, 0, nice) } == 0 {
        Ok("nice audio priority")
    } else {
        Ok("AAudio low-latency")
    }
}

/// CPU performance boost hint; AAudio handles power scheduling per stream.
pub fn full_speed(_: bool) {}

fn aaudio_format(format: Format) -> AudioFormat {
    if format.float {
        AudioFormat::PCM_Float
    } else {
        AudioFormat::PCM_I16
    }
}

fn validate_stream(stream: &AudioStream, expected: Format) -> Result<()> {
    let actual_rate = stream.sample_rate();
    let actual_channels = stream.channel_count();
    let actual_format = stream.format();
    if actual_rate != expected.rate as i32
        || actual_channels != i32::from(expected.channels)
        || actual_format != aaudio_format(expected)
    {
        bail!(
            "AAudio opened {} Hz × {} ({actual_format:?}), expected {} Hz × {}",
            actual_rate,
            actual_channels,
            expected.rate,
            expected.channels
        );
    }
    Ok(())
}

/// An opened Android AAudio speaker stream.
pub struct Output {
    stream: AudioStream,
    format: Format,
}

// SAFETY: only one thread reads/writes an `Output` at a time (`audio_io::Output`).
unsafe impl Send for Output {}

impl Output {
    /// Opens an Android AAudio output stream matching `format`.
    pub fn open(
        _speaker: Option<&str>,
        name: &'static str,
        format: Format,
        queue: Option<Duration>,
    ) -> Result<Self> {
        let voice =
            queue.is_some() || matches!(name, "call" | "voice" | "Discord" | "stream audio");
        let stream = AudioStreamBuilder::new()
            .map_err(|error| anyhow!("could not create AAudio output builder: {error}"))?
            .direction(AudioDirection::Output)
            .sharing_mode(AudioSharingMode::Shared)
            .performance_mode(AudioPerformanceMode::LowLatency)
            .sample_rate(format.rate as i32)
            .channel_count(i32::from(format.channels))
            .format(aaudio_format(format))
            .usage(if voice {
                AudioUsage::VoiceCommunication
            } else {
                AudioUsage::Media
            })
            .content_type(if voice {
                AudioContentType::Speech
            } else {
                AudioContentType::Music
            })
            .open_stream()
            .map_err(|error| anyhow!("could not open AAudio output stream ({name}): {error}"))?;

        validate_stream(&stream, format)?;

        if let Some(target_queue) = queue {
            let burst = stream.frames_per_burst().max(1);
            let target_frames = ((format.bytes(target_queue) / format.frame_bytes().max(1))
                .min(i32::MAX as usize) as i32)
                .max(burst * 2)
                .min(stream.buffer_capacity_in_frames().max(burst * 2));
            let _ = stream.set_buffer_size_in_frames(target_frames);
        }

        stream
            .request_start()
            .map_err(|error| anyhow!("could not start AAudio output stream: {error}"))?;

        Ok(Self { stream, format })
    }

    /// Writes `bytes` synchronously, blocking until accepted by AAudio.
    pub fn write(&mut self, bytes: &[u8]) -> Result<()> {
        let frame_bytes = self.format.frame_bytes();
        if frame_bytes == 0 {
            bail!("invalid audio frame size");
        }
        let total_frames = bytes.len() / frame_bytes;
        let mut written = 0usize;
        while written < total_frames {
            let remaining = (total_frames - written).min(i32::MAX as usize) as i32;
            // SAFETY: `written < total_frames` so `written * frame_bytes < bytes.len()`,
            // and `remaining * frame_bytes` fits inside `&bytes[written * frame_bytes..]`.
            let ptr = unsafe { bytes.as_ptr().add(written * frame_bytes) } as *const c_void;
            let count = unsafe { self.stream.write(ptr, remaining, IO_TIMEOUT_NS) }
                .map_err(|error| anyhow!("AAudio output write failed: {error}"))?;
            if count == 0 {
                bail!("AAudio output stream stalled");
            }
            written += count as usize;
        }
        Ok(())
    }

    /// Waits for queued audio to finish playing out.
    pub fn drain(&mut self) -> Result<()> {
        let wait = self.latency();
        if !wait.is_zero() {
            std::thread::sleep(wait);
        }
        Ok(())
    }

    /// Discards queued playback frames cleanly when interrupting playback.
    pub fn flush(&mut self) {
        let _ = self.stream.request_pause();
        let _ = self
            .stream
            .wait_for_state_change(AudioStreamState::Pausing, FLUSH_TIMEOUT_NS);
        let _ = self.stream.request_flush();
        let _ = self
            .stream
            .wait_for_state_change(AudioStreamState::Flushing, FLUSH_TIMEOUT_NS);
        let _ = self.stream.request_start();
    }

    /// Estimated hardware buffer latency based on AAudio buffer size.
    pub fn latency(&self) -> Duration {
        let frames = u64::try_from(self.stream.buffer_size_in_frames()).unwrap_or(0);
        let rate = u64::from(self.format.rate.max(1));
        Duration::from_nanos(frames.saturating_mul(1_000_000_000) / rate)
    }
}

impl Drop for Output {
    fn drop(&mut self) {
        let _ = self.stream.request_stop();
    }
}

/// An opened Android AAudio microphone stream.
pub struct Input {
    stream: AudioStream,
    format: Format,
}

// SAFETY: only one thread reads from an `Input` at a time (`audio_io::Input`).
unsafe impl Send for Input {}

impl Input {
    /// Opens an Android AAudio input stream matching `format`.
    pub fn open(
        _microphone: Option<&str>,
        name: &'static str,
        format: Format,
        fragment: Duration,
    ) -> Result<Self> {
        let stream = AudioStreamBuilder::new()
            .map_err(|error| anyhow!("could not create AAudio input builder: {error}"))?
            .direction(AudioDirection::Input)
            .sharing_mode(AudioSharingMode::Shared)
            .performance_mode(AudioPerformanceMode::LowLatency)
            .input_preset(AudioInputPreset::VoiceCommunication)
            .sample_rate(format.rate as i32)
            .channel_count(i32::from(format.channels))
            .format(aaudio_format(format))
            .open_stream()
            .map_err(|error| anyhow!("could not open AAudio input stream ({name}): {error}"))?;

        validate_stream(&stream, format)?;

        let burst = stream.frames_per_burst().max(1);
        let target_frames = ((fragment.as_secs_f64() * f64::from(format.rate)).round() as i32)
            .max(burst * 2)
            .min(stream.buffer_capacity_in_frames().max(burst * 2));
        let _ = stream.set_buffer_size_in_frames(target_frames);

        stream
            .request_start()
            .map_err(|error| anyhow!("could not start AAudio input stream: {error}"))?;

        Ok(Self { stream, format })
    }

    /// Fills `bytes` from the microphone, blocking until complete.
    pub fn read(&mut self, bytes: &mut [u8]) -> Result<()> {
        let frame_bytes = self.format.frame_bytes();
        if frame_bytes == 0 {
            bail!("invalid audio frame size");
        }
        let total_frames = bytes.len() / frame_bytes;
        let mut read = 0usize;
        while read < total_frames {
            let remaining = (total_frames - read).min(i32::MAX as usize) as i32;
            // SAFETY: `read < total_frames` so `read * frame_bytes < bytes.len()`,
            // and `remaining * frame_bytes` fits inside `&mut bytes[read * frame_bytes..]`.
            let ptr = unsafe { bytes.as_mut_ptr().add(read * frame_bytes) } as *mut c_void;
            let count = unsafe { self.stream.read(ptr, remaining, IO_TIMEOUT_NS) }
                .map_err(|error| anyhow!("AAudio input read failed: {error}"))?;
            if count == 0 {
                bail!("AAudio input stream timed out");
            }
            read += count as usize;
        }
        Ok(())
    }
}

impl Drop for Input {
    fn drop(&mut self) {
        let _ = self.stream.request_stop();
    }
}

/// Device endpoint identification string; Android relies on the OS routing.
pub fn endpoint(_: Option<&str>, _: bool) -> String {
    String::new()
}

/// System-wide loopback recording is unsupported for normal Android apps.
#[cfg(feature = "discord")]
pub fn others(
    _: fn(&str) -> bool,
    _: super::Hear,
    _: Arc<AtomicBool>,
) -> Result<std::thread::JoinHandle<()>> {
    Err(anyhow!(
        "loopback audio capture is not supported on Android"
    ))
}
