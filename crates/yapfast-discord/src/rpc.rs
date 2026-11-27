//! Discord RPC over local IPC, for games and tools using discord-rpc/arRPC.
//! Protocol: https://docs.discord.food/topics/rpc#ipc-transport.
//! Only local Rich Presence is exposed; account/voice control needs OAuth2
//! and is deliberately unsupported here.

use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};

use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    sync::{mpsc, watch},
    task::JoinSet,
};

use crate::{
    api::Api,
    service::{Snapshot, Status},
};

#[cfg(target_os = "linux")]
#[path = "rpc/linux.rs"]
mod transport;
#[cfg(windows)]
#[path = "rpc/windows.rs"]
mod transport;
use transport::{Socket, Stream, runtime_directory};

const MAX_PACKET: usize = 64 * 1024;
/// The most image addresses a client's images are kept for: a music player
/// shows a new cover with each song.
const KEPT_IMAGES: usize = 32;
const MAX_CLIENTS: usize = 16;
const MAX_PENDING: usize = 4;
const WAIT: Duration = Duration::from_secs(15);

pub(crate) fn start(
    api: Arc<Api>,
    snapshots: watch::Receiver<Arc<Snapshot>>,
) -> watch::Receiver<Option<Arc<Value>>> {
    let (activity, receiver) = watch::channel(None);
    tokio::spawn(async move {
        // Tests must opt in to avoid advertising their fixture account on
        // the desktop. An explicit directory also helps Wine/sandbox bridges.
        let directory = std::env::var_os("YAPFAST_DISCORD_RPC_DIR").map(PathBuf::from);
        let disabled = std::env::var_os("YAPFAST_DISCORD_RPC").is_some_and(|v| v == "0")
            || (crate::client::test_server().is_some() && directory.is_none());
        if !disabled {
            match Socket::bind(&directory.unwrap_or_else(runtime_directory)).await {
                Ok(socket) => {
                    tracing::info!(path = %socket.path.display(), "Discord RPC listening");
                    serve(socket, api, snapshots, &activity).await;
                    activity.send_replace(None);
                    return;
                }
                Err(error) => tracing::warn!(%error, "Discord RPC unavailable"),
            }
        }
        // Keep the sender alive without work until the service drops its
        // receiver. No retry timer or extra thread when RPC is disabled.
        activity.closed().await;
    });
    receiver
}

async fn serve(
    mut socket: Socket,
    api: Arc<Api>,
    snapshots: watch::Receiver<Arc<Snapshot>>,
    activity: &watch::Sender<Option<Arc<Value>>>,
) {
    let (updates, mut incoming) = mpsc::channel(MAX_CLIENTS);
    let mut clients = JoinSet::new();
    let mut connected = HashSet::new();
    let mut next = 0_u64;
    let mut active = Vec::<(u64, Arc<Value>)>::new();
    loop {
        let removed = tokio::select! {
            biased;
            () = activity.closed() => break,
            Some(done) = clients.join_next(), if !clients.is_empty() => done.ok(),
            accepted = socket.accept(), if clients.len() < MAX_CLIENTS => {
                let Ok(stream) = accepted else {
                    // Out of descriptors, say: wait rather than end RPC for good.
                    tokio::time::sleep(Duration::from_millis(500)).await;
                    continue;
                };
                next += 1;
                let id = next;
                connected.insert(id);
                let api = api.clone();
                let snapshots = snapshots.clone();
                let updates = updates.clone();
                clients.spawn(async move {
                    if let Err(error) = connection(stream, api, snapshots, updates, id).await {
                        tracing::debug!(%error, "Discord RPC client ended");
                    }
                    id
                });
                continue;
            }
            Some((id, value)) = incoming.recv() => {
                // A client may finish between polling this queue and polling
                // JoinSet. Its queued final update must not resurrect a game.
                if !connected.contains(&id) {
                    continue;
                }
                active.retain(|(other, _)| *other != id);
                if let Some(value) = value {
                    active.push((id, value));
                }
                None
            }
        };
        if let Some(id) = removed {
            connected.remove(&id);
            active.retain(|(other, _)| *other != id);
        }
        activity.send_if_modified(|shown| {
            let next = active.last().map(|(_, value)| value);
            if shown.as_ref() == next {
                false
            } else {
                *shown = next.cloned();
                true
            }
        });
    }
    // Dropping JoinSet aborts clients; Socket releases the listener.
}

