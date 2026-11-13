//! `stream_worker`'s processor.

use super::*;

/// Runs what's due on GLib's main context. The portal's D-Bus replies arrive
/// there, and nothing else runs it in this process: without this the
/// dialog never opens.
pub(crate) fn run_glib() {
    #[cfg(target_os = "linux")]
    // SAFETY: the default context; only the capture thread iterates it.
    while unsafe { g_main_context_iteration(std::ptr::null_mut(), 0) } != 0 {}
}

#[derive(Default)]
pub(crate) struct Captured {
    pub(crate) size: Option<(u32, u32)>,
    /// Capture stopped for good, or a frame couldn't be sent.
    pub(crate) failed: Option<String>,
}

/// What a share from the card hands back when its loop is done: nothing more
/// to share, or a source the card's path can't take, which the processor's
/// does.
#[cfg(any(wgc, dup, target_os = "linux"))]
pub(crate) enum After {
    /// The share is over: it was stopped, or what it captured closed.
    Ended,
    /// Share `source` next. A window or a camera isn't a screen: the card's
    /// paths take a display, and the processor's capturer is what shares one,
    /// so the stream goes on there.
    Processor(Source),
}

/// A capturer sending a screen's, window's or camera's frames to the sink.
pub(crate) struct Capture {
    /// A screen's or window's, which gives a frame when asked.
    pub(crate) capturer: Option<livekit::webrtc::desktop_capturer::DesktopCapturer>,
    /// A camera's, which sends its frames from its own thread.
    pub(crate) _camera: Option<crate::camera::Camera>,
    pub(crate) captured: Arc<Mutex<Captured>>,
}

/// Whether the processor's capturer is asked for a window of its own: a window
/// is shared as that window — the same one the card's rungs refused, when they
/// did — and every other source as a screen. Where [`portal_picks`], the
/// system's own dialog picks either, so this is not what it is told.
pub(crate) fn capturer_takes_a_window(source: &Source) -> bool {
    matches!(source, Source::Window(_))
}

/// The kind of source the processor's capturer is asked for. Where
/// [`portal_picks`], the system's own screen sharing dialog picks a screen or a
/// window, and the kind only says what the dialog offers first — and the generic
/// kind is one only libwebrtc's Linux and macOS builds make.
pub(crate) fn capturer_kind(
    source: &Source,
) -> livekit::webrtc::desktop_capturer::DesktopCaptureSourceType {
    #[cfg(target_os = "linux")]
    if portal_picks() {
        return livekit::webrtc::desktop_capturer::DesktopCaptureSourceType::Generic;
    }
    if capturer_takes_a_window(source) {
        livekit::webrtc::desktop_capturer::DesktopCaptureSourceType::Window
    } else {
        livekit::webrtc::desktop_capturer::DesktopCaptureSourceType::Screen
    }
}

impl Capture {
    /// Starts capturing `source`, fitting its frames in `max_width` by
    /// `max_height`; where [`portal_picks`], a screen or window the system's
    /// dialog picks.
    pub(crate) fn start(
        source: Source,
        sink: &Arc<Mutex<Sink>>,
        max_width: u32,
        max_height: u32,
    ) -> Result<Self> {
        if let Source::Camera(id) = &source {
            return Self::camera(id, sink, max_width, max_height);
        }
        use livekit::webrtc::desktop_capturer::{
            CaptureError, DesktopCapturer, DesktopCapturerOptions,
        };
        let kind = capturer_kind(&source);
        let mut options = DesktopCapturerOptions::new(kind);
        options.set_include_cursor(true);
        let mut capturer = DesktopCapturer::new(options)
            .context("Screen sharing isn't available on this system.")?;
        let captured = Arc::new(Mutex::new(Captured::default()));
        let (into, seen) = (sink.clone(), captured.clone());
        // The frames this capture converts and scales, kept for the next.
        let mut scratch = Scratch::default();
        // A screen showing HDR hands its frames over as HDR's codes, so they
        // come through a tonemap on the way to I420; a screen showing SDR hands
        // over what it shows, which needs none. Asked once, here: what a screen
        // shows doesn't change under a share, and the frames say nothing of it
        // themselves.
        let tone = tone_of(&source);
        let source = if portal_picks() {
            None
        } else {
            let mut listed = capturer.get_source_list().into_iter();
            let found = match source {
                Source::Display(id) | Source::Window(id) => listed.find(|listed| listed.id() == id),
                _ => listed.next(),
            };
            Some(found.context("That screen or window is gone.")?)
        };
        capturer.start_capture(source, move |result| match result {
            Ok(frame) => {
                let (width, height) = (frame.width().max(1) as u32, frame.height().max(1) as u32);
                let size = fit(width, height, max_width, max_height);
                to_sink(
                    &into,
                    &seen,
                    &mut scratch,
                    frame.data(),
                    frame.stride(),
                    ((width, height), size),
                    &tone,
                );
            }
            // While the dialog is open, or between frames.
            Err(CaptureError::Temporary) => {}
            Err(CaptureError::Permanent) => {
                seen.lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .failed
                    .get_or_insert_with(|| "Screen sharing stopped.".into());
            }
        });
        Ok(Self {
            capturer: Some(capturer),
            _camera: None,
            captured,
        })
    }

