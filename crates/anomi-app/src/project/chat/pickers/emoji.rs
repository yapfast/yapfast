//! The emoji picker from the core's state (`show_emoji`,
//! `src/ui/callbacks.rs:426`; `rows`, `src/emoji.rs:450`): the account's
//! favourites, what it uses most (or reacts with most, for a reaction), each
//! server's emoji, then Unicode's categories, as the core names and offers
//! them (`yapfast_core::emoji`); and the custom emoji's pictures, asked for
//! within three screens of the rows in sight, nearest first, and let go of
//! beyond (`prepare_emoji_rows`, `let_go_of_far_emoji`,
//! `src/ui/callbacks.rs:42`-`:120`).
//!
//! The thousands of Unicode emoji are the `emojis` crate's own, a pointer
//! each; the grid builds only the rows near its view. What the picker keeps
//! besides ([`Held`]) goes with it.

use std::collections::HashMap;

use yapfast_core::application::{AppState, Application};
use yapfast_core::chat::CustomEmoji;
use yapfast_core::emoji;
use yapfast_lang::tr;

use anomi_theme::fonts::twemoji;
use anomi_vm::chat::emoji_picker::{
    self, Emoji, EmojiPicker, Purpose, SHOWN_ROWS, Section, SectionIcon,
};
use anomi_vm::kind::Expected;

use crate::pictures::{self, Requests};

/// How many of this device's reactions follow the account's own frequent
/// emoji (`frequent_reactions(16)`).
const FREQUENT_REACTIONS: usize = 16;

/// What the emoji picker keeps beside its view-model while it shows.
#[derive(Default)]
pub(crate) struct Held {
    /// Each custom emoji's picture address, by its id, as the picker was
    /// built: a picture asked for or come is found by it.
    urls: HashMap<Box<str>, Box<str>>,
    /// The sections shown by a picture (a server's icon, or its first
    /// emoji's), by their place, with its address.
    icons: Vec<(usize, Box<str>)>,
    /// The custom emoji pictures the core held as the picker opened, which
    /// the rest of the window shows: they stay as the picker lets go of its
    /// own (`KeepEmojiPictures`).
    pub(crate) before: Vec<String>,
    /// The core's count of pictures as the rows near the view were last
    /// filled.
    changes: Option<u64>,
}

impl Held {
    /// The picture address of the custom emoji `id`: as the picker found
    /// it, else as Discord serves a still one.
    fn url(&self, id: &str) -> Option<String> {
        match self.urls.get(id) {
            Some(url) => Some(url.to_string()),
            None => yapfast_core::discord::emoji_picture(id),
        }
    }

    /// Notes the custom emoji pictures the core holds, before the picker
    /// asks for any.
    pub(crate) fn opened(&mut self, state: &AppState) {
        self.before = (state.pictures.keys())
            .filter(|url| url.contains("/emojis/"))
            .cloned()
            .collect();
    }
}

/// Builds every section of `picker` for its purpose, keeping its search and
/// its matches, and asks for the pictures of the rows near the view and of
/// the rail's servers.
pub(crate) fn build(
    core: &Application,
    picker: &mut EmojiPicker,
    held: &mut Held,
    requests: &mut Requests,
) {
    let state = &core.state;
    let sets = core.emoji_sets();
    let by_id: HashMap<&str, &CustomEmoji> = (sets.iter())
        .flat_map(|set| &set.emoji)
        .map(|emoji| (emoji.id.as_str(), emoji))
        .collect();
    // The account's favourites, then what it uses most, or reacts with most
    // for a reaction, as its other clients keep them, and what this device
    // reacts with.
    let lists = &state.discord_emojis;
    let used = match picker.purpose {
        Purpose::Reaction => &lists.reactions,
        _ => &lists.frequent,
    };
    let mut frequent = emoji::discord_keys(used, &by_id);
    for key in core.frequent_reactions(FREQUENT_REACTIONS) {
        if !frequent.iter().any(|(known, _)| *known == key) {
            frequent.push((key, true));
        }
    }
    let favorites = emoji::discord_keys(&lists.favorites, &by_id);
    picker.favorites = favorites.iter().map(|(key, _)| key.clone()).collect();
    held.urls = (by_id.values())
        .map(|emoji| (emoji.id.as_str().into(), emoji.picture.as_str().into()))
        .collect();
    held.icons.clear();
    let mut sections = Vec::with_capacity(sets.len() + 11);
    let favorites = cells(&favorites);
    push(
        &mut sections,
        tr!("Favorites"),
        SectionIcon::Favorites,
        favorites,
    );
    let frequent = cells(&frequent);
    push(
        &mut sections,
        tr!("Frequently Used"),
        SectionIcon::Frequent,
        frequent,
    );
    for set in &sets {
        let Some(first) = set.emoji.first() else {
            continue;
        };
        // Shown in the rail by its icon, else by its first emoji.
        let icon = set.icon.as_deref().unwrap_or(&first.picture);
        held.icons.push((sections.len(), icon.into()));
        let emoji = (set.emoji.iter()).map(|emoji| {
            let name = emoji.text.trim_matches(':');
            Emoji::custom(name, &emoji.id, Expected::Waiting, emoji.usable)
        });
        let picture = Expected::Waiting;
        let id = set.id.clone();
        let icon = SectionIcon::Server { id, picture };
        push(&mut sections, &set.server, icon, emoji.collect());
    }
    // Those Twemoji draws, of the versions Discord's client offers.
    let drawn = |unicode: &_| emoji::offered(unicode, twemoji::has_picture);
    sections.extend(emoji_picker::categories(emoji::category, drawn));
    picker.sections = sections;
    let query = std::mem::take(&mut picker.query);
    search(picker, query);
    // The rail shows every server's icon at once.
    let (servers, first): (Vec<&str>, Vec<&str>) = (held.icons.iter())
        .map(|(_, url)| &**url)
        .partition(|url| !url.contains("/emojis/"));
    requests.want(state, servers);
    requests.want_first(state, first);
    held.changes = None;
    fill(state, picker, held);
}

