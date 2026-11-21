//! Profiles: the card a member's line, a message's author or a friend
//! opens; the same card docked beside a one-to-one DM; and the full profile
//! over the window, the card down its left and tabs of what they do and
//! share.
//!
//! [`look`] works out what a profile shows once each time the core's view
//! of it changes; `body` draws the card in each of its three places;
//! `full` the full profile's tabs; `activity` an activity's card;
//! [`editor`] the profile editor, drawn with the card's parts, and
//! [`name_style`] its Display Name Style window.

mod activity;
pub mod body;
mod cosmetic;
pub mod editor;
mod full;
pub mod look;
pub mod name_style;

use crate::widgets::scroll_view;
use std::{cell::RefCell, collections::HashMap, sync::Arc};

use iced::{
    Element, Length, Point, Task,
    advanced::{graphics::text::Version, image::Handle},
    widget::{column, container, mouse_area, opaque, stack},
};
use yapfast_core::{
    application::{Action, ProfileTab, ProfileView},
    chat,
};

use crate::{
    app::{App, Message},
    controls::{Kind, action},
    ctx::{Cx, Focus, Request},
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
const PANEL_BUTTON: f32 = 40.0;
const PANEL_FOOT: f32 = 56.0;
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

/// What a one-to-one DM's opening does about its person, as Discord's:
/// befriend them or accept their request, ignore their request, unfriend
/// or block them (both asked first), or unblock them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Relate {
    Befriend,
    Ignore,
    Unfriend,
    Block,
    Unblock,
}

/// What's done on a profile.
#[derive(Clone, Debug)]
pub enum Event {
    /// The full profile, on a tab or the first: from the card, or from the
    /// DM's panel.
    Enlarge(Option<Tab>, bool),
    Close,
    Befriend(String),
    /// What a one-to-one DM's opening does about its person, by their id.
    Relate(String, Relate),
    Message(String),
    /// Their menu, from the More or Friends button, where it was pressed.
    More(String),
    /// Ctrl+C on the name or username selected.
    Copy(String),
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

/// An activity's more menu's width.
pub(crate) const ACTIVITY_MENU: f32 = 170.0;

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
    pub editor: crate::slot::Slot<editor::Editor>,
    /// The account's own profile as its popover shows it, while it's open.
    pub account: Option<(Arc<chat::Profile>, Shown)>,
    /// The floating card, while it shows.
    pub card: Option<Card>,
}

impl Profiles {
    /// Opens `user`'s card at `at`.
    pub fn open_card(&mut self, user: String, at: Point, cx: &mut Cx<'_>) {
        cx.dispatch(Action::ShowProfile {
            user: user.clone(),
            large: false,
        });
        self.card = Some(Card { user, at });
        self.field.clear();
        self.unpoint_card();
    }

    /// The full profile's tab, once per tab and person: Board, Activity and
    /// Wishlist ask Discord for what they show. The tab is settled as the
    /// profile opens and as another person's shows in it, so a Board that
    /// comes with Discord's answer doesn't take over, and a chosen tab
    /// that goes falls back to the first for good.
    fn tell_tab(&mut self, cx: &mut Cx<'_>) {
        let Some(shown) = self.shown.as_ref().filter(|shown| shown.look.view.large) else {
            self.told = None;
            return;
        };
        let look = &shown.look;
        let other = self
            .told
            .as_ref()
            .is_some_and(|(person, _)| *person != look.profile().id);
        if other || self.tab.is_none_or(|tab| !look.tabs.contains(&tab)) {
            self.tab = Some(look.tabs[0]);
        }
        let tab = self.tab_for(&shown.look);
        let told = (shown.look.profile().id.clone(), tab);
        if self.told.as_ref() == Some(&told) {
            return;
        }
        self.told = Some(told);
        let tab = match tab {
            Tab::Activity => ProfileTab::Activity,
            Tab::Board => ProfileTab::Board,
            Tab::Wishlist => ProfileTab::Wishlist,
            Tab::Friends | Tab::Servers => return,
        };
        cx.dispatch(Action::OpenProfileTab(tab));
    }

    /// The tab the full profile shows: the one chosen while it's there,
    /// else the first.
    pub fn tab_for(&self, look: &Look) -> Tab {
        self.tab
            .filter(|tab| look.tabs.contains(tab))
            .unwrap_or(look.tabs[0])
    }

