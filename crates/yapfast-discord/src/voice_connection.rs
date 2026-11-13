//! A connection to a Discord voice server, as calls and Go Live streams both
//! make one: the voice gateway, version 8 as Discord's clients use it; UDP
//! with Discord's transport encryption; and DAVE for what's end-to-end
//! encrypted. `call_connection.rs` and `stream_connection.rs` run the media.

use std::{net::IpAddr, sync::Mutex, time::Duration};

use anyhow::{Context, Result, anyhow, bail};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::{Value, json};
use serenity_voice_model::{Event, serialize_binary_event};
use tokio::{
    net::{TcpStream, UdpSocket},
    time::timeout,
};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, tungstenite::Message};

use crate::{
    dave::{Dave, Out},
    rtp::{Codec, Transport},
    websocket,
};

/// The voice gateway version Discord's clients use, as Discord-video-stream
/// does.
const VERSION: u8 = 8;
pub(crate) const HANDSHAKE: Duration = Duration::from_secs(15);

pub(crate) fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The voice gateway's address. A test server on this machine may give a
/// plain WebSocket address.
fn gateway_url(endpoint: &str) -> Result<reqwest::Url> {
    if let Some(local) = endpoint.strip_prefix("ws://") {
        let url = reqwest::Url::parse(&format!("ws://{local}/?v={VERSION}"))?;
        return match url.host() {
            Some(url::Host::Ipv4(ip)) if ip.is_loopback() => Ok(url),
            Some(url::Host::Domain("localhost")) => Ok(url),
            _ => bail!("Discord gave a voice server Yapfast won't connect to"),
        };
    }
    Ok(reqwest::Url::parse(&format!(
        "wss://{endpoint}/?v={VERSION}"
    ))?)
}

/// The heartbeat interval the voice server asks for, in milliseconds, held
/// to a sane range.
fn heartbeat(milliseconds: f64) -> Duration {
    crate::limits::sent_seconds(
        milliseconds / 1000.0,
        Duration::from_secs(1),
        Duration::from_secs(300),
    )
}

pub(crate) type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

pub(crate) async fn send_json(ws: &mut Socket, op: u8, data: Value) -> Result<()> {
    ws.send(Message::text(json!({"op": op, "d": data}).to_string()))
        .await?;
    Ok(())
}

async fn send_out(ws: &mut Socket, out: Vec<Out>) -> Result<()> {
    for out in out {
        let message = match out {
            Out::Json(event) => Message::text(serde_json::to_string(&event)?),
            Out::Binary(event) => {
                Message::binary(serialize_binary_event(&event).map_err(|error| anyhow!("{error}"))?)
            }
        };
        ws.send(message).await?;
    }
    Ok(())
}

/// The voice gateway closed, with Discord's close code if it gave one:
/// 4014 means this account was disconnected from the call, 4006 that its
/// session is no longer valid.
#[derive(Debug)]
pub(crate) struct Closed {
    pub code: Option<u16>,
    pub reason: String,
}

impl std::fmt::Display for Closed {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.code {
            Some(code) => write!(
                formatter,
                "Discord closed the voice connection ({code}: {})",
                self.reason
            ),
            None => write!(formatter, "Discord closed the voice connection"),
        }
    }
}

impl std::error::Error for Closed {}

/// A message from the voice gateway: its op and data, or a DAVE event.
pub(crate) enum Incoming {
    Json(u64, Value),
    Dave(Event),
}

/// The next message, noting its sequence number for heartbeats to
/// acknowledge: version 8 numbers JSON messages in `seq`, and binary ones in
/// their first two bytes.
pub(crate) async fn receive(ws: &mut Socket, seq: &mut Option<u64>) -> Result<Incoming> {
    loop {
        let Some(message) = ws.next().await else {
            return Err(Closed {
                code: None,
                reason: String::new(),
            }
            .into());
        };
        match message? {
            Message::Text(text) => {
                let value: Value = serde_json::from_str(&text)?;
                if let Some(number) = value["seq"].as_u64() {
                    *seq = Some(number);
                }
                let op = value["op"].as_u64().unwrap_or(u64::MAX);
                if let Some(event) = dave_json(op, &value["d"]) {
                    return Ok(Incoming::Dave(event));
                }
                return Ok(Incoming::Json(op, value["d"].clone()));
            }
            Message::Binary(bytes) => match binary(&bytes) {
                Ok((number, event)) => {
                    *seq = Some(number);
                    return Ok(Incoming::Dave(event));
                }
                Err(error) => tracing::debug!(%error, "unknown binary voice message"),
            },
            Message::Close(frame) => {
                return Err(Closed {
                    code: frame.as_ref().map(|frame| u16::from(frame.code)),
                    reason: frame
                        .map(|frame| frame.reason.to_string())
                        .unwrap_or_default(),
                }
                .into());
            }
            _ => {}
        }
    }
}

