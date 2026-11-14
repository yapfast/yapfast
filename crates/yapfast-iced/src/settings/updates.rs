//! About's updates, as the Slint client's `src/ui/updates.rs` drives them:
//! the desktop's `Updates` looks at the releases and installs one
//! (`desktop.rs`); this asks for a look when the check is turned on, on
//! Check now, once at start with the check on and every six hours after,
//! shows what it found, and tells of a newer release once in a
//! notification that opens About. Nothing installs unless Update and
//! restart is pressed; once the update is ready, the window ends so `main`
//! can finish it.

use std::time::Duration;

use iced::{
    Element, Length, Task,
    widget::{column, container, row, scrollable, text},
};

use super::{Event as SettingsEvent, Section};
use crate::{
    app::{App, Message},
    controls::{Kind, action, bar},
    desktop::{self, ABOUT, UpdateChoices, UpdateStatus, UpdatesShown},
    form,
    pictures::Icon,
    style::scrollbar,
};

/// How long between looks while Yapfast runs. GitHub allows an address 60
/// requests an hour without an account.
pub const EVERY: Duration = Duration::from_secs(6 * 60 * 60);

/// The tallest the release's notes stand before they scroll.
const NOTES_MOST: f32 = 220.0;

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

impl App {
    fn update_choices(&self) -> UpdateChoices {
        let prefs = &self.layout.prefs;
        UpdateChoices {
            nightlies: prefs.update_nightlies,
            skipped: prefs.skipped_update.clone(),
        }
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

    /// What About shows of the releases, where the build can look.
    pub fn updates_shown(&self) -> Option<UpdatesShown> {
        Some(desktop::desktop().updates()?.shown(&self.update_choices()))
    }

    /// An update downloads: About shows how far, a few times a second.
    pub fn updates_installing(&self) -> bool {
        self.settings.is_some() && self.updates_shown().is_some_and(|shown| shown.installing)
    }

    /// Settings on About, as the release's notification opens it.
    pub fn open_about(&mut self) -> Task<Message> {
        self.open_settings(Section::About)
    }

    /// Under Settings' sections while a newer release is out, as the Slint
    /// client's `UpdateNotice`: a click goes to About.
    pub fn update_notice(&self) -> Option<Element<'_, Message>> {
        let shown = self.updates_shown()?;
        if shown.status != UpdateStatus::Available {
            return None;
        }
        let theme = &self.theme;
        let accent = theme.accent;
        let (rest, lit) = (
            iced::Color { a: 0.14, ..accent },
            iced::Color { a: 0.22, ..accent },
        );
        let face = iced::widget::button(
            text(format!("Update available · {}", shown.latest))
                .size(13)
                .font(crate::fonts::BOLD)
                .color(accent)
                .wrapping(text::Wrapping::None),
        )
        .padding([0, 12])
        .width(Length::Fill)
        .height(28)
        .on_press(Message::Settings(SettingsEvent::Open(Section::About)))
        .style(crate::controls::lit_style(Some(rest), lit, 4.0));
        Some(
            container(iced::widget::center_y(face))
                .padding([4, 16])
                .height(36)
                .into(),
        )
    }