    /// The note typed on the shown profile goes to the core, if it
    /// changed: as Enter does, before the profile closes or changes.
    fn save_note(&self, cx: &mut Cx<'_>) {
        let Some(shown) = &self.shown else {
            return;
        };
        let profile = shown.look.profile();
        if self.note != profile.note {
            let (user, note) = (profile.id.clone(), self.note.clone());
            cx.dispatch(Action::SaveNote { user, note });
        }
    }

    /// Escape: what was typed in the note goes back to what's kept.
    pub fn revert_note(&mut self) {
        if let Some(shown) = &self.shown {
            self.note = shown.look.profile().note.clone();
        }
    }

    /// What's done on a profile. Its More, an activity's menu, Manage
    /// Privacy, a server's card and the editor are the root's
    /// (`App::profile_event`).
    pub fn update(&mut self, event: Event, cx: &mut Cx<'_>) -> Task<Message> {
        match event {
            Event::Enlarge(tab, panel) => {
                self.save_note(cx);
                self.tab = tab;
                if panel {
                    let Some(room) = cx.state().selected.clone() else {
                        return Task::none();
                    };
                    cx.dispatch(Action::ShowDmProfile(room));
                } else {
                    cx.dispatch(Action::EnlargeProfile);
                }
                self.card = None;
                self.unpoint_card();
            }
            Event::Close => {
                self.save_note(cx);
                cx.dispatch(Action::CloseProfile);
                self.card = None;
                cx.request(Request::Focus(Focus::Default));
            }
            Event::Befriend(user) => cx.dispatch(Action::Befriend(user)),
            Event::Relate(user, relate) => {
                use yapfast_core::{application::PersonAsk, chat::RelationshipChange};
                let location = crate::person_menu::CONTEXT_MENU;
                let ask = |ask| Action::AskPerson {
                    user: user.clone(),
                    ask,
                    location,
                };
                cx.dispatch(match relate {
                    Relate::Befriend => Action::Befriend(user.clone()),
                    Relate::Ignore => Action::RemoveFriendRequest(user.clone()),
                    Relate::Unfriend => ask(PersonAsk::Unfriend),
                    Relate::Block => ask(PersonAsk::Block),
                    Relate::Unblock => Action::ChangeRelationship {
                        user: user.clone(),
                        change: RelationshipChange::Unblock,
                        location,
                    },
                });
            }
            Event::Message(user) => {
                self.save_note(cx);
                cx.dispatch(Action::CloseProfile);
                cx.dispatch(Action::MessagePerson(user));
            }
            Event::More(user) => cx.request(Request::PersonMenu {
                user,
                location: crate::person_menu::PROFILE_OVERFLOW,
            }),
            Event::Field(text) => {
                self.field = text;
                return Task::none();
            }
            Event::Copy(text) => return iced::clipboard::write(text),
            Event::Send => {
                let user = self
                    .card
                    .as_ref()
                    .map(|card| card.user.clone())
                    .or_else(|| {
                        self.panel
                            .as_ref()
                            .map(|shown| shown.look.profile().id.clone())
                    });
                let Some(user) = user else {
                    return Task::none();
                };
                if self.field.trim().is_empty() {
                    return Task::none();
                }
                let text = std::mem::take(&mut self.field);
                self.card = None;
                cx.dispatch(Action::MessageFromProfile { user, text });
            }
            Event::Note(text) => {
                self.note = text;
                return Task::none();
            }
            Event::SaveNote => self.save_note(cx),
            Event::Tab(tab) => {
                self.tab = Some(tab);
                self.tell_tab(cx);
            }
            Event::Friend(user) => {
                self.save_note(cx);
                let large = self
                    .shown
                    .as_ref()
                    .is_some_and(|shown| shown.look.view.large);
                self.tab = None;
                cx.dispatch(Action::ShowFriendProfile { user, large });
            }
            // The card's person's full profile, on its Activity tab.
            Event::AllActivity => {
                return self.update(Event::Enlarge(Some(Tab::Activity), false), cx);
            }
            Event::Pointed(pointed) => {
                self.pointed = pointed;
                cx.request(Request::Ask);
                return self.point_effect(false, pointed, cx.state());
            }
            Event::PanelPointed(pointed) => {
                self.panel_pointed = pointed;
                cx.request(Request::Ask);
                return self.point_effect(true, pointed, cx.state());
            }
            Event::EffectTick(generation) => return self.effect_tick(generation, cx.state()),
            Event::Bubble(pointed) => {
                self.bubble = pointed;
                return Task::none();
            }
            Event::EditProfile => {
                cx.dispatch(Action::CloseProfile);
                self.card = None;
                cx.dispatch(Action::EditProfile);
            }
            Event::TogglePanel => {
                let shown = !cx.state().profile_panel;
                cx.keep("profile_panel_hidden", !shown);
                cx.dispatch(Action::ShowProfilePanel(shown));
            }
            Event::ActivityMenu(_)
            | Event::ActivityPrivacy
            | Event::Server(_)
            | Event::Editor(_) => {}
        }
        cx.touch();
        Task::none()
    }
}

