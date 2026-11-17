//! A channel in Browse Channels (`BrowseItem`, `ui/browse.slint:26`):
//! clicking the row ticks its box or clears it, and its button opens it, as
//! in Discord's client. A channel followed through its category is locked:
//! its row doesn't answer, and a tip says why.

use iced::widget::{Column, button, container, row, svg, text};
use iced::{Alignment, Border, Fill};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::browse::{Intent, Kind, Row};
use anomi_widgets::button::{Kind as Look, action_button};
use anomi_widgets::icon::{Icon, icon};
use anomi_widgets::tip::{Side, tip};
use yapfast_lang::{text_in, tr};

use super::ease::{raised, tick_box};

/// The row's height, its corner and its inset.
const HEIGHT: f32 = 56.0;
const RADIUS: f32 = 8.0;
const INSET: f32 = 12.0;
/// The box and its tick.
const BOX: f32 = 20.0;
const TICK: f32 = 14.0;
/// The gap between a channel's name and its topic (`ui/browse.slint:65`).
const WORDS: f32 = 2.0;

pub fn view<'a>(channel: &'a Row, motion: Motion) -> Element<'a, Intent> {
    let name = text(channel.name.as_str())
        .size(typography::size::BODY)
        .font(typography::SEMIBOLD)
        .wrapping(text::Wrapping::None)
        .ellipsis(text::Ellipsis::End)
        .style(if channel.by_category {
            catalog::text::muted
        } else {
            catalog::text::primary
        });
    let mut words: Vec<Element<'a, Intent>> = vec![name.boxed()];
    if !channel.topic.is_empty() {
        words.push(
            text(channel.topic.as_str())
                .size(typography::size::CAPTION)
                .wrapping(text::Wrapping::None)
                .ellipsis(text::Ellipsis::End)
                .style(catalog::text::muted)
                .boxed(),
        );
    }
    let words = Column::with_children(words).spacing(WORDS).width(Fill);
    let label = match channel.kind {
        Kind::Voice => text_in("BrowseItem", "Join"),
        Kind::Forum => text_in("BrowseItem", "Open"),
        _ => text_in("BrowseItem", "View"),
    };
    let open = action_button(label, Look::Secondary, motion)
        .small()
        .on_press(Intent::View(channel.id.clone()));
    let content = row![
        icon(glyph(channel.kind), metrics::ICON_ROW).style(|theme: &Theme, _| svg::Style {
            color: Some(theme.palette().muted),
        }),
        words,
        container(open).center_y(HEIGHT),
        container(tick_box(
            channel.followed,
            channel.by_category,
            BOX,
            TICK,
            motion
        ))
        .center_y(HEIGHT),
    ]
    .spacing(INSET)
    .padding([0.0, INSET])
    .align_y(Alignment::Center)
    .height(HEIGHT);

    let toggle = (!channel.by_category).then(|| Intent::Follow {
        room: channel.id.clone(),
        follow: !channel.followed,
    });
    let item = button(content)
        .padding(0)
        .width(Fill)
        .on_press_maybe(toggle)
        .style(|theme: &Theme, _| button::Style {
            text_color: theme.palette().text,
            border: Border {
                radius: RADIUS.into(),
                ..Border::default()
            },
            ..button::Style::default()
        });
    let item = raised(item, channel.by_category, RADIUS, motion);
    let why = if channel.by_category {
        tr!("You must unfollow this category to add or remove specific channels.")
    } else {
        ""
    };
    tip(item, why, Side::Above, motion).boxed()
}

/// The icon of a kind of channel, as the sidebar draws it.
fn glyph(kind: Kind) -> Icon {
    match kind {
        Kind::Voice => Icon::Volume,
        Kind::Forum => Icon::MessageSquareText,
        Kind::Announcement => Icon::Megaphone,
        Kind::Category | Kind::Text => Icon::Hash,
    }
}
