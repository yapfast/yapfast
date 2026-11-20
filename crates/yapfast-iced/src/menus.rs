//! Context menus (a message's, the composer's, a place's, someone's). While
//! one is open it has the keys: the window's focus goes to [`NOBODY`]. A
//! row may open a submenu beside it (mute times, notification levels,
//! roles, servers to invite to, apps, channels to move to), under the
//! pointer, on a click, or with Right; Left or Escape closes it. A row
//! whose submenu is a choice of how (a mute's times) does its own thing
//! when clicked itself.

use crate::widgets::scroll_view;
use iced::{
    Alignment, Border, Element, Length, Padding, Point, Task,
    keyboard::key,
    widget::{Space, button, column, container, mouse_area, opaque, operation, row, text},
};
use yapfast_core::application::{Action, ThreadFrom};

use crate::{
    app::{App, Message, NOBODY},
    controls::{Item, Light, menu_panel},
    ctx::Ctx,
    field::MenuEdit,
    picker::{Tab, Target},
    pictures::Icon,
    style::blend,
    widgets::{Anchor, Place},
};
use yapfast_core::i18n::tr;

/// How far a menu stays in from the window's edges.
const MENU_MARGIN: f32 = 8.0;

/// A menu's width; the composer's and a message's are wider.
pub const WIDTH: f32 = 224.0;
pub const FIELD_WIDTH: f32 = 255.0;
/// A message's menu as Discord's: 280 px, its rows 36 px with a 20 px
/// icon and a 16 px chevron, its quick reactions 44 px tiles with a 20 px
/// emoji.
pub const MESSAGE_WIDTH: f32 = 280.0;
const MESSAGE_ROW: f32 = 36.0;
const MESSAGE_ICON: u32 = 20;
const MESSAGE_CHEVRON: u32 = 16;
const QUICK_TILE: f32 = 44.0;
const QUICK_EMOJI: u32 = 20;
/// How many of the reactions this device uses most a message's menu
/// offers over its rows (the core fills them with Discord's until it has
/// reacted); the first three are on its hover bar.
pub const QUICK: usize = 4;

/// An open context menu.
pub struct Menu {
    pub at: Point,
    pub rows: Vec<MenuRow>,
    /// The row lit by the pointer or the arrow keys.
    pub lit: Option<usize>,
    /// The message's item, for a message's menu.
    pub item: Option<String>,
    /// A message's menu in the timeline, opened before its room's apps'
    /// commands came: its Apps row goes in once they have.
    pub apps_due: bool,
    /// The quick reactions above the rows, the most used first, taken as
    /// it opens; none for a menu without them.
    pub reactions: Vec<String>,
    /// The place whose menu it is (`place_menus.rs`), and whether it's a
    /// server's header menu.
    pub place: Option<String>,
    pub header: bool,
    /// Where Discord's client says someone's menu was opened from.
    pub location: &'static str,
    /// How its point is read, and its width.
    pub anchor: Anchor,
    pub width: f32,
    /// Someone in a call, by its room and their id: their voice menu, with
    /// their volumes over its rows while they're someone else.
    pub voice: Option<(String, String)>,
    pub volumes: Option<crate::call::Volumes>,
    /// The row whose submenu shows, and the submenu's row the keys light.
    pub sub: Option<usize>,
    pub sub_lit: Option<usize>,
    /// How far down its rows are scrolled, when it's taller than the
    /// window: a submenu opens beside its row where it shows.
    pub scrolled: f32,
    /// How wide its rows need it, and each row's submenu, measured once
    /// as it first shows: the words don't change while it's open.
    pub fits: std::cell::OnceCell<Vec<f32>>,
}

impl Menu {
    /// A menu of `rows` opening at the pointer at `at`.
    pub fn new(at: Point, rows: Vec<MenuRow>) -> Self {
        Self {
            at,
            rows,
            lit: None,
            item: None,
            apps_due: false,
            reactions: Vec::new(),
            place: None,
            header: false,
            location: crate::person_menu::CONTEXT_MENU,
            anchor: Anchor::Pointer,
            width: WIDTH,
            voice: None,
            volumes: None,
            sub: None,
            sub_lit: None,
            scrolled: 0.0,
            fits: Default::default(),
        }
    }

    /// Whether its rows leave room for an icon or tick: a message's always,
    /// others when a row has one.
    fn slot(&self) -> bool {
        self.place.is_none() || ticks(&self.rows) || self.rows.iter().any(|row| row.icon.is_some())
    }

    /// How wide the menu (`None`) or row `sub`'s submenu shows.
    fn fit(&self, sub: Option<usize>) -> f32 {
        let message = self.item.is_some();
        let fits = self.fits.get_or_init(|| {
            std::iter::once(fit(&self.rows, self.slot(), message))
                .chain(self.rows.iter().map(|row| {
                    if row.submenu.is_empty() {
                        0.0
                    } else {
                        fit(&row.submenu, ticks(&row.submenu), message)
                    }
                }))
                .collect()
        });
        match sub {
            None => fits[0].max(self.width),
            Some(sub) => fits.get(sub + 1).copied().unwrap_or(0.0).max(SUB_WIDTH),
        }
    }

