//! The pickers' projection on the core's own states: what the emoji picker
//! lists and the pictures it holds near the view, the GIFs' tiles and the
//! stickers' sections, each filled in place as pictures come.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use yapfast_core::application::{Action, Application, StickerPicture};
use yapfast_core::chat::{Event, GifCategory, Picture, Pixels, Sticker, StickerCollection};
use yapfast_lang::tr;

use anomi_vm::chat::emoji_picker::{EmojiPicker, Purpose, SectionIcon};
use anomi_vm::chat::gif_picker::{GifPicker, Kind, Page};
use anomi_vm::chat::sticker_picker::{self, StickerPicker};
use anomi_vm::kind::Expected;

use super::{emoji, gifs, stickers};
use crate::pictures::{self, Requests};

fn picture() -> Arc<Picture> {
    pictures::install();
    Arc::new(Picture {
        width: 1,
        height: 1,
        rgba: Pixels::new(1, 1, vec![1, 2, 3, 255]),
        ..Default::default()
    })
}

fn ready(expected: Option<&Expected>) -> bool {
    matches!(expected, Some(Expected::Ready(_)))
}

/// A reaction's picker on a core whose device reacted with a server's
/// `:pog:` most.
fn reaction(core: &mut Application) -> (EmojiPicker, emoji::Held, Requests) {
    core.remember_reaction_uses(HashMap::from([("pog:5".to_owned(), 9)]));
    let mut picker = EmojiPicker {
        purpose: Purpose::Reaction,
        columns: 6,
        ..EmojiPicker::default()
    };
    let mut held = emoji::Held::default();
    let mut requests = Requests::default();
    held.opened(&core.state);
    emoji::build(core, &mut picker, &mut held, &mut requests);
    (picker, held, requests)
}

#[test]
fn the_emoji_picker_lists_what_is_used_then_unicodes_categories() {
    let mut core = Application::default();
    let (mut picker, ..) = reaction(&mut core);
    // No favourites: what this device reacts with, its own first.
    let frequent = &picker.sections[0];
    assert_eq!(frequent.icon, SectionIcon::Frequent);
    assert_eq!(frequent.name, tr!("Frequently Used"));
    assert_eq!(frequent.emoji[0].key(), "pog:5", "a custom one by name:id");
    assert!(frequent.emoji.len() > 1, "then Discord's usual reactions");
    // Then Unicode's nine categories, as the core names them.
    assert_eq!(picker.sections.len(), 10);
    assert_eq!(picker.sections[1].name, tr!("Smileys & emotion"));
    assert_eq!(picker.sections[9].name, tr!("Flags"));
    // A search shows its matches from their top; colons are no part of it.
    picker.shown = (40, 47);
    emoji::search(&mut picker, ":pizza".into());
    assert!(picker.searching());
    assert!(picker.found.iter().any(|emoji| emoji.key() == "🍕"));
    assert_eq!(picker.shown.0, 0);
    emoji::search(&mut picker, String::new());
    assert!(picker.found.is_empty());
}

#[test]
fn custom_emoji_near_the_view_take_their_pictures_and_far_ones_go() {
    let mut core = Application::default();
    // A custom emoji the window showed before the picker opened.
    let elsewhere = "https://cdn.discordapp.com/emojis/77.png?size=48".to_owned();
    core.state.pictures.insert(elsewhere.clone(), picture());
    let (mut picker, mut held, mut requests) = reaction(&mut core);
    let pog = yapfast_core::discord::emoji_picture("5").expect("Discord's address");
    // Asked for first, as its row is in sight.
    let keep = emoji::seen(&core.state, &mut picker, &mut held, &mut requests);
    let asked = requests.take(Instant::now());
    assert!(matches!(&asked[0], Action::LoadPicturesFirst(urls) if urls.contains(&pog)));
    assert!(keep.contains(&pog) && keep.contains(&elsewhere));
    // It comes: the cell takes it in place.
    core.state.pictures.insert(pog.clone(), picture());
    core.state.picture_changes += 1;
    emoji::fill(&core.state, &mut picker, &mut held);
    assert!(ready(picker.sections[0].emoji[0].picture()));
    // Scrolled far, its row lets go, and the core keeps what it showed
    // before alone.
    picker.shown = (400, 407);
    let keep = emoji::seen(&core.state, &mut picker, &mut held, &mut requests);
    assert_eq!(
        picker.sections[0].emoji[0].picture(),
        Some(&Expected::Waiting)
    );
    assert!(!keep.contains(&pog) && keep.contains(&elsewhere));
}

