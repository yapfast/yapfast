//! The sizes a stream is shown and sent at: how large a shared screen goes out,
//! the box a preview of it fits in, and the tile a watched stream is scaled to.

/// The largest frame shared at 1080p: the box a stream's picture is scaled down
/// to fit, which turns with a screen on its side — see [`fit`]. Discord's own
/// client trials 1080p at 30 fps for accounts without Nitro, up to 5 Mbit/s (its
/// "frontier tuning" experiment); larger screens are scaled down to it.
pub const SHARE_WIDTH: u32 = 1920;
pub const SHARE_HEIGHT: u32 = 1080;
/// The longest edge a preview drawn on the card is scaled to. It is the
/// stream's own picture, shown where the card drew it — the only copy that
/// costs nothing a frame — so it is worth the stream's pixels: 1920 is the
/// largest a 1080p share sends, and a portrait screen's 1080x1920 fits the
/// same box stood up. A share of a larger screen sends its picture at this
/// edge too, so the preview and the stream agree.
#[cfg(any(wgc, dup))]
pub const PREVIEW_LONG_EDGE: u32 = 1920;
/// The largest picture shown of someone else's stream; larger ones are
/// scaled down to it.
pub const WATCH_WIDTH: u32 = 1920;
pub const WATCH_HEIGHT: u32 = 1080;

/// The box a preview is scaled into when the machine is short of memory, which
/// is smaller than any tile it is shown in and never upscales: what it saves is
/// the texture, the readback and the scale itself, not the picture that goes out.
#[cfg(any(wgc, dup))]
pub const CONSTRAINED_WIDTH: u32 = 640;
#[cfg(any(wgc, dup))]
pub const CONSTRAINED_HEIGHT: u32 = 360;

/// How large a shared screen is sent, as Discord's client offers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Resolution {
    P720,
    #[default]
    P1080,
    /// The screen's own size, up to 4K.
    Source,
}

/// What a stream is scaled to fit, given the size in pixels of the tile
/// showing it, as the UI says it: none while no tile shows it, which says
/// nothing about the size, and never more than a stream is shown at, however
/// large the window is.
pub fn tile_size((width, height): (u32, u32)) -> Option<(u32, u32)> {
    (width > 0 && height > 0).then(|| (width.min(WATCH_WIDTH), height.min(WATCH_HEIGHT)))
}

/// The largest frame shared at `resolution`; screens of another shape fit
/// inside it.
pub fn bounds(resolution: Resolution) -> (u32, u32) {
    match resolution {
        Resolution::P720 => (1280, 720),
        Resolution::P1080 => (SHARE_WIDTH, SHARE_HEIGHT),
        Resolution::Source => (3840, 2160),
    }
}

/// The size `width` by `height` takes when scaled down to fit `max_width`
/// by `max_height`, keeping its shape. Codecs want even sizes.
///
/// The box is a landscape frame — a stream's resolution is named that way — and
/// it turns with the picture, because a screen on its side is the same screen:
/// a 1080x1920 display shared at 1080p is 1080x1920, where a box that stayed
/// on its feet would leave it 606x1080 and a third of its width behind. A
/// square box fits either way round.
pub fn fit(width: u32, height: u32, max_width: u32, max_height: u32) -> (u32, u32) {
    let (width, height) = (width.max(1), height.max(1));
    let (max_width, max_height) = if (width > height) == (max_width > max_height) {
        (max_width, max_height)
    } else {
        (max_height, max_width)
    };
    let scale = f64::min(
        1.0,
        f64::min(
            f64::from(max_width) / f64::from(width),
            f64::from(max_height) / f64::from(height),
        ),
    );
    let even = |value: f64| ((value as u32) & !1).max(2);
    (
        even(f64::from(width) * scale),
        even(f64::from(height) * scale),
    )
}

/// The size a preview of a `width` by `height` frame is carried at: the stream's
/// own picture, scaled down to [`PREVIEW_LONG_EDGE`] at most.
///
/// A preview drawn on the card costs nothing a frame — it is the same scale that
/// fills the texture the UI shows — so it keeps every pixel the stream has, a
/// portrait screen included. The pixels read back for a stream the card can't
/// share a picture with stop at the preview's 1280 by 720, since
/// that copy is made on the processor for every frame it is made for. A machine
/// short of memory takes the smaller box as well, whatever the path: the tile
/// is the one thing a share can give up, and the stream it sends is untouched.
#[cfg(any(wgc, dup))]
pub fn preview_size(width: u32, height: u32, constrained: bool) -> (u32, u32) {
    if constrained {
        return fit(width, height, CONSTRAINED_WIDTH, CONSTRAINED_HEIGHT);
    }
    fit(width, height, PREVIEW_LONG_EDGE, PREVIEW_LONG_EDGE)
}
