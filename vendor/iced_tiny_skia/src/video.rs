//! Yapfast port (patch 13): pictures the app draws into the frame itself.
//!
//! A video frame is a new picture every frame, at the size it was made at,
//! in the channel order its decoder wrote: a watched stream's frames come
//! straight RGBA from the stream worker's shared mapping, a camera's from the
//! core's scaler. Stock, each new handle would be converted into a
//! premultiplied copy in the raster cache, and drawn scaled through
//! tiny-skia's pattern pipeline; patch 1 drops the copy but takes the bytes
//! as BGRA. The app sets a hook that recognises its frames by their memory
//! and copies or scales them into the frame (libyuv, SIMD), touching only
//! the part of the target inside the clip. Anything the hook declines draws
//! as before.

use std::sync::OnceLock;

use crate::core::Rectangle;

/// A picture to draw, and where.
pub struct Draw<'a> {
    /// The picture's pixels as its handle holds them, `width * 4` bytes a row.
    pub source: &'a [u8],
    pub width: u32,
    pub height: u32,
    /// Where it's drawn, in the target's pixels; it may reach outside it.
    pub x: i32,
    pub y: i32,
    pub to_width: u32,
    pub to_height: u32,
    /// The part of the target to write: x, y, width and height, inside the
    /// target and the drawn rectangle; empty when nothing shows.
    pub clip: [u32; 4],
    /// The frame, premultiplied BGRA, `target_width * 4` bytes a row.
    pub target: &'a mut [u8],
    pub target_width: u32,
}

/// Draws the picture and says so, or returns false for the stock path.
pub type Hook = fn(&mut Draw<'_>) -> bool;

static HOOK: OnceLock<Hook> = OnceLock::new();

/// Sets the hook, once.
pub fn set_hook(hook: Hook) {
    let _ = HOOK.set(hook);
}

/// Offers a picture drawn at `bounds`, in the target's pixels, to the hook.
pub(crate) fn draw(
    source: &[u8],
    width: u32,
    height: u32,
    bounds: Rectangle,
    clip_bounds: Rectangle,
    pixels: &mut tiny_skia::PixmapMut<'_>,
) -> bool {
    let Some(hook) = HOOK.get() else {
        return false;
    };
    let (x, y) = (bounds.x.round() as i32, bounds.y.round() as i32);
    let (to_width, to_height) = (
        bounds.width.round().max(0.0) as u32,
        bounds.height.round().max(0.0) as u32,
    );
    let (target_width, target_height) = (pixels.width(), pixels.height());
    let left = x
        .max(clip_bounds.x.floor() as i32)
        .max(0);
    let top = y
        .max(clip_bounds.y.floor() as i32)
        .max(0);
    let right = (x + to_width as i32)
        .min((clip_bounds.x + clip_bounds.width).ceil() as i32)
        .min(target_width as i32);
    let bottom = (y + to_height as i32)
        .min((clip_bounds.y + clip_bounds.height).ceil() as i32)
        .min(target_height as i32);
    let clip = if right > left && bottom > top {
        [left as u32, top as u32, (right - left) as u32, (bottom - top) as u32]
    } else {
        [0; 4]
    };
    hook(&mut Draw {
        source,
        width,
        height,
        x,
        y,
        to_width,
        to_height,
        clip,
        target: pixels.data_mut(),
        target_width,
    })
}
