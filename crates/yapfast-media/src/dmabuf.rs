//! The channel a stream's picture descriptors travel on, on Linux.
//!
//! A picture the worker draws on the GPU on Windows is a shared texture, and
//! the handle it crosses in is a number that names the same object in both
//! processes, so it goes in the frames file both map. A DMA-BUF is a file
//! descriptor, which names a different object in every process: it can't be
//! written into the file, so it goes beside it, over a socket the window makes
//! before it starts the worker — one message for each picture published, with
//! the descriptors attached through `SCM_RIGHTS`, which has the kernel take a
//! reference to the memory for each process that receives them.
//!
//! The socket belongs to one stream. [`Reader`] holds the window's end and
//! takes pictures out of it; [`Sender`] holds the worker's end, which it took
//! from the number the frames header names — a descriptor is inherited across
//! the worker's `exec` at the number it had, since its close-on-exec flag was
//! cleared for that one spawn, so the header is all the two sides need to
//! agree on.
//!
//! Nothing here allocates a message: the datagram is a fixed struct sent with
//! one `sendmsg`, the receive buffers are fixed arrays on the stack, and the
//! reader keeps one picture per slot of the frames file in a table of its own.
//! A picture no slot holds is closed as it is displaced, and one that is taken
//! is closed when the window lets go of it — which it does as it retires the
//! slot, so a descriptor lives exactly as long as the picture it names is one
//! the window may still be sampling.

use std::{
    io,
    mem::MaybeUninit,
    os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd},
};

use crate::frames::SLOTS as FRAME_SLOTS;

/// The most planes a picture may have: DRM names four, for the formats that
/// carry luma, chroma and transparency in planes of their own.
pub const MAX_PLANES: usize = 4;

/// How many slots the frames file has, which is how many pictures wait here at
/// once: a picture is taken from the slot the header names it in, so one per
/// slot is all a reader ever needs.
const SLOTS: usize = FRAME_SLOTS as usize;

/// How many messages one drain reads at most, so that a take of a stream whose
/// window stopped taking pictures for a while works through the socket a little
/// at a time rather than in one go.
const DRAINED: usize = SLOTS * 4;

/// What every message starts with, and the version of the layout after it: a
/// message of another magic or version is one this build can't read, and the
/// descriptors it carried are closed rather than taken for a picture of this
/// one.
const MAGIC: u32 = u32::from_le_bytes(*b"YPDM");
/// Version 2 carries the colour signal a picture was coded with, in the bytes
/// version 1 kept as padding.
const VERSION: u16 = 2;

/// The range a picture's codes use.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Range {
    /// 16..235 luma and 16..240 chroma, which is how every 8-bit stream
    /// Discord carries is coded.
    Limited = 0,
    /// The whole range of the sample's own depth.
    Full = 1,
}

/// The matrix a picture's luma and chroma are combined with.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Matrix {
    Bt709 = 0,
    /// BT.2020's non-constant-luminance matrix, which HDR10 is coded with.
    Bt2020Ncl = 1,
}

/// The curve a picture's samples are in.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Transfer {
    /// A display-referred curve: BT.709's for 8-bit, BT.1886's gamma for 10-bit.
    Sdr = 0,
    /// SMPTE ST 2084, which HDR10 is coded with.
    Pq = 1,
}

/// The colour signal a picture was coded with, as the worker's shim read it out
/// of the stream: the range its codes use, the matrix luma and chroma are
/// combined with, and the curve they are in. The numbers are the shim's — see
/// `native/hwdec_vaapi.h` — and the tile pass reads them to show a 10-bit
/// picture in the colour it was coded with rather than in the 8-bit path's one
/// fixed matrix.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Colour {
    pub range: Range,
    pub matrix: Matrix,
    pub transfer: Transfer,
}

impl Colour {
    /// The colour an 8-bit stream is, and what a caller that has nothing to say
    /// about its pictures says: the tile pass's 8-bit path has one matrix.
    pub const SDR: Self = Self {
        range: Range::Limited,
        matrix: Matrix::Bt709,
        transfer: Transfer::Sdr,
    };

    /// What the shim's three numbers name, which are `Range`, `Matrix` and
    /// `Transfer`'s own discriminants: a number this build doesn't know is the
    /// 8-bit answer, since no stream it can't read a colour off is one it shows
    /// as 10-bit.
    pub fn of(range: i32, matrix: i32, transfer: i32) -> Self {
        let range = match range {
            1 => Range::Full,
            _ => Range::Limited,
        };
        let matrix = match matrix {
            1 => Matrix::Bt2020Ncl,
            _ => Matrix::Bt709,
        };
        let transfer = match transfer {
            1 => Transfer::Pq,
            _ => Transfer::Sdr,
        };
        Self {
            range,
            matrix,
            transfer,
        }
    }

