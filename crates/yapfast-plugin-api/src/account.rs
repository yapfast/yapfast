//! The payloads of the account's own calls: the activity a plugin shows, the
//! profile it writes, the reads of search, drafts and signed-in devices, the
//! linked Spotify, and the keybinds and sounds a manifest names.
//!
//! Each call has a capability of its own ([`crate::capability::Capability`]),
//! and each is carried out by the parent, which holds the token: a plugin
//! names what it wants and never reaches Discord or Spotify itself.

use serde::{Deserialize, Serialize};

// Only the limits Discord and Spotify keep themselves are here, so a plugin
// meets what the services would refuse anyway and nothing Yapfast made up:
// how often a presence goes out is the gateway's five in twenty seconds
// (`yapfast-discord`'s gateway coalesces to the latest), and Discord's and
// Spotify's own rate limits answer 429, which the plugin is told.

/// Discord's own limits on an activity's text, in characters
/// (docs.discord.food, resources/presence, Activity Object).
pub const ACTIVITY_NAME_CHARS: usize = 128;
pub const ACTIVITY_LINE_CHARS: usize = 128;
/// Discord shows at most two buttons (the same page, `buttons`).
pub const ACTIVITY_BUTTONS: usize = 2;
/// An asset image key, as Discord takes it (`large_image`, max 313); a web
/// address goes through the media proxy first and is longer before it does.
pub const ACTIVITY_IMAGE_BYTES: usize = 2048;

/// Discord's limits on a profile (docs.discord.food, resources/user, Modify
/// User Profile): a bio of 190 characters and pronouns of 40.
pub const BIO_CHARS: usize = 190;
pub const PRONOUNS_CHARS: usize = 40;
/// A custom status line: an activity's `state`, at most 128 characters.
pub const CUSTOM_STATUS_CHARS: usize = 128;

/// The results one search answer holds, which is Discord's own page.
pub const SEARCH_PAGE: usize = 25;

/// The sound events a plugin may replace, by the names a manifest writes:
/// the same twelve Yapfast's own sound sets play.
pub const SOUND_EVENTS: [&str; 12] = [
    "join",
    "leave",
    "mute",
    "unmute",
    "deafen",
    "undeafen",
    "message",
    "mention",
    "viewer_join",
    "viewer_leave",
    "stream_start",
    "stream_stop",
];

/// `activity.set`: what the account shows it is doing, beside the game and the
/// music Yapfast already shares.
///
/// `name` is the activity's own; with `application_id`, the Discord
/// application of the plugin's author, its pictures can be web addresses,
/// which Discord's media proxy takes through that application as the Rich
/// Presence bridge does, or the application's own asset keys. Without one an
/// activity has no pictures.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivitySet {
    /// `playing`, `streaming`, `listening`, `watching` or `competing`;
    /// `playing` when left out. `streaming` needs `url`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    /// Unix milliseconds: since when, and until when, it runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub application_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub large_image: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub large_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub small_image: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub small_text: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub buttons: Vec<ActivityButton>,
    /// The stream a `streaming` activity's Watch button opens: a Twitch or
    /// YouTube address, the two Discord shows as streaming.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// The party it shows: `1 of 4`, and an id other activities can join on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub party: Option<ActivityParty>,
}

/// An activity's party (docs.discord.food, resources/presence, Activity Party
/// Object).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivityParty {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// How many are in it now, and how many it takes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<(u32, u32)>,
}

/// A button under an activity: its words and the page it opens.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivityButton {
    pub label: String,
    pub url: String,
}

impl ActivitySet {
    /// Discord's activity type for `kind`: 0 playing, 2 listening, 3 watching,
    /// 5 competing (docs.discord.food, Activity Type). Streaming and custom
    /// are Discord's own and not a plugin's.
    pub fn activity_type(&self) -> Result<u8, String> {
        match self.kind.as_deref().unwrap_or("playing") {
            "playing" => Ok(0),
            "streaming" => Ok(1),
            "listening" => Ok(2),
            "watching" => Ok(3),
            "competing" => Ok(5),
            other => Err(format!(
                "`{other}` isn't an activity a plugin shows: `playing`, `streaming`, \
                 `listening`, `watching` or `competing`"
            )),
        }
    }

