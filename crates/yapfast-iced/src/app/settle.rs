//! The root's side of the screen contract (`docs/client/structure.md`): the
//! screens' slots opened, updated and closed through a [`Cx`], and the one
//! pass after each message that does what their updates asked
//! ([`App::settle`]).

use iced::Task;

use super::{App, Message};
use crate::{
    ctx::{Ctx, Focus, Pref, Request, Screen, cx},
    settings::{Event, Section, Settings},
    slot::Why,
};

/// The most passes `settle` makes.
const PASSES: usize = 4;

impl App {
    /// The view's context: the look, the pictures, the core's state and
    /// the root's.
    pub fn ctx(&self) -> Ctx<'_> {
        Ctx {
            look: self.look(),
            pictures: &self.win.pictures,
            app: &self.client.app,
            state: self.client.state(),
            plays: crate::pictures::plays(self),
            layout: &self.layout,
            themes: &self.themes,
            account: &self.account,
            build: &self.build,
            video: &self.video,
            scale: self.device_scale(),
            keybinds: &self.keybinds,
            own_title_bar: self.shell.own_title_bar,
            gpu_notice: self.shell.gpu_notice,
            tray: self
                .shell
                .tray
                .as_ref()
                .is_some_and(|tray| tray.available()),
            sign_in: &self.sign_in,
            lanes: crate::timeline::lanes(self),
            link_spot: &self.link_spot,
            profiles: &self.profiles,
            minute: self.minute,
            pointed: &self.pointed,
            pressed: &self.pressed,
            menu: self.win.menu.as_ref(),
            picker: self.win.picker.get(),
            above: self.shell.frame.height - self.window.height,
            composer: &self.composer,
            playback: &self.playback,
            modifiers: self.modifiers,
            in_server: self.in_server,
            post_author: self.post_author.as_deref(),
            header: &self.header,
            drafts: &self.drafts,
            forum_post: &self.forum_post,
            uploads: &self.uploads,
            searching: &self.searching,
            call: &self.call,
            header_spot: &self.header_spot,
            friends: &self.friends,
            active_now: &self.active_now,
            account_pointed: self.account_pointed,
            browse_filter: &self.sidebar.browse_filter,
            aside_width: crate::thread::aside_width(self),
            open: crate::ctx::Open {
                members: crate::header::members_shown(self),
                aside: crate::thread::aside_shown(self),
            },
        }
    }

    /// Opens Settings on a section, or shows the section in Settings
    /// already open.
    pub fn open_settings(&mut self, section: Section) -> Task<Message> {
        let mut cx = cx!(self);
        let settings = self.win.settings.open(|| Settings::open(&mut cx));
        let task = settings.show(section, &mut cx);
        self.win.settings_section = section;
        Task::batch([task, self.settle()])
    }

    pub fn close_settings(&mut self) -> Task<Message> {
        self.close_settings_for(Why::User)
    }

    pub fn close_settings_for(&mut self, why: Why) -> Task<Message> {
        let mut cx = cx!(self);
        if !self.win.settings.close(&mut cx, why) {
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
        if let Event::Reopen = event {
            return self.open_settings(self.win.settings_section);
        }
        let mut cx = cx!(self);
        let Some(settings) = self.win.settings.get_mut() else {
            return Task::none();
        };
        let task = settings.update(event, &mut cx);
        self.win.settings_section = settings.section();
        Task::batch([task, self.settle()])
    }

    pub fn settings_escape(&mut self) -> Task<Message> {
        let mut cx = cx!(self);
        if let Some(settings) = self.win.settings.get_mut() {
            settings.escape(&mut cx);
        }
        self.settle()
    }

    /// Whether a keybind is being recorded, so the keys go to it.
    pub fn recording_keybind(&self) -> bool {
        self.win.settings.get().is_some_and(Settings::recording)
    }

    /// The camera Voice's settings try, for the window's video sources.
    pub fn camera_test(&self) -> Option<&yapfast_core::picture::Feed> {
        self.win.settings.get()?.camera_test().0
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
        // A sync may close a screen the core closed, whose close asks
        // more; a few passes are enough for any chain the screens make.
        for _ in 0..PASSES {
            if self.requests.is_empty() && !self.dirty {
                break;
            }
            tasks.push(self.settle_pass());
        }
        let videos = self
            .win
            .settings
            .get()
            .map(|settings| settings.camera_test())
            .map(|(feed, tries)| (feed.is_some(), tries))
            .unwrap_or_default();
        if videos != self.videos_for {
            self.videos_for = videos;
            crate::call::stage::place_videos(self);
        }
        // Keybinds rest while one is recorded, so pressing a toggle's keys
        // to record them doesn't toggle it.
        let recording = self.recording_keybind();
        if recording != self.keybinds.paused {
            self.keybinds.paused = recording;
            crate::desktop::desktop().pause_keybinds(recording);
        }
        Task::batch(tasks)
    }

    /// One pass of [`App::settle`]: the requests, the preferences heard,
    /// the focus, then the sync.
    fn settle_pass(&mut self) -> Task<Message> {
        let mut tasks = Vec::new();
        let mut focus = None;
        let mut heard: Vec<Pref> = Vec::new();
        // A close may ask more of its own, so this runs until none is
        // left.
        while !self.requests.is_empty() {
            for request in std::mem::take(&mut self.requests) {
                match request {
                    Request::Close(screen) => self.close_screen(screen, Why::User),
                    Request::CloseMenus => self.win.menu = None,
                    Request::ClosePopups => {
                        crate::call::popups::close_popup(self);
                        // They cover the signal, whose touch says nothing
                        // as it's covered.
                        self.call.details.pointed = false;
                    }
                    Request::Focus(to) => {
                        self.focus_asked = true;
                        focus = Some(to);
                    }
                    Request::Changed(pref) => {
                        if !heard.contains(&pref) {
                            heard.push(pref);
                        }
                    }
                    Request::TitleBar(system) => {
                        tasks.push(crate::window_bar::choose_title_bar(self, system));
                    }
                    Request::LookForUpdates(announce) => {
                        crate::updates::look_for_updates(self, announce);
                    }
                    Request::Ask => tasks.push(self.ask()),
                    Request::KeepEmoji { near, first } => {
                        crate::picker::keep_emoji(self, near, first);
                    }
                    Request::Restart => {
                        crate::exit_for(crate::Exit::Restart);
                        tasks.push(self.quit());
                    }
                    Request::PlaceEmoji { target, key } => {
                        tasks.push(crate::picker::place_emoji(self, target, key));
                    }
                    Request::PersonMenu { user, location } => {
                        let at = self.pressed.get();
                        tasks.push(crate::person_menu::open_person_menu(
                            self, &user, at, location,
                        ));
                    }
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
            Some(Focus::Default) => tasks.push(crate::composer::refocus(self)),
            Some(Focus::Nobody) => tasks.push(iced::widget::operation::focus(super::NOBODY)),
            Some(Focus::Id(id)) => tasks.push(iced::widget::operation::focus(id)),
            None => {}
        }
        if std::mem::take(&mut self.dirty) {
            tasks.push(self.sync());
        }
        Task::batch(tasks)
    }

    /// Where the keys go stays explicit (`Request::Focus`), since about a
    /// dozen updates refocus with nothing opened or closed and some
    /// layers keep the keys where they are on purpose. This is the
    /// derived rule's backstop: an update that changed what's on top and
    /// asked nothing through `Request::Focus` is logged at debug, for
    /// finding one that leaves the keys in a field that's gone. Updates
    /// not yet on the contract focus with a task of their own and show
    /// here too.
    pub(super) fn focus_backstop(&self, top: Option<super::layers::Layer>) {
        let now = self.layers().next();
        if now != top && !self.focus_asked {
            tracing::debug!(from = ?top, to = ?now, "the top layer changed with no focus asked");
        }
    }

    /// Closes a screen the window opened, running its close.
    fn close_screen(&mut self, screen: Screen, why: Why) {
        let mut cx = cx!(self);
        match screen {
            Screen::Settings => {
                self.win.settings.close(&mut cx, why);
            }
            Screen::Picker => {
                self.win.picker.close(&mut cx, why);
            }
            Screen::TagCard => {
                self.win.tag_card.close(&mut cx, why);
            }
        }
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
            Pref::Keybinds => task = crate::keybinds::watch_keybinds(self),
            Pref::Idle => crate::away::idle_setting_changed(self),
            Pref::Theme => {
                let shown = self
                    .win
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
        let cx = cx!(self);
        if let Some(settings) = self.win.settings.get_mut() {
            settings.changed(pref, &cx);
        }
        task
    }
}
