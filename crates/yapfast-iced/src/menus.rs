//! Context menus (a message's, the composer's, a place's, someone's). While
//! one is open it has the keys: the window's focus goes to [`NOBODY`]. A
//! row may open a submenu beside it (mute times, notification levels,
//! roles, servers to invite to, apps, channels to move to), under the
//! pointer, on a click, or with Right; Left or Escape closes it. A row
//! whose submenu is a choice of how (a mute's times) does its own thing
//! when clicked itself.

use crate::widgets::scroll_view;
use iced::{
    Element, Length, Padding, Point, Rectangle, Task, Vector,
    advanced::widget::{Id, Operation, operation::Scrollable},
    keyboard::key,
    widget::{
        Space, column, container, mouse_area, opaque, operation, row, scrollable::AbsoluteOffset,
    },
};
use yapfast_core::application::{Action, MenuChoice, ThreadFrom};

use crate::{
    app::{App, Message, NOBODY},
    controls::{Item, Light, menu_panel_over},
    ctx::Ctx,
    field::MenuEdit,
    picker::{Tab, Target},
    pictures::Icon,
    style::blend,
    widgets::{Anchor, Place, RightPress},
};
use yapfast_core::i18n::tr;

/// How far a menu stays in from the window's edges.
const MENU_MARGIN: f32 = 8.0;
/// The Roles submenu's scrollable, and its rows' height, which
/// [`RevealRole`] counts on.
const ROLE_LIST: &str = "menu-roles";
const ROLE_ROW_HEIGHT: f32 = 36.0;