/// Keeps partial framing across select! cancellations while an HTTP lookup
/// finishes. No extra reader task or polling timer per connected app.
#[derive(Default)]
struct Packets {
    header: [u8; 8],
    header_read: usize,
    payload: Vec<u8>,
    payload_read: usize,
}

impl Packets {
    async fn read(&mut self, stream: &mut (impl AsyncRead + Unpin)) -> Result<(u32, Vec<u8>)> {
        while self.header_read < self.header.len() {
            let read = stream.read(&mut self.header[self.header_read..]).await?;
            ensure!(read > 0, "RPC client disconnected");
            self.header_read += read;
        }
        let opcode = u32::from_le_bytes(self.header[..4].try_into()?);
        let size = u32::from_le_bytes(self.header[4..].try_into()?) as usize;
        ensure!(size <= MAX_PACKET, "RPC packet too large");
        self.payload.resize(size, 0);
        while self.payload_read < size {
            let read = stream.read(&mut self.payload[self.payload_read..]).await?;
            ensure!(read > 0, "RPC client disconnected");
            self.payload_read += read;
        }
        self.header_read = 0;
        self.payload_read = 0;
        Ok((opcode, std::mem::take(&mut self.payload)))
    }
}

#[cfg(test)]
async fn read(stream: &mut (impl AsyncRead + Unpin)) -> Result<(u32, Vec<u8>)> {
    Packets::default().read(stream).await
}

async fn write(stream: &mut (impl AsyncWrite + Unpin), opcode: u32, payload: &[u8]) -> Result<()> {
    tokio::time::timeout(WAIT, async {
        stream.write_u32_le(opcode).await?;
        stream.write_u32_le(payload.len() as u32).await?;
        stream.write_all(payload).await
    })
    .await??;
    Ok(())
}

async fn send(stream: &mut (impl AsyncWrite + Unpin), value: Value) -> Result<()> {
    write(stream, 1, &serde_json::to_vec(&value)?).await
}

