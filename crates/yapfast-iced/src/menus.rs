//! Context menus (a message's, the composer's, a place's, someone's), as
//! the Slint client's `ContextMenu`. While one is open it has the keys: the
//! window's focus goes to [`NOBODY`]. A row may open a submenu beside it
//! (mute times, notification levels, roles, servers to invite to, apps,
//! channels to move to), under the pointer, on a click, or with Right;
//! Left or Escape closes it. A row whose submenu is a choice of how (a
//! mute's times) does its own thing when clicked itself.

use iced::{
    Element, Length, Padding, Point, Task,
    keyboard::key,
    widget::{Space, column, container, mouse_area, opaque, operation, row, scrollable},
};
use yapfast_core::application::{Action, ThreadFrom};

use crate::{
    app::{App, Message, NOBODY},
    controls::{Item, Light, menu_panel, separator},
    field::MenuEdit,
    picker::{Tab, Target},
    pictures::Icon,
    style::blend,
    widgets::{Anchor, Place},
};

/// How far a menu stays in from the window's edges.
const MENU_MARGIN: f32 = 8.0;

/// A message's menu's width; the composer's is wider, as in the Slint
/// client.
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
    /// A message's menu: its row, to build again as its Apps come.
    pub row: Option<usize>,
    pub item: Option<String>,
    /// The quick reactions above the rows.
    pub reactions: bool,
    /// The place whose menu it is (`places.rs`), and whether it's a
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
            row: None,
            reactions: false,
            place: None,
            header: false,
            location: crate::person::CONTEXT_MENU,
            anchor: Anchor::Pointer,
            width: WIDTH,
            voice: None,
            volumes: None,
            sub: None,
            sub_lit: None,
            fits: Default::default(),
        }
    }

    /// Whether its rows leave room for an icon or tick: a message's always,
    /// others when a row has a tick.
    fn slot(&self) -> bool {
        self.place.is_none() || ticks(&self.rows)
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
    /// A row of someone's menu (`person.rs`).
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
}

