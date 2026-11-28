//! Game Activity: the games Yapfast knows and the ones added by hand.

use iced::{
    Alignment, Element, Length,
    widget::{column, row, text},
};
use yapfast_core::application::Action;

use super::{GameDraft, GameEvent};
use crate::settings::{Event, Page, Settings};
use crate::{
    app::{App, Message},
    controls::{Kind, action},
    ctx::{Cx, Request},
    form::{self, switch},
    layout::BlockedGame,
};

/// A custom game's name, at its longest.
pub(super) const GAME_NAME: usize = 128;

pub(super) fn game_message(event: GameEvent) -> Message {
    crate::app::settings(Event::Game(event))
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
/// case, isn't added twice, and is turned on if it was off. False when it can't be kept.
pub fn add_blocked(blocked: &mut Vec<BlockedGame>, entry: &str) -> bool {
    let entry = entry.trim();
    if entry.is_empty() || entry.len() > 4096 || entry.chars().any(char::is_control) {
        return false;
    }
    match blocked
        .iter_mut()
        .find(|other| other.name.eq_ignore_ascii_case(entry))
    {
        // Listed already, perhaps turned off: it is on again.
        Some(listed) => listed.on = true,
        None => blocked.push(BlockedGame {
            name: entry.to_owned(),
            on: true,
        }),
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
            GameEvent::Detected => cx.request(Request::DetectedGamesMenu),
            GameEvent::ToggleBlocked(index) => {
                if let Some(game) = cx.layout.prefs.blocked_games.get_mut(index) {
                    game.on = !game.on;
                    blocked_changed(cx);
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

/// The menu of the games detection has taken the running programs for, at
/// `at`, one of which is chosen never to be shown, so it isn't typed.
pub fn open_detected_games_menu(app: &mut App, at: iced::Point) -> iced::Task<Message> {
    use crate::menus::{Menu, MenuAct, MenuRow};
    let names = yapfast_core::discord::detected_games();
    let rows = if names.is_empty() {
        vec![MenuRow {
            disabled: true,
            ..MenuRow::plain(MenuAct::BlockGame, "No game detected right now")
        }]
    } else {
        names
            .into_iter()
            .map(|name| MenuRow::plain(MenuAct::BlockGame, name.clone()).key(name))
            .collect()
    };
    app.win.menu = Some(Menu::new(at, rows));
    iced::widget::operation::focus(crate::app::NOBODY)
}

/// Never shows the game named `name`, chosen in the detected games' menu.
pub fn block_game(app: &mut App, name: &str) {
    let cx = crate::ctx::cx!(app);
    if add_blocked(&mut cx.layout.prefs.blocked_games, name) {
        blocked_changed(&cx);
    }
}

/// Keeps the games never shown, and detection takes them at once.
fn blocked_changed(cx: &Cx<'_>) {
    cx.keep("blocked_games", &cx.layout.prefs.blocked_games);
    cx.layout.prefs.apply_blocked_games();
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
            form::title(theme, crate::i18n::tr!(self.cx.i18n, "Game activity")),
            switch(
                "Share the game you play",
                shown,
                linked.then(|| crate::app::settings(Event::Game(GameEvent::Show(!shown)))),
            )
            .description("Yapfast looks for games among the programs running, and on Linux takes the Rich Presence games send. Your Discord account keeps this setting, so it's the same in Discord's apps.")
            .locked("Link a Discord account first.")
            .view(self.cx.look),
            form::heading(theme, crate::i18n::tr!(self.cx.i18n, "CUSTOM GAMES")),
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
        parts.push(form::heading(
            theme,
            crate::i18n::tr!(self.cx.i18n, "NEVER SHOW AS A GAME"),
        ));
        parts.push(form::note(
            theme,
            "Some programs Yapfast takes for games aren't. Name one by its executable, a file name or a full path, or by the name shown after Playing, and it isn't shown as the game you play.",
        ));
        for (index, entry) in self.cx.layout.prefs.blocked_games.iter().enumerate() {
            // On, it's hidden; off, it's allowed to show as a game again,
            // and stays listed to be hidden with one press.
            parts.push(
                row![
                    column![
                        text(entry.name.as_str())
                            .size(15)
                            .color(if entry.on { theme.text } else { theme.muted })
                            .wrapping(text::Wrapping::None),
                        text(if entry.on {
                            "Hidden: never shown as a game"
                        } else {
                            "Allowed: shown as a game while it runs"
                        })
                        .size(12)
                        .color(theme.muted)
                    ]
                    .spacing(2)
                    .width(Length::Fill),
                    form::toggle(
                        theme,
                        entry.on,
                        Some(game_message(GameEvent::ToggleBlocked(index)))
                    ),
                    action(
                        theme,
                        "Remove",
                        Kind::Plain,
                        Some(game_message(GameEvent::Unblock(index)))
                    )
                    .small()
                ]
                .spacing(12)
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
        // The games found among the running programs are only found while the
        // game played is shared.
        let scanning = linked && shown;
        parts.push(form::buttons(vec![
            action(
                theme,
                "Choose from detected games",
                Kind::Plain,
                scanning.then(|| game_message(GameEvent::Detected)),
            )
            .into(),
            action(
                theme,
                "Never show",
                Kind::Plain,
                (!draft.blocked.trim().is_empty()).then(|| game_message(GameEvent::Block)),
            )
            .into(),
        ]));
        if !scanning {
            parts.push(form::note(
                theme,
                "Choose from detected games lists the programs Yapfast takes for games while Share the game you play is on.",
            ));
        }
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
        assert_eq!(blocked.len(), 1, "kept once");
        assert_eq!(blocked[0].name, "Spotify.exe", "as first written");
        assert!(add_blocked(&mut blocked, "Visual Studio Code"));
        assert_eq!(blocked.len(), 2);
        assert!(!add_blocked(&mut blocked, "   "));
        assert!(!add_blocked(&mut blocked, "two\nlines"));
        assert!(!add_blocked(&mut blocked, &"x".repeat(4097)));
        assert_eq!(blocked.len(), 2, "refused entries add nothing");
    }

    #[test]
    fn adding_a_game_turned_off_turns_it_on_again_and_only_those_on_are_never_shown() {
        use crate::layout::active_blocked;
        let mut blocked = vec![];
        add_blocked(&mut blocked, "Roblox");
        add_blocked(&mut blocked, "Spotify");
        blocked[0].on = false;
        assert_eq!(
            active_blocked(&blocked),
            ["Spotify"],
            "an allowed game isn't left out"
        );
        assert!(add_blocked(&mut blocked, "roblox"));
        assert_eq!(blocked.len(), 2, "it stayed one entry");
        assert!(blocked[0].on, "and is hidden again");
        assert_eq!(active_blocked(&blocked), ["Roblox", "Spotify"]);
    }
}