    /// Everything wrong with it, as the plugin is told.
    pub fn problems(&self) -> Result<(), String> {
        let kind = self.activity_type()?;
        // Discord shows an activity as streaming only for a Twitch or YouTube
        // address (docs.discord.food, Activity Object's `url`).
        let streams = |url: &str| {
            [
                "https://twitch.tv/",
                "https://www.twitch.tv/",
                "https://youtube.com/",
                "https://www.youtube.com/",
            ]
            .iter()
            .any(|start| url.starts_with(start))
        };
        match (&self.url, kind) {
            (Some(url), 1) if !streams(url) => {
                return Err(
                    "a streaming activity's `url` is a Twitch or YouTube address".to_owned(),
                );
            }
            (None, 1) => {
                return Err("a streaming activity needs the `url` of its stream".to_owned());
            }
            (Some(_), _) if kind != 1 => {
                return Err("`url` is for a `streaming` activity".to_owned());
            }
            _ => {}
        }
        if let Some(party) = &self.party {
            if let Some((now, most)) = party.size
                && (now > most || most == 0)
            {
                return Err("a party's size is how many are in it of how many it takes".to_owned());
            }
            if party
                .id
                .as_ref()
                .is_some_and(|id| id.is_empty() || id.len() > 128)
            {
                return Err("a party's id is 1 to 128 characters".to_owned());
            }
        }
        let name = self.name.trim();
        if name.is_empty() || name.chars().count() > ACTIVITY_NAME_CHARS {
            return Err(format!(
                "an activity's name is 1 to {ACTIVITY_NAME_CHARS} characters"
            ));
        }
        crate::text::check(name).map_err(|why| format!("its name: {why}"))?;
        for (field, text) in [
            ("details", &self.details),
            ("state", &self.state),
            ("large_text", &self.large_text),
            ("small_text", &self.small_text),
        ] {
            if let Some(text) = text {
                if text.chars().count() > ACTIVITY_LINE_CHARS {
                    return Err(format!(
                        "an activity's `{field}` is at most {ACTIVITY_LINE_CHARS} characters"
                    ));
                }
                crate::text::check(text).map_err(|why| format!("its `{field}`: {why}"))?;
            }
        }
        if let (Some(start), Some(end)) = (self.start_ms, self.end_ms)
            && end < start
        {
            return Err("an activity can't end before it starts".to_owned());
        }
        if let Some(id) = &self.application_id
            && !(id.len() <= 20 && !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()))
        {
            return Err("`application_id` is a Discord application's id, a number".to_owned());
        }
        if (self.large_image.is_some() || self.small_image.is_some())
            && self.application_id.is_none()
        {
            return Err(
                "an activity's pictures go through a Discord application: name yours in \
                 `application_id`"
                    .to_owned(),
            );
        }
        for image in [&self.large_image, &self.small_image].into_iter().flatten() {
            if image.len() > ACTIVITY_IMAGE_BYTES || image.starts_with("http://") {
                return Err(
                    "an activity's picture is an `https` address or the application's asset key"
                        .to_owned(),
                );
            }
        }
        if self.buttons.len() > ACTIVITY_BUTTONS {
            return Err(format!(
                "an activity has at most {ACTIVITY_BUTTONS} buttons"
            ));
        }
        for button in &self.buttons {
            let label = button.label.trim();
            if label.is_empty() {
                return Err("a button needs a label".to_owned());
            }
            crate::text::check(label).map_err(|why| format!("a button's label: {why}"))?;
            if !button.url.starts_with("https://") {
                return Err("a button opens an `https` address".to_owned());
            }
        }
        Ok(())
    }
}

