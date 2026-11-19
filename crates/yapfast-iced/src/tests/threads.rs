//! The thread beside a channel.

use std::sync::Arc;

use iced::{
    keyboard::{self, key},
    widget::Id,
};
use yapfast_core::chat::ChatMessage;

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
    assert!(
        app.composer.draft.is_empty(),
        "the channel's draft stays its own"
    );
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
fn the_thread_besides_edge_drags_it_wider_within_its_room_and_keeps_it() {
    use crate::{app::Message, thread::Event};
    let (mut app, folder) = thread_beside();
    app.shell.frame = iced::Size::new(1600.0, 900.0);
    crate::window_bar::fit_window(&mut app);
    assert_eq!(crate::thread::aside_width(&app), crate::thread::WIDTH);
    // Dragged 100 px to the left, the panel grows by as much.
    let _ = app.update(Message::Thread(Event::Resize(-60.0)));
    let _ = app.update(Message::Thread(Event::Resize(-100.0)));
    assert_eq!(crate::thread::aside_width(&app), 520.0);
    let _ = app.update(Message::Thread(Event::Resized));
    assert_eq!(
        crate::layout::Layout::read(folder.path()).aside_width,
        Some(520.0),
        "kept as it's let go"
    );
    // Never past 800 px, nor narrower than 320.
    let _ = app.update(Message::Thread(Event::Resize(-900.0)));
    assert_eq!(crate::thread::aside_width(&app), 800.0);
    let _ = app.update(Message::Thread(Event::Resized));
    let _ = app.update(Message::Thread(Event::Resize(900.0)));
    assert_eq!(crate::thread::aside_width(&app), 320.0);
    let _ = app.update(Message::Thread(Event::Resized));
    // A double click puts it back.
    let _ = app.update(Message::Thread(Event::ResetWidth));
    assert_eq!(crate::thread::aside_width(&app), crate::thread::WIDTH);
}

#[test]
fn escape_from_anywhere_cancels_the_channels_reply_then_the_threads() {
    use crate::app::Message;
    use yapfast_core::application::Action;
    let (mut app, _folder) = thread_beside();
    let channels = app
        .timeline
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
    assert!(app.composer.draft.is_empty());
    // The channel's once it's written in.
    interact(&mut app, |ui| {
        ui.click(Id::new(crate::composer::COMPOSER))
            .expect("the channel's composer");
        ui.typewrite("b");
    });
    letter(&mut app, "c");
    assert_eq!(app.composer.draft.text(), "bc");
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
    let _ = crate::uploads::update(&mut app, Event::Hovered);
    let _ = crate::uploads::update(
        &mut app,
        Event::Moved(iced::Point::new(width - 50.0, 300.0)),
    );
    assert!(app.uploads.drop.aside);
    let _ = crate::uploads::update(&mut app, Event::Dropped(file.clone()));
    let _ = crate::uploads::update(&mut app, Event::Drop);
    assert_eq!(
        app.client.state().aside_staged().len(),
        1,
        "staged in the thread"
    );
    assert!(app.client.state().staged().is_empty());
    // Over the channel they go to the channel.
    let _ = crate::uploads::update(&mut app, Event::Moved(iced::Point::new(500.0, 300.0)));
    let _ = crate::uploads::update(&mut app, Event::Dropped(file));
    let _ = crate::uploads::update(&mut app, Event::Drop);
    assert_eq!(app.client.state().staged().len(), 1);
}

#[test]
fn files_dropped_where_search_hides_the_thread_go_to_the_channel() {
    use crate::uploads::Event;
    let (mut app, folder) = thread_beside();
    change(&mut app, |state| {
        state.preview = false;
        state.trusted = true;
        for room in &mut state.rooms {
            Arc::make_mut(room).encrypted = true;
        }
        // Search's results take the thread's place.
        for room in &mut state.rooms {
            Arc::make_mut(room).capabilities.search = true;
        }
        let place = state.search_place().expect("a place to search");
        state.search = Some(yapfast_core::application::Search {
            place,
            query: "seeds".into(),
            ..Default::default()
        });
    });
    assert!(crate::search::search_shown(&app) && !crate::thread::aside_shown(&app));
    let file = folder.path().join("notes.txt");
    std::fs::write(&file, "seeds").expect("a file");
    let width = app.shell.frame.width;
    let _ = crate::uploads::update(&mut app, Event::Hovered);
    let _ = crate::uploads::update(
        &mut app,
        Event::Moved(iced::Point::new(width - 50.0, 300.0)),
    );
    assert!(!app.uploads.drop.aside, "not over a thread that shows");
    let _ = crate::uploads::update(&mut app, Event::Dropped(file));
    let _ = crate::uploads::update(&mut app, Event::Drop);
    assert!(app.client.state().aside_staged().is_empty());
    assert_eq!(
        app.client.state().staged().len(),
        1,
        "staged in the channel"
    );
}

