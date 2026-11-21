//! Game Activity: the games Yapfast knows and the ones added by hand.

use iced::{
    Alignment, Element, Length,
    widget::{column, row, text},
};
use yapfast_core::application::Action;

use super::{GameDraft, GameEvent};
use crate::settings::{Event, Page, Settings};
use crate::{
    app::Message,
    controls::{Kind, action},
    ctx::Cx,
    form::{self, switch},
};

/// A custom game's name, at its longest.
pub(super) const GAME_NAME: usize = 128;

pub(super) fn game_message(event: GameEvent) -> Message {
    Message::Settings(Event::Game(event))
}

/// Saves a game: both parts needed, without line breaks, the name at most
/// 128 characters; another entry for the same executable gives way to this
/// one. False when it can't be saved.
pub fn save_game(
    games: &mut Vec<(String, String)>,
    editing: Option<usize>,
    executable: &str,
    name: &str,
) -> bool {
    let (executable, name) = (executable.trim(), name.trim());
    if executable.is_empty()
        || name.is_empty()
        || executable.len() > 4096
        || name.chars().count() > GAME_NAME
        || executable.chars().any(char::is_control)
        || name.chars().any(char::is_control)
    {
        return false;
    }
    let editing = editing.filter(|&index| index < games.len());
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
    true
}

/// Adds a program or game this device never shows as the game played: an
/// executable, a file name or a full path, or the name shown after Playing.
/// Without line breaks, of at most 4096 bytes; one already listed, in any
/// case, isn't added twice. False when it can't be kept.
pub fn add_blocked(blocked: &mut Vec<String>, entry: &str) -> bool {
    let entry = entry.trim();
    if entry.is_empty() || entry.len() > 4096 || entry.chars().any(char::is_control) {
        return false;
    }
    if !blocked.iter().any(|other| other.eq_ignore_ascii_case(entry)) {
        blocked.push(entry.to_owned());
    }
    true
}

impl GameDraft {
    /// The custom game's fields cleared, and what is being written for the
    /// games never shown left as it is.
    fn clear_game(&mut self) {
        *self = GameDraft {
            blocked: std::mem::take(&mut self.blocked),
            blocked_invalid: self.blocked_invalid,
            ..Default::default()
        };
    }
}

impl Settings {
    pub(in crate::settings) fn game_event(&mut self, event: GameEvent, cx: &mut Cx<'_>) {
        let draft = &mut self.game;
        match event {
            GameEvent::Show(on) => cx.dispatch(Action::ShowGame(on)),
            GameEvent::Executable(text) => draft.executable = text,
            GameEvent::Name(text) => draft.name = text,
            GameEvent::Edit(index) => {
                if let Some((executable, name)) = cx.layout.prefs.game_executables.get(index) {
                    *draft = GameDraft {
                        executable: executable.clone(),
                        name: name.clone(),
                        editing: Some(index),
                        invalid: false,
                        ..std::mem::take(draft)
                    };
                }
            }
            GameEvent::Remove(index) => {
                draft.clear_game();
                let games = &mut cx.layout.prefs.game_executables;
                if index < games.len() {
                    games.remove(index);
                    games_changed(cx);
                }
            }
            GameEvent::Cancel => draft.clear_game(),
            GameEvent::Save => {
                let editing = draft.editing;
                let saved = save_game(
                    &mut cx.layout.prefs.game_executables,
                    editing,
                    &draft.executable,
                    &draft.name,
                );
                if saved {
                    draft.clear_game();
                    games_changed(cx);
                } else {
                    draft.invalid = true;
                }
            }
            GameEvent::Blocked(text) => {
                draft.blocked = text;
                draft.blocked_invalid = false;
            }
            GameEvent::Block => {
                if add_blocked(&mut cx.layout.prefs.blocked_games, &draft.blocked) {
                    draft.blocked.clear();
                    draft.blocked_invalid = false;
                    blocked_changed(cx);
                } else {
                    draft.blocked_invalid = true;
                }
            }
            GameEvent::Unblock(index) => {
                let blocked = &mut cx.layout.prefs.blocked_games;
                if index < blocked.len() {
                    blocked.remove(index);
                    blocked_changed(cx);
                }
            }
        }
    }
}

/// Keeps the custom games, and detection takes them at once.
fn games_changed(cx: &Cx<'_>) {
    let games = &cx.layout.prefs.game_executables;
    cx.keep("game_executables", games);
    yapfast_core::discord::set_custom_games(games);
}

/// Keeps the games never shown, and detection takes them at once.
fn blocked_changed(cx: &Cx<'_>) {
    let blocked = &cx.layout.prefs.blocked_games;
    cx.keep("blocked_games", blocked);
    yapfast_core::discord::set_blocked_games(blocked);
}

