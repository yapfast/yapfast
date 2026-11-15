//! Settings, as the Slint client's `SettingsScreen` with its Discord look
//! (`ui/settings.slint`, `ui/yapfast/settings-nav.slint`): a card over the
//! dimmed window, the sections down its left under the account and a
//! search, which build this is at their foot, and the chosen section's
//! page on the right under its title and the close button. Escape, the
//! close button or a click outside closes it.
//!
//! Its state lives only while it's open ([`Settings`], boxed in the app):
//! closing it lets go of the theme editor, the camera being tried, the
//! microphone's gate meter and whatever the pages made. What the pages
//! change is kept at once in `layout.json`, under the keys the Slint client
//! keeps them by, and handed to the core as the Slint client hands it.
//! The pages are built of the form kit (`form.rs`); each page's module
//! holds its view and what its controls do.

mod appearance;
mod pages;
pub mod updates;

pub use pages::message_size;
pub mod voice;

use iced::{
    Alignment, Color, Element, Length, Task,
    widget::{Space, button, column, container, row, scrollable, text},
};
use yapfast_core::application::Action;
use yapfast_core::new_tags::{self, Page};

use crate::{
    app::{App, Message},
    controls::{self, bar},
    fonts::BOLD,
    form,
    pictures::Icon,
    style::scrollbar,
};

use appearance::Editor;
#[cfg(test)]
pub use appearance::Event as ThemeEvent;

/// What a theme editor's change sends.
#[cfg(test)]
pub fn appearance_event(event: ThemeEvent) -> Message {
    Message::Settings(Event::Theme(event))
}

/// How far the card sits in from the window's edges, and the sections'
/// column's width.
const MARGIN: f32 = 72.0;
const NAV: f32 = 266.0;
/// A section's row in the column, the page's header over it.
const NAV_ROW: f32 = 40.0;
const HEADER: f32 = 48.0;
/// The search's field, and the page's scrollable.
pub const SEARCH: &str = "settings-search";
const PAGE: &str = "settings-page";

/// A section, by the number the Slint client's `settings-section` gives it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    Account,
    Voice,
    Messages,
    Notifications,
    Discord,
    Sounds,
    Advanced,
    Appearance,
    Games,
    About,
}

impl Section {
    /// The page as NEW tags name it, for those with tagged settings.
    fn page(self) -> Option<Page> {
        match self {
            Section::About => Some(Page::About),
            _ => None,
        }
    }

    /// The page's title in the header.
    fn title(self) -> &'static str {
        match self {
            Section::Account => "My Account",
            Section::Voice => "Voice & Video",
            Section::Messages => "Messages",
            Section::Notifications => "Notifications",
            Section::Discord => "Discord",
            Section::Sounds => "Sounds",
            Section::Advanced => "Advanced",
            Section::Appearance => "Appearance",
            Section::Games => "Game Activity",
            Section::About => "About",
        }
    }

    fn icon(self) -> Icon {
        match self {
            Section::Account => Icon::User,
            Section::Voice => Icon::Mic,
            Section::Messages => Icon::MessageCircle,
            Section::Notifications => Icon::Bell,
            Section::Discord => Icon::Discord,
            Section::Sounds => Icon::Volume,
            Section::Advanced => Icon::Settings,
            Section::Appearance => Icon::Palette,
            Section::Games => Icon::Play,
            Section::About => Icon::Help,
        }
    }
}

/// The column's groups and their sections, as Discord's settings group
/// them.
const GROUPS: [(&str, &[Section]); 3] = [
    ("Account", &[Section::Account]),
    (
        "Experience",
        &[
            Section::Voice,
            Section::Messages,
            Section::Notifications,
            Section::Sounds,
            Section::Appearance,
        ],
    ),
    (
        "App Settings",
        &[
            Section::Discord,
            Section::Games,
            Section::Advanced,
            Section::About,
        ],
    ),
];

/// Which dropdown is open, at most one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Drop {
    Microphone,
    Speaker,
    Camera,
    Entrance,
    Start,
}

