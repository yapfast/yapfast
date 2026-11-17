//! An incoming call, and the question before switching calls. Slint's
//! `IncomingCall` and `SwitchDialog` (`ui/voice.slint:1213`, `:1269`).
//!
//! [`view`] is the card of a call ringing this account in a DM that isn't
//! open: the caller's avatar inside a ring that pulses while the window is
//! in front, their name, what rings, and "Decline" and "Accept". The shell
//! hangs it at the top of the window as a floating layer, arriving by the
//! dialog's pace, and takes nothing from what lies under it. [`switch`] is
//! the dialog of `Layer::SwitchCall`.

mod card;
mod keys;
mod ring;
mod switch;

use iced::widget::Id;
use iced::widget::text::LineHeight;
use iced::widget::{column, container, row, stack, text};
use iced::{Alignment, Fill, Length, Padding};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, typography};
use anomi_vm::icon::Icon;
use anomi_vm::voice::incoming::{Incoming, Intent};
use anomi_widgets::avatar::portrait;
use anomi_widgets::button::call_button;
use yapfast_lang::tr;

use card::card;

pub use switch::{CANCEL, CONFIRM, view as switch};

/// The ids of the two buttons, for focus and for tests.
pub const DECLINE: Id = Id::new("incoming-decline");
pub const ACCEPT: Id = Id::new("incoming-accept");

/// The card's width, padding and the space between its parts (`:1220`,
/// `:1233`).
const WIDTH: f32 = 300.0;
const PADDING: f32 = 20.0;
const SPACING: f32 = 10.0;
/// The avatar, inset in its ring (`:1241`, `:1250`).
const AVATAR: f32 = 72.0;
const INSET: f32 = 10.0;
/// The name and the detail (`:1257`, `:1258`).
const NAME: f32 = 18.0;
/// Slint's whole-pixel lines for the name and the detail
/// (`ui/voice.slint:1249`, `:1250`); iced's default 1.375 would leave
/// everything below the ring a pixel low.
pub(crate) const NAME_LINE: f32 = 24.0;
pub(crate) const DETAIL_LINE: f32 = 20.0;
/// Between the two buttons, and above them (`:1259`).
const BUTTONS: f32 = 20.0;
const BUTTONS_TOP: f32 = 6.0;

pub fn view(incoming: &Incoming, motion: Motion) -> impl View<Intent> + '_ {
    let face = stack![
        ring::ring(motion),
        container(portrait(&incoming.avatar, AVATAR).motion(motion).view()).padding(INSET),
    ]
    .width(ring::SIZE)
    .height(ring::SIZE);
    let buttons = row![
        call_button(
            Icon::PhoneOff,
            tr!("Decline"),
            |palette| palette.danger,
            motion
        )
        .id(DECLINE)
        .on_press(Intent::Decline(incoming.room.clone())),
        call_button(Icon::Phone, tr!("Accept"), |palette| palette.live, motion)
            .id(ACCEPT)
            .on_press(Intent::Accept(incoming.room.clone())),
    ]
    .spacing(BUTTONS);
    let body = column![
        container(face).center_x(Fill),
        text(&incoming.name)
            .size(NAME)
            .line_height(LineHeight::Absolute(NAME_LINE.into()))
            .font(typography::BOLD)
            .style(catalog::text::primary)
            .width(Fill)
            .align_x(Alignment::Center)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End),
        text(&incoming.detail)
            .size(typography::size::BODY)
            .line_height(LineHeight::Absolute(DETAIL_LINE.into()))
            .style(catalog::text::secondary)
            .width(Fill)
            .align_x(Alignment::Center),
        container(buttons)
            .center_x(Fill)
            .padding(Padding::ZERO.top(BUTTONS_TOP)),
    ]
    .spacing(SPACING);
    container(body)
        .padding(PADDING)
        .width(Length::Fixed(WIDTH))
        .style(card)
}

/// Its shots for `anomi-shots`, each the whole window from a fixture (see
/// `crate::shots`). The ringing card has a golden shot; the switch dialog
/// has none: compare it with the Slint app under `scripts/ui-session`.
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    use crate::shots::{GOLDEN, Shot};
    use anomi_vm::fixtures::voice::incoming as sample;
    vec![
        Shot {
            area: "dmcall",
            name: "incoming",
            size: GOLDEN,
            theme: "bonfire",
            vm: sample::ringing_window,
        },
        Shot {
            area: "dmcall",
            name: "switch",
            size: GOLDEN,
            theme: "bonfire",
            vm: sample::switching_window,
        },
    ]
}
