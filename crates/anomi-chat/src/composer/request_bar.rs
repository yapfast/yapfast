//! The bars around the composer's field: a message request's, in place of
//! the field (`RequestBar`, `ui/composer.slint:196`); the reply bar on the
//! field's top (`ui/yapfast.slint:4150`) with its "@ On" toggle and close;
//! the bar saying nothing may be sent here (`:4160`); the hint under the
//! field (`:4310`); and who is typing above it. The frame around the field
//! and the typing dots are this module's `frame` and `typing`.

pub(super) mod buttons;
mod frame;
mod typing;

use std::borrow::Cow;

use iced::widget::{column, container, row, svg, text};
use iced::{Border, Length};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::chat::composer::{Intent, Reply};
use anomi_vm::icon::Icon;
use anomi_widgets::button::{Kind, action_button, icon_button, ping_toggle};
use anomi_widgets::icon::icon;
use yapfast_lang::tr;

pub use frame::{Frame, frame};
pub(super) use typing::typing;

/// `radius: 10px` of the bars.
const RADIUS: f32 = 10.0;
/// The reply bar's height, its padding (`14px` before, `4px` after) and gap.
const REPLY_HEIGHT: f32 = 36.0;
const REPLY_GAP: f32 = 8.0;
/// The reply icon's width, and the close button's side and glyph.
const REPLY_ICON: f32 = 16.0;
const CLOSE: f32 = 28.0;
const CLOSE_GLYPH: f32 = 16.0;
/// The hint's height (`height: 22px`), its gap to the lock and the lock's
/// width.
pub const HINT_HEIGHT: f32 = 22.0;
const HINT_GAP: f32 = 6.0;
const LOCK: f32 = 12.0;

/// An icon in `Theme.muted` whatever the pointer does.
pub(super) fn muted(theme: &Theme, _status: svg::Status) -> svg::Style {
    svg::Style {
        color: Some(theme.palette().muted),
    }
}

/// A message request in place of the field: who it is from and what the
/// two answers do.
pub(super) fn request<'a>(title: &'a str, motion: Motion) -> Element<'a, Intent> {
    let words = column![
        text(title)
            .size(typography::size::BODY)
            .font(typography::BOLD)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End)
            .style(catalog::text::primary),
        // One line each: a wrapped line would spill over the hint below.
        text(tr!(
            "Accept to reply. Ignoring removes it without telling them."
        ))
        .size(typography::size::SMALL)
        .wrapping(text::Wrapping::None)
        .ellipsis(text::Ellipsis::End)
        .style(catalog::text::secondary),
    ]
    .spacing(2)
    .width(Length::Fill);
    let ignore = tr!("Ignore");
    let accept = tr!("Accept");
    container(
        row![
            words,
            action_button(ignore, Kind::Secondary, motion).on_press(Intent::Ignore),
            action_button(accept, Kind::Primary, motion).on_press(Intent::Accept),
        ]
        .spacing(10)
        .align_y(iced::alignment::Vertical::Center),
    )
    .padding(iced::Padding {
        top: 12.0,
        right: 12.0,
        bottom: 12.0,
        left: 16.0,
    })
    .width(Length::Fill)
    .style(|theme: &Theme| container::Style {
        background: Some(theme.palette().field.into()),
        border: Border::default().rounded(RADIUS),
        ..container::Style::default()
    })
    .boxed()
}

/// "Replying to ...", its toggle for whether the reply mentions its
/// author, and its close.
pub(super) fn reply<'a>(reply: &'a Reply, motion: Motion) -> Element<'a, Intent> {
    let middle = |content: Element<'a, Intent>| container(content).center_y(Length::Fill);
    container(
        row![
            container(icon(Icon::Reply, REPLY_ICON).style(muted)).center_y(Length::Fill),
            container(
                text(reply.label.as_str())
                    .size(typography::size::SMALL)
                    .wrapping(text::Wrapping::None)
                    .ellipsis(text::Ellipsis::End)
                    .style(catalog::text::secondary),
            )
            .center_y(Length::Fill)
            .width(Length::Fill),
            middle(
                ping_toggle(reply.ping, motion)
                    .on_press(Intent::ToggleReplyPing)
                    .boxed()
            ),
            middle(
                icon_button(Icon::X, tr!("Cancel reply"), motion)
                    .size(CLOSE)
                    .glyph(CLOSE_GLYPH)
                    .on_press(Intent::CancelReply)
                    .view()
                    .boxed()
            ),
        ]
        .spacing(REPLY_GAP)
        .height(REPLY_HEIGHT),
    )
    .padding(iced::Padding {
        top: 0.0,
        right: 4.0,
        bottom: 0.0,
        left: 14.0,
    })
    .width(Length::Fill)
    .height(REPLY_HEIGHT)
    .style(|theme: &Theme| container::Style {
        background: Some(theme.palette().sidebar.into()),
        border: Border {
            radius: iced::border::top(RADIUS),
            ..Border::default()
        },
        ..container::Style::default()
    })
    .boxed()
}

/// The one disabled bar that stands in for the field where nothing may be
/// sent: no buttons, no caption.
pub(super) fn locked<'a>(says: &'a str) -> Element<'a, Intent> {
    container(
        text(says)
            .size(typography::size::TITLE)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End)
            .style(catalog::text::muted),
    )
    .center_y(Length::Fill)
    .padding([0.0, 16.0])
    .width(Length::Fill)
    .height(metrics::COMPOSER_HEIGHT)
    .style(|theme: &Theme| {
        let palette = theme.palette();
        container::Style {
            background: Some(palette.field.into()),
            border: Border::default()
                .rounded(RADIUS)
                .width(1)
                .color(palette.outline_field),
            ..container::Style::default()
        }
    })
    .boxed()
}

/// The line under the field: a lock where the room is encrypted, then
/// what it says.
pub(super) fn hint<'a, M: 'a>(lock: bool, says: Cow<'a, str>) -> Element<'a, M> {
    let mut line = row![].spacing(HINT_GAP).height(HINT_HEIGHT);
    if lock {
        line = line.push(
            container(icon(Icon::Lock, LOCK).style(muted))
                .center_y(Length::Fill)
                .boxed(),
        );
    }
    let line = line.push(
        container(
            text(says)
                .size(typography::size::LABEL)
                .wrapping(text::Wrapping::None)
                .ellipsis(text::Ellipsis::End)
                .style(catalog::text::muted),
        )
        .center_y(Length::Fill)
        .width(Length::Fill)
        .boxed(),
    );
    container(line)
        .padding(iced::Padding {
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
            left: 4.0,
        })
        .boxed()
}
