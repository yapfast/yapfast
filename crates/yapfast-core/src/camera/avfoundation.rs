//! macOS cameras through AVFoundation. There is no macOS backend in
//! libwebrtc's capture module, so this one lists the devices with a discovery
//! session and films with an `AVCaptureSession` whose video output calls a
//! delegate on a serial dispatch queue.
//!
//! The output is asked for NV12 (`420v`, two planes: luma, then interleaved
//! chroma), which every Mac camera can give without a copy of its own. The
//! sink wants I420, so each frame's luma goes on as it is and only the chroma
//! plane is split into two, which costs a quarter of the frame's bytes.

use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{Mutex, mpsc},
    time::Duration,
};

use anyhow::{Context, Result, bail};
use dispatch2::{DispatchQueue, DispatchRetained};
use objc2::{
    AnyThread, DefinedClass, define_class, msg_send,
    rc::Retained,
    runtime::{AnyObject, ProtocolObject},
};
use objc2_av_foundation::{
    AVAuthorizationStatus, AVCaptureConnection, AVCaptureDevice, AVCaptureDeviceDiscoverySession,
    AVCaptureDeviceFormat, AVCaptureDeviceInput, AVCaptureDevicePosition, AVCaptureOutput,
    AVCaptureSession, AVCaptureSessionPresetInputPriority, AVCaptureVideoDataOutput,
    AVCaptureVideoDataOutputSampleBufferDelegate, AVMediaTypeVideo,
};
use objc2_core_foundation::CFString;
use objc2_core_media::{CMSampleBuffer, CMTime, CMVideoFormatDescriptionGetDimensions};
use objc2_core_video::{
    CVPixelBufferGetBaseAddressOfPlane, CVPixelBufferGetBytesPerRowOfPlane,
    CVPixelBufferGetHeightOfPlane, CVPixelBufferGetPixelFormatType, CVPixelBufferGetPlaneCount,
    CVPixelBufferGetWidthOfPlane, CVPixelBufferLockBaseAddress, CVPixelBufferLockFlags,
    CVPixelBufferUnlockBaseAddress, kCVPixelBufferPixelFormatTypeKey,
    kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange, kCVReturnSuccess,
};
use objc2_foundation::{NSArray, NSDictionary, NSNumber, NSObject, NSObjectProtocol, NSString};

use super::{Sink, Size};
use crate::{picture::I420, voice::Device};

/// How long to wait for the person to answer the system's camera prompt.
const ACCESS_PROMPT_TIMEOUT: Duration = Duration::from_secs(120);

/// A frame rate is set as a fraction of a second: `1000 / (fps * 1000)`.
const FRAME_DURATION_TIMESCALE: f64 = 1000.0;

/// The label of the queue frames arrive on.
const QUEUE_LABEL: &str = "yapfast.camera.frames";

/// The planes of an NV12 picture.
const LUMA_PLANE: usize = 0;
const CHROMA_PLANE: usize = 1;
const NV12_PLANES: usize = 2;

const DENIED_MESSAGE: &str = "Camera access is turned off for Yapfast. Allow it in System Settings, Privacy & Security, Camera, then try again.";
const RESTRICTED_MESSAGE: &str = "Camera access is restricted on this Mac, for example by a profile or Screen Time.";

// ---- Pure logic: what to ask of a camera, and how to lay a picture out ----

/// What a camera's format offers, as far as choosing one goes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct FormatInfo {
    pub width: u32,
    pub height: u32,
    /// The slowest and fastest frame rate any of its ranges allows.
    pub min_fps: f64,
    pub max_fps: f64,
}

impl FormatInfo {
    fn area(&self) -> u64 {
        u64::from(self.width) * u64::from(self.height)
    }

    fn fits(&self, (width, height, _): Size) -> bool {
        self.width <= width && self.height <= height
    }

    fn gives(&self, (_, _, fps): Size) -> bool {
        self.min_fps <= f64::from(fps) && f64::from(fps) <= self.max_fps
    }
}

