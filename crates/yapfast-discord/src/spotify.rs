//! Spotify as Discord's client follows it: Discord lends the linked account a
//! Spotify token, Spotify's dealer socket says what that account plays on any
//! device, and this session shares it as "Listening to Spotify". Spotify's
//! apps and libraries tell Discord nothing themselves.
//!
//! The tokens stay in memory, and neither they nor the socket's address, which
//! holds one, go in a log or an error; nothing here derives `Debug` for that
//! reason. An idle socket costs one ping every 30 seconds; an account with no
//! linked Spotify costs nothing.

use std::{
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use futures_util::{SinkExt, StreamExt};
use reqwest::{Client, Response, StatusCode, header};
use serde_json::{Value, json};
use tokio::{
    net::TcpStream,
    sync::{mpsc, watch},
    task::JoinSet,
    time::{Instant, MissedTickBehavior, timeout},
};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream,
    tungstenite::{self, Message, client::IntoClientRequest, http::HeaderValue},
};

use crate::{
    api::{self, Api, Refused},
    client, websocket,
};

type Activity = Option<Arc<Value>>;
type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

const API: &str = "https://api.spotify.com/v1";
const DEALER: &str = "wss://dealer.spotify.com";
/// What a dealer message about the socket's connection id starts with.
const CONNECTION: &str = "hm://pusher/v1/connections/";
/// What the cover's address starts with; the activity keeps the rest.
const IMAGE: &str = "https://i.scdn.co/image/";
/// Discord's "connection revoked" refusal.
const REVOKED: u64 = 40012;

const PING: Duration = Duration::from_secs(30);
/// How long a song may run past its end with no word before it's taken as
/// over, for a player that stopped and said nothing.
const GRACE: Duration = Duration::from_secs(5);
/// A player state this close to the last one is the same play, not a seek.
const SAME_PLAY_MS: u64 = 1500;
/// Device changes ask for the player at most this often.
const DEVICES_THROTTLE: Duration = Duration::from_secs(3);
/// Spotify answers 202 to a subscription it wants asked again, after a wait.
const SUBSCRIBE_TRIES: usize = 3;
const SUBSCRIBE_WAIT: Duration = Duration::from_secs(5);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
const FLAG_SYNC: u64 = 16;
const FLAG_PLAY: u64 = 32;
/// Discord's client names up to five artists, and cuts each text past 128 of
/// JavaScript's characters, the most an activity's field holds.
const ARTISTS: usize = 5;
const LONGEST: usize = 128;

/// The account's Spotify links that share what they play, from READY or
/// Discord's list of connections.
#[derive(Clone, Default)]
pub(crate) struct Accounts {
    me: String,
    linked: Vec<Account>,
}

#[derive(Clone)]
struct Account {
    id: String,
    /// READY's token, if it gave one; Discord lends another when it's refused.
    token: Option<String>,
}

impl Accounts {
    /// Takes in Discord's list of connections, for `me`. Returns whether what
    /// to watch changed. A link with `show_activity` off, Discord's "Display
    /// Spotify as your status", isn't watched. The tokens aren't compared: a
    /// new one for the same link changes nothing.
    pub(crate) fn keep(&mut self, me: &str, connections: &Value) -> bool {
        let linked: Vec<Account> = connections
            .as_array()
            .into_iter()
            .flatten()
            .filter(|connection| {
                connection["type"] == "spotify"
                    && connection["show_activity"] == true
                    && connection["revoked"] != true
            })
            .filter_map(|connection| {
                Some(Account {
                    id: connection["id"].as_str()?.to_owned(),
                    token: connection["access_token"].as_str().map(str::to_owned),
                })
            })
            .collect();
        let same = self.me == me
            && self
                .linked
                .iter()
                .map(|a| &a.id)
                .eq(linked.iter().map(|a| &a.id));
        if !same {
            self.me = me.to_owned();
            self.linked = linked;
        }
        !same
    }
}

/// Follows the accounts' players, and gives the activity to share, until the
/// receiver goes or the accounts' sender does.
pub(crate) fn start(
    api: Arc<Api>,
    accounts: watch::Receiver<Accounts>,
) -> watch::Receiver<Activity> {
    let (shown, receiver) = watch::channel(None);
    // A test server doesn't stand in for Spotify, so nothing reaches it.
    if client::test_server().is_none() {
        tokio::spawn(run(api, Endpoints::spotify(), accounts, shown));
    }
    receiver
}

/// Where Spotify is, and how soon to act, which tests shorten.
#[derive(Clone)]
struct Endpoints {
    api: String,
    dealer: String,
    ping: Duration,
    grace: Duration,
}

impl Endpoints {
    fn spotify() -> Self {
        Self {
            api: API.to_owned(),
            dealer: DEALER.to_owned(),
            ping: PING,
            grace: GRACE,
        }
    }
}