    pub fn anchor(mut self, anchor: Anchor) -> Self {
        self.anchor = anchor;
        self
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    /// What a chosen row acts in: the menu without its rows.
    fn context(&self) -> Context {
        Context {
            at: self.at,
            item: self.item.clone(),
            place: self.place.clone(),
            location: self.location,
            voice: self.voice.clone(),
        }
    }
}

/// What a menu's chosen row acts in.
pub struct Context {
    pub at: Point,
    pub item: Option<String>,
    pub place: Option<String>,
    pub location: &'static str,
    pub voice: Option<(String, String)>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MenuAct {
    AddReaction,
    ViewReactions,
    Edit,
    Reply,
    Forward,
    CopyText,
    CopySelection,
    EndPoll,
    /// Pin, or unpin.
    Pin,
    MarkUnread,
    CopyLink,
    CopyId,
    Delete,
    /// Delete without asking, as Shift and the hover bar's Delete do.
    DeleteNow,
    Field(crate::composer::Writing, MenuEdit),
    /// The composer's + menu.
    Upload,
    /// Create Thread, from the composer's + or a message; View Thread.
    NewThread,
    ViewThread,
    NewPoll,
    Place(yapfast_core::application::MenuChoice),
    /// A row of someone's menu (`person_menu.rs`).
    Person(yapfast_core::application::MenuChoice),
    /// A change to the place's notifications: a mute for a time, a level,
    /// a server's switch.
    Notify(yapfast_core::chat::NotificationChange),
    /// A DM muted for a time, its own or the person's.
    MuteDm(yapfast_core::chat::Muting),
    /// Every channel of a community server shown, or only those followed.
    ShowAll,
    /// The rows that carry what they're about in their `key`: a role
    /// given or taken, a server to invite to, an app's command, a voice
    /// channel to move to.
    Role,
    InviteTo,
    App,
    MoveTo,
    /// One of a server's scheduled events, which only shows.
    Event,
    /// A message's picture or video, by its place in the row's `key`:
    /// copied, saved, its address copied or opened.
    CopyImage,
    SaveMedia,
    CopyMediaLink,
    OpenMediaLink,
    /// A link in the message's text, its address in the row's `key`:
    /// copied or opened.
    CopyTextLink,
    OpenTextLink,
    /// An activity's more menu on a profile's card: the full profile's
    /// Activity tab, and one's own game's privacy.
    AllActivity,
    ActivityPrivacy,
    /// An order of the search's Sort menu, by its row's `key`.
    SearchSort,
}

#[derive(Clone)]
pub struct MenuRow {
    pub act: MenuAct,
    pub label: std::borrow::Cow<'static, str>,
    /// Its icon; a row without one keeps its room where another row of
    /// the menu has one.
    pub icon: Option<Icon>,
    pub danger: bool,
    /// A line above it.
    pub separated: bool,
    /// Shown greyed, not to be chosen.
    pub disabled: bool,
    /// A second line under it, and its tick or checkbox.
    pub subtitle: Option<std::borrow::Cow<'static, str>>,
    pub check: crate::controls::Check,
    /// What the row is about, for the acts that need it (a role's or a
    /// server's id, a command, a channel).
    pub key: String,
    /// The submenu it opens beside it; empty for none.
    pub submenu: Vec<MenuRow>,
    /// What a click on the row itself does instead of opening its submenu,
    /// as a mute row mutes until it's turned back on.
    pub click: Option<MenuAct>,
}

impl MenuRow {
    /// A row with its icon.
    pub fn new(act: MenuAct, label: &'static str, icon: Icon) -> Self {
        Self {
            icon: Some(icon),
            ..Self::plain(act, label)
        }
    }

    /// A row without an icon.
    pub fn plain(act: MenuAct, label: impl Into<std::borrow::Cow<'static, str>>) -> Self {
        Self {
            act,
            label: label.into(),
            icon: None,
            danger: false,
            separated: false,
            disabled: false,
            subtitle: None,
            check: crate::controls::Check::None,
            key: String::new(),
            submenu: Vec::new(),
            click: None,
        }
    }