async fn connection(
    mut stream: Stream,
    api: Arc<Api>,
    snapshots: watch::Receiver<Arc<Snapshot>>,
    updates: mpsc::Sender<(u64, Option<Arc<Value>>)>,
    id: u64,
) -> Result<()> {
    let mut packets = Packets::default();
    let (opcode, payload) = tokio::time::timeout(WAIT, packets.read(&mut stream)).await??;
    let handshake: Value = serde_json::from_slice(&payload)?;
    let client = handshake["client_id"].as_str().unwrap_or_default();
    if opcode != 0 || handshake["v"] != 1 || !snowflake(client) {
        write(
            &mut stream,
            2,
            br#"{"code":4000,"message":"Invalid RPC handshake"}"#,
        )
        .await?;
        return Ok(());
    }
    let client = client.to_owned();
    // A connected, idle game must not retain its handshake JSON/packet buffer.
    drop(handshake);
    drop(payload);
    // A game that retries while Discord is offline must not cost an API call each time.
    ensure!(
        snapshots.borrow().status == Status::Online,
        "Discord is not connected"
    );
    let application = tokio::time::timeout(WAIT, api.rpc_application(&client)).await??;
    let name = application["name"]
        .as_str()
        .context("RPC application has no name")?
        .to_owned();
    drop(application);
    let user = {
        let snapshot = snapshots.borrow();
        ensure!(
            snapshot.status == Status::Online,
            "Discord is not connected"
        );
        json!({"id": snapshot.user_id, "username": snapshot.account.as_ref().map(|a| a.name.as_str()),
            "discriminator": "0", "avatar": null, "bot": false})
    };
    send(&mut stream, json!({"cmd":"DISPATCH", "evt":"READY", "nonce":null,
        "data":{"v":1,"config":{"cdn_host":"cdn.discordapp.com","api_endpoint":"//discord.com/api","environment":"production"},"user":user}})).await?;
    let mut assets = None;
    let mut images = Assets::new();
    let (mut input, mut output) = tokio::io::split(stream);
    let mut pending = VecDeque::<(u32, Vec<u8>)>::new();
    let mut pending_bytes = 0;
    loop {
        let (opcode, payload) = match pending.pop_front() {
            Some((opcode, payload)) => {
                pending_bytes -= payload.len();
                (opcode, payload)
            }
            None => packets.read(&mut input).await?,
        };
        match opcode {
            2 => return Ok(()),
            3 => {
                write(&mut output, 4, &payload).await?;
                continue;
            }
            4 => continue,
            1 => {}
            _ => bail!("invalid RPC opcode"),
        }
        let packet: Value = serde_json::from_slice(&payload)?;
        drop(payload);
        let cmd = packet["cmd"].as_str().unwrap_or_default();
        let nonce = &packet["nonce"];
        if cmd != "SET_ACTIVITY" {
            send(
                &mut output,
                json!({"cmd":cmd,"nonce":nonce,"evt":"ERROR",
                "data":{"code":4002,"message":"Only SET_ACTIVITY is supported"}}),
            )
            .await?;
            continue;
        }
        let resolve = async {
            let args = &packet["args"];
            ensure!(
                args["pid"].as_u64().is_some_and(|pid| pid > 0),
                "pid must be a positive integer"
            );
            ensure!(
                args.get("activity").is_some(),
                "activity is required (null clears it)"
            );
            if args["activity"].is_null() {
                return Ok(None);
            }
            let pictures = &args["activity"]["assets"];
            let mut addresses = Vec::new();
            let mut named_assets = false;
            for image in ["large_image", "small_image"]
                .iter()
                .filter_map(|key| pictures[key].as_str())
            {
                if web_address(image, usize::MAX) {
                    ensure!(image.len() <= 512, "image address is too long");
                    if !images.contains_key(image) && !addresses.contains(&image) {
                        addresses.push(image);
                    }
                } else {
                    // Asset ids already have their gateway form; only named
                    // keys need an HTTP lookup.
                    named_assets |= !snowflake(image);
                }
            }
            if assets.is_none() && named_assets {
                let response = tokio::time::timeout(WAIT, api.rpc_assets(&client)).await??;
                assets = Some(asset_lookup(&response)?);
            }
            if !addresses.is_empty() {
                // As the desktop client does, Discord's media proxy takes them.
                let response =
                    tokio::time::timeout(WAIT, api.rpc_external_assets(&client, &addresses))
                        .await??;
                remember_images(&mut images, &response, pictures)?;
            }
            normalize(&args["activity"], &client, &name, assets.as_ref(), &images)
                .map(|value| Some(Arc::new(value)))
        };
        tokio::pin!(resolve);
        let result = loop {
            tokio::select! {
                // Read disconnects/CLOSE even while Discord resolves images
                // or holds the HTTP request back behind a rate limit. With
                // the queue full, stop reading: the app waits, as before,
                // rather than losing its connection and its activity.
                received = packets.read(&mut input),
                    if pending.len() < MAX_PENDING && pending_bytes < MAX_PACKET =>
                {
                    let (opcode, payload) = received?;
                    match opcode {
                        2 => return Ok(()),
                        3 => write(&mut output, 4, &payload).await?,
                        4 => {},
                        1 => {
                            pending_bytes += payload.len();
                            pending.push_back((opcode, payload));
                        }
                        _ => bail!("invalid RPC opcode"),
                    }
                }
                result = &mut resolve => break result,
            }
        };
        match result {
            Ok(value) => {
                // Bound queued updates even when games publish very frequently.
                updates.send((id, value.clone())).await?;
                send(
                    &mut output,
                    json!({"cmd":cmd,"nonce":nonce,"evt":null,"data":value}),
                )
                .await?;
            }
            Err(error) => {
                send(
                    &mut output,
                    json!({"cmd":cmd,"nonce":nonce,"evt":"ERROR",
                    "data":{"code":4000,"message":error.to_string()}}),
                )
                .await?;
            }
        }
    }
}

fn snowflake(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 20
        && value.bytes().all(|b| b.is_ascii_digit())
        && value.parse::<u64>().is_ok_and(|id| id > 0)
}

