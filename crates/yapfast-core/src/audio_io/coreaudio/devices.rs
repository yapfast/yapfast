//! The devices Core Audio's hardware layer (the HAL) lists, and what to ask
//! it about them. A device is named as LiveKit lists it: by its UID, or by
//! `name:` and its name when LiveKit gives it no id (see [`NAMED`]).

use std::{ptr, ptr::NonNull, time::Duration};

use anyhow::{Context as _, Result, anyhow, bail};
use objc2_core_audio::{
    AudioObjectGetPropertyData, AudioObjectGetPropertyDataSize, AudioObjectID,
    AudioObjectPropertyAddress, AudioObjectPropertyScope, AudioObjectPropertySelector,
    kAudioDevicePropertyBufferFrameSize, kAudioDevicePropertyDeviceUID,
    kAudioDevicePropertyIsHidden, kAudioDevicePropertyLatency,
    kAudioDevicePropertyNominalSampleRate, kAudioDevicePropertySafetyOffset,
    kAudioDevicePropertyStreams, kAudioHardwarePropertyDefaultInputDevice,
    kAudioHardwarePropertyDefaultOutputDevice, kAudioHardwarePropertyDevices,
    kAudioObjectPropertyElementMain, kAudioObjectPropertyName, kAudioObjectPropertyScopeGlobal,
    kAudioObjectPropertyScopeInput, kAudioObjectPropertyScopeOutput, kAudioObjectSystemObject,
    kAudioStreamPropertyLatency,
};
use objc2_core_foundation::{CFRetained, CFString};

use crate::voice::NAMED;

/// What Core Audio calls answer with: 0 for success.
pub type OsStatus = i32;

/// The HAL's number for a device; it changes when the device reconnects.
pub type DeviceId = AudioObjectID;

/// The HAL's id for "no device", which a default lookup answers when the
/// machine has none of that kind.
const NO_DEVICE: DeviceId = 0;

/// Whatever the HAL answered, as the four characters Apple writes its
/// status codes in when they're printable, otherwise as a number.
pub fn describe(status: OsStatus) -> String {
    let bytes = status.to_be_bytes();
    if bytes
        .iter()
        .all(|byte| byte.is_ascii_graphic() || *byte == b' ')
    {
        format!("'{}' ({status})", String::from_utf8_lossy(&bytes))
    } else {
        status.to_string()
    }
}

/// Turns a Core Audio status into a result that names the failed `call`.
pub fn check(call: &str, status: OsStatus) -> Result<()> {
    if status == 0 {
        Ok(())
    } else {
        bail!("{call} failed with status {}", describe(status))
    }
}

/// A device as the HAL lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Device {
    pub id: DeviceId,
    /// Stays the same when the device reconnects.
    pub uid: String,
    pub name: String,
    /// How many input streams the device has; none for speakers.
    pub input_streams: usize,
    pub output_streams: usize,
    /// Devices the system keeps from users, such as an aggregate an
    /// application made for itself.
    pub hidden: bool,
}

impl Device {
    /// Whether the device can be opened as speakers (`output`) or as a
    /// microphone.
    pub fn serves(&self, output: bool) -> bool {
        !self.hidden
            && if output {
                self.output_streams > 0
            } else {
                self.input_streams > 0
            }
    }
}

/// The device that `wanted` names among those that serve `output`: by UID,
/// or by name when `wanted` starts with [`NAMED`].
pub fn pick<'a>(devices: &'a [Device], wanted: &str, output: bool) -> Option<&'a Device> {
    let named = wanted.strip_prefix(NAMED);
    devices
        .iter()
        .filter(|device| device.serves(output))
        .find(|device| match named {
            Some(name) => device.name == name,
            None => device.uid == wanted,
        })
}

/// The device to open: `wanted`'s, or the system's default for none.
pub fn resolve(wanted: Option<&str>, output: bool) -> Result<Device> {
    match wanted {
        None => default_device(output),
        Some(wanted) => {
            let devices = list()?;
            pick(&devices, wanted, output)
                .cloned()
                .ok_or_else(|| anyhow!("no {} named {wanted}", direction(output)))
        }
    }
}

fn direction(output: bool) -> &'static str {
    if output { "speakers" } else { "microphone" }
}

/// Every device the HAL lists, with the ones it can't describe left out.
pub fn list() -> Result<Vec<Device>> {
    let ids: Vec<u32> = array(
        kAudioObjectSystemObject as DeviceId,
        kAudioHardwarePropertyDevices,
        kAudioObjectPropertyScopeGlobal,
    )
    .context("could not list the audio devices")?;
    Ok(ids
        .into_iter()
        .filter_map(|id| {
            read(id)
                .inspect_err(|error| {
                    tracing::debug!(id, error = format!("{error:#}"), "audio device skipped");
                })
                .ok()
        })
        .collect())
}