    /// Films with the camera `id` into the sink, at the stream's size and
    /// frame rate at most. Each picture goes into a buffer kept for the next.
    pub(crate) fn camera(
        id: &str,
        sink: &Arc<Mutex<Sink>>,
        max_width: u32,
        max_height: u32,
    ) -> Result<Self> {
        let fps = sink
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .fps;
        let captured = Arc::new(Mutex::new(Captured::default()));
        let (into, seen) = (sink.clone(), captured.clone());
        let mut kept: Option<I420Buffer> = None;
        // The picture scaled for an odd size, kept for the frame after it.
        let mut shared: Option<I420Buffer> = None;
        let camera = crate::camera::Camera::open_at(
            Some(id),
            (max_width, max_height, fps),
            Box::new(move |frame| {
                let size = (frame.width, frame.height);
                let picture = buffer(&mut kept, size.0, size.1);
                copy_planes(frame, picture);
                // The camera fits the stream already; an odd size, which
                // codecs can't take, loses a row or a column. Scaling into
                // the buffer kept for it comes before the sink is locked.
                let fitted = fit(size.0, size.1, max_width, max_height);
                let taken = if fitted == size {
                    into.lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .frame(picture)
                } else {
                    let scaled = buffer(&mut shared, fitted.0, fitted.1);
                    scale_into(&planes(picture), scaled, fitted.0, fitted.1);
                    into.lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .frame(scaled)
                };
                let mut seen = seen.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                match taken {
                    Ok(()) => seen.size = Some(fitted),
                    Err(error) => seen.failed = Some(format!("{error:#}")),
                }
            }),
        )?;
        Ok(Self {
            capturer: None,
            _camera: Some(camera),
            captured,
        })
    }

    /// Asks a screen's or window's capturer for a frame; a camera sends its
    /// own.
    pub(crate) fn capture_frame(&mut self) {
        if let Some(capturer) = &mut self.capturer {
            capturer.capture_frame();
        }
    }

