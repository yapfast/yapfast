//! Files for the next message (`src/ui/callbacks.rs:2519`-`:2570`,
//! `src/ui.rs:2828`, `:2883`): chosen in the file dialog (rfd's, as Slint's), pasted (files
//! copied in a file manager, or a picture), or dropped on the window; and
//! the previews of the staged pictures, read off the window's thread.
//!
//! The clipboard is read by `platform::paste`: files, else a picture as the
//! bytes it was copied as, so a GIF pastes animated; Ctrl+V finding neither
//! pastes the text. Files dropped come as winit reports them (X11,
//! Windows), and on Wayland, where winit has no drag and drop, as
//! `platform::drop` follows them.

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
use crate::project::chat::composer::{fill_staged, show_staged};

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
    /// The clipboard's files, or its picture saved as a file; none for
    /// text.
    Pasted(Option<Vec<PathBuf>>),
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
        Done::Pasted(Some(files)) if !files.is_empty() => stage(app, files),
        Done::Pasted(_) => paste_text(field(app)),
        Done::Preview(path, picture) => {
            // Unless it was taken out of the composer meanwhile.
            let shown = &mut app.shown.composer;
            if let Some(slot) = shown.previews.get_mut(&path) {
                *slot = picture;
                let (state, chat) = (&app.core.state, &mut app.vm.chat);
                show_staged(state, &mut chat.composer, shown);
                let beside = &mut chat.thread_panel.composer.staged;
                fill_staged(state.aside_staged(), beside, &shown.previews);
                if let Some(post) = &mut chat.forum.composing {
                    fill_staged(state.staged(), &mut post.staged, &shown.previews);
                }
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

/// The files go with the next message, the channel's or the thread
/// beside's; the core says why they can't.
fn stage(app: &mut App, files: Vec<PathBuf>) -> Task<Message> {
    app.dispatch(if app.shown.composer.to_aside {
        Action::StageAside(files)
    } else {
        Action::Stage(files)
    })
}

/// The field the files on their way are for.
fn field(app: &App) -> iced::advanced::widget::Id {
    if app.shown.composer.to_aside {
        focus::ASIDE_COMPOSER
    } else {
        focus::COMPOSER
    }
}

/// The plus, or the menu's Upload a File: the file dialog ("Attach
/// files"), for the channel's composer or the thread beside's.
pub(super) fn attach(app: &mut App, aside: bool) -> Task<Message> {
    let state = &app.core.state;
    let can = if aside {
        state.aside_can_attach()
    } else {
        state.can_attach()
    };
    if !can {
        return Task::none();
    }
    app.shown.composer.to_aside = aside;
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
pub(super) fn paste(app: &mut App, aside: bool) -> Task<Message> {
    app.shown.composer.to_aside = aside;
    let state = &app.core.state;
    let (can, staged) = if aside {
        (state.aside_can_attach(), state.aside_staged())
    } else {
        (state.can_attach(), state.staged())
    };
    if !(can || staged.len() >= MAX_FILES) {
        return paste_text(field(app));
    }
    crate::platform::paste::read(app)
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

/// Files over the window say where they'd go, where they can: the thread
/// beside the channel when the pointer is over it (`drag`,
/// `show_drop_target`, `src/ui.rs:2883`, `:2905`); a refusal stays until
/// dismissed.
pub(crate) fn hovered(app: &mut App, over: bool) {
    let drop = &mut app.vm.shell.drop;
    if matches!(drop, Some(Files::Refused { .. })) {
        return;
    }
    let state = &app.core.state;
    let aside = app.shown.composer.over_aside;
    let (can, destination) = if aside {
        let room = state.aside_room();
        (
            state.aside_can_attach(),
            room.map(|room| state.destination(room)),
        )
    } else {
        (state.can_attach(), Some(state.upload_destination()))
    };
    *drop = (over && can).then(|| Files::Over {
        destination: destination.unwrap_or_default(),
        aside,
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
    let aside = std::mem::take(&mut app.shown.composer.over_aside);
    app.shown.composer.to_aside = aside;
    let state = &app.core.state;
    let (can, refusal) = if aside {
        (state.aside_can_attach(), state.aside_drop_refusal(&paths))
    } else {
        (state.can_attach(), state.drop_refusal(&paths))
    };
    // Staging says why it can't, as in a message being edited.
    if !can {
        return stage(app, paths);
    }
    if let Some(refusal) = refusal {
        app.vm.shell.drop = Some(Files::Refused {
            reason: refusal.title().to_owned(),
            help: refusal.help(),
            aside,
        });
        return Task::none();
    }
    let task = stage(app, paths);
    Task::batch([task, focus::to(field(app))])
}
