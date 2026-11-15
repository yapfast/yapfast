//! The window, which closing to the tray drops and opening makes again, and
//! what keeps Yapfast running meanwhile, as the Slint client's
//! `src/ui/shell.rs`: the tray icon, notifications, a later start handing
//! over its link, and zooming the window. The desktop's own parts are the
//! root crate's (`desktop.rs`); this decides when each is used.

use std::time::Duration;

use iced::{
    Size, Subscription, Task,
    keyboard::{self, key},
    window,
};
use yapfast_core::{application::Action, chat::Alert, sounds::Sound, voice};

use crate::{
    after,
    app::{App, Message},
    desktop::{self, ABOUT, Heard, Notifier, Tell, Tray, TrayChoice},
    zoom,
};

/// The least size the window lays out in, as the Slint client's window.
pub const MIN_SIZE: Size = Size::new(800.0, 560.0);

/// How long the zoom level shows after it changes.
const ZOOM_NOTE: Duration = Duration::from_millis(1200);

/// How long after the window closes its memory is given back: iced lets
/// go of the window's last pieces as its event loop goes on.
const RELEASE_AFTER: Duration = Duration::from_millis(500);

/// How this start of Yapfast opens.
pub struct Start {
    /// To the tray, with a saved session, as starting at sign-in does.
    pub background: bool,
    /// A Discord link handed to this start.
    pub link: Option<String>,
}

/// The window while it's open, and what stays without it.
#[derive(Default)]
pub struct Shell {
    /// The window, while open.
    pub window: Option<window::Id>,
    pub focused: bool,
    pub maximized: bool,
    pub full_screen: bool,
    /// The size the window had, which one made again from the tray takes.
    pub size: Option<Size>,
    /// The whole window's size, in the app's units; `App::window` is the
    /// room under the title bar.
    pub frame: Size,
    pub tray: Option<Box<dyn Tray>>,
    pub notifier: Option<Box<dyn Notifier>>,
    /// Hands the desktop's words to the window's event loop.
    pub tell: Option<Tell>,
    /// A Discord link handed to this start or a later one, kept until
    /// Discord is online and nothing is open over the window.
    pub pending_link: Option<String>,
    /// The activation token the window being made takes the focus with.
    pub token: Option<String>,
    /// How far the whole window is zoomed, as one of `zoom::LEVELS`, and the
    /// level's note while it shows, by its count.
    pub zoom: f32,
    pub zoom_note: Option<u64>,
    /// The level was held to the first window's room, as it opened.
    pub zoom_fitted: bool,
    /// The display's own scale factor, as the window last said, which
    /// the zoom multiplies.
    pub display_scale: Option<f32>,
    /// The window's own title bar is drawn (`window_bar.rs`), its frame left
    /// out.
    pub own_title_bar: bool,
}

/// Something the desktop or the window's frame did.
#[derive(Clone, Debug)]
pub enum Event {
    Heard(Heard),
    /// The window was made, and opened.
    Opened(window::Id),
    /// The window's own frame: its close button, its focus, gone.
    CloseRequested(window::Id),
    Focused(window::Id, bool),
    Closed(window::Id),
    /// The window's memory is due to be given back.
    Release,
    /// The zoom level's note is due to go.
    ZoomNoted(u64),
    /// The display's scale factor, as the window opened or moved to
    /// another display.
    Scaled(window::Id, f32),
}

impl App {
    /// Starts the tray, notifications and listening for later starts, then
    /// opens the window, unless this start goes to the tray.
    pub fn start_shell(&mut self, start: Start) -> Task<Message> {
        let (sender, heard) = tokio::sync::mpsc::unbounded_channel();
        let tell: Tell = std::sync::Arc::new(move |heard| {
            let _ = sender.send(heard);
        });
        let desktop = desktop::desktop();
        desktop.start(tell.clone());
        self.shell.tray = desktop.tray(tell.clone());
        self.shell.notifier = desktop.notifier(self.client.root(), tell.clone());
        self.shell.tell = Some(tell);
        self.shell.pending_link = start.link;
        self.show_tray();
        // Once at start with the check on, as the Slint client looks.
        if self.layout.prefs.check_for_updates {
            self.look_for_updates(true);
        }
        let heard = Task::run(
            iced::futures::stream::unfold(heard, |mut heard| async move {
                let next = heard.recv().await?;
                Some((next, heard))
            }),
            |heard| Message::Shell(Event::Heard(heard)),
        );
        // Windows knows at once whether the icon went in. On Linux the bus
        // says a moment later, and without a tray, `Available` opens the
        // window.
        let in_tray = start.background
            && self
                .shell
                .tray
                .as_ref()
                .is_some_and(|tray| !cfg!(windows) || tray.available());
        if in_tray {
            tracing::info!("started in the tray");
            return Task::batch([heard, after(RELEASE_AFTER, Message::Shell(Event::Release))]);
        }
        Task::batch([heard, self.open_window()])
    }

