//! A camera on Linux as a DMA-BUF the card can read: V4L2's own capture, for a
//! device that films NV12 and exports what it captures.
//!
//! The path is narrow, and worth naming first. A camera's frames reach an
//! encoder with no pixel touched by the processor only where three things hold:
//! the device captures single-planar NV12, its driver answers `VIDIOC_EXPBUF` so
//! the buffer can be handed on as a file descriptor, and the encoder that
//! opened reads a DMA-BUF — `encode.rs`'s `Refused::Uploads`, which only a
//! VA-API encoder doesn't answer. Everything else films through libwebrtc
//! (`native/camera.cc`) and its picture is assembled by the processor, which is
//! what this module declines to be: a camera it won't take is an error naming
//! why, and a caller logs that and opens the processor's capturer instead.
//!
//! **A webcam that can't export is folklore.** UVC captures through
//! `vb2_vmalloc_memops`, whose `get_dmabuf` is `dma_buf_export` like any other
//! vb2 exporter, and `uvcvideo` routes `VIDIOC_EXPBUF`. What closes the path on
//! the cameras to hand is the format: a UVC camera that offers only YUYV or MJPEG
//! has nothing NV12 to export, and those formats cost the processor exactly what
//! they cost there. `choose` weighs them in the order that costs least, and
//! `negotiate` is where a device with no NV12 is refused rather than filmed in
//! them.
//!
//! **The buffer is described, not read.** What a frame hands on is the driver's
//! own buffer: NV12 at the pitch `VIDIOC_S_FMT` answered, its chroma plane at
//! that pitch times the height — the pitch and not the width, since a padded row
//! pads the chroma with it — both in one descriptor, with
//! `DRM_FORMAT_MOD_LINEAR` for a modifier. Nothing in `videodev2.h` answers what
//! modifier a buffer was laid out with, so that zero is a claim about the driver,
//! and it is sound for the three exporters there are — vb2's vmalloc, dma-sg and
//! dma-contig export plain pages — and is kept sound by the formats accepted
//! here: a driver that tiles its frames names the tiling in the fourcc, and the
//! tiled spellings of NV12 are not asked for. Its consequence is the only check
//! there is: an import that fails is a picture for the processor, and an import
//! that succeeds and looks wrong is the one failure nothing here can see.
//!
//! **Nothing fences an importer against the device.** An exported vb2 buffer
//! carries no reservation object, so the handshake is the queue itself: the
//! driver does not write a buffer between `VIDIOC_DQBUF` and `VIDIOC_QBUF`, and a
//! buffer is given back when the next frame is asked for, after the picture it
//! held has been encoded. Queuing a buffer twice, or never giving one back,
//! starves the queue and shows up as a camera that stopped rather than as an
//! error — so a caller must ask for the next frame before it lets go of the
//! picture it has.
//!
//! The size asked for is the encoder's, [`camera::WIDTH`] by [`camera::HEIGHT`]
//! at `camera::FPS`: a driver may answer `VIDIOC_S_FMT` with another size, and
//! that picture is the video processor's to scale where the build has one.
//! Timestamps and rate are the device's: `v4l2_buffer.timestamp` is
//! `CLOCK_MONOTONIC` where the driver says so, which is the clock a call's
//! pictures are timed on, and the rate is whatever the driver delivers.
//!
//! It is `camera.rs`'s `card` module that opens it, for a camera filmed at the
//! size a call shares: [`Capture::open`] is the entry point and
//! [`Capture::frame`] the loop the thread it films on runs, handing
//! [`Picture::descriptor`] to the encoder where the module would have handed I420
//! to `native/camera.cc`. An open that fails is the processor's: the caller logs
//! the error and opens the processor's capturer for that camera instead. Each
//! open is one camera for as long as the call films — deciding that a device can
//! export costs a real `VIDIOC_EXPBUF`, a few hundred milliseconds, and what it
//! answered is kept for the stream's life rather than asked again.
use std::fmt::Write as _;
use std::io;
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, FromRawFd, OwnedFd, RawFd};
use std::path::{Path, PathBuf};
use std::ptr::NonNull;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};

use crate::camera;
use crate::dmabuf::{Descriptor, Plane};

/// The size and rate a camera shares at, which is the encoder's: another size is
/// the video processor's to scale, and never this module's.
const AT: (u32, u32, u32) = (camera::WIDTH, camera::HEIGHT, camera::FPS);

/// DRM's fourcc for NV12, as a descriptor names its format, and V4L2's
/// `V4L2_PIX_FMT_NV12`: one luma plane and then an interleaved chroma plane, the
/// shape a VA-API surface is made of without converting a pixel.
const NV12: u32 = u32::from_le_bytes(*b"NV12");

/// The other formats a camera can be read in, as `videodev2.h` spells them: 4:2:0
/// planar in both chroma orders, 4:2:2 packed, and JPEG. They are what a device
/// with no NV12 offers, and what [`Rung`] weighs.
const YUV420: u32 = u32::from_le_bytes(*b"YU12");
const YV12: u32 = u32::from_le_bytes(*b"YV12");
const YUYV: u32 = u32::from_le_bytes(*b"YUYV");
const MJPEG: u32 = u32::from_le_bytes(*b"MJPG");

/// `DRM_FORMAT_MOD_LINEAR`: rows one after another, `stride` bytes apart. It is
/// the one layout V4L2's single-planar buffers have, and the one this descriptor
/// claims — the claim's evidence is the formats accepted, not an ioctl.
const LINEAR: u64 = 0;

/// `V4L2_BUF_TYPE_VIDEO_CAPTURE`: a device filming into a single-plane queue,
/// which is the only kind whose NV12 is one buffer.
const CAPTURE: u32 = 1;

/// `V4L2_MEMORY_MMAP`: the driver's own buffers. `V4L2_MEMORY_DMABUF` is the
/// direction a caller fills, and a queue allocated that way has nothing to
/// export.
const MMAP: u32 = 1;

/// How many buffers are asked for: one being filled, one queued behind it, one
/// with the encoder, one spare, so a slow encode never leaves the device with
/// nowhere to film. Two is the fewest taken from a driver that grants less.
const BUFFERS: u32 = 4;

/// `V4L2_CAP_*`: streaming, a single-planar capture queue, one that captures in
/// planes, the bits saying `device_caps` is the node's own, and — in
/// `v4l2_captureparm.capability` rather than here — a frame rate the device
/// takes.
const CAP_STREAMING: u32 = 0x0400_0000;
const CAP_CAPTURE: u32 = 0x0000_0001;
const CAP_CAPTURE_MPLANE: u32 = 0x0000_1000;
const CAP_DEVICE_CAPS: u32 = 0x8000_0000;
const CAP_TIMEPERFRAME: u32 = 0x0000_1000;

/// `V4L2_FIELD_NONE`: a camera's frames are whole, never one field of a pair.
const FIELD_NONE: u32 = 1;

/// `V4L2_BUF_FLAG_*`: a frame the driver marked as broken, and the bits that say
/// which clock it stamped frames in.
const BUF_ERROR: u32 = 0x0000_0040;
const BUF_TIMESTAMP_MASK: u32 = 0x0000_e000;
const BUF_TIMESTAMP_MONOTONIC: u32 = 0x0000_2000;

/// `V4L2_FMT_FLAG_COMPRESSED`: the frames in a format are compressed, which is
/// MJPEG and nothing else here.
const FMT_COMPRESSED: u32 = 0x0000_0001;

/// `V4L2_FRMSIZE_TYPE_DISCRETE` and `_STEPWISE`, which `V4L2_FRMIVAL_TYPE_*`
/// spell the same way: one size or interval for an index, or the bounds and step
/// of all of them.
const LISTED: u32 = 1;
const STEPWISE: u32 = 3;