    /// How the three travel: one byte each, laid out as the shim numbers them.
    fn bytes(self) -> [u8; 3] {
        [self.range as u8, self.matrix as u8, self.transfer as u8]
    }

    /// The colour three bytes name, or none while one of them isn't a number
    /// this build knows: a message of another layout's colour is no picture.
    fn read(bytes: [u8; 3]) -> Option<Self> {
        let [range, matrix, transfer] = bytes;
        Some(Self {
            range: match range {
                0 => Range::Limited,
                1 => Range::Full,
                _ => return None,
            },
            matrix: match matrix {
                0 => Matrix::Bt709,
                1 => Matrix::Bt2020Ncl,
                _ => return None,
            },
            transfer: match transfer {
                0 => Transfer::Sdr,
                1 => Transfer::Pq,
                _ => return None,
            },
        })
    }
}

impl Default for Colour {
    fn default() -> Self {
        Self::SDR
    }
}

/// One plane of a picture: where it starts in the memory its descriptor names,
/// and how many bytes apart its rows are.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Plane {
    /// The plane's first byte, from the start of the descriptor's memory.
    pub offset: u32,
    /// The bytes between the start of one row and the start of the next.
    pub stride: u32,
}

/// A picture the worker drew, as the writer is handed it: the shape of the
/// memory holding it and the descriptors that hold it, which stay the caller's
/// throughout — the kernel makes its own reference for each process the
/// message reaches, so nothing here is duplicated before it is sent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Descriptor<'a> {
    /// The picture's size, in pixels.
    pub width: u32,
    pub height: u32,
    /// The DRM format's fourcc, as `DRM_FORMAT_*` spells it: the layout of the
    /// pixels the memory holds, which the importer has to be told.
    pub fourcc: u32,
    /// The DRM modifier, which says how the driver laid the memory out —
    /// linear, tiled, or compressed — and is part of what a buffer has to be
    /// imported as.
    pub modifier: u64,
    /// Each plane's place in the memory, in the order of [`Self::fds`].
    pub planes: &'a [Plane],
    /// The colour signal the picture was coded with, which the tile pass reads
    /// to show a 10-bit picture in its own colour.
    pub colour: Colour,
    /// One descriptor for each plane: the memory itself.
    pub fds: &'a [RawFd],
}

/// One picture's message, as it goes over the socket: a fixed size, so a send
/// is one datagram and a receive needs no reassembly.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Message {
    magic: u32,
    version: u16,
    /// The slot of the frames file the picture goes in.
    slot: u16,
    /// The number of the picture, which only grows: the reader takes this for
    /// the slot only once the header says the slot holds that generation, so a
    /// message can arrive before the picture is announced.
    generation: u64,
    width: u32,
    height: u32,
    /// The DRM format's fourcc.
    fourcc: u32,
    /// How many of [`Self::places`], and of the descriptors attached, are the
    /// picture's.
    planes: u8,
    /// The colour signal, in [`Colour::bytes`]'s three bytes, where version 1
    /// had padding.
    colour: [u8; 3],
    modifier: u64,
    places: [Plane; MAX_PLANES],
}

/// The message stays one datagram a socket sends without a copy, and stays what
/// both ends agree on: a field added here is a version of the layout after it.
const _: () = assert!(size_of::<Message>() <= 128);

impl Message {
    fn of(slot: u32, generation: u64, picture: &Descriptor<'_>, planes: usize) -> Self {
        let mut places = [Plane::default(); MAX_PLANES];
        places[..planes].copy_from_slice(&picture.planes[..planes]);
        Self {
            magic: MAGIC,
            version: VERSION,
            slot: slot as u16,
            generation,
            width: picture.width,
            height: picture.height,
            fourcc: picture.fourcc,
            planes: planes as u8,
            colour: picture.colour.bytes(),
            modifier: picture.modifier,
            places,
        }
    }

    /// How many planes this names, when it is a message of this build's layout
    /// at all and names a number of planes a picture can have.
    fn plane_count(&self) -> Option<usize> {
        if self.magic != MAGIC || self.version != VERSION {
            return None;
        }
        let planes = self.planes as usize;
        (1..=MAX_PLANES).contains(&planes).then_some(planes)
    }
}

/// One control message with room for a picture's descriptors, laid out as the
/// kernel reads and writes it: the header, then the descriptors.
#[repr(C)]
struct Control {
    header: libc::cmsghdr,
    fds: [RawFd; MAX_PLANES],
}

impl Control {
    fn empty() -> Self {
        Self {
            header: libc::cmsghdr {
                cmsg_len: 0,
                cmsg_level: 0,
                cmsg_type: 0,
            },
            fds: [0; MAX_PLANES],
        }
    }

    /// The control message's length for `fds` descriptors: the header and the
    /// descriptors with no padding between them, both already aligned.
    fn length(fds: usize) -> usize {
        size_of::<libc::cmsghdr>() + fds * size_of::<RawFd>()
    }
}

