//! The window, which closing to the tray drops and opening makes again, and
//! what keeps Yapfast running meanwhile: the tray icon, notifications, a
//! later start handing over its link, and zooming the window. The desktop's
//! own parts are the root crate's (`desktop.rs`); this decides when each is
//! used.

use std::time::Duration;

use iced::{
    Size, Subscription, Task,
    keyboard::{self, key},
    window,
};
use yapfast_core::{application::Action, chat::Alert, sounds::Sound, voice, zoom};

use crate::{
    after,
    app::{App, Message},
    desktop::{self, ABOUT, Heard, Notifier, Tell, Tray, TrayChoice},
};

/// The least size the window lays out in.
pub const MIN_SIZE: Size = Size::new(800.0, 560.0);
/// The most a side of the window is asked to be, whatever `layout.json`
/// says.
const MOST: f32 = 16384.0;

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
    /// This start replaced one whose GPU failed to start.
    pub gpu_failed: bool,
    /// The window's size is the test display's, whatever the last was.
    pub fixed_size: bool,
}

/// The window while it's open, and what stays without it.
#[derive(Default)]
pub struct Shell {
    /// The window, while open.
    pub window: Option<window::Id>,
    pub focused: bool,
    pub maximized: bool,
    pub full_screen: bool,
    /// The size the window had while it wasn't maximized, which one made
    /// again from the tray takes, and the first what `layout.json` kept.
    pub size: Option<Size>,
    /// How many sizes the window has taken, by which a read of its state
    /// tells whether it was asked at the size it has now.
    pub sized: u32,
    /// Where the window's frame was then, in the desktop's pixels, for
    /// the next window as the size is; none where the desktop doesn't
    /// keep it (`Desktop::window_place`).
    pub position: Option<[i32; 2]>,
    /// The test display sets the window's size: none is kept or read.
    pub fixed_size: bool,
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
    /// The scale of the monitor the window is on, where the desktop says
    /// and it may differ from the window's (`Desktop::monitor_scale`).
    pub monitor_scale: Option<f32>,
    /// The window's own title bar is drawn (`window_bar.rs`), its frame left
    /// out.
    pub own_title_bar: bool,
    /// That the GPU failed to start, until the banner is dismissed.
    pub gpu_notice: bool,
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
    /// The window moved or changed size, so it may be on another monitor
    /// and at another place; asked on Windows only, where the monitor's
    /// scale and the window's place are read.
    Placed(window::Id),
    /// Where the window's frame is, as the desktop read it.
    At(window::Id, Option<[i32; 2]>),
    /// The scale of the monitor the window is on, as the desktop read it.
    MonitorScaled(window::Id, Option<f32>),
    /// The banner that the GPU failed to start, dismissed.
    DismissGpuNotice,
    /// The window took another size, or opened at one.
    Resized(Size),
    /// A key pressed with Ctrl, which may zoom.
    Zoom(keyboard::Key, key::Physical),
    /// Ctrl and the wheel.
    ZoomStep(zoom::Step),
}