    /// The row with `key`.
    pub fn key(mut self, key: impl Into<String>) -> Self {
        self.key = key.into();
        self
    }
}

/// A submenu's width, the composer's menu's.
pub const SUB_WIDTH: f32 = FIELD_WIDTH;
/// Between a menu and its submenu.
const SUB_GAP: f32 = 4.0;
/// A menu's padding, which lifts a submenu so its first row is level with
/// the row that opened it.
const PAD: f32 = 9.0;

/// What's done in the open menu, its submenu and a message's hover bar.
#[derive(Clone, Debug)]
pub enum Event {
    /// The menu's row under the pointer or the keys, by its place.
    Light(usize),
    Choose(usize),
    /// A menu taller than the window was scrolled this far down.
    Scrolled(f32),
    /// The open submenu's row under the pointer, or chosen.
    SubLight(usize),
    SubChoose(usize),
    Close,
    /// A button of a message's hover bar: what it does, to which message,
    /// and where what it opens goes.
    Act(MenuAct, String, Point),
    /// One of the menu's quick reactions.
    React(String, String),
}

/// What each of the menu's events does.
pub fn update(app: &mut App, event: Event) -> Task<Message> {
    match event {
        Event::Light(index) => menu_light(app, index),
        Event::SubLight(index) => {
            if let Some(menu) = &mut app.win.menu {
                menu.sub_lit = Some(index);
            }
            Task::none()
        }
        Event::SubChoose(index) => choose_sub(app, index),
        Event::Choose(index) => {
            // Shift and a click on Delete message deletes without asking,
            // as the hover bar's Delete under Shift does.
            if app.modifiers.shift()
                && let Some(menu) = &mut app.win.menu
                && let Some(row) = menu.rows.get_mut(index)
                && matches!(row.act, MenuAct::Delete)
            {
                row.act = MenuAct::DeleteNow;
            }
            choose(app, index)
        }
        Event::Scrolled(down) => {
            if let Some(menu) = &mut app.win.menu {
                menu.scrolled = down;
            }
            Task::none()
        }
        Event::Act(act, item, at) => self::act(app, act, item, at, None),
        Event::Close => {
            app.win.menu = None;
            crate::composer::refocus(app)
        }
        Event::React(item, key) => {
            app.win.menu = None;
            app.client.dispatch(Action::React { item, key });
            Task::batch([app.sync(), crate::composer::refocus(app)])
        }
    }
}

/// Whether this account may add a reaction nobody added yet in row
/// `index`'s timeline, which Add reaction and the quick reactions need.
pub fn adds_reactions(app: &App, index: usize) -> bool {
    crate::message::reacts(app.ctx(), index) == yapfast_core::chat::Reacts::Any
}

/// A right click on a message: what one may do with it.
pub fn open_menu(app: &mut App, index: usize, at: Point) -> Task<Message> {
    let Some(message) = crate::timeline::row_message(app, index).cloned() else {
        return Task::none();
    };
    if message.deleted {
        return Task::none();
    }
    let row = MenuRow::new;
    let mut rows = Vec::new();
    let mut group: Option<u8> = None;
    let mut push = |section: u8, mut entry: MenuRow| {
        entry.separated = group.replace(section).is_some_and(|last| last != section);
        rows.push(entry);
    };
    let state = app.client.state();
    let writes = crate::composer::composer_look(app.ctx()).enabled;
    let sent = message.delivery == yapfast_core::chat::Delivery::default();
    let reacts = message.can_react && adds_reactions(app, index);
    // Its chevron as Discord's; a click opens the picker.
    if reacts {
        push(
            1,
            MenuRow {
                check: crate::controls::Check::Submenu,
                ..row(MenuAct::AddReaction, tr("Add reaction"), Icon::SmilePlus)
            },
        );
    }
    if !message.reactions.is_empty() {
        push(
            1,
            row(MenuAct::ViewReactions, tr("View Reactions"), Icon::Smile),
        );
    }
    if message.can_edit && writes {
        push(2, row(MenuAct::Edit, tr("Edit message"), Icon::Pencil));
    }
    if message.can_reply && writes {
        push(2, row(MenuAct::Reply, tr("Reply"), Icon::Reply));
    }
    if message.can_forward && crate::forward::allowed(app.client.state()) {
        push(2, row(MenuAct::Forward, tr("Forward"), Icon::Forward));
    }
    // A message someone wrote starts a thread, or leads to its own.
    if index < crate::timeline::ASIDE
        && app.client.app.can_start_threads()
        && message.can_reply
        && sent
        && message.thread.is_none()
    {
        push(
            2,
            row(MenuAct::NewThread, tr("Create Thread"), Icon::Thread),
        );
    }
    if message.can_reply && message.thread.is_some() {
        push(2, row(MenuAct::ViewThread, tr("View Thread"), Icon::Thread));
    }
    if !message.body.is_empty() {
        push(3, row(MenuAct::CopyText, tr("Copy text"), Icon::Copy));
    }
    if crate::timeline::selection::copied_selection(app).is_some() {
        push(
            3,
            row(
                MenuAct::CopySelection,
                tr("Copy selection"),
                Icon::TextSelect,
            ),
        );
    }
    if message.poll.as_ref().is_some_and(|poll| poll.can_end) {
        push(
            3,
            row(MenuAct::EndPoll, tr("End Poll Now"), Icon::CircleStop),
        );
    }
    if message.can_pin && sent {
        push(
            3,
            if message.pinned {
                row(MenuAct::Pin, tr("Unpin message"), Icon::PinOff)
            } else {
                row(MenuAct::Pin, tr("Pin message"), Icon::Pin)
            },
        );
    }
    // The apps' message commands, beside it as Discord's are; not in
    // the thread beside, whose room the index isn't for, nor on an answer
    // only this account sees or one still to come.
    let takes_apps = index < crate::timeline::ASIDE
        && sent
        && message.flags & (crate::message::EPHEMERAL | crate::message::LOADING) == 0;
    let apps = takes_apps.then(|| apps_row(app)).flatten();
    let apps_due = takes_apps && apps.is_none();
    if let Some(apps) = apps {
        push(3, apps);
    }
    // After Apps, as in Discord's menu.
    if app.client.app.marks_unread(&message.item) {
        push(
            3,
            row(MenuAct::MarkUnread, tr("Mark Unread"), Icon::MessageCircle),
        );
    }
    let link = state.message_link(&message.item);
    if link.is_some() {
        push(
            3,
            row(MenuAct::CopyLink, tr("Copy Message Link"), Icon::Chain),
        );
    }
    if message.can_delete && writes {
        push(
            4,
            MenuRow {
                danger: true,
                ..row(MenuAct::Delete, tr("Delete message"), Icon::Trash)
            },
        );
    }
    // Last, as Discord's client puts it.
    if link.is_some() {
        push(5, row(MenuAct::CopyId, tr("Copy Message ID"), Icon::Id));
    }
    if rows.is_empty() {
        return Task::none();
    }
    app.win.menu = Some(Menu {
        item: Some(message.item.clone()),
        apps_due,
        reactions: if reacts {
            app.client.app.frequent_reactions(QUICK)
        } else {
            Vec::new()
        },
        width: MESSAGE_WIDTH,
        ..Menu::new(at, rows)
    });
    // Its Apps follow the permissions as they are now.
    app.client.dispatch(Action::AskAppCommands);
    operation::focus(NOBODY)
}

/// The open room's apps' message commands as a menu's Apps row; none
/// while there are none, or its index hasn't come.
fn apps_row(app: &App) -> Option<MenuRow> {
    let room = app.client.state().selected.as_deref()?;
    let apps: Vec<MenuRow> = app
        .client
        .app
        .commands_for_target(room, yapfast_core::application::CommandTarget::Message)
        .into_iter()
        .map(|command| MenuRow {
            subtitle: Some(command.app_name.into()),
            ..MenuRow::plain(MenuAct::App, command.name).key(command.command)
        })
        .collect();
    (!apps.is_empty()).then(|| MenuRow {
        submenu: apps,
        ..MenuRow::new(MenuAct::App, tr("Apps"), Icon::Slash)
    })
}

/// A message's menu opened before its room's apps' commands came: its
/// Apps row goes in where it would have been, once they have, and the
/// rest of the menu (a picture's rows, the light, an open submenu)
/// stays as it is.
pub fn menu_apps_sync(app: &mut App) -> Task<Message> {
    if !app.win.menu.as_ref().is_some_and(|menu| menu.apps_due) {
        return Task::none();
    }
    let Some(mut apps) = apps_row(app) else {
        return Task::none();
    };
    let Some(menu) = &mut app.win.menu else {
        return Task::none();
    };
    // Before Mark Unread, Copy Message Link, link/media rows, Delete and Copy Message ID.
    let at = menu
        .rows
        .iter()
        .position(|row| {
            matches!(
                row.act,
                MenuAct::MarkUnread
                    | MenuAct::CopyLink
                    | MenuAct::CopyTextLink
                    | MenuAct::OpenTextLink
                    | MenuAct::CopyImage
                    | MenuAct::SaveMedia
                    | MenuAct::CopyMediaLink
                    | MenuAct::OpenMediaLink
                    | MenuAct::Delete
                    | MenuAct::DeleteNow
                    | MenuAct::CopyId
            )
        })
        .unwrap_or(menu.rows.len());
    // Apps belongs to Group 3: separate it if it becomes the first row of Group 3.
    let starts_group = at > 0
        && !matches!(
            menu.rows[at - 1].act,
            MenuAct::CopyText | MenuAct::CopySelection | MenuAct::EndPoll | MenuAct::Pin
        );
    apps.separated = starts_group;
    if let Some(next) = menu.rows.get_mut(at)
        && matches!(next.act, MenuAct::MarkUnread | MenuAct::CopyLink)
    {
        next.separated = false;
    }
    menu.rows.insert(at, apps);
    menu.apps_due = false;
    // Rows after it moved down one.
    for index in [&mut menu.lit, &mut menu.sub].into_iter().flatten() {
        if *index >= at {
            *index += 1;
        }
    }
    menu.fits = Default::default();
    Task::none()
}

/// The pointer came onto a row: it's lit, and its submenu opens, or
/// another's closes.
pub fn menu_light(app: &mut App, index: usize) -> Task<Message> {
    if let Some(menu) = &mut app.win.menu {
        menu.lit = Some(index);
        let opens = menu
            .rows
            .get(index)
            .is_some_and(|row| !row.submenu.is_empty() && !row.disabled);
        if menu.sub != Some(index) {
            menu.sub = opens.then_some(index);
            menu.sub_lit = None;
        }
    }
    Task::none()
}

/// A row of the open menu was chosen, by click or key.
pub fn choose(app: &mut App, index: usize) -> Task<Message> {
    // A greyed row does nothing, and the menu stays.
    let Some(row) = app
        .win
        .menu
        .as_ref()
        .and_then(|menu| menu.rows.get(index))
        .filter(|row| !row.disabled)
        .cloned()
    else {
        return Task::none();
    };
    // One with a submenu opens it, unless clicking it does its own thing.
    if !row.submenu.is_empty() {
        match row.click {
            Some(click) => {
                let row = MenuRow {
                    act: click,
                    check: crate::controls::Check::None,
                    ..row
                };
                return pick(app, row, None);
            }
            None => {
                if let Some(menu) = &mut app.win.menu {
                    menu.sub = Some(index);
                    menu.lit = Some(index);
                }
                return Task::none();
            }
        }
    }
    pick(app, row, Some(index))
}

/// A row of the open submenu was chosen.
pub fn choose_sub(app: &mut App, index: usize) -> Task<Message> {
    let Some(row) = app
        .win
        .menu
        .as_ref()
        .and_then(|menu| menu.rows.get(menu.sub?)?.submenu.get(index))
        .filter(|row| !row.disabled)
        .cloned()
    else {
        return Task::none();
    };
    if let crate::controls::Check::Box(ticked) = row.check {
        if let Some(menu) = &mut app.win.menu
            && let Some(sub) = menu.sub
            && let Some(row) = menu.rows[sub].submenu.get_mut(index)
        {
            row.check = crate::controls::Check::Box(!ticked);
        }
        return carry_out(app, &row, Some(!ticked), true);
    }
    carry_out(app, &row, None, false)
}

/// Carries out a chosen row of the menu's own: a checkbox's flips and
/// the menu stays; anything else closes it.
fn pick(app: &mut App, row: MenuRow, index: Option<usize>) -> Task<Message> {
    if let (crate::controls::Check::Box(ticked), Some(index)) = (row.check, index) {
        if let Some(shown) = app
            .win
            .menu
            .as_mut()
            .and_then(|menu| menu.rows.get_mut(index))
        {
            shown.check = crate::controls::Check::Box(!ticked);
        }
        return carry_out(app, &row, Some(!ticked), true);
    }
    carry_out(app, &row, None, false)
}

/// What a chosen row does, in the open menu's place: `ticked` is a
/// checkbox's new state, and `stays` keeps the menu open.
fn carry_out(app: &mut App, row: &MenuRow, ticked: Option<bool>, stays: bool) -> Task<Message> {
    let Some(menu) = (if stays {
        app.win.menu.as_ref().map(Menu::context)
    } else {
        app.win.menu.take().map(|menu| menu.context())
    }) else {
        return Task::none();
    };
    let focus = if stays {
        Task::none()
    } else {
        crate::composer::refocus(app)
    };
    match row.act {
        MenuAct::Person(choice) => match &menu.voice {
            Some((room, user)) => {
                let task = crate::call::popups::person::choose_voice(
                    app,
                    room.clone(),
                    user.clone(),
                    choice,
                    ticked,
                );
                Task::batch([focus, task])
            }
            None => {
                let user = menu.place.clone().unwrap_or_default();
                Task::batch([
                    focus,
                    crate::person_menu::choose_person(app, user, menu.location, choice, ticked),
                ])
            }
        },
        MenuAct::Notify(_)
        | MenuAct::MuteDm(_)
        | MenuAct::ShowAll
        | MenuAct::Role
        | MenuAct::InviteTo
        | MenuAct::App
        | MenuAct::MoveTo => {
            Task::batch([focus, crate::place_menus::sub_act(app, &menu, row, ticked)])
        }
        MenuAct::CopyImage
        | MenuAct::SaveMedia
        | MenuAct::CopyMediaLink
        | MenuAct::OpenMediaLink => {
            let item = menu.item.clone().unwrap_or_default();
            let index = row.key.parse().unwrap_or_default();
            Task::batch([
                focus,
                crate::image_viewer::media_act(app, row.act, item, index),
            ])
        }
        MenuAct::CopyTextLink => Task::batch([focus, iced::clipboard::write(row.key.clone())]),
        MenuAct::SearchSort => {
            let sort = crate::search::sort_of(&row.key);
            app.client
                .dispatch(yapfast_core::application::Action::SearchSort(sort));
            Task::batch([focus, app.sync(), crate::search::to_top()])
        }
        MenuAct::OpenTextLink => {
            crate::browser::open(&row.key);
            focus
        }
        act => {
            let item = menu.item.clone().unwrap_or_default();
            self::act(app, act, item, menu.at, menu.place)
        }
    }
}

/// The web address a link in a message's text leads to: a masked link's,
/// behind the question asked before leaving. A mention or one of Yapfast's
/// own links has none.
pub fn web_address(link: &str) -> Option<&str> {
    let link = yapfast_core::links::masked(link).unwrap_or(link);
    (yapfast_core::links::openable(link) && yapfast_core::mention::target(link).is_none())
        .then_some(link)
}

/// Where link or media rows go in a message's menu: before Delete,
/// Copy Message ID, or any link/media rows already spliced in.
pub(crate) fn message_tail(rows: &[MenuRow]) -> usize {
    rows.iter()
        .position(|row| {
            matches!(
                row.act,
                MenuAct::Delete
                    | MenuAct::DeleteNow
                    | MenuAct::CopyId
                    | MenuAct::CopyTextLink
                    | MenuAct::OpenTextLink
                    | MenuAct::CopyImage
                    | MenuAct::SaveMedia
                    | MenuAct::CopyMediaLink
                    | MenuAct::OpenMediaLink
            )
        })
        .unwrap_or(rows.len())
}

/// Adds Copy Link and Open Link to the message's menu just opened, for the
/// link in its text the pointer was on, before Delete and Copy Message ID.
pub fn add_link_rows(app: &mut App, link: &str) {
    let (Some(address), Some(menu)) = (web_address(link), &mut app.win.menu) else {
        return;
    };
    let row = |act, label, icon| MenuRow::new(act, label, icon).key(address.to_owned());
    let at = message_tail(&menu.rows);
    if let Some(after) = menu.rows.get_mut(at) {
        after.separated = true;
    }
    let rows = [
        MenuRow {
            separated: at > 0,
            ..row(MenuAct::CopyTextLink, tr("Copy Link"), Icon::Chain)
        },
        row(MenuAct::OpenTextLink, tr("Open Link"), Icon::Link),
    ];
    menu.rows.splice(at..at, rows);
}

/// What a menu's row, or the button of a message's hover bar, does:
/// to the message `item`, or the place `place`, opened at `at`.
pub fn act(
    app: &mut App,
    act: MenuAct,
    item: String,
    at: Point,
    place: Option<String>,
) -> Task<Message> {
    let focus = crate::composer::refocus(app);
    match act {
        MenuAct::AddReaction => {
            crate::picker::open_picker(app, Target::React(item, at), Tab::Emoji)
        }
        MenuAct::ViewReactions => {
            app.client
                .dispatch(Action::ShowReactions { item, key: None });
            Task::batch([app.sync(), operation::focus(crate::app::NOBODY)])
        }
        MenuAct::Edit => crate::edit::begin_edit(app, item),
        MenuAct::Reply => {
            app.client.dispatch(Action::Reply(item));
            Task::batch([app.sync(), focus])
        }
        MenuAct::CopyText => {
            let body = app
                .client
                .state()
                .message(&item)
                .map(|message| message.body.clone())
                .unwrap_or_default();
            Task::batch([iced::clipboard::write(body), focus])
        }
        MenuAct::CopySelection => match crate::timeline::selection::copied_selection(app) {
            Some(copied) => Task::batch([iced::clipboard::write(copied), focus]),
            None => focus,
        },
        MenuAct::Delete => {
            app.client.dispatch(Action::RequestDelete(item));
            app.confirm_focus = true;
            Task::batch([app.sync(), operation::focus(NOBODY)])
        }
        MenuAct::DeleteNow => {
            app.client.dispatch(Action::DeleteNow(item));
            Task::batch([app.sync(), focus])
        }
        MenuAct::Forward => {
            app.client.dispatch(Action::StartForward(item));
            Task::batch([app.sync(), app.forward.open(app.client.state())])
        }
        MenuAct::EndPoll => {
            app.client.dispatch(Action::AskEndPoll(item));
            Task::batch([app.sync(), operation::focus(NOBODY)])
        }
        MenuAct::Pin => {
            app.client.dispatch(Action::PinMessage(item));
            Task::batch([app.sync(), focus])
        }
        MenuAct::MarkUnread => {
            app.client.dispatch(Action::MarkUnread(item));
            Task::batch([app.sync(), focus])
        }
        MenuAct::CopyLink | MenuAct::CopyId => {
            let (link, id) = app.client.state().message_link(&item).unwrap_or_default();
            let copied = if act == MenuAct::CopyLink { link } else { id };
            Task::batch([iced::clipboard::write(copied), focus])
        }
        MenuAct::Field(which, edit) => crate::composer::field_menu(app, which, edit),
        MenuAct::Upload => Task::batch([focus, crate::uploads::attach(app)]),
        MenuAct::NewThread if item.is_empty() => {
            app.client
                .dispatch(crate::thread::start_from(ThreadFrom::PlusButton));
            Task::batch([app.sync(), operation::focus(crate::thread::FIRST)])
        }
        MenuAct::NewThread => crate::thread::update(app, crate::thread::Event::Start(item)),
        MenuAct::ViewThread => {
            let room = app
                .client
                .state()
                .message(&item)
                .and_then(|message| message.thread.as_ref())
                .map(|thread| thread.room.clone());
            match room {
                Some(room) => crate::thread::update(app, crate::thread::Event::Open(room)),
                None => focus,
            }
        }
        MenuAct::Event => focus,
        MenuAct::AllActivity => {
            Task::batch([focus, app.profile_event(crate::profile::Event::AllActivity)])
        }
        MenuAct::ActivityPrivacy => Task::batch([
            focus,
            app.profile_event(crate::profile::Event::ActivityPrivacy),
        ]),
        MenuAct::NewPoll => app.switcher_event(crate::switcher::Event::OpenPoll),
        MenuAct::Place(choice) => {
            let place = place.unwrap_or_default();
            let chose = crate::place_menus::choose_place(app, place, choice);
            // Asked after the choice, which may have opened settings
            // that keep the keys from the composer.
            Task::batch([chose, crate::composer::refocus(app)])
        }
        MenuAct::Person(choice) => {
            let user = place.unwrap_or_default();
            Task::batch([
                focus,
                crate::person_menu::choose_person(
                    app,
                    user,
                    crate::person_menu::CONTEXT_MENU,
                    choice,
                    None,
                ),
            ])
        }
        // A menu's own, which `carry_out` hands to `sub_act`.
        MenuAct::SearchSort
        | MenuAct::Notify(_)
        | MenuAct::MuteDm(_)
        | MenuAct::ShowAll
        | MenuAct::Role
        | MenuAct::InviteTo
        | MenuAct::App
        | MenuAct::MoveTo
        | MenuAct::CopyImage
        | MenuAct::SaveMedia
        | MenuAct::CopyMediaLink
        | MenuAct::OpenMediaLink
        | MenuAct::CopyTextLink
        | MenuAct::OpenTextLink => focus,
    }
}

/// The open menu's keys: the arrows, Tab, Home and End light a row
/// (Shift+Tab the one before, as Up), Enter or Space chooses it, Right
/// opens its submenu and Left leaves it, Escape closes the submenu, then
/// the menu.
pub fn menu_key(app: &mut App, named: Option<key::Named>) -> Task<Message> {
    let named = match named {
        Some(key::Named::Tab) if app.modifiers.shift() => Some(key::Named::ArrowUp),
        named => named,
    };
    let Some(menu) = &mut app.win.menu else {
        return Task::none();
    };
    // In a submenu the keys move through its rows.
    if let Some(sub) = menu.sub {
        let rows = &menu.rows[sub].submenu;
        if let Some(lit) = menu.sub_lit {
            match named {
                Some(key::Named::ArrowDown | key::Named::Tab) => {
                    menu.sub_lit = step(rows, lit + 1, 1).or(Some(lit));
                }
                Some(key::Named::ArrowUp) => {
                    let count = rows.len();
                    menu.sub_lit = step(rows, lit + count - 1, count - 1).or(Some(lit));
                }
                Some(key::Named::Home) => menu.sub_lit = step(rows, 0, 1),
                Some(key::Named::End) => {
                    let count = rows.len();
                    menu.sub_lit = step(rows, count - 1, count - 1);
                }
                Some(key::Named::Enter | key::Named::Space) => return choose_sub(app, lit),
                Some(key::Named::ArrowLeft | key::Named::Escape) => {
                    menu.sub = None;
                    menu.sub_lit = None;
                }
                _ => {}
            }
            return Task::none();
        }
        if named == Some(key::Named::Escape) || named == Some(key::Named::ArrowLeft) {
            menu.sub = None;
            return Task::none();
        }
    }
    let count = menu.rows.len();
    if count == 0 {
        return Task::none();
    }
    let lit = menu.lit;
    let rows = &menu.rows;
    match named {
        Some(key::Named::ArrowDown | key::Named::Tab) => {
            menu.lit = step(rows, lit.map_or(0, |lit| lit + 1), 1).or(lit);
            menu.sub = None;
        }
        Some(key::Named::ArrowUp) => {
            let from = lit.map_or(count - 1, |lit| lit + count - 1);
            menu.lit = step(rows, from, count - 1).or(lit);
            menu.sub = None;
        }
        Some(key::Named::Home) => menu.lit = step(rows, 0, 1),
        Some(key::Named::End) => menu.lit = step(rows, count - 1, count - 1),
        Some(key::Named::ArrowRight | key::Named::Enter | key::Named::Space) => {
            let Some(lit) = lit else {
                return Task::none();
            };
            if !rows[lit].submenu.is_empty() && !rows[lit].disabled {
                menu.sub = Some(lit);
                menu.sub_lit = step(&rows[lit].submenu, 0, 1);
            } else if named != Some(key::Named::ArrowRight) {
                return choose(app, lit);
            }
        }
        Some(key::Named::Escape) => return app.escape(),
        _ => {}
    }
    Task::none()
}

/// The menu open, at its place, with its submenu beside its row.
pub fn view<'a>(menu: &'a Menu, cx: Ctx<'a>) -> Option<Element<'a, Message>> {
    View { s: menu, cx }.view()
}

/// The menu and what it reads, as its parts draw them.
#[derive(Clone, Copy)]
struct View<'a> {
    s: &'a Menu,
    cx: Ctx<'a>,
}

impl<'a> View<'a> {
    fn view(self) -> Option<Element<'a, Message>> {
        let menu = self.s;
        let theme = self.cx.look.theme;
        let mut rows = column![].padding(PAD);
        let volumes = menu
            .volumes
            .as_ref()
            .map(|volumes| crate::call::popups::person::volumes_view(self.cx, volumes));
        if !menu.reactions.is_empty()
            && let Some(item) = &menu.item
        {
            // The quick reactions as Discord's: 44 px tiles a step over the
            // menu's ground, round by 8, lifted under the pointer, spread
            // across the menu 15 px in from its rows' edges.
            let mut quick = row![]
                .width(Length::Fill)
                .align_y(Alignment::Center)
                .padding(Padding {
                    top: 8.0,
                    right: 15.0,
                    bottom: 8.0,
                    left: 15.0,
                });
            let (ground, lifted) = (
                blend(theme.menu, theme.wash(0.06)),
                blend(theme.menu, theme.wash(0.12)),
            );
            for (index, key) in menu.reactions.iter().enumerate() {
                if index > 0 {
                    quick = quick.push(Space::new().width(Length::Fill));
                }
                quick = quick.push(
                    button(
                        container(crate::suggest::emoji_face(self.cx, key, QUICK_EMOJI))
                            .center(QUICK_TILE),
                    )
                    .padding(0)
                    .width(QUICK_TILE)
                    .height(QUICK_TILE)
                    .style(crate::controls::lit_style(Some(ground), lifted, 8.0))
                    .on_press(Message::Menu(Event::React(item.clone(), key.clone()))),
                );
            }
            rows = rows.push(quick);
        }
        // Rows line up past a tick's room when one row has a tick, and the
        // menu widens to its widest row.
        let (slot, width) = (menu.slot(), menu.fit(None));
        let message = menu.item.is_some();
        // The child of the rows' column the open submenu lines up with.
        let mut children = usize::from(!menu.reactions.is_empty() && menu.item.is_some());
        let mut sub_child = None;
        for (index, entry) in menu.rows.iter().enumerate() {
            if entry.separated && index > 0 {
                rows = rows.push(crate::controls::menu_separator(theme));
                children += 1;
            }
            let lit =
                (menu.lit == Some(index) && menu.sub_lit.is_none()) || menu.sub == Some(index);
            let light = Light::Lit(lit && !entry.disabled);
            rows = rows.push(
                mouse_area(self.item(
                    self.menu_entry(entry, slot),
                    light,
                    Message::Menu(Event::Choose(index)),
                    message,
                ))
                .on_enter(Message::Menu(Event::Light(index))),
            );
            if menu.sub == Some(index) {
                sub_child = Some(children);
            }
            children += 1;
        }
        let panel: Element<'a, Message> = if menu.rows.is_empty() {
            Space::new().into()
        } else {
            container(rows).width(width).style(menu_panel(theme)).into()
        };
        // Someone's volumes, in a panel of their own over their rows.
        let panel: Element<'a, Message> = match volumes {
            Some(volumes) => column![
                container(volumes).width(width).style(menu_panel(theme)),
                panel
            ]
            .spacing(8)
            .into(),
            None => panel,
        };
        // A menu taller than the window scrolls.
        let panel = container(
            scroll_view(panel)
                .height(Length::Shrink)
                .on_scroll(|viewport| Message::Menu(Event::Scrolled(viewport.absolute_offset().y)))
                .style(crate::style::scrollbar(theme)),
        )
        .max_height(self.cx.look.window.height - 2.0 * MENU_MARGIN);
        let mut place = Place::new(opaque(panel), menu.at, MENU_MARGIN).anchor(menu.anchor);
        if let (Some(sub), Some(child)) = (menu.sub, sub_child) {
            // Through the frame and the scrollable to the rows.
            let mut path = if menu.volumes.is_some() {
                vec![0, 0, 1, 0]
            } else {
                vec![0, 0, 0]
            };
            path.push(child);
            // The layout's places are the rows' unscrolled ones.
            let lift = PAD + menu.scrolled;
            place = place.beside(opaque(self.submenu_view(menu, sub)), path, lift, SUB_GAP);
        }
        Some(place.into())
    }

    /// A row: a message's menu's as Discord's ([`message_item`]), any
    /// other's as [`App::menu_item`] draws a context menu's.
    fn item(
        self,
        item: Item<'a>,
        light: Light,
        press: Message,
        message: bool,
    ) -> Element<'a, Message> {
        if message {
            message_item(self.cx, item, light, press)
        } else {
            self.cx.look.context_item(item, light, press)
        }
    }

    /// A menu row as [`App::menu_item`] draws it, with its submenu's
    /// chevron.
    fn menu_entry(self, entry: &'a MenuRow, slot: bool) -> Item<'a> {
        Item {
            label: entry.label.as_ref().into(),
            icon: entry.icon,
            danger: entry.danger,
            slot,
            disabled: entry.disabled,
            subtitle: entry.subtitle.as_deref().map(Into::into),
            check: if entry.submenu.is_empty() {
                entry.check
            } else {
                crate::controls::Check::Submenu
            },
        }
    }

    /// The open submenu: its rows, beside the row that opened it.
    fn submenu_view(self, menu: &'a Menu, sub: usize) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let entries = &menu.rows[sub].submenu;
        let slot = ticks(entries);
        let mut rows = column![].padding(PAD);
        for (index, entry) in entries.iter().enumerate() {
            if entry.separated && index > 0 {
                rows = rows.push(crate::controls::menu_separator(theme));
            }
            let light = Light::Lit(menu.sub_lit == Some(index) && !entry.disabled);
            rows = rows.push(
                mouse_area(self.item(
                    self.menu_entry(entry, slot),
                    light,
                    Message::Menu(Event::SubChoose(index)),
                    menu.item.is_some(),
                ))
                .on_enter(Message::Menu(Event::SubLight(index))),
            );
        }
        container(rows)
            .width(menu.fit(Some(sub)))
            .style(menu_panel(theme))
            .into()
    }
}

