//! The composer on the core's preview and on a Discord channel: drafts
//! that stay with their rooms, keystrokes and typing that reach the
//! composer alone, mentions and emoji that complete, files that stage
//! with their previews.

use std::sync::Arc;

use iced::widget::image;
use yapfast_core::application::Action;
use yapfast_core::chat::{ChatRoom, ChatServer};

use anomi_vm::chat::Intent as Chat;
use anomi_vm::chat::composer::{Intent, Suggest};
use anomi_vm::chat::suggestions::{Offer, Suggestions};

use crate::app::App;
use crate::intent::chat::composer::Done;
use crate::message::Message;
use crate::platform::Heard;
use crate::project::Reach;
use crate::tests::{Folder, start};

fn ask(intent: Intent) -> Message {
    Message::Intent(Chat::Composer(intent).into())
}

fn open(app: &mut App, room: &str) {
    drop(app.dispatch(Action::Open(room.into())));
}

/// Types `text` into the composer as the field reports it, the caret at
/// its end.
fn write(app: &mut App, text: &str) {
    let room = app.vm.chat.composer.room;
    let edited = Intent::Edited {
        room,
        text: text.into(),
        caret: text.len(),
    };
    drop(app.update(ask(edited)));
}

fn offer(app: &App) -> &Offer {
    &app.vm.chat.composer.suggestions.offer
}

#[test]
fn each_room_keeps_its_draft_and_a_late_keystroke_lands_nowhere() {
    let mut window = start("composer-drafts", true);
    let app = &mut window.app;
    open(app, "general");
    write(app, "hello");
    let general = app.vm.chat.composer.room;
    assert_eq!(app.vm.chat.composer.draft, "hello");
    assert_eq!(app.vm.chat.composer.placeholder, "Message #general");
    open(app, "games");
    assert_eq!(app.vm.chat.composer.draft, "");
    assert_ne!(app.vm.chat.composer.room, general);
    // A keystroke the general's field sent before the switch.
    let late = Intent::Edited {
        room: general,
        text: "hello!".into(),
        caret: 6,
    };
    drop(app.update(ask(late)));
    assert_eq!(app.core.state.draft(), "");
    open(app, "general");
    assert_eq!(app.vm.chat.composer.draft, "hello");
    // Sending clears it.
    drop(app.update(ask(Intent::Send)));
    assert_eq!(app.vm.chat.composer.draft, "");
    assert_eq!(app.core.state.draft(), "");
}

#[test]
fn who_is_typing_reaches_the_line_alone() {
    let mut window = start("composer-typing", true);
    let app = &mut window.app;
    open(app, "general");
    app.core
        .state
        .typing
        .insert("general".into(), vec!["Sam".into()]);
    // What only a keystroke would show stays as it was.
    let placeholder = std::mem::take(&mut app.vm.chat.composer.placeholder);
    app.show(Reach::TYPING);
    assert_eq!(app.vm.chat.composer.typing, "Sam is typing...");
    assert!(app.vm.chat.composer.placeholder.is_empty());
    app.vm.chat.composer.placeholder = placeholder;
    app.core.state.typing.clear();
    app.show(Reach::TYPING);
    assert!(app.vm.chat.composer.typing.is_empty());
}

#[test]
fn a_mention_completes_and_escape_closes_its_word_alone() {
    let mut window = start("composer-mention", true);
    let app = &mut window.app;
    open(app, "general");
    write(app, "hi @Ri");
    let Offer::Mentions { rows, index: 0 } = offer(app) else {
        panic!("{:?}", offer(app));
    };
    assert_eq!(rows[0].name, "Riley");
    assert_eq!(rows[0].heading, "MEMBERS");
    // Tab or Enter: the name goes in where the word was, with a space.
    drop(app.update(ask(Intent::Suggest(Suggest::Choose))));
    assert_eq!(app.core.state.draft(), "hi @Riley ");
    assert_eq!(app.vm.chat.composer.draft, "hi @Riley ");
    assert!(!app.vm.chat.composer.suggestions.showing());
    assert_eq!(app.core.state.mention_query, None);
    // Escape closes the word's list; typing on in it keeps it closed, and
    // a new word opens one again.
    write(app, "hi @Riley @S");
    assert!(app.vm.chat.composer.suggestions.showing());
    drop(app.update(ask(Intent::Suggest(Suggest::Dismiss))));
    assert!(!app.vm.chat.composer.suggestions.showing());
    write(app, "hi @Riley @Sa");
    assert!(!app.vm.chat.composer.suggestions.showing());
    write(app, "hi @Riley @Sa @");
    assert!(app.vm.chat.composer.suggestions.showing());
}

#[test]
fn an_emoji_name_offers_emoji_moves_round_and_puts_the_emoji_in() {
    let mut window = start("composer-emoji", true);
    let app = &mut window.app;
    open(app, "general");
    write(app, "so :smil");
    let Offer::Emoji { query, cells, .. } = offer(app) else {
        panic!("{:?}", offer(app));
    };
    assert_eq!(query, "smil");
    assert!(cells[0].name.starts_with(":smil"), "{:?}", cells[0].name);
    let count = cells.len();
    // Up from the first goes round to the last.
    drop(app.update(ask(Intent::Suggest(Suggest::Move(-1)))));
    let Offer::Emoji { index, cells, .. } = offer(app) else {
        panic!("{:?}", offer(app));
    };
    assert_eq!(*index, count - 1);
    let chosen = cells[count - 1].emoji.key().to_owned();
    drop(app.update(ask(Intent::Suggest(Suggest::Choose))));
    assert_eq!(app.core.state.draft(), format!("so {chosen} "));
    assert_eq!(app.vm.chat.composer.suggestions, Suggestions::default());
}