/// DAVE's JSON ops, and who's connected, as the voice model knows them.
/// Its parser for a whole message wants `op` first and nothing after `d`,
/// which version 8's `seq` breaks, so only the data goes through it.
fn dave_json(op: u64, data: &Value) -> Option<Event> {
    use serenity_voice_model::payload::{
        ClientDisconnect, ClientsConnect, DaveExecuteTransition, DavePrepareEpoch,
        DavePrepareTransition,
    };
    let event = match op {
        11 => ClientsConnect::deserialize(data).map(Event::from),
        13 => ClientDisconnect::deserialize(data).map(Event::from),
        21 => DavePrepareTransition::deserialize(data).map(Event::from),
        22 => DaveExecuteTransition::deserialize(data).map(Event::from),
        24 => DavePrepareEpoch::deserialize(data).map(Event::from),
        _ => return None,
    };
    event
        .inspect_err(|error| tracing::warn!(op, %error, "an unreadable voice gateway message"))
        .ok()
}

/// A binary DAVE message and its sequence number. Version 8 puts the number
/// before the opcode; the voice model reads from the opcode on.
fn binary(bytes: &[u8]) -> Result<(u64, Event)> {
    let Some((number, message)) = bytes.split_first_chunk::<2>() else {
        bail!("a binary voice message too short for its sequence number");
    };
    let event = serenity_voice_model::deserialize_binary_event(message)
        .map_err(|error| anyhow!("{error}"))?;
    Ok((u64::from(u16::from_be_bytes(*number)), event))
}

/// How long address discovery waits for an answer before it asks again,
/// and how many times it asks. One lost packet, on the way there or back,
/// no longer ends the connection; a voice server that never answers still
/// fails after five seconds, as it did with a single ask.
const DISCOVERY_WAIT: Duration = Duration::from_secs(1);
const DISCOVERY_ASKS: u32 = 5;

/// Discord's address discovery: the server tells the client its public
/// address and port, which it then selects the protocol with.
async fn discover(udp: &UdpSocket, ssrc: u32) -> Result<(IpAddr, u16)> {
    discover_waiting(udp, ssrc, DISCOVERY_WAIT).await
}

/// Asks up to [`DISCOVERY_ASKS`] times, `wait` apart, and takes the first
/// answer it can read. Anything else that arrives is passed over, so a
/// stray packet can't end the connection either.
async fn discover_waiting(udp: &UdpSocket, ssrc: u32, wait: Duration) -> Result<(IpAddr, u16)> {
    let mut request = [0u8; 74];
    request[..2].copy_from_slice(&1u16.to_be_bytes());
    request[2..4].copy_from_slice(&70u16.to_be_bytes());
    request[4..8].copy_from_slice(&ssrc.to_be_bytes());
    // Room for any datagram: Windows fails a receive that doesn't fit.
    let mut answer = [0u8; 1500];
    let mut unreadable = None;
    for ask in 1..=DISCOVERY_ASKS {
        udp.send(&request)
            .await
            .context("couldn't send address discovery to the voice server")?;
        let next = tokio::time::Instant::now() + wait;
        loop {
            let length = match tokio::time::timeout_at(next, udp.recv(&mut answer)).await {
                Err(_) => break,
                Ok(Ok(length)) => length,
                // An ICMP "port unreachable" for an earlier packet, which
                // Windows hands to the next receive on the socket.
                Ok(Err(error))
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::ConnectionRefused
                    ) =>
                {
                    tracing::debug!(ask, %error, "address discovery: no answer yet");
                    continue;
                }
                Ok(Err(error)) => return Err(error.into()),
            };
            match discovered(&answer[..length]) {
                Ok(found) if ask == 1 => {
                    tracing::debug!("address discovery answered the first ask");
                    return Ok(found);
                }
                Ok(found) => {
                    tracing::info!(
                        "address discovery answered on attempt {ask} of {DISCOVERY_ASKS}"
                    );
                    return Ok(found);
                }
                Err(error) => {
                    tracing::debug!(ask, length, %error, "address discovery passed over a packet");
                    unreadable = Some(error);
                }
            }
        }
    }
    tracing::warn!(
        attempts = DISCOVERY_ASKS,
        "address discovery got no answer after {DISCOVERY_ASKS} attempts"
    );
    let seconds = (wait * DISCOVERY_ASKS).as_secs_f32();
    Err(match unreadable {
        Some(error) => error.context(format!(
            "the voice server's address discovery answer was malformed, after {DISCOVERY_ASKS} tries over {seconds} s"
        )),
        None => anyhow!(
            "the voice server didn't answer address discovery, asked {DISCOVERY_ASKS} times over {seconds} s"
        ),
    })
}

