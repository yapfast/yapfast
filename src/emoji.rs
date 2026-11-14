//! Emoji: the font for reactions and message text, and the picker's grid.
//!
//! Every emoji is Twemoji's, as Discord's client draws them, from the font the
//! program carries ([`crate::emoji_text`]): no system emoji font is used, on
//! any platform.
//!
//! On Windows, text falls back to fonts by script, and emoji, most symbols and
//! the styled letters of Unicode's mathematical alphabets that names use, such
//! as 𝓯𝓪𝓷𝓬𝔂, are all of the "Common" script, which no font is picked for. So
//! Yapfast puts the fonts Windows ships for them in the fallback itself.

use std::collections::HashMap;
use std::sync::OnceLock;

use slint::{
    Model, ModelRc, SharedString, VecModel,
    fontique_012::{fontique, shared_collection},
};
use yapfast_core::{chat::EmojiSet, i18n::tr};
use yapfast_ui::{EmojiCell, EmojiRow, EmojiSection, SectionGlyph};

use crate::custom_emoji;

/// The heading Frequently Used's section goes under, which both pickers draw a
/// clock for rather than an emoji's picture.
pub const FREQUENTLY_USED: &str = "Frequently Used";

/// The heading Favorites' section goes under, which both pickers draw the icon
/// set's star for.
pub const FAVORITES: &str = "Favorites";

/// What a section draws in place of a picture, from the icon set rather than the
/// platform's emoji font: the sections without a server's icon are the two this
/// device keeps.
pub fn section_glyph(heading: &str) -> SectionGlyph {
    match heading {
        FREQUENTLY_USED => SectionGlyph::Clock,
        FAVORITES => SectionGlyph::Star,
        _ => SectionGlyph::None,
    }
}

/// The emoji font family to request, everywhere an emoji shows: Twemoji's,
/// which the program carries ([`crate::twemoji`]) and which Discord's client
/// draws its emoji from, rather than the system's. Call after selecting the
/// Slint backend.
pub fn family() -> &'static str {
    static FAMILY: OnceLock<&'static str> = OnceLock::new();
    FAMILY.get_or_init(|| {
        if cfg!(windows) {
            prefer_families(&mut shared_collection(), &WINDOWS_FALLBACK);
        }
        crate::emoji_text::FAMILY
    })
}

/// Emoji per row of the picker's grid: a button is 56px, as Discord's, and the
/// sidebar beside the grid and the grid's own padding take 64px of the width it
/// shows at, as `EmojiPicker` in ui/messages.slint lays them out.
pub const CELL: f32 = 56.0;
/// The size a glyph is drawn at in that grid, as `EmojiPicker` in
/// ui/messages.slint passes it.
pub const GLYPH: f32 = 36.0;

/// The height of a grid row of emoji, which the rail's sections are placed by.
pub const ROW_HEIGHT: f32 = 56.0;

/// The height of a section's heading row: a row of its own, tighter than the
/// emoji's, so a heading sits at the top of the grid as the rail's buttons do
/// and its emoji follow close under it.
pub const HEADING_HEIGHT: f32 = 28.0;

/// The size the picker's rail draws its icons at, which the stickers' rail
/// draws its own at too.
pub const RAIL_GLYPH: f32 = 32.0;
const BESIDE: f32 = 64.0;

/// The emoji per row of a grid showing at `width`, which follows the picker's
/// width as the GIFs' columns do: as many as it holds, with a column kept even
/// when it is narrower than one emoji.
pub fn columns(width: f32) -> usize {
    (((width - BESIDE) / CELL).floor() as usize).max(1)
}

/// Windows 11 and the Noto Color Emoji builds Yapfast is tested with draw Unicode
/// 15.0; newer emoji would show as empty boxes.
const NEWEST: emojis::UnicodeVersion = emojis::UnicodeVersion::new(15, 0);

/// Whether an emoji is one the picker and the suggestions offer: one Twemoji
/// draws, since no emoji is drawn by the system's font, and of the versions
/// Discord's client offers.
fn drawn(emoji: &&'static emojis::Emoji) -> bool {
    emoji.unicode_version() <= NEWEST && crate::twemoji::has_picture(emoji.as_str())
}

