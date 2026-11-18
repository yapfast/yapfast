//! Pictures the core decodes, kept in buffers iced draws without a copy;
//! which of them move; asking for those the rows on screen show; and the
//! frames of moving pictures such as a camera in a call. Ported from
//! `src/ui/pictures.rs` (`decode_into_slint_buffers`, `cached_picture`,
//! `avatar_animation`, `animated_url`, `make_frames_in_slint_buffers`) and
//! the `PictureStore.request` of `src/ui/callbacks.rs`.
//!
//! The core makes every picture's pixels with the function it's handed
//! ([`yapfast_core::chat::set_pixel_buffers`]), as the Slint app hands it
//! one making Slint's buffers. Here a buffer holds its pixels as shared
//! bytes and an iced handle over the same bytes, made once, so each row
//! that shows the picture clones the handle and the renderer caches it by
//! the handle's id. The pixels are copied once, as they come.
//!
//! What's made from a picture to draw it (a round copy at the size shown,
//! a spoiler's blur, a moving picture's frames) is the widget's, kept
//! within the eight MiB of `anomi_widgets::picture::BUDGET`; the core's own
//! pictures are the core's to let go of.

mod frames;
mod moving;
mod requests;
mod scale;
mod watch;

use std::cell::Cell;
use std::sync::Arc;
use std::time::Instant;

use bytes::Bytes;
use iced::Task;
use iced::widget::image;
use yapfast_core::application::AppState;
use yapfast_core::chat::{self, Picture, PixelBuffer};

use crate::app::{App, later};
use crate::effects;
use crate::message::Message;

pub use frames::frame;
pub use requests::Requests;
pub use watch::{Watch, noting};

/// A picture's pixels and the handle that draws them.
struct Drawn {
    handle: image::Handle,
    pixels: Bytes,
}

impl PixelBuffer for Drawn {
    fn bytes(&self) -> &[u8] {
        &self.pixels
    }
}

fn make(width: u32, height: u32, rgba: &[u8]) -> Box<dyn PixelBuffer> {
    let pixels = Bytes::copy_from_slice(rgba);
    Box::new(Drawn {
        handle: image::Handle::from_rgba(width, height, pixels.clone()),
        pixels,
    })
}

/// Has the core decode every picture from now on into buffers iced draws,
/// and make moving pictures' frames in them, which the renderer scales
/// with libyuv where they're drawn at another size; has the widgets find
/// which pictures move. Call it before the backend starts.
pub fn install() {
    chat::set_pixel_buffers(make);
    yapfast_core::picture::set_frame_buffers(frames::make);
    iced_tiny_skia::video::set_hook(scale::scale);
    anomi_widgets::picture::set_animations(moving::find);
}

/// The handle that draws a picture; none for one decoded before
/// [`install`]. A moving picture's handle plays only through [`find`] or
/// [`shown`], which say it moves.
pub fn handle(picture: &Picture) -> Option<image::Handle> {
    picture
        .rgba
        .buffer::<Drawn>()
        .map(|drawn| drawn.handle.clone())
}

/// The handle that draws a picture the core holds, which plays where it
/// shows if it moves: an attachment's preview, a sticker, a GIF.
pub fn shown(picture: &Arc<Picture>) -> Option<image::Handle> {
    let handle = handle(picture)?;
    if picture.animation.is_some() {
        moving::remember(&handle, picture);
    }
    Some(handle)
}

/// The picture at an address, once it arrived. Ask for it with
/// [`Requests::want`] when its row shows and it hasn't.
pub fn find(state: &AppState, address: Option<&str>) -> Option<image::Handle> {
    let address = address?;
    watch::looked_up(address);
    state.pictures.get(address).and_then(shown)
}

/// The address of the moving version of a collectible's still picture:
/// Discord serves a decoration's or nameplate's `/static` PNG and, beside
/// it, an `/animated` APNG. Any other address is its own. While the
/// profile switches hold collectibles still, a collectible has none, so its
/// APNG is never asked for. `animated_url` in `src/ui/pictures.rs`.
pub fn animated_url(url: &str) -> String {
    match url.strip_suffix("/static") {
        Some(base) if url.contains("/collectibles-shop/") => {
            if MOVING_COLLECTIBLES.get() {
                format!("{base}/animated")
            } else {
                String::new()
            }
        }
        _ => url.to_owned(),
    }
}

/// Whether decorations and nameplates may play; see [`animated_url`].
pub fn set_moving_collectibles(moving: bool) {
    MOVING_COLLECTIBLES.set(moving);
}

thread_local! {
    static MOVING_COLLECTIBLES: Cell<bool> = const { Cell::new(true) };
}

/// Asks the core for the pictures the rows on screen want, at most
/// every 50 ms (`Requests::due`): at once when it may, else after a wait
/// that ends in [`Message::AskPictures`]. Nothing waits while nothing is
/// wanted. Asking changes nothing shown, so nothing is shown again.
pub fn ask(app: &mut App) -> Task<Message> {
    let now = Instant::now();
    let Some(due) = app.pictures.due(now) else {
        return Task::none();
    };
    if due > now {
        if app.pictures_asking {
            return Task::none();
        }
        app.pictures_asking = true;
        return later(due - now, Message::AskPictures);
    }
    let tasks: Vec<Task<Message>> = app
        .pictures
        .take(now)
        .into_iter()
        .map(|action| {
            let effects = app.core.dispatch(action);
            effects::run(app, effects)
        })
        .collect();
    Task::batch(tasks)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collectibles_move_from_their_animated_address() {
        let still = "https://cdn.discordapp.com/media/v1/collectibles-shop/5/static";
        let animated = "https://cdn.discordapp.com/media/v1/collectibles-shop/5/animated";
        // Held still by the profile switches, a collectible has none.
        set_moving_collectibles(false);
        assert_eq!(animated_url(still), "");
        set_moving_collectibles(true);
        assert_eq!(animated_url(still), animated);
        assert_eq!(animated_url(animated), animated);
        let avatar = "https://cdn.discordapp.com/avatars/5/abc.png/static";
        assert_eq!(animated_url(avatar), avatar, "only collectibles have one");
    }

    #[test]
    fn a_decoded_picture_is_drawn_from_its_own_pixels() {
        install();
        let picture = Picture {
            width: 1,
            height: 1,
            rgba: chat::Pixels::new(1, 1, vec![10, 20, 30, 255]),
            ..Default::default()
        };
        let drawn = handle(&picture).expect("made in an iced buffer");
        let image::Handle::Rgba { pixels, .. } = &drawn else {
            panic!("pixels");
        };
        assert_eq!(pixels.as_ptr(), picture.rgba.as_ptr(), "not a copy");
    }
}
