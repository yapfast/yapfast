//! The profile editor, as the Slint client's `ProfileEditorDialog`: this
//! account's card, each part of it edited in place as Discord's editors
//! allow (the core's `ProfileEditor` says which), and Discord's bar asking
//! to save once something changed. The main profile has what it wears down
//! its left (`ProfileCustomize`) and its tabs on the right where there's
//! room; a server's profile is the card alone, with Nitro's offer over what
//! takes it.
//!
//! What the card shows is a profile view made of what's written, as the
//! Slint client's adapter makes it, and drawn with the profile card's own
//! parts; it's made again only when the editor changes.

use std::{path::PathBuf, sync::Arc};

use iced::{
    Alignment, Border, Color, Element, Length, Padding, Task,
    widget::{Id, Space, button, column, container, opaque, operation, row, stack, text},
};
use yapfast_core::{
    application::{Action, AppState, ProfileEditor, ProfileText, ProfileView},
    chat::{self, ProfilePicture},
};

use crate::profile::{Event as ProfileEvent, Mode, Shown, look::Tab};
use crate::{
    app::{App, Message},
    controls::{lights, placed},
    ctx::{Cx, Focus, Request},
    field::{self, Draft},
    fonts::{BOLD, MEDIUM, SEMIBOLD},
    pictures::Icon,
    slot::{Closes, Slot, Why},
    style::{faded, rounded},
    theme::rgb,
    widgets::{Hsv, hsv_rgb, rgb_hsv},
};

mod card;
mod colour;
mod menus;
mod styles;

use colour::parse_colour;
use menus::item;

/// The text fields' ids.
const NAME: &str = "profile-edit-name";
const PRONOUNS: &str = "profile-edit-pronouns";
const BIO: &str = "profile-edit-bio";
const COLOUR: &str = "profile-edit-colour";
const THEME_COLOUR: &str = "profile-edit-theme";
const TAG_SEARCH: &str = "profile-edit-tag";
/// The styles column beside the main profile, and the avatar's size.
const SIDE: f32 = 241.0;
/// A tile's width, two to a row of the styles column.
const TILE: f32 = (SIDE - 32.0 - 8.0) / 2.0;
const AVATAR: f32 = 120.0;
/// Where Discord's client sends someone for what only its shop sets.
const SHOP: &str = "https://discord.com/shop";
const CONNECTIONS: &str = "https://discord.com/settings/connections";
/// The banner colours offered without typing, as the Slint client's.
const SWATCHES: [u32; 9] = [
    0x5865f2, 0x3ba55d, 0xfaa61a, 0xed4245, 0xeb459e, 0x1abc9c, 0x9b59b6, 0x607d8b, 0x2c2f33,
];

/// A text being edited in place.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Text {
    Name,
    Pronouns,
    Bio,
}

/// What opens over the editor: a picture's menu, the server tags, the
/// card's More, a theme colour's code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pop {
    Avatar,
    Banner,
    Tag,
    More,
    Top,
    Bottom,
}

/// What's done in the editor.
#[derive(Clone, Debug)]
pub enum Event {
    Edit(Option<Text>),
    Write(ProfileText, String),
    Bio(field::Event),
    /// The avatar or banner clicked: its menu, the file dialog, or the
    /// banner colour's box.
    Picture(ProfilePicture),
    Pick(ProfilePicture),
    Picked(ProfilePicture, PathBuf),
    Reset(ProfilePicture),
    Pop(Option<Pop>),
    /// A theme colour chosen on its pad.
    Hsv(Hsv),
    Tag(String),
    TagQuery(String),
    Tab(Tab),
    Side(bool),
    Undo,
    Save,
    Nitro,
    CopyId,
    Status,
    Note,
    Close,
    /// In the Display Name Style window.
    Style(crate::profile::name_style::Event),
}

fn message(event: Event) -> Message {
    Message::Profile(ProfileEvent::Editor(event))
}

/// The editor on screen: its card, and what's open in it.
pub struct Editor {
    pub(super) shown: Shown,
    editing: Option<Text>,
    colour_open: bool,
    pop: Option<Pop>,
    /// The open theme colour's, as its pad shows it.
    hsv: Hsv,
    tab: Option<Tab>,
    side: bool,
    tag_query: String,
    bio: Draft,
    /// The Display Name Style window, while it's open.
    pub(crate) style: Option<crate::profile::name_style::Window>,
    /// The core's editor and lists changes its card was made at.
    made: (u64, u64),
}