/// Settings while open.
#[derive(Default)]
pub struct Settings {
    pub section: Option<Section>,
    pub query: String,
    pub open: Option<Drop>,
    /// The version was copied a moment ago, which the stamp and About say.
    pub copied: bool,
    /// Appearance's theme editor while it's open.
    pub editor: Option<Editor>,
    /// Voice's state while it shows: the push-to-talk key being recorded,
    /// the camera being tried, the gate's meter, the entrance sounds.
    pub voice: voice::Voice,
    /// Game Activity's draft of a game.
    pub game: pages::GameDraft,
    /// How many people blocked the Discord page lists.
    pub blocked_shown: usize,
    /// What the desktop says of starting at sign-in and Discord's links,
    /// read as Settings opens and after each change.
    pub platform: Platform,
}

/// Starting at sign-in and opening Discord's links here, where the build and
/// the desktop can: none where they can't.
#[derive(Clone, Copy, Debug, Default)]
pub struct Platform {
    pub autostart: Option<bool>,
    pub discord_links: Option<bool>,
}

impl Settings {
    pub fn section(&self) -> Section {
        self.section.unwrap_or(Section::Account)
    }
}

#[derive(Clone, Debug)]
pub enum Event {
    Open(Section),
    Close,
    Search(String),
    /// A dropdown opened or closed.
    Drop(Drop),
    CopyVersion,
    CopiedDone,
    EditProfile,
    /// A preference changed: see [`pages::Pref`].
    Pref(pages::Pref),
    /// My Account's and the Discord page's buttons.
    Account(pages::AccountEvent),
    Game(pages::GameEvent),
    Voice(voice::Event),
    Theme(appearance::Event),
    /// About's updates.
    Updates(updates::Event),
}

impl App {
    /// Whether the setting `key` is new to this person, for its NEW tag.
    pub(super) fn fresh(&self, key: &str) -> bool {
        new_tags::key_new(
            self.layout.first_version.as_deref(),
            &self.layout.seen_new,
            key,
        )
    }

    /// Settings left `section`: what was new on it has been seen, kept in
    /// `layout.json` so its tags don't come back.
    fn leave_new(&mut self, section: Section) {
        let Some(page) = section.page() else {
            return;
        };
        if new_tags::leave(&mut self.layout.seen_new, page) {
            crate::layout::store(self.client.root(), "seen_new", &self.layout.seen_new);
        }
    }

    /// Opens Settings on a section; Voice & video also lists the devices
    /// again, as the Slint client does.
    pub fn open_settings(&mut self, section: Section) -> Task<Message> {
        self.menu = None;
        self.close_popup();
        let settings = self.settings.get_or_insert_with(Default::default);
        settings.blocked_shown = 5;
        self.read_platform_choices();
        let task = self.show_section(section);
        Task::batch([task, iced::widget::operation::focus(crate::app::NOBODY)])
    }

    /// Shows another section: leaving Appearance cancels the theme being
    /// edited, leaving Voice stops what it ran.
    fn show_section(&mut self, section: Section) -> Task<Message> {
        let Some(settings) = self.settings.as_mut() else {
            return Task::none();
        };
        let before = settings.section;
        settings.section = Some(section);
        settings.open = None;
        if let Some(before) = before.filter(|before| *before != section) {
            self.leave_new(before);
        }
        if before == Some(Section::Appearance) && section != Section::Appearance {
            self.cancel_theme();
        }
        if before == Some(Section::Voice) && section != Section::Voice {
            self.leave_voice_settings();
        }
        // Another page starts at its top.
        let top = iced::widget::operation::snap_to(
            iced::widget::Id::new(PAGE),
            iced::widget::scrollable::RelativeOffset::START,
        );
        if section == Section::Voice && before != Some(Section::Voice) {
            return Task::batch([top, self.enter_voice_settings()]);
        }
        Task::batch([top, self.sync()])
    }