/// The address and port in an address discovery answer.
fn discovered(answer: &[u8]) -> Result<(IpAddr, u16)> {
    let Some(answer) = answer.first_chunk::<74>() else {
        bail!("{} bytes, short of an answer's 74", answer.len());
    };
    if u16::from_be_bytes([answer[0], answer[1]]) != 2 {
        bail!("not an answer");
    }
    let address = &answer[8..72];
    let end = address
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(address.len());
    let ip = std::str::from_utf8(&address[..end])?.parse()?;
    Ok((ip, u16::from_be_bytes([answer[72], answer[73]])))
}

/// Who a connection identifies as, and with which server.
pub(crate) struct Identity {
    pub endpoint: String,
    /// A call's server, or its DM; a stream's RTC server.
    pub server_id: String,
    pub user_id: String,
    pub session_id: String,
    pub token: String,
    /// The video identify announces: a call's camera, or a stream's screen.
    pub streams: Value,
    /// This account, and the DAVE group's id, as far as it's known.
    pub user: u64,
    pub group: u64,
}

/// A connection after its handshake.
pub(crate) struct Connection {
    pub ws: Socket,
    pub udp: UdpSocket,
    pub transport: Transport,
    pub dave: Dave,
    pub ssrc: u32,
    pub video: u32,
    pub rtx: u32,
    pub interval: Duration,
    /// The video codec Discord chose, if it said.
    pub codec: Option<Codec>,
    /// Gateway messages that arrived during the handshake.
    pub early: Vec<(u64, Value)>,
    /// The last sequence number the server sent, for heartbeats.
    pub seq: Option<u64>,
}