/// Takes in the core's profiles: each one's look is made again only
/// when its view changed. The full profile tells the core which tab
/// shows, once per tab and person.
pub fn profile_sync(app: &mut App) -> Task<Message> {
    let state = app.client.state();
    // The card the core closed, or one shown large, goes.
    if app.profiles.card.is_some() && state.profile.as_ref().is_none_or(|view| view.large) {
        app.profiles.card = None;
    }
    let link = crate::theme::rgb(app.theme.link);
    let (shown, panel) = (app.profiles.shown.take(), app.profiles.panel.take());
    let core = &app.client.app;
    let state = &core.state;
    let panel_view = state.panel_profile.as_ref().filter(|_| state.profile_panel);
    app.profiles.shown = make_shown(core, shown, state.profile.as_ref(), None, link);
    let live = panel_view
        .and(state.selected_room())
        .map(|room| look::Live {
            presence: room.presence,
            activity: room.activity.clone(),
            activities: room.activities.clone(),
        });
    app.profiles.panel = make_shown(core, panel, panel_view, live, link);
    {
        let mut cx = crate::ctx::cx!(app);
        editor::follow(&mut app.profiles.editor, &mut cx);
    }
    account_card_sync(app, link);
    // The note follows the profile's while it isn't being written.
    if let Some(profile) = app
        .profiles
        .shown
        .as_ref()
        .filter(|shown| shown.look.view.large)
        .map(|shown| shown.look.profile())
    {
        if app.profiles.note_of != profile.id {
            app.profiles.note_of = profile.id.clone();
            app.profiles.note = profile.note.clone();
        }
    } else {
        app.profiles.note_of.clear();
    }
    let mut cx = crate::ctx::cx!(app);
    app.profiles.tell_tab(&mut cx);
    Task::none()
}

/// The account's own card while its popover shows: made again only
/// when the core's profile of it changes, and let go as it closes.
fn account_card_sync(app: &mut App, link: u32) {
    let state = app.client.state();
    let Some(profile) = state
        .account_profile
        .as_ref()
        .filter(|_| app.status.popover.is_some())
    else {
        app.profiles.account = None;
        return;
    };
    if let Some((kept, _)) = &app.profiles.account
        && Arc::ptr_eq(kept, profile)
    {
        return;
    }
    let view = Arc::new(ProfileView {
        profile: chat::Profile::clone(profile),
        ..Default::default()
    });
    let look = Look::new(state, &view, None, &[], link);
    app.profiles.account = Some((profile.clone(), Shown::new(look)));
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

/// The tab the full profile shows.
pub fn profile_tab(ctx: crate::ctx::Ctx<'_>, look: &Look) -> Tab {
    ctx.profiles.tab_for(look)
}

/// The floating card, beside its list or message, as far up as the
/// window lets it; it scrolls once it's taller than the window.
pub fn card_view<'a>(ctx: crate::ctx::Ctx<'a>) -> Option<Element<'a, Message>> {
    let card = ctx.profiles.card.as_ref()?;
    let shown = ctx
        .profiles
        .shown
        .as_ref()
        .filter(|shown| !shown.look.view.large)?;
    let body =
        crate::profile::body::profile_body(ctx, shown, Mode::Card, WIDTH, ctx.profiles.pointed);
    let limit = ctx.look.window.height - 16.0;
    let content = container(scroll_view(body).style(scrollbar(ctx.look.theme)))
        .width(WIDTH)
        .max_height(limit);
    let content = crate::profile::cosmetic::dressed(ctx, shown, Mode::Card, WIDTH, content.into());
    let pointer = mouse_area(content)
        .on_enter(Message::Profile(Event::Pointed(true)))
        .on_exit(Message::Profile(Event::Pointed(false)));
    Some(Place::new(crate::widgets::motion::grow(opaque(pointer)), card.at, 8.0).into())
}