/// A picture that arrived, with the descriptors holding it, which are this
/// process's from here and are closed when it is dropped.
#[derive(Debug)]
pub struct Picture {
    slot: u32,
    generation: u64,
    width: u32,
    height: u32,
    fourcc: u32,
    modifier: u64,
    /// Each plane's place in the memory and the descriptor that holds it, in
    /// the order the worker sent them.
    planes: [Option<(Plane, OwnedFd)>; MAX_PLANES],
    colour: Colour,
}

impl Picture {
    /// The slot of the frames file it was published in, which is the slot the
    /// window retires with it.
    pub fn slot(&self) -> u32 {
        self.slot
    }

    /// The number of the picture, which the slot's header is compared with
    /// before it is taken.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// The picture's size, in pixels.
    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// The DRM format's fourcc: what the memory holds, plane by plane.
    pub fn fourcc(&self) -> u32 {
        self.fourcc
    }

    /// The DRM modifier the memory was laid out with.
    pub fn modifier(&self) -> u64 {
        self.modifier
    }

    /// The colour signal the picture was coded with: its range, its matrix and
    /// its transfer curve, which the tile pass reads for a 10-bit picture.
    pub fn colour(&self) -> Colour {
        self.colour
    }

    /// How many planes the picture has, and how many descriptors came with it.
    pub fn plane_count(&self) -> usize {
        self.planes.iter().flatten().count()
    }

    /// Each plane's place in the memory and the descriptor an importer takes to
    /// it, which is this process's own: it is valid for as long as this picture
    /// is held.
    ///
    /// This is what the window's Vulkan importer will be handed, one
    /// `VK_EXTERNAL_MEMORY_HANDLE_TYPE_DMA_BUF_BIT_EXT` per plane; until that
    /// exists, holding the picture is what keeps the memory there, and dropping
    /// it is what closes the descriptors.
    pub fn planes(&self) -> impl Iterator<Item = (Plane, RawFd)> + '_ {
        self.planes
            .iter()
            .flatten()
            .map(|(plane, fd)| (*plane, fd.as_raw_fd()))
    }
}

/// The window's end of a stream's socket, and the pictures taken out of it that
/// no take has asked for yet.
#[derive(Default)]
pub struct Reader {
    /// The window's end, once a worker has one to send pictures on.
    socket: Option<OwnedFd>,
    /// This process's copy of the worker's end, kept until the process that
    /// inherited it has started; see [`Self::spawned`].
    worker: Option<OwnedFd>,
    /// The newest picture that arrived for each slot, by the generation it was
    /// sent with: a message can arrive before the header says the slot holds
    /// it, so it waits for the take that asks for that generation.
    waiting: [Option<Picture>; SLOTS],
}

impl Reader {
    /// Makes the pair of ends a stream's pictures travel on, keeps the reading
    /// one here, holds the worker's until [`Self::spawned`], and gives back the
    /// number the frames header names so that the process about to be started
    /// finds it.
    ///
    /// Both ends are non-blocking: the window drains what has arrived and goes
    /// on drawing, and a worker with a window that stopped reading loses the
    /// picture rather than the frame.
    pub fn open(&mut self) -> io::Result<RawFd> {
        let (reader, worker) = pair()?;
        nonblocking(reader.as_raw_fd())?;
        let fd = worker.as_raw_fd();
        self.socket = Some(reader);
        self.worker = Some(worker);
        self.waiting = std::array::from_fn(|_| None);
        Ok(fd)
    }

    /// Lets go of this process's copy of the worker's end, once the process
    /// that inherited it is running: from here the reading end sees the socket
    /// close when that worker goes, which is how a stream's pictures end.
    pub fn spawned(&mut self) {
        self.worker = None;
    }

    /// A socket whose two ends are both here, for a test: the window's end in a
    /// [`Reader`] and the worker's in a [`Sender`], with the spawn that would
    /// make them two processes left out. A stream makes them two, since the
    /// worker's end is inherited across its `exec`.
    #[cfg(test)]
    pub fn paired() -> (Self, Sender) {
        let (reader, worker) = pair().expect("a socket");
        nonblocking(reader.as_raw_fd()).expect("a socket that doesn't wait");
        nonblocking(worker.as_raw_fd()).expect("a socket that doesn't wait");
        (
            Self {
                socket: Some(reader),
                worker: None,
                waiting: std::array::from_fn(|_| None),
            },
            Sender {
                socket: Some(worker),
                broken: false,
            },
        )
    }

