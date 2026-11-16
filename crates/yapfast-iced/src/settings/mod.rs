//! Settings, as the Slint client's `SettingsScreen` with its Discord look
//! (`ui/settings.slint`, `ui/yapfast/settings-nav.slint`): a card over the
//! dimmed window, the sections down its left under the account and a
//! search, which build this is at their foot, and the chosen section's
//! page on the right under its title and the close button. Escape, the
//! close button or a click outside closes it.
//!
//! It's a screen on the contract `docs/iced-client.md` describes: its
//! state ([`Settings`]) lives in the app's slot only while it's open, and
//! closing it lets go of the theme editor, the camera being tried, the
//! microphone's gate meter and whatever the pages made. Its updates work
//! through a [`Cx`]: what the pages change is kept at once in
//! `layout.json`, under the keys the Slint client keeps them by, and handed
//! to the core as the Slint client hands it; what others show of it is
//! told with [`Request::Changed`]. Its view reads a [`Ctx`]. The window's
//! theme follows [`Settings::preview`] while the editor is open, and its
//! videos the camera being tried. The pages are built of the form kit
//! (`form.rs`); each page's module holds its view and what its controls
//! do.

mod appearance;
pub mod keybinds;
mod pages;
pub mod updates;

pub use pages::message_size;
pub mod voice;

use crate::widgets::scroll_view;
use iced::{
    Alignment, Color, Element, Length, Task,
    widget::{Space, button, column, container, row, text},
};
use yapfast_core::application::{Action, AppState};
use yapfast_core::new_tags::{self, Page as NewPage};

use crate::{
    app::Message,
    controls::{self, bar},
    ctx::{self, Ctx, Cx, Focus, Request, Screen},
    fonts::BOLD,
    form,
    pictures::Icon,
    slot::{Closes, Why},
    style::scrollbar,
    theme::Theme,
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
    Keybinds,
    About,
}

