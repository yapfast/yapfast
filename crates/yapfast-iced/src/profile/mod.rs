//! Profiles, as the Slint client's `ProfileCard` and `ProfileModal`: the
//! card a member's line, a message's author or a friend opens; the same
//! card docked beside a one-to-one DM; and the full profile over the
//! window, the card down its left and tabs of what they do and share.
//!
//! [`look`] works out what a profile shows once each time the core's view
//! of it changes; `body` draws the card in each of its three places;
//! `full` the full profile's tabs; `activity` an activity's card;
//! [`editor`] the profile editor, drawn with the card's parts.

mod activity;
mod body;
pub mod editor;
mod full;
pub mod look;

use std::{cell::RefCell, collections::HashMap, sync::Arc};

use iced::{
    Element, Length, Point, Task,
    advanced::{graphics::text::Version, image::Handle},
    widget::{Space, column, container, mouse_area, opaque, scrollable, stack},
};
use yapfast_core::{
    application::{Action, Navigation, ProfileTab, ProfileView},
    chat,
};

use crate::{
    app::{App, Message},
    controls::{Kind, action},
    looks,
    paint::{self, StyledPicture},
    pictures::shared,
    style::{card_shadow, fill, scrollbar},
    widgets::Place,
};
use look::{Look, Tab};

/// The card's width beside a list.
pub const WIDTH: f32 = 300.0;
/// The panel beside a one-to-one DM, and the card's inset in it.
pub const PANEL_WIDTH: f32 = 320.0;
const PANEL_INSET: f32 = 8.0;
/// The panel's View Full Profile button, and the room it keeps.
const PANEL_BUTTON: f32 = 36.0;
const PANEL_FOOT: f32 = 52.0;
/// The message box's id, and the note's.
pub const MESSAGE_FIELD: &str = "profile-message";
pub const NOTE_FIELD: &str = "profile-note";

/// Where a profile shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// A card beside a list or a message.
    Card,
    /// The full profile's left column.
    Side,
    /// Docked beside a one-to-one DM.
    Docked,
}

impl Mode {
    /// It shows everything: the bio in full, the dates.
    fn full(self) -> bool {
        self != Mode::Card
    }

    fn large(self) -> bool {
        self == Mode::Side
    }
}

/// What's done on a profile.
#[derive(Clone, Debug)]
pub enum Event {
    /// The full profile, on a tab or the first: from the card, or from the
    /// DM's panel.
    Enlarge(Option<Tab>, bool),
    Close,
    Befriend(String),
    Message(String),
    /// Their menu, from the More or Friends button, where it was pressed.
    More(String),
    /// The message box's text, and Enter in it.
    Field(String),
    Send,
    /// The note's text, and Enter in it.
    Note(String),
    SaveNote,
    Tab(Tab),
    /// A mutual friend's profile: in full from the full profile.
    Friend(String),
    Server(String),
    /// The pointer on the card (its decoration plays), on the docked one,
    /// and on the status's bubble (it shows whole).
    Pointed(bool),
    PanelPointed(bool),
    Bubble(bool),
    /// The header's profile button: the panel beside a DM shows or goes.
    TogglePanel,
    /// Edit Profile on one's own: the card or full profile goes and the
    /// editor opens.
    EditProfile,
    Editor(editor::Event),
}

/// The floating card: whose, where.
pub struct Card {
    pub user: String,
    pub at: Point,
}

/// A profile on screen: what it shows, and what it alone draws, let go of
/// when it closes.
pub struct Shown {
    pub look: Look,
    /// Its pictures at the sizes it draws them, by address.
    pictures: RefCell<HashMap<(String, u32, u32), Handle>>,
    /// The name drawn with its whole style, and what it was drawn of.
    name: RefCell<Option<StyledName>>,
    /// Pictures chosen in the editor and not yet sent, by the names its
    /// card gives them.
    picked: RefCell<HashMap<String, Arc<yapfast_core::chat::Picture>>>,
}

impl Shown {
    fn new(look: Look) -> Self {
        Self {
            look,
            pictures: Default::default(),
            name: Default::default(),
            picked: Default::default(),
        }
    }
}

struct StyledName {
    name: String,
    /// The style, by where the core keeps it, its size, the font system's
    /// version (a font loaded since for a script the name uses draws it
    /// again), and whether the theme is dark, which its colours are made
    /// readable on.
    key: (usize, u32, Version, bool),
    image: Handle,
    picture: StyledPicture,
}

