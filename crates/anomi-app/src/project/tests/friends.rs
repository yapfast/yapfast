//! Friend requests and message requests, a person's two as one row.

use std::sync::Arc;

use yapfast_core::application::Navigation;
use yapfast_core::chat::{ChatRoom, FriendRequest, RequestPreview};
use yapfast_core::discord;

use anomi_vm::people::requests::Friend;

use super::preview;
use crate::project::people::friends::rows_of as requests;

#[test]
fn requests_list_someone_once_with_their_message() {
    let mut state = preview().state;
    let request = |id: &str, name: &str, incoming, spam| FriendRequest {
        id: id.into(),
        name: name.into(),
        avatar: None,
        incoming,
        spam,
    };
    let room = |id: &str, person: &str, category: &str| {
        Arc::new(ChatRoom {
            id: id.into(),
            name: person.into(),
            direct: true,
            unread: 1,
            category: category.into(),
            request: true,
            person: Some(person.into()),
            preview: Some(Box::new(RequestPreview {
                text: format!("Hi from {person}"),
                at: 1_700_000_000_000,
            })),
            ..(*state.rooms[0]).clone()
        })
    };
    // The requests are the DMs' screen's.
    state.navigation = Navigation::DirectMessages;
    state.friend_requests = vec![
        request("erin", "Erin", true, false),
        request("dave", "Dave", false, false),
        request("zed", "Zed", true, true),
    ];
    state.rooms = vec![
        room("dm-erin", "erin", discord::REQUESTS),
        room("dm-gil", "gil", discord::REQUESTS),
        room("dm-frank", "frank", discord::SPAM),
    ];
    let rows = requests(&state, false);
    let listed =
        |rows: &[anomi_vm::people::requests::Request]| -> Vec<(String, Friend, String, String)> {
            rows.iter()
                .map(|row| {
                    let (room, text) = row
                        .message
                        .as_ref()
                        .map_or_else(Default::default, |message| {
                            (message.room.clone(), message.text.read())
                        });
                    (row.id.clone(), row.friend, room, text)
                })
                .collect()
        };
    let row =
        |id: &str, friend, room: &str, text: &str| (id.into(), friend, room.into(), text.into());
    // Erin's friend request and message share a row; Gil only wrote.
    assert_eq!(
        listed(&rows.incoming),
        [
            row("erin", Friend::Asked, "dm-erin", "Hi from erin"),
            row("gil", Friend::No, "dm-gil", "Hi from gil")
        ]
    );
    assert!(rows.incoming.iter().all(|row| {
        row.message
            .as_ref()
            .is_some_and(|message| message.unread && !message.time.is_empty())
    }));
    assert_eq!(listed(&rows.sent), [row("dave", Friend::Sent, "", "")]);
    assert_eq!(
        listed(&rows.spam),
        [
            row("zed", Friend::Asked, "", ""),
            row("frank", Friend::No, "dm-frank", "Hi from frank")
        ]
    );
}