    pub fn close_settings(&mut self) -> Task<Message> {
        if self.settings.is_none() {
            return Task::none();
        }
        self.cancel_theme();
        self.leave_voice_settings();
        if let Some(section) = self.settings.as_ref().and_then(|settings| settings.section) {
            self.leave_new(section);
        }
        self.settings = None;
        // Linking Discord from its page stops with it, as the Slint client's.
        if self.client.state().discord_link.is_some() {
            self.client.dispatch(Action::CancelDiscordLink);
        }
        // The pictures only its pages showed go with it.
        Task::batch([self.sync(), self.refocus()])
    }

    pub fn settings_event(&mut self, event: Event) -> Task<Message> {
        if self.settings.is_none() && !matches!(event, Event::Open(_) | Event::Updates(_)) {
            return Task::none();
        }
        match event {
            Event::Open(section) => {
                if self.settings.is_none() {
                    return self.open_settings(section);
                }
                self.show_section(section)
            }
            Event::Close => self.close_settings(),
            Event::Search(query) => {
                if let Some(settings) = &mut self.settings {
                    settings.query = query;
                }
                Task::none()
            }
            Event::Drop(drop) => {
                if let Some(settings) = &mut self.settings {
                    settings.open = (settings.open != Some(drop)).then_some(drop);
                }
                // What opened may show pictures, such as the entrance
                // sounds' soundboard.
                self.ask()
            }
            Event::CopyVersion => {
                if let Some(settings) = &mut self.settings {
                    settings.copied = true;
                }
                Task::batch([
                    iced::clipboard::write(self.build.summary.clone()),
                    crate::after(
                        std::time::Duration::from_secs(2),
                        Message::Settings(Event::CopiedDone),
                    ),
                ])
            }
            Event::CopiedDone => {
                if let Some(settings) = &mut self.settings {
                    settings.copied = false;
                }
                Task::none()
            }
            // The editor opens over Settings, as the Slint client's does.
            Event::EditProfile => {
                self.client.dispatch(Action::EditProfile);
                self.sync()
            }
            Event::Pref(pref) => self.pref(pref),
            Event::Account(event) => self.account_settings_event(event),
            Event::Game(event) => self.game_event(event),
            Event::Voice(event) => self.voice_settings_event(event),
            Event::Theme(event) => self.theme_event(event),
            Event::Updates(event) => self.updates_event(event),
        }
    }

    /// Reads what the desktop says of starting at sign-in and Discord's
    /// links, for Advanced. A Discord build only offers the links.
    pub fn read_platform_choices(&mut self) {
        let desktop = crate::desktop::desktop();
        let platform = Platform {
            autostart: desktop.autostart(),
            discord_links: desktop.discord_links(self.client.root()),
        };
        if let Some(settings) = &mut self.settings {
            settings.platform = platform;
        }
    }

    /// Settings' keys: Escape closes an open dropdown, stops recording a
    /// key, then closes Settings.
    pub fn settings_escape(&mut self) -> Task<Message> {
        let Some(settings) = &mut self.settings else {
            return Task::none();
        };
        if settings.open.take().is_some() {
            return Task::none();
        }
        if settings.voice.recording {
            settings.voice.recording = false;
            return Task::none();
        }
        self.close_settings()
    }

    /// Whether this account is linked to Discord, so its pages show.
    fn discord_linked(&self) -> bool {
        let state = self.client.state();
        state.discord_linked || state.servers.iter().any(|server| server.discord)
    }