/// The `_IOWR('V', …)` numbers `videodev2.h` defines, as the kernel answers them.
/// A number carries the size of the C struct it is asked with, so each of these
/// and the layout below it are one fact.
const QUERYCAP: u32 = 0x8068_5600;
const ENUM_FMT: u32 = 0xc040_5602;
const ENUM_FRAMESIZES: u32 = 0xc02c_564a;
const ENUM_FRAMEINTERVALS: u32 = 0xc034_564b;
const TRY_FMT: u32 = 0xc0d0_5640;
const S_FMT: u32 = 0xc0d0_5605;
const G_PARM: u32 = 0xc0cc_5615;
const S_PARM: u32 = 0xc0cc_5616;
const REQBUFS: u32 = 0xc014_5608;
const QUERYBUF: u32 = 0xc058_5609;
const EXPBUF: u32 = 0xc040_5610;
const QBUF: u32 = 0xc058_560f;
const DQBUF: u32 = 0xc058_5611;
const STREAMON: u32 = 0x4004_5612;
const STREAMOFF: u32 = 0x4004_5613;

/// The formats this module asks a camera for, cheapest for the processor first.
///
/// The order is the whole of the cost question: NV12 is the one a surface is made
/// of, 4:2:0 planar needs no colour conversion at all — only the row copy an
/// encoder makes when it fills planes of its own — 4:2:2 packed costs a
/// subsample, and JPEG costs a decode and then a conversion, and is the frame
/// that arrives latest.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Rung {
    Nv12,
    Planar,
    Packed,
    Jpeg,
}

impl Rung {
    /// The rung a fourcc belongs to, or none for a format a camera's pictures are
    /// never read in — H.264 passthrough, and the tiled spellings of NV12, which
    /// a descriptor claiming a linear layout must never name.
    fn of(fourcc: u32) -> Option<Self> {
        match fourcc {
            NV12 => Some(Self::Nv12),
            YUV420 | YV12 => Some(Self::Planar),
            YUYV => Some(Self::Packed),
            MJPEG => Some(Self::Jpeg),
            _ => None,
        }
    }

    /// What a frame in this rung costs the processor, smallest first.
    fn cost(self) -> u8 {
        match self {
            Self::Nv12 => 0,
            Self::Planar => 1,
            Self::Packed => 2,
            Self::Jpeg => 3,
        }
    }
}

/// One format a device enumerated, as `VIDIOC_ENUM_FMT` and the enumerations
/// after it answered: what the frames are, in the driver's own words, and whether
/// the format delivers the size and rate a call films at.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Offered {
    /// V4L2's fourcc.
    fourcc: u32,
    /// The driver's description of it, empty where it gave none.
    description: String,
    /// Whether its frames are compressed, which is why the rung that reads them
    /// comes last.
    compressed: bool,
    /// Whether the device says it delivers [`AT`], which is what puts a rung
    /// ahead of a cheaper one that cannot.
    delivers: bool,
}

impl Offered {
    /// What to call this format: the driver's description, or its fourcc where
    /// the driver gave none.
    fn name(&self) -> String {
        if self.description.is_empty() {
            let fourcc = self.fourcc.to_le_bytes();
            String::from_utf8_lossy(&fourcc).into_owned()
        } else {
            self.description.clone()
        }
    }
}

/// The rung a device is asked for, and V4L2's fourcc of it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Chosen {
    fourcc: u32,
    rung: Rung,
}

/// The format a device is asked for first: the cheapest [`Rung`] that delivers
/// the size and rate a call films at, and, where no rung does, the cheapest that
/// delivers any picture at all — a device that films 1080p where the call wants
/// 720p is a picture the video processor scales, not one to refuse.
///
/// The size comes before the cost because a rung that cannot deliver is a rung a
/// slower-looking one beats: a camera may offer 720p30 in MJPEG alone, and asking
/// it for YUYV would film at whatever rate the bus had left. Nothing here is a
/// device's, so it is checked without one: [`Offered::delivers`] is what the
/// device answered about [`AT`] when it was enumerated.
fn choose(offered: &[Offered]) -> Result<Chosen> {
    let candidates = offered.iter().enumerate().filter_map(|(index, one)| {
        let rung = Rung::of(one.fourcc)?;
        // The tier first, so that every rung which delivers beats every rung that
        // doesn't, and the cost second, within a tier.
        Some((u8::from(!one.delivers), rung.cost(), index, rung))
    });
    let Some((_, _, index, rung)) = candidates.min() else {
        bail!(
            "a camera is read in NV12, YUV420, YV12, YUYV or MJPEG, and this one enumerated {}",
            listing(offered)
        );
    };
    Ok(Chosen {
        fourcc: offered[index].fourcc,
        rung,
    })
}

/// The format a device at `path` is asked for, from what it enumerated, when that
/// is one this module can hand on — and otherwise the words a caller logs before
/// it opens the processor's capturer.
fn negotiate(path: &Path, offered: &[Offered]) -> Result<u32> {
    let chosen = match choose(offered) {
        Ok(chosen) => chosen,
        Err(why) => bail!("{}: {why}", path.display()),
    };
    if chosen.rung != Rung::Nv12 {
        bail!(
            "{} films {} and not NV12: a camera's frames reach the card in NV12 alone, so this \
             device's pictures stay with the processor",
            path.display(),
            listing(offered)
        );
    }
    Ok(chosen.fourcc)
}

/// What a device enumerated, for a refusal to name: "MJPEG (compressed) and YUYV
/// 4:2:2", in the order the device listed them.
fn listing(offered: &[Offered]) -> String {
    if offered.is_empty() {
        return "nothing at all".to_owned();
    }
    let mut listed = String::new();
    for (position, one) in offered.iter().enumerate() {
        if position > 0 {
            let last = position + 1 == offered.len();
            listed.push_str(if last { " and " } else { ", " });
        }
        listed.push_str(&one.name());
        if one.compressed {
            listed.push_str(" (compressed)");
        }
    }
    listed
}

/// What the device films in, as `VIDIOC_S_FMT` answered it: the numbers a picture
/// is described with, which a driver is entitled to make different from the ones
/// it was asked for and which are therefore never assumed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Format {
    width: u32,
    height: u32,
    /// The bytes between one row and the next, which is the luma plane's pitch
    /// and, in single-planar NV12, the chroma plane's too.
    stride: u32,
}

impl Format {
    /// What the device will film, from the answer `VIDIOC_S_FMT` gave, when that
    /// answer is a picture a descriptor can name.
    fn of(pix: V4l2PixFormat) -> Result<Self> {
        let V4l2PixFormat {
            width,
            height,
            pixelformat,
            bytesperline: stride,
            sizeimage,
            ..
        } = pix;
        if pixelformat != NV12 {
            let fourcc = pixelformat.to_le_bytes();
            bail!(
                "the device answered {} instead of NV12",
                String::from_utf8_lossy(&fourcc)
            );
        }
        if width == 0 || height == 0 {
            bail!("the device answered an empty picture, {width}x{height}");
        }
        // NV12's chroma is subsampled by two in each direction, so a picture with
        // an odd side has no chroma plane that arithmetic can place.
        if width % 2 != 0 || height % 2 != 0 {
            bail!("the device answered {width}x{height}, and NV12's chroma is subsampled by two");
        }
        if stride < width {
            bail!("the device answered a pitch of {stride} bytes for a row of {width} pixels");
        }
        let format = Self {
            width,
            height,
            stride,
        };
        let needed = format.needed();
        if needed > u64::from(u32::MAX) {
            bail!(
                "a {width}x{height} NV12 picture is {needed} bytes, which a descriptor's plane \
                 offsets cannot name"
            );
        }
        // A driver that pads the picture past its height puts the chroma plane
        // somewhere nothing here can find: V4L2 never says how far it padded, and
        // a descriptor that placed the chroma at pitch times the height would name
        // the wrong bytes for a picture that still looked almost right.
        if u64::from(sizeimage) != needed {
            bail!(
                "the device's buffers hold {sizeimage} bytes for a picture of {needed}: its rows \
                 and its chroma are not the pitch times the height, so a descriptor cannot name \
                 where they are"
            );
        }
        Ok(format)
    }

    /// How much of a buffer the picture fills: NV12's rows at the pitch, and then
    /// its chroma rows, half as many, at the same pitch. It is the size
    /// `encode.rs` works out from the same two planes.
    fn needed(&self) -> u64 {
        let rows = u64::from(self.height) + u64::from(self.height.div_ceil(2));
        u64::from(self.stride) * rows
    }