/// The index of the format closest to `wanted`: one that fits inside it and
/// gives its frame rate, the largest of those; else the largest that fits;
/// else, when every format is larger, the smallest one.
pub(super) fn choose_format(formats: &[FormatInfo], wanted: Size) -> Option<usize> {
    let rank = |format: &FormatInfo| {
        let closeness = if format.fits(wanted) {
            format.area()
        } else {
            u64::MAX - format.area()
        };
        (format.fits(wanted), format.gives(wanted), closeness)
    };
    let mut best: Option<(usize, (bool, bool, u64))> = None;
    for (index, format) in formats.iter().enumerate() {
        let key = rank(format);
        if best.is_none_or(|(_, held)| key > held) {
            best = Some((index, key));
        }
    }
    best.map(|(index, _)| index)
}

/// The frame rate to film at: the one wanted, kept inside what `format` allows.
pub(super) fn frame_rate_for(format: &FormatInfo, wanted: Size) -> f64 {
    f64::from(wanted.2).clamp(format.min_fps, format.max_fps)
}

/// What the person's camera permission allows, from the system's status.
#[derive(Debug, PartialEq)]
pub(super) enum Access {
    Allowed,
    /// Not asked yet: ask now.
    Ask,
    Refused(&'static str),
}

pub(super) fn access_for(status: AVAuthorizationStatus) -> Access {
    match status {
        AVAuthorizationStatus::Authorized => Access::Allowed,
        AVAuthorizationStatus::NotDetermined => Access::Ask,
        AVAuthorizationStatus::Restricted => Access::Refused(RESTRICTED_MESSAGE),
        _ => Access::Refused(DENIED_MESSAGE),
    }
}

/// Splits NV12's interleaved chroma rows (`U V U V ...`, `src_stride` bytes
/// apart) into two planes, `width` bytes a row each, over `rows` rows.
pub(super) fn split_chroma(
    src: &[u8],
    src_stride: usize,
    width: usize,
    rows: usize,
    u: &mut Vec<u8>,
    v: &mut Vec<u8>,
) {
    u.clear();
    v.clear();
    u.reserve(width * rows);
    v.reserve(width * rows);
    for row in src.chunks(src_stride).take(rows) {
        for pair in row[..(width * 2).min(row.len())].chunks_exact(2) {
            u.push(pair[0]);
            v.push(pair[1]);
        }
    }
}

// ---- Authorization ----

/// Fails unless the camera may be used, asking the person once when they
/// have not been asked.
fn ensure_authorized() -> Result<()> {
    let media = unsafe { AVMediaTypeVideo }.context("AVFoundation has no video media type")?;
    // SAFETY: a plain class method with a valid media type.
    let status = unsafe { AVCaptureDevice::authorizationStatusForMediaType(media) };
    match access_for(status) {
        Access::Allowed => Ok(()),
        Access::Refused(message) => {
            tracing::warn!(?status, "camera access refused");
            bail!(message)
        }
        Access::Ask => {
            tracing::info!("asking for camera access");
            request_access(media)
        }
    }
}

fn request_access(media: &objc2_av_foundation::AVMediaType) -> Result<()> {
    let (answer, answered) = mpsc::sync_channel::<bool>(1);
    let handler = block2::RcBlock::new(move |granted: objc2::runtime::Bool| {
        let _ = answer.try_send(granted.as_bool());
    });
    // SAFETY: the handler is copied by the call and may run on any queue.
    unsafe { AVCaptureDevice::requestAccessForMediaType_completionHandler(media, &handler) };
    match answered.recv_timeout(ACCESS_PROMPT_TIMEOUT) {
        Ok(true) => {
            tracing::info!("camera access granted");
            Ok(())
        }
        Ok(false) => bail!(DENIED_MESSAGE),
        Err(_) => bail!("No answer to the camera prompt. Try again and choose Allow."),
    }
}

// ---- Listing ----

/// The camera types listed: the built in one, external ones (USB, virtual)
/// and Continuity Camera.
fn device_types() -> Retained<NSArray<objc2_av_foundation::AVCaptureDeviceType>> {
    // SAFETY: these are constants of AVFoundation.
    let types = unsafe {
        [
            objc2_av_foundation::AVCaptureDeviceTypeBuiltInWideAngleCamera,
            objc2_av_foundation::AVCaptureDeviceTypeExternal,
            objc2_av_foundation::AVCaptureDeviceTypeContinuityCamera,
        ]
    };
    NSArray::from_slice(&types)
}

fn video_devices() -> Vec<Retained<AVCaptureDevice>> {
    let Some(media) = (unsafe { AVMediaTypeVideo }) else {
        tracing::error!("AVFoundation has no video media type");
        return vec![];
    };
    // SAFETY: valid constants and a plain class method.
    let session = unsafe {
        AVCaptureDeviceDiscoverySession::discoverySessionWithDeviceTypes_mediaType_position(
            &device_types(),
            Some(media),
            AVCaptureDevicePosition::Unspecified,
        )
    };
    // SAFETY: reads the session's list.
    unsafe { session.devices() }.to_vec()
}

/// Adds each camera to `devices`.
pub(super) fn list(devices: &mut Vec<Device>) {
    for device in video_devices() {
        // SAFETY: plain property reads.
        let (id, name) = unsafe { (device.uniqueID(), device.localizedName()) };
        tracing::debug!(%id, %name, "camera found");
        devices.push(Device {
            id: id.to_string(),
            name: name.to_string(),
        });
    }
}

fn find_device(id: &str) -> Result<Retained<AVCaptureDevice>> {
    video_devices()
        .into_iter()
        // SAFETY: a plain property read.
        .find(|device| unsafe { device.uniqueID() }.to_string() == id)
        .with_context(|| format!("The camera {id} is not connected any more."))
}

// ---- Choosing a format ----

fn format_info(format: &AVCaptureDeviceFormat) -> FormatInfo {
    // SAFETY: plain property reads on a format of a live device.
    let (dimensions, ranges) = unsafe {
        (
            CMVideoFormatDescriptionGetDimensions(&format.formatDescription()),
            format.videoSupportedFrameRateRanges(),
        )
    };
    let mut info = FormatInfo {
        width: dimensions.width.max(0) as u32,
        height: dimensions.height.max(0) as u32,
        min_fps: f64::MAX,
        max_fps: 0.0,
    };
    for range in ranges.iter() {
        // SAFETY: plain property reads.
        let (min, max) = unsafe { (range.minFrameRate(), range.maxFrameRate()) };
        info.min_fps = info.min_fps.min(min);
        info.max_fps = info.max_fps.max(max);
    }
    info
}

/// Makes the device film at the format closest to `wanted`, at its frame rate.
fn configure_device(device: &AVCaptureDevice, wanted: Size) -> Result<()> {
    // SAFETY: plain property read.
    let formats = unsafe { device.formats() };
    let formats = formats.to_vec();
    let infos: Vec<FormatInfo> = formats.iter().map(|format| format_info(format)).collect();
    let Some(index) = choose_format(&infos, wanted) else {
        bail!("The camera offers no video format.");
    };
    let (format, info) = (&formats[index], infos[index]);
    let fps = frame_rate_for(&info, wanted);
    tracing::info!(
        width = info.width,
        height = info.height,
        fps,
        "camera format chosen"
    );
    // SAFETY: the configuration lock is held around the changes and released
    // on every path.
    unsafe {
        device
            .lockForConfiguration()
            .map_err(|error| anyhow::anyhow!("Could not configure the camera: {error}"))?;
        device.setActiveFormat(format);
        let duration = CMTime::new(
            FRAME_DURATION_TIMESCALE as i64,
            (fps * FRAME_DURATION_TIMESCALE).round() as i32,
        );
        device.setActiveVideoMinFrameDuration(duration);
        device.setActiveVideoMaxFrameDuration(duration);
        device.unlockForConfiguration();
    }
    Ok(())
}

// ---- The frame delegate ----

/// Where the frames of one camera go, on the delegate's queue.
struct FrameState {
    sink: Sink,
    u: Vec<u8>,
    v: Vec<u8>,
    /// Set once a frame in another pixel format has been logged.
    warned: bool,
}

struct DelegateIvars {
    state: Mutex<FrameState>,
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements, and the class has no
    // Drop of its own beyond its ivars.
    #[unsafe(super(NSObject))]
    #[ivars = DelegateIvars]
    struct FrameDelegate;