/// A Discord channel that takes files, open.
fn channel(folder: &Folder) -> App {
    std::fs::create_dir_all(folder.path()).expect("the data folder");
    let mut app = App::for_tests(folder.path());
    drop(app.dispatch(Action::Preview));
    let state = &mut app.core.state;
    let template = (*state.rooms[0]).clone();
    state.preview = false;
    state.discord_online = true;
    state.discord_user = Some("discord:u1".into());
    state.servers = vec![Arc::new(ChatServer {
        id: "discord:g1".into(),
        name: "Server".into(),
        channels: vec!["discord:g1:c1".into()],
        discord: true,
        ..ChatServer::default()
    })];
    state.rooms = vec![Arc::new(ChatRoom {
        id: "discord:g1:c1".into(),
        name: "general".into(),
        upload_limit: 10 << 20,
        network: yapfast_core::network::Network::Discord,
        ..template
    })];
    open(&mut app, "discord:g1:c1");
    app.core.state.room_ready = true;
    app.motion.front = true;
    app.refresh();
    app
}

#[test]
fn a_staged_picture_shows_its_preview_once_read() {
    let folder = Folder::new("composer-staged");
    let mut app = channel(&folder);
    let state = &app.core.state;
    assert!(
        app.vm.chat.composer.enabled && app.vm.chat.composer.more.attach,
        "{:?} {} {} {}",
        state.selected,
        state.can_write(),
        state.room_ready,
        state.busy
    );
    let path = folder.path().join("dot.png");
    ::image::RgbaImage::new(2, 2)
        .save(&path)
        .expect("a picture");
    drop(app.update(Message::Control(format!("stage {}", path.display()))));
    let staged = &app.vm.chat.composer.staged;
    assert_eq!(staged.len(), 1);
    assert_eq!(staged[0].name, "dot.png");
    assert!(staged[0].picture.is_none(), "read off the window's thread");
    // The read was started as the update ended; its picture comes.
    assert_eq!(app.shown.composer.previews.get(&path), Some(&None));
    let picture = image::Handle::from_rgba(1, 1, vec![0; 4]);
    drop(app.update(Message::Composer(Done::Preview(
        path.clone(),
        Some(picture),
    ))));
    assert!(app.vm.chat.composer.staged[0].picture.is_some());
    // Taken out, it lets its preview go.
    drop(app.update(ask(Intent::Unstage(0))));
    assert!(app.vm.chat.composer.staged.is_empty());
    assert!(app.shown.composer.previews.is_empty());
}

#[test]
fn files_dropped_together_stage_together_and_a_refusal_says_why() {
    let folder = Folder::new("composer-dropped");
    let mut app = channel(&folder);
    let path = folder.path().join("notes.txt");
    std::fs::write(&path, "notes").expect("a file");
    drop(app.update(Message::Composer(Done::Hovered(true))));
    assert!(matches!(
        &app.vm.shell.drop,
        Some(anomi_vm::shell::Files::Over { destination, .. }) if destination == "#general"
    ));
    drop(app.update(Message::Composer(Done::Dropped(path.clone()))));
    drop(app.update(Message::Composer(Done::Dropped(path.clone()))));
    drop(app.update(Message::Composer(Done::DroppedAll)));
    assert_eq!(app.vm.shell.drop, None);
    assert_eq!(app.vm.chat.composer.staged.len(), 2);
    // Eleven files are one too many: none go, and the card says why.
    let many = vec![path; 11];
    for file in many {
        drop(app.update(Message::Composer(Done::Dropped(file))));
    }
    drop(app.update(Message::Composer(Done::DroppedAll)));
    assert!(matches!(
        app.vm.shell.drop,
        Some(anomi_vm::shell::Files::Refused { .. })
    ));
    assert_eq!(app.vm.chat.composer.staged.len(), 2);
}

/// On Wayland the app's own data device tells of a drag
/// (`platform::drop`), and it goes as winit's own drops do.
#[test]
fn files_dragged_over_the_window_on_wayland_stage_as_winits_do() {
    use anomi_desktop::drop::Drag;

    let folder = Folder::new("composer-wayland");
    let mut app = channel(&folder);
    let path = folder.path().join("notes.txt");
    std::fs::write(&path, "notes").expect("a file");
    let drag = |app: &mut App, drag| drop(app.update(Message::Desktop(Heard::Drag(drag))));
    drag(&mut app, Drag::Moved { x: 400.0, y: 300.0 });
    drag(&mut app, Drag::Entered);
    assert!(matches!(
        app.vm.shell.drop,
        Some(anomi_vm::shell::Files::Over { .. })
    ));
    drag(&mut app, Drag::Left);
    assert_eq!(app.vm.shell.drop, None);
    drag(&mut app, Drag::Entered);
    drag(&mut app, Drag::Dropped(vec![path.clone(), path]));
    // They gather a moment, as winit tells of each on its own.
    drop(app.update(Message::Composer(Done::DroppedAll)));
    assert_eq!(app.vm.shell.drop, None);
    assert_eq!(app.vm.chat.composer.staged.len(), 2);
}

#[test]
fn typing_is_not_said_where_the_person_turned_it_off() {
    let folder = Folder::new("composer-share-typing");
    let mut app = channel(&folder);
    let typing = yapfast_core::chat::Command::Typing {
        room: "discord:g1:c1".into(),
        typing: true,
    };
    assert!(!crate::effects::kept_back(&app, &typing));
    app.layout.edit(|saved| saved.share_typing = Some(false));
    assert!(crate::effects::kept_back(&app, &typing));
}

#[test]
fn the_composer_is_focused_by_its_own_name() {
    assert_eq!(
        iced::widget::Id::new(super::COMPOSER),
        anomi_widgets::focus::COMPOSER
    );
}
