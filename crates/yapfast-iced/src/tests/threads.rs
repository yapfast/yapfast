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
            can_reply: true,
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

#[test]
fn the_thread_besides_edge_drags_it_wider_within_its_room_and_keeps_it() {
    use crate::{app::Message, thread::Event};
    let (mut app, folder) = thread_beside();
    app.shell.frame = iced::Size::new(1600.0, 900.0);
    app.fit_window();
    assert_eq!(app.aside_width(), crate::thread::WIDTH);
    // Dragged 100 px to the left, the panel grows by as much.
    let _ = app.update(Message::Thread(Event::Resize(-60.0)));
    let _ = app.update(Message::Thread(Event::Resize(-100.0)));
    assert_eq!(app.aside_width(), 520.0);
    let _ = app.update(Message::Thread(Event::Resized));
    assert_eq!(
        crate::layout::Layout::read(folder.path()).aside_width,
        Some(520.0),
        "kept as it's let go"
    );
    // Never past 800 px, nor narrower than 320.
    let _ = app.update(Message::Thread(Event::Resize(-900.0)));
    assert_eq!(app.aside_width(), 800.0);
    let _ = app.update(Message::Thread(Event::Resized));
    let _ = app.update(Message::Thread(Event::Resize(900.0)));
    assert_eq!(app.aside_width(), 320.0);
    let _ = app.update(Message::Thread(Event::Resized));
    // A double click puts it back.
    let _ = app.update(Message::Thread(Event::ResetWidth));
    assert_eq!(app.aside_width(), crate::thread::WIDTH);
}

#[test]
fn escape_from_anywhere_cancels_the_channels_reply_then_the_threads() {
    use crate::app::Message;
    use yapfast_core::application::Action;
    let (mut app, _folder) = thread_beside();
    let channels = app
        .messages
        .last()
        .expect("the channel's messages")
        .item
        .clone();
    app.client.dispatch(Action::Reply(channels));
    app.client.dispatch(Action::Reply("t0".into()));
    let _ = app.sync();
    let escape = |app: &mut crate::app::App| {
        let _ = app.update(Message::Key(
            keyboard::Key::Named(key::Named::Escape),
            keyboard::Modifiers::default(),
        ));
    };
    let replies = |app: &crate::app::App| {
        let state = app.client.state();
        (state.compose().is_some(), state.aside_compose().is_some())
    };
    assert_eq!(replies(&app), (true, true));
    escape(&mut app);
    assert_eq!(replies(&app), (false, true), "the channel's first");
    escape(&mut app);
    assert_eq!(replies(&app), (false, false));
}

#[test]
fn typing_with_nothing_focused_goes_to_the_composer_used_last() {
    use crate::app::Message;
    let (mut app, _folder) = thread_beside();
    let letter = |app: &mut crate::app::App, text: &str| {
        let _ = app.update(Message::Key(
            keyboard::Key::Character(text.into()),
            keyboard::Modifiers::default(),
        ));
    };
    // The thread's from when it opens.
    letter(&mut app, "a");
    assert_eq!(app.client.state().aside_draft(), "a");
    assert!(app.draft.is_empty());
    // The channel's once it's written in.
    interact(&mut app, |ui| {
        ui.click(Id::new(crate::composer::COMPOSER))
            .expect("the channel's composer");
        ui.typewrite("b");
    });
    letter(&mut app, "c");
    assert_eq!(app.draft.text(), "bc");
    assert_eq!(app.client.state().aside_draft(), "a");
}

#[test]
fn files_dropped_over_the_thread_go_to_it() {
    use crate::uploads::Event;
    let (mut app, folder) = thread_beside();
    // The preview takes no files: as a trusted Matrix device's instead.
    change(&mut app, |state| {
        state.preview = false;
        state.trusted = true;
        for room in &mut state.rooms {
            Arc::make_mut(room).encrypted = true;
        }
    });
    let file = folder.path().join("notes.txt");
    std::fs::write(&file, "seeds").expect("a file");
    let width = app.shell.frame.width;
    let _ = app.uploads_event(Event::Hovered);
    let _ = app.uploads_event(Event::Moved(iced::Point::new(width - 50.0, 300.0)));
    assert!(app.drop.aside);
    let _ = app.uploads_event(Event::Dropped(file.clone()));
    let _ = app.uploads_event(Event::Drop);
    assert_eq!(
        app.client.state().aside_staged().len(),
        1,
        "staged in the thread"
    );
    assert!(app.client.state().staged().is_empty());
    // Over the channel they go to the channel.
    let _ = app.uploads_event(Event::Moved(iced::Point::new(500.0, 300.0)));
    let _ = app.uploads_event(Event::Dropped(file));
    let _ = app.uploads_event(Event::Drop);
    assert_eq!(app.client.state().staged().len(), 1);
}

#[test]
fn the_thread_besides_composer_suggests_over_itself() {
    let (mut app, _folder) = thread_beside();
    interact(&mut app, |ui| {
        ui.click(Id::new(COMPOSER)).expect("the thread's composer");
        ui.typewrite("hi :smi");
    });
    assert!(
        app.suggesting(crate::composer::Writing::Aside),
        "emoji suggested for the thread's draft"
    );
    assert!(shows(&app, "EMOJI MATCHING :smi"));
    assert!(!app.suggesting(crate::composer::Writing::Draft));
}

/// The platform's pointer as X11's and Windows' are asked, at a point set
/// by the test.
struct Pointer(std::sync::Mutex<(f64, f64)>);

impl crate::desktop::PointerAt for Pointer {
    fn at(&self) -> Option<(f64, f64)> {
        Some(*self.0.lock().expect("the point"))
    }
}

#[test]
fn where_winit_says_nothing_of_a_drag_the_platforms_pointer_says_where() {
    use crate::uploads::{Event, Following};
    let (mut app, folder) = thread_beside();
    change(&mut app, |state| {
        state.preview = false;
        state.trusted = true;
        for room in &mut state.rooms {
            Arc::make_mut(room).encrypted = true;
        }
    });
    let file = folder.path().join("notes.txt");
    std::fs::write(&file, "seeds").expect("a file");
    let width = f64::from(app.shell.frame.width);
    let pointer = Arc::new(Pointer(std::sync::Mutex::new((500.0, 300.0))));
    let _ = app.uploads_event(Event::Hovered);
    let _ = app.uploads_event(Event::Pointer(Following(Some(pointer.clone()))));
    assert!(!app.drop.aside, "over the channel");
    assert!(
        app.follow_subscription().is_some(),
        "asked while they hover"
    );
    // Moved over the thread and let go there, without a look in between:
    // where it was let go counts.
    *pointer.0.lock().expect("the point") = (width - 50.0, 300.0);
    let _ = app.uploads_event(Event::Dropped(file));
    let _ = app.uploads_event(Event::Drop);
    assert_eq!(
        app.client.state().aside_staged().len(),
        1,
        "staged in the thread"
    );
    assert!(app.follow_subscription().is_none(), "and no longer asked");
}
