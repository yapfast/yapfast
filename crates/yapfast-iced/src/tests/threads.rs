//! The thread beside a channel and the picture viewer, as the Slint
//! client's tests of them.

use std::{collections::HashMap, sync::Arc};

use iced::{
    keyboard::{self, key},
    widget::Id,
};
use yapfast_core::{
    application::Viewer,
    chat::{Attachment, ChatMessage, Picture, Pixels, Placement},
};

use super::{change, interact, preview, shows};
use crate::thread::COMPOSER;

/// The preview with a thread of its open channel, "Plans", beside it,
/// holding one message.
fn thread_beside() -> (crate::app::App, tempfile::TempDir) {
    let (mut app, folder) = preview();
    change(&mut app, |state| {
        let channel = state.selected.clone().expect("an open channel");
        let mut thread = (*state.rooms[0]).clone();
        thread.id = "plans".into();
        thread.name = "Plans".into();
        thread.parent = Some(channel);
        state.rooms.push(Arc::new(thread));
        // The core's type isn't named outside it; its fields are.
        state.aside = Some(Default::default());
        let aside = state.aside.as_mut().expect("the thread beside");
        aside.room = "plans".into();
        aside.ready = true;
        aside.history_start = true;
        aside.messages = Arc::new(vec![Arc::new(ChatMessage {
            item: "t0".into(),
            author: "Sam".into(),
            body: "Saturday works for me".into(),
            timestamp: 1,
            ..Default::default()
        })]);
    });
    (app, folder)
}

#[test]
fn a_thread_beside_shows_its_messages_and_writes_in_its_own_room() {
    let (mut app, _folder) = thread_beside();
    assert!(shows(&app, "Saturday works for me"));
    assert!(shows(&app, "Plans"));
    interact(&mut app, |ui| {
        ui.click(Id::new(COMPOSER)).expect("the thread's composer");
        ui.typewrite("see you there");
    });
    let state = app.client.state();
    assert_eq!(state.aside_draft(), "see you there");
    assert!(app.draft.is_empty(), "the channel's draft stays its own");
}

#[test]
fn closing_the_thread_beside_lets_go_of_its_rows() {
    let (mut app, _folder) = thread_beside();
    assert!(app.aside.is_some());
    let _ = app.update(crate::app::Message::Thread(crate::thread::Event::Close));
    assert!(app.client.state().aside.is_none());
    assert!(app.aside.is_none());
    assert!(!shows(&app, "Saturday works for me"));
}

#[test]
fn closing_the_viewer_lets_go_of_its_picture() {
    let (mut app, _folder) = preview();
    let picture = Arc::new(Picture {
        width: 40,
        height: 30,
        rgba: Pixels::new(40, 30, vec![128; 40 * 30 * 4]),
        ..Default::default()
    });
    change(&mut app, |state| {
        state.messages = Arc::new(vec![Arc::new(ChatMessage {
            item: "i0".into(),
            author: "Sam".into(),
            attachments: vec![Attachment {
                name: "a.png".into(),
                size: None,
                image: true,
                dimensions: Some((40, 30)),
                preview: Some(picture),
                preview_failed: false,
                placement: Placement::Message,
                gif: None,
                spoiler: false,
                video: false,
                url: None,
                description: None,
            }],
            ..Default::default()
        })]);
        state.viewer = Some(Viewer {
            item: "i0".into(),
            index: 0,
            pictures: HashMap::new(),
        });
    });
    assert!(app.viewer_size.is_some());
    assert!(shows(&app, "a.png"));
    let _ = app.update(crate::app::Message::Key(
        keyboard::Key::Named(key::Named::Escape),
        keyboard::Modifiers::default(),
    ));
    assert!(app.client.state().viewer.is_none());
    assert!(app.viewer_size.is_none());
    assert!(!shows(&app, "1 / 1"));
}
