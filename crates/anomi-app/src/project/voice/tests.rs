//! The call's projection: the dock and the stage from a call, a tile built
//! again only when what it shows changed, the stream watched shown large,
//! the corner while the call is off screen, and nothing followed behind.

use std::sync::Arc;
use std::time::Instant;

use yapfast_core::voice::{Participant, Phase, State, Watching};

use anomi_vm::conversation::Call;
use anomi_vm::shell::Layer;
use anomi_vm::voice::dock::Tone;
use anomi_vm::voice::tiles::{Fill, Mark, Outline};

use crate::app::App;
use crate::project::Reach;

pub(crate) fn person(id: &str, name: &str) -> Participant {
    Participant {
        id: id.into(),
        user: name.into(),
        person: id.into(),
        local: id == "alice",
        muted: false,
        deafened: false,
        server_muted: false,
        server_deafened: false,
        speaking: false,
        connected: true,
        problem: None,
        streaming: false,
        video: None,
        avatar: None,
    }
}

/// An app in a call in "discord:lounge" with Alice, Bob and Carol.
pub(crate) fn in_call(name: &str) -> (App, std::path::PathBuf) {
    let folder = std::env::temp_dir().join(format!("anomi-voice-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    let mut app = App::for_tests(&folder);
    app.core.state.voice = State {
        phase: Phase::Connected,
        room: Some("discord:lounge".into()),
        name: "Lounge".into(),
        muted: false,
        participants: Arc::from([
            person("alice", "Alice"),
            person("bob", "Bob"),
            person("carol", "Carol"),
        ]),
        ..State::default()
    };
    app.show(Reach::VOICE);
    (app, folder)
}

fn change(app: &mut App, edit: impl FnOnce(&mut Vec<Participant>)) {
    let mut people = app.core.state.voice.participants.to_vec();
    edit(&mut people);
    app.core.state.voice.participants = people.into();
    app.show(Reach::VOICE);
}

#[test]
fn a_call_shows_its_dock_and_a_tile_for_each() {
    let (app, folder) = in_call("dock");
    let dock = &app.vm.voice.dock;
    assert_eq!(dock.status, "Voice connected");
    assert_eq!(dock.tone, Tone::Connected);
    assert_eq!(dock.detail, "Lounge · Secure connection");
    let time = dock
        .time
        .map(|time| anomi_vm::voice::dock::elapsed(time.at(Instant::now())));
    assert_eq!(time.as_deref(), Some("0:00"));
    assert!(
        !crate::project::voice::ticking(&app.vm),
        "the dock's time needs no second's clock"
    );
    assert!(app.vm.shell.voice_dock && app.vm.voice.controls.ready);
    let names: Vec<&str> = (app.vm.voice.stage.tiles.iter())
        .map(|tile| tile.chip.name.as_str())
        .collect();
    assert_eq!(names, ["Alice", "Bob", "Carol"]);
    drop(app);
    let _ = std::fs::remove_dir_all(folder);
}

#[test]
fn someone_speaking_or_muting_builds_their_tile_alone() {
    let (mut app, folder) = in_call("speak");
    let built = app.shown.voice.tiles.builds;
    change(&mut app, |people| people[1].speaking = true);
    assert_eq!(app.shown.voice.tiles.builds, built + 1, "Bob's tile alone");
    assert_eq!(app.vm.voice.stage.tiles[1].outline, Outline::Speaking);
    change(&mut app, |people| people[2].muted = true);
    assert_eq!(app.shown.voice.tiles.builds, built + 2);
    assert_eq!(app.vm.voice.stage.tiles[2].chip.mark, Some(Mark::Muted));
    // Nothing changed: nothing is built.
    app.show(Reach::VOICE);
    assert_eq!(app.shown.voice.tiles.builds, built + 2);
    drop(app);
    let _ = std::fs::remove_dir_all(folder);
}

#[test]
fn a_stream_watched_shows_large_and_in_the_corner_off_screen() {
    let (mut app, folder) = in_call("watch");
    change(&mut app, |people| people[1].streaming = true);
    let bob = &app.vm.voice.stage.tiles[1];
    assert!(bob.watch && matches!(bob.fill, Fill::Avatar { .. }));
    app.core.state.voice.watching = Some(Box::new(Watching {
        user: "bob".into(),
        name: "Bob".into(),
        opening: true,
        frames: None,
        viewers: vec!["alice".into(), "bob".into()],
    }));
    app.show(Reach::VOICE);
    let stage = &app.vm.voice.stage;
    assert_eq!(stage.focused(), Some(1));
    let Fill::Watched { frame, status } = &stage.tiles[1].fill else {
        panic!("Bob's stream");
    };
    assert!(frame.is_none());
    assert_eq!(status, "Connecting to Bob's stream...");
    assert_eq!(stage.tiles[1].viewers.count, 1, "Bob left out");
    // The call isn't the open conversation: the stream sits in the corner.
    assert_ne!(app.vm.conversation.call, Call::Here { focused: true });
    assert!(app.vm.shell.is_open(Layer::Pip));
    assert_eq!(app.vm.voice.pip.title, "Bob's screen");
    drop(app);
    let _ = std::fs::remove_dir_all(folder);
}

/// Bob streams beside his camera: the stream's own tile shows its preview
/// as it comes, again after watching stopped.
#[test]
fn a_stream_beside_a_camera_shows_its_preview_when_it_comes() {
    use yapfast_core::chat::{Picture, Pixels};
    let (mut app, folder) = in_call("preview");
    change(&mut app, |people| {
        people[1].streaming = true;
        people[1].video = Some(yapfast_core::picture::Feed::default());
    });
    let offer = |app: &App| match &app.vm.voice.stage.tiles[0].fill {
        Fill::Offer { preview } => preview.is_some(),
        _ => panic!("Bob's stream offered"),
    };
    assert!(!offer(&app));
    crate::pictures::install();
    let preview = Arc::new(Picture {
        width: 1,
        height: 1,
        rgba: Pixels::new(1, 1, vec![1, 2, 3, 255]),
        ..Default::default()
    });
    app.core.state.stream_previews.insert("bob".into(), preview);
    app.show(Reach::VOICE);
    assert!(offer(&app));
    drop(app);
    let _ = std::fs::remove_dir_all(folder);
}

#[test]
fn a_camera_behind_the_window_is_not_decoded() {
    let (mut app, folder) = in_call("camera");
    let feed = yapfast_core::picture::Feed::default();
    let camera = feed.clone();
    change(&mut app, move |people| people[1].video = Some(camera));
    app.shown.voice.placement = Some(anomi_vm::voice::stage::Placement {
        columns: 3,
        width: 300.0,
        height: 170.0,
        shown: 600.0,
        ..Default::default()
    });
    // The call isn't on screen and nothing is in front: no size asked.
    app.show(Reach::VOICE);
    assert_eq!(feed.shown_at(), None);
    drop(app);
    let _ = std::fs::remove_dir_all(folder);
}

#[test]
fn the_call_ending_takes_its_dock_and_tiles_away() {
    let (mut app, folder) = in_call("end");
    app.core.state.voice = State::default();
    app.show(Reach::VOICE);
    assert!(!app.vm.shell.voice_dock);
    assert!(app.vm.voice.stage.tiles.is_empty());
    assert!(app.vm.voice.dock.time.is_none());
    drop(app);
    let _ = std::fs::remove_dir_all(folder);
}
