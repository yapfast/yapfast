//! Calls: voice, cameras and Go Live.

use super::*;

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
    // The fake reports every 50 packets, and the share is read from two
    // reports in a row while it's lost; `say` pushes a frame a little less
    // often than the call ticks, and a tick without a whole frame sends
    // nothing, so this leaves a margin.
    post_control(server, "/_control/voice-link?loss=10", "");
    track.play().unwrap();
    say(&microphone, 160).await;
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

/// Address discovery's UDP asks can be lost, on a lossy link or behind a
/// NAT: a call and a stream ask again until the voice server answers, and a
/// stream whose server never answers gives up after five asks, five seconds.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn asks_again_when_address_discovery_is_lost() {
    let Fixture {
        _fake,
        discord,
        server,
        guild,
        lounge,
        bob,
        ..
    } = Fixture::start("asks_again_when_address_discovery_is_lost").await;
    let asks = || -> serde_json::Value {
        serde_json::from_str(&control(server, "GET", "/_control/discovery-drop", "")).unwrap()
    };
    let assigned = discord.join(&guild, &lounge).await.unwrap();
    post_control(server, "/_control/discovery-drop?count=2", "");
    let _call = timeout(
        WAIT,
        Call::connect(&assigned, &guild, &lounge, Arc::new(Quiet)),
    )
    .await
    .expect("timed out connecting to voice")
    .unwrap();
    assert_eq!(asks(), serde_json::json!({"dropped": 2, "answered": 1}));

    // Watching Bob, the stream's first three asks are lost.
    let watched = discord
        .watch(&stream::key(&guild, &lounge, &bob))
        .await
        .unwrap();
    post_control(server, "/_control/discovery-drop?count=3", "");
    let decodes = vec![Codec::H264, Codec::Vp8];
    let again = (watched.clone(), decodes.clone());
    tokio::task::spawn_blocking(move || watch_frames(&again.0, again.1, 3))
        .await
        .unwrap();
    assert_eq!(asks(), serde_json::json!({"dropped": 5, "answered": 2}));

    // A voice server that never answers.
    post_control(server, "/_control/discovery-drop?count=100", "");
    let started = std::time::Instant::now();
    let receiver = StreamReceiver::start(watched, decodes).unwrap();
    let reason = loop {
        assert!(started.elapsed() < WAIT, "{:?}", receiver.link());
        if let Link::Ended(reason) = receiver.link() {
            break reason;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    let took = started.elapsed();
    assert_eq!(
        reason,
        "the voice server didn't answer address discovery, asked 5 times over 5 s"
    );
    assert!(
        (Duration::from_secs(5)..Duration::from_secs(7)).contains(&took),
        "{took:?}"
    );
    assert_eq!(asks(), serde_json::json!({"dropped": 10, "answered": 2}));
}

/// A call's audio, as [`Hearer`] hears it, and why its connection dropped.
#[derive(Default)]
struct Dropping {
    heard: Hearer,
    dropped: std::sync::Mutex<Vec<String>>,
}

impl Listener for Dropping {
    fn tick(&self, voices: &[(u64, &[i16])]) {
        self.heard.tick(voices);
    }
    fn disconnected(&self, reason: &str) {
        self.dropped.lock().unwrap().push(reason.to_owned());
    }
}

/// Joins the Lounge, connecting a call that reports to `listener`.
async fn join_lounge(
    fixture: &Fixture,
    listener: Arc<Dropping>,
) -> (yapfast_discord::service::VoiceServer, Call) {
    let assigned = fixture
        .discord
        .join(&fixture.guild, &fixture.lounge)
        .await
        .unwrap();
    let call = timeout(
        WAIT,
        Call::connect(&assigned, &fixture.guild, &fixture.lounge, listener),
    )
    .await
    .expect("timed out connecting to voice")
    .unwrap();
    (assigned, call)
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn a_call_follows_a_move_to_another_voice_server() {
    let fixture = Fixture::start("a_call_follows_a_move_to_another_voice_server").await;
    let (discord, server, guild) = (&fixture.discord, fixture.server, &fixture.guild);
    let listener = Arc::new(Dropping::default());
    let (assigned, mut call) = join_lounge(&fixture, listener.clone()).await;
    let mut live = discord.live();
    // Someone drags Alice to Create Room, on another voice server: her
    // voice state says so, the old server closes the call with 4014, and
    // the new server is named just after, as in a user's log.
    let to = discord
        .snapshots()
        .borrow()
        .servers
        .iter()
        .flat_map(|server| &server.voice_channels)
        .find(|found| found.name == "Create Room")
        .map(|found| found.id.clone())
        .unwrap();
    post_control(server, &format!("/_control/voice-move?channel={to}"), "");
    let channel = live_where(&mut live, |event| match event {
        Live::VoiceChannel {
            guild: moved,
            channel,
        } if moved == *guild => Some(channel),
        _ => None,
    })
    .await;
    assert_eq!(channel, to);
    let moved = live_where(&mut live, |event| match event {
        Live::VoiceMoved {
            guild: moved,
            server,
        } if moved == *guild => Some(server),
        _ => None,
    })
    .await;
    assert_eq!(moved.session_id, assigned.session_id);
    until("the old connection closed", || {
        !listener.dropped.lock().unwrap().is_empty()
    })
    .await;
    assert!(listener.dropped.lock().unwrap()[0].contains("4014"));
    // The call waited, and carries on there: Bob's tone comes through.
    timeout(WAIT, call.move_to(&moved, &to))
        .await
        .expect("timed out moving the call")
        .unwrap();
    post_control(server, "/_control/bob-speak", "");
    let bob: u64 = fixture.bob.parse().unwrap();
    until("Bob's tone was heard", || {
        listener
            .heard
            .0
            .lock()
            .unwrap()
            .get(&bob)
            .copied()
            .unwrap_or(0)
            > 3_000
    })
    .await;
    assert_eq!(listener.dropped.lock().unwrap().len(), 1);
    call.leave();
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn a_call_disconnected_from_voice_ends_after_the_grace() {
    let fixture = Fixture::start("a_call_disconnected_from_voice_ends_after_the_grace").await;
    let listener = Arc::new(Dropping::default());
    let (assigned, mut call) = join_lounge(&fixture, listener.clone()).await;
    // Someone disconnects Alice from voice: 4014, and no new server.
    post_control(fixture.server, "/_control/voice-kick", "");
    until("the connection closed", || {
        !listener.dropped.lock().unwrap().is_empty()
    })
    .await;
    tokio::time::sleep(yapfast_discord::voice::MOVE_GRACE + Duration::from_millis(500)).await;
    let late = call.move_to(&assigned, &fixture.lounge).await.unwrap_err();
    assert_eq!(late.to_string(), "the call has ended");
    assert_eq!(listener.dropped.lock().unwrap().len(), 1);
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
