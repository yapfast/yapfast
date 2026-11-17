//! The signal icon the dock shows (`SignalIcon`, `ui/voice.slint:136`) and
//! what opens the popout from it (`LinkTouch`, `:148`).

use iced::widget::{mouse_area, svg};

use anomi_theme::prelude::*;
use anomi_theme::{Motion, Theme};
use anomi_vm::menu::Spot;
use anomi_vm::voice::connection::Intent;
use anomi_widgets::icon::{Icon, icon};
use anomi_widgets::menu::opener;
use anomi_widgets::tip::{Side, tip};

/// The gap under the icon (`call-link-menu`, `ui/yapfast.slint:2675`).
const LINK_GAP: f32 = 4.0;

/// The bars and the colour of a connection by how good it is: 1 good
/// (green), 2 middling (accent), 3 poor (red), anything else unknown (grey).
pub fn signal<'a, M: 'a>(quality: u8, size: f32) -> impl View<M> + 'a {
    let glyph = match quality {
        3 => Icon::SignalLow,
        2 => Icon::SignalMedium,
        _ => Icon::SignalHigh,
    };
    icon(glyph, size).style(move |theme: &Theme, _status| {
        let palette = theme.palette();
        svg::Style {
            color: Some(match quality {
                1 => palette.live,
                2 => palette.accent,
                3 => palette.danger,
                _ => palette.muted,
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
