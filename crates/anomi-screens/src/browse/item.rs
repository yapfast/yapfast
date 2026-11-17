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

/// The row's height, its corner and its inset.
const HEIGHT: f32 = 56.0;
const RADIUS: f32 = 8.0;
const INSET: f32 = 12.0;
/// The box and its tick, and the box's ring and corner.
const BOX: f32 = 20.0;
const TICK: f32 = 14.0;
const BOX_RING: f32 = 1.5;
const BOX_RADIUS: f32 = 4.0;
/// The gap between a channel's name and its topic (`ui/browse.slint:65`).
const WORDS: f32 = 2.0;
/// A locked box fades to this.
const LOCKED: f32 = 0.5;

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
        container(tick_box(channel.followed, channel.by_category)).center_y(HEIGHT),
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
        .style(|theme: &Theme, status| button::Style {
            background: matches!(status, button::Status::Hovered | button::Status::Pressed)
                .then(|| theme.palette().raised.into()),
            text_color: theme.palette().text,
            border: Border {
                radius: RADIUS.into(),
                ..Border::default()
            },
            ..button::Style::default()
        });
    if channel.by_category {
        tip(
            item,
            tr!("You must unfollow this category to add or remove specific channels."),
            Side::Above,
            motion,
        )
        .boxed()
    } else {
        item.boxed()
    }
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

/// The box: a ring, filled with the accent and ticked while followed.
fn tick_box<'a>(followed: bool, locked: bool) -> Element<'a, Intent> {
    let fade = if locked { LOCKED } else { 1.0 };
    let tick = followed.then(|| {
        icon(Icon::Check, TICK).style(move |theme: &Theme, status| {
            catalog::svg::colored(theme.palette().on_accent.scale_alpha(fade))(theme, status)
        })
    });
    container(tick)
        .center(BOX)
        .style(move |theme: &Theme| {
            let palette = theme.palette();
            let ring = if followed {
                palette.accent
            } else {
                palette.muted
            };
            container::Style {
                background: followed.then(|| palette.accent.scale_alpha(fade).into()),
                border: Border {
                    color: ring.scale_alpha(fade),
                    width: BOX_RING,
                    radius: BOX_RADIUS.into(),
                },
                ..container::Style::default()
            }
        })
        .boxed()
}