    unsafe impl NSObjectProtocol for FrameDelegate {}

    unsafe impl AVCaptureVideoDataOutputSampleBufferDelegate for FrameDelegate {
        #[unsafe(method(captureOutput:didOutputSampleBuffer:fromConnection:))]
        fn capture_output_did_output_sample_buffer_from_connection(
            &self,
            _output: &AVCaptureOutput,
            sample_buffer: &CMSampleBuffer,
            _connection: &AVCaptureConnection,
        ) {
            // A panic must not unwind into AVFoundation.
            let result = catch_unwind(AssertUnwindSafe(|| self.deliver(sample_buffer)));
            if result.is_err() {
                tracing::error!("the camera frame handler panicked; the frame is dropped");
            }
        }
    }
);

impl FrameDelegate {
    fn new(sink: Sink) -> Retained<Self> {
        let this = Self::alloc().set_ivars(DelegateIvars {
            state: Mutex::new(FrameState {
                sink,
                u: Vec::new(),
                v: Vec::new(),
                warned: false,
            }),
        });
        // SAFETY: NSObject's init.
        unsafe { msg_send![super(this), init] }
    }

    /// Sends one frame to the sink.
    fn deliver(&self, sample: &CMSampleBuffer) {
        // SAFETY: a plain getter; the buffer stays retained for this call.
        let Some(image) = (unsafe { sample.image_buffer() }) else {
            tracing::debug!("camera sample without a picture");
            return;
        };
        let Ok(mut state) = self.ivars().state.lock() else {
            tracing::error!("the camera state is poisoned");
            return;
        };
        let state = &mut *state;
        if CVPixelBufferGetPixelFormatType(&image) != kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange
            || CVPixelBufferGetPlaneCount(&image) != NV12_PLANES
        {
            if !state.warned {
                state.warned = true;
                tracing::warn!("camera frames are not NV12; they are dropped");
            }
            return;
        }
        // SAFETY: locked until the guard drops, read only.
        let status =
            unsafe { CVPixelBufferLockBaseAddress(&image, CVPixelBufferLockFlags::ReadOnly) };
        if status != kCVReturnSuccess {
            tracing::warn!(status, "could not lock a camera frame");
            return;
        }
        let planes = read_planes(&image);
        if let Some((width, height, luma, luma_stride, chroma, chroma_stride)) = planes {
            let chroma_width = width.div_ceil(2);
            let chroma_height = height.div_ceil(2);
            split_chroma(
                chroma,
                chroma_stride,
                chroma_width,
                chroma_height,
                &mut state.u,
                &mut state.v,
            );
            let picture = I420 {
                width: width as u32,
                height: height as u32,
                y: luma,
                u: &state.u,
                v: &state.v,
                strides: [luma_stride as u32, chroma_width as u32, chroma_width as u32],
            };
            (state.sink)(&picture);
        }
        // SAFETY: locked above, and the slices are not used past this point.
        let status =
            unsafe { CVPixelBufferUnlockBaseAddress(&image, CVPixelBufferLockFlags::ReadOnly) };
        if status != kCVReturnSuccess {
            tracing::warn!(status, "could not unlock a camera frame");
        }
    }
}

/// An NV12 frame's size, then its luma plane and stride, then its chroma plane
/// and stride. The buffer must be locked, and the slices end before it unlocks.
type Planes<'a> = (usize, usize, &'a [u8], usize, &'a [u8], usize);

fn read_planes(image: &objc2_core_video::CVPixelBuffer) -> Option<Planes<'_>> {
    let width = CVPixelBufferGetWidthOfPlane(image, LUMA_PLANE);
    let height = CVPixelBufferGetHeightOfPlane(image, LUMA_PLANE);
    let luma = CVPixelBufferGetBaseAddressOfPlane(image, LUMA_PLANE).cast::<u8>();
    let chroma = CVPixelBufferGetBaseAddressOfPlane(image, CHROMA_PLANE).cast::<u8>();
    let luma_stride = CVPixelBufferGetBytesPerRowOfPlane(image, LUMA_PLANE);
    let chroma_stride = CVPixelBufferGetBytesPerRowOfPlane(image, CHROMA_PLANE);
    let chroma_rows = CVPixelBufferGetHeightOfPlane(image, CHROMA_PLANE);
    if luma.is_null() || chroma.is_null() || width == 0 || height == 0 {
        tracing::warn!("camera frame without planes");
        return None;
    }
    // SAFETY: the planes are as long as their rows times their stride while
    // the buffer is locked, which the caller holds for the slices' life.
    unsafe {
        Some((
            width,
            height,
            std::slice::from_raw_parts(luma, luma_stride * height),
            luma_stride,
            std::slice::from_raw_parts(chroma, chroma_stride * chroma_rows),
            chroma_stride,
        ))
    }
}

