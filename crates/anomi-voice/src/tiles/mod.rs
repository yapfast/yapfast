//! A call's tile: someone's picture or camera, a stream, or this account's
//! shared screen, with its name, its marks and, under the pointer, its
//! buttons. Task V03 (docs/iced/tasks/voice.md). The Slint side:
//! `StageTile` (`ui/voice.slint:751`) and what the loops of
//! `ParticipantGrid` put in it (`:988`-`:1116`), `TileChip` (`:819`),
//! `LivePill` (`:41`), `ViewerStack` (`:60`), `StreamPicture` (`:840`) and
//! `StreamPreview` (`:849`); `StreamWaiting` (`:658`) draws a stream on its
//! way here too, for the stage and picture in picture alike.
//!
//! [`view`] fills the size its caller gives it (the stage's grid, V04) in
//! four layers, each drawn over the one before:
//!
//! - its ground and what fills it (`fill`): a picture on the raised ground,
//!   or on black a camera, a stream, a preview or this account's screen;
//! - its outline and its rounded corners (`ring`);
//! - its marks (`marks`): the camera on its way, the pill offering a stream,
//!   and its name's chip (`chip`, with `live`'s LIVE tag);
//! - its buttons (`controls`), which `swap` changes under the pointer: who
//!   watches (`viewers`), stop watching, hide the others, full screen.
//!
//! A camera's and a stream's frames are read from their `Live` as they're
//! drawn, so a frame builds no view, and drawn square and whole, never
//! copied (`anomi_widgets::picture::live`). The corners are rounded over
//! them, in the stage's colour, which every tile lies on; the renderer now
//! cuts a frame's own corners too (`vendor/iced_tiny_skia`, patch 13), for
//! task I18 to weigh against this band.
//!
//! A click watches the stream a tile offers, or else shows it large; on the
//! large one a double click goes full screen and back; a right click on
//! someone's opens their volume menu at the pointer. A press on the
//! buttons or the viewers stops there and asks the tile nothing. The
//! outline's colour eases over `QUICK`; the camera's pill beats while the
//! window plays and the waiting ring turns while someone may be looking
//! (`spinner`, `pulse`). With a still `Motion` nothing asks for a frame.

mod chip;
mod controls;
mod fill;
pub mod live;
mod marks;
mod pulse;
mod ring;
mod spinner;
mod swap;
pub mod viewers;
pub mod waiting;

use std::sync::OnceLock;
use std::time::{Duration, Instant};

use iced::widget::mouse_area;
use iced::{Color, Fill, mouse};

use anomi_theme::prelude::*;
use anomi_theme::{rgb, rgba};
use anomi_vm::menu::Spot;
use anomi_vm::voice::tiles::{Intent, Tile};
use anomi_widgets::menu;

/// A tile's corners, square over the whole window (`:769`).
const RADIUS: f32 = 12.0;
/// How far its marks and buttons sit from its edges (`:792`, `:827`).
const INSET: f32 = 10.0;
/// What lies under white text over a picture: the chip, the viewers, the
/// buttons (`:68`, `:794`, `:829`).
const SCRIM: Color = rgba(0x0000_00a6);
/// What shows on the black of a stream's tile, in every theme (`:664`,
/// `:1003`).
const GREY: Color = rgb(0xb5b8c1);
/// A pill over a tile: a stream offered, the camera on its way (`:1094`,
/// `:1100`): 26 px tall, its text 13 px bold and 10 px from its ends.
const PILL: f32 = 26.0;
const PILL_TEXT: f32 = 13.0;
const PILL_SIDE: f32 = 10.0;

/// A tile at the size it's laid out at; see the module's doc.
pub fn view(tile: &Tile, motion: Motion) -> impl View<Intent> + '_ {
    let radius = if tile.full { 0.0 } else { RADIUS };
    let layers = iced::widget::stack![
        fill::view(tile, radius, motion),
        ring::ring(tile.outline, radius, motion),
        marks::view(tile, motion),
    ]
    .width(Fill)
    .height(Fill);
    // The buttons and the viewers lie over what a press on the tile asks,
    // so one they take stops there, the right button's too.
    iced::widget::stack![
        pressed(tile, layers),
        swap::swap(
            controls::view(tile, false, motion),
            controls::view(tile, true, motion),
        ),
    ]
    .width(Fill)
    .height(Fill)
}

/// What a press on the tile's `layers` asks: see the module's doc.
fn pressed<'a>(tile: &'a Tile, layers: impl View<Intent> + 'a) -> Element<'a, Intent> {
    let mut area = mouse_area(layers);
    if let Some(click) = tile.click() {
        area = area.on_press(click);
    }
    if tile.focused {
        area = area.on_double_click(Intent::FullScreen(!tile.full));
    } else {
        area = area.interaction(mouse::Interaction::Pointer);
    }
    let opener = menu::opener(area);
    match tile.key.participant() {
        Some(id) => {
            let id = id.to_owned();
            opener
                .on_right_press(move |at| Intent::Menu(id.clone(), Spot::pointer(at)))
                .boxed()
        }
        None => opener.boxed(),
    }
}

/// How long the interface has been running at `now`, which a turning ring
/// and a beating pill take their place from, so all of them move together
/// as Slint's `animation-tick()` did.
fn tick(now: Instant) -> Duration {
    static START: OnceLock<Instant> = OnceLock::new();
    now.saturating_duration_since(*START.get_or_init(|| now))
}

/// Its shots for `anomi-shots`: none of its own. A tile draws only where
/// the stage (V04) or picture in picture (V06) places it, so the goldens
/// it answers to (`voice/stage`, `voice/speaking_ring`,
/// `voice/with_camera`, `voice/watching_stream`) are the stage's shots,
/// made from `anomi_vm::fixtures::voice::tiles`; its tests draw those
/// tiles at the goldens' places (`tests/areas/tiles.rs`).
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    Vec::new()
}
