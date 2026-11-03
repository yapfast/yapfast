//! Discord's main gateway, as a user client: identify, heartbeats, and
//! dispatched events. Voice media goes through the voice connection
//! (`voice_connection.rs`); this only carries the voice state and voice server
//! details it needs to connect.
//!
//! A lost connection resumes the session where it can, as Discord asks of
//! every client: Yapfast connects to the session's resume address, sends RESUME
//! with the last sequence number, and Discord replays what was missed. A new
//! IDENTIFY, which Discord limits, follows only when Discord won't resume,
//! and [`Reconnects`] spaces those out and caps them. Yapfast also keeps under
//! Discord's limit on what a connection sends, in the connection's own task,
//! so commands held back don't hold back the heartbeats.

use std::{
    collections::{HashMap, VecDeque},
    time::Duration,
};

use anyhow::{Context, Result, anyhow, bail};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::{
    net::TcpStream,
    sync::mpsc,
    task::JoinHandle,
    time::{Instant, timeout},
};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async,
    tungstenite::{
        Message,
        client::IntoClientRequest,
        http::{HeaderValue, header},
        protocol::frame::coding::CloseCode,
    },
};

use crate::{client, event::Event, member_list};

const URL: &str = "wss://gateway.discord.gg";
/// What the web client asks of the gateway: JSON, compressed as one zlib
/// stream for the whole connection.
const QUERY: &str = "/?encoding=json&v=9&compress=zlib-stream";
/// The capability bits the web client sends, as used by mautrix-discord.
const CAPABILITIES: u64 = 1734653;

const DISPATCH: u64 = 0;
const HEARTBEAT: u64 = 1;
const IDENTIFY: u64 = 2;
const PRESENCE_UPDATE: u64 = 3;
const VOICE_STATE_UPDATE: u64 = 4;
const RESUME: u64 = 6;
const RECONNECT: u64 = 7;
const REQUEST_GUILD_MEMBERS: u64 = 8;
const INVALID_SESSION: u64 = 9;
const HELLO: u64 = 10;
const HEARTBEAT_ACK: u64 = 11;
const STREAM_CREATE: u64 = 18;
const STREAM_DELETE: u64 = 19;
const STREAM_WATCH: u64 = 20;
const STREAM_SET_PAUSED: u64 = 22;
const REQUEST_SOUNDBOARD_SOUNDS: u64 = 31;
const GUILD_SUBSCRIPTIONS_BULK: u64 = 37;

/// Discord closed the gateway, with its close code.
#[derive(Debug)]
pub struct Closed {
    pub code: u16,
    pub reason: String,
}

impl Closed {
    /// Close codes after which reconnecting with the same token can't help.
    pub fn is_final(&self) -> bool {
        // Authentication failed, invalid shard, sharding required, invalid
        // API version, invalid intents, disallowed intents.
        matches!(self.code, 4004 | 4010 | 4011 | 4012 | 4013 | 4014)
    }
}

impl std::fmt::Display for Closed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Discord closed the gateway: {} {}",
            self.code, self.reason
        )
    }
}

impl std::error::Error for Closed {}

impl Closed {
    /// Close codes after which the session can't resume: an unknown sequence
    /// number, or a session that timed out.
    fn ends_session(&self) -> bool {
        self.is_final() || matches!(self.code, 4007 | 4009)
    }
}

/// Discord ended the session, so the next connection must identify again.
#[derive(Debug)]
pub struct Invalidated;

impl std::fmt::Display for Invalidated {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Discord ended the gateway session")
    }
}

impl std::error::Error for Invalidated {}

/// Discord allows 120 gateway commands a minute on a connection; Yapfast keeps
/// a margin for heartbeats.
const SEND_LIMIT: usize = 110;
const SEND_WINDOW: Duration = Duration::from_secs(60);
/// Discord allows 5 presence updates in 20 seconds, which the web client
/// keeps to.
const PRESENCE_LIMIT: usize = 5;

/// How long [`PRESENCE_LIMIT`] updates take: Discord's 20 seconds, or 300 ms
/// in unit tests and against the test server, which has no such limit, so
/// that tests changing the status don't wait out Discord's.
fn presence_window() -> Duration {
    if cfg!(test) || client::test_server().is_some() {
        Duration::from_millis(300)
    } else {
        Duration::from_secs(20)
    }
}
/// The longest Yapfast holds back member requests for a server when Discord
/// asks it to wait.
const MEMBERS_HELD: Duration = Duration::from_secs(5 * 60);
/// How many times a lost connection tries to resume before Yapfast identifies
/// again.
const RESUME_TRIES: u32 = 3;
/// A resume that didn't hold waits this long, doubled with each try so far,
/// and up to this long more, before the next.
#[cfg(not(test))]
const RESUME_WAIT: Duration = Duration::from_secs(1);
#[cfg(test)]
const RESUME_WAIT: Duration = Duration::from_millis(10);
/// How long opening a connection, Discord's HELLO, or sending may take before
/// Yapfast gives up on the connection.
#[cfg(not(test))]
const IO_TIMEOUT: Duration = Duration::from_secs(10);
#[cfg(test)]
const IO_TIMEOUT: Duration = Duration::from_millis(500);

/// The first wait before connecting again, which doubles with each session
/// that fails, up to [`LAST_RETRY`].
const FIRST_RETRY: Duration = Duration::from_secs(2);
const LAST_RETRY: Duration = Duration::from_secs(60);
/// A session up this long counts as working, and the wait starts over.
const STABLE: Duration = Duration::from_secs(60);
/// The most IDENTIFYs Yapfast sends in [`IDENTIFY_WINDOW`]. Discord flags
/// accounts that keep identifying, so Yapfast stops rather than go past this.
const IDENTIFY_LIMIT: usize = 10;
const IDENTIFY_WINDOW: Duration = Duration::from_secs(60 * 60);

/// Yapfast sent [`IDENTIFY_LIMIT`] IDENTIFYs within the hour, and stopped.
#[derive(Debug)]
pub struct TooManyIdentifies;

impl std::fmt::Display for TooManyIdentifies {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Yapfast started too many Discord gateway sessions within the hour")
    }
}

impl std::error::Error for TooManyIdentifies {}

/// When to connect again after a session ends, so a session that keeps
/// failing doesn't identify over and over.
pub struct Reconnects {
    /// The wait before the next connection, before its jitter.
    retry: Duration,
    /// When the IDENTIFYs of the last [`IDENTIFY_WINDOW`] went.
    identifies: VecDeque<Instant>,
}

