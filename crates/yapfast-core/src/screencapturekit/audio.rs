//! What the other applications play, from ScreenCaptureKit: the sound of a
//! display, minus Yapfast's own and the Discord clients', as interleaved
//! stereo `f32` at 48 kHz, which is what the stream's mixer takes. The system
//! asks for a picture too, so the stream captures a tiny one, once a second.

use std::{
    ptr::NonNull,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
    time::Duration,
};

use anyhow::{Context, Result, bail};
use objc2::{AnyThread, rc::Retained};
use objc2_core_audio_types::{
    AudioBufferList, kAudioFormatFlagIsFloat, kAudioFormatFlagIsNonInterleaved,
};
use objc2_core_foundation::CFRetained;
use objc2_core_media::{
    CMAudioFormatDescriptionGetStreamBasicDescription, CMBlockBuffer, CMSampleBuffer,
};
use objc2_foundation::NSArray;
use objc2_screen_capture_kit::{
    SCContentFilter, SCRunningApplication, SCStreamOutputType, SCWindow,
};

use super::{
    content::{Capturable, Content},
    stream::{Settings, Stream},
};

/// The stream's picture is as small as it can be, as it is not used.
const PICTURE_SIDE: u32 = 64;
const PICTURE_FPS: u32 = 1;

/// How often the thread looks whether it should stop.
const STOP_POLL: Duration = Duration::from_millis(50);

/// What a float sample is made of.
const BITS_PER_F32: u32 = 32;

/// How a sample's audio buffers are laid out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Layout {
    pub channels: usize,
    /// One buffer for each channel, rather than one with them interleaved.
    pub planar: bool,
}

/// The layout the format flags describe, if it is 32 bit float, the only
/// format ScreenCaptureKit gives.
pub(super) fn layout(flags: u32, channels: u32, bits: u32) -> Option<Layout> {
    let float = flags & kAudioFormatFlagIsFloat != 0;
    (float && bits == BITS_PER_F32 && channels > 0).then_some(Layout {
        channels: channels as usize,
        planar: flags & kAudioFormatFlagIsNonInterleaved != 0,
    })
}

/// Makes interleaved stereo of `buffers`, which are laid out as `layout`
/// says: a mono sound goes on both sides, and channels past two are left out.
pub(super) fn to_stereo(layout: Layout, buffers: &[&[f32]], out: &mut Vec<f32>) {
    out.clear();
    if layout.planar {
        let left = buffers.first().copied().unwrap_or_default();
        let right = buffers.get(1).copied().unwrap_or(left);
        for (left, right) in left.iter().zip(right) {
            out.extend([*left, *right]);
        }
    } else if let Some(frames) = buffers.first() {
        for frame in frames.chunks_exact(layout.channels) {
            let right = frame.get(1).unwrap_or(&frame[0]);
            out.extend([frame[0], *right]);
        }
    }
}

/// Hears the other applications until `stop`, calling `on_audio` with
/// interleaved stereo, on a thread of its own, and `on_end` as that thread
/// ends; applications `skip` picks by name are left out. Returns once it
/// hears, or with why it can't.
pub(crate) fn listen(
    skip: fn(&str) -> bool,
    on_audio: impl FnMut(&[f32]) + Send + 'static,
    on_end: impl FnOnce() + Send + 'static,
    stop: Arc<AtomicBool>,
) -> Result<JoinHandle<()>> {
    let (ready, started) = std::sync::mpsc::sync_channel(1);
    let thread = std::thread::Builder::new()
        .name("yapfast-screen-sound".into())
        .spawn(move || {
            let stream = match start(skip, on_audio) {
                Ok(stream) => {
                    let _ = ready.send(Ok(()));
                    stream
                }
                Err(error) => {
                    let _ = ready.send(Err(error));
                    return;
                }
            };
            while !stop.load(Ordering::Relaxed) {
                std::thread::sleep(STOP_POLL);
            }
            drop(stream);
            on_end();
        })
        .context("could not start hearing the other applications")?;
    started
        .recv()
        .context("hearing the other applications stopped before it started")??;
    Ok(thread)
}