#[derive(Clone)]
pub struct MenuRow {
    pub act: MenuAct,
    pub label: std::borrow::Cow<'static, str>,
    /// Its icon; a place's menu has none but Copy ID's, and no room kept
    /// for one.
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

/// A submenu's width, as the Slint client's narrowest menu.
pub const SUB_WIDTH: f32 = FIELD_WIDTH;
/// Between a menu and its submenu.
const SUB_GAP: f32 = 4.0;
/// A menu's padding, which lifts a submenu so its first row is level with
/// the row that opened it.
const PAD: f32 = 6.0;

impl App {
    /// A right click on a message: what one may do with it, in the Slint
    /// client's order.
    pub fn open_menu(&mut self, index: usize, at: Point) -> Task<Message> {
        let Some(message) = self.row_message(index).cloned() else {
            return Task::none();
        };
        if message.deleted {
            return Task::none();
        }
        let row = MenuRow::new;
        let mut rows = Vec::new();
        let state = self.client.state();
        let writes = self.composer_look().enabled;
        let sent = message.delivery == yapfast_core::chat::Delivery::default();
        if message.can_react {
            rows.push(row(MenuAct::AddReaction, "Add reaction", Icon::SmilePlus));
        }
        if !message.reactions.is_empty() {
            rows.push(row(MenuAct::ViewReactions, "View Reactions", Icon::Smile));
        }
        if message.can_edit && writes {
            rows.push(row(MenuAct::Edit, "Edit message", Icon::Pencil));
        }
        if message.can_reply && writes {
            rows.push(row(MenuAct::Reply, "Reply", Icon::Reply));
        }
        if message.can_forward && self.forward_allowed() {
            rows.push(row(MenuAct::Forward, "Forward", Icon::Forward));
        }
        // A message someone wrote starts a thread, or leads to its own.
        if index < crate::timeline::ASIDE
            && self.client.app.can_start_threads()
            && message.can_reply
            && sent
            && message.thread.is_none()
        {
            rows.push(row(MenuAct::NewThread, "Create Thread", Icon::Thread));
        }
        if message.can_reply && message.thread.is_some() {
            rows.push(row(MenuAct::ViewThread, "View Thread", Icon::Thread));
        }
        if !message.body.is_empty() {
            rows.push(row(MenuAct::CopyText, "Copy text", Icon::Copy));
        }
        if self.copied().is_some() {
            rows.push(row(
                MenuAct::CopySelection,
                "Copy selection",
                Icon::TextSelect,
            ));
        }
        if message.poll.as_ref().is_some_and(|poll| poll.can_end) {
            rows.push(row(MenuAct::EndPoll, "End Poll Now", Icon::CircleStop));
        }
        if message.can_pin && sent {
            rows.push(if message.pinned {
                row(MenuAct::Pin, "Unpin message", Icon::PinOff)
            } else {
                row(MenuAct::Pin, "Pin message", Icon::Pin)
            });
        }
        // The apps' message commands, beside it as Discord's are; not in
        // the thread beside, whose room the index isn't for.
        let apps: Vec<MenuRow> = match state.selected.as_deref() {
            Some(room) if index < crate::timeline::ASIDE && sent => self
                .client
                .app
                .commands_for_target(room, yapfast_core::application::CommandTarget::Message)
                .into_iter()
                .map(|command| MenuRow {
                    subtitle: Some(command.app_name.into()),
                    ..MenuRow::plain(MenuAct::App, command.name).key(command.command)
                })
                .collect(),
            _ => Vec::new(),
        };
        if !apps.is_empty() {
            rows.push(MenuRow {
                submenu: apps,
                ..row(MenuAct::App, "Apps", Icon::Slash)
            });
        }
        // After Apps, as in Discord's menu.
        if self.client.app.marks_unread(&message.item) {
            rows.push(row(MenuAct::MarkUnread, "Mark Unread", Icon::MessageCircle));
        }
        let link = state.message_link(&message.item);
        if link.is_some() {
            rows.push(row(MenuAct::CopyLink, "Copy Message Link", Icon::Chain));
        }
        if message.can_delete && writes {
            rows.push(MenuRow {
                danger: true,
                separated: true,
                ..row(MenuAct::Delete, "Delete message", Icon::Trash)
            });
        }
        // Last, as Discord's client puts it.
        if link.is_some() {
            rows.push(MenuRow {
                separated: true,
                ..row(MenuAct::CopyId, "Copy Message ID", Icon::Id)
            });
        }
        if rows.is_empty() {
            return Task::none();
        }
        self.menu = Some(Menu {
            item: Some(message.item.clone()),
            row: Some(index),
            reactions: message.can_react,
            ..Menu::new(at, rows)
        });
        // Its Apps follow the permissions as they are now.
        self.client.dispatch(Action::AskAppCommands);
        operation::focus(NOBODY)
    }

    /// A message's menu opened before its room's apps' commands came: it's
    /// built again with its Apps once they have.
    pub fn menu_apps_sync(&mut self) -> Task<Message> {
        let Some(menu) = &self.menu else {
            return Task::none();
        };
        let (Some(row), Some(room)) = (menu.row, self.client.state().selected.as_deref()) else {
            return Task::none();
        };
        let has_apps = menu.rows.iter().any(|row| row.act == MenuAct::App);
        if has_apps || row >= crate::timeline::ASIDE || menu.sub.is_some() {
            return Task::none();
        }
        let commands = self
            .client
            .app
            .commands_for_target(room, yapfast_core::application::CommandTarget::Message);
        if commands.is_empty() {
            return Task::none();
        }
        let at = menu.at;
        self.open_menu(row, at)
    }

    /// The pointer came onto a row: it's lit, and its submenu opens, or
    /// another's closes.
    pub fn menu_light(&mut self, index: usize) -> Task<Message> {
        if let Some(menu) = &mut self.menu {
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
    pub fn choose(&mut self, index: usize) -> Task<Message> {
        // A greyed row does nothing, and the menu stays.
        let Some(row) = self
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
                    return self.pick(row, None);
                }
                None => {
                    if let Some(menu) = &mut self.menu {
                        menu.sub = Some(index);
                        menu.lit = Some(index);
                    }
                    return Task::none();
                }
            }
        }
        self.pick(row, Some(index))
    }

