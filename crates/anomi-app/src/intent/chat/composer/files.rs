//! Files for the next message (`src/ui/callbacks.rs:2519`-`:2570`,
//! `src/ui.rs:2828`, `:2883`): chosen in the file dialog (rfd's, as Slint's), pasted (files
//! copied in a file manager, or a picture), or dropped on the window; and
//! the previews of the staged pictures, read off the window's thread.
//!
//! The clipboard is read through iced's (arboard: files as a URI list, a
//! picture as its pixels, saved as PNG in a folder of the app's own), so a
//! GIF copied as one pastes still; Ctrl+V finding neither pastes the text.
//! Files dropped come as winit reports them (X11, Windows); on Wayland,
//! where winit has no drag and drop, `platform::drop` will (I15).

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use iced::Task;
use iced::widget::image;
use yapfast_core::application::Action;
use yapfast_core::chat::MAX_FILES;
use yapfast_lang::tr;

use anomi_vm::shell::Files;
use anomi_widgets::composer::paste_text;
use anomi_widgets::focus;

use crate::app::{App, later};
use crate::message::Message;
use crate::project::chat::composer::show_staged;

/// The longest side of a staged picture's preview, as Slint's.
const PREVIEW: u32 = 240;
/// How long files dropped together wait for the rest of them: winit tells
/// of each on its own.
const DROPPED_TOGETHER: Duration = Duration::from_millis(30);

/// What the composer's work off the window's thread brought back.
#[derive(Clone, Debug)]
pub enum Done {
    /// The file dialog's files, for the next message.
    Chosen(Vec<PathBuf>),
    /// A file for the option of the command being written at this index.
    CommandFile(usize, PathBuf),
    /// The clipboard's files, or none.
    PastedFiles(Option<Vec<PathBuf>>),
    /// The clipboard's picture, saved as a file, or none.
    PastedPicture(Option<PathBuf>),
    /// A staged picture's preview, or none for a file that isn't one.
    Preview(PathBuf, Option<image::Handle>),
    /// Files are dragged over the window (`true`), or left it.
    Hovered(bool),
    /// A file dropped on the window.
    Dropped(PathBuf),
    /// The files dropped together have all come.
    DroppedAll,
}

pub(super) fn done(app: &mut App, done: Done) -> Task<Message> {
    match done {
        Done::Chosen(files) => stage(app, files),
        Done::CommandFile(index, path) => app.dispatch(Action::SetCommandOption {
            index,
            value: path.to_string_lossy().into_owned(),
        }),
        Done::PastedFiles(Some(files)) if !files.is_empty() => stage(app, files),
        Done::PastedFiles(_) => paste_picture(),
        Done::PastedPicture(Some(file)) => stage(app, vec![file]),
        Done::PastedPicture(None) => paste_text(focus::COMPOSER),
        Done::Preview(path, picture) => {
            // Unless it was taken out of the composer meanwhile.
            let shown = &mut app.shown.composer;
            if let Some(slot) = shown.previews.get_mut(&path) {
                *slot = picture;
                show_staged(&app.core.state, &mut app.vm.chat.composer, shown);
            }
            Task::none()
        }
        Done::Hovered(over) => {
            hovered(app, over);
            Task::none()
        }
        Done::Dropped(path) => {
            let dropped = &mut app.shown.composer.dropped;
            dropped.push(path);
            if dropped.len() == 1 {
                later(DROPPED_TOGETHER, Message::Composer(Done::DroppedAll))
            } else {
                Task::none()
            }
        }
        Done::DroppedAll => dropped(app),
    }
}

/// The files go with the next message; the core says why they can't.
fn stage(app: &mut App, files: Vec<PathBuf>) -> Task<Message> {
    app.dispatch(Action::Stage(files))
}

/// The plus, or the menu's Upload a File: the file dialog ("Attach
/// files").
pub(super) fn attach(app: &mut App) -> Task<Message> {
    if !app.core.state.can_attach() {
        return Task::none();
    }
    choose(|| {
        let files = rfd::FileDialog::new()
            .set_title(tr!("Attach files"))
            .pick_files()?;
        Some(Done::Chosen(files))
    })
}

/// A file option's file, from the same dialog.
pub(super) fn command_file(_app: &mut App, index: usize) -> Task<Message> {
    choose(move || {
        let file = rfd::FileDialog::new()
            .set_title(tr!("Choose a file"))
            .pick_file()?;
        Some(Done::CommandFile(index, file))
    })
}

/// Runs the file dialog off the window's thread; nothing comes of one
/// closed without a choice.
fn choose(pick: impl FnOnce() -> Option<Done> + Send + 'static) -> Task<Message> {
    Task::perform(
        async move { tokio::task::spawn_blocking(pick).await.ok().flatten() },
        |done| done,
    )
    .and_then(|done| Task::done(Message::Composer(done)))
}

