//! The players and the viewer on the preview's #general: a video in its
//! cell and full screen, its frames, a voice message's card, the picture
//! large.

use std::sync::Arc;

use yapfast_core::application::Action;
use yapfast_core::chat::{self, Attachment, ChatMessage, VoiceMessage, VoicePlayback};
use yapfast_core::frames::{Frames, Writer};

use anomi_vm::chat::Intent as Chat;
use anomi_vm::chat::timeline::{Timeline, group};
use anomi_vm::chat::video::{Intent as VideoIntent, Player};
use anomi_vm::chat::viewer::{Intent as ViewerIntent, Note, Zoom};
use anomi_vm::chat::voice_message::VoiceMessage as Card;
use anomi_vm::shell::{self, Layer};

use super::frames::Found;
use crate::app::App;
use crate::message::Message;
use crate::project::Reach;
use crate::tests::Folder;

/// A file named `name`, sent and shown with the message's.
fn attachment(name: &str) -> Attachment {
    Attachment {
        name: name.into(),
        size: None,
        image: false,
        dimensions: None,
        preview: None,
        preview_failed: false,
        preview_percent: 0,
        placement: chat::Placement::Message,
        spoiler: false,
        video: false,
        gif: None,
        url: None,
        description: None,
    }
}

fn message(id: &str, timestamp: u64, attachments: Vec<Attachment>) -> Arc<ChatMessage> {
    Arc::new(ChatMessage {
        id: id.into(),
        item: id.into(),
        author: "Bob".into(),
        sender: "Bob".into(),
        timestamp,
        attachments,
        ..ChatMessage::default()
    })
}

/// The preview's #general in front, with Bob's clip, voice message and two
/// photos after its talk.
fn general(folder: &Folder) -> App {
    let mut app = App::for_tests(folder.path());
    drop(app.dispatch(Action::Preview));
    drop(app.dispatch(Action::Open("general".into())));
    let state = &mut app.core.state;
    state.preview = false;
    let messages = Arc::make_mut(&mut state.messages);
    let last = messages.last().map_or(0, |message| message.timestamp);
    let clip = Attachment {
        video: true,
        dimensions: Some((480, 270)),
        url: Some("https://cdn.example/clip.mp4".into()),
        ..attachment("clip.mp4")
    };
    messages.push(message("clip", last + 60, vec![clip]));
    let mut voice = (*message("voice", last + 120, vec![attachment("voice-message.ogg")])).clone();
    voice.voice = Some(VoiceMessage {
        duration_ms: 2_000,
        waveform: Arc::from(vec![40u8; 40]),
    });
    messages.push(Arc::new(voice));
    let photo = |name: &str| Attachment {
        image: true,
        dimensions: Some((4000, 1000)),
        ..attachment(name)
    };
    messages.push(message(
        "photos",
        last + 180,
        vec![photo("a.png"), photo("b.png")],
    ));
    app.vm.chrome.focused = true;
    app.motion.front = true;
    app.refresh();
    app
}

fn row<'a>(timeline: &'a Timeline, item: &str) -> &'a anomi_vm::chat::timeline::Row {
    let key = group::key(item);
    timeline
        .rows
        .iter()
        .find(|row| row.key == key)
        .expect("its row")
}

fn cell_player(app: &App) -> Option<&Player> {
    row(&app.vm.chat.timeline, "clip")
        .message
        .attachments
        .gallery
        .cells[0]
        .video
        .as_ref()
        .and_then(|video| video.player.as_ref())
}

fn card(app: &App) -> &Card {
    let row = row(&app.vm.chat.timeline, "voice");
    row.message.attachments.files[0]
        .voice
        .as_ref()
        .expect("a voice card")
}

/// The clip plays, with frames from a worker's mapping.
fn play(app: &mut App) -> Writer {
    let frames = Arc::new(Frames::create(4, 2).expect("frames"));
    let writer = Writer::open(&frames.path().expect("its file")).expect("the worker's side");
    frames.unlink();
    app.core.state.video = Some(chat::Video {
        room: "general".into(),
        item: "clip".into(),
        index: 0,
        frames: Some(frames),
        duration_ms: Some(6_000),
        size: Some((480, 270)),
        loading: false,
        playing: true,
        ended: false,
        sound: true,
        muted: false,
        volume: 1.0,
        error: None,
    });
    app.show(Reach::VIDEO);
    writer
}

fn ask(app: &mut App, intent: impl Into<anomi_vm::Intent>) {
    drop(crate::intent::handle(app, intent.into()));
}