    /// Takes every message that has arrived, keeping the newest picture of each
    /// slot and closing the ones they displace. A message this build can't read
    /// has its descriptors closed here, and one for a slot the frames file
    /// doesn't have is closed the same way.
    pub fn drain(&mut self) {
        let Some(socket) = self.socket.as_ref() else {
            return;
        };
        for _ in 0..DRAINED {
            match read(socket.as_raw_fd()) {
                Read::Picture(picture) => {
                    let Some(waiting) = self.waiting.get_mut(picture.slot as usize) else {
                        // A slot the frames file has no room for: the picture
                        // and its descriptors go with the message.
                        continue;
                    };
                    // What was waiting there is displaced, and closing it here
                    // is what keeps a window that stopped taking pictures from
                    // holding memory it will never show.
                    *waiting = Some(picture);
                }
                Read::Ignored => continue,
                Read::Nothing => return,
                Read::Gone => {
                    // The worker's end closed, or the socket failed: nothing
                    // more comes this stream, and the frames are pixels.
                    self.socket = None;
                    return;
                }
            }
        }
    }

    /// The picture slot `slot` holds when that is the one numbered
    /// `generation`, and nothing while it is another: a message for a newer
    /// generation is a picture the header hasn't announced, and it waits for
    /// the take that asks for it.
    ///
    /// The picture is the caller's from here, descriptors and all.
    pub fn take(&mut self, slot: u32, generation: u64) -> Option<Picture> {
        let waiting = self.waiting.get_mut(slot as usize)?;
        match waiting {
            Some(picture) if picture.generation == generation => waiting.take(),
            _ => None,
        }
    }
}

/// The worker's end of a stream's socket: it sends one message per picture it
/// publishes, and gives the picture path up once a send fails.
#[derive(Default)]
pub struct Sender {
    socket: Option<OwnedFd>,
    /// Whether a send has failed, which this doesn't come back from: a socket
    /// that closed, or a window that stopped reading, is not worked around at
    /// frame rate — the frames after it are pixels, which need no socket.
    broken: bool,
}

impl Sender {
    /// Takes the worker's end of the socket the frames header names, as this
    /// process inherited it at the spawn, and answers with no socket when the
    /// descriptor there is not this stream's.
    ///
    /// The number alone is not evidence enough: the socket is one this process
    /// inherited, and a header that names a descriptor which is not the
    /// window's socket would have the worker send pictures into another file's
    /// socket. Its type must be the one the window made, and its peer must be
    /// the process that wrote the header — which is the window.
    pub fn adopt(fd: u32, ui: u32) -> Self {
        if fd == 0 {
            return Self::default();
        }
        let fd = fd as RawFd;
        match peer(fd) {
            Some(peer) if peer.pid == ui as libc::pid_t => {}
            _ => return Self::default(),
        }
        if let Err(error) = nonblocking(fd) {
            tracing::warn!(
                error = format!("{error:#}"),
                "the pictures socket stays blocking; this stream's pictures are pixels"
            );
            return Self::default();
        }
        // It was inherited with the flag clear; from here it is this process's
        // alone, and no process this one starts is handed it.
        let _ = cloexec(fd, true);
        Self {
            // SAFETY: the descriptor is one this process inherited at that
            // number and nothing else here holds it.
            socket: Some(unsafe { OwnedFd::from_raw_fd(fd) }),
            broken: false,
        }
    }

    /// Whether this worker has a socket to send pictures on: no socket while
    /// the window made none, or one this process can't be sure is the stream's.
    pub fn present(&self) -> bool {
        self.socket.is_some()
    }

    /// Sends one picture's message, descriptors and all, and says whether it
    /// went: `false` means this worker has no socket, or one it can't send on,
    /// and the frame's pixels are what the window gets.
    ///
    /// The descriptors are the caller's, and stay so: a message that is sent
    /// has the kernel take a reference to each for the process that receives
    /// it, and one that fails — a socket that is gone or full, which a
    /// `SOCK_SEQPACKET` send refuses whole rather than in part — takes no
    /// reference at all, so there is nothing left here to close.
    pub fn send(&mut self, slot: u32, generation: u64, picture: &Descriptor<'_>) -> bool {
        if self.broken {
            return false;
        }
        let Some(socket) = self.socket.as_ref() else {
            return false;
        };
        let planes = picture.planes.len();
        if planes == 0 || planes > MAX_PLANES || picture.fds.len() != planes {
            // A picture this can't describe is a mistake above, and a picture
            // it can't describe is worse than pixels.
            self.broken = true;
            tracing::warn!(
                planes,
                fds = picture.fds.len(),
                "a picture's planes and descriptors don't match; this stream's pictures are pixels"
            );
            return false;
        }
        let message = Message::of(slot, generation, picture, planes);
        let mut control = Control::empty();
        control.header.cmsg_len = Control::length(planes) as _;
        control.header.cmsg_level = libc::SOL_SOCKET;
        control.header.cmsg_type = libc::SCM_RIGHTS;
        control.fds[..planes].copy_from_slice(&picture.fds[..planes]);
        let mut iov = libc::iovec {
            iov_base: (&message as *const Message).cast_mut().cast(),
            iov_len: size_of::<Message>(),
        };
        let mut header: libc::msghdr = unsafe { std::mem::zeroed() };
        header.msg_iov = &mut iov;
        header.msg_iovlen = 1;
        header.msg_control = (&mut control as *mut Control).cast();
        header.msg_controllen = Control::length(planes) as _;
        // SAFETY: the message and the control message are this frame's own
        // memory, and the header describes them as it says it does.
        let sent = unsafe { libc::sendmsg(socket.as_raw_fd(), &header, libc::MSG_NOSIGNAL) };
        if sent == size_of::<Message>() as isize {
            return true;
        }
        // One line for the rest of the stream: a socket that failed once is one
        // every frame after it would fail on, and the log is not the place to
        // say so thirty times a second.
        self.broken = true;
        let error = if sent < 0 {
            io::Error::last_os_error().to_string()
        } else {
            format!("{sent} bytes of {}", size_of::<Message>())
        };
        tracing::warn!(
            error,
            "could not send a picture's descriptors; this stream's pictures are pixels from now on"
        );
        false
    }
}

