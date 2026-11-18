//! The sticker picker from the core's state (`show_stickers`,
//! `src/ui/stickers.rs:84`): the collections the core lists for the search
//! (Favorites, Frequently Used, each server's own, Discord's packs), each
//! shown in the rail by its server's icon, by a clock, or by its first
//! sticker. The pictures are the core's, which holds those of the tiles near
//! the view alone, as it's told them (`StickersShown`): one that comes
//! fills its tile in place, and nothing is listed again.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};

use yapfast_core::application::{AppState, Application, StickerPicture};

use anomi_vm::chat::sticker_picker::{Section, SectionIcon, Sticker, StickerPicker};
use anomi_vm::kind::Expected;

use crate::pictures;

/// What the sticker picker's sections were made from.
#[derive(Default)]
pub(crate) struct Shown {
    key: Option<u64>,
    /// Each sticker's picture address, by its id.
    urls: HashMap<Box<str>, Box<str>>,
    /// Each section's icon's address, by its place: a server's icon, or
    /// its first sticker's picture; none for Frequently Used.
    icons: Vec<Option<Box<str>>>,
    /// The sections were listed again: the pictures near the view are
    /// asked for once the update is over.
    pub(crate) ask: bool,
    /// What was asked last.
    pub(crate) asked: Vec<String>,
}

impl Shown {
    /// The addresses of the pictures the grid keeps: every section's icon,
    /// and the stickers of the rows near the view.
    pub(crate) fn held(&self, picker: &StickerPicker) -> Vec<String> {
        let mut held: Vec<String> = (self.icons.iter().flatten())
            .map(|url| url.to_string())
            .collect();
        let grid = picker.grid();
        for (section, range) in picker.held().filter_map(|row| grid.span(row)) {
            for sticker in &picker.sections[section].stickers[range] {
                if let Some(url) = self.urls.get(sticker.id.as_str())
                    && !held.iter().any(|known| **known == **url)
                {
                    held.push(url.to_string());
                }
            }
        }
        held
    }
}

/// The server a collection is of, by the core's id for it: its icon's
/// address; none for Discord's packs.
fn server_icon(state: &AppState, id: &str) -> Option<String> {
    let server = yapfast_core::discord::server_id(id);
    (state.servers.iter())
        .find(|known| known.id == server)?
        .icon
        .clone()
}

/// The picture at `url`: the picker's own, decoded at the tile's size, else
/// the window's, as a server's icon may be.
fn picture(state: &AppState, url: &str) -> Expected {
    match state.stickers.pictures.get(url) {
        Some(StickerPicture::Shown(picture)) => {
            pictures::shown(picture).map_or(Expected::Waiting, Expected::Ready)
        }
        _ => pictures::find(state, Some(url)).map_or(Expected::Waiting, Expected::Ready),
    }
}

/// Shows the collections the core lists, or only the pictures that came or
/// went when it lists the same.
pub(crate) fn show(core: &Application, picker: &mut StickerPicker, shown: &mut Shown) {
    let state = &core.state;
    let stickers = &state.stickers;
    if !stickers.open {
        return;
    }
    let listed = stickers.listed(core.discord_nitro());
    // The same inputs list the same tiles, so pictures that come rewrite
    // only the tiles near the view (`structure`).
    let mut hasher = DefaultHasher::new();
    (&stickers.query, &stickers.error).hash(&mut hasher);
    for collection in &listed {
        (collection.id, collection.name, collection.stickers.len()).hash(&mut hasher);
        for (sticker, usable) in &collection.stickers {
            (&sticker.id, usable).hash(&mut hasher);
        }
    }
    for sticker in &stickers.favorites {
        sticker.id.hash(&mut hasher);
    }
    let key = hasher.finish();
    if shown.key == Some(key) {
        fill(state, picker, shown);
        return;
    }
    shown.key = Some(key);
    shown.ask = true;
    shown.urls.clear();
    shown.icons.clear();
    picker.query.clone_from(&stickers.query);
    picker.favorites = stickers
        .favorites
        .iter()
        .map(|sticker| sticker.id.clone())
        .collect();
    picker.error = stickers.error.clone();
    picker.sections = (listed.iter())
        .map(|collection| {
            let tiles: Vec<Sticker> = (collection.stickers.iter())
                .map(|(sticker, usable)| {
                    shown
                        .urls
                        .insert(sticker.id.as_str().into(), sticker.picture.as_str().into());
                    Sticker {
                        id: sticker.id.clone(),
                        name: sticker.name.clone(),
                        picture: Expected::Waiting,
                        usable: *usable,
                    }
                })
                .collect();
            let first = collection.stickers.first().map(|(sticker, _)| *sticker);
            let (icon, url) = match (collection.id, server_icon(state, collection.id), first) {
                ("frequent", ..) => (SectionIcon::Frequent, None),
                (id, Some(url), _) => {
                    let picture = picture(state, &url);
                    (
                        SectionIcon::Server {
                            id: id.to_owned(),
                            picture,
                        },
                        Some(url),
                    )
                }
                (_, None, first) => {
                    let url = first
                        .map(|sticker| sticker.picture.clone())
                        .unwrap_or_default();
                    let id = first.map(|sticker| sticker.id.clone()).unwrap_or_default();
                    (
                        SectionIcon::Sticker {
                            id,
                            picture: picture(state, &url),
                        },
                        Some(url),
                    )
                }
            };
            shown.icons.push(url.map(Into::into));
            Section {
                id: collection.id.to_owned(),
                name: collection.name.to_owned(),
                icon,
                stickers: tiles,
            }
        })
        .collect();
    fill(state, picker, shown);
}

/// Gives the rail's icons and the stickers near the view the pictures the
/// core holds now.
fn fill(state: &AppState, picker: &mut StickerPicker, shown: &Shown) {
    for (section, url) in picker.sections.iter_mut().zip(&shown.icons) {
        if let (
            SectionIcon::Server { picture, .. } | SectionIcon::Sticker { picture, .. },
            Some(url),
        ) = (&mut section.icon, url)
        {
            *picture = self::picture(state, url);
        }
    }
    let spans: Vec<_> = {
        let grid = picker.grid();
        picker.held().filter_map(|row| grid.span(row)).collect()
    };
    for (section, range) in spans {
        for sticker in &mut picker.sections[section].stickers[range] {
            if let Some(url) = shown.urls.get(sticker.id.as_str()) {
                sticker.picture = picture(state, url);
            }
        }
    }
}