/// What the profiles on screen keep beyond the core's state.
#[derive(Default)]
pub struct Profiles {
    /// The core's `profile`, as the card or the full profile shows it.
    pub shown: Option<Shown>,
    /// The core's `panel_profile`, beside a one-to-one DM.
    pub panel: Option<Shown>,
    /// The full profile's tab chosen, and the tab and person told to the
    /// core last.
    pub tab: Option<Tab>,
    told: Option<(String, Tab)>,
    /// The message box's and the note's text, and whose note it is.
    pub field: String,
    pub note: String,
    note_of: String,
    pub pointed: bool,
    pub panel_pointed: bool,
    pub bubble: bool,
    /// The profile editor, while the core has one open.
    pub editor: Option<editor::Editor>,
}

impl App {
    pub fn open_card(&mut self, user: String, at: Point) -> Task<Message> {
        self.client.dispatch(Action::ShowProfile {
            user: user.clone(),
            large: false,
        });
        self.card = Some(Card { user, at });
        self.profiles.field.clear();
        self.sync()
    }

    /// Takes in the core's profiles: each one's look is made again only
    /// when its view changed. The full profile tells the core which tab
    /// shows, once per tab and person.
    pub fn profile_sync(&mut self) -> Task<Message> {
        let state = self.client.state();
        // The card the core closed, or one shown large, goes.
        if self.card.is_some() && state.profile.as_ref().is_none_or(|view| view.large) {
            self.card = None;
        }
        let link = crate::theme::rgb(self.theme.link);
        let (shown, panel) = (self.profiles.shown.take(), self.profiles.panel.take());
        let app = &self.client.app;
        let state = &app.state;
        let panel_view = state.panel_profile.as_ref().filter(|_| state.profile_panel);
        self.profiles.shown = make_shown(app, shown, state.profile.as_ref(), None, link);
        let live = panel_view
            .and(state.selected_room())
            .map(|room| look::Live {
                presence: room.presence,
                activity: room.activity.clone(),
                activities: room.activities.clone(),
            });
        self.profiles.panel = make_shown(app, panel, panel_view, live, link);
        self.editor_sync();
        // The note follows the profile's while it isn't being written.
        if let Some(profile) = self
            .profiles
            .shown
            .as_ref()
            .filter(|shown| shown.look.view.large)
            .map(|shown| shown.look.profile())
        {
            if self.profiles.note_of != profile.id {
                self.profiles.note_of = profile.id.clone();
                self.profiles.note = profile.note.clone();
            }
        } else {
            self.profiles.note_of.clear();
        }
        self.tell_tab()
    }

    /// The full profile's tab, once per tab and person: Board, Activity and
    /// Wishlist ask Discord for what they show.
    fn tell_tab(&mut self) -> Task<Message> {
        let Some(shown) = self
            .profiles
            .shown
            .as_ref()
            .filter(|shown| shown.look.view.large)
        else {
            self.profiles.told = None;
            return Task::none();
        };
        let tab = self.profile_tab(&shown.look);
        let told = (shown.look.profile().id.clone(), tab);
        if self.profiles.told.as_ref() == Some(&told) {
            return Task::none();
        }
        self.profiles.told = Some(told);
        let tab = match tab {
            Tab::Activity => ProfileTab::Activity,
            Tab::Board => ProfileTab::Board,
            Tab::Wishlist => ProfileTab::Wishlist,
            Tab::Friends | Tab::Servers => return Task::none(),
        };
        self.client.dispatch(Action::OpenProfileTab(tab));
        Task::none()
    }

    /// The tab the full profile shows: the one chosen while it's there,
    /// else the first.
    pub fn profile_tab(&self, look: &Look) -> Tab {
        self.profiles
            .tab
            .filter(|tab| look.tabs.contains(tab))
            .unwrap_or(look.tabs[0])
    }