type Assets = HashMap<String, String>;

/// Evict older covers without evicting a cached image this very activity
/// still uses, such as the artist badge beside a new album cover.
fn remember_images(images: &mut Assets, response: &Value, pictures: &Value) -> Result<()> {
    let new = external_lookup(response)?;
    if images.len() + new.len() > KEPT_IMAGES {
        images.retain(|url, _| {
            ["large_image", "small_image"]
                .iter()
                .any(|key| pictures[key].as_str() == Some(url))
        });
    }
    images.extend(new);
    Ok(())
}

fn web_address(text: &str, longest: usize) -> bool {
    text.len() <= longest && (text.starts_with("https://") || text.starts_with("http://"))
}

/// The media proxy's names for image addresses, as `mp:` asset keys.
fn external_lookup(response: &Value) -> Result<Assets> {
    Ok(response
        .as_array()
        .context("invalid external assets")?
        .iter()
        .filter_map(|asset| {
            Some((
                asset["url"].as_str()?.to_owned(),
                format!("mp:{}", asset["external_asset_path"].as_str()?),
            ))
        })
        .collect())
}

fn asset_lookup(response: &Value) -> Result<Assets> {
    Ok(response
        .as_array()
        .context("invalid application assets")?
        .iter()
        .filter_map(|asset| {
            Some((
                asset["name"].as_str()?.to_owned(),
                asset["id"].as_str()?.to_owned(),
            ))
        })
        .collect())
}

