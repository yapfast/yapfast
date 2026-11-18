//! The Friends screen, the Requests screen and Add Friend, from the core's
//! friends, requests and DMs (the friend half of `show_sidebar`,
//! `src/ui/refresh.rs:1059`; `request_rows`, `:5845`; `show_dialogs`,
//! `:3528`). Each screen is made only while it shows, again only when the
//! core's count of people changes (`people_changes`), the lists change
//! (who's in voice, the requests' messages) or what's typed in its search.

use std::collections::HashMap;

use yapfast_core::application::AppState;
use yapfast_core::chat::{ChatRoom, Presence};
use yapfast_core::discord;

use anomi_vm::kind::Spot;
use anomi_vm::people::add_friend::{AddFriend, Note};
use anomi_vm::people::friends::{FriendRow, Friends, Voice};
use anomi_vm::people::requests::{Friend, Message, Request, Requests};
use anomi_vm::shell::Layer;
use anomi_vm::shell::Shell;

use crate::layout::Layout;
use crate::pictures::{self, Requests as Wanted, Watch};
use crate::project::rows;
use crate::project::sidebar::people::voice_places;

use super::lines;

/// Add Friend's field, which takes the keys as it opens
/// (`anomi_people::friends::add::name_id`).
const NAME: &str = "add-friend-name";

/// What the screens were made from.
#[derive(Default)]
pub(crate) struct Shown {
    friends: Option<Key>,
    friends_pictures: Watch<()>,
    requests: Option<Key>,
    requests_pictures: Watch<()>,
}

#[derive(PartialEq)]
struct Key {
    people: u64,
    lists: u64,
    filter: String,
    all: bool,
}

impl Key {
    fn same(&self, state: &AppState, filter: &str, all: bool) -> bool {
        self.people == state.people_changes
            && self.lists == state.list_changes
            && self.filter == filter
            && self.all == all
    }
}

/// The friends list, while the Friends screen shows: those around, or all,
/// narrowed by its search.
pub(super) fn show_friends(
    state: &AppState,
    open: bool,
    friends: &mut Friends,
    shown: &mut Shown,
    wanted: &mut Wanted,
) {
    if !open {
        if shown.friends.take().is_some() {
            friends.rows = Vec::new();
            shown.friends_pictures = Watch::default();
        }
        return;
    }
    let moved = !shown.friends_pictures.moved(state).is_empty();
    if !moved
        && shown
            .friends
            .as_ref()
            .is_some_and(|key| key.same(state, &friends.filter, friends.all))
    {
        return;
    }
    let rooms: HashMap<&str, &ChatRoom> = state
        .rooms
        .iter()
        .map(|room| (room.id.as_str(), &**room))
        .collect();
    let in_voice = voice_places(state, &rooms);
    let query = friends.filter.trim().to_lowercase();
    let all = friends.all;
    let mut watch = std::mem::take(&mut shown.friends_pictures);
    friends.rows = pictures::noting(state, &mut watch, || {
        state
            .friends
            .iter()
            .filter(|friend| friend.name.to_lowercase().contains(&query))
            .filter(|friend| all || !matches!(friend.presence, None | Some(Presence::Offline)))
            .map(|friend| FriendRow {
                id: friend.id.clone(),
                name: friend.name.clone(),
                avatar: rows::avatar(state, &friend.name, &friend.id, friend.avatar.as_deref()),
                presence: rows::presence(Some(friend.presence.unwrap_or(Presence::Offline))),
                status: lines::status(state, &friend.activities, friend.activity.as_deref()),
                voice: in_voice.get(friend.id.as_str()).map(|place| Voice {
                    room: place.room.to_owned(),
                    spot: Spot::new(place.channel, place.server),
                }),
            })
            .collect()
    });
    wanted.want(state, watch.addresses());
    shown.friends_pictures = watch;
    friends.total = state.friends.len();
    shown.friends = Some(Key {
        people: state.people_changes,
        lists: state.list_changes,
        filter: friends.filter.clone(),
        all,
    });
}