impl Editor {
    fn new(shown: Shown) -> Self {
        Self {
            shown,
            editing: None,
            colour_open: false,
            pop: None,
            hsv: [0.0; 3],
            tab: None,
            side: true,
            tag_query: String::new(),
            bio: Draft::default(),
            style: None,
            made: (0, 0),
        }
    }

    /// Ends what's being edited or open on the card.
    pub(super) fn stop_editing(&mut self) {
        self.editing = None;
        self.colour_open = false;
        self.pop = None;
    }
}

/// The card as it would show once saved: what's written over what Discord
/// has, as the Slint client's adapter makes it.
fn preview(state: &AppState, editor: &ProfileEditor) -> ProfileView {
    let fallback = editor.profile.as_ref().map(|profile| &profile.fallback);
    let text = |text: Option<&String>| text.cloned().unwrap_or_default();
    let written = |written: &str, fallback: Option<&String>| {
        let written = written.trim();
        if written.is_empty() {
            text(fallback)
        } else {
            written.to_owned()
        }
    };
    // A picture just chosen goes by a name of its own, which the card's
    // pictures take from the editor (`Shown::picked`).
    let picture = |kind: ProfilePicture, own: Option<&String>| match editor.picture(kind) {
        (_, Some(_)) => Some(picked_url(kind, editor.picture_edits)),
        (Some(kept), None) => Some(kept.to_owned()),
        (None, None) => own.cloned(),
    };
    let kept_accent = editor.profile.as_ref().and_then(|profile| profile.accent);
    let accent = if editor.may_write(ProfileText::Colour) {
        editor.accent().unwrap_or(kept_accent)
    } else {
        kept_accent
    };
    let tag = match &editor.chosen_tag {
        None => fallback.and_then(|own| own.tag.clone()),
        Some(None) => None,
        Some(Some(chosen)) => state
            .servers
            .iter()
            .find(|server| server_id(&server.id) == chosen)
            .and_then(|server| server.tag.clone()),
    };
    let profile = chat::Profile {
        id: fallback.map(|own| own.id.clone()).unwrap_or_default(),
        name: written(&editor.name, fallback.map(|own| &own.name)),
        handle: text(fallback.map(|own| &own.handle)),
        avatar: picture(
            ProfilePicture::Avatar,
            fallback.and_then(|own| own.avatar.as_ref()),
        ),
        banner: picture(
            ProfilePicture::Banner,
            fallback.and_then(|own| own.banner.as_ref()),
        ),
        accent: accent.or_else(|| fallback.and_then(|own| own.accent)),
        pronouns: written(&editor.pronouns, fallback.map(|own| &own.pronouns)),
        // Its times as the profile shows them, as Discord's preview does.
        bio: yapfast_core::clock::render_timestamps(&written(
            &editor.bio,
            fallback.map(|own| &own.bio),
        )),
        created: fallback.and_then(|own| own.created.clone()),
        joined: fallback.and_then(|own| own.joined.clone()),
        server_name: text(fallback.map(|own| &own.server_name)),
        server_icon: fallback.and_then(|own| own.server_icon.clone()),
        theme: editor.theme().ok().flatten(),
        style: editor.name_style().map(Arc::new),
        widgets: fallback.map(|own| own.widgets.clone()).unwrap_or_default(),
        board_asked: fallback.is_some_and(|own| own.board_asked),
        board_named: fallback.is_some_and(|own| own.board_named),
        wishlist_place: fallback.and_then(|own| own.wishlist_place.clone()),
        wishlist: editor.wishlist.clone(),
        tag,
        // What the editor doesn't change comes from the profile it read.
        decoration: fallback.and_then(|own| own.decoration.clone()),
        effect: fallback.and_then(|own| own.effect.clone()),
        badges: fallback.map(|own| own.badges.clone()).unwrap_or_default(),
        connections: fallback
            .map(|own| own.connections.clone())
            .unwrap_or_default(),
        // The account's own, as the card at the window's foot shows it.
        presence: crate::account::account(state)
            .presence
            .filter(|_| editor.main()),
        activity: fallback.and_then(|own| own.activity.clone()),
        activities: fallback
            .map(|own| own.activities.clone())
            .unwrap_or_default(),
        ..Default::default()
    };
    ProfileView {
        profile,
        ..Default::default()
    }
}

