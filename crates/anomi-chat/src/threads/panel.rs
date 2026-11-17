//! A thread beside its channel, in place of its member list: Discord opens a
//! thread from a message, or a forum's post, this way. A header (the thread's
//! icon and name, "More", "Open in full view" and the cross), the thread's
//! own timeline (C01's, over the thread's room) and its own composer
//! ([`composer`]), on the chat's ground with a 1 px edge at its left. The
//! Slint side: `ThreadPanel` (`ui/thread.slint:13`); the glue is
//! `show_aside` (`src/ui/refresh.rs:3979`) and the `aside-*` callbacks
//! (`src/ui/callbacks.rs:755`, `:1131`, `:2738`).
//!
//! What the timeline's rows and the composer ask for rides in
//! `thread_panel::Intent::Within`, so the app applies it to the thread's
//! room. The panel takes its room at once: Slint fades it in with
//! `Popup.opacity`, which tiny-skia doesn't (see `docking`'s doc).
//!
//! Not drawn here, being other tasks' parts of a row: the message's hover bar
//! with its fewer reactions for the narrower panel (C02), its menu and the
//! delete question (C12). The timeline's list keeps the keyboard's id
//! ([`focus::TIMELINE`](anomi_widgets::focus::TIMELINE)) with the channel's:
//! with the thread beside it, Alt+Up steps through whichever the app says.

mod composer;
mod head;
#[cfg(feature = "shots")]
mod shots;

use iced::Fill;
use iced::widget::{column, container, row, rule, stack, text};
use iced::{Padding, alignment};
use yapfast_lang::tr;

use anomi_theme::prelude::*;
use anomi_theme::{catalog, typography};
use anomi_vm::chat::thread_panel::{Intent, ThreadPanel};
use anomi_vm::chat::{self};

use crate::timeline;

pub use head::{CLOSE_ID, EXPAND_ID, MORE_ID};
#[cfg(feature = "shots")]
pub use shots::shots;

/// `x: 16px; width: parent.width - 32px`.
const EMPTY_INSET: f32 = 16.0;

pub fn view(panel: &ThreadPanel, motion: Motion) -> impl View<chat::Intent> + '_ {
    let within = |intent: chat::Intent| chat::Intent::from(Intent::within(intent));
    // The header's line is its own last pixel row, as in Slint, so the
    // messages start 48 px down.
    let head = stack![
        head::view(panel, motion).map(chat::Intent::from),
        container(rule::horizontal(1).style(catalog::rule::line)).align_bottom(Fill),
    ];
    let body = column![
        head,
        container(messages(panel, motion).map(within))
            .width(Fill)
            .height(Fill),
        composer::view(&panel.composer, motion).map(within),
    ]
    .width(Fill)
    .height(Fill);
    container(row![
        rule::vertical(1).style(|theme: &Theme| rule::Style {
            color: theme.palette().edge,
            ..catalog::rule::line(theme)
        }),
        body
    ])
    .width(Fill)
    .height(Fill)
    .style(catalog::container::chat)
}

/// The thread's messages. A thread with none says where it starts, centred
/// (`ui/thread.slint:350`), where the channel's timeline draws a welcome;
/// while its history is on its way the timeline's skeletons stand in.
fn messages(panel: &ThreadPanel, motion: Motion) -> Element<'_, chat::Intent> {
    let timeline = &panel.timeline;
    if timeline.ready && timeline.rows.is_empty() {
        return container(
            text(tr!("This is the start of the {} thread.", panel.name))
                .size(typography::size::SMALL)
                .align_x(alignment::Horizontal::Center)
                .style(catalog::text::muted),
        )
        .padding(Padding::ZERO.left(EMPTY_INSET).right(EMPTY_INSET))
        .center(Fill)
        .boxed();
    }
    timeline::view(timeline, motion).boxed()
}