async fn run(
    api: Arc<Api>,
    endpoints: Endpoints,
    mut accounts: watch::Receiver<Accounts>,
    shown: watch::Sender<Activity>,
) {
    loop {
        let Accounts { me, linked } = accounts.borrow_and_update().clone();
        let (found, mut finds) = mpsc::unbounded_channel();
        // Dropped, and with it what they watch, when the accounts change.
        let mut watching = JoinSet::new();
        if !linked.is_empty() {
            match http() {
                Ok(http) => {
                    for (slot, account) in linked.iter().enumerate() {
                        watching.spawn(
                            Watch {
                                api: api.clone(),
                                http: http.clone(),
                                endpoints: endpoints.clone(),
                                me: me.clone(),
                                id: account.id.clone(),
                                token: account.token.clone(),
                                player: Player::default(),
                                ends: None,
                                slot,
                                found: found.clone(),
                            }
                            .run(),
                        );
                    }
                }
                Err(error) => tracing::warn!(%error, "Spotify can't be followed"),
            }
        }
        drop(found);
        let mut playing: Vec<Activity> = vec![None; linked.len()];
        show(&shown, None);
        loop {
            tokio::select! {
                () = shown.closed() => return,
                changed = accounts.changed() => {
                    if changed.is_err() {
                        show(&shown, None);
                        return;
                    }
                    break;
                }
                Some((slot, activity)) = finds.recv() => {
                    playing[slot] = activity;
                    // The first link that plays, as in Discord's client.
                    show(&shown, playing.iter().flatten().next().cloned());
                }
            }
        }
    }
}

fn show(shown: &watch::Sender<Activity>, next: Activity) {
    shown.send_if_modified(|old| {
        let changed = *old != next;
        *old = next;
        changed
    });
}

