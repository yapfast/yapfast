//! Official Discord pages that need a browser. These are external actions,
//! so opening one leaves the conversation and its draft where they were.

use iced::{
    Element, Length, Point,
    widget::{button, column, text},
};
use yapfast_core::application::{AppState, Navigation};

use crate::{app::Message, controls, ctx::Ctx, fonts::MEDIUM};

mod focus;

const BUTTON_HEIGHT: f32 = 40.0;
const GAP: f32 = 6.0;
const PADDING: f32 = 6.0;
pub(super) const HEIGHT: f32 = 3.0 * BUTTON_HEIGHT + 2.0 * GAP + 2.0 * PADDING;

pub(crate) struct Destination {
    pub id: &'static str,
    pub label: &'static str,
    pub url: &'static str,
}

// Sources and the browser-only scope are documented in client/navigation.md.
pub(crate) const DESTINATIONS: [Destination; 3] = [
    Destination {
        id: "dm-nitro-browser",
        label: "Nitro in browser",
        url: "https://discord.com/nitro",
    },
    Destination {
        id: "dm-shop-browser",
        label: "Shop in browser",
        url: "https://discord.com/shop",
    },
    Destination {
        id: "dm-quests-browser",
        label: "Quests in browser",
        url: "https://discord.com/discovery/quests",
    },
];

pub(crate) fn visible(state: &AppState) -> bool {
    state.navigation == Navigation::DirectMessages && state.discord_linked && !state.preview
}

impl Destination {
    fn message(&self) -> Message {
        Message::Link(self.url.into(), Point::ORIGIN)
    }
}

pub(super) fn view(cx: Ctx<'_>) -> Element<'_, Message> {
    let theme = cx.look.theme;
    let mut links = column![].spacing(GAP).padding([PADDING, 0.0]);
    for destination in &DESTINATIONS {
        let face = button(
            text(destination.label)
                .size(13)
                .font(MEDIUM)
                .color(theme.text),
        )
        .width(Length::Fill)
        .height(BUTTON_HEIGHT)
        .padding([0, 8])
        .on_press(destination.message())
        .style(controls::outlined_lit_style(
            theme.raised,
            theme.hover,
            theme.line,
            theme.accent,
            8.0,
        ));
        links = links.push(focus::Focusable::new(
            face.into(),
            destination,
            theme.accent,
        ));
    }
    links.into()
}