    /// Where the chroma plane starts, which is the luma plane's size in bytes.
    fn chroma(&self) -> u32 {
        // Checked against a buffer's own length when the buffers were exported,
        // and against [`Self::needed`] when the format was read.
        self.stride * self.height
    }
}

/// One frame, where the driver put it: which of the device's buffers holds it, and
/// what the card reads it as.
///
/// It is a view of the driver's memory and not a copy of it, so it borrows the
/// capture: the buffer is the device's again from the next [`Capture::frame`] on,
/// and the descriptor this describes is valid until then.
#[derive(Debug)]
pub struct Picture {
    width: u32,
    height: u32,
    /// Each plane's first byte and the bytes between its rows: luma, then chroma
    /// at the same pitch.
    planes: [Plane; 2],
    /// The one buffer both planes are in, named once for each plane — a surface
    /// is made of one buffer, and a picture in two is one no surface can be.
    fds: [RawFd; 2],
    /// Which of the device's buffers this is, which is how it is given back.
    index: u32,
    /// The device's own stamp for the frame, where the driver stamps in
    /// `CLOCK_MONOTONIC` — the clock a call times its pictures on — and `None`
    /// where it stamps on arrival or in a clock nothing here knows.
    stamp: Option<Duration>,
    /// The number the device gave the frame. A gap is a frame the driver dropped,
    /// rather than one that has not arrived.
    sequence: u32,
}

impl Picture {
    /// The picture as the encoder reads it: NV12, linear, two planes in one
    /// buffer at the device's pitch.
    pub fn descriptor(&self) -> Descriptor<'_> {
        Descriptor {
            width: self.width,
            height: self.height,
            fourcc: NV12,
            modifier: LINEAR,
            planes: &self.planes,
            fds: &self.fds,
        }
    }

    /// The picture's size in pixels, which is the device's answer and not
    /// necessarily the size a call asked for.
    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// The bytes between one row and the next.
    pub fn stride(&self) -> u32 {
        self.planes[0].stride
    }

    /// The device's stamp for the frame, where its clock is `CLOCK_MONOTONIC`.
    pub fn stamp(&self) -> Option<Duration> {
        self.stamp
    }

    /// The number the device gave the frame.
    pub fn sequence(&self) -> u32 {
        self.sequence
    }

    /// The picture a frame in `format`'s buffers holds, from the answer the
    /// driver gave when it handed the buffer over.
    fn of(format: &Format, index: u32, fd: RawFd, filled: &V4l2Buffer) -> Self {
        Self {
            width: format.width,
            height: format.height,
            planes: [
                Plane {
                    offset: 0,
                    stride: format.stride,
                },
                Plane {
                    offset: format.chroma(),
                    stride: format.stride,
                },
            ],
            fds: [fd, fd],
            index,
            stamp: stamp(filled),
            sequence: filled.sequence,
        }
    }
}

/// The clock a frame was stamped on, where the driver says it is the monotonic
/// one, as `CLOCK_MONOTONIC` reads: the seconds and nanoseconds since boot.
fn stamp(filled: &V4l2Buffer) -> Option<Duration> {
    if filled.flags & BUF_TIMESTAMP_MASK != BUF_TIMESTAMP_MONOTONIC {
        return None;
    }
    let seconds = u64::try_from(filled.timestamp.tv_sec).ok()?;
    let nanos = u32::try_from(filled.timestamp.tv_usec)
        .ok()?
        .checked_mul(1_000)?;
    Some(Duration::new(seconds, nanos))
}

/// What asking a [`Capture`] for its next frame found.
///
/// None of these is a failure: a camera films at its own rate, and a device that
/// has gone is the end of the capture rather than a bug in it.
#[derive(Debug)]
pub enum Arrival {
    /// A picture arrived, and it is [`Capture::picture`]'s. The buffer it is in is
    /// given back to the driver when the next frame is asked for, so a caller
    /// encodes it before then.
    Frame,
    /// Nothing arrived before the timeout passed. The picture handed over last
    /// time has already been given back to the driver, which may be filming into
    /// that buffer again, so a caller may sleep and ask again: this is the
    /// camera's own rate.
    Still,
    /// The device stopped delivering: it was unplugged, or taken by another
    /// opener. Nothing more comes from this capture, and [`Capture::ending`] says
    /// what the driver said.
    Closed,
}

/// A camera filming into its own memory, handing out each frame as the DMA-BUF
/// the driver captured into.
///
/// It owns the node it opened and the exported descriptors of every buffer, and
/// lets go of all of them when it is dropped: the stream stops, the buffers go
/// back to the driver, and an interface opened in between is free to film.
pub struct Capture {
    node: OwnedFd,
    path: PathBuf,
    /// What the device films at, as `VIDIOC_S_FMT` answered.
    format: Format,
    /// The formats the device enumerated, which is what [`Capture::describe`]
    /// reports and what a picture was chosen from.
    offered: Vec<Offered>,
    /// The driver's buffers, exported and mapped. They are the queue's and live
    /// as long as the queue does, which is what makes a descriptor one field of
    /// the picture rather than something made per frame.
    buffers: Vec<Exported>,
    /// The buffer the current picture is in, which is the driver's again as soon
    /// as it is queued.
    held: Option<u32>,
    picture: Option<Picture>,
    streaming: bool,
    /// Frames the driver delivered whole and this dropped, for the one line
    /// [`Capture::describe`] logs.
    dropped: u64,
    /// Why the capture ended, in the words of the [`Arrival::Closed`] it answered,
    /// and empty while it still runs.
    ending: String,
}