impl Default for Reconnects {
    fn default() -> Self {
        Self {
            retry: FIRST_RETRY,
            identifies: VecDeque::new(),
        }
    }
}

impl Reconnects {
    /// Counts an IDENTIFY about to go, or refuses it past [`IDENTIFY_LIMIT`].
    fn identify(&mut self, now: Instant) -> Result<()> {
        while self
            .identifies
            .front()
            .is_some_and(|sent| now.duration_since(*sent) >= IDENTIFY_WINDOW)
        {
            self.identifies.pop_front();
        }
        if self.identifies.len() >= IDENTIFY_LIMIT {
            return Err(TooManyIdentifies.into());
        }
        self.identifies.push_back(now);
        Ok(())
    }

    /// How long to wait before connecting again, after a session that was up
    /// for `up`, or never connected, ended with `error`. Waits double until a
    /// session stays up for [`STABLE`], spread so clients don't return
    /// together.
    pub fn wait(&mut self, error: &anyhow::Error, up: Option<Duration>) -> Duration {
        if up.is_some_and(|up| up >= STABLE) {
            self.retry = FIRST_RETRY;
        }
        let mut wait = self.retry.mul_f64(rand::random_range(0.5..1.0));
        if error.downcast_ref::<Invalidated>().is_some() {
            // Discord asks for a random wait of one to five seconds after it
            // ends a session; longer when it keeps ending them.
            wait = wait.max(Duration::from_millis(rand::random_range(1_000..5_000)));
        }
        self.retry = (self.retry * 2).min(LAST_RETRY);
        wait
    }
}

/// A dispatched event, its name parsed once as it arrives.
pub struct Dispatch {
    pub event: Event,
    pub data: Value,
}

/// A command for the connection's task.
enum Command {
    Send(u64, Value),
    /// Close the connection, which ends the session.
    Close,
}

/// The gateway session. Its connection runs in a task of its own, which
/// keeps up the heartbeats and resumes a lost connection, while commands
/// wait their turn under Discord's send limit.
pub struct Gateway {
    commands: mpsc::UnboundedSender<Command>,
    /// Dispatched events, and last, the error that ended the session.
    events: mpsc::UnboundedReceiver<Result<Dispatch>>,
    task: JoinHandle<()>,
    /// Servers whose members Discord said Yapfast asked about too often, with
    /// when it may ask again.
    members_held: HashMap<String, Instant>,
}

impl Drop for Gateway {
    fn drop(&mut self) {
        self.task.abort();
    }
}

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// Opens a gateway connection to `url`, as the web client does.
async fn open(url: &str) -> Result<Socket> {
    let mut request = url.into_client_request()?;
    let headers = request.headers_mut();
    headers.insert(
        header::USER_AGENT,
        HeaderValue::from_static(client::user_agent()),
    );
    headers.insert(
        header::ORIGIN,
        HeaderValue::from_static("https://discord.com"),
    );
    let (socket, _) = timeout(IO_TIMEOUT, connect_async(request))
        .await
        .context("Discord's gateway didn't answer")?
        .context("could not reach Discord's gateway")?;
    Ok(socket)
}

impl Gateway {
    /// Connects and identifies with `presence`, see
    /// [`crate::settings::presence`], returning the gateway and the READY
    /// payload. Refuses with [`TooManyIdentifies`] when `reconnects` has
    /// identified too often.
    pub async fn connect(
        token: &str,
        presence: Value,
        reconnects: &mut Reconnects,
    ) -> Result<(Self, Value)> {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        let url = match client::test_server() {
            Some(server) => {
                let server = server
                    .replacen("https://", "wss://", 1)
                    .replacen("http://", "ws://", 1);
                format!("{server}{QUERY}")
            }
            None => format!("{URL}{QUERY}"),
        };
        let limiter = Limiter::new(SEND_LIMIT, SEND_WINDOW);
        Self::connect_to(&url, token, presence, reconnects, limiter).await
    }

    async fn connect_to(
        url: &str,
        token: &str,
        presence: Value,
        reconnects: &mut Reconnects,
        limiter: Limiter,
    ) -> Result<(Self, Value)> {
        let mut connection = Connection {
            socket: open(url).await?,
            inflater: Inflater::default(),
            sequence: None,
            interval: Duration::from_secs(40),
            next_heartbeat: Instant::now(),
            acked: true,
            token: token.to_owned(),
            session_id: None,
            resume_url: None,
            limiter,
            presences: Limiter::new(PRESENCE_LIMIT, presence_window()),
            presence: None,
            resumes: 0,
        };
        connection.greeted().await?;
        reconnects.identify(Instant::now())?;
        let identify = json!({
            "token": token,
            "capabilities": CAPABILITIES,
            "properties": client::properties(),
            "presence": presence,
            "compress": false,
            "client_state": {"guild_versions": {}},
        });
        connection.write(IDENTIFY, &identify).await?;

        let (commands, commands_received) = mpsc::unbounded_channel();
        let (events_sent, events) = mpsc::unbounded_channel();
        let task = tokio::spawn(connection.run(commands_received, events_sent));
        let mut gateway = Self {
            commands,
            events,
            task,
            members_held: HashMap::new(),
        };
        loop {
            let event = gateway.next().await?;
            if event.event == Event::Ready {
                return Ok((gateway, event.data));
            }
        }
    }

    /// Queues a command, which goes when Discord's send limit allows,
    /// after resuming the session if the connection was lost.
    async fn send(&mut self, op: u64, data: Value) -> Result<()> {
        if self.commands.send(Command::Send(op, data)).is_ok() {
            return Ok(());
        }
        Err(self.ended().await)
    }

    /// The error that ended the session, once its task stopped.
    async fn ended(&mut self) -> anyhow::Error {
        while let Some(event) = self.events.recv().await {
            if let Err(error) = event {
                return error;
            }
        }
        anyhow!("the Discord gateway connection stopped")
    }

    /// Tells Discord this session's status and activities.
    pub async fn update_presence(&mut self, presence: Value) -> Result<()> {
        self.send(PRESENCE_UPDATE, presence).await
    }

    /// Join a voice channel, or leave voice with `channel_id` set to `None`.
    /// Sending it again in the same channel only changes the mute and deafen
    /// state others see.
    pub async fn update_voice_state(
        &mut self,
        guild_id: Option<&str>,
        channel_id: Option<&str>,
        self_mute: bool,
        self_deaf: bool,
        self_video: bool,
    ) -> Result<()> {
        self.send(
            VOICE_STATE_UPDATE,
            json!({
                "guild_id": guild_id,
                "channel_id": channel_id,
                "self_mute": self_mute,
                "self_deaf": self_deaf,
                "self_video": self_video,
            }),
        )
        .await
    }

