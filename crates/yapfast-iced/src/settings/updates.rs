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
        Some(notice(self.cx.look.theme.accent, &shown.latest))
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
                        "Update and restart",
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
                        "View release",
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
                .icon(self.cx.look.icon(Icon::Refresh, 18, plain))
                .into(),
            );
            parts.push(form::buttons(buttons));
        }
        parts
    }
}

/// The notice's button, in the accent. Its words wrap and it grows to fit
/// them, so a nightly's long version stays inside the sidebar.
fn notice<'a>(accent: iced::Color, latest: &str) -> Element<'a, Message> {
    let (rest, lit) = (
        iced::Color { a: 0.14, ..accent },
        iced::Color { a: 0.22, ..accent },
    );
    let face = iced::widget::button(
        text(format!("Update available · {latest}"))
            .size(13)
            .font(crate::fonts::BOLD)
            .color(accent)
            .wrapping(text::Wrapping::WordOrGlyph),
    )
    .padding([6, 12])
    .width(Length::Fill)
    .on_press(crate::app::settings(SettingsEvent::Open(Section::About)))
    .style(crate::controls::lit_style(Some(rest), lit, 4.0));
    container(face).padding([4, 16]).into()
}

fn update_message(event: Event) -> Message {
    Message::Updates(event)
}

#[cfg(test)]
mod tests {
    use iced_test::{Simulator, selector::Candidate};

    /// A nightly's version is too long for the sidebar on one line: the
    /// notice wraps it onto another and grows, rather than running off.
    #[test]
    fn a_long_version_wraps_inside_the_sidebar() {
        let shown = super::notice(iced::Color::WHITE, "v0.5.0-nightly.20261124.21");
        let mut ui = Simulator::with_size(
            iced::Settings::default(),
            iced::Size::new(232.0, 400.0),
            shown,
        );
        let mut words = None;
        let _ = ui.find(|candidate: Candidate<'_>| {
            if let Candidate::Text {
                content, bounds, ..
            } = candidate
                && content.starts_with("Update available")
            {
                words = Some(bounds);
            }
            None::<()>
        });
        let words = words.expect("the notice's words");
        assert!(words.x + words.width <= 232.0, "inside the sidebar");
        assert!(words.height > 13.0 * 1.5, "on more than one line");
    }
}