    pub fn shell_event(&mut self, event: Event) -> Task<Message> {
        match event {
            Event::Heard(heard) => self.heard(heard),
            Event::Opened(id) => self.opened(id),
            Event::CloseRequested(id) if self.shell.window == Some(id) => self.close_requested(),
            Event::CloseRequested(_) => Task::none(),
            Event::Focused(id, focused) if self.shell.window == Some(id) => {
                self.shell.focused = focused;
                if focused {
                    return window::request_user_attention(id, None);
                }
                Task::none()
            }
            Event::Focused(..) => Task::none(),
            Event::Closed(id) => {
                // Closed by the desktop rather than by Yapfast.
                if self.shell.window == Some(id) {
                    self.shell.window = None;
                    return self.forget_window();
                }
                Task::none()
            }
            Event::Release => {
                desktop::desktop().release_memory();
                Task::none()
            }
            Event::ZoomNoted(count) => {
                if self.shell.zoom_note == Some(count) {
                    self.shell.zoom_note = None;
                }
                Task::none()
            }
            Event::Scaled(id, scale) => {
                if self.shell.window == Some(id) && self.shell.display_scale != Some(scale) {
                    self.shell.display_scale = Some(scale);
                    // Frames are made at the window's pixels.
                    self.place_videos();
                }
                Task::none()
            }
        }
    }

    /// The window's pixels to the app's units: the display's scale times
    /// the zoom, which iced draws at.
    pub fn device_scale(&self) -> f32 {
        self.shell.display_scale.unwrap_or(1.0) * self.shell.zoom
    }

    fn heard(&mut self, heard: Heard) -> Task<Message> {
        match heard {
            Heard::Tray(TrayChoice::Open) => {
                self.shell.token = desktop::desktop().tray_token();
                self.open_window()
            }
            Heard::Tray(TrayChoice::Disconnect) => {
                self.client.dispatch(Action::Voice(voice::Command::Leave));
                self.sync()
            }
            Heard::Tray(TrayChoice::Quit) => self.quit(),
            Heard::Tray(TrayChoice::Available) => {
                let available = self
                    .shell
                    .tray
                    .as_ref()
                    .is_some_and(|tray| tray.available());
                // Without a tray to open it from, the window can't stay closed.
                if self.shell.window.is_none() && !available {
                    tracing::info!("the tray went; opening the window");
                    return self.open_window();
                }
                Task::none()
            }
            Heard::Opened(room) => {
                let open = self.open_window();
                // The notification of a newer release opens About.
                if room == ABOUT {
                    return Task::batch([open, self.open_about()]);
                }
                let closed = self.close_settings();
                self.client.dispatch(Action::OpenChat(room));
                Task::batch([open, closed, self.sync()])
            }
            Heard::Launched { token, link } => {
                tracing::info!("Yapfast started again; showing the window");
                if let Some(link) = link {
                    if yapfast_core::discord::discord_route(&link).is_some() {
                        self.shell.pending_link = Some(link);
                    } else {
                        tracing::warn!("a later start handed Yapfast a link it can't open");
                    }
                }
                self.shell.token = token;
                Task::batch([self.open_window(), self.open_pending_link()])
            }
            Heard::Updates => self.updates_heard(),
            Heard::Drag(drag) => self.dragged(drag),
        }
    }

    /// Opens the window, or brings it to the front: made again as the
    /// closed one was, at its size.
    pub fn open_window(&mut self) -> Task<Message> {
        if let Some(id) = self.shell.window {
            let token = self.shell.token.take();
            return Task::batch([
                window::set_mode(id, window::Mode::Windowed),
                window::minimize(id, false),
                window::gain_focus(id),
                self.activate(id, token),
            ]);
        }
        let (id, opened) = window::open(self.window_settings());
        self.shell.window = Some(id);
        opened.map(|id| Message::Shell(Event::Opened(id)))
    }

