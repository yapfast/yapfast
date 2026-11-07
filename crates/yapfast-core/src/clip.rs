//! Favourite GIFs' thumbnails: an animation encoded as VP9 at a constant
//! quality, through native/clip.c and the libvpx inside libwebrtc, in a WebM
//! file of its own. Yapfast plays it as any video, at a fraction of a GIF's size
//! and decoding work: a tenth of a WebP's size, measured on Tenor's GIFs.

use std::ffi::{c_int, c_longlong, c_uchar, c_uint, c_ulong, c_void};

use livekit::webrtc::native::yuv_helper;

use crate::chat::Picture;

/// libvpx's constant quality level: 0 best, 63 smallest.
const QUALITY: u32 = 36;
/// What shows through a transparent GIF, which VP9 can't keep: the panel's
/// tone.
const BACKGROUND: [u8; 3] = [0x2b, 0x2d, 0x31];

#[repr(C)]
struct Raw {
    _private: [u8; 0],
}

type Packet = extern "C" fn(*mut c_void, *const c_uchar, usize, c_longlong, c_int);

unsafe extern "C" {
    fn yapfast_clip_new(
        width: c_uint,
        height: c_uint,
        quality: c_uint,
        threads: c_uint,
    ) -> *mut Raw;
    #[allow(clippy::too_many_arguments)]
    fn yapfast_clip_encode(
        clip: *mut Raw,
        y: *const c_uchar,
        y_stride: c_int,
        u: *const c_uchar,
        u_stride: c_int,
        v: *const c_uchar,
        v_stride: c_int,
        pts: c_longlong,
        duration: c_ulong,
        packet: Packet,
        user: *mut c_void,
    ) -> c_int;
    fn yapfast_clip_free(clip: *mut Raw);
}

/// The encoder, freed when dropped.
struct Clip(*mut Raw);

impl Drop for Clip {
    fn drop(&mut self) {
        // SAFETY: the pointer came from yapfast_clip_new and is freed once.
        unsafe { yapfast_clip_free(self.0) }
    }
}

/// An encoded frame, shown from `time`, in milliseconds.
struct Encoded {
    time: u64,
    keyframe: bool,
    data: Vec<u8>,
}

extern "C" fn take(
    user: *mut c_void,
    data: *const c_uchar,
    size: usize,
    pts: c_longlong,
    key: c_int,
) {
    // SAFETY: `user` is the frame list `encode` passes, borrowed for the
    // call, and libvpx's packet is `size` bytes at `data` until it returns.
    let (frames, data) = unsafe {
        (
            &mut *user.cast::<Vec<Encoded>>(),
            std::slice::from_raw_parts(data, size),
        )
    };
    frames.push(Encoded {
        time: u64::try_from(pts).unwrap_or(0),
        keyframe: key != 0,
        data: data.to_vec(),
    });
}

