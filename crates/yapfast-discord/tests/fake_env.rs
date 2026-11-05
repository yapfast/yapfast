//! Yapfast's Discord service against testing/fake_discord.ts running an
//! environment, testing/envs/example.json, in place of its fixture. Start the
//! fake with `--env testing/envs/example.json`, then run with
//! YAPFAST_DISCORD_TEST_SERVER pointing at it:
//! `cargo test -p yapfast-discord --test fake_env -- --ignored`.
//! scripts/test-fake-discord does both.

use std::{sync::Arc, time::Duration};

use tokio::time::timeout;
use yapfast_discord::{
    api::Span,
    client,
    member_list::Presence,
    rtp::Codec,
    service::{Discord, Live, Snapshot, Status},
    stream,
    stream_connection::StreamReceiver,
    token,
    voice::{Call, Listener},
};

const WAIT: Duration = Duration::from_secs(10);
/// The example's own files, as the fake serves them.
const EXAMPLE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testing/envs/example");

struct Quiet;

impl Listener for Quiet {
    fn tick(&self, _: &[(u64, &[i16])]) {}
    fn disconnected(&self, reason: &str) {
        panic!("voice disconnected: {reason}");
    }
}

/// One of the fake's controls: its status and body.
fn control(server: &str, method: &str, path: &str) -> (u16, String) {
    use std::io::{Read, Write};
    let address = server.trim_start_matches("http://");
    let mut connection = std::net::TcpStream::connect(address).unwrap();
    write!(
        connection,
        "{method} {path} HTTP/1.1\r\nHost: {address}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    )
    .unwrap();
    let mut answer = String::new();
    connection.read_to_string(&mut answer).unwrap();
    let status = answer[9..12].parse().unwrap();
    let body = answer
        .split_once("\r\n\r\n")
        .map(|(_, body)| body.to_owned());
    (status, body.unwrap_or_default())
}

async fn snapshot_where(discord: &Discord, check: impl Fn(&Snapshot) -> bool) -> Arc<Snapshot> {
    let mut snapshots = discord.snapshots();
    timeout(WAIT, async {
        loop {
            let snapshot = snapshots.borrow_and_update().clone();
            if let Status::Offline(reason) = &snapshot.status {
                panic!("offline: {reason}");
            }
            if check(&snapshot) {
                return snapshot;
            }
            snapshots.changed().await.unwrap();
        }
    })
    .await
    .expect("timed out waiting for a snapshot")
}

async fn live_where<T>(
    live: &mut tokio::sync::broadcast::Receiver<Live>,
    pick: impl Fn(Live) -> Option<T>,
) -> T {
    timeout(WAIT, async {
        loop {
            if let Some(found) = pick(live.recv().await.unwrap()) {
                return found;
            }
        }
    })
    .await
    .expect("timed out waiting for a live event")
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs testing/fake_discord.ts --env testing/envs/example.json and YAPFAST_DISCORD_TEST_SERVER"]
async fn shows_the_example_environment() {
    let server = client::test_server().expect("set YAPFAST_DISCORD_TEST_SERVER");
    let discord = Discord::start(token::load().unwrap()).unwrap();

    // READY: the account, its servers with their channels, and who sits in
    // voice, from READY_SUPPLEMENTAL.
    let snapshot = snapshot_where(&discord, |snapshot| {
        snapshot.status == Status::Online && !snapshot.voice.is_empty()
    })
    .await;
    let account = snapshot.account.as_ref().unwrap();
    assert_eq!(account.name, "Robin");
    assert!(account.avatar.contains("/avatars/1400000000000000001/env"));
    assert_eq!(
        snapshot
            .custom_status
            .as_ref()
            .map(|status| status.text.as_str()),
        Some("Planting tomatoes")
    );
    let names: Vec<_> = snapshot
        .servers
        .iter()
        .map(|server| server.name.as_str())
        .collect();
    assert_eq!(names.len(), 2);
    assert!(names.contains(&"Pixel Garden") && names.contains(&"Weekend Hikers"));
    let garden = snapshot
        .servers
        .iter()
        .find(|server| server.name == "Pixel Garden")
        .unwrap();
    assert!(garden.icon.is_some());
    let text: Vec<_> = garden
        .text_channels
        .iter()
        .map(|channel| channel.name.as_str())
        .collect();
    assert_eq!(text, ["welcome", "general", "showcase"]);
    let voice: Vec<_> = garden
        .voice_channels
        .iter()
        .map(|channel| channel.name.as_str())
        .collect();
    assert_eq!(voice, ["Greenhouse", "Potting shed"]);
    let general = garden.text_channels().nth(1).unwrap();
    assert_eq!(
        general.topic.as_deref(),
        Some("Seeds, soil and everything green. Be kind.")
    );
    assert!(general.marks.unread && general.marks.mentions == 1);
    assert!(!garden.text_channels().nth(2).unwrap().marks.unread);
    let greenhouse = &garden.voice_channels[0];
    let seated: Vec<_> = snapshot.voice[&greenhouse.id]
        .iter()
        .map(|member| {
            (
                member.name.as_str(),
                member.streaming,
                member.muted,
                member.deafened,
                member.video,
            )
        })
        .collect();
    assert_eq!(seated.len(), 4);
    for expected in [
        ("Maya", true, false, false, false),
        ("Theo 🌱", false, true, false, false),
        ("Priya", false, true, true, false),
        ("Jonas", false, false, false, true),
    ] {
        assert!(seated.contains(&expected), "{expected:?} in {seated:?}");
    }

    // DMs and the group, the latest first; unread DM messages are mentions.
    let listed: Vec<_> = snapshot
        .conversations
        .iter()
        .map(|dm| (dm.name.as_str(), dm.group, dm.unread, dm.mentions))
        .collect();
    assert_eq!(
        listed,
        [
            ("Maya", false, true, 2),
            ("Board game night", true, true, 1),
            ("Theo", false, false, 0)
        ]
    );
    assert_eq!(snapshot.friends.len(), 3);
    assert_eq!(snapshot.friend_requests.len(), 1);
    assert_eq!(snapshot.friend_requests[0].name, "Lena");

    // A picture comes from the env's file.
    let avatar = discord
        .download(
            snapshot.conversations[0].avatar.as_deref().unwrap(),
            1 << 20,
        )
        .await
        .unwrap();
    assert_eq!(
        avatar,
        std::fs::read(format!("{EXAMPLE}/maya.png")).unwrap()
    );

    // #general's history: mentions, a reply, reactions, a photo and a video.
    let (guild, general) = (garden.id.clone(), general.id.clone());
    let history = discord
        .history(&guild, &general, Span::Latest)
        .await
        .unwrap()
        .messages;
    assert_eq!(history.len(), 8);
    assert_eq!(history[0].author, "Theo");
    let photo = &history[1];
    assert_eq!(
        (
            photo.reactions[0].emoji.as_str(),
            photo.reactions[0].count,
            photo.reactions[0].me
        ),
        ("🌅", 3, true)
    );
    let image = &photo.attachments[0];
    assert_eq!(
        (image.name.as_str(), image.dimensions),
        ("sunrise.jpg", Some((960, 600)))
    );
    let jpeg = discord.download(&image.url, 1 << 20).await.unwrap();
    assert_eq!(
        jpeg,
        std::fs::read(format!("{EXAMPLE}/sunrise.jpg")).unwrap()
    );
    assert!(history[3].reply.is_some(), "Priya answers Robin");
    assert!(history[4].bot);
    let clip = &history[6].attachments[0];
    assert_eq!(clip.dimensions, Some((480, 270)));
    assert!(clip.video_url.is_some());
    let poster = discord
        .download(clip.preview_url.as_deref().unwrap(), 1 << 20)
        .await
        .unwrap();
    assert!(poster.starts_with(b"\x89PNG"), "the clip's first frame");
    assert_eq!(
        history[7].content,
        "<@1400000000000000001> can you check the shed key is back on its hook?"
    );

    // The member list: hoisted roles, then online, then offline.
    let mut live = discord.live();
    discord.view(&guild, Some(&general));
    let members = live_where(&mut live, |event| match event {
        Live::Members { channel, members } if channel == general => Some(members),
        _ => None,
    })
    .await;
    let rows: Vec<_> = members
        .iter()
        .map(|member| (member.name.as_str(), member.group.as_str(), member.presence))
        .collect();
    assert_eq!(
        rows,
        [
            ("Maya", "Gardeners — 1", Presence::Online),
            ("Robin", "Helpers — 2", Presence::Online),
            ("Theo 🌱", "Helpers — 2", Presence::Idle),
            ("Jonas", "Online — 3", Presence::Mobile),
            ("Priya", "Online — 3", Presence::DoNotDisturb),
            ("Sprout", "Online — 3", Presence::Online),
            ("Lena", "Offline — 1", Presence::Offline),
        ]
    );
    assert_eq!(members[0].color, Some(0xe91e63), "Gardeners' pink");
    assert_eq!(
        members[0].activity.as_deref(),
        Some("Playing Pixel Painter")
    );

    // A profile: bio, pronouns, banner and the servers shared.
    let maya = members[0].user_id.clone();
    let profile = discord
        .profile(&guild, &maya, Default::default(), |_| {})
        .await
        .unwrap();
    assert_eq!(
        profile.bio.as_deref(),
        Some("Draws tiny worlds, one pixel at a time.")
    );
    assert_eq!(profile.pronouns.as_deref(), Some("she/her"));
    assert!(profile.banner.is_some());
    assert_eq!(profile.mutual_servers.len(), 1);

    // Watching Maya's stream sends the env's video, converted to VP8 at its size.
    let greenhouse = greenhouse.id.clone();
    let assigned = discord.join(&guild, &greenhouse).await.unwrap();
    let _call = timeout(
        WAIT,
        Call::connect(&assigned, &guild, &greenhouse, Arc::new(Quiet)),
    )
    .await
    .expect("timed out connecting to voice")
    .unwrap();
    let watched = discord
        .watch(&stream::key(&guild, &greenhouse, &maya))
        .await
        .unwrap();
    let frames = tokio::task::spawn_blocking(move || {
        let receiver = StreamReceiver::start(watched, vec![Codec::H264, Codec::Vp8]).unwrap();
        let deadline = std::time::Instant::now() + WAIT;
        let mut frames = vec![];
        while frames.len() < 12 {
            assert!(
                std::time::Instant::now() < deadline,
                "only {} frames",
                frames.len()
            );
            if let Some(frame) = receiver.next(Duration::from_millis(100)) {
                frames.push(frame);
            }
        }
        frames
    })
    .await
    .unwrap();
    let first = &frames[0];
    assert!(first.keyframe);
    let size = |at: usize| u16::from_le_bytes([first.data[at], first.data[at + 1]]) & 0x3fff;
    assert_eq!(&first.data[3..6], &[0x9d, 0x01, 0x2a], "a VP8 keyframe");
    assert_eq!((size(6), size(8)), (640, 360));

    // Controls that act on the fixture's people answer that they can't.
    assert_eq!(control(server, "POST", "/_control/bob-away").0, 400);
    assert_eq!(control(server, "GET", "/_control/health").0, 200);
    discord.leave(&guild);
}