/// An emoji and its name, such as "Thumbs up", by the reaction it adds: the
/// emoji itself, or a Discord custom emoji's `name:id`, which shows its glyph
/// once it has one, and its name until then.
pub fn cell(key: &str) -> EmojiCell {
    if let Some((name, id)) = key.split_once(':')
        && !name.is_empty()
        && !id.is_empty()
        && id.bytes().all(|byte| byte.is_ascii_digit())
    {
        let name = format!(":{name}:");
        return EmojiCell {
            glyph: custom_emoji::glyph(id)
                .map_or_else(|| name.clone(), String::from)
                .into(),
            name: name.into(),
            key: key.into(),
            picture: custom_emoji::image(id).unwrap_or_default(),
            usable: true,
        };
    }
    let name = emojis::get(key).map_or(key, emojis::Emoji::name);
    let mut chars = name.chars();
    EmojiCell {
        glyph: key.into(),
        name: chars
            .next()
            .map(|first| first.to_uppercase().chain(chars).collect::<String>())
            .unwrap_or_default()
            .into(),
        key: key.into(),
        picture: slint::Image::default(),
        usable: true,
    }
}

/// A reaction's emoji as Discord's client names it: `:name:` for a custom
/// one, whose key is already that, and the first shortcode of a Unicode one
/// that's a word, such as `:thumbsup:` rather than `:+1:`; the emoji itself
/// without one.
pub fn shortcode(key: &str) -> String {
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

/// The cells as Twemoji draws them, each with its picture in place of its glyph,
/// at the size the picker's grid draws; a smaller button shows it smaller.
pub fn with_pictures(cells: Vec<EmojiCell>, scale: f32) -> Vec<EmojiCell> {
    cells
        .into_iter()
        .map(
            |cell| match crate::twemoji::picture(&cell.glyph, GLYPH, scale) {
                Some(picture) if cell.picture.size().width == 0 => EmojiCell {
                    usable: true,
                    picture,
                    ..cell
                },
                _ => cell,
            },
        )
        .collect()
}

/// Gives the picker's custom emoji the pictures that arrived since its rows
/// were built, changing only those cells.
#[cfg(test)]
pub fn fill_pictures(rows: &VecModel<EmojiRow>, sections: &VecModel<EmojiSection>) {
    fill_pictures_in(rows, 0, usize::MAX);
    fill_section_pictures(sections);
}

/// [`fill_pictures`] of the rows from `first` to `last` alone.
pub fn fill_pictures_in(rows: &VecModel<EmojiRow>, first: usize, last: usize) {
    let count = rows.row_count();
    if count == 0 || first >= count {
        return;
    }
    for index in first..=last.min(count - 1) {
        let Some(row) = rows.row_data(index) else {
            continue;
        };
        for (cell, shown) in row.cells.iter().enumerate() {
            if let Some(filled) = with_picture(shown) {
                row.cells.set_row_data(cell, filled);
            }
        }
    }
}

/// The sidebar's custom emoji icons that have their pictures now.
pub fn fill_section_pictures(sections: &VecModel<EmojiSection>) {
    for (index, section) in sections.iter().enumerate() {
        if let Some(icon) = with_picture(section.icon.clone()) {
            sections.set_row_data(index, EmojiSection { icon, ..section });
        }
    }
}

/// A custom emoji shown without its picture, with the picture once it has arrived.
fn with_picture(shown: EmojiCell) -> Option<EmojiCell> {
    if shown.picture.size().width > 0 || !shown.key.contains(':') {
        return None;
    }
    let picture = cell(&shown.key).picture;
    (picture.size().width > 0).then_some(EmojiCell { picture, ..shown })
}

/// The picker's key for an emoji as Discord's settings name it, and whether it
/// may be used here: a custom emoji's id, found in `custom`, or a Unicode
/// emoji's Discord name, such as `slight_smile`, `thumbsup_tone2`,
/// `person_tone1_bald` or `flag_fr`, from Discord's own names where they differ
/// from GitHub's shortcodes. `None` for a custom emoji not there, or a name
/// Yapfast doesn't know. A number can be a Unicode emoji's name too: `100`.
pub fn discord_key(
    name: &str,
    custom: &HashMap<&str, &yapfast_core::chat::CustomEmoji>,
) -> Option<(String, bool)> {
    if let Some(emoji) = custom.get(name) {
        return Some((custom_key(emoji), emoji.usable));
    }
    // A skin tone comes last in most names, but in some within them.
    let (base, tone) = match name.split_once("_tone") {
        Some((start, rest)) if !rest.contains("_tone") => {
            let (tone, end) = rest.split_at_checked(1)?;
            (format!("{start}{end}"), tone)
        }
        _ => (name.to_owned(), ""),
    };
    let names = crate::discord_emoji_names::NAMES;
    let emoji = names
        .binary_search_by_key(&base.as_str(), |(name, _)| name)
        .ok()
        .and_then(|at| emojis::get(names[at].1))
        .or_else(|| country_flag(&base))
        .or_else(|| emojis::get_by_shortcode(&base))?;
    let emoji = match tone {
        "" => emoji,
        "1" => emoji.with_skin_tone(emojis::SkinTone::Light)?,
        "2" => emoji.with_skin_tone(emojis::SkinTone::MediumLight)?,
        "3" => emoji.with_skin_tone(emojis::SkinTone::Medium)?,
        "4" => emoji.with_skin_tone(emojis::SkinTone::MediumDark)?,
        "5" => emoji.with_skin_tone(emojis::SkinTone::Dark)?,
        _ => return None,
    };
    (emoji.unicode_version() <= NEWEST).then(|| (emoji.as_str().to_owned(), true))
}

/// The name Discord's settings keep the emoji of picker key `key` by, as
/// [`discord_key`] reads it back: a custom emoji's id, or a Unicode emoji's
/// Discord name without its skin tone, as Discord's client stars the emoji
/// itself.
pub fn discord_name(key: &str) -> Option<String> {
    if let Some((_, id)) = key.rsplit_once(':') {
        return Some(id.to_owned());
    }
    let emoji = emojis::get(key)?;
    let base = emoji
        .with_skin_tone(emojis::SkinTone::Default)
        .unwrap_or(emoji);
    let flag = || {
        let letters: Option<String> = base
            .as_str()
            .chars()
            .map(|point| {
                let letter = u32::from(point).checked_sub(0x1F1E6)?;
                (letter < 26).then(|| char::from(b'a' + letter as u8))
            })
            .collect();
        letters
            .filter(|letters| letters.len() == 2)
            .map(|letters| format!("flag_{letters}"))
    };
    // The emoji's first name, which Discord's client takes a favourite out
    // by, then any other it reads back as this emoji: some of GitHub's name
    // another one on Discord.
    let none = HashMap::new();
    let plain = |text: &str| text.replace('\u{FE0F}', "");
    let first = crate::discord_emoji_names::FIRST
        .iter()
        .find(|(glyph, _)| plain(glyph) == plain(base.as_str()))
        .map(|(_, name)| (*name).to_owned());
    first
        .into_iter()
        .chain(
            base.shortcodes()
                .filter(|code| code.starts_with(|c: char| c.is_ascii_alphanumeric()))
                .map(str::to_owned),
        )
        .chain(
            crate::discord_emoji_names::NAMES
                .iter()
                .filter(|(_, glyph)| {
                    emojis::get(glyph).map(emojis::Emoji::as_str) == Some(base.as_str())
                })
                .map(|(name, _)| (*name).to_owned()),
        )
        .chain(flag())
        .chain(base.shortcodes().map(str::to_owned))
        .find(|name| discord_key(name, &none).is_some_and(|(found, _)| found == base.as_str()))
}

/// The flag `flag_xx` names, from its country's two letters: GitHub's shortcode
/// `cl` is the CL button, not Chile's flag.
fn country_flag(name: &str) -> Option<&'static emojis::Emoji> {
    let code = name.strip_prefix("flag_").filter(|code| code.len() == 2)?;
    let flag: String = code
        .bytes()
        .map(|letter| char::from_u32(0x1F1E6 + u32::from(letter.checked_sub(b'a')?)))
        .collect::<Option<_>>()?;
    emojis::get(&flag)
}