fn start(skip: fn(&str) -> bool, on_audio: impl FnMut(&[f32]) + Send + 'static) -> Result<Stream> {
    let content = Content::fetch()?;
    let display = content.display(Capturable::MainDisplay)?;
    let skipped: Vec<Retained<SCRunningApplication>> = content
        .applications()
        .into_iter()
        .filter(|(name, _)| skip(name))
        .map(|(_, application)| application)
        .collect();
    tracing::info!(skipped = skipped.len(), "hearing the other applications");
    let applications = NSArray::from_retained_slice(&skipped);
    let no_windows: Retained<NSArray<SCWindow>> = NSArray::new();
    // SAFETY: the display and arrays are live.
    let filter = unsafe {
        SCContentFilter::initWithDisplay_excludingApplications_exceptingWindows(
            SCContentFilter::alloc(),
            &display,
            &applications,
            &no_windows,
        )
    };
    Stream::start(
        &filter,
        Settings {
            width: PICTURE_SIDE,
            height: PICTURE_SIDE,
            fps: PICTURE_FPS,
            audio: true,
        },
        sample_handler(on_audio),
        Box::new(|message| tracing::warn!(%message, "hearing the other applications ended")),
    )
}

/// What the stream does with each sample: the sound goes to `on_audio`.
fn sample_handler(
    mut on_audio: impl FnMut(&[f32]) + Send + 'static,
) -> super::stream::SampleHandler {
    let mut stereo = Vec::new();
    let mut warned = false;
    Box::new(move |kind: SCStreamOutputType, sample: &CMSampleBuffer| {
        if kind != SCStreamOutputType::Audio {
            return;
        }
        match read(sample, &mut stereo) {
            Ok(()) => on_audio(&stereo),
            Err(error) if !warned => {
                warned = true;
                tracing::warn!(
                    error = format!("{error:#}"),
                    "could not read the other applications' sound"
                );
            }
            Err(_) => {}
        }
    })
}

/// Reads a sample's sound into `stereo`.
fn read(sample: &CMSampleBuffer, stereo: &mut Vec<f32>) -> Result<()> {
    // SAFETY: plain getters; the description lives as long as the sample.
    let description = unsafe { sample.format_description() }.context("sound without a format")?;
    // SAFETY: null for a description that is not audio, which is checked.
    let format = unsafe { CMAudioFormatDescriptionGetStreamBasicDescription(&description) };
    // SAFETY: a pointer to a description that lives as long as `description`.
    let format = unsafe { format.as_ref() }.context("sound with no audio format")?;
    let Some(layout) = layout(
        format.mFormatFlags,
        format.mChannelsPerFrame,
        format.mBitsPerChannel,
    ) else {
        bail!("the sound is not 32 bit float");
    };
    // The list has a buffer for each channel, which a plain list does not
    // have room for: the system says how much it needs.
    let mut needed = 0usize;
    // SAFETY: with no list given it only tells the size needed.
    let status = unsafe {
        sample.audio_buffer_list_with_retained_block_buffer(
            &mut needed,
            std::ptr::null_mut(),
            0,
            None,
            None,
            0,
            std::ptr::null_mut(),
        )
    };
    if needed == 0 {
        bail!("the sound has no buffer list (status {status})");
    }
    // Eight byte units keep the list aligned.
    let mut room = vec![0u64; needed.div_ceil(8)];
    let list = room.as_mut_ptr().cast::<AudioBufferList>();
    let mut block: *mut CMBlockBuffer = std::ptr::null_mut();
    // SAFETY: `room` is at least as large as the system asked for.
    let status = unsafe {
        sample.audio_buffer_list_with_retained_block_buffer(
            std::ptr::null_mut(),
            list,
            room.len() * 8,
            None,
            None,
            0,
            &mut block,
        )
    };
    // SAFETY: the call retained the block buffer, which this releases when done.
    let _block = NonNull::new(block).map(|block| unsafe { CFRetained::from_raw(block) });
    if status != 0 {
        bail!("could not get the sound's buffers (status {status})");
    }
    // SAFETY: the list was filled in above, and its buffers last as long as `_block`.
    let buffers = unsafe { slices(&*list) };
    to_stereo(layout, &buffers, stereo);
    Ok(())
}

