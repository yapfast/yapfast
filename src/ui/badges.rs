//! The badge windows: someone's, opened from a badge on their profile, and
//! the account's own; see `ui/badges.slint` and "Badge windows" in
//! docs/discord/people.md.

use yapfast_core::chat::DirectoryBadge;

use super::*;

impl Adapter {
    /// The badge window open over the profile, if any.
    pub(super) fn show_badges(&self, window: &YapfastWindow) {
        let state = &self.app.state;
        let Some(open) = state.badge_window.as_ref() else {
            window.set_badges_open(false);
            return;
        };
        let own = open.user.is_empty();
        let mine = state.own_badges.as_deref().unwrap_or_default();
        let (rows, available, note, failed): (Vec<BadgeRow>, Vec<BadgeRow>, &str, bool) = if own {
            let note = match (&state.own_badges, &state.own_badges_error) {
                (None, Some(_)) => "Discord didn't share your badges.",
                (None, None) => "Finding your badges...",
                _ => "",
            };
            (
                mine.iter()
                    .filter(|badge| badge.owned)
                    .map(own_badge_row)
                    .collect(),
                mine.iter()
                    .filter(|badge| !badge.owned && badge.earnable)
                    .map(own_badge_row)
                    .collect(),
                note,
                state.own_badges.is_none() && state.own_badges_error.is_some(),
            )
        } else {
            let note = match (&open.badges, &open.error) {
                (None, Some(_)) => "Oh no! Something went wrong.",
                (None, None) => "Finding their badges...",
                (Some(badges), _) if !badges.iter().any(|badge| badge.owned) => {
                    "No badges to show."
                }
                _ => "",
            };
            (
                open.badges
                    .as_deref()
                    .unwrap_or_default()
                    .iter()
                    .filter(|badge| badge.owned)
                    .map(|badge| {
                        person_badge_row(badge, mine.iter().find(|mine| mine.id == badge.id))
                    })
                    .collect(),
                Vec::new(),
                note,
                open.badges.is_none() && open.error.is_some(),
            )
        };
        let first = open
            .badge
            .and_then(|id| rows.iter().position(|row| i64::from(row.id) == id as i64))
            .unwrap_or(0);
        window.set_badges_person(if own {
            SharedString::new()
        } else {
            badge_window_name(&open.name).into()
        });
        window.set_badges_avatar(open.avatar.as_deref().unwrap_or_default().into());
        window.set_badges_first(first as i32);
        window.set_badges_note(note.into());
        window.set_badges_failed(failed);
        // Set only when they change, so the badge picked in the window stays
        // through a refresh.
        if !same_rows(&window.get_badges_owned(), &rows) {
            window.set_badges_owned(ModelRc::new(VecModel::from(rows)));
        }
        if !same_rows(&window.get_badges_available(), &available) {
            window.set_badges_available(ModelRc::new(VecModel::from(available)));
        }
        window.set_badges_open(true);
    }
}

/// Whether the rows on screen are these, levels and all.
fn same_rows(shown: &ModelRc<BadgeRow>, rows: &[BadgeRow]) -> bool {
    let bare = |row: &BadgeRow| BadgeRow {
        levels: ModelRc::default(),
        ..row.clone()
    };
    shown.row_count() == rows.len()
        && rows.iter().enumerate().all(|(index, row)| {
            shown.row_data(index).is_some_and(|shown| {
                bare(&shown) == bare(row) && shown.levels.iter().eq(row.levels.iter())
            })
        })
}

fn rarity(rarity: u8) -> SharedString {
    match rarity {
        1 => "Common",
        2 => "Rare",
        3 => "Epic",
        5 => "Mythic",
        _ => "",
    }
    .into()
}

fn level_rows(badge: &DirectoryBadge) -> ModelRc<BadgeLevelRow> {
    ModelRc::new(VecModel::from(
        badge
            .levels
            .iter()
            .map(|level| BadgeLevelRow {
                name: level.name.as_str().into(),
                icon: level.icon.as_str().into(),
                owned: level.owned,
                milestone: level.milestone.as_str().into(),
            })
            .collect::<Vec<_>>(),
    ))
}