/// RPC uses seconds for timestamps and asset names; the Gateway needs
/// milliseconds and asset ids. Select fields so callers cannot inject a
/// custom status, another application id, or internal gateway metadata.
fn normalize(
    input: &Value,
    client: &str,
    registered_name: &str,
    assets: Option<&Assets>,
    addresses: &Assets,
) -> Result<Value> {
    ensure!(input.is_object(), "activity must be an object");
    let kind = match input.get("type") {
        None => 0,
        Some(value) => value.as_u64().context("invalid activity type")?,
    };
    ensure!(matches!(kind, 0 | 2 | 3 | 5), "unsupported activity type");
    let name = input["name"]
        .as_str()
        .filter(|name| name.chars().take(129).count() <= 128 && !name.trim().is_empty())
        .unwrap_or(registered_name);
    let mut output = json!({"name":name,"type":kind,"application_id":client});
    for key in [
        "state",
        "details",
        "state_url",
        "details_url",
        "party",
        "secrets",
        "instance",
        "status_display_type",
    ] {
        if let Some(value) = input.get(key).filter(|v| !v.is_null()) {
            output[key] = value.clone();
        }
    }
    for key in ["state", "details"] {
        if let Some(value) = output.get(key) {
            ensure!(
                value.as_str().is_some_and(|s| s.chars().count() <= 128),
                "invalid {key}"
            );
        }
    }
    for key in ["state_url", "details_url"] {
        if let Some(value) = output.get(key) {
            ensure!(
                value
                    .as_str()
                    .is_some_and(|s| s.len() <= 256
                        && (s.starts_with("https://") || s.starts_with("http://"))),
                "invalid {key}"
            );
        }
    }
    if let Some(value) = output.get("instance") {
        ensure!(value.is_boolean(), "instance must be a boolean");
    }
    if let Some(value) = output.get("status_display_type") {
        ensure!(
            value.as_u64().is_some_and(|n| n <= 2),
            "invalid status_display_type"
        );
    }
    if let Some(party) = output.get("party") {
        ensure!(party.is_object(), "party must be an object");
        if let Some(id) = party.get("id") {
            ensure!(
                id.as_str().is_some_and(|s| s.chars().count() <= 128),
                "invalid party id"
            );
        }
        if let Some(size) = party.get("size") {
            ensure!(
                size.as_array().is_some_and(|s| s.len() == 2
                    && s[0]
                        .as_u64()
                        .zip(s[1].as_u64())
                        .is_some_and(|(current, max)| max > 0 && current <= max)),
                "invalid party size"
            );
        }
    }
    if let Some(secrets) = output.get("secrets") {
        ensure!(secrets.is_object(), "secrets must be an object");
        for key in ["join", "spectate", "match"] {
            if let Some(value) = secrets.get(key) {
                ensure!(
                    value.as_str().is_some_and(|s| s.chars().count() <= 128),
                    "invalid {key} secret"
                );
            }
        }
    }
    if let Some(timestamps) = input.get("timestamps").filter(|v| !v.is_null()) {
        ensure!(timestamps.is_object(), "invalid timestamps");
        let mut converted = json!({});
        for key in ["start", "end"] {
            if let Some(value) = timestamps.get(key) {
                let seconds = value
                    .as_u64()
                    .context("timestamp must be a positive integer")?;
                // Some modern SDKs already send milliseconds.
                converted[key] = json!(if seconds < 1_000_000_000_000 {
                    seconds.checked_mul(1000).context("timestamp overflow")?
                } else {
                    seconds
                });
            }
        }
        output["timestamps"] = converted;
    }
    if let Some(images) = input.get("assets").filter(|v| !v.is_null()) {
        ensure!(images.is_object(), "invalid assets");
        let mut converted = json!({});
        for key in ["large_image", "small_image", "large_text", "small_text"] {
            if let Some(value) = images.get(key).filter(|v| !v.is_null()) {
                let text = value.as_str().context("asset must be text")?;
                let text = if key.ends_with("_image") && web_address(text, usize::MAX) {
                    addresses
                        .get(text)
                        .map(String::as_str)
                        .context("Discord didn't take the image's address")?
                } else if key.ends_with("_image") {
                    assets
                        .and_then(|lookup| lookup.get(text))
                        .map(String::as_str)
                        .or_else(|| snowflake(text).then_some(text))
                        .context("unknown application asset")?
                } else {
                    text
                };
                converted[key] = json!(text);
            }
        }
        output["assets"] = converted;
    }
    if let Some(buttons) = input.get("buttons").filter(|v| !v.is_null()) {
        let buttons = buttons.as_array().context("buttons must be an array")?;
        ensure!(buttons.len() <= 2, "at most two buttons");
        let mut labels = Vec::new();
        let mut urls = Vec::new();
        for button in buttons {
            let label = button["label"].as_str().context("button needs a label")?;
            let url = button["url"].as_str().context("button needs a URL")?;
            ensure!(
                !label.is_empty() && label.chars().count() <= 32,
                "invalid button label"
            );
            ensure!(
                url.len() <= 512 && (url.starts_with("https://") || url.starts_with("http://")),
                "invalid button URL"
            );
            labels.push(label);
            urls.push(url);
        }
        output["buttons"] = json!(labels);
        output["metadata"] = json!({"button_urls":urls});
    }
    // Leave room for a custom status and the rest of the Gateway envelope.
    ensure!(
        serde_json::to_vec(&output)?.len() <= 3072,
        "activity is too large"
    );
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::own_activity::merge;

    #[cfg(target_os = "linux")]
    use {std::os::unix::fs::PermissionsExt, tokio::net::UnixListener};

    #[cfg(target_os = "linux")]
    struct Directory(PathBuf);
    #[cfg(target_os = "linux")]
    impl Directory {
        fn new() -> Self {
            static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "yapfast-rpc-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    #[cfg(target_os = "linux")]
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn converts_sdk_fields_and_uses_the_handshakes_application() {
        let title = "Resident Evil 2: DualShock Ver.";
        let assets = asset_lookup(&json!([{"name":"cover","id":"1234"}])).unwrap();
        let shown = normalize(
            &json!({"name":title,"application_id":"999","state":"In game",
            "timestamps":{"start":1700000000_u64,"end":1700000123000_u64},
            "assets":{"large_image":"cover","small_image":"5678","large_text":"Hover"},
            "buttons":[{"label":"Website","url":"https://example.com"}],
            "metadata":{"injected":true},"id":"spoof"}),
            "42",
            "CheevoPresence",
            Some(&assets),
            &Assets::new(),
        )
        .unwrap();
        assert_eq!(shown["type"], 0);
        assert_eq!(shown["name"], title);
        assert_eq!(shown["application_id"], "42");
        assert_eq!(
            shown["timestamps"],
            json!({"start":1700000000000_u64,"end":1700000123000_u64})
        );
        assert_eq!(shown["assets"]["large_image"], "1234");
        assert_eq!(shown["assets"]["small_image"], "5678");
        assert_eq!(shown["buttons"], json!(["Website"]));
        assert_eq!(
            shown["metadata"],
            json!({"button_urls":["https://example.com"]})
        );
        assert!(shown.get("id").is_none());
    }

    #[test]
    fn falls_back_to_the_registered_name_for_unusable_overrides() {
        let registered = "Game";
        let max_length = "é".repeat(128);
        let assets = Assets::new();
        for (activity, expected) in [
            (json!({}), registered),
            (json!({"name":null}), registered),
            (json!({"name":42}), registered),
            (json!({"name":""}), registered),
            (json!({"name":" \t"}), registered),
            (json!({"name":"é".repeat(129)}), registered),
            (json!({"name":"X"}), "X"),
            (json!({"name":max_length}), max_length.as_str()),
        ] {
            let shown = normalize(&activity, "42", registered, None, &assets).unwrap();
            assert_eq!(shown["name"], expected, "{activity}");
        }
    }

    #[test]
    fn shows_images_given_by_address_through_the_media_proxy() {
        let images = external_lookup(&json!([{"url":"https://example.com/cover.png",
            "external_asset_path":"external/abc/https/example.com/cover.png"}]))
        .unwrap();
        let activity = json!({"assets":{"large_image":"https://example.com/cover.png"}});
        let shown = normalize(&activity, "42", "Game", None, &images).unwrap();
        assert_eq!(
            shown["assets"]["large_image"],
            "mp:external/abc/https/example.com/cover.png"
        );
        let other = json!({"assets":{"large_image":"https://example.com/other.png"}});
        assert!(normalize(&other, "42", "Game", None, &images).is_err());
    }

    #[test]
    fn keeps_a_music_players_track_and_display_preference() {
        // The wire fields used by Liner's @xhayper/discord-rpc client.
        let shown = normalize(
            &json!({
                "name":"Artist", "type":2, "details":"Track", "state":"Artist",
                "status_display_type":1,
                "timestamps":{"start":1700000000_u64,"end":1700000180_u64},
                "buttons":[{"label":"Listen on Liner","url":"https://link.tryliner.fun/track?id=1"}]
            }),
            "1561339119617974362",
            "Liner",
            None,
            &Assets::new(),
        )
        .unwrap();
        assert_eq!(shown["type"], 2);
        assert_eq!(shown["name"], "Artist");
        assert_eq!(shown["details"], "Track");
        assert_eq!(shown["state"], "Artist");
        assert_eq!(shown["status_display_type"], 1);
        assert_eq!(shown["timestamps"]["end"], 1700000180000_u64);
        assert_eq!(shown["buttons"][0], "Listen on Liner");
    }

    #[test]
    fn rotating_covers_keeps_the_current_activities_cached_artist_image() {
        let artist = "https://example.com/artist.png";
        let cover = "https://example.com/new-cover.png";
        let mut images = Assets::from([(artist.into(), "mp:artist".into())]);
        for n in 1..KEPT_IMAGES {
            images.insert(
                format!("https://example.com/old-{n}.png"),
                format!("mp:old-{n}"),
            );
        }
        let activity = json!({"type":2,"assets":{"large_image":cover,"small_image":artist}});
        remember_images(
            &mut images,
            &json!([{"url":cover,"external_asset_path":"external/new-cover"}]),
            &activity["assets"],
        )
        .unwrap();
        let shown = normalize(&activity, "42", "Liner", None, &images).unwrap();
        assert_eq!(shown["assets"]["small_image"], "mp:artist");
        assert_eq!(shown["assets"]["large_image"], "mp:external/new-cover");
        assert_eq!(images.len(), 2);
    }

    #[test]
    fn rejects_invalid_activity_without_losing_the_previous_one() {
        for value in [
            json!([]),
            json!({"type":4}),
            json!({"type":"0"}),
            json!({"state":42}),
            json!({"details":"a".repeat(129)}),
            json!({"timestamps":{"start":-1}}),
            json!({"assets":{"large_image":"unknown"}}),
            json!({"buttons":[{"label":"Click","url":"javascript:alert(1)"}]}),
        ] {
            assert!(
                normalize(&value, "42", "Game", None, &Assets::new()).is_err(),
                "{value}"
            );
        }
        for kind in [0, 2, 3, 5] {
            assert!(normalize(&json!({"type":kind}), "42", "Game", None, &Assets::new()).is_ok());
        }
    }

    #[test]
    fn merging_keeps_custom_status_and_availability_and_replaces_stale_games() {
        let custom = json!({"type":4,"name":"Custom Status","state":"Tea"});
        let mut presence = json!({"status":"dnd","since":123,"afk":true,
            "activities":[custom,{"type":0,"name":"Old game"}]});
        let song = json!({"type":2,"name":"Spotify"});
        merge(
            &mut presence,
            Some(&json!({"type":0,"name":"Game"})),
            Some(&song),
        );
        assert_eq!(presence["activities"].as_array().unwrap().len(), 3);
        assert_eq!(presence["activities"][1]["name"], "Game");
        assert_eq!(presence["activities"][2], song, "the music goes last");
        merge(&mut presence, None, None);
        assert_eq!(
            presence,
            json!({"status":"dnd","since":123,"afk":true,"activities":[custom]})
        );
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn leaves_live_sockets_alone_and_reclaims_stale_ones() {
        let directory = Directory::new();
        let existing = UnixListener::bind(directory.0.join("discord-ipc-0")).unwrap();
        let first = Socket::bind(&directory.0).await.unwrap();
        assert!(first.path.ends_with("discord-ipc-1"));
        let second = Socket::bind(&directory.0).await.unwrap();
        assert!(second.path.ends_with("discord-ipc-2"));
        assert_eq!(
            std::fs::metadata(&first.path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        drop(first);
        assert!(!directory.0.join("discord-ipc-1").exists());
        drop(existing);
        let reclaimed = Socket::bind(&directory.0).await.unwrap();
        assert!(reclaimed.path.ends_with("discord-ipc-0"));
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn does_not_remove_a_replacement_or_follow_a_lock_symlink() {
        let directory = Directory::new();
        let target = directory.0.join("keep");
        std::fs::write(&target, b"keep me").unwrap();
        std::os::unix::fs::symlink(&target, directory.0.join("yapfast-discord-ipc-0.lock"))
            .unwrap();
        let socket = Socket::bind(&directory.0).await.unwrap();
        assert!(socket.path.ends_with("discord-ipc-1"));
        std::fs::remove_file(&socket.path).unwrap();
        std::fs::write(&socket.path, b"replacement").unwrap();
        let path = socket.path.clone();
        drop(socket);
        assert_eq!(std::fs::read(path).unwrap(), b"replacement");
        assert_eq!(std::fs::read(target).unwrap(), b"keep me");
    }

    #[tokio::test]
    async fn framing_handles_partial_reads_and_rejects_oversize_before_allocating() {
        let (mut client, mut server) = tokio::io::duplex(64);
        let writer = tokio::spawn(async move {
            for byte in [1, 0, 0, 0, 2, 0, 0, 0, b'{', b'}'] {
                client.write_all(&[byte]).await.unwrap();
                tokio::task::yield_now().await;
            }
            client.write_u32_le(1).await.unwrap();
            client.write_u32_le(MAX_PACKET as u32 + 1).await.unwrap();
        });
        assert_eq!(read(&mut server).await.unwrap(), (1, b"{}".to_vec()));
        assert!(
            read(&mut server)
                .await
                .unwrap_err()
                .to_string()
                .contains("too large")
        );
        writer.await.unwrap();
    }

    #[tokio::test]
    async fn framing_keeps_partial_headers_and_payloads_when_a_lookup_finishes() {
        let (mut client, mut stream) = tokio::io::duplex(64);
        let mut packets = Packets::default();
        let frame = [1, 0, 0, 0, 4, 0, 0, 0, b't', b'e', b's', b't'];
        for part in [&frame[..3], &frame[3..10]] {
            client.write_all(part).await.unwrap();
            // The HTTP side wins select! while a partial packet is buffered.
            let reading = packets.read(&mut stream);
            tokio::pin!(reading);
            assert!(futures_util::poll!(&mut reading).is_pending());
        }
        client.write_all(&frame[10..]).await.unwrap();
        assert_eq!(
            packets.read(&mut stream).await.unwrap(),
            (1, b"test".to_vec())
        );
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn a_busy_app_waits_for_a_slow_image_lookup_instead_of_losing_its_connection() {
        use tokio::net::{TcpListener, UnixStream};

        let http = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mut api = Api::anonymous().unwrap();
        api.base = format!("http://{}", http.local_addr().unwrap());
        let (looking_up, mut lookup_started) = watch::channel(false);
        let (release, released) = watch::channel(false);
        let http_worker = tokio::spawn(async move {
            loop {
                let (mut stream, _) = http.accept().await.unwrap();
                let mut request = Vec::new();
                while !request.ends_with(b"\r\n\r\n") {
                    request.push(stream.read_u8().await.unwrap());
                }
                let body = if request.starts_with(b"POST /applications/22/external-assets ") {
                    // Discord holds the image lookup back, as behind a rate limit.
                    looking_up.send_replace(true);
                    released.clone().wait_for(|go| *go).await.unwrap();
                    r#"[{"url":"https://example.com/cover.png","external_asset_path":"external/cover"}]"#
                } else {
                    r#"{"name":"Fixture"}"#
                };
                let head = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                stream.write_all(head.as_bytes()).await.unwrap();
                stream.write_all(body.as_bytes()).await.unwrap();
            }
        });
        let (_snapshots, snapshots) = watch::channel(Arc::new(Snapshot {
            status: Status::Online,
            user_id: Some("42".into()),
            ..Default::default()
        }));
        let (updates, mut published) = mpsc::channel(32);
        let (mut app, served) = UnixStream::pair().unwrap();
        let server = tokio::spawn(connection(served, Arc::new(api), snapshots, updates, 1));

        let handshake = serde_json::to_vec(&json!({"v":1,"client_id":"22"})).unwrap();
        write(&mut app, 0, &handshake).await.unwrap();
        let (_, ready) = read(&mut app).await.unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&ready).unwrap()["evt"],
            "READY"
        );
        let set = |nonce: &str, assets: Value| {
            serde_json::to_vec(&json!({"cmd":"SET_ACTIVITY","nonce":nonce,
                "args":{"pid":2,"activity":{"details":nonce,"assets":assets}}}))
            .unwrap()
        };
        write(
            &mut app,
            1,
            &set(
                "cover",
                json!({"large_image":"https://example.com/cover.png"}),
            ),
        )
        .await
        .unwrap();
        tokio::time::timeout(
            Duration::from_secs(1),
            lookup_started.wait_for(|started| *started),
        )
        .await
        .unwrap()
        .unwrap();
        // More updates than the queue holds while the lookup is stuck.
        let nonces: Vec<String> = (0..=MAX_PENDING + 1)
            .map(|n| format!("update-{n}"))
            .collect();
        for nonce in &nonces {
            write(&mut app, 1, &set(nonce, json!({}))).await.unwrap();
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
        release.send_replace(true);

        for nonce in std::iter::once("cover").chain(nonces.iter().map(String::as_str)) {
            let (opcode, payload) = tokio::time::timeout(Duration::from_secs(2), read(&mut app))
                .await
                .expect("the app gets an answer")
                .expect("the connection stays open");
            assert_eq!(opcode, 1);
            let answer: Value = serde_json::from_slice(&payload).unwrap();
            assert_eq!(answer["nonce"], nonce);
            assert!(answer["evt"].is_null(), "{answer}");
            assert_eq!(answer["data"]["details"], nonce);
        }
        let mut shown = Vec::new();
        while let Ok((_, Some(activity))) = published.try_recv() {
            shown.push(activity["details"].as_str().unwrap().to_owned());
        }
        assert_eq!(shown.len(), nonces.len() + 1);
        assert_eq!(shown.last(), nonces.last());
        drop(app);
        tokio::time::timeout(Duration::from_secs(1), server)
            .await
            .unwrap()
            .unwrap()
            .ok();
        http_worker.abort();
    }
}