/// A message's menu's width; the composer's is wider.
pub const WIDTH: f32 = 224.0;
pub const FIELD_WIDTH: f32 = 255.0;
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
    /// A submenu's row right-clicked, and its own choices beside the
    /// submenu (a folder's server's), with the row the keys light.
    pub side: Option<(usize, Vec<MenuRow>)>,
    pub side_lit: Option<usize>,
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
            side: None,
            side_lit: None,
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
        let fits = self.fits.get_or_init(|| {
            std::iter::once(fit(&self.rows, self.slot()))
                .chain(self.rows.iter().map(|row| {
                    if row.submenu.is_empty() {
                        0.0
                    } else {
                        fit(&row.submenu, ticks(&row.submenu))
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
    /// A game the detected games' menu lists, by its name in the row's
    /// `key`: never shown as the game played.
    BlockGame,
    /// A server of a folder's Servers list, by its row's `key`: it opens,
    /// as its icon on the rail does.
    GoServer,
    /// Close every open folder on the rail, as Discord's folder menu does.
    CloseFolders,
    /// A change to the notifications of the server in the row's `key`,
    /// from its own choices beside a folder's Servers list: that server
    /// alone, not the folder.
    NotifyServer(yapfast_core::chat::NotificationChange),
    /// A box of the inbox's Mentions filter, by its row's `key`.
    MentionsFilter,
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
    /// What a right click on it opens beside its submenu: a folder's
    /// server's own choices. Empty for none.
    pub side: Vec<MenuRow>,
}

impl MenuRow {
    fn opens_roles(&self) -> bool {
        matches!(
            self.act,
            MenuAct::Person(MenuChoice::Roles) | MenuAct::Place(MenuChoice::Roles)
        )
    }

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
            side: Vec::new(),
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
pub(crate) const SUB_GAP: f32 = 4.0;
/// A menu's padding, which lifts a submenu so its first row is level with
/// the row that opened it.
pub(crate) const PAD: f32 = 9.0;

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
    /// A right click on the open submenu's row: its own choices beside it.
    SideOpen(usize),
    /// The row of those choices under the pointer, or chosen.
    SideLight(usize),
    SideChoose(usize),
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
        Event::SideOpen(index) => {
            if let Some(menu) = &mut app.win.menu
                && let Some(side) = menu
                    .sub
                    .and_then(|sub| menu.rows.get(sub)?.submenu.get(index))
                    .map(|row| row.side.clone())
                    .filter(|side| !side.is_empty())
            {
                menu.sub_lit = Some(index);
                menu.side = Some((index, side));
                menu.side_lit = None;
            }
            Task::none()
        }
        Event::SideLight(index) => {
            if let Some(menu) = &mut app.win.menu {
                menu.side_lit = Some(index);
            }
            Task::none()
        }
        Event::SideChoose(index) => choose_side(app, index),
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
            app.cue(yapfast_core::themes::media::SoundSlot::ReactionAdd);
            let at = app.pressed.get();
            app.themes.particle_event(
                &app.theme.id,
                yapfast_core::themes::package::art::Trigger::ReactionAdded,
                Some((at.x, at.y)),
            );
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
    let state = app.client.state();
    let writes = crate::composer::composer_look(app.ctx()).enabled;
    let sent = message.delivery == yapfast_core::chat::Delivery::default();
    let reacts = message.can_react && adds_reactions(app, index);
    if reacts {
        rows.push(row(
            MenuAct::AddReaction,
            tr("Add reaction"),
            Icon::SmilePlus,
        ));
    }
    if !message.reactions.is_empty() {
        rows.push(row(
            MenuAct::ViewReactions,
            tr("View Reactions"),
            Icon::Smile,
        ));
    }
    if message.can_edit && writes {
        rows.push(row(MenuAct::Edit, tr("Edit message"), Icon::Pencil));
    }
    if message.can_reply && writes {
        rows.push(row(MenuAct::Reply, tr("Reply"), Icon::Reply));
    }
    if message.can_forward && crate::forward::allowed(app.client.state()) {
        rows.push(row(MenuAct::Forward, tr("Forward"), Icon::Forward));
    }
    // A message someone wrote starts a thread, or leads to its own.
    if index < crate::timeline::ASIDE
        && app.client.app.can_start_threads()
        && message.can_reply
        && sent
        && message.thread.is_none()
    {
        rows.push(row(MenuAct::NewThread, tr("Create Thread"), Icon::Thread));
    }
    if message.can_reply && message.thread.is_some() {
        rows.push(row(MenuAct::ViewThread, tr("View Thread"), Icon::Thread));
    }
    if !message.body.is_empty() {
        rows.push(row(MenuAct::CopyText, tr("Copy text"), Icon::Copy));
    }
    if crate::timeline::selection::copied_selection(app).is_some() {
        rows.push(row(
            MenuAct::CopySelection,
            tr("Copy selection"),
            Icon::TextSelect,
        ));
    }
    if message.poll.as_ref().is_some_and(|poll| poll.can_end) {
        rows.push(row(MenuAct::EndPoll, tr("End Poll Now"), Icon::CircleStop));
    }
    if message.can_pin && sent {
        rows.push(if message.pinned {
            row(MenuAct::Pin, tr("Unpin message"), Icon::PinOff)
        } else {
            row(MenuAct::Pin, tr("Pin message"), Icon::Pin)
        });
    }
    // The apps' message commands, beside it as Discord's are; not in
    // the thread beside, whose room the index isn't for, nor on an answer
    // only this account sees or one still to come.
    let takes_apps = index < crate::timeline::ASIDE
        && sent
        && message.flags & (crate::message::EPHEMERAL | crate::message::LOADING) == 0;
    let apps = takes_apps.then(|| apps_row(app)).flatten();
    let apps_due = takes_apps && apps.is_none();
    rows.extend(apps);
    // After Apps, as in Discord's menu.
    if app.client.app.marks_unread(&message.item) {
        rows.push(row(
            MenuAct::MarkUnread,
            tr("Mark Unread"),
            Icon::MessageCircle,
        ));
    }
    let link = state.message_link(&message.item);
    if link.is_some() {
        rows.push(row(MenuAct::CopyLink, tr("Copy Message Link"), Icon::Chain));
    }
    if message.can_delete && writes {
        rows.push(MenuRow {
            danger: true,
            separated: true,
            ..row(MenuAct::Delete, tr("Delete message"), Icon::Trash)
        });
    }
    // Last, as Discord's client puts it.
    if link.is_some() {
        rows.push(MenuRow {
            separated: true,
            ..row(MenuAct::CopyId, tr("Copy Message ID"), Icon::Id)
        });
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
    let Some(apps) = apps_row(app) else {
        return Task::none();
    };
    let Some(menu) = &mut app.win.menu else {
        return Task::none();
    };
    // Before Mark Unread, Copy Message Link and what's set apart.
    let at = menu
        .rows
        .iter()
        .position(|row| row.separated || matches!(row.act, MenuAct::MarkUnread | MenuAct::CopyLink))
        .unwrap_or(menu.rows.len());
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
            menu.side = None;
            menu.side_lit = None;
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
                    ..row.clone()
                };
                return pick(app, row, None);
            }
            None => {
                if let Some(menu) = &mut app.win.menu {
                    if menu.sub != Some(index) {
                        menu.side = None;
                        menu.side_lit = None;
                    }
                    menu.sub = Some(index);
                    menu.lit = Some(index);
                }
                return Task::none();
            }
        }
    }
    pick(app, row.clone(), Some(index))
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

/// A row of the choices beside the submenu was chosen: it applies and the
/// menu stays, its choice checked in place (a level, a switch); a mute,
/// whose rows change once Discord confirms it, puts the choices away.
pub fn choose_side(app: &mut App, index: usize) -> Task<Message> {
    let Some(row) = app
        .win
        .menu
        .as_ref()
        .and_then(|menu| menu.side.as_ref()?.1.get(index))
        .filter(|row| !row.disabled)
        .cloned()
    else {
        return Task::none();
    };
    let ticked = match row.check {
        crate::controls::Check::Box(ticked) => Some(!ticked),
        _ => None,
    };
    if let Some(menu) = &mut app.win.menu
        && let Some((_, rows)) = &mut menu.side
    {
        match row.check {
            crate::controls::Check::Box(ticked) => {
                rows[index].check = crate::controls::Check::Box(!ticked);
            }
            crate::controls::Check::Current | crate::controls::Check::None
                if matches!(
                    row.act,
                    MenuAct::NotifyServer(yapfast_core::chat::NotificationChange::Level(_))
                ) =>
            {
                for (at, shown) in rows.iter_mut().enumerate() {
                    if matches!(
                        shown.act,
                        MenuAct::NotifyServer(yapfast_core::chat::NotificationChange::Level(_))
                    ) {
                        shown.check = if at == index {
                            crate::controls::Check::Current
                        } else {
                            crate::controls::Check::None
                        };
                    }
                }
            }
            _ => {
                menu.side = None;
                menu.side_lit = None;
            }
        }
    }
    carry_out(app, &row, ticked, true)
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
    if let Some(on) = ticked {
        app.cue(if on {
            yapfast_core::themes::media::SoundSlot::ToggleOn
        } else {
            yapfast_core::themes::media::SoundSlot::ToggleOff
        });
    }
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
        | MenuAct::NotifyServer(_)
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
        MenuAct::BlockGame => {
            crate::settings::block_game(app, &row.key);
            focus
        }
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
        MenuAct::GoServer => {
            let server = row.key.clone();
            app.client
                .dispatch(yapfast_core::application::Action::Navigate(
                    yapfast_core::application::Navigation::Server(server),
                ));
            Task::batch([focus, app.sync()])
        }
        MenuAct::CloseFolders => {
            // A tree that opens one folder at a time keeps it apart from
            // the rail's open folders, as a click on a folder does.
            if crate::sidebar::tree::one_folder(app) {
                app.layout.tree_folder = None;
                crate::layout::store(app.client.root(), "tree_folder", &app.layout.tree_folder);
            } else {
                app.layout.open_folders.clear();
                crate::layout::store(app.client.root(), "open_folders", &app.layout.open_folders);
            }
            app.rail.items = crate::rail::rail(
                app.client.state(),
                &crate::sidebar::tree::folders_shown(app),
            );
            Task::batch([focus, app.ask()])
        }
        MenuAct::MentionsFilter => {
            if let Some(on) = ticked {
                crate::window_bar::change_filter(app, &row.key, on);
            }
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

/// Adds Copy Link and Open Link to the message's menu just opened, for the
/// link in its text the pointer was on, before Delete and Copy Message ID.
pub fn add_link_rows(app: &mut App, link: &str) {
    let (Some(address), Some(menu)) = (web_address(link), &mut app.win.menu) else {
        return;
    };
    let row = |act, label, icon| MenuRow::new(act, label, icon).key(address.to_owned());
    let at = menu
        .rows
        .iter()
        .position(|row| row.separated)
        .unwrap_or(menu.rows.len());
    let rows = [
        MenuRow {
            separated: true,
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
        | MenuAct::GoServer
        | MenuAct::CloseFolders
        | MenuAct::NotifyServer(_)
        | MenuAct::MentionsFilter
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
        | MenuAct::OpenTextLink
        | MenuAct::BlockGame => focus,
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
    // Beside a submenu, a row's own choices take the keys first.
    if let Some((_, rows)) = &menu.side {
        let count = rows.len();
        match named {
            Some(key::Named::ArrowDown | key::Named::Tab) => {
                menu.side_lit = step(rows, menu.side_lit.map_or(0, |lit| lit + 1), 1);
            }
            Some(key::Named::ArrowUp) if count > 0 => {
                let from = menu.side_lit.map_or(count - 1, |lit| lit + count - 1);
                menu.side_lit = step(rows, from, count - 1);
            }
            Some(key::Named::Enter | key::Named::Space) => {
                if let Some(lit) = menu.side_lit {
                    return choose_side(app, lit);
                }
            }
            Some(key::Named::ArrowLeft | key::Named::Escape) => {
                menu.side = None;
                menu.side_lit = None;
            }
            _ => {}
        }
        return Task::none();
    }
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
            if menu.sub == Some(sub)
                && menu.sub_lit != Some(lit)
                && menu.rows[sub].opens_roles()
                && let Some(index) = menu.sub_lit
            {
                return iced_runtime::task::widget(RevealRole(index));
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
                if rows[lit].opens_roles()
                    && let Some(index) = menu.sub_lit
                {
                    return iced_runtime::task::widget(RevealRole(index));
                }
            } else if named != Some(key::Named::ArrowRight) {
                return choose(app, lit);
            }
        }
        Some(key::Named::Escape) => return app.escape(),
        _ => {}
    }
    Task::none()
}

/// Keeps the keyboard selection in view using the scrollable's current layout.
struct RevealRole(usize);

impl Operation<Message> for RevealRole {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation<Message>)) {
        operate(self);
    }

    fn scrollable(
        &mut self,
        id: Option<&Id>,
        bounds: Rectangle,
        content_bounds: Rectangle,
        translation: Vector,
        state: &mut dyn Scrollable,
    ) {
        if id != Some(&Id::new(ROLE_LIST)) {
            return;
        }
        let top = self.0 as f32 * ROLE_ROW_HEIGHT;
        let bottom = top + ROLE_ROW_HEIGHT;
        let offset = if top < translation.y {
            top
        } else if bottom > translation.y + bounds.height {
            bottom - bounds.height
        } else {
            return;
        };
        state.scroll_by(
            AbsoluteOffset {
                x: 0.0,
                y: offset - translation.y,
            },
            bounds,
            content_bounds,
        );
    }
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
        let surface = yapfast_core::themes::package::art::ArtSurface::Menu;
        let decorated = self.cx.themes.decorates(&theme.id, surface);
        let mut rows = column![].padding(PAD);
        let volumes = menu
            .volumes
            .as_ref()
            .map(|volumes| crate::call::popups::person::volumes_view(self.cx, volumes));
        if !menu.reactions.is_empty()
            && let Some(item) = &menu.item
        {
            let mut quick = row![].spacing(4).padding(Padding {
                top: 0.0,
                right: 2.0,
                bottom: 4.0,
                left: 2.0,
            });
            // The quick reactions: the field's ground, lifted under the
            // pointer.
            let (ground, lifted) = (theme.field, blend(theme.field, theme.wash(0.12)));
            for key in &menu.reactions {
                quick = quick.push(crate::suggest::emoji_button(
                    self.cx,
                    key,
                    24,
                    44.0,
                    (Some(ground), lifted),
                    Message::Menu(Event::React(item.clone(), key.clone())),
                ));
            }
            rows = rows.push(quick);
        }
        // Rows line up past a tick's room when one row has a tick, and the
        // menu widens to its widest row.
        let (slot, width) = (menu.slot(), menu.fit(None));
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
                mouse_area(self.cx.look.context_item(
                    self.menu_entry(entry, slot),
                    light,
                    Message::Menu(Event::Choose(index)),
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
            container(rows)
                .width(width)
                .style(menu_panel_over(theme, decorated))
                .into()
        };
        // Someone's volumes, in a panel of their own over their rows.
        let panel: Element<'a, Message> = match volumes {
            Some(volumes) => column![
                container(volumes)
                    .width(width)
                    .style(menu_panel_over(theme, decorated)),
                panel
            ]
            .spacing(8)
            .into(),
            None => panel,
        };
        let panel = crate::theme_art::surface(panel, self.cx, surface);
        let panel = crate::theme_art::outlined(
            panel,
            self.cx,
            yapfast_core::themes::layout::BorderPlace::Menu,
            "menu",
            8.0,
            1.0,
            theme.menu_line,
        );
        // A menu taller than the window scrolls. Its shadow lies outside the
        // scroll, which would cut it off (Zine's hard blue offset).
        let shadow = theme.surface_shadow("menu", iced::Shadow::default());
        let panel = container(
            scroll_view(panel)
                .height(Length::Shrink)
                .on_scroll(|viewport| Message::Menu(Event::Scrolled(viewport.absolute_offset().y)))
                .style(crate::style::scrollbar(theme)),
        )
        .max_height(self.cx.look.window.height - 2.0 * MENU_MARGIN)
        .style(move |_| container::Style {
            shadow,
            ..Default::default()
        });
        let mut place = Place::new(opaque(panel), menu.at, MENU_MARGIN).anchor(menu.anchor);
        if let (Some(sub), Some(child)) = (menu.sub, sub_child) {
            // Through the frame and the scrollable to the rows.
            let mut path = vec![0, 0];
            if self
                .cx
                .themes
                .package_layout(&theme.id)
                .and_then(|layout| {
                    layout
                        .borders
                        .get(&yapfast_core::themes::layout::BorderPlace::Menu)
                })
                .is_some()
            {
                path.push(0);
            }
            if !self.cx.themes.overlays(&theme.id, false).is_empty()
                || !self
                    .cx
                    .themes
                    .procedural_overlays(&theme.id, false)
                    .is_empty()
                || self.cx.themes.particles(&theme.id).is_some()
            {
                path.push(0);
            }
            // The rows over what `theme_art::surface` lays under them.
            if let Some(index) = crate::theme_art::surface_content_index(self.cx, surface) {
                path.push(index);
            }
            if self.cx.themes.art_reserve(&theme.id, surface).is_some() {
                path.push(0);
            }
            if menu.volumes.is_some() {
                path.extend([1, 0]);
            } else {
                path.push(0);
            }
            path.push(child);
            // The layout's places are the rows' unscrolled ones.
            let lift = PAD + menu.scrolled;
            place = place.beside(self.submenu_view(menu, sub), path, lift, SUB_GAP);
        }
        Some(place.into())
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
        let surface = yapfast_core::themes::package::art::ArtSurface::Menu;
        let decorated = self.cx.themes.decorates(&theme.id, surface);
        let parent = &menu.rows[sub];
        let roles = parent.opens_roles();
        let entries = &parent.submenu;
        let slot = ticks(entries);
        let mut rows = column![].padding(if roles { [0.0, PAD] } else { [PAD, PAD] });
        for (index, entry) in entries.iter().enumerate() {
            if entry.separated && index > 0 {
                rows = rows.push(crate::controls::menu_separator(theme));
            }
            let light = Light::Lit(menu.sub_lit == Some(index) && !entry.disabled);
            let item = self.menu_entry(entry, slot);
            let press = Message::Menu(Event::SubChoose(index));
            let row = if roles {
                self.cx
                    .look
                    .menu_item_sized(item, light, press, ROLE_ROW_HEIGHT)
            } else {
                self.cx.look.context_item(item, light, press)
            };
            let item: Element<'a, Message> = mouse_area(row)
                .on_enter(Message::Menu(Event::SubLight(index)))
                .into();
            // A row with choices of its own opens them on a right click.
            rows = rows.push(if entry.side.is_empty() {
                item
            } else {
                RightPress::new(item, move |_| Message::Menu(Event::SideOpen(index))).into()
            });
        }
        let rows: Element<'a, Message> = if roles {
            scroll_view(rows)
                .id(ROLE_LIST)
                .height(Length::Shrink)
                .style(crate::style::scrollbar(theme))
                .into()
        } else {
            rows.into()
        };
        // Each panel takes the pointer; the room above the choices beside
        // it doesn't, so a click there still closes the menu.
        let panel = self.menu_surface(
            container(rows)
                .width(menu.fit(Some(sub)))
                .padding(if roles { [PAD, 0.0] } else { [0.0, 0.0] })
                // As tall as the window allows, as Discord's submenus are.
                .max_height(if roles {
                    (self.cx.look.window.height - 2.0 * MENU_MARGIN).max(0.0)
                } else {
                    f32::INFINITY
                })
                .style(menu_panel_over(theme, decorated))
                .into(),
        );
        let Some((at, side)) = &menu.side else {
            return panel;
        };
        // The choices beside the submenu, their first row level with the
        // row right-clicked: the rows above it are as tall as Discord's.
        let above: f32 = entries
            .iter()
            .take(*at)
            .enumerate()
            .map(|(index, entry)| {
                let line = if entry.separated && index > 0 {
                    SEPARATOR
                } else {
                    0.0
                };
                line + row_height(entry)
            })
            .sum::<f32>()
            + if entries.get(*at).is_some_and(|entry| entry.separated) && *at > 0 {
                SEPARATOR
            } else {
                0.0
            };
        let side_slot = ticks(side);
        let mut choices = column![].padding(PAD);
        for (index, entry) in side.iter().enumerate() {
            if entry.separated && index > 0 {
                choices = choices.push(crate::controls::menu_separator(theme));
            }
            let light = Light::Lit(menu.side_lit == Some(index) && !entry.disabled);
            choices = choices.push(
                mouse_area(self.cx.look.context_item(
                    self.menu_entry(entry, side_slot),
                    light,
                    Message::Menu(Event::SideChoose(index)),
                ))
                .on_enter(Message::Menu(Event::SideLight(index))),
            );
        }
        let width = fit(side, side_slot).max(SUB_WIDTH);
        row![
            panel,
            column![
                Space::new().height(above),
                self.menu_surface(
                    container(choices)
                        .width(width)
                        .style(menu_panel_over(theme, decorated))
                        .into(),
                ),
            ]
        ]
        .spacing(SUB_GAP)
        .into()
    }
}

impl<'a> View<'a> {
    /// A menu panel as the theme draws menus: its art surface and outline,
    /// taking the pointer so a click on it doesn't close the menu.
    fn menu_surface(self, panel: Element<'a, Message>) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let panel = crate::theme_art::surface(
            panel,
            self.cx,
            yapfast_core::themes::package::art::ArtSurface::Menu,
        );
        opaque(crate::theme_art::outlined(
            panel,
            self.cx,
            yapfast_core::themes::layout::BorderPlace::Menu,
            "menu",
            8.0,
            1.0,
            theme.menu_line,
        ))
    }
}