impl Capture {
    /// Opens a camera at `path` and starts it filming: negotiates NV12 at the size
    /// a call shares, asks the driver for its own buffers, exports each as a
    /// DMA-BUF, and starts the stream.
    ///
    /// Everything that can refuse does so here, before a frame: a node that is not
    /// a camera, a device that cannot stream or that captures only in planes, one
    /// that films in none of the formats a camera is read in, one with no NV12, a
    /// driver whose buffers this can't export, and a picture a descriptor cannot
    /// name are each an error saying what it looked at. A caller that gets one
    /// **opens the processor's capturer instead**; there is nothing here to retry.
    ///
    /// A capture that fails to open leaves nothing behind: the node is closed,
    /// which releases whatever the driver had granted.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        // Read-write because a capture's ioctls are all on the queue, and because
        // the buffers are the driver's to be given; `O_CLOEXEC` is what Rust opens
        // with, and what the exported descriptors are asked for.
        let node = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .with_context(|| format!("opening the camera at {}", path.display()))?;
        let capability = V4l2Capability::ask(node.as_fd())
            .with_context(|| format!("asking {} what it can do", path.display()))?;
        let abilities = capability.abilities();
        if abilities & CAP_STREAMING == 0 {
            bail!(
                "{} has no V4L2_CAP_STREAMING, so it has no queue of buffers to film into",
                path.display()
            );
        }
        // A device that captures only in planes hands out a descriptor per plane,
        // and a picture in two buffers is one no surface can be made of; a UVC
        // camera offers both queues and takes this one.
        if abilities & CAP_CAPTURE == 0 {
            bail!(
                "{} has no single-planar capture queue{}, and a camera's NV12 is filmed through \
                 one: its two planes are one buffer, which is what a descriptor names",
                path.display(),
                if abilities & CAP_CAPTURE_MPLANE != 0 {
                    ", only one that captures in planes"
                } else {
                    ""
                }
            );
        }
        let offered = offered(node.as_fd(), path)?;
        let fourcc = negotiate(path, &offered)?;
        let format = film(node.as_fd(), path, fourcc)?;
        pace(node.as_fd());
        let buffers =
            exported(node.as_fd(), &format).with_context(|| format!("from {}", path.display()))?;
        let mut capture = Self {
            node: node.into(),
            path: path.to_path_buf(),
            format,
            offered,
            buffers,
            held: None,
            picture: None,
            streaming: false,
            dropped: 0,
            ending: String::new(),
        };
        capture.start()?;
        Ok(capture)
    }

    /// Asks for the next frame: [`Arrival::Frame`] when the device delivered one,
    /// [`Arrival::Still`] when `timeout` passed with none, and
    /// [`Arrival::Closed`] when the device stopped delivering at all.
    ///
    /// The picture handed over last time is given back to the driver before
    /// anything else, since the device films into that buffer only once it is the
    /// driver's again. A frame the driver marked as broken, or one too short to
    /// hold the picture, is given straight back and counted rather than handed on:
    /// a broken frame encoded is a picture the far end keeps for a second.
    ///
    /// `timeout` is how long a camera that has nothing to say is waited for;
    /// [`Duration::ZERO`] looks once and returns.
    pub fn frame(&mut self, timeout: Duration) -> Result<Arrival> {
        if !self.ending.is_empty() {
            return Ok(Arrival::Closed);
        }
        self.give_back();
        let deadline = Instant::now() + timeout;
        loop {
            match filled(self.node.as_fd(), deadline) {
                Ok(true) => {}
                Ok(false) => return Ok(Arrival::Still),
                Err(why) => return self.stopped(why),
            }
            let mut dequeued = V4l2Buffer::of(0);
            match ask(self.node.as_fd(), DQBUF, &mut dequeued) {
                Ok(()) => {}
                // The wait was for a frame the driver has not got: an error it
                // reports as an error is read on the next pass, and answering a
                // still frame here keeps a poll that was woken by one from
                // becoming the end of the capture.
                Err(why) if why.kind() == io::ErrorKind::WouldBlock => return Ok(Arrival::Still),
                Err(why) => return self.stopped(why),
            }
            if dequeued.flags & BUF_ERROR != 0
                || u64::from(dequeued.bytesused) < self.format.needed()
            {
                self.dropped += 1;
                // A buffer the driver won't take back has left the queue, and a
                // queue that lost one stops filming: the capture ends with the
                // driver's words here rather than going quiet.
                if let Err(why) = self.queue(dequeued.index) {
                    self.stalled(dequeued.index, why);
                    return Ok(Arrival::Closed);
                }
                if Instant::now() >= deadline {
                    return Ok(Arrival::Still);
                }
                continue;
            }
            let Some(buffer) = self.buffers.get(dequeued.index as usize) else {
                bail!(
                    "the camera answered a frame from buffer {}, which it was never given",
                    dequeued.index
                );
            };
            let fd = buffer.fd.as_raw_fd();
            self.picture = Some(Picture::of(&self.format, dequeued.index, fd, &dequeued));
            self.held = Some(dequeued.index);
            return Ok(Arrival::Frame);
        }
    }

    /// The picture that arrived with the last [`Arrival::Frame`], and nothing
    /// after an [`Arrival::Still`], since the buffer went back to the driver.
    ///
    /// The descriptor [`Picture::descriptor`] describes is the driver's own
    /// buffer, not a copy of it: it is valid for as long as this picture is held,
    /// and the buffer is the driver's again at the start of the next
    /// [`Capture::frame`], which gives it back before it asks for anything.
    /// Holding the picture is what keeps that from happening under an encoder
    /// still reading it, so a consumer whose encoder reads the descriptor past
    /// that call holds the picture itself rather than relying on the buffer still
    /// being there.
    pub fn picture(&self) -> Option<&Picture> {
        self.picture.as_ref()
    }

    /// The picture where the driver put it, for a caller that has to read it:
    /// NV12's luma plane first and its chroma plane after it,
    /// [`Picture::stride`] bytes to a row. It is the driver's own memory, mapped
    /// once when the stream opened, so reading it is no copy — and the driver is
    /// not writing it while the picture is held.
    ///
    /// This is what a path that can't encode the descriptor falls back to, and
    /// nothing else reads it: an encoder that takes the descriptor never looks at
    /// a pixel.
    pub fn rows(&self) -> Option<&[u8]> {
        let picture = self.picture.as_ref()?;
        let buffer = self.buffers.get(picture.index as usize)?;
        // SAFETY: `memory` is a mapping of `length` bytes made when the stream
        // opened and unmapped when the capture is dropped, and it is the mapping
        // of the buffer this picture is in.
        Some(unsafe { std::slice::from_raw_parts(buffer.memory.as_ptr(), buffer.length) })
    }

    /// The size the device films at, which is its own answer and not necessarily
    /// the size a call asked for.
    pub fn size(&self) -> (u32, u32) {
        (self.format.width, self.format.height)
    }

    /// The bytes between one row of the picture and the next.
    pub fn stride(&self) -> u32 {
        self.format.stride
    }

    /// Why the capture ended, when it did: the sentence written for the
    /// [`Arrival::Closed`] it answered, and empty while the capture still runs.
    pub fn ending(&self) -> &str {
        &self.ending
    }

    /// What this capture is and how it went, in one line: the node, the size it
    /// films and at what pitch, where its two planes are, how many buffers the
    /// driver granted, what it offered, and how many frames were dropped.
    pub fn describe(&self) -> String {
        let mut line = format!(
            "{}: V4L2 NV12 {}x{} at {} bytes a row, chroma at {}, {} buffers of the {} asked for",
            self.path.display(),
            self.format.width,
            self.format.height,
            self.format.stride,
            self.format.chroma(),
            self.buffers.len(),
            BUFFERS,
        );
        let _ = write!(line, ", offered {}", listing(&self.offered));
        if self.dropped > 0 {
            let _ = write!(line, ", {} frames dropped", self.dropped);
        }
        line
    }

    /// Puts every buffer in the driver's queue and starts the stream: from here
    /// the device films into its own memory, and the queue decides which buffer is
    /// filled next.
    fn start(&mut self) -> Result<()> {
        for index in 0..self.buffers.len() as u32 {
            self.queue(index)
                .with_context(|| format!("queuing {}'s buffer {index}", self.path.display()))?;
        }
        let mut kind = CAPTURE;
        ask(self.node.as_fd(), STREAMON, &mut kind)
            .with_context(|| format!("starting {}'s stream", self.path.display()))?;
        self.streaming = true;
        Ok(())
    }

    /// Gives buffer `index` to the driver to film into.
    fn queue(&self, index: u32) -> io::Result<()> {
        ask(self.node.as_fd(), QBUF, &mut V4l2Buffer::of(index))
    }

    /// Hands the buffer under the current picture back to the driver, so that it
    /// is the device's again: a queue whose buffers are all held has stopped, and
    /// nothing says so but the camera going quiet.
    fn give_back(&mut self) {
        self.picture = None;
        let Some(index) = self.held.take() else {
            return;
        };
        if let Err(why) = self.queue(index) {
            self.stalled(index, why);
        }
    }

    /// Ends the capture with the driver's words for a buffer it would not take
    /// back, since a queue that has lost one has stopped filming.
    fn stalled(&mut self, index: u32, why: io::Error) {
        self.ending = format!(
            "{} would not take its buffer {index} back: {why}",
            self.path.display()
        );
    }

    /// Ends the capture with what the driver said, as a device that has stopped
    /// delivering at all is an ending rather than a failure.
    fn stopped(&mut self, why: io::Error) -> Result<Arrival> {
        self.ending = format!("{} stopped delivering pictures: {why}", self.path.display());
        self.picture = None;
        self.held = None;
        Ok(Arrival::Closed)
    }
}

impl Drop for Capture {
    /// Stops the stream and gives the driver its buffers back, in the one order
    /// that leaves nothing dangling: the stream stops writing them, the exported
    /// descriptors and their mappings go, and `VIDIOC_REQBUFS` with none frees the
    /// memory — which is what makes every descriptor exported from it a stale
    /// handle. A caller drops the encoder that imported them first; freeing driver
    /// memory under a surface still holding one is a fault in the driver rather
    /// than a borrow this side can check.
    fn drop(&mut self) {
        let mut kind = CAPTURE;
        if self.streaming {
            let _ = ask(self.node.as_fd(), STREAMOFF, &mut kind);
        }
        self.buffers.clear();
        let _ = ask(self.node.as_fd(), REQBUFS, &mut V4l2RequestBuffers::of(0));
    }
}