/// A client of its own, so that Discord's cookies and login never reach Spotify.
fn http() -> reqwest::Result<Client> {
    Client::builder()
        .user_agent(client::user_agent())
        .connect_timeout(REQUEST_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .build()
}

/// Why following a link stopped or paused.
#[derive(Debug, PartialEq)]
enum Failed {
    /// Discord won't lend a token for it: it was revoked or removed. It's
    /// left be until the connections change.
    Stop,
    /// Discord lent no token this time.
    NoToken,
    /// Spotify refused, or couldn't be reached.
    Spotify,
}

/// Following one link: a token, what's playing now, then the socket.
struct Watch {
    api: Arc<Api>,
    http: Client,
    endpoints: Endpoints,
    me: String,
    id: String,
    token: Option<String>,
    player: Player,
    /// When the song on show should have ended.
    ends: Option<Instant>,
    slot: usize,
    found: mpsc::UnboundedSender<(usize, Activity)>,
}

impl Watch {
    async fn run(mut self) {
        let mut backoff = Backoff::default();
        loop {
            if self.connection(&mut backoff).await == Err(Failed::Stop) {
                self.show(None);
                return;
            }
            if backoff.fails == 0 {
                tracing::info!("Spotify's connection ended; reconnecting");
            }
            tokio::time::sleep(backoff.fail()).await;
        }
    }

    fn show(&mut self, activity: Option<Value>) {
        if activity.is_none() {
            self.player = Player::default();
        }
        self.ends = self
            .player
            .ends_in
            .map(|left| Instant::now() + left + self.endpoints.grace);
        let _ = self.found.send((self.slot, activity.map(Arc::new)));
    }

    fn apply(&mut self, state: &Value) {
        let activity = self.player.apply(&self.me, state, now_ms());
        self.show(activity);
    }

    /// One connection of the dealer socket: as Discord's client does, the
    /// player comes first, which also finds out if the token still works.
    async fn connection(&mut self, backoff: &mut Backoff) -> Result<(), Failed> {
        self.poll().await?;
        let token = self.token.as_deref().ok_or(Failed::NoToken)?;
        let url = format!("{}/?access_token={token}", self.endpoints.dealer);
        match open(url).await {
            Ok(socket) => self.listen(socket, backoff).await,
            Err(status) => {
                if status == Some(StatusCode::UNAUTHORIZED) {
                    self.token = None;
                }
                Ok(())
            }
        }
    }

    async fn listen(&mut self, socket: Socket, backoff: &mut Backoff) -> Result<(), Failed> {
        let (mut write, mut read) = socket.split();
        let ping_every = self.endpoints.ping;
        let mut pings = tokio::time::interval_at(Instant::now() + ping_every, ping_every);
        pings.set_missed_tick_behavior(MissedTickBehavior::Delay);
        let mut heard = Instant::now();
        let mut subscribing = JoinSet::new();
        // When a device change may ask for the player next, and whether one
        // waits to.
        let mut devices_free = Instant::now();
        let mut devices_due = None;
        loop {
            tokio::select! {
                _ = pings.tick() => {
                    // A socket that heard nothing for three pings is gone.
                    // Discord's client sends the dealer's own ping, which it
                    // answers with a pong; the socket's ping is answered by
                    // any server that still reads.
                    if heard.elapsed() >= ping_every * 3 {
                        return Ok(());
                    }
                    let ping = json!({"type": "ping"}).to_string();
                    if write.send(Message::text(ping)).await.is_err()
                        || write.send(Message::Ping(Vec::new().into())).await.is_err()
                    {
                        return Ok(());
                    }
                }
                message = read.next() => {
                    heard = Instant::now();
                    let text = match message {
                        Some(Ok(Message::Text(text))) => text,
                        Some(Ok(Message::Close(_)) | Err(_)) | None => return Ok(()),
                        Some(Ok(_)) => continue,
                    };
                    let Ok(message) = serde_json::from_str::<Value>(text.as_str()) else {
                        continue;
                    };
                    if message["type"] != "message" {
                        continue;
                    }
                    if let Some(id) = message["uri"].as_str().and_then(|uri| uri.strip_prefix(CONNECTION)) {
                        let token = self.token.clone().ok_or(Failed::NoToken)?;
                        // The id comes encoded for an address, as it goes.
                        let url = format!("{}/me/notifications/player?connection_id={id}", self.endpoints.api);
                        subscribing.spawn(subscribe(self.http.clone(), url, token));
                        continue;
                    }
                    for event in events(&message) {
                        match event["type"].as_str() {
                            Some("PLAYER_STATE_CHANGED") if !event["event"]["state"].is_null() => {
                                self.apply(&event["event"]["state"]);
                            }
                            Some("DEVICE_STATE_CHANGED") => {
                                let now = Instant::now();
                                if now >= devices_free {
                                    devices_free = now + DEVICES_THROTTLE;
                                    self.poll().await?;
                                } else {
                                    devices_due = Some(devices_free);
                                }
                            }
                            _ => {}
                        }
                    }
                }
                Some(subscribed) = subscribing.join_next() => {
                    // Without it the socket says nothing, so it's replaced.
                    if !subscribed.unwrap_or(false) {
                        tracing::warn!("Spotify refused to send its player's changes");
                        return Ok(());
                    }
                    backoff.succeed();
                    tracing::info!("Following Spotify's player");
                }
                () = tokio::time::sleep_until(devices_due.unwrap_or_else(Instant::now)), if devices_due.is_some() => {
                    devices_due = None;
                    devices_free = Instant::now() + DEVICES_THROTTLE;
                    self.poll().await?;
                }
                () = tokio::time::sleep_until(self.ends.unwrap_or_else(Instant::now)), if self.ends.is_some() => {
                    self.show(None);
                }
            }
        }
    }

    /// Asks what's playing now. As in Discord's client, nothing playing and
    /// a failure both show nothing.
    async fn poll(&mut self) -> Result<(), Failed> {
        let url = format!(
            "{}/me/player?additional_types=track%2Cepisode",
            self.endpoints.api
        );
        let state = match self.get(&url).await {
            Ok(response) => body(response).await,
            Err(Failed::Spotify) => Value::Null,
            Err(failed) => {
                self.show(None);
                return Err(failed);
            }
        };
        self.apply(&state);
        Ok(())
    }

    /// Asks Spotify with the token. As in Discord's client, a 401 asks
    /// Discord for a new one and tries once more; that is the only time a
    /// token is renewed, as its age isn't known.
    async fn get(&mut self, url: &str) -> Result<Response, Failed> {
        if self.token.is_none() {
            self.new_token().await?;
        }
        let mut renewed = false;
        loop {
            let token = self.token.as_deref().ok_or(Failed::NoToken)?;
            let response = self
                .http
                .get(url)
                .bearer_auth(token)
                .send()
                .await
                .map_err(|error| {
                    let error = error.without_url();
                    tracing::warn!(%error, "couldn't reach Spotify");
                    Failed::Spotify
                })?;
            let status = response.status();
            if status.is_success() {
                return Ok(response);
            }
            if renewed || status != StatusCode::UNAUTHORIZED {
                tracing::warn!(%status, "Spotify refused a request");
                return Err(Failed::Spotify);
            }
            self.new_token().await?;
            renewed = true;
        }
    }

    async fn new_token(&mut self) -> Result<(), Failed> {
        self.token = None;
        let error = match self.api.spotify_access_token(&self.id).await {
            Ok(token) => {
                self.token = Some(token);
                return Ok(());
            }
            Err(error) => error,
        };
        if api::code(&error) == Some(REVOKED) {
            return Err(Failed::Stop);
        }
        tracing::warn!(%error, "Discord gave no Spotify token");
        // Asking again wouldn't change a refusal, and refusals count against
        // everything else Yapfast asks of Discord.
        match error.downcast_ref::<Refused>() {
            Some(Refused(status))
                if status.is_client_error() && *status != StatusCode::TOO_MANY_REQUESTS =>
            {
                Err(Failed::Stop)
            }
            _ => Err(Failed::NoToken),
        }
    }
}

/// The events a dealer message carries.
fn events(message: &Value) -> impl Iterator<Item = &Value> {
    message["payloads"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|payload| payload["events"].as_array().into_iter().flatten())
}

/// Opens the dealer socket, trusting what HTTPS and Discord's own sockets
/// trust: tokio-tungstenite's `connect_async` has no certificates of its own
/// here, so it can't open a `wss://` address at all. An error gives Spotify's
/// refusal; the log gives why it failed, but never the address, which holds
/// the token.
async fn open(url: String) -> Result<Socket, Option<StatusCode>> {
    let mut request = url.into_client_request().map_err(|_| None)?;
    let headers = request.headers_mut();
    headers.insert(
        header::USER_AGENT,
        HeaderValue::from_static(client::user_agent()),
    );
    headers.insert(
        header::ORIGIN,
        HeaderValue::from_static("https://discord.com"),
    );
    match timeout(REQUEST_TIMEOUT, websocket::connect(request)).await {
        Ok(Ok((socket, _))) => Ok(socket),
        Ok(Err(tungstenite::Error::Http(response))) => {
            let status = response.status();
            tracing::warn!(%status, "Spotify refused its socket");
            Err(StatusCode::from_u16(status.as_u16()).ok())
        }
        // A URL error may quote the address.
        Ok(Err(tungstenite::Error::Url(_))) => {
            tracing::warn!("couldn't reach Spotify's socket: a bad address");
            Err(None)
        }
        Ok(Err(error)) => {
            tracing::warn!(%error, "couldn't reach Spotify's socket");
            Err(None)
        }
        Err(_) => {
            tracing::warn!("couldn't reach Spotify's socket: no answer in time");
            Err(None)
        }
    }
}

/// Asks Spotify to send the player's changes down this connection, trying
/// as often as Discord's client does.
async fn subscribe(http: Client, url: String, token: String) -> bool {
    for attempt in 0..SUBSCRIBE_TRIES {
        if attempt > 0 {
            tokio::time::sleep(SUBSCRIBE_WAIT).await;
        }
        let answer = http.put(&url).bearer_auth(&token).send().await;
        if answer.is_ok_and(|answer| {
            answer.status().is_success() && answer.status() != StatusCode::ACCEPTED
        }) {
            return true;
        }
    }
    false
}

/// A response's JSON, or null for an empty one: Spotify's 204 for no player.
async fn body(response: Response) -> Value {
    match response.bytes().await {
        Ok(body) => serde_json::from_slice(&body).unwrap_or_default(),
        Err(_) => Value::Null,
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_millis() as u64)
}

/// Reconnecting's waits, as Discord's client grows them: half a second at
/// first, growing by up to twice itself at random, up to a minute.
struct Backoff {
    current: Duration,
    fails: u32,
}

const BACKOFF_MIN: Duration = Duration::from_millis(500);
const BACKOFF_MAX: Duration = Duration::from_secs(60);

impl Default for Backoff {
    fn default() -> Self {
        Self {
            current: BACKOFF_MIN,
            fails: 0,
        }
    }
}

impl Backoff {
    fn fail(&mut self) -> Duration {
        self.fails += 1;
        let grown = self.current.mul_f64(2.0 * rand::random::<f64>());
        self.current = (self.current + grown).min(BACKOFF_MAX);
        self.current
    }

    fn succeed(&mut self) {
        *self = Self::default();
    }
}

/// What's playing on the link, between player states.
#[derive(Default)]
struct Player {
    /// When the song started, in Unix milliseconds.
    start_ms: Option<u64>,
    /// How long is left of it.
    ends_in: Option<Duration>,
}

impl Player {
    /// The activity for a player state, in Discord's client's words: none
    /// unless it plays a song or an episode. `me` is this Discord account,
    /// whose party it is.
    fn apply(&mut self, me: &str, state: &Value, now_ms: u64) -> Option<Value> {
        let activity = self.activity(me, state, now_ms);
        if activity.is_none() {
            *self = Self::default();
        }
        activity
    }

    fn activity(&mut self, me: &str, state: &Value, now_ms: u64) -> Option<Value> {
        // A private session is one the account asked Spotify not to share.
        if state["is_playing"] != true || state["device"]["is_private_session"] == true {
            return None;
        }
        let item = &state["item"];
        let duration = item["duration_ms"].as_u64()?;
        let (id, album, single, artists): (_, _, _, Vec<&Value>) = match item["type"].as_str()? {
            "track" => (
                // A song relinked for this market keeps the id it was asked for by.
                item["linked_from"]["id"].as_str().or(item["id"].as_str()),
                &item["album"],
                item["album"]["type"] == "single",
                item["artists"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|artist| !artist["id"].is_null() && !artist["name"].is_null())
                    .take(ARTISTS)
                    .collect(),
            ),
            "episode" => (item["id"].as_str(), &item["show"], false, Vec::new()),
            _ => return None,
        };
        let position = state["progress_ms"].as_u64().unwrap_or(0);
        let started = now_ms.saturating_sub(position);
        let start = match self.start_ms {
            Some(start) if start.abs_diff(started) <= SAME_PLAY_MS => start,
            _ => started,
        };
        self.start_ms = Some(start);
        self.ends_in = Some(Duration::from_millis(duration.saturating_sub(position)));

        let mut assets = json!({});
        if let Some(cover) = album["images"][0]["url"]
            .as_str()
            .and_then(|url| url.strip_prefix(IMAGE))
            .filter(|cover| !cover.is_empty())
        {
            assets["large_image"] = json!(format!("spotify:{cover}"));
        }
        if !single {
            assets["large_text"] = json!(clip(album["name"].as_str().unwrap_or_default()));
        }
        let mut activity = json!({
            "type": 2,
            "name": "Spotify",
            "assets": assets,
            "details": clip(item["name"].as_str().unwrap_or_default()),
            "timestamps": {"start": start, "end": start + duration},
            "party": {"id": format!("spotify:{me}")},
        });
        if !artists.is_empty() {
            let names: Vec<String> = artists
                .iter()
                .map(|artist| artist["name"].as_str().unwrap_or_default().replace(';', ""))
                .collect();
            activity["state"] = json!(clip(&names.join("; ")));
        }
        // A local file isn't on Spotify for anyone to play along with.
        if item["is_local"] != true {
            if let Some(id) = id {
                activity["sync_id"] = json!(id);
            }
            activity["flags"] = json!(FLAG_PLAY | FLAG_SYNC);
            let mut metadata = json!({
                "album_id": album["id"].as_str().unwrap_or_default(),
                "artist_ids": artists.iter().map(|artist| &artist["id"]).collect::<Vec<_>>(),
                "type": item["type"],
                "button_urls": [],
            });
            // Discord's client also gives a public playlist, which takes a
            // request of its own to tell from a private one.
            if state["context"]["type"] == "album"
                && let Some(context) = state["context"]["uri"].as_str()
            {
                metadata["context_uri"] = json!(context);
            }
            activity["metadata"] = metadata;
        }
        Some(activity)
    }
}

/// Text for an activity, as Discord's client cuts it: past 128 of
/// JavaScript's characters, to 125 and an ellipsis.
fn clip(text: &str) -> String {
    if text.encode_utf16().count() <= LONGEST {
        return text.to_owned();
    }
    let mut left = LONGEST - 3;
    let cut: String = text
        .chars()
        .take_while(|letter| {
            let fits = letter.len_utf16() <= left;
            left = left.saturating_sub(letter.len_utf16());
            fits
        })
        .collect();
    cut + "..."
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };
    use tokio_tungstenite::{
        accept_hdr_async,
        tungstenite::handshake::server::{Request, Response as Answer},
    };

    use super::*;

    const WAIT: Duration = Duration::from_secs(10);

    /// `GET /v1/me/player` as Spotify answers it, cut to what's read.
    fn song() -> Value {
        json!({
            "is_playing": true,
            "progress_ms": 10_000,
            "device": {"id": "d", "is_private_session": false},
            "context": {"type": "album", "uri": "spotify:album:al"},
            "item": {
                "type": "track", "id": "song", "name": "A Song", "duration_ms": 180_000,
                "album": {"id": "al", "name": "An Album", "type": "album",
                    "images": [{"url": "https://i.scdn.co/image/cover"}, {"url": "https://i.scdn.co/image/small"}]},
                "artists": [{"id": "a1", "name": "One;"}, {"id": "a2", "name": "Two"}, {"id": null, "name": "Local"}],
            },
        })
    }

    /// Word for word what Discord's client's `getActivity` puts together,
    /// with the `type` its LocalActivityStore adds.
    #[test]
    fn a_song_is_shared_as_discords_client_shares_it() {
        let mut player = Player::default();
        let shared = player.apply("7", &song(), 100_000).unwrap();
        assert_eq!(
            shared,
            json!({
                "type": 2,
                "name": "Spotify",
                "assets": {"large_image": "spotify:cover", "large_text": "An Album"},
                "details": "A Song",
                "state": "One; Two",
                "timestamps": {"start": 90_000, "end": 270_000},
                "party": {"id": "spotify:7"},
                "sync_id": "song",
                "flags": 48,
                "metadata": {
                    "context_uri": "spotify:album:al",
                    "album_id": "al",
                    "artist_ids": ["a1", "a2"],
                    "type": "track",
                    "button_urls": [],
                },
            })
        );
        assert_eq!(player.ends_in, Some(Duration::from_secs(170)));
        // As other people's cards read it.
        let read = crate::activity::activities(&json!([shared]));
        assert!(read[0].spotify());
        assert_eq!(read[0].heading(), "Listening to Spotify");
        assert_eq!(
            read[0].large_image.as_deref(),
            Some("https://i.scdn.co/image/cover")
        );
        assert_eq!(
            read[0].link.as_deref(),
            Some("https://open.spotify.com/track/song")
        );
    }

    #[test]
    fn other_kinds_of_item_follow_discords_client() {
        let mut player = Player::default();
        let mut state = song();
        state["item"]["linked_from"] = json!({"id": "asked"});
        state["item"]["album"]["type"] = json!("single");
        state["context"] = json!({"type": "playlist", "uri": "spotify:playlist:p"});
        let shared = player.apply("7", &state, 100_000).unwrap();
        assert_eq!(shared["sync_id"], "asked", "the id it was asked for by");
        assert!(shared["assets"].get("large_text").is_none());
        assert!(shared["metadata"].get("context_uri").is_none());

        // A local file offers no playing along.
        let mut state = song();
        state["item"]["is_local"] = json!(true);
        let shared = player.apply("7", &state, 100_000).unwrap();
        for field in ["sync_id", "flags", "metadata"] {
            assert!(shared.get(field).is_none(), "{field}");
        }

        let episode = json!({
            "is_playing": true, "progress_ms": 0,
            "context": {"type": "show", "uri": "spotify:show:sh"},
            "item": {"type": "episode", "id": "ep", "name": "Episode", "duration_ms": 1000,
                "show": {"id": "sh", "name": "Show", "images": [{"url": "https://i.scdn.co/image/art"}]}},
        });
        let shared = player.apply("7", &episode, 100_000).unwrap();
        assert_eq!(shared["sync_id"], "ep");
        assert!(shared.get("state").is_none());
        assert_eq!(shared["metadata"]["type"], "episode");
        assert_eq!(shared["metadata"]["album_id"], "sh");
        assert_eq!(shared["assets"]["large_text"], "Show");

        let mut state = song();
        state["item"]["artists"] = (0..7)
            .map(|n| json!({"id": n.to_string(), "name": n.to_string()}))
            .collect();
        let shared = player.apply("7", &state, 100_000).unwrap();
        assert_eq!(shared["state"], "0; 1; 2; 3; 4", "the first five artists");
    }

    #[test]
    fn a_pause_an_advert_or_a_private_session_shares_nothing() {
        let mut player = Player::default();
        assert!(player.apply("7", &song(), 100_000).is_some());
        let mut paused = song();
        paused["is_playing"] = json!(false);
        assert_eq!(player.apply("7", &paused, 110_000), None);
        assert_eq!(player.ends_in, None);
        let advert = json!({"is_playing": true, "item": null, "currently_playing_type": "ad"});
        assert_eq!(player.apply("7", &advert, 110_000), None);
        let mut private = song();
        private["device"]["is_private_session"] = json!(true);
        assert_eq!(player.apply("7", &private, 110_000), None);
        // Spotify's 204, no player at all.
        assert_eq!(player.apply("7", &Value::Null, 110_000), None);
    }

    #[test]
    fn the_start_holds_until_the_song_is_really_moved() {
        let mut player = Player::default();
        let mut start = |position: u64, now: u64| {
            let mut state = song();
            state["progress_ms"] = json!(position);
            player.apply("7", &state, now).unwrap()["timestamps"]["start"].as_u64()
        };
        assert_eq!(start(10_000, 100_000), Some(90_000));
        assert_eq!(start(20_000, 111_000), Some(90_000), "said a moment late");
        assert_eq!(start(5_000, 112_000), Some(107_000), "someone seeked");
    }

    #[test]
    fn long_text_is_cut_as_javascript_counts() {
        assert_eq!(clip(&"a".repeat(128)), "a".repeat(128));
        assert_eq!(clip(&"a".repeat(129)), format!("{}...", "a".repeat(125)));
        // An emoji counts as two, and isn't split.
        let cut = clip(&format!("{}{}", "a".repeat(124), "🎵".repeat(10)));
        assert_eq!(cut, format!("{}...", "a".repeat(124)));
        // Every text of the activity, not only the song's name.
        let mut state = song();
        let long = "b".repeat(200);
        state["item"]["album"]["name"] = json!(long);
        state["item"]["artists"] = (0..5)
            .map(|n| json!({"id": n.to_string(), "name": long}))
            .collect();
        let shared = Player::default().apply("7", &state, 100_000).unwrap();
        for text in [&shared["state"], &shared["assets"]["large_text"]] {
            assert_eq!(text.as_str().unwrap().len(), 128);
        }
    }

    #[test]
    fn only_a_shared_spotify_link_is_watched() {
        let mut accounts = Accounts::default();
        for unwatched in [
            json!([{"type": "github", "id": "x", "show_activity": true}]),
            json!([{"type": "spotify", "id": "x", "show_activity": false}]),
            json!([{"type": "spotify", "id": "x", "show_activity": true, "revoked": true}]),
        ] {
            assert!(!accounts.keep("", &unwatched), "{unwatched}");
        }
        assert!(accounts.keep("7", &json!([])), "this account is known now");
        let linked =
            json!([{"type": "spotify", "id": "x", "show_activity": true, "access_token": "t"}]);
        assert!(accounts.keep("7", &linked));
        assert_eq!(accounts.linked[0].token.as_deref(), Some("t"));
        // A new token alone leaves the socket as it is.
        let mut renewed = linked.clone();
        renewed[0]["access_token"] = json!("u");
        assert!(!accounts.keep("7", &renewed));
        assert!(accounts.keep("8", &linked), "another account");
        assert!(accounts.keep("8", &json!([])), "unlinked");
    }

    #[test]
    fn backing_off_grows_to_its_limit_and_starts_over() {
        let mut backoff = Backoff::default();
        let mut last = BACKOFF_MIN;
        for _ in 0..200 {
            let wait = backoff.fail();
            assert!(wait >= last && wait <= BACKOFF_MAX && wait <= last * 3);
            last = wait;
        }
        assert_eq!(last, BACKOFF_MAX);
        backoff.succeed();
        assert_eq!((backoff.current, backoff.fails), (BACKOFF_MIN, 0));
    }

    /// A fake of Discord's and Spotify's HTTP, answering each request by its
    /// first line and `authorization`, and keeping both.
    type Seen = Arc<Mutex<Vec<(String, String)>>>;

    async fn fake_http(
        answer: impl Fn(&str, &str) -> (u16, String) + Send + Sync + 'static,
    ) -> (String, Seen) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let seen = Seen::default();
        let answer = Arc::new(answer);
        let keep = seen.clone();
        tokio::spawn(async move {
            loop {
                let (mut socket, _) = listener.accept().await.unwrap();
                let (answer, keep) = (answer.clone(), keep.clone());
                tokio::spawn(async move {
                    let mut read = vec![];
                    let head = loop {
                        let mut chunk = [0; 4096];
                        let got = socket.read(&mut chunk).await.unwrap();
                        if got == 0 {
                            return;
                        }
                        read.extend_from_slice(&chunk[..got]);
                        let text = String::from_utf8_lossy(&read).into_owned();
                        if let Some((head, _)) = text.split_once("\r\n\r\n") {
                            break head.to_owned();
                        }
                    };
                    let line = head.lines().next().unwrap().to_owned();
                    let authorization = head
                        .lines()
                        .find_map(|line| {
                            let (name, value) = line.split_once(": ")?;
                            name.eq_ignore_ascii_case("authorization")
                                .then(|| value.to_owned())
                        })
                        .unwrap_or_default();
                    let (status, body) = answer(&line, &authorization);
                    keep.lock().unwrap().push((line, authorization));
                    let response = format!(
                        "HTTP/1.1 {status} X\r\ncontent-length: {}\r\ncontent-type: application/json\r\nconnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    socket.write_all(response.as_bytes()).await.unwrap();
                });
            }
        });
        (format!("http://{address}"), seen)
    }

    /// Takes the next connection to a fake dealer, and the address it asked for.
    #[allow(clippy::result_large_err)] // The size of tungstenite's callback.
    async fn dealer_connects(listener: &TcpListener) -> (WebSocketStream<TcpStream>, String) {
        let (stream, _) = timeout(WAIT, listener.accept()).await.unwrap().unwrap();
        let asked = Arc::new(Mutex::new(String::new()));
        let keep = asked.clone();
        let socket = accept_hdr_async(stream, move |request: &Request, answer: Answer| {
            *keep.lock().unwrap() = request.uri().to_string();
            Ok(answer)
        })
        .await
        .unwrap();
        let asked = asked.lock().unwrap().clone();
        (socket, asked)
    }

    fn dealer_event(kind: &str, event: Value) -> Message {
        let message = json!({"type": "message", "payloads": [{"events": [
            {"type": kind, "event": event},
        ]}]});
        Message::text(message.to_string())
    }

    struct Started {
        accounts: watch::Sender<Accounts>,
        shown: watch::Receiver<Activity>,
        task: tokio::task::JoinHandle<()>,
    }

    fn follow(
        http: &str,
        dealer: &TcpListener,
        connections: Value,
        tweak: impl FnOnce(&mut Endpoints),
    ) -> Started {
        let mut api = Api::new("discord-token".to_owned()).unwrap();
        api.base = format!("{http}/api/v9");
        let mut endpoints = Endpoints {
            api: format!("{http}/v1"),
            dealer: format!("ws://{}", dealer.local_addr().unwrap()),
            ping: PING,
            grace: GRACE,
        };
        tweak(&mut endpoints);
        let mut kept = Accounts::default();
        kept.keep("7", &connections);
        let (accounts, receiver) = watch::channel(kept);
        let (sender, shown) = watch::channel(None);
        let task = tokio::spawn(run(Arc::new(api), endpoints, receiver, sender));
        Started {
            accounts,
            shown,
            task,
        }
    }

    async fn next_shown(shown: &mut watch::Receiver<Activity>) -> Activity {
        timeout(WAIT, shown.changed()).await.unwrap().unwrap();
        shown.borrow_and_update().clone()
    }

    async fn until_seen(seen: &Seen, wanted: impl Fn(&str) -> bool) {
        timeout(WAIT, async {
            while !seen.lock().unwrap().iter().any(|(line, _)| wanted(line)) {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
    }

    fn count(seen: &Seen, prefix: &str) -> usize {
        let seen = seen.lock().unwrap();
        seen.iter()
            .filter(|(line, _)| line.starts_with(prefix))
            .count()
    }

    const PLAYER: &str = "GET /v1/me/player?additional_types=track%2Cepisode ";
    const TOKEN: &str = "GET /api/v9/users/@me/connections/spotify/x/access-token ";

    /// A whole run, over sockets: a token that has expired is renewed, a
    /// song shows, a pause clears it, another song shows, a device change
    /// asks the player again, and a lost socket comes back.
    #[tokio::test]
    async fn follows_the_player_through_a_lost_token_and_socket() {
        let tokens = Arc::new(Mutex::new(vec!["fresh", "stale"]));
        let plays = Arc::new(Mutex::new(vec![(200, song().to_string())]));
        let (http, seen) = fake_http(move |line, authorization| {
            if line.starts_with(TOKEN) {
                let token = tokens.lock().unwrap().pop().unwrap();
                return (200, json!({"access_token": token}).to_string());
            }
            if authorization == "Bearer stale" {
                return (401, String::new());
            }
            if line.starts_with(PLAYER) {
                // Spotify's 204 once nothing plays.
                return plays.lock().unwrap().pop().unwrap_or((204, String::new()));
            }
            (200, String::new())
        })
        .await;
        let dealer = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mut started = follow(
            &http,
            &dealer,
            json!([{"type": "spotify", "id": "x", "show_activity": true}]),
            |_| {},
        );

        let shown = next_shown(&mut started.shown).await.unwrap();
        assert_eq!(shown["sync_id"], "song");
        let (mut socket, asked) = dealer_connects(&dealer).await;
        assert_eq!(asked, "/?access_token=fresh", "the renewed token");
        // Spotify says which connection this is; Yapfast asks for its changes.
        let hello = json!({"type": "message", "uri": "hm://pusher/v1/connections/abc%3D"});
        socket.send(Message::text(hello.to_string())).await.unwrap();
        until_seen(&seen, |line| {
            line.starts_with("PUT /v1/me/notifications/player?connection_id=abc%3D ")
        })
        .await;

        let mut paused = song();
        paused["is_playing"] = json!(false);
        let changed = |state| json!({"state": state});
        socket
            .send(dealer_event("PLAYER_STATE_CHANGED", changed(paused)))
            .await
            .unwrap();
        assert_eq!(next_shown(&mut started.shown).await, None);
        let mut next = song();
        next["item"]["id"] = json!("another");
        socket
            .send(dealer_event("PLAYER_STATE_CHANGED", changed(next)))
            .await
            .unwrap();
        let shown = next_shown(&mut started.shown).await.unwrap();
        assert_eq!(shown["sync_id"], "another");

        // The player went away with its device.
        socket
            .send(dealer_event("DEVICE_STATE_CHANGED", json!({})))
            .await
            .unwrap();
        assert_eq!(next_shown(&mut started.shown).await, None);
        assert_eq!(count(&seen, PLAYER), 3);

        // The socket goes; the player is asked again before the next one.
        socket.close(None).await.unwrap();
        drop(socket);
        let (_socket, asked) = dealer_connects(&dealer).await;
        assert_eq!(asked, "/?access_token=fresh");
        assert_eq!(count(&seen, PLAYER), 4);
        assert_eq!(count(&seen, TOKEN), 2, "renewed only when refused");
        for (line, authorization) in seen.lock().unwrap().iter() {
            if line.contains("/api/v9/") {
                assert_eq!(authorization, "discord-token");
            } else {
                assert!(authorization.starts_with("Bearer "), "{line}");
            }
        }

        // The session going ends it.
        drop(started.accounts);
        timeout(WAIT, started.task).await.unwrap().unwrap();
    }

    /// READY's token goes unasked; a song that ends with no word clears, and
    /// a socket gone silent is left for a new one.
    #[tokio::test]
    async fn a_song_that_runs_out_clears_and_a_silent_socket_is_replaced() {
        let (http, seen) = fake_http(|line, _| {
            if line.starts_with(PLAYER) {
                let mut short = song();
                short["item"]["duration_ms"] = json!(10_000);
                return (200, short.to_string());
            }
            (500, String::new())
        })
        .await;
        let dealer = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mut started = follow(
            &http,
            &dealer,
            json!([{"type": "spotify", "id": "x", "show_activity": true, "access_token": "ready"}]),
            |endpoints| {
                endpoints.ping = Duration::from_millis(100);
                endpoints.grace = Duration::from_millis(50);
            },
        );
        assert!(next_shown(&mut started.shown).await.is_some());
        // Held, but never read, so no ping is answered.
        let (_first, asked) = dealer_connects(&dealer).await;
        assert_eq!(asked, "/?access_token=ready");
        assert_eq!(next_shown(&mut started.shown).await, None, "it ran out");
        let (_second, _) = dealer_connects(&dealer).await;
        assert_eq!(count(&seen, TOKEN), 0, "READY's token was good");
    }

    /// The fakes here speak `ws://`, which never needs a certificate; this
    /// reaches the real dealer over TLS with no token, which it refuses with
    /// 401 only once the TLS handshake and the request got through. Needs
    /// the network: `cargo test -p yapfast-discord -- --ignored
    /// reaches_spotifys_real_dealer`.
    #[tokio::test]
    #[ignore = "reaches dealer.spotify.com"]
    async fn reaches_spotifys_real_dealer() {
        let refused = open(format!("{DEALER}/?access_token=")).await.err();
        assert_eq!(refused, Some(Some(StatusCode::UNAUTHORIZED)));
    }

    #[tokio::test]
    async fn a_revoked_link_stops_asking_and_no_link_never_asks() {
        let (http, seen) = fake_http(|_, _| {
            let refusal = json!({"code": REVOKED, "message": "Connection revoked"});
            (400, refusal.to_string())
        })
        .await;
        let dealer = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let started = follow(
            &http,
            &dealer,
            json!([{"type": "github", "id": "x"}]),
            |_| {},
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(seen.lock().unwrap().is_empty(), "nothing to watch");
        // Linking one starts it, and being refused ends it, for good.
        let linked = json!([{"type": "spotify", "id": "x", "show_activity": true}]);
        started.accounts.send_modify(|accounts| {
            accounts.keep("7", &linked);
        });
        until_seen(&seen, |line| line.starts_with(TOKEN)).await;
        tokio::time::sleep(BACKOFF_MIN * 3).await;
        assert_eq!(seen.lock().unwrap().len(), 1, "asked once");
        assert!(started.shown.borrow().is_none());
    }
}
