//! A forum, in place of a channel's messages: a search of its posts'
//! titles, then its posts as cards (the active ones first, older ones a page
//! at a time as the list scrolls, or what a search found), or, while a
//! post is written, [`new_post`]'s form. Task C26's
//! (docs/iced/tasks/chat.md). The Slint side: `ForumPanel` (`ui/forum.slint:473`);
//! the glue `show_forum` (`src/ui/refresh.rs:1995`) and `on_open_post`,
//! `on_search_posts`, `on_more_posts` (`src/ui/callbacks.rs:1785`, `:1795`).

mod card;
mod list;
pub mod new_post;

use anomi_theme::prelude::*;
use anomi_vm::chat::{self, forum::Forum};

pub use list::SEARCH;

pub fn view(forum: &Forum, motion: Motion) -> impl View<chat::Intent> + '_ {
    match &forum.composing {
        Some(post) => new_post::view(post, motion).boxed(),
        None => list::view(forum, motion).map(chat::Intent::from).boxed(),
    }
}

/// Its shots for `anomi-shots`, each the whole window on the first server
/// with the forum "ideas" open (see `crate::shots`), named after the golden
/// screenshots: `forum/list`, `small/forum`, `light/forum`.
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    use crate::shots::{GOLDEN, SMALL, Shot};
    use anomi_vm::fixtures::chat::forum as sample;

    let shot = |area, name, size, theme, vm| Shot {
        area,
        name,
        size,
        theme,
        vm,
    };
    vec![
        shot("forum", "list", GOLDEN, "bonfire", sample::open),
        shot("small", "forum", SMALL, "bonfire", sample::open),
        shot("light", "forum", GOLDEN, "catppuccin-latte", sample::open),
        shot("forum", "loading_more", GOLDEN, "bonfire", || {
            sample::window(sample::loading_more())
        }),
        shot("forum", "found", GOLDEN, "bonfire", || {
            sample::window(sample::found())
        }),
        shot("forum", "nothing_found", GOLDEN, "bonfire", || {
            sample::window(sample::nothing_found())
        }),
        shot("forum", "empty", GOLDEN, "bonfire", || {
            sample::window(sample::empty())
        }),
        shot("forum", "loading", GOLDEN, "bonfire", || {
            sample::window(sample::loading())
        }),
        shot("forum", "read_only", GOLDEN, "bonfire", || {
            sample::window(sample::read_only())
        }),
        shot("forum", "plain", GOLDEN, "bonfire", || {
            sample::window(sample::plain())
        }),
    ]
}
