//! The settings window: a card over the dimmed window with the sections on
//! its left and the page, under a bar with its title and the close button,
//! on its right. The Slint side: `SettingsScreen`, `SettingsFrame` and
//! `SettingsCard` (`ui/settings.slint:582`, `:408`, `:555`) in Discord's
//! layout, the only one Anomi has.
//!
//! - [`window`] draws the whole settings: the frame with the page `page`
//!   names, from the pages' own view-models. The shell calls it for
//!   `Layer::Settings` (its stub calls [`view`], the frame alone, which
//!   shows no page).
//! - **Escape** and a click on the dimmed window close it: the layer's, with
//!   the way in and out and the focus kept inside. A press on the gutter of
//!   the card sends [`Intent::Close`] too, as the card is the layer's whole
//!   room.
//! - **The page** scrolls under the bar, in a column at most 696 px wide
//!   and centred, 24 px under the bar and 56 above the bottom; a page opens
//!   at its top. A bar for unsaved changes lies over the bottom.
//! - **The sections** scroll between the search and the build lines, so a
//!   small window still reaches the last.

pub(crate) mod bar;
mod nav;
mod page;
mod pane;
#[cfg(feature = "shots")]
mod shots;
mod stamp;

use iced::widget::space;

use anomi_theme::prelude::*;
use anomi_vm::settings::frame::{Frame, Intent as FrameIntent};
use anomi_vm::settings::{self, Settings};

pub use page::{icon, label};
pub use pane::{Pane, pane};
#[cfg(feature = "shots")]
pub use shots::shots;

/// The whole settings window: the frame with the page the frame names.
pub fn window(settings: &Settings, motion: Motion) -> impl View<settings::Intent> + '_ {
    frame(
        &settings.frame,
        move || page::body(settings, motion),
        motion,
    )
}

/// The frame alone, its page empty: what the shell shows until it calls
/// [`window`].
pub fn view(frame_vm: &Frame, motion: Motion) -> impl View<settings::Intent> + '_ {
    frame(frame_vm, || space().boxed(), motion)
}

fn frame<'a>(
    vm: &'a Frame,
    body: impl Fn() -> Element<'a, settings::Intent> + 'a,
    motion: Motion,
) -> Element<'a, settings::Intent> {
    let intent = |intent: FrameIntent| settings::Intent::from(intent);
    pane(
        move || {
            nav::navigation(vm, motion)
                .map(settings::Intent::from)
                .boxed()
        },
        body,
        page::label(vm.page),
        &vm.close,
        intent(FrameIntent::Close),
        motion,
    )
    .key(vm.page as u64)
    .unsaved(
        vm.unsaved.as_ref(),
        intent(FrameIntent::Undo),
        intent(FrameIntent::Save),
    )
    .view()
}