#[test]
fn the_thread_besides_composer_suggests_over_itself() {
    let (mut app, _folder) = thread_beside();
    interact(&mut app, |ui| {
        ui.click(Id::new(COMPOSER)).expect("the thread's composer");
        ui.typewrite("hi :smi");
    });
    assert!(
        crate::suggest::suggesting(app.ctx(), crate::composer::Writing::Aside),
        "emoji suggested for the thread's draft"
    );
    assert!(shows(&app, "EMOJI MATCHING :smi"));
    assert!(!crate::suggest::suggesting(
        app.ctx(),
        crate::composer::Writing::Draft
    ));
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
    let _ = crate::uploads::update(&mut app, Event::Hovered);
    let _ = crate::uploads::update(&mut app, Event::Pointer(Following(Some(pointer.clone()))));
    assert!(!app.uploads.drop.aside, "over the channel");
    assert!(
        crate::uploads::follow_subscription(&app).is_some(),
        "asked while they hover"
    );
    // Moved over the thread and let go there, without a look in between:
    // where it was let go counts.
    *pointer.0.lock().expect("the point") = (width - 50.0, 300.0);
    let _ = crate::uploads::update(&mut app, Event::Dropped(file));
    let _ = crate::uploads::update(&mut app, Event::Drop);
    assert_eq!(
        app.client.state().aside_staged().len(),
        1,
        "staged in the thread"
    );
    assert!(
        crate::uploads::follow_subscription(&app).is_none(),
        "and no longer asked"
    );
}

#[test]
fn a_report_the_throttle_holds_back_follows_after_it() {
    use crate::{app::Message, thread::Event};
    let (mut app, _folder) = thread_beside();
    let _ = app.update(Message::Thread(Event::Report));
    // A second within the throttle: held back, and a timer for it.
    let held = app.update(Message::Thread(Event::Report));
    assert!(held.units() > 0, "a timer for the held report");
}

#[test]
fn a_pictures_menu_in_the_thread_beside_opens_with_its_messages_rows() {
    use crate::{app::Message, image_viewer::Event};
    let (mut app, _folder) = thread_beside();
    change(&mut app, |state| {
        let aside = state.aside.as_mut().expect("the thread beside");
        let messages = Arc::make_mut(&mut aside.messages);
        let message = Arc::make_mut(&mut messages[0]);
        message.attachments = vec![yapfast_core::chat::Attachment {
            name: "map.png".into(),
            size: None,
            image: true,
            dimensions: Some((400, 300)),
            preview: None,
            preview_failed: false,
            placement: yapfast_core::chat::Placement::Message,
            spoiler: false,
            video: false,
            gif: None,
            url: Some("https://example.com/map.png".into()),
            description: None,
        }];
    });
    let _ = app.update(Message::Image(Event::Menu("t0".into(), 0, false)));
    let menu = app.win.menu.as_ref().expect("a menu");
    assert_eq!(menu.item.as_deref(), Some("t0"));
    assert!(
        menu.rows.iter().any(|row| row.label == "Copy Link"),
        "with the picture's rows"
    );
}

#[test]
fn a_sent_threads_picture_lets_go_of_its_preview_beside_a_channels_file() {
    use crate::uploads::Event;
    let (mut app, folder) = thread_beside();
    change(&mut app, |state| {
        state.preview = false;
        state.trusted = true;
        for room in &mut state.rooms {
            Arc::make_mut(room).encrypted = true;
        }
    });
    let (notes, picture) = (folder.path().join("notes.txt"), folder.path().join("a.png"));
    std::fs::write(&notes, "seeds").expect("a file");
    std::fs::write(&picture, "not really").expect("a picture");
    let _ = crate::uploads::update(&mut app, Event::Chosen(vec![notes]));
    let _ = crate::uploads::update(&mut app, Event::ChosenAside(vec![picture.clone()]));
    assert!(
        app.uploads.thumbnails.contains_key(&picture),
        "its preview asked for"
    );
    // One staged in each place, one preview: as many as the channel has.
    let _ = crate::uploads::update(&mut app, Event::UnstageAside(0));
    assert!(app.client.state().aside_staged().is_empty());
    assert!(
        !app.uploads.thumbnails.contains_key(&picture),
        "the thread's picture's preview let go"
    );
}

