//! `frames.rs`'s layout.

use super::*;

pub const MAGIC: u32 = u32::from_le_bytes(*b"TKFR");

/// The version of this header the window writes, which a reader of another
/// version refuses the file for rather than reading its fields at another
/// meaning (Linux). Windows leaves the field it is in zero, and reads it not at
/// all.
#[cfg(target_os = "linux")]
pub const VERSION: u32 = 2;
/// Where the slots start. It's well past what the header says, so a field
/// added later lands in the room this one leaves rather than moving every
/// slot in a file the other process has already mapped.
pub const HEADER: usize = 256;
pub const SLOTS: u32 = 3;
/// Set beside the middle slot's index when the worker put a frame there that
/// the UI hasn't taken.
pub const FRESH: u32 = 4;

/// How many times the room a frame needs the file may hold before the rest of
/// it is worth giving back, and how long the frames have to keep needing less
/// before it is: a share that went full-screen and came back to a tile settles
/// in ten seconds, and a window passing through a large size doesn't.
pub const ROOM_SLACK: usize = 4;
pub const ROOM_BACK_AFTER: Duration = Duration::from_secs(10);

/// How long a slot the reader took and hasn't retired keeps the worker from
/// drawing a picture: a frame or two of the window's own lag is ordinary, and
/// a second of it is a window that stopped saying it is done with pictures —
/// one that was closed, or whose renderer lost the stream. Nothing waits on
/// that window: the worker gives the picture path up and writes pixels, which
/// touch no texture and rotate the ring again.
pub const HELD_FOR: Duration = Duration::from_secs(1);

/// What the UI says about a stream's GPU pictures: nothing yet, that it can
/// show them on the adapter in [`Header::ui_luid`], or that it can't, which
/// the worker reads as an answer to write pixels instead.
pub const UI_UNSET: u32 = 0;
pub const UI_USABLE: u32 = 1;
pub const UI_REFUSED: u32 = 2;

/// What a slot's picture is on Linux, where the memory it is in is named by the
/// descriptors that came over the socket rather than by anything in the file: a
/// slot of pixels has zero in the field this marks, so the two can't be taken for
/// each other, and the value itself means nothing.
#[cfg(target_os = "linux")]
pub const PICTURE: u64 = 1;

/// The header at the start of the file; slots follow it.
#[repr(C)]
pub struct Header {
    pub magic: u32,
    /// The bytes a slot holds, which is where each slot starts: it changes as
    /// frames need more room and as they stop needing it, and the process that
    /// changes it says so here before it writes a frame at the new size.
    pub capacity: AtomicU32,
    /// The middle slot's index, with [`FRESH`].
    pub middle: AtomicU32,
    /// Nothing on Windows. On Linux it is [`VERSION`], the version of this
    /// header the window made it with, which a reader of another version
    /// refuses the file for rather than reading its fields at another meaning.
    #[cfg(not(target_os = "linux"))]
    pub _reserved: u32,
    #[cfg(target_os = "linux")]
    pub version: AtomicU32,
    /// Each slot's frame size, as width and height in pixels.
    pub sizes: [[AtomicU32; 2]; SLOTS as usize],
    /// When each slot's frame shows in a video file, in milliseconds.
    pub times: [AtomicU64; SLOTS as usize],
    /// The adapter the worker draws its pictures on and the fence they are
    /// drawn behind, as the worker shared them: a stream has one device and
    /// one fence, so these belong to the file rather than to a slot. Zero
    /// while the stream publishes pixels.
    pub luid: AtomicU64,
    pub fence: AtomicU64,
    /// What each slot holds when it isn't pixels.
    pub pictures: [Shared; SLOTS as usize],
    /// The UI's answer on this stream's pictures; see [`UI_UNSET`].
    pub ui: AtomicU32,
    /// The UI's process, which the worker duplicates its shared handles into:
    /// a handle's value means nothing outside the process that took it.
    pub ui_pid: AtomicU32,
    /// The adapter the UI draws on, when its answer is [`UI_USABLE`].
    pub ui_luid: AtomicU64,
    /// The descriptor the worker's end of the socket its pictures travel on has
    /// in the process the window started for it (Linux); zero while the stream
    /// has no such socket, which a stream of pixels needs none of. Nothing on
    /// Windows, where a picture crosses as a handle in a slot instead.
    #[cfg(target_os = "linux")]
    pub socket: AtomicU32,
    /// Nonzero while the worker wants the file to itself, to give its room
    /// back: a reader that sees it lets its view go after the take it is in,
    /// and maps the file for each take until the worker has shortened it,
    /// rather than keeping one view from take to take.
    pub release: AtomicU32,
}

/// The largest frame `width` by `height` RGBA pixels fit in.
pub fn capacity(width: u32, height: u32) -> usize {
    width as usize * height as usize * 4
}

/// The header at the start of a view of the file.
pub fn header_of(map: &MmapMut) -> &Header {
    // SAFETY: every view here is at least HEADER bytes, page aligned, and
    // Header is plain integers.
    unsafe { &*map.as_ptr().cast::<Header>() }
}

/// Where slot `index` starts in a view whose slots hold `capacity` bytes each.
pub fn slot_of(map: &MmapMut, capacity: usize, index: u32) -> *mut u8 {
    // SAFETY: whoever maps the file checks that it holds every slot.
    unsafe {
        map.as_ptr()
            .cast_mut()
            .add(HEADER + index as usize * capacity)
    }
}