/// `profile.set`: what the plugin changes on the account's profile. A field
/// left out stays as it is; an empty one clears it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileSet {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bio: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pronouns: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_status: Option<CustomStatusSet>,
}

/// The custom status a `profile.set` writes: a line and an emoji, both
/// optional; neither clears it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CustomStatusSet {
    #[serde(default)]
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emoji: Option<String>,
}

impl ProfileSet {
    /// Everything wrong with it, as the plugin is told.
    pub fn problems(&self) -> Result<(), String> {
        if self.bio.is_none() && self.pronouns.is_none() && self.custom_status.is_none() {
            return Err(
                "`profile.set` changes `bio`, `pronouns` or `custom_status`, and names none"
                    .to_owned(),
            );
        }
        if let Some(bio) = &self.bio
            && bio.chars().count() > BIO_CHARS
        {
            return Err(format!("a bio is at most {BIO_CHARS} characters"));
        }
        if let Some(pronouns) = &self.pronouns {
            if pronouns.chars().count() > PRONOUNS_CHARS {
                return Err(format!("pronouns are at most {PRONOUNS_CHARS} characters"));
            }
            crate::text::check(pronouns).map_err(|why| format!("its pronouns: {why}"))?;
        }
        if let Some(status) = &self.custom_status {
            if status.text.chars().count() > CUSTOM_STATUS_CHARS {
                return Err(format!(
                    "a custom status is at most {CUSTOM_STATUS_CHARS} characters"
                ));
            }
            crate::text::check(&status.text).map_err(|why| format!("its status: {why}"))?;
        }
        Ok(())
    }
}

/// `messages.search`: what to look for, in one conversation or one server, as
/// the search box asks it, and from which result.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SearchRequest {
    /// A room id, or a server's.
    pub place: String,
    /// The words, with the search box's own filters (`from:`, `has:` …).
    pub query: String,
    #[serde(default)]
    pub offset: u32,
}

/// `messages.search`'s answer: how many Discord found, and a page of them.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchPage {
    pub total: u32,
    pub hits: Vec<SearchHit>,
    /// Discord is still indexing older messages, so `total` may grow.
    #[serde(default)]
    pub indexing: bool,
}

/// One message a search found.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchHit {
    pub room: String,
    pub message: String,
    pub author: String,
    pub body: String,
    /// Unix milliseconds.
    pub at: u64,
}

/// One of `drafts.list`'s answers: a draft the person left in a conversation.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DraftView {
    pub room: String,
    pub body: String,
}

/// One of `account.sessions`' answers: a device signed in to the account, as
/// Discord's Devices page lists it (docs.discord.food, authentication, Auth
/// Session Object). Read-only: nothing here signs anything out.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionView {
    /// Discord's hash of the session's id, which names it and nothing more.
    pub id_hash: String,
    /// When it was last used, as Discord writes it (ISO 8601).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_used: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub os: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
}

/// `relationships.affinities`' payload: whose affinities, and how much of
/// Discord's answer.
///
/// Discord's own scores (docs.discord.food, resources/user, Get User
/// Affinities): `GET /users/@me/affinities/users`, `.../v2/users` for the
/// detailed form, and `.../guilds`. Read-only, and needs `relationships.read`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AffinitiesRequest {
    /// `users`, the default, or `servers`.
    pub of: AffinityOf,
    /// For `users`: Discord's v2 answer, with its probabilities and ranks for
    /// DMs, calls, server messages and talking at all.
    pub detailed: bool,
}

/// Whose affinities `relationships.affinities` reads.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AffinityOf {
    #[default]
    Users,
    Servers,
}

/// `relationships.affinities`' answer.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Affinities {
    /// For `users`: each person Discord scores, with the score, highest first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub users: Vec<UserAffinity>,
    /// For `servers`: each server, as `rooms.list` names it, with its score.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub servers: Vec<ServerAffinity>,
}