    /// A row of the open submenu was chosen.
    pub fn choose_sub(&mut self, index: usize) -> Task<Message> {
        let Some(row) = self
            .menu
            .as_ref()
            .and_then(|menu| menu.rows.get(menu.sub?)?.submenu.get(index))
            .filter(|row| !row.disabled)
            .cloned()
        else {
            return Task::none();
        };
        if let crate::controls::Check::Box(ticked) = row.check {
            if let Some(menu) = &mut self.menu
                && let Some(sub) = menu.sub
                && let Some(row) = menu.rows[sub].submenu.get_mut(index)
            {
                row.check = crate::controls::Check::Box(!ticked);
            }
            return self.carry_out(&row, Some(!ticked), true);
        }
        self.carry_out(&row, None, false)
    }

    /// Carries out a chosen row of the menu's own: a checkbox's flips and
    /// the menu stays; anything else closes it.
    fn pick(&mut self, row: MenuRow, index: Option<usize>) -> Task<Message> {
        if let (crate::controls::Check::Box(ticked), Some(index)) = (row.check, index) {
            if let Some(shown) = self.menu.as_mut().and_then(|menu| menu.rows.get_mut(index)) {
                shown.check = crate::controls::Check::Box(!ticked);
            }
            return self.carry_out(&row, Some(!ticked), true);
        }
        self.carry_out(&row, None, false)
    }

    /// What a chosen row does, in the open menu's place: `ticked` is a
    /// checkbox's new state, and `stays` keeps the menu open.
    fn carry_out(&mut self, row: &MenuRow, ticked: Option<bool>, stays: bool) -> Task<Message> {
        let Some(menu) = (if stays {
            self.menu.as_ref().map(Menu::context)
        } else {
            self.menu.take().map(|menu| menu.context())
        }) else {
            return Task::none();
        };
        let focus = if stays { Task::none() } else { self.refocus() };
        match row.act {
            MenuAct::Person(choice) => match &menu.voice {
                Some((room, user)) => {
                    let task = self.choose_voice(room.clone(), user.clone(), choice, ticked);
                    Task::batch([focus, task])
                }
                None => {
                    let user = menu.place.clone().unwrap_or_default();
                    Task::batch([focus, self.choose_person(user, menu.location, choice)])
                }
            },
            MenuAct::Notify(_)
            | MenuAct::MuteDm(_)
            | MenuAct::ShowAll
            | MenuAct::Role
            | MenuAct::InviteTo
            | MenuAct::App
            | MenuAct::MoveTo => Task::batch([focus, self.sub_act(&menu, row, ticked)]),
            MenuAct::CopyImage
            | MenuAct::SaveMedia
            | MenuAct::CopyMediaLink
            | MenuAct::OpenMediaLink => {
                let item = menu.item.clone().unwrap_or_default();
                let index = row.key.parse().unwrap_or_default();
                Task::batch([focus, self.media_act(row.act, item, index)])
            }
            act => {
                let item = menu.item.clone().unwrap_or_default();
                self.act(act, item, menu.at, menu.place)
            }
        }
    }