    /// Subscribes to a server while one of its channels is on screen, as the
    /// web client does. Large servers send their messages only to subscribers.
    /// With `channel`, Discord also sends that channel's member list, its
    /// first [`member_list::ROWS`] rows, as GUILD_MEMBER_LIST_UPDATE.
    pub async fn subscribe(&mut self, guild_id: &str, channel: Option<&str>) -> Result<()> {
        let mut subscription = json!({
            "typing": true,
            "activities": true,
            "threads": true,
        });
        if let Some(channel) = channel {
            subscription["channels"] = json!({channel: [[0, member_list::ROWS - 1]]});
        }
        self.send(
            GUILD_SUBSCRIPTIONS_BULK,
            json!({"subscriptions": {guild_id: subscription}}),
        )
        .await
    }

    /// Starts sharing to the voice channel this account is in, or with no
    /// server, the DM call. Discord answers with STREAM_CREATE and then
    /// STREAM_SERVER_UPDATE.
    pub async fn create_stream(&mut self, guild_id: Option<&str>, channel_id: &str) -> Result<()> {
        self.send(
            STREAM_CREATE,
            json!({
                "type": if guild_id.is_some() { "guild" } else { "call" },
                "guild_id": guild_id,
                "channel_id": channel_id,
                "preferred_region": null,
            }),
        )
        .await
    }

    /// Starts watching someone's stream, answered like [`Self::create_stream`].
    pub async fn watch_stream(&mut self, stream_key: &str) -> Result<()> {
        self.send(STREAM_WATCH, json!({"stream_key": stream_key}))
            .await
    }

    /// Ends this account's own stream, or stops watching someone else's.
    pub async fn delete_stream(&mut self, stream_key: &str) -> Result<()> {
        self.send(STREAM_DELETE, json!({"stream_key": stream_key}))
            .await
    }

    pub async fn pause_stream(&mut self, stream_key: &str, paused: bool) -> Result<()> {
        self.send(
            STREAM_SET_PAUSED,
            json!({"stream_key": stream_key, "paused": paused}),
        )
        .await
    }

    /// Asks for these servers' soundboard sounds, which Discord sends as one
    /// SOUNDBOARD_SOUNDS for each.
    pub async fn request_soundboard_sounds(&mut self, guild_ids: &[String]) -> Result<()> {
        self.send(REQUEST_SOUNDBOARD_SOUNDS, json!({"guild_ids": guild_ids}))
            .await
    }

    /// Whether Discord asked Yapfast to wait before asking about `guild_id`'s
    /// members again; requests until then aren't sent.
    pub fn members_held(&mut self, guild_id: &str) -> bool {
        let now = Instant::now();
        self.members_held.retain(|_, until| *until > now);
        self.members_held.contains_key(guild_id)
    }

    /// Asks for these members' server profiles, which arrive as GUILD_MEMBERS_CHUNK.
    pub async fn request_members(
        &mut self,
        guild_id: &str,
        user_ids: &[String],
        nonce: Option<&str>,
    ) -> Result<()> {
        if self.members_held(guild_id) {
            return Ok(());
        }
        let mut request = json!({"guild_id": [guild_id], "user_ids": user_ids, "presences": false});
        if let Some(nonce) = nonce {
            request["nonce"] = nonce.into();
        }
        self.send(REQUEST_GUILD_MEMBERS, request).await
    }

    /// Asks for up to `limit` members whose names start with `query`; they arrive as a
    /// GUILD_MEMBERS_CHUNK carrying `nonce`.
    pub async fn search_members(
        &mut self,
        guild_id: &str,
        query: &str,
        limit: u64,
        nonce: &str,
    ) -> Result<()> {
        if self.members_held(guild_id) {
            return Ok(());
        }
        self.send(
            REQUEST_GUILD_MEMBERS,
            json!({
                "guild_id": [guild_id],
                "query": query,
                "limit": limit,
                "presences": false,
                "nonce": nonce,
            }),
        )
        .await
    }

    /// The next dispatched event; an error ends the session.
    pub async fn next(&mut self) -> Result<Dispatch> {
        let dispatch = match self.events.recv().await {
            Some(event) => event?,
            None => return Err(anyhow!("the Discord gateway connection stopped")),
        };
        if dispatch.event == Event::RateLimited {
            self.rate_limited(&dispatch.data);
        }
        Ok(dispatch)
    }

    /// Discord refused a command for being sent too often. Only member
    /// requests are limited this way, per server; Yapfast holds them back for
    /// as long as Discord asks.
    fn rate_limited(&mut self, data: &Value) {
        let opcode = data["opcode"].as_u64();
        let wait = data["retry_after"].as_f64().unwrap_or(0.0);
        tracing::warn!(
            ?opcode,
            wait,
            "Discord's gateway asked Yapfast to slow down"
        );
        if opcode != Some(REQUEST_GUILD_MEMBERS) {
            return;
        }
        let Some(guild) = data["meta"]["guild_id"].as_str() else {
            return;
        };
        let wait = crate::limits::sent_seconds(wait, Duration::from_secs(1), MEMBERS_HELD);
        self.members_held
            .insert(guild.to_owned(), Instant::now() + wait);
    }

    /// Closes the connection, which ends the session.
    pub async fn close(mut self) {
        if self.commands.send(Command::Close).is_ok() {
            let _ = timeout(IO_TIMEOUT, &mut self.task).await;
        }
    }
}

/// Keeps commands under Discord's limit on what a connection sends.
struct Limiter {
    limit: usize,
    window: Duration,
    /// When the commands of the last window went.
    sent: VecDeque<Instant>,
}

impl Limiter {
    fn new(limit: usize, window: Duration) -> Self {
        Self {
            limit,
            window,
            sent: VecDeque::new(),
        }
    }

    /// When the next command may go.
    fn free_at(&mut self, now: Instant) -> Instant {
        while self
            .sent
            .front()
            .is_some_and(|sent| now.duration_since(*sent) >= self.window)
        {
            self.sent.pop_front();
        }
        match self.sent.front() {
            Some(oldest) if self.sent.len() >= self.limit => *oldest + self.window,
            _ => now,
        }
    }
}