    /// The window as Yapfast makes it: its size (the test display's, or the
    /// one it had), its frame unless Yapfast draws its own title bar, and
    /// its icon and app ID, by which the desktop finds its entry.
    fn window_settings(&self) -> window::Settings {
        let mut settings = window::Settings {
            size: self.shell.size.unwrap_or(self.shell.frame),
            min_size: Some(MIN_SIZE),
            decorations: !self.shell.own_title_bar,
            exit_on_close_request: false,
            icon: desktop::desktop()
                .window_icon()
                .and_then(|(pixels, width, height)| {
                    window::icon::from_rgba(pixels, width, height).ok()
                }),
            ..window::Settings::default()
        };
        #[cfg(target_os = "linux")]
        {
            settings.platform_specific.application_id = desktop::desktop().app_id().to_owned();
        }
        let _ = &mut settings;
        settings
    }

    /// The window opened: what iced doesn't give it, then the focus with a
    /// later start's token, and what was waiting for it.
    fn opened(&mut self, id: window::Id) -> Task<Message> {
        if self.shell.window != Some(id) {
            return Task::none();
        }
        self.shell.focused = true;
        let tell = self.shell.tell.clone();
        let decorate = window::run(id, move |window| {
            if let Some(tell) = tell {
                desktop::desktop().decorate(window, tell);
            }
        })
        .discard();
        let token = self.shell.token.take();
        let mut tasks = vec![
            decorate,
            self.activate(id, token),
            self.open_pending_link(),
            self.read_window_state(),
            window::scale_factor(id).map(move |scale| Message::Shell(Event::Scaled(id, scale))),
        ];
        if self.shell.maximized {
            tasks.push(window::maximize(id, true));
        }
        if self.workspace() && self.client.state().shows_text() {
            tasks.push(iced::widget::operation::focus(crate::composer::COMPOSER));
        }
        // The call's videos show again.
        self.place_videos();
        Task::batch(tasks)
    }

    /// Brings the window forward with the token a later start or the tray
    /// click brought; without one, Wayland only marks it as asking.
    fn activate(&self, id: window::Id, token: Option<String>) -> Task<Message> {
        match token {
            Some(token) => window::run(id, move |window| {
                desktop::desktop().activate(window, &token);
            })
            .discard(),
            None if cfg!(target_os = "linux") => window::run(id, |window| {
                use window::raw_window_handle::RawDisplayHandle;
                window
                    .display_handle()
                    .is_ok_and(|display| matches!(display.as_raw(), RawDisplayHandle::Wayland(_)))
            })
            .then(move |wayland| {
                if wayland {
                    window::request_user_attention(id, Some(window::UserAttention::Informational))
                } else {
                    Task::none()
                }
            }),
            None => Task::none(),
        }
    }

    /// The window's close button: Yapfast goes on in the tray, or without a
    /// tray to show it, or with the setting off, quits.
    pub fn close_requested(&mut self) -> Task<Message> {
        let to_tray = self.layout.prefs.close_to_tray.unwrap_or(true)
            && self
                .shell
                .tray
                .as_ref()
                .is_some_and(|tray| tray.available());
        if !to_tray {
            tracing::info!("window closed without a tray to close to; quitting");
            return self.quit();
        }
        self.close_window()
    }

    /// Closes the window to the tray: iced drops it, its renderer and its
    /// widgets, and the window's own pictures go with them.
    pub fn close_window(&mut self) -> Task<Message> {
        if self.shell.window.is_none() {
            return Task::none();
        }
        Task::batch([
            self.drop_window(),
            after(RELEASE_AFTER, Message::Shell(Event::Release)),
        ])
    }

    /// Drops the window, keeping its size for the next.
    pub fn drop_window(&mut self) -> Task<Message> {
        let Some(id) = self.shell.window.take() else {
            return Task::none();
        };
        self.shell.size = Some(self.shell.frame);
        let forgot = self.forget_window();
        tracing::info!("window closed");
        Task::batch([forgot, window::close(id)])
    }

