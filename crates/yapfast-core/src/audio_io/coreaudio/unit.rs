//! An AUHAL unit (`kAudioUnitSubType_HALOutput`): Core Audio's audio unit
//! for one hardware device, either to play to it or to record from it.
//!
//! Yapfast runs its own echo cancellation and noise suppression, so this is
//! the plain HAL unit and not VoiceProcessingIO, which would process the
//! voice a second time and take over the device's gain and ducking.
//!
//! The unit converts between the device's format and the one Yapfast asks
//! for, rate and channel count included, on the app side of its buses.

use std::{ffi::c_void, ptr, ptr::NonNull};

use anyhow::{Context as _, Result, bail};
use objc2_audio_toolbox::{
    AURenderCallback, AURenderCallbackStruct, AudioComponentDescription, AudioComponentFindNext,
    AudioComponentInstanceDispose, AudioComponentInstanceNew, AudioOutputUnitStart,
    AudioOutputUnitStop, AudioUnit, AudioUnitInitialize, AudioUnitSetProperty,
    AudioUnitUninitialize, kAudioOutputUnitProperty_CurrentDevice,
    kAudioOutputUnitProperty_EnableIO, kAudioOutputUnitProperty_SetInputCallback,
    kAudioUnitManufacturer_Apple, kAudioUnitProperty_MaximumFramesPerSlice,
    kAudioUnitProperty_SetRenderCallback, kAudioUnitProperty_StreamFormat, kAudioUnitScope_Global,
    kAudioUnitScope_Input, kAudioUnitScope_Output, kAudioUnitSubType_HALOutput,
    kAudioUnitType_Output,
};
use objc2_core_audio_types::{
    AudioStreamBasicDescription, kAudioFormatFlagIsFloat, kAudioFormatFlagIsPacked,
    kAudioFormatFlagIsSignedInteger, kAudioFormatLinearPCM,
};

use super::{
    super::Format,
    devices::{DeviceId, check},
};

/// The HAL unit's bus that feeds the speakers, and its scope on the app's
/// side: what the app gives the unit to play.
const OUTPUT_BUS: u32 = 0;
/// The bus that records, and its scope on the app's side: what the unit
/// gives the app.
const INPUT_BUS: u32 = 1;

/// The most frames one callback may carry. The unit's default is 4096; a
/// microphone's scratch buffer is made for this many.
pub const MAX_FRAMES: u32 = 8_192;

const ON: u32 = 1;
const OFF: u32 = 0;

/// What the app side of a unit's bus carries for `format`: interleaved
/// 16-bit integers or 32-bit floats in the machine's byte order.
pub fn description(format: Format) -> AudioStreamBasicDescription {
    let bytes_per_sample = if format.float { 4 } else { 2 };
    let kind = if format.float {
        kAudioFormatFlagIsFloat
    } else {
        kAudioFormatFlagIsSignedInteger
    };
    let bytes_per_frame = u32::from(format.channels) * bytes_per_sample;
    AudioStreamBasicDescription {
        mSampleRate: f64::from(format.rate),
        mFormatID: kAudioFormatLinearPCM,
        mFormatFlags: kind | kAudioFormatFlagIsPacked,
        mBytesPerPacket: bytes_per_frame,
        mFramesPerPacket: 1,
        mBytesPerFrame: bytes_per_frame,
        mChannelsPerFrame: u32::from(format.channels),
        mBitsPerChannel: bytes_per_sample * 8,
        mReserved: 0,
    }
}

/// Which way a unit goes.
#[derive(Clone, Copy)]
pub enum Direction {
    Playback,
    Capture,
}

/// A HAL unit, stopped and disposed of when dropped.
pub struct Unit {
    unit: AudioUnit,
    started: bool,
}

// SAFETY: an audio unit may be configured, started and stopped from any one
// thread at a time, and a `Unit` is used by its owner alone.
unsafe impl Send for Unit {}

