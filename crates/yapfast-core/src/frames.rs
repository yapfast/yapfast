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
//! be made again too. A file gives its room back as well: a share that went
//! full-screen once doesn't hold four-K slots for the stream's life, since the
//! worker shortens the file once the frames have needed a fraction of it for
//! long enough. A reader maps the file for the length of one take and lets it
//! go after, holding the file shared while it does, so nothing is ever
//! shortened under a mapping: the worker takes the file to itself for it, and
//! a reader that is reading is what it finds there.
//!
//! A slot holds either pixels or a picture the worker drew on the GPU and
//! shared with the UI's process, which the UI shows without copying it: the
//! header says which, slot by slot, so a reader never takes one for the
//! other. A stream that doesn't draw on the GPU is a stream of pixels as it
//! always was, and one whose pictures the UI can't open says so in the header
//! and goes on with pixels from its next frame: a picture that can't be shown
//! is a slow stream, not a blank one.
//!
//! A GPU slot is the worker's to draw into again only once the UI says it has
//! finished with the picture that was there, slot by slot and generation by
//! generation. Taking a picture is not finishing with it: the window shows it
//! for as long as it is on screen, and the GPU may still be sampling the
//! texture for a frame after the next one replaced it. So the reader says so
//! itself, through [`Frames::retire`], when the rendering queue has passed the
//! picture it took, and nothing else counts as the worker's leave to draw
//! there.

