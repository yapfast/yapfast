//! Someone's badge window, as Discord's client opens it from a badge on a
//! profile: their badges at the levels they reached, each with its rarity,
//! what it says, when they got it and its levels; this account's own lists
//! every badge there is, with how far the next level is.

use iced::{
    Alignment, Element, Length,
    widget::{Space, column, container, row, text},
};
use yapfast_core::chat::{BadgeWindow, DirectoryBadge};

use super::Event;
use crate::{
    app::Message,
    fonts::{MEDIUM, SEMIBOLD},
    style::rounded,
    widgets::Pic,
};

/// The window's width, and a badge's picture in it.
const WIDTH: f32 = 440.0;
const ICON: f32 = 48.0;

/// A badge's rarity in Discord's words.
pub fn rarity(rarity: u8) -> &'static str {
    match rarity {
        2 => "Rare",
        3 => "Epic",
        4 => "Legendary",
        5 => "Mythic",
        _ => "Common",
    }
}

/// The badges in the order the window lists them: the one clicked first,
/// then the rest as Discord gave them, owned before still to be had.
pub fn ordered(window: &BadgeWindow) -> Vec<&DirectoryBadge> {
    let mut badges: Vec<&DirectoryBadge> = window.badges.iter().flatten().collect();
    badges.sort_by_key(|badge| (Some(badge.id) != window.badge, !badge.owned));
    badges
}

/// The window open, over everything but menus.
pub fn view<'a>(ctx: crate::ctx::Ctx<'a>) -> Option<Element<'a, Message>> {
    let window = ctx.state.badge_window.as_ref()?;
    let theme = ctx.look.theme;
    let own = window.user == "@me";
    let title = if own {
        "Your Badges".to_owned()
    } else if window.name.is_empty() {
        "Badges".to_owned()
    } else {
        format!("{}'s Badges", window.name)
    };
    let mut body: Vec<Element<'a, Message>> =
        vec![text(title).size(20).font(SEMIBOLD).color(theme.text).into()];
    match (&window.badges, &window.error) {
        (_, Some(error)) => body.push(text(error.clone()).size(14).color(theme.muted).into()),
        (None, None) => body.push(
            text("Loading the badges...")
                .size(14)
                .color(theme.muted)
                .into(),
        ),
        (Some(_), None) => {
            for badge in ordered(window) {
                body.push(badge_row(ctx, badge, own, Some(badge.id) == window.badge));
            }
        }
    }
    Some(
        ctx.look
            .framed(body, WIDTH, Message::Profile(Event::CloseBadges)),
    )
}

/// One badge: its picture, name and level, rarity, what it says, when it
/// was had, its levels, and for one's own, how far the next level is.
fn badge_row<'a>(
    ctx: crate::ctx::Ctx<'a>,
    badge: &'a DirectoryBadge,
    own: bool,
    chosen: bool,
) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let art = if badge.level_art.is_empty() {
        &badge.icon
    } else {
        &badge.level_art
    };
    let picture: Element<'a, Message> = match ctx.picture(art, ICON as u32, ICON as u32, false) {
        Some(picture) => Pic::square(picture, ICON)
            .opacity(if badge.owned { 1.0 } else { 0.4 })
            .into(),
        None => Space::new().width(ICON).height(ICON).into(),
    };
    let mut name = row![
        text(badge.name.as_str())
            .size(15)
            .font(SEMIBOLD)
            .color(theme.text)
    ]
    .spacing(6)
    .align_y(Alignment::Center);
    if let Some(level) = &badge.level {
        name = name.push(
            text(level.as_str())
                .size(13)
                .font(MEDIUM)
                .color(theme.secondary),
        );
    }
    let mut lines = column![
        name,
        text(rarity(badge.level_rarity.max(badge.rarity)))
            .size(12)
            .color(theme.muted),
    ]
    .spacing(3)
    .width(Length::Fill);
    for words in [&badge.description, &badge.info] {
        if !words.is_empty() {
            lines = lines.push(text(words.as_str()).size(13).color(theme.secondary));
        }
    }
    if let Some(unlocked) = &badge.unlocked {
        lines = lines.push(
            text(format!("Unlocked {unlocked}"))
                .size(12)
                .color(theme.muted),
        );
    } else if !badge.owned {
        lines = lines.push(text("Not unlocked yet").size(12).color(theme.muted));
    }
    if !badge.levels.is_empty() {
        let mut levels = row![].spacing(6);
        for level in &badge.levels {
            let ink = if level.owned { theme.text } else { theme.muted };
            let words = if level.milestone.is_empty() {
                level.name.clone()
            } else {
                format!("{} \u{00b7} {}", level.name, level.milestone)
            };
            // Lighter than the window's raised ground and the clicked
            // badge's row, so the levels show as pills.
            let fill = crate::style::blend(theme.raised, theme.wash(0.10));
            levels = levels.push(
                container(text(words).size(11).color(ink))
                    .padding([2, 6])
                    .style(move |_| rounded(fill, 8)),
            );
        }
        lines = lines.push(levels.wrap().vertical_spacing(4));
    }
    // How far the next level is, on one's own.
    if own && badge.span > 0 {
        let share = (badge.done as f32 / badge.span as f32).clamp(0.0, 1.0);
        let (track, ink) = (theme.raised, theme.accent);
        lines = lines.push(
            container(
                container(Space::new())
                    .width(Length::FillPortion((share * 100.0).round().max(1.0) as u16))
                    .height(4)
                    .style(move |_| rounded(ink, 2)),
            )
            .width(Length::Fill)
            .style(move |_| rounded(track, 2)),
        );
        if !badge.helper.is_empty() {
            lines = lines.push(text(badge.helper.as_str()).size(12).color(theme.muted));
        }
    }
    // The window's ground is the raised one: the clicked badge's row is a
    // shade lighter.
    let ground = if chosen {
        crate::style::blend(theme.raised, theme.wash(0.04))
    } else {
        iced::Color::TRANSPARENT
    };
    container(row![picture, lines].spacing(12).align_y(Alignment::Start))
        .padding(10)
        .width(Length::Fill)
        .style(move |_| rounded(ground, 8))
        .into()
}

/// The pictures the window shows.
pub fn pictures(ctx: crate::ctx::Ctx<'_>) -> Vec<String> {
    let Some(window) = ctx.state.badge_window.as_ref() else {
        return Vec::new();
    };
    window
        .badges
        .iter()
        .flatten()
        .map(|badge| {
            if badge.level_art.is_empty() {
                badge.icon.clone()
            } else {
                badge.level_art.clone()
            }
        })
        .filter(|url| !url.is_empty())
        .collect()
}
