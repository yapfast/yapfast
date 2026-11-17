//! What heads the list: the Friends and Requests rows, the "Direct
//! messages" heading with its two buttons, and the line a list with no rows
//! says. The Slint side: `DmScreenLinks` (`ui/rail.slint:1114`) and
//! `DmHeading` (`ui/yapfast/dm-heading.slint:4`).

use iced::widget::{button, container, row, space, text};
use iced::{Alignment, Fill, Padding};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::sidebar::dms::{Empty, Intent};
use anomi_widgets::badge::badge;
use anomi_widgets::button::{Side, icon_button};
use anomi_widgets::icon::{Icon, icon};
use yapfast_lang::tr;

use super::gutter::gutter;
use super::look::{Look, style};

/// Friends and Requests are as tall as a channel's row (`ui/rail.slint:507`).
pub const ROW: f32 = metrics::CHANNEL_ROW_HEIGHT;
/// The heading's height and its buttons (`dm-heading.slint:8`, `:15`).
pub const HEADING: f32 = 40.0;
const TOOL: f32 = 24.0;
const TOOL_GLYPH: f32 = 16.0;
/// What the heading's text is set in from the row's edge, and its tools' gap.
const TEXT_INSET: f32 = 8.0;
const TOOLS_GAP: f32 = 4.0;

/// Friends or Requests: an icon, a label and, for Requests, a count.
pub fn link<'a>(
    glyph: Icon,
    label: &'a str,
    count: &'a str,
    selected: bool,
    intent: Intent,
) -> Element<'a, Intent> {
    let tint = move |theme: &Theme, _status| {
        let palette = theme.palette();
        iced::widget::svg::Style {
            color: Some(if selected {
                palette.text
            } else {
                palette.faint
            }),
        }
    };
    let mut content = row![
        icon(glyph, metrics::ICON_ROW).style(tint),
        text(label)
            .size(typography::size::BODY)
            .font(typography::MEDIUM)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End),
        space().width(Fill),
    ]
    .spacing(metrics::SPACE_S)
    .align_y(Alignment::Center);
    if !count.is_empty() {
        content = content.push(badge(count).boxed());
    }
    let look = move |hovered| Look {
        selected,
        hovered,
        unread: false,
    };
    let face = button(container(content).center_y(ROW))
        .padding([0.0, metrics::SPACE_S])
        .width(Fill)
        .on_press(intent)
        .style(move |theme: &Theme, status| {
            style(
                theme,
                look(matches!(
                    status,
                    button::Status::Hovered | button::Status::Pressed
                )),
            )
        });
    sided(face.boxed())
}

/// "Direct messages" with the buttons that add a friend and start a DM.
pub fn heading<'a>(linked: bool, motion: Motion) -> Element<'a, Intent> {
    let mut tools = row![].spacing(TOOLS_GAP).align_y(Alignment::Center);
    if linked {
        tools = tools.push(
            icon_button(Icon::UserPlus, tr!("Add a friend"), motion)
                .size(TOOL)
                .glyph(TOOL_GLYPH)
                .tip_side(Side::Below)
                .on_press(Intent::AddFriend)
                .view(),
        );
    }
    tools = tools.push(
        icon_button(Icon::SquarePen, tr!("New message"), motion)
            .size(TOOL)
            .glyph(TOOL_GLYPH)
            .tip_side(Side::Below)
            .on_press(Intent::NewMessage)
            .view(),
    );
    let line = row![
        text(tr!("Direct Messages"))
            .size(typography::size::CAPTION)
            .font(typography::SEMIBOLD)
            .style(catalog::text::muted)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End)
            .width(Fill),
        tools,
    ]
    .spacing(TOOLS_GAP)
    .align_y(Alignment::Center);
    sided(
        container(line)
            .padding(Padding::ZERO.left(TEXT_INSET))
            .height(HEADING)
            .center_y(HEADING)
            .boxed(),
    )
}

/// What a list with no rows says, under what heads it.
pub fn empty<'a>(why: Empty) -> Option<Element<'a, Intent>> {
    let words = match why {
        Empty::None => return None,
        Empty::NoMatch => tr!("No matching conversations."),
        Empty::NoDms => tr!("No direct messages yet. Existing DMs appear here after syncing."),
    };
    Some(sided(
        container(
            text(words)
                .size(typography::size::LABEL)
                .style(catalog::text::muted),
        )
        .padding(metrics::SPACE_S)
        .boxed(),
    ))
}

/// `content` between the sidebar's edge strip and the same room on its
/// other side, as every row of the list sits.
pub fn sided<'a>(content: Element<'a, Intent>) -> Element<'a, Intent> {
    row![
        gutter(false),
        container(content).width(Fill),
        space().width(metrics::ROW_INSET),
    ]
    .boxed()
}
