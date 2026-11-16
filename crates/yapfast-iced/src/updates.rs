//! Updates, as the Slint client's `src/ui/updates.rs` drives them:
//! the desktop's `Updates` looks at the releases and installs one
//! (`desktop.rs`); this asks for a look when the check is turned on, on
//! Check now, once at start with the check on and every six hours after,
//! shows what it found, and tells of a newer release once in a
//! notification that opens About. Nothing installs unless Update and
//! restart is pressed; once the update is ready, the window ends so `main`
//! can finish it. It runs with Settings closed; About only shows it
//! (`settings/updates.rs`).

use std::time::Duration;

use iced::Task;

use crate::{
    app::{App, Message},
    desktop::{self, ABOUT, UpdateChoices, UpdateStatus, UpdatesShown},
    layout::Layout,
    settings::Section,
};

/// How long between looks while Yapfast runs. GitHub allows an address 60
/// requests an hour without an account.
pub const EVERY: Duration = Duration::from_secs(6 * 60 * 60);

#[derive(Clone, Debug)]
pub enum Event {
    /// Six hours went by.
    Due,
    CheckNow,
    Install,
    Cancel,
    Skip,
    /// The offered release's page, in the browser.
    View,
}

/// What the person chose of updates: nightlies or not, and the release
/// skipped.
pub fn choices(layout: &Layout) -> UpdateChoices {
    let prefs = &layout.prefs;
    UpdateChoices {
        nightlies: prefs.update_nightlies,
        skipped: prefs.skipped_update.clone(),
    }
}

/// What About shows of the updates, where the desktop offers them.
pub fn shown(layout: &Layout) -> Option<UpdatesShown> {
    Some(desktop::desktop().updates()?.shown(&choices(layout)))
}

impl App {
    pub fn update_choices(&self) -> UpdateChoices {
        choices(&self.layout)
    }

    /// Looks at the releases; `announce` tells of a newer one in a
    /// notification, as a look nobody asked for does.
    pub fn look_for_updates(&mut self, announce: bool) {
        let Some(updates) = desktop::desktop().updates() else {
            return;
        };
        self.announce_updates |= announce;
        updates.look();
    }

    /// The look or the update went further: a release to tell of, or an
    /// update ready to finish.
    pub fn updates_heard(&mut self) -> Task<Message> {
        let Some(updates) = desktop::desktop().updates() else {
            return Task::none();
        };
        if updates.ready() {
            tracing::info!("the update is ready; ending to finish it");
            crate::exit_for(crate::Exit::Update);
            return self.quit();
        }
        let choices = self.update_choices();
        if self.announce_updates && updates.shown(&choices).status != UpdateStatus::Checking {
            self.announce_updates = false;
            self.announce_update(updates.offered(&choices));
        }
        Task::none()
    }

    /// Tells of the newest release in a notification, once for each release,
    /// unless notifications are off or the person skipped it.
    fn announce_update(&mut self, offered: Option<desktop::Offered>) {
        let Some(offered) = offered.filter(|offered| !offered.skipped) else {
            return;
        };
        let prefs = &mut self.layout.prefs;
        if prefs.announced_update.as_deref() == Some(offered.tag.as_str()) {
            return;
        }
        prefs.announced_update = Some(offered.tag.clone());
        crate::layout::store(self.client.root(), "announced_update", &offered.tag);
        if !self.layout.prefs.notifications.unwrap_or(true) {
            return;
        }
        if let Some(notifier) = &self.shell.notifier {
            notifier.show(
                yapfast_core::chat::Alert {
                    room: ABOUT.into(),
                    title: format!("Yapfast {} is available", offered.tag),
                    body: "Click to see what's new.".into(),
                    picture: None,
                    call: false,
                    mention: false,
                },
                true,
            );
        }
    }

    pub fn updates_event(&mut self, event: Event) -> Task<Message> {
        let Some(updates) = desktop::desktop().updates() else {
            return Task::none();
        };
        let choices = self.update_choices();
        match event {
            Event::Due => {
                if self.layout.prefs.check_for_updates {
                    self.look_for_updates(true);
                }
            }
            Event::CheckNow => self.look_for_updates(false),
            Event::Install => updates.install(&choices),
            Event::Cancel => updates.cancel(),
            Event::Skip => {
                if let Some(offered) = updates.offered(&choices).filter(|offered| !offered.skipped)
                {
                    crate::layout::store(self.client.root(), "skipped_update", &offered.tag);
                    self.layout.prefs.skipped_update = Some(offered.tag);
                }
            }
            Event::View => {
                if let Some(offered) = updates.offered(&choices) {
                    crate::browser::open(&offered.page);
                }
            }
        }
        Task::none()
    }

    /// An update downloads: About shows how far, a few times a second.
    pub fn updates_installing(&self) -> bool {
        self.win.settings.is_open()
            && desktop::desktop()
                .updates()
                .is_some_and(|updates| updates.installing())
    }

    /// Settings on About, as the release's notification opens it.
    pub fn open_about(&mut self) -> Task<Message> {
        self.open_settings(Section::About)
    }
}
