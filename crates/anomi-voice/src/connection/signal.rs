//! The signal icon the dock shows (`SignalIcon`, `ui/voice.slint:136`) and
//! what opens the popout from it (`LinkTouch`, `:148`).

use iced::widget::{mouse_area, svg};

use anomi_theme::prelude::*;
use anomi_theme::{Motion, Theme};
use anomi_vm::menu::Spot;
use anomi_vm::voice::connection::Intent;
use anomi_vm::voice::dock::Signal;
use anomi_widgets::icon::{Icon, icon};
use anomi_widgets::menu::opener;
use anomi_widgets::tip::{Side, tip};

/// The gap under the icon (`call-link-menu`, `ui/yapfast.slint:2675`).
const LINK_GAP: f32 = 4.0;

/// The bars and the colour of a connection by how good it is: fine gives
/// full bars in green, average two in the accent, bad one in red, unknown
/// full bars in grey.
pub fn signal<'a, M: 'a>(quality: Signal, size: f32) -> impl View<M> + 'a {
    let glyph = match quality {
        Signal::Bad => Icon::SignalLow,
        Signal::Average => Icon::SignalMedium,
        Signal::Fine | Signal::Unknown => Icon::SignalHigh,
    };
    icon(glyph, size).style(move |theme: &Theme, _status| {
        let palette = theme.palette();
        svg::Style {
            color: Some(match quality {
                Signal::Fine => palette.live,
                Signal::Average => palette.accent,
                Signal::Bad => palette.danger,
                Signal::Unknown => palette.muted,
            }),
        }
    })
}

/// `content` (the signal icon, or the call's status) with the ping as its
/// tooltip, a click that opens the popout there, and a fresh read of the
/// link when the pointer enters (`ui/voice.slint:153`). Nothing happens while
/// the call isn't `measured` yet. The dock maps the intent into its own.
pub fn touch<'a>(
    content: impl View<Intent> + 'a,
    ping: &'a str,
    measured: bool,
    side: Side,
    motion: Motion,
) -> Element<'a, Intent> {
    if !measured {
        return content.boxed();
    }
    let opening = opener(content).on_press(|bounds| {
        Intent::Open(Spot {
            gap: LINK_GAP,
            ..Spot::below(bounds)
        })
    });
    let reading = mouse_area(opening).on_enter(Intent::Read);
    tip(reading, ping, side, motion).boxed()
}
