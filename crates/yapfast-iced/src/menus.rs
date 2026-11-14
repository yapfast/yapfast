//! Context menus (a message's, the composer's), as the Slint client's
//! `ContextMenu`. While one is open it has the keys: the window's focus
//! goes to [`NOBODY`].

use iced::{
    Element, Padding, Point, Task,
    keyboard::key,
    widget::{column, container, mouse_area, opaque, operation, row},
};
use yapfast_core::application::Action;

use crate::{
    app::{App, Message, NOBODY},
    controls::{Item, Light, menu_panel, separator},
    field::MenuEdit,
    picker::{Tab, Target},
    pictures::Icon,
    style::blend,
    widgets::{Anchor, Place},
};

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
}

impl Menu {
    /// A menu of `rows` opening at the pointer at `at`.
    pub fn new(at: Point, rows: Vec<MenuRow>) -> Self {
        Self {
            at,
            rows,
            lit: None,
            item: None,
            reactions: false,
            place: None,
            header: false,
            location: crate::person::CONTEXT_MENU,
            anchor: Anchor::Pointer,
            width: WIDTH,
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
    NewThread,
    NewPoll,
    Place(yapfast_core::application::MenuChoice),
    /// A row of someone's menu (`person.rs`).
    Person(yapfast_core::application::MenuChoice),
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
}

impl MenuRow {
    /// A row with its icon.
    pub fn new(act: MenuAct, label: &'static str, icon: Icon) -> Self {
        Self {
            act,
            label: label.into(),
            icon: Some(icon),
            danger: false,
            separated: false,
            disabled: false,
        }
    }
}

impl App {
    /// A right click on a message: what one may do with it, in the Slint
    /// client's order.
    pub fn open_menu(&mut self, index: usize, at: Point) -> Task<Message> {
        let Some(message) = self.messages.get(index) else {
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
            reactions: message.can_react,
            ..Menu::new(at, rows)
        });
        operation::focus(NOBODY)
    }

    /// A row of the open menu was chosen, by click or key.
    pub fn choose(&mut self, index: usize) -> Task<Message> {
        // A greyed row does nothing, and the menu stays.
        if self
            .menu
            .as_ref()
            .and_then(|menu| menu.rows.get(index))
            .is_none_or(|row| row.disabled)
        {
            return Task::none();
        }
        let Some(menu) = self.menu.take() else {
            return Task::none();
        };
        let Some(row) = menu.rows.get(index) else {
            return Task::none();
        };
        if let MenuAct::Person(choice) = row.act {
            let user = menu.place.clone().unwrap_or_default();
            return Task::batch([
                self.refocus(),
                self.choose_person(user, menu.location, choice),
            ]);
        }
        let item = menu.item.clone().unwrap_or_default();
        self.act(row.act, item, menu.at, menu.place)
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
            MenuAct::NewThread | MenuAct::NewPoll => focus,
            MenuAct::Place(choice) => {
                let place = place.unwrap_or_default();
                Task::batch([self.choose_place(place, choice), focus])
            }
            MenuAct::Person(choice) => {
                let user = place.unwrap_or_default();
                Task::batch([
                    focus,
                    self.choose_person(user, crate::person::CONTEXT_MENU, choice),
                ])
            }
        }
    }

    /// The open menu's keys: the arrows, Tab, Home and End light a row,
    /// Enter or Space chooses it, Escape closes the menu.
    pub fn menu_key(&mut self, named: Option<key::Named>) -> Task<Message> {
        let Some(menu) = &mut self.menu else {
            return Task::none();
        };
        let count = menu.rows.len();
        if count == 0 {
            return Task::none();
        }
        let lit = menu.lit;
        // The next row the keys can light from `from` on, `step` at a time,
        // past greyed ones.
        let rows = &menu.rows;
        let next = |from: usize, step: usize| {
            (0..count)
                .map(|offset| (from + offset * step) % count)
                .find(|&index| !rows[index].disabled)
        };
        match named {
            Some(key::Named::ArrowDown | key::Named::Tab) => {
                menu.lit = next(lit.map_or(0, |lit| (lit + 1) % count), 1).or(lit);
            }
            Some(key::Named::ArrowUp) => {
                let from = lit.map_or(count - 1, |lit| (lit + count - 1) % count);
                menu.lit = next(from, count - 1).or(lit);
            }
            Some(key::Named::Home) => menu.lit = next(0, 1),
            Some(key::Named::End) => menu.lit = next(count - 1, count - 1),
            Some(key::Named::Enter | key::Named::Space) => {
                if let Some(lit) = lit {
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
        for (index, entry) in menu.rows.iter().enumerate() {
            if entry.separated && index > 0 {
                rows = rows.push(separator(theme));
            }
            let item = Item {
                label: entry.label.as_ref().into(),
                icon: entry.icon,
                danger: entry.danger,
                slot: menu.place.is_none(),
                disabled: entry.disabled,
            };
            let light = Light::Lit(menu.lit == Some(index) && !entry.disabled);
            rows = rows.push(
                mouse_area(self.menu_item(item, light, Message::MenuChoose(index)))
                    .on_enter(Message::MenuLight(index)),
            );
        }
        let panel = container(rows).width(menu.width).style(menu_panel(theme));
        Some(
            Place::new(opaque(panel), menu.at, 8.0)
                .anchor(menu.anchor)
                .into(),
        )
    }
}
