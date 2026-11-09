//! A picture the compositor drew, read back into memory.
//!
//! A share whose pictures are DMA-BUFs keeps every one of them on the card: the
//! window imports the descriptor and draws it where it lies, and no pixel of the
//! tile crosses the processor. Two things still want pixels, and both are the
//! copy a share is allowed to make — Discord's still of the stream, and the
//! picture a VA-API encoder is handed when it won't take the descriptor as it
//! lies. This is the read they are made from: the compositor's own memory,
//! mapped for reading at the offset and stride it named.
//!
//! `mmap` of a DMA-BUF is the driver handing over its pages, so there is no copy
//! on the way in either: the caller reads the rows of the picture where they
//! are, and the mapping is unmapped when the picture goes.
//!
//! What can't be read this way is a picture the driver laid out as anything but
//! linear — a tiled or compressed modifier is a layout a GPU knows and this
//! mapping would read as noise — so such a picture is refused by name rather
//! than handed over as wrong bytes. The share that asked for it then falls back
//! to the capturer that has pixels, which is what it did before any of this
//! existed.
use std::io;
use std::os::fd::{AsRawFd, BorrowedFd};
use std::ptr::NonNull;

use anyhow::{Context, Result, bail};

/// Which way round a picture's red and blue bytes are, as the desktop fixated
/// it. It is the one thing about a picture's bytes a reader has to know before
/// it can call them a colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Order {
    /// B, G, R, and a byte nothing reads: DRM's `ARGB8888`, which is what a
    /// compositor renders a desktop into, and libyuv's ARGB.
    Bgra,
    /// The same with red and blue the other way round: DRM's `ABGR8888`, and
    /// libyuv's ABGR.
    Rgba,
}

impl Order {
    /// The DRM fourcc this order names, as [`crate::dmabuf::Descriptor`] carries
    /// it: four bytes a pixel, red first in the format's own name and last in
    /// memory for [`Order::Bgra`].
    pub const fn fourcc(self) -> u32 {
        match self {
            Self::Bgra => u32::from_le_bytes(*b"AR24"),
            Self::Rgba => u32::from_le_bytes(*b"AB24"),
        }
    }

    /// What this order is called in a log line.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Bgra => "BGRA",
            Self::Rgba => "RGBA",
        }
    }

    /// How many bytes a pixel takes, which a stride is a multiple of.
    pub const fn bytes(self) -> u32 {
        4
    }
}

/// DRM's `DRM_FORMAT_MOD_LINEAR`: rows one after another, which is the one
/// layout this mapping reads.
const LINEAR: u64 = 0;

/// The picture's memory, mapped for reading: `rows` is its first byte, and
/// [`Readback::stride`] is how far the next row is from it.
pub struct Readback {
    memory: NonNull<u8>,
    length: usize,
    stride: u32,
}

impl Readback {
    /// The picture's bytes, its first row first: `stride` bytes apart, of which
    /// the first `width * 4` are the row's pixels.
    pub fn rows(&self) -> &[u8] {
        // SAFETY: `memory` is a mapping of `length` bytes this struct owns, and
        // `length` was worked out from the stride and height it was made with.
        unsafe { std::slice::from_raw_parts(self.memory.as_ptr(), self.length) }
    }

    /// How many bytes apart the picture's rows are.
    pub fn stride(&self) -> u32 {
        self.stride
    }
}

impl Drop for Readback {
    fn drop(&mut self) {
        // SAFETY: this mapping is the one `read` made, it is unmapped once and
        // only here, and nothing borrows it after this.
        unsafe { libc::munmap(self.memory.as_ptr().cast(), self.length) };
    }
}

