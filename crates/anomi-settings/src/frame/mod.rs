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

mod bar;
mod nav;
mod page;
#[cfg(feature = "shots")]
mod shots;
mod stamp;

use iced::widget::{column, container, row, space, stack};
use iced::{Length, Padding, alignment};

use anomi_theme::prelude::*;
use anomi_vm::settings::frame::{Frame, Intent as FrameIntent};
use anomi_vm::settings::{self, Settings};
use anomi_widgets::list::List;

use crate::kit::{card, unsaved_bar};

pub use page::{icon, label};
#[cfg(feature = "shots")]
pub use shots::shots;

/// The room under the page's last line, and over it where the bar for
/// unsaved changes lies (`bottom: 56px`; the bar is 60 tall).
const BOTTOM: f32 = 56.0;
const TOP: f32 = 24.0;

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
    let close = settings::Intent::from(FrameIntent::Close);
    card(
        move |_size| {
            let sections = nav::navigation(vm, motion).map(settings::Intent::from);
            row![sections, content(vm, body(), motion)].boxed()
        },
        close,
    )
}

/// The bar over the page and the page.
fn content<'a>(
    vm: &'a Frame,
    body: Element<'a, settings::Intent>,
    motion: Motion,
) -> Element<'a, settings::Intent> {
    let bar = bar::bar(page::label(vm.page), motion).map(settings::Intent::from);
    let room = Padding {
        top: TOP,
        bottom: BOTTOM,
        left: bar::SIDES,
        right: bar::SIDES,
    };
    let column_of = container(container(body).width(Length::Fill.max(bar::COLUMN)))
        .padding(room)
        .center_x(Length::Fill);
    // A page opens at its top: each page is its own row in the scroll.
    let scroll = List::with_rows([(vm.page as u64, column_of.boxed())], motion)
        .report_pixels(true)
        .boxed();
    let mut layers: Vec<Element<'a, settings::Intent>> = vec![scroll];
    if let Some(unsaved) = &vm.unsaved {
        let notice = unsaved_bar(
            unsaved,
            bar::SIDES,
            settings::Intent::from(FrameIntent::Undo),
            settings::Intent::from(FrameIntent::Save),
            motion,
        );
        layers.push(
            container(notice)
                .align_bottom(Length::Fill)
                .align_x(alignment::Horizontal::Left)
                .boxed(),
        );
    }
    column![bar, stack(layers).height(Length::Fill)]
        .width(Length::Fill)
        .boxed()
}