impl Unit {
    /// Makes a unit for `device`, taking `format` on the app's side, that
    /// will call `callback` with `context` once [`Unit::start`] runs. The
    /// context must stay alive until the unit drops.
    pub fn open(
        direction: Direction,
        device: DeviceId,
        format: Format,
        callback: AURenderCallback,
        context: *mut c_void,
    ) -> Result<Self> {
        let mut unit = Self::create()?;
        unit.configure(direction, device, format, callback, context)?;
        Ok(unit)
    }

    /// Starts the callbacks.
    pub fn start(&mut self) -> Result<()> {
        // SAFETY: the unit is a live HAL unit that is configured.
        check("AudioUnitInitialize", unsafe {
            AudioUnitInitialize(self.unit)
        })?;
        // SAFETY: as above, and initialized.
        check("AudioOutputUnitStart", unsafe {
            AudioOutputUnitStart(self.unit)
        })?;
        self.started = true;
        Ok(())
    }

    /// The raw unit, for a capture callback to render from.
    pub fn raw(&self) -> AudioUnit {
        self.unit
    }

    fn create() -> Result<Self> {
        let description = AudioComponentDescription {
            componentType: kAudioUnitType_Output,
            componentSubType: kAudioUnitSubType_HALOutput,
            componentManufacturer: kAudioUnitManufacturer_Apple,
            componentFlags: 0,
            componentFlagsMask: 0,
        };
        // SAFETY: the description is a live local; null starts the search.
        let component =
            unsafe { AudioComponentFindNext(ptr::null_mut(), NonNull::from(&description)) };
        if component.is_null() {
            bail!("Core Audio has no HAL output unit");
        }
        let mut unit: AudioUnit = ptr::null_mut();
        // SAFETY: `component` was just found, and `unit` is a live local.
        check("AudioComponentInstanceNew", unsafe {
            AudioComponentInstanceNew(component, NonNull::from(&mut unit))
        })?;
        Ok(Self {
            unit,
            started: false,
        })
    }

    fn configure(
        &mut self,
        direction: Direction,
        device: DeviceId,
        format: Format,
        callback: AURenderCallback,
        context: *mut c_void,
    ) -> Result<()> {
        let (playing, recording) = match direction {
            Direction::Playback => (ON, OFF),
            Direction::Capture => (OFF, ON),
        };
        self.set(
            kAudioOutputUnitProperty_EnableIO,
            kAudioUnitScope_Input,
            INPUT_BUS,
            &recording,
        )?;
        self.set(
            kAudioOutputUnitProperty_EnableIO,
            kAudioUnitScope_Output,
            OUTPUT_BUS,
            &playing,
        )?;
        self.set(
            kAudioOutputUnitProperty_CurrentDevice,
            kAudioUnitScope_Global,
            OUTPUT_BUS,
            &device,
        )?;
        self.set(
            kAudioUnitProperty_MaximumFramesPerSlice,
            kAudioUnitScope_Global,
            OUTPUT_BUS,
            &MAX_FRAMES,
        )?;
        let wanted = description(format);
        let call = AURenderCallbackStruct {
            inputProc: callback,
            inputProcRefCon: context,
        };
        match direction {
            Direction::Playback => {
                self.set(
                    kAudioUnitProperty_StreamFormat,
                    kAudioUnitScope_Input,
                    OUTPUT_BUS,
                    &wanted,
                )?;
                self.set(
                    kAudioUnitProperty_SetRenderCallback,
                    kAudioUnitScope_Input,
                    OUTPUT_BUS,
                    &call,
                )
            }
            Direction::Capture => {
                self.set(
                    kAudioUnitProperty_StreamFormat,
                    kAudioUnitScope_Output,
                    INPUT_BUS,
                    &wanted,
                )
                .context("the microphone won't deliver this rate or channel count")?;
                self.set(
                    kAudioOutputUnitProperty_SetInputCallback,
                    kAudioUnitScope_Global,
                    OUTPUT_BUS,
                    &call,
                )
            }
        }
    }

