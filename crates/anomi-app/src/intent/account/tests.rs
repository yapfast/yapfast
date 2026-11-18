//! The card's switches outside a call, its popover, and the custom status
//! dialog opening, keeping its draft and closing.

use yapfast_core::chat::CustomStatus;
use yapfast_core::sounds::Sound;

use anomi_vm::account::{Choice, Intent};
use anomi_vm::menu::Spot;
use anomi_vm::people::status::{self, ClearAfter};
use anomi_vm::shell::Layer;

use super::{handle, on_join};
use crate::app::App;
use crate::layout::Layout;

fn folder(name: &str) -> std::path::PathBuf {
    let folder = std::env::temp_dir().join(format!("anomi-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    folder
}

#[test]
fn outside_a_call_the_switches_say_how_the_next_is_joined() {
    let folder = folder("account-join");
    {
        let mut app = App::for_tests(&folder);
        drop(handle(&mut app, Intent::ToggleMute));
        assert!(app.vm.account.muted && !app.vm.account.deafened);
        drop(handle(&mut app, Intent::ToggleDeafen));
        assert!(app.vm.account.muted && app.vm.account.deafened);
        // Unmuting while deafened undeafens too.
        drop(handle(&mut app, Intent::ToggleMute));
        assert!(!app.vm.account.muted && !app.vm.account.deafened);
        drop(handle(&mut app, Intent::ToggleMute));
    }
    // Kept for the next start, in the Slint app's keys.
    let layout = Layout::read(&folder);
    assert_eq!(layout.unmute_on_join, Some(false));
    assert!(!layout.deafen_on_join);
    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn each_change_has_its_sound() {
    let mut saved = crate::layout::Saved::default();
    assert_eq!(on_join(&mut saved, false), Sound::Mute);
    assert_eq!(on_join(&mut saved, false), Sound::Unmute);
    assert_eq!(on_join(&mut saved, true), Sound::Deafen);
    assert_eq!(on_join(&mut saved, false), Sound::Undeafen);
    assert_eq!(saved.unmute_on_join, Some(true));
    assert_eq!(on_join(&mut saved, true), Sound::Deafen);
    assert_eq!(on_join(&mut saved, true), Sound::Undeafen);
}

#[test]
fn the_popover_opens_folded_and_a_choice_closes_it() {
    let folder = folder("account-popover");
    let mut app = App::for_tests(&folder);
    app.vm.account.popover.expanded = true;
    drop(handle(&mut app, Intent::Popover(Spot::default())));
    assert!(app.vm.shell.is_open(Layer::Account));
    assert!(!app.vm.account.popover.expanded);
    drop(handle(&mut app, Intent::ToggleStatuses));
    assert!(app.vm.account.popover.expanded);
    drop(handle(&mut app, Intent::Choose(Choice::DoNotDisturb)));
    assert!(!app.vm.shell.is_open(Layer::Account));
    drop(app);
    let _ = std::fs::remove_dir_all(folder);
}

#[test]
fn the_dialog_starts_from_the_status_there_is_and_closes_through_the_core() {
    use crate::intent::status::handle as status;
    let folder = folder("account-status");
    let mut app = App::for_tests(&folder);
    // Without Discord the core opens nothing.
    drop(handle(&mut app, Intent::CustomStatus));
    assert!(!app.vm.shell.is_open(Layer::Status));
    let state = &mut app.core.state;
    state.discord_user = Some("1".into());
    state.custom_status = Some(CustomStatus {
        text: "Testing Yapfast".into(),
        emoji: Some("\u{1f9ea}".into()),
        ..CustomStatus::default()
    });
    drop(handle(&mut app, Intent::CustomStatus));
    assert!(app.vm.shell.is_open(Layer::Status));
    let draft = &app.vm.people.status;
    assert_eq!(draft.text, "Testing Yapfast");
    assert_eq!(
        draft.emoji.as_ref().map(|emoji| emoji.key.as_str()),
        Some("\u{1f9ea}")
    );
    assert!(draft.has_custom);
    drop(status(&mut app, status::Intent::Text("Away".into())));
    drop(status(
        &mut app,
        status::Intent::ClearAfter(ClearAfter::AnHour),
    ));
    drop(status(&mut app, status::Intent::RemoveEmoji));
    let draft = &app.vm.people.status;
    assert_eq!(
        (draft.text.as_str(), draft.clear_after),
        ("Away", ClearAfter::AnHour)
    );
    assert_eq!(draft.emoji, None);
    // Escape or a press beside: the shell's close goes through the core.
    drop(crate::intent::shell::handle(
        &mut app,
        anomi_vm::shell::Intent::Close(Layer::Status),
    ));
    assert!(app.core.state.status_editor.is_none());
    assert!(!app.vm.shell.is_open(Layer::Status));
    drop(app);
    let _ = std::fs::remove_dir_all(folder);
}

#[test]
fn the_time_played_ticks_only_while_the_popover_shows_it_in_front() {
    use yapfast_core::chat::Playing;
    let folder = folder("account-clock");
    let mut app = App::for_tests(&folder);
    let minute = |app: &mut App| {
        drop(crate::clocks::arm(app));
        app.clocks.running()[0]
    };
    app.core.state.discord_playing = Some(Playing {
        name: "Factorio".into(),
        started_ms: Some(1),
    });
    app.refresh();
    app.motion.front = true;
    assert!(!minute(&mut app), "nothing shows the time");
    drop(handle(&mut app, Intent::Popover(Spot::default())));
    let game = app.vm.account.popover.game.as_ref().expect("the game");
    assert!(!game.duration.is_empty(), "said as it opens");
    assert_eq!(
        crate::intent::account::game_started(&app),
        Some(1),
        "a whole minute on from its start"
    );
    assert!(minute(&mut app));
    app.motion.front = false;
    assert!(!minute(&mut app), "behind another window");
    app.motion.front = true;
    app.vm.shell.close(Layer::Account);
    assert!(!minute(&mut app), "closed");
    drop(app);
    let _ = std::fs::remove_dir_all(folder);
}
