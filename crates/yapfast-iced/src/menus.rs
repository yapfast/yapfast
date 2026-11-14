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
    composer::COMPOSER,
    controls::{Item, Light, menu_panel, separator},
    field::MenuEdit,
    picker::Target,
    pictures::Icon,
    style::blend,
    widgets::{Anchor, Place},
};

/// A message's menu's width; the composer's is wider, as in the Slint
/// client.
const WIDTH: f32 = 224.0;
const FIELD_WIDTH: f32 = 255.0;
/// Discord's quick reactions over a message's menu, until this device has
/// reacted; the first three are on its hover bar.
pub const QUICK: [&str; 4] = ["👍", "❤️", "😂", "🎉"];

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
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MenuAct {
    AddReaction,
    Edit,
    Reply,
    CopyText,
    CopySelection,
    Delete,
    Field(MenuEdit),
    Place(yapfast_core::application::MenuChoice),
}

#[derive(Clone, Copy)]
pub struct MenuRow {
    pub act: MenuAct,
    pub label: &'static str,
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
            label,
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
        if message.can_react {
            rows.push(row(MenuAct::AddReaction, "Add reaction", Icon::SmilePlus));
        }
        if message.can_edit {
            rows.push(row(MenuAct::Edit, "Edit message", Icon::Pencil));
        }
        if message.can_reply {
            rows.push(row(MenuAct::Reply, "Reply", Icon::Reply));
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
        if message.can_delete {
            rows.push(MenuRow {
                danger: true,
                separated: true,
                ..row(MenuAct::Delete, "Delete message", Icon::Trash)
            });
        }
        if rows.is_empty() {
            return Task::none();
        }
        self.menu = Some(Menu {
            at,
            rows,
            lit: None,
            item: Some(message.item.clone()),
            reactions: message.can_react,
            place: None,
            header: false,
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
        let focus = operation::focus(COMPOSER);
        match act {
            MenuAct::AddReaction => self.open_picker(Target::React(item, at)),
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
            MenuAct::Field(edit) => self.composer_menu(edit),
            MenuAct::Place(choice) => {
                let place = place.unwrap_or_default();
                Task::batch([self.choose_place(place, choice), focus])
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
            for key in QUICK {
                quick = quick.push(self.emoji_button(
                    key,
                    24,
                    44.0,
                    (Some(ground), lifted),
                    Message::React(item.clone(), key),
                ));
            }
            rows = rows.push(quick);
        }
        for (index, entry) in menu.rows.iter().enumerate() {
            if entry.separated && index > 0 {
                rows = rows.push(separator(theme));
            }
            let item = Item {
                label: entry.label,
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
        let width = if menu.item.is_some() {
            WIDTH
        } else {
            FIELD_WIDTH
        };
        let panel = container(rows).width(width).style(menu_panel(theme));
        Some(
            Place::new(opaque(panel), menu.at, 8.0)
                .anchor(Anchor::Pointer)
                .into(),
        )
    }
}
