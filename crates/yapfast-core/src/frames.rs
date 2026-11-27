//! Video frames from the stream worker to the UI, through a memory-mapped
//! file both processes open. Three slots make a triple buffer: the worker
//! fills its back slot and swaps it into the middle; the UI swaps the middle
//! out when it's newer than what it shows. Neither waits for the other, and
//! neither reads a slot the other is writing.
//!
//! The file lives in the runtime directory, which is memory on Linux, and is
//! removed once the worker has it open, so the memory returns when both
//! processes let go. Windows won't remove a mapped file, so there it's a
//! temporary file, kept in memory while it fits, that goes once the last
//! process closes it.

use std::{
    fs::{File, OpenOptions},
    path::{Path, PathBuf},
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicU32, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use anyhow::{Context, Result, ensure};
use memmap2::MmapMut;

const MAGIC: u32 = u32::from_le_bytes(*b"TKFR");
const HEADER: usize = 64;
const SLOTS: u32 = 3;
/// Set beside the middle slot's index when the worker put a frame there that
/// the UI hasn't taken.
const FRESH: u32 = 4;
/// How long a frame the UI hasn't taken means it isn't on screen: the UI
/// takes each frame within a draw or two while it shows them.
const UNSEEN_AFTER: Duration = Duration::from_millis(250);
/// How long the largest size the UI drew a source at stays its wish while it
/// draws it smaller too, as two tiles of one stream do: a second after the
/// large one goes, the frames come at the smaller one's size.
const WISH_HOLDS: Duration = Duration::from_secs(1);

/// The header at the start of the file; slots follow it.
#[repr(C)]
struct Header {
    magic: u32,
    capacity: u32,
    /// The middle slot's index, with [`FRESH`].
    middle: AtomicU32,
    /// The size the UI draws the frames at, in pixels, width in the high
    /// half and height in the low; zero until it has drawn one.
    wanted: AtomicU32,
    /// Each slot's frame size, as width and height in pixels.
    sizes: [[AtomicU32; 2]; SLOTS as usize],
    /// When each slot's frame shows in a video file, in milliseconds.
    times: [AtomicU64; SLOTS as usize],
}

const _: () = assert!(size_of::<Header>() <= HEADER);

/// The line a worker writes among its reports after each frame, while the
/// UI asked to be woken for frames ([`crate::picture::set_frame_wake`]):
/// the UI's side reads the slots without a wake of its own, so the reports'
/// pipe brings it.
#[cfg_attr(
    not(feature = "discord"),
    allow(dead_code, reason = "only Discord uses it")
)]
pub(crate) const FRAME_LINE: &str = "\"frame\"";

static TELL: OnceLock<fn()> = OnceLock::new();

/// Calls `tell` after each frame a [`Writer`] shows, from the thread that
/// wrote it.
#[cfg_attr(
    not(feature = "discord"),
    allow(dead_code, reason = "only Discord uses it")
)]
pub(crate) fn tell_each_frame(tell: fn()) {
    let _ = TELL.set(tell);
}

/// The largest frame `width` by `height` RGBA pixels fit in.
fn capacity(width: u32, height: u32) -> usize {
    width as usize * height as usize * 4
}

struct Mapping {
    map: MmapMut,
    capacity: usize,
}

impl Mapping {
    fn open(file: &File) -> Result<Self> {
        // SAFETY: only Yapfast's own processes open the file, and they touch the
        // slots through the triple buffer, never the same slot at once.
        let map = unsafe { MmapMut::map_mut(file) }.context("could not map the video frames")?;
        ensure!(map.len() >= HEADER, "the video frames file is too short");
        let mapping = Self { capacity: 0, map };
        ensure!(mapping.header().magic == MAGIC, "not a video frames file");
        let capacity = mapping.header().capacity as usize;
        ensure!(
            mapping.map.len() >= HEADER + SLOTS as usize * capacity,
            "the video frames file is too short"
        );
        Ok(Self {
            capacity,
            ..mapping
        })
    }