    /// What's done on a profile.
    pub fn profile_event(&mut self, event: Event) -> Task<Message> {
        match event {
            Event::Enlarge(tab, panel) => {
                self.profiles.tab = tab;
                if panel {
                    let Some(room) = self.client.state().selected.clone() else {
                        return Task::none();
                    };
                    self.client.dispatch(Action::ShowDmProfile(room));
                } else {
                    self.client.dispatch(Action::EnlargeProfile);
                }
                self.card = None;
            }
            Event::Close => {
                self.client.dispatch(Action::CloseProfile);
                self.card = None;
                return Task::batch([self.sync(), self.refocus()]);
            }
            Event::Befriend(user) => self.client.dispatch(Action::Befriend(user)),
            Event::Message(user) => {
                self.client.dispatch(Action::CloseProfile);
                self.client.dispatch(Action::MessagePerson(user));
            }
            Event::More(user) => {
                let at = self.pressed.get();
                return self.open_person_menu(&user, at, crate::person::PROFILE_OVERFLOW);
            }
            Event::Field(text) => {
                self.profiles.field = text;
                return Task::none();
            }
            Event::Send => {
                let user = self
                    .card
                    .as_ref()
                    .map(|card| card.user.clone())
                    .or_else(|| {
                        self.profiles
                            .panel
                            .as_ref()
                            .map(|shown| shown.look.profile().id.clone())
                    });
                let Some(user) = user else {
                    return Task::none();
                };
                if self.profiles.field.trim().is_empty() {
                    return Task::none();
                }
                let text = std::mem::take(&mut self.profiles.field);
                self.card = None;
                self.client
                    .dispatch(Action::MessageFromProfile { user, text });
            }
            Event::Note(text) => {
                self.profiles.note = text;
                return Task::none();
            }
            Event::SaveNote => {
                let Some(shown) = &self.profiles.shown else {
                    return Task::none();
                };
                let profile = shown.look.profile();
                if self.profiles.note != profile.note {
                    let (user, note) = (profile.id.clone(), self.profiles.note.clone());
                    self.client.dispatch(Action::SaveNote { user, note });
                }
            }
            Event::Tab(tab) => {
                self.profiles.tab = Some(tab);
                return Task::batch([self.tell_tab(), self.sync()]);
            }
            Event::Friend(user) => {
                let large = self
                    .profiles
                    .shown
                    .as_ref()
                    .is_some_and(|shown| shown.look.view.large);
                self.profiles.tab = None;
                self.client
                    .dispatch(Action::ShowFriendProfile { user, large });
            }
            Event::Server(server) => {
                self.client.dispatch(Action::CloseProfile);
                self.card = None;
                self.client
                    .dispatch(Action::Navigate(Navigation::Server(server)));
            }
            Event::Pointed(pointed) => {
                self.profiles.pointed = pointed;
                return self.ask();
            }
            Event::PanelPointed(pointed) => {
                self.profiles.panel_pointed = pointed;
                return self.ask();
            }
            Event::Bubble(pointed) => {
                self.profiles.bubble = pointed;
                return Task::none();
            }
            Event::EditProfile => {
                self.client.dispatch(Action::CloseProfile);
                self.card = None;
                self.client.dispatch(Action::EditProfile);
            }
            Event::Editor(event) => return self.editor_event(event),
            Event::TogglePanel => {
                let shown = !self.client.state().profile_panel;
                crate::layout::store(self.client.root(), "profile_panel_hidden", !shown);
                self.client.dispatch(Action::ShowProfilePanel(shown));
            }
        }
        self.sync()
    }

