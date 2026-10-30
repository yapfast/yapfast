//! Yapfast's Discord service against testing/fake_discord.ts, which each
//! test starts afresh with bun. `scripts/test-fake-discord [FILTER]` runs
//! them, side by side under cargo nextest.

use std::{sync::Arc, time::Duration};

use tokio::time::timeout;
use yapfast_discord::{
    api::Span,
    client,
    member_list::{ListedMember, Presence},
    message::Shown,
    notifications::{Change, Level},
    ready::{Access, Deletes, PRIVATE},
    remote_auth::{self, Step},
    rtp::Codec,
    service::{Discord, Live, Snapshot, Status},
    settings::CustomStatus,
    stream,
    stream_connection::{Frame, Link, SEND_CODEC, Sound, StreamReceiver, StreamSender},
    token,
    voice::{Call, Listener},
};

const WAIT: Duration = Duration::from_secs(10);

/// A call's audio, unheard.
struct Quiet;

impl Listener for Quiet {
    fn tick(&self, _: &[(u64, &[i16])]) {}
    fn disconnected(&self, reason: &str) {
        panic!("voice disconnected: {reason}");
    }
}

/// A call's audio: the loudest sample heard from each user.
#[derive(Default)]
struct Hearer(std::sync::Mutex<std::collections::HashMap<u64, i16>>);

impl Listener for Hearer {
    fn tick(&self, voices: &[(u64, &[i16])]) {
        let mut heard = self.0.lock().unwrap();
        for (user, samples) in voices {
            let loudest = samples
                .iter()
                .map(|sample| sample.saturating_abs())
                .max()
                .unwrap_or(0);
            let entry = heard.entry(*user).or_default();
            *entry = (*entry).max(loudest);
        }
    }
    fn disconnected(&self, reason: &str) {
        panic!("voice disconnected: {reason}");
    }
}

/// What the fake heard from the call connection that identified with `token`.
fn call_audio(server: &str, token: &str) -> serde_json::Value {
    let calls: serde_json::Value =
        serde_json::from_str(&control(server, "GET", "/_control/call-audio", "")).unwrap();
    calls
        .as_object()
        .unwrap()
        .values()
        .rev()
        .find(|call| call["token"] == token)
        .cloned()
        .unwrap_or_default()
}

/// What the fake knows of cameras in calls: what Alice's sent, what each
/// call connection asked for, and Bob's camera sent to them.
fn call_video(server: &str) -> serde_json::Value {
    serde_json::from_str(&control(server, "GET", "/_control/call-video", "")).unwrap()
}