    /// What a menu's row, or the button of a message's hover bar, does:
    /// to the message `item`, or the place `place`, opened at `at`.
    pub fn act(
        &mut self,
        act: MenuAct,
        item: String,
        at: Point,
        place: Option<String>,
    ) -> Task<Message> {
        let focus = self.refocus();
        match act {
            MenuAct::AddReaction => self.open_picker(Target::React(item, at), Tab::Emoji),
            MenuAct::ViewReactions => {
                self.client
                    .dispatch(Action::ShowReactions { item, key: None });
                Task::batch([self.sync(), operation::focus(crate::app::NOBODY)])
            }
            MenuAct::Edit => self.begin_edit(item),
            MenuAct::Reply => {
                self.client.dispatch(Action::Reply(item));
                Task::batch([self.sync(), focus])
            }
            MenuAct::CopyText => {
                let body = self
                    .client
                    .state()
                    .message(&item)
                    .map(|message| message.body.clone())
                    .unwrap_or_default();
                Task::batch([iced::clipboard::write(body), focus])
            }
            MenuAct::CopySelection => match self.copied() {
                Some(copied) => Task::batch([iced::clipboard::write(copied), focus]),
                None => focus,
            },
            MenuAct::Delete => {
                self.client.dispatch(Action::RequestDelete(item));
                self.confirm_focus = true;
                Task::batch([self.sync(), operation::focus(NOBODY)])
            }
            MenuAct::DeleteNow => {
                self.client.dispatch(Action::DeleteNow(item));
                Task::batch([self.sync(), focus])
            }
            MenuAct::Forward => {
                self.client.dispatch(Action::StartForward(item));
                Task::batch([self.sync(), self.forward_focus()])
            }
            MenuAct::EndPoll => {
                self.client.dispatch(Action::AskEndPoll(item));
                Task::batch([self.sync(), operation::focus(NOBODY)])
            }
            MenuAct::Pin => {
                self.client.dispatch(Action::PinMessage(item));
                Task::batch([self.sync(), focus])
            }
            MenuAct::MarkUnread => {
                self.client.dispatch(Action::MarkUnread(item));
                Task::batch([self.sync(), focus])
            }
            MenuAct::CopyLink | MenuAct::CopyId => {
                let (link, id) = self.client.state().message_link(&item).unwrap_or_default();
                let copied = if act == MenuAct::CopyLink { link } else { id };
                Task::batch([iced::clipboard::write(copied), focus])
            }
            MenuAct::Field(which, edit) => self.field_menu(which, edit),
            MenuAct::Upload => Task::batch([focus, self.attach()]),
            MenuAct::NewThread if item.is_empty() => {
                self.client
                    .dispatch(crate::thread::start_from(ThreadFrom::PlusButton));
                Task::batch([self.sync(), operation::focus(crate::thread::FIRST)])
            }
            MenuAct::NewThread => self.thread_event(crate::thread::Event::Start(item)),
            MenuAct::ViewThread => {
                let room = self
                    .client
                    .state()
                    .message(&item)
                    .and_then(|message| message.thread.as_ref())
                    .map(|thread| thread.room.clone());
                match room {
                    Some(room) => self.thread_event(crate::thread::Event::Open(room)),
                    None => focus,
                }
            }
            MenuAct::Event => focus,
            MenuAct::NewPoll => self.switcher_event(crate::switcher::Event::OpenPoll),
            MenuAct::Place(choice) => {
                let place = place.unwrap_or_default();
                let chose = self.choose_place(place, choice);
                // Asked after the choice, which may have opened settings
                // that keep the keys from the composer.
                Task::batch([chose, self.refocus()])
            }
            MenuAct::Person(choice) => {
                let user = place.unwrap_or_default();
                Task::batch([
                    focus,
                    self.choose_person(user, crate::person::CONTEXT_MENU, choice),
                ])
            }
            // A menu's own, which `carry_out` hands to `sub_act`.
            MenuAct::Notify(_)
            | MenuAct::MuteDm(_)
            | MenuAct::ShowAll
            | MenuAct::Role
            | MenuAct::InviteTo
            | MenuAct::App
            | MenuAct::MoveTo
            | MenuAct::CopyImage
            | MenuAct::SaveMedia
            | MenuAct::CopyMediaLink
            | MenuAct::OpenMediaLink => focus,
        }
    }

