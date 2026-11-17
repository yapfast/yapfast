//! The channel list as the virtual list's items. Slint's `RoomList` builds
//! one list row a room, stacking in it Browse Channels, the line under the
//! pinned channels, its heading, the channel and who is in it
//! (`ui/rail.slint:1342`-`:1608`); here each of those is an item of its own,
//! so the list builds only those near the view, and a heading, a channel and
//! someone in a voice channel each drag by themselves (F07's `reorder`).
//! Each item keeps the room Slint's layout gave it: the rows' 2 px apart,
//! 12 px under the last, people 30 px tall with 4 px under the last, and a
//! thread's 2 px above it, where its line starts.

use std::hash::{DefaultHasher, Hash, Hasher};

use anomi_vm::sidebar::channels::{ChannelRow, Channels};

/// What an item of the list shows, by row index.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Item {
    /// Browse Channels, at the top of a Community server's list.
    Browse,
    /// The line under the pinned channels, above row `r`.
    Divider(usize),
    /// The pinned channels' heading, above row `r`.
    Pinned(usize),
    /// A category's heading, above row `r`.
    Heading(usize),
    /// Row `r`'s channel, or thread.
    Channel(usize),
    /// Someone under row `r`'s voice channel.
    Person(usize, usize),
}

/// The heights Slint's layout gives them (`ui/rail.slint:1362`, `:1367`,
/// `:854`, `:1494`; a channel's is `ClientStyle.channel-row-height`).
pub const DIVIDER: f32 = 9.0;
pub const PINNED: f32 = 36.0;
pub const HEADING: f32 = 38.0;
pub const PERSON: f32 = 30.0;
/// Between two rows, under the last, and under a channel's people
/// (`:1343`-`:1345`, `:1486`).
pub const GAP: f32 = 2.0;
pub const END: f32 = 12.0;
pub const PEOPLE_END: f32 = 4.0;

/// Room above and under an item's own height.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Room {
    pub above: f32,
    pub under: f32,
}

/// The items `channels` lists, in order.
pub fn items(channels: &Channels) -> Vec<Item> {
    let rows = &channels.rows;
    let mut items = Vec::with_capacity(rows.len() * 2);
    if channels.browse.is_some() && !rows.is_empty() {
        items.push(Item::Browse);
    }
    for (index, row) in rows.iter().enumerate() {
        if index > 0 && !row.pinned && rows[index - 1].pinned {
            items.push(Item::Divider(index));
        }
        if row.pinned_heading() {
            items.push(Item::Pinned(index));
        } else if !row.heading.is_empty() {
            items.push(Item::Heading(index));
        }
        if !row.hidden {
            items.push(Item::Channel(index));
            if row.voice() {
                items.extend((0..row.people.len()).map(|person| Item::Person(index, person)));
            }
        }
    }
    items
}

/// The room around item `at` of `items`, by what follows it.
pub fn room(rows: &[ChannelRow], items: &[Item], at: usize) -> Room {
    let next = items.get(at + 1).copied();
    let end = if next.is_none() { END } else { GAP };
    let thread_next = matches!(next, Some(Item::Channel(r)) if rows[r].thread());
    let room = |above, under| Room { above, under };
    match items[at] {
        Item::Browse | Item::Divider(_) | Item::Pinned(_) => room(0.0, GAP),
        Item::Heading(_) => room(0.0, end),
        Item::Channel(r) => {
            // A thread carries the gap above it, where its line starts.
            let above = if rows[r].thread() { GAP } else { 0.0 };
            room(above, if thread_next { 0.0 } else { end })
        }
        Item::Person(r, p) if p + 1 == rows[r].people.len() => room(0.0, PEOPLE_END + end),
        Item::Person(..) => room(0.0, 0.0),
    }
}

