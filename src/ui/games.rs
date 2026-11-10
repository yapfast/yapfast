//! The game the Discord account shares: the switch for sharing it, the
//! games this device names itself, and how long the game shown has run.

use super::*;

pub(super) fn bind(window: &YapfastWindow, adapter: &Rc<RefCell<Adapter>>) {
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_show_game_toggled(move |shown| {
        if let Some(window) = weak.upgrade() {
            state.borrow_mut().action(&window, Action::ShowGame(shown));
        }
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_game_duration_due(move || {
        if let Some(window) = weak.upgrade() {
            let started = state
                .borrow()
                .app
                .state
                .discord_playing
                .as_ref()
                .and_then(|playing| playing.started_ms);
            window.set_game_activity_duration(
                started
                    .map(|started| played_for(now_ms().saturating_sub(started)))
                    .unwrap_or_default()
                    .into(),
            );
        }
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_save_game_executable(move |index, executable, name| {
        let (executable, name) = (executable.trim(), name.trim());
        if executable.is_empty()
            || name.is_empty()
            || executable.len() > 4096
            || name.chars().count() > 128
            || executable.chars().any(char::is_control)
            || name.chars().any(char::is_control)
        {
            return false;
        }
        let mut adapter = state.borrow_mut();
        let games = &mut adapter.layout.game_executables;
        let editing = usize::try_from(index).ok().filter(|&i| i < games.len());
        // Another entry for the same executable gives way to this one.
        let same = games
            .iter()
            .position(|(other, _)| other == executable)
            .filter(|&same| Some(same) != editing);
        let entry = (executable.to_owned(), name.to_owned());
        match (editing, same) {
            (Some(index), same) => {
                games[index] = entry;
                if let Some(same) = same {
                    games.remove(same);
                }
            }
            (None, Some(same)) => games[same] = entry,
            (None, None) => games.push(entry),
        }
        changed(&mut adapter, weak.upgrade());
        true
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_remove_game_executable(move |index| {
        let mut adapter = state.borrow_mut();
        if let Ok(index) = usize::try_from(index)
            && index < adapter.layout.game_executables.len()
        {
            adapter.layout.game_executables.remove(index);
            changed(&mut adapter, weak.upgrade());
        }
    });
}

/// Saves the custom games and shows them, and detection takes them.
fn changed(adapter: &mut Adapter, window: Option<YapfastWindow>) {
    adapter.layout.write(&adapter.root);
    if let Some(window) = window {
        adapter.layout.apply_custom_games(&window);
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// How long a game has run, as the account's menu says after "Playing for".
pub(super) fn played_for(elapsed_ms: u64) -> String {
    let minutes = elapsed_ms / 60_000;
    let (hours, minutes) = (minutes / 60, minutes % 60);
    match (hours, minutes) {
        (0, 0) => tr("less than a minute").into(),
        (0, minutes) => fill(tr("{} min"), &[&minutes]),
        (hours, 0) => fill(tr("{} hr"), &[&hours]),
        (hours, minutes) => fill(tr("{0} hr {1} min"), &[&hours, &minutes]),
    }
}