/// The requests, while the Requests screen shows.
pub(super) fn show_requests(
    state: &AppState,
    layout: &Layout,
    open: bool,
    requests: &mut Requests,
    shown: &mut Shown,
    wanted: &mut Wanted,
) {
    if !open {
        if shown.requests.take().is_some() {
            *requests = Requests {
                spam_open: requests.spam_open,
                ..Requests::default()
            };
            shown.requests_pictures = Watch::default();
        }
        return;
    }
    let moved = !shown.requests_pictures.moved(state).is_empty();
    if !moved
        && shown
            .requests
            .as_ref()
            .is_some_and(|key| key.same(state, "", false))
    {
        return;
    }
    let mut watch = std::mem::take(&mut shown.requests_pictures);
    let made = pictures::noting(state, &mut watch, || {
        rows_of(state, layout.twelve_hour_clock)
    });
    wanted.want(state, watch.addresses());
    shown.requests_pictures = watch;
    *requests = Requests {
        spam_open: requests.spam_open,
        marks: crate::project::rail::MARKS,
        ..made
    };
    shown.requests = Some(Key {
        people: state.people_changes,
        lists: state.list_changes,
        filter: String::new(),
        all: false,
    });
}

/// Friend requests and message requests, a person's two as one row,
/// sorted into those waiting for an answer, those sent, and spam.
pub(super) fn rows_of(state: &AppState, twelve_hour: bool) -> Requests {
    let waiting = |name: &str| -> Vec<&ChatRoom> {
        state
            .scoped_rooms()
            .filter(|room| room.category == name)
            .collect()
    };
    let (asking, spam) = (waiting(discord::REQUESTS), waiting(discord::SPAM));
    let mut rows = Requests::default();
    let message = |room: &ChatRoom| {
        let (text, time) = room
            .preview
            .as_ref()
            .map_or_else(Default::default, |preview| {
                let seconds = preview.at / 1000;
                let today = yapfast_core::clock::date(yapfast_core::clock::now_ms() / 1000);
                let time = if yapfast_core::clock::date(seconds) == today {
                    yapfast_core::clock::time_on(seconds, twelve_hour)
                } else {
                    yapfast_core::clock::day(seconds)
                };
                (anomi_vm::text::plain(&preview.text), time)
            });
        Message {
            room: room.id.clone(),
            text,
            time,
            unread: room.unread > 0,
        }
    };
    for request in &state.friend_requests {
        let room = asking
            .iter()
            .chain(&spam)
            .find(|room| room.person.as_deref() == Some(request.id.as_str()));
        let row = Request {
            id: request.id.clone(),
            name: request.name.clone(),
            avatar: rows::avatar(state, &request.name, &request.id, request.avatar.as_deref()),
            friend: if request.incoming {
                Friend::Asked
            } else {
                Friend::Sent
            },
            message: room.map(|room| message(room)),
        };
        let spammed = request.spam || room.is_some_and(|room| room.category == discord::SPAM);
        if spammed {
            rows.spam.push(row);
        } else if request.incoming || room.is_some() {
            // A message waits for an answer, whoever asked to be friends.
            rows.incoming.push(row);
        } else {
            rows.sent.push(row);
        }
    }
    let befriending = |room: &&&ChatRoom| {
        !state
            .friend_requests
            .iter()
            .any(|request| room.person.as_deref() == Some(request.id.as_str()))
    };
    for (list, rooms) in [(&mut rows.incoming, &asking), (&mut rows.spam, &spam)] {
        for room in rooms.iter().filter(befriending) {
            list.push(Request {
                id: room.person.clone().unwrap_or_else(|| room.id.clone()),
                name: room.name.clone(),
                avatar: rows::avatar(state, &room.name, &room.id, room.avatar.as_deref()),
                friend: Friend::No,
                message: Some(message(room)),
            });
        }
    }
    rows
}

/// Add Friend, open while the core has it: its field empties as it opens
/// and as a request goes.
pub(super) fn show_add_friend(
    state: &AppState,
    shell: &mut Shell,
    add_friend: &mut AddFriend,
    focus: &mut Option<&'static str>,
) {
    let Some(dialog) = &state.add_friend else {
        shell.close(Layer::AddFriend);
        return;
    };
    if !shell.is_open(Layer::AddFriend) {
        shell.open(Layer::AddFriend);
        add_friend.name.clear();
        *focus = Some(NAME);
    }
    let note = match (&dialog.outcome, dialog.sending) {
        (_, true) => Note::Sending,
        (Some(Ok(name)), _) => Note::Sent(name.clone()),
        (Some(Err(error)), _) => Note::Failed(error.clone()),
        (None, false) => Note::None,
    };
    if matches!(note, Note::Sent(_)) && !matches!(add_friend.note, Note::Sent(_)) {
        add_friend.name.clear();
    }
    add_friend.note = note;
}
