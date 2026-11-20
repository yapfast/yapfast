//! Discord's Quests: sponsored games with rewards, from `GET /quests/@me`
//! and `GET /quests/@me/claimed`. A quest names the game, who sponsors it,
//! what it asks (play it for so many minutes, stream it, watch a video) and
//! what it gives (Orbs, a decoration, a code), and the account's progress
//! if it was accepted. Their pictures are on the CDN under `quests/{id}/`.
//! Yapfast lists them and accepts them; progress is made in the game or
//! the video, which only Discord's own client reports.

use chrono::DateTime;
use serde_json::Value;

use crate::ready;

/// Where Discord's client says an action came from, `location`, for the
/// desktop Quest Home page.
pub const QUEST_HOME_DESKTOP: u64 = 11;

/// Discord's numbers for a reward's type.
const REWARD_CODE: u64 = 1;
const REWARD_IN_GAME: u64 = 2;
const REWARD_COLLECTIBLE: u64 = 3;
const REWARD_ORBS: u64 = 4;
const REWARD_NITRO: u64 = 5;

/// How far this account is with a quest.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Status {
    /// Not accepted.
    #[default]
    Available,
    /// Accepted, and not yet done.
    Accepted,
    /// Done, and its reward not claimed.
    Completed,
    /// Done and claimed.
    Claimed,
}

/// What a reward is, as far as the page words it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RewardKind {
    #[default]
    Other,
    Code,
    InGame,
    Decoration,
    Orbs,
    Nitro,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Reward {
    pub kind: RewardKind,
    /// Its name, such as "Cybernetic Headgear Decoration".
    pub name: String,
    /// The Orbs it gives, for Orbs.
    pub orbs: Option<u64>,
    pub picture: Option<String>,
}

/// What a quest asks of the account.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Task {
    /// Play the game on a computer.
    #[default]
    Play,
    /// Play it and stream it.
    Stream,
    /// Watch a video.
    Watch,
    /// Play an activity inside Discord.
    Activity,
    /// Something the game reports itself.
    Achievement,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Quest {
    pub id: String,
    pub name: String,
    pub game: String,
    pub publisher: String,
    /// The wide picture of the quest's banner.
    pub hero: Option<String>,
    /// The picture over a quest's card, narrower than the banner's.
    pub bar: Option<String>,
    pub tile: Option<String>,
    pub logo: Option<String>,
    /// The gradient the quest's card takes, as `0xRRGGBB`.
    pub colors: Option<[u32; 2]>,
    /// When it ends, in seconds since 1970.
    pub expires: Option<i64>,
    pub reward: Option<Reward>,
    pub task: Task,
    /// What the task asks, in seconds for a task of time.
    pub target: u64,
    /// How much of it is done.
    pub progress: u64,
    pub status: Status,
    /// The game's page or store, and what its button says.
    pub link: Option<String>,
    pub button: Option<String>,
}

impl Quest {
    /// The share of the task done, from 0 to 1.
    pub fn fraction(&self) -> f32 {
        match self.status {
            Status::Completed | Status::Claimed => 1.0,
            _ if self.target == 0 => 0.0,
            _ => (self.progress as f32 / self.target as f32).clamp(0.0, 1.0),
        }
    }

    /// Whole minutes of the task still to do, rounded up.
    pub fn minutes_left(&self) -> u64 {
        self.target.saturating_sub(self.progress).div_ceil(60)
    }
}

/// The quests in `GET /quests/@me`, the ones Discord lists first.
pub fn parse(value: &Value) -> Vec<Quest> {
    value["quests"]
        .as_array()
        .map(|quests| quests.iter().filter_map(quest).collect())
        .unwrap_or_default()
}

/// The quests in `GET /quests/@me/claimed`.
pub fn parse_claimed(value: &Value) -> Vec<Quest> {
    value["quests"]
        .as_array()
        .map(|quests| quests.iter().filter_map(claimed).collect())
        .unwrap_or_default()
}