/// One person's affinity: their Discord user id, as the relationship events
/// name people, and Discord's score. The `detailed` fields are v2's, each
/// absent from the plain answer.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct UserAffinity {
    pub person: String,
    /// v1's score; in the detailed answer, `communication_probability`.
    pub affinity: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_friend: Option<bool>,
    /// `HFU_MAU`, `NON_HFU_MAU` or `NON_MAU`, as Discord writes them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_segment: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub other_user_segment: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dm_probability: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dm_rank: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vc_probability: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vc_rank: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_message_probability: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_message_rank: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub communication_probability: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub communication_rank: Option<u32>,
}

/// One server's affinity.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ServerAffinity {
    pub server: String,
    pub affinity: f64,
}

/// One service a plugin signs in to with its own app, from its manifest's
/// `oauth`: `oauth.login`. Authorization Code with PKCE, the flow a desktop
/// app with no secret uses, in the person's browser, with a redirect to this
/// computer's loopback.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OAuthDecl {
    /// `spotify`, whose addresses Yapfast knows, or a name of the plugin's
    /// own with `authorize` and `token` given.
    pub provider: String,
    /// The plugin's own app's client id, from the service's developer page.
    pub client_id: String,
    #[serde(default)]
    pub scopes: Vec<String>,
    /// The service's authorization page, `https`, for a provider Yapfast
    /// doesn't know.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authorize: Option<String>,
    /// Its token endpoint, `https`, likewise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    /// The loopback port the app's redirect URI names, for a service that
    /// wants it exact; a free port is picked when this is left out, which
    /// Spotify allows for `http://127.0.0.1` (its redirect URI docs).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub redirect_port: Option<u16>,
}

/// Spotify's two addresses (developer.spotify.com, Authorization Code with
/// PKCE Flow).
pub const SPOTIFY_AUTHORIZE: &str = "https://accounts.spotify.com/authorize";
pub const SPOTIFY_TOKEN: &str = "https://accounts.spotify.com/api/token";

/// The path every redirect comes back to.
pub const OAUTH_REDIRECT_PATH: &str = "/callback";

impl OAuthDecl {
    /// The authorization page and the token endpoint.
    pub fn endpoints(&self) -> (&str, &str) {
        match self.provider.as_str() {
            "spotify" => (SPOTIFY_AUTHORIZE, SPOTIFY_TOKEN),
            _ => (
                self.authorize.as_deref().unwrap_or_default(),
                self.token.as_deref().unwrap_or_default(),
            ),
        }
    }

    /// Why this can't be signed in with, if it can't.
    pub fn problems(&self) -> Result<(), String> {
        let plain = |text: &str| {
            !text.is_empty()
                && text.len() <= 128
                && text
                    .bytes()
                    .all(|byte| byte.is_ascii_graphic() && byte != b'&' && byte != b'#')
        };
        if !(1..=32).contains(&self.provider.len())
            || !self
                .provider
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        {
            return Err("a provider is a short lowercase name".to_owned());
        }
        if !plain(&self.client_id) {
            return Err(format!("{}'s client_id isn't one", self.provider));
        }
        if self.scopes.iter().any(|scope| !plain(scope)) {
            return Err(format!(
                "{}'s scopes are words with no spaces",
                self.provider
            ));
        }
        match self.provider.as_str() {
            "spotify" if self.authorize.is_some() || self.token.is_some() => Err(
                "Yapfast knows Spotify's addresses, so `spotify` takes no `authorize` or `token`"
                    .to_owned(),
            ),
            "spotify" => Ok(()),
            other => {
                let https = |url: &Option<String>| {
                    url.as_deref()
                        .is_some_and(|url| url.starts_with("https://") && url.len() > 8)
                };
                if https(&self.authorize) && https(&self.token) {
                    Ok(())
                } else {
                    Err(format!(
                        "`{other}` needs an `authorize` and a `token` address, each https"
                    ))
                }
            }
        }
    }
}

/// `oauth.login`, `oauth.token` and `oauth.logout`'s payload.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OAuthRequest {
    pub provider: String,
}