/// Runs `spawn` with the descriptor `fd` kept open across it, which is what
/// makes a descriptor the worker inherits at the number the frames header names.
///
/// The flag goes back on once `spawn` has returned, so no later child of this
/// process — a browser opening a link, the next stream's worker — is handed
/// this stream's socket. The window between the two calls is a spawn's, and a
/// child started from another thread in it would inherit one more descriptor,
/// which costs that child's process one socket it doesn't use.
pub fn inheriting<T>(fd: Option<RawFd>, spawn: impl FnOnce() -> T) -> T {
    let Some(fd) = fd else {
        return spawn();
    };
    // SAFETY: fcntl on a descriptor this process holds.
    let held = unsafe { libc::fcntl(fd, libc::F_GETFD) };
    if held < 0 {
        return spawn();
    }
    // SAFETY: as above; F_SETFD takes the file descriptor flags.
    unsafe { libc::fcntl(fd, libc::F_SETFD, held & !libc::FD_CLOEXEC) };
    let made = spawn();
    if held & libc::FD_CLOEXEC != 0 {
        // SAFETY: as above.
        unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) };
    }
    made
}

/// What one read of the socket gave.
enum Read {
    /// A picture, with its descriptors taken out of the message.
    Picture(Picture),
    /// A message this build can't use, whose descriptors were closed as it was
    /// read.
    Ignored,
    /// Nothing has arrived, and nothing is waited for.
    Nothing,
    /// The other end is closed, or the socket failed: no picture comes again.
    Gone,
}

fn read(fd: RawFd) -> Read {
    loop {
        let mut message = MaybeUninit::<Message>::uninit();
        let mut control = Control::empty();
        let mut iov = libc::iovec {
            iov_base: message.as_mut_ptr().cast(),
            iov_len: size_of::<Message>(),
        };
        let mut header: libc::msghdr = unsafe { std::mem::zeroed() };
        header.msg_iov = &mut iov;
        header.msg_iovlen = 1;
        header.msg_control = (&mut control as *mut Control).cast();
        header.msg_controllen = size_of::<Control>() as _;
        // SAFETY: the header points at this frame's own message and control
        // buffers, which are as long as it says they are.
        let received =
            unsafe { libc::recvmsg(fd, &mut header, libc::MSG_DONTWAIT | libc::MSG_CMSG_CLOEXEC) };
        if received < 0 {
            return match io::Error::last_os_error().kind() {
                io::ErrorKind::WouldBlock => Read::Nothing,
                io::ErrorKind::Interrupted => continue,
                _ => Read::Gone,
            };
        }
        if received == 0 {
            // A `SOCK_SEQPACKET` socket reads nothing at the end of the socket,
            // which is the worker's end closed.
            return Read::Gone;
        }
        // The descriptors are taken out of the message before anything about
        // the message is judged: a message that can't be used is one whose
        // descriptors are closed here, and never one that keeps them.
        let mut fds: [Option<OwnedFd>; MAX_PLANES] = std::array::from_fn(|_| None);
        let mut carried = 0;
        let controls = header.msg_controllen as usize;
        if controls >= size_of::<libc::cmsghdr>()
            && control.header.cmsg_level == libc::SOL_SOCKET
            && control.header.cmsg_type == libc::SCM_RIGHTS
        {
            // The length the kernel put in the header is what the buffer holds:
            // with a control buffer too small for the message, the descriptors
            // that didn't fit were let go of by the kernel rather than left
            // with this process, and this one is refused below for it.
            let bytes = control
                .header
                .cmsg_len
                .saturating_sub(size_of::<libc::cmsghdr>());
            carried = (bytes / size_of::<RawFd>()).min(MAX_PLANES);
            for (fd, raw) in fds.iter_mut().zip(control.fds[..carried].iter()) {
                // SAFETY: recvmsg put these descriptors in this message, and
                // they are this process's to close from here.
                *fd = Some(unsafe { OwnedFd::from_raw_fd(*raw) });
            }
        }
        let truncated = header.msg_flags & libc::MSG_CTRUNC != 0;
        let whole = received as usize == size_of::<Message>() && !truncated;
        // SAFETY: recvmsg wrote as many bytes as it returned, and the message
        // is used only when that is the whole of it.
        let message = whole.then(|| unsafe { message.assume_init() });
        // A message of another layout, one that lost descriptors, or one whose
        // planes and descriptors don't agree, is no picture: the descriptors
        // taken out of it go with it.
        let Some(message) = message else {
            return Read::Ignored;
        };
        let Some(named) = Message::plane_count(&message).filter(|planes| *planes == carried) else {
            return Read::Ignored;
        };
        let Some(colour) = Colour::read(message.colour) else {
            return Read::Ignored;
        };
        let mut planes: [Option<(Plane, OwnedFd)>; MAX_PLANES] = std::array::from_fn(|_| None);
        for (index, plane) in message.places[..named].iter().enumerate() {
            planes[index] = fds[index].take().map(|fd| (*plane, fd));
        }
        return Read::Picture(Picture {
            slot: u32::from(message.slot),
            generation: message.generation,
            width: message.width,
            height: message.height,
            fourcc: message.fourcc,
            modifier: message.modifier,
            planes,
            colour,
        });
    }
}

