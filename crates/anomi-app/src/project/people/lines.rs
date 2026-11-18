//! What many of people's rows share: the line under a name (a custom status
//! or what they're doing, with its icon), someone's activities as their
//! profile's cards show them, and the times those count from. Ported from
//! `subline`, `activity_line`, `activity_row` and `names_an_activity` of
//! `src/ui/profile.rs`.

use yapfast_core::application::AppState;
use yapfast_core::chat::{Activities, NameStyle as ChatStyle, ServerTag};
use yapfast_core::discord;

use anomi_vm::kind::{Doing, NameStyle, Status, Tag};
use anomi_vm::people::activity::card::Card;

use crate::project::rows;

/// The time now, in Unix seconds: the clock activity cards, Active Now and
/// the profile's `now` count from.
pub fn now() -> i64 {
    i64::try_from(yapfast_core::clock::now_ms() / 1000).unwrap_or(i64::MAX)
}

/// Discord's Unix milliseconds in seconds; 0 when unknown.
pub fn seconds(ms: Option<u64>) -> i64 {
    ms.map_or(0, |ms| i64::try_from(ms / 1000).unwrap_or(0))
}

/// Words on one line, their runs of spaces and breaks made one space.
pub fn one_line(words: &str) -> String {
    words.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The icon a [`yapfast_core::yapfast_discord::activity::Subline`] names.
fn doing(icon: u8) -> Doing {
    match icon {
        1 => Doing::Playing,
        2 => Doing::Listening,
        3 => Doing::Watching,
        4 => Doing::Streaming,
        5 => Doing::Competing,
        _ => Doing::Nothing,
    }
}

/// The line under a name in the member list and the DM list: the custom
/// status, or what they're doing with its kind's icon, and whether a bullet
/// follows the icon (a custom status beside an activity).
pub fn subline(state: &AppState, activities: &Activities, line: Option<&str>) -> (Status, bool) {
    let line = crate::language::once::line(activities, line);
    let line = line.as_deref();
    #[cfg(feature = "discord")]
    let (icon, bullet, named) = {
        let subline = yapfast_core::yapfast_discord::activity::subline(activities, line);
        (subline.icon, subline.bullet, subline.name)
    };
    #[cfg(not(feature = "discord"))]
    let (icon, bullet, named): (u8, bool, Option<String>) = {
        let _ = activities;
        (0, false, None)
    };
    let shown = named.as_deref().or(line);
    let (emoji, words) = shown.map_or((None, ""), discord::status_parts);
    let status = Status {
        text: one_line(words),
        emoji: rows::expected(state, emoji.as_deref()),
        doing: doing(icon),
    };
    (status, bullet)
}

/// The server tag someone wears, while tags show, with the server whose
/// card a click on it opens (`tag_text`, `tag_badge`, `tag_server`).
pub fn tag(state: &AppState, tag: Option<&ServerTag>) -> Option<Tag> {
    state.cosmetics.tag(tag).map(|tag| Tag {
        text: tag.text.to_string(),
        badge: rows::expected(state, tag.badge.as_deref()),
        server: tag
            .guild
            .map(|guild| discord::server_id(&guild.to_string()))
            .unwrap_or_default(),
    })
}

/// How someone draws their name, while styles show; in a server, its face
/// alone (`name_style::font_only_in`).
pub fn name_style(
    state: &AppState,
    style: Option<&ChatStyle>,
    in_server: bool,
) -> Option<NameStyle> {
    let style = state.cosmetics.style(style)?;
    let style = NameStyle::new(style.font, style.effect, style.colors())?;
    Some(if in_server { style.font_only() } else { style })
}

/// A place is in a Discord server, not a DM (`name_style::in_server`).
pub fn in_server(place: Option<&str>) -> bool {
    place
        .and_then(discord::parse_room)
        .is_some_and(|(guild, _)| guild != discord::DM_GUILD)
}

/// The address of the custom emoji a line under a name starts with.
pub fn line_emoji(line: Option<&str>) -> Option<String> {
    line.and_then(|line| discord::status_parts(line).0)
}

/// A custom status or what someone's doing as one line, with no icon: a
/// friend's row (`activity_line`).
pub fn status(state: &AppState, activities: &Activities, line: Option<&str>) -> Status {
    let line = crate::language::once::line(activities, line);
    let line = line.as_deref();
    let (emoji, words) = line.map_or((None, ""), discord::status_parts);
    Status {
        text: one_line(words),
        emoji: rows::expected(state, emoji.as_deref()),
        ..Status::default()
    }
}

/// Whether a line under a name only says what one of the activities does,
/// which their cards say more of, in whichever language it was worded.
pub fn names_an_activity(line: &str, activities: &Activities) -> bool {
    crate::language::once::names_an_activity(line, activities)
}

/// Someone's activities as their profile's cards show them.
pub fn cards(state: &AppState, activities: &Activities) -> Vec<Card> {
    #[cfg(feature = "discord")]
    return activities
        .iter()
        .map(|activity| card(state, activity))
        .collect();
    #[cfg(not(feature = "discord"))]
    {
        let _ = (state, activities);
        Vec::new()
    }
}

/// One activity's card (`activity_row`): Spotify's titled by the song, then
/// by whom and on which album; a Hang status's emoji before its words.
#[cfg(feature = "discord")]
fn card(state: &AppState, activity: &yapfast_core::chat::Activity) -> Card {
    use yapfast_lang::tr;
    let text = |text: &Option<Box<str>>| text.as_deref().unwrap_or_default().to_owned();
    let name = match &activity.emoji {
        Some(emoji) => format!("{emoji} {}", activity.name),
        None => activity.name.to_string(),
    };
    let (name_emoji, name) = discord::status_parts(&name);
    let party = match (&activity.state, activity.party) {
        (Some(state), Some((size, most))) => tr!("{} ({} of {})", state, size, most),
        (None, Some((size, most))) => tr!("{} of {}", size, most),
        (state, None) => state.as_deref().unwrap_or_default().to_owned(),
    };
    let (title, details, line) = if activity.spotify() {
        let after = |text: &Option<Box<str>>, say: fn(&str) -> String| {
            text.as_deref().map_or(String::new(), say)
        };
        (
            activity.details.as_deref().unwrap_or(name).to_owned(),
            after(&activity.state, |text| tr!("by {}", text)),
            after(&activity.large_text, |text| tr!("on {}", text)),
        )
    } else {
        (
            name.to_owned(),
            activity.details.as_deref().unwrap_or_default().to_owned(),
            party,
        )
    };
    Card {
        heading: activity.heading().to_uppercase(),
        name: Status {
            text: title,
            emoji: rows::expected(state, name_emoji.as_deref()),
            ..Status::default()
        },
        details,
        state: line,
        large: rows::expected(state, activity.large_image.as_deref()),
        large_text: text(&activity.large_text),
        small: rows::expected(state, activity.small_image.as_deref()),
        small_text: text(&activity.small_text),
        start: seconds(activity.start),
        end: seconds(activity.end),
        url: text(&activity.url),
        link: text(&activity.link),
        game: activity.kind == yapfast_core::chat::ActivityKind::Playing,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_keeps_its_words_on_one_line() {
        assert_eq!(one_line("  out \n for  lunch "), "out for lunch");
        assert_eq!(seconds(Some(5_500)), 5);
        assert_eq!(seconds(None), 0);
    }
}