use std::{
    fs::{File, OpenOptions, TryLockError},
    path::{Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicU32, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
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

/// How many times the room a frame needs the file may hold before the rest of
/// it is worth giving back, and how long the frames have to keep needing less
/// before it is: a share that went full-screen and came back to a tile settles
/// in ten seconds, and a window passing through a large size doesn't.
const ROOM_SLACK: usize = 4;
const ROOM_BACK_AFTER: Duration = Duration::from_secs(10);

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
    /// The bytes a slot holds, which is where each slot starts: it changes as
    /// frames need more room and as they stop needing it, and the process that
    /// changes it says so here before it writes a frame at the new size.
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
    /// The number of the set of textures and fence this slot's picture is of,
    /// which only grows within the slot: a set is three textures and one fence,
    /// rebuilt when a stream's pictures change size or a share is stopped and
    /// started, and a reader that has opened a picture of one tells the sets
    /// apart by this rather than by a handle, since Windows hands a closed
    /// handle's value out again.
    ///
    /// It belongs to the slot rather than to the file because a slot is read
    /// only with the picture the worker published in it: the set the header
    /// held while the reader read a slot another picture was published into
    /// would pair this slot's handle with the number of a set it is not of.
    set: AtomicU64,
    /// The newest generation of this slot the reader has finished with, which
    /// only grows and is the reader's alone to write: a worker draws into the
    /// slot again only once this is the generation that is there. Zero until
    /// a reader has finished with one, and a generation of zero — a slot that
    /// never held a picture — is the worker's from the start.
    retired: AtomicU64,
    /// The newest generation of this slot the reader has taken, which only
    /// grows and is the reader's alone to write. A generation the reader never
    /// took is a picture nobody showed and nothing can be sampling, so the
    /// worker's slot there is already its own; one it did take is on the screen
    /// until [`Self::retired`] reaches it, which a newer take of another slot
    /// does not say.
    took: AtomicU64,
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
    /// The number of the set of textures and fence it was drawn in, which only
    /// grows: a picture of a set the reader hasn't opened is a worker that
    /// rebuilt them, and what was opened for the set before it goes. A handle
    /// can't say that, because its value is handed out again once it's closed.
    pub set: u64,
    /// The slot it is in, which [`Frames::retire`] names it by: a picture's
    /// generation says which picture of that slot it is, and the two together
    /// are what the worker keeps a slot for.
    pub slot: u32,
    /// The picture's size, in pixels.
    pub width: u32,
    pub height: u32,
}

/// The handle of a texture shared into the UI's process, and the number of the
/// set of textures and fence it was shared in.
///
/// The two travel together because a handle's value names a texture only in the
/// process that holds it, and Windows gives a value out again once the handle
/// that had it is closed: a picture that carried the handle alone could be taken
/// for one of the set after it, and its reader would show the wrong texture or
/// open a number that is no texture at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SharedTexture {
    /// The handle, as the UI's process holds it.
    pub handle: u64,
    /// The number of the set it was duplicated in, which only grows.
    pub set: u64,
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
    /// UI's process, with the number of the set it was shared in.
    pub texture: SharedTexture,
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

/// The header at the start of a view of the file.
fn header_of(map: &MmapMut) -> &Header {
    // SAFETY: every view here is at least HEADER bytes, page aligned, and
    // Header is plain integers.
    unsafe { &*map.as_ptr().cast::<Header>() }
}

/// Where slot `index` starts in a view whose slots hold `capacity` bytes each.
fn slot_of(map: &MmapMut, capacity: usize, index: u32) -> *mut u8 {
    // SAFETY: whoever maps the file checks that it holds every slot.
    unsafe {
        map.as_ptr()
            .cast_mut()
            .add(HEADER + index as usize * capacity)
    }
}

/// The worker's view of the file: the slots it writes, kept between frames and
/// let go for as long as the file is being made smaller.
struct Mapping {
    /// The view, or none while the file is shortened: a file can't be ended
    /// while any view of it exists, this one included.
    map: Option<MmapMut>,
    capacity: usize,
    /// How many mappings to make fail, so a test can drive the path a file
    /// system's refusal takes without one. A failed mapping must leave the
    /// worker its slots.
    #[cfg(test)]
    fail_remap: u32,
}

impl Mapping {
    fn open(file: &File) -> Result<Self> {
        // SAFETY: only Yapfast's own processes open the file, and they touch the
        // slots through the triple buffer, never the same slot at once.
        let map = unsafe { MmapMut::map_mut(file) }.context("could not map the video frames")?;
        ensure!(map.len() >= HEADER, "the video frames file is too short");
        let mut mapping = Self {
            map: Some(map),
            capacity: 0,
            #[cfg(test)]
            fail_remap: 0,
        };
        ensure!(mapping.header().magic == MAGIC, "not a video frames file");
        let capacity = mapping.capacity_now();
        ensure!(
            mapping.view().len() >= HEADER + SLOTS as usize * capacity,
            "the video frames file is too short"
        );
        mapping.capacity = capacity;
        Ok(mapping)
    }

    /// The view itself: every use of a mapping is a use of the slots in it, and
    /// the only time there is none is between letting the view go and mapping
    /// the shortened file.
    fn view(&self) -> &MmapMut {
        self.map.as_ref().expect("the slots are mapped")
    }

    /// The bytes a slot holds, as the header says: frames larger than those
    /// that wrote it moved every slot, and this mapping with them.
    fn capacity_now(&self) -> usize {
        self.header().capacity.load(Ordering::Acquire) as usize
    }

    /// Maps the file again at `length`, which is what a resize needs: a file
    /// can't be ended or lengthened under the view it already has. A failure
    /// leaves whatever view there was, and the slots where they were.
    fn remap(&mut self, file: &File, length: usize) -> Result<()> {
        #[cfg(test)]
        if self.fail_remap > 0 {
            self.fail_remap -= 1;
            return Err(anyhow::anyhow!("the video frames can't be mapped again"));
        }
        // SAFETY: as `open`.
        let map = unsafe { MmapMut::map_mut(file) }.context("could not map the video frames")?;
        ensure!(map.len() >= length, "the video frames file is too short");
        self.map = Some(map);
        Ok(())
    }

    /// Maps the file again when a shortening left no view, taking the capacity
    /// the file really holds: the header's number is trusted only as far as the
    /// file backs it, so a file that was shortened and not mapped again still
    /// gives the worker slots to write in rather than a panic.
    fn remap_any(&mut self, file: &File) -> Result<()> {
        // SAFETY: as `open`.
        let map = unsafe { MmapMut::map_mut(file) }.context("could not map the video frames")?;
        ensure!(map.len() >= HEADER, "the video frames file is too short");
        self.map = Some(map);
        let held = (self.view().len() - HEADER) / SLOTS as usize;
        let said = self.capacity_now();
        self.capacity = said.min(held);
        if self.capacity != said {
            // Every slot is where the file puts it now, so the header says the
            // stride the slots really have, and the frame that moved with the
            // room is not read at the old one.
            let header = self.header();
            header
                .capacity
                .store(u32::try_from(self.capacity)?, Ordering::Release);
            header.middle.fetch_and(!FRESH, Ordering::AcqRel);
        }
        Ok(())
    }

    /// Grows the file and this mapping to hold slots of `capacity` bytes,
    /// when they hold smaller ones, and says so in the header. Only growing
    /// keeps a mapping another process holds whole: the file it read stays
    /// where it was, and the header is what tells it to map again.
    fn grow(&mut self, file: &File, capacity: usize) -> Result<()> {
        if self.map.is_none() {
            // A shortening that couldn't map the file again left no view: map
            // it where it is now before asking it for more room.
            self.remap_any(file)?;
        }
        if capacity <= self.capacity {
            return Ok(());
        }
        let length = HEADER + SLOTS as usize * capacity;
        file.set_len(length as u64)
            .context("could not size the video frames file")?;
        self.remap(file, length)?;
        self.capacity = capacity;
        self.header()
            .capacity
            .store(u32::try_from(capacity)?, Ordering::Release);
        Ok(())
    }

    /// Makes the slots `capacity` bytes, and this view with them, when they
    /// hold more of the file than the frames need: the room a share used when
    /// it was shown large goes back to the machine rather than being held for
    /// the stream's life.
    ///
    /// The caller holds the file to itself for the whole of this — every reader
    /// takes the file shared for as long as it has a view of it — so no view but
    /// this one exists, which is what makes ending the file safe rather than
    /// merely possible: Windows won't end a file any process has a view of, and
    /// a view of a file that shrank reads past its end. Answers whether the file
    /// gave its room back; a length the file system won't end keeps its room and
    /// loses no frame.
    fn shorten(&mut self, file: &File, capacity: usize) -> Result<bool> {
        if capacity >= self.capacity {
            return Ok(false);
        }
        let old_length = HEADER + SLOTS as usize * self.capacity;
        // This view goes first: it is a view of the file like any other, and
        // one of them is enough to keep the file its length.
        self.map = None;
        let length = HEADER + SLOTS as usize * capacity;
        if let Err(error) = file.set_len(length as u64) {
            tracing::debug!(
                error = format!("{error:#}"),
                "the video frames keep their room"
            );
            self.remap(file, old_length)?;
            return Ok(false);
        }
        // The room is gone from the file; mapping it again is what makes the
        // slots smaller. A mapping that fails here must not leave the writer
        // without slots, so the room goes back on the file and it is mapped at
        // it: no room given back rather than no frames. A file that can't be
        // mapped even at its old length is mapped again by `grow` before the
        // next frame, which skips that frame rather than panicking.
        if let Err(error) = self.remap(file, length) {
            if file.set_len(old_length as u64).is_ok() && self.remap(file, old_length).is_ok() {
                return Ok(false);
            }
            self.capacity = 0;
            return Err(error);
        }
        self.capacity = capacity;
        let header = self.header();
        header
            .capacity
            .store(u32::try_from(capacity)?, Ordering::Release);
        // Every slot moved with the file, so a frame the reader hasn't taken is
        // no longer where its size says it is: the freshness goes with the room,
        // and the next frame written is the first read at the new stride.
        header.middle.fetch_and(!FRESH, Ordering::AcqRel);
        Ok(true)
    }

    fn header(&self) -> &Header {
        header_of(self.view())
    }

    fn slot(&self, index: u32) -> *mut u8 {
        slot_of(self.view(), self.capacity, index)
    }
}

/// A reader's view of the file, held for the length of one take and let go with
/// the shared lock that goes with it. A reader that isn't reading has no view up,
/// which is what lets the worker give the file's room back: it shortens the file
/// only while it can hold the file to itself, and it asks by taking the lock.
struct Reading<'a> {
    file: &'a File,
    /// Whether the file is held shared for as long as this view exists. A file
    /// system that can't lock the file is one where the room isn't given back
    /// either — the worker's own attempt fails there too — so a refusal is not
    /// a reason to stop showing frames.
    locked: bool,
    /// The view, or none for the moment between letting it go and the lock
    /// going with it.
    map: Option<MmapMut>,
    /// The bytes a slot held when this view was made, which is the stride its
    /// slots are read at.
    capacity: usize,
}

