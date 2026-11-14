//! The channel list (a server's channels under their categories, or the
//! DMs), virtualized, and the account panel at its foot.

use iced::{
    Alignment, Element, Length,
    widget::{column, container, mouse_area, row, scrollable, text},
};
use yapfast_core::application::{AppState, Navigation};

use crate::{
    app::{App, Message, key},
    fonts::{BOLD, REGULAR},
    style::{FIELD, MUTED, PANEL, SELECTED, TEXT, fill, hex, rounded, scrollbar},
    widgets::Rows,
};

/// The list's width, as the port laid it out beside Freya's for
/// screenshots side by side.
pub const WIDTH: f32 = 304.0;
/// A row's height.
pub const ROW: f32 = 34.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SideKind {
    Category,
    Text,
    Voice,
    Dm,
}

/// A row of the list: a category, a channel or a DM.
#[derive(Clone, Debug, PartialEq)]
pub struct SideItem {
    pub id: String,
    pub name: String,
    pub kind: SideKind,
    pub avatar: Option<String>,
    pub selected: bool,
    pub unread: bool,
    pub mentions: u64,
}

/// The list's rows, from the core's state: the open server's rooms with
/// their categories' headings, or the DMs; message requests stay out.
pub fn sidebar(state: &AppState) -> Vec<SideItem> {
    let direct = state.navigation == Navigation::DirectMessages;
    let mut items = Vec::new();
    let mut category: Option<&str> = None;
    for room in state.scoped_rooms() {
        if room.request || room.category == yapfast_core::discord::REQUESTS {
            continue;
        }
        if !direct && category != Some(room.category.as_str()) {
            category = Some(room.category.as_str());
            if !room.category.is_empty() {
                items.push(SideItem {
                    id: format!("category:{}", room.category),
                    name: room.category.to_uppercase(),
                    kind: SideKind::Category,
                    avatar: None,
                    selected: false,
                    unread: false,
                    mentions: 0,
                });
            }
        }
        items.push(SideItem {
            id: room.id.clone(),
            name: room.name.clone(),
            kind: if direct {
                SideKind::Dm
            } else if room.voice {
                SideKind::Voice
            } else {
                SideKind::Text
            },
            avatar: room.avatar.clone(),
            selected: state.selected.as_ref() == Some(&room.id),
            unread: room.unread > 0 && !room.muted,
            mentions: room.mentions,
        });
    }
    items
}

impl App {
    pub fn sidebar_view(&self) -> Element<'_, Message> {
        let (first, last) = self.side_scroll.range(self.side.len(), ROW, 0.0);
        let mut rows = Rows::new(self.side.len() as f32 * ROW);
        for (index, item) in self.side.iter().enumerate().take(last).skip(first) {
            rows.push(key(&item.id), index as f32 * ROW, ROW, self.side_row(item));
        }
        let list = container(
            scrollable(rows)
                .on_scroll(Message::Side)
                .height(Length::Fill)
                .style(scrollbar),
        )
        .padding(8)
        .width(Length::Fill)
        .height(Length::Fill);
        let (name, avatar) = &self.account;
        let account = container(
            row![
                self.avatar(avatar.as_deref(), name, 36.0, 18.0, 1.0),
                text(name.clone()).font(BOLD)
            ]
            .spacing(10)
            .align_y(Alignment::Center),
        )
        .padding(10)
        .width(Length::Fill)
        .height(60)
        .align_y(Alignment::Center)
        .style(|_| fill(FIELD));
        container(column![list, account])
            .width(WIDTH)
            .height(Length::Fill)
            .style(|_| fill(PANEL))
            .into()
    }

    fn side_row<'a>(&'a self, item: &'a SideItem) -> Element<'a, Message> {
        if item.kind == SideKind::Category {
            return container(
                text(item.name.as_str())
                    .size(12)
                    .font(BOLD)
                    .color(hex(MUTED)),
            )
            .height(ROW)
            .padding([0, 8])
            .align_y(Alignment::Center)
            .into();
        }
        let lead: Element<'a, Message> = match item.kind {
            SideKind::Dm => self.avatar(item.avatar.as_deref(), &item.name, 24.0, 12.0, 1.0),
            SideKind::Voice => text("🔊").color(hex(MUTED)).into(),
            _ => text("#").size(20).color(hex(MUTED)).into(),
        };
        let selected = item.selected;
        let label = text(item.name.as_str())
            .color(hex(if selected || item.unread { TEXT } else { MUTED }))
            .font(if item.unread { BOLD } else { REGULAR })
            .wrapping(text::Wrapping::None);
        let ground = hex(if selected { SELECTED } else { PANEL });
        mouse_area(
            container(row![lead, label].spacing(8).align_y(Alignment::Center))
                .width(Length::Fill)
                .height(ROW)
                .padding([0, 8])
                .align_y(Alignment::Center)
                .clip(true)
                .style(move |_| rounded(ground, 4)),
        )
        .on_press(if item.kind == SideKind::Voice {
            Message::OpenVoice(item.id.clone())
        } else {
            Message::SelectRoom(item.id.clone())
        })
        .into()
    }
}