/// `picture`'s animation as a WebM file at the picture's size, every frame
/// shown as long as it was; none for a still picture, or when encoding fails.
pub fn thumbnail(picture: &Picture) -> Option<Vec<u8>> {
    let animation = picture.animation.as_ref()?;
    let (width, height) = (picture.width, picture.height);
    // SAFETY: plain values; null means libvpx refused them.
    let raw = unsafe { yapfast_clip_new(width, height, QUALITY, 1) };
    if raw.is_null() {
        return None;
    }
    let clip = Clip(raw);
    let chroma_width = width.div_ceil(2);
    let pixels = (width * height) as usize;
    let chroma = (chroma_width * height.div_ceil(2)) as usize;
    let mut planes = vec![0; pixels + 2 * chroma];
    let mut frames: Vec<Encoded> = Vec::with_capacity(animation.delays.len());
    let encode = |planes: Option<&[u8]>, time: u64, delay: u32, frames: &mut Vec<Encoded>| {
        let (y, u, v) = match planes {
            Some(planes) => {
                let (y, chroma_planes) = planes.split_at(pixels);
                let (u, v) = chroma_planes.split_at(chroma);
                (y.as_ptr(), u.as_ptr(), v.as_ptr())
            }
            None => (std::ptr::null(), std::ptr::null(), std::ptr::null()),
        };
        // SAFETY: the planes are the encoder's size, or null to finish;
        // `frames` outlives the call, which is all `take` borrows it for.
        let encoded = unsafe {
            yapfast_clip_encode(
                clip.0,
                y,
                width as c_int,
                u,
                chroma_width as c_int,
                v,
                chroma_width as c_int,
                time as c_longlong,
                c_ulong::from(delay),
                take,
                std::ptr::from_mut(frames).cast(),
            )
        };
        (encoded >= 0).then_some(())
    };
    let (mut time, mut count) = (0, 0);
    for (mut rgba, &delay) in picture.frames().zip(&animation.delays) {
        flatten(&mut rgba);
        let (y, chroma_planes) = planes.split_at_mut(pixels);
        let (u, v) = chroma_planes.split_at_mut(chroma);
        // libyuv's ABGR is R, G, B, A in memory.
        yuv_helper::abgr_to_i420(
            &rgba,
            width * 4,
            y,
            width,
            u,
            chroma_width,
            v,
            chroma_width,
            width as i32,
            height as i32,
        );
        encode(Some(&planes), time, delay, &mut frames)?;
        time += u64::from(delay);
        count += 1;
    }
    encode(None, 0, 0, &mut frames)?;
    // Every frame came out, and they make an animation.
    (frames.len() == count && count > 1).then_some(())?;
    Some(webm(width, height, &frames, time))
}

/// Puts transparent pixels over [`BACKGROUND`].
fn flatten(rgba: &mut [u8]) {
    for pixel in rgba.chunks_exact_mut(4) {
        let alpha = u16::from(pixel[3]);
        if alpha == 255 {
            continue;
        }
        for (channel, background) in pixel.iter_mut().zip(BACKGROUND) {
            *channel =
                ((u16::from(*channel) * alpha + u16::from(background) * (255 - alpha)) / 255) as u8;
        }
        pixel[3] = 255;
    }
}

// The Matroska elements a WebM file of one video track needs.
const EBML: u32 = 0x1A45_DFA3;
const EBML_VERSION: u32 = 0x4286;
const EBML_READ_VERSION: u32 = 0x42F7;
const EBML_MAX_ID_LENGTH: u32 = 0x42F2;
const EBML_MAX_SIZE_LENGTH: u32 = 0x42F3;
const DOC_TYPE: u32 = 0x4282;
const DOC_TYPE_VERSION: u32 = 0x4287;
const DOC_TYPE_READ_VERSION: u32 = 0x4285;
const SEGMENT: u32 = 0x1853_8067;
const INFO: u32 = 0x1549_A966;
const TIMESTAMP_SCALE: u32 = 0x2A_D7B1;
const DURATION: u32 = 0x4489;
const MUXING_APP: u32 = 0x4D80;
const WRITING_APP: u32 = 0x5741;
const TRACKS: u32 = 0x1654_AE6B;
const TRACK_ENTRY: u32 = 0xAE;
const TRACK_NUMBER: u32 = 0xD7;
const TRACK_UID: u32 = 0x73C5;
const TRACK_TYPE: u32 = 0x83;
const FLAG_LACING: u32 = 0x9C;
const CODEC_ID: u32 = 0x86;
const VIDEO: u32 = 0xE0;
const PIXEL_WIDTH: u32 = 0xB0;
const PIXEL_HEIGHT: u32 = 0xBA;
const CLUSTER: u32 = 0x1F43_B675;
const TIMESTAMP: u32 = 0xE7;
const SIMPLE_BLOCK: u32 = 0xA3;

/// A block's time is 16 bits from its cluster's, so a cluster spans less.
const CLUSTER_MS: u64 = 30_000;

