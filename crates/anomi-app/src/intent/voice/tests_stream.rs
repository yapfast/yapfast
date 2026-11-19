//! The stream's own menu, as the share button, the gear on "Your screen"
//! and the gear on this account's stream's tile open it while live: its
//! quality changes the stream at once and it stays open, "Change Windows"
//! lists what to switch to, "Stop Streaming" ends it, and it closes when
//! the stream ends.

use std::sync::Arc;

use yapfast_core::stream_worker::{Shareable, Source};
use yapfast_core::voice::{Resolution, SharePhase, Sharing, StreamQuality};

use anomi_vm::menu::{Key, Spot};
use anomi_vm::shell::Layer;
use anomi_vm::voice::share::{self, Part};
use anomi_vm::voice::{Intent, tiles};

use super::handle;
use crate::app::App;
use crate::project::voice::tests::in_call;

/// A call in the preview, which applies a voice command to the state as
/// it's dispatched, this account's stream `phase`, at 720p and 15 fps.
fn streaming(name: &str, phase: SharePhase) -> (App, std::path::PathBuf) {
    let (mut app, folder) = in_call(name);
    app.core.state.preview = true;
    let frames = yapfast_core::frames::Frames::create(4, 2).expect("frames");
    app.core.state.voice.sharing = Some(Sharing {
        phase,
        frames: Arc::new(frames),
        viewers: Vec::new(),
    });
    app.core.state.voice.quality = StreamQuality {
        resolution: Resolution::P720,
        fps: 15,
        high_quality: true,
    };
    (app, folder)
}

fn ask(app: &mut App, intent: share::Intent) {
    drop(handle(app, Intent::Share(intent)));
}

/// The texts of a submenu's rows marked chosen.
fn marked(rows: &[anomi_vm::menu::Row<share::Intent>]) -> Vec<String> {
    let marked = rows.iter().filter(|row| row.checked);
    marked.map(|row| row.text.clone()).collect()
}

#[test]
fn while_live_the_share_button_opens_the_streams_menu_at_once() {
    let (mut app, folder) = streaming("stream-menu", SharePhase::Live);
    drop(super::share::open(&mut app, Spot::default()));
    assert!(app.vm.shell.is_open(Layer::Share), "no listing first");
    let live = app.vm.voice.share.live.as_ref().expect("the stream's menu");
    assert!(live.changeable);
    assert_eq!(
        marked(&live.quality.submenu),
        ["15 fps", "720p", "Higher quality"]
    );
    // A second click closes it.
    drop(super::share::open(&mut app, Spot::default()));
    assert!(!app.vm.shell.is_open(Layer::Share));
    drop(app);
    let _ = std::fs::remove_dir_all(folder);
}

#[test]
fn the_gear_on_the_streams_tile_opens_it_too() {
    let (mut app, folder) = streaming("stream-gear", SharePhase::Live);
    drop(handle(
        &mut app,
        Intent::Tiles(tiles::Intent::StreamSettings(Spot::default())),
    ));
    assert!(app.vm.shell.is_open(Layer::Share));
    assert!(app.vm.voice.share.live.is_some());
    drop(app);
    let _ = std::fs::remove_dir_all(folder);
}

#[test]
fn a_choice_changes_the_stream_at_once_and_the_menu_stays_open() {
    let (mut app, folder) = streaming("stream-quality", SharePhase::Live);
    drop(super::share::open(&mut app, Spot::default()));
    ask(&mut app, share::Intent::Hover(Some(Part::Quality)));
    ask(&mut app, share::Intent::Fps(60));
    ask(
        &mut app,
        share::Intent::Resolution(share::Resolution::Source),
    );
    let quality = app.core.state.voice.quality;
    assert_eq!(quality.fps, 60);
    assert_eq!(quality.resolution, Resolution::Source);
    assert!(app.vm.shell.is_open(Layer::Share), "the menu stays open");
    let live = app.vm.voice.share.live.as_ref().unwrap();
    assert_eq!(live.open, Some(Part::Quality), "its submenu too");
    assert_eq!(
        marked(&live.quality.submenu),
        ["60 fps", "Source", "Higher quality"]
    );
    drop(app);
    let _ = std::fs::remove_dir_all(folder);
}

/// The keys go into the quality's submenu past "FRAME RATE" and choose
/// as a click would.
#[test]
fn the_keys_choose_in_the_streams_menu() {
    let (mut app, folder) = streaming("stream-keys", SharePhase::Live);
    drop(super::share::open(&mut app, Spot::default()));
    for key in [Key::Down, Key::Right, Key::Down, Key::Enter] {
        ask(&mut app, share::Intent::Key(key));
    }
    assert_eq!(app.core.state.voice.quality.fps, 30);
    assert!(app.vm.shell.is_open(Layer::Share));
    drop(app);
    let _ = std::fs::remove_dir_all(folder);
}

/// "Change Windows" lists the sources in the menu's place, nothing picked,
/// so its Switch waits for a pick, as Slint's did.
#[test]
fn change_windows_lists_what_to_switch_to() {
    let (mut app, folder) = streaming("stream-change", SharePhase::Live);
    drop(super::share::open(&mut app, Spot::default()));
    ask(&mut app, share::Intent::Change);
    let screen = Shareable {
        source: Source::Display(7),
        title: "Screen 1".into(),
    };
    let listed = super::share::Listed(1, crate::message::Once::new(vec![screen]));
    drop(super::share::listed(&mut app, listed));
    assert!(app.vm.shell.is_open(Layer::Share));
    let menu = &app.vm.voice.share;
    assert!(menu.live.is_none(), "the sources, not the stream's menu");
    assert!(menu.switching && menu.source.is_none() && !menu.can_go());
    assert_eq!(menu.screens.len(), 1);
    drop(app);
    let _ = std::fs::remove_dir_all(folder);
}

#[test]
fn stop_streaming_closes_the_menu() {
    let (mut app, folder) = streaming("stream-stop", SharePhase::Live);
    drop(super::share::open(&mut app, Spot::default()));
    ask(&mut app, share::Intent::Stop);
    assert!(!app.vm.shell.is_open(Layer::Share));
    drop(app);
    let _ = std::fs::remove_dir_all(folder);
}

/// Before the stream is live, "Change Windows" waits; once live it can be
/// chosen; when the stream ends the menu closes.
#[test]
fn the_streams_menu_follows_the_stream() {
    let (mut app, folder) = streaming("stream-follow", SharePhase::Starting);
    drop(super::share::open(&mut app, Spot::default()));
    let changeable = |app: &App| {
        let live = app.vm.voice.share.live.as_ref().unwrap();
        (live.changeable, live.change.disabled)
    };
    assert_eq!(changeable(&app), (false, true));
    if let Some(sharing) = app.core.state.voice.sharing.as_mut() {
        sharing.phase = SharePhase::Live;
    }
    drop(super::share::follow(&mut app));
    assert_eq!(changeable(&app), (true, false));
    app.core.state.voice.sharing = None;
    drop(super::share::follow(&mut app));
    assert!(!app.vm.shell.is_open(Layer::Share));
    drop(app);
    let _ = std::fs::remove_dir_all(folder);
}
