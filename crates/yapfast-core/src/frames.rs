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
//!
//! The file is made for the size its frames show at, so a stream shown in a
//! small tile costs a small file. One shown larger than the file was made for
//! grows it: the worker lengthens the file and maps it again, and the header
//! says where the slots lie now, which is how the UI's mapping learns it must
//! be made again too. A file never shrinks, so a mapping one process still
//! holds stays whole under it.
//!
//! A slot holds either pixels or a picture the worker drew on the GPU and
//! shared with the UI's process, which the UI shows without copying it: the
//! header says which, slot by slot, so a reader never takes one for the
//! other. A stream that doesn't draw on the GPU is a stream of pixels as it
//! always was, and one whose pictures the UI can't open says so in the header
//! and goes on with pixels from its next frame: a picture that can't be shown
//! is a slow stream, not a blank one.

use std::{
    fs::{File, OpenOptions},
    path::{Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicU32, AtomicU64, Ordering},
    },
};

use anyhow::{Context, Result, ensure};
use memmap2::MmapMut;

const MAGIC: u32 = u32::from_le_bytes(*b"TKFR");
/// Where the slots start. It's well past what the header says, so a field
/// added later lands in the room this one leaves rather than moving every
/// slot in a file the other process has already mapped.
const HEADER: usize = 256;
const SLOTS: u32 = 3;
/// Set beside the middle slot's index when the worker put a frame there that
/// the UI hasn't taken.
const FRESH: u32 = 4;

/// What the UI says about a stream's GPU pictures: nothing yet, that it can
/// show them on the adapter in [`Header::ui_luid`], or that it can't, which
/// the worker reads as an answer to write pixels instead.
const UI_UNSET: u32 = 0;
const UI_USABLE: u32 = 1;
const UI_REFUSED: u32 = 2;

/// The header at the start of the file; slots follow it.
#[repr(C)]
struct Header {
    magic: u32,
    /// The bytes a slot holds, which is where each slot starts: it changes
    /// only by growing, and the process that grows it says so here before it
    /// writes a frame at the new size.
    capacity: AtomicU32,
    /// The middle slot's index, with [`FRESH`].
    middle: AtomicU32,
    _reserved: u32,
    /// Each slot's frame size, as width and height in pixels.
    sizes: [[AtomicU32; 2]; SLOTS as usize],
    /// When each slot's frame shows in a video file, in milliseconds.
    times: [AtomicU64; SLOTS as usize],
    /// The adapter the worker draws its pictures on and the fence they are
    /// drawn behind, as the worker shared them: a stream has one device and
    /// one fence, so these belong to the file rather than to a slot. Zero
    /// while the stream publishes pixels.
    luid: AtomicU64,
    fence: AtomicU64,
    /// What each slot holds when it isn't pixels.
    pictures: [Shared; SLOTS as usize],
    /// The UI's answer on this stream's pictures; see [`UI_UNSET`].
    ui: AtomicU32,
    /// The UI's process, which the worker duplicates its shared handles into:
    /// a handle's value means nothing outside the process that took it.
    ui_pid: AtomicU32,
    /// The adapter the UI draws on, when its answer is [`UI_USABLE`].
    ui_luid: AtomicU64,
}

/// One slot's picture, when the worker drew it on the GPU instead of writing
/// its pixels.
#[repr(C)]
struct Shared {
    /// The shared handle of the texture holding the picture, as it is in the
    /// UI's process, or zero for a slot of pixels: this is what tells a
    /// reader which of the two a slot holds.
    texture: AtomicU64,
    /// The value the fence reaches once the picture is drawn, which a reader
    /// waits for before showing what the texture holds.
    value: AtomicU64,
    /// The number of the picture, which only grows: a reader compares it with
    /// the one it showed to tell a new picture from the one before it.
    generation: AtomicU64,
}

const _: () = assert!(size_of::<Header>() <= HEADER);