/// One of the driver's buffers, as this capture holds it: the descriptor a picture
/// publishes, and the pages a readback reads.
struct Exported {
    /// The buffer as `VIDIOC_EXPBUF` made it, kept for the stream's life. It names
    /// the driver's memory and nothing beyond it, so it goes with the queue.
    fd: OwnedFd,
    /// The mapping `mmap` made, which is what `munmap` is given back.
    memory: NonNull<u8>,
    length: usize,
}

impl Drop for Exported {
    fn drop(&mut self) {
        // SAFETY: this mapping is the one `mmap` made for this buffer, it is
        // unmapped once and only here, and nothing reads it after this: the
        // picture that borrowed the descriptor is gone before the capture is.
        unsafe { libc::munmap(self.memory.as_ptr().cast(), self.length) };
    }
}

/// The formats the device enumerates, and whether each films what a call asks for,
/// as `VIDIOC_ENUM_FMT` and the enumerations after it answer. The list ends where
/// the device answers `EINVAL`, which is how V4L2 spells "no more".
fn offered(fd: BorrowedFd<'_>, path: &Path) -> Result<Vec<Offered>> {
    let mut offered = Vec::new();
    for index in 0.. {
        let mut found = V4l2FmtDesc {
            index,
            kind: CAPTURE,
            ..V4l2FmtDesc::default()
        };
        match ask(fd, ENUM_FMT, &mut found) {
            Ok(()) => {}
            Err(why) if why.raw_os_error() == Some(libc::EINVAL) => break,
            Err(why) => {
                return Err(why)
                    .with_context(|| format!("reading the formats {} offers", path.display()));
            }
        }
        offered.push(Offered {
            fourcc: found.pixelformat,
            description: text(&found.description),
            compressed: found.flags & FMT_COMPRESSED != 0,
            // A format the ladder doesn't know is not asked about its sizes: what
            // it delivers changes nothing about a picture this can't hand on.
            delivers: Rung::of(found.pixelformat).is_some_and(|_| delivers(fd, found.pixelformat)),
        });
    }
    Ok(offered)
}

/// Whether the device says `fourcc` films [`AT`]: a size `VIDIOC_ENUM_FRAMESIZES`
/// holds, and a rate `VIDIOC_ENUM_FRAMEINTERVALS` calls fast enough. The rate is
/// compared as a proportion, so a camera that films 29.97 a second answers for 30,
/// which is what it films at and what a call's video tolerates.
fn delivers(fd: BorrowedFd<'_>, fourcc: u32) -> bool {
    for index in 0.. {
        let mut size = V4l2Framesize {
            index,
            format: fourcc,
            ..V4l2Framesize::default()
        };
        if ask(fd, ENUM_FRAMESIZES, &mut size).is_err() {
            return false;
        }
        if !holds(size.kind, &size.sizes, AT.0, AT.1) {
            continue;
        }
        if fast_enough(fd, fourcc) {
            return true;
        }
    }
    false
}

/// Whether the device delivers [`AT`]'s size at [`AT`]'s rate or faster, as
/// `VIDIOC_ENUM_FRAMEINTERVALS` answers for that size: an interval shorter than a
/// frame's worth of time is a rate fast enough.
fn fast_enough(fd: BorrowedFd<'_>, fourcc: u32) -> bool {
    for index in 0.. {
        let mut interval = V4l2Frameinterval {
            index,
            format: fourcc,
            width: AT.0,
            height: AT.1,
            ..V4l2Frameinterval::default()
        };
        if ask(fd, ENUM_FRAMEINTERVALS, &mut interval).is_err() {
            return false;
        }
        // A discrete interval lists one, and a stepwise range begins with its
        // shortest: both are the first proportion.
        let (numerator, denominator) = (interval.intervals[0], interval.intervals[1]);
        if u64::from(denominator) >= u64::from(numerator) * u64::from(AT.2) {
            return true;
        }
    }
    false
}

/// Whether a size enumeration holds `width` by `height`: the one size it lists, or
/// one of the range it describes.
fn holds(kind: u32, numbers: &[u32; 6], width: u32, height: u32) -> bool {
    let stepped = |low: u32, high: u32, step: u32, value: u32| {
        // A step of zero is a range a driver described as continuous.
        value >= low && value <= high && (step == 0 || (value - low).is_multiple_of(step))
    };
    match kind {
        LISTED => numbers[0] == width && numbers[1] == height,
        STEPWISE => {
            stepped(numbers[0], numbers[1], numbers[2], width)
                && stepped(numbers[3], numbers[4], numbers[5], height)
        }
        // `V4L2_FRMSIZE_TYPE_CONTINUOUS`: every size between the bounds.
        _ => {
            width >= numbers[0]
                && width <= numbers[1]
                && height >= numbers[3]
                && height <= numbers[4]
        }
    }
}

/// Asks the device to film `fourcc` at [`AT`], and answers what it will film at.
///
/// The try comes first so that a driver which answers with another format is
/// refused before the device is set to it; the size it substitutes is not a
/// refusal, since the video processor scales a picture of another size. The
/// numbers a picture is described with are S_FMT's own answer, re-read and never
/// assumed.
fn film(fd: BorrowedFd<'_>, path: &Path, fourcc: u32) -> Result<Format> {
    let mut tried = V4l2Format::of(fourcc, AT.0, AT.1);
    ask(fd, TRY_FMT, &mut tried).with_context(|| {
        format!(
            "asking {} whether it can film at {}x{}",
            path.display(),
            AT.0,
            AT.1
        )
    })?;
    if tried.pix.pixelformat != fourcc {
        let answered = tried.pix.pixelformat.to_le_bytes();
        bail!(
            "{} answered a try of NV12 with {}",
            path.display(),
            String::from_utf8_lossy(&answered)
        );
    }
    let mut asked = V4l2Format::of(fourcc, AT.0, AT.1);
    ask(fd, S_FMT, &mut asked)
        .with_context(|| format!("setting {} to film in NV12", path.display()))?;
    Format::of(asked.pix).with_context(|| format!("{} answered VIDIOC_S_FMT with", path.display()))
}

/// Asks the device to film [`AT`]'s rate, where it takes a frame rate at all.
///
/// It is advisory and best-effort: a camera clamps it to what its bus has left, and
/// one that films 15 a second slows the call's video, which the encoder already
/// tolerates.
fn pace(fd: BorrowedFd<'_>) {
    let mut parm = V4l2Streamparm::of();
    if ask(fd, G_PARM, &mut parm).is_err() {
        return;
    }
    if parm.capture.capability & CAP_TIMEPERFRAME == 0 {
        return;
    }
    parm.capture.timeperframe = V4l2Fraction {
        numerator: 1,
        denominator: AT.2,
    };
    let _ = ask(fd, S_PARM, &mut parm);
}

/// Requests the driver's own buffers, exports each as a descriptor, and maps it.
///
/// The queue is asked for in the driver's memory because that is the only memory
/// V4L2 can hand out: `V4L2_MEMORY_DMABUF` is the direction a caller fills, and a
/// queue allocated that way has nothing to export. The export sits between
/// `VIDIOC_REQBUFS` and the first `VIDIOC_QBUF` because a buffer in flight is not
/// the caller's to export.
fn exported(fd: BorrowedFd<'_>, format: &Format) -> Result<Vec<Exported>> {
    let mut requested = V4l2RequestBuffers::of(BUFFERS);
    ask(fd, REQBUFS, &mut requested).context("asking the camera for buffers")?;
    let count = requested.count;
    if count < 2 {
        bail!(
            "the camera granted {count} buffer{}, and one is a picture the encoder holds while \
             the device has nowhere to film",
            if count == 1 { "" } else { "s" }
        );
    }
    let mut buffers = Vec::with_capacity(count as usize);
    for index in 0..count {
        let mut queried = V4l2Buffer::of(index);
        ask(fd, QUERYBUF, &mut queried)
            .with_context(|| format!("measuring the camera's buffer {index}"))?;
        if u64::from(queried.length) < format.needed() {
            bail!(
                "the camera's buffer {index} holds {} bytes, and a {}x{} NV12 picture needs {}",
                queried.length,
                format.width,
                format.height,
                format.needed()
            );
        }
        let mut export = V4l2ExportBuffer::of(index);
        ask(fd, EXPBUF, &mut export).with_context(|| {
            format!(
                "exporting the camera's buffer {index} as a DMA-BUF: this driver has no way to \
                 hand one out"
            )
        })?;
        // SAFETY: `export.fd` is the descriptor `VIDIOC_EXPBUF` made for this
        // buffer, which is this process's alone and is closed once, here.
        let buffer = unsafe { OwnedFd::from_raw_fd(export.fd) };
        let length = queried.length as usize;
        // SAFETY: the node is the capture's own, open for longer than the mapping;
        // the offset and length are the driver's answer for this buffer; and the
        // mapping is read-only and private to this process.
        let memory = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                length,
                libc::PROT_READ,
                libc::MAP_SHARED,
                fd.as_raw_fd(),
                queried.offset as libc::off_t,
            )
        };
        if memory == libc::MAP_FAILED {
            return Err(io::Error::last_os_error())
                .with_context(|| format!("mapping the camera's buffer {index} to read it back"));
        }
        buffers.push(Exported {
            fd: buffer,
            memory: NonNull::new(memory.cast::<u8>())
                .expect("mmap answers nothing, which is checked"),
            length,
        });
    }
    Ok(buffers)
}