/// The address of a quest's picture on the CDN: the first of `names` that
/// names one. Discord gives a path from the CDN's root, `quests/<id>/<file>`,
/// and `PLACEHOLDER` where a quest has no picture of that kind; a bare file
/// name stands for one in the quest's own folder. Videos are passed over,
/// as nothing here plays them.
fn asset(id: &str, names: &[&Value]) -> Option<String> {
    names.iter().find_map(|name| {
        let name = name.as_str()?.trim_start_matches('/');
        let video = [".mp4", ".webm", ".m3u8", ".vtt"]
            .iter()
            .any(|extension| name.ends_with(extension));
        if name.is_empty() || name == "PLACEHOLDER" || video {
            return None;
        }
        Some(if name.contains('/') {
            format!("{}/{name}", ready::cdn())
        } else {
            format!("{}/quests/{id}/{name}", ready::cdn())
        })
    })
}

/// A quest's banner, card strip, game tile and logo, in the dark theme's
/// version where Discord has one.
fn pictures(id: &str, assets: &Value) -> [Option<String>; 4] {
    [
        asset(id, &[&assets["hero"], &assets["quest_bar_hero"]]),
        asset(id, &[&assets["quest_bar_hero"], &assets["hero"]]),
        asset(
            id,
            &[
                &assets["game_tile_dark"],
                &assets["game_tile"],
                &assets["game_tile_light"],
            ],
        ),
        asset(
            id,
            &[
                &assets["logotype_dark"],
                &assets["logotype"],
                &assets["logotype_light"],
            ],
        ),
    ]
}

fn seconds(timestamp: &Value) -> Option<i64> {
    DateTime::parse_from_rfc3339(timestamp.as_str()?)
        .ok()
        .map(|time| time.timestamp())
}

fn colors(gradient: &Value) -> Option<[u32; 2]> {
    let hex = |value: &Value| {
        u32::from_str_radix(value.as_str()?.trim_start_matches('#'), 16)
            .ok()
            .filter(|rgb| *rgb <= 0xff_ffff)
    };
    Some([hex(&gradient["primary"])?, hex(&gradient["secondary"])?])
}

fn task(config: &Value) -> (Task, u64) {
    let Some(tasks) = config["task_config_v2"]["tasks"].as_object() else {
        return (Task::Play, 0);
    };
    let Some((kind, task)) = tasks.iter().next() else {
        return (Task::Play, 0);
    };
    let task_kind = match kind.as_str() {
        "STREAM_ON_DESKTOP" => Task::Stream,
        "WATCH_VIDEO" | "WATCH_VIDEO_ON_MOBILE" => Task::Watch,
        "PLAY_ACTIVITY" => Task::Activity,
        "ACHIEVEMENT_IN_GAME" | "ACHIEVEMENT_IN_ACTIVITY" => Task::Achievement,
        _ => Task::Play,
    };
    (task_kind, task["target"].as_u64().unwrap_or(0))
}

fn reward(id: &str, reward: &Value) -> Option<Reward> {
    let kind = match reward["type"].as_u64()? {
        REWARD_CODE => RewardKind::Code,
        REWARD_IN_GAME => RewardKind::InGame,
        REWARD_COLLECTIBLE => RewardKind::Decoration,
        REWARD_ORBS => RewardKind::Orbs,
        REWARD_NITRO => RewardKind::Nitro,
        _ => RewardKind::Other,
    };
    let orbs = reward["orb_quantity"].as_u64().filter(|orbs| *orbs > 0);
    let name = reward["messages"]["name"]
        .as_str()
        .or_else(|| reward["name"].as_str())
        .unwrap_or_default()
        .to_owned();
    Some(Reward {
        kind,
        name,
        orbs,
        picture: asset(id, &[&reward["asset"]]),
    })
}

/// The progress of a quest's one task, and where the account is with it.
fn standing(status: &Value, target: u64) -> (Status, u64) {
    if !status["claimed_at"].is_null() {
        return (Status::Claimed, target);
    }
    if !status["completed_at"].is_null() {
        return (Status::Completed, target);
    }
    let progress = status["progress"]
        .as_object()
        .and_then(|progress| progress.values().next())
        .and_then(|task| task["value"].as_u64())
        .or_else(|| status["stream_progress_seconds"].as_u64())
        .unwrap_or(0);
    if status["enrolled_at"].is_null() {
        (Status::Available, 0)
    } else {
        (Status::Accepted, progress)
    }
}