    fn header(&self) -> &Header {
        // SAFETY: the map is at least HEADER bytes, page aligned, and Header
        // is plain integers.
        unsafe { &*self.map.as_ptr().cast::<Header>() }
    }

    fn slot(&self, index: u32) -> *mut u8 {
        // SAFETY: Mapping::open checked the file holds every slot.
        unsafe {
            self.map
                .as_ptr()
                .cast_mut()
                .add(HEADER + index as usize * self.capacity)
        }
    }
}

/// The UI's side: it creates the file and shows the newest frame.
pub struct Frames {
    mapping: Mapping,
    /// The slot the UI reads, while it reads it.
    front: Mutex<u32>,
    name: Mutex<Option<Name>>,
    /// The size the UI asks frames at, and when a draw last needed it.
    wish: Mutex<Option<((u32, u32), Instant)>>,
}

/// What keeps the file's name until [`Frames::unlink`].
#[cfg(not(windows))]
type Name = tempfile::TempPath;

#[cfg(windows)]
struct Name {
    path: PathBuf,
    /// Opened to delete the file once every process closes it.
    _file: File,
}

#[cfg(windows)]
impl std::ops::Deref for Name {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.path
    }
}

#[cfg(not(windows))]
fn create_file(directory: PathBuf) -> std::io::Result<(File, Name)> {
    let file = tempfile::Builder::new()
        .prefix("yapfast-frames-")
        .tempfile_in(directory)?;
    Ok(file.into_parts())
}

