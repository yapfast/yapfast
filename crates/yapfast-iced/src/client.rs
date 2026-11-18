//! The application and its backend: actions go to the application, its
//! effects to the backend, and the backend's events back to the
//! application.

use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use tokio::sync::mpsc::UnboundedReceiver;
use yapfast_core::{
    application::{Action, AppState, Application, Effect},
    backend::Backend,
    chat::Event,
    discord_link::Progress,
    runtime::{BackgroundRuntime, BackgroundTask},
};

/// Rows reported beyond each end of those in view, so the core decodes
/// their previews before they show.
const VISIBLE_MARGIN: usize = 6;

/// The least time between two reports of the rows in view.
pub const REPORT_EVERY: Duration = Duration::from_millis(50);

pub struct Client {
    runtime: Arc<BackgroundRuntime>,
    root: PathBuf,
    pub app: Application,
    backend: Option<Backend>,
    events: Option<UnboundedReceiver<Event>>,
    last_visible: Option<Instant>,
    /// Linking a Discord account, while it runs: dropping it stops it. Its
    /// progress goes to the window once ([`Client::take_linking`]); a stop
    /// is noted for the window to let go of its listener.
    linking: Option<BackgroundTask<()>>,
    link_progress: Option<UnboundedReceiver<Progress>>,
    link_stopped: bool,
    /// Sounds the core asked for, for the window to play; new messages to
    /// tell of; a picture to put on the clipboard.
    sounds: Vec<yapfast_core::sounds::Sound>,
    alerts: Vec<yapfast_core::chat::Alert>,
    copied_picture: Option<Vec<u8>>,
    /// Others see when this account types, as Messages' settings say; off,
    /// the core's typing commands go nowhere.
    pub share_typing: bool,
    /// A backend started since the window last looked, as an account
    /// signing in starts one: the window hands the core what this device
    /// kept for it.
    pub started: bool,
    /// What the core sent its backend, for the tests to read.
    #[cfg(test)]
    pub sent: Vec<yapfast_core::chat::Command>,
}

impl Client {
    pub fn new(runtime: Arc<BackgroundRuntime>, root: PathBuf) -> Self {
        Self {
            runtime,
            root,
            app: Application::default(),
            backend: None,
            events: None,
            last_visible: None,
            linking: None,
            link_progress: None,
            started: false,
            link_stopped: false,
            sounds: Vec::new(),
            alerts: Vec::new(),
            copied_picture: None,
            share_typing: true,
            #[cfg(test)]
            sent: Vec::new(),
        }
    }

    /// The backend's runtime, for what runs beside the window.
    pub fn runtime(&self) -> &BackgroundRuntime {
        &self.runtime
    }

    /// The data folder.
    pub fn root(&self) -> &std::path::Path {
        &self.root
    }

    pub fn state(&self) -> &AppState {
        &self.app.state
    }

    /// The owner of the selected room's server, whose member row wears a
    /// crown.
    pub fn server_owner(&self) -> Option<String> {
        self.app
            .state
            .selected
            .as_deref()
            .and_then(|room| self.app.server_owner(room))
    }

    /// The backend's events once it has started; the window receives them
    /// on its executor and hands each to [`Client::apply`].
    pub fn take_events(&mut self) -> Option<UnboundedReceiver<Event>> {
        self.events.take()
    }

    pub fn dispatch(&mut self, action: Action) {
        let effects = self.app.dispatch(action);
        self.execute(effects);
    }

    /// Carries out what Settings → Plugins asked for, executing what the core
    /// says the window must do about it.
    #[cfg(feature = "plugins")]
    pub fn plugin_request(&mut self, request: yapfast_core::plugins::page::Request) {
        let effects = self.app.plugins_page_request(request);
        self.execute(effects);
    }

    pub fn apply(&mut self, event: Event) {
        let effects = self.app.apply(event);
        self.execute(effects);
    }

    /// Linking's progress, once linking has started; the window hands each
    /// step to [`Client::linking`].
    pub fn take_linking(&mut self) -> Option<UnboundedReceiver<Progress>> {
        self.link_progress.take()
    }

    /// The sounds the core asked for since this was last asked.
    pub fn take_sounds(&mut self) -> Vec<yapfast_core::sounds::Sound> {
        std::mem::take(&mut self.sounds)
    }

    /// The new messages to tell of since this was last asked.
    pub fn take_alerts(&mut self) -> Vec<yapfast_core::chat::Alert> {
        std::mem::take(&mut self.alerts)
    }

    /// The picture to put on the clipboard, since this was last asked.
    pub fn take_copied_picture(&mut self) -> Option<Vec<u8>> {
        self.copied_picture.take()
    }

