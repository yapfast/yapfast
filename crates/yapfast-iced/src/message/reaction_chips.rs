//! A message's reaction chips, and the tooltip naming who reacted.

use std::time::Duration;

use iced::{
    Alignment, Color, Element, Length,
    widget::{Row, Space, container, mouse_area, row, text, tooltip},
};
use yapfast_core::chat::{ChatMessage, Reacts};

use crate::{
    app::Message,
    controls,
    fonts::{BOLD, MEDIUM},
    widgets::Pic,
};

use super::REACTION_EMOJI;

/// What a reaction's tooltip says, worded as Discord's client words it.
pub fn reaction_words(emoji: &str, names: &[String], count: usize) -> String {
    let others = count.saturating_sub(names.len());
    let more = match others {
        1 => "1 other".to_owned(),
        others => format!("{others} others"),
    };
    let people = match (names, others) {
        ([one], 0) => one.clone(),
        ([one], _) => format!("{one} and {more}"),
        ([one, two], 0) => format!("{one} and {two}"),
        ([one, two], _) => format!("{one}, {two}, and {more}"),
        ([one, two, three], 0) => format!("{one}, {two}, and {three}"),
        ([one, two, three, ..], _) => format!("{one}, {two}, {three}, and {more}"),
        _ => format!("{count} people"),
    };
    format!("{emoji} reacted by {people}")
}

/// A reaction's emoji as Discord's client names it: `:name:` for a custom
/// one, the first shortcode that's a word for a Unicode one.
pub fn emoji_shortcode(key: &str) -> String {
    if key.starts_with(':') && key.ends_with(':') && key.len() > 2 {
        return key.to_owned();
    }
    let codes = || {
        emojis::get(key)
            .into_iter()
            .flat_map(emojis::Emoji::shortcodes)
    };
    codes()
        .find(|code| code.starts_with(|c: char| c.is_ascii_alphanumeric()))
        .or_else(|| codes().next())
        .map_or_else(|| key.to_owned(), |code| format!(":{code}:"))
}

/// What reacting this account may do in row `index`'s timeline: the
/// channel's, or the thread's beside it.
pub fn reacts(ctx: crate::ctx::Ctx<'_>, index: usize) -> Reacts {
    let room = if index >= crate::timeline::ASIDE {
        ctx.lanes.aside.map(|aside| &aside.room)
    } else {
        ctx.state.selected.as_ref()
    };
    room.map_or(Reacts::None, |room| ctx.state.reacts_in(room))
}

/// A message's reaction chips, on as many lines as the message's width
/// asks. A click adds this account's reaction or takes it back, where it
/// may react at all. Each has its tooltip from the start, so it knows the
/// pointer is on the chip when the names come: empty until then.
pub(super) fn reactions<'a>(
    ctx: crate::ctx::Ctx<'a>,
    index: usize,
    message: &'a ChatMessage,
) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let clicks = message.can_react && reacts(ctx, index) != Reacts::None;
    let mut chips = Vec::with_capacity(message.reactions.len());
    for reaction in &message.reactions {
        // The accent's wash and edge when this account reacted.
        let (ground, edge, count) = if reaction.mine {
            (
                theme.accent_soft,
                Color {
                    a: 0.7,
                    ..theme.accent
                },
                theme.accent,
            )
        } else {
            (theme.raised, Color::TRANSPARENT, theme.secondary)
        };
        // A custom emoji's picture, as the message's own emoji have it,
        // playing when it moves.
        let custom = message
            .emoji
            .iter()
            .find(|emoji| emoji.text == reaction.key);
        let playing = custom.and_then(|emoji| {
            ctx.playing_emoji(&emoji.picture, REACTION_EMOJI, &ctx.pictures.players)
        });
        let face: Element<'a, Message> = if let Some(playing) = playing {
            playing.into()
        } else if let Some(picture) = custom
            .and_then(|emoji| ctx.picture(&emoji.picture, REACTION_EMOJI, REACTION_EMOJI, false))
        {
            Pic::square(picture, REACTION_EMOJI as f32).into()
        } else {
            text(reaction.key.as_str()).color(theme.text).into()
        };
        let chip = container(
            row![
                face,
                text(reaction.count.to_string()).font(BOLD).color(count)
            ]
            .spacing(4)
            .align_y(Alignment::Center),
        )
        .padding([2, 8])
        .style(move |_| crate::style::outlined(ground, edge, 8.0));
        let mut chip = mouse_area(chip);
        if clicks {
            chip = chip
                .on_press(Message::Menu(crate::menus::Event::React(
                    message.item.clone(),
                    reaction.key.clone(),
                )))
                .interaction(iced::mouse::Interaction::Pointer);
        }
        let chip = chip
            .on_enter(Message::Timeline(crate::timeline::Event::PointReaction(
                message.item.clone(),
                reaction.key.clone(),
                true,
            )))
            .on_exit(Message::Timeline(crate::timeline::Event::PointReaction(
                message.item.clone(),
                reaction.key.clone(),
                false,
            )));
        let words = reaction_tip(ctx, &message.item, &reaction.key)
            .unwrap_or_else(|| Space::new().width(0).height(0).into());
        chips.push(
            tooltip(chip, words, tooltip::Position::Top)
                .gap(6)
                .delay(Duration::from_millis(500))
                .style(|_| container::Style::default())
                .into(),
        );
    }
    Row::from_vec(chips)
        .spacing(4)
        .wrap()
        .vertical_spacing(4)
        .into()
}

/// A reaction's tooltip, once the core has the names, for the chip the
/// pointer stays on.
pub(super) fn reaction_tip<'a>(
    ctx: crate::ctx::Ctx<'a>,
    item: &str,
    key: &str,
) -> Option<Element<'a, Message>> {
    let (tip_of, reaction) = ctx.state.reaction_tip()?;
    if tip_of.item != item || tip_of.key != key {
        return None;
    }
    let theme = ctx.look.theme;
    let names = tip_of.names.as_deref().unwrap_or_default();
    let words = reaction_words(&emoji_shortcode(key), names, reaction.count);
    let picture = emojis::get(key)
        .map(emojis::Emoji::as_str)
        .and_then(|key| ctx.emoji_picture(key, 32));
    let face: Element<'a, Message> = match picture {
        Some(picture) => Pic::square(picture, 32.0).into(),
        None => text(key.to_owned()).size(26).into(),
    };
    Some(
        container(
            row![
                face,
                text(words)
                    .size(14)
                    .font(MEDIUM)
                    .color(theme.text)
                    .width(Length::Shrink)
            ]
            .spacing(12)
            .align_y(Alignment::Center),
        )
        .padding(16)
        .max_width(288)
        .style(controls::tip_style(theme))
        .into(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reactions_tooltip_names_up_to_three_then_counts_the_others() {
        let names = |names: &[&str]| {
            names
                .iter()
                .map(|name| (*name).to_owned())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            reaction_words(":tada:", &names(&["Alice"]), 1),
            ":tada: reacted by Alice"
        );
        assert_eq!(
            reaction_words(":tada:", &names(&["Alice", "Bob", "Carol"]), 4),
            ":tada: reacted by Alice, Bob, Carol, and 1 other"
        );
        assert_eq!(emoji_shortcode("🎉"), ":tada:");
    }
}