/// An item's identity, which stays with it as others come and go.
pub fn key(rows: &[ChannelRow], item: Item) -> u64 {
    let mut hasher = DefaultHasher::new();
    match item {
        Item::Browse => "browse".hash(&mut hasher),
        Item::Divider(r) => ("divider", &rows[r].id).hash(&mut hasher),
        Item::Pinned(r) | Item::Heading(r) => ("heading", &rows[r].id).hash(&mut hasher),
        Item::Channel(r) => ("channel", &rows[r].id).hash(&mut hasher),
        Item::Person(r, p) => ("person", &rows[r].id, &rows[r].people[p].id).hash(&mut hasher),
    }
    hasher.finish()
}

/// The place an item stands for to a channel or category dragged: its
/// heading's, its channel's, or for someone under a channel, the channel's,
/// as the pointer over the people counts for it (`:1461`).
pub fn place(rows: &[ChannelRow], item: Item) -> Option<u32> {
    let place = match item {
        Item::Heading(r) => rows[r].heading_place,
        Item::Channel(r) | Item::Person(r, _) => rows[r].place,
        _ => 0,
    };
    (place > 0).then_some(place)
}

/// The channel an item stands for to someone dragged: a channel's, or
/// for someone under one, the channel's (`:1468`).
pub fn room_of(rows: &[ChannelRow], item: Item) -> Option<&str> {
    match item {
        Item::Channel(r) | Item::Person(r, _) if !rows[r].thread() => Some(rows[r].id.as_str()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use anomi_vm::fixtures::sidebar::channels;

    use super::*;

    #[test]
    fn the_samples_rows_become_items_in_slints_order() {
        let list = channels::channels();
        assert_eq!(
            items(&list),
            [
                Item::Heading(0),
                Item::Channel(0),
                Item::Channel(1),
                Item::Channel(2),
                Item::Channel(3),
                Item::Person(3, 0),
                Item::Person(3, 1),
                Item::Person(3, 2),
                Item::Channel(4),
            ]
        );
    }

    #[test]
    fn pinned_rows_have_their_heading_and_a_line_after_them() {
        let list = channels::pinned();
        let items = items(&list);
        assert_eq!(
            items[..3],
            [Item::Pinned(0), Item::Channel(0), Item::Channel(1)]
        );
        assert_eq!(items[3..5], [Item::Divider(2), Item::Heading(2)]);
    }

    #[test]
    fn a_folded_category_with_nothing_left_is_its_heading() {
        let list = channels::folded();
        let items = items(&list);
        let last = list.rows.len() - 1;
        assert_eq!(items.last(), Some(&Item::Heading(last)));
        assert_eq!(room(&list.rows, &items, items.len() - 1).under, END);
    }

    #[test]
    fn rows_keep_slints_room() {
        let list = channels::channels();
        let items = items(&list);
        let rows = &list.rows;
        let room = |at| room(rows, &items, at);
        assert_eq!(
            room(0),
            Room {
                above: 0.0,
                under: GAP
            },
            "the heading"
        );
        assert_eq!(room(1).under, 0.0, "#general, its thread next");
        assert_eq!(
            room(2),
            Room {
                above: GAP,
                under: GAP
            },
            "the thread"
        );
        assert_eq!(room(4).under, GAP, "the Lounge, its people next");
        assert_eq!(room(5).under, 0.0);
        assert_eq!(room(7).under, PEOPLE_END + GAP, "the last of them");
        assert_eq!(room(8).under, END, "the last row");
    }

    #[test]
    fn people_count_as_their_channel_to_a_drag() {
        let list = channels::channels();
        let rows = &list.rows;
        assert_eq!(place(rows, Item::Heading(0)), Some(1));
        assert_eq!(
            place(rows, Item::Person(3, 1)),
            place(rows, Item::Channel(3))
        );
        assert_eq!(place(rows, Item::Channel(1)), None, "a thread");
        assert_eq!(room_of(rows, Item::Person(3, 0)), Some(channels::LOUNGE));
        assert_eq!(room_of(rows, Item::Channel(1)), None);
    }

    #[test]
    fn keys_differ_between_items() {
        let list = channels::joined();
        let all = items(&list);
        let mut keys: Vec<u64> = all.iter().map(|item| key(&list.rows, *item)).collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), all.len());
    }
}