/// The picker's keys of the emoji `names`, those Discord's settings name, and
/// whether each may be used here, leaving out those there's no showing.
pub fn discord_keys<'a>(
    names: impl IntoIterator<Item = &'a String>,
    custom: &HashMap<&str, &yapfast_core::chat::CustomEmoji>,
) -> Vec<(String, bool)> {
    names
        .into_iter()
        .filter_map(|name| discord_key(name, custom))
        .collect()
}

/// A custom emoji's reaction key, Discord's `name:id`.
pub fn custom_key(emoji: &yapfast_core::chat::CustomEmoji) -> String {
    format!("{}:{}", emoji.text.trim_matches(':'), emoji.id)
}

/// What an emoji puts in the composer: the emoji itself, or a custom emoji's
/// `:name:`, which goes out in Discord's form; see `yapfast_core::chat::wire_emoji`.
pub fn composer_text(key: &str) -> String {
    match key.split_once(':') {
        Some((name, _)) => format!(":{name}:"),
        None => key.to_owned(),
    }
}

/// Emoji names shorter than this aren't looked up while typing, as on Discord.
const MIN_QUERY: usize = 2;
const MAX_QUERY: usize = 32;

/// The `:name` word ending at `cursor`, a byte offset into `text`: where its
/// `:` is, and the name so far, of at least two letters.
pub fn query(text: &str, cursor: usize) -> Option<(usize, &str)> {
    let before = text.get(..cursor)?;
    let colon = before.rfind(':')?;
    let query = &before[colon + 1..];
    let starts_word = before[..colon]
        .chars()
        .next_back()
        .is_none_or(|previous| previous.is_whitespace() || "([{\"'".contains(previous));
    let name = |c: char| c.is_ascii_alphanumeric() || "_+-".contains(c);
    (starts_word && (MIN_QUERY..=MAX_QUERY).contains(&query.len()) && query.chars().all(name))
        .then_some((colon, query))
}

