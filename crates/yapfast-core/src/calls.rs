//! MatrixRTC and native media dependencies stay behind this module.

use crate::session::SessionLease;
use anyhow::{Context, Result, bail, ensure};
use futures_util::StreamExt;
use livekit::{
    RoomEvent,
    options::TrackPublishOptions,
    prelude::{LocalAudioTrack, LocalTrack, RemoteTrack, RtcAudioSource, TrackSource},
    webrtc::{
        audio_source::{AudioSourceOptions, native::NativeAudioSource},
        audio_stream::native::NativeAudioStream,
        prelude::AudioFrame,
    },
};
use matrix_rtc_livekit::{Call, CallOptions};
use matrix_sdk::Room;
use serde::Serialize;
use std::{borrow::Cow, collections::BTreeMap, time::Duration};

#[derive(Serialize)]
pub struct ExperimentReport {
    pub protocol: &'static str,
    pub synthetic_audio_both_directions: bool,
    pub receiver_without_correct_key_cannot_decode: bool,
    pub receiver_recovers_after_key_restore: bool,
    pub third_participant_joins_and_leaves: bool,
    pub key_rotates_after_departure: bool,
    pub obsolete_key_cannot_decode_after_rotation: bool,
    pub departed_participant_receives_no_new_key: bool,
}

/// Dedicated thread and LocalSet keep the library's non-Send command sender local.
/// Matrix clients and Yapfast's report are the only types crossing this boundary.
pub(crate) async fn run_local_experiment(
    sessions: Vec<SessionLease>,
    room_id: matrix_sdk::ruma::OwnedRoomId,
) -> Result<ExperimentReport> {
    let (mut sender, receiver) = tokio::sync::oneshot::channel();
    std::thread::Builder::new()
        .name("yapfast-call".into())
        .spawn(move || {
            let result = (|| {
                let rooms = sessions
                    .iter()
                    .map(|session| {
                        session
                            .client
                            .get_room(&room_id)
                            .context("account has not synced the call room")
                    })
                    .collect::<Result<Vec<_>>>()?;
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()?;
                let local = tokio::task::LocalSet::new();
                runtime.block_on(local.run_until(experiment(rooms, &mut sender)))
            })();
            drop(sessions);
            let _ = sender.send(result);
        })
        .context("could not start the native call thread")?;
    receiver
        .await
        .context("the native call thread stopped without a result")?
}

async fn experiment(
    rooms: Vec<Room>,
    consumer: &mut tokio::sync::oneshot::Sender<Result<ExperimentReport>>,
) -> Result<ExperimentReport> {
    ensure!(
        rooms.len() == 3,
        "the local experiment needs three accounts"
    );
    for room in &rooms {
        require_trusted_encrypted_room(room).await?;
    }
    matrix_rtc_livekit::open_slot(
        &rooms[0].client(),
        rooms[0].room_id().as_str(),
        "m.call#ROOM",
        "m.call",
        Some(matrix_rtc_core::SlotEncryption {
            encryption_type: "m.per_member".into(),
            extra: BTreeMap::new(),
        }),
    )
    .await
    .map_err(|_| anyhow::anyhow!("could not open an encrypted RTC slot"))?;
    // The membership bridge fetches slot state; SessionSync owns the live sync loop.
    let mut calls = Vec::new();
    let result = tokio::select! {
        result = tokio::time::timeout(Duration::from_secs(120), experiment_audio(&rooms, &mut calls)) => result.context("native audio experiment exceeded its run deadline").and_then(|result| result),
        _ = consumer.closed() => Err(anyhow::anyhow!("the call experiment was cancelled")),
    };
    let mut cleanup_failed = false;
    while let Some(call) = calls.pop() {
        let leave = tokio::time::timeout(Duration::from_secs(10), call.leave()).await;
        cleanup_failed |= !matches!(leave, Ok(Ok(())));
    }
    if cleanup_failed && result.is_ok() {
        bail!("audio checks passed but normal call departure failed")
    }
    result
}