impl Connection {
    /// Identifies, offers `codecs`, and agrees encryption. DAVE starts
    /// knowing `known` are in the call.
    pub async fn open(identity: &Identity, codecs: Value, known: &[u64]) -> Result<Self> {
        let url = gateway_url(&identity.endpoint)?;
        let (mut ws, _) = timeout(HANDSHAKE, websocket::connect(url.as_str()))
            .await
            .context("the voice server didn't answer")?
            .context("couldn't reach the voice server")?;
        send_json(
            &mut ws,
            0,
            json!({
                "server_id": identity.server_id,
                "user_id": identity.user_id,
                "session_id": identity.session_id,
                "token": identity.token,
                "video": true,
                "streams": identity.streams,
                "max_dave_protocol_version": davey::DAVE_PROTOCOL_VERSION,
            }),
        )
        .await?;

        let mut dave_early = vec![];
        let mut early = vec![];
        let mut seq = None;
        let handshake = async {
            let (mut interval, mut ready) = (None, None);
            while interval.is_none() || ready.is_none() {
                match receive(&mut ws, &mut seq).await? {
                    Incoming::Json(8, hello) => interval = hello["heartbeat_interval"].as_f64(),
                    Incoming::Json(2, data) => ready = Some(data),
                    Incoming::Json(op, data) => early.push((op, data)),
                    Incoming::Dave(event) => dave_early.push(event),
                }
            }
            let (interval, ready) = (interval.unwrap_or(41_250.0), ready.unwrap_or_default());
            tracing::debug!(%ready, "the voice server is ready");
            let ssrc = ready["ssrc"]
                .as_u64()
                .context("the voice server sent no SSRC")? as u32;
            let ip: IpAddr = ready["ip"]
                .as_str()
                .context("the voice server sent no address")?
                .parse()?;
            let port = ready["port"]
                .as_u64()
                .context("the voice server sent no port")? as u16;
            let modes: Vec<&str> = ready["modes"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect();
            let mode = Transport::MODES
                .into_iter()
                .find(|mode| modes.contains(mode))
                .context("the voice server offers no encryption Yapfast knows")?;
            // Discord numbers the video SSRCs after the audio one.
            let stream = &ready["streams"][0];
            let video = stream["ssrc"]
                .as_u64()
                .map_or(ssrc.wrapping_add(1), |ssrc| ssrc as u32);
            let rtx = stream["rtx_ssrc"]
                .as_u64()
                .map_or(ssrc.wrapping_add(2), |ssrc| ssrc as u32);

            let udp = UdpSocket::bind(if ip.is_ipv4() { "0.0.0.0:0" } else { "[::]:0" }).await?;
            udp.connect((ip, port)).await?;
            let (address, port) = discover(&udp, ssrc).await?;
            send_json(
                &mut ws,
                1,
                json!({
                    "protocol": "udp",
                    "codecs": codecs,
                    "data": {"address": address.to_string(), "port": port, "mode": mode},
                }),
            )
            .await?;
            let session = loop {
                match receive(&mut ws, &mut seq).await? {
                    Incoming::Json(4, session) => break session,
                    Incoming::Json(op, data) => early.push((op, data)),
                    Incoming::Dave(event) => dave_early.push(event),
                }
            };
            let mut described = session.clone();
            if let Some(fields) = described.as_object_mut() {
                fields.remove("secret_key");
            }
            tracing::debug!(session = %described, "the voice server described the session");
            let key: Vec<u8> = session["secret_key"]
                .as_array()
                .context("the voice server sent no key")?
                .iter()
                .filter_map(|byte| byte.as_u64().map(|byte| byte as u8))
                .collect();
            let transport = Transport::new(session["mode"].as_str().unwrap_or(mode), &key)?;
            let version = session["dave_protocol_version"].as_u64().unwrap_or(0) as u16;
            let codec = session["video_codec"].as_str().and_then(Codec::from_name);
            anyhow::Ok((interval, ssrc, video, rtx, udp, transport, version, codec))
        };
        let (interval, ssrc, video, rtx, udp, transport, version, codec) =
            timeout(HANDSHAKE, handshake)
                .await
                .context("the voice server took too long to connect")??;

        let mut dave = Dave::new(identity.user, identity.group);
        for &user in known {
            dave.recognise(user);
        }
        for (op, data) in &early {
            let mut text = data.to_string();
            text.truncate(400);
            tracing::debug!(op, data = %text, "the voice server said while connecting");
            if let Some(user) = announced(*op, data) {
                dave.recognise(user);
            }
        }
        send_out(&mut ws, dave.start(version)).await?;
        for event in dave_early {
            send_out(&mut ws, dave.handle(event)).await?;
        }
        tracing::info!(
            dave = version,
            ?codec,
            server = %identity.server_id,
            group = identity.group,
            "connected to a Discord voice server"
        );
        Ok(Self {
            ws,
            udp,
            transport,
            dave,
            ssrc,
            video,
            rtx,
            interval: heartbeat(interval),
            codec,
            early,
            seq,
        })
    }

    /// Opens the voice gateway again after it dropped, carrying on the same
    /// session: the UDP socket, its keys and DAVE's group stay as they were.
    pub async fn resume(&mut self, identity: &Identity) -> Result<()> {
        let url = gateway_url(&identity.endpoint)?;
        let resumed = async {
            let (mut ws, _) = websocket::connect(url.as_str()).await?;
            send_json(
                &mut ws,
                7,
                json!({
                    "server_id": identity.server_id,
                    "session_id": identity.session_id,
                    "token": identity.token,
                    "seq_ack": self.seq.map_or(-1, |seq| seq as i64),
                }),
            )
            .await?;
            let mut seq = self.seq;
            loop {
                match receive(&mut ws, &mut seq).await? {
                    Incoming::Json(9, _) => break,
                    Incoming::Json(8, hello) => {
                        if let Some(interval) = hello["heartbeat_interval"].as_f64() {
                            self.interval = heartbeat(interval);
                        }
                    }
                    Incoming::Json(op, data) => self.early.push((op, data)),
                    Incoming::Dave(event) => {
                        let out = self.dave.handle(event);
                        send_out(&mut ws, out).await?;
                    }
                }
            }
            anyhow::Ok((ws, seq))
        };
        let (ws, seq) = timeout(HANDSHAKE, resumed)
            .await
            .context("the voice server took too long to resume")??;
        self.ws = ws;
        self.seq = seq;
        tracing::info!(server = %identity.server_id, "resumed the Discord voice connection");
        Ok(())
    }

    pub async fn heartbeat(&mut self) -> Result<()> {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.as_millis() as u64);
        send_json(
            &mut self.ws,
            3,
            json!({"t": nonce, "seq_ack": self.seq.map_or(-1, |seq| seq as i64)}),
        )
        .await
    }