    fn set<T>(&mut self, property: u32, scope: u32, element: u32, value: &T) -> Result<()> {
        // SAFETY: `value` is a live `T`, and the size is its own.
        let status = unsafe {
            AudioUnitSetProperty(
                self.unit,
                property,
                scope,
                element,
                ptr::from_ref(value).cast(),
                size_of::<T>() as u32,
            )
        };
        check(&format!("AudioUnitSetProperty {property}"), status)
    }
}

impl Drop for Unit {
    fn drop(&mut self) {
        if self.started {
            // SAFETY: the unit is live and started.
            let status = unsafe { AudioOutputUnitStop(self.unit) };
            log_failure("AudioOutputUnitStop", check("AudioOutputUnitStop", status));
        }
        // SAFETY: the unit is live; uninitializing an unconfigured one is
        // harmless, and disposing ends it. No callback runs after Stop.
        let status = unsafe { AudioUnitUninitialize(self.unit) };
        log_failure(
            "AudioUnitUninitialize",
            check("AudioUnitUninitialize", status),
        );
        // SAFETY: as above, and the unit isn't used again.
        let status = unsafe { AudioComponentInstanceDispose(self.unit) };
        log_failure(
            "AudioComponentInstanceDispose",
            check("AudioComponentInstanceDispose", status),
        );
    }
}

/// What a callback works on: made on the heap for the unit to point at, and
/// freed when this drops, which must come after the unit's own drop.
pub struct Context<T>(NonNull<T>);

// SAFETY: the context is owned by one stream, and its callback is the only
// other thread to touch it, through the pointer the unit holds.
unsafe impl<T: Send> Send for Context<T> {}

impl<T> Context<T> {
    pub fn new(value: T) -> Self {
        Self(NonNull::from(Box::leak(Box::new(value))))
    }

    /// The pointer a unit gives its callback.
    pub fn as_ptr(&self) -> *mut c_void {
        self.0.as_ptr().cast()
    }

    /// The context for setting up what the callback shares, before the unit
    /// starts. Only fields built for sharing, such as atomics, may be used.
    pub fn shared(&self) -> &T {
        // SAFETY: the pointer is live until drop, and no callback runs yet
        // when this is used, so nothing else holds a mutable reference.
        unsafe { self.0.as_ref() }
    }
}

impl<T> Drop for Context<T> {
    fn drop(&mut self) {
        // SAFETY: the pointer came from `Box::leak` and is freed here once.
        drop(unsafe { Box::from_raw(self.0.as_ptr()) });
    }
}

fn log_failure(call: &str, result: Result<()>) {
    if let Err(error) = result {
        tracing::debug!(call, error = format!("{error:#}"), "closing an audio unit");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speech_for_discord_is_mono_16_bit_at_48_khz() {
        let wanted = description(Format {
            rate: 48_000,
            channels: 1,
            float: false,
        });
        assert_eq!(wanted.mSampleRate, 48_000.0);
        assert_eq!(wanted.mBytesPerFrame, 2);
        assert_eq!(wanted.mBitsPerChannel, 16);
        assert_eq!(wanted.mChannelsPerFrame, 1);
        assert_eq!(
            wanted.mFormatFlags,
            kAudioFormatFlagIsSignedInteger | kAudioFormatFlagIsPacked
        );
    }

    #[test]
    fn stereo_float_has_eight_bytes_a_frame() {
        let wanted = description(Format {
            rate: 44_100,
            channels: 2,
            float: true,
        });
        assert_eq!(wanted.mBytesPerFrame, 8);
        assert_eq!(wanted.mBytesPerPacket, 8);
        assert_eq!(wanted.mFramesPerPacket, 1);
        assert_eq!(wanted.mBitsPerChannel, 32);
        assert_eq!(
            wanted.mFormatFlags,
            kAudioFormatFlagIsFloat | kAudioFormatFlagIsPacked
        );
    }
}