/// A message's menu's row as Discord's: 36 px (51 with a second line), a
/// 20 px icon in the secondary ink (a tick in the accent for the chosen
/// row), 14 px semibold words, a 16 px muted chevron for a submenu; the
/// overlay's wash when lit, a red one for a destructive row, round by 4.
fn message_item<'a>(
    cx: Ctx<'a>,
    item: Item<'a>,
    light: Light,
    press: Message,
) -> Element<'a, Message> {
    use crate::controls::Check;
    let look = cx.look;
    let theme = look.theme;
    let lit = light == Light::Lit(true);
    let ink = if item.danger {
        theme.danger_text
    } else {
        theme.text
    };
    let icon_ink = if item.danger || lit {
        ink
    } else {
        theme.secondary
    };
    let current = item.check == Check::Current;
    let mut face = row![].spacing(ROW_GAP).align_y(Alignment::Center);
    match item.icon {
        _ if current => face = face.push(look.glyph(Icon::Check, MESSAGE_ICON, theme.accent)),
        Some(icon) => face = face.push(look.glyph(icon, MESSAGE_ICON, icon_ink)),
        None if item.slot => {
            let slot = MESSAGE_ICON as f32;
            face = face.push(Space::new().width(slot).height(slot));
        }
        None => {}
    }
    let label = text(item.label)
        .size(14)
        .font(if current {
            crate::fonts::BOLD
        } else {
            crate::fonts::SEMIBOLD
        })
        .wrapping(text::Wrapping::None);
    let tall = item.subtitle.is_some();
    let words: Element<'a, Message> = match item.subtitle {
        Some(subtitle) => column![
            label,
            text(subtitle)
                .size(12)
                .color(theme.muted)
                .wrapping(text::Wrapping::None)
        ]
        .spacing(3)
        .into(),
        None => label.into(),
    };
    face = face.push(container(words).width(Length::Fill).clip(true));
    if let Check::Box(ticked) = item.check {
        face = face.push(look.checkbox(ticked, lit));
    }
    if item.check == Check::Submenu {
        face = face.push(look.glyph(Icon::ChevronRight, MESSAGE_CHEVRON, theme.muted));
    }
    let light_fill = match (item.danger, theme.bonfire) {
        (true, true) => crate::style::hex(0x34282d),
        (true, false) => crate::style::faded(theme.danger, 0.1),
        (false, _) => theme.wash(0.06),
    };
    // A disabled row: the whole of it at .45.
    let ink = if item.disabled {
        crate::style::faded(ink, 0.45)
    } else {
        ink
    };
    button(
        container(face)
            .height(Length::Fill)
            .align_y(Alignment::Center),
    )
    .padding([0, 8])
    .width(Length::Fill)
    .height(if tall { 51.0 } else { MESSAGE_ROW })
    .on_press_maybe((!item.disabled).then_some(press))
    .style(move |_, status| {
        let on = match light {
            Light::Pointer => crate::controls::lights(status),
            Light::Lit(lit) => lit,
        } && !matches!(status, button::Status::Disabled);
        button::Style {
            background: on.then(|| light_fill.into()),
            text_color: ink,
            border: Border::default().rounded(4),
            ..Default::default()
        }
    })
    .into()
}

