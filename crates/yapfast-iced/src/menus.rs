//! Context menus (a message's, the composer's) and Delete's confirmation,
//! as the Slint client's `ContextMenu` and confirmation dialogs. While one
//! is open it has the keys: the window's focus goes to [`NOBODY`].

use iced::{
    Alignment, Border, Color, Element, Length, Padding, Point, Size, Task,
    keyboard::key,
    widget::{Space, button, column, container, mouse_area, opaque, operation, row, stack, text},
};
use yapfast_core::application::Action;

use crate::{
    app::{App, Message, NOBODY},
    composer::COMPOSER,
    field::MenuEdit,
    fonts::{BOLD, MEDIUM},
    picker::Target,
    pictures::Icon,
    style::{
        ACCENT, BACKGROUND, DANGER, DANGER_TEXT, FIELD, LINE, MENU, MENU_LINE, SECONDARY_INK, TEXT,
        blend, fill, hex, raised_shadow, white,
    },
    widgets::{Anchor, Pic, Place},
};

/// A message's menu's width; the composer's is wider, as in the Slint
/// client.
const WIDTH: f32 = 224.0;
const FIELD_WIDTH: f32 = 255.0;
/// Discord's quick reactions over a message's menu, until this device has
/// reacted.
const QUICK: [&str; 4] = ["👍", "❤️", "😂", "🎉"];

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
}

