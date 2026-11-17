//! The participant grid: the call's tiles on the stage. Task V04
//! (docs/iced/tasks/voice.md). The Slint side is `ParticipantGrid`
//! (`ui/voice.slint:857`), placed over `CallControls` under the call's line
//! (`ui/yapfast.slint:3515`-`:3550`) and alone over the whole window full
//! screen (`:4597`); the glue is `show_stage` (`src/ui/refresh.rs:4541`),
//! `src/ui/tiles.rs` and the `call-tiles` and `focus-tile` callbacks
//! (`src/ui/callbacks.rs:3759`, `:3789`).
//!
//! [`view`] draws the grid in the room it gets: each tile is V03's
//! `tiles::view`, laid out by `layout` (the Slint properties, line by line)
//! and gliding to a new place over `SLIDE` (`glide`), in a widget of its
//! own (`grid`, `widget`) as only a layout knows the room. With a tile
//! large the others sit small in rows beneath it, or nowhere while hidden
//! or full screen, where the large one fills the grid edge to edge. Tiles
//! taller than the room scroll under the wheel; a click beside the large
//! tile shows them all again. As where the tiles are changes, the grid
//! tells the app (`Intent::Placed`), which decodes cameras only for tiles
//! in sight, at their size; only the tiles that show are built.
//!
//! [`with_controls`] is the call as the conversation shows it: the grid
//! over V05's floating pill. Full screen, the layer shows [`view`] alone.
//!
//! Nothing ticks: a glide asks for frames only while it moves and the
//! window plays, and a still window places tiles at once.

mod glide;
mod grid;
mod layout;
#[cfg(feature = "shots")]
mod shots;
mod widget;

use iced::Fill;
use iced::widget::column;

use anomi_theme::prelude::*;
use anomi_vm::voice::{self, Voice, stage::Stage};

pub use layout::{Arrangement, Geometry};

#[cfg(feature = "shots")]
pub use shots::shots;

/// The grid of `stage`'s tiles, filling the room it's given.
pub fn view(stage: &Stage, motion: Motion) -> impl View<voice::Intent> + '_ {
    grid::grid(stage, motion)
}

/// The call in the conversation: the grid, and V05's pill under it
/// (`ui/yapfast.slint:3520`, `:3537`).
pub fn with_controls(voice: &Voice, motion: Motion) -> impl View<voice::Intent> + '_ {
    column![
        view(&voice.stage, motion),
        crate::controls::view(&voice.controls, motion).map(voice::Intent::from),
    ]
    .width(Fill)
    .height(Fill)
}