#[test]
fn the_gifs_home_lists_its_tiles_and_their_pictures_fill_in_place() {
    let mut core = Application::default();
    let gifs = &mut core.state.gifs;
    gifs.open = true;
    gifs.categories = vec![GifCategory {
        name: "happy".into(),
        src: "https://gifs/happy.gif".into(),
    }];
    let mut picker = GifPicker {
        shown: (300.0, 380.0),
        ..GifPicker::default()
    };
    let mut shown = gifs::Shown::default();
    gifs::show(&core.state, &mut picker, &mut shown);
    let kinds: Vec<Kind> = picker.tiles.iter().map(|tile| tile.kind).collect();
    assert_eq!(kinds, [Kind::Favorites, Kind::Trending, Kind::Term]);
    assert_eq!(picker.tiles[0].label, tr!("Favorites"));
    assert_eq!(picker.tiles[2].picture, Expected::Waiting);
    assert_eq!(picker.page, Page::Home);
    assert_eq!(picker.shown, (0.0, 0.0), "a page starts at its top");
    assert!(shown.ask);
    shown.ask = false;
    // The core keeps the tile's picture as it's in sight, and it comes.
    let src = picker.tiles[2].src.clone();
    let asked = Action::GifsShown {
        shown: vec![(src.clone(), 10, 10)],
        near_end: false,
    };
    drop(core.dispatch(asked));
    let came = Event::GifPicture {
        url: src,
        picture: Some(picture()),
    };
    drop(core.apply(came));
    let tiles = picker.tiles.as_ptr();
    gifs::show(&core.state, &mut picker, &mut shown);
    assert!(ready(Some(&picker.tiles[2].picture)));
    assert_eq!(picker.tiles.as_ptr(), tiles, "not listed again");
    assert!(!shown.ask);
}

#[test]
fn the_stickers_list_by_collection_and_keep_the_pictures_near_the_view() {
    let mut core = Application::default();
    let url = "https://media.discordapp.net/stickers/1.png".to_owned();
    let stickers = &mut core.state.stickers;
    stickers.open = true;
    stickers.packs = vec![StickerCollection {
        id: "pack".into(),
        name: "Wumpus".into(),
        stickers: vec![Sticker {
            id: "1".into(),
            name: "Wave".into(),
            format: 1,
            picture: url.clone(),
            usable: true,
        }],
    }];
    let mut picker = StickerPicker {
        columns: sticker_picker::columns(sticker_picker::WIDTH),
        ..StickerPicker::default()
    };
    let mut shown = stickers::Shown::default();
    stickers::show(&core, &mut picker, &mut shown);
    // Discord's first stickers are Frequently Used until one is sent.
    let names: Vec<&str> = picker.sections.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, [tr!("Frequently Used"), "Wumpus"]);
    assert_eq!(
        picker.sections[0].icon,
        sticker_picker::SectionIcon::Frequent
    );
    assert_eq!(shown.held(&picker), std::slice::from_ref(&url));
    assert!(shown.ask);
    // Its picture comes: the tile and the pack's rail button take it.
    let sections = picker.sections.as_ptr();
    let came = StickerPicture::Shown(picture());
    core.state.stickers.pictures.insert(url, came);
    stickers::show(&core, &mut picker, &mut shown);
    assert_eq!(picker.sections.as_ptr(), sections, "not listed again");
    assert!(ready(Some(&picker.sections[1].stickers[0].picture)));
    assert!(ready(picker.sections[1].icon.picture()));
}