/// The system's default speakers or microphone.
pub fn default_device(output: bool) -> Result<Device> {
    let selector = if output {
        kAudioHardwarePropertyDefaultOutputDevice
    } else {
        kAudioHardwarePropertyDefaultInputDevice
    };
    let id: DeviceId = property(
        kAudioObjectSystemObject as DeviceId,
        selector,
        kAudioObjectPropertyScopeGlobal,
    )
    .with_context(|| format!("could not ask for the default {}", direction(output)))?;
    if id == NO_DEVICE {
        bail!("this machine has no {}", direction(output));
    }
    read(id)
}

fn read(id: DeviceId) -> Result<Device> {
    Ok(Device {
        id,
        uid: string(id, kAudioDevicePropertyDeviceUID)?,
        name: string(id, kAudioObjectPropertyName)?,
        input_streams: streams(id, kAudioObjectPropertyScopeInput).len(),
        output_streams: streams(id, kAudioObjectPropertyScopeOutput).len(),
        hidden: property::<u32>(
            id,
            kAudioDevicePropertyIsHidden,
            kAudioObjectPropertyScopeGlobal,
        )
        .is_ok_and(|hidden| hidden != 0),
    })
}

fn streams(id: DeviceId, scope: AudioObjectPropertyScope) -> Vec<u32> {
    array(id, kAudioDevicePropertyStreams, scope).unwrap_or_default()
}

/// How long audio takes between the system and the device's hardware, or
/// the other way: the device's own latency, its safety offset, one buffer,
/// and the first stream's latency. A reading the device won't give counts
/// as nothing.
pub fn latency(id: DeviceId, output: bool) -> Duration {
    let scope = if output {
        kAudioObjectPropertyScopeOutput
    } else {
        kAudioObjectPropertyScopeInput
    };
    let frame_count = |selector| frames(id, selector, scope);
    let stream = streams(id, scope).first().map_or(0, |stream| {
        frames(*stream, kAudioStreamPropertyLatency, scope)
    });
    let total = frame_count(kAudioDevicePropertyLatency)
        + frame_count(kAudioDevicePropertySafetyOffset)
        + frame_count(kAudioDevicePropertyBufferFrameSize)
        + stream;
    frames_to_duration(total, nominal_rate(id))
}

/// The rate the device runs at now, or 0 when it won't say.
pub fn nominal_rate(id: DeviceId) -> f64 {
    property::<f64>(
        id,
        kAudioDevicePropertyNominalSampleRate,
        kAudioObjectPropertyScopeGlobal,
    )
    .inspect_err(|error| {
        tracing::debug!(id, error = format!("{error:#}"), "audio rate not known");
    })
    .unwrap_or(0.0)
}

fn frames(
    object: AudioObjectID,
    selector: AudioObjectPropertySelector,
    scope: AudioObjectPropertyScope,
) -> u32 {
    property::<u32>(object, selector, scope)
        .inspect_err(|error| {
            tracing::debug!(
                object,
                error = format!("{error:#}"),
                "audio latency not known"
            );
        })
        .unwrap_or(0)
}

/// How long `frames` take at `rate` frames a second; nothing for no rate.
pub fn frames_to_duration(frames: u32, rate: f64) -> Duration {
    if rate > 0.0 && rate.is_finite() {
        Duration::from_secs_f64(f64::from(frames) / rate)
    } else {
        Duration::ZERO
    }
}

fn address(
    selector: AudioObjectPropertySelector,
    scope: AudioObjectPropertyScope,
) -> AudioObjectPropertyAddress {
    AudioObjectPropertyAddress {
        mSelector: selector,
        mScope: scope,
        mElement: kAudioObjectPropertyElementMain,
    }
}

/// A property that is one plain number, such as a `u32` or an `f64`.
fn property<T: Copy + Default>(
    object: AudioObjectID,
    selector: AudioObjectPropertySelector,
    scope: AudioObjectPropertyScope,
) -> Result<T> {
    let address = address(selector, scope);
    let mut value = T::default();
    let mut size = size_of::<T>() as u32;
    // SAFETY: the address, the size and the value are live locals, and the
    // HAL writes at most `size` bytes into a `T` that takes any bit pattern
    // (callers ask for integers and floats).
    let status = unsafe {
        AudioObjectGetPropertyData(
            object,
            NonNull::from(&address),
            0,
            ptr::null(),
            NonNull::from(&mut size),
            NonNull::from(&mut value).cast(),
        )
    };
    check("AudioObjectGetPropertyData", status)?;
    Ok(value)
}