impl<'a> Reading<'a> {
    /// Maps the file for a read: the lock comes first and the view after it, so
    /// a view never exists while the worker could be shortening the file.
    fn open(file: &'a File) -> Result<Self> {
        // A lock error is not a reason to stop showing frames: it is the same
        // answer the writer gets when it asks, so nothing shortens this file.
        let locked = file.lock_shared().is_ok();
        // SAFETY: only Yapfast's own processes map the file, and they touch the
        // slots through the triple buffer, never the same slot at once.
        let map = match unsafe { MmapMut::map_mut(file) } {
            Ok(map) => map,
            Err(error) => {
                if locked {
                    let _ = file.unlock();
                }
                return Err(error).context("could not map the video frames");
            }
        };
        // From here the view letting go of the file is also what unlocks it,
        // so a file that isn't the frames file is let go of like one that is.
        let mut reading = Self {
            file,
            locked,
            map: Some(map),
            capacity: 0,
        };
        ensure!(
            reading.view().len() >= HEADER,
            "the video frames file is too short"
        );
        ensure!(reading.header().magic == MAGIC, "not a video frames file");
        reading.capacity = reading.capacity_now();
        ensure!(
            reading.view().len() >= HEADER + SLOTS as usize * reading.capacity,
            "the video frames file is too short"
        );
        Ok(reading)
    }

    fn view(&self) -> &MmapMut {
        self.map.as_ref().expect("the frames are mapped")
    }

    fn header(&self) -> &Header {
        header_of(self.view())
    }

    fn capacity_now(&self) -> usize {
        self.header().capacity.load(Ordering::Acquire) as usize
    }

    fn slot(&self, index: u32) -> *mut u8 {
        slot_of(self.view(), self.capacity, index)
    }
}

impl Drop for Reading<'_> {
    fn drop(&mut self) {
        // The view goes before the lock: the worker shortens the file only
        // while nothing has a view of it, and a lock let go with the view still
        // up would say the file was the worker's to shorten.
        self.map = None;
        if self.locked {
            let _ = self.file.unlock();
        }
    }
}

/// The UI's side: it creates the file and shows the newest frame.
pub struct Frames {
    /// The file, mapped for the length of a take and let go after it: the
    /// worker shortens it only while nothing has it mapped, so a view held
    /// between takes would hold the file's room with it.
    file: File,
    /// The slot the UI read last, which is the one it hands back when it takes
    /// the middle one.
    front: Mutex<Front>,
    name: Mutex<Option<Name>>,
}