/// Ctrl+V or the field's Paste: files and pictures on the clipboard wait to
/// go with the message, and anything else is pasted as text. Where files
/// can't go, the text pastes at once; a full composer still looks, to say
/// so.
pub(super) fn paste(app: &mut App) -> Task<Message> {
    let state = &app.core.state;
    if !(state.can_attach() || state.staged().len() >= MAX_FILES) {
        return paste_text(focus::COMPOSER);
    }
    iced::clipboard::read_files()
        .map(|files| Message::Composer(Done::PastedFiles(files.ok().map(|files| files.to_vec()))))
}

/// No files on the clipboard: a picture, saved where the app keeps them.
fn paste_picture() -> Task<Message> {
    iced::clipboard::read_image()
        .then(|read| match read {
            Ok(picture) => Task::perform(
                async move {
                    tokio::task::spawn_blocking(move || save(&picture))
                        .await
                        .ok()
                        .flatten()
                },
                |file| file,
            ),
            Err(_) => Task::done(None),
        })
        .map(|file| Message::Composer(Done::PastedPicture(file)))
}

/// A pasted picture as `image.png`, as Discord names it, in a folder of
/// its own for each paste under the app's in the system's temporary place.
fn save(picture: &iced::clipboard::Image) -> Option<PathBuf> {
    let folder = std::env::temp_dir().join("anomi-pasted");
    std::fs::create_dir_all(&folder).ok()?;
    let folder = tempfile::Builder::new()
        .prefix("")
        .tempdir_in(folder)
        .ok()?
        .keep();
    let path = folder.join("image.png");
    let pixels = ::image::RgbaImage::from_raw(
        picture.size.width,
        picture.size.height,
        picture.rgba.to_vec(),
    )?;
    pixels
        .save_with_format(&path, ::image::ImageFormat::Png)
        .ok()?;
    Some(path)
}

/// Starts reading the previews of the staged pictures that lack one, and
/// lets go of those no longer staged: after every update, as staging,
/// unstaging and opening a room all change what's staged. Nothing to do
/// costs a look at a few files.
pub(crate) fn previews(app: &mut App) -> Task<Message> {
    let state = &app.core.state;
    let staged = state.staged();
    let aside = state.aside_staged();
    let previews = &mut app.shown.composer.previews;
    if previews.len() > staged.len() + aside.len() {
        previews.retain(|path, _| staged.iter().chain(aside).any(|file| &file.path == path));
    }
    let mut tasks = Vec::new();
    for file in staged.iter().chain(aside).filter(|file| file.image) {
        if previews.contains_key(&file.path) {
            continue;
        }
        previews.insert(file.path.clone(), None);
        let path = file.path.clone();
        let runtime = Arc::clone(&app.runtime);
        tasks.push(Task::perform(
            async move {
                let read = path.clone();
                let picture = runtime
                    .spawn_blocking(move || {
                        yapfast_core::chat::decode_picture_at(&read, PREVIEW, PREVIEW)
                    })
                    .await
                    .ok()
                    .flatten();
                let handle = picture.as_ref().and_then(crate::pictures::handle);
                (path, handle)
            },
            |(path, handle)| Message::Composer(Done::Preview(path, handle)),
        ));
    }
    Task::batch(tasks)
}

/// Files over the window say where they'd go, where they can (`drag`,
/// `src/ui.rs:2883`); a refusal stays until dismissed.
fn hovered(app: &mut App, over: bool) {
    let drop = &mut app.vm.shell.drop;
    if matches!(drop, Some(Files::Refused { .. })) {
        return;
    }
    let state = &app.core.state;
    *drop = (over && state.can_attach()).then(|| Files::Over {
        destination: state.upload_destination(),
        aside: false,
    });
}

/// Files let go over the window go with the next message: all of them, or
/// none and why. The composer then has the keys.
fn dropped(app: &mut App) -> Task<Message> {
    let paths = std::mem::take(&mut app.shown.composer.dropped);
    if matches!(app.vm.shell.drop, Some(Files::Over { .. })) {
        app.vm.shell.drop = None;
    }
    if paths.is_empty() {
        return Task::none();
    }
    let state = &app.core.state;
    // Staging says why it can't, as in a message being edited.
    if !state.can_attach() {
        return stage(app, paths);
    }
    if let Some(refusal) = state.drop_refusal(&paths) {
        app.vm.shell.drop = Some(Files::Refused {
            reason: refusal.title().to_owned(),
            help: refusal.help(),
            aside: false,
        });
        return Task::none();
    }
    let task = stage(app, paths);
    Task::batch([task, focus::to(focus::COMPOSER)])
}
