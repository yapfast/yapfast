//! The question before switching voice channels, or joining one to watch a
//! stream: Slint's `SwitchDialog` (`ui/voice.slint:1269`). A round speaker
//! on the accent, the title and the app's words, then "Cancel" and the
//! main button at the end. Enter switches, Escape cancels, a press outside
//! cancels (the shell's layer).

use iced::widget::text::LineHeight;
use iced::widget::{Id, column, container, row, space, svg, text};
use iced::{Alignment, Border, Fill, Length};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::icon::Icon;
use anomi_vm::voice::incoming::{Incoming, Intent};
use anomi_widgets::button::{Kind, action_button};
use anomi_widgets::icon::icon;
use yapfast_lang::tr;

use super::card::card;
use super::keys::keys;
use super::{DETAIL_LINE, NAME_LINE};

/// The widest the card gets (`min(420px, root.width - 48px)`, `:1286`; the
/// layer keeps it inside the window's margins), its padding and spacings.
const WIDTH: f32 = 420.0;
const PADDING: f32 = 20.0;
const SPACING: f32 = 18.0;
const GAP: f32 = 14.0;
const TEXTS: f32 = 6.0;
/// The speaker's disc and its picture (`:1307`).
const DISC: f32 = 40.0;
const GLYPH: f32 = 20.0;
/// The ids of the two buttons, for focus and for tests.
pub const CANCEL: Id = Id::new("switch-cancel");
pub const CONFIRM: Id = Id::new("switch-confirm");
/// The title (`font-size: 18px; font-weight: 700`).
const TITLE: f32 = 18.0;

pub fn view(incoming: &Incoming, motion: Motion) -> impl View<Intent> + '_ {
    let switch = &incoming.switch;
    let title = if switch.joins {
        tr!("Watch stream?")
    } else {
        tr!("Switch voice channel?")
    };
    let confirm = if switch.joins {
        tr!("Join")
    } else {
        tr!("Switch channel")
    };
    let speaker = container(
        icon(Icon::Volume, GLYPH).style(|theme: &Theme, _status| svg::Style {
            color: Some(theme.palette().accent),
        }),
    )
    .center(Length::Fixed(DISC))
    .style(disc);
    let words = column![
        text(title)
            .size(TITLE)
            .line_height(LineHeight::Absolute(NAME_LINE.into()))
            .font(typography::BOLD)
            .style(catalog::text::primary)
            .width(Fill),
        text(&switch.text)
            .size(typography::size::BODY)
            .line_height(LineHeight::Absolute(DETAIL_LINE.into()))
            .style(catalog::text::secondary)
            .width(Fill),
    ]
    .spacing(TEXTS)
    .width(Fill);
    let buttons = row![
        space().width(Fill),
        action_button(tr!("Cancel"), Kind::Ghost, motion)
            .id(CANCEL)
            .on_press(Intent::CancelSwitch),
        action_button(confirm, Kind::Primary, motion)
            .id(CONFIRM)
            .on_press(Intent::ConfirmSwitch),
    ]
    .spacing(metrics::SPACE_S);
    let body = column![
        row![speaker, words].spacing(GAP).align_y(Alignment::Start),
        buttons
    ]
    .spacing(SPACING);
    keys(
        container(body)
            .padding(PADDING)
            .width(Length::Fixed(WIDTH))
            .style(card),
        Intent::ConfirmSwitch,
        Intent::CancelSwitch,
    )
}

/// The speaker's disc: the soft accent, round.
fn disc(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(theme.palette().accent_soft.into()),
        border: Border::default().rounded(DISC / 2.0),
        ..container::Style::default()
    }
}
