//! The server rail down the window's left: the DMs, a line, then each
//! server as its icon, rounder while it's not the open one.

use std::collections::HashMap;

use iced::{
    Alignment, Element, Length,
    widget::{Space, column, container, mouse_area},
};
use yapfast_core::{
    application::{AppState, Navigation},
    chat::ChatRoom,
};

use crate::{
    app::{App, Message},
    style::{LINE, PANEL, fill},
};

pub const WIDTH: f32 = 72.0;

/// One entry in the rail, the DMs first.
#[derive(Clone, Debug, PartialEq)]
pub struct RailItem {
    pub id: String,
    pub name: String,
    pub icon: Option<String>,
    pub selected: bool,
    pub unread: bool,
    pub mentions: u64,
    pub home: bool,
}

/// The rail's entries, from the core's state.
pub fn rail(state: &AppState) -> Vec<RailItem> {
    let rooms: HashMap<&str, &ChatRoom> = state
        .rooms
        .iter()
        .map(|room| (room.id.as_str(), &**room))
        .collect();
    let mut items = vec![RailItem {
        id: String::new(),
        name: "Direct Messages".into(),
        icon: None,
        selected: state.navigation == Navigation::DirectMessages,
        unread: false,
        mentions: state
            .rooms
            .iter()
            .filter(|room| room.direct)
            .map(|room| room.mentions)
            .sum(),
        home: true,
    }];
    items.extend(state.servers.iter().map(|server| {
        let channels = server
            .channels
            .iter()
            .filter_map(|id| rooms.get(id.as_str()));
        let (mut unread, mut mentions) = (false, 0);
        for room in channels {
            unread |= room.unread > 0 && !room.muted;
            mentions += room.mentions;
        }
        RailItem {
            id: server.id.clone(),
            name: server.name.clone(),
            icon: server.icon.clone(),
            selected: state.navigation == Navigation::Server(server.id.clone()),
            unread,
            mentions,
            home: false,
        }
    }));
    items
}

impl App {
    pub fn rail_view(&self) -> Element<'_, Message> {
        let mut items = column![].spacing(8).align_x(Alignment::Center).width(WIDTH);
        for (index, item) in self.rail.iter().enumerate() {
            let navigation = if item.home {
                Navigation::DirectMessages
            } else {
                Navigation::Server(item.id.clone())
            };
            let radius = if item.selected { 14.0 } else { 20.0 };
            let icon = mouse_area(self.avatar(item.icon.as_deref(), &item.name, 40.0, radius, 1.0))
                .on_press(Message::Navigate(navigation));
            if index == 0 {
                // The DMs, then a line, as Discord's rail; the first server
                // lands at y 56 to 96, as in the Slint client.
                items = items.push(
                    column![
                        icon,
                        container(Space::new())
                            .width(32)
                            .height(1)
                            .style(|_| fill(LINE))
                    ]
                    .spacing(7)
                    .align_x(Alignment::Center),
                );
            } else {
                items = items.push(icon);
            }
        }
        container(items)
            .width(WIDTH)
            .height(Length::Fill)
            .style(|_| fill(PANEL))
            .into()
    }
}