    /// What only the window used goes with it; the core's state stays, so
    /// the next window shows it without asking again. Settings close as
    /// their own button closes them, letting go of what they ran; the
    /// call's popup goes with the microphone it may have open; the call's
    /// videos stop being made for tiles nobody sees.
    fn forget_window(&mut self) -> Task<Message> {
        self.shell.focused = false;
        self.close_popup();
        self.call.details.pointed = false;
        self.menu = None;
        self.picker = None;
        self.card = None;
        let settings = self.close_settings();
        self.place_videos();
        self.pictures.forget();
        settings
    }

    /// Ends Yapfast: the icon goes first, rather than when the desktop next
    /// looks at it.
    pub fn quit(&mut self) -> Task<Message> {
        self.shell.tray = None;
        iced::exit()
    }

    /// The tray icon's unread dot, for a mention or an unread DM as Discord
    /// counts them, and Disconnect in its menu during a call.
    pub fn show_tray(&self) {
        let Some(tray) = &self.shell.tray else {
            return;
        };
        let state = self.client.state();
        tray.show(
            state
                .rooms
                .iter()
                .any(|room| room.mentions > 0 || (room.direct && !room.muted && room.unread > 0)),
            state.voice.active(),
        );
    }

    /// After each change: the tray's icon, the alerts the core raised, and a
    /// link waiting for Discord.
    pub fn shell_sync(&mut self) -> Task<Message> {
        self.show_tray();
        let mut tasks = Vec::new();
        for alert in self.client.take_alerts() {
            tasks.push(self.alert(alert));
        }
        if let Some(bytes) = self.client.take_copied_picture() {
            desktop::desktop().copy_picture(bytes);
        }
        if self.shell.pending_link.is_some() && self.shell.window.is_some() {
            tasks.push(self.open_pending_link());
        }
        Task::batch(tasks)
    }

    /// A new message the core says to tell of, as the Slint client does: it
    /// sounds unless it's in the conversation in front (a call rings
    /// instead); while Yapfast isn't in front, the taskbar flashes and a
    /// notification shows, as the settings allow.
    fn alert(&mut self, alert: Alert) -> Task<Message> {
        let focused = self.shell.focused;
        let reading = focused && self.client.state().selected.as_ref() == Some(&alert.room);
        if !alert.call && !reading {
            self.play(if alert.mention {
                Sound::Mention
            } else {
                Sound::Message
            });
        }
        let prefs = &self.layout.prefs;
        let mut task = Task::none();
        if !focused
            && !alert.call
            && prefs.flash_taskbar.unwrap_or(true)
            && let Some(id) = self.shell.window
        {
            task = window::request_user_attention(id, Some(window::UserAttention::Informational));
        }
        if prefs.notifications.unwrap_or(true)
            && !focused
            && let Some(notifier) = &self.shell.notifier
        {
            notifier.show(alert, prefs.notification_text.unwrap_or(true));
        }
        task
    }

    /// Opens a Discord link a start handed over, once Discord is online and
    /// nothing is open over the window that would hide where it goes.
    fn open_pending_link(&mut self) -> Task<Message> {
        let state = self.client.state();
        if !state.discord_online || state.dialog_open() || self.shell.window.is_none() {
            return Task::none();
        }
        let Some(link) = self.shell.pending_link.take() else {
            return Task::none();
        };
        let middle = iced::Point::new(self.window.width / 2.0, self.window.height / 3.0);
        self.follow_link(&link, middle)
    }

    /// Files dragged over the window, as the desktop follows them where
    /// winit doesn't (Wayland).
    fn dragged(&mut self, drag: desktop::Drag) -> Task<Message> {
        use crate::uploads::Event;
        match drag {
            desktop::Drag::Entered => self.uploads_event(Event::Hovered),
            desktop::Drag::Moved(x, y) => {
                let zoom = self.shell.zoom;
                self.uploads_event(Event::Moved(iced::Point::new(
                    (x / f64::from(zoom)) as f32,
                    (y / f64::from(zoom)) as f32,
                )))
            }
            desktop::Drag::Left => self.uploads_event(Event::Left),
            desktop::Drag::Dropped(files) => {
                let tasks: Vec<_> = files
                    .into_iter()
                    .map(|file| self.uploads_event(Event::Dropped(file)))
                    .collect();
                Task::batch(tasks)
            }
        }
    }