/// A WebM file of VP9 `frames`, each shown from its time until the next's
/// and the last until `duration`, in milliseconds. Each element's size is
/// worked out before it's written, so the frames are copied once.
fn webm(width: u32, height: u32, frames: &[Encoded], duration: u64) -> Vec<u8> {
    let mut header = Vec::new();
    uint(&mut header, EBML_VERSION, 1);
    uint(&mut header, EBML_READ_VERSION, 1);
    uint(&mut header, EBML_MAX_ID_LENGTH, 4);
    uint(&mut header, EBML_MAX_SIZE_LENGTH, 8);
    bytes(&mut header, DOC_TYPE, b"webm");
    uint(&mut header, DOC_TYPE_VERSION, 4);
    uint(&mut header, DOC_TYPE_READ_VERSION, 2);
    let mut info = Vec::new();
    uint(&mut info, TIMESTAMP_SCALE, 1_000_000);
    head(&mut info, DURATION, 8);
    info.extend((duration as f64).to_be_bytes());
    bytes(&mut info, MUXING_APP, b"RatFast");
    bytes(&mut info, WRITING_APP, b"RatFast");
    let mut video = Vec::new();
    uint(&mut video, PIXEL_WIDTH, width.into());
    uint(&mut video, PIXEL_HEIGHT, height.into());
    let mut track = Vec::new();
    uint(&mut track, TRACK_NUMBER, 1);
    uint(&mut track, TRACK_UID, 1);
    uint(&mut track, TRACK_TYPE, 1);
    uint(&mut track, FLAG_LACING, 0);
    bytes(&mut track, CODEC_ID, b"V_VP9");
    bytes(&mut track, VIDEO, &video);
    let mut tracks = Vec::new();
    bytes(&mut tracks, TRACK_ENTRY, &track);

    let mut clusters: Vec<&[Encoded]> = Vec::new();
    let mut rest = frames;
    while let Some(first) = rest.first() {
        let count = rest
            .iter()
            .position(|frame| frame.time - first.time >= CLUSTER_MS)
            .unwrap_or(rest.len());
        let (cluster, after) = rest.split_at(count);
        clusters.push(cluster);
        rest = after;
    }
    let block = |frame: &Encoded| 4 + frame.data.len() as u64;
    let cluster_size = |cluster: &[Encoded]| {
        element_size(TIMESTAMP, uint_size(cluster[0].time))
            + cluster
                .iter()
                .map(|frame| element_size(SIMPLE_BLOCK, block(frame)))
                .sum::<u64>()
    };
    let segment = element_size(INFO, info.len() as u64)
        + element_size(TRACKS, tracks.len() as u64)
        + clusters
            .iter()
            .map(|cluster| element_size(CLUSTER, cluster_size(cluster)))
            .sum::<u64>();

    let mut out =
        Vec::with_capacity((element_size(EBML, header.len() as u64) + segment + 12) as usize);
    bytes(&mut out, EBML, &header);
    head(&mut out, SEGMENT, segment);
    bytes(&mut out, INFO, &info);
    bytes(&mut out, TRACKS, &tracks);
    for cluster in clusters {
        let start = cluster[0].time;
        head(&mut out, CLUSTER, cluster_size(cluster));
        uint(&mut out, TIMESTAMP, start);
        for frame in cluster {
            head(&mut out, SIMPLE_BLOCK, block(frame));
            // Track 1, as a size-coded number.
            out.push(0x81);
            out.extend(((frame.time - start) as i16).to_be_bytes());
            out.push(if frame.keyframe { 0x80 } else { 0 });
            out.extend_from_slice(&frame.data);
        }
    }
    out
}

fn id_size(id: u32) -> u64 {
    u64::from(4 - id.leading_zeros() / 8)
}

/// How many bytes a size takes: 7 bits of each, the all-ones value of each
/// length meaning unknown.
fn length_size(size: u64) -> u64 {
    (1..8)
        .find(|&bytes| size < (1 << (7 * bytes)) - 1)
        .unwrap_or(8)
}

