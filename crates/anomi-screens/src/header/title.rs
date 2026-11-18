//! The header's title: a DM's person (a button that opens their card), a
//! group, a channel or a thread with its way back and its topic, and the
//! titles of Friends, Requests and Browse Channels. Task N07's
//! (docs/iced/tasks/shell.md). The Slint side: `ui/yapfast.slint:3141`-`:3300`.

use iced::widget::{button, container, row, rule, space, text};
use iced::{Alignment, Background, Border, Fill, Padding};

use anomi_people::friends::tabs::tab;
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

use super::limit::{gives_way, least};
use super::person::person;
use super::topic;
use super::{Intent, tinted};

/// The title's picture, and the icon before a channel's name.
const AVATAR: f32 = 26.0;
const GLYPH: f32 = 22.0;
/// The person's button (`dm-title`, `:3150`).
const BUTTON: f32 = 32.0;
/// The person's button: its least width (`:3162`), the space it leaves
/// before the picture and its row's own spacing (`dm-title-row`, `:3176`).
const PERSON_MIN: f32 = 42.0;
const PERSON_PAD_LEFT: f32 = 4.0;
const PERSON_GAP: f32 = 10.0;
/// The activity bubble: 24 px tall, round, 10 px in (`:3279`-`:3287`).
const BUBBLE_HEIGHT: f32 = 24.0;
const BUBBLE_RADIUS: f32 = 12.0;
const BUBBLE_PAD: f32 = 10.0;
/// The rule between Friends and its tabs (`:3297`), and the chevron after a
/// thread's channel (`:3226`).
const RULE: f32 = 24.0;
const CHEVRON: f32 = 16.0;
/// The least room the activity bubble keeps before it gives way (`:3284`).
const BUBBLE_ROOM: f32 = 96.0;

/// The title of `place`, its parts `gap` apart.
pub(super) fn view<'a>(place: &'a Place, gap: f32, motion: Motion) -> Element<'a, Intent> {
    match &place.title {
        Title::Direct(direct) => direct_title(direct, gap, motion),
        Title::Channel(channel) => channel_title(channel, gap, motion),
        Title::Screen(screen) => screen_title(*screen, gap, motion),
        Title::Sidebar(words) => sidebar_title(words),
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

fn direct_title<'a>(direct: &'a Direct, gap: f32, motion: Motion) -> Element<'a, Intent> {
    let picture = || avatar(&direct.avatar, AVATAR, direct.presence, Ground::Chat);
    let head: Element<'a, Intent> = if direct.person {
        let face = container(
            row![picture(), name(&direct.name)]
                .spacing(PERSON_GAP)
                .align_y(Alignment::Center),
        )
        .padding(Padding::from([0.0, metrics::SPACE_S]).left(PERSON_PAD_LEFT))
        .height(BUTTON)
        .align_y(Alignment::Center);
        let open = |bounds| Intent::Profile(Spot::below(bounds));
        person(opener(face).on_press(open), motion, open).boxed()
    } else {
        row![picture(), name(&direct.name)]
            .spacing(gap)
            .align_y(Alignment::Center)
            .boxed()
    };
    // It takes what room is left, cut at the buttons, and gives way where
    // that is less than its own width and less than 96 px (`:3284`).
    let bubble = direct.activity.as_ref().map(|status| {
        let bubble = container(
            status_text(status, motion)
                .color(|palette| palette.secondary)
                .font(typography::MEDIUM)
                .view(),
        )
        .padding([0.0, BUBBLE_PAD])
        .center_y(BUBBLE_HEIGHT)
        .style(|theme: &Theme| container::Style {
            background: Some(Background::Color(theme.palette().raised)),
            border: Border::default().rounded(BUBBLE_RADIUS),
            ..container::Style::default()
        });
        gives_way(container(bubble).center_y(Fill).clip(true), BUBBLE_ROOM)
    });
    let mut line = row![head]
        .spacing(gap)
        .align_y(Alignment::Center)
        .width(Fill);
    line = match bubble {
        Some(bubble) => line.push(bubble.boxed()),
        None => line.push(space().width(Fill).boxed()),
    };
    // Slint keeps the person's button at its `min-width` and elides the
    // name, where the buttons leave less; a group's picture stays round.
    least(line, if direct.person { PERSON_MIN } else { AVATAR }).boxed()
}

fn channel_title<'a>(channel: &'a Channel, gap: f32, motion: Motion) -> Element<'a, Intent> {
    let mut items: Vec<Element<'a, Intent>> = Vec::new();
    if let Some(parent) = &channel.parent {
        items.push(back(parent, gap).boxed());
    }
    items.push(kind(channel.icon).boxed());
    items.push(name(&channel.name).boxed());
    items.push(match channel.line.as_ref().or(channel.topic.as_ref()) {
        Some(text) => topic::line(text, gap, motion),
        None => space().width(Fill).boxed(),
    });
    row(items)
        .spacing(gap)
        .align_y(Alignment::Center)
        .width(Fill)
        .boxed()
}

/// A thread follows its channel, which leads back to it (`:3211`-`:3226`).
fn back<'a>(parent: &'a Parent, gap: f32) -> impl View<Intent> + 'a {
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
        tinted(Icon::ChevronRight, CHEVRON, |palette| palette.faint),
    ]
    .spacing(gap)
    .align_y(Alignment::Center)
}

fn screen_title<'a>(screen: Screen, gap: f32, motion: Motion) -> Element<'a, Intent> {
    let (glyph, words) = match screen {
        Screen::Friends { .. } => (Icon::Users, tr!("Friends")),
        Screen::Requests => (Icon::Inbox, tr!("Requests")),
        Screen::Browse => (Icon::Hash, tr!("Browse Channels")),
    };
    let mut items: Vec<Element<'a, Intent>> = vec![kind(glyph).boxed(), name(words).boxed()];
    if let Screen::Friends { all } = screen {
        items.push(
            container(rule::vertical(1).style(catalog::rule::line))
                .height(RULE)
                .boxed(),
        );
        items.push(tab(tr!("Online"), !all, Intent::FriendsAll(false)).boxed());
        items.push(tab(tr!("All"), all, Intent::FriendsAll(true)).boxed());
        items.push(
            action_button(tr!("Add Friend"), Kind::Primary, motion)
                .small()
                .on_press(Intent::AddFriend)
                .boxed(),
        );
    }
    row(items)
        .spacing(gap)
        .align_y(Alignment::Center)
        .width(Fill)
        .boxed()
}

/// The sidebar's title where no room is open: "Direct messages" or the
/// server's name, in 16 px bold secondary (`:3233`).
fn sidebar_title<'a>(words: &'a str) -> Element<'a, Intent> {
    row![
        text(words)
            .size(typography::size::TITLE)
            .font(typography::BOLD)
            .style(catalog::text::secondary)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End),
        space().width(Fill),
    ]
    .align_y(Alignment::Center)
    .width(Fill)
    .boxed()
}