impl Section {
    /// The page as NEW tags name it, for those with tagged settings.
    fn page(self) -> Option<NewPage> {
        match self {
            Section::About => Some(NewPage::About),
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
            Section::Keybinds => "Keybinds",
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
            Section::Keybinds => Icon::Keyboard,
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
            Section::Keybinds,
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
    /// A keybind's action, by its row.
    Keybind(usize),
}

/// Settings while open.
#[derive(Default)]
pub struct Settings {
    pub section: Option<Section>,
    pub query: String,
    pub open: Option<Drop>,
    /// The version was copied a moment ago, which the stamp and About say.
    pub copied: bool,
    /// Appearance's theme editor while it's open, and the theme as edited,
    /// which the window shows meanwhile.
    pub editor: Option<Editor>,
    preview: Option<Theme>,
    /// Voice's state while it shows: the push-to-talk key being recorded,
    /// the camera being tried, the gate's meter, the entrance sounds.
    pub voice: voice::Voice,
    /// Game Activity's draft of a game.
    pub game: pages::GameDraft,
    /// The keybind being recorded, and the keys pressed so far.
    pub record: Option<keybinds::Recording>,
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
    /// Recording a keybind starts or stops: push to talk's or a row's.
    Record(keybinds::Target, bool),
    /// A key went down or up, or the window lost the focus, while one is
    /// recorded.
    Heard(keybinds::Heard),
    Keybind(keybinds::Event),
    /// Advanced's Restart Yapfast, after the GPU setting changed.
    Restart,
}

impl Settings {
    /// Settings as they open: the menus and the call's popups close under
    /// them, the keys go nowhere, and what the desktop says of the platform
    /// choices is read. [`Settings::show`] then shows a section.
    pub fn open(cx: &mut Cx<'_>) -> Self {
        cx.request(Request::CloseMenus);
        cx.request(Request::ClosePopups);
        cx.request(Request::Focus(Focus::Nobody));
        let mut settings = Self {
            blocked_shown: pages::BLOCKED_STEP,
            ..Self::default()
        };
        settings.read_platform_choices(cx);
        settings
    }

    /// Shows a section: leaving Appearance cancels the theme being edited,
    /// leaving Voice stops what it ran; Voice & video also lists the
    /// devices again, as the Slint client does.
    pub fn show(&mut self, section: Section, cx: &mut Cx<'_>) -> Task<Message> {
        let before = self.section;
        self.section = Some(section);
        self.open = None;
        self.record = None;
        if let Some(before) = before.filter(|before| *before != section) {
            leave_new(cx, before);
        }
        if before == Some(Section::Appearance) && section != Section::Appearance {
            self.cancel_theme(cx);
        }
        if before == Some(Section::Voice) && section != Section::Voice {
            self.voice = voice::Voice::default();
        }
        if section == Section::Voice && before != Some(Section::Voice) {
            self.enter_voice(cx);
        }
        cx.touch();
        // Another page starts at its top.
        iced::widget::operation::snap_to(
            iced::widget::Id::new(PAGE),
            iced::widget::scrollable::RelativeOffset::START,
        )
    }

    pub fn update(&mut self, event: Event, cx: &mut Cx<'_>) -> Task<Message> {
        match event {
            Event::Open(section) => return self.show(section, cx),
            Event::Close => cx.request(Request::Close(Screen::Settings)),
            Event::Search(query) => self.query = query,
            Event::Drop(drop) => {
                self.open = (self.open != Some(drop)).then_some(drop);
                // What opened may show pictures, such as the entrance
                // sounds' soundboard.
                cx.touch();
            }
            Event::CopyVersion => {
                self.copied = true;
                return Task::batch([
                    iced::clipboard::write(cx.build.summary.clone()),
                    crate::after(
                        std::time::Duration::from_secs(2),
                        Message::Settings(Event::CopiedDone),
                    ),
                ]);
            }
            Event::CopiedDone => self.copied = false,
            // The editor opens over Settings, as the Slint client's does.
            Event::EditProfile => cx.dispatch(Action::EditProfile),
            Event::Restart => cx.request(Request::Restart),
            Event::Pref(pref) => self.pref(pref, cx),
            Event::Account(event) => self.account_event(event, cx),
            Event::Game(event) => self.game_event(event, cx),
            Event::Voice(event) => self.voice_event(event, cx),
            Event::Theme(event) => self.theme_event(event, cx),
            Event::Record(target, on) => self.record(target, on),
            Event::Heard(heard) => self.heard(heard, cx),
            Event::Keybind(event) => self.keybind_event(event, cx),
        }
        Task::none()
    }

    /// Escape closes an open dropdown, stops recording a key, then closes
    /// Settings.
    pub fn escape(&mut self, cx: &mut Cx<'_>) {
        if self.open.take().is_some() {
            return;
        }
        if self.record.take().is_some() {
            return;
        }
        cx.request(Request::Close(Screen::Settings));
    }

    /// A preference others show changed: Voice's gate meter follows the
    /// input mode.
    pub fn changed(&mut self, pref: ctx::Pref, cx: &mut Cx<'_>) {
        if pref == ctx::Pref::Keybinds {
            self.meter_gate(cx);
        }
    }

    /// The theme the window shows while the editor is open: the theme as
    /// edited, kept nowhere.
    pub fn preview(&self) -> Option<&Theme> {
        self.preview.as_ref()
    }

    /// Reads what the desktop says of starting at sign-in and Discord's
    /// links, for Advanced. A Discord build only offers the links.
    fn read_platform_choices(&mut self, cx: &Cx<'_>) {
        let desktop = crate::desktop::desktop();
        self.platform = Platform {
            autostart: desktop.autostart(),
            discord_links: desktop.discord_links(cx.client.root()),
        };
    }
}

impl Closes for Settings {
    const LEAVES: bool = true;

    /// What its pages ran stops with it: the theme being edited shows no
    /// more, the camera being tried, the gate's meter and the key's
    /// recording go with its state, and linking Discord from its page
    /// stops, as the Slint client's.
    fn close(&mut self, cx: &mut Cx<'_>, why: Why) {
        if let Some(section) = self.section {
            leave_new(cx, section);
        }
        if self.preview.is_some() {
            cx.request(Request::Changed(ctx::Pref::Theme));
        }
        if cx.state().discord_link.is_some() {
            cx.dispatch(Action::CancelDiscordLink);
        }
        // The pictures only its pages showed go with it.
        cx.touch();
        if why != Why::Window {
            cx.request(Request::Focus(Focus::Default));
        }
    }
}

/// Settings left `section`: what was new on it has been seen, kept in
/// `layout.json` so its tags don't come back.
fn leave_new(cx: &mut Cx<'_>, section: Section) {
    let Some(page) = section.page() else {
        return;
    };
    if new_tags::leave(&mut cx.layout.seen_new, page) {
        cx.keep("seen_new", &cx.layout.seen_new);
    }
}

/// Whether this account is linked to Discord, so its pages show.
fn discord_linked(state: &AppState) -> bool {
    state.discord_linked || state.servers.iter().any(|server| server.discord)
}

/// The card over the dimmed window, while Settings is open.
pub fn view<'a>(settings: &'a Settings, cx: Ctx<'a>) -> Element<'a, Message> {
    Page { s: settings, cx }.view()
}

/// Settings' state and what they read, as their pages draw them.
#[derive(Clone, Copy)]
struct Page<'a> {
    s: &'a Settings,
    cx: Ctx<'a>,
}

impl<'a> Page<'a> {
    /// Whether the setting `key` is new to this person, for its NEW tag.
    fn fresh(self, key: &str) -> bool {
        new_tags::key_new(
            self.cx.layout.first_version.as_deref(),
            &self.cx.layout.seen_new,
            key,
        )
    }

