//! The editor's words, switches and lists from the core's editor
//! (`ProfileEditorPrompt`, `show_profile_editor`, `src/ui/refresh.rs:3036`):
//! the texts as written, what this account may change, how saving stands,
//! the server tags to wear, and the preview's tabs.

use yapfast_core::application::{
    AppState, BIO_LIMIT, NAME_LIMIT, PRONOUNS_LIMIT, ProfileEditor, ProfileText,
};
use yapfast_core::chat::ProfilePicture;
use yapfast_lang::{tr, trn};

use anomi_vm::people::editor::{Choice, Editor, May, Picture};
use anomi_vm::people::editor_preview::EditorPreview;
use anomi_vm::people::profile::Card;

use super::view::server_id;
use crate::pictures;
use crate::project::people::full;

/// Sets `to` to `from` when they differ, without copying otherwise.
fn set(to: &mut String, from: &str) {
    if to != from {
        from.clone_into(to);
    }
}

/// Everything but the card, which `card` is.
pub fn fill(state: &AppState, editor: &ProfileEditor, card: Card, vm: &mut Editor) {
    let main = editor.main();
    let fallback = editor.profile.as_ref().map(|profile| &profile.fallback);
    let (title, detail, bio_placeholder) = if main {
        (
            tr!("Edit profile"),
            tr!("Your profile").to_owned(),
            tr!("Tell everyone a bit about yourself"),
        )
    } else {
        (
            tr!("Edit server profile"),
            tr!("Your profile in {}", editor.server_name),
            tr!("Tell this server a bit about yourself"),
        )
    };
    set(&mut vm.title, title);
    set(&mut vm.detail, &detail);
    set(&mut vm.bio_placeholder, bio_placeholder);
    vm.main = main;
    set(&mut vm.name, &editor.name);
    set(&mut vm.pronouns, &editor.pronouns);
    set(&mut vm.bio, &editor.bio);
    set(&mut vm.colour, &editor.colour);
    set(
        &mut vm.name_placeholder,
        fallback.map_or("", |own| own.handle.as_str()),
    );
    set(
        &mut vm.pronouns_fallback,
        fallback.map_or("", |own| own.pronouns.as_str()),
    );
    vm.loading = editor.loading();
    vm.saving = editor.saving;
    set(&mut vm.error, editor.error.as_deref().unwrap_or_default());
    vm.may = May {
        name: editor.may_write(ProfileText::Name),
        avatar: editor.may_picture(ProfilePicture::Avatar),
        banner: editor.may_picture(ProfilePicture::Banner),
        bio: editor.may_write(ProfileText::Bio),
        look: editor.may_write(ProfileText::ThemeTop),
        colour: editor.may_write(ProfileText::Colour),
    };
    vm.offer = !main && !editor.nitro();
    vm.upgrade = editor.upgrade();
    vm.banner_shown = !main || editor.nitro();
    vm.colour_wrong = editor.accent().is_err();
    vm.avatar_reset = editor.resettable(ProfilePicture::Avatar);
    vm.banner_reset = editor.resettable(ProfilePicture::Banner);
    vm.reading = editor.reading.as_ref().map(|(kind, _)| match kind {
        ProfilePicture::Avatar => Picture::Avatar,
        ProfilePicture::Banner => Picture::Banner,
    });
    let left = BIO_LIMIT as i64 - editor.bio.chars().count() as i64;
    vm.bio_left = i32::try_from(left).unwrap_or(i32::MIN);
    set(&mut vm.too_long, &too_long(editor));
    vm.changed = editor.change().is_some();
    vm.decoration_set = fallback.is_some_and(|own| own.decoration.is_some());
    vm.effect_set = fallback.is_some_and(|own| own.effect.is_some());
    vm.frame_set = fallback.is_some_and(|own| !own.frame.is_empty());
    vm.tags = tags(state);
    set(
        &mut vm.tag_chosen,
        editor
            .chosen_tag
            .as_ref()
            .and_then(|chosen| chosen.as_deref())
            .unwrap_or_default(),
    );
    preview(state, editor, &card, &mut vm.preview);
    vm.card = card;
}

/// What's longer than Discord takes, said as its field's limit.
fn too_long(editor: &ProfileEditor) -> String {
    if editor.name.chars().count() > NAME_LIMIT {
        if editor.main() {
            trn!(
                "A display name can be up to {n} character.",
                "A display name can be up to {n} characters.",
                NAME_LIMIT
            )
        } else {
            trn!(
                "A nickname can be up to {n} character.",
                "A nickname can be up to {n} characters.",
                NAME_LIMIT
            )
        }
    } else if editor.pronouns.chars().count() > PRONOUNS_LIMIT {
        trn!(
            "Pronouns can be up to {n} character.",
            "Pronouns can be up to {n} characters.",
            PRONOUNS_LIMIT
        )
    } else {
        String::new()
    }
}

/// The servers whose tags this account may wear.
fn tags(state: &AppState) -> Vec<Choice> {
    state
        .servers
        .iter()
        .filter_map(|server| {
            let tag = server.tag.as_ref()?;
            Some(Choice {
                id: server_id(&server.id).to_owned(),
                name: server.name.clone(),
                icon: pictures::find(state, server.icon.as_deref()),
                tag: tag.text.to_string(),
            })
        })
        .collect()
}

/// The tabs beside the card: its theme and what it's doing, the board's
/// widgets and the wishlist, with the words for each while it has none.
fn preview(state: &AppState, editor: &ProfileEditor, card: &Card, preview: &mut EditorPreview) {
    let fallback = editor.profile.as_ref().map(|profile| &profile.fallback);
    preview.theme = card.theme;
    preview.activities.clone_from(&card.activities);
    preview.now = card.now;
    preview.widgets = full::widgets(state, fallback.map_or(&[][..], |own| &own.widgets));
    let note = if editor.loading() {
        tr!("Loading the board...")
    } else if editor.profile.is_none() {
        tr!("Couldn't load the board.")
    } else if fallback.is_some_and(|own| own.board_asked && !own.board_named) {
        tr!("Loading the board...")
    } else {
        ""
    };
    set(&mut preview.board_note, note);
    let place = fallback.and_then(|own| own.wishlist_place.as_ref());
    let (wishes, note) = full::wishlist(state, place, &editor.wishlist);
    preview.wishlist = wishes;
    set(&mut preview.wishlist_note, note);
}