/// A property that is a list of 32-bit numbers: streams, or devices.
fn array<T: Copy + Default>(
    object: AudioObjectID,
    selector: AudioObjectPropertySelector,
    scope: AudioObjectPropertyScope,
) -> Result<Vec<T>> {
    let address = address(selector, scope);
    let mut size = 0u32;
    // SAFETY: as in `property`.
    let status = unsafe {
        AudioObjectGetPropertyDataSize(
            object,
            NonNull::from(&address),
            0,
            ptr::null(),
            NonNull::from(&mut size),
        )
    };
    check("AudioObjectGetPropertyDataSize", status)?;
    let mut values = vec![T::default(); size as usize / size_of::<T>()];
    let mut size = (values.len() * size_of::<T>()) as u32;
    if values.is_empty() {
        return Ok(values);
    }
    // SAFETY: `values` holds `size` bytes, which the HAL fills with numbers.
    let status = unsafe {
        AudioObjectGetPropertyData(
            object,
            NonNull::from(&address),
            0,
            ptr::null(),
            NonNull::from(&mut size),
            NonNull::new(values.as_mut_ptr())
                .context("no room for the list")?
                .cast(),
        )
    };
    check("AudioObjectGetPropertyData", status)?;
    values.truncate(size as usize / size_of::<T>());
    Ok(values)
}

/// A property that is a CFString, which the caller releases.
fn string(object: AudioObjectID, selector: AudioObjectPropertySelector) -> Result<String> {
    let address = address(selector, kAudioObjectPropertyScopeGlobal);
    let mut raw: *const CFString = ptr::null();
    let mut size = size_of::<*const CFString>() as u32;
    // SAFETY: the HAL writes one retained CFString pointer into `raw`.
    let status = unsafe {
        AudioObjectGetPropertyData(
            object,
            NonNull::from(&address),
            0,
            ptr::null(),
            NonNull::from(&mut size),
            NonNull::from(&mut raw).cast(),
        )
    };
    check("AudioObjectGetPropertyData", status)?;
    let string = NonNull::new(raw.cast_mut()).context("the HAL gave no string")?;
    // SAFETY: a string property comes with a +1 retain count, as `from_raw`
    // needs, and it's released when this drops.
    let string: CFRetained<CFString> = unsafe { CFRetained::from_raw(string) };
    Ok(string.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(id: u32, uid: &str, name: &str, inputs: usize, outputs: usize) -> Device {
        Device {
            id,
            uid: uid.into(),
            name: name.into(),
            input_streams: inputs,
            output_streams: outputs,
            hidden: false,
        }
    }

    fn sample() -> Vec<Device> {
        vec![
            device(1, "BuiltInSpeakerDevice", "MacBook Pro Speakers", 0, 1),
            device(2, "BuiltInMicrophoneDevice", "MacBook Pro Microphone", 1, 0),
            device(3, "AppleUSBAudioEngine:1", "USB Headset", 1, 1),
        ]
    }

    #[test]
    fn a_uid_picks_its_device_in_the_right_direction() {
        let devices = sample();
        assert_eq!(
            pick(&devices, "AppleUSBAudioEngine:1", true).map(|d| d.id),
            Some(3)
        );
        assert_eq!(
            pick(&devices, "AppleUSBAudioEngine:1", false).map(|d| d.id),
            Some(3)
        );
        assert_eq!(
            pick(&devices, "BuiltInMicrophoneDevice", false).map(|d| d.id),
            Some(2)
        );
        assert_eq!(pick(&devices, "BuiltInMicrophoneDevice", true), None);
    }

    #[test]
    fn a_named_id_picks_by_name() {
        let devices = sample();
        let picked = pick(&devices, "name:MacBook Pro Speakers", true);
        assert_eq!(picked.map(|d| d.id), Some(1));
        assert_eq!(pick(&devices, "name:Nothing", true), None);
    }

    #[test]
    fn an_unknown_uid_picks_nothing() {
        assert_eq!(pick(&sample(), "gone", true), None);
    }

    #[test]
    fn hidden_devices_and_devices_without_streams_serve_nobody() {
        let mut hidden = device(4, "Aggregate", "Aggregate", 1, 1);
        hidden.hidden = true;
        assert!(!hidden.serves(true));
        assert!(!hidden.serves(false));
        assert!(!device(5, "x", "x", 0, 0).serves(true));
        assert!(device(6, "y", "y", 0, 2).serves(true));
        assert!(!device(6, "y", "y", 0, 2).serves(false));
    }

    #[test]
    fn frames_become_time_at_the_devices_rate() {
        assert_eq!(frames_to_duration(480, 48_000.0), Duration::from_millis(10));
        assert_eq!(frames_to_duration(480, 0.0), Duration::ZERO);
        assert_eq!(frames_to_duration(480, f64::NAN), Duration::ZERO);
    }

    #[test]
    fn statuses_read_as_four_characters_when_they_are() {
        assert_eq!(describe(0x7768_6174), "'what' (2003329396)");
        assert_eq!(describe(-50), "-50");
    }

    #[test]
    fn a_failed_status_names_the_call() {
        assert!(check("AudioUnitInitialize", 0).is_ok());
        let error = check("AudioUnitInitialize", -10_868)
            .unwrap_err()
            .to_string();
        assert!(error.contains("AudioUnitInitialize"), "{error}");
    }
}
