//! A new forum post: the form that takes the forum's place while a post is
//! written, as Discord's at the top of a forum: a title, up to five of the
//! forum's tags, its post guidelines, and a first message with files.
//! Enter in the message posts; closing after writing asks first. Task C27's
//! (docs/iced/tasks/chat.md). The Slint side: `NewPostForm`
//! (`ui/forum.slint:212`); the glue is `show_new_post`
//! (`src/ui/refresh.rs:2065`) and `src/ui/callbacks.rs:1723`-`:1751`.
//!
//! The menu of tags and the question on leaving are layers of the form
//! (`anomi_widgets::layer`), over the form alone as the Slint ones are, not
//! of the window: the shell's layers are for what opens over the window.

mod ask;
mod bar;
mod emoji;
mod fields;
mod guidelines;
mod tags;

use iced::widget::column;
use iced::{Fill, Task};

use anomi_theme::prelude::*;
use anomi_vm::chat::{
    self,
    new_post::{Intent, NewPost},
};
use anomi_widgets::field::{self, Id};
use anomi_widgets::layer::{Place, layer, layers};
use anomi_widgets::list::scroll_view;

/// The ids of the two fields, for a focus task.
pub const TITLE: &str = "new-post-title";
pub const BODY: &str = "new-post-message";

/// The room around the fields and between them (`padding: 16px; spacing:
/// 16px`).
const PAD: f32 = 16.0;

/// Gives the title the keys (`focus-title`), for the app to run as the form
/// opens.
pub fn focus_title<T: Send + 'static>() -> Task<T> {
    field::focus(Id::new(TITLE))
}

/// Gives the message the keys, as Enter in the title does.
pub fn focus_body<T: Send + 'static>() -> Task<T> {
    field::focus(Id::new(BODY))
}

pub fn view(post: &NewPost, motion: Motion) -> impl View<chat::Intent> + '_ {
    let mut parts = column![fields::title(post, motion)].spacing(PAD);
    if let Some(section) = tags::section(post, motion) {
        parts = parts.push(section);
    }
    if let Some(card) = guidelines::view(post, motion) {
        parts = parts.push(card);
    }
    parts = parts.push(fields::message(post, motion));
    if !post.error.is_empty() {
        parts = parts.push(fields::error(&post.error));
    }
    parts = parts.push(fields::publish(post, motion));
    let base = column![
        bar::view(motion),
        scroll_view(parts.padding(PAD).width(Fill), motion).height(Fill),
    ];
    let tags_menu = layer("tags", tags::menu(post, motion))
        .open(post.tags_open && !post.choices.is_empty())
        .place(Place::At(tags::anchor(post)))
        .on_close(Intent::Tags(false));
    let leaving = layer("leaving", ask::view(motion))
        .open(post.leaving)
        .dialog()
        .on_close(Intent::Stay);
    let form: Element<'_, Intent> = layers(base, motion).push(tags_menu).push(leaving).boxed();
    form.map(chat::Intent::from)
}

/// Its shots for `anomi-shots`, each the whole window from a fixture (see
/// `crate::shots`), named after the golden screenshots: `forum/new_post`
/// and `forum/new_post_tags`, and the form's other states.
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    use crate::shots::{GOLDEN, Shot};
    use anomi_vm::fixtures::chat::new_post as sample;

    let shot = |name, vm: fn() -> anomi_vm::Vm| Shot {
        area: "forum",
        name,
        size: GOLDEN,
        theme: "bonfire",
        vm,
    };
    vec![
        shot("new_post", || sample::window(sample::blank())),
        shot("new_post_tags", || sample::window(sample::tags_menu())),
        shot("new_post_written", || sample::window(sample::written())),
        shot("new_post_all_tags", || sample::window(sample::all_tags())),
        shot("new_post_required", || sample::window(sample::required())),
        shot("new_post_plain", || sample::window(sample::plain())),
        shot("new_post_posting", || sample::window(sample::posting())),
        shot("new_post_failed", || sample::window(sample::failed())),
        shot("new_post_leaving", || sample::window(sample::leaving())),
    ]
}