// ---- The camera ----

/// A camera filming through AVFoundation until dropped.
pub(super) struct Camera {
    session: Retained<AVCaptureSession>,
    output: Retained<AVCaptureVideoDataOutput>,
    queue: DispatchRetained<DispatchQueue>,
    /// Alive as long as the output calls it, which holds it weakly.
    _delegate: Retained<FrameDelegate>,
}

// SAFETY: AVCaptureSession starts and stops from any thread, and the camera
// is only touched through this handle by whoever holds it; frames arrive on
// the camera's own queue, into the sink, which is Send.
unsafe impl Send for Camera {}

impl Camera {
    pub(super) fn open(id: &str, wanted: Size, sink: Sink) -> Result<Self> {
        ensure_authorized()?;
        let device = find_device(id)?;
        let delegate = FrameDelegate::new(sink);
        let queue = DispatchQueue::new(QUEUE_LABEL, None);
        let (session, output) = build_session(&device, &delegate, &queue)?;
        configure_device(&device, wanted)?;
        // SAFETY: starts the session, which blocks until it runs.
        unsafe { session.startRunning() };
        // SAFETY: a plain property read.
        if !unsafe { session.isRunning() } {
            stop(&session, &output, &queue);
            bail!("Could not start your camera. It may be in use by another app.");
        }
        tracing::info!(%id, "camera running");
        Ok(Self {
            session,
            output,
            queue,
            _delegate: delegate,
        })
    }
}