/// Suggestions for a `:name` being typed, at most `limit`: emoji whose name
/// starts with `query`, then those whose name contains it, a server's custom
/// emoji first. Each is named by its `:name:`.
pub fn suggestions(query: &str, custom: &[EmojiSet], limit: usize) -> Vec<EmojiCell> {
    let query = query.to_lowercase();
    // The emoji's rank and the name it matched by.
    let rank = |name: &str| {
        let lower = name.to_lowercase();
        if lower == query {
            Some(0)
        } else if lower.starts_with(&query) {
            Some(1)
        } else if lower.contains(&query) {
            Some(2)
        } else {
            None
        }
    };
    let mut found: Vec<(u8, bool, String, String)> = Vec::new();
    for emoji in custom.iter().flat_map(|set| &set.emoji) {
        let name = emoji.text.trim_matches(':');
        if let Some(rank) = rank(name) {
            found.push((rank, false, name.to_owned(), custom_key(emoji)));
        }
    }
    for emoji in emojis::iter().filter(drawn) {
        if let Some((rank, code)) = emoji
            .shortcodes()
            .filter_map(|code| Some((rank(code)?, code)))
            .min_by_key(|(rank, _)| *rank)
        {
            found.push((rank, true, code.to_owned(), emoji.as_str().to_owned()));
        }
    }
    // Stable, so each rank keeps custom emoji, then Unicode's own order.
    found.sort_by_key(|(rank, unicode, ..)| (*rank, *unicode));
    found
        .into_iter()
        .take(limit)
        .map(|(_, _, name, key)| EmojiCell {
            usable: true,

            name: format!(":{name}:").into(),
            ..cell(&key)
        })
        .collect()
}

/// The rows the grid shows with `top` at its top edge and `height` deep, both
/// in the grid's own space: the first whose foot is past `top`, and the last
/// whose head is above `top + height`. The rows go down the grid in order, so
/// each is found by halving rather than by walking every row above it, as a
/// scroll asks at every step.
pub fn visible_rows(rows: &VecModel<EmojiRow>, top: f32, height: f32) -> (usize, usize) {
    let count = rows.row_count();
    if count == 0 {
        return (0, 0);
    }
    // The first row for which `past` holds, with every row after it holding too.
    let first_where = |past: &dyn Fn(&EmojiRow) -> bool| {
        let (mut low, mut high) = (0, count);
        while low < high {
            let middle = (low + high) / 2;
            if rows.row_data(middle).is_some_and(|row| past(&row)) {
                high = middle;
            } else {
                low = middle + 1;
            }
        }
        low
    };
    let first = first_where(&|row| row.top + row.height >= top).min(count - 1);
    let last = first_where(&|row| row.top > top + height)
        .saturating_sub(1)
        .clamp(first, count - 1);
    (first, last)
}

