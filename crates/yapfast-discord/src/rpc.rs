//! Linux Discord RPC over IPC, for games and tools using discord-rpc/arRPC.
//! Protocol: https://docs.discord.food/topics/rpc#ipc-transport.
//! Only local Rich Presence is exposed; account/voice control needs OAuth2
//! and is deliberately unsupported here.

use std::{
    collections::{HashMap, HashSet},
    fs::{File, Metadata},
    io,
    os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{UnixListener, UnixStream},
    sync::{mpsc, watch},
    task::JoinSet,
};

use crate::{
    api::Api,
    service::{Snapshot, Status},
};

const MAX_PACKET: usize = 64 * 1024;
/// The most image addresses a client's images are kept for: a music player
/// shows a new cover with each song.
const KEPT_IMAGES: usize = 32;
const MAX_CLIENTS: usize = 16;
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

fn runtime_directory() -> PathBuf {
    ["XDG_RUNTIME_DIR", "TMPDIR", "TMP", "TEMP"]
        .into_iter()
        .filter_map(std::env::var_os)
        .find(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"))
}

/// The lock coordinates Yapfast processes; probing the socket also respects
/// Discord and other RPC servers, which do not take our lock.
struct Socket {
    listener: UnixListener,
    path: PathBuf,
    identity: Metadata,
    _lock: File,
}

impl Socket {
    async fn bind(directory: &Path) -> Result<Self> {
        let uid = std::fs::metadata("/proc/self")?.uid();
        for slot in 0..10 {
            let path = directory.join(format!("discord-ipc-{slot}"));
            let lock_path = directory.join(format!("yapfast-discord-ipc-{slot}.lock"));
            use std::os::unix::fs::OpenOptionsExt;
            let lock = match File::options()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&lock_path)
            {
                Ok(lock) => lock,
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                    // A replaced lock must neither follow a symlink nor
                    // block opening a FIFO.
                    let Ok(lock) = File::options()
                        .write(true)
                        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
                        .open(&lock_path)
                    else {
                        continue;
                    };
                    let metadata = lock.metadata()?;
                    let link = std::fs::symlink_metadata(&lock_path)?;
                    if !link.is_file() || metadata.uid() != uid || !same_file(&metadata, &link) {
                        continue;
                    }
                    lock
                }
                Err(_) => continue,
            };
            if lock.try_lock().is_err() {
                continue;
            }
            if let Ok(metadata) = std::fs::symlink_metadata(&path) {
                if !metadata.file_type().is_socket() || metadata.uid() != uid {
                    continue;
                }
                // A live server with a full backlog must not block a Tokio
                // worker while we look for a free slot.
                match tokio::time::timeout(Duration::from_millis(100), UnixStream::connect(&path))
                    .await
                {
                    Ok(Err(error)) if error.kind() == io::ErrorKind::ConnectionRefused => {
                        if !std::fs::symlink_metadata(&path)
                            .is_ok_and(|now| same_file(&metadata, &now))
                        {
                            continue;
                        }
                        std::fs::remove_file(&path)?;
                    }
                    _ => continue,
                }
            }
            let Ok(listener) = UnixListener::bind(&path) else {
                continue;
            };
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
            let identity = std::fs::symlink_metadata(&path)?;
            return Ok(Self {
                listener,
                path,
                identity,
                _lock: lock,
            });
        }
        bail!("no free Discord IPC socket in {}", directory.display())
    }
}

fn same_file(left: &Metadata, right: &Metadata) -> bool {
    left.ino() == right.ino() && left.dev() == right.dev()
}

impl Drop for Socket {
    fn drop(&mut self) {
        if std::fs::symlink_metadata(&self.path).is_ok_and(|now| same_file(&self.identity, &now)) {
            let _ = std::fs::remove_file(&self.path);
        }
        // Leave the lock inode in place: unlinking a locked file would let
        // another process create a second independent lock for the same slot.
    }
}

