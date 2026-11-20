//! A world a test publishes into, standing in for the application's.
//!
//! The application's world is the application's: this crate only names the trait
//! it implements, so a test that publishes a room and asks a guest about it
//! holds the values directly, in the wire's own shapes.

use std::sync::Mutex;

use yapfast_plugin_api::call::{AccountView, AppInfo};
use yapfast_plugin_api::event::MessageView;

use crate::bridge::world::MemberAnswer;
use yapfast_plugin_api::event::{PersonView, RoomView};

/// What the test published, which the bridge reads back.
#[derive(Default)]
struct Held {
    me: Option<AccountView>,
    rooms: Vec<RoomView>,
    selected: Option<String>,
    timeline: Option<(String, Vec<MessageView>)>,
}

pub(super) struct StubWorld {
    held: Mutex<Held>,
}

impl StubWorld {
    pub(super) fn new() -> Self {
        Self {
            held: Mutex::new(Held::default()),
        }
    }

    pub(super) fn publish_account(
        &self,
        id: Option<&str>,
        name: Option<&str>,
        username: Option<&str>,
        avatar: Option<&str>,
        status: Option<&str>,
        trusted: bool,
    ) {
        self.held.lock().expect("the world").me = id.map(|id| AccountView {
            id: id.to_owned(),
            name: name.unwrap_or_default().to_owned(),
            username: username.map(str::to_owned),
            avatar: avatar.map(str::to_owned),
            status: status.map(str::to_owned),
            trusted,
        });
    }

    pub(super) fn publish_lists(&self, _changes: u64, rooms: Vec<RoomView>) {
        self.held.lock().expect("the world").rooms = rooms;
    }

    pub(super) fn publish_selected(&self, id: Option<&str>) {
        self.held.lock().expect("the world").selected = id.map(str::to_owned);
    }

    pub(super) fn publish_timeline(&self, room: &str, messages: Vec<MessageView>) {
        self.held.lock().expect("the world").timeline = Some((room.to_owned(), messages));
    }
}

impl super::World for StubWorld {
    fn app(&self) -> AppInfo {
        AppInfo {
            version: "0.0.0".to_owned(),
            locale: "en-GB".to_owned(),
            theme_mode: "dark".to_owned(),
        }
    }

    fn me(&self) -> Option<AccountView> {
        self.held.lock().expect("the world").me.clone()
    }

    fn selected(&self) -> Option<RoomView> {
        let held = self.held.lock().expect("the world");
        let selected = held.selected.as_deref()?;
        held.rooms.iter().find(|room| room.id == selected).cloned()
    }

    fn room(&self, id: &str) -> Option<RoomView> {
        let held = self.held.lock().expect("the world");
        held.rooms.iter().find(|room| room.id == id).cloned()
    }

    fn rooms(&self, limit: Option<usize>) -> Vec<RoomView> {
        let held = self.held.lock().expect("the world");
        held.rooms
            .iter()
            .take(limit.unwrap_or(usize::MAX))
            .cloned()
            .collect()
    }

    fn message(&self, room: &str, id: &str) -> Option<MessageView> {
        let held = self.held.lock().expect("the world");
        let (timeline_room, messages) = held.timeline.as_ref()?;
        if timeline_room != room {
            return None;
        }
        messages.iter().find(|message| message.id == id).cloned()
    }

    fn history(
        &self,
        room: &str,
        before: Option<&str>,
        limit: Option<usize>,
    ) -> Option<Vec<MessageView>> {
        let held = self.held.lock().expect("the world");
        let (timeline_room, messages) = held.timeline.as_ref()?;
        if timeline_room != room {
            return None;
        }
        let end = match before {
            Some(before) => messages
                .iter()
                .position(|message| message.id == before)
                .unwrap_or(messages.len()),
            None => messages.len(),
        };
        let most = limit.unwrap_or(usize::MAX);
        Some(
            messages[..end]
                .iter()
                .rev()
                .take(most)
                .rev()
                .cloned()
                .collect(),
        )
    }

    fn person(&self, id: &str) -> Option<PersonView> {
        self.me().filter(|me| me.id == id).map(|me| PersonView {
            id: me.id,
            name: me.name,
            display_name: None,
            own: true,
            bot: false,
        })
    }

    fn member(&self, _server: &str, _id: &str) -> MemberAnswer {
        MemberAnswer::NotOpen
    }
}