/// The picker's rows: with no query, frequently used emoji, the custom emoji of
/// each server in `custom`, and then every category under its heading; otherwise
/// the emoji whose name or shortcode contains the query. With no query, also
/// the sections the sidebar jumps to, each shown by `server_icon` for a server
/// that has one, and its first emoji otherwise.
pub fn rows(
    query: &str,
    favorites: &[(String, bool)],
    frequent: &[(String, bool)],
    custom: &[EmojiSet],
    columns: usize,
    server_icon: impl Fn(&EmojiSet) -> Option<(String, slint::Image)>,
) -> (Vec<EmojiRow>, Vec<EmojiSection>) {
    let query = query.trim().trim_matches(':').to_lowercase();
    let available = || emojis::iter().filter(drawn);
    // Each custom emoji with whether this account may use it where it is
    // looking: the picker shows the others greyed with a lock.
    let custom_keys: Vec<Vec<(String, bool)>> = custom
        .iter()
        .map(|set| {
            set.emoji
                .iter()
                .filter(|emoji| query.is_empty() || emoji.text.to_lowercase().contains(&query))
                .map(|emoji| (custom_key(emoji), emoji.usable))
                .collect()
        })
        .collect();
    let mut rows = Vec::new();
    let mut sections = Vec::new();
    // Where the next row starts, down the grid.
    let mut top = 0.0;
    // A section under a heading of its icon's name, or the matches, under none.
    let mut section = |icon: Option<(EmojiCell, String)>, glyphs: Vec<(String, bool)>| {
        if glyphs.is_empty() {
            return;
        }
        let start = top;
        let heading = rows.len() as i32;
        if let Some((icon, _)) = &icon {
            rows.push(EmojiRow {
                heading: icon.name.to_uppercase().into(),
                cells: ModelRc::default(),
                // A server's icon beside its name, once its picture has come;
                // the picker fills it then (see `fill_row_icons`).
                icon: icon.picture.clone(),
                glyph: section_glyph(icon.name.as_str()),
                top,
                height: HEADING_HEIGHT,
            });
            top += HEADING_HEIGHT;
        }
        for chunk in glyphs.chunks(columns) {
            rows.push(EmojiRow {
                heading: SharedString::default(),
                icon: Default::default(),
                glyph: SectionGlyph::None,
                top,
                height: ROW_HEIGHT,
                cells: ModelRc::new(VecModel::from(
                    chunk
                        .iter()
                        .map(|(glyph, usable)| EmojiCell {
                            usable: *usable,
                            ..cell(glyph)
                        })
                        .collect::<Vec<_>>(),
                )),
            });
            top += ROW_HEIGHT;
        }
        if let Some((icon, icon_url)) = icon {
            sections.push(EmojiSection {
                heading,
                top: start,
                end: top,
                icon_url: icon_url.into(),
                icon,
            });
        }
    };
    let titled = |glyph: &str, name: &str| EmojiCell {
        usable: true,

        name: name.into(),
        ..cell(glyph)
    };
    if !query.is_empty() {
        section(
            None,
            custom_keys
                .iter()
                .flatten()
                .cloned()
                .chain(
                    available()
                        .filter(|emoji| {
                            emoji.name().contains(&query)
                                || emoji.shortcodes().any(|code| code.contains(&query))
                        })
                        .map(|emoji| (emoji.as_str().to_owned(), true)),
                )
                .collect(),
        );
        return (rows, sections);
    }
    section(
        Some((titled("⭐", FAVORITES), String::new())),
        favorites.to_vec(),
    );
    section(
        Some((titled("🕒", FREQUENTLY_USED), String::new())),
        frequent.to_vec(),
    );
    for (set, keys) in custom.iter().zip(custom_keys) {
        let Some((first_key, _)) = keys.first() else {
            continue;
        };
        let first = cell(first_key);
        // Until a picture comes, the server's icon or its first emoji's, the
        // button is the quiet square a picture on its way is, not the
        // emoji's `:name:` as text.
        let icon = EmojiCell {
            name: set.server.as_str().into(),
            glyph: set.server.as_str().into(),
            picture: server_icon(set)
                .map(|(_, picture)| picture)
                .unwrap_or_else(|| first.picture.clone()),
            usable: true,
            ..first
        };
        let icon_url = set.icon.clone().unwrap_or_default();
        section(Some((icon, icon_url)), keys);
    }
    for group in emojis::Group::iter() {
        let (glyph, name) = match group {
            emojis::Group::SmileysAndEmotion => ("😀", tr("Smileys & emotion")),
            emojis::Group::PeopleAndBody => ("👋", tr("People & body")),
            emojis::Group::AnimalsAndNature => ("🐻", tr("Animals & nature")),
            emojis::Group::FoodAndDrink => ("🍔", tr("Food & drink")),
            emojis::Group::TravelAndPlaces => ("✈️", tr("Travel & places")),
            emojis::Group::Activities => ("⚽", tr("Activities")),
            emojis::Group::Objects => ("💡", tr("Objects")),
            emojis::Group::Symbols => ("🔣", tr("Symbols")),
            emojis::Group::Flags => ("🏁", tr("Flags")),
        };
        section(
            Some((titled(glyph, name), String::new())),
            group
                .emojis()
                .filter(drawn)
                .map(|emoji| (emoji.as_str().to_owned(), true))
                .collect(),
        );
    }
    (rows, sections)
}

/// The fonts every Windows 10 and 11 has for what Segoe UI and Arial, its
/// usual fallback, lack: text, symbols and arrows, the mathematical alphabets,
/// and historic scripts. Not Segoe UI Emoji: emoji are Twemoji's.
const WINDOWS_FALLBACK: [&str; 4] = [
    "Segoe UI",
    "Segoe UI Symbol",
    "Cambria Math",
    "Segoe UI Historic",
];

