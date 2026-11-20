//! Sound through Core Audio, on macOS.
//!
//! Each stream is a HAL output unit (AUHAL) tied to one device. Its callback
//! runs on a real-time thread the system owns and is joined to the
//! blocking API by a lock-free queue ([`ring`]), so neither side waits on
//! a lock. The unit converts between the device's format and the caller's.
//!
//! Streams stay on the device they opened; [`super::lasting`] follows the
//! default device, with [`endpoint`] telling which one that is now.

mod devices;
mod input;
mod output;
mod resample;
mod ring;
mod unit;
mod watch;

use std::sync::Mutex;
#[cfg(feature = "discord")]
use std::sync::{Arc, atomic::AtomicBool};

use anyhow::Result;
use objc2::{rc::Retained, runtime::ProtocolObject};
use objc2_foundation::{NSActivityOptions, NSObjectProtocol, NSProcessInfo, NSString};

pub use input::Input;
pub use output::Output;

/// Why the system is kept busy during a call, as Activity Monitor shows it.
const ACTIVITY_REASON: &str = "Yapfast voice call";

/// Says how it raised the calling thread, or why it couldn't. Core Audio
/// runs its own callbacks on real-time threads; this is for the threads that
/// feed and read them, and macOS has no priority numbers for those, only
/// quality of service. Threads keep it for good.
#[cfg_attr(
    not(feature = "discord"),
    allow(dead_code, reason = "only Discord uses it")
)]
pub fn raise_priority(priority: super::Priority) -> Result<&'static str, String> {
    let (class, how) = match priority {
        super::Priority::Playback => (
            libc::qos_class_t::QOS_CLASS_USER_INTERACTIVE,
            "QOS_CLASS_USER_INTERACTIVE",
        ),
        super::Priority::Capture => (
            libc::qos_class_t::QOS_CLASS_USER_INITIATED,
            "QOS_CLASS_USER_INITIATED",
        ),
    };
    // SAFETY: this only sets the calling thread's own quality of service.
    let status = unsafe { libc::pthread_set_qos_class_self_np(class, 0) };
    if status == 0 {
        Ok(how)
    } else {
        Err(std::io::Error::from_raw_os_error(status).to_string())
    }
}

/// An activity macOS was told about, which keeps App Nap from slowing the
/// process while it's held.
struct Activity(Retained<ProtocolObject<dyn NSObjectProtocol>>);

// SAFETY: the token is only handed back to NSProcessInfo, which may be
// called from any thread.
unsafe impl Send for Activity {}

static ACTIVITY: Mutex<Option<Activity>> = Mutex::new(None);

/// Keeps macOS from napping the process (App Nap) while `on`, as a call
/// needs with its window hidden: a napped process wakes its timers late,
/// and voice frames with them.
#[cfg(feature = "discord")]
pub fn full_speed(on: bool) {
    let mut held = ACTIVITY
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if on {
        let options = NSActivityOptions::UserInitiatedAllowingIdleSystemSleep
            | NSActivityOptions::LatencyCritical;
        let reason = NSString::from_str(ACTIVITY_REASON);
        let token = NSProcessInfo::processInfo().beginActivityWithOptions_reason(options, &reason);
        *held = Some(Activity(token));
    } else if let Some(Activity(token)) = held.take() {
        // SAFETY: the token came from `beginActivityWithOptions:reason:`
        // and is ended once.
        unsafe { NSProcessInfo::processInfo().endActivity(&token) };
    }
}

/// The UID of the device a stream for `id` would open now: its own while
/// it's connected, otherwise the default's. Empty if there's none.
#[cfg(feature = "discord")]
pub fn endpoint(id: Option<&str>, output: bool) -> String {
    devices::resolve(id, output)
        .or_else(|_| devices::resolve(None, output))
        .map(|device| device.uid)
        .unwrap_or_default()
}

