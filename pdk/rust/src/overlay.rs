//! A layer of the plugin's own pictures over the window, which a cat that
//! follows the pointer is drawn on: `overlay`.
//!
//! [`set`] says what the layer shows and how many frames a second the plugin
//! wants; each frame arrives at [`Plugin::input`](crate::Plugin::input) as
//! [`Input::OverlayFrame`](crate::input::Input::OverlayFrame), with the pointer
//! and the window's size, and the plugin answers it with the next [`set`]. A
//! plugin slower than its pace is given frames at its own. Clicks pass through
//! the layer except on a sprite made [`Sprite::clickable`], which arrives as
//! `OverlayClick`.

use serde::Serialize;

use crate::Result;
use crate::call::{self, method};

/// One picture on the layer: a picture asset of the package, where its top
/// left goes in the window's logical pixels, and optionally its size, the part
/// of a sheet to show (`[x, y, width, height]` in its own pixels) and its
/// opacity.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Sprite {
    pub id: String,
    pub image: String,
    pub x: f32,
    pub y: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub crop: Option<[u32; 4]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opacity: Option<f32>,
    pub clickable: bool,
}

/// What the layer shows now, and the frames a second wanted: 0 for none, at
/// most 60.
pub fn set(sprites: &[Sprite], fps: u8) -> Result<()> {
    call::ok(
        method::OVERLAY_SET,
        serde_json::json!({ "sprites": sprites, "fps": fps }),
    )?;
    Ok(())
}

/// What the layer shows, its frames a second, and whether a frame also comes
/// each time the pointer moves (at any pace, 0 included).
pub fn set_with_pointer(sprites: &[Sprite], fps: u8, pointer: bool) -> Result<()> {
    call::ok(
        method::OVERLAY_SET,
        serde_json::json!({ "sprites": sprites, "fps": fps, "pointer": pointer }),
    )?;
    Ok(())
}

/// A picture from bytes the plugin holds (a sheet the person chose, one it
/// fetched or kept in storage): answers the name a [`Sprite::image`] uses,
/// `held:<n>`. PNG, JPEG, GIF or WebP, at most 8 MiB; a plugin holds 16 and a
/// new one past that lets the oldest go.
pub fn picture(bytes: &[u8]) -> Result<String> {
    let answer = call::ok(
        method::OVERLAY_PICTURE,
        serde_json::json!({ "bytes_base64": crate::wire::base64::encode(bytes) }),
    )?;
    answer
        .get("image")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| crate::Error::msg("`overlay.picture` answered no name"))
}

/// Takes the layer away, and its frames stop.
pub fn clear() -> Result<()> {
    call::ok(method::OVERLAY_CLEAR, serde_json::json!({}))?;
    Ok(())
}