    pub(crate) fn seen(&self) -> std::sync::MutexGuard<'_, Captured> {
        self.captured
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// Copies a camera's picture into `picture`, of the same size.
pub(crate) fn copy_planes(frame: &crate::picture::I420<'_>, picture: &mut I420Buffer) {
    let (width, height) = (frame.width as usize, frame.height as usize);
    let (chroma_width, chroma_height) = (width.div_ceil(2), height.div_ceil(2));
    let (y_stride, u_stride, v_stride) = picture.strides();
    let into_strides = [y_stride, u_stride, v_stride];
    let (y, u, v) = picture.data_mut();
    for (plane, (from, into)) in [(frame.y, y), (frame.u, u), (frame.v, v)]
        .into_iter()
        .enumerate()
    {
        let (width, height) = if plane == 0 {
            (width, height)
        } else {
            (chroma_width, chroma_height)
        };
        let rows = from
            .chunks(frame.strides[plane] as usize)
            .zip(into.chunks_mut(into_strides[plane] as usize));
        for (from, into) in rows.take(height) {
            into[..width].copy_from_slice(&from[..width]);
        }
    }
}

/// The buffers a capture keeps from one frame to the next: the frame as it
/// came, converted to I420, and the picture it's scaled to for sharing. Each is
/// made again when a frame comes at another size, so a capture holds one
/// frame's worth of each rather than every size it has shown, and lets go of
/// both when sharing stops.
#[derive(Default)]
pub(crate) struct Scratch {
    pub(crate) full: Option<I420Buffer>,
    pub(crate) shared: Option<I420Buffer>,
}

/// The tone a capture of `source` comes through: none, because no capturer here
/// hands over HDR's codes. Windows Graphics Capture is asked for
/// `B8G8R8A8UIntNormalized` — `native/wgc.cc` on the card's rung, and
/// libwebrtc's capturer on the processor's, which asks for the same format — and
/// the system maps an HDR screen's composition into that 8-bit SDR picture
/// itself, as measured on this desktop (`docs/measurements/README.md`: "WGC
/// hands back BGRA8, already tonemapped, and encodes correctly"). Putting a
/// curve over those frames maps SDR twice, which is a share darker than the
/// screen it came from.
///
/// `Tone` is what a capturer that *does* hand HDR's codes over would need, and
/// Desktop Duplication is the one that can (`R16G16B16A16_FLOAT`, scRGB); it is
/// also the rung documented as broken on HDR frames, which is why a share on
/// Windows 11 uses Graphics Capture. Reading a tone would take the picture's
/// format from the capture rather than the display's setting, which is the
/// question `picture::tone_of_screen` asks and the wrong one to ask it of.
pub(crate) fn tone_of(source: &Source) -> Tone {
    match source {
        Source::Display(monitor) => picture::tone_of_screen(*monitor),
        Source::Window(window) => picture::tone_of_window(*window),
        _ => Tone::SDR,
    }
}

/// A captured frame on its way to the sink: converted and scaled in the
/// buffers this capture keeps, then handed over under the sink's lock. The
/// capture's status is noted afterwards, under its own lock: no lock is held
/// while a frame is made, and no frame is sent while the status is held. The
/// two sizes are the frame's own, and the one it's shared at.
pub(crate) fn to_sink(
    sink: &Mutex<Sink>,
    captured: &Mutex<Captured>,
    scratch: &mut Scratch,
    argb: &[u8],
    stride: u32,
    (size, shared): ((u32, u32), (u32, u32)),
    tone: &Tone,
) {
    let full = buffer(&mut scratch.full, size.0, size.1);
    convert(full, argb, stride, tone);
    let taken = if shared == size {
        sink.lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .frame(full)
    } else {
        let scaled = buffer(&mut scratch.shared, shared.0, shared.1);
        scale_into(&planes(full), scaled, shared.0, shared.1);
        sink.lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .frame(scaled)
    };
    let mut seen = captured
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    match taken {
        Ok(()) => seen.size = Some(shared),
        Err(error) => seen.failed = Some(format!("{error:#}")),
    }
}

/// The I420 buffer `kept` holds for pictures `width` by `height`, made in
/// place of one of another size.
pub(crate) fn buffer(kept: &mut Option<I420Buffer>, width: u32, height: u32) -> &mut I420Buffer {
    let picture = kept.get_or_insert_with(|| I420Buffer::new(width, height));
    if (picture.width(), picture.height()) != (width, height) {
        *picture = I420Buffer::new(width, height);
    }
    picture
}

/// `argb` pixels, a row every `stride` bytes, as I420 in `picture`, which is
/// `width` by `height`, through `tone` where the screen they came from shows
/// HDR and as they came where it doesn't.
pub(crate) fn convert(picture: &mut I420Buffer, argb: &[u8], stride: u32, tone: &Tone) {
    if let Some(table) = tone.table() {
        picture::toned_into(picture, argb, stride, table);
        return;
    }
    let (width, height) = (picture.width(), picture.height());
    let (y_stride, u_stride, v_stride) = picture.strides();
    let (y, u, v) = picture.data_mut();
    // libyuv's ARGB is B, G, R, A in memory, as the capturer's frames are.
    yuv_helper::argb_to_i420(
        argb,
        stride,
        y,
        y_stride,
        u,
        u_stride,
        v,
        v_stride,
        width as i32,
        height as i32,
    );
}

/// The same, for a picture whose red and blue bytes came in `order`: a
/// compositor draws a desktop in one of the two, and libyuv names the other
/// one ABGR.
///
/// The tone is SDR's: a desktop the compositor drew is the picture the viewer's
/// player would show, and a DMA-BUF carries no metadata to tone from.
#[cfg(target_os = "linux")]
pub(crate) fn convert_ordered(picture: &mut I420Buffer, rows: &[u8], stride: u32, order: Order) {
    let (width, height) = (picture.width(), picture.height());
    let (y_stride, u_stride, v_stride) = picture.strides();
    let (y, u, v) = picture.data_mut();
    let (width, height) = (width as i32, height as i32);
    match order {
        Order::Bgra => yuv_helper::argb_to_i420(
            rows, stride, y, y_stride, u, u_stride, v, v_stride, width, height,
        ),
        Order::Rgba => yuv_helper::abgr_to_i420(
            rows, stride, y, y_stride, u, u_stride, v, v_stride, width, height,
        ),
    }
}

/// `picture`'s planes, as the libyuv helpers take them.
pub(crate) fn planes(picture: &I420Buffer) -> I420<'_> {
    let (y, u, v) = picture.data();
    let (y_stride, u_stride, v_stride) = picture.strides();
    I420 {
        width: picture.width(),
        height: picture.height(),
        y,
        u,
        v,
        strides: [y_stride, u_stride, v_stride],
    }
}

/// `picture` scaled to `width` by `height` in `into`, which is that size: a
/// frame scaled down for sharing needs no picture of its own.
pub(crate) fn scale_into(picture: &I420<'_>, into: &mut I420Buffer, width: u32, height: u32) {
    let strides = into.strides();
    let (y, u, v) = into.data_mut();
    picture.scale_into((y, u, v), strides, width, height);
}

/// The I420 picture `to_rgba` scaled into `scratch`, which is `width` by
/// `height`'s planes.
pub(crate) fn previewed(scratch: &[u8], width: u32, height: u32) -> I420<'_> {
    let (w, h) = (width as usize, height as usize);
    let chroma = (w.div_ceil(2), h.div_ceil(2));
    let (y, planes) = scratch.split_at(w * h);
    let (u, v) = planes.split_at(chroma.0 * chroma.1);
    I420 {
        width,
        height,
        y,
        u,
        v: &v[..chroma.0 * chroma.1],
        strides: [width, chroma.0 as u32, chroma.0 as u32],
    }
}
