//! The timeline's rows from the core's preview: built once per message,
//! marked by their neighbours, and made again only where something changed.

use std::sync::Arc;

use yapfast_core::application::{Action, Application};
use yapfast_core::chat::{ChatMessage, Reaction};

use anomi_vm::chat::timeline::{Edge, Timeline};

use super::{Shown, show};
use crate::pictures::Requests;
use crate::project::rail::tests::layout;

/// The preview with "general" open, its rows shown.
fn general() -> (Application, Timeline, Shown) {
    let mut core = Application::default();
    drop(core.dispatch(Action::Preview));
    drop(core.dispatch(Action::Open("general".into())));
    let (mut timeline, mut shown) = (Timeline::default(), Shown::default());
    again(&core, &mut timeline, &mut shown);
    (core, timeline, shown)
}

fn again(core: &Application, timeline: &mut Timeline, shown: &mut Shown) {
    show(
        core,
        &layout(),
        true,
        timeline,
        shown,
        &mut Requests::default(),
    );
}

fn message(id: &str, author: &str, body: &str, timestamp: u64) -> Arc<ChatMessage> {
    Arc::new(ChatMessage {
        id: id.into(),
        item: id.into(),
        author: author.into(),
        sender: author.into(),
        body: body.into(),
        timestamp,
        ..ChatMessage::default()
    })
}

/// Whether each row's body is the very one built before.
fn kept(before: &Timeline, after: &Timeline) -> Vec<bool> {
    after
        .rows
        .iter()
        .map(|row| {
            before
                .rows
                .iter()
                .find(|old| old.key == row.key)
                .is_some_and(|old| old.message.body.same(&row.message.body))
        })
        .collect()
}

#[test]
fn the_open_room_shows_its_messages_grouped() {
    let (core, timeline, _) = general();
    let state = &core.state;
    assert_eq!(timeline.rows.len(), state.messages.len());
    assert!(timeline.rows.len() > 2, "the preview's general has a talk");
    assert_eq!(
        timeline.room.as_ref().map(|room| room.name.as_str()),
        Some("general")
    );
    let first = &timeline.rows[0];
    assert!(
        first.first && first.message.first,
        "the first row starts a group"
    );
    assert_eq!(first.message.author.name, state.messages[0].author);
    // Sam's second line follows his first without his name.
    assert_eq!(state.messages[1].author, state.messages[0].author);
    assert!(!timeline.rows[1].first);
    assert_eq!(
        timeline.rows[1].message.edge,
        anomi_vm::chat::message::Edge::Message
    );
    assert!(timeline.locked, "the preview loads no history");
}

#[test]
fn one_message_coming_builds_its_row_alone() {
    let (mut core, mut timeline, mut shown) = general();
    let before = timeline.clone();
    let messages = Arc::make_mut(&mut core.state.messages);
    let last = messages.last().map_or(0, |message| message.timestamp);
    messages.push(message("new", "Dave", "Here at last", last + 60));
    again(&core, &mut timeline, &mut shown);
    let kept = kept(&before, &timeline);
    assert_eq!(kept.len(), before.rows.len() + 1);
    assert!(kept[..before.rows.len()].iter().all(|&kept| kept));
    assert!(!kept[before.rows.len()]);
    let new = timeline.rows.last().expect("its row");
    assert_eq!(new.message.body.read(), "Here at last");
    assert!(new.first, "Dave's starts a group");
    // Nothing changed: nothing is built.
    let still = timeline.clone();
    again(&core, &mut timeline, &mut shown);
    assert!(kept_all(&still, &timeline));
}

fn kept_all(before: &Timeline, after: &Timeline) -> bool {
    kept(before, after).into_iter().all(|kept| kept)
}

