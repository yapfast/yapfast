//! Settings, with its Discord look: a card over the dimmed window, the
//! sections down its left under the account and a search, which build this
//! is at their foot, and the chosen section's page on the right under its
//! title and the close button. Escape, the close button or a click outside
//! closes it.
//!
//! It's a screen on the contract `docs/client/structure.md` describes: its
//! state ([`Settings`]) lives in the app's slot only while it's open, and
//! closing it lets go of the theme editor, the camera being tried, the
//! microphone's gate meter and whatever the pages made. Its updates work
//! through a [`Cx`]: what the pages change is kept at once in
//! `layout.json` and handed to the core; what others show of it is told
//! with [`Request::Changed`]. Its view reads a [`Ctx`]. The window's theme
//! follows [`Settings::preview`] while the editor is open, and its videos
//! the camera being tried. The pages are built of the form kit
//! (`form.rs`); each page's module holds its view and what its controls
//! do.

mod appearance;
mod font;
pub mod keybinds;
mod pages;
pub(crate) mod theme_creator;
mod theme_keys;
pub(crate) mod theme_marketplace;
mod theme_page;
pub use theme_keys::ThemeKey;
pub use theme_page::trial_banner;
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
pub(crate) use appearance::{COLOUR_CODE as THEME_COLOUR_CODE, Event as ThemeEvent};
pub(crate) use font::FontEvent;

/// What a theme editor's change sends.
#[cfg(test)]
pub fn appearance_event(event: ThemeEvent) -> Message {
    crate::app::settings(Event::Theme(event))
}

/// What a Font event sends.
#[cfg(test)]
pub fn font_message(event: FontEvent) -> Message {
    crate::app::settings(Event::Font(event))
}

/// How far the card sits in from the window's edges, and the sections'
/// column's width.
const MARGIN: f32 = 72.0;
const NAV: f32 = 266.0;
/// A section's row in the column, the page's header over it.
const NAV_ROW: f32 = 40.0;
const HEADER: f32 = 48.0;
/// The theme editor's column, wider than a page's.
const EDITOR_COLUMN: f32 = 960.0;
/// The search's field, and the page's scrollable.
pub const SEARCH: &str = "settings-search";
const PAGE: &str = "settings-page";