/// Waits until the device has a frame to hand over, or `deadline` passes: `true`
/// when there is one to dequeue, `false` when there is not.
fn filled(fd: BorrowedFd<'_>, deadline: Instant) -> io::Result<bool> {
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let millis = remaining.as_millis().min(i32::MAX as u128) as libc::c_int;
        let mut describing = [libc::pollfd {
            fd: fd.as_raw_fd(),
            events: libc::POLLIN | libc::POLLERR,
            revents: 0,
        }];
        // SAFETY: `describing` is one live `pollfd` whose descriptor is the
        // capture's node, open for as long as the capture is, and the array's
        // length is passed with it.
        let ready = unsafe { libc::poll(describing.as_mut_ptr(), 1, millis) };
        match ready {
            // A timeout is a camera that filmed nothing while it was asked, which
            // is a still frame rather than a failure.
            0 => return Ok(false),
            ready if ready > 0 => return Ok(true),
            _ => {
                let why = io::Error::last_os_error();
                // A signal arriving while waiting is the wait cut short, not the
                // device having nothing: the wait goes on with what is left of the
                // caller's time.
                if why.kind() != io::ErrorKind::Interrupted {
                    return Err(why);
                }
            }
        }
    }
}

/// What `VIDIOC_QUERYCAP` answers: the driver's name and the abilities of the node
/// it was asked of. Laid out as `struct v4l2_capability`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct V4l2Capability {
    driver: [u8; 16],
    card: [u8; 32],
    bus: [u8; 32],
    version: u32,
    /// The driver's abilities, which are the union over its nodes.
    capabilities: u32,
    /// This node's own abilities, where `capabilities` says they are here.
    device_capabilities: u32,
    reserved: [u32; 3],
}

/// Asks the camera what it can do, which is what decides whether it can be filmed
/// at all.
impl V4l2Capability {
    fn ask(fd: BorrowedFd<'_>) -> io::Result<Self> {
        let mut capability = Self::default();
        ask(fd, QUERYCAP, &mut capability)?;
        Ok(capability)
    }

    /// What this node can do: its own abilities where the driver says it has a set
    /// of its own, and the driver's otherwise. A driver's set is the union over
    /// every node it makes, which for a camera includes its metadata node.
    fn abilities(&self) -> u32 {
        if self.capabilities & CAP_DEVICE_CAPS != 0 {
            self.device_capabilities
        } else {
            self.capabilities
        }
    }
}

/// One format `VIDIOC_ENUM_FMT` answers, by index. Laid out as `struct
/// v4l2_fmtdesc`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct V4l2FmtDesc {
    index: u32,
    kind: u32,
    flags: u32,
    /// The driver's name for the format, as a NUL-terminated string.
    description: [u8; 32],
    pixelformat: u32,
    mbus_code: u32,
    reserved: [u32; 3],
}

/// One size `VIDIOC_ENUM_FRAMESIZES` answers, by index: one size, or the bounds and
/// step of all of them. Laid out as `struct v4l2_frmsizeenum`, whose union is the
/// six numbers here.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct V4l2Framesize {
    index: u32,
    format: u32,
    /// `LISTED` for a single size, `STEPWISE` for a range.
    kind: u32,
    /// The union: a discrete answer names its width and height, and a stepwise one
    /// the low bound, the high bound and the step of each side.
    sizes: [u32; 6],
    reserved: [u32; 2],
}

/// One frame interval `VIDIOC_ENUM_FRAMEINTERVALS` answers for a size, by index: one
/// interval, or the bounds and step of all of them. Laid out as `struct
/// v4l2_frmivalenum`, whose union is the six numbers here.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct V4l2Frameinterval {
    index: u32,
    format: u32,
    width: u32,
    height: u32,
    kind: u32,
    /// The numerator and denominator of an interval, or the low bound, the high
    /// bound and the step of a stepwise range, as proportions.
    intervals: [u32; 6],
    reserved: [u32; 2],
}

/// A picture's geometry, as `v4l2_pix_format` describes it. Laid out as that struct.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct V4l2PixFormat {
    width: u32,
    height: u32,
    pixelformat: u32,
    field: u32,
    bytesperline: u32,
    sizeimage: u32,
    colorspace: u32,
    private: u32,
    flags: u32,
    ycbcr_enc: u32,
    quantization: u32,
    xfer_func: u32,
}

/// The format a request asks for or an answer gives, as `struct v4l2_format` lays
/// it out: a type, and a union whose picture member is the only one read here.
///
/// The eight-byte alignment is the C union's, where one member carries pointers;
/// the four bytes before the picture are the padding that alignment adds.
#[repr(C, align(8))]
#[derive(Clone, Copy)]
struct V4l2Format {
    kind: u32,
    _align: u32,
    pix: V4l2PixFormat,
    /// The rest of the union: `v4l2_window` and its arrays are its widest member.
    _rest: [u8; 200 - 48],
}

impl V4l2Format {
    /// A request that the device film `fourcc` at `width` by `height`, with
    /// everything else zero: a request names only what it asks about.
    fn of(fourcc: u32, width: u32, height: u32) -> Self {
        Self {
            kind: CAPTURE,
            _align: 0,
            pix: V4l2PixFormat {
                width,
                height,
                pixelformat: fourcc,
                field: FIELD_NONE,
                ..V4l2PixFormat::default()
            },
            _rest: [0; 200 - 48],
        }
    }
}

/// A request for buffers, or the driver's answer to one, as `struct
/// v4l2_requestbuffers` lays it out.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct V4l2RequestBuffers {
    count: u32,
    kind: u32,
    memory: u32,
    capabilities: u32,
    reserved: [u32; 1],
}

impl V4l2RequestBuffers {
    /// A request for `count` buffers of the driver's own memory: a queue of the
    /// node's single-planar capture.
    fn of(count: u32) -> Self {
        Self {
            count,
            kind: CAPTURE,
            memory: MMAP,
            capabilities: 0,
            reserved: [0; 1],
        }
    }
}

/// One buffer, as `struct v4l2_buffer` lays it out: what the driver wrote into it
/// when it handed it over, and which one it is when it is being given back.
#[repr(C)]
#[derive(Clone, Copy)]
struct V4l2Buffer {
    index: u32,
    kind: u32,
    bytesused: u32,
    flags: u32,
    field: u32,
    /// When the frame was captured, in the clock the flags name.
    timestamp: libc::timeval,
    /// `struct v4l2_timecode`, whose sixteen bytes a camera leaves zero.
    timecode: [u8; 16],
    sequence: u32,
    memory: u32,
    /// The union `m`, of which a queue in the driver's memory names an `offset` to
    /// map: a pointer and a descriptor are its other members.
    offset: u32,
    _union: u32,
    length: u32,
    reserved: u32,
    request: u32,
}