/// A chosen picture's name on the card, new with each choice.
fn picked_url(kind: ProfilePicture, edits: u32) -> String {
    let kind = match kind {
        ProfilePicture::Avatar => "avatar",
        ProfilePicture::Banner => "banner",
    };
    format!("picked:{kind}:{edits}")
}

/// A server's Discord id, from Yapfast's id for it.
fn server_id(id: &str) -> &str {
    id.rsplit(':').next().unwrap_or(id)
}

impl Editor {
    /// A new editor on the core's, its card made of what's written.
    fn open(state: &AppState, link: u32) -> Self {
        let core = state
            .profile_editor
            .as_ref()
            .expect("the editor opens only while the core's is open");
        let view = Arc::new(preview(state, core));
        Self::new(Shown::new(crate::profile::look::Look::new(
            state,
            &view,
            None,
            &[],
            link,
        )))
    }

    /// Takes in the core's editor: its card is made again only when what it
    /// would show changed.
    fn sync(&mut self, state: &AppState, core: &ProfileEditor, link: u32) {
        // The preview is made from the editor and the servers' tags alone.
        let made = (state.profile_editor_changes, state.list_changes);
        if self.made == made {
            return;
        }
        let view = Arc::new(preview(state, core));
        if *self.shown.look.view != *view {
            self.shown.look = crate::profile::look::Look::new(state, &view, None, &[], link);
        }
        let mut picked = std::collections::HashMap::new();
        for kind in [ProfilePicture::Avatar, ProfilePicture::Banner] {
            if let (_, Some(preview)) = core.picture(kind) {
                picked.insert(picked_url(kind, core.picture_edits), preview.clone());
            }
        }
        *self.shown.picked.borrow_mut() = picked;
        self.made = made;
        // The bio's field follows the core while it isn't being written.
        if self.editing != Some(Text::Bio) && self.bio.text() != core.bio {
            self.bio.reset(&core.bio);
        }
    }