#[cfg(windows)]
fn create_file(directory: PathBuf) -> std::io::Result<(File, Name)> {
    use std::{os::windows::fs::OpenOptionsExt, sync::atomic::AtomicUsize};

    const FILE_ATTRIBUTE_TEMPORARY: u32 = 0x100;
    const FILE_FLAG_DELETE_ON_CLOSE: u32 = 0x0400_0000;
    static CREATED: AtomicUsize = AtomicUsize::new(0);
    loop {
        let path = directory.join(format!(
            "yapfast-frames-{}-{}",
            std::process::id(),
            CREATED.fetch_add(1, Ordering::Relaxed)
        ));
        // The worker opens it with the standard library's sharing, which lets
        // it be deleted.
        let opened = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .attributes(FILE_ATTRIBUTE_TEMPORARY)
            .custom_flags(FILE_FLAG_DELETE_ON_CLOSE)
            .open(&path);
        match opened {
            Ok(file) => {
                let name = file.try_clone()?;
                return Ok((file, Name { path, _file: name }));
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }
}

impl std::fmt::Debug for Frames {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Frames")
            .field("capacity", &self.mapping.capacity)
            .finish_non_exhaustive()
    }
}

impl Frames {
    /// Creates a file for frames up to `width` by `height` pixels.
    pub fn create(width: u32, height: u32) -> Result<Self> {
        let directory = std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .filter(|directory| directory.is_dir())
            .unwrap_or_else(std::env::temp_dir);
        let (file, name) =
            create_file(directory).context("could not create the video frames file")?;
        let capacity = capacity(width, height);
        file.set_len((HEADER + SLOTS as usize * capacity) as u64)
            .context("could not size the video frames file")?;
        {
            // SAFETY: the file was just created and no one else has it yet.
            let mut map =
                unsafe { MmapMut::map_mut(&file) }.context("could not map the video frames")?;
            map[..4].copy_from_slice(&MAGIC.to_ne_bytes());
            map[4..8].copy_from_slice(&u32::try_from(capacity)?.to_ne_bytes());
            // The worker starts with slot 0 and the UI with slot 2.
            map[8..12].copy_from_slice(&1u32.to_ne_bytes());
        }
        Ok(Self {
            mapping: Mapping::open(&file)?,
            front: Mutex::new(2),
            name: Mutex::new(Some(name)),
            wish: Mutex::new(None),
        })
    }

    /// Where the worker opens the file, until [`Self::unlink`].
    pub fn path(&self) -> Option<PathBuf> {
        self.name
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .as_deref()
            .map(Path::to_owned)
    }

    /// Removes the file's name once the worker has it open. On Windows the
    /// name stays until both processes close the file.
    pub fn unlink(&self) {
        self.name
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take();
    }

    /// Says the UI draws the frames at `width` by `height` pixels, so the
    /// worker can make them no larger. While another draw of them asked for
    /// more in the last [`WISH_HOLDS`], the larger size stays.
    pub fn want(&self, width: u32, height: u32) {
        let (width, height) = (width.clamp(1, 0xffff), height.clamp(1, 0xffff));
        let now = Instant::now();
        let mut wish = self
            .wish
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let size = match *wish {
            Some(((held_width, held_height), at)) if now.duration_since(at) < WISH_HOLDS => {
                if width < held_width && height < held_height {
                    return;
                }
                (width.max(held_width), height.max(held_height))
            }
            _ => (width, height),
        };
        *wish = Some((size, now));
        self.mapping
            .header()
            .wanted
            .store(size.0 << 16 | size.1, Ordering::Relaxed);
    }

    /// Calls `show` with the newest frame's width, height and RGBA pixels,
    /// if one arrived since the last call.
    pub fn take<T>(&self, show: impl FnOnce(u32, u32, &[u8]) -> T) -> Option<T> {
        self.take_timed(show).map(|(shown, _)| shown)
    }

    /// As [`Self::take`], with when the frame shows in a video file.
    pub fn take_timed<T>(&self, show: impl FnOnce(u32, u32, &[u8]) -> T) -> Option<(T, u64)> {
        let header = self.mapping.header();
        if header.middle.load(Ordering::Acquire) & FRESH == 0 {
            return None;
        }
        let mut front = self
            .front
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *front = header.middle.swap(*front, Ordering::AcqRel) & !FRESH;
        let [width, height] = &header.sizes[*front as usize];
        let (width, height) = (
            width.load(Ordering::Relaxed),
            height.load(Ordering::Relaxed),
        );
        let length = capacity(width, height).min(self.mapping.capacity);
        // SAFETY: the worker no longer writes this slot: it's the UI's front
        // until the UI swaps it back.
        let pixels = unsafe { std::slice::from_raw_parts(self.mapping.slot(*front), length) };
        let time = header.times[*front as usize].load(Ordering::Relaxed);
        Some((show(width, height, pixels), time))
    }
}

/// The worker's side: it fills frames.
pub struct Writer {
    mapping: Mapping,
    back: u32,
    /// When it last showed a frame.
    written: Option<Instant>,
}

impl Writer {
    pub fn open(path: &Path) -> Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .context("could not open the video frames file")?;
        Ok(Self {
            mapping: Mapping::open(&file)?,
            back: 0,
            written: None,
        })
    }

    /// The largest frame, in pixels, that fits.
    pub fn fits(&self, width: u32, height: u32) -> bool {
        capacity(width, height) <= self.mapping.capacity
    }

    /// Whether the UI took the last frame shown, or none was shown yet.
    pub fn taken(&self) -> bool {
        self.mapping.header().middle.load(Ordering::Acquire) & FRESH == 0
    }

    /// Whether the UI has left the last frame untaken for [`UNSEEN_AFTER`]:
    /// it takes frames only while they're on screen, so nothing shows the
    /// ones after it, and they needn't be written until it takes that one.
    /// A UI slow to take one frame isn't this: it gets the newest.
    pub fn unseen(&self) -> bool {
        !self.taken()
            && self
                .written
                .is_some_and(|written| written.elapsed() >= UNSEEN_AFTER)
    }

    /// The size the UI draws the frames at, once it has drawn one.
    pub fn wanted(&self) -> Option<(u32, u32)> {
        let wanted = self.mapping.header().wanted.load(Ordering::Relaxed);
        (wanted != 0).then_some((wanted >> 16, wanted & 0xffff))
    }

    /// Writes a frame with `fill`, which gets its RGBA pixels, then shows it.
    pub fn write(&mut self, width: u32, height: u32, fill: impl FnOnce(&mut [u8])) {
        self.write_timed(width, height, 0, fill);
    }

    /// As [`Self::write`], for a frame of a video file showing at `time_ms`.
    pub fn write_timed(
        &mut self,
        width: u32,
        height: u32,
        time_ms: u64,
        fill: impl FnOnce(&mut [u8]),
    ) {
        assert!(self.fits(width, height), "a frame larger than its slot");
        // SAFETY: the back slot is the worker's alone until it's swapped in.
        let pixels = unsafe {
            std::slice::from_raw_parts_mut(self.mapping.slot(self.back), capacity(width, height))
        };
        fill(pixels);
        let header = self.mapping.header();
        let [w, h] = &header.sizes[self.back as usize];
        w.store(width, Ordering::Relaxed);
        h.store(height, Ordering::Relaxed);
        header.times[self.back as usize].store(time_ms, Ordering::Relaxed);
        self.back = header.middle.swap(self.back | FRESH, Ordering::AcqRel) & !FRESH;
        self.written = Some(Instant::now());
        if let Some(tell) = TELL.get() {
            tell();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_pass_from_writer_to_reader() {
        let frames = Frames::create(4, 2).unwrap();
        let path = frames.path().unwrap();
        let mut writer = Writer::open(&path).unwrap();
        frames.unlink();
        assert!(frames.path().is_none());
        if cfg!(not(windows)) {
            assert!(!path.exists());
        }
        assert!(frames.take(|_, _, _| ()).is_none(), "nothing yet");
        assert!(writer.fits(2, 4) && !writer.fits(5, 2));
        assert!(writer.taken(), "nothing shown yet to wait for");

        writer.write(2, 1, |pixels| pixels.fill(1));
        assert!(!writer.taken());
        writer.write(4, 2, |pixels| pixels.fill(2));
        let newest = frames.take(|width, height, pixels| (width, height, pixels.to_vec()));
        assert_eq!(newest, Some((4, 2, vec![2; 32])), "only the newest");
        assert!(writer.taken());
        assert!(frames.take(|_, _, _| ()).is_none(), "taken once");

        // Many frames while the UI holds one: it never sees a partial frame.
        for value in 3..10 {
            writer.write(1, 1, |pixels| pixels.fill(value));
        }
        assert_eq!(
            frames.take(|_, _, pixels| pixels.to_vec()),
            Some(vec![9; 4])
        );

        // A video's frames carry their time.
        writer.write_timed(1, 1, 1234, |pixels| pixels.fill(0));
        assert_eq!(frames.take_timed(|_, _, _| ()), Some(((), 1234)));

        drop((writer, frames));
        assert!(!path.exists(), "gone once both let go");
    }

    /// The UI's size for the frames: the largest a draw asked for lately,
    /// then a smaller one once nothing has asked for the larger in a while.
    #[test]
    fn the_writer_learns_the_size_the_ui_draws_at() {
        let frames = Frames::create(64, 64).unwrap();
        let writer = Writer::open(&frames.path().unwrap()).unwrap();
        assert_eq!(writer.wanted(), None, "nothing drawn yet");
        frames.want(40, 30);
        assert_eq!(writer.wanted(), Some((40, 30)));
        frames.want(20, 15);
        assert_eq!(writer.wanted(), Some((40, 30)), "another, smaller tile");
        frames.want(50, 10);
        assert_eq!(writer.wanted(), Some((50, 30)), "wider, not as tall");
        *frames.wish.lock().unwrap() = Some(((50, 30), Instant::now() - WISH_HOLDS));
        frames.want(20, 15);
        assert_eq!(writer.wanted(), Some((20, 15)), "the large one went");
    }
}