/// Whether a row of `rows` has a tick, so that all leave room for one.
fn ticks(rows: &[MenuRow]) -> bool {
    rows.iter()
        .any(|row| row.check == crate::controls::Check::Current)
}

/// How wide a menu of `rows` has to be to show each whole: a row's padding,
/// its icon or tick's room, its words, and its checkbox or chevron, inside
/// the panel's padding; a `message`'s menu's rows as [`message_item`]
/// draws them.
fn fit(rows: &[MenuRow], slot: bool, message: bool) -> f32 {
    use crate::controls::{Check, ITEM_ICON};
    use iced::advanced::{
        graphics::text::Paragraph,
        text::{self, LineHeight, Paragraph as _, Shaping, Wrapping},
    };
    let words = |content: &str, size: f32, font| {
        Paragraph::with_text(text::Text {
            content,
            bounds: iced::Size::INFINITE,
            size: size.into(),
            line_height: LineHeight::default(),
            font,
            align_x: text::Alignment::Default,
            align_y: iced::alignment::Vertical::Top,
            shaping: Shaping::Advanced,
            wrapping: Wrapping::None,
        })
        .min_width()
    };
    rows.iter()
        .map(|row| {
            let current = row.check == Check::Current;
            let font = if current {
                crate::fonts::BOLD
            } else if message {
                crate::fonts::SEMIBOLD
            } else {
                crate::fonts::MEDIUM
            };
            let (glyph, chevron) = if message {
                (MESSAGE_ICON, MESSAGE_CHEVRON)
            } else {
                (ITEM_ICON, crate::controls::SUBMENU_ARROW)
            };
            let label = words(&row.label, 14.0, font);
            let subtitle = row
                .subtitle
                .as_deref()
                .map_or(0.0, |subtitle| words(subtitle, 12.0, crate::fonts::REGULAR));
            let icon = if slot || row.icon.is_some() || current {
                glyph as f32 + ROW_GAP
            } else {
                0.0
            };
            let end = match row.check {
                Check::Box(_) => CHECKBOX + ROW_GAP,
                Check::Submenu => chevron as f32 + ROW_GAP,
                _ if !row.submenu.is_empty() => chevron as f32 + ROW_GAP,
                _ => 0.0,
            };
            2.0 * (ROW_PAD + PAD) + icon + label.max(subtitle) + end
        })
        .fold(MENU_NARROWEST, f32::max)
        .ceil()
}

/// The narrowest a menu is, as Discord's.
const MENU_NARROWEST: f32 = 188.0;

/// A row's padding at each side, the gap between its parts, and its
/// checkbox, as [`App::menu_item`] draws them.
const ROW_PAD: f32 = 8.0;
const ROW_GAP: f32 = 8.0;
const CHECKBOX: f32 = 18.0;

/// The next row the keys can light from `from` on (wrapping), `step` at a
/// time, past greyed ones.
fn step(rows: &[MenuRow], from: usize, step: usize) -> Option<usize> {
    let count = rows.len();
    (0..count)
        .map(|offset| (from + offset * step) % count)
        .find(|&index| !rows[index].disabled)
}