    /// What's done in the editor.
    pub fn update(&mut self, event: Event, cx: &mut Cx<'_>) -> Task<Message> {
        let editor = self;
        match event {
            Event::Style(event) => return crate::profile::name_style::update(editor, event, cx),
            Event::Edit(field) => {
                editor.editing = field;
                editor.colour_open = false;
                editor.pop = None;
                let id = match field {
                    Some(Text::Name) => NAME,
                    Some(Text::Pronouns) => PRONOUNS,
                    Some(Text::Bio) => BIO,
                    None => return operation::focus(crate::app::NOBODY),
                };
                return operation::focus(Id::new(id));
            }
            Event::Write(field, text) => {
                // A theme colour's code typed moves its pad there.
                if matches!(field, ProfileText::ThemeTop | ProfileText::ThemeBottom)
                    && let Some(rgb) = parse_colour(&text)
                {
                    editor.hsv = rgb_hsv(rgb);
                }
                cx.dispatch(Action::WriteProfile { field, text });
            }
            Event::Hsv(hsv) => {
                let field = match editor.pop {
                    Some(Pop::Top) => ProfileText::ThemeTop,
                    Some(Pop::Bottom) => ProfileText::ThemeBottom,
                    _ => return Task::none(),
                };
                editor.hsv = hsv;
                cx.dispatch(Action::WriteProfile {
                    field,
                    text: format!("#{:06x}", hsv_rgb(hsv)),
                });
            }
            Event::Bio(event) => match event {
                field::Event::Edited(text, _) => {
                    cx.dispatch(Action::WriteProfile {
                        field: ProfileText::Bio,
                        text,
                    });
                }
                // Escape comes as a key too, which `editor_escape` takes.
                field::Event::Send => {
                    editor.editing = None;
                    return operation::focus(crate::app::NOBODY);
                }
                _ => return Task::none(),
            },
            Event::Picture(kind) => {
                editor.editing = None;
                let state = cx.state();
                let Some(core) = state.profile_editor.as_ref() else {
                    return Task::none();
                };
                if kind == ProfilePicture::Banner && core.may_write(ProfileText::Colour) {
                    editor.colour_open = !editor.colour_open;
                    editor.pop = None;
                    return if editor.colour_open {
                        operation::focus(Id::new(COLOUR))
                    } else {
                        Task::none()
                    };
                }
                if core.resettable(kind) {
                    editor.pop = Some(match kind {
                        ProfilePicture::Avatar => Pop::Avatar,
                        ProfilePicture::Banner => Pop::Banner,
                    });
                    return Task::none();
                }
                return editor.update(Event::Pick(kind), cx);
            }
            Event::Pick(kind) => {
                editor.pop = None;
                let title = match kind {
                    ProfilePicture::Avatar => "Change Avatar",
                    ProfilePicture::Banner => "Change Banner",
                };
                return Task::perform(
                    async move {
                        rfd::AsyncFileDialog::new()
                            .set_title(title)
                            .add_filter("Pictures", &["png", "jpg", "jpeg", "gif", "webp"])
                            .pick_file()
                            .await
                            .map(|file| file.path().to_owned())
                    },
                    move |path| match path {
                        Some(path) => message(Event::Picked(kind, path)),
                        None => Message::Nothing,
                    },
                );
            }
            Event::Picked(kind, path) => {
                cx.dispatch(Action::PickProfilePicture { kind, path });
            }
            Event::Reset(kind) => {
                editor.pop = None;
                cx.dispatch(Action::ResetProfilePicture(kind));
            }
            Event::Pop(pop) => {
                editor.pop = if editor.pop == pop { None } else { pop };
                editor.editing = None;
                editor.tag_query.clear();
                match editor.pop {
                    Some(Pop::Tag) => return operation::focus(Id::new(TAG_SEARCH)),
                    Some(pop @ (Pop::Top | Pop::Bottom)) => {
                        let core = cx.state().profile_editor.as_ref();
                        let code = core.map_or("", |core| match pop {
                            Pop::Top => core.theme_top.as_str(),
                            _ => core.theme_bottom.as_str(),
                        });
                        editor.hsv = rgb_hsv(parse_colour(code).unwrap_or(0x5865f2));
                        return operation::focus(Id::new(THEME_COLOUR));
                    }
                    _ => return Task::none(),
                }
            }
            Event::Tag(server) => {
                editor.pop = None;
                cx.dispatch(Action::ChooseServerTag(server));
            }
            Event::TagQuery(query) => {
                editor.tag_query = query;
                return Task::none();
            }
            Event::Tab(tab) => {
                editor.tab = Some(tab);
                if tab == Tab::Wishlist {
                    cx.dispatch(Action::OpenWishlist);
                }
            }
            Event::Side(side) => editor.side = side,
            Event::Undo => {
                editor.editing = None;
                editor.colour_open = false;
                cx.dispatch(Action::UndoProfileEdits);
            }
            Event::Save => {
                editor.editing = None;
                cx.dispatch(Action::SaveProfile);
            }
            Event::Nitro => cx.dispatch(Action::UnlockNitro),
            Event::CopyId => {
                editor.pop = None;
                let id = editor.shown.look.profile().id.clone();
                return iced::clipboard::write(id);
            }
            Event::Status => cx.dispatch(Action::OpenStatusEditor),
            Event::Note => {
                let Some(user) = cx.state().discord_user.clone() else {
                    return Task::none();
                };
                cx.dispatch(Action::AskPerson {
                    user,
                    ask: yapfast_core::application::PersonAsk::Note,
                    location: "ContextMenu",
                });
            }
            // The slot closes as the core's editor goes.
            Event::Close => cx.dispatch(Action::CloseProfileEditor),
        }
        cx.touch();
        Task::none()
    }

    /// Escape in the editor: what's being edited or open first, then the
    /// editor.
    pub fn escape(&mut self, cx: &mut Cx<'_>) -> Task<Message> {
        if crate::profile::name_style::escape(self) {
            return Task::none();
        }
        let editor = self;
        if editor.editing.is_some() || editor.colour_open || editor.pop.is_some() {
            editor.editing = None;
            editor.colour_open = false;
            editor.pop = None;
            return operation::focus(crate::app::NOBODY);
        }
        editor.update(Event::Close, cx)
    }
}