/// Maps the `width` by `height` picture in `memory` for reading, which starts
/// `offset` bytes into it with `stride` bytes between its rows and was laid out
/// as `modifier` says.
///
/// A picture the driver tiled, or one whose rows are narrower than its pixels,
/// is refused with the reason rather than mapped: what a reader would get back
/// is not the picture.
pub fn read(
    memory: BorrowedFd<'_>,
    width: u32,
    height: u32,
    offset: u32,
    stride: u32,
    modifier: Option<u64>,
) -> Result<Readback> {
    match modifier {
        Some(LINEAR) | None => {}
        Some(modifier) => bail!(
            "the picture is laid out as {modifier:#018x}, which is no linear mapping: reading it \
             back needs an importer this build doesn't carry"
        ),
    }
    let row = width
        .checked_mul(Order::Bgra.bytes())
        .filter(|_| width > 0 && height > 0)
        .filter(|row| stride >= *row)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "a picture {width} by {height} cannot have {stride} bytes to a row, which is less \
                 than its {width} four-byte pixels"
            )
        })?;
    let length = usize::try_from(offset)
        .ok()
        .and_then(|offset| {
            usize::try_from(stride)
                .ok()
                .and_then(|stride| usize::try_from(height).ok()?.checked_mul(stride))
                .and_then(|rows| offset.checked_add(rows))
        })
        .context("a picture too large to map")?;
    debug_assert!(length >= row as usize * height as usize);
    // SAFETY: the fd is the picture's own, which the caller holds for as long as
    // it reads; `length` is the picture's rows and the offset before them, and
    // the mapping is private to this process and read-only. `mmap` wants a
    // page-aligned offset, so the whole picture is mapped from zero and the
    // caller's offset is applied to the slice it gets instead.
    let memory = unsafe {
        libc::mmap(
            std::ptr::null_mut(),
            length,
            libc::PROT_READ,
            libc::MAP_SHARED,
            memory.as_raw_fd(),
            0,
        )
    };
    if memory == libc::MAP_FAILED {
        return Err(io::Error::last_os_error())
            .context("mapping the picture the compositor drew, to read it back");
    }
    let memory = NonNull::new(memory.cast::<u8>()).expect("mmap answers nothing, which is checked");
    let mut picture = Readback {
        memory,
        length,
        stride,
    };
    // The offset is where the picture starts, which is not always zero: a plane
    // can begin partway into an allocation. The mapping starts at the memory's
    // own first byte, so the picture's rows begin later in it.
    let start = usize::try_from(offset).unwrap_or(0).min(picture.length);
    picture.memory = NonNull::new(unsafe { picture.memory.as_ptr().add(start) })
        .expect("a pointer into a mapping is not null");
    picture.length -= start;
    Ok(picture)
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;
    use std::os::fd::AsFd;

    use super::{Order, read};

    /// A file of `bytes`, as a picture's memory: the DMA-BUF itself is a
    /// driver's, and what a reader is given of one is pages and a layout.
    fn memory(bytes: &[u8]) -> std::fs::File {
        let mut file = tempfile::tempfile().unwrap();
        file.write_all(bytes).unwrap();
        file
    }

    /// The picture is the memory from its offset on, one stride to a row, and it
    /// is read where it lies rather than copied anywhere first.
    #[test]
    fn a_linear_picture_is_read_at_the_offset_and_stride_that_names_it() {
        let bytes: Vec<u8> = (0..64).collect();
        let file = memory(&bytes);
        // Two pixels a row, four bytes each, eight to a row: three rows from
        // the eighth byte, which is four rows of nothing before them.
        let picture = read(file.as_fd(), 2, 3, 8, 8, Some(0)).unwrap();
        assert_eq!(picture.stride(), 8);
        assert_eq!(picture.rows(), &bytes[8..32]);
        // A modifier a DRI3 .0 server couldn't say is read as it lies, which is
        // the implicit model such a server draws under.
        let picture = read(file.as_fd(), 2, 3, 8, 8, None).unwrap();
        assert_eq!(picture.rows(), &bytes[8..32]);
    }

    /// A picture the driver tiled is refused by name: what a CPU read of it
    /// would call a picture is noise, and a share says so rather than sending
    /// it.
    #[test]
    fn a_tiled_picture_is_refused_rather_than_read_wrongly() {
        let file = memory(&[0; 64]);
        let tiled = Some(0x0100_0000_0000_0002);
        let error = read(file.as_fd(), 2, 3, 0, 8, tiled)
            .err()
            .expect("a tiled picture isn't read");
        assert!(
            format!("{error:#}").contains("0x0100000000000002"),
            "{error:#}"
        );
        assert!(
            format!("{error:#}").contains("no linear mapping"),
            "{error:#}"
        );
        // And a row narrower than the pixels in it is a picture this reader has
        // no way to step through.
        let error = read(file.as_fd(), 4, 3, 0, 8, Some(0))
            .err()
            .expect("a row narrower than its pixels isn't read");
        assert!(
            format!("{error:#}").contains("less than its 4"),
            "{error:#}"
        );
    }

    /// The two orders a desktop fixates are the two DRM formats a descriptor can
    /// name, and neither is the other.
    #[test]
    fn an_order_names_the_format_the_descriptor_carries() {
        assert_eq!(Order::Bgra.fourcc(), u32::from_le_bytes(*b"AR24"));
        assert_eq!(Order::Rgba.fourcc(), u32::from_le_bytes(*b"AB24"));
        assert_ne!(Order::Bgra.fourcc(), Order::Rgba.fourcc());
        assert_eq!(Order::Bgra.bytes(), 4);
    }
}