/// A slot's picture: the pixels the worker wrote into the file, or one it
/// drew on the GPU and shared, which the UI shows without copying.
pub enum Frame<'a> {
    /// The worker's pixels, `width` by `height` RGBA bytes, in a slot the UI
    /// holds until its next take.
    Pixels {
        width: u32,
        height: u32,
        pixels: &'a [u8],
    },
    /// A picture in a texture the worker shares, and how to know it's drawn.
    Gpu(Picture),
}

/// A picture the worker drew on the GPU, as its slot's header describes it.
/// Everything here is a handle or a number the UI can check without opening
/// anything, which is what makes a picture it can't show cheap to refuse.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Picture {
    /// The adapter the worker draws on, as its DXGI adapter's LUID, low half
    /// first: Direct3D opens a shared texture on the adapter that made it and
    /// on no other, so this decides whether the picture can be shown before
    /// anything is opened.
    pub luid: u64,
    /// The shared handle of the fence the picture is drawn behind, as it is
    /// in the UI's process.
    pub fence: u64,
    /// The shared handle of the texture holding the picture, as it is in the
    /// UI's process: the worker duplicated it there.
    pub texture: u64,
    /// The value the fence reaches when the picture is drawn.
    pub fence_value: u64,
    /// The number of the picture, which only grows.
    pub generation: u64,
    /// The picture's size, in pixels.
    pub width: u32,
    pub height: u32,
}

/// What a worker hands [`Writer::publish`]: the picture it drew, without the
/// number, which publishing gives it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Drawn {
    /// The adapter it drew on, as the LUID of its DXGI adapter, low half
    /// first; the UI opens the texture only on that adapter.
    pub luid: u64,
    /// The shared handle of the fence it signals once the picture is drawn,
    /// as it is in the UI's process.
    pub fence: u64,
    /// The shared handle of the texture holding the picture, as it is in the
    /// UI's process.
    pub texture: u64,
    /// The value that fence reaches when the picture is drawn: it must only
    /// grow, and the worker must signal it after the drawing, before it
    /// publishes.
    pub fence_value: u64,
    /// The picture's size, which the texture must have: Direct3D 12 trusts
    /// it, and a texture smaller than this is read past its end.
    pub width: u32,
    pub height: u32,
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
        let capacity = mapping.capacity_now();
        ensure!(
            mapping.map.len() >= HEADER + SLOTS as usize * capacity,
            "the video frames file is too short"
        );
        Ok(Self {
            capacity,
            ..mapping
        })
    }

    /// The bytes a slot holds, as the header says: frames larger than those
    /// that wrote it moved every slot, and this mapping with them.
    fn capacity_now(&self) -> usize {
        self.header().capacity.load(Ordering::Acquire) as usize
    }

    /// Grows the file and this mapping to hold slots of `capacity` bytes,
    /// when they hold smaller ones, and says so in the header. Only growing
    /// keeps a mapping another process holds whole: the file it read stays
    /// where it was, and the header is what tells it to map again.
    fn grow(&mut self, file: &File, capacity: usize) -> Result<()> {
        if capacity <= self.capacity {
            return Ok(());
        }
        let length = HEADER + SLOTS as usize * capacity;
        file.set_len(length as u64)
            .context("could not size the video frames file")?;
        // SAFETY: only Yapfast's own processes map the file, and they touch
        // the slots through the triple buffer, never the same slot at once.
        let map = unsafe { MmapMut::map_mut(file) }.context("could not map the video frames")?;
        ensure!(map.len() >= length, "the video frames file is too short");
        self.map = map;
        self.capacity = capacity;
        self.header()
            .capacity
            .store(u32::try_from(capacity)?, Ordering::Release);
        Ok(())
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
    /// The mapping the UI reads through and the slot it reads: the two change
    /// together, so a frame is never read through a mapping replaced under it.
    front: Mutex<Front>,
    /// The file, kept to map it again at the size the worker grows it to.
    file: File,
    name: Mutex<Option<Name>>,
}