impl Drop for Camera {
    fn drop(&mut self) {
        stop(&self.session, &self.output, &self.queue);
        tracing::info!("camera stopped");
    }
}

/// Stops the session and waits until no frame is still being delivered.
fn stop(session: &AVCaptureSession, output: &AVCaptureVideoDataOutput, queue: &DispatchQueue) {
    // SAFETY: plain calls on live objects.
    unsafe {
        session.stopRunning();
        output.setSampleBufferDelegate_queue(None, None);
    }
    // A frame already on the queue finishes before this empty job does.
    queue.exec_sync(|| {});
}

/// A session with the device as input and a video output to the delegate.
fn build_session(
    device: &AVCaptureDevice,
    delegate: &FrameDelegate,
    queue: &DispatchQueue,
) -> Result<(Retained<AVCaptureSession>, Retained<AVCaptureVideoDataOutput>)> {
    // SAFETY: plain object creation and configuration; the configuration block
    // is committed on every path before the session is returned or dropped.
    unsafe {
        let input = AVCaptureDeviceInput::deviceInputWithDevice_error(device)
            .map_err(|error| anyhow::anyhow!("Could not open the camera: {error}"))?;
        let output = AVCaptureVideoDataOutput::new();
        output.setVideoSettings(Some(&nv12_settings()));
        output.setAlwaysDiscardsLateVideoFrames(true);
        let delegate = ProtocolObject::from_ref(delegate);
        output.setSampleBufferDelegate_queue(Some(delegate), Some(queue));
        let session = AVCaptureSession::new();
        // Keeps the format chosen for the device, which a preset would replace.
        session.setSessionPreset(AVCaptureSessionPresetInputPriority);
        session.beginConfiguration();
        let result = if !session.canAddInput(&input) {
            Err(anyhow::anyhow!("The camera cannot be used by this session."))
        } else if !session.canAddOutput(&output) {
            Err(anyhow::anyhow!("The session cannot take the camera's video."))
        } else {
            session.addInput(&input);
            session.addOutput(&output);
            Ok(())
        };
        session.commitConfiguration();
        result?;
        Ok((session, output))
    }
}

