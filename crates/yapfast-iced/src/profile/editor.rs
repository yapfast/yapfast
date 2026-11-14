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
    Alignment, Border, Color, Element, Length, Padding, Point, Size, Task,
    widget::{
        Id, Space, button, column, container, mouse_area, opaque, operation, row, scrollable,
        stack, text, text_input,
    },
};
use yapfast_core::{
    application::{
        Action, AppState, BIO_LIMIT, NAME_LIMIT, PRONOUNS_LIMIT, ProfileEditor, ProfileText,
        ProfileView,
    },
    chat::{self, ProfilePicture},
};

use super::{Event as ProfileEvent, Mode, Shown, look::Tab};
use crate::{
    app::{App, Message},
    controls::{self, Check, Item, Kind, Light, action, lights, placed},
    field::{self, Draft, Field},
    fonts::{BOLD, MEDIUM, SEMIBOLD},
    form,
    pictures::Icon,
    popover::{Popover, Side, Width},
    style::{card_shadow, corners, faded, hex, rounded, scrollbar},
    theme::rgb,
    widgets::{Hover, Pic},
};

/// The text fields' ids.
const NAME: &str = "profile-edit-name";
const PRONOUNS: &str = "profile-edit-pronouns";
const BIO: &str = "profile-edit-bio";
const COLOUR: &str = "profile-edit-colour";
const THEME_COLOUR: &str = "profile-edit-theme";
const TAG_SEARCH: &str = "profile-edit-tag";
/// The styles column beside the main profile, and the avatar's size.
const SIDE: f32 = 241.0;
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
    tab: Option<Tab>,
    side: bool,
    tag_query: String,
    bio: Draft,
}

