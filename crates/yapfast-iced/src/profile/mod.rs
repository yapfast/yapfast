//! Profiles, as the Slint client's `ProfileCard` and `ProfileModal`: the
//! card a member's line, a message's author or a friend opens; the same
//! card docked beside a one-to-one DM; and the full profile over the
//! window, the card down its left and tabs of what they do and share.
//!
//! [`look`] works out what a profile shows once each time the core's view
//! of it changes; `body` draws the card in each of its three places;
//! `full` the full profile's tabs; `activity` an activity's card;
//! [`editor`] the profile editor, drawn with the card's parts, and
//! [`name_style`] its Display Name Style window.

mod activity;
mod body;
mod cosmetic;
pub mod editor;
mod full;
pub mod look;
pub mod name_style;

use std::{cell::RefCell, collections::HashMap, sync::Arc};

use iced::{
    Element, Length, Point, Task,
    advanced::{graphics::text::Version, image::Handle},
    widget::{column, container, mouse_area, opaque, scrollable, stack},
};
use yapfast_core::{
    application::{Action, ProfileTab, ProfileView},
    chat,
};

use crate::{
    app::{App, Message},
    controls::{Kind, action},
    looks, paint,
    pictures::shared,
    style::{card_shadow, scrollbar},
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
    /// An activity's more menu on the card, with Manage Privacy for one's
    /// own game.
    ActivityMenu(bool),
    /// Its View All Activity and Manage Privacy.
    AllActivity,
    ActivityPrivacy,
    /// The pointer on the card (its decoration plays), on the docked one,
    /// and on the status's bubble (it shows whole).
    Pointed(bool),
    PanelPointed(bool),
    Bubble(bool),
    /// A layer of a playing effect shows or goes, by the clock's
    /// generation it was asked in.
    EffectTick(u64),
    /// The header's profile button: the panel beside a DM shows or goes.
    TogglePanel,
    /// Edit Profile on one's own: the card or full profile goes and the
    /// editor opens.
    EditProfile,
    Editor(editor::Event),
}

/// An activity's more menu's width, as the Slint client's.
const ACTIVITY_MENU: f32 = 170.0;

/// The floating card: whose, where.
pub struct Card {
    pub user: String,
    pub at: Point,
}

/// One picture's handles, by the size each is drawn at.
type Sizes = HashMap<(u32, u32), Handle>;

/// A profile on screen: what it shows, and what it alone draws, let go of
/// when it closes.
pub struct Shown {
    pub look: Look,
    /// Its pictures at the sizes it draws them, by address.
    pictures: RefCell<HashMap<String, Sizes>>,
    /// The name drawn with its whole style, and what it was drawn of.
    name: RefCell<Option<StyledName>>,
    /// Pictures chosen in the editor and not yet sent, by the names its
    /// card gives them.
    picked: RefCell<HashMap<String, Arc<yapfast_core::chat::Picture>>>,
    /// The avatar's colour, by its address, once it has arrived.
    avatar_colour: RefCell<Option<(String, u32)>>,
}