impl Closes for Editor {
    /// The keys go back to where they were, unless the window went.
    fn close(self, cx: &mut Cx<'_>, why: Why) {
        if why != Why::Window {
            cx.request(Request::Focus(Focus::Default));
        }
    }
}

/// The editor follows the core's: made as it opens, closed as it closes,
/// its card made again as what it shows changes.
pub fn follow(slot: &mut Slot<Editor>, cx: &mut Cx<'_>) {
    let link = rgb(cx.theme.link);
    let open = cx.state().profile_editor.is_some().then_some(());
    let Some(editor) = slot.follow(
        open,
        cx,
        |_, _| true,
        |(), cx| Editor::open(cx.state(), link),
    ) else {
        return;
    };
    let state = cx.state();
    if let Some(core) = state.profile_editor.as_ref() {
        editor.sync(state, core, link);
    }
}

impl App {
    /// The pictures the editor's card shows.
    pub(super) fn editor_pictures(&self) -> Vec<String> {
        let Some(editor) = self.profiles.editor.get() else {
            return Vec::new();
        };
        let profile = editor.shown.look.profile();
        let cosmetics = self.client.state().cosmetics;
        let mut urls: Vec<String> = [
            profile.avatar.clone(),
            profile.banner.clone(),
            profile.banner.as_deref().and_then(chat::moving_banner),
        ]
        .into_iter()
        .flatten()
        .filter(|url| !url.starts_with("picked:"))
        .collect();
        urls.extend(
            cosmetics
                .decoration(profile.decoration.as_deref())
                .map(str::to_owned),
        );
        urls.extend(profile.badges.iter().map(|badge| badge.icon.clone()));
        urls.extend(profile.server_icon.clone());
        urls.extend(
            cosmetics
                .tag(profile.tag.as_ref())
                .and_then(|tag| tag.badge.as_deref())
                .map(str::to_owned),
        );
        // The effect's still, on its tile.
        urls.extend(crate::profile::cosmetic::effect_pictures(
            self.client.state(),
            &editor.shown,
            false,
        ));
        urls
    }

    /// The editor over the window dimmed by 60%; a click outside the card
    /// closes it.
    /// The editor's card's width: the main profile's with the styles
    /// column or without, a server profile's.
    fn editor_card_width(&self, editor: &Editor, main: bool) -> f32 {
        let width = self.window.width;
        if main {
            if editor.side {
                (width - 8.0).min(1199.0)
            } else {
                (width - 96.0).min(958.0)
            }
        } else {
            (width - 48.0).min(600.0)
        }
    }

    /// Whether the card has room for the tabs beside the middle.
    fn editor_tabs_shown(card_width: f32, main: bool) -> bool {
        main && card_width >= 880.0
    }

    /// The middle column's width: what the styles column (and its line)
    /// leave of the card, halved with the tabs, which fill as it does.
    fn editor_middle_width(&self, editor: &Editor, main: bool) -> f32 {
        let card = self.editor_card_width(editor, main);
        let rest = card - if main && editor.side { SIDE + 1.0 } else { 0.0 };
        if Self::editor_tabs_shown(card, main) {
            rest / 2.0
        } else {
            rest
        }
    }