/// One of this account's badges, as its own window shows it: headed as
/// Discord heads it, with the level it's reached, and how far to the next.
pub(super) fn own_badge_row(badge: &DirectoryBadge) -> BadgeRow {
    BadgeRow {
        id: i32::try_from(badge.id).unwrap_or_default(),
        name: match badge.levels.iter().rev().find(|level| level.owned) {
            Some(level) if badge.owned => format!("{} {}", badge.name, level.name),
            _ => badge.name.clone(),
        }
        .to_uppercase()
        .into(),
        owned: badge.owned,
        description: badge.description.as_str().into(),
        info: badge.info.as_str().into(),
        rarity: rarity(badge.rarity),
        icon: badge.icon.as_str().into(),
        art: badge.art.as_deref().unwrap_or(&badge.icon).into(),
        unlocked: badge.unlocked.as_deref().unwrap_or_default().into(),
        has_next: badge.next_name.is_some(),
        next_name: badge.next_name.as_deref().unwrap_or_default().into(),
        next_icon: badge.next_icon.as_deref().unwrap_or_default().into(),
        progress: if badge.span > 0 {
            (badge.done as f32 / badge.span as f32).clamp(0.0, 1.0)
        } else {
            0.0
        },
        helper: badge.helper.as_str().into(),
        levels: level_rows(badge),
        ..Default::default()
    }
}

/// Someone's badge, as their window shows it: at the level they've reached,
/// with what the client takes from this account's own badge, `mine`: the words
/// about unlocking it, and whether to offer its own.
pub(super) fn person_badge_row(badge: &DirectoryBadge, mine: Option<&DirectoryBadge>) -> BadgeRow {
    /// The client's badge numbers it treats specially.
    const NITRO: u64 = 1;
    const QUESTS: u64 = 14;
    const APRIL_FOOLS: u64 = 15;
    const ORBS: u64 = 16;
    const GIFTING: u64 = 17;
    const STAFF: u64 = 22;
    let nitro = badge.id == NITRO;
    let (eyebrow, name) = match (&badge.level, nitro) {
        (Some(level), true) => (String::new(), format!("Nitro {level}")),
        (None, true) => (String::new(), "Nitro".to_owned()),
        (Some(level), false) => (badge.name.clone(), level.clone()),
        (None, false) => (String::new(), badge.name.clone()),
    };
    let mine_owned = mine.is_some_and(|mine| mine.owned);
    // Those from before badges could be earned, which the client's map knows:
    // the numbers below Gifting.
    let legacy = badge.id < GIFTING && !badge.earnable;
    let date = if badge.id == STAFF {
        None
    } else if badge.id == APRIL_FOOLS {
        Some("April 2026".to_owned())
    } else if legacy && badge.unlocked.is_none() {
        // The client picks one of three at random when it has no date; this
        // picks the same one each time.
        let lines = ["Very, very early", "iykyk", "Lost to time"];
        Some(lines[badge.id as usize % lines.len()].to_owned())
    } else {
        badge.unlocked.clone()
    };
    // What its button does in the client, as far as a page of Discord's goes:
    // it never gives or buys anything from here. Boosting has only settings.
    let (action, link) = match badge.id {
        STAFF => ("Work With Us", "https://discord.com/careers"),
        NITRO if mine_owned => ("Nitro Home", "https://discord.com/store"),
        NITRO => ("Get Nitro", "https://discord.com/store"),
        ORBS => ("Orbs Shop", "https://discord.com/shop?tab=orbs"),
        QUESTS => ("Quests Home", "https://discord.com/quest-home"),
        GIFTING => ("Send Gift", "https://discord.com/shop"),
        _ => ("", ""),
    };
    let maxed =
        mine.is_some_and(|mine| mine.owned && !mine.levels.is_empty() && mine.next_name.is_none());
    BadgeRow {
        id: i32::try_from(badge.id).unwrap_or_default(),
        name: name.to_uppercase().into(),
        owned: true,
        description: if maxed {
            "You've maxed out this badge. Sit back, relax, and enjoy the view from the top.".into()
        } else {
            mine.map_or(badge.description.as_str(), |mine| mine.description.as_str())
                .into()
        },
        info: badge.info.as_str().into(),
        rarity: rarity(badge.level_rarity),
        icon: badge.icon.as_str().into(),
        art: badge.level_art.as_str().into(),
        unlocked: date.unwrap_or_default().into(),
        levels: level_rows(badge),
        eyebrow: eyebrow.into(),
        date_label: if badge.first_level {
            "Date unlocked"
        } else {
            "Level unlocked"
        }
        .into(),
        // Account Age, Streaming, Game Time and Game Variety.
        beta: matches!(badge.id, 18..=21),
        legacy,
        gone: !badge.earnable && !mine_owned && badge.id != STAFF,
        action: action.into(),
        action_link: link.into(),
        action_quiet: mine_owned,
        view_yours: mine_owned,
        ..Default::default()
    }
}

/// A name as the badge window's headings shorten it.
pub(super) fn badge_window_name(name: &str) -> String {
    if name.chars().count() > 19 {
        format!("{}...", name.chars().take(19).collect::<String>())
    } else {
        name.to_owned()
    }
}
