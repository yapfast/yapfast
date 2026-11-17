//! The conversation's header: `show_header` in `src/ui/refresh.rs`, for the
//! open room's name and avatar or topic. Task I04
//! (docs/iced/tasks/integration.md) takes it further.

use yapfast_core::application::{AppState, DmScreen};

use anomi_vm::header::Header;

use super::rows;

#[derive(PartialEq)]
pub(super) struct Key {
    lists: u64,
    pictures: u64,
    selected: Option<String>,
    /// Friends or Requests show in place of the selected room.
    screen: Option<DmScreen>,
    topic: Option<String>,
}

pub(super) fn show(state: &AppState, header: &mut Header, shown: &mut Option<Key>) {
    let key = Key {
        lists: state.list_changes,
        pictures: state.picture_changes,
        selected: state.selected.clone(),
        screen: state.dm_screen,
        topic: state.topic.as_ref().map(|topic| topic.body.clone()),
    };
    if shown.as_ref() == Some(&key) {
        return;
    }
    *header = match state.selected_room().filter(|_| state.dm_screen.is_none()) {
        None => Header::None,
        Some(room) if room.direct => Header::Direct {
            name: room.name.clone(),
            avatar: rows::avatar(state, &room.name, &room.id, room.avatar.as_deref()),
        },
        Some(room) => Header::Channel {
            name: room.name.clone(),
            topic: key.topic.clone().unwrap_or_default(),
        },
    };
    *shown = Some(key);
}