/// What the UI holds between takes: the slot it reads, and nothing of the file
/// itself, which is mapped again for each take.
struct Front {
    slot: u32,
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
        // A frame being read holds the file, and a log line doesn't wait for
        // it: what it holds is printed when it can be read.
        let capacity = self.front.try_lock().ok().and_then(|_front| {
            Reading::open(&self.file)
                .ok()
                .map(|reading| reading.capacity)
        });
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
            file,
            front: Mutex::new(Front { slot: 2 }),
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
        // The file is mapped for this take and not held between them: the worker
        // shortens it only while nothing has it mapped, and a view of a file that
        // shrank reads another frame's pixels.
        let reading = match Reading::open(&self.file) {
            Ok(reading) => reading,
            Err(error) => {
                // The frames are lost with this view, but a stream's sound and
                // the call go on: the next take maps the file again.
                tracing::warn!(
                    error = format!("{error:#}"),
                    "could not map the video frames"
                );
                return None;
            }
        };
        let held = reading.capacity;
        if reading.header().middle.load(Ordering::Acquire) & FRESH == 0 {
            return None;
        }
        front.slot = reading.header().middle.swap(front.slot, Ordering::AcqRel) & !FRESH;
        // The worker may have grown the file since: the frame just taken was
        // written at a slot size this view doesn't hold, so it's let go for the
        // frame after it, which is read where it was written. A file that
        // shrank can't be seen here: the reader holds it shared while it reads,
        // and the worker shortens it only while it has the file to itself.
        if reading.capacity_now() != held {
            return None;
        }
        let header = reading.header();
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
            let pixels = unsafe { std::slice::from_raw_parts(reading.slot(front.slot), length) };
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
                set: shared.set.load(Ordering::Relaxed),
                slot: front.slot,
                width,
                height,
            })
        };
        // The picture the reader took, slot by slot: a picture it never took
        // is one nobody showed and nothing can be sampling, so the worker may
        // draw over it, and one it did take is its own to keep until the
        // rendering queue says it is done with it.
        if texture != 0 {
            shared
                .took
                .fetch_max(shared.generation.load(Ordering::Acquire), Ordering::AcqRel);
        }
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

    /// Says the reader has finished with slot `slot`'s picture `generation`,
    /// which the worker may draw into again from then on: the picture is off
    /// the screen and the rendering queue has passed it, so nothing here reads
    /// that texture any more.
    ///
    /// Taking a picture is not finishing with it. A picture the reader couldn't
    /// show — a fence that didn't arrive in time, a texture it couldn't open —
    /// is still the one on the screen, and the slot it is in is the reader's
    /// until the picture it replaced is off it. Says whether this was the
    /// first time the slot heard it.
    pub fn retire(&self, slot: u32, generation: u64) -> bool {
        let _front = self
            .front
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let Ok(reading) = Reading::open(&self.file) else {
            return false;
        };
        let shared = &reading.header().pictures[slot as usize];
        if shared.generation.load(Ordering::Acquire) != generation {
            // A slot the worker has drawn into since is not this picture's to
            // retire: the worker only draws there after a retirement, so this
            // is a retirement of something already finished with.
            return false;
        }
        shared.retired.fetch_max(generation, Ordering::AcqRel) < generation
    }

    /// Says the UI can show this stream's pictures from the GPU, on `luid`,
    /// the adapter its window draws on: the worker publishes them from its
    /// next frame on. Says whether that changed what the header said.
    pub fn accept_gpu(&self, luid: u64) -> bool {
        // Held so that one reader maps the file at a time: the lock each view
        // takes is the file's, and two views of one process would share it.
        let _front = self
            .front
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let Ok(reading) = Reading::open(&self.file) else {
            return false;
        };
        let header = reading.header();
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
        // Held so that one reader maps the file at a time, as `accept_gpu`
        // holds it.
        let _front = self
            .front
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let Ok(reading) = Reading::open(&self.file) else {
            return false;
        };
        reading.header().ui.swap(UI_REFUSED, Ordering::AcqRel) != UI_REFUSED
    }
}