    /// The floating card, beside its list or message, as far up as the
    /// window lets it; it scrolls once it's taller than the window.
    pub fn card_view(&self) -> Option<Element<'_, Message>> {
        let card = self.card.as_ref()?;
        let shown = self
            .profiles
            .shown
            .as_ref()
            .filter(|shown| !shown.look.view.large)?;
        let body = self.profile_body(shown, Mode::Card, self.profiles.pointed);
        let limit = self.window.height - 16.0;
        let content = container(scrollable(body).style(scrollbar(&self.theme)))
            .width(WIDTH)
            .max_height(limit);
        let pointer = mouse_area(content)
            .on_enter(Message::Profile(Event::Pointed(true)))
            .on_exit(Message::Profile(Event::Pointed(false)));
        Some(Place::new(opaque(pointer), card.at, 8.0).into())
    }

    /// The full profile over the window, dimmed under it; a click outside
    /// closes it.
    pub fn full_profile_view(&self) -> Option<Element<'_, Message>> {
        let shown = self
            .profiles
            .shown
            .as_ref()
            .filter(|shown| shown.look.view.large)?;
        let (window_width, window_height) = (self.window.width, self.window.height);
        let width = (window_width * 0.7).max(720.0).min(window_width - 32.0);
        let height = (window_height * 0.75).max(520.0).min(window_height - 32.0);
        let modal = self.full_view(shown, width, height);
        let dim = iced::Color {
            a: 0.7,
            ..iced::Color::BLACK
        };
        let backdrop = mouse_area(
            container(Space::new())
                .width(Length::Fill)
                .height(Length::Fill)
                .style(move |_| fill(dim)),
        )
        .on_press(Message::Profile(Event::Close));
        Some(opaque(stack![
            opaque(backdrop),
            container(opaque(modal)).center(Length::Fill),
        ]))
    }

    /// The profile docked beside a one-to-one DM, filling its height, with
    /// View Full Profile at its foot.
    pub fn profile_panel_view(&self) -> Option<Element<'_, Message>> {
        let shown = self.profiles.panel.as_ref()?;
        let theme = &self.theme;
        let inks = self.inks(&shown.look, Mode::Docked);
        let body = self.profile_body(shown, Mode::Docked, self.profiles.panel_pointed);
        let scroll = scrollable(body)
            .height(Length::Fill)
            .style(scrollbar(theme));
        let foot = container(
            action(
                theme,
                "View Full Profile",
                Kind::Plain,
                Some(Message::Profile(Event::Enlarge(None, true))),
            )
            .wide(PANEL_BUTTON),
        )
        .padding([0.0, 16.0])
        .height(PANEL_FOOT);
        let card = container(column![scroll, foot])
            .width(Length::Fill)
            .height(Length::Fill)
            .clip(true)
            .style(frame_style(inks, theme.edge, 8.0, false));
        let card = mouse_area(card)
            .on_enter(Message::Profile(Event::PanelPointed(true)))
            .on_exit(Message::Profile(Event::PanelPointed(false)));
        Some(
            container(card)
                .padding(PANEL_INSET)
                .width(PANEL_WIDTH)
                .height(Length::Fill)
                .into(),
        )
    }

    /// Whether the profile panel shows beside the open DM.
    pub fn panel_shown(&self) -> bool {
        self.header.profile_available && self.profiles.panel.is_some()
    }

    /// A picture for a profile at its size, made once while it's shown.
    fn profile_picture(
        &self,
        shown: &Shown,
        url: &str,
        width: u32,
        height: u32,
        cover: bool,
    ) -> Option<Handle> {
        let key = (url.to_owned(), width, height);
        if let Some(handle) = shown.pictures.borrow().get(&key) {
            return Some(handle.clone());
        }
        let picked = shown.picked.borrow().get(url).cloned();
        let picture = match &picked {
            Some(picture) => picture,
            None => self.client.state().pictures.get(url)?,
        };
        if picture.width == 0 || picture.height == 0 {
            return None;
        }
        let handle = if picture.width == width && picture.height == height {
            shared(picture)
        } else {
            let pixels = paint::fitted(
                &picture.rgba,
                (picture.width, picture.height),
                (width, height),
                cover,
            );
            Handle::from_rgba(width, height, pixels)
        };
        shown.pictures.borrow_mut().insert(key, handle.clone());
        Some(handle)
    }

    /// The pictures the profiles on screen show, and the moving ones of
    /// those under the pointer.
    pub fn profile_pictures(&self) -> Vec<String> {
        let state = self.client.state();
        let cosmetics = state.cosmetics;
        let mut urls = self.editor_pictures();
        for (shown, pointed) in [
            (&self.profiles.shown, self.profiles.pointed),
            (&self.profiles.panel, self.profiles.panel_pointed),
        ] {
            let Some(shown) = shown else {
                continue;
            };
            let profile = shown.look.profile();
            urls.extend(profile.avatar.clone());
            let decoration = cosmetics.decoration(profile.decoration.as_deref());
            urls.extend(decoration.map(str::to_owned));
            if pointed || shown.look.view.large {
                urls.extend(decoration.map(looks::animated_url));
            }
            urls.extend(profile.banner.clone());
            urls.extend(profile.banner.as_deref().and_then(chat::moving_banner));
            urls.extend(
                cosmetics
                    .tag(profile.tag.as_ref())
                    .and_then(|tag| tag.badge.as_deref())
                    .map(str::to_owned),
            );
            urls.extend(shown.look.thought.emoji.clone());
            urls.extend(profile.badges.iter().map(|badge| badge.icon.clone()));
            urls.extend(profile.server_icon.clone());
            for activity in &shown.look.activities {
                urls.extend(activity.large.clone());
                urls.extend(activity.small.clone());
                urls.extend(activity.name_emoji.clone());
            }
            if let Some(bio) = &shown.look.bio {
                let emoji = bio.parts.iter().flat_map(|part| &part.runs);
                urls.extend(emoji.filter_map(|run| run.emoji.as_ref().map(|(url, _)| url.clone())));
            }
            let friends = profile.friends.iter();
            urls.extend(friends.take(3).filter_map(|friend| friend.avatar.clone()));
            if !shown.look.view.large {
                continue;
            }
            match self.profile_tab(&shown.look) {
                Tab::Friends => {
                    urls.extend(
                        profile
                            .friends
                            .iter()
                            .filter_map(|friend| friend.avatar.clone()),
                    );
                }
                Tab::Servers => {
                    urls.extend(
                        profile
                            .servers
                            .iter()
                            .filter_map(|server| server.icon.clone()),
                    );
                }
                Tab::Activity => {
                    urls.extend(
                        profile
                            .recent
                            .iter()
                            .filter_map(|recent| recent.picture.clone()),
                    );
                }
                Tab::Board => {
                    for widget in &profile.widgets {
                        urls.extend(widget.games.iter().filter_map(|game| game.icon.clone()));
                    }
                }
                Tab::Wishlist => {
                    if let chat::Wishlist::Loaded(items) = &profile.wishlist {
                        urls.extend(items.iter().filter_map(|item| item.picture.clone()));
                    }
                }
            }
        }
        urls
    }

    /// The profile's ink, ground and the rest, from its theme or Yapfast's.
    pub fn inks(&self, look: &Look, mode: Mode) -> look::Inks {
        let plain = if mode == Mode::Docked {
            self.theme.panel
        } else {
            self.theme.sidebar
        };
        look::Inks::new(&self.theme, look.profile().theme, plain)
    }

    /// Whether a shown profile times an activity, which a clock brings up
    /// to date each second.
    pub fn profile_ticks(&self) -> bool {
        [&self.profiles.shown, &self.profiles.panel]
            .into_iter()
            .flatten()
            .any(|shown| shown.look.activities.iter().any(look::ActivityLook::timed))
    }
}