/// A separator's height between rows, as `controls::menu_separator`
/// draws it: a line with 8 px either side.
const SEPARATOR: f32 = 17.0;

/// How tall a menu row stands, as `Look::context_item` draws it.
fn row_height(row: &MenuRow) -> f32 {
    use crate::controls::Check;
    if row.subtitle.is_some() {
        51.0
    } else if row.icon.is_some()
        || (row.submenu.is_empty() && matches!(row.check, Check::Current | Check::Box(_)))
    {
        36.0
    } else {
        // 32 px and 2 under, or 34 with a submenu's arrow.
        34.0
    }
}

/// Whether a row of `rows` has a tick, so that all leave room for one.
fn ticks(rows: &[MenuRow]) -> bool {
    rows.iter()
        .any(|row| row.check == crate::controls::Check::Current)
}

/// How wide a menu of `rows` has to be to show each whole: a row's padding,
/// its icon or tick's room, its words, and its checkbox or chevron, inside
/// the panel's padding.
fn fit(rows: &[MenuRow], slot: bool) -> f32 {
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
            } else {
                crate::fonts::MEDIUM
            };
            let label = words(&row.label, 14.0, font);
            let subtitle = row
                .subtitle
                .as_deref()
                .map_or(0.0, |subtitle| words(subtitle, 12.0, crate::fonts::REGULAR));
            let icon = if slot || row.icon.is_some() || current {
                ITEM_ICON as f32 + ROW_GAP
            } else {
                0.0
            };
            let end = match row.check {
                Check::Box(_) => CHECKBOX + ROW_GAP,
                _ if !row.submenu.is_empty() => crate::controls::SUBMENU_ARROW as f32 + ROW_GAP,
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