/// Hearing other applications takes a screen recording permission and
/// ScreenCaptureKit, which come with screen sharing.
#[cfg(feature = "discord")]
pub fn others(
    _: fn(&str) -> bool,
    _: super::Hear,
    _: Arc<AtomicBool>,
) -> Result<std::thread::JoinHandle<()>> {
    anyhow::bail!("Yapfast shares the sound of other applications on Linux only for now")
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{
        super::{Format, RATE},
        Input, Output, devices,
    };

    const TONE_HZ: f32 = 440.0;
    /// Quiet enough to be heard without startling anyone.
    const TONE_LEVEL: f32 = 0.05;
    const TONE_LENGTH: Duration = Duration::from_millis(500);
    const LISTEN_FOR: Duration = Duration::from_secs(1);
    const BLOCK: Duration = Duration::from_millis(10);

    /// Interleaved 16-bit samples of a sine wave, the same on every channel.
    fn tone(format: Format, length: Duration) -> Vec<i16> {
        let frames = format.bytes(length) / format.frame_bytes();
        (0..frames)
            .flat_map(|frame| {
                let phase = frame as f32 / format.rate as f32 * TONE_HZ * std::f32::consts::TAU;
                let sample = (phase.sin() * TONE_LEVEL * f32::from(i16::MAX)) as i16;
                std::iter::repeat_n(sample, usize::from(format.channels))
            })
            .collect()
    }

    fn samples_as_bytes(samples: &[i16]) -> &[u8] {
        // SAFETY: i16 has no padding and any alignment suits u8.
        unsafe { std::slice::from_raw_parts(samples.as_ptr().cast(), size_of_val(samples)) }
    }

    fn samples_as_bytes_mut(samples: &mut [i16]) -> &mut [u8] {
        // SAFETY: as above, and every bit pattern is a valid i16.
        unsafe { std::slice::from_raw_parts_mut(samples.as_mut_ptr().cast(), size_of_val(samples)) }
    }

    #[test]
    fn a_tone_has_its_length_and_level() {
        let format = Format {
            rate: RATE,
            channels: 2,
            float: false,
        };
        let samples = tone(format, TONE_LENGTH);
        assert_eq!(samples.len(), 24_000 * 2);
        let peak = samples
            .iter()
            .map(|sample| sample.unsigned_abs())
            .max()
            .unwrap();
        assert!(peak > 1_500 && peak < 1_700, "{peak}");
        assert_eq!(samples[0], 0);
        assert_eq!(samples[2], samples[3]);
    }

    /// Reads `LISTEN_FOR` of the default microphone at `rate` and says how
    /// loud it was, as the highest sample.
    fn listen(rate: u32) -> u16 {
        let format = Format {
            rate,
            channels: 1,
            float: false,
        };
        let mut input =
            Input::open(None, "test microphone", format, BLOCK).expect("open the microphone");
        let mut block = vec![0i16; format.bytes(BLOCK) / format.frame_bytes()];
        let mut peak = 0;
        for _ in 0..(LISTEN_FOR.as_millis() / BLOCK.as_millis()) {
            input
                .read(samples_as_bytes_mut(&mut block))
                .expect("read the microphone");
            peak = peak.max(
                block
                    .iter()
                    .map(|sample| sample.unsigned_abs())
                    .max()
                    .unwrap_or(0),
            );
        }
        peak
    }

    /// Plays on the machine's speakers: run by hand with `--ignored`.
    #[test]
    #[ignore = "plays a quiet tone on the default speakers"]
    fn the_default_speakers_play_a_tone() {
        let format = Format {
            rate: RATE,
            channels: 2,
            float: false,
        };
        let mut output = Output::open(None, "test tone", format, None).expect("open the speakers");
        output
            .write(samples_as_bytes(&tone(format, TONE_LENGTH)))
            .expect("write the tone");
        println!("latency while playing: {:?}", output.latency());
        output.drain().expect("drain the speakers");
    }

    #[test]
    #[ignore = "plays quiet tones at other rates and channel counts on the default speakers"]
    fn the_speakers_convert_other_rates_and_channels() {
        for (rate, channels) in [(16_000, 1), (44_100, 2), (48_000, 1)] {
            let format = Format {
                rate,
                channels,
                float: false,
            };
            let mut output =
                Output::open(None, "test tone", format, None).expect("open the speakers");
            output
                .write(samples_as_bytes(&tone(format, TONE_LENGTH)))
                .expect("write the tone");
            output.drain().expect("drain the speakers");
            println!("played {rate} Hz, {channels} channels");
        }
    }

    /// Records from the microphone: macOS may ask for permission first, and
    /// refuses a program that isn't an app bundle.
    #[test]
    #[ignore = "records one second from the default microphone"]
    fn the_default_microphone_delivers_samples() {
        println!("microphone peak at 48 kHz: {}", listen(RATE));
    }

    #[test]
    #[ignore = "records at other rates, to see what the unit converts"]
    fn the_microphone_converts_other_rates() {
        for rate in [16_000, 44_100] {
            println!("microphone peak at {rate} Hz: {}", listen(rate));
        }
    }

    #[test]
    #[ignore = "lists the devices this machine has"]
    fn the_hal_lists_this_machines_devices() {
        for device in devices::list().expect("list devices") {
            println!("{device:?}");
        }
        println!("default speakers: {:?}", devices::default_device(true));
        println!("default microphone: {:?}", devices::default_device(false));
    }
}