/// One connection to the gateway, and the session it carries on.
struct Connection {
    socket: Socket,
    inflater: Inflater,
    sequence: Option<u64>,
    interval: Duration,
    next_heartbeat: Instant,
    /// Discord acknowledged the last heartbeat; a connection that stops
    /// answering is dead, and resumes.
    acked: bool,
    token: String,
    /// The session, and where it resumes, from READY.
    session_id: Option<String>,
    resume_url: Option<String>,
    limiter: Limiter,
    /// Presence updates, which Discord allows 5 of in 20 seconds; the
    /// latest waits, replacing any before it, while that's used up.
    presences: Limiter,
    presence: Option<Value>,
    /// Resumes tried since Discord last confirmed one.
    resumes: u32,
}

impl Connection {
    /// Carries the session until the gateway closes it or it ends; then
    /// passes on why.
    async fn run(
        mut self,
        mut commands: mpsc::UnboundedReceiver<Command>,
        events: mpsc::UnboundedSender<Result<Dispatch>>,
    ) {
        // A command waiting for the send limit, or for the connection to
        // resume.
        let mut held = None;
        loop {
            let lost = match self.serve(&mut commands, &events, &mut held).await {
                Ok(()) => return,
                Err(error) => error,
            };
            if let Err(error) = self.resume(lost).await {
                let _ = events.send(Err(error));
                return;
            }
        }
    }

    /// Sends commands and heartbeats and passes on events until the gateway
    /// closes the connection, or it's lost.
    async fn serve(
        &mut self,
        commands: &mut mpsc::UnboundedReceiver<Command>,
        events: &mpsc::UnboundedSender<Result<Dispatch>>,
        held: &mut Option<(u64, Value)>,
    ) -> Result<()> {
        loop {
            let now = Instant::now();
            let free_at = self.limiter.free_at(now);
            let presence_at = self.presences.free_at(now).max(free_at);
            tokio::select! {
                () = tokio::time::sleep_until(self.next_heartbeat) => {
                    if !self.acked {
                        bail!("Discord's gateway stopped answering heartbeats");
                    }
                    self.write(HEARTBEAT, &json!(self.sequence)).await?;
                    self.acked = false;
                    self.next_heartbeat += self.interval;
                }
                () = tokio::time::sleep_until(free_at), if held.is_some() => {
                    if let Some((op, data)) = held.as_ref() {
                        self.write(*op, data).await?;
                    }
                    *held = None;
                }
                () = tokio::time::sleep_until(presence_at), if self.presence.is_some() => {
                    if let Some(presence) = self.presence.take() {
                        self.presences.sent.push_back(Instant::now());
                        self.write(PRESENCE_UPDATE, &presence).await?;
                    }
                }
                command = commands.recv(), if held.is_none() => match command {
                    Some(Command::Send(PRESENCE_UPDATE, data)) => self.presence = Some(data),
                    Some(Command::Send(op, data)) => {
                        if free_at > Instant::now() {
                            tracing::warn!("holding back a gateway command to stay under Discord's limit");
                        }
                        *held = Some((op, data));
                    }
                    Some(Command::Close) | None => {
                        let _ = timeout(IO_TIMEOUT, self.socket.close(None)).await;
                        return Ok(());
                    }
                },
                message = self.receive() => {
                    if let Some(dispatch) = self.handle(message?).await? {
                        let _ = events.send(Ok(dispatch));
                    }
                }
            }
        }
    }

    /// Acts on a message from the gateway, returning it if it's an event.
    async fn handle(&mut self, mut message: Value) -> Result<Option<Dispatch>> {
        if let Some(sequence) = message["s"].as_u64() {
            self.sequence = Some(sequence);
        }
        match message["op"].as_u64() {
            Some(DISPATCH) => {
                let event = Event::parse(message["t"].as_str().unwrap_or_default());
                let data = message.get_mut("d").map(Value::take).unwrap_or(Value::Null);
                match event {
                    Event::Ready => {
                        if let Some(token) = crate::ready::auth_token(&data) {
                            self.token = token.to_owned();
                        }
                        self.session_id = data["session_id"].as_str().map(str::to_owned);
                        self.resume_url = data["resume_gateway_url"]
                            .as_str()
                            .filter(|url| secure(url))
                            .map(str::to_owned);
                    }
                    Event::Resumed => {
                        tracing::info!("resumed the Discord gateway session");
                        self.resumes = 0;
                    }
                    _ => {}
                }
                return Ok(Some(Dispatch { event, data }));
            }
            Some(HEARTBEAT) => self.write(HEARTBEAT, &json!(self.sequence)).await?,
            Some(HEARTBEAT_ACK) => self.acked = true,
            Some(RECONNECT) => bail!("Discord asked the gateway to reconnect"),
            // `d` says whether the session may resume.
            Some(INVALID_SESSION) if message["d"] == true => {
                bail!("Discord asked the gateway to resume")
            }
            Some(INVALID_SESSION) => {
                self.session_id = None;
                return Err(Invalidated.into());
            }
            _ => {}
        }
        Ok(None)
    }

    /// Sends a command on this connection, counting it against the send
    /// limit unless it's a heartbeat.
    async fn write(&mut self, op: u64, data: &Value) -> Result<()> {
        if op != HEARTBEAT {
            self.limiter.sent.push_back(Instant::now());
        }
        let payload = json!({"op": op, "d": data}).to_string();
        timeout(IO_TIMEOUT, self.socket.send(Message::text(payload)))
            .await
            .context("Discord's gateway stopped taking commands")?
            .context("could not send to Discord's gateway")
    }

    /// Reads HELLO and sets the heartbeat from it.
    async fn greeted(&mut self) -> Result<()> {
        let hello = timeout(IO_TIMEOUT, self.receive())
            .await
            .context("Discord's gateway didn't greet")??;
        if hello["op"] != HELLO {
            bail!("Discord's gateway didn't greet as expected");
        }
        let interval = hello["d"]["heartbeat_interval"]
            .as_u64()
            .context("Discord's gateway sent no heartbeat interval")?;
        self.interval = crate::limits::sent_seconds(
            interval as f64 / 1000.0,
            Duration::from_millis(100),
            Duration::from_secs(300),
        );
        // Discord asks for the first heartbeat after a random part of the interval.
        self.next_heartbeat = Instant::now() + self.interval.mul_f64(rand::random::<f64>());
        self.acked = true;
        Ok(())
    }