/// Cameras both ways in Alice's call in `channel` of `guild`, whose
/// connection identified with `token`. Alice films: Discord hears of it in
/// her voice state and op 12, and her frames go out, one resent when the
/// fake reports it lost. Bob films: his frames come in whole once asked for,
/// one repaired, and stop when no longer wanted or when he stops.
async fn cameras_in_call(
    server: &str,
    discord: &Discord,
    call: &mut Call,
    (guild, channel, token): (&str, &str, &str),
    bob: u64,
) {
    let me = discord.snapshots().borrow().user_id.clone().unwrap();
    let filming = |user: &str, on: bool| {
        let user = user.to_owned();
        let channel = channel.to_owned();
        move |snapshot: &Snapshot| {
            snapshot.voice.get(&channel).is_some_and(|members| {
                members
                    .iter()
                    .any(|member| member.user_id == user && member.video == on)
            })
        }
    };
    let frames = call.video_frames().expect("the call's camera frames");
    assert!(call.video_frames().is_none(), "one taker");

    discord.set_self(guild, channel, false, false, true);
    snapshot_where(discord, filming(&me, true)).await;
    let camera = call.film(320, 180, 10);
    let recorded = recording(Codec::H264);
    for (index, data) in recorded.iter().take(10).enumerate() {
        camera.send(Frame {
            data: data.clone(),
            codec: Codec::H264,
            timestamp: index as u32 * 9000,
            keyframe: index == 0,
        });
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let mine = |video: &serde_json::Value| {
        video["cameras"]
            .as_array()
            .unwrap()
            .iter()
            .find(|camera| camera["stream"] == format!("call:{token}"))
            .cloned()
            .unwrap_or_default()
    };
    until("Alice's camera went out", || {
        let sent = mine(&call_video(server));
        sent["frames"].as_u64() >= Some(10) && sent["retransmitted"] == 1
    })
    .await;
    let sent = mine(&call_video(server));
    assert_eq!(sent["codec"], "H264", "{sent}");
    assert_eq!(
        (sent["width"].as_u64(), sent["height"].as_u64()),
        (Some(320), Some(180))
    );
    assert_eq!(
        sent["announced"],
        serde_json::json!({"type": "fixed", "width": 320, "height": 180})
    );
    assert!(sent["keyframes"].as_u64() >= Some(1), "{sent}");
    assert_eq!(
        (
            &sent["undecryptable"],
            &sent["unattached"],
            &sent["badRetransmissions"]
        ),
        (
            &serde_json::json!(0),
            &serde_json::json!(0),
            &serde_json::json!(0)
        ),
        "{sent}"
    );
    assert!(sent["senderReports"].as_u64() >= Some(1), "{sent}");
    drop(camera);
    discord.set_self(guild, channel, false, false, false);
    until("Alice's camera stopped", || {
        mine(&call_video(server))["stops"] == 1
    })
    .await;
    snapshot_where(discord, filming(&me, false)).await;

    // Bob's camera, asked for at full quality for a 320 by 180 tile.
    call.want_video([(bob, (100, 320 * 180))].into());
    post_control(server, "/_control/camera", "");
    snapshot_where(discord, filming(&bob.to_string(), true)).await;
    let mut came = vec![];
    timeout(WAIT, async {
        while came.len() < 12 {
            match frames.try_recv() {
                Ok(frame) => came.push(frame),
                Err(_) => tokio::time::sleep(Duration::from_millis(10)).await,
            }
        }
    })
    .await
    .unwrap_or_else(|_| panic!("only {} of Bob's frames came", came.len()));
    let expected = recording(Codec::H264);
    assert!(came[0].keyframe, "Bob's camera starts with a keyframe");
    for (index, frame) in came.iter().enumerate() {
        assert_eq!((frame.user, frame.codec), (bob, Codec::H264));
        assert!(frame.data == expected[index], "Bob's frame {index} differs");
    }
    let watched = call_video(server);
    assert!(
        watched["repairs"]["nacks"].as_u64() >= Some(1)
            && watched["repairs"]["resent"].as_u64() >= Some(1),
        "{watched}"
    );
    // Off screen: asked for at no quality, and no longer sent.
    call.want_video(std::collections::HashMap::new());
    until("Bob's camera was no longer asked for", || {
        call_video(server)["wants"]
            .as_object()
            .unwrap()
            .values()
            .any(|wants| wants["6001"] == 0)
    })
    .await;
    let sent_to = || {
        call_video(server)["watchers"]
            .as_array()
            .unwrap()
            .iter()
            .map(|watcher| watcher["framesSent"].as_u64().unwrap())
            .sum::<u64>()
    };
    let stopped = sent_to();
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(sent_to(), stopped, "Bob's camera still came");
    post_control(server, "/_control/camera?off", "");
    snapshot_where(discord, filming(&bob.to_string(), false)).await;
    while frames.try_recv().is_ok() {}
}

/// Waits until `check` holds, polling the fake.
async fn until(what: &str, mut check: impl FnMut() -> bool) {
    let started = std::time::Instant::now();
    while !check() {
        assert!(started.elapsed() < WAIT, "timed out waiting until {what}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Speaks `frames` 20 ms frames of a tone into the microphone, in time.
async fn say(microphone: &yapfast_discord::voice::Microphone, frames: usize) {
    for frame in 0..frames {
        let tone: Vec<f32> = (0..yapfast_discord::voice::FRAME)
            .map(|at| {
                let t = (frame * yapfast_discord::voice::FRAME + at) as f32 / 48_000.0;
                (t * 440.0 * std::f32::consts::TAU).sin() * 0.4
            })
            .collect();
        microphone.push(&tone);
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// The recordings the fake streams as Bob, as Yapfast should reassemble them.
fn recording(codec: Codec) -> Vec<Vec<u8>> {
    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testing/fixtures");
    match codec {
        // IVF: a header, then frames after 12-byte headers with their size.
        Codec::Vp8 => {
            let file = std::fs::read(fixtures.join("stream-vp8.ivf")).unwrap();
            let mut at = usize::from(u16::from_le_bytes([file[6], file[7]]));
            let mut frames = vec![];
            while at + 12 <= file.len() {
                let size = u32::from_le_bytes(file[at..at + 4].try_into().unwrap()) as usize;
                frames.push(file[at + 12..at + 12 + size].to_vec());
                at += 12 + size;
            }
            frames
        }
        // OBUs, split into temporal units at their delimiters, as Yapfast
        // reassembles them: without the delimiter, each OBU giving its size
        // but the last, as DAVE leaves it.
        Codec::Av1 => {
            let file = std::fs::read(fixtures.join("stream-av1.obu")).unwrap();
            let mut frames: Vec<Vec<u8>> = vec![];
            let mut at = 0;
            while at < file.len() {
                // Each OBU gives its size, which fits in one LEB128 byte or two.
                let (size, used) = match file[at + 1] {
                    low if low & 0x80 == 0 => (usize::from(low), 1),
                    low => (usize::from(low & 0x7f) | usize::from(file[at + 2]) << 7, 2),
                };
                let end = at + 1 + used + size;
                if file[at] >> 3 & 0x0f == 2 {
                    frames.push(vec![]);
                }
                frames.last_mut().unwrap().extend_from_slice(&file[at..end]);
                at = end;
            }
            frames
                .iter()
                .map(|unit| {
                    let elements = yapfast_discord::rtp::obu_elements(unit).unwrap();
                    let mut frame = vec![];
                    for (index, element) in elements.iter().enumerate() {
                        if index + 1 == elements.len() {
                            frame.extend_from_slice(element);
                            break;
                        }
                        // libaom's OBUs have no extension byte, and sizes
                        // up to 16383 take two LEB128 bytes at most.
                        let size = element.len() - 1;
                        frame.push(element[0] | 0x02);
                        match size {
                            ..128 => frame.push(size as u8),
                            _ => frame.extend_from_slice(&[size as u8 | 0x80, (size >> 7) as u8]),
                        }
                        frame.extend_from_slice(&element[1..]);
                    }
                    frame
                })
                .collect()
        }
        // Annex B, split at access unit delimiters, which aren't sent; each
        // unit gets a four-byte start code.
        Codec::H264 | Codec::H265 => {
            let (name, delimiter): (_, fn(&[u8]) -> bool) = match codec {
                Codec::H264 => ("stream-h264.h264", |unit: &[u8]| unit[0] & 0x1f == 9),
                _ => ("stream-h265.h265", |unit: &[u8]| unit[0] >> 1 & 0x3f == 35),
            };
            let file = std::fs::read(fixtures.join(name)).unwrap();
            let starts: Vec<usize> = (0..file.len() - 2)
                .filter(|&at| file[at..at + 3] == [0, 0, 1])
                .map(|at| at + 3)
                .collect();
            let mut frames: Vec<Vec<u8>> = vec![];
            for (index, &start) in starts.iter().enumerate() {
                let mut end = starts.get(index + 1).map_or(file.len(), |next| next - 3);
                if file[end - 1] == 0 {
                    end -= 1;
                }
                let unit = &file[start..end];
                if delimiter(unit) || frames.is_empty() {
                    frames.push(vec![]);
                }
                if !delimiter(unit) {
                    frames.last_mut().unwrap().extend_from_slice(&[0, 0, 0, 1]);
                    frames.last_mut().unwrap().extend_from_slice(unit);
                }
            }
            frames.retain(|frame| !frame.is_empty());
            frames
        }
    }
}

/// Watches Bob's stream, decoding `decodes`, until `count` frames arrive,
/// and checks his sound came too: the fake sends Opus silence.
fn watch_frames(
    watched: &yapfast_discord::stream::StreamServer,
    decodes: Vec<Codec>,
    count: usize,
) -> Vec<(Codec, Vec<u8>, bool)> {
    let mut receiver = StreamReceiver::start(watched.clone(), decodes).unwrap();
    let sounds = receiver.take_sounds().unwrap();
    assert!(receiver.take_sounds().is_none(), "one player");
    let deadline = std::time::Instant::now() + WAIT;
    let mut frames = vec![];
    while frames.len() < count {
        assert!(
            std::time::Instant::now() < deadline,
            "only {} frames arrived: {:?}",
            frames.len(),
            receiver.link()
        );
        if let Some(frame) = receiver.next(Duration::from_millis(100)) {
            frames.push((frame.codec, frame.data, frame.keyframe));
        }
    }
    assert_eq!(receiver.link(), Link::Connected);
    let heard: Vec<Vec<u8>> = sounds.try_iter().collect();
    assert!(heard.len() >= 5, "only {} sounds arrived", heard.len());
    assert!(heard.iter().all(|sound| sound == &[0xf8, 0xff, 0xfe]));
    frames
}

/// Offers `gpu`, AV1 or H265, first, as a GPU encoder lets Yapfast:
/// Discord chooses it, and when a viewer whose client decodes only H264 and
/// VP8 starts watching, it switches the stream to H264 with op 14, and
/// Yapfast asks for a keyframe to send in it.
async fn streams_gpu_codec_until_a_viewer_cant_decode_it(
    discord: &Discord,
    server: &str,
    mine: &yapfast_discord::stream::StreamServer,
    gpu: Codec,
) {
    let sender = StreamSender::start(mine.clone(), 320, 180, 30, vec![gpu, SEND_CODEC]).unwrap();
    let chosen = |codec| {
        let sender = &sender;
        timeout(WAIT, async move {
            while sender.codec() != Some(codec) {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
    };
    chosen(gpu).await.expect("Discord chose the GPU's codec");
    assert!(sender.take_keyframe_request());
    let send = |codec, from: usize| {
        for (index, data) in recording(codec).into_iter().take(3).enumerate() {
            sender.send(Frame {
                data,
                codec,
                timestamp: (from + index) as u32 * 3000,
                keyframe: index == 0,
            });
        }
    };
    let received = |frames| async move {
        timeout(WAIT, async {
            loop {
                let body = discord
                    .download(&format!("{server}/_control/video"), 1 << 16)
                    .await
                    .unwrap();
                let streams: serde_json::Value = serde_json::from_slice(&body).unwrap();
                let last = streams["received"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .rfind(|found| found["stream"] == mine.stream_key.as_str())
                    .cloned();
                if let Some(found) = last.filter(|found| found["frames"] == frames) {
                    return found;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("the fake didn't get {frames} frames"))
    };
    send(gpu, 0);
    let video = received(3).await;
    assert_eq!(
        (
            &video["codec"],
            &video["keyframes"],
            &video["width"],
            &video["height"]
        ),
        (
            &serde_json::json!(gpu.name()),
            &serde_json::json!(1),
            &serde_json::json!(320),
            &serde_json::json!(180)
        ),
        "H265's conformance window crops the coded 184 lines"
    );

    post_control(
        server,
        "/_control/stream-viewer?user=bob&decodes=h264,vp8",
        "",
    );
    chosen(Codec::H264).await.expect("Discord switched to H264");
    assert!(sender.take_keyframe_request(), "the new codec starts whole");
    // Frames still in the GPU's codec are dropped rather than sent as H264.
    send(gpu, 3);
    send(Codec::H264, 6);
    let video = received(6).await;
    assert_eq!(
        (
            &video["codec"],
            &video["keyframes"],
            &video["undecryptable"]
        ),
        (
            &serde_json::json!("H264"),
            &serde_json::json!(2),
            &serde_json::json!(0)
        )
    );
    post_control(server, "/_control/stream-viewer?user=bob&stop", "");
    chosen(gpu)
        .await
        .expect("back to the GPU's codec without him");
}

async fn wait_for_stream_video(
    discord: &Discord,
    server: &str,
    stream_key: &str,
    mut ready: impl FnMut(&serde_json::Value) -> bool,
) -> serde_json::Value {
    timeout(WAIT, async {
        loop {
            let body = discord
                .download(&format!("{server}/_control/video"), 1 << 16)
                .await
                .unwrap();
            let streams: serde_json::Value = serde_json::from_slice(&body).unwrap();
            let last = streams["received"]
                .as_array()
                .unwrap()
                .iter()
                .rfind(|found| found["stream"] == stream_key)
                .cloned();
            if let Some(found) = last
                && ready(&found)
            {
                return found;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("the fake didn't receive the expected frames for {stream_key}"))
}

async fn send_recording(sender: &StreamSender, codec: Codec, from: usize, count: usize) {
    let frames = recording(codec);
    assert!(
        frames.len() >= count,
        "not enough {codec:?} recording frames"
    );
    for (index, data) in frames.into_iter().take(count).enumerate() {
        sender.send(Frame {
            data,
            codec,
            timestamp: (from + index) as u32 * 3000,
            keyframe: index == 0,
        });
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
}

/// Offers the AV1, H265 and H264 encoders in priority order. The fake picks
/// AV1, then H265 for an AV1-incompatible viewer, then AV1 again. Withdrawing
/// AV1 and H265 in turn leaves H264 sending usable frames.
async fn streams_av1_with_viewer_fallback_and_encoder_withdrawal(
    discord: &Discord,
    server: &str,
    mine: &yapfast_discord::stream::StreamServer,
) {
    let sender = StreamSender::start(
        mine.clone(),
        320,
        180,
        30,
        vec![Codec::Av1, Codec::H265, Codec::H264],
    )
    .unwrap();
    let chosen = |codec| {
        let sender = &sender;
        timeout(WAIT, async move {
            while sender.codec() != Some(codec) {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
    };
    chosen(Codec::Av1).await.expect("Discord chose AV1");
    assert!(sender.take_keyframe_request());
    let initial = wait_for_stream_video(discord, server, &mine.stream_key, |video| {
        video["codec"] == "AV1"
    })
    .await;
    let offers = initial["offers"].as_array().unwrap();
    assert_eq!(
        offers
            .iter()
            .map(|offer| offer["name"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["AV1", "H265", "H264"]
    );
    assert_eq!(
        offers
            .iter()
            .map(|offer| offer["payloadType"].as_u64().unwrap())
            .collect::<Vec<_>>(),
        vec![107, 103, 101]
    );
    assert!(offers.iter().all(|offer| offer["encode"] == true));
    assert_eq!(initial["payloadType"], 107);

    send_recording(&sender, Codec::Av1, 0, 3).await;
    let first_av1 = wait_for_stream_video(discord, server, &mine.stream_key, |video| {
        video["av1Frames"] == 3 && video["retransmitted"].as_u64().unwrap_or(0) >= 1
    })
    .await;
    assert_eq!(first_av1["codec"], "AV1");
    assert_eq!(first_av1["payloadType"], 107);
    assert_eq!(first_av1["frames"], 3);
    assert_eq!(first_av1["av1Frames"], 3);
    assert_eq!(first_av1["av1Markers"], 3);
    assert_eq!(first_av1["av1MalformedPackets"], 0);
    assert_eq!(
        first_av1["av1Packets"], first_av1["av1ValidPackets"],
        "every AV1 RTP payload passed the fake's aggregation/OBU checks"
    );
    assert!(first_av1["av1FragmentedPackets"].as_u64().unwrap() > 0);
    assert!(first_av1["av1Obus"].as_u64().unwrap() > 0);
    assert_eq!(first_av1["lastRtxPayloadType"], 108);
    assert_eq!(first_av1["badRetransmissions"], 0);
    assert_eq!(first_av1["undecryptable"], 0);
    assert_eq!(first_av1["unattached"], 0);
    let av1_packets = first_av1["av1Packets"].as_u64().unwrap();

    post_control(
        server,
        "/_control/stream-viewer?user=bob&decodes=h265,h264",
        "",
    );
    chosen(Codec::H265)
        .await
        .expect("Discord switched to H265 for the incompatible viewer");
    assert!(
        sender.take_keyframe_request(),
        "the fallback codec starts with a keyframe"
    );
    send_recording(&sender, Codec::Av1, 3, 1).await;
    send_recording(&sender, Codec::H265, 3, 3).await;
    let h265 = wait_for_stream_video(discord, server, &mine.stream_key, |video| {
        video["codec"] == "H265" && video["frames"] == 6
    })
    .await;
    assert_eq!(h265["payloadType"], 103);
    assert_eq!(h265["av1Packets"].as_u64().unwrap(), av1_packets);
    assert_eq!(h265["av1MalformedPackets"], 0);
    assert_eq!(
        h265["undecryptable"], 0,
        "the AV1 frame was dropped while H265 was chosen"
    );

    post_control(server, "/_control/stream-viewer?user=bob&stop", "");
    chosen(Codec::Av1)
        .await
        .expect("Discord returned to AV1 after the viewer left");
    assert!(
        sender.take_keyframe_request(),
        "AV1 resumes with a keyframe"
    );
    send_recording(&sender, Codec::Av1, 6, 3).await;
    let av1_again = wait_for_stream_video(discord, server, &mine.stream_key, |video| {
        video["codec"] == "AV1" && video["av1Frames"] == 6
    })
    .await;
    assert_eq!(av1_again["payloadType"], 107);
    assert_eq!(av1_again["frames"], 9);
    assert_eq!(av1_again["av1Markers"], 6);
    assert_eq!(av1_again["av1MalformedPackets"], 0);
    assert_eq!(
        av1_again["av1Packets"], av1_again["av1ValidPackets"],
        "both AV1 bursts passed RTP/OBU validation"
    );
    assert_eq!(av1_again["av1Packets"].as_u64().unwrap(), av1_packets * 2);
    assert!(av1_again["av1FragmentedPackets"].as_u64().unwrap() > 0);
    assert_eq!(av1_again["badRetransmissions"], 0);

    sender.withdraw(Codec::Av1);
    // The next frame carries the offer update and is dropped until Discord
    // acknowledges the new codec.
    send_recording(&sender, Codec::H265, 9, 1).await;
    wait_for_stream_video(discord, server, &mine.stream_key, |video| {
        video["codec"] == "H265"
    })
    .await;
    timeout(WAIT, async {
        while !sender.take_keyframe_request() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the renegotiated H265 stream requests a keyframe");
    send_recording(&sender, Codec::H265, 9, 3).await;
    let h265_after_withdrawal = wait_for_stream_video(discord, server, &mine.stream_key, |video| {
        video["codec"] == "H265" && video["frames"] == 12
    })
    .await;
    assert_eq!(h265_after_withdrawal["payloadType"], 103);
    assert_eq!(
        h265_after_withdrawal["offers"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|offer| offer["encode"] == true)
            .map(|offer| offer["name"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["H265", "H264"]
    );

    sender.withdraw(Codec::H265);
    send_recording(&sender, Codec::H264, 12, 1).await;
    wait_for_stream_video(discord, server, &mine.stream_key, |video| {
        video["codec"] == "H264"
    })
    .await;
    timeout(WAIT, async {
        while !sender.take_keyframe_request() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the renegotiated H264 stream requests a keyframe");
    send_recording(&sender, Codec::H264, 12, 3).await;
    let h264 = wait_for_stream_video(discord, server, &mine.stream_key, |video| {
        video["codec"] == "H264" && video["frames"] == 15
    })
    .await;
    assert_eq!(h264["payloadType"], 101);
    let encodable: Vec<_> = h264["offers"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|offer| offer["encode"] == true)
        .map(|offer| offer["name"].as_str().unwrap())
        .collect();
    assert_eq!(encodable, vec!["H264"]);
    assert_eq!(
        h264["keyframes"].as_u64().unwrap(),
        h265_after_withdrawal["keyframes"].as_u64().unwrap() + 1
    );
    assert_eq!(h264["undecryptable"], 0);
    assert_eq!(h264["unattached"], 0);
}

/// Tells the fake which codec Bob streams in.
fn stream_codec(server: &str, codec: &str) {
    post_control(server, "/_control/stream-codec", codec);
}

/// Joins "Server" again by an invite to #general, as Join a Server does
/// with a vanity code: Discord asks for a captcha first, which a page
/// shows, and the server comes back with its GUILD_CREATE.
async fn joins_by_an_invite_as_discords_client_does(
    discord: &Discord,
    server: &str,
    general: &str,
) {
    use yapfast_discord::invite::JoinFrom;

    let unknown = discord.invite("gone", None).await.unwrap_err();
    assert_eq!(
        unknown.to_string(),
        "This invite is invalid or has expired."
    );
    let preview = discord.invite("server", Some("server")).await.unwrap();
    assert_eq!(
        (preview.name.as_str(), preview.channel.as_str()),
        ("Server", general)
    );
    assert!(preview.members.is_some());

    post_control(server, "/_control/invite-captcha", "");
    let mut pages = Vec::new();
    let joined = discord
        .join_invite(&preview, JoinFrom::Dialog, |page| {
            pages.push(page.to_owned());
            let page = page.to_owned();
            tokio::spawn(async move {
                reqwest::Client::new()
                    .post(page)
                    .body("solved")
                    .send()
                    .await
            });
        })
        .await
        .unwrap();
    assert_eq!(pages.len(), 1);
    assert_eq!(
        (joined.guild.as_str(), joined.channel.as_str()),
        (preview.guild.as_str(), general)
    );
    assert!(!joined.verification_form);
    let joins: serde_json::Value =
        serde_json::from_str(&control(server, "GET", "/_control/invite-joins", "")).unwrap();
    let join = joins.as_array().unwrap().last().unwrap();
    assert_eq!(join["code"], "server");
    assert_eq!(
        join["context"],
        serde_json::json!({
            "location": "Join Guild",
            "location_guild_id": preview.guild,
            "location_channel_id": general,
            "location_channel_type": 0,
        })
    );

    // A server with a form to fill in says so; she's in it already, which
    // joins nothing more.
    post_control(server, "/_control/invite-form", "");
    let joined = discord
        .join_invite(&preview, JoinFrom::Link, |_| panic!("no captcha"))
        .await
        .unwrap();
    assert!(joined.verification_form);
    let joins: serde_json::Value =
        serde_json::from_str(&control(server, "GET", "/_control/invite-joins", "")).unwrap();
    assert_eq!(
        joins.as_array().unwrap().last().unwrap()["context"]["location"],
        "Desktop Invite Modal"
    );
}

/// Posts `body` to one of the fake's controls.
fn post_control(server: &str, path: &str, body: &str) {
    control(server, "POST", path, body);
}

/// Asks one of the fake's controls, and returns its answer's body.
fn control(server: &str, method: &str, path: &str, body: &str) -> String {
    use std::io::{Read, Write};
    let address = server.trim_start_matches("http://");
    let mut connection = std::net::TcpStream::connect(address).unwrap();
    write!(
        connection,
        "{method} {path} HTTP/1.1\r\nHost: {address}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .unwrap();
    // Read the body by its length: Bun can leave the connection open after
    // an async handler's answer until its idle timeout, about 10 seconds.
    let mut answer = Vec::new();
    let mut byte = [0_u8];
    while !answer.ends_with(b"\r\n\r\n") {
        connection.read_exact(&mut byte).unwrap();
        answer.push(byte[0]);
    }
    let head = String::from_utf8(answer).unwrap();
    assert!(head.starts_with("HTTP/1.1 200"), "{head}");
    let length = head.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.eq_ignore_ascii_case("content-length")
            .then(|| value.trim().parse::<usize>().unwrap())
    });
    let mut body = Vec::new();
    match length {
        Some(length) => {
            body.resize(length, 0);
            connection.read_exact(&mut body).unwrap();
        }
        None => {
            connection.read_to_end(&mut body).unwrap();
        }
    }
    String::from_utf8(body).unwrap()
}

/// The presence Alice's session last sent the fake. Against it, Yapfast
/// holds presence updates back for 300 ms rather than Discord's 20 seconds,
/// and looks for games every half second rather than 15.
async fn presence_where(server: &str, check: impl Fn(&serde_json::Value) -> bool) {
    timeout(WAIT + Duration::from_secs(40), async {
        loop {
            let presence = serde_json::from_str(&control(server, "GET", "/_control/presence", ""))
                .unwrap_or_default();
            if check(&presence) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("timed out waiting for the presence");
}

/// The first snapshot that passes `check`.
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

/// The next live event that `pick` accepts.
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

/// A real SDK-shaped IPC exchange all the way to the fake Gateway, while
/// Alice has a custom status. The normal service runner opts in with a private
/// directory so this test never takes the desktop's Discord IPC socket.
#[cfg(target_os = "linux")]
async fn plays_through_rpc(discord: &Discord, server: &str, directory: &std::path::Path) {
    use serde_json::{Value, json};
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::UnixStream,
    };

    struct GameProcess {
        child: std::process::Child,
        _directory: tempfile::TempDir,
    }
    impl Drop for GameProcess {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
    let mut detected = if let Some(cache) = std::env::var_os("YAPFAST_DISCORD_GAMES_CACHE") {
        let executable = std::env::current_exe().unwrap();
        let directory = tempfile::tempdir_in(executable.parent().unwrap()).unwrap();
        let game = directory.path().join("yapfast_detected_game");
        std::fs::hard_link(executable, &game).unwrap();
        let child = std::process::Command::new(game)
            .args(["--exact", "waits_as_detected_game"])
            .env("YAPFAST_TEST_DETECTED_GAME", "1")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let process = GameProcess {
            child,
            _directory: directory,
        };
        presence_where(server, |p| {
            p["activities"]
                .as_array()
                .is_some_and(|a| a.iter().any(|a| a["name"] == "Detected Fixture"))
        })
        .await;
        let cache = std::path::PathBuf::from(cache);
        let cached: Value = serde_json::from_slice(&std::fs::read(&cache).unwrap()).unwrap();
        assert!(
            cached[0].get("description").is_none(),
            "discard unused remote metadata"
        );
        assert_eq!(
            std::fs::read_to_string(cache.with_extension("etag")).unwrap(),
            r#"W/"fixture-games""#,
            "the CDN's version, to ask with tomorrow"
        );
        Some(process)
    } else {
        None
    };

    async fn send(stream: &mut UnixStream, opcode: u32, value: Value) {
        let payload = serde_json::to_vec(&value).unwrap();
        stream.write_u32_le(opcode).await.unwrap();
        stream.write_u32_le(payload.len() as u32).await.unwrap();
        stream.write_all(&payload).await.unwrap();
    }
    async fn receive(stream: &mut UnixStream) -> Value {
        timeout(Duration::from_secs(20), async {
            assert_eq!(stream.read_u32_le().await.unwrap(), 1);
            let size = stream.read_u32_le().await.unwrap();
            assert!(size <= 65536);
            let mut payload = vec![0; size as usize];
            stream.read_exact(&mut payload).await.unwrap();
            serde_json::from_slice(&payload).unwrap()
        })
        .await
        .unwrap()
    }
    async fn connect(directory: &std::path::Path) -> UnixStream {
        let mut stream = UnixStream::connect(directory.join("discord-ipc-0"))
            .await
            .unwrap();
        send(
            &mut stream,
            0,
            json!({"v":1,"client_id":"123456789012345678"}),
        )
        .await;
        assert_eq!(receive(&mut stream).await["evt"], "READY");
        stream
    }
    let mut first = connect(directory).await;
    // PING is an opaque IPC payload and must be echoed as PONG.
    send(&mut first, 3, json!({"ping":1})).await;
    assert_eq!(first.read_u32_le().await.unwrap(), 4);
    let size = first.read_u32_le().await.unwrap();
    let mut pong = vec![0; size as usize];
    first.read_exact(&mut pong).await.unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&pong).unwrap(),
        json!({"ping":1})
    );
    let activity = json!({"details":"In a match","timestamps":{"start":1700000000_u64},
        "assets":{"large_image":"game_cover","large_text":"Fixture",
            "small_image":"https://example.com/badge.png"},
        "buttons":[{"label":"Website","url":"https://example.com/game"}]});
    send(
        &mut first,
        1,
        json!({"cmd":"SET_ACTIVITY","nonce":"game","args":{"pid":42,"activity":activity}}),
    )
    .await;
    let response = receive(&mut first).await;
    assert_eq!(response["nonce"], "game");
    assert!(response["evt"].is_null());
    assert_eq!(response["data"]["name"], "Fixture Game");
    presence_where(server, |presence| {
        let activities = presence["activities"].as_array().unwrap();
        activities
            .iter()
            .any(|a| a["type"] == 4 && a["state"] == "Testing Yapfast")
            && activities.iter().any(|a| {
                a["name"] == "Fixture Game"
                    && a["application_id"] == "123456789012345678"
                    && a["timestamps"]["start"] == 1700000000000_u64
                    && a["assets"]["large_image"] == "123456789012345679"
                    && a["assets"]["small_image"]
                        == "mp:external/fixture/https/example.com/badge.png"
                    && a["metadata"]["button_urls"][0] == "https://example.com/game"
            })
    })
    .await;
    send(
        &mut first,
        1,
        json!({"cmd":"SET_ACTIVITY","nonce":"invalid",
        "args":{"pid":42,"activity":{"type":4,"state":"Cannot replace custom status"}}}),
    )
    .await;
    let refused = receive(&mut first).await;
    assert_eq!(refused["evt"], "ERROR");
    assert_eq!(refused["nonce"], "invalid");

    let mut second = connect(directory).await;
    send(
        &mut second,
        1,
        json!({"cmd":"SET_ACTIVITY","nonce":"second",
        "args":{"pid":43,"activity":{"details":"Second game"}}}),
    )
    .await;
    assert!(receive(&mut second).await["evt"].is_null());
    presence_where(server, |p| {
        p["activities"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["details"] == "Second game")
    })
    .await;
    // Explicit clear restores the other connected application.
    send(
        &mut second,
        1,
        json!({"cmd":"SET_ACTIVITY","nonce":"clear","args":{"pid":43,"activity":null}}),
    )
    .await;
    assert!(receive(&mut second).await["data"].is_null());
    presence_where(server, |p| {
        p["activities"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["details"] == "In a match")
    })
    .await;
    drop(second);
    // Disconnect without a clear must remove the game, preserving custom status.
    drop(first);
    if detected.is_some() {
        use yapfast_discord::game_detection::custom_games::{self, CustomGame};
        custom_games::set(vec![CustomGame {
            executable: "yapfast_detected_game".into(),
            name: "My custom game".into(),
        }]);
        presence_where(server, |p| {
            p["activities"]
                .as_array()
                .is_some_and(|a| a.iter().any(|a| a["name"] == "My custom game"))
        })
        .await;
        // The account's menu shows the same game.
        snapshot_where(discord, |snapshot| {
            snapshot
                .playing
                .as_ref()
                .is_some_and(|playing| playing.name == "My custom game")
        })
        .await;
        let mut disabled_client = connect(directory).await;
        send(&mut disabled_client, 1, json!({"cmd":"SET_ACTIVITY","nonce":"before-disable","args":{"pid":44,"activity":{"details":"Before disabling"}}})).await;
        receive(&mut disabled_client).await;
        presence_where(server, |p| {
            p["activities"]
                .as_array()
                .is_some_and(|a| a.iter().any(|a| a["details"] == "Before disabling"))
        })
        .await;
        discord.set_show_game(false).await.unwrap();
        presence_where(server, |p| {
            p["activities"]
                .as_array()
                .is_some_and(|a| a.len() == 1 && a[0]["type"] == 4)
        })
        .await;
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
        while directory.join("discord-ipc-0").exists() {
            assert!(
                tokio::time::Instant::now() < deadline,
                "disabled RPC must release its socket"
            );
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        let mut byte = [0];
        assert_eq!(
            timeout(Duration::from_secs(5), disabled_client.read(&mut byte))
                .await
                .unwrap()
                .unwrap(),
            0
        );
        snapshot_where(discord, |snapshot| {
            !snapshot.show_game && snapshot.playing.is_none()
        })
        .await;
        discord.set_show_game(true).await.unwrap();
        presence_where(server, |p| {
            p["activities"]
                .as_array()
                .is_some_and(|a| a.iter().any(|a| a["name"] == "My custom game"))
        })
        .await;
        custom_games::set(Vec::new());
        drop(connect(directory).await);
        presence_where(server, |p| {
            p["activities"]
                .as_array()
                .is_some_and(|a| a.iter().any(|a| a["name"] == "Detected Fixture"))
        })
        .await;
        // With no RPC clients left, stopping the actual detected process
        // eventually removes the game, preserving Alice's custom status.
        drop(detected.take());
    }
    presence_where(server, |p| {
        p["activities"]
            .as_array()
            .is_some_and(|a| a.len() == 1 && a[0]["type"] == 4)
    })
    .await;
}

#[test]
fn waits_as_detected_game() {
    if std::env::var_os("YAPFAST_TEST_DETECTED_GAME").is_some() {
        use std::io::Read;
        let _ = std::io::stdin().read_exact(&mut [0_u8]);
    }
}

/// The fake Discord, started afresh for each test, so no test sees what
/// another left. The first in a process lets the fake pick a free port and
/// points Yapfast at it; the rest take turns on that port, as `cargo test`
/// runs them. `cargo nextest` runs each test in a process of its own, so
/// there each test has a port of its own and they run side by side.
struct Fake {
    process: std::process::Child,
    _games: Option<Games>,
    _turn: tokio::sync::MutexGuard<'static, ()>,
}

impl Drop for Fake {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
    }
}

/// Directories of a test's own for Rich Presence's socket, the game list
/// and Steam, which turn both on against the fake while it runs.
struct Games {
    _directory: tempfile::TempDir,
}

impl Games {
    const VARIABLES: [&str; 3] = [
        "YAPFAST_DISCORD_RPC_DIR",
        "YAPFAST_DISCORD_GAMES_CACHE",
        "YAPFAST_STEAM_DIR",
    ];

    fn new() -> Games {
        // In /tmp, as a socket's path must stay under 108 bytes.
        let directory = tempfile::Builder::new()
            .prefix("yapfast-rpc")
            .tempdir_in("/tmp")
            .unwrap();
        let paths = [
            directory.path().to_owned(),
            directory.path().join("games.json"),
            directory.path().join("steam"),
        ];
        for (variable, path) in Self::VARIABLES.into_iter().zip(paths) {
            // SAFETY: the test holds the turn, and its session, which
            // reads these, hasn't started.
            unsafe { std::env::set_var(variable, path) };
        }
        Games {
            _directory: directory,
        }
    }
}

impl Drop for Games {
    fn drop(&mut self) {
        for variable in Self::VARIABLES {
            // SAFETY: the test still holds the turn.
            unsafe { std::env::remove_var(variable) };
        }
    }
}

/// A test's fake Discord and Alice's session on it, once READY and its
/// voice states have arrived.
struct Fixture {
    _fake: Fake,
    discord: Discord,
    server: &'static str,
    snapshot: Arc<Snapshot>,
    guild: String,
    general: String,
    lounge: String,
    bob: String,
}

impl Fixture {
    /// Starts the fake for the test `name`, logging to
    /// target/fake-discord-`name`.log, and Alice's session on it.
    async fn start(name: &str) -> Fixture {
        Self::open(name, false).await
    }

    /// The same, with Rich Presence and game detection on.
    async fn start_with_games(name: &str) -> Fixture {
        Self::open(name, true).await
    }

    async fn open(name: &str, games: bool) -> Fixture {
        static TURN: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
        static PORT: std::sync::OnceLock<u16> = std::sync::OnceLock::new();
        let turn = TURN.lock().await;
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let log =
            std::fs::File::create(root.join(format!("target/fake-discord-{name}.log"))).unwrap();
        let port = PORT.get().copied().unwrap_or(0).to_string();
        let mut process = std::process::Command::new("bun")
            .args(["testing/fake_discord.ts", "--port", &port])
            .current_dir(&root)
            .stdout(log.try_clone().unwrap())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("could not start bun testing/fake_discord.ts");
        // The fake says where it listens once it does; the rest of what it
        // says goes to the log.
        let said = process.stderr.take().unwrap();
        let (listening, address) = tokio::sync::oneshot::channel();
        std::thread::spawn(move || {
            use std::io::{BufRead, Write};
            let mut log = log;
            let mut listening = Some(listening);
            for line in std::io::BufReader::new(said).lines().map_while(Result::ok) {
                let found = line
                    .strip_prefix("fake Discord on ")
                    .and_then(|rest| rest.split_whitespace().next());
                if let (Some(found), Some(listening)) = (found, listening.take()) {
                    let _ = listening.send(found.to_owned());
                }
                let _ = writeln!(log, "{line}");
            }
        });
        let fake = Fake {
            process,
            _games: games.then(Games::new),
            _turn: turn,
        };
        let address = timeout(WAIT, address)
            .await
            .expect("the fake Discord didn't start")
            .unwrap_or_else(|_| {
                panic!("the fake Discord exited; see target/fake-discord-{name}.log")
            });
        if PORT.get().is_none() {
            PORT.set(address.rsplit(':').next().unwrap().parse().unwrap())
                .unwrap();
            // SAFETY: the first test of the process sets this, holding the
            // turn, before anything reads it.
            unsafe { std::env::set_var("YAPFAST_DISCORD_TEST_SERVER", &address) };
        }
        let server = client::test_server().unwrap();
        assert_eq!(
            server, address,
            "the test server was read before the fake started"
        );
        // A test that refreshed the login, or failed, may have left another.
        token::store(client::TEST_TOKEN).unwrap();
        let token = token::load().unwrap();
        let discord = Discord::start(token).unwrap();
        let snapshot = snapshot_where(&discord, |snapshot| {
            snapshot.status == Status::Online && !snapshot.voice.is_empty()
        })
        .await;
        let shown = &snapshot.servers[0];
        let guild = shown.id.clone();
        let general = shown.text_channels().next().unwrap().id.clone();
        let lounge = shown.voice_channels[0].id.clone();
        let bob = snapshot.voice[&lounge][0].user_id.clone();
        Fixture {
            _fake: fake,
            guild,
            general,
            lounge,
            bob,
            discord,
            server,
            snapshot,
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn reads_history_and_member_lists() {
    let Fixture {
        _fake,
        discord,
        server,
        snapshot,
        guild,
        general,
        lounge,
        bob,
    } = Fixture::start("reads_history_and_member_lists").await;
    // READY, then the voice states from READY_SUPPLEMENTAL, which the
    // fixture waited for.
    assert_eq!(snapshot.servers.len(), 1);
    let server_shown = &snapshot.servers[0];
    assert_eq!(server_shown.name, "Server");
    let general_shown = server_shown.text_channels().next().unwrap();
    assert_eq!(general_shown.name, "general");
    assert!(
        general_shown.marks.unread,
        "the fixture leaves the last messages unread"
    );
    assert_eq!(server_shown.voice_channels[0].name, "Lounge");
    let in_voice: Vec<_> = snapshot.voice[&lounge]
        .iter()
        .map(|member| member.name.as_str())
        .collect();
    assert_eq!(in_voice, ["Bob", "Carol", "Dave"]);
    assert!(snapshot.voice[&lounge][0].streaming, "Bob is live");
    assert_eq!(snapshot.voice[&lounge][0].user_id, bob);
    let last_read = general_shown.marks.last_read;

    // Two pages of history: 50, then the rest, oldest first. The latest
    // starts with a boost, which counts but doesn't show.
    let latest = discord
        .history(&guild, &general, Span::Latest)
        .await
        .unwrap();
    assert!(latest.full);
    let (oldest, latest) = (latest.oldest.unwrap(), latest.messages);
    assert_eq!(latest.len(), 49);
    assert!(oldest < latest[0].order());
    assert_eq!(
        last_read.to_string(),
        latest[latest.len() - 6].id,
        "read up to the fixture's last five messages"
    );
    let older = discord
        .history(&guild, &general, Span::Before(&oldest.to_string()))
        .await
        .unwrap();
    assert!(!older.full);
    let older = older.messages;
    assert_eq!(older.len(), 10);
    assert!(older[0].content.starts_with("Welcome to **Yapfast**"));
    assert_eq!(
        (older[0].reactions[0].count, older[0].reactions[0].me),
        (2, true)
    );
    assert!(older[3].reply.is_some());
    // A jump asks for the messages around one, and scrolling down from
    // there for those after, as Discord's client does.
    let welcome = &older[0].id;
    let around = discord
        .history(&guild, &general, Span::Around(welcome))
        .await
        .unwrap();
    assert_eq!(
        &around.messages[0].id, welcome,
        "nothing is before the first"
    );
    assert!(around.messages.len() >= 20 && around.messages.len() <= 26);
    let newest = around.messages.last().unwrap().id.clone();
    let after = discord
        .history(&guild, &general, Span::After(&newest))
        .await
        .unwrap();
    assert!(!after.full, "the rest of the channel");
    assert!(after.messages[0].order() > around.messages.last().unwrap().order());
    assert_eq!(
        after.messages.last().map(|message| &message.id),
        latest.last().map(|message| &message.id),
        "up to the newest, oldest first"
    );
    let image = &older[1].attachments[0];
    let preview = discord
        .download(image.preview_url.as_deref().unwrap(), 1 << 20)
        .await
        .unwrap();
    assert!(preview.starts_with(b"\x89PNG"));
    assert_eq!(&preview[16..24], &[0, 0, 1, 104, 0, 0, 0, 225], "360x225");

    // The channel's member list arrives once it's on screen, grouped as in
    // Discord: the hoisted Mods role, then online, then offline.
    let mut live = discord.live();
    discord.view(&guild, Some(&general));
    let members = live_where(&mut live, |event| match event {
        Live::Members { channel, members } if channel == general => Some(members),
        _ => None,
    })
    .await;
    let listed: Vec<_> = members
        .iter()
        .map(|member| (member.name.as_str(), member.group.as_str(), member.presence))
        .collect();
    assert_eq!(
        listed,
        [
            ("Bob", "Mods — 1", Presence::DoNotDisturb),
            ("Alice", "Online — 2", Presence::Online),
            ("Carol", "Online — 2", Presence::Idle),
            ("Dave", "Offline — 1", Presence::Offline)
        ]
    );
    assert_eq!(members[0].color, Some(0x3498db), "the Mods role's blue");
    assert_eq!(members[0].activity.as_deref(), Some("Playing Factorio"));
    assert_eq!(
        members[2].activity.as_deref(),
        Some("🌙 back later\nafter dinner, probably")
    );
    assert!(
        members[2].avatar.contains("/guilds/"),
        "Carol's server avatar"
    );

    // Lounge has the same readers as #general, so it shares the list
    // without asking Discord again, and so does going back.
    discord.view(&guild, Some(&lounge));
    let shared = live_where(&mut live, |event| match event {
        Live::Members { channel, members } if channel == lounge => Some(members),
        _ => None,
    })
    .await;
    assert_eq!(shared, members);
    discord.view(&guild, Some(&general));
    live_where(&mut live, |event| match event {
        Live::Members { channel, .. } if channel == general => Some(()),
        _ => None,
    })
    .await;
    assert_eq!(
        self::control(server, "GET", "/_control/list-requests", ""),
        "1"
    );

    // Hidden from Bots, Lounge has a list of its own. Asking for it stops
    // #general's, whose rows show again at once, without asking again.
    post_control(server, "/_control/hide-from-bots", &lounge);
    snapshot_where(&discord, |snapshot| {
        snapshot.servers.iter().any(|server| {
            server
                .voice_channels
                .iter()
                .any(|channel| channel.id == lounge && !channel.overwrites.is_empty())
        })
    })
    .await;
    discord.view(&guild, Some(&lounge));
    let own = live_where(&mut live, |event| match event {
        Live::Members { channel, members } if channel == lounge => Some(members),
        _ => None,
    })
    .await;
    assert_eq!(own, members);
    discord.view(&guild, Some(&general));
    let again = live_where(&mut live, |event| match event {
        Live::Members { channel, members } if channel == general => Some(members),
        _ => None,
    })
    .await;
    assert_eq!(again, members, "#general's list as it was");
    assert_eq!(
        self::control(server, "GET", "/_control/list-requests", ""),
        "2"
    );

    // Invisible, Alice's row shows her offline, as Discord's apps show
    // their own account, though Discord still lists her online.
    let alice = |members: Vec<ListedMember>| {
        members
            .into_iter()
            .find(|member| member.name == "Alice")
            .map(|member| member.presence)
    };
    for (status, presence) in [
        ("invisible", Presence::Offline),
        ("online", Presence::Online),
    ] {
        discord.set_status(status).await.unwrap();
        let shown = live_where(&mut live, |event| match event {
            Live::Members { channel, members } if channel == general => Some(members),
            _ => None,
        })
        .await;
        assert_eq!(alice(shown), Some(presence), "{status}");
    }
    post_control(server, "/_control/hide-from-bots", "");

    let account = snapshot.account.as_ref().expect("the account");
    assert_eq!(account.name, "Alice");
    assert!(
        account
            .avatar
            .contains("/avatars/1400000000000000001/alice1.png")
    );

    // Reading the channel clears its unread mark.
    discord.mark_read(&guild, &general, true);
    let newest = latest.last().unwrap().id.clone();
    snapshot_where(&discord, |snapshot| {
        let general = snapshot.servers[0].text_channels().next().unwrap().marks;
        !general.unread && general.last_read.to_string() == newest
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn sends_edits_and_deletes_messages() {
    let Fixture {
        _fake,
        discord,
        server,
        snapshot,
        guild,
        general,
        ..
    } = Fixture::start("sends_edits_and_deletes_messages").await;
    let mut live = discord.live();
    // Bob chats live until stopped.
    let control = reqwest::Client::new();
    control
        .post(format!("{server}/_control/chat-start"))
        .send()
        .await
        .unwrap();
    let message = live_where(&mut live, |event| match event {
        Live::Created(message) => Some(message),
        _ => None,
    })
    .await;
    control
        .post(format!("{server}/_control/chat-stop"))
        .send()
        .await
        .unwrap();
    assert_eq!(message.author, "Bob");
    assert!(message.content.starts_with("Live Message 0"));
    assert_eq!(
        discord.color(&snapshot.servers[0], &message.author_id),
        Some(0x3498db),
        "Bob's name takes the Mods role's blue"
    );
    snapshot_where(&discord, |snapshot| {
        snapshot.servers[0]
            .text_channels()
            .next()
            .unwrap()
            .marks
            .unread
    })
    .await;

    // Sending, reacting, editing and deleting.
    let nonce = yapfast_discord::message::nonce();
    let sent = discord
        .send(
            &guild,
            &general,
            "hello from Yapfast",
            sending(&nonce, Some(&message.id)),
            vec![],
        )
        .await
        .unwrap();
    assert!(sent.reply.is_some());
    let echoed = live_where(&mut live, |event| match event {
        Live::Created(message) if message.id == sent.id => Some(message),
        _ => None,
    })
    .await;
    assert_eq!(echoed.nonce.as_deref(), Some(nonce.as_str()));
    discord.react(&general, &sent.id, "🎉", true).await.unwrap();
    live_where(&mut live, |event| match event {
        Live::Reaction {
            me: true,
            added: true,
            ..
        } => Some(()),
        _ => None,
    })
    .await;
    let edited = discord.edit(&general, &sent.id, "edited").await.unwrap();
    assert!(edited.edited);
    discord.delete(&general, &sent.id).await.unwrap();
    live_where(&mut live, |event| match event {
        Live::Deleted { id, .. } if id == sent.id => Some(()),
        _ => None,
    })
    .await;
    // Deleting one Discord no longer has, as from a timeline that still
    // showed it, does what was asked.
    discord.delete(&general, &sent.id).await.unwrap();

    // Someone else's message goes only with Manage Messages, which Mods
    // gives.
    post_control(server, "/_control/mention", "");
    let bobs = live_where(&mut live, |event| match event {
        Live::Created(message) if message.content.contains("co-op tonight") => Some(message),
        _ => None,
    })
    .await;
    assert!(discord.delete(&general, &bobs.id).await.is_err());
    let alice = discord.snapshots().borrow().user_id.clone().unwrap();
    let mods = "1400000000000000110";
    for add in [true, false] {
        discord
            .member_role(&guild, &alice, mods, add)
            .await
            .unwrap();
        let snapshot = snapshot_where(&discord, |snapshot| {
            snapshot.servers[0]
                .marks
                .roles
                .iter()
                .any(|role| role == mods)
                == add
        })
        .await;
        let joined = &snapshot.servers[0];
        assert_eq!(
            joined.deletes(&joined.text_channels[0], &alice, &joined.marks.roles),
            if add { Deletes::Any } else { Deletes::Own }
        );
        if add {
            discord.delete(&general, &bobs.id).await.unwrap();
            live_where(&mut live, |event| match event {
                Live::Deleted { id, .. } if id == bobs.id => Some(()),
                _ => None,
            })
            .await;
        }
    }

    // Timed out, Alice only reads "Server" until the timeout ends by
    // itself.
    post_control(server, "/_control/timeout-alice", "2");
    let timed_out = snapshot_where(&discord, |snapshot| {
        snapshot.servers[0].my_timeout.is_some()
    })
    .await;
    let access = |snapshot: &yapfast_discord::service::Snapshot| {
        snapshot.servers[0]
            .text_channels()
            .next()
            .unwrap()
            .marks
            .access
    };
    assert_eq!(access(&timed_out), Access::Locked);
    let nonce = yapfast_discord::message::nonce();
    assert!(
        discord
            .send(&guild, &general, "let me in", sending(&nonce, None), vec![])
            .await
            .is_err()
    );
    let lifted = snapshot_where(&discord, |snapshot| {
        snapshot.servers[0].my_timeout.is_none()
    })
    .await;
    assert_eq!(access(&lifted), Access::Join);
    let nonce = yapfast_discord::message::nonce();
    discord
        .send(&guild, &general, "back", sending(&nonce, None), vec![])
        .await
        .unwrap();
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn votes_in_and_posts_polls() {
    let Fixture {
        _fake,
        discord,
        server,
        guild,
        general,
        ..
    } = Fixture::start("votes_in_and_posts_polls").await;
    let control = reqwest::Client::new();
    let mut live = discord.live();
    // Polls: a vote comes back live, who voted lists by answer, and Alice's
    // own poll ends early with a notice of its winner.
    let posted = control
        .post(format!("{server}/_control/poll?alice"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    let poll = live_where(&mut live, |event| match event {
        Live::Created(message) if message.poll.is_some() => Some(message),
        _ => None,
    })
    .await;
    assert_eq!(poll.id, posted.trim());
    assert!(!poll.editable && !poll.forwardable);
    discord.vote(&general, &poll.id, &[2]).await.unwrap();
    live_where(&mut live, |event| match event {
        Live::Vote {
            answer: 2,
            me: true,
            added: true,
            ..
        } => Some(()),
        _ => None,
    })
    .await;
    let voters = discord
        .voters(&guild, &general, &poll.id, 2, None, 100)
        .await
        .unwrap();
    assert_eq!(voters.len(), 3);
    assert_eq!(voters.iter().filter(|voter| voter.me).count(), 1);
    discord.end_poll(&general, &poll.id).await.unwrap();
    let closed = live_where(&mut live, |event| match event {
        Live::Updated(message) if message.id == poll.id => Some(message),
        _ => None,
    })
    .await;
    assert!(closed.poll.is_some_and(|poll| poll.finalized));
    let notice = live_where(&mut live, |event| match event {
        Live::Created(message) if message.system.is_some() => Some(message),
        _ => None,
    })
    .await;
    assert!(
        notice
            .system
            .as_deref()
            .is_some_and(|said| said.ends_with("Winning answer: 🍜 Ramen, 3 votes (75%).")),
        "{notice:?}"
    );

    // Posting a poll, as Discord's "Create a Poll" dialog does.
    let new_poll = yapfast_discord::api::NewPoll {
        question: "Movie tonight?".into(),
        answers: vec![
            yapfast_discord::api::NewAnswer {
                text: "Yes".into(),
                emoji: Some("🍿".into()),
            },
            yapfast_discord::api::NewAnswer {
                text: "No".into(),
                emoji: None,
            },
        ],
        hours: 4,
        multiselect: false,
    };
    discord
        .send_poll(&guild, &general, &new_poll)
        .await
        .unwrap();
    let posted = live_where(&mut live, |event| match event {
        Live::Created(message) => message
            .poll
            .filter(|poll| poll.question == "Movie tonight?"),
        _ => None,
    })
    .await;
    let answers: Vec<_> = posted
        .answers
        .iter()
        .map(|answer| (answer.id, answer.text.as_str(), answer.emoji.clone()))
        .collect();
    assert_eq!(
        answers,
        [(1, "Yes", Some(("🍿".to_owned(), None))), (2, "No", None)]
    );
    let expiry = posted.expiry.unwrap();
    let now = yapfast_discord::message::snowflake_ms(&yapfast_discord::message::nonce());
    assert!(
        expiry.abs_diff(now + 4 * 3_600_000) < 60_000,
        "it takes votes for four hours"
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn forwards_messages() {
    let Fixture {
        _fake,
        discord,
        server,
        guild,
        general,
        ..
    } = Fixture::start("forwards_messages").await;
    let control = reqwest::Client::new();
    let mut live = discord.live();
    // A message of Bob's to forward.
    post_control(server, "/_control/chat-start", "");
    let message = live_where(&mut live, |event| match event {
        Live::Created(message) => Some(message),
        _ => None,
    })
    .await;
    post_control(server, "/_control/chat-stop", "");
    // Forwarding, as Discord's client does: a message of its own that
    // carries the original, then the message added as another.
    discord
        .forward((&guild, &general, &message.id), (&guild, &general), false)
        .await
        .unwrap();
    let forward = live_where(&mut live, |event| match event {
        Live::Created(forward) if forward.forwarded => Some(forward),
        _ => None,
    })
    .await;
    assert_eq!(forward.content, message.content);
    assert!(!forward.editable && forward.forwardable);
    discord
        .send_forward_note((&guild, &general), "look at this", false)
        .await
        .unwrap();
    live_where(&mut live, |event| match event {
        Live::Created(note) if note.content == "look at this" => Some(()),
        _ => None,
    })
    .await;
    let forwards: serde_json::Value = serde_json::from_str(
        &control
            .get(format!("{server}/_control/forwards"))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(forwards[0]["target"], general.as_str());
    // {"location":"forwarding"}
    assert_eq!(
        forwards[0]["context"],
        "eyJsb2NhdGlvbiI6ImZvcndhcmRpbmcifQ=="
    );
    // A message Discord doesn't have isn't forwarded.
    assert!(
        discord
            .forward((&guild, &general, "1"), (&guild, &general), false)
            .await
            .is_err()
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn lists_dms_and_notifies() {
    let Fixture {
        _fake,
        discord,
        server,
        snapshot,
        guild,
        general,
        ..
    } = Fixture::start("lists_dms_and_notifies").await;
    let control = reqwest::Client::new();
    let mut live = discord.live();
    // DMs and message requests, the latest first; each unread message in a
    // DM counts as a mention, and requests count none.
    let listed: Vec<_> = snapshot
        .conversations
        .iter()
        .map(|dm| {
            (
                dm.name.as_str(),
                dm.request,
                dm.spam,
                dm.unread,
                dm.mentions,
            )
        })
        .collect();
    assert_eq!(
        listed,
        [
            ("Erin", true, false, true, 0),
            ("Free Nitro", true, true, false, 0),
            ("Bob", false, false, true, 2),
            ("Weekend plans", false, false, false, 0)
        ]
    );
    let id = |name: &str| {
        snapshot
            .conversations
            .iter()
            .find(|dm| dm.name == name)
            .unwrap()
            .id
            .clone()
    };
    let (dm_bob, weekend) = (id("Bob"), id("Weekend plans"));
    assert!(
        snapshot.conversations[2]
            .avatar
            .as_deref()
            .is_some_and(|avatar| avatar.contains("/avatars/1400000000000000002/bob1.png"))
    );
    let history = discord
        .history(PRIVATE, &dm_bob, Span::Latest)
        .await
        .unwrap()
        .messages;
    let said: Vec<_> = history
        .iter()
        .map(|message| message.content.as_str())
        .collect();
    assert_eq!(
        said,
        [
            "Did you see the trip notes?",
            "Yes, bringing snacks",
            "Great",
            "See you at nine"
        ]
    );
    discord.mark_read(PRIVATE, &dm_bob, true);
    snapshot_where(&discord, |snapshot| {
        let bob = snapshot.conversations.iter().find(|dm| dm.id == dm_bob);
        bob.is_some_and(|bob| !bob.unread && bob.mentions == 0)
    })
    .await;
    let nonce = yapfast_discord::message::nonce();
    let sent = discord
        .send(
            PRIVATE,
            &dm_bob,
            "On my way",
            sending(&nonce, Some(&history[3].id)),
            // A message carries several files.
            vec![
                ("map.txt".into(), b"left at the lake".to_vec()),
                ("list.txt".into(), b"snacks".to_vec()),
            ],
        )
        .await
        .unwrap();
    assert!(sent.reply.is_some());
    let names: Vec<_> = sent
        .attachments
        .iter()
        .map(|file| file.name.as_str())
        .collect();
    assert_eq!(names, ["map.txt", "list.txt"]);
    live_where(&mut live, |event| match event {
        Live::Created(message) if message.id == sent.id => Some(()),
        _ => None,
    })
    .await;

    // A group DM lists everyone in it, with friends' presence.
    discord.view(PRIVATE, Some(&weekend));
    let members = live_where(&mut live, |event| match event {
        Live::Members { channel, members } if channel == weekend => Some(members),
        _ => None,
    })
    .await;
    let listed: Vec<_> = members
        .iter()
        .map(|member| (member.name.as_str(), member.group.as_str(), member.presence))
        .collect();
    assert_eq!(
        listed,
        [
            ("Alice", "Members — 3", Presence::Online),
            ("Bob", "Members — 3", Presence::DoNotDisturb),
            ("Carol", "Members — 3", Presence::Idle)
        ]
    );

    // Dave writes for the first time: his new DM comes first, unread.
    control
        .post(format!("{server}/_control/dm"))
        .send()
        .await
        .unwrap();
    snapshot_where(&discord, |snapshot| {
        snapshot.conversations.len() == 5
            && snapshot.conversations[0].name == "Dave"
            && snapshot.conversations[0].mentions == 1
    })
    .await;
    // His message notifies, as DMs do.
    let notified = live_where(&mut live, |event| match event {
        Live::Notify { guild, message } => Some((guild, message.author)),
        _ => None,
    })
    .await;
    assert_eq!(notified, (None, "Dave".to_owned()));
    // The server notifies for mentions only: Bob's photos don't, his mention does.
    for control_path in ["gallery", "mention"] {
        control
            .post(format!("{server}/_control/{control_path}"))
            .send()
            .await
            .unwrap();
    }
    let notified = live_where(&mut live, |event| match event {
        Live::Notify { guild, message } => Some((guild, message.content)),
        _ => None,
    })
    .await;
    assert_eq!(notified.0.as_deref(), Some(guild.as_str()));
    assert!(
        notified.1.ends_with("are you in for co-op tonight?"),
        "{}",
        notified.1
    );

    // Reading #general and Dave's DM together, as marking a server read
    // does, acknowledges both in one request.
    let dm_dave = discord.snapshots().borrow().conversations[0].id.clone();
    let before = serde_json::from_str::<Vec<Vec<String>>>(&self::control(
        server,
        "GET",
        "/_control/acks",
        "",
    ))
    .unwrap()
    .len();
    discord.mark_read(&guild, &general, true);
    discord.mark_read(PRIVATE, &dm_dave, true);
    // Reads from just before, such as Bob's DM, may go in the same request.
    let both = |request: &Vec<String>| request.contains(&general) && request.contains(&dm_dave);
    timeout(WAIT, async {
        loop {
            let acks: Vec<Vec<String>> =
                serde_json::from_str(&self::control(server, "GET", "/_control/acks", "")).unwrap();
            if acks[before..].iter().any(both) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("both reads are acknowledged in one request");
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn types_and_mentions() {
    let Fixture {
        _fake,
        discord,
        server,
        guild,
        general,
        ..
    } = Fixture::start("types_and_mentions").await;
    let control = reqwest::Client::new();
    let mut live = discord.live();
    // Bob starts typing, and Yapfast says Alice is typing.
    control
        .post(format!("{server}/_control/typing"))
        .send()
        .await
        .unwrap();
    let typing = live_where(&mut live, |event| match event {
        Live::Typing {
            guild,
            channel,
            name,
            ..
        } => Some((guild, channel, name)),
        _ => None,
    })
    .await;
    assert_eq!(
        typing,
        (Some(guild.clone()), general.clone(), "Bob".to_owned())
    );
    discord.typing(&general).await.unwrap();
    let typed = control
        .get(format!("{server}/_control/typed"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    let typed: Vec<String> = serde_json::from_str(&typed).unwrap();
    assert_eq!(typed, vec![general.clone()]);

    // Mentions: a search finds members by name, and only Mods may be mentioned,
    // since Alice can't mention everyone.
    let found = discord.search_members(&guild, "car").await.unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(discord.name(&guild, &found[0]).as_deref(), Some("Carol"));
    let listed = discord.snapshots().borrow().servers[0].clone();
    assert!(
        !listed
            .text_channels()
            .next()
            .unwrap()
            .marks
            .mention_everyone
    );
    let mentionable: Vec<_> = listed
        .mentionable(false)
        .into_iter()
        .map(|(_, name, _)| name)
        .collect();
    assert_eq!(mentionable, ["Mods"]);
    assert_eq!(
        listed.mentionable(true).len(),
        5,
        "Bots, Crew, Staff and Leads too, for those who may"
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn opens_dms_and_searches_pins() {
    let Fixture {
        _fake,
        discord,
        snapshot,
        guild,
        general,
        bob,
        ..
    } = Fixture::start("opens_dms_and_searches_pins").await;
    let mut live = discord.live();
    let dm = |name: &str| {
        snapshot
            .conversations
            .iter()
            .find(|dm| dm.name == name)
            .unwrap()
            .id
            .clone()
    };
    let dm_bob = dm("Bob");
    // New DMs: friends come first, others by name, and opening a DM with
    // Carol starts one, which the DM list gains; opening it again finds it.
    let friends: Vec<_> = discord
        .find_people("")
        .into_iter()
        .map(|person| (person.name, person.friend))
        .collect();
    assert_eq!(
        friends,
        [("Bob".to_owned(), true), ("Carol".to_owned(), true)]
    );
    let found = discord.find_people("da");
    assert_eq!(found[0].name, "Dave");
    assert!(!found[0].friend);
    let carol_id = discord.find_people("carol")[0].id.clone();
    let carol_dm = discord.open_dm(&carol_id).await.unwrap();
    snapshot_where(&discord, |snapshot| {
        snapshot
            .conversations
            .iter()
            .any(|conversation| conversation.id == carol_dm)
    })
    .await;
    assert_eq!(discord.open_dm(&carol_id).await.unwrap(), carol_dm);

    // A DM's pins, and a search of it, which waits while Discord indexes
    // the DM, newest first.
    let pinned: Vec<_> = discord.pins(PRIVATE, &dm_bob).await.unwrap();
    assert_eq!(pinned.len(), 1);
    assert_eq!(pinned[0].content, "Did you see the trip notes?");
    let found = discord
        .search(PRIVATE, Some(&dm_bob), "SEE", 0)
        .await
        .unwrap();
    let said: Vec<_> = found
        .messages
        .iter()
        .map(|message| message.content.as_str())
        .collect();
    assert_eq!(said, ["See you at nine", "Did you see the trip notes?"]);
    assert_eq!(found.total, 2);

    // A server channel's pins, and a search of its server, here narrowed to
    // the channel with in:, which Discord indexes the same way.
    let pinned = discord.pins(&guild, &general).await.unwrap();
    assert_eq!(pinned.len(), 1);
    assert!(pinned[0].content.starts_with("Welcome to **Yapfast**"));
    // Bob's picture after it, pinned as Alice's Crew role allows: Discord
    // says it's pinned, and it leads the pins until it's unpinned.
    let after = discord
        .history(&guild, &general, Span::After(&pinned[0].id))
        .await
        .unwrap()
        .messages;
    let sunset = after
        .iter()
        .find(|message| {
            message
                .attachments
                .iter()
                .any(|attachment| attachment.image)
        })
        .expect("Bob's picture follows the welcome");
    assert!(pinned[0].pinned && !sunset.pinned);
    discord.pin(&general, &sunset.id, true).await.unwrap();
    let updated = live_where(&mut live, |event| match event {
        Live::Updated(message) if message.id == sunset.id => Some(message),
        _ => None,
    })
    .await;
    assert!(updated.pinned);
    let pins = discord.pins(&guild, &general).await.unwrap();
    assert_eq!(pins.len(), 2);
    assert_eq!(pins[0].id, sunset.id);
    assert!(pins[0].attachments[0].preview_url.is_some());
    discord.pin(&general, &sunset.id, false).await.unwrap();
    assert_eq!(discord.pins(&guild, &general).await.unwrap().len(), 1);
    let found = discord
        .search(&guild, None, "welcome in:general", 0)
        .await
        .unwrap();
    let said: Vec<_> = found
        .messages
        .iter()
        .map(|message| message.content.as_str())
        .collect();
    assert_eq!(said.len(), 2);
    assert_eq!(said[0], "The welcome has the plan");
    assert!(said[1].starts_with("Welcome to **Yapfast**"));
    assert_eq!(discord.name(&guild, &bob).as_deref(), Some("Bob"));
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn makes_group_dms_and_answers_requests() {
    let Fixture {
        _fake,
        discord,
        server,
        snapshot,
        ..
    } = Fixture::start("makes_group_dms_and_answers_requests").await;
    let dm = |name: &str| {
        snapshot
            .conversations
            .iter()
            .find(|dm| dm.name == name)
            .unwrap()
            .id
            .clone()
    };
    let (erin, frank) = (dm("Erin"), dm("Free Nitro"));
    let carol_id = discord.find_people("carol")[0].id.clone();
    // Friends added to a DM make a group of them; more join it later.
    let dave = discord.find_people("da")[0].id.clone();
    let group = discord
        .create_group(&[carol_id.clone(), dave.clone()])
        .await
        .unwrap();
    let people_in = |snapshot: &Snapshot| {
        snapshot
            .conversations
            .iter()
            .find(|conversation| conversation.id == group && conversation.group)
            .map(|conversation| conversation.people)
    };
    snapshot_where(&discord, |snapshot| people_in(snapshot) == Some(3)).await;
    let bob_id = discord.find_people("bob")[0].id.clone();
    discord.add_to_group(&group, &[bob_id]).await.unwrap();
    snapshot_where(&discord, |snapshot| people_in(snapshot) == Some(4)).await;
    // Leaving it without telling the others, as its dialog's box does.
    discord.leave_group(&group, true).await.unwrap();
    snapshot_where(&discord, |snapshot| people_in(snapshot).is_none()).await;
    assert_eq!(
        self::control(server, "GET", "/_control/group-leaves", ""),
        format!(r#"[{{"channel":"{group}","silent":"true"}}]"#)
    );

    // Each request shows its latest message, as Discord's requests list
    // loads it, and stops once it's answered.
    snapshot_where(&discord, |snapshot| {
        snapshot.conversations.iter().any(|dm| {
            dm.id == erin
                && dm.preview.as_ref().is_some_and(|preview| {
                    preview.text.starts_with("Hi Alice, Bob gave me your name")
                })
        })
    })
    .await;
    // A request can be read before answering it. Accepting Erin's makes it
    // a DM; ignoring the spam removes it.
    let asked = discord
        .history(PRIVATE, &erin, Span::Latest)
        .await
        .unwrap()
        .messages;
    assert!(
        asked[0]
            .content
            .starts_with("Hi Alice, Bob gave me your name")
    );
    discord.answer_request(&erin, true).await.unwrap();
    discord.answer_request(&frank, false).await.unwrap();
    snapshot_where(&discord, |snapshot| {
        snapshot
            .conversations
            .iter()
            .any(|dm| dm.id == erin && !dm.request && dm.mentions == 1 && dm.preview.is_none())
            && !snapshot.conversations.iter().any(|dm| dm.id == frank)
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn shows_profiles() {
    let Fixture {
        _fake,
        discord,
        guild,
        bob,
        ..
    } = Fixture::start("shows_profiles").await;
    // Bob's profile: in the server, his server bio wins over his own; from
    // a DM, his own shows.
    let in_server = discord.profile(&guild, &bob, |_| {}).await.unwrap();
    assert_eq!(in_server.name, "Bob");
    assert_eq!(in_server.pronouns.as_deref(), Some("he/him"));
    assert_eq!(in_server.bio.as_deref(), Some("Keeps #general tidy."));
    assert!(
        in_server
            .banner
            .is_some_and(|banner| banner.contains("/banners/"))
    );
    assert_eq!(in_server.roles.len(), 1, "Bob is a Mod");
    assert!(in_server.joined.is_some());
    assert_eq!(in_server.mutual_servers.len(), 1);
    assert_eq!(in_server.mutual_servers[0].id, guild);
    assert_eq!(in_server.mutual_friends.len(), 1);
    assert_eq!(in_server.mutual_friends[0].name, "Carol");
    let own = discord.profile(PRIVATE, &bob, |_| {}).await.unwrap();
    assert!(own.bio.is_some_and(|bio| bio.starts_with("Plays bass")));
    assert!(own.roles.is_empty() && own.joined.is_none());
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn shows_embeds_and_media() {
    let Fixture {
        _fake,
        discord,
        server,
        guild,
        general,
        ..
    } = Fixture::start("shows_embeds_and_media").await;
    let control = reqwest::Client::new();
    let mut live = discord.live();
    // Link previews: the article's arrives in an update, as Discord crawls
    // the page after the message is sent. A GIF link shows as a picture,
    // without its address.
    control
        .post(format!("{server}/_control/embeds"))
        .send()
        .await
        .unwrap();
    let article = live_where(&mut live, |event| match event {
        Live::Updated(message) if !message.embeds.is_empty() => Some(message),
        _ => None,
    })
    .await;
    assert_eq!(article.embeds[0].provider.as_deref(), Some("Example News"));
    assert_eq!(article.attachments[0].shown, Shown::Embed(0));
    let latest = discord
        .history(&guild, &general, Span::Latest)
        .await
        .unwrap()
        .messages;
    let [.., video, gif, card] = latest.as_slice() else {
        panic!("the embeds were posted");
    };
    assert!(video.embeds[0].video);
    assert_eq!(video.attachments[0].shown, Shown::Poster(0));
    assert!(gif.content.is_empty() && gif.embeds.is_empty());
    assert_eq!(gif.attachments[0].shown, Shown::Message);
    assert_eq!(card.author, "Status Bot");
    assert_eq!(card.embeds[0].fields.len(), 4);
    // Pictures come from Discord's proxy; the linked site is never asked.
    // The GIF is an MP4, with its first frame as a picture.
    let preview = gif.attachments[0].preview_url.as_deref().unwrap();
    assert_eq!(
        &discord.download(preview, 1 << 20).await.unwrap()[4..8],
        b"ftyp"
    );
    let still = gif.attachments[0].still_url.as_deref().unwrap();
    assert!(!discord.download(still, 1 << 20).await.unwrap().is_empty());
    assert!(
        discord
            .download("https://example.com/bike.png", 1 << 20)
            .await
            .is_err()
    );

    // An attached video plays from its file, which downloads as it plays,
    // under a poster the media proxy makes.
    control
        .post(format!("{server}/_control/clip"))
        .send()
        .await
        .unwrap();
    let posted = live_where(&mut live, |event| match event {
        Live::Created(message) if message.content == "Watch this" => Some(message),
        _ => None,
    })
    .await;
    let clip = &posted.attachments[0];
    assert!(!clip.image && clip.dimensions == Some((480, 270)));
    let poster = clip.preview_url.as_deref().unwrap();
    assert!(!discord.download(poster, 1 << 20).await.unwrap().is_empty());
    let path = std::env::temp_dir().join(format!("yapfast-clip-{}.mp4", std::process::id()));
    let mut file = std::fs::File::create(&path).unwrap();
    let size = discord
        .download_into(clip.video_url.as_deref().unwrap(), &mut file, 1 << 20)
        .await
        .unwrap();
    assert_eq!(Some(size), clip.size);
    let downloaded = std::fs::read(&path).unwrap();
    let _ = std::fs::remove_file(&path);
    assert_eq!(&downloaded[4..8], b"ftyp");
    // A forward of it shows the clip it carries.
    let forward = live_where(&mut live, |event| match event {
        Live::Created(message) if message.forwarded => Some(message),
        _ => None,
    })
    .await;
    assert_eq!(forward.content, "Watch this");
    assert_eq!(forward.attachments, posted.attachments);
    // So does a linked post's, its card's thumbnail the poster.
    let linked = live_where(&mut live, |event| match event {
        Live::Created(message) if message.content.starts_with("https://vxtwitter.com/") => {
            Some(message)
        }
        _ => None,
    })
    .await;
    assert!(linked.embeds[0].video);
    let poster = &linked.attachments[0];
    assert_eq!(poster.shown, Shown::Poster(0));
    let mut file = std::fs::File::create(&path).unwrap();
    discord
        .download_into(poster.video_url.as_deref().unwrap(), &mut file, 1 << 20)
        .await
        .unwrap();
    let _ = std::fs::remove_file(&path);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn calls_and_goes_live() {
    let Fixture {
        _fake,
        discord,
        server,
        guild,
        lounge,
        bob,
        ..
    } = Fixture::start("calls_and_goes_live").await;
    let mut live = discord.live();
    // Go Live: watching Bob needs the voice channel; then Alice joins the
    // call, watches him and shares her own screen.
    let bob_stream = stream::key(&guild, &lounge, &bob);
    let refused = discord.watch(&bob_stream).await.unwrap_err();
    assert_eq!(refused.to_string(), "Join the call to watch its streams.");
    let assigned = discord.join(&guild, &lounge).await.unwrap();
    let hearer = std::sync::Arc::new(Hearer::default());
    let mut call = timeout(
        WAIT,
        Call::connect(&assigned, &guild, &lounge, hearer.clone()),
    )
    .await
    .expect("timed out connecting to voice")
    .unwrap();
    let in_lounge = |snapshot: &Snapshot| {
        snapshot.voice.get(&lounge).and_then(|members| {
            members
                .iter()
                .find(|member| member.name == "Alice")
                .map(|member| member.streaming)
        })
    };
    snapshot_where(&discord, |snapshot| in_lounge(snapshot).is_some()).await;

    // Discord moves the call to another voice server, which Yapfast hears of
    // and connects to, while the call goes on.
    post_control(server, "/_control/voice-move", "");
    let moved = live_where(&mut live, |event| match event {
        Live::VoiceMoved {
            guild: moved,
            server,
        } if moved == guild => Some(server),
        _ => None,
    })
    .await;
    assert_eq!(moved.token, "fake-voice-token-moved");
    assert_eq!(moved.session_id, assigned.session_id);
    timeout(WAIT, call.move_to(&moved, &lounge))
        .await
        .expect("timed out moving the call")
        .unwrap();
    let identified: Vec<String> = serde_json::from_str(&crate::control(
        server,
        "GET",
        "/_control/voice-identifies",
        "",
    ))
    .unwrap();
    assert_eq!(
        identified.last().map(String::as_str),
        Some("fake-voice-token-moved")
    );

    // The call's audio, on the server it moved to: Alice's microphone goes
    // out as Opus, marked speaking, then five silent frames when it stops.
    let (microphone, input) = yapfast_discord::voice::live_microphone();
    let track = call.play(input);
    say(&microphone, 15).await;
    track.pause().unwrap();
    until("Alice's microphone stopped", || {
        call_audio(server, "fake-voice-token-moved")["speaking"] == serde_json::json!([1, 0])
    })
    .await;
    let sent = call_audio(server, "fake-voice-token-moved");
    assert!(sent["packets"].as_u64().unwrap() >= 15, "{sent}");
    assert_eq!(sent["undecryptable"], 0, "{sent}");
    assert_eq!(sent["silent"], 5, "{sent}");
    // Bob speaks: his tone comes through, decoded.
    let bob_user: u64 = bob.parse().unwrap();
    post_control(server, "/_control/bob-speak", "");
    until("Bob's tone was heard", || {
        hearer
            .0
            .lock()
            .unwrap()
            .get(&bob_user)
            .copied()
            .unwrap_or(0)
            > 3_000
    })
    .await;
    // The voice server restarts: the call resumes and carries on.
    post_control(server, "/_control/voice-drop", "");
    until("the call resumed", || {
        call_audio(server, "fake-voice-token-moved")["resumed"] == 1
    })
    .await;
    let before = call_audio(server, "fake-voice-token-moved")["packets"]
        .as_u64()
        .unwrap();
    track.play().unwrap();
    say(&microphone, 10).await;
    track.pause().unwrap();
    until("audio went out after resuming", || {
        call_audio(server, "fake-voice-token-moved")["packets"]
            .as_u64()
            .unwrap()
            >= before + 10
    })
    .await;
    // The call measures its connection, as Discord's voice panel shows it:
    // the heartbeats' round trips, and the share of this account's audio
    // Discord's receiver reports say was lost.
    post_control(server, "/_control/voice-link?loss=10", "");
    track.play().unwrap();
    say(&microphone, 120).await;
    track.pause().unwrap();
    post_control(server, "/_control/voice-link", "");
    let stats = call.stats();
    until("a ping and the loss were measured", || {
        let read = stats.read();
        !read.pings.is_empty() && read.loss.is_some_and(|loss| (9.0..=11.0).contains(&loss))
    })
    .await;
    assert!(!stats.read().end_to_end, "the fake has no DAVE");
    cameras_in_call(
        server,
        &discord,
        &mut call,
        (&guild, &lounge, "fake-voice-token-moved"),
        bob_user,
    )
    .await;

    let watched = discord.watch(&bob_stream).await.unwrap();
    assert_eq!(watched.stream_key, bob_stream);
    assert_eq!(watched.rtc_server_id, "fake-rtc");
    assert!(!watched.session_id.is_empty() && !watched.endpoint.is_empty());

    // Bob's video arrives whole: the recording's frames, in order, from the
    // first keyframe, in VP8, in H264, and in H265 or AV1 to a viewer that
    // decodes it, while one that doesn't gets H264, though the fake loses
    // one packet on the way and delivers another late each time.
    let older = vec![Codec::H264, Codec::Vp8];
    let all = vec![Codec::Av1, Codec::H265, Codec::H264, Codec::Vp8];
    for (codec, name, decodes) in [
        (Codec::Vp8, "vp8", older.clone()),
        (Codec::H264, "h264", older.clone()),
        (Codec::H265, "h265", all.clone()),
        (Codec::H264, "h265", older.clone()),
        (Codec::Av1, "av1", all),
        (Codec::H264, "av1", older),
    ] {
        stream_codec(server, name);
        let watched = watched.clone();
        let frames = tokio::task::spawn_blocking(move || watch_frames(&watched, decodes, 12))
            .await
            .unwrap();
        let expected = recording(codec);
        assert!(frames[0].2, "{codec:?} starts with a keyframe");
        for (index, (received, data, _)) in frames.iter().enumerate() {
            assert_eq!(*received, codec);
            assert!(data == &expected[index], "{codec:?} frame {index} differs");
        }
    }
    let body = discord
        .download(&format!("{server}/_control/video"), 1 << 16)
        .await
        .unwrap();
    let repairs = serde_json::from_slice::<serde_json::Value>(&body).unwrap()["repairs"].clone();
    assert_eq!(
        (repairs["dropped"].as_u64(), repairs["reordered"].as_u64()),
        (Some(6), Some(6))
    );
    assert!(
        repairs["nacks"].as_u64() >= Some(2) && repairs["resent"].as_u64() >= Some(2),
        "the lost packets were asked for and resent: {repairs}"
    );
    assert!(repairs["receiverReports"].as_u64() >= Some(1), "{repairs}");
    stream_codec(server, "vp8");
    discord.end_stream(&bob_stream);
    let reason = live_where(&mut live, |event| match event {
        Live::StreamEnded { stream_key, reason } if stream_key == bob_stream => Some(reason),
        _ => None,
    })
    .await;
    assert_eq!(reason.as_deref(), Some("user_requested"));

    let mine = discord.go_live(&guild, &lounge).await.unwrap();
    assert_eq!(
        stream::parse(&mine.stream_key),
        Some((guild.as_str(), lounge.as_str(), mine.user_id.as_str()))
    );
    assert_eq!(
        mine.session_id, watched.session_id,
        "both reuse the voice session"
    );
    snapshot_where(&discord, |snapshot| in_lounge(snapshot) == Some(true)).await;
    // The fake starts streams paused, and Yapfast unpauses its own at once.
    live_where(&mut live, |event| match event {
        Live::StreamUpdated {
            stream_key,
            paused: false,
            ..
        } if stream_key == mine.stream_key => Some(()),
        _ => None,
    })
    .await;
    // Someone starting and stopping watching shows in the stream's viewers.
    post_control(server, "/_control/stream-viewer?user=bob", "");
    live_where(&mut live, |event| match event {
        Live::StreamUpdated {
            stream_key,
            viewers,
            ..
        } if stream_key == mine.stream_key && viewers == ["1400000000000000002"] => Some(()),
        _ => None,
    })
    .await;
    post_control(server, "/_control/stream-viewer?user=bob&stop", "");
    live_where(&mut live, |event| match event {
        Live::StreamUpdated {
            stream_key,
            viewers,
            ..
        } if stream_key == mine.stream_key && viewers.is_empty() => Some(()),
        _ => None,
    })
    .await;
    // Discord moves the stream to another server, which Yapfast hears of.
    post_control(server, "/_control/stream-move", &mine.stream_key);
    let moved = live_where(&mut live, |event| match event {
        Live::StreamMoved(moved) if moved.stream_key == mine.stream_key => Some(moved),
        _ => None,
    })
    .await;
    assert_eq!(moved.rtc_server_id, "fake-rtc-moved");
    assert_eq!(
        (&moved.session_id, &moved.token),
        (&mine.session_id, &mine.token)
    );

    // The stream's video: the recording's first frames in the codec Yapfast
    // sends, a 320x180 keyframe and then deltas, arrive decrypted and whole
    // at the fake.
    let sender = StreamSender::start(mine.clone(), 320, 180, 30, vec![SEND_CODEC]).unwrap();
    timeout(WAIT, async {
        while sender.link() == Link::Connecting {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(sender.link(), Link::Connected);
    assert!(
        sender.take_keyframe_request(),
        "the stream starts with a keyframe"
    );
    let sound = sender.sound();
    for (index, data) in recording(SEND_CODEC).into_iter().take(5).enumerate() {
        sender.send(Frame {
            data,
            codec: SEND_CODEC,
            timestamp: index as u32 * 3000,
            keyframe: index == 0,
        });
        // Opus's 20 ms of silence.
        sound.send(Sound {
            opus: vec![0xf8, 0xff, 0xfe],
            timestamp: index as u32 * 960,
        });
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
    let video = timeout(WAIT, async {
        loop {
            let body = discord
                .download(&format!("{server}/_control/video"), 1 << 16)
                .await
                .unwrap();
            let streams: serde_json::Value = serde_json::from_slice(&body).unwrap();
            if let Some(found) = streams["received"]
                .as_array()
                .unwrap()
                .iter()
                .find(|found| {
                    found["stream"] == mine.stream_key.as_str()
                        && found["frames"] == 5
                        && found["audioPackets"] == 5
                        && found["retransmitted"] == 1
                })
            {
                return found.clone();
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("the fake didn't get five frames");
    assert_eq!(video["undecryptable"], 0);
    assert_eq!(video["unattached"], 0, "every packet names its stream");
    assert_eq!(video["audioUndecryptable"], 0);
    assert_eq!(
        video["badRetransmissions"], 0,
        "the reported packet came back as RTX"
    );
    assert_eq!(
        video["audioSenderReports"], 1,
        "one report in the first second"
    );
    assert_eq!(video["speaking"], 2, "the sound is marked as soundshare");
    // The stream's preview: a JPEG's start and end, as a data URI.
    discord
        .stream_preview(&mine.stream_key, "data:image/jpeg;base64,/9j/2Q==")
        .await
        .unwrap();
    assert!(
        discord
            .stream_preview(&bob_stream, "data:image/jpeg;base64,/9j/2Q==")
            .await
            .is_err(),
        "only the streamer shows a preview"
    );
    let body = discord
        .download(&format!("{server}/_control/video"), 1 << 16)
        .await
        .unwrap();
    let previews: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        previews["previews"],
        serde_json::json!([{"stream": mine.stream_key, "bytes": 4, "jpeg": true}])
    );
    assert_eq!(video["senderReports"], 1, "one report in the first second");
    assert!(video["reportedPackets"].as_u64() >= Some(1));
    assert_eq!(
        (
            video["keyframes"].as_u64(),
            video["width"].as_u64(),
            video["height"].as_u64()
        ),
        (Some(1), Some(320), Some(180))
    );
    assert_eq!(
        video["announced"],
        serde_json::json!({"type": "fixed", "width": 320, "height": 180})
    );
    // What the call's details show: the video's bytes, and the one packet
    // the fake reported lost, resent.
    let sent = sender.sent();
    assert!(sent.video_bytes > 1000, "{sent:?}");
    assert_eq!(sent.resent, 1);
    drop(sender);
    for codec in [Codec::Av1, Codec::H265] {
        streams_gpu_codec_until_a_viewer_cant_decode_it(&discord, server, &mine, codec).await;
    }

    streams_av1_with_viewer_fallback_and_encoder_withdrawal(&discord, server, &mine).await;
    discord.pause_stream(&mine.stream_key, true);
    live_where(&mut live, |event| match event {
        Live::StreamUpdated {
            stream_key,
            paused: true,
            ..
        } if stream_key == mine.stream_key => Some(()),
        _ => None,
    })
    .await;
    discord.end_stream(&mine.stream_key);
    snapshot_where(&discord, |snapshot| in_lounge(snapshot) == Some(false)).await;
    call.leave();
    discord.leave(&guild);
    snapshot_where(&discord, |snapshot| in_lounge(snapshot).is_none()).await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn follows_the_call_across_rooms_and_clients() {
    let Fixture {
        _fake,
        discord,
        server,
        snapshot,
        guild,
        lounge,
        ..
    } = Fixture::start("follows_the_call_across_rooms_and_clients").await;
    let mut live = discord.live();
    // Joining Create Room, a bot moves Alice to a room it makes, which puts
    // her call on another voice server; Yapfast hears where, and connects there.
    let voice_channel = |snapshot: &Snapshot, name: &str| {
        snapshot
            .servers
            .iter()
            .flat_map(|server| &server.voice_channels)
            .find(|found| found.name == name)
            .map(|found| found.id.clone())
    };
    let create_room = voice_channel(&discord.snapshots().borrow(), "Create Room").unwrap();
    discord.join(&guild, &create_room).await.unwrap();
    let room = live_where(&mut live, |event| match event {
        Live::VoiceChannel {
            guild: moved,
            channel,
        } if moved == guild && channel != lounge && channel != create_room => Some(channel),
        _ => None,
    })
    .await;
    let moved = live_where(&mut live, |event| match event {
        Live::VoiceMoved {
            guild: moved,
            server,
        } if moved == guild => Some(server),
        _ => None,
    })
    .await;
    assert_eq!(moved.token, "fake-voice-token-moved");
    let made = snapshot_where(&discord, |snapshot| {
        voice_channel(snapshot, "Alice's room").is_some()
    })
    .await;
    assert_eq!(voice_channel(&made, "Alice's room"), Some(room.clone()));
    let mut call = timeout(
        WAIT,
        Call::connect(&moved, &guild, &room, std::sync::Arc::new(Quiet)),
    )
    .await
    .expect("timed out connecting to the room")
    .unwrap();
    call.leave();
    discord.leave(&guild);
    let me = snapshot.user_id.clone().unwrap();
    let mine = |me: &str, channel: &str| {
        let (me, channel) = (me.to_owned(), channel.to_owned());
        move |snapshot: &Snapshot| {
            snapshot.voice.get(&channel).and_then(|members| {
                members
                    .iter()
                    .find(|member| member.user_id == me)
                    .map(|member| member.elsewhere)
            })
        }
    };
    snapshot_where(&discord, |snapshot| mine(&me, &room)(snapshot).is_none()).await;

    // Alice joins the Lounge on another client: she shows there as on
    // another client. Joining here takes the call over, with this
    // session's voice session.
    post_control(server, "/_control/elsewhere", "");
    snapshot_where(&discord, |snapshot| {
        mine(&me, &lounge)(snapshot) == Some(true)
    })
    .await;
    let assigned = discord.join(&guild, &lounge).await.unwrap();
    assert_ne!(assigned.session_id, "voice-alice-phone");
    snapshot_where(&discord, |snapshot| {
        mine(&me, &lounge)(snapshot) == Some(false)
    })
    .await;
    // The other client takes it back, which Yapfast hears of; leaving then
    // sends Discord nothing, which would take her out of that call too.
    post_control(server, "/_control/elsewhere", "");
    live_where(&mut live, |event| match event {
        Live::VoiceElsewhere { guild: there } if there == guild => Some(()),
        _ => None,
    })
    .await;
    snapshot_where(&discord, |snapshot| {
        mine(&me, &lounge)(snapshot) == Some(true)
    })
    .await;
    discord.leave(&guild);
    // A presence after it shows the fake had all this session sent before.
    discord.set_status("idle").await.unwrap();
    presence_where(server, |presence| presence["status"] == "idle").await;
    discord.set_status("online").await.unwrap();
    presence_where(server, |presence| presence["status"] == "online").await;
    post_control(server, "/_control/elsewhere?leave", "");
    snapshot_where(&discord, |snapshot| mine(&me, &lounge)(snapshot).is_none()).await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn plays_the_soundboard() {
    let Fixture {
        _fake,
        discord,
        server,
        snapshot,
        guild,
        lounge,
        bob,
        ..
    } = Fixture::start("plays_the_soundboard").await;
    let mut live = discord.live();
    let me = snapshot.user_id.clone().unwrap();
    // Discord's own sounds, asked for once, and the server's, with op 31.
    let defaults = discord.default_sounds().await.unwrap();
    assert_eq!(defaults.len(), 6);
    assert!(defaults.iter().all(|sound| sound.guild.is_none()));
    assert!(Arc::ptr_eq(
        &defaults,
        &discord.default_sounds().await.unwrap()
    ));
    discord.request_soundboard(vec![guild.clone()]);
    let sounds = live_where(&mut live, |event| match event {
        Live::SoundboardSounds { guild: of, sounds } if of == guild => Some(sounds),
        _ => None,
    })
    .await;
    let names: Vec<&str> = sounds.iter().map(|sound| sound.name.as_str()).collect();
    assert_eq!(names, ["Bob's laugh", "Applause"]);
    assert_eq!(sounds[0].emoji_id.as_deref(), Some("1400000000000000300"));

    // Playing one takes being in the call; then everyone there hears of
    // it, Alice too, with the sound's volume.
    let quack = defaults.iter().find(|sound| sound.name == "quack").unwrap();
    assert!(discord.send_sound(&lounge, quack).await.is_err());
    discord.join(&guild, &lounge).await.unwrap();
    discord.send_sound(&lounge, quack).await.unwrap();
    discord.send_sound(&lounge, &sounds[0]).await.unwrap();
    let played = |user: String| {
        move |event| match event {
            Live::SoundPlayed(played) if played.user == user => Some(played),
            _ => None,
        }
    };
    let heard = live_where(&mut live, played(me.clone())).await;
    assert_eq!(
        (heard.channel.as_str(), heard.sound.as_str()),
        (lounge.as_str(), "1")
    );
    let heard = live_where(&mut live, played(me.clone())).await;
    assert_eq!(
        (heard.sound.as_str(), heard.volume),
        (sounds[0].id.as_str(), 0.8)
    );
    // As Discord's client sends them: the server only for a server's sound.
    let sent: serde_json::Value =
        serde_json::from_str(&control(server, "GET", "/_control/soundboard-sends", "")).unwrap();
    assert_eq!(sent[1]["sound_id"], "1");
    assert_eq!(sent[1]["emoji_name"], "🦆");
    assert!(sent[1].get("source_guild_id").is_none(), "{sent}");
    assert_eq!(sent[2]["source_guild_id"], guild.as_str());
    assert_eq!(sent[2]["emoji_id"], "1400000000000000300");

    // Bob plays one; its file comes from the CDN, Ogg for the server's.
    post_control(server, "/_control/soundboard-play?user=bob", "");
    let heard = live_where(&mut live, played(bob.clone())).await;
    let file = discord.sound_file(&heard.sound).await.unwrap();
    assert!(file.starts_with(b"OggS"));
    assert!(discord.sound_file(&quack.id).await.unwrap().len() > 1000);

    // A sound added to the server arrives on its own.
    let added = control(server, "POST", "/_control/soundboard-add?name=Bell", "");
    let sound = live_where(&mut live, |event| match event {
        Live::SoundboardSound(sound) => Some(sound),
        _ => None,
    })
    .await;
    assert_eq!(
        (sound.id.as_str(), sound.name.as_str()),
        (added.trim(), "Bell")
    );
    assert_eq!(sound.guild.as_deref(), Some(guild.as_str()));
    discord.leave(&guild);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn keeps_the_soundboards_settings() {
    let Fixture {
        _fake,
        discord,
        server,
        snapshot,
        guild,
        lounge,
        ..
    } = Fixture::start("keeps_the_soundboards_settings").await;
    let mut live = discord.live();
    let me = snapshot.user_id.clone().unwrap();
    assert_eq!(snapshot.soundboard_volume, None);
    assert!(snapshot.join_sounds.is_empty());

    // The favourites, in their order, and changed, which every session
    // hears of.
    assert_eq!(
        discord.favorite_sounds().await.unwrap(),
        ["1400000000000000321", "2"]
    );
    discord
        .set_favorite_sounds(&["2".to_owned(), "1".to_owned()])
        .await
        .unwrap();
    let favorites = live_where(&mut live, |event| match event {
        Live::FavoriteSounds(favorites) => Some(favorites),
        _ => None,
    })
    .await;
    assert_eq!(favorites, ["2", "1"]);
    assert_eq!(discord.favorite_sounds().await.unwrap(), ["2", "1"]);

    // The volume, and a join sound for the server and one for every
    // server, keeping the custom status beside them.
    discord.set_soundboard_volume(42.5).await.unwrap();
    discord
        .set_join_sound(Some(&guild), Some(("1400000000000000320", Some(&guild))))
        .await
        .unwrap();
    discord
        .set_join_sound(None, Some(("2", None)))
        .await
        .unwrap();
    let snapshot = snapshot_where(&discord, |snapshot| snapshot.join_sounds.len() == 2).await;
    assert_eq!(snapshot.soundboard_volume, Some(42.5));
    assert_eq!(
        snapshot
            .custom_status
            .as_ref()
            .map(|custom| custom.text.as_str()),
        Some("Testing Yapfast")
    );

    // Joining with Nitro, the server's own plays for the others.
    post_control(server, "/_control/nitro", "");
    discord.join(&guild, &lounge).await.unwrap();
    discord.play_join_sound(&lounge).await.unwrap();
    let played = live_where(&mut live, |event| match event {
        Live::SoundPlayed(played) if played.user == me => Some(played),
        _ => None,
    })
    .await;
    assert_eq!(played.sound, "1400000000000000320");
    let asked: serde_json::Value =
        serde_json::from_str(&control(server, "GET", "/_control/custom-call-sounds", "")).unwrap();
    let animation = asked[0]["animation_id"].as_u64().unwrap();
    assert!((10..=15).contains(&animation), "{asked}");

    // Taken away, the one for every server is left.
    discord.set_join_sound(Some(&guild), None).await.unwrap();
    let snapshot = snapshot_where(&discord, |snapshot| snapshot.join_sounds.len() == 1).await;
    assert_eq!(snapshot.join_sounds[0].server, None);
    assert_eq!(snapshot.join_sounds[0].sound, "2");
    discord.leave(&guild);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn moderates_voice() {
    let Fixture {
        _fake,
        discord,
        server,
        snapshot,
        guild,
        lounge,
        ..
    } = Fixture::start("moderates_voice").await;
    let create_room = snapshot
        .servers
        .iter()
        .flat_map(|server| &server.voice_channels)
        .find(|found| found.name == "Create Room")
        .unwrap()
        .id
        .clone();
    // Alice's Staff role may mute, deafen and move members in voice, as
    // the voice menu does: Dave, muted and deafened by himself, is server
    // muted and deafened, then not, moved to Create Room and disconnected.
    let dave_in = |channel: &str| {
        let channel = channel.to_owned();
        move |snapshot: &Snapshot| {
            snapshot.voice.get(&channel).and_then(|members| {
                members
                    .iter()
                    .find(|member| member.name == "Dave")
                    .map(|member| (member.server_muted, member.server_deafened, member.muted))
            })
        }
    };
    let dave = snapshot_where(&discord, |snapshot| dave_in(&lounge)(snapshot).is_some())
        .await
        .voice[&lounge]
        .iter()
        .find(|member| member.name == "Dave")
        .unwrap()
        .user_id
        .clone();
    discord.server_mute(&guild, &dave, true).await.unwrap();
    snapshot_where(&discord, |snapshot| {
        dave_in(&lounge)(snapshot) == Some((true, false, true))
    })
    .await;
    discord.server_deafen(&guild, &dave, true).await.unwrap();
    snapshot_where(&discord, |snapshot| {
        dave_in(&lounge)(snapshot) == Some((true, true, true))
    })
    .await;
    discord.server_mute(&guild, &dave, false).await.unwrap();
    discord.server_deafen(&guild, &dave, false).await.unwrap();
    // Still muted by himself.
    snapshot_where(&discord, |snapshot| {
        dave_in(&lounge)(snapshot) == Some((false, false, true))
    })
    .await;
    discord
        .move_member(&guild, &dave, Some(&create_room))
        .await
        .unwrap();
    snapshot_where(&discord, |snapshot| {
        dave_in(&create_room)(snapshot).is_some() && dave_in(&lounge)(snapshot).is_none()
    })
    .await;
    discord.move_member(&guild, &dave, None).await.unwrap();
    snapshot_where(&discord, |snapshot| {
        dave_in(&create_room)(snapshot).is_none()
    })
    .await;
    discord
        .server_mute(&guild, &dave, true)
        .await
        .expect_err("he isn't in voice");
    let changes: serde_json::Value = serde_json::from_str(&crate::control(
        server,
        "GET",
        "/_control/voice-changes",
        "",
    ))
    .unwrap();
    assert_eq!(
        changes,
        serde_json::json!([
            {"user": dave, "mute": true},
            {"user": dave, "deaf": true},
            {"user": dave, "mute": false},
            {"user": dave, "deaf": false},
            {"user": dave, "channel_id": create_room},
            {"user": dave, "channel_id": null},
        ])
    );
    post_control(server, "/_control/voice-reset", "");
    snapshot_where(&discord, |snapshot| dave_in(&lounge)(snapshot).is_some()).await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn calls_in_dms() {
    let Fixture {
        _fake,
        discord,
        server,
        snapshot,
        bob,
        ..
    } = Fixture::start("calls_in_dms").await;
    let mut live = discord.live();
    let me = snapshot.user_id.clone().unwrap();
    // A DM call: Alice starts one with Bob, connecting with the DM for the
    // server, and rings him; he answers.
    let dm = discord
        .snapshots()
        .borrow()
        .conversations
        .iter()
        .find(|conversation| conversation.person.as_deref() == Some(bob.as_str()))
        .unwrap()
        .id
        .clone();
    let assigned = discord.join(PRIVATE, &dm).await.unwrap();
    let mut call = timeout(
        WAIT,
        Call::connect(&assigned, PRIVATE, &dm, std::sync::Arc::new(Quiet)),
    )
    .await
    .expect("timed out connecting to the DM call")
    .unwrap();
    let started = live_where(&mut live, |event| match event {
        Live::Created(message) if message.channel_id == dm => Some(message),
        _ => None,
    })
    .await;
    assert_eq!(started.system.as_deref(), Some("started a call."));
    let in_call = |snapshot: &Snapshot| -> Vec<String> {
        snapshot
            .voice
            .get(&dm)
            .into_iter()
            .flatten()
            .map(|member| member.name.clone())
            .collect()
    };
    let ringing = |snapshot: &Snapshot| -> Vec<String> {
        snapshot
            .conversations
            .iter()
            .find(|conversation| conversation.id == dm)
            .map(|conversation| {
                conversation
                    .ringing
                    .iter()
                    .map(|person| person.user_id.clone())
                    .collect()
            })
            .unwrap_or_default()
    };
    snapshot_where(&discord, |snapshot| in_call(snapshot) == ["Alice"]).await;
    assert!(discord.ring(&dm).await.unwrap(), "Bob is a friend");
    snapshot_where(&discord, |snapshot| ringing(snapshot) == [bob.clone()]).await;
    snapshot_where(&discord, |snapshot| {
        in_call(snapshot) == ["Alice", "Bob"] && ringing(snapshot).is_empty()
    })
    .await;
    cameras_in_call(
        server,
        &discord,
        &mut call,
        (PRIVATE, &dm, &format!("call-token-{dm}")),
        bob.parse().unwrap(),
    )
    .await;

    // Going live in a DM call names the stream by the call.
    let mine = discord.go_live(PRIVATE, &dm).await.unwrap();
    assert_eq!(mine.stream_key, format!("call:{dm}:{me}"));
    assert_eq!(
        stream::parse(&mine.stream_key),
        Some((PRIVATE, dm.as_str(), me.as_str()))
    );
    discord.end_stream(&mine.stream_key);
    call.leave();
    discord.leave(PRIVATE);
    snapshot_where(&discord, |snapshot| in_call(snapshot) == ["Bob"]).await;
    post_control(server, "/_control/hang-up", "");
    snapshot_where(&discord, |snapshot| !snapshot.voice.contains_key(&dm)).await;
    let ended = live_where(&mut live, |event| match event {
        Live::Updated(message) if message.id == started.id => Some(message),
        _ => None,
    })
    .await;
    assert_eq!(
        ended.system.as_deref(),
        Some("started a call that lasted a few seconds.")
    );

    // Bob calls Alice, and she declines; he stays until he hangs up.
    post_control(server, "/_control/call", "");
    snapshot_where(&discord, |snapshot| {
        in_call(snapshot) == ["Bob"] && ringing(snapshot) == [me.clone()]
    })
    .await;
    discord.decline(&dm).await.unwrap();
    snapshot_where(&discord, |snapshot| ringing(snapshot).is_empty()).await;
    assert_eq!(in_call(&discord.snapshots().borrow()), ["Bob"]);
    post_control(server, "/_control/hang-up", "");
    snapshot_where(&discord, |snapshot| !snapshot.voice.contains_key(&dm)).await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn rings_and_befriends() {
    let Fixture {
        _fake,
        discord,
        server,
        ..
    } = Fixture::start("rings_and_befriends").await;
    // Dave isn't a friend, and Discord doesn't let Alice ring him.
    post_control(server, "/_control/dm", "");
    let dave = snapshot_where(&discord, |snapshot| {
        snapshot
            .conversations
            .iter()
            .any(|found| found.name == "Dave")
    })
    .await
    .conversations
    .iter()
    .find(|found| found.name == "Dave")
    .cloned()
    .unwrap();
    let (dave, dave_user) = (dave.id.clone(), dave.person.clone().unwrap());
    let assigned = discord.join(PRIVATE, &dave).await.unwrap();
    let mut call = timeout(
        WAIT,
        Call::connect(&assigned, PRIVATE, &dave, std::sync::Arc::new(Quiet)),
    )
    .await
    .expect("timed out connecting to the DM call")
    .unwrap();
    snapshot_where(&discord, |snapshot| {
        snapshot
            .conversations
            .iter()
            .any(|found| found.id == dave && found.call)
    })
    .await;
    assert!(!discord.ring(&dave).await.unwrap(), "Dave can't be rung");

    // Alice sends Dave a friend request, which he accepts; then he can be rung.
    discord.befriend(&dave_user).await.unwrap();
    let requested = |snapshot: &Snapshot| -> Vec<(String, bool)> {
        snapshot
            .friend_requests
            .iter()
            .map(|request| (request.name.clone(), request.incoming))
            .collect()
    };
    snapshot_where(&discord, |snapshot| {
        requested(snapshot) == [("Erin".to_owned(), true), ("Dave".to_owned(), false)]
    })
    .await;
    snapshot_where(&discord, |snapshot| snapshot.friends.contains(&dave_user)).await;
    assert!(discord.ring(&dave).await.unwrap(), "Dave is a friend now");
    call.leave();
    discord.leave(PRIVATE);
    snapshot_where(&discord, |snapshot| !snapshot.voice.contains_key(&dave)).await;

    // Erin's friend request is ignored.
    let erin = discord.snapshots().borrow().friend_requests[0]
        .user_id
        .clone();
    discord.remove_friend_request(&erin).await.unwrap();
    snapshot_where(&discord, |snapshot| snapshot.friend_requests.is_empty()).await;

    // Friend requests by username: to someone unknown, to Frank, who takes
    // none, and to Erin.
    let refused = |name: &'static str| {
        let discord = &discord;
        async move {
            discord
                .add_friend_by_username(name)
                .await
                .expect_err("refused")
                .to_string()
        }
    };
    assert_eq!(
        refused("nobody").await,
        "Discord has nobody by that username."
    );
    assert_eq!(
        refused("frank_deals").await,
        "They don't take friend requests from people they don't know."
    );
    assert_eq!(refused("bob").await, "You're already friends.");
    discord.add_friend_by_username("erin").await.unwrap();
    snapshot_where(&discord, |snapshot| {
        requested(snapshot) == [("Erin".to_owned(), false)]
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn shows_status_and_games() {
    let Fixture {
        _fake,
        discord,
        server,
        ..
    } = Fixture::start_with_games("shows_status_and_games").await;
    // Alice's custom status from her settings shows from this session, and
    // changing it changes what the session shows.
    let custom = snapshot_where(&discord, |snapshot| snapshot.custom_status.is_some())
        .await
        .custom_status
        .clone()
        .unwrap();
    assert_eq!(
        (custom.text.as_str(), custom.emoji_name.as_deref()),
        ("Testing Yapfast", Some("🧪"))
    );
    presence_where(server, |presence| {
        presence["activities"][0]["state"] == "Testing Yapfast"
    })
    .await;
    #[cfg(target_os = "linux")]
    if let Some(directory) = std::env::var_os("YAPFAST_DISCORD_RPC_DIR") {
        plays_through_rpc(&discord, server, std::path::Path::new(&directory)).await;
    }
    let away = CustomStatus {
        text: "Fixing the bridge".into(),
        emoji_name: Some("🔧".into()),
        ..Default::default()
    };
    discord.set_custom_status(Some(away.clone())).await.unwrap();
    snapshot_where(&discord, |snapshot| {
        snapshot.custom_status.as_ref() == Some(&away)
    })
    .await;
    presence_where(server, |presence| {
        presence["activities"][0]["emoji"]["name"] == "🔧"
    })
    .await;
    discord.set_custom_status(None).await.unwrap();
    snapshot_where(&discord, |snapshot| snapshot.custom_status.is_none()).await;
    presence_where(server, |presence| {
        presence["activities"] == serde_json::json!([])
    })
    .await;
    // So does the status.
    discord.set_status("dnd").await.unwrap();
    snapshot_where(&discord, |snapshot| snapshot.presence == "dnd").await;
    presence_where(server, |presence| presence["status"] == "dnd").await;
    discord.set_status("online").await.unwrap();
    presence_where(server, |presence| presence["status"] == "online").await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn acts_from_a_dms_menu() {
    let Fixture {
        _fake,
        discord,
        server,
        ..
    } = Fixture::start("acts_from_a_dms_menu").await;
    // Dave writes, and Alice and he become friends.
    post_control(server, "/_control/dm", "");
    let dave = snapshot_where(&discord, |snapshot| {
        snapshot
            .conversations
            .iter()
            .any(|found| found.name == "Dave")
    })
    .await
    .conversations
    .iter()
    .find(|found| found.name == "Dave")
    .cloned()
    .unwrap();
    let (dave, dave_user) = (dave.id.clone(), dave.person.clone().unwrap());
    discord.befriend(&dave_user).await.unwrap();
    snapshot_where(&discord, |snapshot| snapshot.friends.contains(&dave_user)).await;
    // From Dave's DM's menu: a nickname for him and a note on him, ignoring
    // and muting, then unfriending, blocking and unblocking, and closing it.
    let dave_dm = |snapshot: &Snapshot| {
        snapshot
            .conversations
            .iter()
            .find(|found| found.id == dave)
            .cloned()
    };
    discord
        .set_friend_nickname(&dave_user, "Davey")
        .await
        .unwrap();
    snapshot_where(&discord, |snapshot| {
        dave_dm(snapshot).is_some_and(|dm| dm.name == "Davey")
            && snapshot
                .friend_list
                .iter()
                .any(|friend| friend.name == "Davey" && friend.nickname.as_deref() == Some("Davey"))
    })
    .await;
    assert_eq!(discord.note(&dave_user).await.unwrap(), "", "no note yet");
    discord
        .set_note(&dave_user, "Met at the jam")
        .await
        .unwrap();
    assert_eq!(discord.note(&dave_user).await.unwrap(), "Met at the jam");
    discord.ignore(&dave_user, true).await.unwrap();
    snapshot_where(&discord, |snapshot| {
        *snapshot.ignored == [dave_user.clone()]
    })
    .await;
    discord.ignore(&dave_user, false).await.unwrap();
    snapshot_where(&discord, |snapshot| snapshot.ignored.is_empty()).await;
    discord
        .mute_private(&dave, true, Some(15 * 60))
        .await
        .unwrap();
    snapshot_where(&discord, |snapshot| {
        dave_dm(snapshot).is_some_and(|dm| dm.muted)
    })
    .await;
    discord.mute_private(&dave, false, None).await.unwrap();
    snapshot_where(&discord, |snapshot| {
        dave_dm(snapshot).is_some_and(|dm| !dm.muted)
    })
    .await;
    discord.mute_private(&dave, true, None).await.unwrap();
    snapshot_where(&discord, |snapshot| {
        dave_dm(snapshot).is_some_and(|dm| dm.muted)
    })
    .await;

    discord.unfriend(&dave_user).await.unwrap();
    snapshot_where(&discord, |snapshot| {
        !snapshot.friends.contains(&dave_user)
            && dave_dm(snapshot).is_some_and(|dm| dm.name == "Dave")
    })
    .await;
    discord.block(&dave_user, true).await.unwrap();
    snapshot_where(&discord, |snapshot| {
        snapshot.blocked.contains(&dave_user)
            && snapshot
                .blocked_list
                .iter()
                .any(|blocked| blocked.user_id == dave_user && blocked.name == "Dave")
    })
    .await;
    discord.block(&dave_user, false).await.unwrap();
    snapshot_where(&discord, |snapshot| {
        !snapshot.blocked.contains(&dave_user) && snapshot.blocked_list.is_empty()
    })
    .await;
    discord.close_dm(&dave).await.unwrap();
    snapshot_where(&discord, |snapshot| dave_dm(snapshot).is_none()).await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn changes_notification_settings() {
    let Fixture {
        _fake,
        discord,
        server,
        guild,
        general,
        ..
    } = Fixture::start("changes_notification_settings").await;
    // The server's and #general's notification settings, as Discord
    // confirms them: a channel's mute keeps its level, and the server's
    // switches are its own.
    let ours = |snapshot: &Snapshot| {
        snapshot
            .servers
            .iter()
            .find(|server| server.id == guild)
            .cloned()
            .unwrap()
    };
    let change = |channel: Option<&'static str>, change| {
        let (discord, guild, general) = (discord.clone(), guild.clone(), general.clone());
        async move {
            let channel = channel.map(|_| general.as_str());
            discord
                .change_notifications(&guild, channel, change)
                .await
                .unwrap();
        }
    };
    let here = Some("general");
    change(
        here,
        Change::Mute {
            muted: true,
            seconds: Some(3600),
        },
    )
    .await;
    change(here, Change::Level(Some(Level::Nothing))).await;
    change(None, Change::HideMutedChannels(true)).await;
    change(None, Change::Level(Some(Level::All))).await;
    let muted = snapshot_where(&discord, |snapshot| {
        let server = ours(snapshot);
        server.marks.notifications.hide_muted_channels
            && server.marks.notifications.level == Some(Level::All)
            && server.text_channels().next().unwrap().marks.muted
            && server.marks.notifications.of(&general).level == Some(Level::Nothing)
    })
    .await;
    let own = ours(&muted).marks.notifications.of(&general);
    assert!(own.mute.muted && own.mute.until.is_some());
    change(
        here,
        Change::Mute {
            muted: false,
            seconds: None,
        },
    )
    .await;
    change(
        None,
        Change::Mute {
            muted: true,
            seconds: None,
        },
    )
    .await;
    snapshot_where(&discord, |snapshot| {
        let server = ours(snapshot);
        server.marks.muted
            && server.marks.notifications.mute.until.is_none()
            && !server.text_channels().next().unwrap().marks.muted
            && server.marks.notifications.of(&general).level == Some(Level::Nothing)
    })
    .await;
    // As it was, for what follows.
    change(
        None,
        Change::Mute {
            muted: false,
            seconds: None,
        },
    )
    .await;
    change(None, Change::HideMutedChannels(false)).await;
    change(None, Change::Level(Some(Level::Mentions))).await;
    change(here, Change::Level(None)).await;
    snapshot_where(&discord, |snapshot| {
        let server = ours(snapshot);
        !server.marks.muted
            && server.marks.notifications.level == Some(Level::Mentions)
            && server.marks.notifications.of(&general).level.is_none()
    })
    .await;

    // Pinning #general to the top sets its override's flags, through the
    // settings of several servers at once, and keeps its level; unpinning
    // leaves the opt-in flag that pinning set.
    change(here, Change::Level(Some(Level::All))).await;
    let flags = ours(&discord.snapshots().borrow())
        .marks
        .notifications
        .of(&general)
        .pin_flags(true);
    assert_eq!(flags, 6144);
    discord
        .channel_flags(&guild, &general, flags)
        .await
        .unwrap();
    snapshot_where(&discord, |snapshot| {
        let own = ours(snapshot).marks.notifications.of(&general);
        own.pinned() && own.level == Some(Level::All)
    })
    .await;
    discord.channel_flags(&guild, &general, 4096).await.unwrap();
    change(here, Change::Level(None)).await;
    let unpinned = snapshot_where(&discord, |snapshot| {
        let own = ours(snapshot).marks.notifications.of(&general);
        !own.pinned() && own.level.is_none()
    })
    .await;
    assert_eq!(ours(&unpinned).marks.notifications.of(&general).flags, 4096);

    // A Community server lists only the channels followed once Show All
    // Channels is off, which the server's own flags say, following channels
    // with it; a channel's flags take it off the list.
    post_control(server, "/_control/guild", r#"{"features": ["COMMUNITY"]}"#);
    snapshot_where(&discord, |snapshot| ours(snapshot).community).await;
    discord
        .show_all_channels(&guild, 16384, &[(general.clone(), 4096)])
        .await
        .unwrap();
    snapshot_where(&discord, |snapshot| {
        let server = ours(snapshot);
        server.follows_only() && server.marks.notifications.follows(&general, None)
    })
    .await;
    discord.channel_flags(&guild, &general, 0).await.unwrap();
    discord.show_all_channels(&guild, 0, &[]).await.unwrap();
    snapshot_where(&discord, |snapshot| {
        let server = ours(snapshot);
        !server.follows_only() && !server.marks.notifications.follows(&general, None)
    })
    .await;
    post_control(server, "/_control/guild", r#"{"features": []}"#);
    snapshot_where(&discord, |snapshot| !ours(snapshot).community).await;

    // A category mutes as a channel does, and its channels show muted.
    let category = ours(&discord.snapshots().borrow())
        .text_channels
        .iter()
        .find(|channel| channel.id == general)
        .and_then(|channel| channel.parent.clone())
        .unwrap();
    let mute_category = |muted: bool| {
        let (discord, guild, category) = (discord.clone(), guild.clone(), category.clone());
        async move {
            let seconds = muted.then_some(900);
            discord
                .change_notifications(&guild, Some(&category), Change::Mute { muted, seconds })
                .await
                .unwrap();
        }
    };
    let general_muted = |snapshot: &Snapshot| {
        ours(snapshot)
            .text_channels()
            .any(|channel| channel.id == general && channel.marks.muted)
    };
    mute_category(true).await;
    snapshot_where(&discord, |snapshot| {
        general_muted(snapshot) && ours(snapshot).marks.notifications.of(&category).mute.muted
    })
    .await;
    mute_category(false).await;
    snapshot_where(&discord, |snapshot| !general_muted(snapshot)).await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn follows_the_topic() {
    let Fixture {
        _fake,
        discord,
        server,
        guild,
        general,
        bob,
        ..
    } = Fixture::start("follows_the_topic").await;
    let ours = |snapshot: &Snapshot| {
        snapshot
            .servers
            .iter()
            .find(|server| server.id == guild)
            .cloned()
            .unwrap()
    };
    // #general's topic comes with the server, and CHANNEL_UPDATE changes it.
    let topic = |snapshot: &Snapshot| {
        ours(snapshot)
            .text_channels
            .iter()
            .find(|channel| channel.id == general)
            .and_then(|channel| channel.topic.clone())
    };
    let first = topic(&discord.snapshots().borrow()).unwrap();
    assert!(first.contains(&format!("<@{bob}>")), "{first}");
    post_control(server, "/_control/topic", "New **plans**");
    snapshot_where(&discord, |snapshot| {
        topic(snapshot).as_deref() == Some("New **plans**")
    })
    .await;
    post_control(server, "/_control/topic", "");
    snapshot_where(&discord, |snapshot| topic(snapshot).is_none()).await;
    post_control(server, "/_control/topic", &first);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn resumes_the_gateway() {
    let Fixture {
        _fake,
        discord,
        server,
        ..
    } = Fixture::start("resumes_the_gateway").await;
    let mut live = discord.live();
    // Discord asks the gateway to reconnect: the session resumes, without a
    // new identify, and carries on.
    post_control(server, "/_control/gateway-reconnect", "");
    timeout(WAIT, async {
        while self::control(server, "GET", "/_control/sessions", "")
            != r#"{"identifies":1,"resumes":1}"#
        {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("the session didn't resume");
    post_control(server, "/_control/typing", "");
    live_where(&mut live, |event| {
        matches!(event, Live::Typing { .. }).then_some(())
    })
    .await;
    assert_eq!(discord.snapshots().borrow().status, Status::Online);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn links_by_qr_code() {
    let Fixture { _fake, server, .. } = Fixture::start("links_by_qr_code").await;
    // Linking by QR code: the code shows, Alice scans it and confirms on her
    // phone, and the login arrives.
    let (steps, mut shown) = tokio::sync::mpsc::unbounded_channel();
    let linking = tokio::spawn(remote_auth::link(move |step| {
        let _ = steps.send(step);
    }));
    let Step::Code(code) = next_step(&mut shown).await else {
        panic!("no code")
    };
    assert!(code.starts_with("https://discord.com/ra/"), "{code}");
    post_control(server, "/_control/qr-scan", "");
    assert!(matches!(next_step(&mut shown).await, Step::Scanned { name, .. } if name == "alice"));
    post_control(server, "/_control/qr-confirm", "");
    let login = timeout(WAIT, linking).await.unwrap().unwrap().unwrap();
    assert_eq!(login, client::TEST_TOKEN);

    // Saying no on the phone stops linking.
    let (steps, mut shown) = tokio::sync::mpsc::unbounded_channel();
    let linking = tokio::spawn(remote_auth::link(move |step| {
        let _ = steps.send(step);
    }));
    assert!(matches!(next_step(&mut shown).await, Step::Code(_)));
    post_control(server, "/_control/qr-cancel", "");
    let refused = timeout(WAIT, linking).await.unwrap().unwrap().unwrap_err();
    assert_eq!(
        refused.downcast_ref(),
        Some(&remote_auth::Stopped::Cancelled)
    );

    // Discord asks for a captcha before it hands the login over: Yapfast
    // serves the challenge on a page of its own and trades the ticket again
    // with the solution the page posts back.
    post_control(server, "/_control/qr-captcha", "");
    let (steps, mut shown) = tokio::sync::mpsc::unbounded_channel();
    let linking = tokio::spawn(remote_auth::link(move |step| {
        let _ = steps.send(step);
    }));
    assert!(matches!(next_step(&mut shown).await, Step::Code(_)));
    post_control(server, "/_control/qr-scan", "");
    assert!(matches!(next_step(&mut shown).await, Step::Scanned { .. }));
    post_control(server, "/_control/qr-confirm", "");
    let Step::Captcha(page) = next_step(&mut shown).await else {
        panic!("no captcha")
    };
    let browser = reqwest::Client::new();
    let html = browser
        .get(&page)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(html.contains(r#""fake-sitekey""#), "{html}");
    assert!(html.contains(r#""fake-rqdata""#), "{html}");
    browser.post(&page).body("solved").send().await.unwrap();
    let login = timeout(WAIT, linking).await.unwrap().unwrap().unwrap();
    assert_eq!(login, client::TEST_TOKEN);

    // Yapfast sent back the device cookie the API set, once it had it: the API
    // set it once, or twice if the first requests went together.
    let cookies: serde_json::Value =
        serde_json::from_str(&self::control(server, "GET", "/_control/cookies", "")).unwrap();
    // The cookie jar outlives each test's fake, so this one may find it
    // already set.
    assert!(cookies["returned"].as_u64().unwrap() > 0, "{cookies}");
    assert!(cookies["set"].as_u64().unwrap() <= 2, "{cookies}");
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn makes_invites() {
    let Fixture {
        _fake,
        discord,
        server,
        general,
        ..
    } = Fixture::start("makes_invites").await;
    let carol_id = discord.find_people("carol")[0].id.clone();
    let carol_dm = discord.open_dm(&carol_id).await.unwrap();
    // An invite to #general, as Discord's invite dialog asks for one.
    let code = discord.create_invite(&general, 3600, 5).await.unwrap();
    assert_eq!(code.len(), 8);
    let invites: serde_json::Value =
        serde_json::from_str(&self::control(server, "GET", "/_control/invites", "")).unwrap();
    let made = invites
        .as_array()
        .unwrap()
        .iter()
        .find(|invite| invite["code"] == code.as_str())
        .unwrap();
    assert_eq!(
        (made["max_age"].as_u64(), made["max_uses"].as_u64()),
        (Some(3600), Some(5))
    );
    assert_eq!(made["channel"]["id"], general.as_str());
    assert!(discord.create_invite(&general, 3600, 101).await.is_err());

    // Invite to Server, from Carol's menu: an invite that works once and
    // lasts Discord's default day, its link sent in her DM.
    discord.send_invite(&carol_id, &general).await.unwrap();
    let invites: serde_json::Value =
        serde_json::from_str(&self::control(server, "GET", "/_control/invites", "")).unwrap();
    let sent = invites.as_array().unwrap().last().unwrap();
    assert_eq!(
        (sent["max_age"].as_u64(), sent["max_uses"].as_u64()),
        (Some(86_400), Some(1))
    );
    assert_eq!(sent["location"], "User Invite Context Menu");
    let in_dm = discord
        .history(PRIVATE, &carol_dm, Span::Latest)
        .await
        .unwrap()
        .messages;
    assert_eq!(
        in_dm.last().map(|message| message.content.as_str()),
        Some(format!("https://discord.gg/{}", sent["code"].as_str().unwrap()).as_str())
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn manages_channels_and_categories() {
    let Fixture {
        _fake,
        discord,
        guild,
        ..
    } = Fixture::start("manages_channels_and_categories").await;
    // Channels and categories, which her Crew role may manage: one made
    // shows once its CHANNEL_CREATE comes, with Discord's name for it.
    use yapfast_discord::service::ChannelSettings;
    let named = |name: &str| ChannelSettings {
        name: name.into(),
        ..Default::default()
    };
    let shelf = discord
        .create_channel(&guild, 4, None, None, &named("Shelf"))
        .await
        .unwrap();
    let plans = discord
        .create_channel(
            &guild,
            0,
            Some(&shelf),
            None,
            &ChannelSettings {
                topic: "What's next".into(),
                slowmode: 30,
                ..named("Plans For Later")
            },
        )
        .await
        .unwrap();
    let snapshot = snapshot_where(&discord, |snapshot| {
        snapshot.servers[0]
            .text_channels
            .iter()
            .any(|channel| channel.id == plans)
    })
    .await;
    let listed = &snapshot.servers[0];
    assert_eq!(listed.categories[&shelf].name, "Shelf");
    let made = listed
        .text_channels
        .iter()
        .find(|channel| channel.id == plans)
        .unwrap();
    assert_eq!(made.name, "plans-for-later");
    assert_eq!(made.parent.as_deref(), Some(shelf.as_str()));
    let settings = discord.channel_settings(&plans).await.unwrap();
    assert_eq!(
        (settings.topic.as_str(), settings.slowmode, settings.nsfw),
        ("What's next", 30, false)
    );

    // Changing it, as its settings do, and copying it, as Duplicate Channel
    // does, with its category and settings.
    discord
        .edit_channel(
            &plans,
            true,
            &ChannelSettings {
                name: "plans".into(),
                topic: "Soon".into(),
                slowmode: 0,
                nsfw: true,
            },
        )
        .await
        .unwrap();
    snapshot_where(&discord, |snapshot| {
        snapshot.servers[0]
            .text_channels
            .iter()
            .any(|channel| channel.id == plans && channel.name == "plans")
    })
    .await;
    let copy = discord
        .create_channel(&guild, 0, None, Some(&plans), &named("plans-copy"))
        .await
        .unwrap();
    let copied = discord.channel_settings(&copy).await.unwrap();
    assert_eq!(
        (copied.name.as_str(), copied.topic.as_str(), copied.nsfw),
        ("plans-copy", "Soon", true)
    );
    let snapshot = snapshot_where(&discord, |snapshot| {
        snapshot.servers[0]
            .text_channels
            .iter()
            .any(|channel| channel.id == copy)
    })
    .await;
    assert_eq!(
        snapshot.servers[0]
            .text_channels
            .iter()
            .find(|channel| channel.id == copy)
            .unwrap()
            .parent
            .as_deref(),
        Some(shelf.as_str())
    );

    // Deleting a category leaves its channels outside any; then they go too.
    discord.delete_channel(&shelf).await.unwrap();
    snapshot_where(&discord, |snapshot| {
        let listed = &snapshot.servers[0];
        !listed.categories.contains_key(&shelf)
            && listed
                .text_channels
                .iter()
                .filter(|channel| channel.id == plans || channel.id == copy)
                .all(|channel| channel.parent.is_none())
    })
    .await;
    discord.delete_channel(&plans).await.unwrap();
    discord.delete_channel(&copy).await.unwrap();
    snapshot_where(&discord, |snapshot| {
        !snapshot.servers[0]
            .text_channels
            .iter()
            .any(|channel| channel.id == plans || channel.id == copy)
    })
    .await;
    assert!(
        discord
            .create_channel(&guild, 0, None, None, &named(""))
            .await
            .is_err()
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn highlights_mentions() {
    let Fixture {
        _fake,
        discord,
        server,
        guild,
        general,
        ..
    } = Fixture::start("highlights_mentions").await;
    // Bob calls on Alice every way Discord highlights for her, through her
    // Crew role among them, and two ways it doesn't.
    post_control(server, "/_control/mentions", "");
    let snapshot = discord.snapshots().borrow().clone();
    let me = snapshot.user_id.clone().unwrap();
    assert!(
        snapshot.servers[0]
            .marks
            .roles
            .contains(&"1400000000000000112".to_owned())
    );
    let latest = discord
        .history(&guild, &general, Span::Latest)
        .await
        .unwrap()
        .messages;
    let highlighted: Vec<bool> = latest[latest.len() - 7..]
        .iter()
        .map(|message| message.mentions_me(&me, Some(&snapshot.servers[0].marks)))
        .collect();
    assert_eq!(
        highlighted,
        [true, true, true, true, true, false, false],
        "by name, Crew, @everyone, @here and a reply; not Mods, nor nothing"
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn acts_from_a_members_menu() {
    let Fixture {
        _fake,
        discord,
        server,
        snapshot,
        guild,
        general,
        bob,
        ..
    } = Fixture::start("acts_from_a_members_menu").await;
    let me = snapshot.user_id.clone().unwrap();
    // A member's menu. Alice is Staff: she renames herself, and Dave, who
    // has no role, but not Carol, a Lead above her. Each change comes back
    // as GUILD_MEMBER_UPDATE, and the member list follows.
    let (carol, dave) = ("1400000000000000003", "1400000000000000004");
    let mods = "1400000000000000110";
    let mut members = discord.live();
    discord.view(&guild, Some(&general));
    let listed =
        |live: Live,
         check: &dyn Fn(Option<&yapfast_discord::member_list::ListedMember>) -> bool| {
            let Live::Members { channel, members } = live else {
                return None;
            };
            (channel == general && check(members.iter().find(|member| member.user_id == dave)))
                .then_some(())
        };
    discord.set_member_nick(&guild, &me, "Ally").await.unwrap();
    discord
        .set_member_nick(&guild, dave, "Davey")
        .await
        .unwrap();
    assert!(discord.set_member_nick(&guild, carol, "Caz").await.is_err());
    live_where(&mut members, |live| {
        listed(live, &|dave| dave.is_some_and(|dave| dave.name == "Davey"))
    })
    .await;
    assert_eq!(discord.name(&guild, &me).as_deref(), Some("Ally"));

    // Roles below hers she gives and takes; Dave moves under Mods and
    // takes its colour.
    discord.member_role(&guild, dave, mods, true).await.unwrap();
    live_where(&mut members, |live| {
        listed(live, &|dave| {
            dave.is_some_and(|dave| {
                dave.group.starts_with("Mods")
                    && dave.color == Some(3447003)
                    && dave.roles.contains(&mods.to_owned())
            })
        })
    })
    .await;
    for above in ["1400000000000000113", "1400000000000000114"] {
        assert!(
            discord
                .member_role(&guild, dave, above, true)
                .await
                .is_err(),
            "{above}"
        );
    }
    discord
        .member_role(&guild, dave, mods, false)
        .await
        .unwrap();
    live_where(&mut members, |live| {
        listed(live, &|dave| {
            dave.is_some_and(|dave| !dave.group.starts_with("Mods"))
        })
    })
    .await;

    // A timeout, with its reason in the audit log, and its end.
    let soon = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
        + 60_000;
    discord
        .time_out(&guild, dave, Some(soon), "spam, ☕")
        .await
        .unwrap();
    live_where(&mut members, |live| {
        listed(live, &|dave| {
            dave.is_some_and(|dave| dave.timed_out_until.is_some())
        })
    })
    .await;
    assert!(
        discord
            .time_out(&guild, carol, Some(soon), "")
            .await
            .is_err()
    );
    discord.time_out(&guild, dave, None, "").await.unwrap();
    live_where(&mut members, |live| {
        listed(live, &|dave| {
            dave.is_some_and(|dave| dave.timed_out_until.is_none())
        })
    })
    .await;

    // A kick and a ban take him off the list; the owner can't be.
    assert!(discord.kick(&guild, &bob, "").await.is_err());
    discord.kick(&guild, dave, "").await.unwrap();
    live_where(&mut members, |live| listed(live, &|dave| dave.is_none())).await;
    post_control(server, "/_control/members-reset", "");
    live_where(&mut members, |live| listed(live, &|dave| dave.is_some())).await;
    discord.ban(&guild, dave, 3600, "raids").await.unwrap();
    live_where(&mut members, |live| listed(live, &|dave| dave.is_none())).await;
    let audit: serde_json::Value =
        serde_json::from_str(&self::control(server, "GET", "/_control/audit-log", "")).unwrap();
    assert_eq!(
        audit,
        serde_json::json!([
            {"action": "timeout", "user": dave, "reason": "spam, ☕"},
            {"action": "timeout-removed", "user": dave, "reason": null},
            {"action": "kick", "user": dave, "reason": null},
            {"action": "ban", "user": dave, "reason": "raids"},
        ])
    );
    post_control(server, "/_control/members-reset", "");
    live_where(&mut members, |live| listed(live, &|dave| dave.is_some())).await;
    assert_eq!(discord.name(&guild, &me).as_deref(), Some("Alice"));
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn edits_the_server_profile() {
    let Fixture {
        _fake,
        discord,
        server,
        snapshot,
        guild,
        ..
    } = Fixture::start("edits_the_server_profile").await;
    let me = snapshot.user_id.clone().unwrap();
    // Her per-server profile: a nickname and pronouns anyone may set, the
    // rest only with Nitro. Her server avatar comes back with
    // GUILD_MEMBER_UPDATE; her banner, bio and pronouns with her profile.
    let own = discord.own_profile(Some(&guild)).await.unwrap();
    assert_eq!(
        (own.name.as_str(), own.avatar.as_deref(), own.bio.as_str()),
        ("", None, "")
    );
    assert_eq!(own.fallback.name, "Alice");
    let none = serde_json::json!({});
    discord
        .save_server_profile(
            &guild,
            &serde_json::json!({"nick": "Ally"}),
            &serde_json::json!({"pronouns": "she/her"}),
        )
        .await
        .unwrap();
    let bio = serde_json::json!({"bio": "Here for #general."});
    assert!(
        discord
            .save_server_profile(&guild, &none, &bio)
            .await
            .is_err(),
        "a server bio takes Nitro"
    );
    post_control(server, "/_control/nitro", "");
    snapshot_where(&discord, |snapshot| {
        snapshot
            .account
            .as_ref()
            .is_some_and(|account| account.premium_type == 2)
    })
    .await;
    let dot = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";
    discord
        .save_server_profile(
            &guild,
            &serde_json::json!({"avatar": dot}),
            &serde_json::json!({"banner": dot, "bio": "Here for #general."}),
        )
        .await
        .unwrap();
    until("her server avatar shows", || {
        discord
            .avatar(Some(&guild), &me)
            .contains(&format!("/guilds/{guild}/users/{me}/avatars/"))
    })
    .await;
    let own = discord.own_profile(Some(&guild)).await.unwrap();
    assert_eq!(own.name, "Ally");
    assert_eq!(own.pronouns, "she/her");
    assert_eq!(own.bio, "Here for #general.");
    assert!(
        own.banner
            .as_deref()
            .is_some_and(|banner| banner.contains("/banners/"))
    );
    let card = discord.profile(&guild, &me, |_| {}).await.unwrap();
    assert_eq!(card.bio.as_deref(), Some("Here for #general."));
    assert_eq!(card.pronouns.as_deref(), Some("she/her"));
    // Empty texts and no pictures go by her main profile again.
    discord
        .save_server_profile(
            &guild,
            &serde_json::json!({"nick": "", "avatar": null}),
            &serde_json::json!({"banner": null, "bio": "", "pronouns": ""}),
        )
        .await
        .unwrap();
    let own = discord.own_profile(Some(&guild)).await.unwrap();
    assert_eq!(
        (own.name, own.pronouns, own.bio, own.avatar, own.banner),
        (String::new(), String::new(), String::new(), None, None)
    );
    post_control(server, "/_control/nitro?off", "");
    snapshot_where(&discord, |snapshot| {
        snapshot
            .account
            .as_ref()
            .is_some_and(|account| account.premium_type == 0)
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn leaves_and_joins_a_server() {
    let Fixture {
        _fake,
        discord,
        server,
        guild,
        general,
        ..
    } = Fixture::start("leaves_and_joins_a_server").await;
    // Leaving "Server", which Bob owns, takes it out of her list; the fake
    // lets her back in, so the tests after these find it again.
    discord.leave_server(&guild).await.unwrap();
    snapshot_where(&discord, |snapshot| {
        !snapshot.servers.iter().any(|known| known.id == guild)
    })
    .await;
    assert!(
        discord.leave_server(&guild).await.is_err(),
        "she isn't in it"
    );
    joins_by_an_invite_as_discords_client_does(&discord, server, &general).await;
    let snapshot = snapshot_where(&discord, |snapshot| {
        snapshot.servers.iter().any(|known| known.id == guild)
    })
    .await;
    assert!(
        snapshot.servers[0]
            .text_channels
            .iter()
            .any(|channel| channel.id == general)
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn refreshes_the_login_and_signs_out() {
    let Fixture {
        _fake,
        discord,
        server,
        guild,
        ..
    } = Fixture::start("refreshes_the_login_and_signs_out").await;
    // Discord refreshes the login in a READY: Yapfast asks with the new one
    // from then on, and keeps it. The fake takes only the latest, and hands
    // the first back the same way for the tests after these.
    for login in ["yapfast-refreshed-token", client::TEST_TOKEN] {
        post_control(server, "/_control/refresh-token", login);
        timeout(Duration::from_secs(30), async {
            while token::load().unwrap() != login {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("the refreshed login wasn't kept");
        discord.own_profile(Some(&guild)).await.unwrap();
    }

    // Signing out ends the session on Discord.
    discord.logout().await.unwrap();
    assert_eq!(self::control(server, "GET", "/_control/logouts", ""), "1");
}

/// Changing "Server"'s profile in its settings as Discord's client does,
/// which Crew's Manage Server allows, and confirming with 2FA what Discord
/// holds back; part of [`talks_to_the_fake_server`], whose session it
/// shares. It puts "Server" and its members back after.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn changes_server_settings_as_discords_client_does() {
    let fixture = Fixture::start("changes_server_settings_as_discords_client_does").await;
    let discord = &fixture.discord;
    let server = fixture.server;
    let guild = fixture.guild.as_str();
    use serde_json::json;
    use yapfast_discord::mfa::Method;
    let profile = discord.guild_profile(guild).await.unwrap();
    assert_eq!(
        (profile.name.as_str(), profile.description.as_str()),
        ("Server", "")
    );
    assert!(profile.icon.is_some());

    // Discord's own words for a field it refuses.
    let refused = discord
        .save_guild_profile(guild, &json!({"name": "S"}))
        .await
        .unwrap_err();
    assert_eq!(
        refused.to_string(),
        "Name: Must be between 2 and 100 in length."
    );
    let saved = discord
        .save_guild_profile(
            guild,
            &json!({"name": "Crew", "description": "Where we talk", "icon": null}),
        )
        .await
        .unwrap();
    assert_eq!(
        (saved.name.as_str(), saved.description.as_str(), saved.icon),
        ("Crew", "Where we talk", None)
    );
    snapshot_where(discord, |snapshot| {
        snapshot
            .servers
            .iter()
            .any(|known| known.id == guild && known.name == "Crew" && known.icon.is_none())
    })
    .await;

    // Once "Server" requires 2FA for moderation, a kick waits for her to
    // confirm, and goes again with the token; a wrong code leaves the
    // prompt open.
    post_control(
        server,
        "/_control/mfa",
        r#"{"methods": ["totp", "backup"]}"#,
    );
    let dave = "1400000000000000004";
    let mut live = discord.live();
    let required = |live: Live| match live {
        Live::MfaRequired { id, methods } => Some((id, methods)),
        _ => None,
    };
    let kick = tokio::spawn({
        let (discord, guild) = (discord.clone(), guild.to_owned());
        async move {
            discord
                .stepping_up(|| discord.kick(&guild, dave, "spam"))
                .await
        }
    });
    let (id, methods) = live_where(&mut live, required).await;
    assert_eq!(methods, [Method::Totp, Method::Backup]);
    let wrong = discord
        .finish_mfa(id, Method::Totp, "000000")
        .await
        .unwrap_err();
    assert_eq!(
        wrong.to_string(),
        "That didn't work. Check it and try again."
    );
    discord
        .finish_mfa(id, Method::Totp, "123456")
        .await
        .unwrap();
    kick.await.unwrap().unwrap();

    // Cancelling fails what waits.
    let save = tokio::spawn({
        let (discord, guild) = (discord.clone(), guild.to_owned());
        async move {
            let body = json!({"name": "Server"});
            discord
                .stepping_up(|| discord.save_guild_profile(&guild, &body))
                .await
        }
    });
    let (id, _) = live_where(&mut live, required).await;
    discord.cancel_mfa(id);
    assert_eq!(
        save.await.unwrap().unwrap_err().to_string(),
        "Two-factor authentication was cancelled."
    );
    let tried: serde_json::Value =
        serde_json::from_str(&control(server, "GET", "/_control/mfa", "")).unwrap();
    assert_eq!(
        tried,
        json!([{"mfa_type": "totp", "data": "000000"}, {"mfa_type": "totp", "data": "123456"}])
    );
    post_control(server, "/_control/mfa?off", "");

    // Engagement: the fields Discord's page sends, each as GUILD_UPDATE
    // brings it back.
    let settings = discord.guild_settings(guild).await.unwrap();
    assert_eq!(
        (
            settings.system_channel.as_deref(),
            settings.afk_timeout,
            settings.premium_tier
        ),
        (Some("1400000000000000102"), 300, 0)
    );
    let lounge = "1400000000000000103";
    let saved = discord
        .save_guild_settings(
            guild,
            &json!({"system_channel_flags": 5, "default_message_notifications": 0,
                "afk_channel_id": lounge, "afk_timeout": 900}),
        )
        .await
        .unwrap();
    assert_eq!(
        (
            saved.system_flags,
            saved.default_notifications,
            saved.afk_channel.as_deref(),
            saved.afk_timeout
        ),
        (5, 0, Some(lounge), 900)
    );
    live_where(&mut live, |live| match live {
        Live::ServerSettings {
            guild: changed,
            settings,
        } if changed == guild && settings.afk_timeout == 900 => Some(()),
        _ => None,
    })
    .await;

    // Boost Perks: a banner waits for level 2.
    let dot = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";
    assert!(
        discord
            .save_guild_settings(guild, &json!({"banner": dot}))
            .await
            .is_err()
    );
    post_control(server, "/_control/guild", r#"{"premium_tier": 2}"#);
    let saved = discord
        .save_guild_settings(
            guild,
            &json!({"banner": dot, "premium_progress_bar_enabled": true}),
        )
        .await
        .unwrap();
    let banner = saved.banner.expect("a banner at level 2");
    assert!(banner.contains(&format!("/banners/{guild}/")));
    assert!(saved.progress_bar);
    snapshot_where(discord, |snapshot| {
        snapshot
            .servers
            .iter()
            .any(|known| known.id == guild && known.banner.is_some())
    })
    .await;

    post_control(server, "/_control/guild-reset", "");
    post_control(server, "/_control/members-reset", "");
    snapshot_where(discord, |snapshot| {
        snapshot
            .servers
            .iter()
            .any(|known| known.id == guild && known.name == "Server")
    })
    .await;
}

/// Making, changing, moving and deleting a role in "Server"'s Roles as
/// Discord's client does, below Staff, her highest; part of
/// [`talks_to_the_fake_server`], whose session it shares. It puts the roles
/// and members back after.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn changes_roles_as_discords_client_does() {
    let fixture = Fixture::start("changes_roles_as_discords_client_does").await;
    let discord = &fixture.discord;
    let server = fixture.server;
    let guild = fixture.guild.as_str();
    use serde_json::json;
    let (staff, leads, crew, bots, mods) = (
        "1400000000000000114",
        "1400000000000000113",
        "1400000000000000112",
        "1400000000000000111",
        "1400000000000000110",
    );
    let (alice, dave) = ("1400000000000000001", "1400000000000000004");
    let names = |roles: &[yapfast_discord::server_settings::Role]| -> Vec<String> {
        roles.iter().map(|role| role.name.clone()).collect()
    };
    let page = discord.roles_page(guild).await.unwrap();
    assert_eq!(
        names(&page.roles),
        ["Leads", "Staff", "Crew", "Bots", "Mods", "@everyone"]
    );
    assert_eq!(page.counts.get(staff), Some(&1));

    // A new role starts at the bottom, above @everyone.
    let mut live = discord.live();
    let body = json!({"name": "new role", "color": 0,
        "colors": {"primary_color": 0, "secondary_color": null, "tertiary_color": null},
        "permissions": "0"});
    let made = discord.create_role(guild, &body).await.unwrap();
    live_where(&mut live, |live| match live {
        Live::ServerRole { role, .. } if role.id == made => Some(()),
        _ => None,
    })
    .await;

    // Saving moves it first, then changes it whole, permissions as a
    // decimal string; a permission she doesn't hold is refused.
    let positions = [
        (made.clone(), 3),
        (bots.to_owned(), 2),
        (mods.to_owned(), 1),
    ];
    let role = json!({"name": "Helpers", "permissions": "1024", "color": 3447003,
        "colors": {"primary_color": 3447003, "secondary_color": null, "tertiary_color": null},
        "hoist": true, "mentionable": false, "unicode_emoji": null});
    discord
        .save_roles(guild, &positions, Some((&made, &role)))
        .await
        .unwrap();
    let page = discord.roles_page(guild).await.unwrap();
    assert_eq!(
        names(&page.roles),
        [
            "Leads",
            "Staff",
            "Crew",
            "Helpers",
            "Bots",
            "Mods",
            "@everyone"
        ]
    );
    let helpers = page.roles.iter().find(|known| known.id == made).unwrap();
    assert_eq!(
        (helpers.permissions, helpers.color, helpers.hoist),
        (1024, 3447003, true)
    );
    let admin = json!({"permissions": "8"});
    assert!(
        discord
            .save_roles(guild, &[], Some((&made, &admin)))
            .await
            .is_err()
    );
    // Leads, above her, stays put; listing it where it is goes.
    assert!(
        discord
            .save_roles(guild, &[(leads.to_owned(), 1)], None)
            .await
            .is_err()
    );
    discord
        .save_roles(guild, &[(leads.to_owned(), 6), (crew.to_owned(), 4)], None)
        .await
        .unwrap();

    // Members: Staff's are hers to see, Helpers' to change.
    assert_eq!(discord.role_members(guild, staff).await.unwrap(), [alice]);
    discord
        .add_role_members(guild, &made, &[dave.to_owned()])
        .await
        .unwrap();
    assert_eq!(discord.role_members(guild, &made).await.unwrap(), [dave]);
    assert_eq!(
        discord.role_member_counts(guild).await.unwrap().get(&made),
        Some(&1)
    );
    discord
        .remove_role_member(guild, &made, dave)
        .await
        .unwrap();
    assert!(discord.role_members(guild, &made).await.unwrap().is_empty());

    discord.delete_role(guild, &made).await.unwrap();
    live_where(&mut live, |live| match live {
        Live::ServerRoleDeleted { role, .. } if role == made => Some(()),
        _ => None,
    })
    .await;
    post_control(server, "/_control/roles-reset", "");
    post_control(server, "/_control/members-reset", "");
}

/// Changing #general's overwrites as Channel Permissions does: one at a
/// time, taking one off, and syncing with the category; part of
/// [`talks_to_the_fake_server`], whose session it shares. It puts the
/// channels' overwrites back after.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn changes_channel_permissions_as_discords_client_does() {
    let fixture = Fixture::start("changes_channel_permissions_as_discords_client_does").await;
    let discord = &fixture.discord;
    let server = fixture.server;
    let guild = fixture.guild.as_str();
    use yapfast_discord::ready::{Overwrite, permission};
    let (general, crew) = ("1400000000000000102", "1400000000000000112");
    let overwrites = |known: &yapfast_discord::service::Snapshot| -> Option<Vec<Overwrite>> {
        let server = known.servers.iter().find(|server| server.id == guild)?;
        Some(server.channel(general)?.overwrites.to_vec())
    };
    let allowed = Overwrite {
        id: crew.into(),
        member: false,
        allow: permission::VIEW_CHANNEL,
        deny: permission::SEND_MESSAGES,
    };
    discord
        .put_overwrites(general, std::slice::from_ref(&allowed))
        .await
        .unwrap();
    snapshot_where(discord, |known| {
        overwrites(known) == Some(vec![allowed.clone()])
    })
    .await;

    // Only what she holds herself, as Discord checks.
    let webhooks = Overwrite {
        allow: permission::MANAGE_WEBHOOKS,
        ..allowed.clone()
    };
    assert!(discord.put_overwrites(general, &[webhooks]).await.is_err());

    discord.delete_overwrite(general, crew).await.unwrap();
    snapshot_where(discord, |known| overwrites(known) == Some(vec![])).await;
    let everyone = Overwrite {
        id: guild.into(),
        member: false,
        allow: 0,
        deny: 0,
    };
    discord
        .sync_overwrites(general, std::slice::from_ref(&everyone))
        .await
        .unwrap();
    snapshot_where(discord, |known| {
        overwrites(known) == Some(vec![everyone.clone()])
    })
    .await;

    // An empty body takes every channel's overwrites off.
    post_control(server, "/_control/hide-from-bots", "");
    snapshot_where(discord, |known| overwrites(known) == Some(vec![])).await;
}

/// Safety Setup and Bans in Server Settings, as Discord's client uses
/// them; part of [`talks_to_the_fake_server`], whose session it shares.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn moderates_as_discords_client_does() {
    let fixture = Fixture::start("moderates_as_discords_client_does").await;
    let discord = &fixture.discord;
    let server = fixture.server;
    let guild = fixture.guild.as_str();
    use serde_json::json;
    use yapfast_discord::mfa::Method;
    // Safety Setup's levels, saved with the guild's PATCH.
    let settings = discord
        .save_guild_settings(
            guild,
            &json!({"verification_level": 3, "explicit_content_filter": 1}),
        )
        .await
        .unwrap();
    assert_eq!(
        (
            settings.verification_level,
            settings.explicit_content_filter
        ),
        (3, 1)
    );
    post_control(server, "/_control/guild", r#"{"features": ["COMMUNITY"]}"#);
    assert!(
        discord
            .save_guild_settings(guild, &json!({"verification_level": 0}))
            .await
            .is_err(),
        "a Community server can't have none"
    );

    // Only the owner, with 2FA on, has moderators need it, and confirms.
    assert!(discord.set_mfa_level(guild, 1).await.is_err());
    post_control(
        server,
        "/_control/guild",
        r#"{"owner": "1400000000000000001", "alice_mfa": true, "features": []}"#,
    );
    snapshot_where(discord, |snapshot| {
        snapshot
            .account
            .as_ref()
            .is_some_and(|account| account.mfa_enabled)
    })
    .await;
    let mut live = discord.live();
    let set = tokio::spawn({
        let (discord, guild) = (discord.clone(), guild.to_owned());
        async move {
            discord
                .stepping_up(|| discord.set_mfa_level(&guild, 1))
                .await
        }
    });
    let id = live_where(&mut live, |live| match live {
        Live::MfaRequired { id, .. } => Some(id),
        _ => None,
    })
    .await;
    discord
        .finish_mfa(id, Method::Totp, "123456")
        .await
        .unwrap();
    assert_eq!(set.await.unwrap().unwrap().mfa_level, 1);
    post_control(server, "/_control/guild-reset", "");
    snapshot_where(discord, |snapshot| {
        snapshot
            .account
            .as_ref()
            .is_some_and(|account| !account.mfa_enabled)
    })
    .await;

    // Bans, a page at a time by id, and Discord's search by name or id.
    let (mallory, trent) = ("1400000000000000901", "1400000000000000902");
    let bans = discord.bans(guild, None).await.unwrap();
    let listed: Vec<(&str, &str, Option<&str>)> = bans
        .iter()
        .map(|ban| {
            (
                ban.user.id.as_str(),
                ban.user.username.as_str(),
                ban.reason.as_deref(),
            )
        })
        .collect();
    assert_eq!(
        listed,
        [
            (mallory, "mallory", Some("Spamming invites in #general")),
            (trent, "trent_42", None)
        ]
    );
    let after = discord.bans(guild, Some(mallory)).await.unwrap();
    assert_eq!(after.len(), 1);
    let found = discord.search_bans(guild, "MAL").await.unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].user.global_name.as_deref(), Some("Mallory"));
    let found = discord
        .search_bans(guild, &format!("nobody, {trent}"))
        .await
        .unwrap();
    assert_eq!(found[0].user.id, trent);

    // Revoke Ban, which GUILD_BAN_REMOVE follows.
    discord.unban(guild, mallory).await.unwrap();
    live_where(&mut live, |live| match live {
        Live::ServerBan {
            ban, banned: false, ..
        } if ban.user.id == mallory => Some(()),
        _ => None,
    })
    .await;
    assert!(
        discord.unban(guild, mallory).await.is_err(),
        "no longer banned"
    );

    // Audit Log, newest first: the unban, then the 2FA requirement and
    // the levels set above.
    let actions = |log: &serde_json::Value| -> Vec<u64> {
        log["audit_log_entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| entry["action_type"].as_u64().unwrap())
            .collect()
    };
    let log = discord.audit_log(guild, None, None, None).await.unwrap();
    let newest = &log["audit_log_entries"][0];
    assert_eq!(
        (newest["action_type"].as_u64(), newest["target_id"].as_str()),
        (Some(23), Some(mallory))
    );
    assert_eq!(newest["user_id"], "1400000000000000001");
    assert_eq!(actions(&log)[1..3], [1, 1]);
    assert_eq!(
        log["audit_log_entries"][1]["changes"][0]["key"],
        "mfa_level"
    );
    // By Bob, and by action.
    let bob = "1400000000000000002";
    let by_bob = discord
        .audit_log(guild, None, Some(bob), None)
        .await
        .unwrap();
    assert_eq!(actions(&by_bob), [24, 24, 22, 22, 13, 1]);
    assert!(
        by_bob["users"]
            .as_array()
            .unwrap()
            .iter()
            .any(|user| user["id"] == bob && user["username"] == "bob")
    );
    let bans = discord
        .audit_log(guild, None, None, Some(22))
        .await
        .unwrap();
    assert!(actions(&bans).iter().all(|action| *action == 22));
    // Fifty at a time, from before the last.
    post_control(server, "/_control/audit-entries", r#"{"count": 60}"#);
    let first = discord
        .audit_log(guild, None, Some(bob), None)
        .await
        .unwrap();
    assert_eq!(actions(&first).len(), 50);
    let last = first["audit_log_entries"][49]["id"].as_str().unwrap();
    let next = discord
        .audit_log(guild, Some(last), Some(bob), None)
        .await
        .unwrap();
    assert_eq!(actions(&next).len(), 16);
    post_control(server, "/_control/audit-reset", "");
    post_control(server, "/_control/members-reset", "");
}

/// Members and Invites in Server Settings, as Discord's client asks for
/// them; part of [`talks_to_the_fake_server`], whose session it shares.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn lists_members_and_invites_as_discords_client_does() {
    let fixture = Fixture::start("lists_members_and_invites_as_discords_client_does").await;
    let discord = &fixture.discord;
    let server = fixture.server;
    let guild = fixture.guild.as_str();
    use serde_json::json;
    use yapfast_discord::api::MembersSearch;
    let search = |body: serde_json::Value| async move {
        match discord.members_search(guild, &body).await.unwrap() {
            MembersSearch::Found(found) => {
                yapfast_discord::server_settings::parse_members_search(&found)
            }
            MembersSearch::Indexing(wait) => panic!("indexing for {wait} s"),
        }
    };
    let names = |found: &[yapfast_discord::server_settings::FoundMember]| {
        found
            .iter()
            .map(|member| member.user.username.clone())
            .collect::<Vec<_>>()
    };
    // Newest first, with how each joined.
    let (found, total) = search(json!({"or_query": {}, "and_query": {}, "limit": 250})).await;
    assert_eq!(names(&found), ["dave", "carol", "alice", "bob"]);
    assert_eq!(total, Some(4));
    assert_eq!(
        (found[1].join_source, found[1].invite_code.as_deref()),
        (5, Some("carol1"))
    );
    assert!(found[1].unusual_dms_until > 0 && found[0].unusual_dms_until == 0);
    // By name or id, and in another order.
    let carol = found[1].user.id.clone();
    let (found, _) = search(json!({"or_query": {}, "and_query": {
        "usernames": {"or_query": ["BO"]}, "user_id": {"or_query": [carol]}}, "limit": 250}))
    .await;
    assert!(found.is_empty(), "both must match, as and_query asks");
    let (found, _) = search(
        json!({"or_query": {}, "and_query": {"usernames": {"or_query": ["bo", "car"]}},
               "limit": 250, "sort": 2}),
    )
    .await;
    assert_eq!(names(&found), ["bob", "carol"]);
    // A chunk at a time, after the last member of the one before.
    post_control(server, "/_control/members", r#"{"count": 260}"#);
    let (first, total) = search(json!({"or_query": {}, "and_query": {}, "limit": 250})).await;
    assert_eq!((first.len(), total), (250, Some(264)));
    let last = first.last().unwrap();
    let (rest, _) = search(json!({"or_query": {}, "and_query": {}, "limit": 250,
        "after": {"guild_joined_at": last.joined_at, "user_id": last.user.id}}))
    .await;
    assert_eq!(
        names(&rest),
        [
            "member_10",
            "member_9",
            "member_8",
            "member_7",
            "member_6",
            "member_5",
            "member_4",
            "member_3",
            "member_2",
            "member_1",
            "dave",
            "carol",
            "alice",
            "bob"
        ]
    );
    // While Discord indexes the server, it asks to wait.
    post_control(
        server,
        "/_control/members",
        r#"{"indexing": true, "count": 0}"#,
    );
    assert!(matches!(
        discord
            .members_search(guild, &json!({"or_query": {}, "and_query": {}, "limit": 250}))
            .await
            .unwrap(),
        MembersSearch::Indexing(wait) if wait > 0.0
    ));

    // Invites, and revoking one.
    let invites = discord.guild_invites(guild).await.unwrap();
    let codes: Vec<&str> = invites.iter().map(|invite| invite.code.as_str()).collect();
    assert!(codes.starts_with(&["bobinvite", "carol1"]), "{codes:?}");
    let carols = &invites[1];
    assert_eq!(
        (carols.uses, carols.max_uses, carols.roles.len()),
        (3, 10, 1)
    );
    assert!(carols.expires.is_some() && invites[0].expires.is_none());
    assert_eq!(carols.channel.as_ref().unwrap().1, "general");
    discord.revoke_invite("carol1").await.unwrap();
    assert!(discord.revoke_invite("carol1").await.is_err());
    assert!(
        !discord
            .guild_invites(guild)
            .await
            .unwrap()
            .iter()
            .any(|invite| invite.code == "carol1")
    );

    // Security Actions: invites paused for two hours, then resumed, which
    // GUILD_UPDATE tells.
    let mut live = discord.live();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let (invites, dms) = discord.pause_server(guild, true, false, 2).await.unwrap();
    assert!(invites.is_some_and(|until| until.abs_diff(now + 7200) < 60));
    assert_eq!(dms, None);
    live_where(&mut live, |live| match live {
        Live::ServerSettings {
            guild: changed,
            settings,
        } if changed == guild && settings.invites_paused_until == invites => Some(()),
        _ => None,
    })
    .await;
    assert_eq!(
        discord.pause_server(guild, false, false, 2).await.unwrap(),
        (None, None)
    );
    post_control(server, "/_control/guild-reset", "");
}

/// Emoji and Stickers in Server Settings: listing them with who uploaded
/// each, adding, renaming and deleting her own, as her Crew role's Create
/// Expressions allows, and not Bob's; part of [`talks_to_the_fake_server`].
#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn keeps_emoji_and_stickers_as_discords_client_does() {
    let fixture = Fixture::start("keeps_emoji_and_stickers_as_discords_client_does").await;
    let discord = &fixture.discord;
    let server = fixture.server;
    let guild = fixture.guild.as_str();
    use base64::Engine;
    let dot = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";
    let png = base64::engine::general_purpose::STANDARD
        .decode(dot)
        .unwrap();
    let code = |error: anyhow::Error| yapfast_discord::api::code(&error);
    let names = |emojis: &[yapfast_discord::server_settings::GuildEmoji]| {
        emojis
            .iter()
            .map(|emoji| emoji.name.clone())
            .collect::<Vec<_>>()
    };
    // Each kind has 50 slots at level 0, as premium_features says.
    let settings = discord.guild_settings(guild).await.unwrap();
    assert_eq!(settings.extra_emoji_slots, Some(0));
    let emojis = discord.guild_emojis(guild).await.unwrap();
    assert_eq!(names(&emojis), ["talk", "party"]);
    assert!(emojis[1].animated);
    let bobs = emojis[0].clone();
    assert_eq!(bobs.user.as_ref().unwrap().username, "bob");

    // Hers, which GUILD_EMOJIS_UPDATE gives everyone.
    let wave = discord
        .create_emoji(guild, "wave", &format!("data:image/png;base64,{dot}"))
        .await
        .unwrap();
    assert_eq!(wave.user.as_ref().unwrap().username, "alice");
    let has_emoji = |name: &'static str| {
        move |snapshot: &Snapshot| {
            snapshot.servers.iter().any(|known| {
                known.id == guild && known.emojis.iter().any(|emoji| emoji.name == name)
            })
        }
    };
    snapshot_where(discord, has_emoji("wave")).await;
    discord
        .rename_emoji(guild, &wave.id, "hello")
        .await
        .unwrap();
    snapshot_where(discord, has_emoji("hello")).await;
    assert_eq!(
        code(
            discord
                .rename_emoji(guild, &bobs.id, "mine")
                .await
                .unwrap_err()
        ),
        Some(50013),
        "Bob's isn't hers to change"
    );
    let big = format!("data:image/png;base64,{}", "A".repeat(400 * 1024));
    assert_eq!(
        code(discord.create_emoji(guild, "big", &big).await.unwrap_err()),
        Some(50045)
    );
    discord.delete_emoji(guild, &wave.id).await.unwrap();
    assert_eq!(
        names(&discord.guild_emojis(guild).await.unwrap()),
        ["talk", "party"]
    );

    // Stickers: five slots at level 0, each new one sent as a form.
    let mut live = discord.live();
    fn fields(name: &str) -> [(&str, &str); 3] {
        [("name", name), ("tags", "👋"), ("description", "")]
    }
    discord
        .create_sticker(guild, fields("Wave"), ("wave.png", "image/png", &png))
        .await
        .unwrap();
    let stickers = live_where(&mut live, |live| match live {
        Live::ServerStickers {
            guild: changed,
            stickers,
        } if changed == guild && !stickers.is_empty() => Some(stickers),
        _ => None,
    })
    .await;
    // Only the list says who uploaded it.
    assert_eq!(
        (stickers[0].name.as_str(), stickers[0].user_id.as_deref()),
        ("Wave", None)
    );
    let listed = discord.guild_stickers(guild).await.unwrap();
    assert_eq!(listed[0].user.as_ref().unwrap().username, "alice");
    assert_eq!((listed[0].format, listed[0].tags.as_str()), (1, "👋"));
    let id = listed[0].id.clone();
    discord
        .edit_sticker(guild, &id, "Hello", "👋", "A hand waving")
        .await
        .unwrap();
    assert!(
        discord
            .edit_sticker(guild, &id, "Hello", "👋", "x")
            .await
            .is_err(),
        "a description of one character"
    );
    let listed = discord.guild_stickers(guild).await.unwrap();
    assert_eq!(
        (listed[0].name.as_str(), listed[0].description.as_str()),
        ("Hello", "A hand waving")
    );
    for n in 1..5 {
        let name = format!("Sticker {n}");
        discord
            .create_sticker(guild, fields(&name), ("s.png", "image/png", &png))
            .await
            .unwrap();
    }
    assert_eq!(
        code(
            discord
                .create_sticker(guild, fields("Sixth"), ("s.png", "image/png", &png))
                .await
                .unwrap_err()
        ),
        Some(30039)
    );
    discord.delete_sticker(guild, &id).await.unwrap();
    assert_eq!(discord.guild_stickers(guild).await.unwrap().len(), 4);
    post_control(server, "/_control/guild-reset", "");
}

/// Transfer Ownership and Delete Server, as the server's owner: by the code
/// Discord emails without 2FA, or by confirming with it; part of
/// [`talks_to_the_fake_server`].
#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn gives_up_the_server_as_discords_client_does() {
    let fixture = Fixture::start("gives_up_the_server_as_discords_client_does").await;
    let discord = &fixture.discord;
    let server = fixture.server;
    let guild = fixture.guild.as_str();
    use yapfast_discord::mfa::Method;
    let (alice, bob) = ("1400000000000000001", "1400000000000000002");
    let code = |error: anyhow::Error| yapfast_discord::api::code(&error);
    let owner = |id: &'static str| {
        move |snapshot: &Snapshot| {
            snapshot
                .servers
                .iter()
                .any(|known| known.id == guild && known.owner_id.as_deref() == Some(id))
        }
    };
    // Bob's, so not hers to give or delete.
    assert_eq!(
        code(discord.delete_guild(guild).await.unwrap_err()),
        Some(50013)
    );
    post_control(
        server,
        "/_control/guild",
        &format!(r#"{{"owner": "{alice}"}}"#),
    );
    snapshot_where(discord, owner(alice)).await;

    // Without 2FA, the code Discord emails gives it away.
    let account = snapshot_where(discord, |snapshot| snapshot.account.is_some()).await;
    assert!(account.account.as_ref().unwrap().has_email);
    assert_eq!(
        code(discord.transfer_guild(guild, bob, None).await.unwrap_err()),
        Some(50035)
    );
    discord.send_guild_pincode(guild).await.unwrap();
    assert!(
        discord
            .transfer_guild(guild, bob, Some("000000"))
            .await
            .is_err()
    );
    discord
        .transfer_guild(guild, bob, Some("424242"))
        .await
        .unwrap();
    snapshot_where(discord, owner(bob)).await;

    // With 2FA, confirming it does; then deleting the server takes it away.
    post_control(
        server,
        "/_control/guild",
        &format!(r#"{{"owner": "{alice}", "alice_mfa": true}}"#),
    );
    snapshot_where(discord, owner(alice)).await;
    let mut live = discord.live();
    let confirm = |id| discord.finish_mfa(id, Method::Totp, "123456");
    let transfer = tokio::spawn({
        let (discord, guild) = (discord.clone(), guild.to_owned());
        async move {
            discord
                .stepping_up(|| discord.transfer_guild(&guild, bob, None))
                .await
        }
    });
    let id = live_where(&mut live, |live| match live {
        Live::MfaRequired { id, .. } => Some(id),
        _ => None,
    })
    .await;
    confirm(id).await.unwrap();
    transfer.await.unwrap().unwrap();
    snapshot_where(discord, owner(bob)).await;
    post_control(
        server,
        "/_control/guild",
        &format!(r#"{{"owner": "{alice}"}}"#),
    );
    snapshot_where(discord, owner(alice)).await;
    let delete = tokio::spawn({
        let (discord, guild) = (discord.clone(), guild.to_owned());
        async move { discord.stepping_up(|| discord.delete_guild(&guild)).await }
    });
    let id = live_where(&mut live, |live| match live {
        Live::MfaRequired { id, .. } => Some(id),
        _ => None,
    })
    .await;
    confirm(id).await.unwrap();
    delete.await.unwrap().unwrap();
    snapshot_where(discord, |snapshot| {
        !snapshot.servers.iter().any(|known| known.id == guild)
    })
    .await;
    post_control(server, "/_control/rejoin", "");
    snapshot_where(discord, |snapshot| {
        snapshot.servers.iter().any(|known| known.id == guild)
    })
    .await;
    post_control(server, "/_control/guild-reset", "");
    snapshot_where(discord, owner(bob)).await;
}

/// Editing her main profile as Discord's client does, then putting it
/// back; part of [`talks_to_the_fake_server`], whose session it shares.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn edits_the_main_profile_as_discords_client_does() {
    let fixture = Fixture::start("edits_the_main_profile_as_discords_client_does").await;
    let discord = &fixture.discord;
    let server = fixture.server;
    let own = discord.own_profile(None).await.unwrap();
    assert_eq!(
        (own.name.as_str(), own.bio.as_str(), own.accent),
        ("Alice", "", None)
    );
    assert!(own.avatar.is_some());
    assert_eq!(own.fallback.name, "alice", "her username");
    let dot = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";
    discord
        .save_main_profile(
            &serde_json::json!({"global_name": "Ally", "avatar": dot}),
            &serde_json::json!({"bio": "Hi, I'm **Ally**", "pronouns": "she/her", "accent_color": 0x5865f2}),
        )
        .await
        .unwrap();
    // USER_UPDATE renames the account and gives it the new avatar.
    let renamed = |name: &'static str| {
        move |snapshot: &Snapshot| {
            snapshot
                .account
                .as_ref()
                .is_some_and(|account| account.name == name)
        }
    };
    let snapshot = snapshot_where(discord, renamed("Ally")).await;
    assert!(!snapshot.account.as_ref().unwrap().avatar.contains("alice1"));
    let own = discord.own_profile(None).await.unwrap();
    assert_eq!(own.name, "Ally");
    assert_eq!(own.bio, "Hi, I'm **Ally**");
    assert_eq!(own.pronouns, "she/her");
    assert_eq!(own.accent, Some(0x5865f2));
    assert!(
        discord
            .save_main_profile(&serde_json::json!({}), &serde_json::json!({"banner": dot}))
            .await
            .is_err(),
        "a banner takes Nitro"
    );
    // An empty display name goes by her username; no avatar, Discord's
    // default.
    discord
        .save_main_profile(
            &serde_json::json!({"global_name": "", "avatar": null}),
            &serde_json::json!({"accent_color": null}),
        )
        .await
        .unwrap();
    let snapshot = snapshot_where(discord, renamed("alice")).await;
    assert!(
        snapshot
            .account
            .as_ref()
            .unwrap()
            .avatar
            .contains("/embed/avatars/")
    );
    let own = discord.own_profile(None).await.unwrap();
    assert_eq!(
        (own.name.as_str(), own.avatar, own.accent),
        ("", None, None)
    );
    post_control(server, "/_control/profile-reset", "");
    snapshot_where(discord, renamed("Alice")).await;
}

/// What linking showed next.
async fn next_step(shown: &mut tokio::sync::mpsc::UnboundedReceiver<Step>) -> Step {
    timeout(WAIT, shown.recv())
        .await
        .expect("timed out linking")
        .expect("linking stopped")
}

/// Joining a thread from its menu, then leaving it, as Discord's client
/// does; part of [`talks_to_the_fake_server`], whose session it shares.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn joins_and_leaves_threads_as_discords_client_does() {
    let fixture = Fixture::start("joins_and_leaves_threads_as_discords_client_does").await;
    let discord = &fixture.discord;
    let server = fixture.server;
    let book_club = |snapshot: &Snapshot| {
        snapshot.servers[0]
            .threads
            .iter()
            .find(|thread| thread.name == "Book club")
            .and_then(|thread| Some((thread.id.clone(), thread.thread.as_deref()?.joined)))
    };
    // Viewing the server subscribes to its threads.
    discord.view(&fixture.guild, Some(&fixture.general));
    let (id, joined) =
        book_club(&*snapshot_where(discord, |snapshot| book_club(snapshot).is_some()).await)
            .unwrap();
    assert!(!joined);
    discord
        .join_thread(&id, true, "Context Menu")
        .await
        .unwrap();
    snapshot_where(discord, |snapshot| {
        book_club(snapshot).is_some_and(|(_, joined)| joined)
    })
    .await;
    discord
        .join_thread(&id, false, "Toolbar Overflow")
        .await
        .unwrap();
    snapshot_where(discord, |snapshot| {
        book_club(snapshot).is_some_and(|(_, joined)| !joined)
    })
    .await;
    assert_eq!(
        self::control(server, "GET", "/_control/thread-members", ""),
        r#"["POST Context Menu","DELETE Toolbar Overflow"]"#
    );
}

/// A forum's posts and what their cards show, as Discord's client loads
/// them; part of [`talks_to_the_fake_server`], whose session it shares.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn shows_a_forum_as_discords_client_does() {
    let fixture = Fixture::start("shows_a_forum_as_discords_client_does").await;
    let discord = &fixture.discord;
    let server = fixture.server;
    let guild = fixture.guild.as_str();
    use yapfast_discord::forum::Emoji;
    // Viewing the server subscribes to its threads, and so its forum posts.
    discord.view(guild, Some(&fixture.general));
    // The forum lists among the text channels, and its active posts came
    // with the server's threads.
    let snapshot = snapshot_where(discord, |snapshot| {
        snapshot.servers[0]
            .threads
            .iter()
            .any(|thread| thread.name == "Keyboard shortcut list?")
    })
    .await;
    let listed = &snapshot.servers[0];
    let forum = listed
        .text_channels
        .iter()
        .find(|channel| channel.name == "ideas")
        .expect("the forum lists");
    let details = forum.forum.as_deref().expect("as a forum");
    let tags: Vec<_> = details.tags.iter().map(|tag| tag.name.as_str()).collect();
    assert_eq!(tags, ["Feature", "Bug", "Question", "Done"]);
    assert!(details.tags[3].moderated);
    assert_eq!(details.reaction, Some(Emoji::Unicode("👍".into())));
    assert!(!details.require_tag && !details.by_creation);
    let post = |name: &str| {
        listed
            .threads
            .iter()
            .find(|thread| thread.name == name)
            .unwrap_or_else(|| panic!("{name} lists"))
    };
    let details = |name: &str| post(name).thread.as_deref().unwrap();
    let active: Vec<_> = listed
        .threads
        .iter()
        .filter(|thread| thread.parent.as_deref() == Some(forum.id.as_str()))
        .map(|thread| thread.name.as_str())
        .collect();
    assert_eq!(
        active,
        [
            "Keyboard shortcut list?",
            "Crash when pasting a GIF",
            "Dark theme for the sidebar",
            "Welcome to #ideas"
        ]
    );
    assert!(details("Welcome to #ideas").pinned);
    assert_eq!(
        details("Crash when pasting a GIF").tags,
        [details_tag(forum, 1), details_tag(forum, 2)]
    );
    let opened = |name: &str| {
        listed
            .threads()
            .find(|thread| thread.name == name)
            .is_some_and(|thread| thread.marks.opened)
    };
    let dark = details("Dark theme for the sidebar");
    assert!(dark.joined && opened("Dark theme for the sidebar"));
    // Its first message isn't counted.
    assert_eq!(dark.messages, 2);
    assert!(!opened("Keyboard shortcut list?"));

    // Their first messages, as their cards show them.
    let ids: Vec<&str> = active.iter().map(|name| post(name).id.as_str()).collect();
    let starters = discord.posts(guild, &forum.id, &ids).await.unwrap();
    let starter = |name: &str| {
        let id = &post(name).id;
        starters
            .iter()
            .find(|(post, _)| post == id)
            .and_then(|(_, starter)| starter.clone())
            .unwrap_or_else(|| panic!("{name}'s first message"))
    };
    let dark = starter("Dark theme for the sidebar");
    assert_eq!(dark.content, "The sidebar could follow the system theme.");
    assert_eq!(dark.reaction, Some((Emoji::Unicode("👍".into()), 3)));
    assert!(dark.picture.is_none());
    let crash = starter("Crash when pasting a GIF");
    let picture = crash.picture.expect("its screenshot");
    assert!(
        picture.contains("/crash.png?format=webp&width=144&height=108"),
        "{picture}"
    );
    assert!(!crash.spoiler);
    assert!(starter("Keyboard shortcut list?").reaction.is_none());
    assert_eq!(
        self::control(server, "GET", "/_control/post-data", ""),
        format!("[{}]", serde_json::to_string(&ids).unwrap())
    );

    // Older posts come a page at a time, with their first messages.
    let page = discord.threads(guild, &forum.id, None, 0).await.unwrap();
    let older: Vec<_> = page
        .threads
        .iter()
        .map(|thread| thread.name.as_str())
        .collect();
    assert_eq!(older, ["Old idea 1", "Old idea 2", "Old idea 3"]);
    let first = page
        .starters
        .iter()
        .map(|(_, starter)| starter.as_ref().map(|starter| starter.content.as_str()))
        .collect::<Vec<_>>();
    assert_eq!(
        first,
        [
            Some("Idea 1, done long ago."),
            Some("Idea 2, done long ago."),
            Some("Idea 3, done long ago.")
        ]
    );

    // Read, the forum has nothing new until someone posts.
    discord.mark_read(guild, &forum.id, true);
    let unread = |snapshot: &Snapshot| {
        snapshot.servers[0]
            .text_channels()
            .any(|channel| channel.name == "ideas" && channel.marks.unread)
    };
    snapshot_where(discord, |snapshot| !unread(snapshot)).await;
    post_control(server, "/_control/forum-post", "");
    let snapshot = snapshot_where(discord, |snapshot| {
        unread(snapshot)
            && snapshot.servers[0]
                .threads
                .iter()
                .any(|thread| thread.name == "Emoji picker search")
    })
    .await;
    let new = snapshot.servers[0]
        .threads
        .iter()
        .find(|thread| thread.name == "Emoji picker search")
        .unwrap();
    assert_eq!(new.thread.as_deref().unwrap().tags, [details_tag(forum, 0)]);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn posts_in_a_forum_as_discords_client_does() {
    let fixture = Fixture::start("posts_in_a_forum_as_discords_client_does").await;
    let discord = &fixture.discord;
    let server = fixture.server;
    let guild = fixture.guild.as_str();
    let forum = |snapshot: &Snapshot| {
        snapshot.servers[0]
            .text_channels
            .iter()
            .find(|channel| channel.name == "ideas")
            .cloned()
            .unwrap()
    };
    // A forum that requires a tag refuses a post without one.
    post_control(server, "/_control/forum-require-tag", "");
    let snapshot = snapshot_where(discord, |snapshot| {
        forum(snapshot)
            .forum
            .as_deref()
            .is_some_and(|forum| forum.require_tag)
    })
    .await;
    let ideas = forum(&snapshot);
    let refused = discord
        .post_in_forum(guild, &ideas.id, "Untagged", &[], "Hello", vec![])
        .await;
    assert!(refused.is_err_and(|error| error.to_string().contains("tag is required")));

    // A post with a tag, text and a file lists, its first message with
    // them, as Discord sends it.
    let bug = details_tag(&ideas, 1);
    let post = discord
        .post_in_forum(
            guild,
            &ideas.id,
            "Crash when dragging",
            std::slice::from_ref(&bug),
            "It closes.",
            vec![("notes.txt".into(), b"steps".to_vec())],
        )
        .await
        .unwrap();
    let snapshot = snapshot_where(discord, |snapshot| {
        snapshot.servers[0]
            .threads
            .iter()
            .any(|thread| thread.id == post)
    })
    .await;
    let listed = snapshot.servers[0]
        .threads
        .iter()
        .find(|thread| thread.id == post)
        .unwrap();
    assert_eq!(listed.name, "Crash when dragging");
    assert_eq!(listed.parent.as_deref(), Some(ideas.id.as_str()));
    assert_eq!(
        listed.thread.as_deref().unwrap().tags,
        std::slice::from_ref(&bug)
    );
    let first = discord
        .history(guild, &post, Span::Latest)
        .await
        .unwrap()
        .messages;
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].id, post, "the first message has the post's id");
    assert_eq!(first[0].content, "It closes.");
    assert_eq!(first[0].attachments.len(), 1);
    assert_eq!(first[0].attachments[0].name, "notes.txt");
    let creates: serde_json::Value =
        serde_json::from_str(&control(server, "GET", "/_control/thread-creates", "")).unwrap();
    let body = creates.as_array().unwrap().last().unwrap();
    assert_eq!(body["name"], "Crash when dragging");
    assert_eq!(body["applied_tags"], serde_json::json!([bug]));
    assert_eq!(body["auto_archive_duration"], 4320);
    assert_eq!(body["message"]["content"], "It closes.");
    assert_eq!(body["message"]["attachments"][0]["filename"], "notes.txt");
    post_control(server, "/_control/forum-require-tag?off", "");
}

/// The id of a forum's `index`th tag.
fn details_tag(forum: &yapfast_discord::ready::Channel, index: usize) -> String {
    forum.forum.as_deref().unwrap().tags[index].id.clone()
}

/// A channel's list of threads, as Discord's client fills it; part of
/// [`talks_to_the_fake_server`], whose session it shares.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn lists_a_channels_threads_as_discords_client_does() {
    let fixture = Fixture::start("lists_a_channels_threads_as_discords_client_does").await;
    let discord = &fixture.discord;
    let guild = fixture.guild.as_str();
    let general = fixture.general.as_str();
    let (guild, general) = (guild.to_owned(), general.to_owned());
    discord.view(&guild, Some(&general));
    // Viewing the server subscribed to its threads: the active ones came,
    // with their newest messages, but not a private one she isn't in.
    let snapshot = snapshot_where(discord, |snapshot| {
        snapshot.servers[0]
            .threads
            .iter()
            .any(|thread| thread.name == "Book club")
    })
    .await;
    let threads = &snapshot.servers[0].threads;
    let book_club = threads
        .iter()
        .find(|thread| thread.name == "Book club")
        .unwrap();
    let details = book_club.thread.as_deref().unwrap();
    assert!(!details.joined);
    assert_eq!(details.members, 2);
    let recent = details.recent.as_deref().expect("its newest message");
    assert_eq!(recent.content, "Chapter 3 by Friday?");
    assert!(
        threads
            .iter()
            .all(|thread| thread.name != "Surprise for Bob")
    );

    // Archived threads come a page at a time, the newest first.
    let first = discord.threads(&guild, &general, None, 0).await.unwrap();
    assert_eq!(first.threads.len(), 25);
    assert!(first.more);
    assert!(
        first
            .threads
            .iter()
            .all(|thread| thread.thread.as_ref().unwrap().archived)
    );
    assert_eq!(first.threads[0].name, "Game night 1");
    let owner = first.threads[0]
        .thread
        .as_ref()
        .unwrap()
        .owner
        .clone()
        .unwrap();
    assert_eq!(discord.name(&guild, &owner).as_deref(), Some("Bob"));
    let second = discord.threads(&guild, &general, None, 25).await.unwrap();
    assert!(!second.more);
    assert!(second.threads.len() >= 5);
    // A search finds active and archived ones by name.
    let found = discord
        .threads(&guild, &general, Some("night"), 0)
        .await
        .unwrap();
    assert!(found.threads.len() >= 5);
    assert!(
        found
            .threads
            .iter()
            .all(|thread| thread.name.contains("night"))
    );

    // One the list found opens from what Discord gave, and lists meanwhile.
    let id = first.threads[3].id.clone();
    let listed = discord.listed_thread(&id);
    assert!(listed.is_some());
    discord.forget_listed_threads();
    assert!(discord.listed_thread(&id).is_none());
    discord
        .open_thread(&guild, &id, listed, false)
        .await
        .unwrap();
    snapshot_where(discord, |snapshot| {
        snapshot.viewing.contains(&id)
            && snapshot.servers[0]
                .threads
                .iter()
                .any(|thread| thread.id == id)
    })
    .await;
}

/// The GIF picker's searches, its pictures from the provider, the
/// favourites in the second settings, which another client's change
/// updates live, and one sent by a direct link to its file. Part of
/// [`talks_to_the_fake_server`].
#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn finds_gifs_as_discords_client_does() {
    let fixture = Fixture::start("finds_gifs_as_discords_client_does").await;
    let discord = &fixture.discord;
    let server = fixture.server;
    let guild = fixture.guild.as_str();
    let general = fixture.general.as_str();
    let trending = discord.gifs_trending().await.unwrap();
    assert_eq!(trending.categories[0].name, "happy");
    assert!(trending.preview.is_some());
    let found = discord.gifs_search("dance").await.unwrap();
    let names: Vec<&str> = found.iter().map(|gif| gif.url.as_str()).collect();
    assert_eq!(
        names,
        [
            "https://klipy.com/gifs/dance-9102",
            "https://klipy.com/gifs/party-9104",
            "https://klipy.com/gifs/spin-9109"
        ]
    );
    assert!(discord.gifs_search("nothing").await.unwrap().is_empty());
    assert_eq!(discord.gifs_trending_gifs().await.unwrap().len(), 10);
    assert_eq!(discord.gifs_suggest("dance", 5).await.unwrap().len(), 5);
    // The picture, which the picker animates.
    let picture = discord.download(&found[0].src, 1 << 20).await.unwrap();
    assert_eq!(&picture[8..12], b"WEBP");
    discord
        .gif_select(found[0].id.as_deref().unwrap(), "dance")
        .await
        .unwrap();
    assert!(
        control(server, "GET", "/_control/gif-selects", "").contains(r#""id":"9102","q":"dance""#)
    );

    // The same settings count the places Alice used, which rank where the
    // Forward To dialog lists them: Weekend plans and #general.
    let places = discord.places_used().await.unwrap();
    let uses: Vec<_> = places
        .iter()
        .map(|(id, uses)| (id.as_str(), uses.total, uses.recent.len()))
        .collect();
    assert_eq!(
        uses,
        [
            ("1400000000000000121", 12, 3),
            ("1400000000000000102", 3, 2)
        ]
    );
    assert!(places[0].1.recent.is_sorted());

    // Two favourites, kept by another client: one by its page, one by a
    // direct link to its file.
    let mut favorites = discord.favorite_gifs().await.unwrap();
    assert_eq!(favorites.len(), 2);
    assert!(favorites[0].src.ends_with("/_gifs/hello.gif"));
    let linked = favorites[1].url.clone();
    assert_eq!(linked, "https://static.klipy.com/ii/wave.webp");
    let mut live = discord.live();

    // Sent, the link unfurls into an image embed whose picture moves and
    // keeps the link, which Discord's client stars it by.
    let nonce = yapfast_discord::message::nonce();
    let sent = discord
        .send(guild, general, &linked, sending(&nonce, None), vec![])
        .await
        .unwrap();
    let unfurled = live_where(&mut live, |event| match event {
        Live::Updated(message) if message.id == sent.id && !message.attachments.is_empty() => {
            Some(message)
        }
        _ => None,
    })
    .await;
    assert!(unfurled.content.is_empty());
    let picture = &unfurled.attachments[0];
    assert!(picture.animated && picture.image);
    assert_eq!(picture.link_url.as_deref(), Some(linked.as_str()));
    let data = discord.download(&picture.url, 1 << 20).await.unwrap();
    assert_eq!(&data[8..12], b"WEBP");
    discord.delete(general, &sent.id).await.unwrap();

    // Adding one keeps them all, and every session hears of it.
    favorites.push(yapfast_discord::gifs::Favorite {
        url: found[0].url.clone(),
        src: found[0].src.clone(),
        video: false,
        width: found[0].width,
        height: found[0].height,
        order: 3,
    });
    discord.set_favorite_gifs(&favorites).await.unwrap();
    let heard = live_where(&mut live, |event| match event {
        Live::FavoriteGifs(heard) => Some(heard),
        _ => None,
    })
    .await;
    assert_eq!(heard, favorites);
    assert_eq!(discord.favorite_gifs().await.unwrap(), favorites);
}

/// Who reacted to a message, a page at a time, as a reaction's tooltip and
/// the Reactions window ask; and a moderator removing someone's reaction.
/// Part of [`talks_to_the_fake_server`], whose session it shares.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn lists_who_reacted_as_discords_client_does() {
    let fixture = Fixture::start("lists_who_reacted_as_discords_client_does").await;
    let discord = &fixture.discord;
    let server = fixture.server;
    let guild = fixture.guild.as_str();
    let general = fixture.general.as_str();
    let latest = discord.history(guild, general, Span::Latest).await.unwrap();
    let newest = latest
        .messages
        .iter()
        .find(|message| {
            message
                .reactions
                .iter()
                .any(|reaction| reaction.key == "🎉")
        })
        .expect("the fixture's newest message, with reactions");
    let id = newest.id.clone();
    let names = |people: &[yapfast_discord::service::Reactor]| -> Vec<(String, String, bool)> {
        people
            .iter()
            .map(|person| (person.name.clone(), person.username.clone(), person.me))
            .collect()
    };
    let first = discord
        .reactors(guild, general, &id, "🎉", None, 2)
        .await
        .unwrap();
    assert_eq!(
        names(&first),
        [
            ("Alice".into(), "alice".into(), true),
            ("Bob".into(), "bob".into(), false)
        ]
    );
    let rest = discord
        .reactors(guild, general, &id, "🎉", Some(&first[1].id), 100)
        .await
        .unwrap();
    assert_eq!(
        rest.iter()
            .map(|person| person.name.as_str())
            .collect::<Vec<_>>(),
        ["Carol", "Dave"]
    );
    assert!(!rest[0].avatar.is_empty());
    let pages = self::control(server, "GET", "/_control/reactor-pages", "");
    assert!(pages.contains("limit=2&type=0"), "{pages}");
    assert!(pages.contains(&format!("after={}", first[1].id)), "{pages}");

    // A moderator removes Dave's.
    discord
        .remove_reaction(general, &id, "🎉", &rest[1].id)
        .await
        .unwrap();
    let removed = self::control(server, "GET", "/_control/reactor-removals", "");
    assert!(removed.contains(&format!("🎉 {}", rest[1].id)), "{removed}");
}

/// A thread this account is in goes quiet its archive duration after its
/// last activity, without Discord saying so; part of
/// [`talks_to_the_fake_server`], whose session it shares.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn marks_quiet_threads_as_discords_client_does() {
    let fixture = Fixture::start("marks_quiet_threads_as_discords_client_does").await;
    let discord = &fixture.discord;
    let server = fixture.server;
    let plans_quiet = |snapshot: &Snapshot| {
        snapshot.servers[0]
            .threads()
            .find(|thread| thread.name == "Plans")
            .map(|thread| thread.marks.quiet)
    };
    // Opened again as the fake started, it isn't quiet yet.
    assert_eq!(plans_quiet(&discord.snapshots().borrow()), Some(false));
    // Once its time passes, a snapshot marks it quiet with nothing else
    // happening.
    post_control(server, "/_control/thread-quiet?in=1500", "");
    let updated = snapshot_where(discord, |snapshot| {
        snapshot.servers[0].threads.iter().any(|thread| {
            thread
                .thread
                .as_deref()
                .is_some_and(|details| details.archive_after == 60)
        })
    })
    .await;
    assert_eq!(plans_quiet(&updated), Some(false));
    snapshot_where(discord, |snapshot| plans_quiet(snapshot) == Some(true)).await;
    post_control(server, "/_control/thread-quiet?reset", "");
    snapshot_where(discord, |snapshot| plans_quiet(snapshot) == Some(false)).await;
}

/// Threads Alice starts, as Discord's client starts them; part of
/// [`talks_to_the_fake_server`], whose session it shares.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn starts_threads_as_discords_client_does() {
    let fixture = Fixture::start("starts_threads_as_discords_client_does").await;
    let discord = &fixture.discord;
    let server = fixture.server;
    let guild = fixture.guild.as_str();
    let general = fixture.general.as_str();
    let latest = discord
        .history(guild, general, Span::Latest)
        .await
        .unwrap()
        .messages;
    let message = latest.last().unwrap().id.clone();
    // From a message: the thread takes its id, and she's in it.
    let thread = discord
        .start_thread(guild, general, Some(&message), "Snacks", false, "Message")
        .await
        .unwrap();
    assert_eq!(thread, message);
    snapshot_where(discord, |snapshot| {
        snapshot.servers[0].threads.iter().any(|listed| {
            listed.id == thread
                && listed.name == "Snacks"
                && listed.thread.as_ref().is_some_and(|thread| thread.joined)
        })
    })
    .await;
    // A message has one thread at most, and a name has 1 to 100 letters.
    assert!(
        discord
            .start_thread(guild, general, Some(&message), "Again", false, "Message")
            .await
            .is_err()
    );
    assert!(
        discord
            .start_thread(guild, general, None, &"a".repeat(101), false, "Plus Button")
            .await
            .is_err()
    );
    // Without a message, private as she chose.
    let private = discord
        .start_thread(
            guild,
            general,
            None,
            "Surprise",
            true,
            "Thread Browser Toolbar",
        )
        .await
        .unwrap();
    snapshot_where(discord, |snapshot| {
        snapshot.servers[0].threads.iter().any(|listed| {
            listed.id == private && listed.thread.as_ref().is_some_and(|thread| thread.private)
        })
    })
    .await;
    let creates: serde_json::Value =
        serde_json::from_str(&control(server, "GET", "/_control/thread-creates", "")).unwrap();
    let creates = creates.as_array().unwrap();
    assert_eq!(
        creates[0],
        serde_json::json!({
            "channel": general,
            "message": message,
            "name": "Snacks",
            "type": 11,
            "auto_archive_duration": 4320,
            "location": "Message",
        })
    );
    assert_eq!(
        creates.last().unwrap(),
        &serde_json::json!({
            "channel": general,
            "name": "Surprise",
            "type": 12,
            "auto_archive_duration": 4320,
            "location": "Thread Browser Toolbar",
        })
    );
}

/// A plain send, as the composer makes it, with `nonce` and `reply`.
fn sending<'a>(nonce: &'a str, reply: Option<&'a str>) -> yapfast_discord::api::Sending<'a> {
    yapfast_discord::api::Sending {
        nonce,
        reply,
        ping_reply: true,
        silent: false,
    }
}