    /// The card over the dimmed window, while Settings is open.
    pub fn settings_view(&self) -> Option<Element<'_, Message>> {
        let settings = self.settings.as_ref()?;
        let theme = &self.theme;
        let section = settings.section();
        let page = match section {
            Section::Account => self.account_page(),
            Section::Voice => self.voice_page(settings),
            Section::Messages => self.messages_page(),
            Section::Notifications => self.notifications_page(),
            Section::Discord => self.discord_page(settings),
            Section::Sounds => self.sounds_page(),
            Section::Advanced => self.advanced_page(),
            Section::Appearance => self.appearance_page(settings),
            Section::Games => self.games_page(settings),
            Section::About => self.about_page(settings),
        };
        let line = bar(theme.line, Length::Fill, 1);
        let header = column![
            row![
                text(section.title())
                    .size(15)
                    .font(BOLD)
                    .color(theme.text)
                    .width(Length::Fill),
                self.icon_button(
                    Icon::X,
                    "Close Settings",
                    32.0,
                    18,
                    Message::Settings(Event::Close),
                    true
                )
            ]
            .padding(iced::Padding {
                left: 16.0,
                right: 8.0,
                ..iced::Padding::ZERO
            })
            .height(HEADER - 1.0)
            .align_y(Alignment::Center),
            line
        ];
        let body = scrollable(
            container(container(page).max_width(form::COLUMN))
                .padding(iced::Padding {
                    top: form::PAGE_TOP,
                    right: form::PAGE_SIDE,
                    bottom: form::PAGE_BOTTOM,
                    left: form::PAGE_SIDE,
                })
                .center_x(Length::Fill),
        )
        .id(iced::widget::Id::new(PAGE))
        .style(scrollbar(theme))
        .height(Length::Fill);
        let panel = theme.panel;
        let content = container(column![header, body])
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_| crate::style::fill(panel));
        let chat = theme.chat;
        let card = row![
            container(self.settings_nav(settings))
                .width(NAV)
                .height(Length::Fill)
                .style(move |_| crate::style::fill(chat)),
            content
        ];
        Some(crate::sheet::backdrop(
            theme,
            card,
            MARGIN,
            Message::Settings(Event::Close),
        ))
    }

    /// The sections' column: the account with Edit Profile, the search,
    /// the sections under their groups (only those that match a search),
    /// and which build this is.
    fn settings_nav<'a>(&'a self, settings: &'a Settings) -> Element<'a, Message> {
        let theme = &self.theme;
        let account = &self.account;
        let linked = self.discord_linked();
        let mut names = column![
            text(account.name.as_str())
                .size(15)
                .font(BOLD)
                .color(theme.text)
                .wrapping(text::Wrapping::None)
        ]
        .spacing(4);
        if linked {
            let (muted, ink) = (theme.muted, theme.text);
            names = names.push(
                button(
                    row![
                        text("Edit Profile").size(13),
                        self.glyph(Icon::Pencil, 12, theme.muted)
                    ]
                    .spacing(4)
                    .align_y(Alignment::Center),
                )
                .padding(0)
                .height(20)
                .on_press(Message::Settings(Event::EditProfile))
                .style(controls::ink_style(muted, ink)),
            );
        }
        let head = row![
            self.avatar(account.avatar.as_deref(), &account.name, 48.0, 24.0, 1.0),
            container(names).width(Length::Fill).clip(true)
        ]
        .spacing(12)
        .align_y(Alignment::Center)
        .padding(iced::Padding {
            left: 24.0,
            right: 16.0,
            ..iced::Padding::ZERO
        })
        .height(56);
        let search = container(
            controls::field(
                theme,
                SEARCH,
                "Search settings",
                &settings.query,
                false,
                true,
            )
            .size(14)
            .padding([11, 12])
            .on_input(|query| Message::Settings(Event::Search(query))),
        )
        .padding([0, 16]);
        let query = settings.query.to_lowercase();
        let mut entries = column![];
        for (index, (group, sections)) in GROUPS.iter().enumerate() {
            if query.is_empty() {
                entries = entries.push(
                    container(text(*group).size(12).color(theme.muted))
                        .padding(iced::Padding {
                            top: if index == 0 { 0.0 } else { 16.0 },
                            left: 8.0,
                            ..iced::Padding::ZERO
                        })
                        .height(if index == 0 { 24.0 } else { 40.0 })
                        .align_y(Alignment::Center),
                );
            }
            for &section in *sections {
                // Discord's own sections, outside the preview.
                let discord_only = matches!(section, Section::Discord | Section::Games);
                if discord_only && self.client.state().preview {
                    continue;
                }
                if !query.is_empty() && !section.title().to_lowercase().contains(&query) {
                    continue;
                }
                entries = entries.push(self.nav_row(section, settings.section() == section));
            }
        }
        let list = scrollable(container(entries).padding(iced::Padding {
            left: 16.0,
            right: 16.0,
            bottom: 16.0,
            ..iced::Padding::ZERO
        }))
        .style(scrollbar(theme))
        .height(Length::Fill);
        let mut nav = column![head, search, list];
        if let Some(notice) = self.update_notice() {
            nav = nav.push(notice);
        }
        nav.push(self.build_stamp(settings.copied))
            .spacing(16)
            .padding(iced::Padding {
                top: 16.0,
                ..iced::Padding::ZERO
            })
            .into()
    }

    /// A section's row, as the Slint client's `ChannelRow`.
    fn nav_row(&self, section: Section, selected: bool) -> Element<'_, Message> {
        let fresh = section.page().is_some_and(|page| {
            new_tags::page_new(
                self.layout.first_version.as_deref(),
                &self.layout.seen_new,
                page,
            )
        });
        crate::sheet::nav_row(
            self,
            section.icon(),
            section.title(),
            crate::sheet::NavState {
                selected,
                danger: false,
                fresh,
            },
            NAV_ROW,
            Message::Settings(Event::Open(section)),
        )
    }

    /// Which build this is, under the sections, as the Slint client's
    /// `BuildStamp`: a click copies it as one line.
    fn build_stamp(&self, copied: bool) -> Element<'_, Message> {
        let theme = &self.theme;
        let build = &self.build;
        if build.headline.is_empty() {
            return Space::new().into();
        }
        let tint = build_tint(theme, build.tone);
        let mut second = row![
            text(if copied {
                "Copied"
            } else {
                build.channel.as_str()
            })
            .size(12)
            .font(BOLD)
            .color(if copied { theme.text } else { tint })
        ];
        if !copied && !build.details.is_empty() {
            second = second.push(
                text(format!(" · {}", build.details))
                    .size(12)
                    .color(theme.muted)
                    .wrapping(text::Wrapping::None),
            );
        }
        let face = button(
            column![
                text(build.headline.as_str())
                    .size(12)
                    .color(theme.muted)
                    .wrapping(text::Wrapping::None),
                second
            ]
            .spacing(4),
        )
        .padding([4, 8])
        .width(Length::Fill)
        .on_press(Message::Settings(Event::CopyVersion))
        .style(|_, _| button::Style::default());
        let face = iced::widget::tooltip(
            face,
            controls::tip(theme, "Click to copy"),
            iced::widget::tooltip::Position::Top,
        );
        column![bar(theme.line, Length::Fill, 1), face]
            .spacing(8)
            .padding(iced::Padding {
                left: 16.0,
                right: 16.0,
                bottom: 8.0,
                ..iced::Padding::ZERO
            })
            .height(62)
            .into()
    }

    /// Keeps `value` under `key` in `layout.json`.
    fn keep(&self, key: &str, value: impl serde::Serialize) {
        crate::layout::store(self.client.root(), key, value);
    }
}

/// How loud a build's channel is drawn: a release's in the live colour, a
/// prerelease's in the accent, a development build's red.
fn build_tint(theme: &crate::theme::Theme, tone: i32) -> Color {
    match tone {
        0 => theme.live,
        1 => theme.accent,
        _ => theme.danger_text,
    }
}

/// Which build this is, as the root crate's `build_info` words it.
#[derive(Clone, Debug, Default)]
pub struct Build {
    pub headline: String,
    /// Stable, Nightly, Pre-release or Development build.
    pub channel: String,
    /// The commit and day.
    pub details: String,
    /// 0 a release, 1 a prerelease, 2 a development build.
    pub tone: i32,
    /// Everything on one line, which a click copies.
    pub summary: String,
    /// Its version, such as `0.4.1` or `0.5.0-nightly.20261107.12`.
    pub version: String,
}
