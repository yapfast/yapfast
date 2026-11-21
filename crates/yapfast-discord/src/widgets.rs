//! The widgets on someone's profile board, as `widgets` in their profile
//! gives them: `{id, data: {type, ...}}`, the game lists' `data` holding
//! `games: [{game_id, comment, tags}]`. Only game lists are shown.

use serde_json::Value;

use crate::ready;

/// What Discord's client calls a game it has no name for.
const UNKNOWN_GAME: &str = "Unknown game";
/// How many games Discord's client asks after at once.
pub const GAMES_AT_ONCE: usize = 20;

/// A widget on a board.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Widget {
    /// Its heading, as Discord's client gives its type.
    pub title: String,
    /// The games it lists.
    pub games: Vec<WidgetGame>,
}

/// A game on a widget.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WidgetGame {
    pub id: String,
    /// What they said about it, on a Favorite Game.
    pub comment: Option<String>,
    /// Its name, once [`name_games`] has it.
    pub name: String,
    /// The address of its picture, once [`name_games`] has it.
    pub icon: Option<String>,
}

/// A game as `GET /games` describes it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Game {
    pub id: String,
    pub name: String,
    /// The address of its icon, else of its cover.
    pub icon: Option<String>,
}

/// The game lists in a profile. A widget of another type (an application, a
/// personal note, a clips gallery), whose contents Yapfast doesn't show, is
/// left out with those Discord's client doesn't know; a game listed twice
/// shows once.
pub fn parse(value: &Value) -> Vec<Widget> {
    value["widgets"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(widget)
        .collect()
}

fn widget(value: &Value) -> Option<Widget> {
    let data = &value["data"];
    let title = match data["type"].as_str()? {
        "favorite_games" => "Favorite Game",
        "current_games" => "Games in Rotation",
        "want_to_play_games" => "Want to Play",
        "played_games" => "Games I Like",
        _ => return None,
    };
    let mut seen = std::collections::HashSet::new();
    let games = data["games"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|game| {
            let id = game["game_id"].as_str()?;
            seen.insert(id).then(|| WidgetGame {
                id: id.to_owned(),
                comment: game["comment"]
                    .as_str()
                    .filter(|comment| !comment.trim().is_empty())
                    .map(str::to_owned),
                name: UNKNOWN_GAME.to_owned(),
                icon: None,
            })
        })
        .collect();
    Some(Widget {
        title: title.to_owned(),
        games,
    })
}

/// The games in an answer from `GET /games?game_ids=...`.
pub fn parse_games(value: &Value) -> Vec<Game> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|game| {
            let id = game["id"].as_str()?;
            let media = &game["media"];
            Some(Game {
                id: id.to_owned(),
                name: game["name"].as_str().unwrap_or(UNKNOWN_GAME).to_owned(),
                icon: game_picture(id, &media["icon"])
                    .or_else(|| game_picture(id, &media["cover"])),
            })
        })
        .collect()
}

/// A game's picture, which Discord's client reads as `{type, value}`: the
/// hash of one on its CDN, or an address.
fn game_picture(id: &str, media: &Value) -> Option<String> {
    let value = media["value"].as_str().filter(|value| !value.is_empty())?;
    match media["type"].as_str()? {
        "hash" => Some(ready::application_icon(id, value)),
        "url" => Some(value.to_owned()),
        _ => None,
    }
}

/// Gives the widgets' games the names and pictures in `games`; one that isn't
/// there stays an unknown game, as in Discord's client.
pub fn name_games(widgets: &mut [Widget], games: &[Game]) {
    for game in widgets.iter_mut().flat_map(|widget| &mut widget.games) {
        if let Some(known) = games.iter().find(|known| known.id == game.id) {
            game.name.clone_from(&known.name);
            game.icon.clone_from(&known.icon);
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn game_lists_are_read_as_the_client_reads_them() {
        let profile = json!({"widgets": [
            {"id": "w1", "data": {"type": "favorite_games", "games": [
                {"game_id": "10", "comment": "The best", "tags": ["rpg"]},
            ]}},
            {"id": "w2", "data": {"type": "current_games", "games": [
                {"game_id": "11", "comment": ""},
                {"game_id": "12"},
                {"game_id": "11"},
            ]}},
            {"id": "w3", "data": {"type": "want_to_play_games", "games": []}},
            {"id": "w4", "data": {"type": "played_games", "games": [{"game_id": "13"}]}},
        ]});
        let widgets = parse(&profile);
        let titles: Vec<_> = widgets.iter().map(|widget| widget.title.as_str()).collect();
        assert_eq!(
            titles,
            [
                "Favorite Game",
                "Games in Rotation",
                "Want to Play",
                "Games I Like"
            ]
        );
        assert_eq!(widgets[0].games[0].id, "10");
        assert_eq!(widgets[0].games[0].comment.as_deref(), Some("The best"));
        let rotation: Vec<_> = widgets[1].games.iter().map(|game| &game.id).collect();
        assert_eq!(rotation, ["11", "12"], "a game listed twice shows once");
        assert_eq!(widgets[1].games[0].comment, None);
        assert_eq!(widgets[0].games[0].name, "Unknown game");
    }

    #[test]
    fn widgets_that_are_not_game_lists_and_unknown_ones_go() {
        let profile = json!({"widgets": [
            {"id": "a", "data": {"type": "application", "application_id": "20"}},
            {"id": "b", "data": {"type": "personal", "header": "Hi", "sections": []}},
            {"id": "c", "data": {"type": "clips_gallery", "clips": []}},
            {"id": "d", "data": {"type": "something_new"}},
            {"id": "e"},
        ]});
        assert_eq!(parse(&profile), []);
        assert_eq!(parse(&json!({})), []);
        assert_eq!(parse(&json!({"widgets": null})), []);
    }

    #[test]
    fn games_get_their_names_and_pictures() {
        let games = parse_games(&json!([
            {"id": "10", "name": "Icon Game", "media": {
                "icon": {"type": "hash", "value": "ic"},
                "cover": {"type": "hash", "value": "co"},
            }},
            {"id": "11", "name": "Cover Game", "media": {
                "icon": {"type": "hash", "value": ""},
                "cover": {"type": "url", "value": "https://cdn.discordapp.com/x.png"},
            }},
            {"id": "12", "name": "Bare Game"},
            {"name": "No id"},
        ]));
        assert_eq!(games.len(), 3);
        assert!(
            games[0]
                .icon
                .as_deref()
                .unwrap()
                .contains("/app-icons/10/ic.png")
        );
        assert_eq!(
            games[1].icon.as_deref(),
            Some("https://cdn.discordapp.com/x.png")
        );
        assert_eq!(games[2].icon, None);

        let mut widgets = parse(&json!({"widgets": [
            {"data": {"type": "played_games", "games": [{"game_id": "12"}, {"game_id": "99"}]}},
        ]}));
        name_games(&mut widgets, &games);
        assert_eq!(widgets[0].games[0].name, "Bare Game");
        assert_eq!(widgets[0].games[1].name, "Unknown game");
    }
}