    /// The open menu's keys: the arrows, Tab, Home and End light a row,
    /// Enter or Space chooses it, Right opens its submenu and Left leaves
    /// it, Escape closes the submenu, then the menu.
    pub fn menu_key(&mut self, named: Option<key::Named>) -> Task<Message> {
        let Some(menu) = &mut self.menu else {
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
                    Some(key::Named::Enter | key::Named::Space) => return self.choose_sub(lit),
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
                    return self.choose(lit);
                }
            }
            Some(key::Named::Escape) => return self.escape(),
            _ => {}
        }
        Task::none()
    }

    pub fn menu_view(&self) -> Option<Element<'_, Message>> {
        let menu = self.menu.as_ref()?;
        let theme = &self.theme;
        let mut rows = column![].spacing(2).padding(6);
        let volumes = menu
            .volumes
            .as_ref()
            .map(|volumes| self.volumes_view(volumes));
        if menu.reactions
            && let Some(item) = &menu.item
        {
            let mut quick = row![].spacing(4).padding(Padding {
                top: 0.0,
                right: 2.0,
                bottom: 4.0,
                left: 2.0,
            });
            // As the Slint client's quick reactions: the field's ground,
            // lifted under the pointer.
            let (ground, lifted) = (theme.field, blend(theme.field, theme.wash(0.12)));
            for key in self.client.app.frequent_reactions(QUICK) {
                quick = quick.push(self.emoji_button(
                    &key,
                    24,
                    44.0,
                    (Some(ground), lifted),
                    Message::React(item.clone(), key.clone()),
                ));
            }
            rows = rows.push(quick);
        }
        // Rows line up past a tick's room when one row has a tick, and the
        // menu widens to its widest row.
        let (slot, width) = (menu.slot(), menu.fit(None));
        // The child of the rows' column the open submenu lines up with.
        let mut children = usize::from(menu.reactions && menu.item.is_some());
        let mut sub_child = None;
        for (index, entry) in menu.rows.iter().enumerate() {
            if entry.separated && index > 0 {
                rows = rows.push(separator(theme));
                children += 1;
            }
            let lit =
                (menu.lit == Some(index) && menu.sub_lit.is_none()) || menu.sub == Some(index);
            let light = Light::Lit(lit && !entry.disabled);
            rows = rows.push(
                mouse_area(self.menu_item(
                    self.menu_entry(entry, slot),
                    light,
                    Message::MenuChoose(index),
                ))
                .on_enter(Message::MenuLight(index)),
            );
            if menu.sub == Some(index) {
                sub_child = Some(children);
            }
            children += 1;
        }
        let panel: Element<'_, Message> = if menu.rows.is_empty() {
            Space::new().into()
        } else {
            container(rows).width(width).style(menu_panel(theme)).into()
        };
        // Someone's volumes, in a panel of their own over their rows.
        let panel: Element<'_, Message> = match volumes {
            Some(volumes) => column![
                container(volumes).width(width).style(menu_panel(theme)),
                panel
            ]
            .spacing(8)
            .into(),
            None => panel,
        };
        // A menu taller than the window scrolls, as the Slint client's
        // does past its tallest.
        let panel = container(
            scrollable(panel)
                .height(Length::Shrink)
                .style(crate::style::scrollbar(theme)),
        )
        .max_height(self.window.height - 2.0 * MENU_MARGIN);
        let mut place = Place::new(opaque(panel), menu.at, MENU_MARGIN).anchor(menu.anchor);
        if let (Some(sub), Some(child)) = (menu.sub, sub_child) {
            // Through the frame and the scrollable to the rows.
            let mut path = if menu.volumes.is_some() {
                vec![0, 0, 1, 0]
            } else {
                vec![0, 0, 0]
            };
            path.push(child);
            place = place.beside(opaque(self.submenu_view(menu, sub)), path, PAD, SUB_GAP);
        }
        Some(place.into())
    }

    /// A menu row as [`App::menu_item`] draws it, with its submenu's
    /// chevron.
    fn menu_entry<'a>(&self, entry: &'a MenuRow, slot: bool) -> Item<'a> {
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
    fn submenu_view<'a>(&'a self, menu: &'a Menu, sub: usize) -> Element<'a, Message> {
        let theme = &self.theme;
        let entries = &menu.rows[sub].submenu;
        let slot = ticks(entries);
        let mut rows = column![].spacing(2).padding(PAD);
        for (index, entry) in entries.iter().enumerate() {
            if entry.separated && index > 0 {
                rows = rows.push(separator(theme));
            }
            let light = Light::Lit(menu.sub_lit == Some(index) && !entry.disabled);
            rows = rows.push(
                mouse_area(self.menu_item(
                    self.menu_entry(entry, slot),
                    light,
                    Message::SubChoose(index),
                ))
                .on_enter(Message::SubLight(index)),
            );
        }
        container(rows)
            .width(menu.fit(Some(sub)))
            .style(menu_panel(theme))
            .into()
    }
}

/// Whether a row of `rows` has a tick, so that all leave room for one.
fn ticks(rows: &[MenuRow]) -> bool {
    rows.iter()
        .any(|row| row.check == crate::controls::Check::Current)
}

/// How wide a menu of `rows` has to be to show each whole, as the Slint
/// client's grows past its narrowest: a row's padding, its icon or tick's
/// room, its words, and its checkbox or chevron, inside the panel's padding.
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
        .fold(0.0, f32::max)
        .ceil()
}

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