    fn linked(self) -> bool {
        discord_linked(self.cx.state)
    }

    fn view(self) -> Element<'a, Message> {
        let settings = self.s;
        let theme = self.cx.look.theme;
        let section = settings.section();
        let page = match section {
            Section::Account => self.account_page(),
            Section::Voice => self.voice_page(),
            Section::Messages => self.messages_page(),
            Section::Notifications => self.notifications_page(),
            Section::Discord => self.discord_page(),
            Section::Sounds => self.sounds_page(),
            Section::Advanced => self.advanced_page(),
            Section::Appearance => self.appearance_page(),
            Section::Games => self.games_page(),
            Section::Keybinds => self.keybinds_page(),
            Section::About => self.about_page(),
        };
        let line = bar(theme.line, Length::Fill, 1);
        let header = column![
            row![
                text(section.title())
                    .size(15)
                    .font(BOLD)
                    .color(theme.text)
                    .width(Length::Fill),
                self.cx
                    .look
                    .icon_button(Icon::X, "Close Settings", Message::Settings(Event::Close))
                    .size(32.0, 18)
                    .below()
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
        let body = scroll_view(
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
        let radius = crate::sheet::RADIUS;
        let content = container(column![header, body])
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_| {
                crate::style::rounded(panel, crate::style::corners([0.0, radius, radius, 0.0]))
            });
        let chat = theme.chat;
        let card = row![
            container(self.nav())
                .width(NAV)
                .height(Length::Fill)
                .style(move |_| {
                    crate::style::rounded(chat, crate::style::corners([radius, 0.0, 0.0, radius]))
                }),
            content
        ];
        crate::sheet::backdrop(theme, card, MARGIN, Message::Settings(Event::Close))
    }

    /// The sections' column: the account with Edit Profile, the search,
    /// the sections under their groups (only those that match a search),
    /// and which build this is.
    fn nav(self) -> Element<'a, Message> {
        let settings = self.s;
        let theme = self.cx.look.theme;
        let account = self.cx.account;
        let linked = self.linked();
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
                        self.cx.look.glyph(Icon::Pencil, 12, theme.muted)
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
            self.cx
                .avatar(account.avatar.as_deref(), &account.name, 48.0, 24.0, 1.0),
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
                if discord_only && self.cx.state.preview {
                    continue;
                }
                if !query.is_empty() && !section.title().to_lowercase().contains(&query) {
                    continue;
                }
                entries = entries.push(self.nav_row(section, settings.section() == section));
            }
        }
        let list = scroll_view(container(entries).padding(iced::Padding {
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
    fn nav_row(self, section: Section, selected: bool) -> Element<'a, Message> {
        let fresh = section.page().is_some_and(|page| {
            new_tags::page_new(
                self.cx.layout.first_version.as_deref(),
                &self.cx.layout.seen_new,
                page,
            )
        });
        crate::sheet::nav_row(
            self.cx.look,
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
    fn build_stamp(self, copied: bool) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let build = self.cx.build;
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