/// A signed-in account's access token, for the plugin to send to the
/// service's API itself with `http.fetch`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OAuthToken {
    pub provider: String,
    pub access_token: String,
    /// `Bearer`, as services say.
    pub token_type: String,
    /// When it stops working, in Unix milliseconds; `oauth.token` asks for a
    /// new one before then.
    pub expires_at_ms: u64,
    #[serde(default)]
    pub scopes: Vec<String>,
}

/// What a Spotify control call asks for. The method names it; the payload
/// carries what the call needs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpotifyAction {
    Now,
    Play,
    Pause,
    Next,
    Previous,
    Seek { position_ms: u64 },
    Volume { percent: u8 },
    Shuffle { on: bool },
    Repeat { mode: String },
}

/// `spotify.seek`'s payload.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpotifySeek {
    pub position_ms: u64,
}

/// `spotify.volume`'s payload: 0 to 100.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpotifyVolume {
    pub percent: u8,
}

/// `spotify.shuffle`'s payload.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpotifyShuffle {
    pub on: bool,
}

/// `spotify.repeat`'s payload: `off`, `track` or `context`, Spotify's words
/// (developer.spotify.com, Set Repeat Mode).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpotifyRepeat {
    pub mode: String,
}

impl SpotifyAction {
    /// The action a Spotify method names with its payload, or why it isn't one.
    pub fn of(method: &str, payload: &serde_json::Value) -> Result<Self, String> {
        use crate::call::method;
        let parse = |what: &str| -> Result<serde_json::Value, String> {
            Ok(payload
                .get(what)
                .cloned()
                .unwrap_or(serde_json::Value::Null))
        };
        let wrong = |error: serde_json::Error| format!("`{method}`'s payload: {error}");
        Ok(match method {
            method::SPOTIFY_NOW => SpotifyAction::Now,
            method::SPOTIFY_PLAY => SpotifyAction::Play,
            method::SPOTIFY_PAUSE => SpotifyAction::Pause,
            method::SPOTIFY_NEXT => SpotifyAction::Next,
            method::SPOTIFY_PREVIOUS => SpotifyAction::Previous,
            method::SPOTIFY_SEEK => {
                let seek: SpotifySeek = serde_json::from_value(payload.clone()).map_err(wrong)?;
                SpotifyAction::Seek {
                    position_ms: seek.position_ms,
                }
            }
            method::SPOTIFY_VOLUME => {
                let volume: SpotifyVolume =
                    serde_json::from_value(payload.clone()).map_err(wrong)?;
                if volume.percent > 100 {
                    return Err("`spotify.volume` is 0 to 100".to_owned());
                }
                SpotifyAction::Volume {
                    percent: volume.percent,
                }
            }
            method::SPOTIFY_SHUFFLE => {
                let shuffle: SpotifyShuffle =
                    serde_json::from_value(payload.clone()).map_err(wrong)?;
                SpotifyAction::Shuffle { on: shuffle.on }
            }
            method::SPOTIFY_REPEAT => {
                let mode = parse("mode")?;
                let mode = mode.as_str().unwrap_or_default();
                if !matches!(mode, "off" | "track" | "context") {
                    return Err("`spotify.repeat` is `off`, `track` or `context`".to_owned());
                }
                SpotifyAction::Repeat {
                    mode: mode.to_owned(),
                }
            }
            other => return Err(format!("`{other}` isn't a Spotify call")),
        })
    }
}

/// `spotify.now`'s answer, and the `spotify_state` event's payload: what the
/// linked Spotify plays, as Spotify's player says it. `null` when nothing does.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpotifyNow {
    pub playing: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub track: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artists: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub album: Option<String>,
    /// Spotify's URI for what plays, `spotify:track:…`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shuffle: Option<bool>,
    /// `off`, `track` or `context`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repeat: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device: Option<String>,
}

