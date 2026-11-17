//! The header's title: a DM's person (a button that opens their card), a
//! group, a channel or a thread with its way back and its topic, and the
//! titles of Friends, Requests and Browse Channels. Task N07's
//! (docs/iced/tasks/shell.md). The Slint side: `ui/yapfast.slint:3141`-`:3300`.

use iced::widget::{button, container, hover, row, rule, space, text};
use iced::{Alignment, Background, Border, Fill, Padding};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::header::{Channel, Direct, Parent, Place, Screen, Title};
use anomi_vm::icon::Icon;
use anomi_vm::menu::Spot;
use anomi_widgets::avatar::{Ground, avatar};
use anomi_widgets::button::{Kind, action_button};
use anomi_widgets::menu::opener;
use anomi_widgets::presence::status_text;
use yapfast_lang::tr;

use super::topic;
use super::{Intent, tinted};

/// The title's picture, and the icon before a channel's name.
const AVATAR: f32 = 26.0;
const GLYPH: f32 = 22.0;
/// The person's button (`dm-title`, `:3150`).
const BUTTON: f32 = 32.0;
/// Between a title's parts (`:3145`).
const GAP: f32 = 10.0;

pub(super) fn view<'a>(place: &'a Place, motion: Motion) -> Element<'a, Intent> {
    match &place.title {
        Title::Direct(direct) => direct_title(direct, motion),
        Title::Channel(channel) => channel_title(channel, motion),
        Title::Screen(screen) => screen_title(*screen, motion),
    }
}

/// The name, in the header's type, cut short where it runs out of room.
fn name<'a>(name: &'a str) -> impl View<Intent> + 'a {
    text(name)
        .size(typography::size::TITLE)
        .font(typography::BOLD)
        .style(catalog::text::primary)
        .wrapping(text::Wrapping::None)
        .ellipsis(text::Ellipsis::End)
}

/// An icon before the name, muted (`colorize: Theme.muted`).
fn kind<'a>(glyph: Icon) -> impl View<Intent> + 'a {
    tinted(glyph, GLYPH, |palette| palette.muted)
}

fn direct_title<'a>(direct: &'a Direct, motion: Motion) -> Element<'a, Intent> {
    let picture = || avatar(&direct.avatar, AVATAR, direct.presence, Ground::Chat);
    let head: Element<'a, Intent> = if direct.person {
        let face = |lit: bool| {
            container(
                row![picture(), name(&direct.name)]
                    .spacing(GAP)
                    .align_y(Alignment::Center),
            )
            .padding(Padding::from([0.0, metrics::SPACE_S]).left(4.0))
            .height(BUTTON)
            .align_y(Alignment::Center)
            .style(move |theme: &Theme| container::Style {
                background: lit.then(|| Background::Color(theme.palette().lift(0.1))),
                border: Border::default().rounded(metrics::RADIUS_ROW),
                ..container::Style::default()
            })
        };
        opener(hover(face(false), face(true)))
            .on_press(|bounds| Intent::Profile(Spot::below(bounds)))
            .boxed()
    } else {
        row![picture(), name(&direct.name)]
            .spacing(GAP)
            .align_y(Alignment::Center)
            .boxed()
    };
    let bubble = direct.activity.as_ref().map(|status| {
        container(
            status_text(status, motion)
                .size(typography::size::SMALL)
                .color(|palette| palette.secondary)
                .font(typography::MEDIUM)
                .view(),
        )
        .padding([0.0, GAP])
        .height(24.0)
        .center_y(24.0)
        .style(|theme: &Theme| container::Style {
            background: Some(Background::Color(theme.palette().raised)),
            border: Border::default().rounded(12.0),
            ..container::Style::default()
        })
    });
    row![head]
        .extend(bubble.map(|bubble| bubble.boxed()))
        .push(space().width(Fill).boxed())
        .spacing(GAP)
        .align_y(Alignment::Center)
        .width(Fill)
        .boxed()
}

fn channel_title<'a>(channel: &'a Channel, motion: Motion) -> Element<'a, Intent> {
    let mut items: Vec<Element<'a, Intent>> = Vec::new();
    if let Some(parent) = &channel.parent {
        items.push(back(parent).boxed());
    }
    items.push(kind(channel.icon).boxed());
    items.push(name(&channel.name).boxed());
    items.push(match &channel.topic {
        Some(text) => topic::line(text, motion),
        None => space().width(Fill).boxed(),
    });
    row(items)
        .spacing(GAP)
        .align_y(Alignment::Center)
        .width(Fill)
        .boxed()
}

/// A thread follows its channel, which leads back to it (`:3211`-`:3226`).
fn back<'a>(parent: &'a Parent) -> impl View<Intent> + 'a {
    let glyph = if parent.forum {
        Icon::MessageSquareText
    } else {
        Icon::Hash
    };
    let channel = button(
        text(parent.name.as_str())
            .size(typography::size::TITLE)
            .font(typography::BOLD)
            .style(catalog::text::inherit),
    )
    .padding(0)
    .on_press(Intent::Parent)
    .style(|theme: &Theme, status| {
        let palette = theme.palette();
        button::Style {
            text_color: match status {
                button::Status::Hovered | button::Status::Pressed => palette.text,
                _ => palette.secondary,
            },
            ..catalog::button::bare(theme, status)
        }
    });
    row![
        kind(glyph),
        channel,
        tinted(Icon::ChevronRight, metrics::SPACE_L, |palette| palette
            .faint),
    ]
    .spacing(GAP)
    .align_y(Alignment::Center)
}

fn screen_title<'a>(screen: Screen, motion: Motion) -> Element<'a, Intent> {
    let (glyph, words) = match screen {
        Screen::Friends { .. } => (Icon::Users, tr!("Friends")),
        Screen::Requests => (Icon::Inbox, tr!("Requests")),
        Screen::Browse => (Icon::Hash, tr!("Browse Channels")),
    };
    let mut items: Vec<Element<'a, Intent>> = vec![kind(glyph).boxed(), name(words).boxed()];
    if let Screen::Friends { all } = screen {
        items.push(rule::vertical(1).style(catalog::rule::line).boxed());
        items.push(tab(tr!("Online"), !all, false).boxed());
        items.push(tab(tr!("All"), all, true).boxed());
        items.push(
            action_button(tr!("Add Friend"), Kind::Primary, motion)
                .small()
                .on_press(Intent::AddFriend)
                .boxed(),
        );
    }
    let line = row(items).spacing(GAP).align_y(Alignment::Center);
    row![line.height(24.0), space().width(Fill)]
        .align_y(Alignment::Center)
        .width(Fill)
        .boxed()
}

/// A tab of the Friends header (`HeaderTab`, `ui/friends.slint:313`).
fn tab<'a>(words: &'a str, selected: bool, all: bool) -> impl View<Intent> + 'a {
    button(
        container(
            text(words)
                .size(typography::size::BODY)
                .font(typography::MEDIUM)
                .style(catalog::text::inherit),
        )
        .center_y(Fill),
    )
    .padding([0.0, GAP])
    .height(28.0)
    .on_press(Intent::FriendsAll(all))
    .style(move |theme: &Theme, status| {
        let palette = theme.palette();
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        button::Style {
            background: if selected {
                Some(Background::Color(palette.active))
            } else if hovered {
                Some(Background::Color(palette.hover))
            } else {
                None
            },
            text_color: if selected || hovered {
                palette.text
            } else {
                palette.secondary
            },
            border: Border::default().rounded(metrics::RADIUS_ROW),
            ..button::Style::default()
        }
    })
}
