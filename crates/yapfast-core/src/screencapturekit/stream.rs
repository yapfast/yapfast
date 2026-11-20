//! An `SCStream` running until dropped, with its delegate on a serial
//! dispatch queue. [`super::video`] and [`super::audio`] each build one with
//! what they capture and what they do with the samples.

use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{Mutex, mpsc},
    time::Duration,
};

use anyhow::{Result, anyhow, bail};
use block2::RcBlock;
use dispatch2::{DispatchQueue, DispatchRetained};
use objc2::{
    AnyThread, DefinedClass, define_class, msg_send, rc::Retained, runtime::ProtocolObject,
};
use objc2_core_media::{CMSampleBuffer, CMTime};
use objc2_core_video::kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange;
use objc2_foundation::{NSError, NSObject, NSObjectProtocol};
use objc2_screen_capture_kit::{
    SCContentFilter, SCStream, SCStreamConfiguration, SCStreamDelegate, SCStreamOutput,
    SCStreamOutputType,
};

use super::permission;

/// How long to wait for the stream to start or stop.
const START_TIMEOUT: Duration = Duration::from_secs(10);
const STOP_TIMEOUT: Duration = Duration::from_secs(3);

/// The label of the queue samples arrive on.
const QUEUE_LABEL: &str = "yapfast.screen.samples";

/// How many frames the system keeps for a slow queue before it drops them.
const QUEUE_DEPTH: isize = 4;

/// The sound's format: what Yapfast's stream mixes.
const AUDIO_SAMPLE_RATE: isize = 48_000;
const AUDIO_CHANNELS: isize = 2;

/// What to capture besides the filter's content.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Settings {
    /// The size of the pictures, in pixels.
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    /// Whether the other applications' sound comes too.
    pub audio: bool,
}

/// Gets every sample the stream makes, on its queue.
pub(super) type SampleHandler = Box<dyn FnMut(SCStreamOutputType, &CMSampleBuffer) + Send>;

/// Hears why the stream ended, which the person may have done from the
/// system's menu bar.
pub(super) type StopHandler = Box<dyn Fn(String) + Send + Sync>;

/// How long a frame lasts at `fps`, as `(value, timescale)` of a `CMTime`.
pub(super) fn frame_interval(fps: u32) -> (i64, i32) {
    (1, fps.clamp(1, 120) as i32)
}

struct DelegateIvars {
    on_sample: Mutex<SampleHandler>,
    on_stop: StopHandler,
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements, and the class has no
    // Drop of its own beyond its ivars.
    #[unsafe(super(NSObject))]
    #[ivars = DelegateIvars]
    struct StreamDelegate;

    unsafe impl NSObjectProtocol for StreamDelegate {}

    unsafe impl SCStreamOutput for StreamDelegate {
        #[unsafe(method(stream:didOutputSampleBuffer:ofType:))]
        fn stream_did_output_sample_buffer_of_type(
            &self,
            _stream: &SCStream,
            sample: &CMSampleBuffer,
            kind: SCStreamOutputType,
        ) {
            // A panic must not unwind into ScreenCaptureKit.
            let result = catch_unwind(AssertUnwindSafe(|| match self.ivars().on_sample.lock() {
                Ok(mut handler) => handler(kind, sample),
                Err(_) => tracing::error!("the screen sample handler is poisoned"),
            }));
            if result.is_err() {
                tracing::error!("the screen sample handler panicked; the sample is dropped");
            }
        }
    }

    unsafe impl SCStreamDelegate for StreamDelegate {
        #[unsafe(method(stream:didStopWithError:))]
        fn stream_did_stop_with_error(&self, _stream: &SCStream, error: &NSError) {
            let result = catch_unwind(AssertUnwindSafe(|| {
                let message =
                    permission::explain(error.code(), &error.localizedDescription().to_string());
                tracing::warn!(%message, "the screen stream stopped");
                (self.ivars().on_stop)(message);
            }));
            if result.is_err() {
                tracing::error!("the screen stop handler panicked");
            }
        }
    }
);

impl StreamDelegate {
    fn new(on_sample: SampleHandler, on_stop: StopHandler) -> Retained<Self> {
        let this = Self::alloc().set_ivars(DelegateIvars {
            on_sample: Mutex::new(on_sample),
            on_stop,
        });
        // SAFETY: NSObject's init.
        unsafe { msg_send![super(this), init] }
    }
}

/// A capture running through ScreenCaptureKit until dropped.
pub(super) struct Stream {
    stream: Retained<SCStream>,
    queue: DispatchRetained<DispatchQueue>,
    audio: bool,
    /// Alive as long as the stream calls it, which holds it weakly.
    delegate: Retained<StreamDelegate>,
}

// SAFETY: SCStream starts and stops from any thread, and is only touched
// through this handle by whoever holds it; samples arrive on its own queue
// into handlers that are Send.
unsafe impl Send for Stream {}