impl V4l2Buffer {
    /// Buffer `index` of the node's capture queue, with nothing the driver
    /// answered in it.
    fn of(index: u32) -> Self {
        Self {
            index,
            kind: CAPTURE,
            bytesused: 0,
            flags: 0,
            field: 0,
            timestamp: libc::timeval {
                tv_sec: 0,
                tv_usec: 0,
            },
            timecode: [0; 16],
            sequence: 0,
            memory: MMAP,
            offset: 0,
            _union: 0,
            length: 0,
            reserved: 0,
            request: 0,
        }
    }
}

/// One buffer's export, as `struct v4l2_exportbuffer` lays it out: which buffer to
/// hand out, and the descriptor it was handed out as.
#[repr(C)]
#[derive(Clone, Copy)]
struct V4l2ExportBuffer {
    kind: u32,
    index: u32,
    plane: u32,
    flags: u32,
    fd: i32,
    reserved: [u32; 11],
}

impl V4l2ExportBuffer {
    /// The export of buffer `index`, whose descriptor is closed on exec and open
    /// for reading and writing as an importer needs.
    fn of(index: u32) -> Self {
        Self {
            kind: CAPTURE,
            index,
            plane: 0,
            flags: (libc::O_CLOEXEC | libc::O_RDWR) as u32,
            fd: -1,
            reserved: [0; 11],
        }
    }
}

/// A numerator and a denominator, as `struct v4l2_fract` lays them out.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct V4l2Fraction {
    numerator: u32,
    denominator: u32,
}

/// A capture's parameters, as `struct v4l2_captureparm` lays them out.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct V4l2Captureparm {
    capability: u32,
    capturemode: u32,
    timeperframe: V4l2Fraction,
    extendedmode: u32,
    readbuffers: u32,
    reserved: [u32; 4],
}

/// A stream's parameters, as `struct v4l2_streamparm` lays it out: a type, and a
/// union whose capture member is the only one read here.
#[repr(C)]
#[derive(Clone, Copy)]
struct V4l2Streamparm {
    kind: u32,
    capture: V4l2Captureparm,
    /// The rest of the union: `v4l2_outputparm` and the raw array are its widest
    /// members.
    _rest: [u8; 200 - 40],
}

impl V4l2Streamparm {
    /// A request about the node's capture queue's parameters.
    fn of() -> Self {
        Self {
            kind: CAPTURE,
            capture: V4l2Captureparm::default(),
            _rest: [0; 200 - 40],
        }
    }
}

// Each of these is the C struct's size, which is what the ioctl number above it was
// numbered with: a layout that drifts from it is asked for with the wrong size, and
// the kernel answers `ENOTTY` rather than a wrong picture.
const _: () = assert!(std::mem::size_of::<V4l2Capability>() == 104);
const _: () = assert!(std::mem::size_of::<V4l2FmtDesc>() == 64);
const _: () = assert!(std::mem::size_of::<V4l2Framesize>() == 44);
const _: () = assert!(std::mem::size_of::<V4l2Frameinterval>() == 52);
const _: () = assert!(std::mem::size_of::<V4l2Format>() == 208);
const _: () = assert!(std::mem::size_of::<V4l2RequestBuffers>() == 20);
const _: () = assert!(std::mem::size_of::<V4l2Buffer>() == 88);
const _: () = assert!(std::mem::size_of::<V4l2ExportBuffer>() == 64);
const _: () = assert!(std::mem::size_of::<V4l2Streamparm>() == 204);