/// A keybind action a manifest names: what the person binds a key to in
/// Settings → Keybinds, where it shows as "<plugin>: <label>". The plugin is
/// told the `action` ran, through `yf_input`, and nothing about the key.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeybindDecl {
    pub action: String,
    pub label: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::call::method;
    use serde_json::json;

    fn activity() -> ActivitySet {
        ActivitySet {
            name: "Pomodoro".into(),
            details: Some("Focus".into()),
            ..ActivitySet::default()
        }
    }

    #[test]
    fn an_activity_is_checked_against_discords_limits() {
        assert_eq!(activity().problems(), Ok(()));
        assert_eq!(activity().activity_type(), Ok(0));
        let long = ActivitySet {
            name: "x".repeat(129),
            ..activity()
        };
        assert!(long.problems().unwrap_err().contains("1 to 128"));
        let streaming = ActivitySet {
            kind: Some("streaming".into()),
            ..activity()
        };
        assert!(streaming.problems().is_err(), "a stream needs its url");
        let live = ActivitySet {
            url: Some("https://www.twitch.tv/someone".into()),
            ..streaming.clone()
        };
        assert_eq!(live.problems(), Ok(()));
        assert_eq!(live.activity_type(), Ok(1));
        let elsewhere = ActivitySet {
            url: Some("https://example.com/live".into()),
            ..streaming
        };
        assert!(
            elsewhere.problems().is_err(),
            "Discord streams Twitch and YouTube"
        );
        let party = ActivitySet {
            party: Some(ActivityParty {
                id: Some("lobby".into()),
                size: Some((5, 4)),
            }),
            ..activity()
        };
        assert!(party.problems().is_err(), "five of four");
        let pictured = ActivitySet {
            large_image: Some("https://example.com/a.png".into()),
            ..activity()
        };
        assert!(pictured.problems().unwrap_err().contains("application_id"));
        let with_app = ActivitySet {
            application_id: Some("1234".into()),
            ..pictured
        };
        assert_eq!(with_app.problems(), Ok(()));
        let buttons = ActivitySet {
            buttons: vec![
                ActivityButton {
                    label: "Site".into(),
                    url: "http://example.com".into(),
                };
                1
            ],
            ..activity()
        };
        assert!(buttons.problems().unwrap_err().contains("https"));
        let three = ActivitySet {
            buttons: vec![
                ActivityButton {
                    label: "Site".into(),
                    url: "https://example.com".into(),
                };
                3
            ],
            ..activity()
        };
        assert!(three.problems().is_err());
    }

    #[test]
    fn a_profile_write_keeps_to_discords_lengths() {
        assert!(ProfileSet::default().problems().is_err());
        let bio = ProfileSet {
            bio: Some("a".repeat(BIO_CHARS)),
            ..ProfileSet::default()
        };
        assert_eq!(bio.problems(), Ok(()));
        let long = ProfileSet {
            bio: Some("a".repeat(BIO_CHARS + 1)),
            ..ProfileSet::default()
        };
        assert!(long.problems().is_err());
        let pronouns = ProfileSet {
            pronouns: Some("they/them\u{202e}".into()),
            ..ProfileSet::default()
        };
        assert!(pronouns.problems().is_err());
    }

    #[test]
    fn spotify_methods_read_their_payloads() {
        assert_eq!(
            SpotifyAction::of(method::SPOTIFY_NEXT, &json!(null)),
            Ok(SpotifyAction::Next)
        );
        assert_eq!(
            SpotifyAction::of(method::SPOTIFY_SEEK, &json!({ "position_ms": 5000 })),
            Ok(SpotifyAction::Seek { position_ms: 5000 })
        );
        assert!(SpotifyAction::of(method::SPOTIFY_VOLUME, &json!({ "percent": 101 })).is_err());
        assert!(SpotifyAction::of(method::SPOTIFY_REPEAT, &json!({ "mode": "all" })).is_err());
        assert_eq!(
            SpotifyAction::of(method::SPOTIFY_REPEAT, &json!({ "mode": "track" })),
            Ok(SpotifyAction::Repeat {
                mode: "track".into()
            })
        );
        for name in method::SPOTIFY {
            assert!(method::ALL.contains(&name), "{name}");
        }
    }
}