#[derive(Clone, Copy)]
pub struct MenuRow {
    pub act: MenuAct,
    pub label: &'static str,
    pub icon: Icon,
    pub danger: bool,
    /// A line above it.
    pub separated: bool,
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
        let row = |act, label, icon| MenuRow {
            act,
            label,
            icon,
            danger: false,
            separated: false,
        };
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
                act: MenuAct::Delete,
                label: "Delete message",
                icon: Icon::Trash,
                danger: true,
                separated: true,
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
        });
        operation::focus(NOBODY)
    }

    /// A row of the open menu was chosen, by click or key.
    pub fn choose(&mut self, index: usize) -> Task<Message> {
        let Some(menu) = self.menu.take() else {
            return Task::none();
        };
        let Some(row) = menu.rows.get(index) else {
            return Task::none();
        };
        let item = menu.item.clone().unwrap_or_default();
        let focus = operation::focus(COMPOSER);
        match row.act {
            MenuAct::AddReaction => self.open_picker(Target::React(item, menu.at)),
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
        }
    }

    /// The open menu's keys: the arrows, Tab, Home and End light a row,
    /// Enter or Space chooses it, Escape closes the menu.
    pub fn menu_key(&mut self, named: Option<key::Named>) -> Task<Message> {
        let Some(menu) = &mut self.menu else {
            return Task::none();
        };
        let count = menu.rows.len();
        let lit = menu.lit;
        match named {
            Some(key::Named::ArrowDown | key::Named::Tab) => {
                menu.lit = Some(lit.map_or(0, |lit| (lit + 1) % count));
            }
            Some(key::Named::ArrowUp) => {
                menu.lit = Some(lit.map_or(count - 1, |lit| (lit + count - 1) % count));
            }
            Some(key::Named::Home) => menu.lit = Some(0),
            Some(key::Named::End) => menu.lit = Some(count - 1),
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

    /// The confirmation's keys: Escape cancels, Enter does what the focused
    /// button does, Tab and the side arrows go round the two buttons.
    pub fn confirm_key(&mut self, named: Option<key::Named>) -> Task<Message> {
        match named {
            Some(key::Named::Escape) => self.update(Message::CancelDelete),
            Some(key::Named::Enter) => {
                if self.confirm_focus {
                    self.update(Message::ConfirmDelete)
                } else {
                    self.update(Message::CancelDelete)
                }
            }
            Some(key::Named::Tab | key::Named::ArrowLeft | key::Named::ArrowRight) => {
                self.confirm_focus = !self.confirm_focus;
                Task::none()
            }
            _ => Task::none(),
        }
    }

    pub fn menu_view(&self) -> Option<Element<'_, Message>> {
        let menu = self.menu.as_ref()?;
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
            for key in QUICK {
                let face: Element<'_, Message> = match self.emoji_picture(key, 24) {
                    Some(picture) => container(Pic::new(picture, Size::new(24.0, 24.0)))
                        .center(44)
                        .into(),
                    None => text(key).size(22).into(),
                };
                quick = quick.push(
                    button(face)
                        .padding(0)
                        .width(44)
                        .height(44)
                        .style(|_, status| button::Style {
                            background: Some(
                                if matches!(
                                    status,
                                    button::Status::Hovered | button::Status::Pressed
                                ) {
                                    blend(0x2a2b32, white(0.12))
                                } else {
                                    hex(FIELD)
                                }
                                .into(),
                            ),
                            border: Border::default().rounded(6),
                            ..Default::default()
                        })
                        .on_press(Message::React(item.clone(), key)),
                );
            }
            rows = rows.push(quick);
        }
        for (index, entry) in menu.rows.iter().enumerate() {
            if entry.separated && index > 0 {
                rows = rows.push(
                    container(
                        container(Space::new())
                            .width(Length::Fill)
                            .height(1)
                            .style(|_| fill(LINE)),
                    )
                    .padding([4, 0]),
                );
            }
            let lit = menu.lit == Some(index);
            let (ink, icon_ink) = match (entry.danger, lit) {
                (true, true) => (Color::WHITE, 0xffffff),
                (true, false) => (hex(DANGER_TEXT), DANGER_TEXT),
                (false, true) => (hex(TEXT), 0xeef0f4),
                (false, false) => (hex(TEXT), SECONDARY_INK),
            };
            let danger = entry.danger;
            rows = rows.push(
                mouse_area(
                    button(
                        row![
                            Pic::new(self.icon(entry.icon, 16, icon_ink), Size::new(16.0, 16.0)),
                            text(entry.label).size(14).font(MEDIUM).color(ink),
                        ]
                        .spacing(8)
                        .align_y(Alignment::Center),
                    )
                    .padding([0, 8])
                    .width(Length::Fill)
                    .height(32)
                    .on_press(Message::MenuChoose(index))
                    .style(move |_, _| button::Style {
                        background: lit
                            .then(|| if danger { hex(DANGER) } else { white(0.09) }.into()),
                        border: Border::default().rounded(4),
                        ..Default::default()
                    }),
                )
                .on_enter(Message::MenuLight(index)),
            );
        }
        let width = if menu.item.is_some() {
            WIDTH
        } else {
            FIELD_WIDTH
        };
        let panel = container(rows).width(width).style(|_| container::Style {
            background: Some(hex(MENU).into()),
            border: Border {
                color: hex(MENU_LINE),
                width: 1.0,
                radius: 8.0.into(),
            },
            shadow: raised_shadow(),
            ..Default::default()
        });
        Some(
            Place::new(opaque(panel), menu.at, 8.0)
                .anchor(Anchor::Pointer)
                .into(),
        )
    }

    /// Delete's confirmation over the dimmed window: the question, the
    /// message as it shows, Cancel and Delete (focused first). A click on
    /// the dimmed window cancels; nothing under it takes a key.
    pub fn modal_view(&self) -> Option<Element<'_, Message>> {
        let state = self.client.state();
        let item = state.confirm_delete.as_ref()?;
        let message = state.message(item);
        let focus = self.confirm_focus;
        let mut body = column![
            text("Delete Message").size(20).font(BOLD).color(hex(TEXT)),
            text("Are you sure you want to delete this message?")
                .size(15)
                .color(hex(SECONDARY_INK)),
        ]
        .spacing(12)
        .padding(16);
        if let Some(message) = message {
            body = body.push(
                container(
                    column![
                        text(message.author.clone()).font(MEDIUM).color(hex(TEXT)),
                        text(message.body.clone())
                            .color(hex(TEXT))
                            .wrapping(text::Wrapping::WordOrGlyph),
                    ]
                    .spacing(2),
                )
                .padding(12)
                .width(Length::Fill)
                .max_height(160)
                .clip(true)
                .style(|_| container::Style {
                    border: Border {
                        color: hex(LINE),
                        width: 1.0,
                        radius: 6.0.into(),
                    },
                    ..Default::default()
                }),
            );
        }
        body = body.push(
            row![
                Space::new().width(Length::Fill),
                dialog_button("Cancel", false, !focus, Message::CancelDelete),
                dialog_button("Delete", true, focus, Message::ConfirmDelete),
            ]
            .spacing(8),
        );
        let card = opaque(container(body).width(440).style(|_| container::Style {
            background: Some(hex(BACKGROUND).into()),
            border: Border::default().rounded(8),
            shadow: raised_shadow(),
            ..Default::default()
        }));
        let dim = mouse_area(
            container(Space::new())
                .width(Length::Fill)
                .height(Length::Fill)
                .style(|_| container::Style {
                    background: Some(
                        Color {
                            a: 0xaa as f32 / 255.0,
                            ..Color::BLACK
                        }
                        .into(),
                    ),
                    ..Default::default()
                }),
        )
        .on_press(Message::CancelDelete);
        Some(opaque(stack![dim, container(card).center(Length::Fill)]))
    }
}

/// One of the dialog's buttons, ringed in the accent while it's focused.
fn dialog_button<'a>(
    label: &'static str,
    danger: bool,
    focused: bool,
    message: Message,
) -> Element<'a, Message> {
    button(
        container(text(label).size(15).font(BOLD).color(if danger {
            Color::WHITE
        } else {
            hex(TEXT)
        }))
        .height(Length::Fill)
        .align_y(Alignment::Center),
    )
    .padding([0, 16])
    .height(36)
    .on_press(message)
    .style(move |_, status| {
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        button::Style {
            background: Some(
                match (danger, hovered) {
                    (true, false) => hex(DANGER),
                    (true, true) => hex(0xf46f77),
                    (false, false) => Color::TRANSPARENT,
                    (false, true) => white(0.08),
                }
                .into(),
            ),
            border: Border {
                color: hex(ACCENT),
                width: if focused { 2.0 } else { 0.0 },
                radius: 6.0.into(),
            },
            ..Default::default()
        }
    })
    .into()
}
