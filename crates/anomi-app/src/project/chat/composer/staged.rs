//! The files waiting to go with the message (`staged_rows`,
//! `src/ui/refresh.rs:5483`): each with its name, its size and, for a
//! picture, a preview read off the window's thread
//! (`crate::intent::chat::composer::files::previews`), at most 240 px a
//! side as Slint's. A spoiler's preview is blurred where it's drawn.

use std::collections::HashMap;
use std::path::PathBuf;

use iced::widget::image;
use yapfast_core::application::AppState;
use yapfast_core::chat;
use yapfast_lang::tr;

use anomi_vm::chat::composer::{Composer, Staged};

/// The staged pictures' previews, by path: `None` while one is read, or for
/// a file that turned out no picture.
pub(crate) type Previews = HashMap<PathBuf, Option<image::Handle>>;

pub(super) fn show(state: &AppState, composer: &mut Composer, previews: &Previews) {
    fill(state.staged(), &mut composer.staged, previews);
}

/// Shows `staged` in `shown`, unless it shows them already: the channel's,
/// the thread beside's, or a new forum post's.
pub(crate) fn fill(staged: &[chat::Staged], shown: &mut Vec<Staged>, previews: &Previews) {
    let same = shown.len() == staged.len()
        && shown.iter().zip(staged).all(|(shown, file)| {
            shown.name == file.name
                && shown.spoiler == file.spoiler
                && shown.picture.as_ref() == preview(previews, file)
        });
    if same {
        return;
    }
    *shown = staged
        .iter()
        .map(|file| Staged {
            name: file.name.clone(),
            size: chat::file_size(file.size),
            picture: preview(previews, file).cloned(),
            spoiler: file.spoiler,
            remove_label: tr!("Remove {}", file.name),
            spoil_label: if file.spoiler {
                tr!("Stop covering {}", file.name)
            } else {
                tr!("Mark {} as a spoiler", file.name)
            },
        })
        .collect();
}

fn preview<'a>(previews: &'a Previews, file: &chat::Staged) -> Option<&'a image::Handle> {
    previews.get(&file.path).and_then(Option::as_ref)
}
