//! A stream in picture in picture: a stream or a camera kept in a corner of
//! the window while its call isn't on screen, as Discord's is
//! (`StreamPip`, `ui/voice.slint:676`; `StreamWaiting`, `:658` is
//! [`tiles::waiting`](crate::tiles::waiting)). Task V06
//! (docs/iced/tasks/voice.md).
//!
//! The shell opens it as the layer `Layer::Pip`, in the corner it keeps
//! (`shell/layers`); this is the card, 320 by 180 px, black, with its 1 px
//! edge and 8 px corners. It shows the newest picture of what it follows, or
//! until the first the ring and what it waits for; the LIVE tag when this
//! account's screen is live and who watches it; and under the pointer a
//! shade, the title and the stop button. A click returns to the call, and a
//! drag moves the card, as [`grab`] says: it is drawn lifted off its corner
//! by [`Pip::drag`], over the window, until the app sets it in the nearest
//! corner.
//!
//! Slint glides the card to its corner over `SLIDE` once let go; the app
//! sets it down at once, so there is no glide.

mod grab;
mod parts;

use iced::widget::float;

use anomi_theme::prelude::*;
use anomi_vm::voice::pip::{Intent, Pip};

/// The card's 1 px edge and its corners (`:681`, `:684`).
const EDGE: f32 = 1.0;
const RADIUS: f32 = 8.0;

pub fn view(pip: &Pip, motion: Motion) -> impl View<Intent> + '_ {
    let drag = pip.drag;
    let card = grab::grab(
        parts::base(pip, motion),
        parts::marks(pip, motion),
        parts::hover(pip, motion),
        drag,
    );
    // Over the rest of the window while it is dragged, as Slint draws it
    // at an offset of its own place.
    float(card).translate(move |_, _| drag)
}

/// Its shots for `anomi-shots`, each the whole window from a fixture (see
/// `crate::shots`). `stream_pip_over_text_channel` has a golden; the rest
/// have none: compare them with the Slint app under `scripts/ui-session`.
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    use crate::shots::{GOLDEN, Shot};
    use anomi_vm::fixtures::voice::pip as sample;
    let shot = |name, vm| Shot {
        area: "voice",
        name,
        size: GOLDEN,
        theme: "bonfire",
        vm,
    };
    vec![
        shot("stream_pip_over_text_channel", sample::watching_window),
        shot("stream_pip_waiting", sample::waiting_window),
        shot("stream_pip_own_screen", sample::sharing_window),
        shot("stream_pip_camera", sample::camera_window),
        shot("stream_pip_in_corner", sample::bottom_left_window),
    ]
}