/// The float samples of each buffer in `list`.
///
/// # Safety
/// `list` must be filled in with as many buffers as it says, which must
/// outlive the returned slices.
unsafe fn slices(list: &AudioBufferList) -> Vec<&[f32]> {
    // SAFETY: the caller's promise of that many buffers.
    let buffers =
        unsafe { std::slice::from_raw_parts(list.mBuffers.as_ptr(), list.mNumberBuffers as usize) };
    buffers
        .iter()
        .filter_map(|buffer| {
            let data = buffer.mData.cast::<f32>();
            let len = buffer.mDataByteSize as usize / size_of::<f32>();
            // SAFETY: the caller's promise that the buffers are live.
            (!data.is_null() && data.is_aligned())
                .then(|| unsafe { std::slice::from_raw_parts(data, len) })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_32_bit_float_is_understood() {
        let planar = kAudioFormatFlagIsFloat | kAudioFormatFlagIsNonInterleaved;
        assert_eq!(
            layout(planar, 2, 32),
            Some(Layout {
                channels: 2,
                planar: true
            })
        );
        assert_eq!(
            layout(kAudioFormatFlagIsFloat, 2, 32),
            Some(Layout {
                channels: 2,
                planar: false
            })
        );
        assert_eq!(layout(0, 2, 16), None);
        assert_eq!(layout(kAudioFormatFlagIsFloat, 2, 64), None);
        assert_eq!(layout(kAudioFormatFlagIsFloat, 0, 32), None);
    }

    #[test]
    fn planar_channels_are_interleaved() {
        let mut out = vec![9.0];
        let layout = Layout {
            channels: 2,
            planar: true,
        };
        to_stereo(layout, &[&[1.0, 2.0], &[10.0, 20.0]], &mut out);
        assert_eq!(out, [1.0, 10.0, 2.0, 20.0]);
    }

    #[test]
    fn a_mono_sound_goes_on_both_sides() {
        let mut out = vec![];
        to_stereo(
            Layout {
                channels: 1,
                planar: true,
            },
            &[&[1.0, 2.0]],
            &mut out,
        );
        assert_eq!(out, [1.0, 1.0, 2.0, 2.0]);
        to_stereo(
            Layout {
                channels: 1,
                planar: false,
            },
            &[&[3.0, 4.0]],
            &mut out,
        );
        assert_eq!(out, [3.0, 3.0, 4.0, 4.0]);
    }

    #[test]
    fn interleaved_surround_keeps_the_first_two_channels() {
        let mut out = vec![];
        let layout = Layout {
            channels: 3,
            planar: false,
        };
        to_stereo(layout, &[&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]], &mut out);
        assert_eq!(out, [1.0, 2.0, 4.0, 5.0]);
    }

    #[test]
    fn no_buffers_give_no_sound() {
        let mut out = vec![1.0];
        to_stereo(
            Layout {
                channels: 2,
                planar: true,
            },
            &[],
            &mut out,
        );
        assert!(out.is_empty());
    }

    /// Hears the system for a few seconds and reports what came, to see on a
    /// Mac with the permission granted that the sound capture works. Play
    /// something meanwhile for samples that are not silence.
    #[test]
    #[ignore = "needs the Screen Recording permission"]
    fn hears_the_other_applications() {
        use std::sync::atomic::AtomicUsize;
        let (samples, loudest) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
        let (counted, peak) = (samples.clone(), loudest.clone());
        let stop = Arc::new(AtomicBool::new(false));
        let thread = listen(
            |_| false,
            move |sound| {
                counted.fetch_add(sound.len(), Ordering::Relaxed);
                let top = sound
                    .iter()
                    .fold(0.0f32, |top, sample| top.max(sample.abs()));
                peak.fetch_max((top * 1000.0) as usize, Ordering::Relaxed);
            },
            || eprintln!("sound capture ended"),
            stop.clone(),
        )
        .expect("start hearing");
        std::thread::sleep(Duration::from_secs(3));
        stop.store(true, Ordering::Relaxed);
        thread.join().expect("the thread ends");
        let count = samples.load(Ordering::Relaxed);
        eprintln!(
            "{count} samples in 3 s (stereo at 48 kHz gives about 288000), peak {:.3}",
            loudest.load(Ordering::Relaxed) as f32 / 1000.0
        );
        assert!(count > 0, "no sound came");
    }
}
