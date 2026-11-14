//! Cameras: listing them, and capturing one for a call or a Go Live stream.
//! Linux and Windows capture with libwebrtc's capture module
//! (`native/camera.cc`), through V4L2 and DirectShow, which converts each
//! frame into the same two I420 buffers.
//!
//! A Linux device that films NV12 and exports the buffers it captures is filmed
//! by V4L2 itself instead (`capture::v4l2`), for a caller that asks for the
//! card's own copy of each picture: the same frame is handed over as the
//! descriptor an encoder imports where it lies and as the planes everything else
//! reads. A device that can't film that way is filmed by the module, in the
//! planes it always was.
//!
//! `YAPFAST_CAMERA_TEST_PATTERN=1` adds a camera that films moving colour bars,
//! for tests and screenshots on machines without a camera, and
//! `YAPFAST_CAMERA_TEST_DELAY_MS` makes it as slow to open as a real one.

use std::{
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

use anyhow::{Result, bail};

use crate::{device::Device, picture::I420};

/// The largest picture captured, and how often: 720p at 30 a second, as
/// Discord's client films without Nitro and LiveKit's default camera does.
/// Tiles in a call show less than this; the encoder sends this much.
pub const WIDTH: u32 = 1280;
pub const HEIGHT: u32 = 720;
pub const FPS: u32 = 30;

/// The test pattern's id and name, and its size.
const TEST_PATTERN: &str = "test-pattern";
const TEST_PATTERN_SIZE: (u32, u32) = (640, 360);

/// What a camera's pictures go to, on the camera's own thread. The planes
/// last until it returns.
pub type Sink = Box<dyn FnMut(&I420<'_>) + Send>;

/// One picture a camera filmed, for a caller whose encoder reads a picture where
/// the card holds it: the planes everything reads, and the card's copy of the
/// same picture where the device filmed it into memory it exported.
pub struct Picture<'a> {
    /// The picture's planes, which last until the sink returns.
    pub planes: &'a I420<'a>,
    /// The same picture in the buffer the driver captured it into: NV12 at the
    /// device's pitch, which is the shape a VA-API surface is made of without a
    /// copy. Nothing for a camera the module filmed, which has no such buffer.
    #[cfg(target_os = "linux")]
    pub card: Option<crate::dmabuf::Descriptor<'a>>,
}

impl<'a> Picture<'a> {
    /// A picture of `planes`, and no copy of it on the card: what a camera the
    /// module films hands over.
    pub fn of(planes: &'a I420<'a>) -> Self {
        Self {
            planes,
            #[cfg(target_os = "linux")]
            card: None,
        }
    }
}

/// What [`Picture`]s go to, on the camera's own thread: the same contract as
/// [`Sink`], for a caller that can take the card's own copy of a picture.
#[cfg(target_os = "linux")]
pub type CardSink = Box<dyn FnMut(&Picture<'_>) + Send>;

/// How a camera hands its pictures on: the planes every consumer reads, or those
/// planes and the card's copy of the picture beside them, which a caller asks for
/// when its encoder reads a picture where it lies.
pub enum Delivery {
    /// The planes alone, as every camera has handed them on.
    Planes(Sink),
    /// The planes, and the descriptor of the picture the card filmed.
    #[cfg(target_os = "linux")]
    Card(CardSink),
}

impl Delivery {
    /// This delivery, calling `delivered` after each picture it hands over:
    /// what [`Camera::start`] waits for, whichever way the camera films.
    fn delivered(self, mut delivered: impl FnMut() + Send + 'static) -> Self {
        match self {
            Self::Planes(mut sink) => Self::Planes(Box::new(move |planes| {
                sink(planes);
                delivered();
            })),
            #[cfg(target_os = "linux")]
            Self::Card(mut sink) => Self::Card(Box::new(move |picture| {
                sink(picture);
                delivered();
            })),
        }
    }

    /// The same pictures handed to `sink` as the planes alone: a camera the
    /// module filmed has no card's copy of them.
    fn planes(self) -> Sink {
        match self {
            Self::Planes(sink) => sink,
            #[cfg(target_os = "linux")]
            Self::Card(mut sink) => Box::new(move |planes: &I420<'_>| sink(&Picture::of(planes))),
        }
    }
}

/// The largest picture a camera gives, and how many a second: width,
/// height, frames.
pub type Size = (u32, u32, u32);

fn test_pattern() -> bool {
    std::env::var_os("YAPFAST_CAMERA_TEST_PATTERN").is_some_and(|value| value != "0")
}

/// A camera opening for a call, until its first picture: see
/// [`Camera::start`].
pub type Opening = Option<Pin<Box<dyn Future<Output = Result<Camera>> + Send>>>;

/// The camera opened, or never while none is opening.
pub async fn opened(opening: &mut Opening) -> Result<Camera> {
    match opening {
        Some(opening) => opening.await,
        None => std::future::pending().await,
    }
}

/// The cameras on this system, without opening any of them.
pub fn list() -> Vec<Device> {
    let mut devices = vec![];
    if test_pattern() {
        devices.push(Device {
            id: TEST_PATTERN.into(),
            name: "Test pattern".into(),
        });
    }
    #[cfg(any(target_os = "linux", windows))]
    native::list(&mut devices);
    devices
}

/// libwebrtc's capture module, through `native/camera.cc`.
#[cfg(any(target_os = "linux", windows))]
mod native {
    use std::ffi::{CStr, CString, c_char, c_int, c_void};

    use anyhow::{Context, Result, bail};

    use super::{Sink, Size};
    use crate::{
        device::Device,
        picture::{I420, RawPicture},
    };

    #[repr(C)]
    struct RawCamera {
        _private: [u8; 0],
    }

    type RawSink = extern "C" fn(user: *mut c_void, picture: *const RawPicture);

    type RawEach = extern "C" fn(user: *mut c_void, name: *const c_char, id: *const c_char);

    unsafe extern "C" {
        fn yapfast_camera_list(each: RawEach, user: *mut c_void);
        fn yapfast_camera_open(
            id: *const c_char,
            width: c_int,
            height: c_int,
            fps: c_int,
            frame: RawSink,
            user: *mut c_void,
        ) -> *mut RawCamera;
        fn yapfast_camera_close(camera: *mut RawCamera);
    }

    extern "C" fn listed(user: *mut c_void, name: *const c_char, id: *const c_char) {
        // SAFETY: `user` is the list being filled, and both strings are
        // terminated, for this call.
        let devices = unsafe { &mut *user.cast::<Vec<Device>>() };
        let text = |chars| {
            unsafe { CStr::from_ptr(chars) }
                .to_string_lossy()
                .into_owned()
        };
        devices.push(Device {
            id: text(id),
            name: text(name),
        });
    }

    /// Adds each camera, from one look at the devices: each look opens
    /// every video device the system has.
    pub(super) fn list(devices: &mut Vec<Device>) {
        // SAFETY: the list outlives the call, which calls back only during it.
        unsafe { yapfast_camera_list(listed, std::ptr::from_mut(devices).cast()) };
    }

    /// A camera filming through the module until dropped.
    pub(super) struct Camera {
        raw: *mut RawCamera,
        /// Where the module sends pictures; it must outlive the camera.
        _sink: Box<Sink>,
    }

    // SAFETY: the camera is only opened and closed through this handle, from
    // whichever thread holds it; pictures arrive on the module's own thread.
    unsafe impl Send for Camera {}

    extern "C" fn deliver(user: *mut c_void, picture: *const RawPicture) {
        // SAFETY: `user` is the camera's boxed sink, alive until the camera
        // closes, which waits for its thread; the planes last for this call.
        let sink = unsafe { &mut *user.cast::<Sink>() };
        if let Some(picture) = unsafe { picture.as_ref().and_then(|raw| I420::from_raw(raw)) } {
            sink(&picture);
        }
    }

    impl Camera {
        pub(super) fn open(id: &str, (width, height, fps): Size, sink: Sink) -> Result<Self> {
            let id = CString::new(id).context("not a camera id")?;
            let mut sink = Box::new(sink);
            let user = std::ptr::from_mut::<Sink>(sink.as_mut()).cast::<c_void>();
            // SAFETY: the id is terminated, and `user` stays boxed until the
            // camera closes.
            let raw = unsafe {
                yapfast_camera_open(
                    id.as_ptr(),
                    width as c_int,
                    height as c_int,
                    fps as c_int,
                    deliver,
                    user,
                )
            };
            if raw.is_null() {
                bail!("Could not start your camera. It may be in use by another app.");
            }
            Ok(Self { raw, _sink: sink })
        }
    }

    impl Drop for Camera {
        fn drop(&mut self) {
            // SAFETY: opened by yapfast_camera_open and closed once; no picture
            // arrives after this returns, so the sink can go.
            unsafe { yapfast_camera_close(self.raw) };
        }
    }
}

/// A camera filming until dropped.
#[derive(Default)]
pub struct Camera {
    #[cfg(target_os = "linux")]
    _native: Option<native::Camera>,
    /// The thread that films, told to stop through the flag: the test
    /// pattern's, or on Windows the one holding the native camera.
    thread: Option<(Arc<AtomicBool>, JoinHandle<()>)>,
}

impl Camera {
    /// Starts filming with the camera `id`, or the first one, sending its
    /// pictures to `sink`, at a call's size: [`WIDTH`] by [`HEIGHT`] at
    /// [`FPS`].
    pub fn open(id: Option<&str>, sink: Sink) -> Result<Self> {
        Self::open_at(id, (WIDTH, HEIGHT, FPS), sink)
    }

    /// As [`Self::open`], with pictures at most `size`: a camera filming
    /// larger is scaled down to fit. The test pattern keeps its own size.
    pub fn open_at(id: Option<&str>, size: Size, sink: Sink) -> Result<Self> {
        Self::open_filming(id, size, Delivery::Planes(sink))
    }

    fn open_filming(id: Option<&str>, size: Size, delivery: Delivery) -> Result<Self> {
        let devices = list();
        let device = match id {
            Some(id) => devices.iter().find(|device| device.id == id),
            None => devices.first(),
        };
        let Some(device) = device.or_else(|| devices.first()) else {
            bail!("No camera found. Connect one and try again.");
        };
        if device.id == TEST_PATTERN {
            let delay = std::env::var("YAPFAST_CAMERA_TEST_DELAY_MS").ok();
            if let Some(delay) = delay.and_then(|delay| delay.parse().ok()) {
                std::thread::sleep(Duration::from_millis(delay));
            }
            return Ok(Self::pattern(delivery));
        }
        #[cfg(target_os = "linux")]
        if let Delivery::Card(sink) = delivery {
            // A device that films a picture the card holds is filmed by V4L2
            // itself, and one that can't is the module's, which hands the same
            // sink those pictures' planes.
            return match card::open(&device.id, size) {
                Some(capture) => card::film(capture, sink),
                None => Self::native(&device.id, size, Delivery::Card(sink).planes()),
            };
        }
        Self::native(&device.id, size, delivery.planes())
    }

    /// Opens the camera `id`, or the first, as [`Self::open`] does, but on a
    /// blocking thread, once `replaced` has closed there. Resolves when the
    /// first picture has gone to `sink`, which can be a while after the
    /// camera opens. Dropped before that, it closes the camera.
    pub async fn start(replaced: Option<Self>, id: Option<String>, sink: Sink) -> Result<Self> {
        Self::start_filming(replaced, id, Delivery::Planes(sink)).await
    }

    /// As [`Self::start`], for a caller that can take the card's own copy of each
    /// picture: the camera films it where its device exports what it captures,
    /// and every other camera films as [`Self::start`] films one.
    pub async fn start_filming(
        replaced: Option<Self>,
        id: Option<String>,
        delivery: Delivery,
    ) -> Result<Self> {
        let (first, came) = tokio::sync::oneshot::channel();
        let mut first = Some(first);
        let delivery = delivery.delivered(move || {
            if let Some(first) = first.take() {
                let _ = first.send(());
            }
        });
        let camera = tokio::task::spawn_blocking(move || {
            drop(replaced);
            Self::open_filming(id.as_deref(), (WIDTH, HEIGHT, FPS), delivery)
        })
        .await??;
        // The sink lasts as long as the camera, so this ends with a picture.
        let _ = came.await;
        Ok(camera)
    }

    #[cfg(target_os = "linux")]
    fn native(id: &str, size: Size, sink: Sink) -> Result<Self> {
        Ok(Self {
            _native: Some(native::Camera::open(id, size, sink)?),
            thread: None,
        })
    }

    /// DirectShow's objects live in a COM apartment that the module joins
    /// on the thread opening the camera, and leaves on the one closing it.
    /// A thread of the camera's own does both, so no other thread's COM
    /// apartment is joined or left by mistake.
    #[cfg(windows)]
    fn native(id: &str, size: Size, sink: Sink) -> Result<Self> {
        let id = id.to_owned();
        let (opened, result) = std::sync::mpsc::sync_channel(1);
        let camera = Self::spawn(move |stop| {
            let camera = match native::Camera::open(&id, size, sink) {
                Ok(camera) => camera,
                Err(error) => {
                    let _ = opened.send(Err(error));
                    return;
                }
            };
            let _ = opened.send(Ok(()));
            while !stop.load(Ordering::Acquire) {
                std::thread::park();
            }
            drop(camera);
        })?;
        match result.recv() {
            Ok(Ok(())) => Ok(camera),
            Ok(Err(error)) => Err(error),
            Err(_) => bail!("Could not start your camera."),
        }
    }

    #[cfg(not(any(target_os = "linux", windows)))]
    fn native(_: &str, _: Size, _: Sink) -> Result<Self> {
        bail!("Cameras work on Linux and Windows for now.")
    }

    /// Runs `film` on the camera's thread until it returns; it's told to
    /// stop through the flag, and woken if parked.
    fn spawn(film: impl FnOnce(&AtomicBool) + Send + 'static) -> std::io::Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let thread = std::thread::Builder::new()
            .name("yapfast-camera".into())
            .spawn(move || film(&stopped))?;
        Ok(Self {
            #[cfg(target_os = "linux")]
            _native: None,
            thread: Some((stop, thread)),
        })
    }

    /// Films moving colour bars at 30 frames a second.
    fn pattern(delivery: Delivery) -> Self {
        let mut sink = delivery.planes();
        let film = move |stopped: &AtomicBool| {
            let (width, height) = TEST_PATTERN_SIZE;
            let chroma = (width / 2 * height / 2) as usize;
            let (mut y, u, v) = (
                vec![0u8; (width * height) as usize],
                vec![128u8; chroma],
                vec![128u8; chroma],
            );
            let started = Instant::now();
            let mut frame = 0u32;
            while !stopped.load(Ordering::Relaxed) {
                let due = started + Duration::from_secs(u64::from(frame)) / FPS;
                if let Some(wait) = due.checked_duration_since(Instant::now()) {
                    std::thread::sleep(wait);
                }
                // Grey bars sliding across, with a bright block bouncing
                // over them, so a still picture shows as one.
                for (row, line) in y.chunks_exact_mut(width as usize).enumerate() {
                    for (column, luma) in line.iter_mut().enumerate() {
                        *luma = 40 + (((column as u32 + frame * 4) / 80) % 6 * 30) as u8;
                        let block = (frame * 6) % (width - 80);
                        if (column as u32).wrapping_sub(block) < 80 && (row as u32) % 120 < 80 {
                            *luma = 235;
                        }
                    }
                }
                let picture = I420 {
                    width,
                    height,
                    y: &y,
                    u: &u,
                    v: &v,
                    strides: [width, width / 2, width / 2],
                };
                sink(&picture);
                frame += 1;
            }
        };
        Self::spawn(film).unwrap_or_default()
    }
}

impl Drop for Camera {
    fn drop(&mut self) {
        if let Some((stop, thread)) = self.thread.take() {
            stop.store(true, Ordering::Release);
            thread.thread().unpark();
            let _ = thread.join();
        }
    }
}

/// A camera filmed by V4L2's own capture, in the memory the driver films into
/// (`capture::v4l2`), for a device that films NV12 as one buffer and exports
/// what it captures. The picture reaches an encoder that reads a DMA-BUF where
/// it lies, and the same picture is converted once into the planes everything
/// else reads.
#[cfg(target_os = "linux")]
mod card {
    use std::{ffi::c_int, sync::atomic::Ordering, time::Duration};

    use anyhow::Result;

    use super::{Camera, CardSink, HEIGHT, Picture, Size, WIDTH};
    use crate::{capture::v4l2, picture::I420};

    /// How long a frame is waited for before the thread looks for a stop: a
    /// device wakes the wait with a frame, and nothing wakes it with a camera
    /// that has been dropped.
    const WAIT: Duration = Duration::from_millis(100);

    unsafe extern "C" {
        /// `native/camera.cc`: the NV12 picture in `data`, `length` bytes of it
        /// at `stride` to a row, as the I420 planes `width` by `height`.
        fn yapfast_camera_nv12_to_i420(
            data: *const u8,
            length: usize,
            width: c_int,
            height: c_int,
            stride: c_int,
            y: *mut u8,
            u: *mut u8,
            v: *mut u8,
        ) -> c_int;
    }

    /// The camera at `path`, opened for V4L2's capture where its driver films the
    /// size a call shares as NV12 in buffers it exports, and nothing for a device
    /// that can't film that way: the module films that one.
    ///
    /// Only the size a call shares is filmed here. The module scales a device
    /// that films larger than the call asked down to fit, and this path describes
    /// the driver's own buffer rather than making one, so another size is the
    /// module's.
    ///
    /// Which way it went, and why, is one line at info, the level a run keeps: a
    /// camera filmed on the processor is the reason a share costs more than it
    /// should, so a device the driver can't film this way says so where people
    /// read it. Nothing per frame is logged here.
    pub(super) fn open(path: &str, size: Size) -> Option<Filming> {
        if (size.0, size.1) != (WIDTH, HEIGHT) {
            tracing::info!(
                asked = ?size,
                "filming the camera on the processor: the card's capture films only the size a \
                 call shares"
            );
            return None;
        }
        let capture = match v4l2::Capture::open(path) {
            Ok(capture) => capture,
            Err(why) => {
                tracing::info!(
                    why = format!("{why:#}"),
                    "filming the camera on the processor"
                );
                return None;
            }
        };
        if capture.size() != (WIDTH, HEIGHT) {
            tracing::info!(
                films = ?capture.size(),
                "filming the camera on the processor: it films another size than the call asks"
            );
            return None;
        }
        tracing::info!("filming the camera on the card: {}", capture.describe());
        Some(Filming(capture))
    }

    /// Films `capture` on a thread of its own until the camera is dropped,
    /// handing `sink` each picture with the descriptor the driver's buffer names.
    pub(super) fn film(capture: Filming, sink: CardSink) -> Result<Camera> {
        let (width, height) = capture.size();
        let mut filmed = Filmed { capture, sink };
        Ok(Camera::spawn(move |stop| {
            // The planes kept for the pictures after this one, which everything
            // but an encoder taking the descriptor reads.
            let mut planes = Planes::of(width, height);
            while !stop.load(Ordering::Acquire) {
                match filmed.capture.frame(WAIT) {
                    Ok(v4l2::Arrival::Frame) => {}
                    Ok(v4l2::Arrival::Still) => continue,
                    Ok(v4l2::Arrival::Closed) => {
                        tracing::info!(why = filmed.capture.ending(), "the camera stopped filming");
                        break;
                    }
                    Err(why) => {
                        tracing::warn!(%why, "the camera stopped filming");
                        break;
                    }
                }
                let Some(picture) = filmed.capture.picture() else {
                    continue;
                };
                // Both of these are the picture just arrived, and the buffer it
                // is in is given back when the next frame is asked for: an
                // encoder has read them both before this returns.
                let card = picture.descriptor();
                let stride = picture.stride();
                let Some(rows) = filmed.capture.rows() else {
                    continue;
                };
                let Some(planes) = planes.hold(rows, stride) else {
                    // A frame shorter than the picture the driver films cannot be
                    // the picture it says, and nothing after it can be read.
                    tracing::warn!("the camera's buffer is shorter than the picture it films");
                    break;
                };
                (filmed.sink)(&Picture {
                    planes: &planes,
                    card: Some(card),
                });
            }
        })?)
    }

    /// A capture and the sink its pictures go to, as one value whose drop order
    /// is the capture's to state: freeing the driver's buffers makes every
    /// descriptor exported from them a stale handle, so an encoder holding one
    /// goes first. Fields drop in the order they are declared, which is the sink
    /// first here.
    struct Filmed {
        /// The sink, and so whatever encoder imported the descriptors: dropped
        /// before the buffers they name are freed.
        sink: CardSink,
        /// The capture, whose drop stops the stream, unmaps its buffers and gives
        /// them back to the driver: last, once nothing reads them.
        capture: Filming,
    }

    /// The capture on the thread that films with it: a capture's buffers are a
    /// mapping of this process's memory and the node's own descriptor, which any
    /// thread may hold, and it is used by one thread at a time.
    pub(super) struct Filming(v4l2::Capture);

    // SAFETY: the above, which nothing in a capture contradicts: the descriptors
    // and the mapping belong to the process rather than the thread that opened
    // it, and the capture is only touched by the thread it was given to.
    unsafe impl Send for Filming {}

    impl std::ops::Deref for Filming {
        type Target = v4l2::Capture;

        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }

    impl std::ops::DerefMut for Filming {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.0
        }
    }

    /// The I420 planes kept for the pictures after this one.
    struct Planes {
        width: u32,
        height: u32,
        data: Vec<u8>,
    }

    impl Planes {
        /// One I420 picture of `width` by `height`, allocated once.
        fn of(width: u32, height: u32) -> Self {
            let chroma = (width.div_ceil(2) * height.div_ceil(2)) as usize;
            Self {
                width,
                height,
                data: vec![0; width as usize * height as usize + 2 * chroma],
            }
        }

        /// Converts the NV12 picture `rows` maps, `stride` bytes to a row, into
        /// these planes, and answers it as the picture to read; `None` when the
        /// mapping holds less than the picture.
        fn hold(&mut self, rows: &[u8], stride: u32) -> Option<I420<'_>> {
            let (width, height) = (self.width, self.height);
            let luma = width as usize * height as usize;
            let chroma = (width.div_ceil(2) * height.div_ceil(2)) as usize;
            let (y, rest) = self.data.split_at_mut(luma);
            let (u, v) = rest.split_at_mut(chroma);
            // SAFETY: `rows` is the driver's own mapping, which the capture read
            // the picture's size and pitch from; the planes hold a picture of
            // that size, at `width` bytes a row of luma and half that of each
            // chroma plane; and the call reads and writes only those.
            let converted = unsafe {
                yapfast_camera_nv12_to_i420(
                    rows.as_ptr(),
                    rows.len(),
                    width as c_int,
                    height as c_int,
                    stride as c_int,
                    y.as_mut_ptr(),
                    u.as_mut_ptr(),
                    v.as_mut_ptr(),
                )
            };
            (converted == 0).then_some(I420 {
                width,
                height,
                y,
                u,
                v,
                strides: [width, width / 2, width / 2],
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_test_pattern_films_until_dropped() {
        // SAFETY: tests that read this variable all set it the same way.
        unsafe { std::env::set_var("YAPFAST_CAMERA_TEST_PATTERN", "1") };
        assert!(list().iter().any(|device| device.id == TEST_PATTERN));
        let (frames, received) = std::sync::mpsc::channel();
        let camera = Camera::open(
            Some(TEST_PATTERN),
            Box::new(move |picture| {
                let _ = frames.send((picture.width, picture.height, picture.y[0]));
            }),
        )
        .unwrap();
        let first = received.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!((first.0, first.1), TEST_PATTERN_SIZE);
        drop(camera);
        while received.try_recv().is_ok() {}
        std::thread::sleep(Duration::from_millis(100));
        assert!(received.try_recv().is_err(), "stopped");
    }

    #[tokio::test]
    async fn starting_ends_with_the_first_picture() {
        // SAFETY: tests that read this variable all set it the same way.
        unsafe { std::env::set_var("YAPFAST_CAMERA_TEST_PATTERN", "1") };
        let pictures = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counted = pictures.clone();
        let sink: Sink = Box::new(move |_| {
            counted.fetch_add(1, Ordering::Relaxed);
        });
        let camera = Camera::start(None, Some(TEST_PATTERN.into()), sink)
            .await
            .unwrap();
        assert!(pictures.load(Ordering::Relaxed) >= 1);
        // Another replaces it, closing it first.
        let (frames, received) = std::sync::mpsc::channel();
        let sink: Sink = Box::new(move |picture| {
            let _ = frames.send(picture.width);
        });
        let _camera = Camera::start(Some(camera), Some(TEST_PATTERN.into()), sink)
            .await
            .unwrap();
        let before = pictures.load(Ordering::Relaxed);
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(pictures.load(Ordering::Relaxed), before, "the first closed");
        assert_eq!(received.try_recv(), Ok(TEST_PATTERN_SIZE.0));
    }

    /// The test pattern films planes, so a caller that asked for the card's own
    /// copy of each picture is handed the planes alone.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_card_delivery_is_handed_the_planes_where_there_is_no_card() {
        let (seen, received) = std::sync::mpsc::channel();
        let sink: CardSink = Box::new(move |picture| {
            let _ = seen.send((picture.planes.width, picture.card.is_some()));
        });
        let mut sink = Delivery::Card(sink).planes();
        let (width, height) = TEST_PATTERN_SIZE;
        let chroma = (width / 2 * height / 2) as usize;
        let (y, u, v) = (
            vec![16u8; (width * height) as usize],
            vec![128u8; chroma],
            vec![128u8; chroma],
        );
        sink(&I420 {
            width,
            height,
            y: &y,
            u: &u,
            v: &v,
            strides: [width, width / 2, width / 2],
        });
        assert_eq!(received.try_recv(), Ok((width, false)));
    }

    /// Films the first real camera for a second, or the first whose name
    /// holds `YAPFAST_TEST_CAMERA`: run with a camera, or a V4L2 loopback
    /// device something writes to.
    #[test]
    #[ignore = "needs a camera"]
    fn films_a_real_camera() {
        let devices: Vec<Device> = list()
            .into_iter()
            .filter(|device| device.id != TEST_PATTERN)
            .collect();
        println!("cameras: {devices:?}");
        let named = std::env::var("YAPFAST_TEST_CAMERA").unwrap_or_default();
        let device = devices.iter().find(|device| device.name.contains(&named));
        let (frames, received) = std::sync::mpsc::channel();
        let camera = Camera::open(
            device.map(|device| device.id.as_str()),
            Box::new(move |picture| {
                let _ = frames.send((picture.width, picture.height));
            }),
        )
        .unwrap();
        let started = Instant::now();
        let mut count = 0;
        while started.elapsed() < Duration::from_secs(1) {
            if let Ok(size) = received.recv_timeout(Duration::from_secs(2)) {
                count += 1;
                assert!(size.0 <= WIDTH && size.1 <= HEIGHT, "{size:?}");
            }
        }
        drop(camera);
        println!("{count} frames in a second");
        assert!(count >= 10);
    }
}