/// The window on a Discord server with a forum, #ideas, holding a post,
/// "Dark mode", which this account follows, open in place with its first
/// message loaded.
fn forum_post_open() -> (crate::app::App, tempfile::TempDir) {
    use yapfast_core::{application::Action, yapfast_discord::ready};
    let (mut app, folder) = super::server::discord_server(
        ready::permission::READ_MESSAGE_HISTORY | ready::permission::ADD_REACTIONS,
        |server, _| {
            let forum = ready::server(&serde_json::json!({
                "id": "10", "owner_id": "99", "properties": {"name": "Test"},
                "roles": [{"id": "10", "name": "@everyone", "position": 0, "permissions": "0"}],
                "channels": [{"id": "15", "type": 15, "name": "ideas", "position": 2}],
                "threads": [{"id": "45", "type": 11, "name": "Dark mode", "parent_id": "15",
                             "guild_id": "10", "last_message_id": "45", "message_count": 1,
                             "thread_metadata": {"archived": false, "locked": false}}],
            }))
            .expect("the forum");
            server.text_channels.extend(forum.text_channels);
            server.threads.extend(forum.threads);
        },
    );
    app.client.dispatch(Action::Navigate(
        yapfast_core::application::Navigation::Server("discord:10".into()),
    ));
    app.client
        .dispatch(Action::SelectRoom("discord:10:45".into()));
    change(&mut app, |state| {
        state.messages = Arc::new(vec![Arc::new(ChatMessage {
            item: "d:45".into(),
            author: "Sam".into(),
            body: "A dark theme, please".into(),
            timestamp: 1,
            can_react: true,
            ..Default::default()
        })]);
    });
    (app, folder)
}

#[test]
fn a_forum_post_opened_in_place_has_its_bar_under_the_header() {
    use yapfast_core::chat::{Command, DiscordCommand};
    let (mut app, _folder) = forum_post_open();
    assert!(shows(&app, "A dark theme, please"));
    for words in ["React to post", "Following"] {
        assert!(shows(&app, words), "{words}");
    }
    // React to post opens the picker for the post's first message.
    interact(&mut app, |ui| {
        ui.click("React to post").expect("React to post");
    });
    let spot = app.forum_post.react_spot.get();
    assert!(spot.height > 0.0, "the button's place is noted");
    assert!(
        matches!(
            app.win.picker.get().map(|picker| &picker.target),
            Some(crate::picker::Target::React(item, at))
                if item == "d:45" && at.x == spot.x && at.y == spot.y + spot.height + 4.0
        ),
        "the picker opens under the button"
    );
    let _ = crate::picker::close_picker(&mut app);
    // Following leaves the post.
    app.client.sent.clear();
    interact(&mut app, |ui| {
        ui.click("Following").expect("Following");
    });
    assert!(
        app.client.sent.iter().any(|command| matches!(
            command,
            Command::Discord(DiscordCommand::JoinThread { room, join: false, .. })
                if room == "discord:10:45"
        )),
        "Following leaves the post"
    );
}

#[test]
fn a_forum_posts_bar_waits_for_its_first_message_to_react_and_a_channel_has_none() {
    let (mut app, _folder) = forum_post_open();
    change(&mut app, |state| {
        state.messages = Arc::new(Vec::new());
    });
    assert!(shows(&app, "Following"));
    assert!(!shows(&app, "React to post"));
    app.client
        .dispatch(yapfast_core::application::Action::SelectRoom(
            "discord:10:11".into(),
        ));
    let _ = app.sync();
    assert!(!shows(&app, "Following"));
}

#[test]
fn a_forums_row_counts_its_posts_mentions_and_the_server_counts_them_once() {
    use crate::sidebar::SideRow;
    let (mut app, _folder) = forum_post_open();
    super::unread(&mut app, "discord:10:45", 2);
    let _ = app.sync();
    assert!(
        app.sidebar.rows.iter().any(|row| matches!(
            row,
            SideRow::Room(room) if room.id == "discord:10:15" && room.badge == "2" && room.posts == "1"
        )),
        "the forum's row counts the post's mentions and the unread post"
    );
    let server = app
        .rail
        .items
        .iter()
        .find(|item| item.id == "discord:10")
        .expect("the server's tile");
    // The fixture's #general has a mention of its own.
    assert_eq!(server.badge, "3", "the post's mentions count once");
}