/// One of Settings' sections.
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
    Themes,
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
            Section::Themes => "Themes",
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
            Section::Themes => Icon::Brush,
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
            Section::Themes,
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
    /// Themes' editor while it's open, and the theme as edited,
    /// which the window shows meanwhile.
    pub editor: Option<Editor>,
    /// How the last theme import or export went, under Appearance's title,
    /// and whether it failed.
    pub theme_note: Option<(String, bool)>,
    /// The Slint build's guided, step-based creator. The palette editor above
    /// remains available as the legacy Advanced fallback while this flow is
    /// being ported.
    pub creator: Option<theme_creator::Creator>,
    /// Installed themes, marketplace, URL fetch and protected review state.
    pub marketplace: theme_marketplace::Marketplace,
    preview: Option<Theme>,
    /// Voice's state while it shows: the push-to-talk key being recorded,
    /// the camera being tried, the gate's meter, the entrance sounds.
    pub voice: voice::Voice,
    /// What Appearance's Font says of the files just chosen that didn't
    /// load, until the next change; none shows the standing note
    /// (`custom_font::note`).
    pub font_note: Option<String>,
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
    /// Opens them on the section last shown, as the account card's cog
    /// does.
    Reopen,
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
    /// Appearance's Font: choosing the font files, removing one, going back
    /// to DM Sans.
    Font(FontEvent),
    Marketplace(theme_marketplace::Event),
    /// Recording a keybind starts or stops: push to talk's or a row's.
    Record(keybinds::Target, bool),
    /// A key went down or up, or the window lost the focus, while one is
    /// recorded.
    Heard(keybinds::Heard),
    Keybind(keybinds::Event),
    /// Advanced's Restart Yapfast, after the GPU setting changed.
    Restart,
    /// Tab, Shift+Tab, Enter or Space on Settings → Themes.
    ThemeKey(ThemeKey),
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
        // Installed shows its themes as cards until the list is chosen.
        settings.marketplace.gallery = true;
        settings.read_platform_choices(cx);
        settings
    }

    /// Shows a section: leaving Themes cancels the theme being edited,
    /// leaving Voice stops what it ran; Voice & video also lists the
    /// devices again.
    pub fn show(&mut self, section: Section, cx: &mut Cx<'_>) -> Task<Message> {
        let before = self.section;
        self.section = Some(section);
        self.open = None;
        self.record = None;
        if let Some(before) = before.filter(|before| *before != section) {
            leave_new(cx, before);
        }
        if before == Some(Section::Themes) && section != Section::Themes {
            self.cancel_theme(cx);
        }
        if before == Some(Section::Voice) && section != Section::Voice {
            self.end_mic_test(cx);
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
            // The root opens them on the section kept, open or not.
            Event::Reopen => {}
            Event::Close => cx.request(Request::Close(Screen::Settings)),
            Event::Search(query) => self.query = query,
            Event::Drop(drop) => {
                self.open = (self.open != Some(drop)).then_some(drop);
                if self.open.is_some()
                    && let Some(editor) = self.editor.as_mut()
                {
                    editor.picker = None;
                }
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
                        crate::app::settings(Event::CopiedDone),
                    ),
                ]);
            }
            Event::CopiedDone => self.copied = false,
            // Settings close as the editor opens (`profile::editor::follow`).
            Event::EditProfile => cx.dispatch(Action::EditProfile),
            Event::Restart => cx.request(Request::Restart),
            Event::Pref(pref) => self.pref(pref, cx),
            Event::Account(event) => self.account_event(event, cx),
            Event::Game(event) => self.game_event(event, cx),
            Event::Voice(event) => self.voice_event(event, cx),
            Event::ThemeKey(key) => return self.theme_key(key, cx),
            Event::Theme(event) => {
                // A click puts the keys back where they were before Tab.
                self.marketplace.focus = None;
                return self.theme_event(event, cx);
            }
            Event::Marketplace(event) => {
                self.marketplace.focus = None;
                let runtime = cx.client.shared_runtime();
                let root = cx.client.root().to_owned();
                let task = self
                    .marketplace
                    .update(event, runtime, &root, cx.layout, cx.themes)
                    .map(|event| crate::app::settings(Event::Marketplace(event)));
                self.marketplace.installed.take();
                if std::mem::take(&mut self.marketplace.theme_applied) {
                    cx.request(Request::ThemeApplied);
                    cx.touch();
                }
                if std::mem::take(&mut self.marketplace.theme_changed) {
                    cx.request(Request::Changed(ctx::Pref::Theme));
                    cx.touch();
                }
                return task;
            }
            Event::Font(event) => return self.font_event(event, cx),
            Event::Record(target, on) => self.record(target, on),
            Event::Heard(heard) => self.heard(heard, cx),
            Event::Keybind(event) => self.keybind_event(event, cx),
        }
        Task::none()
    }

    /// Escape closes an open dropdown or colour picker, stops recording a
    /// key, then closes Settings.
    pub fn escape(&mut self, cx: &mut Cx<'_>) {
        if self.open.take().is_some() {
            return;
        }
        if let Some(editor) = self.editor.as_mut()
            && editor.picker.take().is_some()
        {
            return;
        }
        if self.record.take().is_some() {
            return;
        }
        cx.request(Request::Close(Screen::Settings));
    }

    /// A preference others show changed: Voice's gate meter follows the
    /// input mode.
    pub fn changed(&mut self, pref: ctx::Pref, cx: &Cx<'_>) {
        if pref == ctx::Pref::Keybinds {
            self.meter_gate(cx);
        }
    }

    /// The theme the window shows while the editor is open: the theme as
    /// edited, kept nowhere.
    pub fn preview(&self) -> Option<&Theme> {
        self.marketplace.protected_theme().or(self.preview.as_ref())
    }

    /// Lets go of the draft previewed, as Ctrl+Shift+T does. The editor or
    /// creator stays open on it.
    pub fn drop_preview(&mut self) {
        self.preview = None;
    }

    /// Whether a theme's review is open, which shows in the stock look.
    pub fn protected(&self) -> bool {
        self.marketplace.protected_theme().is_some()
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
    /// more, the mic test ends, the camera being tried, the gate's meter
    /// and the key's recording go with its state, and linking Discord from
    /// its page stops.
    fn close(&mut self, cx: &mut Cx<'_>, why: Why) {
        // The mic test's call hears again, muted as it was.
        self.end_mic_test(cx);
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
            Section::Themes => self.themes_page(),
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
                    .icon_button(
                        Icon::X,
                        "Close Settings",
                        crate::app::settings(Event::Close)
                    )
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
        // The theme editor fills the frame, wider and higher up, so its
        // preview stays in sight beside the controls, which scroll alone.
        let editing =
            section == Section::Themes && (settings.creator.is_some() || settings.editor.is_some());
        let body: Element<'a, Message> = if editing {
            container(
                container(page)
                    .max_width(EDITOR_COLUMN)
                    .height(Length::Fill),
            )
            .padding(iced::Padding {
                top: 22.0,
                right: form::PAGE_SIDE,
                bottom: 0.0,
                left: form::PAGE_SIDE,
            })
            .center_x(Length::Fill)
            .height(Length::Fill)
            .into()
        } else {
            scroll_view(
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
            .height(Length::Fill)
            .into()
        };
        let surface = yapfast_core::themes::package::art::ArtSurface::Settings;
        let decorated =
            self.cx.themes.decorates(&theme.id, surface) || theme.package_paint("panel").is_some();
        let panel = if decorated {
            iced::Color::TRANSPARENT
        } else {
            theme.panel
        };
        // Inside the card's 1 px edge, as the sheet's parts (see
        // `sheet::backdrop`).
        let radius = crate::sheet::inner_radius(theme);
        let content = container(column![header, body])
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_| {
                crate::style::rounded(panel, crate::style::corners([0.0, radius, radius, 0.0]))
            });
        // As Slint: the sections lie on the sidebar's paint unless the
        // settings are glass.
        let glass = self.cx.themes.glass(&theme.id, surface).is_some();
        let chat: iced::Background = if !decorated {
            theme.chat.into()
        } else if glass {
            iced::Color::TRANSPARENT.into()
        } else {
            theme.surface_paint("sidebar")
        };
        let card = row![
            container(self.nav())
                .width(NAV)
                .height(Length::Fill)
                .style(move |_| container::Style {
                    background: Some(chat),
                    border: iced::Border {
                        radius: crate::style::corners([radius, 0.0, 0.0, radius]),
                        ..Default::default()
                    },
                    ..Default::default()
                }),
            content
        ];
        let card = crate::theme_art::surface(card, self.cx, surface);
        // As Slint's settings card: its art and glass clipped to its
        // rounded corners.
        let card = if decorated {
            crate::widgets::Wrap::new(card, crate::widgets::Pane { radius }).into()
        } else {
            card
        };
        let shown = crate::sheet::backdrop(theme, card, MARGIN, crate::app::settings(Event::Close));
        let mut layers = vec![shown];
        if let Some(review) = settings.marketplace.review_dialog(self.cx.look) {
            let review = review.map(|event| crate::app::settings(Event::Marketplace(event)));
            let stock = settings.marketplace.protected_theme().unwrap_or(theme);
            let width = (self.cx.look.window.width - 32.0).min(560.0);
            layers.push(crate::dialog::dialog(
                stock,
                review,
                width,
                crate::app::settings(Event::Marketplace(theme_marketplace::Event::CancelReview)),
            ));
        }
        if layers.len() == 1 {
            return layers.pop().expect("one layer");
        }
        iced::widget::Stack::with_children(layers).into()
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
                .on_press(crate::app::settings(Event::EditProfile))
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
            .on_input(|query| crate::app::settings(Event::Search(query))),
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

    /// A section's row as a theme's channel list draws its rows: Slint's
    /// settings sections are its `ChannelRow`s, each with seed 0 (the
    /// list's first frame), the selected frame or underline, the list's
    /// case, face, weight, inks and the selected words' shadow. `None`
    /// without a theme's channel list.
    fn themed_nav_row(self, section: Section, selected: bool) -> Option<Element<'a, Message>> {
        let theme = self.cx.look.theme;
        // A theme's sections are Slint's `ChannelRow`s with or without a
        // channel list of its own (Phosphor's plain rows); stock keeps
        // Wired's.
        if !theme.packaged() {
            return None;
        }
        let listed = self.cx.themes.channel_rows(&theme.id);
        let fallback = yapfast_core::themes::lists::ChannelRows::default();
        let rows = listed.unwrap_or(&fallback);
        let id = format!("settings:{}", section.title());
        let pointed = self.cx.pointed.side.as_deref() == Some(id.as_str());
        let stock = if selected {
            theme.token_colour("row-selected-text", theme.text)
        } else if pointed {
            theme.secondary
        } else {
            theme.muted
        };
        let ink = if selected {
            rows.ink.selected
        } else if pointed && rows.ink.hover.is_some() {
            rows.ink.hover
        } else {
            rows.ink.read
        }
        .map(crate::themes::argb)
        .unwrap_or(stock);
        let stock_icon = if selected { theme.text } else { theme.faint };
        let icon_ink = if rows.icon.follow_ink {
            ink
        } else if selected {
            rows.icon
                .selected
                .map(crate::themes::argb)
                .unwrap_or(stock_icon)
        } else if pointed && rows.icon.hover.is_some() {
            rows.icon
                .hover
                .map(crate::themes::argb)
                .unwrap_or(stock_icon)
        } else {
            rows.icon
                .colour
                .map(crate::themes::argb)
                .unwrap_or(stock_icon)
        };
        let themed_ink = rows.icon.follow_ink
            || if selected {
                rows.icon.selected.is_some()
            } else {
                rows.icon.colour.is_some()
            };
        let icon_size = rows.icon.size.unwrap_or(20.0);
        let glyph = if themed_ink {
            crate::look::Look::tinted_glyph
        } else {
            crate::look::Look::glyph
        };
        let role = rows
            .font
            .map_or("body", yapfast_core::themes::lists::FontRole::name);
        let font =
            crate::themes::list_font(theme, rows.font, "body", rows.weight, crate::fonts::MEDIUM);
        let shadow = if selected {
            crate::themes::offset_shadow(rows.ink.selected_shadow.map(|c| (c, 0.0, 1.0)))
        } else {
            crate::piece::TextLight::default()
        };
        let words = crate::themes::list_case(rows.case, section.title()).into_owned();
        let underlined = selected && rows.selected.underline.is_some();
        let hug = rows.fit_text || underlined;
        let name = crate::elide::elide(words)
            .size(rows.size.unwrap_or_else(|| theme.type_size(role, 15.0)))
            .font(font)
            .color(ink)
            .light(shadow);
        let name = if hug { name.shrink() } else { name };
        let mut name = column![name];
        if selected && let Some(underline) = rows.selected.underline {
            name = name.spacing(underline.offset).push(crate::controls::bar(
                crate::themes::argb(underline.colour),
                Length::Fill,
                underline.width,
            ));
        }
        let name = name.width(if hug { Length::Shrink } else { Length::Fill });
        let face = row![
            glyph(
                self.cx.look,
                section.icon(),
                icon_size.round() as u32,
                icon_ink
            ),
            name
        ]
        .spacing(rows.icon.gap.unwrap_or(8.0))
        .align_y(Alignment::Center);
        // Slint's settings sections are 40 px rows with no gap between
        // them (`settings-nav.slint`), whatever the channel list's own.
        let gap = 0.0;
        let height = 40.0;
        let row = crate::sidebar::listed_row_face(
            self.cx,
            id,
            face.into(),
            height,
            selected,
            Some(crate::app::settings(Event::Open(section))),
            None,
            listed.is_some(),
            Some(0),
        );
        Some(
            container(row)
                .padding(iced::Padding {
                    bottom: gap,
                    ..iced::Padding::ZERO
                })
                .into(),
        )
    }

    /// A section's row.
    fn nav_row(self, section: Section, selected: bool) -> Element<'a, Message> {
        if let Some(row) = self.themed_nav_row(section, selected) {
            return row;
        }
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
            crate::app::settings(Event::Open(section)),
        )
    }

    /// Which build this is, under the sections: a click copies it as one
    /// line.
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
        .on_press(crate::app::settings(Event::CopyVersion))
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