/// What the UI reads through: the mapping, and the slot it holds.
struct Front {
    mapping: Mapping,
    slot: u32,
}

impl Front {
    /// Maps the file again when the worker has grown it: a frame larger than
    /// these slots holds moved every slot, so a mapping left behind reads
    /// another frame's pixels.
    fn follow(&mut self, file: &File) -> Result<()> {
        let capacity = self.mapping.capacity_now();
        self.mapping.grow(file, capacity)
    }
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
        // A frame being read holds the mapping, and a log line doesn't wait
        // for it: what it holds is printed when it can be read.
        let capacity = self
            .front
            .try_lock()
            .ok()
            .map(|front| front.mapping.capacity);
        formatter
            .debug_struct("Frames")
            .field("capacity", &capacity)
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
        let mapping = Mapping::open(&file)?;
        // The worker duplicates its shared handles into this process, and its
        // value means nothing anywhere else.
        mapping
            .header()
            .ui_pid
            .store(std::process::id(), Ordering::Release);
        Ok(Self {
            front: Mutex::new(Front { mapping, slot: 2 }),
            file,
            name: Mutex::new(Some(name)),
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

    /// Calls `show` with the newest frame, if one arrived since the last
    /// call: its pixels, or the picture the worker drew on the GPU.
    pub fn take_frame<T>(&self, show: impl FnOnce(Frame<'_>) -> T) -> Option<T> {
        self.take_frame_timed(show).map(|(shown, _)| shown)
    }

    /// As [`Self::take_frame`], with when the frame shows in a video file.
    pub fn take_frame_timed<T>(&self, show: impl FnOnce(Frame<'_>) -> T) -> Option<(T, u64)> {
        let mut front = self
            .front
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Err(error) = front.follow(&self.file) {
            // The frames are lost with the mapping, but a stream's sound and
            // the call go on: whoever grows the file asks again next time.
            tracing::warn!(
                error = format!("{error:#}"),
                "could not map the video frames"
            );
            return None;
        }
        let held = front.mapping.capacity;
        if front.mapping.header().middle.load(Ordering::Acquire) & FRESH == 0 {
            return None;
        }
        front.slot = front
            .mapping
            .header()
            .middle
            .swap(front.slot, Ordering::AcqRel)
            & !FRESH;
        // The worker may have grown the file since: the frame just taken was
        // written at a slot size this mapping doesn't know, so it's let go for
        // the frame after it, which is read where it was written.
        if front.mapping.capacity_now() != held {
            let _ = front.follow(&self.file);
            return None;
        }
        let header = front.mapping.header();
        let [width, height] = &header.sizes[front.slot as usize];
        let (width, height) = (
            width.load(Ordering::Relaxed),
            height.load(Ordering::Relaxed),
        );
        let shared = &header.pictures[front.slot as usize];
        let texture = shared.texture.load(Ordering::Relaxed);
        // A handle is what says a slot holds a picture instead of pixels, and
        // a slot the worker fills with pixels has none: the two can't be taken
        // for each other.
        let frame = if texture == 0 {
            let length = capacity(width, height).min(held);
            // SAFETY: the worker no longer writes this slot: it's the UI's
            // front until the UI swaps it back.
            let pixels =
                unsafe { std::slice::from_raw_parts(front.mapping.slot(front.slot), length) };
            Frame::Pixels {
                width,
                height,
                pixels,
            }
        } else {
            Frame::Gpu(Picture {
                luid: header.luid.load(Ordering::Relaxed),
                fence: header.fence.load(Ordering::Relaxed),
                texture,
                fence_value: shared.value.load(Ordering::Relaxed),
                generation: shared.generation.load(Ordering::Relaxed),
                width,
                height,
            })
        };
        let time = header.times[front.slot as usize].load(Ordering::Relaxed);
        Some((show(frame), time))
    }

    /// Calls `show` with the newest frame's width, height and RGBA pixels, if
    /// one arrived since the last call. A picture the worker drew on the GPU
    /// has no pixels in the file, so it's passed over here: a reader that can
    /// show one asks for the frame instead.
    pub fn take<T>(&self, show: impl FnOnce(u32, u32, &[u8]) -> T) -> Option<T> {
        self.take_frame(|frame| match frame {
            Frame::Pixels {
                width,
                height,
                pixels,
            } => Some(show(width, height, pixels)),
            Frame::Gpu(_) => None,
        })
        .flatten()
    }

    /// As [`Self::take`], with when the frame shows in a video file.
    pub fn take_timed<T>(&self, show: impl FnOnce(u32, u32, &[u8]) -> T) -> Option<(T, u64)> {
        self.take_frame_timed(|frame| match frame {
            Frame::Pixels {
                width,
                height,
                pixels,
            } => Some(show(width, height, pixels)),
            Frame::Gpu(_) => None,
        })
        .and_then(|(shown, time)| shown.map(|shown| (shown, time)))
    }

    /// Says the UI can show this stream's pictures from the GPU, on `luid`,
    /// the adapter its window draws on: the worker publishes them from its
    /// next frame on. Says whether that changed what the header said.
    pub fn accept_gpu(&self, luid: u64) -> bool {
        let front = self
            .front
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let header = front.mapping.header();
        // The adapter before the answer, so a worker that reads the answer
        // reads the adapter it goes with.
        let moved = header.ui_luid.swap(luid, Ordering::Relaxed) != luid;
        let said = header.ui.swap(UI_USABLE, Ordering::AcqRel) != UI_USABLE;
        said || moved
    }

    /// Says it can't, once: whether a stream's pictures can be shown is a
    /// property of the pair of adapters and the renderer, which don't change
    /// while the stream lasts, so there's nothing to retry. Says whether this
    /// was the first refusal, so the reason is logged once.
    pub fn refuse_gpu(&self) -> bool {
        let front = self
            .front
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        front.mapping.header().ui.swap(UI_REFUSED, Ordering::AcqRel) != UI_REFUSED
    }
}

/// The worker's side: it fills frames.
pub struct Writer {
    /// The file the slots are mapped from, kept to lengthen it when a frame
    /// needs more room than it has.
    file: File,
    mapping: Mapping,
    back: u32,
    /// How many pictures this writer has published, which numbers them.
    published: u64,
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
            file,
            back: 0,
            published: 0,
        })
    }
    /// The UI's process, which the worker duplicates its shared handles into
    /// before [`Self::publish`] takes them: a handle's value means nothing
    /// outside the process that took it, so the ones the UI opens are the
    /// ones duplicated there.
    pub fn ui_process(&self) -> u32 {
        self.mapping.header().ui_pid.load(Ordering::Acquire)
    }

    /// The largest frame, in pixels, that fits.
    pub fn fits(&self, width: u32, height: u32) -> bool {
        capacity(width, height) <= self.mapping.capacity
    }

    /// Makes room for frames of `width` by `height`, growing the file and the
    /// slots in it when they hold smaller ones: a stream shown larger than
    /// the file was made for grows it once, instead of being scaled down to
    /// what it holds.
    pub fn reserve(&mut self, width: u32, height: u32) -> Result<()> {
        self.mapping.grow(&self.file, capacity(width, height))
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
        self.write_timed_rendered(width, height, time_ms, |pixels| {
            fill(pixels);
            true
        });
    }
    /// Writes a frame with `fill`, which gets its RGBA pixels and says whether
    /// it filled them, and shows it only if it did: a picture that couldn't be
    /// made leaves the frame before it showing, rather than a slot half of one
    /// frame and half of another.
    pub fn write_rendered(
        &mut self,
        width: u32,
        height: u32,
        fill: impl FnOnce(&mut [u8]) -> bool,
    ) -> bool {
        self.write_timed_rendered(width, height, 0, fill)
    }
    /// As [`Self::write_rendered`], for a frame showing at `time_ms`.
    pub fn write_timed_rendered(
        &mut self,
        width: u32,
        height: u32,
        time_ms: u64,
        fill: impl FnOnce(&mut [u8]) -> bool,
    ) -> bool {
        // A frame larger than the slots grows them, as one shown in a larger
        // tile or window is; one that can't grow them is skipped, leaving the
        // frame before it showing rather than writing over the slots.
        if let Err(error) = self.reserve(width, height) {
            tracing::warn!(
                error = format!("{error:#}"),
                "could not grow the video frames"
            );
            return false;
        }
        // SAFETY: the back slot is the worker's alone until it's swapped in.
        let pixels = unsafe {
            std::slice::from_raw_parts_mut(self.mapping.slot(self.back), capacity(width, height))
        };
        if !fill(pixels) {
            return false;
        }
        let header = self.mapping.header();
        // A slot that held a picture the worker drew on the GPU holds pixels
        // now, and its handle goes first: no reader takes this frame to be
        // the texture of the one before it.
        header.pictures[self.back as usize]
            .texture
            .store(0, Ordering::Relaxed);
        let [w, h] = &header.sizes[self.back as usize];
        w.store(width, Ordering::Relaxed);
        h.store(height, Ordering::Relaxed);
        header.times[self.back as usize].store(time_ms, Ordering::Relaxed);
        self.back = header.middle.swap(self.back | FRESH, Ordering::AcqRel) & !FRESH;
        true
    }

    /// Publishes a picture the worker drew on the GPU, to show without a
    /// pixel of it crossing the process boundary, and says whether it did:
    /// `false` means the UI can't show such pictures — it looked and said so,
    /// or it draws on another adapter — and the caller writes the frame's
    /// pixels instead.
    ///
    /// The worker must, before calling this:
    /// * draw into a `DXGI_FORMAT_R8G8B8A8_UNORM` texture of the picture's
    ///   size, one mip, one sample, made with
    ///   `D3D11_RESOURCE_MISC_SHARED_NTHANDLE | D3D11_RESOURCE_MISC_SHARED_KEYEDMUTEX`
    ///   (the second is what Direct3D 11 requires of a shared NT handle) and,
    ///   through `IDXGIResource1::CreateSharedHandle`, one handle to it;
    /// * make one shared fence for the stream,
    ///   `ID3D11Device5::CreateFence(0, D3D11_FENCE_FLAG_SHARED, IID_ID3D12Fence)`,
    ///   and one handle to it;
    /// * duplicate both handles into the UI's process, [`Self::ui_process`],
    ///   and keep the values they have there: those are what this takes;
    /// * after drawing the picture, signal the fence with a value that only
    ///   grows, and pass that value here. The GPU has not finished the
    ///   drawing when this returns, so the value is what says it did, and the
    ///   UI shows nothing until the fence reaches it.
    ///
    /// The slots still hold a frame's size, and a file whose slots are too
    /// small for this picture is not grown: a GPU picture isn't in the file,
    /// and the pixel path above grows the file for itself when it writes one.
    pub fn publish(&mut self, drawn: &Drawn) -> bool {
        let header = self.mapping.header();
        // Nothing said yet is not an answer — the worker may publish before
        // the UI has had a picture to look at — but a refusal is, and so is
        // an adapter the UI doesn't draw on.
        match header.ui.load(Ordering::Acquire) {
            UI_UNSET => {}
            UI_USABLE if header.ui_luid.load(Ordering::Relaxed) == drawn.luid => {}
            _ => return false,
        }
        header.luid.store(drawn.luid, Ordering::Relaxed);
        header.fence.store(drawn.fence, Ordering::Relaxed);
        self.published += 1;
        let shared = &header.pictures[self.back as usize];
        shared.texture.store(drawn.texture, Ordering::Relaxed);
        shared.value.store(drawn.fence_value, Ordering::Relaxed);
        shared.generation.store(self.published, Ordering::Relaxed);
        let [w, h] = &header.sizes[self.back as usize];
        w.store(drawn.width, Ordering::Relaxed);
        h.store(drawn.height, Ordering::Relaxed);
        // A stream's pictures have no time in a file: only a video's frames
        // do, and a video's frames are pixels.
        header.times[self.back as usize].store(0, Ordering::Relaxed);
        self.back = header.middle.swap(self.back | FRESH, Ordering::AcqRel) & !FRESH;
        true
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

        writer.write(2, 1, |pixels| pixels.fill(1));
        writer.write(4, 2, |pixels| pixels.fill(2));
        let newest = frames.take(|width, height, pixels| (width, height, pixels.to_vec()));
        assert_eq!(newest, Some((4, 2, vec![2; 32])), "only the newest");
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

    /// A frame larger than the slots grows the file, and the UI's mapping with
    /// it: a stream shown in a larger window is written at its size, rather
    /// than scaled down to what the file was made for. Growing moves every
    /// slot, so what the UI reads after it is the frame the worker wrote
    /// there, at the size it wrote it.
    #[test]
    fn frames_grow_for_pictures_larger_than_their_slots() {
        let frames = Frames::create(4, 2).unwrap();
        let path = frames.path().unwrap();
        let mut writer = Writer::open(&path).unwrap();
        frames.unlink();
        writer.write(4, 2, |pixels| pixels.fill(1));
        assert_eq!(
            frames.take(|width, height, pixels| (width, height, pixels.to_vec())),
            Some((4, 2, vec![1; 32]))
        );
        assert!(!writer.fits(8, 4), "eight by four is larger than a slot");

        writer.write(8, 4, |pixels| pixels.fill(2));
        assert!(writer.fits(8, 4), "the slots grew with the frame");
        assert_eq!(
            frames.take(|width, height, pixels| (width, height, pixels.to_vec())),
            Some((8, 4, vec![2; 128]))
        );

        // A frame smaller than the grown slots still shows at its own size.
        writer.write(2, 1, |pixels| pixels.fill(3));
        assert_eq!(
            frames.take(|width, height, pixels| (width, height, pixels.to_vec())),
            Some((2, 1, vec![3; 8]))
        );

        drop((writer, frames));
        assert!(!path.exists(), "gone once both let go");
    }

    /// A picture the worker drew on the GPU, as it hands one to
    /// [`Writer::publish`]: handles 512 and 1024 are no real handles, but
    /// they say which texture a test published.
    fn drawn(texture: u64, width: u32, height: u32) -> Drawn {
        Drawn {
            luid: 0x1488a,
            fence: 512,
            texture,
            fence_value: texture * 2,
            width,
            height,
        }
    }

    /// The picture a slot describes, as a reader reads it, and nothing when
    /// it holds pixels.
    fn picture(frames: &Frames) -> Option<Picture> {
        frames
            .take_frame(|frame| match frame {
                Frame::Gpu(picture) => Some(picture),
                Frame::Pixels { width, height, .. } => {
                    assert!(width > 0 && height > 0, "a frame says its size");
                    None
                }
            })
            .flatten()
    }

    /// A slot holds pixels or a picture the worker drew on the GPU, and a
    /// reader is told which: the picture's description comes back whole, the
    /// pixel path after it is the frame the worker wrote rather than the
    /// picture read as bytes, and a reader that only shows pixels passes the
    /// picture over instead of taking its slot for one.
    #[test]
    fn a_slot_holds_pixels_or_a_picture_and_says_which() {
        let frames = Frames::create(4, 2).unwrap();
        let path = frames.path().unwrap();
        let mut writer = Writer::open(&path).unwrap();
        frames.unlink();

        assert!(writer.publish(&drawn(9, 4, 2)), "nothing said against it");
        assert_eq!(
            picture(&frames),
            Some(Picture {
                luid: 0x1488a,
                fence: 512,
                texture: 9,
                fence_value: 18,
                generation: 1,
                width: 4,
                height: 2,
            })
        );
        assert!(
            frames.take(|_, _, _| ()).is_none(),
            "a picture is not pixels"
        );

        writer.publish(&drawn(11, 4, 2));
        assert_eq!(picture(&frames).map(|picture| picture.texture), Some(11));

        writer.write(4, 2, |pixels| pixels.fill(4));
        assert_eq!(
            frames.take(|width, height, pixels| (width, height, pixels.to_vec())),
            Some((4, 2, vec![4; 32])),
            "the pixels after a picture are pixels"
        );

        drop((writer, frames));
        assert!(!path.exists(), "gone once both let go");
    }

    /// The UI decides once per stream whether it can show the worker's
    /// pictures, and says so in the header: a worker drawing for an adapter
    /// the UI doesn't draw on, or for a UI that refused them, writes the
    /// frame's pixels rather than losing the frame.
    #[test]
    fn a_picture_the_ui_cannot_open_comes_out_as_pixels() {
        let frames = Frames::create(4, 2).unwrap();
        let path = frames.path().unwrap();
        let mut writer = Writer::open(&path).unwrap();
        frames.unlink();

        // The UI draws on the worker's adapter, and says so.
        assert!(frames.accept_gpu(0x1488a));
        assert!(!frames.accept_gpu(0x1488a), "said once");
        assert!(writer.publish(&drawn(9, 4, 2)), "the same adapter");
        assert_eq!(picture(&frames).map(|picture| picture.luid), Some(0x1488a));

        // It draws on another one, and the worker stops drawing for it.
        assert!(frames.accept_gpu(0x2079f));
        assert!(
            !writer.publish(&drawn(11, 4, 2)),
            "another adapter is refused before anything is opened"
        );
        writer.write(4, 2, |pixels| pixels.fill(5));
        assert_eq!(
            frames.take(|_, _, pixels| pixels.to_vec()),
            Some(vec![5; 32])
        );

        // And once it has refused them outright, no picture is published.
        assert!(frames.refuse_gpu());
        assert!(!frames.refuse_gpu(), "refused once, logged once");
        assert!(!writer.publish(&drawn(13, 4, 2)), "the UI can't show it");
        writer.write(4, 2, |pixels| pixels.fill(6));
        assert_eq!(
            frames.take(|_, _, pixels| pixels.to_vec()),
            Some(vec![6; 32])
        );

        drop((writer, frames));
        assert!(!path.exists(), "gone once both let go");
    }

    /// A picture carries the size it was drawn at and a number that only
    /// grows, so a reader that shows it notices the worker drawing at another
    /// size and knows which picture it has. A stream that draws on the GPU and
    /// then writes pixels changes kind and size in one frame, and the reader
    /// is told that too.
    #[test]
    fn a_picture_says_its_size_and_number() {
        let frames = Frames::create(4, 2).unwrap();
        let path = frames.path().unwrap();
        let mut writer = Writer::open(&path).unwrap();
        frames.unlink();

        writer.publish(&drawn(9, 640, 360));
        assert_eq!(
            picture(&frames).map(|picture| (picture.width, picture.height, picture.generation)),
            Some((640, 360, 1))
        );
        writer.publish(&drawn(9, 1280, 720));
        assert_eq!(
            picture(&frames).map(|picture| (picture.width, picture.height, picture.generation)),
            Some((1280, 720, 2)),
            "the same texture, drawn at another size"
        );

        writer.write(2, 1, |pixels| pixels.fill(7));
        assert_eq!(
            frames
                .take_frame(|frame| match frame {
                    Frame::Pixels {
                        width,
                        height,
                        pixels,
                    } => Some((width, height, pixels.to_vec())),
                    Frame::Gpu(_) => None,
                })
                .flatten(),
            Some((2, 1, vec![7; 8])),
            "pixels again, at the pixel size"
        );

        drop((writer, frames));
        assert!(!path.exists(), "gone once both let go");
    }
}
