//! What the window needs of the desktop: the tray, the window itself, a
//! single instance, where it opens, dropped and pasted files, links and
//! global keybinds. One module each, its I task's (I14, I15, I18). The
//! toolkit-free parts are `anomi_desktop`'s, which the Slint app shares;
//! what they hear comes back here as [`Heard`], through one channel that
//! wakes the window only when something is said.

pub mod drop;
pub mod forget;
pub mod instance;
pub mod keybinds;
pub mod links;
pub mod paste;
pub mod placement;
#[cfg(test)]
mod tests;
pub mod tray;
pub mod window;

use std::sync::Arc;

use anomi_desktop::instance::{Instance, Launch};
use anomi_desktop::tray::{Choice, Tray};
use iced::Task;
use iced::futures::stream;

use crate::app::App;
use crate::cli::Start;
use crate::message::Message;

/// What the desktop said, from threads of its own.
#[derive(Clone, Debug)]
pub enum Heard {
    /// The tray's icon or its menu.
    Tray(Choice),
    /// Anomi was started again on this folder: the desktop's activation
    /// token for that start, and the link it brought.
    Launched(Launch),
}

/// The desktop's parts the app holds while it runs.
#[derive(Default)]
pub struct Desktop {
    /// The icon in the tray; none where there's no tray to use.
    pub(crate) tray: Option<Tray>,
    /// This start's hold on the data folder, while it's the first there.
    pub(crate) instance: Instance,
    /// A Discord link to open once Discord is online and nothing covers
    /// the window: one this start was handed, or a later start.
    pub(crate) link: Option<String>,
    /// The window's title, with the count of mentions waiting.
    pub(crate) title: String,
    /// The size the window opened at, in the view's pixels, until it says
    /// its display's scale and room (`placement::placed`).
    pub(crate) opened_at: Option<iced::Size>,
    /// Where the timeline was as the window closed to the tray.
    pub(crate) reading: Option<forget::Reading>,
}

impl Desktop {
    /// Whether closing the window leaves the app in a tray that shows.
    pub fn tray_shows(&self) -> bool {
        self.tray.as_ref().is_some_and(Tray::available)
    }
}

/// As the app boots: the icon goes in the tray, later starts are listened
/// for (both tell [`Heard`] through the first task), and the window opens
/// (the second), unless the app started at sign-in with a session to
/// restore and a tray to sit in, where it waits for the tray's Open.
pub fn start(app: &mut App, start: &Start) -> (Task<Message>, Task<Message>) {
    let heard = listen(app);
    if let Some(link) = start.link.clone() {
        links::queue(app, link);
    }
    // Windows knows at once whether the icon went in; on Linux the bus says
    // a moment later, and a tray found missing then opens the window.
    let tray = app.desktop.tray.as_ref();
    let tray = tray.is_some_and(|tray| !cfg!(windows) || tray.available());
    let restores = yapfast_core::chat::can_restore(&app.root);
    if start.background && start.link.is_none() && !start.preview && restores && tray {
        tracing::info!("started in the tray");
        app.hidden = true;
        return (heard, Task::none());
    }
    (heard, window::open(app, None))
}

fn listen(app: &mut App) -> Task<Message> {
    let (tell, heard) = tokio::sync::mpsc::unbounded_channel::<Heard>();
    let told = tell.clone();
    app.desktop.tray = Tray::start(Arc::new(move |choice| {
        let _ = told.send(Heard::Tray(choice));
    }));
    app.desktop.instance.listen(move |launch| {
        let _ = tell.send(Heard::Launched(launch));
    });
    show(app);
    let heard = stream::unfold(heard, |mut heard| async move {
        heard.recv().await.map(|said| (said, heard))
    });
    Task::run(heard, Message::Desktop)
}

/// The window's and the desktop's messages.
pub fn update(app: &mut App, message: Message) -> Task<Message> {
    match message {
        Message::Opened(id) => window::opened(app, id),
        Message::Placed(base, room) => placement::placed(app, base, room),
        Message::Resized(size) => {
            app.size = size;
            window::resized(app)
        }
        Message::Maximized(maximized) => {
            window::maximized(app, maximized);
            Task::none()
        }
        Message::Desktop(Heard::Tray(choice)) => tray::chosen(app, choice),
        Message::Desktop(Heard::Launched(launch)) => instance::launched(app, launch),
        Message::Release => {
            forget::release(app);
            Task::none()
        }
        Message::CloseRequested(_) => window::close(app),
        _ => Task::none(),
    }
}

/// What the core's state changed that the desktop shows, window or not:
/// the tray's dot and Disconnect, and the title's count, from one look at
/// the rooms.
pub fn show(app: &mut App) {
    let state = &app.core.state;
    let (mut mentions, mut unread) = (0, false);
    for room in &state.rooms {
        mentions += room.mentions;
        unread |= room.mentions > 0 || (room.direct && !room.muted && room.unread > 0);
    }
    let call = state.voice.active();
    if let Some(tray) = &app.desktop.tray {
        tray.show(anomi_desktop::tray::Shown { unread, call });
    }
    window::title(&mut app.desktop.title, mentions);
}