/// Put the installed ones of `names` first, in their order, in the generic
/// families text falls back through after the UI typeface.
fn prefer_families(collection: &mut fontique::Collection, names: &[&str]) {
    let preferred: Vec<_> = names
        .iter()
        .filter_map(|name| collection.family_by_name(name))
        .map(|family| family.id())
        .collect();
    if preferred.len() < names.len() {
        tracing::warn!(
            ?names,
            found = preferred.len(),
            "some fallback fonts aren't installed"
        );
    }
    for generic in [
        fontique::GenericFamily::SansSerif,
        fontique::GenericFamily::SystemUi,
    ] {
        let rest: Vec<_> = collection
            .generic_families(generic)
            .filter(|family| !preferred.contains(family))
            .collect();
        collection.set_generic_families(generic, preferred.iter().copied().chain(rest));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slint::Model;
    use std::sync::Arc;

    #[test]
    fn pictures_fill_the_picker_in_place() {
        let custom = [EmojiSet {
            id: "90".into(),
            server: "Games".into(),
            icon: None,
            emoji: vec![yapfast_core::chat::CustomEmoji {
                text: ":gg:".into(),
                id: "77".into(),
                animated: false,
                usable: true,
                picture: String::new(),
            }],
        }];
        let (rows, sections) = rows("", &[], &[], &custom, columns(408.), |_| None);
        let model = VecModel::from(rows);
        let sections = VecModel::from(sections);
        let cells = model.row_data(1).unwrap().cells;
        assert_eq!(cells.row_data(0).unwrap().picture.size().width, 0);
        let picture = yapfast_core::chat::Picture {
            width: 2,
            height: 2,
            rgba: yapfast_core::chat::Pixels::new(2, 2, [255, 0, 0, 255].repeat(4)),
            ..Default::default()
        };
        assert!(custom_emoji::add_images([("77", &picture)]));
        assert!(!custom_emoji::add_images([("77", &picture)]), "kept once");
        fill_pictures(&model, &sections);
        // The same row model, with the cell's picture now in it.
        let gg = cells.row_data(0).unwrap();
        assert_eq!(gg.picture.size().width, 2);
        assert_eq!(
            gg.glyph, ":gg:",
            "its glyph is for text, which needs a font"
        );
        // A server without an icon is shown in the sidebar by its first emoji.
        assert_eq!(sections.row_data(0).unwrap().icon.picture.size().width, 2);
    }

    #[test]
    fn the_grid_carries_whether_an_emoji_may_be_used() {
        let custom = [EmojiSet {
            id: "90".into(),
            server: "Games".into(),
            icon: None,
            emoji: vec![
                yapfast_core::chat::CustomEmoji {
                    text: ":mine:".into(),
                    id: "1".into(),
                    animated: false,
                    usable: true,
                    picture: String::new(),
                },
                yapfast_core::chat::CustomEmoji {
                    text: ":theirs:".into(),
                    id: "2".into(),
                    animated: true,
                    usable: false,
                    picture: String::new(),
                },
            ],
        }];
        let (rows, _) = rows("", &[], &[], &custom, columns(498.), |_| None);
        let cells: Vec<EmojiCell> = rows.iter().flat_map(|row| row.cells.iter()).collect();
        // A locked one is shown all the same, marked so the picker greys it and
        // offers Nitro when it is pressed.
        let mine = cells
            .iter()
            .find(|cell| cell.key == "mine:1")
            .expect("mine");
        let theirs = cells
            .iter()
            .find(|cell| cell.key == "theirs:2")
            .expect("theirs");
        assert!(mine.usable, "an emoji of this server may be used");
        assert!(!theirs.usable, "an animated one may not without Nitro");
    }

    #[test]
    fn the_grid_fits_its_columns_to_the_width_it_shows_at() {
        // A button is 56px, as Discord's, and the sidebar beside the grid and its
        // padding take 64px: the emoji popup opens 408px wide, the picker above
        // the composer 498px at least, and its left edge drags it wider.
        assert_eq!(columns(408.), 6);
        assert_eq!(columns(498.), 7);
        assert_eq!(columns(648.), 10);
        // A width too narrow for one still has one, and a wide window takes as
        // many as it holds, with no cap of its own.
        assert_eq!(columns(0.), 1);
        assert_eq!(columns(4000.), 70);
    }

    #[test]
    fn the_grid_finds_the_rows_a_scroll_shows() {
        let (all, _) = rows(
            "",
            &[],
            &[("🔥".into(), true)],
            &[],
            columns(408.),
            |_| None,
        );
        let model = VecModel::from(all.clone());
        let height = 3.0 * ROW_HEIGHT;
        // A scroll into a heading's tighter row reports that heading.
        let heading = all[0].top;
        assert_eq!(visible_rows(&model, heading, height).0, 0);
        // The heading's row ends where the first emoji's begins, so a scroll
        // to that edge still reports it; a pixel into the emoji reports them.
        assert_eq!(visible_rows(&model, all[1].top, height).0, 0);
        let (first, last) = visible_rows(&model, all[1].top + 1.0, height);
        assert_eq!(first, 1);
        assert!(all[last].top <= all[1].top + height);
        assert!(all.last().unwrap().top + all.last().unwrap().height > all[1].top);
        // Past the end it keeps the last row rather than nothing.
        let (first, _) = visible_rows(&model, all.last().unwrap().top + 500., height);
        assert_eq!(first, all.len() - 1);
    }

    #[test]
    fn the_picker_lists_categories_or_matches_and_names_emoji() {
        let (all, sections) = rows(
            "",
            &[("☠️".into(), true)],
            &[("🔥".into(), true)],
            &[],
            columns(408.),
            |_| None,
        );
        assert_eq!(all[0].heading, "FAVORITES");
        assert_eq!(all[2].heading, "FREQUENTLY USED");
        assert_eq!(all[3].cells.row_count(), 1);
        assert_eq!(all[4].heading, "SMILEYS & EMOTION");
        assert!(all.iter().all(|row| row.cells.row_count() <= columns(408.)));
        // The sidebar jumps to each heading; a section ends where the next begins.
        assert_eq!(sections.len(), 2 + emojis::Group::iter().count());
        assert_eq!((sections[0].heading, sections[0].top), (0, 0.0));
        assert_eq!(sections[2].icon.name, "Smileys & emotion");
        for (section, next) in sections.iter().zip(&sections[1..]) {
            assert!(!all[section.heading as usize].heading.is_empty());
            assert_eq!(section.end, next.top);
        }
        let last = all.last().unwrap();
        assert_eq!(sections.last().unwrap().end, last.top + last.height);
        let (found, sections) = rows(":pizza", &[], &[], &[], columns(408.), |_| None);
        assert_eq!(found.len(), 1);
        assert!(sections.is_empty(), "matches have no sections");
        assert!(found[0].heading.is_empty());
        assert_eq!(found[0].cells.row_data(0).unwrap().glyph, "🍕");
        assert!(
            rows("no such emoji", &[], &[], &[], columns(408.), |_| None)
                .0
                .is_empty()
        );
        assert_eq!(cell("👍").name, "Thumbs up");
        // The composer gets the emoji, or a custom emoji's name.
        assert_eq!(composer_text("👍"), "👍");
        assert_eq!(composer_text("pog:5"), ":pog:");

        // A server's custom emoji follow those used often, and match by name.
        let custom = [EmojiSet {
            id: "90".into(),
            server: "Games".into(),
            icon: None,
            emoji: vec![yapfast_core::chat::CustomEmoji {
                text: ":pog:".into(),
                id: "5".into(),
                animated: false,
                usable: true,
                picture: String::new(),
            }],
        }];
        let icon = slint::Image::from_rgba8(slint::SharedPixelBuffer::new(3, 3));
        let (all, sections) = rows("", &[], &[], &custom, columns(408.), |_| {
            Some((
                "https://cdn.discordapp.com/icons/1/x.png".into(),
                icon.clone(),
            ))
        });
        assert_eq!(all[0].heading, "GAMES");
        let pog = all[1].cells.row_data(0).unwrap();
        assert_eq!((pog.glyph.as_str(), pog.key.as_str()), (":pog:", "pog:5"));
        assert_eq!(sections[0].icon.name, "Games");
        assert_eq!(
            sections[0].icon.picture.size().width,
            3,
            "the server's icon"
        );
        assert_eq!(
            rows("po", &[], &[], &custom, columns(408.), |_| None).0[0]
                .cells
                .row_data(0)
                .unwrap()
                .key,
            "pog:5"
        );
    }

    #[test]
    fn finds_emoji_by_discord_names() {
        let names = crate::discord_emoji_names::NAMES;
        assert!(names.windows(2).all(|pair| pair[0].0 < pair[1].0), "sorted");
        let talk = yapfast_core::chat::CustomEmoji {
            text: ":talk:".into(),
            id: "5".into(),
            animated: false,
            usable: false,
            picture: String::new(),
        };
        let custom = HashMap::from([("5", &talk)]);
        let key = |name| discord_key(name, &custom).map(|(key, _)| key);
        // Discord's own names, then GitHub's that it shares.
        assert_eq!(key("skull_crossbones").as_deref(), Some("☠️"));
        assert_eq!(key("slight_smile").as_deref(), Some("🙂"));
        assert_eq!(key("joy").as_deref(), Some("😂"));
        assert_eq!(key("thumbsup_tone4").as_deref(), Some("👍🏾"));
        assert_eq!(key("person_tone1_bald").as_deref(), Some("🧑🏻‍🦲"));
        assert_eq!(key("6"), None, "a custom emoji not usable here");
        assert_eq!(key("no_such_emoji"), None);
        // Whether it may be used comes with a custom emoji.
        assert_eq!(discord_key("5", &custom), Some(("talk:5".into(), false)));
        // A number that's no custom emoji can be a Unicode emoji's name.
        assert_eq!(key("100").as_deref(), Some("💯"));
        assert_eq!(key("1234").as_deref(), Some("🔢"));
        // Flags are their country's, though GitHub's shortcode is something else.
        assert_eq!(key("flag_fr").as_deref(), Some("🇫🇷"));
        assert_eq!(key("flag_cl").as_deref(), Some("🇨🇱"));
        assert_eq!(key("flag_tv").as_deref(), Some("🇹🇻"));
        // Where JoyPixels and Discord mean different emoji by a name.
        assert_eq!(key("beach_umbrella").as_deref(), Some("⛱️"));
        assert_eq!(key("city_sunset").as_deref(), Some("🌇"));
    }

    #[test]
    fn names_an_emoji_as_discord_keeps_its_favourites() {
        // As Discord's settings name them, read back to the same emoji.
        for (key, name) in [
            ("☠️", "skull_crossbones"),
            ("🙂", "slight_smile"),
            ("😂", "joy"),
            ("🇫🇷", "flag_fr"),
            ("🇨🇱", "flag_cl"),
            ("💯", "100"),
            ("⛱️", "beach_umbrella"),
            ("talk:5", "5"),
        ] {
            assert_eq!(discord_name(key).as_deref(), Some(name), "{key}");
        }
        // Discord's first name, not GitHub's `+1` or the `thumbs_up` it reads
        // too, and a hyphen as Discord writes it.
        assert_eq!(discord_name("👍").as_deref(), Some("thumbsup"));
        assert_eq!(discord_name("📧").as_deref(), Some("e_mail"));
        // A skin tone is the emoji itself, which Discord's client stars.
        assert_eq!(discord_name("👍🏾").as_deref(), Some("thumbsup"));
        assert_eq!(discord_name("not an emoji"), None);
    }

    #[test]
    fn suggests_emoji_for_the_name_being_typed() {
        assert_eq!(query("look :thu", 9), Some((5, "thu")));
        assert_eq!(query(":+1", 3), Some((0, "+1")));
        assert_eq!(query("(:pizza", 7), Some((1, "pizza")));
        assert_eq!(query("look :t", 7), None, "two letters at least");
        assert_eq!(query("at 10:30", 8), None, "not after a letter or digit");
        assert_eq!(query(":thumbs up", 10), None, "a space ends it");
        assert_eq!(query(":pog:", 5), None, "a closed name");

        let custom = [EmojiSet {
            id: "90".into(),
            server: "Games".into(),
            icon: None,
            emoji: vec![yapfast_core::chat::CustomEmoji {
                text: ":thunk:".into(),
                id: "5".into(),
                animated: false,
                usable: true,
                picture: String::new(),
            }],
        }];
        let found = suggestions("thu", &custom, 10);
        assert_eq!(found[0].key, "thunk:5", "a server's emoji first");
        assert_eq!(found[0].name, ":thunk:");
        assert!(
            found[1..]
                .iter()
                .any(|cell| cell.glyph == "👍" && cell.name == ":thumbsup:")
        );
        assert!(found.len() <= 10);
        let exact = suggestions("pizza", &[], 10);
        assert_eq!(
            (exact[0].glyph.as_str(), exact[0].name.as_str()),
            ("🍕", ":pizza:")
        );
        assert!(suggestions("zzzzzz", &[], 10).is_empty());
    }

    #[test]
    fn installed_fallback_fonts_lead_both_generic_families() {
        let mut collection = fontique::Collection::new(fontique::CollectionOptions {
            shared: false,
            system_fonts: false,
        });
        let mut register = |name: &str| {
            let font = include_bytes!("../ui/fonts/AtkinsonHyperlegibleNext-Regular.ttf");
            let registered = collection.register_fonts(
                fontique::Blob::new(Arc::new(font.to_vec())),
                Some(fontique::FontInfoOverride {
                    family_name: Some(name),
                    ..Default::default()
                }),
            );
            registered.first().expect("a family").0
        };
        let (symbols, emoji, arial) = (register("Symbols"), register("Emoji"), register("Arial"));
        let sans = fontique::GenericFamily::SansSerif;
        collection.set_generic_families(sans, [arial, symbols].into_iter());

        prefer_families(&mut collection, &["Emoji", "Not installed", "Symbols"]);

        let order = |collection: &mut fontique::Collection, generic| {
            collection.generic_families(generic).collect::<Vec<_>>()
        };
        assert_eq!(order(&mut collection, sans), [emoji, symbols, arial]);
        assert_eq!(
            order(&mut collection, fontique::GenericFamily::SystemUi),
            [emoji, symbols]
        );
    }
}