/// The two ends of a stream's socket: a sequenced packet socket, which keeps
/// one message one read and lets the peer's end closing be seen.
fn pair() -> io::Result<(OwnedFd, OwnedFd)> {
    let mut fds = [0 as libc::c_int; 2];
    // SAFETY: socketpair writes both descriptors into the array it is given.
    let made = unsafe {
        libc::socketpair(
            libc::AF_UNIX,
            libc::SOCK_SEQPACKET | libc::SOCK_CLOEXEC,
            0,
            fds.as_mut_ptr(),
        )
    };
    if made != 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: the call made both descriptors, and nothing else holds them.
    Ok(unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) })
}

/// What a descriptor is, when it is the reading end's peer: the socket this
/// build makes, and the process on the other end.
fn peer(fd: RawFd) -> Option<libc::ucred> {
    let mut kind: libc::c_int = 0;
    let mut length = size_of::<libc::c_int>() as libc::socklen_t;
    // SAFETY: getsockopt writes an int here, and the length says so.
    let got = unsafe {
        libc::getsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_TYPE,
            (&mut kind as *mut libc::c_int).cast(),
            &mut length,
        )
    };
    if got != 0 || kind != libc::SOCK_SEQPACKET {
        return None;
    }
    let mut credentials = MaybeUninit::<libc::ucred>::uninit();
    let mut length = size_of::<libc::ucred>() as libc::socklen_t;
    // SAFETY: getsockopt writes a `ucred` here, and the length says so.
    let got = unsafe {
        libc::getsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            credentials.as_mut_ptr().cast(),
            &mut length,
        )
    };
    if got != 0 {
        return None;
    }
    // SAFETY: the call succeeded, so it wrote the whole of the structure.
    Some(unsafe { credentials.assume_init() })
}