    /// Each window's view: Yapfast has one.
    pub fn view_of(&self, id: window::Id) -> iced::Element<'_, Message> {
        if self.shell.window == Some(id) {
            self.view()
        } else {
            iced::widget::Space::new().into()
        }
    }

    /// The window's title, counting the mentions waiting, as a browser
    /// tab's does; every message in a DM counts as one.
    pub fn title(&self, _: window::Id) -> String {
        let waiting: u64 = self
            .client
            .state()
            .rooms
            .iter()
            .map(|room| room.mentions)
            .sum();
        match waiting {
            0 => "Yapfast".to_owned(),
            1..=99 => format!("({waiting}) Yapfast"),
            _ => "(99+) Yapfast".to_owned(),
        }
    }

    /// The zoom, which iced multiplies the display's own scale by.
    pub fn scale_factor(&self, _: window::Id) -> f32 {
        self.shell.zoom
    }

    /// Ctrl with + or - steps the zoom, Ctrl+0 returns to 100%, as the
    /// Slint client's (`zoom.rs`).
    pub fn zoom_key(
        &mut self,
        key: &keyboard::Key,
        physical: key::Physical,
    ) -> Option<Task<Message>> {
        let text = match key {
            keyboard::Key::Character(text) => Some(text.as_str()),
            _ => None,
        };
        let place = match physical {
            key::Physical::Code(key::Code::NumpadAdd) => zoom::KeyPlace::NumpadAdd,
            key::Physical::Code(key::Code::NumpadSubtract) => zoom::KeyPlace::NumpadSubtract,
            key::Physical::Code(key::Code::Digit0 | key::Code::Numpad0) => zoom::KeyPlace::Zero,
            _ => zoom::KeyPlace::Other,
        };
        let step = zoom::key_step(text, place)?;
        Some(self.zoom_step(step))
    }

    /// Zooms a step, no further in than the window's room allows, keeps the
    /// level, and shows it a moment.
    pub fn zoom_step(&mut self, step: zoom::Step) -> Task<Message> {
        let level = self.shell.zoom;
        // The window's size before any zoom, in the display's own pixels.
        let width = (self.shell.frame.width * level) as u32;
        let height = (self.shell.frame.height * level) as u32;
        let next = zoom::stepped(level, step, zoom::most(width, height, 1.0));
        if (next - level).abs() < f32::EPSILON {
            return Task::none();
        }
        self.shell.zoom = next;
        // The layout gets fewer logical pixels to fill; the next resize says
        // so exactly.
        self.shell.frame = self.shell.frame * (level / next);
        self.fit_window();
        self.place_videos();
        crate::layout::store(self.client.root(), "zoom", next);
        let count = self.shell.zoom_note.map_or(0, |count| count + 1);
        self.shell.zoom_note = Some(count);
        after(ZOOM_NOTE, Message::Shell(Event::ZoomNoted(count)))
    }

    /// The first window's size is known: a level kept from a larger
    /// screen comes down to what its room allows, as the Slint client
    /// starts at the saved level or less. Not kept: the next start on the
    /// larger screen zooms as before.
    pub fn fit_zoom(&mut self) {
        if std::mem::replace(&mut self.shell.zoom_fitted, true) {
            return;
        }
        let level = self.shell.zoom;
        let width = (self.shell.frame.width * level) as u32;
        let height = (self.shell.frame.height * level) as u32;
        let fitted = level
            .min(zoom::most(width, height, 1.0))
            .max(zoom::LEVELS[0]);
        if fitted < level {
            tracing::info!(level, fitted, "the saved zoom doesn't fit the window");
            self.shell.zoom = fitted;
            self.shell.frame = self.shell.frame * (level / fitted);
            self.fit_window();
        }
    }

    /// What the window's frame says: its close button, its focus, gone.
    pub fn shell_subscription() -> Subscription<Message> {
        iced::event::listen_with(|event, _, id| match event {
            iced::Event::Window(window::Event::CloseRequested) => {
                Some(Message::Shell(Event::CloseRequested(id)))
            }
            iced::Event::Window(window::Event::Focused) => {
                Some(Message::Shell(Event::Focused(id, true)))
            }
            iced::Event::Window(window::Event::Unfocused) => {
                Some(Message::Shell(Event::Focused(id, false)))
            }
            iced::Event::Window(window::Event::Closed) => Some(Message::Shell(Event::Closed(id))),
            iced::Event::Window(window::Event::Rescaled(scale)) => {
                Some(Message::Shell(Event::Scaled(id, scale)))
            }
            _ => None,
        })
    }
}