/// The output's video settings: the pixel format NV12.
fn nv12_settings() -> Retained<NSDictionary<NSString, AnyObject>> {
    // SAFETY: CFString and NSString are the same object, bridged for free.
    let key: &NSString = unsafe {
        &*std::ptr::from_ref::<CFString>(kCVPixelBufferPixelFormatTypeKey).cast::<NSString>()
    };
    let format = NSNumber::new_u32(kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange);
    NSDictionary::from_slices(&[key], &[format.as_ref()])
}

#[cfg(test)]
mod tests {
    use super::*;

    const WANTED: Size = (1280, 720, 30);

    fn format(width: u32, height: u32, min_fps: f64, max_fps: f64) -> FormatInfo {
        FormatInfo {
            width,
            height,
            min_fps,
            max_fps,
        }
    }

    #[test]
    fn the_largest_format_that_fits_and_gives_the_rate_wins() {
        let formats = [
            format(640, 480, 1.0, 30.0),
            format(1920, 1080, 1.0, 30.0),
            format(1280, 720, 1.0, 30.0),
            format(1280, 720, 1.0, 15.0),
        ];
        assert_eq!(choose_format(&formats, WANTED), Some(2));
    }

    #[test]
    fn a_format_that_gives_the_rate_beats_a_larger_one_that_does_not() {
        let formats = [format(1280, 720, 1.0, 10.0), format(640, 360, 1.0, 30.0)];
        assert_eq!(choose_format(&formats, WANTED), Some(1));
    }

    #[test]
    fn when_every_format_is_larger_the_smallest_is_chosen() {
        let formats = [format(3840, 2160, 1.0, 30.0), format(1920, 1080, 1.0, 30.0)];
        assert_eq!(choose_format(&formats, WANTED), Some(1));
    }

    #[test]
    fn no_formats_choose_nothing() {
        assert_eq!(choose_format(&[], WANTED), None);
    }

    #[test]
    fn the_frame_rate_stays_inside_the_range() {
        assert_eq!(frame_rate_for(&format(1, 1, 1.0, 15.0), WANTED), 15.0);
        assert_eq!(frame_rate_for(&format(1, 1, 60.0, 120.0), WANTED), 60.0);
        assert_eq!(frame_rate_for(&format(1, 1, 1.0, 60.0), WANTED), 30.0);
    }

    #[test]
    fn permission_states_map_to_access() {
        assert_eq!(
            access_for(AVAuthorizationStatus::Authorized),
            Access::Allowed
        );
        assert_eq!(access_for(AVAuthorizationStatus::NotDetermined), Access::Ask);
        assert_eq!(
            access_for(AVAuthorizationStatus::Denied),
            Access::Refused(DENIED_MESSAGE)
        );
        assert_eq!(
            access_for(AVAuthorizationStatus::Restricted),
            Access::Refused(RESTRICTED_MESSAGE)
        );
    }

    #[test]
    fn chroma_is_split_ignoring_row_padding() {
        // Two rows of two pairs, each row padded to six bytes.
        let src = [1, 2, 3, 4, 9, 9, 5, 6, 7, 8, 9, 9];
        let (mut u, mut v) = (vec![], vec![]);
        split_chroma(&src, 6, 2, 2, &mut u, &mut v);
        assert_eq!(u, [1, 3, 5, 7]);
        assert_eq!(v, [2, 4, 6, 8]);
    }

    #[test]
    fn splitting_again_replaces_the_planes() {
        let (mut u, mut v) = (vec![0; 10], vec![0; 10]);
        split_chroma(&[1, 2], 2, 1, 1, &mut u, &mut v);
        assert_eq!((u, v), (vec![1], vec![2]));
    }
}
