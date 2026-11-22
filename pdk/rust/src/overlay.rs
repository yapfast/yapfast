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

/// Takes the layer away, and its frames stop.
pub fn clear() -> Result<()> {
    call::ok(method::OVERLAY_CLEAR, serde_json::json!({}))?;
    Ok(())
}
