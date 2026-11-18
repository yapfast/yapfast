//! Snapshot baselines: each accepted shot of anomi-chat as an `iced_test`
//! snapshot, compared with its SHA-256 in
//! `tests/snapshots/<area>/<name>-<renderer>.sha256` on every
//! `scripts/check-iced --review`. Task Q02 (docs/iced/tasks/integration.md).
//!
//! Every shot is drawn as `anomi-shots` draws it, the whole window in front
//! with nothing still moving (`never`), at the shot's size and theme, and
//! hashed at the simulator's 2x. The test names every shot that differs.
//! `ANOMI_UPDATE_SNAPSHOTS=1 cargo test -p anomi-chat snapshots` rewrites
//! the baselines (the folder is emptied first, so a removed shot's goes too).

use std::path::PathBuf;

use iced::Widget as _;
use iced_test::Simulator;

use anomi_theme::{Motion, Theme};
use anomi_vm::Intent;
use anomi_vm::fixtures::Shot;

use super::support;

/// What a shot is drawn under: in front, nothing animating.
const MOTION: Motion = Motion {
    front: true,
    never: true,
    idle: false,
};

fn folder() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/snapshots")
}

/// The shot's hash differs from its baseline (or it could not be drawn).
fn differs(shot: &Shot) -> bool {
    let vm = (shot.vm)();
    let theme = Theme::built_in(Some(shot.theme));
    let view = anomi_screens::window(&vm, MOTION).boxed();
    let mut ui: Simulator<'_, Intent, Theme, _> =
        Simulator::with_size(support::settings(), shot.size, view);
    let path = folder().join(shot.area).join(shot.name);
    match ui
        .snapshot(&theme)
        .map(|snapshot| snapshot.matches_hash(&path))
    {
        Ok(Ok(same)) => !same,
        _ => true,
    }
}

#[test]
fn every_shot_matches_its_baseline() {
    if std::env::var_os("ANOMI_UPDATE_SNAPSHOTS").is_some() {
        let _ = std::fs::remove_dir_all(folder());
    }
    let shots = anomi_chat::shots::all();
    assert!(!shots.is_empty(), "no shots to compare");
    let dotted: Vec<_> = shots
        .iter()
        .filter(|shot| shot.name.contains('.'))
        .map(|shot| shot.name)
        .collect();
    assert!(
        dotted.is_empty(),
        "a name with a dot loses its end in the file's: {dotted:?}"
    );
    let differing: Vec<String> = shots
        .iter()
        .filter(|shot| differs(shot))
        .map(|shot| format!("{}/{}", shot.area, shot.name))
        .collect();
    assert!(
        differing.is_empty(),
        "{} of {} shots differ from their baselines (ANOMI_UPDATE_SNAPSHOTS=1 rewrites them on purpose): {differing:#?}",
        differing.len(),
        shots.len(),
    );
}