impl<'a> Page<'a> {
    pub(in crate::settings) fn games_page(self) -> Element<'a, Message> {
        let settings = self.s;
        let theme = self.cx.look.theme;
        let state = self.cx.state;
        let linked = self.linked();
        let draft = &settings.game;
        let shown = state.discord_show_game;
        let mut parts = vec![
            form::title(theme, "Game activity"),
            switch(
                "Share the game you play",
                shown,
                linked.then(|| Message::Settings(Event::Game(GameEvent::Show(!shown)))),
            )
            .description("Yapfast looks for games among the programs running, and on Linux takes the Rich Presence games send. Your Discord account keeps this setting, so it's the same in Discord's apps.")
            .locked("Link a Discord account first.")
            .view(self.cx.look),
            form::heading(theme, "CUSTOM GAMES"),
            form::note(
                theme,
                "Name a game by its executable, a file name or a full path, and what to show after Playing. These come before the games Yapfast knows, but a game's Rich Presence comes first.",
            ),
        ];
        for (index, (executable, name)) in self.cx.layout.prefs.game_executables.iter().enumerate()
        {
            parts.push(
                column![
                    row![
                        text(name.as_str())
                            .size(15)
                            .color(theme.text)
                            .width(Length::Fill)
                            .wrapping(text::Wrapping::None),
                        action(
                            theme,
                            "Edit",
                            Kind::Plain,
                            Some(game_message(GameEvent::Edit(index)))
                        )
                        .small(),
                        action(
                            theme,
                            "Remove",
                            Kind::Plain,
                            Some(game_message(GameEvent::Remove(index)))
                        )
                        .small()
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                    text(executable.as_str()).size(12).color(theme.muted)
                ]
                .spacing(4)
                .into(),
            );
        }
        parts.push(
            form::field(
                theme,
                "settings-game-executable",
                "Executable name or full path",
                &draft.executable,
            )
            .on_input(|text| game_message(GameEvent::Executable(text)))
            .into(),
        );
        parts.push(
            form::field(
                theme,
                "settings-game-name",
                "Text after Playing (up to 128 characters)",
                &draft.name,
            )
            .on_input(|text| game_message(GameEvent::Name(text)))
            .into(),
        );
        if draft.invalid {
            parts.push(form::note(
                theme,
                "Enter an executable and a name of up to 128 characters, without line breaks.",
            ));
        }
        let ready = !draft.executable.is_empty() && !draft.name.is_empty();
        let mut buttons = vec![
            action(
                theme,
                if draft.editing.is_some() {
                    "Save game"
                } else {
                    "Add game"
                },
                Kind::Plain,
                ready.then(|| game_message(GameEvent::Save)),
            )
            .into(),
        ];
        if draft.editing.is_some() {
            buttons.push(
                action(
                    theme,
                    "Cancel",
                    Kind::Plain,
                    Some(game_message(GameEvent::Cancel)),
                )
                .into(),
            );
        }
        parts.push(form::buttons(buttons));
        parts.push(form::heading(theme, "NEVER SHOW AS A GAME"));
        parts.push(form::note(
            theme,
            "Some programs Yapfast takes for games aren't. Name one by its executable, a file name or a full path, or by the name shown after Playing, and it isn't shown as the game you play.",
        ));
        for (index, entry) in self.cx.layout.prefs.blocked_games.iter().enumerate() {
            parts.push(
                row![
                    text(entry.as_str())
                        .size(15)
                        .color(theme.text)
                        .width(Length::Fill)
                        .wrapping(text::Wrapping::None),
                    action(
                        theme,
                        "Remove",
                        Kind::Plain,
                        Some(game_message(GameEvent::Unblock(index)))
                    )
                    .small()
                ]
                .spacing(8)
                .align_y(Alignment::Center)
                .into(),
            );
        }
        parts.push(
            form::field(
                theme,
                "settings-game-blocked",
                "Executable, full path or name shown after Playing",
                &draft.blocked,
            )
            .on_input(|text| game_message(GameEvent::Blocked(text)))
            .into(),
        );
        if draft.blocked_invalid {
            parts.push(form::note(
                theme,
                "Enter an executable or a name, without line breaks.",
            ));
        }
        parts.push(form::buttons(vec![
            action(
                theme,
                "Never show",
                Kind::Plain,
                (!draft.blocked.trim().is_empty()).then(|| game_message(GameEvent::Block)),
            )
            .into(),
        ]));
        form::page(parts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_blocked_entry_is_trimmed_kept_once_and_refused_when_empty_or_broken() {
        let mut blocked = vec![];
        assert!(add_blocked(&mut blocked, "  Spotify.exe "));
        assert!(add_blocked(&mut blocked, "spotify.EXE"), "already there");
        assert_eq!(blocked, ["Spotify.exe"], "kept once, as first written");
        assert!(add_blocked(&mut blocked, "Visual Studio Code"));
        assert_eq!(blocked.len(), 2);
        assert!(!add_blocked(&mut blocked, "   "));
        assert!(!add_blocked(&mut blocked, "two\nlines"));
        assert!(!add_blocked(&mut blocked, &"x".repeat(4097)));
        assert_eq!(blocked.len(), 2, "refused entries add nothing");
    }
}