fn element_size(id: u32, size: u64) -> u64 {
    id_size(id) + length_size(size) + size
}

fn uint_size(value: u64) -> u64 {
    u64::from((8 - value.leading_zeros() / 8).max(1))
}

/// An element's id and size, before its contents.
fn head(out: &mut Vec<u8>, id: u32, size: u64) {
    out.extend(&id.to_be_bytes()[4 - id_size(id) as usize..]);
    let length = length_size(size);
    let marked = size | (1 << (7 * length));
    out.extend(&marked.to_be_bytes()[8 - length as usize..]);
}

fn bytes(out: &mut Vec<u8>, id: u32, contents: &[u8]) {
    head(out, id, contents.len() as u64);
    out.extend_from_slice(contents);
}

fn uint(out: &mut Vec<u8>, id: u32, value: u64) {
    let size = uint_size(value);
    head(out, id, size);
    out.extend(&value.to_be_bytes()[8 - size as usize..]);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A GIF of `delays.len()` frames, each a shade of red.
    fn gif(width: u32, height: u32, delays: &[u32], alpha: u8) -> Vec<u8> {
        let mut data = Vec::new();
        {
            let mut encoder = image::codecs::gif::GifEncoder::new(&mut data);
            for (index, delay) in delays.iter().enumerate() {
                let shade = image::Rgba([60 + index as u8 * 60, 20, 20, alpha]);
                encoder
                    .encode_frame(image::Frame::from_parts(
                        image::RgbaImage::from_pixel(width, height, shade),
                        0,
                        0,
                        image::Delay::from_numer_denom_ms(*delay, 1),
                    ))
                    .unwrap();
            }
        }
        data
    }

    #[test]
    fn makes_a_webm_yapfast_plays_as_the_gif_did() {
        // An odd size, and a long last frame, as many GIFs end on.
        let delays = [50, 100, 1500];
        let picture = crate::chat::decode_picture(&gif(31, 17, &delays, 255), 64, 64).unwrap();
        let webm = thumbnail(&picture).unwrap();
        assert_eq!(&webm[..4], &[0x1A, 0x45, 0xDF, 0xA3]);
        let played = crate::video::decode_animation(&webm, 64, 64).unwrap();
        assert_eq!((played.width, played.height), (31, 17));
        assert_eq!(played.animation.as_ref().unwrap().delays, delays);
        let frames: Vec<_> = played.frames().collect();
        assert_eq!(frames.len(), 3);
        // Each frame's shade, within what the encoder loses.
        for (index, frame) in frames.iter().enumerate() {
            let red = i32::from(frame[(8 * 31 + 15) * 4]);
            assert!(
                (red - (60 + index as i32 * 60)).abs() < 12,
                "frame {index}: {red}"
            );
        }
    }

    #[test]
    fn puts_transparency_over_the_background() {
        let mut rgba = vec![200, 100, 0, 0, 200, 100, 0, 255, 255, 255, 255, 128];
        flatten(&mut rgba);
        assert_eq!(&rgba[..4], &[0x2b, 0x2d, 0x31, 255]);
        assert_eq!(&rgba[4..8], &[200, 100, 0, 255]);
        assert_eq!(rgba[11], 255);
        assert!((140..=150).contains(&rgba[8]));
    }

    #[test]
    fn sizes_elements_as_matroska_does() {
        let mut out = Vec::new();
        uint(&mut out, TRACK_NUMBER, 1);
        assert_eq!(out, [0xD7, 0x81, 0x01]);
        out.clear();
        head(&mut out, SEGMENT, 200);
        assert_eq!(out, [0x18, 0x53, 0x80, 0x67, 0x40, 200]);
        // 127 would mean an unknown size in one byte.
        assert_eq!(length_size(126), 1);
        assert_eq!(length_size(127), 2);
        assert_eq!(element_size(SIMPLE_BLOCK, 1000), 1 + 2 + 1000);
    }
}