    /// Connects again and resumes the session after the connection was
    /// `lost`; Discord then replays what was missed, ending with RESUMED.
    /// Passes on `lost` when the session can't resume.
    async fn resume(&mut self, lost: anyhow::Error) -> Result<()> {
        let resumable = lost.downcast_ref::<Invalidated>().is_none()
            && !lost
                .downcast_ref::<Closed>()
                .is_some_and(Closed::ends_session);
        let (Some(session_id), Some(resume_url), true) =
            (self.session_id.clone(), self.resume_url.clone(), resumable)
        else {
            return Err(lost);
        };
        tracing::info!(error = %lost, "resuming the Discord gateway session");
        let url = format!("{}{QUERY}", resume_url.trim_end_matches('/'));
        loop {
            if self.resumes >= RESUME_TRIES {
                return Err(lost.context("could not resume the gateway session"));
            }
            // A resume that didn't hold waits longer before the next.
            if self.resumes > 0 {
                let wait = RESUME_WAIT * 2u32.pow(self.resumes)
                    + rand::random_range(Duration::ZERO..RESUME_WAIT);
                tokio::time::sleep(wait).await;
            }
            self.resumes += 1;
            match self.reconnect(&url, &session_id).await {
                Ok(()) => return Ok(()),
                Err(error) => tracing::warn!(%error, "could not resume the gateway session"),
            }
        }
    }

    async fn reconnect(&mut self, url: &str, session_id: &str) -> Result<()> {
        // Dropping the old connection without a close frame keeps the
        // session; a normal close would end it.
        self.socket = open(url).await?;
        self.inflater = Inflater::default();
        self.limiter.sent.clear();
        self.greeted().await?;
        let resume = json!({
            "token": self.token,
            "session_id": session_id,
            "seq": self.sequence,
        });
        self.write(RESUME, &resume).await
    }

    async fn receive(&mut self) -> Result<Value> {
        loop {
            let message = self
                .socket
                .next()
                .await
                .ok_or_else(|| anyhow!("Discord closed the gateway"))?
                .context("lost the connection to Discord's gateway")?;
            match message {
                Message::Text(text) => {
                    return serde_json::from_str(&text)
                        .context("could not understand Discord's gateway");
                }
                Message::Binary(data) => {
                    if let Some(payload) = self.inflater.push(&data)? {
                        return Ok(payload);
                    }
                }
                Message::Close(frame) => {
                    let (code, reason) = frame
                        .map(|frame| (frame.code, frame.reason.to_string()))
                        .unwrap_or((CloseCode::Status, String::new()));
                    return Err(Closed {
                        code: u16::from(code),
                        reason,
                    }
                    .into());
                }
                _ => {}
            }
        }
    }
}

/// The gateway's zlib-stream compression: one zlib stream through the whole
/// connection, each payload ending with a sync flush, so a payload may come
/// in more than one message.
struct Inflater {
    zlib: flate2::Decompress,
    /// A payload's start, until its last message comes.
    partial: Vec<u8>,
    inflated: Vec<u8>,
}

/// The end of each payload: zlib's sync flush marker.
const FLUSHED: [u8; 4] = [0x00, 0x00, 0xff, 0xff];
/// The most one payload may inflate to. READY is the largest, a few MiB on
/// an account in many servers.
const MAX_PAYLOAD: usize = 256 << 20;
/// Buffers larger than this go after the payload that needed them, rather
/// than staying for the connection's life.
const KEPT_BUFFER: usize = 256 << 10;

impl Default for Inflater {
    fn default() -> Self {
        Self {
            zlib: flate2::Decompress::new(true),
            partial: Vec::new(),
            inflated: Vec::new(),
        }
    }
}

impl Inflater {
    /// Takes the next message, returning the payload it completes.
    fn push(&mut self, data: &[u8]) -> Result<Option<Value>> {
        let compressed = if self.partial.is_empty() && data.ends_with(&FLUSHED) {
            data
        } else {
            if self.partial.len() + data.len() > MAX_PAYLOAD {
                bail!("Discord's gateway sent too much at once");
            }
            self.partial.extend_from_slice(data);
            if !self.partial.ends_with(&FLUSHED) {
                return Ok(None);
            }
            &self.partial
        };
        let inflated = inflate(&mut self.zlib, compressed, &mut self.inflated);
        let payload = inflated.and_then(|()| {
            serde_json::from_slice(&self.inflated).context("could not understand Discord's gateway")
        });
        self.partial.clear();
        self.inflated.clear();
        for buffer in [&mut self.partial, &mut self.inflated] {
            if buffer.capacity() > KEPT_BUFFER {
                *buffer = Vec::new();
            }
        }
        payload.map(Some)
    }
}

/// Inflates all of `compressed` into `out`.
fn inflate(zlib: &mut flate2::Decompress, mut compressed: &[u8], out: &mut Vec<u8>) -> Result<()> {
    loop {
        if out.len() == out.capacity() {
            if out.len() >= MAX_PAYLOAD {
                bail!("Discord's gateway sent too much at once");
            }
            out.reserve((compressed.len() * 4).clamp(16 << 10, MAX_PAYLOAD - out.len()));
        }
        let (read, written) = (zlib.total_in(), out.len());
        zlib.decompress_vec(compressed, out, flate2::FlushDecompress::Sync)
            .context("could not inflate what Discord's gateway sent")?;
        let used = (zlib.total_in() - read) as usize;
        compressed = &compressed[used..];
        let room = out.len() < out.capacity();
        // Done once the input is used up and zlib had room to spare.
        if compressed.is_empty() && room {
            return Ok(());
        }
        if used == 0 && out.len() == written && room {
            bail!("Discord's gateway sent a compressed stream that stopped");
        }
    }
}

