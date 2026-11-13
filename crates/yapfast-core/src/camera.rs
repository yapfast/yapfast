//! Cameras: listing them, and capturing one for a call or a Go Live stream.
//! Linux and Windows capture with libwebrtc's capture module
//! (`native/camera.cc`), through V4L2 and DirectShow, which converts each
//! frame into the same two I420 buffers.
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

use crate::{picture::I420, voice::Device};

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
        picture::{I420, RawPicture},
        voice::Device,
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
            return Ok(Self::pattern(sink));
        }
        Self::native(&device.id, size, sink)
    }

    /// Opens the camera `id`, or the first, as [`Self::open`] does, but on a
    /// blocking thread, once `replaced` has closed there. Resolves when the
    /// first picture has gone to `sink`, which can be a while after the
    /// camera opens. Dropped before that, it closes the camera.
    pub async fn start(replaced: Option<Self>, id: Option<String>, mut sink: Sink) -> Result<Self> {
        let (first, came) = tokio::sync::oneshot::channel();
        let mut first = Some(first);
        let sink: Sink = Box::new(move |picture| {
            sink(picture);
            if let Some(first) = first.take() {
                let _ = first.send(());
            }
        });
        let camera = tokio::task::spawn_blocking(move || {
            drop(replaced);
            Self::open(id.as_deref(), sink)
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
    fn pattern(mut sink: Sink) -> Self {
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