fn nonblocking(fd: RawFd) -> io::Result<()> {
    // SAFETY: fcntl on a descriptor this process holds.
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 {
        return Err(io::Error::last_os_error());
    }
    if flags & libc::O_NONBLOCK != 0 {
        return Ok(());
    }
    // SAFETY: as above; F_SETFL takes the file status flags.
    let set = unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) };
    if set < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn cloexec(fd: RawFd, on: bool) -> io::Result<()> {
    // SAFETY: fcntl on a descriptor this process holds.
    let held = unsafe { libc::fcntl(fd, libc::F_GETFD) };
    if held < 0 {
        return Err(io::Error::last_os_error());
    }
    let wanted = if on {
        held | libc::FD_CLOEXEC
    } else {
        held & !libc::FD_CLOEXEC
    };
    // SAFETY: as above; F_SETFD takes the file descriptor flags.
    if unsafe { libc::fcntl(fd, libc::F_SETFD, wanted) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{ffi::CString, path::PathBuf};

    /// A memfd named for one test, which is the "picture" these tests share: a
    /// plain descriptor nothing about it is a GPU's, and a name that says how
    /// many descriptors of this process still name it.
    fn memory(name: &str) -> OwnedFd {
        let name = CString::new(name).unwrap();
        // SAFETY: the name is this test's own C string, and no flags ask for
        // anything of the descriptor but a fresh anonymous one.
        let fd = unsafe { libc::memfd_create(name.as_ptr(), libc::MFD_CLOEXEC) };
        assert!(fd >= 0, "{}", io::Error::last_os_error());
        // SAFETY: the call made the descriptor and nothing else holds it.
        unsafe { OwnedFd::from_raw_fd(fd) }
    }

    /// How many descriptors of this process name the memfd `name`: one per
    /// process that holds one, which is what says a descriptor was transferred
    /// rather than copied, and closed rather than leaked.
    fn holding(name: &str) -> usize {
        let Ok(entries) = std::fs::read_dir("/proc/self/fd") else {
            panic!("the open descriptors are listed");
        };
        entries
            .filter_map(Result::ok)
            .filter(|entry| {
                std::fs::read_link(PathBuf::from("/proc/self/fd").join(entry.file_name()))
                    .is_ok_and(|target| target.to_string_lossy().contains(name))
            })
            .count()
    }

    fn descriptor<'a>(planes: &'a [Plane], fds: &'a [RawFd]) -> Descriptor<'a> {
        Descriptor {
            width: 640,
            height: 360,
            fourcc: u32::from_le_bytes(*b"XR24"),
            modifier: 0,
            planes,
            colour: Colour::SDR,
            fds,
        }
    }

    /// A picture's descriptor travels as itself — the same memory, a descriptor
    /// of this process's own — and is closed exactly once on each side: what
    /// the window received is closed when it lets go of the picture, and what
    /// the worker sent is closed when the worker lets go of its own.
    #[test]
    fn a_pictures_descriptor_travels_and_is_closed_once_on_each_side() {
        let name = "yapfast-test-dmabuf-travels";
        let (mut reader, mut sender) = Reader::paired();
        let memory = memory(name);
        assert_eq!(holding(name), 1, "the worker's own");
        let planes = [Plane {
            offset: 0,
            stride: 640 * 4,
        }];
        let own = [memory.as_raw_fd()];
        let sent = descriptor(&planes, &own);
        assert!(sender.send(0, 7, &sent), "one message, one sendmsg");
        // The sender keeps its own descriptor, and the message made one for the
        // process it went to.
        assert_eq!(holding(name), 1, "still the worker's alone");

        reader.drain();
        assert_eq!(holding(name), 2, "the window has its own now");
        let picture = reader.take(0, 7).expect("the picture that arrived");
        assert_eq!((picture.width(), picture.height()), (640, 360));
        assert_eq!(picture.fourcc(), u32::from_le_bytes(*b"XR24"));
        assert_eq!(picture.plane_count(), 1);
        let planes: Vec<_> = picture.planes().collect();
        assert_eq!(planes.len(), 1, "one plane, one descriptor");
        assert_eq!(
            planes[0].0,
            Plane {
                offset: 0,
                stride: 2560
            }
        );
        assert_ne!(planes[0].1, memory.as_raw_fd(), "a descriptor of its own");
        // The two name the same memory: one object, two descriptors.
        let inode = |fd: RawFd| {
            // SAFETY: fstat writes a stat here for a descriptor this test holds.
            let mut stat = MaybeUninit::<libc::stat>::uninit();
            assert_eq!(unsafe { libc::fstat(fd, stat.as_mut_ptr()) }, 0);
            unsafe { stat.assume_init() }.st_ino
        };
        assert_eq!(inode(planes[0].1), inode(memory.as_raw_fd()));

        // The window's copy goes with the picture, and only it: the worker's is
        // still the worker's until it lets go, which is once.
        drop(picture);
        assert_eq!(holding(name), 1, "the window let go of its own");
        drop(memory);
        assert_eq!(holding(name), 0, "and the worker its own");
    }

    /// Taking a picture is the only way to hold one: a message whose slot is
    /// taken again is closed as it is displaced, so a window that stopped
    /// taking pictures isn't holding every one a worker published.
    #[test]
    fn a_picture_a_newer_one_replaces_is_closed() {
        let name = "yapfast-test-dmabuf-displaced";
        let (mut reader, mut sender) = Reader::paired();
        let memory = memory(name);
        let planes = [Plane {
            offset: 0,
            stride: 4,
        }];
        let own = [memory.as_raw_fd()];
        let sent = descriptor(&planes, &own);
        assert!(sender.send(0, 1, &sent));
        reader.drain();
        assert_eq!(holding(name), 2);
        assert!(sender.send(0, 2, &sent), "the same slot, a newer picture");
        reader.drain();
        assert_eq!(holding(name), 2, "the first was closed as it was displaced");
        // The picture waiting is the newer one, and the older generation is
        // nobody's to take.
        assert!(reader.take(0, 1).is_none(), "the older picture");
        let picture = reader.take(0, 2).expect("the newer picture");
        drop(picture);
        drop(memory);
        assert_eq!(holding(name), 0);
    }

    /// The colour signal a picture was coded with travels with it, which is what
    /// lets the window show a 10-bit picture in its own matrix and curve; a
    /// message whose colour names something this build has no reading for is no
    /// picture at all.
    #[test]
    fn a_pictures_colour_travels_with_it() {
        let name = "yapfast-test-dmabuf-colour";
        let (mut reader, mut sender) = Reader::paired();
        let memory = memory(name);
        let planes = [Plane {
            offset: 0,
            stride: 8,
        }];
        let own = [memory.as_raw_fd()];
        let sent = Colour {
            range: Range::Full,
            matrix: Matrix::Bt2020Ncl,
            transfer: Transfer::Pq,
        };
        let picture = Descriptor {
            colour: sent,
            ..descriptor(&planes, &own)
        };
        assert!(sender.send(0, 3, &picture), "one message, one sendmsg");
        reader.drain();
        let taken = reader.take(0, 3).expect("the picture that arrived");
        assert_eq!(
            taken.colour(),
            sent,
            "the range, matrix and curve it was sent with"
        );

        // Three bytes of anything else are a message of another layout.
        assert!(Colour::read([0, 0, 0]).is_some(), "the 8-bit answer");
        assert!(Colour::read([1, 1, 1]).is_some(), "full range, BT.2020, PQ");
        assert!(Colour::read([2, 0, 0]).is_none(), "no third range");
        assert!(Colour::read([0, 2, 0]).is_none(), "no third matrix");
        assert!(Colour::read([0, 0, 2]).is_none(), "no third curve");
    }

    /// A message this build can't read is not a picture: its descriptors are
    /// closed rather than taken for one, and the next message is read as ever.
    #[test]
    fn a_message_from_another_version_is_refused_and_closed() {
        let name = "yapfast-test-dmabuf-version";
        let (reader, mut sender) = Reader::paired();
        let memory = memory(name);
        let planes = [Plane {
            offset: 0,
            stride: 4,
        }];
        let own = [memory.as_raw_fd()];
        let sent = descriptor(&planes, &own);
        assert!(sender.send(0, 1, &sent));

        // A message of another version, written straight to the socket: the
        // reader has no socket of its own to send on, so the test writes one
        // it made with the same ends.
        let (ours, theirs) = pair().unwrap();
        let mut wrong = Message::of(0, 1, &sent, 1);
        wrong.version = VERSION + 1;
        let mut control = Control::empty();
        control.header.cmsg_len = Control::length(1) as _;
        control.header.cmsg_level = libc::SOL_SOCKET;
        control.header.cmsg_type = libc::SCM_RIGHTS;
        control.fds[0] = memory.as_raw_fd();
        let mut iov = libc::iovec {
            iov_base: (&wrong as *const Message).cast_mut().cast(),
            iov_len: size_of::<Message>(),
        };
        let mut header: libc::msghdr = unsafe { std::mem::zeroed() };
        header.msg_iov = &mut iov;
        header.msg_iovlen = 1;
        header.msg_control = (&mut control as *mut Control).cast();
        header.msg_controllen = Control::length(1) as _;
        // SAFETY: the message and its control are this test's own memory.
        let wrote = unsafe { libc::sendmsg(ours.as_raw_fd(), &header, libc::MSG_NOSIGNAL) };
        assert_eq!(wrote, size_of::<Message>() as isize);

        // The reader takes the socket, as a worker's end, and drains it: the
        // message is refused and its descriptor closed, so the count of what
        // names the memory is the worker's own and the one it sent.
        let mut reader_end = Reader {
            socket: Some(theirs),
            ..Reader::default()
        };
        reader_end.drain();
        assert!(
            reader_end.take(0, 1).is_none(),
            "not a picture of this build"
        );
        assert_eq!(holding(name), 1, "the refused message's descriptor closed");

        drop((ours, reader_end));
        drop(memory);
        assert_eq!(holding(name), 0);
        let _ = sender;
        let _ = reader;
    }

    /// A socket the worker has no end of sends nothing, and says so: a stream
    /// whose window made no socket is a stream of pixels.
    #[test]
    fn a_worker_without_a_socket_sends_nothing() {
        let mut sender = Sender::default();
        assert!(!sender.present());
        let planes = [Plane {
            offset: 0,
            stride: 4,
        }];
        let memory = memory("yapfast-test-dmabuf-no-socket");
        let own = [memory.as_raw_fd()];
        assert!(!sender.send(0, 1, &descriptor(&planes, &own)));
        // A descriptor the window never got is one the worker still has, and
        // closing it is the worker's own business.
        assert_eq!(holding("yapfast-test-dmabuf-no-socket"), 1);
    }

    /// The header names a descriptor, and nothing but the window's socket at
    /// the other end of it is taken for the stream's: a number that names
    /// something else is no socket at all, and the worker writes pixels.
    #[test]
    fn a_descriptor_that_is_not_the_streams_socket_is_refused() {
        let file = tempfile::tempfile().unwrap();
        let adopted = Sender::adopt(file.as_raw_fd() as u32, std::process::id());
        assert!(!adopted.present(), "a file is not the window's socket");

        // The window's own socket, but not the process that wrote the header.
        let mut reader = Reader::default();
        let fd = reader.open().unwrap();
        let adopted = Sender::adopt(fd as u32, u32::MAX);
        assert!(!adopted.present(), "another process's socket");
    }
}
