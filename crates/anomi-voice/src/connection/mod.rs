//! A call's connection details (`CallLinkCard`, `ui/voice.slint:206`): a
//! 320 px card with the tabs Connection, Privacy and Stream (those that
//! apply), the ping graph with its averages, the voice privacy code, how a
//! shared screen goes out, and under all of it the secure notice. The shell
//! draws it for `Layer::Connection` beside the signal icon or the status
//! that opened it ([`signal`], [`touch`]).
//!
//! The view draws what [`Connection`] holds and ticks nothing: the app
//! refreshes the numbers once a second while the popout shows and clears
//! "Copied" after two seconds.

mod graph;
mod parts;
mod signal;
mod tabs;

use iced::widget::{column, container};
use iced::{Border, Color, Shadow, Vector};

use anomi_theme::prelude::*;
use anomi_theme::{Motion, Theme, metrics};
use anomi_vm::voice::connection::{Connection, Intent, Tab};

pub use parts::{COPY, copy_label};
pub use signal::{signal, touch};

/// The card's width (`call-link-menu`, `ui/yapfast.slint:2673`).
pub const WIDTH: f32 = 320.0;
/// The card's shadow (`ui/voice.slint:216`).
const SHADOW_ALPHA: f32 = 0.5;
const SHADOW_OFFSET: f32 = 8.0;
const SHADOW_BLUR: f32 = 24.0;
const PADDING: f32 = 16.0;

pub fn view(connection: &Connection, motion: Motion) -> impl View<Intent> + '_ {
    let mut body = column![].spacing(10);
    if connection.tabbed() {
        body = body.push(tabs::tabs(connection, motion).boxed());
    }
    body = match connection.shown() {
        Tab::Connection => body.push(parts::connection(connection).boxed()),
        Tab::Privacy => body.push(parts::privacy(connection, motion).boxed()),
        Tab::Stream => match &connection.stream {
            Some(stream) => body.push(parts::stream(stream).boxed()),
            None => body,
        },
    };
    body = body.push(parts::notice(connection.end_to_end).boxed());
    container(body).padding(PADDING).width(WIDTH).style(card)
}

/// The raised card, its edge and its shadow.
fn card(theme: &Theme) -> container::Style {
    let palette = theme.palette();
    container::Style {
        border: Border::default()
            .rounded(metrics::RADIUS_CARD)
            .width(1)
            .color(palette.edge),
        shadow: Shadow {
            color: Color::from_rgba8(0, 0, 0, SHADOW_ALPHA),
            offset: Vector::new(0.0, SHADOW_OFFSET),
            blur_radius: SHADOW_BLUR,
        },
        ..anomi_theme::catalog::container::raised(theme)
    }
}

/// The popout over the dock's signal icon in the window, as the golden
/// `voice/connection_popout.png`.
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    use crate::shots::{GOLDEN, Shot};
    use anomi_vm::fixtures::voice::connection;

    vec![Shot {
        area: "voice",
        name: "connection_popout",
        size: GOLDEN,
        theme: "bonfire",
        vm: connection::popout,
    }]
}