    /// About's UPDATES: what the last look found, the update's progress,
    /// the release's notes and its buttons, as the Slint client's
    /// `AboutSettings`.
    pub fn updates_part(&self) -> Vec<Element<'_, Message>> {
        let theme = &self.theme;
        let prefs = &self.layout.prefs;
        let shown = self.updates_shown().unwrap_or_default();
        let mut parts: Vec<Element<'_, Message>> = vec![form::heading(theme, "UPDATES")];
        if !shown.manager.is_empty() {
            parts.push(form::note(
                theme,
                format!(
                    "You installed Yapfast with {}, so update it there.",
                    shown.manager
                ),
            ));
        }
        let status = match shown.status {
            UpdateStatus::Checking => "Checking for updates…".to_owned(),
            UpdateStatus::Current => "You're up to date.".to_owned(),
            UpdateStatus::Available => format!("Yapfast {} is available.", shown.latest),
            UpdateStatus::Skipped => format!(
                "You skipped {}. We'll tell you about the next one.",
                shown.latest
            ),
            UpdateStatus::Failed => format!("Couldn't check for updates: {}", shown.error),
            UpdateStatus::Source => {
                "You're running a build from source, so there's nothing to update.".to_owned()
            }
            UpdateStatus::Idle if prefs.check_for_updates => String::new(),
            UpdateStatus::Idle => "Update checks are turned off.".to_owned(),
        };
        if !status.is_empty() {
            let ink = match shown.status {
                UpdateStatus::Available => theme.accent,
                UpdateStatus::Failed => theme.danger_text,
                _ => theme.muted,
            };
            parts.push(text(status).size(14).color(ink).into());
        }
        if shown.installing {
            let fraction = shown.install_progress.clamp(0.0, 1.0);
            let sidebar = theme.sidebar;
            let track = container(
                row![
                    bar(
                        theme.accent,
                        Length::FillPortion((fraction * 1000.0) as u16),
                        6
                    ),
                    iced::widget::Space::new()
                        .width(Length::FillPortion(((1.0 - fraction) * 1000.0) as u16))
                ]
                .height(6),
            )
            .width(Length::Fill)
            .style(move |_| crate::style::rounded(sidebar, 3.0));
            let mut install = column![
                text(shown.install_text.clone()).size(14).color(theme.text),
                track
            ]
            .spacing(8);
            if shown.can_cancel {
                install = install.push(action(
                    theme,
                    "Cancel",
                    Kind::Plain,
                    Some(update_message(Event::Cancel)),
                ));
            }
            parts.push(install.into());
        } else {
            if !shown.install_error.is_empty() {
                parts.push(
                    text(format!("Couldn't update: {}", shown.install_error))
                        .size(14)
                        .color(theme.danger_text)
                        .into(),
                );
            }
            if shown.status == UpdateStatus::Available && !shown.install_note.is_empty() {
                parts.push(form::note(theme, shown.install_note.clone()));
            }
        }
        let offered = matches!(
            shown.status,
            UpdateStatus::Available | UpdateStatus::Skipped
        );
        if offered && !shown.notes.is_empty() {
            let sidebar = theme.sidebar;
            parts.push(
                column![
                    form::heading(
                        theme,
                        format!("WHAT'S NEW IN {}", shown.latest.to_uppercase())
                    ),
                    container(
                        scrollable(
                            container(text(shown.notes.clone()).size(13).color(theme.secondary))
                                .padding(12)
                        )
                        .style(scrollbar(theme))
                    )
                    .max_height(NOTES_MOST)
                    .width(Length::Fill)
                    .style(move |_| crate::style::rounded(sidebar, 8.0))
                ]
                .spacing(8)
                .into(),
            );
        }
        if !shown.installing {
            let mut buttons = Vec::new();
            let available = shown.status == UpdateStatus::Available;
            let on_accent = crate::theme::rgb(theme.on_accent);
            let plain = crate::theme::rgb(theme.faint);
            if available && shown.can_install {
                buttons.push(
                    action(
                        theme,
                        "Update and restart",
                        Kind::Primary,
                        Some(update_message(Event::Install)),
                    )
                    .icon(self.icon(Icon::Download, 18, on_accent))
                    .into(),
                );
            }
            if offered {
                let primary = available && !shown.can_install;
                buttons.push(
                    action(
                        theme,
                        "View release",
                        if primary { Kind::Primary } else { Kind::Plain },
                        Some(update_message(Event::View)),
                    )
                    .icon(self.icon(Icon::Link, 18, if primary { on_accent } else { plain }))
                    .into(),
                );
            }
            if available {
                buttons.push(
                    action(
                        theme,
                        "Skip this version",
                        Kind::Plain,
                        Some(update_message(Event::Skip)),
                    )
                    .into(),
                );
            }
            buttons.push(
                action(
                    theme,
                    "Check now",
                    Kind::Plain,
                    (shown.status != UpdateStatus::Checking)
                        .then(|| update_message(Event::CheckNow)),
                )
                .icon(self.icon(Icon::Refresh, 18, plain))
                .into(),
            );
            parts.push(form::buttons(buttons));
        }
        parts
    }
}

fn update_message(event: Event) -> Message {
    Message::Settings(SettingsEvent::Updates(event))
}