/// The worker's side: it fills frames.
pub struct Writer {
    /// The file the slots are mapped from, kept to lengthen it when a frame
    /// needs more room than it has and to shorten it once the frames need less.
    file: File,
    mapping: Mapping,
    back: u32,
    /// How many pictures this writer has published, which numbers them.
    published: u64,
    /// The largest frame written since the frames went small, and when the
    /// first of them was; see [`Self::give_room_back`].
    small: Option<(usize, Instant)>,
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
            small: None,
        })
    }
    /// The UI's process, which the worker duplicates its shared handles into
    /// before [`Self::publish`] takes them: a handle's value means nothing
    /// outside the process that took it, so the ones the UI opens are the
    /// ones duplicated there.
    pub fn ui_process(&self) -> u32 {
        self.mapping.header().ui_pid.load(Ordering::Acquire)
    }

    /// The adapter the UI's window draws on, when it has said its pictures can
    /// be shown there: what a stream reports so a share that copies every frame
    /// can say whether the two adapters are why.
    pub fn presented_on(&self) -> Option<u64> {
        let header = self.mapping.header();
        (header.ui.load(Ordering::Acquire) == UI_USABLE)
            .then(|| header.ui_luid.load(Ordering::Acquire))
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
        let wanted = capacity(width, height);
        self.mapping.grow(&self.file, wanted)?;
        self.give_room_back(wanted, ROOM_BACK_AFTER);
        Ok(())
    }

    /// Gives the file's room back when the frames have needed far less of it
    /// for long enough: a share that went full-screen once and came back to a
    /// tile holds three slots of four-K pixels, about a hundred megabytes, it
    /// never writes again.
    ///
    /// A frame that wants more than a quarter of the room is not small, and
    /// starts the wait over; one that wants less starts it, and the largest of
    /// the frames in that wait is the size the file settles at, so a frame of
    /// the size it settled at doesn't grow it again. `waited` is how long the
    /// frames have to have stayed small, which a stream leaves to
    /// [`ROOM_BACK_AFTER`]. The file is held to this writer while it is
    /// shortened, by taking the lock every reader holds for as long as it has
    /// the file mapped: the room goes back only when nobody is reading, and a
    /// reader that is keeps its view whole.
    fn give_room_back(&mut self, wanted: usize, waited: Duration) {
        let held = self.mapping.capacity;
        if wanted == 0 || wanted.saturating_mul(ROOM_SLACK) > held {
            self.small = None;
            return;
        }
        let (largest, since) = match self.small {
            Some((largest, since)) => (largest.max(wanted), since),
            None => (wanted, Instant::now()),
        };
        self.small = Some((largest, since));
        if since.elapsed() < waited {
            return;
        }
        match self.file.try_lock() {
            Ok(()) => {}
            // A reader is reading, and the room waits for one that isn't: a
            // frame is not worth shortening a file a reader has open.
            Err(TryLockError::WouldBlock) => {
                self.small = Some((largest, Instant::now()));
                return;
            }
            // A file system that can't lock the file is one no reader is
            // holding for the writer either, so nothing here is certain enough
            // to end a file over.
            Err(TryLockError::Error(error)) => {
                tracing::debug!(
                    error = format!("{error:#}"),
                    "the video frames keep their room"
                );
                self.small = Some((largest, Instant::now()));
                return;
            }
        }
        let shortened = self.mapping.shorten(&self.file, largest);
        let _ = self.file.unlock();
        self.small = None;
        match shortened {
            Ok(true) => tracing::debug!(
                bytes = largest * SLOTS as usize,
                "the video frames gave their room back"
            ),
            Ok(false) => {}
            Err(error) => tracing::warn!(
                error = format!("{error:#}"),
                "could not give the video frames' room back"
            ),
        }
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

    /// The slot a GPU picture goes in next, when the reader has finished with
    /// the one there: `None` means the UI still holds what that slot holds, and
    /// this frame's preview is dropped rather than drawn over a picture
    /// somebody is looking at. A slot that never held a picture is always free.
    ///
    /// It is the reader's own record that answers, a slot at a time: a picture
    /// taken is a picture on the screen until the next one replaces it and the
    /// rendering queue has drawn that, which can be a frame or more after the
    /// take — so only the reader knows, and it says so with [`Frames::retire`].
    /// A *newer* take of another slot is not that answer: the window may still
    /// be sampling the older picture's texture, so it stays the reader's.
    ///
    /// The answer is always the writer's own back slot, and never another: with
    /// three slots, the one that is neither the middle nor the reader's front is
    /// the writer's, and drawing anywhere else would paint over the slot the
    /// reader is reading or is about to be handed. The three stay apart because
    /// each side only ever swaps its own slot with the middle (see
    /// [`Self::publish_into`]), so the back is never the middle nor the front.
    pub fn claim(&self) -> Option<u32> {
        let header = self.mapping.header();
        debug_assert_ne!(
            self.back,
            header.middle.load(Ordering::Acquire) & !FRESH,
            "the writer's back is never the slot the reader is handed next"
        );
        let shared = &header.pictures[self.back as usize];
        let generation = shared.generation.load(Ordering::Acquire);
        // A picture the reader never took is one nobody has shown and nothing
        // can be sampling, so the slot is the writer's again; one it did take is
        // the worker's only once the reader has finished with it.
        let took = shared.took.load(Ordering::Acquire);
        let retired = shared.retired.load(Ordering::Acquire);
        (took < generation || retired >= generation).then_some(self.back)
    }

    /// Publishes a picture drawn on the GPU into `slot`, which
    /// [`Self::claim`] answered, and says whether it did: `false` means the UI
    /// can't show such pictures — it looked and said so, or it draws on
    /// another adapter — and the caller writes the frame's pixels instead.
    ///
    /// The worker must, before calling this:
    /// * draw into a `DXGI_FORMAT_B8G8R8A8_UNORM` texture of the picture's
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
    ///
    /// A picture of a set older than the one this slot holds is refused, `false`
    /// and nothing written: the number a set is known by only grows, and a
    /// reader that has opened the newer one would take a handle of the older for
    /// one of its own.
    pub fn publish_into(&mut self, slot: u32, drawn: &Drawn) -> bool {
        let header = self.mapping.header();
        // Nothing said yet is not an answer — the worker may publish before
        // the UI has had a picture to look at — but a refusal is, and so is
        // an adapter the UI doesn't draw on.
        match header.ui.load(Ordering::Acquire) {
            UI_UNSET => {}
            UI_USABLE if header.ui_luid.load(Ordering::Relaxed) == drawn.luid => {}
            _ => return false,
        }
        // The slot is the writer's own back, which `claim` answers: it is
        // never the middle and never the reader's front. The reader's swap of
        // the middle may land between the claim and here, which is why nothing
        // is decided by the middle this was claimed against — the swap below
        // takes the middle as it is at that moment, and `back` is the writer's
        // alone until then.
        debug_assert_eq!(slot, self.back, "a picture goes into the writer's own slot");
        let slot = self.back;
        let shared = &header.pictures[slot as usize];
        if drawn.texture.set < shared.set.load(Ordering::Acquire) {
            return false;
        }
        header.luid.store(drawn.luid, Ordering::Relaxed);
        header.fence.store(drawn.fence, Ordering::Relaxed);
        self.published += 1;
        // The set before the handle that is of it, so a reader never pairs a
        // picture with the number of another set.
        shared.set.store(drawn.texture.set, Ordering::Relaxed);
        shared
            .texture
            .store(drawn.texture.handle, Ordering::Relaxed);
        shared.value.store(drawn.fence_value, Ordering::Relaxed);
        shared.generation.store(self.published, Ordering::Relaxed);
        let [w, h] = &header.sizes[slot as usize];
        w.store(drawn.width, Ordering::Relaxed);
        h.store(drawn.height, Ordering::Relaxed);
        // A stream's pictures have no time in a file: only a video's frames
        // do, and a video's frames are pixels.
        header.times[slot as usize].store(0, Ordering::Relaxed);
        self.back = header.middle.swap(slot | FRESH, Ordering::AcqRel) & !FRESH;
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

    /// A file that grew for a picture shown large gives the room back once the
    /// frames have needed a fraction of it for long enough: a share that went
    /// full-screen and came back to a tile holds slots the size of the screen
    /// for the stream's life otherwise. The frame whose slot moved with the
    /// room is not read at the old stride, and the frame written after it is
    /// read where it was written.
    #[test]
    fn frames_give_back_the_room_a_large_picture_took() {
        let frames = Frames::create(4, 2).unwrap();
        let path = frames.path().unwrap();
        let mut writer = Writer::open(&path).unwrap();
        frames.unlink();

        writer.write(64, 48, |pixels| pixels.fill(1));
        assert!(writer.fits(64, 48), "the slots grew for the picture");
        assert_eq!(
            frames.take(|width, height, _| (width, height)),
            Some((64, 48))
        );
        let grown = writer.file.metadata().unwrap().len();
        assert_eq!(grown, HEADER as u64 + SLOTS as u64 * 64 * 48 * 4);

        // A frame much smaller than the slots, and the wait the room goes back
        // after: a test's wait is none, and a stream's is ten seconds.
        writer.write(4, 2, |pixels| pixels.fill(2));
        writer.give_room_back(4 * 2 * 4, Duration::ZERO);
        let held = writer.file.metadata().unwrap().len();
        assert_eq!(held, HEADER as u64 + SLOTS as u64 * 4 * 2 * 4);
        assert!(
            frames.take(|_, _, _| ()).is_none(),
            "the frame whose slot moved went with the room"
        );

        // The frame written now is read at the stride the slots have.
        writer.write(4, 2, |pixels| pixels.fill(3));
        assert_eq!(
            frames.take(|width, height, pixels| (width, height, pixels.to_vec())),
            Some((4, 2, vec![3; 32]))
        );
        assert_eq!(
            writer.file.metadata().unwrap().len(),
            held,
            "a frame of the size the file settled at doesn't grow it again"
        );

        drop((writer, frames));
        assert!(!path.exists(), "gone once both let go");
    }

    /// A picture the worker drew on the GPU, as it hands one to
    /// [`Writer::publish_into`]: handles 512 and 1024 are no real handles, but
    /// they say which texture a test published.
    fn drawn(texture: u64, width: u32, height: u32) -> Drawn {
        drawn_in(1, texture, width, height)
    }

    /// The same, drawn in the set of textures and fence numbered `set`.
    fn drawn_in(set: u64, texture: u64, width: u32, height: u32) -> Drawn {
        Drawn {
            luid: 0x1488a,
            fence: 512,
            texture: SharedTexture {
                handle: texture,
                set,
            },
            fence_value: texture * 2,
            width,
            height,
        }
    }

    /// Publishes a picture the way the worker does: into the slot the reader
    /// has moved on from, which is what [`Writer::claim`] answers.
    fn publish(writer: &mut Writer, drawn: &Drawn) -> bool {
        let Some(slot) = writer.claim() else {
            return false;
        };
        writer.publish_into(slot, drawn)
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

        assert!(
            publish(&mut writer, &drawn(9, 4, 2)),
            "nothing said against it"
        );
        assert_eq!(
            picture(&frames),
            Some(Picture {
                luid: 0x1488a,
                fence: 512,
                texture: 9,
                fence_value: 18,
                generation: 1,
                set: 1,
                slot: 0,
                width: 4,
                height: 2,
            })
        );
        assert!(
            frames.take(|_, _, _| ()).is_none(),
            "a picture is not pixels"
        );

        publish(&mut writer, &drawn(11, 4, 2));
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
        assert!(publish(&mut writer, &drawn(9, 4, 2)), "the same adapter");
        assert_eq!(picture(&frames).map(|picture| picture.luid), Some(0x1488a));

        // It draws on another one, and the worker stops drawing for it.
        assert!(frames.accept_gpu(0x2079f));
        assert!(
            !publish(&mut writer, &drawn(11, 4, 2)),
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
        assert!(
            !publish(&mut writer, &drawn(13, 4, 2)),
            "the UI can't show it"
        );
        writer.write(4, 2, |pixels| pixels.fill(6));
        assert_eq!(
            frames.take(|_, _, pixels| pixels.to_vec()),
            Some(vec![6; 32])
        );

        drop((writer, frames));
        assert!(!path.exists(), "gone once both let go");
    }

    /// A GPU slot is drawn into again only once the reader says it has
    /// finished with the picture that is there, slot by slot and generation by
    /// generation. Taking a picture is not finishing with it: the window shows
    /// it until the next one replaces it, and the GPU may still be sampling
    /// its texture after that, so the reader's own word is what frees the slot.
    /// A window that falls behind costs preview frames and never a torn
    /// picture.
    #[test]
    fn a_slot_is_drawn_into_again_only_once_the_reader_finished_with_it() {
        let frames = Frames::create(4, 2).unwrap();
        let path = frames.path().unwrap();
        let mut writer = Writer::open(&path).unwrap();
        frames.unlink();

        // The file has three slots, and none has held a picture yet: the worker
        // takes its own slot back each time, and the reader is handed the newest.
        for texture in 9..12 {
            let slot = writer.claim().expect("a slot that never held a picture");
            assert!(writer.publish_into(slot, &drawn(texture, 4, 2)));
        }
        // The pictures the reader never took are ones it will never show, so the
        // worker's own slot is still its own. The picture the reader did take is
        // not, until the reader says its queue has finished with it.
        let newest = picture(&frames).expect("the newest picture");
        assert_eq!((newest.slot, newest.generation), (0, 3));
        let slot = writer.claim().expect("a slot the reader never took");
        assert_ne!(slot, newest.slot, "not the picture on the screen");
        assert!(writer.publish_into(slot, &drawn(12, 4, 2)));

        // Finishing with a generation a slot doesn't hold says nothing about
        // the picture that is there, and a slot finished with twice frees
        // nothing more.
        assert!(
            !frames.retire(newest.slot, newest.generation - 1),
            "not the picture the slot holds"
        );
        assert!(frames.retire(newest.slot, newest.generation));
        assert!(!frames.retire(newest.slot, newest.generation), "once");

        // The worker draws its next picture somewhere of its own, and that is
        // the one the reader is handed.
        let slot = writer.claim().expect("a slot the writer may draw in");
        assert!(writer.publish_into(slot, &drawn(13, 4, 2)));
        let newest = picture(&frames).expect("the picture just published");
        assert!(newest.generation > 3);
        assert!(frames.retire(newest.slot, newest.generation));

        drop((writer, frames));
        assert!(!path.exists(), "gone once both let go");
    }

    /// A picture says which set of the worker's textures and fence it was drawn
    /// in, and that number only grows, slot by slot. A handle's value can be
    /// handed out again once the handle that had it is closed, so a reader that
    /// keys what it opened by the handle alone can't tell the set it has from the
    /// one after it; the number is what tells them apart.
    #[test]
    fn a_published_set_number_only_grows() {
        let frames = Frames::create(4, 2).unwrap();
        let path = frames.path().unwrap();
        let mut writer = Writer::open(&path).unwrap();
        frames.unlink();

        publish(&mut writer, &drawn_in(7, 9, 4, 2));
        assert_eq!(
            picture(&frames).map(|picture| (picture.set, picture.texture)),
            Some((7, 9))
        );
        publish(&mut writer, &drawn_in(7, 11, 4, 2));
        assert_eq!(
            picture(&frames).map(|picture| (picture.set, picture.texture)),
            Some((7, 11)),
            "the same set, another of its three textures"
        );

        // A set the worker built after that one, and the handle of a texture of
        // the set before it: only the number says the two are different.
        publish(&mut writer, &drawn_in(8, 9, 4, 2));
        let next = picture(&frames).expect("the picture of the next set");
        assert_eq!((next.set, next.texture), (8, 9));

        // A set numbered before the one the picture holds is a worker that
        // went backwards, and nothing of it is published: the reader would take
        // its handle for one of the set it has, and a picture is not written
        // where the number it is known by would go back.
        assert!(frames.retire(0, 1), "the first picture is off the screen");
        let slot = writer.claim().expect("a slot the reader has finished with");
        assert!(
            !writer.publish_into(slot, &drawn_in(6, 13, 4, 2)),
            "an older set was published"
        );
        assert_eq!(writer.claim(), Some(slot), "nothing was written there");
        assert!(writer.publish_into(slot, &drawn_in(9, 13, 4, 2)));
        assert_eq!(
            picture(&frames).map(|picture| (picture.set, picture.texture)),
            Some((9, 13)),
            "the slot holds the newer set's picture"
        );

        drop((writer, frames));
        assert!(!path.exists(), "gone once both let go");
    }

    /// The worker is never held up by the one slot it would have drawn in next:
    /// a window showing the newest picture keeps that slot, and the slot the
    /// reader never took is free to draw in instead. A rotation the worker
    /// couldn't turn would freeze the preview for the rest of the share.
    #[test]
    fn a_slot_the_reader_keeps_does_not_hold_up_the_worker() {
        let frames = Frames::create(4, 2).unwrap();
        let path = frames.path().unwrap();
        let mut writer = Writer::open(&path).unwrap();
        frames.unlink();

        publish(&mut writer, &drawn(9, 4, 2));
        publish(&mut writer, &drawn(10, 4, 2));
        // The reader takes the newest and goes on showing it: the slot the
        // worker would draw in next is one it hasn't finished with.
        let newest = picture(&frames).expect("the newest picture");
        assert_eq!((newest.slot, newest.generation), (1, 2));
        let slot = writer
            .claim()
            .expect("the slot the reader never took is free");
        assert_ne!(slot, newest.slot);
        assert!(writer.publish_into(slot, &drawn(11, 4, 2)));

        // The reader finishing with what it showed frees that slot in turn.
        assert!(frames.retire(newest.slot, newest.generation));
        assert!(writer.claim().is_some(), "a slot is free again");
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

        publish(&mut writer, &drawn(9, 640, 360));
        assert_eq!(
            picture(&frames).map(|picture| (picture.width, picture.height, picture.generation)),
            Some((640, 360, 1))
        );
        publish(&mut writer, &drawn(9, 1280, 720));
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

    /// The three slots' roles as the two sides hold them: the writer's back,
    /// the middle the reader is handed, and the reader's front.
    fn roles(writer: &Writer, frames: &Frames) -> (u32, u32, u32) {
        let middle = writer.mapping.header().middle.load(Ordering::Acquire) & !FRESH;
        let front = frames
            .front
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .slot;
        (writer.back, middle, front)
    }

    /// Back, middle and front are three different slots: the writer fills one,
    /// the reader is handed another, and reads a third. Two roles in one slot is
    /// the worker painting over the frame that is being read, or the ring
    /// collapsing into a double buffer.
    fn assert_three(writer: &Writer, frames: &Frames, when: &str) {
        let (back, middle, front) = roles(writer, frames);
        assert!(
            back != middle && middle != front && back != front,
            "{when}: the back is {back}, the middle is {middle} and the front is {front}"
        );
    }

    /// Publishing more pictures than the reader takes must never leave two roles
    /// in one slot, and it must not collapse the ring into two slots: three
    /// publishes before the first take used to put the middle in the reader's
    /// slot, after which the worker filled the frame the reader was reading.
    #[test]
    fn the_three_slots_are_never_two() {
        let frames = Frames::create(4, 2).unwrap();
        let path = frames.path().unwrap();
        let mut writer = Writer::open(&path).unwrap();
        frames.unlink();
        assert_three(&writer, &frames, "a fresh file");

        // More pictures than there are slots, and not a take among them: the
        // worker takes its own slot back each time.
        for texture in 9..13 {
            let slot = writer.claim().expect("a slot the writer may draw in");
            assert!(writer.publish_into(slot, &drawn(texture, 4, 2)));
            assert_three(&writer, &frames, "after a picture before a take");
        }

        // Then claims, takes and publishes in the order that used to tear: a
        // claim, the reader's take, the publish into the claimed slot.
        let newest = picture(&frames).expect("the newest picture");
        assert_three(&writer, &frames, "after the first take");
        // A picture for the middle, so the reader has one to take between the
        // claim and the publish.
        let slot = writer.claim().expect("a slot the writer may draw in");
        assert!(writer.publish_into(slot, &drawn(13, 4, 2)));
        let slot = writer
            .claim()
            .expect("the next slot the writer may draw in");
        assert_ne!(slot, newest.slot, "not the picture on the screen");
        let taken = picture(&frames).expect("the picture the middle held");
        assert_ne!(slot, taken.slot, "the middle moved away from the slot");
        assert_three(&writer, &frames, "after a take between claim and publish");
        assert!(writer.publish_into(slot, &drawn(14, 4, 2)));
        assert_three(&writer, &frames, "after the publish");
        // The picture just published is the one the reader is handed next, in
        // the slot the writer drew it in.
        let next = picture(&frames).expect("the picture just published");
        assert_eq!((next.slot, next.texture), (slot, 14));

        // The pixel path rotates the same three slots the same way.
        for value in 1..4 {
            writer.write(4, 2, |pixels| pixels.fill(value));
            assert_three(&writer, &frames, "after pixels");
            assert!(frames.take(|_, _, _| ()).is_some(), "the pixels are read");
            assert_three(&writer, &frames, "after the pixels are read");
        }

        drop((writer, frames));
        assert!(!path.exists(), "gone once both let go");
    }

    /// A picture the reader took stays the worker's only once the reader says its
    /// rendering queue has passed it. A newer take of another slot does not say
    /// that: the window may still be sampling the older picture's texture, and
    /// drawing there would put a picture nobody finished with behind the one on
    /// the screen.
    #[test]
    fn a_picture_the_reader_took_is_not_freed_by_a_newer_take() {
        let frames = Frames::create(4, 2).unwrap();
        let path = frames.path().unwrap();
        let mut writer = Writer::open(&path).unwrap();
        frames.unlink();

        // Two pictures taken one after the other: the first is on the screen
        // until the window's queue has drawn the second.
        publish(&mut writer, &drawn(9, 4, 2));
        let first = picture(&frames).expect("the first picture");
        publish(&mut writer, &drawn(10, 4, 2));
        let second = picture(&frames).expect("the second picture");
        assert_eq!((first.slot, first.generation), (0, 1));
        assert_eq!((second.slot, second.generation), (1, 2));

        // The writer's own slot comes round to the picture it took first, which
        // nothing has retired: it is the window's until the reader says so, and
        // the newer take of the other slot says nothing about it.
        publish(&mut writer, &drawn(11, 4, 2));
        assert_eq!(
            writer.claim(),
            None,
            "the picture the window took is still the window's"
        );
        assert!(frames.retire(first.slot, first.generation));
        assert_eq!(writer.claim(), Some(first.slot), "finished with, so free");

        drop((writer, frames));
        assert!(!path.exists(), "gone once both let go");
    }

    /// A file whose room the worker couldn't give back because the shortened file
    /// wouldn't map keeps its room and its slots: no room given back rather than
    /// no frames, and never a view that isn't there to write through.
    #[test]
    fn a_shortening_that_cannot_be_mapped_again_keeps_its_slots() {
        let frames = Frames::create(4, 2).unwrap();
        let path = frames.path().unwrap();
        let mut writer = Writer::open(&path).unwrap();
        frames.unlink();

        writer.write(64, 48, |pixels| pixels.fill(1));
        let grown = writer.file.metadata().unwrap().len();
        assert_eq!(grown, HEADER as u64 + SLOTS as u64 * 64 * 48 * 4);
        assert!(frames.take(|_, _, _| ()).is_some());

        // The shortened file can't be mapped again once: the room goes back on
        // the file and it is mapped where it was.
        writer.mapping.fail_remap = 1;
        writer.give_room_back(4 * 2 * 4, Duration::ZERO);
        assert_eq!(
            writer.file.metadata().unwrap().len(),
            grown,
            "the room stayed"
        );
        assert!(writer.fits(64, 48), "the slots are still there");

        // The frame after it is written and read where it was written.
        writer.write(8, 4, |pixels| pixels.fill(2));
        assert_eq!(
            frames.take(|width, height, pixels| (width, height, pixels.to_vec())),
            Some((8, 4, vec![2; 128]))
        );

        drop((writer, frames));
        assert!(!path.exists(), "gone once both let go");
    }

    /// A file that can't be mapped again even at the room it had is mapped by the
    /// frame after it: the writer skips a frame rather than panicking, and the
    /// slots it gets are the ones the file really holds.
    #[test]
    fn a_file_that_cannot_be_mapped_is_mapped_by_the_frame_after() {
        let frames = Frames::create(4, 2).unwrap();
        let path = frames.path().unwrap();
        let mut writer = Writer::open(&path).unwrap();
        frames.unlink();

        writer.write(64, 48, |pixels| pixels.fill(1));
        assert!(frames.take(|_, _, _| ()).is_some());

        // Neither the shortened file nor the file it was lengthened back to can
        // be mapped: no view is left, and the frame after asks for one.
        writer.mapping.fail_remap = 2;
        writer.give_room_back(4 * 2 * 4, Duration::ZERO);
        assert!(writer.mapping.map.is_none(), "no view is left");

        writer.write(4, 2, |pixels| pixels.fill(3));
        assert!(writer.mapping.map.is_some(), "the frame after mapped it");
        assert!(frames.take(|_, _, pixels| pixels.len()).is_some());

        drop((writer, frames));
        assert!(!path.exists(), "gone once both let go");
    }
}