/// The configuration for `settings`.
fn configuration(settings: Settings) -> Retained<SCStreamConfiguration> {
    let (value, timescale) = frame_interval(settings.fps);
    // SAFETY: plain setters on an object made here.
    unsafe {
        let config = SCStreamConfiguration::new();
        config.setWidth(settings.width as usize);
        config.setHeight(settings.height as usize);
        config.setMinimumFrameInterval(CMTime::new(value, timescale));
        config.setPixelFormat(kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange);
        config.setShowsCursor(true);
        config.setQueueDepth(QUEUE_DEPTH);
        if settings.audio {
            config.setCapturesAudio(true);
            config.setSampleRate(AUDIO_SAMPLE_RATE);
            config.setChannelCount(AUDIO_CHANNELS);
            // Yapfast's own sound, a call, never goes into its stream.
            config.setExcludesCurrentProcessAudio(true);
        }
        config
    }
}

/// Waits for a completion handler's answer, and what it says went wrong.
fn wait_for(
    answered: &mpsc::Receiver<Option<String>>,
    timeout: Duration,
    what: &str,
) -> Result<()> {
    match answered.recv_timeout(timeout) {
        Ok(None) => Ok(()),
        Ok(Some(message)) => bail!(message),
        Err(_) => bail!("The system did not {what} in time."),
    }
}

/// A completion handler that says whether the call failed, and why.
fn completion(answer: mpsc::SyncSender<Option<String>>) -> RcBlock<dyn Fn(*mut NSError)> {
    RcBlock::new(move |error: *mut NSError| {
        let outcome = (!error.is_null()).then(|| super::content::describe(error));
        let _ = answer.try_send(outcome);
    })
}

impl Stream {
    /// Starts capturing `filter`'s content as `settings` says, calling
    /// `on_sample` with each sample and `on_stop` if the system ends it.
    pub(super) fn start(
        filter: &SCContentFilter,
        settings: Settings,
        on_sample: SampleHandler,
        on_stop: StopHandler,
    ) -> Result<Self> {
        let config = configuration(settings);
        let delegate = StreamDelegate::new(on_sample, on_stop);
        let queue = DispatchQueue::new(QUEUE_LABEL, None);
        // SAFETY: the filter and configuration are live; the delegate outlives
        // the stream through this struct.
        let stream = unsafe {
            SCStream::initWithFilter_configuration_delegate(
                SCStream::alloc(),
                filter,
                &config,
                Some(ProtocolObject::from_ref(&*delegate)),
            )
        };
        let this = Self {
            stream,
            queue,
            audio: settings.audio,
            delegate,
        };
        this.add_outputs()?;
        this.begin()?;
        tracing::info!(?settings, "screen capture running");
        Ok(this)
    }

    /// Takes the video, and the sound when asked for. Video is taken even
    /// for sound alone, or the system logs every frame it drops.
    fn add_outputs(&self) -> Result<()> {
        let output = ProtocolObject::from_ref(&*self.delegate);
        let kinds = [SCStreamOutputType::Screen]
            .into_iter()
            .chain(self.audio.then_some(SCStreamOutputType::Audio));
        for kind in kinds {
            // SAFETY: the output and queue are live for the stream's life.
            unsafe {
                self.stream.addStreamOutput_type_sampleHandlerQueue_error(
                    output,
                    kind,
                    Some(&self.queue),
                )
            }
            .map_err(|error| anyhow!("Could not take the screen's samples: {error}"))?;
        }
        Ok(())
    }

    fn begin(&self) -> Result<()> {
        let (answer, answered) = mpsc::sync_channel(1);
        let handler = completion(answer);
        // SAFETY: the handler is copied by the call and may run on any thread.
        unsafe {
            self.stream
                .startCaptureWithCompletionHandler(Some(&handler))
        };
        wait_for(&answered, START_TIMEOUT, "start screen capture")
    }
}

impl Drop for Stream {
    fn drop(&mut self) {
        let (answer, answered) = mpsc::sync_channel(1);
        let handler = completion(answer);
        // SAFETY: plain calls on a live stream.
        unsafe { self.stream.stopCaptureWithCompletionHandler(Some(&handler)) };
        if let Err(error) = wait_for(&answered, STOP_TIMEOUT, "stop screen capture") {
            tracing::warn!(
                error = format!("{error:#}"),
                "screen capture did not stop cleanly"
            );
        }
        let output = ProtocolObject::from_ref(&*self.delegate);
        let kinds = [SCStreamOutputType::Screen]
            .into_iter()
            .chain(self.audio.then_some(SCStreamOutputType::Audio));
        for kind in kinds {
            // SAFETY: removes what was added.
            if let Err(error) = unsafe { self.stream.removeStreamOutput_type_error(output, kind) } {
                tracing::debug!(%error, "could not remove a screen output");
            }
        }
        // A sample already on the queue finishes before this empty job does.
        self.queue.exec_sync(|| {});
        tracing::info!("screen capture stopped");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_frame_lasts_one_over_the_rate() {
        assert_eq!(frame_interval(30), (1, 30));
        assert_eq!(frame_interval(60), (1, 60));
    }

    #[test]
    fn the_rate_stays_in_a_sane_range() {
        assert_eq!(frame_interval(0), (1, 1));
        assert_eq!(frame_interval(1000), (1, 120));
    }
}
