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
use yapfast_core::i18n::tr;

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
                    };
                }
            }
            GameEvent::Remove(index) => {
                *draft = GameDraft::default();
                let games = &mut cx.layout.prefs.game_executables;
                if index < games.len() {
                    games.remove(index);
                    games_changed(cx);
                }
            }
            GameEvent::Cancel => *draft = GameDraft::default(),
            GameEvent::Save => {
                let editing = draft.editing;
                let saved = save_game(
                    &mut cx.layout.prefs.game_executables,
                    editing,
                    &draft.executable,
                    &draft.name,
                );
                if saved {
                    *draft = GameDraft::default();
                    games_changed(cx);
                } else {
                    draft.invalid = true;
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

impl<'a> Page<'a> {
    pub(in crate::settings) fn games_page(self) -> Element<'a, Message> {
        let settings = self.s;
        let theme = self.cx.look.theme;
        let state = self.cx.state;
        let linked = self.linked();
        let draft = &settings.game;
        let shown = state.discord_show_game;
        let mut parts = vec![
            form::title(theme, tr("Game activity")),
            switch(
                tr("Share the game you play"),
                shown,
                linked.then(|| crate::app::settings(Event::Game(GameEvent::Show(!shown)))),
            )
            .description(tr("Yapfast looks for games among the programs running, and on Linux takes the Rich Presence games send. Your Discord account keeps this setting, so it's the same in Discord's apps."))
            .locked(tr("Link a Discord account first."))
            .view(self.cx.look),
            form::heading(theme, tr("CUSTOM GAMES")),
            form::note(
                theme,
                tr("Name a game by its executable, a file name or a full path, and what to show after Playing. These come before the games Yapfast knows, but a game's Rich Presence comes first."),
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
                            tr("Edit"),
                            Kind::Plain,
                            Some(game_message(GameEvent::Edit(index)))
                        )
                        .small(),
                        action(
                            theme,
                            tr("Remove"),
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
                tr("Executable name or full path"),
                &draft.executable,
            )
            .on_input(|text| game_message(GameEvent::Executable(text)))
            .into(),
        );
        parts.push(
            form::field(
                theme,
                "settings-game-name",
                tr("Text after Playing (up to 128 characters)"),
                &draft.name,
            )
            .on_input(|text| game_message(GameEvent::Name(text)))
            .into(),
        );
        if draft.invalid {
            parts.push(form::note(
                theme,
                tr("Enter an executable and a name of up to 128 characters, without line breaks."),
            ));
        }
        let ready = !draft.executable.is_empty() && !draft.name.is_empty();
        let mut buttons = vec![
            action(
                theme,
                if draft.editing.is_some() {
                    tr("Save game")
                } else {
                    tr("Add game")
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
                    tr("Cancel"),
                    Kind::Plain,
                    Some(game_message(GameEvent::Cancel)),
                )
                .into(),
            );
        }
        parts.push(form::buttons(buttons));
        form::page(parts)
    }
}