pub(crate) async fn require_trusted_encrypted_room(room: &Room) -> Result<()> {
    ensure!(
        room.latest_encryption_state().await?.is_encrypted(),
        "RatFast calls require an encrypted room; upgrade encryption explicitly first"
    );
    let client = room.client();
    ensure!(
        client.supports_sticky_events().await?,
        "This homeserver doesn't support sticky events (MSC4354), which Yapfast voice channels need. Ask its administrator to enable them (Synapse: experimental_features.msc4354_enabled), or use another homeserver."
    );
    let user = client.user_id().context("sign in before joining voice")?;
    let device = client.device_id().context("sign in before joining voice")?;
    let own = client.encryption().get_device(user, device).await?;
    ensure!(
        own.is_some_and(|device| device.is_verified_with_cross_signing()),
        "recover or verify this device before joining encrypted voice"
    );
    Ok(())
}

fn local_options() -> CallOptions {
    CallOptions {
        livekit_service_url_fallback: Some("http://localhost:16080".into()),
        ..Default::default()
    }
}

async fn experiment_audio(rooms: &[Room], calls: &mut Vec<Call>) -> Result<ExperimentReport> {
    calls.push(Call::join(&rooms[0], local_options()).await.map_err(|_| {
        anyhow::anyhow!("first RTC join failed; check sticky events, slot state and authorization")
    })?);
    println!("Call experiment: first participant waiting alone.");
    tokio::time::sleep(Duration::from_secs(3)).await;
    calls.push(
        Call::join(&rooms[1], local_options())
            .await
            .map_err(|_| anyhow::anyhow!("second RTC join failed"))?,
    );
    wait_for_keys(&calls[0], &calls[1]).await?;
    let alice_tone = publish_tone(&calls[0], 440.0).await?;
    let bob_tone = publish_tone(&calls[1], 660.0).await?;
    let mut alice_receives = subscribed_audio(&mut calls[0]).await?;
    let mut bob_receives = subscribed_audio(&mut calls[1]).await?;
    let (alice_signal, bob_signal) = tokio::try_join!(
        tone_energy(&mut alice_receives, 660.0),
        tone_energy(&mut bob_receives, 440.0)
    )?;
    ensure!(
        alice_signal > 100.0 && bob_signal > 100.0,
        "encrypted audio did not decode in both directions"
    );
    ensure!(
        calls
            .iter()
            .all(|call| call.session().room().e2ee_manager().enabled()),
        "frame encryption was not enabled"
    );
    println!("PASS: native synthetic audio decodes in both directions with GCM frame encryption.");

    // Negative control: replace Bob's Alice key, leaving the media path running.
    let alice_identity = calls[0].local_identity().to_owned().into();
    let manager = calls[1].session().room().e2ee_manager();
    let provider = manager
        .key_provider()
        .context("no frame encryption key provider")?;
    let cryptors = manager.frame_cryptors();
    let cryptor = cryptors
        .values()
        .find(|cryptor| cryptor.participant_id() == calls[0].local_identity())
        .context("no receive frame cryptor for Alice")?;
    ensure!(
        cryptor.enabled(),
        "the receiver's frame cryptor is disabled"
    );
    let index = cryptor.key_index();
    let correct_key = provider
        .get_key(&alice_identity, index)
        .context("no received media key")?;
    ensure!(
        provider.set_key(&alice_identity, index, vec![0x5a; 32]),
        "could not install the negative-control key"
    );
    drain_audio(&mut bob_receives, Duration::from_secs(2)).await;
    let wrong_key_signal = tone_energy(&mut bob_receives, 440.0).await?;
    ensure!(
        wrong_key_signal < bob_signal * 0.05,
        "audio still decoded with an incorrect media key"
    );
    ensure!(
        provider.set_key(&alice_identity, index, correct_key),
        "could not restore the received media key"
    );
    drain_audio(&mut bob_receives, Duration::from_millis(500)).await;
    ensure!(
        tone_energy(&mut bob_receives, 440.0).await? > bob_signal * 0.25,
        "audio did not recover after restoring its media key"
    );
    println!("PASS: an incorrect receiver key blocks decoding; restoring the key restores audio.");

    calls.push(
        Call::join(&rooms[2], local_options())
            .await
            .map_err(|_| anyhow::anyhow!("third RTC join failed"))?,
    );
    wait_for_keys(&calls[0], &calls[2]).await?;
    wait_for_members(calls, 3).await?;
    let third = calls.pop().unwrap();
    let sender_cryptor = sender_cryptor(&calls[0])?;
    let old_index = sender_cryptor.key_index();
    let departed_provider = third
        .session()
        .room()
        .e2ee_manager()
        .key_provider()
        .context("third participant has no key provider")?;
    let old_key = departed_provider
        .get_key(&alice_identity, old_index)
        .context("third participant did not receive the current key")?;
    third
        .leave()
        .await
        .map_err(|_| anyhow::anyhow!("third participant could not leave cleanly"))?;
    wait_for_members(calls, 2).await?;
    println!("PASS: the third participant appears while muted and leaves cleanly.");
    tokio::time::timeout(Duration::from_secs(25), async {
        // An arrival rotation may already be scheduled. Wait for a departure
        // key the former participant never held, beyond that earlier switch.
        while sender_cryptor.key_index() == old_index
            || departed_provider
                .get_key(&alice_identity, sender_cryptor.key_index())
                .is_some()
        {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .context("the sender did not rotate its key after a participant left")?;
    let new_index = sender_cryptor.key_index();
    ensure!(
        departed_provider
            .get_key(&alice_identity, new_index)
            .is_none(),
        "the departed participant received the new media key"
    );
    let current_key = provider
        .get_key(&alice_identity, new_index)
        .context("the remaining participant did not receive the new media key")?;
    // Bob stands in for a departed receiver holding only the obsolete key.
    ensure!(
        provider.set_key(&alice_identity, new_index, old_key),
        "could not install the obsolete-key control"
    );
    drain_audio(&mut bob_receives, Duration::from_secs(2)).await;
    ensure!(
        tone_energy(&mut bob_receives, 440.0).await? < bob_signal * 0.05,
        "an obsolete media key decoded audio after rotation"
    );
    ensure!(
        provider.set_key(&alice_identity, new_index, current_key),
        "could not restore the rotated media key"
    );
    drain_audio(&mut bob_receives, Duration::from_millis(500)).await;
    ensure!(
        tone_energy(&mut bob_receives, 440.0).await? > bob_signal * 0.25,
        "the remaining participant could not decode after key rotation"
    );
    println!(
        "PASS: departure rotates the sender key; the departed device gets no new key and its old key cannot decode subsequent audio."
    );
    drop(alice_tone);
    drop(bob_tone);
    Ok(ExperimentReport {
        protocol: "MSC4354 sticky membership, MSC4195 get_token, m.per_member GCM",
        synthetic_audio_both_directions: true,
        receiver_without_correct_key_cannot_decode: true,
        receiver_recovers_after_key_restore: true,
        third_participant_joins_and_leaves: true,
        key_rotates_after_departure: true,
        obsolete_key_cannot_decode_after_rotation: true,
        departed_participant_receives_no_new_key: true,
    })
}

async fn wait_for_keys(first: &Call, second: &Call) -> Result<()> {
    tokio::time::timeout(Duration::from_secs(25), async {
        loop {
            if first.imported_key_for(second.local_identity())
                && second.imported_key_for(first.local_identity())
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .context(
        "encrypted media keys did not reach both participants; check device trust and Matrix sync",
    )
}

fn sender_cryptor(call: &Call) -> Result<livekit::webrtc::native::frame_cryptor::FrameCryptor> {
    call.session()
        .room()
        .e2ee_manager()
        .frame_cryptors()
        .into_values()
        .find(|cryptor| cryptor.participant_id() == call.local_identity())
        .context("the publisher has no frame cryptor")
}

async fn wait_for_members(calls: &[Call], count: usize) -> Result<()> {
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let mut matches = true;
            for call in calls {
                matches &= call.member_count().await == count;
            }
            if matches {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .context("call membership did not settle after join or leave")
}

pub(crate) struct Tone(tokio::task::JoinHandle<()>);
impl Drop for Tone {
    fn drop(&mut self) {
        self.0.abort();
    }
}

pub(crate) async fn publish_tone(call: &Call, frequency: f64) -> Result<Tone> {
    publish_tone_while(call, frequency, true, || true).await
}

/// Publishes the tone only while `speaking()` holds and digital silence otherwise,
/// like a voice-activation gate. `dtx` is LiveKit's Opus silence-suppression request.
pub(crate) async fn publish_tone_while(
    call: &Call,
    frequency: f64,
    dtx: bool,
    speaking: impl Fn() -> bool + Send + 'static,
) -> Result<Tone> {
    let source = NativeAudioSource::new(AudioSourceOptions::default(), 48_000, 1, 50);
    let track = LocalAudioTrack::create_audio_track(
        "RatFast synthetic test",
        RtcAudioSource::Native(source.clone()),
    );
    call.session()
        .room()
        .local_participant()
        .publish_track(
            LocalTrack::Audio(track),
            TrackPublishOptions {
                source: TrackSource::Microphone,
                dtx,
                ..Default::default()
            },
        )
        .await?;
    Ok(Tone(tokio::spawn(async move {
        let mut phase: f64 = 0.0;
        let step = std::f64::consts::TAU * frequency / 48_000.0;
        let mut tick = tokio::time::interval(Duration::from_millis(10));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tick.tick().await;
            let samples = if speaking() {
                (0..480)
                    .map(|_| {
                        let sample = (phase.sin() * 8_000.0) as i16;
                        phase = (phase + step) % std::f64::consts::TAU;
                        sample
                    })
                    .collect()
            } else {
                vec![0; 480]
            };
            if source
                .capture_frame(&AudioFrame {
                    data: Cow::Owned(samples),
                    sample_rate: 48_000,
                    num_channels: 1,
                    samples_per_channel: 480,
                })
                .await
                .is_err()
            {
                break;
            }
        }
    })))
}

async fn subscribed_audio(call: &mut Call) -> Result<NativeAudioStream> {
    tokio::time::timeout(Duration::from_secs(15), async {
        while let Some(event) = call.events().recv().await {
            if let RoomEvent::TrackSubscribed {
                track: RemoteTrack::Audio(track),
                ..
            } = event
            {
                return Ok(NativeAudioStream::new(track.rtc_track(), 48_000, 1));
            }
        }
        bail!("the media connection ended before receiving audio")
    })
    .await
    .context("the peer's audio track was not subscribed")?
}

async fn drain_audio(stream: &mut NativeAudioStream, duration: Duration) {
    let _ = tokio::time::timeout(duration, async { while stream.next().await.is_some() {} }).await;
}

async fn tone_energy(stream: &mut NativeAudioStream, frequency: f64) -> Result<f64> {
    let mut samples = 0;
    let mut real = 0.0;
    let mut imaginary = 0.0;
    let collection = tokio::time::timeout(Duration::from_secs(3), async {
        while let Some(frame) = stream.next().await {
            for value in frame.data.iter() {
                let phase = std::f64::consts::TAU * frequency * samples as f64 / 48_000.0;
                real += f64::from(*value) * phase.cos();
                imaginary += f64::from(*value) * phase.sin();
                samples += 1;
            }
            if samples >= 48_000 {
                return true;
            }
        }
        false
    })
    .await;
    ensure!(
        matches!(collection, Ok(true)),
        "audio frames stopped arriving during the decoding check"
    );
    Ok(real.hypot(imaginary) * 2.0 / samples as f64)
}

pub fn check_native_build() {
    // Exercise both crates' public APIs so the native stack is linked in the smoke test.
    let _ = matrix_rtc_livekit::CallOptions::default();
    let _ = livekit::RoomOptions::default();
    // Allocate a synthetic source to verify the C++ bindings without microphone access.
    let _ = livekit::webrtc::audio_source::native::NativeAudioSource::new(
        livekit::webrtc::audio_source::AudioSourceOptions::default(),
        48_000,
        1,
        10,
    );
}