impl Editor {
    fn new(shown: Shown) -> Self {
        Self {
            shown,
            editing: None,
            colour_open: false,
            pop: None,
            tab: None,
            side: true,
            tag_query: String::new(),
            bio: Draft::default(),
        }
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

impl App {
    /// Takes in the core's editor: its card is made again only when what it
    /// would show changed, and goes with the editor.
    pub(super) fn editor_sync(&mut self) {
        let state = self.client.state();
        let Some(core) = state.profile_editor.as_ref() else {
            self.profiles.editor = None;
            return;
        };
        let view = preview(state, core);
        let link = rgb(self.theme.link);
        let editor = self.profiles.editor.take();
        let editor = match editor {
            Some(editor) if editor.shown.look.view == view => editor,
            Some(mut editor) => {
                editor.shown.look = super::look::Look::new(state, &view, None, &[], link);
                editor
            }
            None => Editor::new(Shown::new(super::look::Look::new(
                state,
                &view,
                None,
                &[],
                link,
            ))),
        };
        let mut picked = std::collections::HashMap::new();
        for kind in [ProfilePicture::Avatar, ProfilePicture::Banner] {
            if let (_, Some(preview)) = core.picture(kind) {
                picked.insert(picked_url(kind, core.picture_edits), preview.clone());
            }
        }
        *editor.shown.picked.borrow_mut() = picked;
        // The bio's field follows the core while it isn't being written.
        if editor.editing != Some(Text::Bio) && editor.bio.text() != core.bio {
            editor.bio.reset(&core.bio);
        }
        self.profiles.editor = Some(editor);
    }

    /// The pictures the editor's card shows.
    pub(super) fn editor_pictures(&self) -> Vec<String> {
        let Some(editor) = &self.profiles.editor else {
            return Vec::new();
        };
        let profile = editor.shown.look.profile();
        let cosmetics = self.client.state().cosmetics;
        let mut urls: Vec<String> = [profile.avatar.clone(), profile.banner.clone()]
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
        urls
    }

    /// What's done in the editor.
    pub(super) fn editor_event(&mut self, event: Event) -> Task<Message> {
        let Some(editor) = self.profiles.editor.as_mut() else {
            return Task::none();
        };
        match event {
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
                self.client.dispatch(Action::WriteProfile { field, text });
            }
            Event::Bio(event) => match event {
                field::Event::Edited(text, _) => {
                    self.client.dispatch(Action::WriteProfile {
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
                let state = self.client.state();
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
                return self.editor_event(Event::Pick(kind));
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
                self.client
                    .dispatch(Action::PickProfilePicture { kind, path });
            }
            Event::Reset(kind) => {
                editor.pop = None;
                self.client.dispatch(Action::ResetProfilePicture(kind));
            }
            Event::Pop(pop) => {
                editor.pop = if editor.pop == pop { None } else { pop };
                editor.editing = None;
                editor.tag_query.clear();
                match editor.pop {
                    Some(Pop::Tag) => return operation::focus(Id::new(TAG_SEARCH)),
                    Some(Pop::Top | Pop::Bottom) => {
                        return operation::focus(Id::new(THEME_COLOUR));
                    }
                    _ => return Task::none(),
                }
            }
            Event::Tag(server) => {
                editor.pop = None;
                self.client.dispatch(Action::ChooseServerTag(server));
            }
            Event::TagQuery(query) => {
                editor.tag_query = query;
                return Task::none();
            }
            Event::Tab(tab) => {
                editor.tab = Some(tab);
                if tab == Tab::Wishlist {
                    self.client.dispatch(Action::OpenWishlist);
                }
            }
            Event::Side(side) => editor.side = side,
            Event::Undo => {
                editor.editing = None;
                editor.colour_open = false;
                self.client.dispatch(Action::UndoProfileEdits);
            }
            Event::Save => {
                editor.editing = None;
                self.client.dispatch(Action::SaveProfile);
            }
            Event::Nitro => self.client.dispatch(Action::UnlockNitro),
            Event::CopyId => {
                editor.pop = None;
                let id = editor.shown.look.profile().id.clone();
                return iced::clipboard::write(id);
            }
            Event::Status => self.client.dispatch(Action::OpenStatusEditor),
            Event::Note => {
                let Some(user) = self.client.state().discord_user.clone() else {
                    return Task::none();
                };
                self.client.dispatch(Action::AskPerson {
                    user,
                    ask: yapfast_core::application::PersonAsk::Note,
                    location: "ContextMenu",
                });
            }
            Event::Close => {
                self.client.dispatch(Action::CloseProfileEditor);
                self.profiles.editor = None;
                return Task::batch([self.sync(), self.refocus()]);
            }
        }
        self.sync()
    }

    /// Escape in the editor: what's being edited or open first, then the
    /// editor.
    pub fn editor_escape(&mut self) -> Task<Message> {
        let Some(editor) = self.profiles.editor.as_mut() else {
            return Task::none();
        };
        if editor.editing.is_some() || editor.colour_open || editor.pop.is_some() {
            editor.editing = None;
            editor.colour_open = false;
            editor.pop = None;
            return operation::focus(crate::app::NOBODY);
        }
        self.editor_event(Event::Close)
    }

    pub fn editor_open(&self) -> bool {
        self.profiles.editor.is_some()
    }

    /// The editor over the window dimmed by 60%; a click outside the card
    /// closes it.
    pub fn profile_editor_view(&self) -> Option<Element<'_, Message>> {
        let editor = self.profiles.editor.as_ref()?;
        let core = self.client.state().profile_editor.as_ref()?;
        let theme = &self.theme;
        let main = core.main();
        let (width, height) = (self.window.width, self.window.height);
        let inks = self.inks(&editor.shown.look, Mode::Side);
        let card_width = if main {
            if editor.side {
                (width - 8.0).min(1199.0)
            } else {
                (width - 96.0).min(958.0)
            }
        } else {
            (width - 48.0).min(600.0)
        };
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
        if main && card_width >= 880.0 {
            parts = parts.push(self.editor_tabs(editor, &inks));
        }
        let mut card = stack![
            container(parts)
                .width(card_width)
                .height(card_height)
                .clip(true)
                // No shadow, as Settings' card: the renderer works one this
                // large out pixel by pixel with each redraw.
                .style(super::frame_style(inks, theme.edge, 12.0, false))
        ];
        if main {
            card = card.push(
                container(self.icon_button(
                    Icon::X,
                    "Close",
                    36.0,
                    18,
                    message(Event::Close),
                    true,
                ))
                .width(card_width)
                .align_x(Alignment::End)
                .padding(12),
            );
            if !editor.side {
                card = card.push(placed(self.show_styles(), 0.0, 40.0));
            }
        }
        let dim = Color {
            a: 0.6,
            ..Color::BLACK
        };
        let backdrop = mouse_area(
            container(Space::new())
                .width(Length::Fill)
                .height(Length::Fill)
                .style(move |_| crate::style::fill(dim)),
        )
        .on_press(message(Event::Close));
        Some(opaque(stack![
            opaque(backdrop),
            container(opaque(card)).center(Length::Fill),
        ]))
    }

    /// The card's middle: the banner and avatar, then each text, then the
    /// bar asking to save.
    fn editor_middle<'a>(
        &'a self,
        editor: &'a Editor,
        core: &'a ProfileEditor,
        inks: &super::look::Inks,
    ) -> Element<'a, Message> {
        let theme = &self.theme;
        let main = core.main();
        let ready = !core.loading() && !core.saving;
        let pad = if main { 32.0 } else { 24.0 };
        let mut body = column![]
            .spacing(if main { 28 } else { 16 })
            .padding(Padding {
                left: pad,
                right: if main { 8.0 } else { pad },
                bottom: pad,
                top: 0.0,
            });
        if editor.colour_open {
            body = body.push(self.banner_colour(core));
        }
        body = body.push(self.editor_names(editor, core, inks, ready));
        if main {
            body = body.push(self.own_actions(editor));
        }
        if !main && !core.nitro() {
            body = body.push(self.nitro_offer(core));
        }
        body = body.push(self.editor_bio(editor, core, inks, ready));
        let profile = editor.shown.look.profile();
        if profile.created.is_some() || profile.joined.is_some() {
            body = body.push(
                column![
                    heading(main, "Member Since", inks),
                    self.member_since(&editor.shown, inks)
                ]
                .spacing(6),
            );
        }
        if main {
            body = body.push(
                column![
                    heading(main, "Connections", inks),
                    link_text(
                        "+  Add Connection",
                        inks.ink2,
                        Message::Link(CONNECTIONS.into(), Point::ORIGIN)
                    ),
                ]
                .spacing(6),
            );
            body = body.push(
                column![
                    heading(main, "Note (only visible to you)", inks),
                    hover_edit(
                        text("Click to add a note")
                            .size(15)
                            .color(inks.ink3)
                            .font(iced::Font {
                                style: iced::font::Style::Italic,
                                ..iced::Font::default()
                            }),
                        true,
                        message(Event::Note),
                    )
                ]
                .spacing(6),
            );
        }
        if let Some(error) = &core.error {
            body = body.push(text(error.as_str()).size(14).color(theme.danger));
        }
        let scroll = scrollable(column![self.editor_top(editor, core, inks, pad), body])
            .height(Length::Fill)
            .style(scrollbar(theme));
        let mut middle = column![scroll];
        if core.change().is_some() || core.saving {
            let may_save = ready
                && core.reading.is_none()
                && core.too_long().is_none()
                && core.accent().is_ok();
            middle = middle.push(form::unsaved_bar(
                theme,
                pad,
                core.saving,
                ready,
                may_save,
                form::Shake::default(),
                message(Event::Undo),
                message(Event::Save),
            ));
        }
        middle.into()
    }

    /// The banner with the avatar hanging over its lower edge, the status's
    /// thought beside it, and on a server's what it edits and its close.
    fn editor_top<'a>(
        &'a self,
        editor: &'a Editor,
        core: &'a ProfileEditor,
        inks: &super::look::Inks,
        pad: f32,
    ) -> Element<'a, Message> {
        let main = core.main();
        let ready = !core.loading() && !core.saving;
        let shown = &editor.shown;
        let profile = shown.look.profile();
        let banner_height = if main { 193.0 } else { 210.0 };
        let reading = core.reading.as_ref().map(|(kind, _)| *kind);
        let banner_editable = ready
            && (core.may_picture(ProfilePicture::Banner) || core.may_write(ProfileText::Colour))
            && reading.is_none();
        let banner_locked = (!main || core.nitro()) && !core.may_picture(ProfilePicture::Banner);
        // The banner: the picture, else its colour, else the card's own.
        let ground = shown.look.accent.map(hex).unwrap_or(if main {
            Color::TRANSPARENT
        } else {
            self.theme.raised
        });
        let top_corners = [12.0, 12.0, 0.0, 0.0];
        let mut banner = stack![
            container(Space::new())
                .width(Length::Fill)
                .height(banner_height)
                .style(move |_| rounded(
                    ground,
                    corners(if main { [0.0; 4] } else { top_corners })
                ))
        ];
        let width = 560.0;
        if let Some(still) = profile.banner.as_deref().and_then(|url| {
            self.profile_picture(shown, url, width as u32, banner_height as u32, true)
        }) {
            banner = banner.push(Pic::new(still, Size::new(width, banner_height)));
        }
        if reading == Some(ProfilePicture::Banner) {
            banner = banner.push(
                container(text("Reading...").size(14).font(BOLD).color(Color::WHITE))
                    .center(Length::Fill)
                    .height(banner_height),
            );
        }
        let banner: Element<'a, Message> = if banner_editable || banner_locked {
            let over = stack![
                container(Space::new())
                    .width(Length::Fill)
                    .height(banner_height)
                    .style(|_| rounded(
                        Color {
                            a: 0.35,
                            ..Color::BLACK
                        },
                        0
                    )),
                container(self.edit_badge(banner_locked))
                    .width(Length::Fill)
                    .align_x(Alignment::End)
                    .padding(Padding {
                        top: 18.0,
                        right: 60.0,
                        ..Padding::ZERO
                    }),
            ];
            let face = button(Hover::new(banner, over))
                .padding(0)
                .on_press_maybe(
                    banner_editable.then(|| message(Event::Picture(ProfilePicture::Banner))),
                )
                .style(|_, _| button::Style::default());
            self.picture_menu(editor, core, ProfilePicture::Banner, face.into())
        } else {
            banner.into()
        };
        let avatar = self.editor_avatar(editor, core, inks, ready);
        let mut top = stack![
            Space::new()
                .width(Length::Fill)
                .height(banner_height + AVATAR / 2.0 + 8.0),
            banner,
            placed(avatar, pad - 6.0, banner_height - AVATAR / 2.0 - 6.0),
        ];
        if main {
            top = top.push(self.status_thought(
                inks,
                pad - 6.0 + AVATAR + 12.0,
                banner_height - AVATAR / 2.0 - 6.0,
            ));
        } else {
            let detail = format!("Your profile in {}", core.server_name);
            top = top.push(placed(
                container(text(detail).size(13).font(SEMIBOLD).color(Color::WHITE))
                    .padding([0, 10])
                    .height(28)
                    .align_y(Alignment::Center)
                    .style(|_| {
                        rounded(
                            Color {
                                a: 0.7,
                                ..Color::BLACK
                            },
                            14,
                        )
                    }),
                16.0,
                16.0,
            ));
            let close = button(self.glyph(Icon::X, 18, Color::WHITE).boxed(36))
                .padding(0)
                .width(36)
                .height(36)
                .on_press(message(Event::Close))
                .style(|_, status| button::Style {
                    background: Some(
                        Color {
                            a: if lights(status) { 0.85 } else { 0.7 },
                            ..Color::BLACK
                        }
                        .into(),
                    ),
                    border: Border::default().rounded(18),
                    ..Default::default()
                });
            top = top.push(
                container(close)
                    .width(Length::Fill)
                    .align_x(Alignment::End)
                    .padding(16),
            );
        }
        top.into()
    }

    /// The avatar on its ring, the pencil (or a lock) over it while pointed
    /// at, and its menu.
    fn editor_avatar<'a>(
        &'a self,
        editor: &'a Editor,
        core: &'a ProfileEditor,
        inks: &super::look::Inks,
        ready: bool,
    ) -> Element<'a, Message> {
        let reading = core.reading.as_ref().map(|(kind, _)| *kind) == Some(ProfilePicture::Avatar);
        let enabled = core.may_picture(ProfilePicture::Avatar);
        let editable = ready && enabled && core.reading.is_none();
        let face = self.profile_avatar(&editor.shown, AVATAR, true, inks.ground, Mode::Side);
        let ring = AVATAR + 12.0;
        let disc = move |alpha: f32| {
            container(Space::new())
                .width(AVATAR)
                .height(AVATAR)
                .style(move |_| {
                    rounded(
                        Color {
                            a: alpha,
                            ..Color::BLACK
                        },
                        AVATAR / 2.0,
                    )
                })
        };
        let face: Element<'a, Message> = if reading {
            stack![
                face,
                container(stack![
                    disc(0.6),
                    container(text("Reading...").size(13).font(BOLD).color(Color::WHITE))
                        .center(AVATAR)
                ])
                .padding(6)
            ]
            .into()
        } else if ready {
            let over = container(stack![
                disc(0.45),
                container(self.edit_badge(!enabled)).center(AVATAR)
            ])
            .padding(6);
            Hover::new(face, over).into()
        } else {
            face
        };
        let face = button(face)
            .padding(0)
            .width(ring)
            .height(ring)
            .on_press_maybe(editable.then(|| message(Event::Picture(ProfilePicture::Avatar))))
            .style(|_, _| button::Style::default());
        self.picture_menu(editor, core, ProfilePicture::Avatar, face.into())
    }

    /// A picture's menu, Change and Remove (Reset in a server), under it.
    fn picture_menu<'a>(
        &'a self,
        editor: &'a Editor,
        core: &'a ProfileEditor,
        kind: ProfilePicture,
        base: Element<'a, Message>,
    ) -> Element<'a, Message> {
        let pop = match kind {
            ProfilePicture::Avatar => Pop::Avatar,
            ProfilePicture::Banner => Pop::Banner,
        };
        let word = if core.main() { "Remove" } else { "Reset" };
        let (change, reset) = match kind {
            ProfilePicture::Avatar => ("Change Avatar", format!("{word} Avatar")),
            ProfilePicture::Banner => ("Change Banner", format!("{word} Banner")),
        };
        let list = (editor.pop == Some(pop)).then(|| {
            self.menu_list(vec![
                (
                    item(change.into(), Some(Icon::Image), false),
                    message(Event::Pick(kind)),
                ),
                (
                    item(reset.into(), Some(Icon::X), true),
                    message(Event::Reset(kind)),
                ),
            ])
        });
        let popover = Popover::new(base, list, Side::Below)
            .width(Width::Own(180.0))
            .gap(4.0)
            .outside(message(Event::Pop(None)));
        if kind == ProfilePicture::Banner {
            popover.right().into()
        } else {
            popover.into()
        }
    }

    /// A menu's rows in its panel.
    fn menu_list<'a>(&'a self, rows: Vec<(Item<'a>, Message)>) -> Element<'a, Message> {
        let mut list = column![].spacing(2);
        for (entry, pressed) in rows {
            list = list.push(self.menu_item(entry, Light::Pointer, pressed));
        }
        opaque(
            container(list)
                .padding(6)
                .width(Length::Fill)
                .style(controls::menu_panel(&self.theme)),
        )
    }

    /// The pencil, or the lock where Nitro is wanted, on a dark disc.
    fn edit_badge(&self, locked: bool) -> Element<'_, Message> {
        let icon = if locked { Icon::Lock } else { Icon::Pencil };
        container(self.glyph(icon, 16, Color::WHITE))
            .center(32)
            .style(|_| {
                rounded(
                    Color {
                        a: 0.7,
                        ..Color::BLACK
                    },
                    16,
                )
            })
            .into()
    }

    /// The custom status as a thought beside the avatar, or the offer to set
    /// one; a click opens its dialog.
    fn status_thought<'a>(
        &'a self,
        inks: &super::look::Inks,
        x: f32,
        y: f32,
    ) -> Element<'a, Message> {
        let (bubble, edge) = (inks.bubble, inks.bubble_edge);
        let thought = &self
            .profiles
            .editor
            .as_ref()
            .map(|editor| &editor.shown.look.thought);
        let set = thought.is_some_and(|thought| !thought.is_empty());
        let dot = move |side: f32| {
            container(Space::new())
                .width(side)
                .height(side)
                .style(move |_| container::Style {
                    border: Border {
                        color: edge,
                        width: 1.0,
                        radius: (side / 2.0).into(),
                    },
                    ..rounded(bubble, side / 2.0)
                })
        };
        let mut words = row![].spacing(4).align_y(Alignment::Center);
        if let Some(thought) = thought.filter(|_| set) {
            if let Some(emoji) = &thought.emoji
                && let Some(picture) = self.picture(emoji, 17, 17, false)
            {
                words = words.push(Pic::square(picture, 17.0));
            }
            words = words.push(text(thought.words.as_str()).size(14).color(inks.ink));
        } else {
            words = words.push(text("Set a custom status").size(14).color(inks.ink2));
        }
        let face = button(words)
            .padding([9, 13])
            .on_press(message(Event::Status))
            .style(move |_, _| button::Style {
                background: Some(bubble.into()),
                border: Border {
                    color: edge,
                    width: 1.0,
                    radius: 16.0.into(),
                },
                ..Default::default()
            });
        stack![
            Space::new().width(Length::Fill).height(Length::Fill),
            placed(dot(10.0), x + 2.0, y + 38.0),
            placed(dot(20.0), x + 16.0, y + 45.0),
            placed(container(face).max_width(180.0), x + 6.0, y + 53.0),
        ]
        .into()
    }

    /// The name, then the username with the pronouns and the server tag,
    /// the badges, and what's too long.
    fn editor_names<'a>(
        &'a self,
        editor: &'a Editor,
        core: &'a ProfileEditor,
        inks: &super::look::Inks,
        ready: bool,
    ) -> Element<'a, Message> {
        let theme = &self.theme;
        let main = core.main();
        let shown = &editor.shown;
        let profile = shown.look.profile();
        let mut names = column![].spacing(2);
        if editor.editing == Some(Text::Name) {
            let placeholder = profile.handle.as_str();
            names = names.push(
                form::field(theme, NAME, placeholder, &core.name)
                    .size(22)
                    .padding(8)
                    .on_input(|text| message(Event::Write(ProfileText::Name, text)))
                    .on_submit(message(Event::Edit(None))),
            );
        } else {
            let name: Element<'a, Message> = if core.loading() {
                text("Loading...")
                    .size(if main { 24 } else { 26 })
                    .font(BOLD)
                    .color(inks.ink)
                    .into()
            } else {
                self.profile_name(shown, if main { 24.0 } else { 26.0 }, inks.ink)
            };
            let pencil = self.glyph(Icon::Pen, 15, hex(0xacadb3));
            let enabled = ready && core.may_write(ProfileText::Name);
            names = names.push(hover_edit(
                row![name, Hover::new(Space::new().width(15), pencil)]
                    .spacing(12)
                    .align_y(Alignment::Center),
                enabled,
                message(Event::Edit(Some(Text::Name))),
            ));
        }
        let mut line = row![
            text(format!("{} \u{b7}", profile.handle))
                .size(14)
                .font(MEDIUM)
                .color(inks.ink2)
        ]
        .spacing(6)
        .align_y(Alignment::Center);
        let fallback = core
            .profile
            .as_ref()
            .map_or("", |profile| profile.fallback.pronouns.as_str());
        if editor.editing == Some(Text::Pronouns) {
            let placeholder = if fallback.is_empty() {
                "Add your pronouns"
            } else {
                fallback
            };
            line = line.push(
                form::field(theme, PRONOUNS, placeholder, &core.pronouns)
                    .size(14)
                    .padding(6)
                    .width(220)
                    .on_input(|text| message(Event::Write(ProfileText::Pronouns, text)))
                    .on_submit(message(Event::Edit(None))),
            );
        } else {
            let (words, ink) = if !core.pronouns.is_empty() {
                (core.pronouns.as_str(), inks.ink2)
            } else if !fallback.is_empty() {
                (fallback, inks.ink2)
            } else {
                ("Add pronouns", inks.ink3)
            };
            line = line.push(hover_edit(
                text(words).size(14).font(MEDIUM).color(ink),
                ready,
                message(Event::Edit(Some(Text::Pronouns))),
            ));
            if main {
                line = line.push(self.tag_picker(editor, inks));
            }
        }
        names = names.push(line);
        if main && editor.editing != Some(Text::Pronouns) && !profile.badges.is_empty() {
            names = names.push(container(self.badge_rows(shown)).padding(Padding {
                top: 10.0,
                ..Padding::ZERO
            }));
        }
        let too_long = if core.name.chars().count() > NAME_LIMIT {
            Some(if main {
                format!("A display name can be up to {NAME_LIMIT} characters.")
            } else {
                format!("A nickname can be up to {NAME_LIMIT} characters.")
            })
        } else if core.pronouns.chars().count() > PRONOUNS_LIMIT {
            Some(format!(
                "Pronouns can be up to {PRONOUNS_LIMIT} characters."
            ))
        } else {
            None
        };
        if let Some(words) = too_long {
            names = names.push(text(words).size(13).color(theme.danger));
        }
        names.into()
    }

    /// The server tag worn, which opens the servers whose tags can be.
    fn tag_picker<'a>(
        &'a self,
        editor: &'a Editor,
        inks: &super::look::Inks,
    ) -> Element<'a, Message> {
        let state = self.client.state();
        let profile = editor.shown.look.profile();
        let tag = state.cosmetics.tag(profile.tag.as_ref());
        let ink2 = inks.ink2;
        let mut face = row![].spacing(5).align_y(Alignment::Center);
        face = match tag {
            Some(tag) => face.push(self.tag_chip(
                &tag.text,
                tag.badge.as_deref(),
                crate::looks::Chip {
                    size: 12.0,
                    font: crate::fonts::REGULAR,
                    height: 20.0,
                    pad: 0.0,
                    radius: 0.0,
                    fill: Color::TRANSPARENT,
                    ink: inks.ink,
                },
                1.0,
            )),
            None => face.push(text("Server Tag").size(12).color(ink2).font(iced::Font {
                style: iced::font::Style::Italic,
                ..iced::Font::default()
            })),
        };
        face = face.push(self.glyph(Icon::ChevronDown, 14, ink2));
        let ink = inks.ink;
        let chip = button(face)
            .padding(Padding {
                left: 7.0,
                right: 10.0,
                top: 0.0,
                bottom: 0.0,
            })
            .height(24)
            .on_press(message(Event::Pop(Some(Pop::Tag))))
            .style(move |_, status| button::Style {
                background: Some(faded(ink, if lights(status) { 0.12 } else { 0.07 }).into()),
                border: Border::default().rounded(7),
                ..Default::default()
            });
        let list = (editor.pop == Some(Pop::Tag)).then(|| self.tag_list(editor));
        Popover::new(chip, list, Side::Below)
            .width(Width::Own(270.0))
            .gap(4.0)
            .outside(message(Event::Pop(None)))
            .into()
    }

    /// The servers whose tags can be worn, found by name or tag, under No
    /// Server Tag.
    fn tag_list<'a>(&'a self, editor: &'a Editor) -> Element<'a, Message> {
        let theme = &self.theme;
        let state = self.client.state();
        let chosen = state
            .profile_editor
            .as_ref()
            .and_then(|core| core.chosen_tag.clone())
            .flatten();
        let query = editor.tag_query.trim().to_lowercase();
        let mut rows = column![self.menu_item(
            item("No Server Tag".into(), None, false),
            Light::Pointer,
            message(Event::Tag(String::new())),
        )]
        .spacing(2);
        for server in &state.servers {
            let Some(tag) = &server.tag else {
                continue;
            };
            if !query.is_empty()
                && !server.name.to_lowercase().contains(&query)
                && !tag.text.to_lowercase().contains(&query)
            {
                continue;
            }
            let id = server_id(&server.id);
            let mut entry = item(format!("{}  {}", server.name, tag.text).into(), None, false);
            if chosen.as_deref() == Some(id) {
                entry.check = Check::Current;
            }
            rows = rows.push(self.menu_item(
                entry,
                Light::Pointer,
                message(Event::Tag(id.to_owned())),
            ));
        }
        opaque(
            container(
                column![
                    form::field(theme, TAG_SEARCH, "Search", &editor.tag_query)
                        .size(13)
                        .padding(7)
                        .on_input(|query| message(Event::TagQuery(query))),
                    scrollable(rows)
                        .height(Length::Shrink)
                        .style(scrollbar(theme)),
                ]
                .spacing(6),
            )
            .padding(8)
            .max_height(290)
            .width(Length::Fill)
            .style(controls::menu_panel(theme)),
        )
    }

    /// What others would press on one's own profile: Message can't be sent
    /// to oneself; the shop; More with Copy User ID.
    fn own_actions<'a>(&'a self, editor: &'a Editor) -> Element<'a, Message> {
        let theme = &self.theme;
        let square = |icon: Icon, pressed: Message| {
            let (ground, lit) = (theme.raised, theme.hover);
            button(self.glyph(icon, 18, theme.text).boxed(34))
                .padding(0)
                .width(34)
                .height(34)
                .on_press(pressed)
                .style(move |_, status| button::Style {
                    background: Some(if lights(status) { lit } else { ground }.into()),
                    border: Border::default().rounded(8),
                    ..Default::default()
                })
        };
        let more = (editor.pop == Some(Pop::More)).then(|| {
            self.menu_list(vec![(
                item("Copy User ID".into(), Some(Icon::Copy), false),
                message(Event::CopyId),
            )])
        });
        let more = Popover::new(
            square(Icon::Ellipsis, message(Event::Pop(Some(Pop::More)))),
            more,
            Side::Below,
        )
        .width(Width::Own(170.0))
        .gap(4.0)
        .outside(message(Event::Pop(None)));
        row![
            action(theme, "Message", Kind::Plain, None),
            square(Icon::Shop, Message::Link(SHOP.into(), Point::ORIGIN)),
            more,
        ]
        .spacing(8)
        .into()
    }

    /// Discord's offer, over what Nitro unlocks in a server.
    fn nitro_offer<'a>(&'a self, core: &'a ProfileEditor) -> Element<'a, Message> {
        let theme = &self.theme;
        let sunken = theme.sunken;
        container(
            row![
                self.glyph(Icon::Lock, 20, theme.accent),
                column![
                    text("Make unique profiles for each server")
                        .size(14)
                        .font(BOLD)
                        .color(theme.text),
                    form::note(
                        theme,
                        "Customize your profile for every server, from avatar decorations to bios. Only with Nitro."
                    ),
                ]
                .spacing(2)
                .width(Length::Fill),
                action(
                    theme,
                    if core.upgrade() {
                        "Upgrade Nitro"
                    } else {
                        "Unlock with Nitro"
                    },
                    Kind::Primary,
                    Some(message(Event::Nitro)),
                )
                .small(),
            ]
            .spacing(12)
            .align_y(Alignment::Center),
        )
        .padding(12)
        .style(move |_| rounded(sunken, 8))
        .into()
    }

    /// The banner colour, opened by a click on the banner: its swatch, its
    /// code, Default, and a few to pick from.
    fn banner_colour<'a>(&'a self, core: &'a ProfileEditor) -> Element<'a, Message> {
        let theme = &self.theme;
        let colour = core.accent();
        let wrong = colour.is_err();
        let swatch_fill = colour.ok().flatten().map_or(theme.raised, hex);
        let line = theme.line;
        let swatch = container(Space::new())
            .width(28)
            .height(28)
            .style(move |_| container::Style {
                border: Border {
                    color: line,
                    width: 1.0,
                    radius: 6.0.into(),
                },
                ..rounded(swatch_fill, 6)
            });
        let (raised, danger) = (theme.raised, theme.danger);
        let field = form::field(theme, COLOUR, "Default", &core.colour)
            .size(14)
            .padding(8)
            .width(116)
            .on_input(|text| message(Event::Write(ProfileText::Colour, text)))
            .on_submit(message(Event::Picture(ProfilePicture::Banner)))
            .style({
                let (ink, hint, accent, selection) =
                    (theme.text, theme.muted, theme.accent, theme.selection);
                move |_, status| text_input::Style {
                    background: if wrong { faded(danger, 0.16) } else { raised }.into(),
                    border: Border {
                        color: faded(accent, 0.8),
                        width: if matches!(status, text_input::Status::Focused { .. }) {
                            1.0
                        } else {
                            0.0
                        },
                        radius: 6.0.into(),
                    },
                    icon: ink,
                    placeholder: hint,
                    value: ink,
                    selection,
                }
            });
        let mut head = row![
            text("Banner color")
                .size(14)
                .font(SEMIBOLD)
                .color(theme.text),
            swatch,
            field
        ]
        .spacing(10)
        .align_y(Alignment::Center);
        if wrong {
            head = head.push(text("Not a colour").size(13).color(theme.danger));
        }
        head = head.push(Space::new().width(Length::Fill));
        if !core.colour.is_empty() {
            head = head.push(
                action(
                    theme,
                    "Default",
                    Kind::Ghost,
                    Some(message(Event::Write(ProfileText::Colour, String::new()))),
                )
                .small(),
            );
        }
        let mut swatches = row![].spacing(6);
        let text_ink = theme.text;
        for swatch in SWATCHES {
            let code = format!("#{swatch:06x}");
            let chosen = core.colour == code;
            let fill = hex(swatch);
            swatches = swatches.push(
                button(Space::new())
                    .width(26)
                    .height(26)
                    .on_press(message(Event::Write(ProfileText::Colour, code)))
                    .style(move |_, status| button::Style {
                        background: Some(fill.into()),
                        border: Border {
                            color: text_ink,
                            width: if chosen {
                                2.0
                            } else if lights(status) {
                                1.0
                            } else {
                                0.0
                            },
                            radius: 6.0.into(),
                        },
                        ..Default::default()
                    }),
            );
        }
        let sunken = theme.sunken;
        container(column![head, swatches].spacing(10))
            .padding(10)
            .style(move |_| rounded(sunken, 8))
            .into()
    }

    /// The bio, as it shows or in its field, with what's left of Discord's
    /// limit.
    fn editor_bio<'a>(
        &'a self,
        editor: &'a Editor,
        core: &'a ProfileEditor,
        inks: &super::look::Inks,
        ready: bool,
    ) -> Element<'a, Message> {
        let theme = &self.theme;
        let main = core.main();
        let enabled = core.may_write(ProfileText::Bio);
        let placeholder = if main {
            "Tell everyone a bit about yourself"
        } else {
            "Tell this server a bit about yourself"
        };
        let mut head = row![heading(main, if main { "Bio" } else { "ABOUT ME" }, inks)]
            .spacing(6)
            .align_y(Alignment::Center);
        if !enabled && !core.loading() {
            head = head.push(self.glyph(Icon::Lock, 13, inks.ink3));
        }
        let left = BIO_LIMIT as i64 - core.bio.chars().count() as i64;
        let count = if left < 0 {
            text(format!("Too long by {}", -left))
                .size(12)
                .color(theme.danger)
        } else {
            text(format!("{left} left")).size(12).color(inks.ink3)
        };
        let mut bio = column![head].spacing(6);
        if editor.editing == Some(Text::Bio) {
            let colors = field::Colors {
                ground: theme.sunken,
                text: theme.text,
                placeholder: theme.muted,
                selection: theme.selection,
            };
            let accent = theme.accent;
            let sunken = theme.sunken;
            let field = Field::new(
                &editor.bio,
                Id::new(BIO),
                placeholder.to_owned(),
                |event| message(Event::Bio(event)),
                colors,
            )
            .size(14.0)
            .lines(5.0)
            .padding(Padding::from([8.0, 10.0]));
            bio = bio.push(
                container(field)
                    .height(110)
                    .style(move |_| container::Style {
                        border: Border {
                            color: faded(accent, 0.8),
                            width: 1.0,
                            radius: 6.0.into(),
                        },
                        ..rounded(sunken, 6)
                    }),
            );
            bio = bio.push(
                row![
                    text("You can use markdown and links if you'd like.")
                        .size(12)
                        .color(inks.ink3)
                        .width(Length::Fill),
                    count
                ]
                .spacing(8),
            );
        } else {
            let shown: Element<'a, Message> = match &editor.shown.look.bio {
                Some(body) => self.bio_view(body, inks, Mode::Side),
                None => text(placeholder).size(14).color(inks.ink3).into(),
            };
            bio = bio.push(hover_edit(
                container(shown).width(if main {
                    Length::Fixed(281.0)
                } else {
                    Length::Fill
                }),
                ready && enabled,
                message(Event::Edit(Some(Text::Bio))),
            ));
            if left < 0 {
                bio = bio.push(count);
            }
        }
        bio.into()
    }

    /// The tabs beside the main profile: its Board, Activity and Wishlist,
    /// as the full profile shows them.
    fn editor_tabs<'a>(
        &'a self,
        editor: &'a Editor,
        inks: &super::look::Inks,
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
            let chosen = *entry == tab;
            let (ink, ink2) = (inks.ink, inks.ink2);
            heads = heads.push(
                button(
                    column![
                        text(label).size(14).font(SEMIBOLD),
                        container(Space::new())
                            .width(Length::Fill)
                            .height(2)
                            .style(move |_| if chosen {
                                rounded(ink, 0)
                            } else {
                                container::Style::default()
                            })
                    ]
                    .spacing(8)
                    .width(Length::Shrink),
                )
                .padding(0)
                .on_press(message(Event::Tab(*entry)))
                .style(move |_, _| button::Style {
                    text_color: if chosen { ink } else { ink2 },
                    ..Default::default()
                }),
            );
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

    /// What the main profile wears, down the card's left, in the order
    /// Discord's editor has it; what only Discord's shop sets opens it.
    fn customize<'a>(
        &'a self,
        editor: &'a Editor,
        core: &'a ProfileEditor,
    ) -> Element<'a, Message> {
        let theme = &self.theme;
        let shown = &editor.shown;
        let profile = shown.look.profile();
        let ready = !core.loading() && !core.saving && core.reading.is_none();
        let avatar_enabled = ready && core.may_picture(ProfilePicture::Avatar);
        let banner_enabled = ready && core.may_picture(ProfilePicture::Banner);
        let look_enabled = core.may_write(ProfileText::ThemeTop);
        let shop = || Some(Message::Link(SHOP.into(), Point::ORIGIN));
        let plus = || Element::from(self.glyph(Icon::Plus, 26, Color::WHITE));
        let group = |title: &'a str, body: Element<'a, Message>| {
            column![text(title).size(13).font(SEMIBOLD).color(theme.text), body].spacing(8)
        };
        let nameplate = self.cell(
            row![
                self.avatar(profile.avatar.as_deref(), &profile.name, 26.0, 13.0, 1.0),
                container(Space::new())
                    .width(Length::Fill)
                    .height(12)
                    .style({
                        let hover = theme.hover;
                        move |_| rounded(hover, 6)
                    }),
                self.glyph(Icon::Plus, 22, Color::WHITE),
            ]
            .spacing(8)
            .padding([0, 8])
            .align_y(Alignment::Center)
            .into(),
            42.0,
            None,
            shop(),
        );
        let avatar = self.cell(
            self.avatar(profile.avatar.as_deref(), &profile.name, 72.0, 36.0, 1.0),
            100.0,
            None,
            avatar_enabled.then(|| message(Event::Pick(ProfilePicture::Avatar))),
        );
        let decoration = self.cell(
            if profile.decoration.is_some() {
                self.profile_avatar(shown, 64.0, false, Color::TRANSPARENT, Mode::Card)
            } else {
                plus()
            },
            100.0,
            None,
            shop(),
        );
        let style = self.cell(self.profile_name(shown, 24.0, theme.text), 42.0, None, None);
        let accent = shown.look.accent.map(hex);
        let theme_fill = profile.theme.map(|[top, bottom]| (hex(top), hex(bottom)));
        let mut theme_cell = stack![self.cell(Space::new().into(), 100.0, theme_fill, None)];
        if look_enabled {
            let [top, bottom] = profile.theme.unwrap_or([0x5865f2, 0x5865f2]);
            theme_cell = theme_cell.push(
                container(
                    column![
                        self.theme_handle(editor, core, Pop::Top, hex(top)),
                        Space::new().height(Length::Fill),
                        self.theme_handle(editor, core, Pop::Bottom, hex(bottom)),
                    ]
                    .align_x(Alignment::Center),
                )
                .width(Length::Fill)
                .height(100)
                .padding(4),
            );
        }
        let banner_still = profile
            .banner
            .as_deref()
            .and_then(|url| self.profile_picture(shown, url, 100, 100, true));
        let banner_face: Element<'a, Message> = match banner_still {
            Some(still) => Pic::new(still, Size::new(100.0, 100.0)).radius(6.0).into(),
            None if accent.is_none() => text("No banner").size(13).color(theme.muted).into(),
            None => Space::new().into(),
        };
        let banner = self.cell(
            banner_face,
            100.0,
            accent.map(|accent| (accent, accent)),
            banner_enabled.then(|| message(Event::Pick(ProfilePicture::Banner))),
        );
        let effect = self.cell(
            if profile.effect.is_some() {
                text("Effect on").size(13).color(theme.text).into()
            } else {
                plus()
            },
            100.0,
            None,
            shop(),
        );
        let frame = self.cell(plus(), 100.0, None, shop());
        let line = theme.line;
        let head = row![
            text("Main Profile")
                .size(16)
                .font(SEMIBOLD)
                .color(theme.text)
                .width(Length::Fill),
            self.icon_button(
                Icon::ChevronsRight,
                "Hide Styles",
                30.0,
                16,
                message(Event::Side(false)),
                true,
            ),
        ]
        .padding(Padding {
            left: 16.0,
            right: 8.0,
            ..Padding::ZERO
        })
        .height(50)
        .align_y(Alignment::Center);
        let list = column![
            group("Nameplate", nameplate),
            group(
                "Avatar & Decoration",
                row![avatar, decoration].spacing(8).into()
            ),
            group("Display Name Style", style),
            group("Theme & Banner", row![theme_cell, banner].spacing(8).into()),
            group(
                "Profile Effect & Frame",
                row![effect, frame].spacing(8).into()
            ),
        ]
        .spacing(16)
        .padding(16);
        let panel = theme.panel;
        container(column![
            head,
            container(Space::new())
                .width(Length::Fill)
                .height(1)
                .style(move |_| rounded(line, 0)),
            scrollable(list)
                .height(Length::Fill)
                .style(scrollbar(theme)),
        ])
        .width(SIDE)
        .height(Length::Fill)
        .style(move |_| crate::style::fill(panel))
        .into()
    }

    /// A tile of the styles column: raised, lit under the pointer while it
    /// does something, filled with a colour or a gradient's two.
    fn cell<'a>(
        &self,
        content: Element<'a, Message>,
        height: f32,
        fill: Option<(Color, Color)>,
        pressed: Option<Message>,
    ) -> Element<'a, Message> {
        let (raised, hover) = (self.theme.raised, self.theme.hover);
        let enabled = pressed.is_some();
        button(container(content).center(Length::Fill).clip(true))
            .padding(0)
            .width(Length::Fill)
            .height(height)
            .on_press_maybe(pressed)
            .style(move |_, status| button::Style {
                background: Some(match fill {
                    Some((top, bottom)) => {
                        iced::gradient::Linear::new(iced::Radians(std::f32::consts::PI))
                            .add_stop(0.0, top)
                            .add_stop(1.0, bottom)
                            .into()
                    }
                    None => if enabled && lights(status) {
                        hover
                    } else {
                        raised
                    }
                    .into(),
                }),
                border: Border::default().rounded(6),
                ..Default::default()
            })
            .into()
    }

    /// A theme colour's handle on its tile, which opens its code.
    fn theme_handle<'a>(
        &'a self,
        editor: &'a Editor,
        core: &'a ProfileEditor,
        pop: Pop,
        fill: Color,
    ) -> Element<'a, Message> {
        let open = editor.pop == Some(pop);
        let handle = button(Space::new())
            .width(24)
            .height(24)
            .on_press(message(Event::Pop(Some(pop))))
            .style(move |_, _| button::Style {
                background: Some(fill.into()),
                border: Border {
                    color: Color::WHITE,
                    width: if open { 3.0 } else { 2.0 },
                    radius: 7.0.into(),
                },
                ..Default::default()
            });
        let (field, label) = match pop {
            Pop::Top => (ProfileText::ThemeTop, core.theme_top.as_str()),
            _ => (ProfileText::ThemeBottom, core.theme_bottom.as_str()),
        };
        let theme = &self.theme;
        let code = open.then(|| {
            let raised = theme.raised;
            let edge = theme.edge;
            opaque(
                container(
                    column![
                        text("Colour code")
                            .size(12)
                            .font(SEMIBOLD)
                            .color(theme.secondary),
                        form::field(theme, THEME_COLOUR, "#rrggbb", label)
                            .size(14)
                            .on_input(move |text| message(Event::Write(field, text)))
                            .on_submit(message(Event::Pop(None))),
                    ]
                    .spacing(6),
                )
                .padding(10)
                .width(Length::Fill)
                .style(move |_| container::Style {
                    border: Border {
                        color: edge,
                        width: 1.0,
                        radius: 12.0.into(),
                    },
                    shadow: card_shadow(),
                    ..rounded(raised, 12)
                }),
            )
        });
        Popover::new(handle, code, Side::Below)
            .width(Width::Own(240.0))
            .gap(6.0)
            .outside(message(Event::Pop(None)))
            .into()
    }

    /// Show Styles: a tab on the card's left edge once the column has gone.
    fn show_styles(&self) -> Element<'_, Message> {
        let edge = self.theme.edge;
        let face = button(self.glyph(Icon::Brush, 18, Color::WHITE).boxed(36))
            .padding(Padding {
                left: 7.0,
                ..Padding::ZERO
            })
            .width(42)
            .height(36)
            .on_press(message(Event::Side(true)))
            .style(move |_, _| button::Style {
                background: Some(Color::BLACK.into()),
                border: Border {
                    color: edge,
                    width: 1.0,
                    radius: 10.0.into(),
                },
                ..Default::default()
            });
        iced::widget::tooltip(
            face,
            controls::tip(&self.theme, "Show Styles"),
            iced::widget::tooltip::Position::Top,
        )
        .gap(6)
        .into()
    }
}

/// A menu row.
fn item<'a>(label: std::borrow::Cow<'a, str>, icon: Option<Icon>, danger: bool) -> Item<'a> {
    Item {
        label,
        icon,
        danger,
        slot: false,
        disabled: false,
        subtitle: None,
        check: Check::None,
    }
}

/// A section's heading on the card: the main profile's in its card's
/// words, a server's in capitals.
fn heading<'a>(main: bool, words: &'a str, inks: &super::look::Inks) -> Element<'a, Message> {
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