impl Shown {
    fn new(look: Look) -> Self {
        Self {
            look,
            pictures: Default::default(),
            name: Default::default(),
            picked: Default::default(),
            avatar_colour: Default::default(),
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
    picture: paint::NamePicture,
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
    /// When the cards' effects began to play.
    effect: cosmetic::EffectClock,
    /// The profile editor, while the core has one open.
    pub editor: Option<editor::Editor>,
    /// The account's own profile as its popover shows it, while it's open.
    pub account: Option<(Arc<chat::Profile>, Shown)>,
}

impl App {
    pub fn open_card(&mut self, user: String, at: Point) -> Task<Message> {
        self.client.dispatch(Action::ShowProfile {
            user: user.clone(),
            large: false,
        });
        self.card = Some(Card { user, at });
        self.profiles.field.clear();
        self.unpoint_card();
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
        self.account_card_sync(link);
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
    /// The note typed on the shown profile goes to the core, if it
    /// changed: as Enter does, and as the Slint client's does when the
    /// field loses the keys, before the profile closes or changes.
    fn save_note(&mut self) {
        let Some(shown) = &self.profiles.shown else {
            return;
        };
        let profile = shown.look.profile();
        if self.profiles.note != profile.note {
            let (user, note) = (profile.id.clone(), self.profiles.note.clone());
            self.client.dispatch(Action::SaveNote { user, note });
        }
    }

    /// Escape: what was typed in the note goes back to what's kept.
    pub fn revert_note(&mut self) {
        if let Some(shown) = &self.profiles.shown {
            self.profiles.note = shown.look.profile().note.clone();
        }
    }

    pub fn profile_event(&mut self, event: Event) -> Task<Message> {
        match event {
            Event::Enlarge(tab, panel) => {
                self.save_note();
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
                self.unpoint_card();
            }
            Event::Close => {
                self.save_note();
                self.client.dispatch(Action::CloseProfile);
                self.card = None;
                return Task::batch([self.sync(), self.refocus()]);
            }
            Event::Befriend(user) => self.client.dispatch(Action::Befriend(user)),
            Event::Message(user) => {
                self.save_note();
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
            Event::SaveNote => self.save_note(),
            Event::Tab(tab) => {
                self.profiles.tab = Some(tab);
                return Task::batch([self.tell_tab(), self.sync()]);
            }
            Event::Friend(user) => {
                self.save_note();
                let large = self
                    .profiles
                    .shown
                    .as_ref()
                    .is_some_and(|shown| shown.look.view.large);
                self.profiles.tab = None;
                self.client
                    .dispatch(Action::ShowFriendProfile { user, large });
            }
            Event::ActivityMenu(privacy) => {
                let mut rows = vec![crate::menus::MenuRow::plain(
                    crate::menus::MenuAct::AllActivity,
                    "View All Activity",
                )];
                if privacy {
                    rows.push(crate::menus::MenuRow::plain(
                        crate::menus::MenuAct::ActivityPrivacy,
                        "Manage Privacy",
                    ));
                }
                let at = self.pressed.get();
                self.menu = Some(
                    crate::menus::Menu::new(Point::new(at.x + 4.0, at.y - 8.0), rows)
                        .width(ACTIVITY_MENU),
                );
                return iced::widget::operation::focus(crate::app::NOBODY);
            }
            // The card's person's full profile, on its Activity tab.
            Event::AllActivity => {
                return self.profile_event(Event::Enlarge(Some(Tab::Activity), false));
            }
            Event::ActivityPrivacy => {
                self.client.dispatch(Action::CloseProfile);
                self.card = None;
                return self.open_settings(crate::settings::Section::Games);
            }
            // Its card over the profile, which goes there.
            Event::Server(server) => {
                return self.info_event(crate::info_cards::Event::Server(server));
            }
            Event::Pointed(pointed) => {
                self.profiles.pointed = pointed;
                return Task::batch([self.point_effect(false, pointed), self.ask()]);
            }
            Event::PanelPointed(pointed) => {
                self.profiles.panel_pointed = pointed;
                return Task::batch([self.point_effect(true, pointed), self.ask()]);
            }
            Event::EffectTick(generation) => return self.effect_tick(generation),
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
        let body = self.profile_body(shown, Mode::Card, WIDTH, self.profiles.pointed);
        let limit = self.window.height - 16.0;
        let content = container(scrollable(body).style(scrollbar(&self.theme)))
            .width(WIDTH)
            .max_height(limit);
        let content = self.dressed(shown, Mode::Card, WIDTH, content.into());
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
        // Over the window dimmed as under a settings card.
        Some(opaque(stack![
            crate::controls::dimmed(Message::Profile(Event::Close), crate::sheet::DIM),
            container(opaque(modal)).center(Length::Fill),
        ]))
    }

    /// The profile docked beside a one-to-one DM, filling its height, with
    /// View Full Profile at its foot.
    pub fn profile_panel_view(&self) -> Option<Element<'_, Message>> {
        let shown = self.profiles.panel.as_ref()?;
        let theme = &self.theme;
        let inks = self.inks(&shown.look, Mode::Docked);
        let body = self.profile_body(
            shown,
            Mode::Docked,
            PANEL_WIDTH - 2.0 * PANEL_INSET,
            self.profiles.panel_pointed,
        );
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
        let card = self.dressed(
            shown,
            Mode::Docked,
            PANEL_WIDTH - 2.0 * PANEL_INSET,
            card.into(),
        );
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

    /// The account's own card while its popover shows: made again only
    /// when the core's profile of it changes, and let go as it closes.
    fn account_card_sync(&mut self, link: u32) {
        let state = self.client.state();
        let Some(profile) = state
            .account_profile
            .as_ref()
            .filter(|_| self.account_popover.is_some())
        else {
            self.profiles.account = None;
            return;
        };
        if let Some((kept, _)) = &self.profiles.account
            && Arc::ptr_eq(kept, profile)
        {
            return;
        }
        let view = Arc::new(ProfileView {
            profile: chat::Profile::clone(profile),
            ..Default::default()
        });
        let look = Look::new(state, &view, None, &[], link);
        self.profiles.account = Some((profile.clone(), Shown::new(look)));
    }

    /// Whether the profile panel shows beside the open DM.
    pub fn panel_shown(&self) -> bool {
        self.header.profile_available && self.profiles.panel.is_some()
    }

    /// A picture for a profile at its size, made once while it's shown.
    /// The colour behind a banner: the accent, else, with no banner and
    /// no theme, the avatar's dominant colour once it has arrived, as
    /// Discord tints it.
    pub(crate) fn banner_ground(&self, shown: &Shown) -> Option<u32> {
        let profile = shown.look.profile();
        if shown.look.accent.is_some() || profile.banner.is_some() || profile.theme.is_some() {
            return shown.look.accent;
        }
        let url = profile.avatar.as_deref()?;
        if let Some((known, colour)) = &*shown.avatar_colour.borrow()
            && known == url
        {
            return Some(*colour);
        }
        // One picked in the editor and not sent yet is the card's own, as
        // its avatar shows it.
        let picked = shown.picked.borrow().get(url).cloned();
        let picture = match &picked {
            Some(picture) => picture,
            None => self.client.state().pictures.get(url)?,
        };
        let order = crate::pictures::byte_order(picture);
        let colour = yapfast_core::dominant_colour::dominant(&picture.rgba, order)?;
        *shown.avatar_colour.borrow_mut() = Some((url.to_owned(), colour));
        Some(colour)
    }

    pub(crate) fn profile_picture(
        &self,
        shown: &Shown,
        url: &str,
        width: u32,
        height: u32,
        cover: bool,
    ) -> Option<Handle> {
        if let Some(handle) = shown
            .pictures
            .borrow()
            .get(url)
            .and_then(|sizes| sizes.get(&(width, height)))
        {
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
        shown
            .pictures
            .borrow_mut()
            .entry(url.to_owned())
            .or_default()
            .insert((width, height), handle.clone());
        Some(handle)
    }

    /// The pictures the profiles on screen show, and the moving ones of
    /// those under the pointer.
    pub fn profile_pictures(&self) -> Vec<String> {
        let state = self.client.state();
        let cosmetics = state.cosmetics;
        let mut urls = self.editor_pictures();
        if let Some((_, shown)) = &self.profiles.account {
            let profile = shown.look.profile();
            urls.extend(profile.banner.clone());
            urls.extend(
                cosmetics
                    .tag(profile.tag.as_ref())
                    .and_then(|tag| tag.badge.as_deref())
                    .map(str::to_owned),
            );
            urls.extend(profile.badges.iter().map(|badge| badge.icon.clone()));
        }
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
            urls.extend(self.effect_pictures(shown, pointed));
            urls.extend(self.frame_pictures(shown));
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
            // A levelled badge's large picture, for its tooltip.
            urls.extend(
                profile
                    .badges
                    .iter()
                    .filter_map(|badge| badge.level.as_ref())
                    .map(|level| level.picture.clone())
                    .filter(|picture| !picture.is_empty()),
            );
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
    view: Option<&Arc<ProfileView>>,
    live: Option<look::Live>,
    link: u32,
) -> Option<Shown> {
    let view = view?;
    // The core shares a view while it stays the same.
    let same = |shown: &Shown| {
        Arc::ptr_eq(&shown.look.view, view)
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
