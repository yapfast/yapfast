//! A row of the status menu, drawn by hand: `StatusOption` with its dot, its
//! name, its note and a check on the current one (`ui/dialogs.slint:567`),
//! which is a press that sends one intent; the row lights under the
//! pointer, as iced's `hover` lays the lit row over the one at rest.

use iced::widget::{button, column, container, hover, row, text};
use iced::{Alignment, Background, Border, Fill};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, typography};
use anomi_vm::account::Choice;
use anomi_vm::icon::Icon;
use anomi_widgets::avatar::Ground;
use anomi_widgets::icon::icon;
use anomi_widgets::presence::status_dot;

/// `height: max(36px, ..)`, its corner and padding (`:575`, `:589`).
const HEIGHT: f32 = 36.0;
const RADIUS: f32 = 4.0;
const PAD_X: f32 = 8.0;
const PAD_Y: f32 = 6.0;
/// `spacing: 10px`.
const GAP: f32 = 10.0;
/// The dot's width (`StatusDot`'s own 16 px) and the check's.
const DOT: f32 = 16.0;
const CHECK: f32 = 16.0;
/// The note under the name (`font-size: 12px`, `spacing: 2px`).
const NOTE: f32 = typography::size::CAPTION;

/// The gap between the name and its note (`spacing: 2px`, `:592`).
const WORDS_GAP: f32 = 2.0;

/// One row, `StatusOption`: the dot of `choice`, `name` with its `note`
/// (empty for none), a check when `checked`; a press sends `on_press`. The
/// classic menu and the account card's unfolded statuses both draw it.
pub fn option<'a, M: Clone + 'a>(
    choice: Choice,
    name: &'a str,
    note: &'a str,
    checked: bool,
    on_press: M,
) -> Element<'a, M> {
    let rest = body(choice, name, note, checked, false, on_press.clone());
    let lit = body(choice, name, note, checked, true, on_press);
    hover(rest, lit).boxed()
}

fn body<'a, M: Clone + 'a>(
    choice: Choice,
    name: &'a str,
    note: &'a str,
    checked: bool,
    lit: bool,
    on_press: M,
) -> Element<'a, M> {
    // The dot's ring is the row's fill. The menu's raised ground has no
    // name in `Ground`, so the panel's stands in at rest.
    let lead: Element<'a, M> = status_dot(
        choice.presence(),
        DOT,
        if lit { Ground::Active } else { Ground::Panel },
    )
    .boxed();
    let mut words = column![
        text(name)
            .size(typography::size::BODY)
            .font(if checked {
                typography::BOLD
            } else {
                typography::UI
            })
            .wrapping(text::Wrapping::None)
            .boxed()
    ]
    .spacing(WORDS_GAP)
    .width(Fill);
    if !note.is_empty() {
        words = words.push(
            text(note)
                .size(NOTE)
                .style(catalog::text::muted)
                .width(Fill)
                .boxed(),
        );
    }
    let mut line = row![lead, words.boxed()]
        .spacing(GAP)
        .align_y(Alignment::Center);
    if checked {
        line = line.push(
            icon(Icon::Check, CHECK)
                .style(|theme: &Theme, _| iced::widget::svg::Style {
                    color: Some(theme.palette().accent),
                })
                .boxed(),
        );
    }
    // At least 36 px tall: a row with no note is exactly that, one with a
    // note is as tall as its two lines.
    let height = if note.is_empty() {
        iced::Length::Fixed(HEIGHT)
    } else {
        iced::Length::Shrink
    };
    button(
        container(line)
            .padding([PAD_Y, PAD_X])
            .center_y(Fill)
            .boxed(),
    )
    .width(Fill)
    .height(height)
    .padding(0)
    .on_press(on_press)
    .style(move |theme: &Theme, _status| {
        let palette = theme.palette();
        button::Style {
            background: lit.then_some(Background::Color(palette.active)),
            text_color: if checked || lit {
                palette.text
            } else {
                palette.secondary
            },
            border: Border::default().rounded(RADIUS),
            ..button::Style::default()
        }
    })
    .boxed()
}
