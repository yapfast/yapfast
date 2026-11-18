//! What Ctrl+V, or the composer field's Paste, finds on the clipboard for
//! the next message (`paste`, `src/ui.rs`): files copied in a file manager,
//! else a picture copied from a browser or a screenshot tool, kept as the
//! bytes it was copied as, so a GIF stays animated. `anomi_desktop::paste`
//! reads the clipboard off the window's thread, as the app that copied may
//! take its time (and may be this one); anything else the composer pastes
//! as text.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anomi_desktop::paste::Clipboard;
use iced::Task;

use crate::app::App;
use crate::intent::chat::composer::Done;
use crate::message::Message;

/// Reads the clipboard, then tells the composer what it found.
pub fn read(app: &mut App) -> Task<Message> {
    let pasted = &mut app.desktop.pasted;
    if pasted.is_none() {
        *pasted = tempfile::Builder::new()
            .prefix("anomi-pasted-")
            .tempdir()
            .inspect_err(|error| {
                tracing::warn!(%error, "could not make a folder for pasted pictures");
            })
            .ok();
    }
    let folder = pasted.as_ref().map(|folder| folder.path().to_owned());
    let runtime = Arc::clone(&app.runtime);
    Task::perform(
        async move {
            let read = move || files(anomi_desktop::paste::read(), folder.as_deref());
            runtime.spawn_blocking(read).await.ok().flatten()
        },
        |files| Message::Composer(Done::Pasted(files)),
    )
}

/// The files to go with the message for what the clipboard held: its
/// files, or its picture as `image` with its type's extension, as Discord
/// names a pasted one, in a folder of its own under `folder`, which goes as
/// the app ends; none for text, or a picture that can't be kept.
fn files(clipboard: Clipboard, folder: Option<&Path>) -> Option<Vec<PathBuf>> {
    match clipboard {
        Clipboard::Files(files) => Some(files),
        Clipboard::Picture(bytes, extension) => {
            let folder = tempfile::Builder::new()
                .prefix("")
                .tempdir_in(folder?)
                .ok()?
                .keep();
            let path = folder.join(format!("image.{extension}"));
            std::fs::write(&path, bytes).ok()?;
            Some(vec![path])
        }
        Clipboard::Other => None,
    }
}

#[cfg(test)]
mod tests {
    use anomi_desktop::paste::Clipboard;

    #[test]
    fn a_pasted_gif_is_kept_as_it_was_copied() {
        let folder = tempfile::tempdir().unwrap();
        let gif = b"GIF89a, every frame of it".to_vec();
        let files = super::files(Clipboard::Picture(gif.clone(), "gif"), Some(folder.path()));
        let [file] = files.as_deref().unwrap() else {
            panic!("one file");
        };
        assert_eq!(file.file_name().unwrap(), "image.gif");
        assert!(file.starts_with(folder.path()));
        assert_eq!(std::fs::read(file).unwrap(), gif);
        // Each paste has a folder of its own, so each is image.gif.
        let again = super::files(Clipboard::Picture(gif, "gif"), Some(folder.path()));
        assert_ne!(again.unwrap()[0], *file);
    }

    #[test]
    fn text_on_the_clipboard_is_no_file() {
        assert_eq!(super::files(Clipboard::Other, None), None);
        let copied = vec![std::path::PathBuf::from("/a")];
        assert_eq!(
            super::files(Clipboard::Files(copied.clone()), None),
            Some(copied)
        );
    }
}
