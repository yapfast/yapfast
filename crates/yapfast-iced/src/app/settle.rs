//! The root's side of the screen contract (`docs/iced-client.md`): the
//! screens' slots opened, updated and closed through a [`Cx`], and the one
//! pass after each message that does what their updates asked
//! ([`App::settle`]).

use iced::Task;

use super::{App, Message};
use crate::{
    ctx::{Ctx, Focus, Pref, Request, cx},
    settings::{Event, Section, Settings},
    slot::Why,
};

impl App {
    /// The view's context: the look, the pictures, the core's state and
    /// the root's.
    pub fn ctx(&self) -> Ctx<'_> {
        Ctx {
            look: self.look(),
            pictures: &self.pictures,
            app: &self.client.app,
            state: self.client.state(),
            plays: self.plays(),
            layout: &self.layout,
            themes: &self.themes,
            account: &self.account,
            build: &self.build,
            video: &self.video,
            scale: self.device_scale(),
            keybinds: &self.keybinds,
            own_title_bar: self.shell.own_title_bar,
            tray: self
                .shell
                .tray
                .as_ref()
                .is_some_and(|tray| tray.available()),
            sign_in: &self.sign_in,
        }
    }

    /// Opens Settings on a section, or shows the section in Settings
    /// already open.
    pub fn open_settings(&mut self, section: Section) -> Task<Message> {
        let mut cx = cx!(self);
        let settings = self.settings.open(|| Settings::open(&mut cx));
        let task = settings.show(section, &mut cx);
        Task::batch([task, self.settle()])
    }

    pub fn close_settings(&mut self) -> Task<Message> {
        self.close_settings_for(Why::User)
    }

    pub fn close_settings_for(&mut self, why: Why) -> Task<Message> {
        let mut cx = cx!(self);
        if !self.settings.close(&mut cx, why) {
            return Task::none();
        }
        self.settle()
    }

    pub fn settings_event(&mut self, event: Event) -> Task<Message> {
        // Open opens them where they're closed, as the update's notice and
        // a profile's Game Activity do.
        if let Event::Open(section) = event {
            return self.open_settings(section);
        }
        let mut cx = cx!(self);
        let Some(settings) = self.settings.get_mut() else {
            return Task::none();
        };
        let task = settings.update(event, &mut cx);
        Task::batch([task, self.settle()])
    }

    pub fn settings_escape(&mut self) -> Task<Message> {
        let mut cx = cx!(self);
        if let Some(settings) = self.settings.get_mut() {
            settings.escape(&mut cx);
        }
        self.settle()
    }

    /// Whether a keybind is being recorded, so the keys go to it.
    pub fn recording_keybind(&self) -> bool {
        self.settings.get().is_some_and(Settings::recording)
    }

    /// The camera Voice's settings try, for the window's video sources.
    pub fn camera_test(&self) -> Option<&yapfast_core::picture::Feed> {
        self.settings.get()?.camera_test().0
    }

    /// After each message: what the screens asked is done (a screen
    /// closed, menus and popups closed, a preference that others show
    /// heard, the title bar, a look at the releases), the window shows the
    /// theme chosen or previewed, the keys go where they were sent, the
    /// window syncs if anything changed, the videos are placed again if
    /// the camera being tried changed, and keybinds rest while one is
    /// recorded. It touches nothing while nothing was asked.
    pub fn settle(&mut self) -> Task<Message> {
        let mut tasks = Vec::new();
        let mut focus = None;
        let mut heard: Vec<Pref> = Vec::new();
        // A close may ask more of its own, so this runs until none is
        // left.
        while !self.requests.is_empty() {
            for request in std::mem::take(&mut self.requests) {
                match request {
                    Request::CloseSelf => {
                        let mut cx = cx!(self);
                        self.settings.close(&mut cx, Why::User);
                    }
                    Request::CloseMenus => self.menu = None,
                    Request::ClosePopups => {
                        self.close_popup();
                        // They cover the signal, whose touch says nothing
                        // as it's covered.
                        self.call.details.pointed = false;
                    }
                    Request::Focus(to) => focus = Some(to),
                    Request::Changed(pref) => {
                        if !heard.contains(&pref) {
                            heard.push(pref);
                        }
                    }
                    Request::TitleBar(system) => tasks.push(self.choose_title_bar(system)),
                    Request::LookForUpdates(announce) => self.look_for_updates(announce),
                }
            }
        }
        for pref in heard {
            tasks.push(self.changed(pref));
        }
        // The keys go where they were sent before the sync, so a field
        // that what the sync opens focuses (the status editor's, as My
        // Account's Custom Status closes Settings) takes them after.
        match focus {
            Some(Focus::Default) => tasks.push(self.refocus()),
            Some(Focus::Nobody) => tasks.push(iced::widget::operation::focus(super::NOBODY)),
            None => {}
        }
        if std::mem::take(&mut self.dirty) {
            tasks.push(self.sync());
        }
        let videos = self
            .settings
            .get()
            .map(|settings| settings.camera_test())
            .map(|(feed, tries)| (feed.is_some(), tries))
            .unwrap_or_default();
        if videos != self.videos_for {
            self.videos_for = videos;
            self.place_videos();
        }
        // Keybinds rest while one is recorded, so pressing a toggle's keys
        // to record them doesn't toggle it.
        let recording = self.recording_keybind();
        if recording != self.keybinds.paused {
            self.keybinds.paused = recording;
            crate::pause_keybinds(recording);
        }
        Task::batch(tasks)
    }

    /// A preference others show changed: the root does its part, then each
    /// screen open hears of it.
    fn changed(&mut self, pref: Pref) -> Task<Message> {
        let mut task = Task::none();
        match pref {
            Pref::Clock | Pref::Rows => {
                let none = super::Messages::default();
                self.measure(&none, true);
                self.dirty = true;
            }
            Pref::Roster => self.show_roster(false),
            Pref::Keybinds => task = self.watch_keybinds(),
            Pref::Idle => self.idle_setting_changed(),
            Pref::Theme => {
                let shown = self
                    .settings
                    .get()
                    .and_then(Settings::preview)
                    .cloned()
                    .unwrap_or_else(|| self.themes.shown());
                self.show_theme(shown);
            }
            Pref::Noise => {
                let suppressor = self.layout.voice.noise_suppression;
                if suppressor != yapfast_core::noise::Suppressor::Off {
                    self.call.noise_last = suppressor;
                }
            }
        }
        let mut cx = cx!(self);
        if let Some(settings) = self.settings.get_mut() {
            settings.changed(pref, &mut cx);
        }
        task
    }
}