/// The card's frame: its ground (a theme's gradient, or plain), corners and
/// outline, and the shadow of one that floats.
pub(crate) fn frame_style(
    inks: look::Inks,
    edge: iced::Color,
    radius: f32,
    floats: bool,
) -> impl Fn(&iced::Theme) -> container::Style {
    move |_| {
        let background: iced::Background = if inks.themed {
            iced::gradient::Linear::new(iced::Radians(std::f32::consts::PI))
                .add_stop(0.0, inks.start)
                .add_stop(1.0, inks.end)
                .into()
        } else {
            inks.ground.into()
        };
        container::Style {
            background: Some(background),
            border: iced::Border {
                color: edge,
                width: if inks.themed { 0.0 } else { 1.0 },
                radius: radius.into(),
            },
            shadow: if floats {
                card_shadow()
            } else {
                iced::Shadow::default()
            },
            ..Default::default()
        }
    }
}

/// A profile's look for `view`, made again only when the view changed; the
/// same person's pictures and name stay drawn.
fn make_shown(
    app: &yapfast_core::application::Application,
    shown: Option<Shown>,
    view: Option<&ProfileView>,
    live: Option<look::Live>,
    link: u32,
) -> Option<Shown> {
    let view = view?;
    let same = |shown: &Shown| {
        shown.look.view == *view
            && shown.look.relation == app.state.relation(&view.profile.id)
            && shown.look.live == live
    };
    if shown.as_ref().is_some_and(same) {
        return shown;
    }
    let look = || {
        let choices = app.person_choices(&view.profile.id);
        Look::new(&app.state, view, live, &choices, link)
    };
    Some(match shown {
        Some(mut shown) if shown.look.view.profile.id == view.profile.id => {
            shown.look = look();
            shown
        }
        _ => Shown::new(look()),
    })
}
