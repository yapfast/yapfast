//! NV12 pictures from Core Video, as the I420 pictures the sinks take. The
//! camera (`camera/avfoundation.rs`) and screen sharing (`screencapturekit`)
//! both get NV12 (`420v`: a luma plane, then one plane of interleaved chroma),
//! which the system gives without a conversion of its own. The luma goes on as
//! it is and only the chroma is split into two planes, which costs a quarter
//! of the frame's bytes.

use objc2_core_video::{
    CVPixelBuffer, CVPixelBufferGetBaseAddressOfPlane, CVPixelBufferGetBytesPerRowOfPlane,
    CVPixelBufferGetHeightOfPlane, CVPixelBufferGetPixelFormatType, CVPixelBufferGetPlaneCount,
    CVPixelBufferGetWidthOfPlane, CVPixelBufferLockBaseAddress, CVPixelBufferLockFlags,
    CVPixelBufferUnlockBaseAddress, kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange,
    kCVReturnSuccess,
};

use crate::picture::I420;

/// The planes of an NV12 picture.
const LUMA_PLANE: usize = 0;
const CHROMA_PLANE: usize = 1;
const NV12_PLANES: usize = 2;

/// Why a pixel buffer did not become a picture.
#[derive(Debug, PartialEq, Eq)]
pub enum Nv12Error {
    /// The buffer is in another pixel format.
    NotNv12,
    /// Core Video would not lock the buffer's memory; the status it gave.
    Lock(i32),
    /// The buffer has no memory behind its planes, or no size.
    NoPlanes,
}

/// The chroma planes a picture is made with, kept from frame to frame so no
/// frame allocates.
#[derive(Default)]
pub struct Chroma {
    u: Vec<u8>,
    v: Vec<u8>,
}

/// Splits NV12's interleaved chroma rows (`U V U V ...`, `src_stride` bytes
/// apart) into two planes, `width` bytes a row each, over `rows` rows.
pub fn split_chroma(
    src: &[u8],
    src_stride: usize,
    width: usize,
    rows: usize,
    u: &mut Vec<u8>,
    v: &mut Vec<u8>,
) {
    u.clear();
    v.clear();
    u.reserve(width * rows);
    v.reserve(width * rows);
    for row in src.chunks(src_stride).take(rows) {
        for pair in row[..(width * 2).min(row.len())].chunks_exact(2) {
            u.push(pair[0]);
            v.push(pair[1]);
        }
    }
}

/// Runs `use_picture` on the I420 picture `image` holds, which lasts until it
/// returns: the buffer is locked meanwhile.
pub fn with_i420<R>(
    image: &CVPixelBuffer,
    chroma: &mut Chroma,
    use_picture: impl FnOnce(&I420<'_>) -> R,
) -> Result<R, Nv12Error> {
    if CVPixelBufferGetPixelFormatType(image) != kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange
        || CVPixelBufferGetPlaneCount(image) != NV12_PLANES
    {
        return Err(Nv12Error::NotNv12);
    }
    // SAFETY: locked until the unlock below, read only.
    let status = unsafe { CVPixelBufferLockBaseAddress(image, CVPixelBufferLockFlags::ReadOnly) };
    if status != kCVReturnSuccess {
        return Err(Nv12Error::Lock(status));
    }
    let result = read_planes(image).ok_or(Nv12Error::NoPlanes).map(|planes| {
        let chroma_width = planes.width.div_ceil(2);
        let chroma_height = planes.height.div_ceil(2);
        split_chroma(
            planes.chroma,
            planes.chroma_stride,
            chroma_width,
            chroma_height,
            &mut chroma.u,
            &mut chroma.v,
        );
        use_picture(&I420 {
            width: planes.width as u32,
            height: planes.height as u32,
            y: planes.luma,
            u: &chroma.u,
            v: &chroma.v,
            strides: [
                planes.luma_stride as u32,
                chroma_width as u32,
                chroma_width as u32,
            ],
        })
    });
    // SAFETY: locked above, and the slices are not used past this point.
    let status = unsafe { CVPixelBufferUnlockBaseAddress(image, CVPixelBufferLockFlags::ReadOnly) };
    if status != kCVReturnSuccess {
        tracing::warn!(status, "could not unlock a frame");
    }
    result
}

/// An NV12 frame's size and planes, as they lie in a locked buffer.
struct Planes<'a> {
    width: usize,
    height: usize,
    luma: &'a [u8],
    luma_stride: usize,
    chroma: &'a [u8],
    chroma_stride: usize,
}

/// The planes of a locked buffer; the slices must end before it unlocks.
fn read_planes(image: &CVPixelBuffer) -> Option<Planes<'_>> {
    let width = CVPixelBufferGetWidthOfPlane(image, LUMA_PLANE);
    let height = CVPixelBufferGetHeightOfPlane(image, LUMA_PLANE);
    let luma = CVPixelBufferGetBaseAddressOfPlane(image, LUMA_PLANE).cast::<u8>();
    let chroma = CVPixelBufferGetBaseAddressOfPlane(image, CHROMA_PLANE).cast::<u8>();
    let luma_stride = CVPixelBufferGetBytesPerRowOfPlane(image, LUMA_PLANE);
    let chroma_stride = CVPixelBufferGetBytesPerRowOfPlane(image, CHROMA_PLANE);
    let chroma_rows = CVPixelBufferGetHeightOfPlane(image, CHROMA_PLANE);
    if luma.is_null() || chroma.is_null() || width == 0 || height == 0 {
        return None;
    }
    // SAFETY: the planes are as long as their rows times their stride while
    // the buffer is locked, which the caller holds for the slices' life.
    unsafe {
        Some(Planes {
            width,
            height,
            luma: std::slice::from_raw_parts(luma, luma_stride * height),
            luma_stride,
            chroma: std::slice::from_raw_parts(chroma, chroma_stride * chroma_rows),
            chroma_stride,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chroma_is_split_ignoring_row_padding() {
        // Two rows of two pairs, each row padded to six bytes.
        let src = [1, 2, 3, 4, 9, 9, 5, 6, 7, 8, 9, 9];
        let (mut u, mut v) = (vec![], vec![]);
        split_chroma(&src, 6, 2, 2, &mut u, &mut v);
        assert_eq!(u, [1, 3, 5, 7]);
        assert_eq!(v, [2, 4, 6, 8]);
    }

    #[test]
    fn splitting_again_replaces_the_planes() {
        let (mut u, mut v) = (vec![0; 10], vec![0; 10]);
        split_chroma(&[1, 2], 2, 1, 1, &mut u, &mut v);
        assert_eq!((u, v), (vec![1], vec![2]));
    }
}