fn quest(quest: &Value) -> Option<Quest> {
    let config = &quest["config"];
    let id = quest["id"].as_str().or_else(|| config["id"].as_str())?;
    let messages = &config["messages"];
    let name = messages["quest_name"].as_str()?.to_owned();
    let (task, target) = task(config);
    let (status, progress) = standing(&quest["user_status"], target);
    let [hero, bar, tile, logo] = pictures(id, &config["assets"]);
    let cta = &config["cta_config"];
    Some(Quest {
        id: id.to_owned(),
        name,
        game: messages["game_title"]
            .as_str()
            .unwrap_or_default()
            .to_owned(),
        publisher: messages["game_publisher"]
            .as_str()
            .unwrap_or_default()
            .to_owned(),
        hero,
        bar,
        tile,
        logo,
        colors: colors(&config["colors"]),
        expires: seconds(&config["expires_at"]),
        reward: config["rewards_config"]["rewards"]
            .as_array()
            .and_then(|rewards| rewards.first())
            .and_then(|first| reward(id, first)),
        task,
        target,
        progress,
        status,
        link: cta["link"].as_str().map(str::to_owned),
        button: cta["button_label"].as_str().map(str::to_owned),
    })
}

/// A claimed quest, whose config holds the rewards it gave.
fn claimed(quest: &Value) -> Option<Quest> {
    let config = &quest["config"];
    let id = quest["id"].as_str().or_else(|| config["id"].as_str())?;
    let messages = &config["messages"];
    let [hero, bar, tile, logo] = pictures(id, &config["assets"]);
    Some(Quest {
        id: id.to_owned(),
        name: messages["quest_name"].as_str()?.to_owned(),
        game: messages["game_title"]
            .as_str()
            .unwrap_or_default()
            .to_owned(),
        publisher: messages["game_publisher"]
            .as_str()
            .unwrap_or_default()
            .to_owned(),
        hero,
        bar,
        tile,
        logo,
        colors: colors(&config["colors"]),
        expires: seconds(&config["expires_at"]),
        reward: config["rewards"]
            .as_array()
            .and_then(|rewards| rewards.first())
            .and_then(|first| reward(id, first)),
        status: Status::Claimed,
        ..Quest::default()
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn available() -> Value {
        json!({
            "id": "11",
            "config": {
                "id": "11",
                "expires_at": "2026-12-01T00:00:00+00:00",
                "colors": {"primary": "#E944D4", "secondary": "#5318A7"},
                "assets": {
                    "hero": "quests/11/hero.jpg", "quest_bar_hero": "quests/11/bar.jpg",
                    "game_tile": "PLACEHOLDER", "game_tile_dark": "quests/11/tile.png",
                    "logotype": "PLACEHOLDER", "logotype_dark": "quests/11/logo.png",
                    "hero_video": "quests/11/hero_720.mp4"
                },
                "messages": {"quest_name": "Kill the Aliens", "game_title": "Cyberalien 2077", "game_publisher": "Alien Studios"},
                "task_config_v2": {"join_operator": "or", "tasks": {"PLAY_ON_DESKTOP": {"type": "PLAY_ON_DESKTOP", "target": 900}}},
                "rewards_config": {"rewards": [{"type": 4, "sku_id": "1", "asset": "orbs.png", "orb_quantity": 700, "messages": {"name": "700 Orbs"}}]},
                "cta_config": {"link": "https://example.com/play", "button_label": "Play Now"}
            },
            "user_status": null
        })
    }

    #[test]
    fn reads_an_available_quest() {
        let quests = parse(&json!({"quests": [available()]}));
        let quest = &quests[0];
        assert_eq!(quest.name, "Kill the Aliens");
        assert_eq!(quest.game, "Cyberalien 2077");
        assert_eq!(quest.publisher, "Alien Studios");
        assert_eq!(quest.status, Status::Available);
        assert_eq!((quest.task, quest.target), (Task::Play, 900));
        assert_eq!(quest.colors, Some([0xe944d4, 0x5318a7]));
        assert_eq!(quest.expires, Some(1_796_083_200));
        assert_eq!(quest.minutes_left(), 15);
        let end = |picture: &Option<String>| picture.as_deref().unwrap().to_owned();
        assert!(end(&quest.hero).ends_with("/quests/11/hero.jpg"));
        assert!(end(&quest.bar).ends_with("/quests/11/bar.jpg"));
        assert!(end(&quest.tile).ends_with("/quests/11/tile.png"));
        assert!(end(&quest.logo).ends_with("/quests/11/logo.png"));
        assert!(!end(&quest.hero).contains("/quests/quests/"));
        let reward = quest.reward.as_ref().unwrap();
        assert_eq!((reward.kind, reward.orbs), (RewardKind::Orbs, Some(700)));
        assert_eq!(quest.button.as_deref(), Some("Play Now"));
    }

    #[test]
    fn finds_pictures_by_path_or_bare_name_and_skips_what_is_missing() {
        let id = "11";
        let assets = json!({
            "hero": "PLACEHOLDER", "quest_bar_hero": "bar.jpg",
            "game_tile": "tile.png", "game_tile_dark": "",
            "logotype": null, "logotype_light": "/quests/11/light.png"
        });
        let [hero, bar, tile, logo] = pictures(id, &assets);
        // A bare name is in the quest's own folder, the strip stands in for
        // a banner that is missing, and the light logo for the dark one.
        assert!(hero.unwrap().ends_with("/quests/11/bar.jpg"));
        assert!(bar.unwrap().ends_with("/quests/11/bar.jpg"));
        assert!(tile.unwrap().ends_with("/quests/11/tile.png"));
        assert!(logo.unwrap().ends_with("/quests/11/light.png"));
        assert_eq!(asset(id, &[&json!("quests/11/clip.mp4")]), None);
        assert_eq!(pictures(id, &json!({})), [None, None, None, None]);
    }

    #[test]
    fn follows_an_accepted_quest_to_its_claim() {
        let mut accepted = available();
        accepted["user_status"] = json!({
            "enrolled_at": "2026-11-20T10:00:00+00:00", "completed_at": null, "claimed_at": null,
            "progress": {"PLAY_ON_DESKTOP": {"value": 300}}
        });
        let quest = &parse(&json!({"quests": [accepted.clone()]}))[0];
        assert_eq!((quest.status, quest.progress), (Status::Accepted, 300));
        assert_eq!(quest.minutes_left(), 10);
        assert!((quest.fraction() - 1.0 / 3.0).abs() < 0.001);

        accepted["user_status"]["completed_at"] = json!("2026-11-20T11:00:00+00:00");
        let done = &parse(&json!({"quests": [accepted.clone()]}))[0];
        assert_eq!(done.status, Status::Completed);
        assert_eq!(done.fraction(), 1.0);

        accepted["user_status"]["claimed_at"] = json!("2026-11-20T11:01:00+00:00");
        assert_eq!(
            parse(&json!({"quests": [accepted]}))[0].status,
            Status::Claimed
        );
    }

    #[test]
    fn skips_a_quest_without_a_name_and_reads_claimed_ones() {
        assert!(parse(&json!({"quests": [{"id": "1", "config": {}}]})).is_empty());
        assert!(parse(&json!({})).is_empty());
        let claimed = parse_claimed(&json!({"quests": [{
            "id": "7",
            "config": {
                "id": "7", "expires_at": "2026-01-01T00:00:00+00:00",
                "messages": {"quest_name": "Old", "game_title": "G", "game_publisher": "P"},
                "assets": {"hero": "h.jpg"},
                "rewards": [{"type": 3, "name": "Headgear", "asset": "h.png", "orb_quantity": null}]
            },
            "user_status": {"claimed_at": "2026-01-02T00:00:00+00:00"}
        }]}));
        assert_eq!(claimed[0].status, Status::Claimed);
        assert_eq!(
            claimed[0].reward.as_ref().map(|reward| reward.kind),
            Some(RewardKind::Decoration)
        );
    }
}