/// The full profile over the window, dimmed under it; a click outside
/// closes it.
pub fn full_profile_view<'a>(ctx: crate::ctx::Ctx<'a>) -> Option<Element<'a, Message>> {
    let shown = ctx
        .profiles
        .shown
        .as_ref()
        .filter(|shown| shown.look.view.large)?;
    let (window_width, window_height) = (ctx.look.window.width, ctx.look.window.height);
    let width = (window_width * 0.7).max(720.0).min(window_width - 32.0);
    let height = (window_height * 0.75).max(520.0).min(window_height - 32.0);
    let modal = crate::profile::full::full_view(ctx, shown, width, height);
    // Over the window dimmed as under a settings card.
    Some(opaque(stack![
        crate::controls::dimmed(Message::Profile(Event::Close), crate::sheet::DIM),
        container(crate::widgets::motion::grow(opaque(modal))).center(Length::Fill),
    ]))
}

/// The profile docked beside a one-to-one DM, filling its height, with
/// View Full Profile at its foot.
pub fn profile_panel_view<'a>(ctx: crate::ctx::Ctx<'a>) -> Option<Element<'a, Message>> {
    let shown = ctx.profiles.panel.as_ref()?;
    let theme = ctx.look.theme;
    let inks = inks(ctx, &shown.look, Mode::Docked);
    let body = crate::profile::body::profile_body(
        ctx,
        shown,
        Mode::Docked,
        PANEL_WIDTH - 2.0 * PANEL_INSET,
        ctx.profiles.panel_pointed,
    );
    let scroll = scroll_view(body)
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
    let card = crate::profile::cosmetic::dressed(
        ctx,
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

/// The colour behind a banner: the accent, else, with no banner and
/// no theme, the avatar's dominant colour once it has arrived, as
/// Discord tints it.
pub(crate) fn banner_ground(ctx: crate::ctx::Ctx<'_>, shown: &Shown) -> Option<u32> {
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
        None => ctx.state.pictures.get(url)?,
    };
    let order = crate::pictures::byte_order(picture);
    let colour = yapfast_core::dominant_colour::dominant(&picture.rgba, order)?;
    *shown.avatar_colour.borrow_mut() = Some((url.to_owned(), colour));
    Some(colour)
}

/// A picture for a profile at its size, made once while it's shown.
pub(crate) fn profile_picture(
    ctx: crate::ctx::Ctx<'_>,
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
        None => ctx.state.pictures.get(url)?,
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
pub fn profile_pictures(ctx: crate::ctx::Ctx<'_>) -> Vec<String> {
    let state = ctx.state;
    let cosmetics = state.cosmetics;
    let mut urls = crate::profile::editor::editor_pictures(ctx);
    if let Some((_, shown)) = &ctx.profiles.account {
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
        (&ctx.profiles.shown, ctx.profiles.pointed),
        (&ctx.profiles.panel, ctx.profiles.panel_pointed),
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
        urls.extend(cosmetic::effect_pictures(state, shown, pointed));
        urls.extend(cosmetic::frame_pictures(state, shown));
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
        // The first mutual servers' icons, by the count of them.
        let servers = profile.servers.iter().take(3);
        urls.extend(servers.filter_map(|server| server.icon.clone()));
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
        match profile_tab(ctx, &shown.look) {
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
pub fn inks(ctx: crate::ctx::Ctx<'_>, look: &Look, mode: Mode) -> look::Inks {
    let plain = if mode == Mode::Docked {
        ctx.look.theme.panel
    } else {
        ctx.look.theme.sidebar
    };
    look::Inks::new(ctx.look.theme, look.profile().theme, plain)
}

/// Whether the profile panel shows beside the open DM.
pub fn panel_shown(app: &App) -> bool {
    app.header.profile_available && app.profiles.panel.is_some()
}