    /// The round trip a heartbeat ACK (op 6) measures: from the
    /// heartbeat's nonce, the time it left, until now.
    pub fn acked(data: &Value) -> Option<std::time::Duration> {
        let sent = data["t"].as_u64()?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_millis() as u64;
        Some(std::time::Duration::from_millis(now.checked_sub(sent)?))
    }

    /// Answers DAVE; other messages go back to the caller.
    pub async fn handle(&mut self, message: Incoming) -> Result<Option<(u64, Value)>> {
        match message {
            Incoming::Dave(event) => {
                let out = self.dave.handle(event);
                send_out(&mut self.ws, out).await?;
                Ok(None)
            }
            Incoming::Json(op, data) => {
                if let Some(user) = announced(op, &data) {
                    self.dave.recognise(user);
                }
                Ok(Some((op, data)))
            }
        }
    }
}

/// Someone a voice gateway message says is in the call, for DAVE's check of
/// the people proposed into its group: speaking (5) and video attributes
/// (12) carry one; op 11, which lists them, goes to DAVE itself.
fn announced(op: u64, data: &Value) -> Option<u64> {
    match op {
        5 | 12 => data["user_id"].as_str()?.parse().ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_websockets_only_on_this_machine() {
        assert_eq!(
            gateway_url("c-ams.discord.media:443").unwrap().as_str(),
            "wss://c-ams.discord.media/?v=8"
        );
        assert_eq!(
            gateway_url("ws://127.0.0.1:9/voice").unwrap().as_str(),
            "ws://127.0.0.1:9/voice/?v=8"
        );
        assert!(gateway_url("ws://example.com/voice").is_err());
    }

    /// As version 8 sends them: `seq` after `d`.
    #[test]
    fn reads_daves_json_messages() {
        let read = |text: &str| {
            let value: Value = serde_json::from_str(text).unwrap();
            dave_json(value["op"].as_u64().unwrap(), &value["d"])
        };
        assert!(matches!(
            read(r#"{"op":22,"d":{"transition_id":7},"seq":12}"#),
            Some(Event::DaveExecuteTransition(execute)) if execute.transition_id == 7
        ));
        assert!(matches!(
            read(r#"{"op":21,"d":{"transition_id":7,"protocol_version":1},"seq":12}"#),
            Some(Event::DavePrepareTransition(prepare)) if prepare.transition_id == 7
        ));
        assert!(matches!(
            read(r#"{"op":24,"d":{"epoch":1,"protocol_version":1},"seq":3}"#),
            Some(Event::DavePrepareEpoch(prepare)) if prepare.epoch == 1
        ));
        assert!(matches!(
            read(r#"{"op":11,"d":{"user_ids":["1","2"]},"seq":3}"#),
            Some(Event::ClientsConnect(connect)) if connect.user_ids.len() == 2
        ));
        assert!(matches!(
            read(r#"{"op":13,"d":{"user_id":"1"},"seq":3}"#),
            Some(Event::ClientDisconnect(disconnect)) if disconnect.user_id.0 == 1
        ));
        assert!(read(r#"{"op":5,"d":{"user_id":"1","ssrc":2},"seq":3}"#).is_none());
    }

    #[test]
    fn reads_binary_messages_after_their_sequence_number() {
        // Sequence 5, op 30 (a welcome) for transition 4.
        let (seq, event) = binary(&[0, 5, 30, 0, 4, 1, 2, 3]).unwrap();
        assert_eq!(seq, 5);
        assert!(
            matches!(&event, Event::DaveMlsWelcome(welcome) if welcome.transition_id == 4 && welcome.welcome == [1, 2, 3]),
            "{event:?}"
        );
        assert!(binary(&[0]).is_err());
    }

    /// A voice server on this machine that passes over the first `lost`
    /// asks, then answers each later one after a stray and a short packet.
    /// Returns its socket's address and the asks it got.
    async fn voice_server(lost: usize) -> (std::net::SocketAddr, std::sync::Arc<Mutex<usize>>) {
        let server = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let address = server.local_addr().unwrap();
        let asks = std::sync::Arc::new(Mutex::new(0));
        let counted = asks.clone();
        tokio::spawn(async move {
            let mut packet = [0u8; 74];
            loop {
                let (length, from) = server.recv_from(&mut packet).await.unwrap();
                assert_eq!(length, 74);
                assert_eq!(packet[..8], [0, 1, 0, 70, 0, 0, 0, 7]);
                *lock(&counted) += 1;
                if *lock(&counted) <= lost {
                    continue;
                }
                server.send_to(&[0x80, 0x78, 0, 1], from).await.unwrap();
                let mut stray = packet;
                stray[1] = 9;
                server.send_to(&stray, from).await.unwrap();
                let mut answer = [0u8; 74];
                answer[..8].copy_from_slice(&[0, 2, 0, 70, 0, 0, 0, 7]);
                answer[8..19].copy_from_slice(b"203.0.113.5");
                answer[72..].copy_from_slice(&50_000u16.to_be_bytes());
                server.send_to(&answer, from).await.unwrap();
            }
        });
        (address, asks)
    }

    async fn client(server: std::net::SocketAddr) -> UdpSocket {
        let udp = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        udp.connect(server).await.unwrap();
        udp
    }

    #[tokio::test]
    async fn address_discovery_asks_again_until_answered() {
        let wait = Duration::from_millis(100);
        for lost in [0, 1, DISCOVERY_ASKS as usize - 1] {
            let (server, asks) = voice_server(lost).await;
            let found = discover_waiting(&client(server).await, 7, wait).await;
            assert_eq!(
                found.unwrap(),
                ("203.0.113.5".parse().unwrap(), 50_000),
                "{lost} lost"
            );
            assert_eq!(*lock(&asks), lost + 1);
        }
    }

    #[tokio::test]
    async fn address_discovery_gives_up_on_a_silent_server() {
        let wait = Duration::from_millis(100);
        let (server, asks) = voice_server(usize::MAX).await;
        let started = std::time::Instant::now();
        let error = discover_waiting(&client(server).await, 7, wait)
            .await
            .unwrap_err();
        let took = started.elapsed();
        assert_eq!(
            error.to_string(),
            "the voice server didn't answer address discovery, asked 5 times over 0.5 s"
        );
        assert!(
            took >= wait * DISCOVERY_ASKS && took < wait * (DISCOVERY_ASKS + 3),
            "{took:?}"
        );
        assert_eq!(*lock(&asks), DISCOVERY_ASKS as usize);
    }

    #[test]
    fn reads_an_address_discovery_answer() {
        let mut answer = [0u8; 74];
        answer[..2].copy_from_slice(&[0, 2]);
        answer[8..11].copy_from_slice(b"::1");
        answer[72..].copy_from_slice(&[1, 2]);
        assert_eq!(discovered(&answer).unwrap(), ("::1".parse().unwrap(), 258));
        assert!(discovered(&answer[..73]).is_err());
        answer[8] = b'x';
        assert!(discovered(&answer).is_err());
        answer[8] = b':';
        answer[1] = 1;
        assert!(discovered(&answer).is_err());
    }
}