async fn serve(
    socket: Socket,
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
            accepted = socket.listener.accept(), if clients.len() < MAX_CLIENTS => {
                let Ok((stream, _)) = accepted else {
                    // Out of descriptors, say: wait rather than end RPC for good.
                    tokio::time::sleep(Duration::from_millis(500)).await;
                    continue;
                };
                if !stream.peer_cred().is_ok_and(|peer| peer.uid() == socket.identity.uid()) {
                    continue;
                }
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
            Some(done) = clients.join_next(), if !clients.is_empty() => done.ok(),
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
    // Dropping JoinSet aborts clients, and Socket removes only our own inode.
}

async fn read(stream: &mut UnixStream) -> Result<(u32, Vec<u8>)> {
    let opcode = stream.read_u32_le().await?;
    let size = stream.read_u32_le().await? as usize;
    ensure!(size <= MAX_PACKET, "RPC packet too large");
    let mut payload = vec![0; size];
    stream.read_exact(&mut payload).await?;
    Ok((opcode, payload))
}

async fn write(stream: &mut UnixStream, opcode: u32, payload: &[u8]) -> Result<()> {
    tokio::time::timeout(WAIT, async {
        stream.write_u32_le(opcode).await?;
        stream.write_u32_le(payload.len() as u32).await?;
        stream.write_all(payload).await
    })
    .await??;
    Ok(())
}

async fn send(stream: &mut UnixStream, value: Value) -> Result<()> {
    write(stream, 1, &serde_json::to_vec(&value)?).await
}

async fn connection(
    mut stream: UnixStream,
    api: Arc<Api>,
    snapshots: watch::Receiver<Arc<Snapshot>>,
    updates: mpsc::Sender<(u64, Option<Arc<Value>>)>,
    id: u64,
) -> Result<()> {
    let (opcode, payload) = tokio::time::timeout(WAIT, read(&mut stream)).await??;
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
    loop {
        let (opcode, payload) = read(&mut stream).await?;
        match opcode {
            2 => return Ok(()),
            3 => {
                write(&mut stream, 4, &payload).await?;
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
                &mut stream,
                json!({"cmd":cmd,"nonce":nonce,"evt":"ERROR",
                "data":{"code":4002,"message":"Only SET_ACTIVITY is supported"}}),
            )
            .await?;
            continue;
        }
        let result = async {
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
            let (addresses, keys): (Vec<&str>, Vec<&str>) = ["large_image", "small_image"]
                .iter()
                .filter_map(|key| pictures[key].as_str())
                .partition(|image| web_address(image, 512));
            let addresses: Vec<&str> = addresses
                .into_iter()
                .filter(|address| !images.contains_key(*address))
                .collect();
            if assets.is_none() && !keys.is_empty() {
                let response = tokio::time::timeout(WAIT, api.rpc_assets(&client)).await??;
                assets = Some(asset_lookup(&response)?);
            }
            if !addresses.is_empty() {
                // As the desktop client does, Discord's media proxy takes them.
                let response =
                    tokio::time::timeout(WAIT, api.rpc_external_assets(&client, &addresses))
                        .await??;
                if images.len() + addresses.len() > KEPT_IMAGES {
                    images.clear();
                }
                images.extend(external_lookup(&response)?);
            }
            normalize(&args["activity"], &client, &name, assets.as_ref(), &images)
                .map(|value| Some(Arc::new(value)))
        }
        .await;
        match result {
            Ok(value) => {
                // Bound queued updates even when games publish very frequently.
                updates.send((id, value.clone())).await?;
                send(
                    &mut stream,
                    json!({"cmd":cmd,"nonce":nonce,"evt":null,"data":value}),
                )
                .await?;
            }
            Err(error) => {
                send(
                    &mut stream,
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
    name: &str,
    assets: Option<&Assets>,
    addresses: &Assets,
) -> Result<Value> {
    ensure!(input.is_object(), "activity must be an object");
    let kind = match input.get("type") {
        None => 0,
        Some(value) => value.as_u64().context("invalid activity type")?,
    };
    ensure!(matches!(kind, 0 | 2 | 3 | 5), "unsupported activity type");
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

    struct Directory(PathBuf);
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
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn converts_sdk_fields_and_uses_the_handshakes_application() {
        let assets = asset_lookup(&json!([{"name":"cover","id":"1234"}])).unwrap();
        let shown = normalize(
            &json!({"name":"Spoof","application_id":"999","state":"In game",
            "timestamps":{"start":1700000000_u64,"end":1700000123000_u64},
            "assets":{"large_image":"cover","small_image":"5678","large_text":"Hover"},
            "buttons":[{"label":"Website","url":"https://example.com"}],
            "metadata":{"injected":true},"id":"spoof"}),
            "42",
            "Game",
            Some(&assets),
            &Assets::new(),
        )
        .unwrap();
        assert_eq!(shown["type"], 0);
        assert_eq!(shown["name"], "Game");
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
        let (mut client, mut server) = UnixStream::pair().unwrap();
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
}