/// One ioctl, with the C struct it is numbered for. A negative answer is the
/// driver's `errno`.
fn ask<T>(fd: BorrowedFd<'_>, request: u32, argument: &mut T) -> io::Result<()> {
    // SAFETY: `argument` is a live `#[repr(C)]` struct of the size the request was
    // numbered with, which is asserted above each of them, and the kernel writes
    // no more of it than that.
    let answer = unsafe { libc::ioctl(fd.as_raw_fd(), request as libc::Ioctl, argument as *mut T) };
    if answer < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// A `__u8` description as text: what the driver calls a format, which is what a
/// refusal names.
fn text(bytes: &[u8]) -> String {
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).trim().to_owned()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{Chosen, Format, Offered, Picture, Rung, choose, negotiate};
    use crate::dmabuf::Plane;

    /// A format a device enumerated, as `VIDIOC_ENUM_FMT` and the enumerations
    /// after it would have answered it.
    fn offered(fourcc: &[u8; 4], description: &str, compressed: bool, delivers: bool) -> Offered {
        Offered {
            fourcc: u32::from_le_bytes(*fourcc),
            description: description.to_owned(),
            compressed,
            delivers,
        }
    }

    /// What a device films at once S_FMT has answered it, at `stride` bytes a row.
    fn filmed(stride: u32) -> Format {
        Format {
            width: 1280,
            height: 720,
            stride,
        }
    }

    /// A picture is NV12, linear, and its two planes in one buffer at the device's
    /// pitch: the shape `encode.rs` imports as a surface without converting a
    /// pixel, and the chroma where the driver put it.
    #[test]
    fn a_picture_is_one_nv12_buffer_at_the_devices_pitch() {
        let picture = Picture::of(&filmed(1280), 3, 7, &filled());
        let descriptor = picture.descriptor();
        assert_eq!((descriptor.width, descriptor.height), (1280, 720));
        assert_eq!(descriptor.fourcc, u32::from_le_bytes(*b"NV12"));
        assert_eq!(descriptor.modifier, 0, "DRM_FORMAT_MOD_LINEAR");
        assert_eq!(
            descriptor.planes,
            [
                Plane {
                    offset: 0,
                    stride: 1280,
                },
                Plane {
                    offset: 1280 * 720,
                    stride: 1280,
                },
            ]
        );
        assert_eq!(descriptor.fds, [7, 7], "one buffer, named once a plane");
        assert_eq!(picture.size(), (1280, 720));
        assert_eq!(picture.stride(), 1280);
    }

    /// A pitch wider than the picture is padding a driver is entitled to, and the
    /// chroma plane is at the padded pitch — never at the width, which would name
    /// bytes that are not the picture. The buffer's own size is the two planes at
    /// that pitch, which is what `encode.rs` works out from the same descriptor.
    #[test]
    fn a_padded_pitch_moves_the_chroma_plane_with_it() {
        let format = filmed(1344);
        let picture = Picture::of(&format, 0, 5, &filled());
        let descriptor = picture.descriptor();
        assert_eq!(
            descriptor.planes[0],
            Plane {
                offset: 0,
                stride: 1344
            }
        );
        assert_eq!(
            descriptor.planes[1],
            Plane {
                offset: 1344 * 720,
                stride: 1344
            }
        );
        // The size the descriptor's planes add up to, as `encode.rs` sizes an
        // import: the luma rows and then the chroma's, at one pitch.
        let size = descriptor
            .planes
            .iter()
            .enumerate()
            .map(|(index, plane)| {
                let rows = if index == 0 { 720 } else { 720 / 2 };
                u64::from(plane.offset) + u64::from(plane.stride) * rows
            })
            .max()
            .unwrap_or(0);
        assert_eq!(size, format.needed());
        assert_eq!(format.chroma(), 1344 * 720);
    }

    /// A frame's own clock and number travel with the picture: the device's
    /// monotonic stamp where it says it stamps in that clock, and none where it
    /// stamps on arrival.
    #[test]
    fn a_frame_carries_the_devices_clock_and_number() {
        let mut frame = filled();
        frame.flags = super::BUF_TIMESTAMP_MONOTONIC;
        frame.timestamp = libc::timeval {
            tv_sec: 12,
            tv_usec: 500,
        };
        frame.sequence = 41;
        let picture = Picture::of(&filmed(1280), 1, 9, &frame);
        assert_eq!(
            picture.stamp(),
            Some(std::time::Duration::from_micros(12_000_500))
        );
        assert_eq!(picture.sequence(), 41);
        assert_eq!(
            Picture::of(&filmed(1280), 1, 9, &filled()).stamp(),
            None,
            "a driver that stamps on arrival has no clock to read"
        );
    }

    /// A frame the driver handed over, as `VIDIOC_DQBUF` would have filled it.
    fn filled() -> super::V4l2Buffer {
        super::V4l2Buffer::of(0)
    }

    /// The ladder: NV12 where it films what a call wants, then the format that
    /// needs no colour conversion, then the packed one, then the compressed one —
    /// and the tiled spellings of NV12, which would break the linear claim, are not
    /// a rung at all.
    #[test]
    fn the_format_asked_for_is_the_cheapest_that_films() {
        let all = [
            offered(b"MJPG", "MJPEG", true, true),
            offered(b"YUYV", "YUYV 4:2:2", false, true),
            offered(b"YU12", "YUV 4:2:0", false, true),
            offered(b"NV12", "NV12", false, true),
        ];
        assert_eq!(rung(&all), Rung::Nv12);
        assert_eq!(rung(&all[..3]), Rung::Planar);
        assert_eq!(rung(&all[..2]), Rung::Packed);
        assert_eq!(rung(&all[..1]), Rung::Jpeg);
        // YV12 is 4:2:0 the other way round, and costs the same.
        assert_eq!(
            rung(&[
                offered(b"YUYV", "YUYV 4:2:2", false, true),
                offered(b"YV12", "YV12", false, true),
            ]),
            Rung::Planar
        );
        let tiled = [
            offered(b"NL16", "NV12 16x16 tiles", false, true),
            offered(b"NM12", "NV12 32x32 tiles", false, true),
        ];
        let why = format!("{:#}", choose(&tiled).expect_err("no rung"));
        assert!(why.contains("NV12 16x16 tiles"), "{why}");
        assert!(why.contains("NV12 32x32 tiles"), "{why}");
    }

    /// The rung `choose` names for what a device offered.
    fn rung(offered: &[Offered]) -> Rung {
        let Chosen { rung, .. } = choose(offered).expect("a rung");
        rung
    }

    /// A rung that films the size and rate comes before a cheaper one that doesn't:
    /// a camera that films 720p30 in MJPEG alone is asked for MJPEG rather than for
    /// the YUYV that keeps up at ten a second. Where nothing films the size, the
    /// cheapest rung is still asked for, and S_FMT answers the size the device has.
    #[test]
    fn a_rung_that_films_the_size_comes_before_a_cheaper_one_that_does_not() {
        assert_eq!(
            rung(&[
                offered(b"YUYV", "YUYV 4:2:2", false, false),
                offered(b"MJPG", "MJPEG", true, true),
            ]),
            Rung::Jpeg
        );
        assert_eq!(
            rung(&[
                offered(b"MJPG", "MJPEG", true, false),
                offered(b"YUYV", "YUYV 4:2:2", false, false),
            ]),
            Rung::Packed,
            "nothing films the size, so the cheapest is asked"
        );
    }

    /// A device that films NV12 is asked for it, whatever else it offers.
    #[test]
    fn a_device_that_films_nv12_is_asked_for_it() {
        let fourcc = negotiate(
            Path::new("/dev/video0"),
            &[
                offered(b"MJPG", "MJPEG", true, true),
                offered(b"YUYV", "YUYV 4:2:2", false, true),
                offered(b"NV12", "NV12", false, true),
            ],
        )
        .expect("NV12");
        assert_eq!(fourcc, u32::from_le_bytes(*b"NV12"));
    }

    /// A device with no NV12 is refused in the words a caller logs before it opens
    /// the processor's capturer: what the device does film, that its frames are
    /// compressed where they are, and why that keeps them on the processor.
    #[test]
    fn a_device_with_no_nv12_is_refused_in_words() {
        let why = format!(
            "{:#}",
            negotiate(
                Path::new("/dev/video0"),
                &[
                    offered(b"MJPG", "MJPEG", true, true),
                    offered(b"YUYV", "YUYV 4:2:2", false, true),
                ],
            )
            .expect_err("no NV12")
        );
        assert!(why.contains("/dev/video0"), "{why}");
        assert!(why.contains("MJPEG (compressed)"), "{why}");
        assert!(why.contains("YUYV 4:2:2"), "{why}");
        assert!(why.contains("not NV12"), "{why}");
        assert!(why.contains("processor"), "{why}");
    }

    /// A device that offers nothing a camera is read in says what it did offer,
    /// rather than leaving a caller to guess why nothing films.
    #[test]
    fn a_device_offering_no_camera_format_says_what_it_offers() {
        let why = format!(
            "{:#}",
            negotiate(
                Path::new("/dev/video2"),
                &[offered(b"H264", "H.264", false, true)],
            )
            .expect_err("no format to film in")
        );
        assert!(why.contains("/dev/video2"), "{why}");
        assert!(why.contains("H.264"), "{why}");
        assert!(why.contains("NV12, YUV420, YV12, YUYV or MJPEG"), "{why}");
    }

    /// A size is one the enumeration holds when it is the size it lists, one the
    /// step it describes lands on, or — for a range a driver called continuous —
    /// anywhere between the bounds. A call asks for 1280 by 720, and a camera that
    /// films it only between a step's bounds has not got it.
    #[test]
    fn a_size_enumeration_holds_the_size_a_call_asks_for() {
        let listed = [1280, 720, 0, 0, 0, 0];
        assert!(super::holds(super::LISTED, &listed, 1280, 720));
        assert!(!super::holds(super::LISTED, &listed, 1920, 1080));

        // 640 to 1920 in steps of 64, and 480 to 1080 in steps of 8.
        let stepped = [640, 1920, 64, 480, 1080, 8];
        assert!(super::holds(super::STEPWISE, &stepped, 1280, 720));
        assert!(
            !super::holds(super::STEPWISE, &stepped, 1281, 720),
            "off the step"
        );
        assert!(
            !super::holds(super::STEPWISE, &stepped, 2560, 720),
            "past the bound"
        );

        let continuous = [320, 2592, 0, 240, 1944, 0];
        assert!(super::holds(2, &continuous, 1280, 720), "any size between");
        assert!(!super::holds(2, &continuous, 320, 120), "below the bottom");
    }

    /// A device that enumerated nothing at all is the same refusal with nothing to
    /// name.
    #[test]
    fn a_device_that_enumerates_nothing_says_so() {
        let why = format!(
            "{:#}",
            negotiate(Path::new("/dev/video3"), &[]).expect_err("nothing")
        );
        assert!(why.contains("nothing at all"), "{why}");
    }

    /// A picture the device cannot describe is refused by name: a format other than
    /// NV12, an empty one, one with an odd side that has no place for a subsampled
    /// chroma plane, a pitch narrower than a row, and a buffer padded past the
    /// height, which would put the chroma plane somewhere a descriptor can't name.
    #[test]
    fn a_picture_a_descriptor_cannot_name_is_refused_by_name() {
        let answered = |width: u32, height: u32, fourcc: &[u8; 4], stride: u32| {
            let sizeimage = stride * (height + height.div_ceil(2));
            super::V4l2PixFormat {
                width,
                height,
                pixelformat: u32::from_le_bytes(*fourcc),
                bytesperline: stride,
                sizeimage,
                ..super::V4l2PixFormat::default()
            }
        };
        assert!(Format::of(answered(1280, 720, b"NV12", 1280)).is_ok());

        let why = format!(
            "{:#}",
            Format::of(answered(1280, 720, b"YUYV", 2560)).expect_err("YUYV")
        );
        assert!(why.contains("instead of NV12"), "{why}");

        let why = format!(
            "{:#}",
            Format::of(answered(0, 0, b"NV12", 0)).expect_err("empty")
        );
        assert!(why.contains("empty"), "{why}");

        let why = format!(
            "{:#}",
            Format::of(answered(1281, 721, b"NV12", 1284)).expect_err("odd")
        );
        assert!(why.contains("subsampled by two"), "{why}");

        let why = format!(
            "{:#}",
            Format::of(answered(1280, 720, b"NV12", 640)).expect_err("narrow")
        );
        assert!(why.contains("pitch"), "{why}");

        // A driver that pads the height answers a sizeimage larger than the two
        // planes at the pitch, which is exactly the case nothing can place.
        let padded = super::V4l2PixFormat {
            sizeimage: 1280 * 800,
            ..answered(1280, 720, b"NV12", 1280)
        };
        let why = format!("{:#}", Format::of(padded).expect_err("padded"));
        assert!(why.contains("pitch times the height"), "{why}");
    }
}
