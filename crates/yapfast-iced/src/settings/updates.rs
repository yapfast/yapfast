//! What About shows of the updates the root looks for (`crate::updates`):
//! the notice under Settings' sections and About's UPDATES.

use crate::widgets::scroll_view;

use iced::{
    Element, Length,
    widget::{column, container, row, text},
};

use super::{Event as SettingsEvent, Page, Section};
use crate::{
    app::Message,
    controls::{Kind, action, bar},
    desktop::UpdateStatus,
    form,
    pictures::Icon,
    style::scrollbar,
    updates::Event,
};
use yapfast_core::i18n::tr;

/// The tallest the release's notes stand before they scroll.
const NOTES_MOST: f32 = 220.0;

impl<'a> Page<'a> {
    /// Under Settings' sections while a newer release is out: a click goes
    /// to About.
    pub(super) fn update_notice(self) -> Option<Element<'a, Message>> {
        let shown = crate::updates::shown(self.cx.layout)?;
        if shown.status != UpdateStatus::Available {
            return None;
        }
        let theme = self.cx.look.theme;
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
    /// the release's notes and its buttons.
    pub(super) fn updates_part(self) -> Vec<Element<'a, Message>> {
        let theme = self.cx.look.theme;
        let prefs = &self.cx.layout.prefs;
        let shown = crate::updates::shown(self.cx.layout).unwrap_or_default();
        let mut parts: Vec<Element<'a, Message>> = vec![form::heading(theme, "UPDATES")];
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
            UpdateStatus::Checking => tr("Checking for updates…").to_owned(),
            UpdateStatus::Current => tr("You're up to date.").to_owned(),
            UpdateStatus::Available => format!("Yapfast {} is available.", shown.latest),
            UpdateStatus::Skipped => format!(
                "You skipped {}. We'll tell you about the next one.",
                shown.latest
            ),
            UpdateStatus::Failed => format!("Couldn't check for updates: {}", shown.error),
            UpdateStatus::Source => {
                tr("You're running a build from source, so there's nothing to update.").to_owned()
            }
            UpdateStatus::Idle if prefs.check_for_updates => String::new(),
            UpdateStatus::Idle => tr("Update checks are turned off.").to_owned(),
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
                    tr("Cancel"),
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
                        scroll_view(
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
                        tr("Update and restart"),
                        Kind::Primary,
                        Some(update_message(Event::Install)),
                    )
                    .icon(self.cx.look.icon(Icon::Download, 18, on_accent))
                    .into(),
                );
            }
            if offered {
                let primary = available && !shown.can_install;
                buttons.push(
                    action(
                        theme,
                        tr("View release"),
                        if primary { Kind::Primary } else { Kind::Plain },
                        Some(update_message(Event::View)),
                    )
                    .icon(self.cx.look.icon(
                        Icon::Link,
                        18,
                        if primary { on_accent } else { plain },
                    ))
                    .into(),
                );
            }
            if available {
                buttons.push(
                    action(
                        theme,
                        tr("Skip this version"),
                        Kind::Plain,
                        Some(update_message(Event::Skip)),
                    )
                    .into(),
                );
            }
            buttons.push(
                action(
                    theme,
                    tr("Check now"),
                    Kind::Plain,
                    (shown.status != UpdateStatus::Checking)
                        .then(|| update_message(Event::CheckNow)),
                )
                .icon(self.cx.look.icon(Icon::Refresh, 18, plain))
                .into(),
            );
            parts.push(form::buttons(buttons));
        }
        parts
    }
}

fn update_message(event: Event) -> Message {
    Message::Updates(event)
}
