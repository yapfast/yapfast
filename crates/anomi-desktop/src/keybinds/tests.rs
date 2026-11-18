use super::*;

fn key(scancode: u32, name: &str) -> Key {
    Key {
        scancode,
        name: name.into(),
        trigger: Some(name.to_lowercase()),
    }
}

fn watched(action: Action, keys: &[u32]) -> Watched {
    Watched {
        id: String::new(),
        action,
        keys: Shortcut::new(
            keys.iter()
                .map(|&code| key(code, &format!("{code}")))
                .collect(),
        )
        .unwrap(),
    }
}

/// Runs `events` (code, down) through keybinds and returns what they did.
fn run(watched: &[Watched], events: &[(u32, bool)]) -> Vec<(Action, bool)> {
    let mut matcher = Matcher::new(watched);
    let mut done = Vec::new();
    for &(code, down) in events {
        matcher.key(code, down, &mut |action, down| done.push((action, down)));
    }
    done
}

const V: u32 = 47;
const M: u32 = 50;
const D: u32 = 32;
#[cfg(target_os = "linux")]
const LEFT_CTRL: u32 = 29;
#[cfg(target_os = "linux")]
const RIGHT_CTRL: u32 = 97;
#[cfg(target_os = "linux")]
const SHIFT: u32 = 42;
#[cfg(windows)]
const LEFT_CTRL: u32 = 0x1D;
#[cfg(windows)]
const RIGHT_CTRL: u32 = 0xE01D;
#[cfg(windows)]
const SHIFT: u32 = 0x2A;

#[test]
fn a_key_holds_while_down_and_repeats_change_nothing() {
    let done = run(
        &[watched(Action::PushToTalk, &[V])],
        &[
            (V, true),
            (V, true),
            (M, true),
            (M, false),
            (V, false),
            (V, false),
        ],
    );
    assert_eq!(
        done,
        [(Action::PushToTalk, true), (Action::PushToTalk, false)]
    );
}

#[test]
fn a_toggle_happens_once_a_press() {
    let done = run(
        &[watched(Action::ToggleMute, &[M])],
        &[(M, true), (M, true), (M, false), (M, true)],
    );
    assert_eq!(
        done,
        [(Action::ToggleMute, true), (Action::ToggleMute, true)]
    );
}

#[test]
#[cfg(any(target_os = "linux", windows))]
fn a_combination_needs_its_keys_held_together_and_either_ctrl() {
    let keybinds = [watched(Action::ToggleMute, &[LEFT_CTRL, M])];
    assert_eq!(run(&keybinds, &[(M, true), (M, false)]), []);
    assert_eq!(
        run(&keybinds, &[(M, true), (LEFT_CTRL, true)]),
        [(Action::ToggleMute, true)],
        "in either order"
    );
    assert_eq!(
        run(&keybinds, &[(RIGHT_CTRL, true), (M, true)]),
        [(Action::ToggleMute, true)]
    );
    // A key alone is its side only.
    let right = [watched(Action::PushToTalk, &[RIGHT_CTRL])];
    assert_eq!(run(&right, &[(LEFT_CTRL, true)]), []);
}

#[test]
#[cfg(any(target_os = "linux", windows))]
fn push_to_talk_holds_while_other_keys_go_down_too() {
    // Talking while moving in a game: other keys don't stop it, and a
    // combination holds while its keys do.
    let done = run(
        &[watched(Action::PushToTalk, &[LEFT_CTRL, V])],
        &[
            (LEFT_CTRL, true),
            (V, true),
            (M, true),
            (D, true),
            (M, false),
            (LEFT_CTRL, false),
            (V, false),
        ],
    );
    assert_eq!(
        done,
        [(Action::PushToTalk, true), (Action::PushToTalk, false)]
    );
}

#[test]
#[cfg(any(target_os = "linux", windows))]
fn a_shorter_toggle_gives_way_to_a_longer_one_its_keys_complete() {
    let keybinds = [
        watched(Action::ToggleMute, &[LEFT_CTRL, D]),
        watched(Action::ToggleDeafen, &[LEFT_CTRL, SHIFT, D]),
        watched(Action::PushToTalk, &[D]),
    ];
    assert_eq!(
        run(&keybinds, &[(LEFT_CTRL, true), (SHIFT, true), (D, true)]),
        [(Action::ToggleDeafen, true), (Action::PushToTalk, true)],
        "a holding keybind still holds"
    );
    assert_eq!(
        run(&keybinds, &[(LEFT_CTRL, true), (D, true)]),
        [(Action::ToggleMute, true), (Action::PushToTalk, true)]
    );
}

#[test]
fn two_push_to_talk_keybinds_hold_until_both_are_up() {
    let done = run(
        &[
            watched(Action::PushToTalk, &[V]),
            watched(Action::PushToTalk, &[M]),
        ],
        &[(V, true), (M, true), (V, false), (M, false)],
    );
    assert_eq!(
        done,
        [(Action::PushToTalk, true), (Action::PushToTalk, false)]
    );
}

#[test]
fn letting_everything_go_ends_what_holds() {
    let mut matcher = Matcher::new(&[
        watched(Action::PushToTalk, &[V]),
        watched(Action::PushToMute, &[M]),
    ]);
    let mut done = Vec::new();
    let mut out = |action, down| done.push((action, down));
    matcher.key(V, true, &mut out);
    matcher.key(M, true, &mut out);
    matcher.release_all(&mut out);
    matcher.key(V, true, &mut out);
    assert_eq!(
        done,
        [
            (Action::PushToTalk, true),
            (Action::PushToMute, true),
            (Action::PushToTalk, false),
            (Action::PushToMute, false),
            (Action::PushToTalk, true),
        ]
    );
}

#[test]
fn a_key_up_that_never_came_is_found() {
    let mut matcher = Matcher::new(&[watched(Action::PushToTalk, &[V])]);
    let mut done = Vec::new();
    let mut out = |action, down| done.push((action, down));
    matcher.key(V, true, &mut out);
    matcher.resync(None, |_| true, &mut out);
    matcher.resync(Some(V), |_| false, &mut out);
    assert!(matcher.holding());
    matcher.resync(None, |_| false, &mut out);
    assert!(!matcher.holding());
    assert_eq!(
        done,
        [(Action::PushToTalk, true), (Action::PushToTalk, false)]
    );
}

#[test]
fn the_desktop_runs_keybinds_by_name() {
    let mut matcher = Matcher::new(&[
        watched(Action::PushToTalk, &[V]),
        watched(Action::ToggleDeafen, &[D]),
    ]);
    let mut done = Vec::new();
    let mut out = |action, down| done.push((action, down));
    matcher.set(0, true, &mut out);
    matcher.set(0, true, &mut out);
    matcher.set(1, true, &mut out);
    matcher.set(1, false, &mut out);
    matcher.set(0, false, &mut out);
    matcher.set(7, true, &mut out);
    assert_eq!(
        done,
        [
            (Action::PushToTalk, true),
            (Action::ToggleDeafen, true),
            (Action::PushToTalk, false),
        ]
    );
}