#[test]
fn a_reaction_builds_its_message_again_and_no_other() {
    let (mut core, mut timeline, mut shown) = general();
    let before = timeline.clone();
    let messages = Arc::make_mut(&mut core.state.messages);
    let reacted = Arc::make_mut(&mut messages[1]);
    reacted.reactions.push(Reaction {
        key: "👍".into(),
        count: 2,
        mine: true,
    });
    again(&core, &mut timeline, &mut shown);
    let kept = kept(&before, &timeline);
    assert_eq!(kept.iter().filter(|&&kept| !kept).count(), 1);
    assert!(!kept[1]);
    let chips = &timeline.rows[1].message.reactions.chips;
    assert_eq!((chips.len(), chips[0].count, chips[0].mine), (1, 2, true));
}

#[test]
fn a_picture_coming_builds_only_the_rows_that_show_it() {
    let (mut core, mut timeline, mut shown) = general();
    let address = "https://cdn.example/avatars/sam.png";
    let messages = Arc::make_mut(&mut core.state.messages);
    let sams: Vec<usize> = (0..messages.len())
        .filter(|&index| messages[index].author == "Sam")
        .collect();
    for &index in &sams {
        Arc::make_mut(&mut messages[index]).avatar = Some(address.into());
    }
    again(&core, &mut timeline, &mut shown);
    let before = timeline.clone();
    // Another picture comes: no row is built.
    let state = &mut core.state;
    state
        .pictures
        .insert("elsewhere.png".into(), Default::default());
    state.picture_changes += 1;
    again(&core, &mut timeline, &mut shown);
    assert!(kept_all(&before, &timeline));
    crate::pictures::install();
    let picture = yapfast_core::chat::Picture {
        width: 1,
        height: 1,
        rgba: yapfast_core::chat::Pixels::new(1, 1, vec![1, 2, 3, 255]),
        ..Default::default()
    };
    let state = &mut core.state;
    state.pictures.insert(address.into(), Arc::new(picture));
    state.picture_changes += 1;
    again(&core, &mut timeline, &mut shown);
    let kept = kept(&before, &timeline);
    for (index, row) in timeline.rows.iter().enumerate() {
        let sam = sams.contains(&index);
        assert_eq!(kept[index], !sam, "row {index}");
        assert_eq!(row.message.author.avatar.picture.is_some(), sam);
    }
}

#[test]
fn the_new_divider_and_a_new_day_mark_their_rows() {
    let (mut core, mut timeline, mut shown) = general();
    let item = core.state.messages[1].item.clone();
    core.state.first_unread = Some(item);
    again(&core, &mut timeline, &mut shown);
    let row = &timeline.rows[1];
    assert!(row.new && row.first, "the author shows again under it");
    assert_eq!(row.message.edge, anomi_vm::chat::message::Edge::Divider);
    assert_eq!(timeline.unread_index(), Some(1));
    // A message of another day starts one, dated.
    let messages = Arc::make_mut(&mut core.state.messages);
    messages.push(message("later", "Sam", "Next day", 1_790_856_000));
    messages.push(message(
        "later2",
        "Sam",
        "Still",
        1_790_856_000 + 30 * 86_400,
    ));
    again(&core, &mut timeline, &mut shown);
    let rows = &timeline.rows;
    let (last, before) = (&rows[rows.len() - 1], &rows[rows.len() - 2]);
    assert!(last.day.is_some() && last.first);
    assert_ne!(last.day, before.day);
    assert_eq!(timeline.earlier == Edge::Start, core.state.history_start);
}

#[test]
fn another_room_starts_afresh_and_a_closed_one_lets_go() {
    let (mut core, mut timeline, mut shown) = general();
    shown.lit = timeline.rows.first().map(|row| row.key);
    drop(core.dispatch(Action::Open("games".into())));
    again(&core, &mut timeline, &mut shown);
    assert_eq!(
        timeline.room.as_ref().map(|room| room.id.as_str()),
        Some("games")
    );
    assert_eq!(timeline.rows.len(), core.state.messages.len());
    assert!(shown.lit.is_none() && timeline.shown.is_none());
    show(
        &core,
        &layout(),
        false,
        &mut timeline,
        &mut shown,
        &mut Requests::default(),
    );
    assert!(timeline.rows.is_empty() && timeline.room.is_none());
}