impl App {
    /// Starts the tray, notifications and listening for later starts, then
    /// opens the window, unless this start goes to the tray.
    pub fn start_shell(&mut self, start: Start) -> Task<Message> {
        let (sender, heard) = tokio::sync::mpsc::unbounded_channel();
        let tell: Tell = std::sync::Arc::new(move |heard| {
            sender.send(heard).ok();
        });
        let desktop = desktop::desktop();
        desktop.start(tell.clone());
        self.shell.tray = desktop.tray(tell.clone());
        self.shell.notifier = desktop.notifier(self.client.root(), tell.clone());
        self.shell.tell = Some(tell);
        self.shell.pending_link = start.link;
        self.shell.gpu_notice = start.gpu_failed;
        self.shell.fixed_size = start.fixed_size;
        self.place_as_kept();
        self.show_tray();
        // Once at start with the check on.
        if self.layout.prefs.check_for_updates {
            crate::updates::look_for_updates(self, true);
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
            // Nobody is looking, which the core takes for granted otherwise.
            self.client.dispatch(Action::WindowFocused(false));
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
                self.focus_changed(focused);
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
                    return self.forget_window(crate::app::Keep::default());
                }
                Task::none()
            }
            Event::Release => {
                desktop::desktop().release_memory();
                Task::none()
            }
            Event::DismissGpuNotice => {
                self.shell.gpu_notice = false;
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
                    crate::call::stage::place_videos(self);
                }
                Task::none()
            }
            Event::Placed(id) => Task::batch([Self::read_monitor_scale(id), Self::read_place(id)]),
            Event::At(id, at) => {
                // None while it's maximized, minimized or full screen: the
                // place it goes back to stands.
                if self.shell.window == Some(id)
                    && !self.shell.full_screen
                    && let Some(at) = at
                {
                    self.shell.position = Some(at);
                    self.keep_placement();
                }
                Task::none()
            }
            Event::MonitorScaled(id, scale) => {
                if self.shell.window == Some(id) && self.shell.monitor_scale != scale {
                    tracing::info!(?scale, window = ?self.shell.display_scale, "the monitor's scale");
                    self.shell.monitor_scale = scale;
                    crate::call::stage::place_videos(self);
                }
                Task::none()
            }
            Event::Resized(size) => {
                self.shell.frame = size;
                self.shell.sized = self.shell.sized.wrapping_add(1);
                self.fit_zoom();
                let fitted = crate::active_now::active_now_fits(self.ctx());
                crate::window_bar::fit_window(self);
                if crate::active_now::active_now_fits(self.ctx()) == fitted {
                    return crate::window_bar::read_window_state(self);
                }
                crate::active_now::active_now_sync(self);
                Task::batch([crate::window_bar::read_window_state(self), self.ask()])
            }
            Event::Zoom(key, physical) => self.zoom_key(&key, physical).unwrap_or_else(Task::none),
            Event::ZoomStep(step) => self.zoom_step(step),
        }
    }

    /// The window took the focus or lost it, or there's no window to have
    /// it. The core hears, which asks Discord for the Activity panel's feed
    /// only while someone may be looking, and counts the open conversation
    /// as read only while the window is in front.
    fn focus_changed(&mut self, focused: bool) {
        if std::mem::replace(&mut self.shell.focused, focused) != focused {
            self.client.dispatch(Action::WindowFocused(focused));
            let reads = self.reads();
            self.client.dispatch(Action::Reading(reads));
        }
    }

    /// Whether someone reads the open conversation: its newest message is in
    /// view and the window is in front. A message that comes while this is
    /// false stays unread.
    pub(crate) fn reads(&self) -> bool {
        self.timeline.at_end && self.shell.focused
    }

    /// The first window opens as the last one was left: at its size and
    /// its place, and maximized if it was. The desktop fits a size its
    /// screen has no room for.
    pub(crate) fn place_as_kept(&mut self) {
        let Some(kept) = self.layout.placement.filter(|_| !self.shell.fixed_size) else {
            return;
        };
        let zoom = self.shell.zoom;
        // What the window's settings take: the app's units.
        let size = Size::new(kept.width as f32 / zoom, kept.height as f32 / zoom);
        self.shell.size = Some(Size::new(
            size.width.clamp(MIN_SIZE.width, MOST),
            size.height.clamp(MIN_SIZE.height, MOST),
        ));
        self.shell.position = kept.position;
        self.shell.maximized = kept.maximized;
    }

    /// Keeps the window's size, its place and whether it's maximized, as
    /// any changed, for the window made again from the tray and for the
    /// next start. Maximized, it keeps the size and place it goes back to;
    /// full screen or minimized, what it had.
    pub fn keep_placement(&mut self) {
        let frame = self.shell.frame;
        if self.shell.window.is_none()
            || self.shell.full_screen
            || frame.width < MIN_SIZE.width
            || frame.height < MIN_SIZE.height
        {
            return;
        }
        let maximized = self.shell.maximized;
        if !maximized {
            self.shell.size = Some(frame);
        }
        if self.shell.fixed_size {
            return;
        }
        let factor = self.window_factor();
        let now = match self.layout.placement {
            Some(before) if maximized => crate::layout::Placement {
                maximized,
                ..before
            },
            _ => crate::layout::Placement {
                width: (frame.width * factor).round() as u32,
                height: (frame.height * factor).round() as u32,
                position: self.shell.position,
                maximized,
            },
        };
        if self.layout.placement != Some(now) {
            self.layout.placement = Some(now);
            crate::layout::store(self.client.root(), "placement", now);
        }
    }

    /// Reads where window `id`'s frame is on the desktop.
    fn read_place(id: window::Id) -> Task<Message> {
        window::run(id, |window| desktop::desktop().window_place(window))
            .map(move |at| Message::Shell(Event::At(id, at)))
    }

    /// Reads the scale of the monitor window `id` is on.
    fn read_monitor_scale(id: window::Id) -> Task<Message> {
        window::run(id, |window| desktop::desktop().monitor_scale(window))
            .map(move |scale| Message::Shell(Event::MonitorScaled(id, scale)))
    }

    /// The display's own scale: the monitor's where the desktop names it,
    /// else the window's (`zoom::display_scale`).
    fn base_scale(&self) -> f32 {
        zoom::display_scale(
            self.shell.monitor_scale,
            self.shell.display_scale.unwrap_or(1.0),
        )
    }

    /// The window's pixels to the app's units: the display's scale times
    /// the zoom, which iced draws at.
    pub fn device_scale(&self) -> f32 {
        self.base_scale() * self.shell.zoom
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
                    return Task::batch([open, crate::updates::open_about(self)]);
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
            Heard::Updates => crate::updates::updates_heard(self),
            Heard::Drag(drag) => self.dragged(drag),
        }
    }

    /// Opens the window, or brings it to the front: made again as the
    /// closed one was, at its size.
    pub fn open_window(&mut self) -> Task<Message> {
        if let Some(id) = self.shell.window {
            let token = self.shell.token.take();
            // A window full screen, by the desktop or for the call's large
            // tile, comes to the front as it is.
            let shown = if self.shell.full_screen {
                Task::none()
            } else {
                window::set_mode(id, window::Mode::Windowed)
            };
            return Task::batch([
                shown,
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
    /// its icon and app ID, by which the desktop finds its entry. With a
    /// place kept it's made hidden, and shown once it's there
    /// (`App::opened`).
    pub(crate) fn window_settings(&self) -> window::Settings {
        let mut settings = window::Settings {
            size: self.shell.size.unwrap_or(self.shell.frame),
            visible: self.shell.position.is_none(),
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
            desktop::desktop()
                .app_id()
                .clone_into(&mut settings.platform_specific.application_id);
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
        self.focus_changed(true);
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
            crate::window_bar::read_window_state(self),
            window::scale_factor(id).map(move |scale| Message::Shell(Event::Scaled(id, scale))),
            Self::read_monitor_scale(id),
        ];
        let maximize = self.shell.maximized.then(|| window::maximize(id, true));
        tasks.push(match self.shell.position {
            // Made hidden: put where the last one was, maximized on the
            // monitor that's on, and only then shown.
            Some(at) => window::run(id, move |window| {
                desktop::desktop().put_window(window, at);
            })
            .discard()
            .chain(maximize.unwrap_or_else(Task::none))
            .chain(window::set_mode(id, window::Mode::Windowed))
            .chain(Self::read_place(id)),
            None => Task::batch([maximize.unwrap_or_else(Task::none), Self::read_place(id)]),
        });
        if crate::signin::workspace(self) && self.client.state().shows_text() {
            tasks.push(iced::widget::operation::focus(crate::composer::COMPOSER));
        }
        // The call's videos show again.
        crate::call::stage::place_videos(self);
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
        self.drop_window_keeping(crate::app::Keep::default())
    }

    /// Drops the window, keeping its size and `keep` for the next.
    pub fn drop_window_keeping(&mut self, keep: crate::app::Keep) -> Task<Message> {
        let Some(id) = self.shell.window.take() else {
            return Task::none();
        };
        // Maximized, the next takes the size this one would go back to.
        if !self.shell.maximized && !self.shell.full_screen {
            self.shell.size = Some(self.shell.frame);
        }
        let forgot = self.forget_window(keep);
        tracing::info!("window closed");
        Task::batch([forgot, window::close(id)])
    }

    /// What only the window used goes with it; the core's state stays, so
    /// the next window shows it without asking again. Everything in
    /// `app::Window` is named here, so a field added there says what
    /// becomes of it. Settings close as their own button closes them,
    /// letting go of what they ran, unless `keep` keeps them; the call's
    /// popup goes with the microphone it may have open; the call's videos
    /// stop being made for tiles nobody sees.
    fn forget_window(&mut self, keep: crate::app::Keep) -> Task<Message> {
        use crate::slot::Why;
        self.focus_changed(false);
        // The next window's timeline starts at its end: where this one was
        // read is kept for it.
        self.timeline.left_at = crate::timeline::scroll::reading(self);
        // The call's large tile leaves full screen with the window it made
        // so: the next one opens as a window, the call in its place.
        self.call.full = false;
        self.shell.full_screen = false;
        crate::call::popups::close_popup(self);
        self.call.details.pointed = false;
        self.profiles.card = None;
        let mut cx = crate::ctx::cx!(self);
        let crate::app::Window {
            menu,
            picker,
            tag_card,
            settings,
            // Kept, so Settings open again where they were.
            settings_section: _,
            // Let go of last, once what Settings' close asked is done.
            pictures: _,
            icons: _,
        } = &mut self.win;
        *menu = None;
        picker.close(&mut cx, Why::Window);
        tag_card.close(&mut cx, Why::Window);
        let settings_closed = !keep.settings && settings.close(&mut cx, Why::Window);
        // What Settings' close asked is done before the pictures go, as
        // their own button does it.
        let settled = if settings_closed {
            self.settle()
        } else {
            Task::none()
        };
        crate::call::stage::place_videos(self);
        self.win.pictures.forget();
        self.win.icons.forget();
        settled
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

    /// A new message the core says to tell of: it sounds unless it's in the
    /// conversation in front (a call rings instead); while Yapfast isn't in
    /// front, the taskbar flashes and a notification shows, as the settings
    /// allow.
    fn alert(&self, alert: Alert) -> Task<Message> {
        let focused = self.shell.focused;
        let reading = focused && self.client.state().selected.as_ref() == Some(&alert.room);
        if !alert.call && !reading {
            crate::call::play(
                self,
                if alert.mention {
                    Sound::Mention
                } else {
                    Sound::Message
                },
            );
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
        crate::links::follow_link(self, &link, middle)
    }

    /// Files dragged over the window, as the desktop follows them where
    /// winit doesn't (Wayland).
    fn dragged(&mut self, drag: desktop::Drag) -> Task<Message> {
        use crate::uploads::Event;
        match drag {
            desktop::Drag::Entered => crate::uploads::update(self, Event::Hovered),
            desktop::Drag::Moved(x, y) => {
                let zoom = self.shell.zoom;
                crate::uploads::update(
                    self,
                    Event::Moved(iced::Point::new(
                        (x / f64::from(zoom)) as f32,
                        (y / f64::from(zoom)) as f32,
                    )),
                )
            }
            desktop::Drag::Left => crate::uploads::update(self, Event::Left),
            desktop::Drag::Dropped(files) => {
                let tasks: Vec<_> = files
                    .into_iter()
                    .map(|file| crate::uploads::update(self, Event::Dropped(file)))
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

    /// The zoom, which iced multiplies the window's own scale by, and the
    /// monitor's scale over the window's where they differ.
    pub fn scale_factor(&self, _: window::Id) -> f32 {
        // iced hands a touchpad's moves on in the window's pixels; asked
        // for as each view is laid out, so this follows both scales.
        crate::widgets::set_pixel_scale(self.device_scale());
        self.window_factor()
    }

    /// What iced multiplies the window's own scale by: the zoom, and the
    /// monitor's scale over the window's.
    pub fn window_factor(&self) -> f32 {
        let window = self.shell.display_scale.unwrap_or(1.0);
        self.shell.zoom * self.base_scale() / window
    }

    /// Ctrl with + or - steps the zoom, Ctrl+0 returns to 100% (the core's
    /// `zoom.rs`).
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
    /// level, and shows it a moment: also a level the step couldn't change,
    /// which says where the zoom stands.
    pub fn zoom_step(&mut self, step: zoom::Step) -> Task<Message> {
        let level = self.shell.zoom;
        // The window's size before any zoom, in the display's own pixels.
        let width = (self.shell.frame.width * level) as u32;
        let height = (self.shell.frame.height * level) as u32;
        let next = zoom::stepped(level, step, zoom::most(width, height, 1.0));
        if (next - level).abs() >= f32::EPSILON {
            self.shell.zoom = next;
            // The layout gets fewer logical pixels to fill; the next resize
            // says so exactly.
            self.shell.frame = self.shell.frame * (level / next);
            crate::window_bar::fit_window(self);
            crate::call::stage::place_videos(self);
            crate::layout::store(self.client.root(), "zoom", next);
        }
        let count = self.shell.zoom_note.map_or(0, |count| count + 1);
        self.shell.zoom_note = Some(count);
        after(ZOOM_NOTE, Message::Shell(Event::ZoomNoted(count)))
    }

    /// The first window's size is known: a level kept from a larger
    /// screen comes down to what its room allows. Not kept: the next start
    /// on the larger screen zooms as before.
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
            crate::window_bar::fit_window(self);
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
            // A window held to one display's density, as by Windows'
            // HIGHDPIAWARE setting, is told no change of scale on the others,
            // so a move or a resize may already mean another monitor.
            iced::Event::Window(window::Event::Moved(_) | window::Event::Resized(_))
                if cfg!(windows) =>
            {
                Some(Message::Shell(Event::Placed(id)))
            }
            _ => None,
        })
    }
}