    pub fn profile_editor_view(&self) -> Option<Element<'_, Message>> {
        let editor = self.profiles.editor.get()?;
        let core = self.client.state().profile_editor.as_ref()?;
        let theme = &self.theme;
        let main = core.main();
        let height = self.window.height;
        let inks = self.inks(&editor.shown.look, Mode::Side);
        let card_width = self.editor_card_width(editor, main);
        let card_height = (height - 48.0).min(if main { 798.0 } else { 620.0 });
        let middle = self.editor_middle(editor, core, &inks);
        let mut parts = row![];
        if main && editor.side {
            let line = theme.line;
            parts = parts.push(self.customize(editor, core)).push(
                container(Space::new())
                    .width(1)
                    .height(Length::Fill)
                    .style(move |_| rounded(line, 0)),
            );
        }
        parts = parts.push(container(middle).width(Length::Fill).height(Length::Fill));
        if Self::editor_tabs_shown(card_width, main) {
            parts = parts.push(self.editor_tabs(editor, &inks));
        }
        let mut card = stack![
            container(parts)
                .width(card_width)
                .height(card_height)
                .clip(true)
                .style(crate::profile::frame_style(inks, theme.edge, 12.0, true))
        ];
        if main {
            card = card.push(
                container(
                    self.look()
                        .icon_button(Icon::X, "Close", message(Event::Close))
                        .size(36.0, 18)
                        .below(),
                )
                .width(card_width)
                .align_x(Alignment::End)
                .padding(12),
            );
            if !editor.side {
                card = card.push(placed(self.show_styles(), 0.0, 40.0));
            }
        }
        // Over the window dimmed as under a dialog.
        let mut layers = stack![
            crate::controls::dimmed(message(Event::Close), crate::controls::DIM),
            container(opaque(card)).center(Length::Fill)
        ];
        if let Some(window) = &editor.style {
            layers = layers.push(self.name_style_view(window, &editor.shown));
        }
        Some(opaque(layers))
    }

    /// The tabs beside the main profile: its Board, Activity and Wishlist,
    /// as the full profile shows them.
    fn editor_tabs<'a>(
        &'a self,
        editor: &'a Editor,
        inks: &crate::profile::look::Inks,
    ) -> Element<'a, Message> {
        let look = &editor.shown.look;
        let tab = editor
            .tab
            .filter(|tab| look.tabs.contains(tab))
            .unwrap_or(look.tabs[0]);
        let mut heads = row![].spacing(24);
        for entry in &look.tabs {
            let label = match entry {
                Tab::Board => "Board",
                Tab::Activity => "Activity",
                Tab::Wishlist => "Wishlist",
                Tab::Friends | Tab::Servers => continue,
            };
            heads = heads.push(crate::controls::tab(
                crate::controls::Tab {
                    label,
                    chosen: *entry == tab,
                    ink: inks.ink,
                    rest: inks.ink2,
                    bar: inks.ink,
                    height: 26.0,
                    rest_font: SEMIBOLD,
                },
                message(Event::Tab(*entry)),
            ));
        }
        let rule_ink = if inks.themed {
            faded(inks.ink, 0.12)
        } else {
            self.theme.line
        };
        column![
            heads,
            container(Space::new())
                .width(Length::Fill)
                .height(1)
                .style(move |_| rounded(rule_ink, 0)),
            self.tab_page(&editor.shown, tab, inks),
        ]
        .spacing(16)
        .padding(Padding {
            top: 58.0,
            right: 32.0,
            bottom: 22.0,
            left: 0.0,
        })
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }
}

/// A section's heading on the card: the main profile's in its card's
/// words, a server's in capitals.
fn heading<'a>(
    main: bool,
    words: &'a str,
    inks: &crate::profile::look::Inks,
) -> Element<'a, Message> {
    if main {
        text(words).size(13).font(MEDIUM).color(inks.ink).into()
    } else {
        text(words.to_uppercase())
            .size(12)
            .font(BOLD)
            .color(inks.ink3)
            .into()
    }
}

/// Words that do something when clicked.
fn link_text<'a>(words: &'a str, ink: Color, pressed: Message) -> Element<'a, Message> {
    button(text(words).size(14).font(MEDIUM).color(ink))
        .padding(0)
        .on_press(pressed)
        .style(|_, _| button::Style::default())
        .into()
}

/// What says a text can be changed: an outline around it while pointed at,
/// as Discord's editor draws it (1 px of 17% white, 7 px round).
fn hover_edit<'a>(
    content: impl Into<Element<'a, Message>>,
    enabled: bool,
    pressed: Message,
) -> Element<'a, Message> {
    let outline = Color {
        a: 0.17,
        ..Color::WHITE
    };
    button(content)
        .padding([4, 7])
        .on_press_maybe(enabled.then_some(pressed))
        .style(move |_, status| button::Style {
            background: None,
            border: Border {
                color: if enabled && lights(status) {
                    outline
                } else {
                    Color::TRANSPARENT
                },
                width: 1.0,
                radius: 7.0.into(),
            },
            ..Default::default()
        })
        .into()
}