#[test]
fn a_video_plays_in_its_cell_and_its_frames_change_no_view_model() {
    let folder = Folder::new("media-video");
    let mut app = general(&folder);
    assert!(cell_player(&app).is_none(), "its poster's play button");
    let mut writer = play(&mut app);
    let player = cell_player(&app).expect("the player on its cell").clone();
    assert!(player.playing && player.frame.is_none());
    assert_eq!(player.time, "0:00 / 0:06");
    let others = app
        .vm
        .chat
        .timeline
        .rows
        .iter()
        .filter(|row| row.key != group::key("clip"));
    assert!(
        others
            .flat_map(|row| &row.message.attachments.gallery.cells)
            .all(|cell| {
                cell.video
                    .as_ref()
                    .is_none_or(|video| video.player.is_none())
            })
    );
    assert!(!app.shown.media.alarm.idle(), "its frames are followed");

    // The first frame builds the view, which then holds its `Live`.
    writer.write_timed(2, 1, 40, |rgba| rgba.fill(7));
    assert_eq!(app.shown.media.alarm.take(), Found::View);
    drop(app.update(Message::Frames));
    let shown = cell_player(&app).expect("still playing").clone();
    let live = shown.frame.clone().expect("its frames");
    assert!(live.newest().is_some(), "drawn");
    // The next only redraws: what the view holds compares the same.
    writer.write_timed(2, 1, 80, |rgba| rgba.fill(8));
    assert_eq!(app.shown.media.alarm.take(), Found::Redraw);
    app.show(Reach::VIDEO);
    assert_eq!(cell_player(&app), Some(&shown));
    // A step of its bar builds it again.
    drop(live.newest());
    writer.write_timed(2, 1, 1_300, |rgba| rgba.fill(9));
    assert_eq!(app.shown.media.alarm.take(), Found::View);
    drop(app.update(Message::Frames));
    let moved = cell_player(&app).expect("playing");
    assert_eq!(moved.time, "0:01 / 0:06");
    assert!((moved.progress - 1.25 / 6.0).abs() < 1e-3);

    // Behind another window nothing is followed, and in front again it is.
    app.vm.chrome.focused = false;
    app.show(Reach::STATE);
    assert!(app.shown.media.alarm.idle(), "behind");
    app.vm.chrome.focused = true;
    app.show(Reach::STATE);
    assert!(!app.shown.media.alarm.idle());
    // Scrolled out of view, neither.
    let first = app.vm.chat.timeline.rows[0].key;
    app.vm.chat.timeline.shown = Some(anomi_vm::chat::timeline::Span {
        top: first,
        bottom: first,
        ..anomi_vm::chat::timeline::Span::default()
    });
    super::seen(&mut app);
    assert!(app.shown.media.alarm.idle(), "out of view");

    // Stopped, the cell shows its poster again.
    app.core.state.video = None;
    app.show(Reach::VIDEO);
    assert!(cell_player(&app).is_none());
}

#[test]
fn full_screen_shows_the_same_player_and_keeps_nothing_after() {
    let folder = Folder::new("media-full");
    let mut app = general(&folder);
    let _writer = play(&mut app);
    ask(&mut app, Chat::Video(VideoIntent::Expand(true)));
    assert!(app.vm.shell.is_open(Layer::Video));
    let full = &app.vm.chat.video;
    assert_eq!(full.name, "clip.mp4");
    assert_eq!(full.link.as_deref(), Some("https://cdn.example/clip.mp4"));
    assert_eq!(
        Some(&full.player),
        cell_player(&app),
        "one player, two places"
    );
    assert!(!app.shown.media.alarm.idle());

    // Escape: back to its cell; once its way out played, nothing stays.
    ask(&mut app, shell::Intent::Close(Layer::Video));
    assert!(!app.vm.shell.is_open(Layer::Video));
    ask(&mut app, shell::Intent::Gone(Layer::Video));
    assert_eq!(app.vm.chat.video, Default::default());

    // A video that stops takes full screen with it.
    ask(&mut app, Chat::Video(VideoIntent::Expand(true)));
    app.core.state.video = None;
    app.show(Reach::VIDEO);
    assert!(!app.vm.shell.is_open(Layer::Video));
}

#[test]
fn a_voice_messages_card_shows_the_player() {
    let folder = Folder::new("media-voice");
    let mut app = general(&folder);
    assert!(card(&app).enabled && !card(&app).active);
    let state = &mut app.core.state;
    state.voice_message = Some(VoicePlayback {
        room: "general".into(),
        item: "voice".into(),
        playing: true,
        position_ms: 500,
        ..VoicePlayback::default()
    });
    state.voice_tuning.speed = 1.5;
    app.show(Reach::VIDEO);
    let card = card(&app);
    assert!(card.active && card.playing);
    assert!((card.progress - 0.25).abs() < 1e-6);
    assert_eq!(card.time, "0:02", "what's left, rounded up");
    assert_eq!(card.speed, "1.5x");
    assert_eq!(
        crate::project::Reach::after(&Action::CycleVoiceMessageSpeed),
        Reach::VIDEO
    );
}

#[test]
fn the_viewer_opens_steps_zooms_and_lets_its_picture_go() {
    let folder = Folder::new("media-viewer");
    let mut app = general(&folder);
    drop(app.dispatch(Action::ViewImage {
        item: "photos".into(),
        index: 1,
    }));
    assert!(app.vm.shell.is_open(Layer::Viewer));
    let viewer = &app.vm.chat.viewer;
    assert_eq!(
        (viewer.name.as_str(), viewer.place),
        ("b.png", Some((2, 2)))
    );
    assert_eq!(
        viewer.size,
        (1920, 480),
        "the full image's, shrunk to the largest"
    );
    assert_eq!(viewer.note, Note::Loading);
    ask(&mut app, Chat::Viewer(ViewerIntent::Zoom(Zoom::In)));
    assert!(app.vm.chat.viewer.zoom > 1.0);
    ask(&mut app, Chat::Viewer(ViewerIntent::Step(1)));
    assert_eq!(app.vm.chat.viewer.index, 0);
    assert_eq!(
        app.vm.chat.viewer.zoom, 1.0,
        "another picture starts at the fit"
    );
    ask(&mut app, shell::Intent::Close(Layer::Viewer));
    assert!(app.core.state.viewer.is_none());
    assert!(!app.vm.shell.is_open(Layer::Viewer));
    assert_eq!(
        app.vm.chat.viewer.name, "a.png",
        "while it plays its way out"
    );
    ask(&mut app, shell::Intent::Gone(Layer::Viewer));
    assert_eq!(app.vm.chat.viewer, Default::default());
}