/// Whether a gateway address keeps the token it's sent private: TLS, or a
/// test server on this machine.
fn secure(url: &str) -> bool {
    let Ok(url) = url::Url::parse(url) else {
        return false;
    };
    match url.scheme() {
        "wss" => true,
        "ws" => match url.host() {
            Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
            Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
            Some(url::Host::Domain(domain)) => domain == "localhost",
            None => false,
        },
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;
    use tokio_tungstenite::tungstenite::protocol::CloseFrame;

    /// Discord's gateway, played in the test.
    struct Discord {
        listener: TcpListener,
        url: String,
    }

    impl Discord {
        async fn new() -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let url = format!("ws://{}", listener.local_addr().unwrap());
            Self { listener, url }
        }

        /// Takes the next connection, greeting it with a heartbeat interval.
        async fn accept(&self, interval: Duration) -> Peer {
            let (stream, _) = self.listener.accept().await.unwrap();
            let socket = tokio_tungstenite::accept_async(stream).await.unwrap();
            let mut peer = Peer {
                socket,
                sequence: 0,
                heartbeats: 0,
            };
            let interval = u64::try_from(interval.as_millis()).unwrap();
            peer.send(json!({"op": HELLO, "d": {"heartbeat_interval": interval}}))
                .await;
            peer
        }

        /// A session that's READY, with the connection Discord holds.
        async fn session(&self, interval: Duration, limiter: Limiter) -> (Gateway, Peer) {
            let mut reconnects = Reconnects::default();
            let (connected, peer) = tokio::join!(
                Gateway::connect_to(&self.url, "token", json!({}), &mut reconnects, limiter),
                async {
                    let mut peer = self.accept(interval).await;
                    assert_eq!(peer.command().await["op"], IDENTIFY);
                    let ready = json!({"session_id": "session", "resume_gateway_url": self.url});
                    peer.dispatch("READY", ready).await;
                    peer
                },
            );
            let (gateway, ready) = connected.unwrap();
            assert_eq!(ready["session_id"], "session");
            (gateway, peer)
        }
    }

    #[tokio::test]
    async fn dispatch_moves_the_payload_without_copying_it() {
        let discord = Discord::new().await;
        let (socket, _peer) = tokio::join!(open(&discord.url), discord.accept(SLOW_HEARTBEAT));
        let mut connection = Connection {
            socket: socket.unwrap(),
            inflater: Inflater::default(),
            sequence: None,
            interval: SLOW_HEARTBEAT,
            next_heartbeat: Instant::now(),
            acked: true,
            token: "token".into(),
            session_id: None,
            resume_url: None,
            limiter: limiter(),
            presences: Limiter::new(PRESENCE_LIMIT, presence_window()),
            presence: None,
            resumes: 0,
        };
        let message = json!({
            "op": DISPATCH,
            "s": 2,
            "t": "READY_SUPPLEMENTAL",
            "d": { "large": "x".repeat(1024 * 1024) },
        });
        let address = message["d"]["large"].as_str().unwrap().as_ptr();
        let dispatch = connection.handle(message).await.unwrap().unwrap();
        assert_eq!(dispatch.data["large"].as_str().unwrap().as_ptr(), address);
        assert_eq!(connection.sequence, Some(2));
    }

    /// One connection, from Discord's side.
    struct Peer {
        socket: WebSocketStream<TcpStream>,
        sequence: u64,
        /// Heartbeats received and acknowledged.
        heartbeats: usize,
    }

    impl Peer {
        async fn send(&mut self, message: Value) {
            self.socket
                .send(Message::text(message.to_string()))
                .await
                .unwrap();
        }

        async fn dispatch(&mut self, name: &str, data: Value) {
            self.sequence += 1;
            let message = json!({"op": DISPATCH, "s": self.sequence, "t": name, "d": data});
            self.send(message).await;
        }

        async fn close(&mut self, code: u16) {
            let frame = CloseFrame {
                code: CloseCode::from(code),
                reason: "test".into(),
            };
            self.socket.close(Some(frame)).await.unwrap();
        }

        /// The next command other than a heartbeat; heartbeats are
        /// acknowledged and counted.
        async fn command(&mut self) -> Value {
            loop {
                let message = timeout(Duration::from_secs(5), self.socket.next())
                    .await
                    .expect("Yapfast sent nothing")
                    .expect("Yapfast closed the connection")
                    .unwrap();
                let Message::Text(text) = message else {
                    continue;
                };
                let command: Value = serde_json::from_str(&text).unwrap();
                if command["op"] != HEARTBEAT {
                    return command;
                }
                self.heartbeats += 1;
                self.send(json!({"op": HEARTBEAT_ACK})).await;
            }
        }
    }

    const SLOW_HEARTBEAT: Duration = Duration::from_secs(40);

    fn limiter() -> Limiter {
        Limiter::new(SEND_LIMIT, SEND_WINDOW)
    }

    async fn next(gateway: &mut Gateway) -> Result<Dispatch> {
        timeout(Duration::from_secs(5), gateway.next())
            .await
            .expect("no event came")
    }

    #[tokio::test]
    async fn resumes_a_lost_connection_where_it_left_off() {
        let discord = Discord::new().await;
        let (mut gateway, mut peer) = discord.session(SLOW_HEARTBEAT, limiter()).await;
        peer.dispatch("MESSAGE_CREATE", json!({"id": "1"})).await;
        assert_eq!(
            next(&mut gateway).await.unwrap().event,
            Event::MessageCreate
        );

        // The connection drops without a close frame.
        drop(peer);
        let mut peer = discord.accept(SLOW_HEARTBEAT).await;
        let resume = peer.command().await;
        assert_eq!(resume["op"], RESUME);
        assert_eq!(resume["d"]["session_id"], "session");
        assert_eq!(resume["d"]["seq"], 2, "READY, then the message");
        peer.sequence = 2;
        peer.dispatch("MESSAGE_CREATE", json!({"id": "2"})).await;
        peer.dispatch("RESUMED", json!({})).await;
        let missed = next(&mut gateway).await.unwrap();
        assert_eq!(missed.data["id"], "2");
        assert_eq!(next(&mut gateway).await.unwrap().event, Event::Resumed);

        // Commands go on the new connection.
        gateway.subscribe("guild", None).await.unwrap();
        assert_eq!(peer.command().await["op"], GUILD_SUBSCRIPTIONS_BULK);

        // Discord's RECONNECT resumes too.
        peer.send(json!({"op": RECONNECT})).await;
        let mut peer = discord.accept(SLOW_HEARTBEAT).await;
        assert_eq!(peer.command().await["op"], RESUME);
    }

    #[tokio::test]
    async fn resumes_or_ends_an_invalid_session_as_discord_says() {
        let discord = Discord::new().await;
        let (mut gateway, mut peer) = discord.session(SLOW_HEARTBEAT, limiter()).await;
        peer.send(json!({"op": INVALID_SESSION, "d": true})).await;
        let mut peer = discord.accept(SLOW_HEARTBEAT).await;
        assert_eq!(peer.command().await["op"], RESUME);

        // Discord won't resume it: the session ends, to identify again.
        peer.send(json!({"op": INVALID_SESSION, "d": false})).await;
        let error = next(&mut gateway).await.err().unwrap();
        assert!(error.downcast_ref::<Invalidated>().is_some(), "{error:#}");
        // Later commands say why.
        let error = gateway.subscribe("guild", None).await.unwrap_err();
        assert!(error.to_string().contains("stopped"), "{error:#}");
    }

    #[test]
    fn close_codes_resume_identify_again_or_end() {
        let closed = |code| Closed {
            code,
            reason: String::new(),
        };
        for code in [1000, 1001, 1006, 4000, 4001, 4002, 4003, 4005, 4008] {
            assert!(!closed(code).ends_session(), "{code} resumes");
        }
        for code in [4007, 4009] {
            assert!(closed(code).ends_session(), "{code} identifies again");
            assert!(!closed(code).is_final(), "{code} identifies again");
        }
        for code in [4004, 4010, 4011, 4012, 4013, 4014] {
            assert!(closed(code).is_final(), "{code} stops");
        }
    }

    #[tokio::test]
    async fn close_codes_reach_the_session_when_it_cant_resume() {
        let discord = Discord::new().await;

        // A close that resumes.
        let (mut gateway, mut peer) = discord.session(SLOW_HEARTBEAT, limiter()).await;
        peer.close(4000).await;
        let mut peer = discord.accept(SLOW_HEARTBEAT).await;
        assert_eq!(peer.command().await["op"], RESUME);

        // A timed out session identifies again.
        peer.close(4009).await;
        let error = next(&mut gateway).await.err().unwrap();
        let closed = error.downcast_ref::<Closed>().expect("closed");
        assert_eq!(closed.code, 4009);
        assert!(!closed.is_final());

        // A refused login stops.
        let (mut gateway, mut peer) = discord.session(SLOW_HEARTBEAT, limiter()).await;
        peer.close(4004).await;
        let error = next(&mut gateway).await.err().unwrap();
        assert!(error.downcast_ref::<Closed>().is_some_and(Closed::is_final));
    }

    #[tokio::test]
    async fn a_session_that_cant_resume_ends_after_a_few_tries() {
        let discord = Discord::new().await;
        let (mut gateway, peer) = discord.session(SLOW_HEARTBEAT, limiter()).await;
        drop(peer);
        // The first try goes at once and is refused; the rest find no answer.
        let mut peer = discord.accept(SLOW_HEARTBEAT).await;
        assert_eq!(peer.command().await["op"], RESUME);
        drop(peer);
        drop(discord);
        let error = timeout(Duration::from_secs(20), gateway.next())
            .await
            .expect("the session still waits")
            .err()
            .unwrap();
        assert!(
            format!("{error:#}").contains("could not resume"),
            "{error:#}"
        );
    }

    #[tokio::test]
    async fn opening_and_hello_time_out() {
        // A server that takes the connection and says nothing.
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}", listener.local_addr().unwrap());
        let silent = tokio::spawn(async move {
            let (_stream, _) = listener.accept().await.unwrap();
            std::future::pending::<()>().await;
        });
        let started = Instant::now();
        let error = Gateway::connect_to(&url, "", json!({}), &mut Reconnects::default(), limiter())
            .await
            .err()
            .unwrap();
        assert!(error.to_string().contains("didn't answer"), "{error:#}");
        assert!(started.elapsed() < IO_TIMEOUT * 2);
        silent.abort();

        // One that opens it and never greets.
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}", listener.local_addr().unwrap());
        let silent = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let _socket = tokio_tungstenite::accept_async(stream).await.unwrap();
            std::future::pending::<()>().await;
        });
        let error = Gateway::connect_to(&url, "", json!({}), &mut Reconnects::default(), limiter())
            .await
            .err()
            .unwrap();
        assert!(error.to_string().contains("didn't greet"), "{error:#}");
        silent.abort();
    }

    #[test]
    fn the_limiter_holds_commands_past_the_limit() {
        let mut limiter = Limiter::new(2, Duration::from_secs(1));
        let now = Instant::now();
        assert_eq!(limiter.free_at(now), now);
        limiter.sent.push_back(now);
        assert_eq!(limiter.free_at(now), now);
        limiter.sent.push_back(now + Duration::from_millis(100));
        assert_eq!(limiter.free_at(now), now + Duration::from_secs(1));
        let later = now + Duration::from_secs(1);
        assert_eq!(limiter.free_at(later), later, "the oldest one aged out");
        assert_eq!(limiter.sent.len(), 1);
    }

    #[tokio::test]
    async fn heartbeats_go_while_commands_wait_for_the_limit() {
        let discord = Discord::new().await;
        let window = Duration::from_millis(800);
        let interval = Duration::from_millis(100);
        // IDENTIFY took one of the two.
        let (mut gateway, mut peer) = discord.session(interval, Limiter::new(2, window)).await;
        let started = Instant::now();
        for guild in ["a", "b", "c"] {
            gateway.subscribe(guild, None).await.unwrap();
        }
        assert!(
            started.elapsed() < Duration::from_millis(50),
            "sending waits"
        );

        let first = peer.command().await;
        assert_eq!(first["d"]["subscriptions"]["a"]["typing"], true);
        let heartbeats = peer.heartbeats;
        let second = peer.command().await;
        assert!(second["d"]["subscriptions"]["b"].is_object());
        assert!(
            started.elapsed() >= window / 2,
            "the second waited for IDENTIFY's to age out"
        );
        assert!(
            peer.heartbeats >= heartbeats + 2,
            "{} heartbeats went meanwhile",
            peer.heartbeats - heartbeats
        );
        let third = peer.command().await;
        assert!(third["d"]["subscriptions"]["c"].is_object());

        // The connection stayed up through it all.
        peer.dispatch("TYPING_START", json!({})).await;
        assert_eq!(next(&mut gateway).await.unwrap().event, Event::TypingStart);

        // An event Yapfast doesn't know passes through by name, with its data.
        peer.dispatch("GUILD_SCHEDULED_EVENT_CREATE", json!({"id": "7"}))
            .await;
        let other = next(&mut gateway).await.unwrap();
        assert_eq!(
            other.event,
            Event::Other("GUILD_SCHEDULED_EVENT_CREATE".into())
        );
        assert_eq!(other.data["id"], "7");
    }

    #[tokio::test]
    async fn unanswered_heartbeats_resume_the_session() {
        let discord = Discord::new().await;
        let interval = Duration::from_millis(100);
        let (_gateway, mut peer) = discord.session(interval, limiter()).await;
        let heartbeat = timeout(Duration::from_secs(5), peer.socket.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(heartbeat.to_text().unwrap().contains(r#""op":1"#));
        // No acknowledgement: by the next one, Yapfast connects again.
        let mut peer = discord.accept(SLOW_HEARTBEAT).await;
        assert_eq!(peer.command().await["op"], RESUME);
    }

    #[test]
    fn reconnects_wait_longer_until_a_session_holds() {
        let mut reconnects = Reconnects::default();
        let lost = anyhow!("lost");
        let between = |wait: Duration, low: u64, high: u64| {
            assert!(
                wait >= Duration::from_millis(low) && wait <= Duration::from_millis(high),
                "{wait:?} not in {low}..={high} ms"
            );
        };
        between(reconnects.wait(&lost, None), 1_000, 2_000);
        between(reconnects.wait(&lost, None), 2_000, 4_000);
        // A session that ended within the minute doesn't start over.
        between(
            reconnects.wait(&lost, Some(Duration::from_secs(5))),
            4_000,
            8_000,
        );
        for _ in 0..10 {
            reconnects.wait(&lost, Some(Duration::from_secs(5)));
        }
        between(reconnects.wait(&lost, None), 30_000, 60_000);
        // One that held for a minute does.
        between(reconnects.wait(&lost, Some(STABLE)), 1_000, 2_000);

        // Ended sessions wait Discord's one to five seconds at first, then
        // longer.
        let mut reconnects = Reconnects::default();
        let invalidated = anyhow::Error::new(Invalidated);
        between(
            reconnects.wait(&invalidated, Some(Duration::ZERO)),
            1_000,
            5_000,
        );
        // 4, 8 and 16 s.
        for _ in 0..3 {
            reconnects.wait(&invalidated, Some(Duration::ZERO));
        }
        between(
            reconnects.wait(&invalidated, Some(Duration::ZERO)),
            16_000,
            32_000,
        );
    }

    #[test]
    fn identifies_stop_at_the_hourly_limit() {
        let mut reconnects = Reconnects::default();
        let now = Instant::now();
        for minute in 0..IDENTIFY_LIMIT as u64 {
            reconnects
                .identify(now + Duration::from_secs(minute * 60))
                .unwrap();
        }
        let refused = reconnects
            .identify(now + Duration::from_secs(30 * 60))
            .unwrap_err();
        assert!(refused.downcast_ref::<TooManyIdentifies>().is_some());
        // An hour after the first, one more may go.
        reconnects.identify(now + IDENTIFY_WINDOW).unwrap();
        assert!(reconnects.identify(now + IDENTIFY_WINDOW).is_err());
    }

    #[tokio::test]
    async fn connecting_counts_its_identify() {
        let discord = Discord::new().await;
        let mut reconnects = Reconnects::default();
        let now = Instant::now();
        for _ in 0..IDENTIFY_LIMIT {
            reconnects.identify(now).unwrap();
        }
        let accept = async {
            // Yapfast opens the connection, then sends nothing.
            let mut peer = discord.accept(SLOW_HEARTBEAT).await;
            timeout(Duration::from_millis(200), peer.socket.next()).await
        };
        let (connected, sent) = tokio::join!(
            Gateway::connect_to(&discord.url, "", json!({}), &mut reconnects, limiter()),
            accept,
        );
        let error = connected.err().unwrap();
        assert!(
            error.downcast_ref::<TooManyIdentifies>().is_some(),
            "{error:#}"
        );
        assert!(
            !matches!(sent, Ok(Some(Ok(Message::Text(_))))),
            "no IDENTIFY went"
        );
    }

    #[tokio::test]
    async fn member_requests_wait_as_long_as_discord_asks() {
        let discord = Discord::new().await;
        let (mut gateway, mut peer) = discord.session(SLOW_HEARTBEAT, limiter()).await;
        let users = ["1".to_owned()];
        gateway.request_members("a", &users, None).await.unwrap();
        assert_eq!(peer.command().await["op"], REQUEST_GUILD_MEMBERS);

        peer.dispatch(
            "RATE_LIMITED",
            json!({"opcode": REQUEST_GUILD_MEMBERS, "retry_after": 30.5, "meta": {"guild_id": "a"}}),
        )
        .await;
        assert_eq!(next(&mut gateway).await.unwrap().event, Event::RateLimited);
        assert!(gateway.members_held("a"));
        assert!(!gateway.members_held("b"));

        // Server a's request is held back; b's goes.
        gateway.request_members("a", &users, None).await.unwrap();
        gateway.search_members("a", "bo", 10, "n").await.unwrap();
        gateway.request_members("b", &users, None).await.unwrap();
        assert_eq!(peer.command().await["d"]["guild_id"], json!(["b"]));
    }

    #[tokio::test]
    async fn presence_updates_keep_to_discords_limit_and_the_latest_goes() {
        let discord = Discord::new().await;
        let (mut gateway, mut peer) = discord.session(SLOW_HEARTBEAT, limiter()).await;
        for n in 0..PRESENCE_LIMIT {
            gateway.update_presence(json!({"n": n})).await.unwrap();
            assert_eq!(peer.command().await["d"]["n"], n);
        }
        // Past the limit, the later replaces the earlier while both wait.
        let held = Instant::now();
        gateway.update_presence(json!({"n": "held"})).await.unwrap();
        gateway
            .update_presence(json!({"n": "latest"}))
            .await
            .unwrap();
        assert_eq!(peer.command().await["d"]["n"], "latest");
        assert!(held.elapsed() >= presence_window() / 2);
    }

    /// Compresses payloads as Discord's gateway does, sharing one stream.
    fn deflated(zlib: &mut flate2::Compress, payload: &Value) -> Vec<u8> {
        let input = payload.to_string();
        let mut out = Vec::with_capacity(input.len() + 64);
        zlib.compress_vec(input.as_bytes(), &mut out, flate2::FlushCompress::Sync)
            .unwrap();
        assert!(out.ends_with(&FLUSHED));
        out
    }

    #[test]
    fn inflates_payloads_sharing_one_stream() {
        let mut zlib = flate2::Compress::new(flate2::Compression::default(), true);
        let mut inflater = Inflater::default();
        let hello = json!({"op": HELLO, "d": {"heartbeat_interval": 41250}});
        let first = deflated(&mut zlib, &hello);
        assert_eq!(inflater.push(&first).unwrap(), Some(hello));

        // A large payload, split across messages, refers back to the first.
        let ready = json!({"op": DISPATCH, "t": "READY", "d": {"guilds": vec!["heartbeat_interval"; 20_000]}});
        let second = deflated(&mut zlib, &ready);
        let (start, end) = second.split_at(second.len() / 2);
        assert_eq!(inflater.push(start).unwrap(), None);
        assert_eq!(inflater.push(end).unwrap(), Some(ready));
        assert_eq!(inflater.inflated.capacity(), 0, "a large buffer stays");

        assert!(inflater.push(b"not zlib\x00\x00\xff\xff").is_err());
    }
}
