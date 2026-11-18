//! The custom status dialog, `Layer::Status`, from the core's
//! `status_editor` (`src/ui/refresh.rs:3766`): open while the core has
//! one, its draft filled from the status there is as it opens, saving and
//! the core's error as they come. After that the draft (words, emoji, when
//! it clears) is the app's: `intent::status` keeps it.

use yapfast_core::application::AppState;
use yapfast_core::chat::CustomStatus;
use yapfast_core::discord;

use anomi_vm::kind::Expected;
use anomi_vm::people::status::{ClearAfter, Emoji, Status};
use anomi_vm::shell::{Layer, Shell};

use crate::pictures::{self, Requests};

pub fn show(state: &AppState, shell: &mut Shell, status: &mut Status, requests: &mut Requests) {
    let editor = state.status_editor.as_ref();
    let open = shell.is_open(Layer::Status);
    match editor {
        Some(_) if !open => {
            fill(state, status, requests);
            shell.open(Layer::Status);
        }
        None if open => shell.close(Layer::Status),
        _ => {}
    }
    status.has_custom = state.custom_status.is_some();
    status.saving = editor.is_some_and(|editor| editor.saving);
    let error = editor
        .and_then(|editor| editor.error.as_deref())
        .unwrap_or_default();
    if status.error != error {
        error.clone_into(&mut status.error);
    }
    // A custom emoji's picture that came while the dialog shows.
    if let Some(emoji) = status
        .emoji
        .as_mut()
        .filter(|emoji| emoji.picture == Expected::Waiting)
    {
        let address = custom_picture(&emoji.key);
        if let Some(picture) = pictures::find(state, address.as_deref()) {
            emoji.picture = Expected::Ready(picture);
        }
    }
}

/// The draft as the dialog opens: the status there is, kept until the
/// person changes it, and "never" for when it clears.
fn fill(state: &AppState, status: &mut Status, requests: &mut Requests) {
    let current = state.custom_status.as_ref();
    status.text = current.map_or_else(String::new, |current| current.text.clone());
    status.emoji = current.and_then(|current| emoji(state, current, requests));
    status.clear_after = ClearAfter::Never;
}

/// The emoji a status carries, as the dialog's button shows it.
fn emoji(state: &AppState, current: &CustomStatus, requests: &mut Requests) -> Option<Emoji> {
    match (&current.emoji, &current.emoji_id) {
        (Some(name), Some(id)) => {
            let key = format!("{name}:{id}");
            let address = custom_picture(&key);
            requests.want(state, address.as_deref());
            let picture = pictures::find(state, address.as_deref());
            Some(Emoji::custom(&key, &format!(":{name}:"), picture))
        }
        (Some(glyph), None) => Some(unicode(glyph)),
        _ => None,
    }
}

/// One of Unicode's, named as the emoji picker names it.
pub fn unicode(glyph: &str) -> Emoji {
    let name = anomi_vm::chat::emoji_picker::Emoji::unicode(glyph)
        .map(|emoji| emoji.name())
        .unwrap_or_default();
    Emoji::unicode(glyph, &name)
}

/// The picture of a server emoji `name:id`; none for a Unicode one.
pub fn custom_picture(key: &str) -> Option<String> {
    let (_, id) = key.rsplit_once(':')?;
    discord::emoji_picture(id)
}