/// A section, unless it has no emoji.
fn push(sections: &mut Vec<Section>, name: &str, icon: SectionIcon, emoji: Vec<Emoji>) {
    if !emoji.is_empty() {
        sections.push(Section {
            name: name.to_owned(),
            icon,
            emoji,
        });
    }
}

/// The emoji of `keys`, the picker's keys with whether each may be used
/// here: a custom one's `name:id`, else Unicode's own.
fn cells(keys: &[(String, bool)]) -> Vec<Emoji> {
    (keys.iter())
        .filter_map(|(key, usable)| match key.split_once(':') {
            Some((name, id)) => Some(Emoji::custom(name, id, Expected::Waiting, *usable)),
            None => Emoji::unicode(key),
        })
        .collect()
}

/// The search field's text: the grid shows its matches, from their top.
pub(crate) fn search(picker: &mut EmojiPicker, query: String) {
    picker.found = emoji_picker::found(&picker.sections, &query, |unicode, query| {
        emoji::offered(unicode, twemoji::has_picture) && emoji::matches(unicode, query)
    });
    picker.query = query;
    picker.shown = (0, SHOWN_ROWS);
}

/// The rows in sight moved, or the grid was laid out again: the custom
/// emoji's pictures far from them go, those near are asked for nearest
/// first, and those here show. Gives what the core keeps of its custom
/// emoji pictures: those near, and those the rest of the window shows.
pub(crate) fn seen(
    state: &AppState,
    picker: &mut EmojiPicker,
    held: &mut Held,
    requests: &mut Requests,
) -> Vec<String> {
    picker.let_go_far();
    let wanted: Vec<String> = (picker.wanted().into_iter())
        .filter_map(|id| held.url(id))
        .collect();
    requests.want_first(state, wanted.iter().map(String::as_str));
    held.changes = None;
    fill(state, picker, held);
    let mut keep = held.before.clone();
    keep.extend(near(picker).filter_map(|id| held.url(id)));
    keep.extend(held.icons.iter().map(|(_, url)| url.to_string()));
    keep
}

/// The ids of the custom emoji in the rows near the view.
fn near(picker: &EmojiPicker) -> impl Iterator<Item = &str> {
    let grid = picker.grid();
    let spans: Vec<_> = picker.held().filter_map(|row| grid.span(row)).collect();
    spans.into_iter().flat_map(move |(section, range)| {
        let emoji = match section {
            Some(section) => &picker.sections[section].emoji[range],
            None => &picker.found[range],
        };
        emoji.iter().filter_map(Emoji::id)
    })
}

/// Gives the custom emoji near the view, and the rail's servers, the
/// pictures that came since they were last filled.
pub(crate) fn fill(state: &AppState, picker: &mut EmojiPicker, held: &mut Held) {
    if held.changes == Some(state.picture_changes) {
        return;
    }
    held.changes = Some(state.picture_changes);
    for (index, url) in &held.icons {
        if let Some(Section {
            icon: SectionIcon::Server { picture, .. },
            ..
        }) = picker.sections.get_mut(*index)
            && *picture == Expected::Waiting
            && let Some(handle) = pictures::find(state, Some(url))
        {
            *picture = Expected::Ready(handle);
        }
    }
    let spans: Vec<_> = {
        let grid = picker.grid();
        picker.held().filter_map(|row| grid.span(row)).collect()
    };
    for (section, range) in spans {
        let emoji = match section {
            Some(section) => &mut picker.sections[section].emoji[range],
            None => &mut picker.found[range],
        };
        for emoji in emoji {
            let Emoji::Custom(custom) = emoji else {
                continue;
            };
            if custom.picture != Expected::Waiting {
                continue;
            }
            let id = custom.key.rsplit_once(':').map_or("", |(_, id)| id);
            let url = held.url(id);
            if let Some(handle) = pictures::find(state, url.as_deref()) {
                custom.picture = Expected::Ready(handle);
            }
        }
    }
}