    /// Whether linking stopped since this was last asked.
    pub fn link_stopped(&mut self) -> bool {
        std::mem::take(&mut self.link_stopped)
    }

    pub fn linking(&mut self, step: Progress) {
        let effects = self.app.linking(step);
        self.execute(effects);
    }

    /// The backend's events ended: it stopped.
    pub fn stopped(&mut self) {
        self.app.stopped();
        self.backend = None;
    }

    fn execute(&mut self, effects: Vec<Effect>) {
        for effect in effects {
            match effect {
                Effect::Start(start) => {
                    self.started = true;
                    self.backend = None;
                    let (backend, events) =
                        Backend::start(self.runtime.clone(), self.root.clone(), start);
                    self.backend = Some(backend);
                    self.events = Some(events);
                }
                Effect::Backend(yapfast_core::chat::Command::Typing { .. })
                    if !self.share_typing => {}
                Effect::Backend(command) => {
                    #[cfg(test)]
                    self.sent.push(command.clone());
                    let result = match &self.backend {
                        Some(backend) => backend.send(command),
                        None => Err(command),
                    };
                    if let Err(command) = result {
                        self.app.backend_failed(&command);
                    }
                }
                Effect::LinkDiscord => self.link_discord(None),
                Effect::LinkDiscordToken(token) => self.link_discord(Some(token)),
                Effect::StopDiscordLink => {
                    self.linking = None;
                    self.link_progress = None;
                    self.link_stopped = true;
                }
                // Discord's captcha, while linking.
                Effect::OpenPage(page) => crate::browser::open(&page),
                // The window plays them, as loud as Voice's settings say.
                Effect::Sound(sound) => self.sounds.push(sound),
                // The window tells of them, as its focus and the settings
                // say (`shell.rs`).
                Effect::Notify(alert) => self.alerts.push(alert),
                Effect::CopyPicture(bytes) => self.copied_picture = Some(bytes),
                // The preview's device lists and camera come with the parts
                // of the window that use them.
                _ => {}
            }
        }
    }

    /// Starts linking a Discord account, by QR code or with a pasted token,
    /// in place of any linking under way. The work runs on the backend's
    /// runtime.
    fn link_discord(&mut self, token: Option<String>) {
        let (task, progress) = yapfast_core::discord_link::start(&self.runtime, token);
        self.linking = Some(task);
        self.link_progress = Some(progress);
    }

    /// The rows from `first` to `last` of the open timeline are in view:
    /// report them with a margin, so the backend decodes their previews and
    /// lets go of distant ones, and ask for earlier history near the top,
    /// and newer near the end of a window of older messages.
    /// `reads` says whether someone reads the newest message: it's in view
    /// and the window is in front.
    /// Returns false when the last report was under [`REPORT_EVERY`] ago;
    /// the window reports again after.
    pub fn visible(&mut self, first: usize, last: usize, at_end: bool, reads: bool) -> bool {
        if self
            .last_visible
            .is_some_and(|last| last.elapsed() < REPORT_EVERY)
        {
            return false;
        }
        self.last_visible = Some(Instant::now());
        let state = &self.app.state;
        let Some(room) = state.selected.clone() else {
            return true;
        };
        let messages = &state.messages;
        if messages.is_empty() {
            return true;
        }
        let end = messages.len() - 1;
        let first = first.min(end);
        let last = last.max(first).min(end);
        let items = messages
            [first.saturating_sub(VISIBLE_MARGIN)..=(last + VISIBLE_MARGIN).min(end)]
            .iter()
            .map(|message| message.item.clone())
            .collect();
        self.dispatch(Action::VisibleMessages {
            room,
            items,
            at_end,
        });
        self.dispatch(Action::Reading(reads));
        // Near the top, the next page of history.
        let state = &self.app.state;
        if first < 5 && !state.loading_history && !state.history_start && !state.history_failed {
            self.dispatch(Action::Earlier);
        }
        let state = &self.app.state;
        if last + 5 >= end
            && state.detached
            && state.room_ready
            && !state.preview
            && !state.loading_later
            && !state.later_failed
        {
            self.dispatch(Action::Later);
        }
        true
    }

    /// Avatars and icons the rows show, which the backend downloads and
    /// decodes once; those already here are skipped.
    pub fn want_pictures(&mut self, urls: Vec<String>) {
        let urls: Vec<String> = urls
            .into_iter()
            .filter(|url| !url.is_empty() && !self.app.state.pictures.contains_key(url))
            .collect();
        if !urls.is_empty() {
            self.dispatch(Action::LoadPictures(urls));
        }
    }
}
