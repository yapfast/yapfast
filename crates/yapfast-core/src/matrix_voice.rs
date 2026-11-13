//! Matrix voice: a MatrixRTC call over LiveKit, from joining until leaving.
use std::{
    collections::{HashMap, HashSet},
    path::Path,
    time::Duration,
};

use anyhow::{Context, Result, bail, ensure};
use livekit::{
    PlatformAudio, RoomEvent,
    options::TrackPublishOptions,
    prelude::{LocalAudioTrack, RemoteTrack, TrackSource},
    rtc_engine::lk_runtime::LkRuntime,
    webrtc::peer_connection_factory::native::PeerConnectionFactoryExt,
};
use matrix_rtc_livekit::{Call, CallOptions, compat::ElementCallCompat};
use matrix_rtc_media::{CallEvent, MediaStreamKind};
use matrix_sdk::{
    Client, Room,
    ruma::{OwnedRoomId, api::client::state::get_state_event_for_key},
};
use tokio::{
    sync::{mpsc, watch},
    time::Instant,
};

use crate::{
    calls::require_trusted_encrypted_room,
    camera::{self, Opening},
    matrix_video::{Filming, Watching},
    session::SessionLease,
    voice::{
        CameraPhase, Chosen, Command, Participant, Phase, State, audio, collect_devices, device_id,
        devices,
    },
};

/// The MatrixRTC slot every Yapfast voice channel uses.
const SLOT: &str = "m.call#ROOM";
/// How long a membership survives without refreshes, for example after a
/// crash. MSC4354 does not allow less than five minutes.
const STICKY_DURATION_MS: u64 = 300_000;

/// A Matrix call in a voice channel, from joining until leaving.
pub(crate) async fn call(
    session: &SessionLease,
    id: String,
    state: &mut State,
    commands: &mut mpsc::UnboundedReceiver<Command>,
    snapshots: &watch::Sender<State>,
    remembered: Option<&Path>,
) -> Result<()> {
    match joining(session, id, state, commands, snapshots).await? {
        Some((call, audio)) => {
            connected(
                call,
                audio,
                &session.client,
                state,
                commands,
                snapshots,
                remembered,
            )
            .await
        }
        None => Ok(()),
    }
}

/// Removes memberships this device left in voice channels, for example after
/// Yapfast was killed.
pub(crate) async fn leave_stale(session: &SessionLease) {
    for room in session
        .client
        .joined_rooms()
        .into_iter()
        .filter(|room| room.is_call())
    {
        leave_stale_memberships(&session.client, &room).await;
    }
}

/// Commands remain responsive while signalling and transport connection are
/// pending. Cancellation is polled through membership cleanup, never dropped.
async fn joining(
    session: &SessionLease,
    id: String,
    state: &mut State,
    commands: &mut mpsc::UnboundedReceiver<Command>,
    snapshots: &watch::Sender<State>,
) -> Result<Option<(Call, PlatformAudio)>> {
    let id = OwnedRoomId::try_from(id)?;
    let room = session
        .client
        .get_room(&id)
        .context("Join this channel in a Matrix client first.")?;
    state.name = room
        .cached_display_name()
        .map(|name| name.to_string())
        .unwrap_or_else(|| id.to_string());
    snapshots.send_replace(state.clone());
    let (cancel, mut cancellation) = watch::channel(false);
    let result = async {
        // Before membership exists, cancellation can discard a slow room check.
        tokio::select! {
            biased;
            _ = cancellation.changed() => return Ok(None),
            result = require_voice_channel(&room) => result?,
        }
        let audio = audio()?;
        let options = call_options(&room)?;
        // A membership left by a crash would otherwise appear beside the new one.
        leave_stale_memberships(&session.client, &room).await;
        let call = Call::join_cancellable(&room, options, cancellation)
            .await
            .map_err(join_error)?;
        let Some(call) = call else {
            return Ok(None);
        };
        if !call.session().room().e2ee_manager().enabled() {
            let _ = tokio::time::timeout(Duration::from_secs(10), call.leave()).await;
            bail!("Voice encryption is unavailable. This connection was refused.");
        }
        Ok::<_, anyhow::Error>(Some((call, audio)))
    };
    tokio::pin!(result);
    let deadline = tokio::time::sleep(Duration::from_secs(35));
    tokio::pin!(deadline);
    let mut cancelled = false;
    loop {
        tokio::select! {
            biased;
            command = commands.recv(), if !cancelled => {
                match command {
                    None | Some(Command::Leave) => {
                        cancelled = true;
                        state.phase = Phase::Leaving;
                        cancel.send_replace(true);
                    }
                    // Saved once the call connects.
                    Some(Command::Microphone(id)) => state.microphone = Some(id),
                    Some(Command::Speaker(id)) => state.speaker = Some(id),
                    Some(Command::Camera(id)) => state.camera = Some(id),
                    Some(Command::Devices) => { let _ = devices(state); }
                    _ => {}
                }
                snapshots.send_replace(state.clone());
            }
            _ = &mut deadline, if !cancelled => {
                cancelled = true;
                state.phase = Phase::Leaving;
                state.error = Some("Voice joining timed out. Check the calling service and retry.".into());
                cancel.send_replace(true);
                snapshots.send_replace(state.clone());
            }
            result = &mut result => {
                // A connection may finish in the same turn as Leave. Never
                // publish Connected for a join the user has cancelled.
                if cancelled {
                    return match result {
                        Ok(Some((call, _audio))) => {
                            tokio::time::timeout(Duration::from_secs(10), call.leave()).await
                                .context("Voice departure timed out. Check the connection before retrying.")?
                                .map_err(|_| anyhow::anyhow!("Could not finish leaving voice. Check the connection before retrying."))?;
                            Ok(None)
                        }
                        other => other,
                    };
                }
                return result;
            }
        }
    }
}

/// The library reports join failures as text; keep the cause users can act on.
fn join_error(error: matrix_rtc_livekit::CallError) -> anyhow::Error {
    let text = error.to_string();
    let message = if text.contains("no livekit transport") {
        "This homeserver doesn't advertise a MatrixRTC calling service (MSC4143). Ask its administrator to add one, or set YAPFAST_RTC_AUTHORIZATION_URL."
    } else if matches!(error, matrix_rtc_livekit::CallError::Media(_)) {
        "The calling service refused the voice connection. Check that it allows this homeserver, then retry."
    } else {
        "Could not join or finish leaving voice. Check the calling service and connection, then retry."
    };
    tracing::warn!(kind = call_error_kind(&error), "voice join failed");
    anyhow::anyhow!(message)
}

fn call_error_kind(error: &matrix_rtc_livekit::CallError) -> &'static str {
    match error {
        matrix_rtc_livekit::CallError::Signalling(_) => "signalling",
        matrix_rtc_livekit::CallError::Media(_) => "media",
        _ => "other",
    }
}

pub(crate) async fn require_voice_channel(room: &Room) -> Result<()> {
    ensure!(
        room.state() == matrix_sdk::RoomState::Joined,
        "Join this channel in a Matrix client first."
    );
    ensure!(room.is_call(), "Select a voice channel to join voice.");
    require_trusted_encrypted_room(room).await?;
    // Read authoritative slot content before joining. An encrypted Matrix room
    // alone does not require the RTC slot to encrypt its media.
    let slot = room
        .client()
        .send(get_state_event_for_key::v3::Request::new(
            room.room_id().to_owned(),
            "m.rtc.slot".into(),
            SLOT.into(),
        ))
        .await
        .map_err(|_| {
            anyhow::anyhow!("This voice channel has no available call. Ask its owner to set it up.")
        })?;
    let content: serde_json::Value = serde_json::from_str(slot.event_or_content.get())?;
    ensure!(
        content
            .get("encryption")
            .and_then(|value| value.get("type"))
            .and_then(|value| value.as_str())
            == Some("m.per_member"),
        "This voice channel does not require encrypted audio. Ask its owner to enable it."
    );
    Ok(())
}

fn call_options(room: &Room) -> Result<CallOptions> {
    let configured = std::env::var("YAPFAST_RTC_AUTHORIZATION_URL").ok();
    let fallback = configured.or_else(|| {
        (room.client().homeserver().as_str() == "http://localhost:18008/")
            .then(|| "http://localhost:16080".into())
    });
    if let Some(url) = &fallback {
        let url = url::Url::parse(url)
            .context("Invalid calling-service URL. Check Yapfast's configuration.")?;
        let local = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
        ensure!(
            url.scheme() == "https" || (url.scheme() == "http" && local),
            "The calling-service URL needs HTTPS."
        );
        ensure!(
            url.username().is_empty() && url.password().is_none(),
            "The calling-service URL must not contain credentials."
        );
    }
    Ok(CallOptions {
        livekit_service_url_fallback: fallback,
        sticky_duration_ms: Some(STICKY_DURATION_MS),
        ..Default::default()
    })
}

/// Chooses a microphone by Yapfast's device id, reopening capture if it was running.
fn use_microphone(audio: &PlatformAudio, id: &str) -> Result<()> {
    let index = audio
        .recording_devices()
        .find(|device| device_id(device.id.as_str(), &device.name) == id)
        .and_then(|device| u16::try_from(device.index).ok())
        .context("Selected microphone is unavailable. Choose another device.")?;
    let runtime = LkRuntime::instance();
    let factory = runtime.pc_factory();
    let running = factory.recording_is_initialized();
    ensure!(
        (!running || factory.stop_recording())
            && factory.set_recording_device(index)
            && (!running || (factory.init_recording() && factory.start_recording())),
        "Could not switch microphone. Refresh devices and choose another."
    );
    Ok(())
}

/// Chooses speakers by Yapfast's device id, reopening playout if it was running.
fn use_speakers(audio: &PlatformAudio, id: &str) -> Result<()> {
    let index = audio
        .playout_devices()
        .find(|device| device_id(device.id.as_str(), &device.name) == id)
        .and_then(|device| u16::try_from(device.index).ok())
        .context("Selected speakers are unavailable. Choose another device.")?;
    let runtime = LkRuntime::instance();
    let factory = runtime.pc_factory();
    let running = factory.playout_is_initialized();
    ensure!(
        (!running || factory.stop_playout())
            && factory.set_playout_device(index)
            && (!running || (factory.init_playout() && factory.start_playout())),
        "Could not switch speakers. Refresh devices and choose another."
    );
    Ok(())
}

async fn connected(
    mut call: Call,
    audio: PlatformAudio,
    client: &Client,
    state: &mut State,
    commands: &mut mpsc::UnboundedReceiver<Command>,
    snapshots: &watch::Sender<State>,
    remembered: Option<&Path>,
) -> Result<()> {
    let result = async {
        collect_devices(&audio, state);
        state.cameras = crate::camera::list();
        ensure!(!state.speakers.is_empty(), "No speakers found. Connect an audio output and retry.");
        // A remembered device may be unplugged; the system default stands in
        // for it, and the choice is kept for when it returns.
        if let Some(id) = &state.microphone && let Err(error) = use_microphone(&audio, id) {
            tracing::warn!(%error, "using the default microphone");
        }
        if let Some(id) = &state.speaker && let Err(error) = use_speakers(&audio, id) {
            tracing::warn!(%error, "using the default speakers");
        }
        Chosen::write(remembered, state);
        let mut microphone: Option<LocalAudioTrack> = None;
        let mut roster = call.subscribe_participants();
        let mut events = call.subscribe_call_events();
        let mut speaking = HashSet::new();
        let mut encryption_fault = false;
        let mut reconnect_deadline = None;
        let own_member = call.membership_id().to_owned();
        let identity = matrix_rtc_livekit::identity_mapper(ElementCallCompat::default());
        // Key problems wait out the normal exchange after someone appears.
        let mut seen: HashMap<String, Instant> = HashMap::new();
        // LiveKit lists a participant only while they publish, and Yapfast joins
        // without a microphone track, so only a lost connection is visible.
        let mut published = HashSet::new();
        let mut device_problems: HashMap<String, Option<&'static str>> = HashMap::new();
        let mut undecryptable = HashSet::new();
        // Frames can arrive just before their key; only a lasting gap is a problem.
        let mut missing_since: HashMap<String, Instant> = HashMap::new();
        let mut checks = tokio::time::interval(KEY_CHECK_INTERVAL);
        checks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut volumes = crate::volume::changes();
        let mut talk = crate::push_to_talk::changes();
        // The volume each remote audio track plays at, by track id.
        let mut volumes_set: HashMap<String, f32> = HashMap::new();
        let mut filming: Option<Filming> = None;
        let mut opening: Opening = None;
        let mut watching = Watching::default();
        // The LiveKit identity of each member, to match cameras to people.
        let mut members: HashMap<String, String> = HashMap::new();
        state.can_film = cfg!(any(target_os = "linux", windows));
        state.phase = Phase::Connected;
        // Yapfast joins only encrypted rooms' calls, whose media LiveKit
        // encrypts frame by frame with the call's keys.
        state.end_to_end = true;
        loop {
            // In push-to-talk the microphone's track is off while the key is
            // up: it sends silence, so nobody hears it or sees it speak, and
            // others don't see it muted. Muting turns it off on its own.
            if let Some(track) = &microphone && !state.muted {
                let sending = crate::push_to_talk::sending();
                if track.rtc_track().enabled() != sending {
                    track.rtc_track().set_enabled(sending);
                }
            }
            let people = roster.borrow_and_update().clone();
            let now = Instant::now();
            seen.retain(|member, _| people.iter().any(|person| &person.member_id == member));
            published.retain(|member| people.iter().any(|person| &person.member_id == member));
            device_problems.retain(|member, _| people.iter().any(|person| &person.member_id == member));
            missing_since.retain(|member, _| people.iter().any(|person| &person.member_id == member));
            for person in people.iter().filter(|person| !person.is_local) {
                seen.entry(person.member_id.clone()).or_insert(now);
                if !device_problems.contains_key(&person.member_id) {
                    device_problems.insert(person.member_id.clone(), device_problem(client, person).await);
                }
            }
            let present: HashSet<String> = call.session().room().remote_participants().keys().map(ToString::to_string).collect();
            let known = members.len();
            members = people.iter()
                .filter_map(|person| Some((identity(&person.user_id, person.device_id.as_deref()?, &person.member_id), person.member_id.clone())))
                .collect();
            if members.len() != known {
                watching.constrain_all(&call, &members);
            }
            state.participants = people.iter().map(|person| {
                let sfu_identity = person.device_id.as_deref().map(|device| identity(&person.user_id, device, &person.member_id));
                let present = sfu_identity.as_ref().is_some_and(|id| present.contains(id));
                // Element mutes its camera track rather than unpublishing it.
                let filmed = person.streams.iter().any(|stream| stream.kind == MediaStreamKind::Camera && !stream.muted);
                let video = if person.is_local {
                    filming.as_ref().filter(|_| state.camera_phase != CameraPhase::Off).map(|filming| filming.feed.clone())
                } else {
                    let feed = sfu_identity.as_deref().and_then(|id| watching.feed(id));
                    // A muted camera's tile shows the avatar: its buffers go,
                    // and the server stops sending it.
                    if !filmed && let Some(feed) = feed {
                        feed.show_at(None);
                    }
                    feed.filter(|_| filmed).cloned()
                };
                if present {
                    published.insert(person.member_id.clone());
                }
                let connected = person.is_local || present || !published.contains(&person.member_id);
                let settled = seen.get(&person.member_id).is_some_and(|since| now.duration_since(*since) >= KEY_GRACE);
                let problem = if person.is_local {
                    None
                } else if let Some(Some(problem)) = device_problems.get(&person.member_id).filter(|_| settled) {
                    Some(*problem)
                } else if undecryptable.contains(&person.member_id) {
                    Some("Their audio can't be decrypted. They may need to rejoin.")
                } else if missing_since.get(&person.member_id).is_some_and(|since| now.duration_since(*since) >= KEY_GRACE)
                    || (settled && connected && sfu_identity.is_some_and(|id| !call.imported_key_for(&id))) {
                    Some("Their audio key hasn't arrived, so you can't hear them yet.")
                } else {
                    None
                };
                Participant {
                    id: person.member_id.clone(), user: person.user_id.clone(), person: person.user_id.clone(), local: person.is_local,
                    muted: if person.is_local { state.muted } else { person.streams.iter().filter(|stream| stream.kind == MediaStreamKind::Microphone).all(|stream| stream.muted) },
                    deafened: person.is_local && state.deafened,
                    server_muted: false,
                    server_deafened: false,
                    speaking: speaking.contains(&person.member_id),
                    connected,
                    problem: problem.map(str::to_owned),
                    avatar: None,
                    streaming: false,
                    video,
                }
            }).collect();
            state.participants.sort_by(|a,b| b.local.cmp(&a.local).then_with(|| a.user.cmp(&b.user)).then_with(|| a.id.cmp(&b.id)));
            let users: HashMap<String, String> = people.iter()
                .filter_map(|person| Some((identity(&person.user_id, person.device_id.as_deref()?, &person.member_id), person.user_id.clone())))
                .collect();
            set_volumes(&call, &users, &mut volumes_set);
            state.warning = state.participants.iter().find_map(|person| person.problem.as_ref().map(|problem| format!("{}: {problem}", person.user)));
            snapshots.send_replace(state.clone());
            tokio::select! {
                command = commands.recv() => match command {
                    None | Some(Command::Leave) => break,
                    Some(Command::Join(_)) => state.error = Some("Leave your current voice channel before joining another.".into()),
                    Some(Command::Mute(muted)) => {
                        // Unmuting while deafened means wanting to talk, which needs hearing too.
                        if !muted && state.deafened {
                            set_hearing(&call, true);
                            state.deafened = false;
                        }
                        let result = set_muted(&call, &audio, &mut microphone, muted).await;
                        match result { Ok(()) => { state.muted = muted; state.error = None; }, Err(error) => { state.muted = true; state.error = Some(error.to_string()); } }
                    }
                    Some(Command::Deafen(deafened)) => {
                        if deafened {
                            set_muted(&call, &audio, &mut microphone, true).await?;
                            state.muted = true;
                        }
                        state.deafened = deafened;
                        set_hearing(&call, !deafened);
                    }
                    Some(Command::Devices) => {
                        collect_devices(&audio, state);
                        state.cameras = crate::camera::list();
                    }
                    Some(Command::Microphone(id)) => {
                        match use_microphone(&audio, &id) {
                            Ok(()) => { state.microphone = Some(id); state.error = None; Chosen::write(remembered, state); },
                            Err(_) => {
                                set_muted(&call, &audio, &mut microphone, true).await?;
                                state.muted = true;
                                state.error = Some("Could not switch microphone. Refresh devices and choose another.".into());
                            }
                        }
                    }
                    Some(Command::Speaker(id)) => {
                        match use_speakers(&audio, &id) {
                            Ok(()) => { state.speaker = Some(id); state.error = None; Chosen::write(remembered, state); },
                            Err(_) => state.error = Some("Could not switch speakers. Refresh devices and choose another.".into()),
                        }
                    }
                    Some(Command::Camera(id)) => {
                        if state.camera_phase != CameraPhase::Off && let Some(filming) = &mut filming {
                            opening = Some(Box::pin(filming.open(Some(&id))));
                        }
                        state.camera = Some(id);
                        Chosen::write(remembered, state);
                    }
                    Some(Command::Video(true)) => if state.camera_phase == CameraPhase::Off {
                        let made = match &mut filming {
                            Some(filming) => Ok(filming),
                            None => Filming::new().map(|made| filming.insert(made)),
                        };
                        match made {
                            Ok(filming) => {
                                opening = Some(Box::pin(filming.open(state.camera.as_deref())));
                                state.camera_phase = CameraPhase::Starting;
                                state.error = None;
                            }
                            Err(error) => state.error = Some(error.to_string()),
                        }
                    }
                    Some(Command::Video(false)) => {
                        opening = None;
                        if let Some(filming) = &mut filming {
                            filming.pause();
                            crate::voice::trim_after_call();
                        }
                        state.camera_phase = CameraPhase::Off;
                    }
                    Some(Command::GoLive(_)) => state.error = Some("Screen sharing works in Discord calls for now.".into()),
                    Some(Command::StopLive | Command::SwitchLive | Command::Watch(_) | Command::StopWatching | Command::Soundboard(_) | Command::PreviewSound(_) | Command::FavoriteSound(..) | Command::LeaveStale) => {}
                },
                result = camera::opened(&mut opening) => {
                    opening = None;
                    let result = match (result, filming.as_mut()) {
                        (Ok(camera), Some(filming)) => filming.opened(&call, camera).await,
                        (result, _) => result.map(drop),
                    };
                    match result {
                        Ok(()) => state.camera_phase = CameraPhase::On,
                        Err(error) => {
                            if let Some(filming) = &mut filming {
                                filming.pause();
                            }
                            state.camera_phase = CameraPhase::Off;
                            state.error = Some(error.to_string());
                        }
                    }
                }
                changed = roster.changed() => if changed.is_err() { bail!("Voice ended. Join again to reconnect."); },
                () = crate::push_to_talk::changed(&mut talk) => {}
                // Device trust can change while connected, for example after
                // verifying someone in another client.
                _ = checks.tick() => device_problems.clear(),
                event = events.recv() => match event {
                    Ok(CallEvent::ActiveSpeakers { speakers }) => speaking = speakers.into_iter().map(|speaker| speaker.member_id).collect(),
                    Ok(CallEvent::MediaConnectionState { degraded }) => {
                        state.phase = if degraded { Phase::Reconnecting } else { Phase::Connected };
                        if degraded { reconnect_deadline.get_or_insert(Instant::now() + Duration::from_secs(60)); } else { reconnect_deadline = None; }
                    }
                    Ok(CallEvent::Ended { .. }) => bail!("Voice disconnected. Check your connection and join again."),
                    // Only our own sender failing means our audio is unprotected.
                    Ok(CallEvent::FrameEncryptionState { member_id, state: encryption, .. }) if member_id == own_member => {
                        if encryption.is_failure() {
                            set_muted(&call, &audio, &mut microphone, true).await?;
                            state.muted = true;
                            encryption_fault = true;
                            state.end_to_end = false;
                            state.error = Some("Audio encryption failed. Your microphone is muted. Check device verification or rejoin.".into());
                        } else if encryption_fault {
                            encryption_fault = false;
                            state.end_to_end = true;
                            state.error = None;
                        }
                    }
                    Ok(CallEvent::FrameEncryptionState { member_id, state: encryption, .. }) => {
                        match encryption {
                            matrix_rtc_media::FrameEncryptionState::Ok => {
                                missing_since.remove(&member_id);
                                if undecryptable.remove(&member_id) {
                                    tracing::info!(member = %member_id, "participant audio decrypts again");
                                }
                            }
                            matrix_rtc_media::FrameEncryptionState::MissingKey => {
                                missing_since.entry(member_id).or_insert_with(Instant::now);
                            }
                            _ => {
                                tracing::warn!(member = %member_id, state = ?encryption, "participant audio cannot be decrypted");
                                missing_since.remove(&member_id);
                                undecryptable.insert(member_id);
                            }
                        }
                    }
                    Ok(CallEvent::KeyDiscarded { .. }) => state.error = Some("A participant's audio key was refused. They may need to verify their device.".into()),
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => bail!("Voice ended. Join again to reconnect."),
                    _ => {}
                },
                // A camera's tile came into sight, went, or changed size.
                () = watching.shown.notified() => watching.constrain_all(&call, &members),
                event = call.events().recv() => match event {
                    Some(RoomEvent::TrackSubscribed { track: RemoteTrack::Audio(track), .. }) => {
                        if state.deafened { track.disable(); }
                    },
                    Some(RoomEvent::TrackSubscribed { track: RemoteTrack::Video(track), publication, participant }) => {
                        if publication.source() == TrackSource::Camera {
                            watching.subscribed(participant.identity().to_string(), &track);
                            watching.constrain_all(&call, &members);
                        }
                    }
                    Some(RoomEvent::TrackUnsubscribed { publication, participant, .. }) if publication.source() == TrackSource::Camera => {
                        watching.unsubscribed(participant.identity().as_str());
                    }
                    // A full reconnect republishes our microphone with a new
                    // frame cryptor at key index 0, which late joiners cannot use.
                    Some(RoomEvent::LocalTrackRepublished { .. }) => {
                        tracing::info!("microphone republished after reconnecting; restoring its key index");
                        call.reassert_local_key_index();
                    }
                    Some(RoomEvent::Disconnected { .. }) | None => bail!("Voice disconnected. Check your connection and join again."),
                    _ => {}
                },
                // The next turn plays everyone at their new volumes.
                Ok(()) = volumes.changed() => {}
                _ = reconnect_timeout(reconnect_deadline) => bail!("Voice could not reconnect within 60 seconds. Join again when your connection returns."),
            }
        }
        // Leaving takes the track out of the call; the camera closes here.
        drop(opening);
        drop(filming);
        drop(watching);
        if let Some(track) = microphone.take() {
            track.mute();
            if audio.is_recording_initialized() { let _ = audio.stop_recording(); }
        }
        Ok(())
    }.await;
    state.phase = Phase::Leaving;
    snapshots.send_replace(state.clone());
    let departure = tokio::time::timeout(Duration::from_secs(10), call.leave()).await;
    if result.is_ok() && !matches!(departure, Ok(Ok(()))) {
        bail!(
            "Audio stopped, but departure could not be confirmed. Channel membership will expire."
        );
    }
    result
}

/// How often device trust is checked again while connected.
const KEY_CHECK_INTERVAL: Duration = Duration::from_secs(5);
/// Media keys normally arrive within a few seconds of someone joining.
const KEY_GRACE: Duration = Duration::from_secs(10);

/// Yapfast's media-key policy shares keys only with cross-signed devices, and the
/// SDK reports withheld keys only in its log. Predict that outcome here so the
/// user learns why someone cannot hear them, instead of hearing silence.
async fn device_problem(
    client: &Client,
    person: &matrix_rtc_media::Participant,
) -> Option<&'static str> {
    let Ok(user) = matrix_sdk::ruma::OwnedUserId::try_from(person.user_id.as_str()) else {
        return Some("Unrecognized account; audio keys can't be exchanged.");
    };
    // One verified user with a changed identity blocks every key we send.
    if let Ok(Some(identity)) = client.encryption().get_user_identity(&user).await
        && identity.has_verification_violation()
    {
        return Some(
            "Changed identity since you verified them. Nobody can receive your audio key until you verify them again in a Matrix client.",
        );
    }
    let Some(device) = person.device_id.as_deref() else {
        return Some("Unknown device; audio keys can't be exchanged.");
    };
    match client.encryption().get_device(&user, device.into()).await {
        Ok(Some(device)) if device.is_cross_signed_by_owner() => None,
        Ok(Some(_)) => {
            Some("Unverified device. It can't receive your audio key, and Yapfast refuses its key.")
        }
        _ => Some("Unknown device; your audio key may not reach it."),
    }
}

/// Leave memberships this device published but never removed, for example
/// before a crash. Peers otherwise list them for up to the five-minute sticky
/// lifetime, because the delayed leave does not replace a sticky membership.
async fn leave_stale_memberships(client: &Client, room: &Room) -> usize {
    let (Some(user), Some(device)) = (client.user_id(), client.device_id()) else {
        return 0;
    };
    let stale: Vec<String> = room
        .sticky_events()
        .live()
        .into_iter()
        .filter(|entry| {
            let event_type = entry.key.event_type.to_string();
            (event_type == "m.rtc.member" || event_type == "org.matrix.msc4143.rtc.member")
                && entry
                    .raw()
                    .get_field::<String>("sender")
                    .ok()
                    .flatten()
                    .as_deref()
                    == Some(user.as_str())
                && entry
                    .encryption_info()
                    .and_then(|info| info.sender_device.as_deref())
                    == Some(device)
        })
        .filter_map(|entry| {
            let content = entry
                .raw()
                .get_field::<serde_json::Value>("content")
                .ok()
                .flatten()?;
            (content["slot_id"] == SLOT && content["member"]["membership"] == "join")
                .then(|| content["member"]["id"].as_str().map(str::to_owned))
                .flatten()
        })
        .collect();
    let mut left = 0;
    for member in stale {
        let membership = matrix_rtc_core::OwnMembershipMachine::new(
            std::sync::Arc::new(matrix_rtc_livekit::SdkCommandSender::new(client.clone())),
            room.room_id().to_string(),
            SLOT.into(),
            member,
            "m.call".into(),
            matrix_rtc_core::MembershipTimings {
                sticky_duration_ms: STICKY_DURATION_MS,
                ..Default::default()
            },
        );
        if matches!(
            tokio::time::timeout(Duration::from_secs(10), membership.leave(None)).await,
            Ok(Ok(()))
        ) {
            left += 1;
        }
    }
    if left > 0 {
        tracing::info!(left, "removed this device's stale voice memberships");
    }
    left
}

async fn reconnect_timeout(deadline: Option<Instant>) {
    match deadline {
        Some(deadline) => tokio::time::sleep_until(deadline).await,
        None => std::future::pending().await,
    }
}

/// Plays or silences everyone else in the call.
/// Plays each person's audio at the volume chosen for them, telling WebRTC
/// only of changes. `users` maps LiveKit identities to Matrix user ids.
fn set_volumes(call: &Call, users: &HashMap<String, String>, set: &mut HashMap<String, f32>) {
    let mut open = HashSet::new();
    for (identity, participant) in call.session().room().remote_participants() {
        let Some(user) = users.get(identity.as_str()) else {
            continue;
        };
        let volume = crate::volume::heard(crate::volume::Kind::Voice, user);
        for publication in participant.track_publications().values() {
            if let Some(RemoteTrack::Audio(track)) = publication.track() {
                let id = track.rtc_track().id();
                if set.get(&id) != Some(&volume)
                    && webrtc_sys::audio_track::ffi::set_audio_track_volume(&id, f64::from(volume))
                {
                    set.insert(id.clone(), volume);
                }
                open.insert(id);
            }
        }
    }
    set.retain(|id, _| open.contains(id));
}

fn set_hearing(call: &Call, hearing: bool) {
    for person in call.session().room().remote_participants().values() {
        for publication in person.track_publications().values() {
            if let Some(RemoteTrack::Audio(track)) = publication.track() {
                if hearing {
                    track.enable();
                } else {
                    track.disable();
                }
            }
        }
    }
}

async fn set_muted(
    call: &Call,
    audio: &PlatformAudio,
    microphone: &mut Option<LocalAudioTrack>,
    muted: bool,
) -> Result<()> {
    if muted {
        if let Some(track) = microphone.as_ref() {
            track.mute();
            if audio.is_recording_initialized() {
                audio.stop_recording().context(
                    "Could not stop microphone capture. Leave voice to release the device.",
                )?;
            }
        }
    } else {
        ensure!(
            audio.recording_devices().next().is_some(),
            "No microphone found. Connect a microphone and refresh devices."
        );
        if let Some(track) = microphone.as_ref() {
            audio.start_recording().context(
                "Could not start your microphone. Check permissions or choose another device.",
            )?;
            track.unmute();
        } else {
            let track =
                LocalAudioTrack::create_audio_track("Yapfast microphone", audio.rtc_source());
            let result = tokio::time::timeout(
                Duration::from_secs(10),
                call.publish_audio_track(
                    track.clone(),
                    TrackPublishOptions {
                        source: TrackSource::Microphone,
                        ..Default::default()
                    },
                ),
            )
            .await;
            match result {
                Ok(Ok(_)) => {
                    *microphone = Some(track);
                    if audio.start_recording().is_err() {
                        if let Some(track) = microphone.as_ref() {
                            track.mute();
                        }
                        let _ = audio.stop_recording();
                        bail!(
                            "Could not start your microphone. Check permissions or choose another device."
                        );
                    }
                }
                _ => {
                    track.mute();
                    let _ = audio.stop_recording();
                    bail!(
                        "Could not publish your microphone. Check permissions and connection, then retry."
                    );
                }
            }
        }
    }
    Ok(())
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use crate::{session::Session, voice::Controller};

    async fn wait(controller: &mut Controller, matches: impl Fn(&State) -> bool) -> State {
        tokio::time::timeout(Duration::from_secs(25), async {
            loop {
                let state = controller.states.borrow_and_update().clone();
                if matches(&state) {
                    return state;
                }
                assert!(state.error.is_none(), "voice error: {:?}", state.error);
                controller
                    .states
                    .changed()
                    .await
                    .expect("voice worker stopped");
            }
        })
        .await
        .expect("voice state deadline exceeded")
    }

    async fn output_energy() -> f64 {
        let output = tokio::task::spawn_blocking(|| {
            std::process::Command::new("timeout")
                .args([
                    "3s",
                    "parec",
                    "--device=yapfast_test_output.monitor",
                    "--format=s16le",
                    "--rate=48000",
                    "--channels=1",
                    "--latency-msec=50",
                ])
                .output()
                .expect("parec must be available")
        })
        .await
        .unwrap();
        let samples: Vec<_> = output
            .stdout
            .chunks_exact(2)
            .map(|pair| i16::from_le_bytes([pair[0], pair[1]]) as f64)
            .collect();
        assert!(
            samples.len() > 48_000,
            "virtual speaker monitor produced too few samples"
        );
        let (real, imaginary) =
            samples
                .iter()
                .enumerate()
                .fold((0.0, 0.0), |(real, imaginary), (i, value)| {
                    let phase = std::f64::consts::TAU * 440.0 * i as f64 / 48_000.0;
                    (real + value * phase.cos(), imaginary + value * phase.sin())
                });
        real.hypot(imaginary) * 2.0 / samples.len() as f64
    }

    async fn virtual_devices(controller: &mut Controller) {
        let state = controller.states.borrow().clone();
        let microphone = state
            .microphones
            .iter()
            .find(|device| device.name.contains("Yapfast_test_microphone"))
            .expect("run scripts/test-voice to create the virtual microphone")
            .id
            .clone();
        let speaker = state
            .speakers
            .iter()
            .find(|device| device.name.contains("Yapfast_test_speakers"))
            .expect("run scripts/test-voice to create virtual speakers")
            .id
            .clone();
        controller.command(Command::Microphone(microphone.clone()));
        controller.command(Command::Speaker(speaker.clone()));
        wait(controller, |state| {
            state.microphone.as_ref() == Some(&microphone)
                && state.speaker.as_ref() == Some(&speaker)
        })
        .await;
    }

    struct ReceiverProcess(std::process::Child);

    impl Drop for ReceiverProcess {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    async fn receiver_ack(root: &std::path::Path, command: &str) {
        tokio::time::timeout(Duration::from_secs(25), async {
            loop {
                if std::fs::read_to_string(root.join("receiver-ack"))
                    .ok()
                    .as_deref()
                    == Some(command)
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await
        .expect("receiver did not acknowledge command");
    }

    async fn receiver_command(root: &std::path::Path, command: &str) {
        std::fs::write(root.join("receiver-command"), command).unwrap();
        receiver_ack(root, command).await;
    }

    #[test]
    #[ignore = "requires a homeserver with MatrixRTC and a saved Alice session in the test data's beta folder; joins muted without recording"]
    fn remote_beta_join_muted_and_leave() {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        let runtime = crate::runtime::BackgroundRuntime::new().unwrap();
        runtime.block_on(async {
            let root = crate::test_data_directory().join("beta");
            let rooms: serde_json::Value =
                serde_json::from_slice(&std::fs::read(root.join("rooms.json")).unwrap()).unwrap();
            let alice = Session::restore(root.join("alice")).await.unwrap();
            let sync = alice.start_sync().await.unwrap();
            sync.wait_until_ready().await.unwrap();
            assert!(alice.device_is_trusted().await.unwrap());
            let mut controller = Controller::start(alice.lease()).unwrap();
            assert!(controller.command(Command::Join(
                rooms["voice"].as_str().unwrap().into()
            )));
            let connected = wait(&mut controller, |state| state.phase == Phase::Connected).await;
            assert!(connected.muted);
            assert_eq!(connected.participants.len(), 1);
            tokio::time::sleep(Duration::from_secs(10)).await;
            let waiting = controller.states.borrow().clone();
            assert_eq!(waiting.phase, Phase::Connected);
            assert!(waiting.muted && waiting.error.is_none());
            controller.command(Command::Leave);
            let departed = wait(&mut controller, |state| state.phase == Phase::Disconnected).await;
            assert!(departed.error.is_none());
            controller.stop().await;
            sync.stop().await;
            println!("PASS: native encrypted voice joined muted through public discovery, waited alone for 10 seconds and left cleanly. No microphone was published.");
        });
    }

    #[test]
    #[ignore = "child process of the slow-authorization cancellation test"]
    fn cancel_pending_join() {
        let root = std::path::PathBuf::from(
            std::env::var("YAPFAST_TEST_VOICE_ROOT").expect("test harness only"),
        );
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        let runtime = crate::runtime::BackgroundRuntime::new().unwrap();
        runtime.block_on(async {
            let manifest: serde_json::Value =
                serde_json::from_slice(&std::fs::read(root.join("rooms.json")).unwrap()).unwrap();
            let bob = Session::restore(root.join("bob")).await.unwrap();
            let sync = bob.start_sync().await.unwrap();
            sync.wait_until_ready().await.unwrap();
            let mut controller = Controller::start(bob.lease()).unwrap();
            controller.command(Command::Join(manifest["voice"].as_str().unwrap().into()));
            wait(&mut controller, |state| state.phase == Phase::Joining).await;
            tokio::time::timeout(Duration::from_secs(25), async {
                while std::fs::read_to_string(root.join("receiver-command"))
                    .ok()
                    .as_deref()
                    != Some("cancel")
                {
                    assert_eq!(controller.states.borrow().phase, Phase::Joining);
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
            })
            .await
            .unwrap();
            let start = Instant::now();
            controller.command(Command::Leave);
            tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    let state = controller.states.borrow_and_update().clone();
                    assert_ne!(
                        state.phase,
                        Phase::Connected,
                        "cancelled join became connected"
                    );
                    assert!(
                        state.error.is_none(),
                        "cancellation error: {:?}",
                        state.error
                    );
                    if state.phase == Phase::Disconnected {
                        break;
                    }
                    controller.states.changed().await.unwrap();
                }
            })
            .await
            .expect("Leave waited for stalled authorization");
            std::fs::write(
                root.join("cancel-milliseconds"),
                start.elapsed().as_millis().to_string(),
            )
            .unwrap();
            controller.stop().await;
            sync.stop().await;
            std::fs::write(root.join("receiver-ack"), "cancel").unwrap();
        });
    }

    #[test]
    #[ignore = "child process of the virtual-device integration test"]
    fn native_receiver() {
        let root = std::path::PathBuf::from(
            std::env::var("YAPFAST_TEST_VOICE_ROOT").expect("test harness only"),
        );
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        let runtime = crate::runtime::BackgroundRuntime::new().unwrap();
        runtime.block_on(async {
            let manifest: serde_json::Value =
                serde_json::from_slice(&std::fs::read(root.join("rooms.json")).unwrap()).unwrap();
            let bob = Session::restore(root.join("bob")).await.unwrap();
            let sync = bob.start_sync().await.unwrap();
            sync.wait_until_ready().await.unwrap();
            let mut controller = Controller::start(bob.lease()).unwrap();
            controller.command(Command::Join(manifest["voice"].as_str().unwrap().into()));
            let state = wait(&mut controller, |state| {
                state.phase == Phase::Connected && state.participants.len() == 2
            })
            .await;
            assert!(state.muted);
            virtual_devices(&mut controller).await;
            std::fs::write(root.join("receiver-ack"), "ready").unwrap();
            let mut previous = String::new();
            tokio::time::timeout(Duration::from_secs(90), async {
                loop {
                    let command =
                        std::fs::read_to_string(root.join("receiver-command")).unwrap_or_default();
                    if command != previous {
                        match command.as_str() {
                            "deafen" => {
                                controller.command(Command::Deafen(true));
                                wait(&mut controller, |state| state.deafened && state.muted).await;
                            }
                            "undeafen" => {
                                controller.command(Command::Deafen(false));
                                let state = wait(&mut controller, |state| !state.deafened).await;
                                assert!(state.muted);
                            }
                            "unmute" => {
                                controller.command(Command::Mute(false));
                                wait(&mut controller, |state| !state.muted).await;
                            }
                            "mute" => {
                                controller.command(Command::Mute(true));
                                wait(&mut controller, |state| state.muted).await;
                            }
                            "leave" => {
                                controller.command(Command::Leave);
                                wait(&mut controller, |state| state.phase == Phase::Disconnected)
                                    .await;
                                std::fs::write(root.join("receiver-ack"), "leave").unwrap();
                                break;
                            }
                            "" => {}
                            _ => panic!("unknown receiver command"),
                        }
                        if !command.is_empty() {
                            std::fs::write(root.join("receiver-ack"), &command).unwrap();
                        }
                        previous = command;
                    }
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            })
            .await
            .expect("receiver harness deadline exceeded");
            controller.stop().await;
            sync.stop().await;
        });
    }

    #[test]
    fn join_errors_keep_the_cause_users_can_act_on() {
        let missing = join_error(matrix_rtc_livekit::CallError::Signalling(
            "the homeserver advertises no livekit transport and no fallback URL is configured"
                .into(),
        ));
        assert!(missing.to_string().contains("MSC4143"));
        let refused = join_error(matrix_rtc_livekit::CallError::Media(
            matrix_rtc_media::TransportError::Connect("401 from the token service".into()),
        ));
        assert!(refused.to_string().contains("refused"));
        assert!(!refused.to_string().contains("401"));
    }

    /// One participant process for `testing/test_voice_reliability.py`. Reads
    /// commands appended to `<name>.commands` and rewrites `<name>.state.json`
    /// after every voice change, so the harness can kill, pause or isolate it.
    #[test]
    #[ignore = "child process of testing/test_voice_reliability.py"]
    fn scripted_participant() {
        let root = std::path::PathBuf::from(
            std::env::var("YAPFAST_TEST_VOICE_ROOT").expect("test harness only"),
        );
        let name = std::env::var("YAPFAST_TEST_PARTICIPANT").expect("test harness only");
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        // Disposable fixture accounts only: dependency diagnostics are opt-in.
        match std::env::var("YAPFAST_TEST_LOG") {
            Ok(filter) => tracing_subscriber::fmt()
                .with_env_filter(tracing_subscriber::EnvFilter::new(filter))
                .init(),
            Err(_) => crate::logging::init(),
        }
        let runtime = crate::runtime::BackgroundRuntime::new().unwrap();
        runtime.block_on(async {
            let manifest: serde_json::Value =
                serde_json::from_slice(&std::fs::read(root.join("rooms.json")).unwrap()).unwrap();
            let voice_id = manifest["voice"].as_str().unwrap().to_string();
            let session = match std::env::var("YAPFAST_TEST_LOGIN") {
                // "<homeserver> <user>": a new device of an existing fixture account.
                Ok(login) if !root.join(&name).join("active.json").exists() => {
                    let (homeserver, user) = login.split_once(' ').unwrap();
                    Session::login(root.join(&name), homeserver, user, "talk-test-only").await.unwrap()
                }
                _ => Session::restore(root.join(&name)).await.unwrap(),
            };
            let sync = session.start_sync().await.unwrap();
            sync.wait_until_ready().await.unwrap();
            let mut controller = Controller::start(session.lease()).unwrap();
            let started = Instant::now();
            let state_path = root.join(format!("{name}.state.json"));
            let write_state = |state: &State, sequence: u64| {
                let snapshot = serde_json::json!({
                    "sequence": sequence,
                    "seconds": started.elapsed().as_secs_f64(),
                    "phase": format!("{:?}", state.phase),
                    "participants": state.participants.iter().map(|person| &person.user).collect::<Vec<_>>(),
                    "people": state.participants.iter().map(|person| serde_json::json!({
                        "user": person.user, "connected": person.connected, "problem": person.problem,
                    })).collect::<Vec<_>>(),
                    "members": state.participants.iter().map(|person| &person.id).collect::<Vec<_>>(),
                    "muted": state.muted,
                    "error": state.error,
                    "warning": state.warning,
                });
                let temporary = state_path.with_extension("tmp");
                std::fs::write(&temporary, serde_json::to_vec(&snapshot).unwrap()).unwrap();
                std::fs::rename(&temporary, &state_path).unwrap();
            };
            let mut sequence = 0;
            write_state(&controller.states.borrow_and_update().clone(), sequence);
            let commands_path = root.join(format!("{name}.commands"));
            let mut handled = 0;
            let mut poll = tokio::time::interval(Duration::from_millis(100));
            'run: loop {
                tokio::select! {
                    changed = controller.states.changed() => {
                        if changed.is_err() { break; }
                        sequence += 1;
                        write_state(&controller.states.borrow_and_update().clone(), sequence);
                    }
                    _ = poll.tick() => {
                        let commands = std::fs::read_to_string(&commands_path).unwrap_or_default();
                        let lines: Vec<_> = commands.lines().collect();
                        for command in &lines[handled.min(lines.len())..] {
                            match *command {
                                "join" => { controller.command(Command::Join(voice_id.clone())); }
                                "leave" => { controller.command(Command::Leave); }
                                "unmute" => { controller.command(Command::Mute(false)); }
                                "mute" => { controller.command(Command::Mute(true)); }
                                "leave-stale" => { controller.command(Command::LeaveStale); }
                                "exit" => break 'run,
                                other => panic!("unknown participant command {other}"),
                            }
                        }
                        handled = lines.len();
                    }
                }
            }
            controller.stop().await;
            sync.stop().await;
        });
    }

    /// Joins with a new, unverified device of a fixture account, as a client
    /// with a looser key policy could. Yapfast's own identity-based policy
    /// refuses to send anything from an unverified device.
    #[test]
    #[ignore = "child process of testing/test_voice_reliability.py"]
    fn unverified_participant() {
        let root = std::path::PathBuf::from(
            std::env::var("YAPFAST_TEST_VOICE_ROOT").expect("test harness only"),
        );
        let user = std::env::var("YAPFAST_TEST_LOGIN_USER").expect("test harness only");
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        crate::logging::init();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(tokio::task::LocalSet::new().run_until(async move {
            let manifest: serde_json::Value =
                serde_json::from_slice(&std::fs::read(root.join("rooms.json")).unwrap()).unwrap();
            let voice_id = OwnedRoomId::try_from(manifest["voice"].as_str().unwrap()).unwrap();
            let client = Client::builder()
                .homeserver_url("http://localhost:18008")
                .sqlite_store(root.join("unverified"), None)
                .with_room_key_recipient_strategy(matrix_sdk_crypto::CollectStrategy::AllDevices)
                .build()
                .await
                .unwrap();
            client
                .matrix_auth()
                .login_username(&user, "talk-test-only")
                .send()
                .await
                .unwrap();
            let sync = matrix_sdk_ui::sync_service::SyncService::builder(client.clone())
                .build()
                .await
                .unwrap();
            sync.start().await;
            let room = tokio::time::timeout(Duration::from_secs(30), async {
                loop {
                    if let Some(room) = client.get_room(&voice_id)
                        && room.state() == matrix_sdk::RoomState::Joined
                    {
                        return room;
                    }
                    tokio::time::sleep(Duration::from_millis(200)).await;
                }
            })
            .await
            .unwrap();
            let call = Call::join(
                &room,
                CallOptions {
                    livekit_service_url_fallback: Some("http://localhost:16080".into()),
                    sticky_duration_ms: Some(STICKY_DURATION_MS),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
            let commands = root.join("unverified.commands");
            let state = root.join("unverified.state.json");
            std::fs::write(
                &state,
                serde_json::json!({"phase":"Connected","member":call.membership_id()}).to_string(),
            )
            .unwrap();
            while !std::fs::read_to_string(&commands)
                .unwrap_or_default()
                .contains("exit")
            {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            tokio::time::timeout(Duration::from_secs(10), call.leave())
                .await
                .unwrap()
                .unwrap();
            std::fs::write(&state, r#"{"phase":"Disconnected"}"#).unwrap();
            sync.stop().await;
        }));
    }

    /// Remote load for `testing/measure_connected.py` and `testing/stress_voice.py`:
    /// Bob plus extra fixture accounts in one process. Generated tones use no audio
    /// device, so one process can publish for every account. Commands appended to
    /// `load.commands`: `chat-start`, `chat-stop`, `say TEXT`, `join N`, `leave`,
    /// `exit`, `dtx on|off` (for later joins), `pattern all|conversation` and
    /// `camera N`, which films the first N joined accounts with the test
    /// pattern (`YAPFAST_CAMERA_TEST_PATTERN=1`), and mutes the rest.
    /// `conversation` passes the tone around, one speaker for 3 s at a time, with
    /// digital silence from everyone else. `YAPFAST_TEST_PUBLISH_ONLY=1` stops the load
    /// accounts subscribing, so this process does not decode every stream.
    #[test]
    #[ignore = "child process of testing/measure_connected.py"]
    fn load_participants() {
        let root = std::path::PathBuf::from(
            std::env::var("YAPFAST_TEST_VOICE_ROOT").expect("test harness only"),
        );
        let extra: usize =
            std::env::var("YAPFAST_TEST_EXTRA_ACCOUNTS").map_or(0, |count| count.parse().unwrap());
        let publish_only =
            std::env::var("YAPFAST_TEST_PUBLISH_ONLY").is_ok_and(|value| value == "1");
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        crate::logging::init();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(tokio::task::LocalSet::new().run_until(async move {
            let manifest: serde_json::Value =
                serde_json::from_slice(&std::fs::read(root.join("rooms.json")).unwrap()).unwrap();
            let voice_id = OwnedRoomId::try_from(manifest["voice"].as_str().unwrap()).unwrap();
            let text_id = OwnedRoomId::try_from(manifest["text"].as_str().unwrap()).unwrap();
            let status = |text: &str| std::fs::write(root.join("load.status"), text).unwrap();
            let bob = Session::restore(root.join("bob")).await.unwrap();
            let mut sessions = vec![bob];
            for index in 0..extra {
                let directory = root.join(format!("load-{index}"));
                let session = if directory.join("active.json").exists() {
                    Session::restore(directory).await.unwrap()
                } else {
                    let username = format!("yapfast_load_{}_{index}", uuid::Uuid::new_v4().simple());
                    let mut request = matrix_sdk::ruma::api::client::account::register::v3::Request::new();
                    request.username = Some(username.clone());
                    request.password = Some("talk-test-only".into());
                    request.inhibit_login = true;
                    request.auth = Some(matrix_sdk::ruma::api::client::uiaa::AuthData::Dummy(Default::default()));
                    sessions[0].client.send(request).await.unwrap();
                    let session = Session::login(directory, "http://localhost:18008", &username, "talk-test-only").await.unwrap();
                    let user = session.client.user_id().unwrap().to_owned();
                    sessions[0].client.get_room(&voice_id).unwrap().invite_user_by_id(&user).await.unwrap();
                    session.client.join_room_by_id(&voice_id).await.unwrap();
                    session
                };
                sessions.push(session);
            }
            let mut syncs = Vec::new();
            for session in &sessions {
                let sync = session.start_sync().await.unwrap();
                sync.wait_until_ready().await.unwrap();
                syncs.push(sync);
            }
            status("ready");
            let commands = root.join("load.commands");
            let mut handled = 0;
            let mut calls = Vec::new();
            let mut tones = Vec::new();
            let mut cameras: Vec<crate::matrix_video::Filming> = Vec::new();
            let mut chatting = false;
            let mut dtx = true;
            let conversation = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let speakers = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let started = std::time::Instant::now();
            let mut sent = 0u64;
            let mut chat = tokio::time::interval(Duration::from_secs(2));
            'run: loop {
                tokio::select! {
                    _ = chat.tick(), if chatting => {
                        sent += 1;
                        let room = sessions[0].client.get_room(&text_id).unwrap();
                        let body = format!("Load message {sent}: the quick brown fox jumps over the lazy dog.");
                        let _ = room.send(matrix_sdk::ruma::events::room::message::RoomMessageEventContent::text_plain(body)).await;
                    }
                    _ = tokio::time::sleep(Duration::from_millis(200)) => {
                        let text = std::fs::read_to_string(&commands).unwrap_or_default();
                        let lines: Vec<String> = text.lines().map(str::to_owned).collect();
                        for command in lines.iter().skip(handled) {
                            match command.split_once(' ').unwrap_or((command, "")) {
                                ("chat-start", _) => chatting = true,
                                ("chat-stop", _) => chatting = false,
                                ("dtx", value) => dtx = value == "on",
                                ("pattern", value) => conversation.store(value == "conversation", std::sync::atomic::Ordering::Relaxed),
                                ("say", text) => {
                                    let room = sessions[0].client.get_room(&text_id).unwrap();
                                    let _ = room.send(matrix_sdk::ruma::events::room::message::RoomMessageEventContent::text_plain(text)).await;
                                }
                                ("join", count) => {
                                    let count: usize = count.parse().unwrap();
                                    for (index, session) in sessions.iter().enumerate().skip(calls.len()).take(count.saturating_sub(calls.len())) {
                                        let room = session.client.get_room(&voice_id).unwrap();
                                        let call = Call::join(&room, CallOptions {
                                            livekit_service_url_fallback: Some("http://localhost:16080".into()),
                                            sticky_duration_ms: Some(STICKY_DURATION_MS),
                                            auto_subscribe: !publish_only,
                                            ..Default::default()
                                        }).await.unwrap();
                                        let (pattern, count) = (conversation.clone(), speakers.clone());
                                        let speaking = move || {
                                            use std::sync::atomic::Ordering::Relaxed;
                                            !pattern.load(Relaxed)
                                                || (started.elapsed().as_secs() / 3) as usize % count.load(Relaxed).max(1) == index
                                        };
                                        // Distinct tones keep every stream audible and decodable.
                                        tones.push(crate::calls::publish_tone_while(&call, 300.0 + 60.0 * index as f64, dtx, speaking).await.unwrap());
                                        calls.push(call);
                                        speakers.store(calls.len(), std::sync::atomic::Ordering::Relaxed);
                                    }
                                    status(&format!("joined {}", calls.len()));
                                }
                                ("camera", count) => {
                                    let count: usize = count.parse().unwrap();
                                    for (index, camera) in cameras.iter_mut().enumerate() {
                                        if index >= count && camera.filming() {
                                            camera.pause();
                                        } else if index < count && !camera.filming() {
                                            let opened = camera.open(Some("test-pattern")).await.unwrap();
                                            camera.opened(&calls[index], opened).await.unwrap();
                                        }
                                    }
                                    while cameras.len() < count.min(calls.len()) {
                                        let mut camera = crate::matrix_video::Filming::new().unwrap();
                                        let opened = camera.open(Some("test-pattern")).await.unwrap();
                                        camera.opened(&calls[cameras.len()], opened).await.unwrap();
                                        cameras.push(camera);
                                    }
                                    let filming = cameras.iter().filter(|camera| camera.filming()).count();
                                    status(&format!("filming {filming}"));
                                }
                                ("leave", _) => {
                                    cameras.clear();
                                    tones.clear();
                                    speakers.store(0, std::sync::atomic::Ordering::Relaxed);
                                    while let Some(call) = calls.pop() {
                                        let _ = tokio::time::timeout(Duration::from_secs(10), call.leave()).await;
                                    }
                                    status("ready");
                                }
                                ("exit", _) => break 'run,
                                (other, _) => panic!("unknown load command {other}"),
                            }
                        }
                        handled = lines.len();
                    }
                }
            }
            cameras.clear();
            tones.clear();
            while let Some(call) = calls.pop() {
                let _ = tokio::time::timeout(Duration::from_secs(10), call.leave()).await;
            }
            for sync in syncs {
                sync.stop().await;
            }
            status("stopped");
        }));
    }

    /// One remote participant per process for `testing/join_voice.py`, so each
    /// client's cost and key exchange can be measured on its own. It restores the
    /// fixture account in `YAPFAST_TEST_ACCOUNT_DIR` and reads `join`, `leave` and
    /// `exit` from `commands` there. While joined it publishes a tone in turn with
    /// the others (`YAPFAST_TEST_INDEX` of `YAPFAST_TEST_COUNT`, 3 s each, silence
    /// otherwise) and decodes every subscribed audio track, as playback would.
    /// `status.json` reports its identity and membership, join and leave times, the
    /// remote identities present and those whose key has arrived, the members whose
    /// audio currently decrypts, and counts of missing-key and decryption failures.
    #[test]
    #[ignore = "child process of testing/join_voice.py"]
    fn stress_participant() {
        use futures_util::StreamExt;
        use std::sync::atomic::{AtomicUsize, Ordering};
        let root = std::path::PathBuf::from(
            std::env::var("YAPFAST_TEST_VOICE_ROOT").expect("test harness only"),
        );
        let account = std::path::PathBuf::from(
            std::env::var("YAPFAST_TEST_ACCOUNT_DIR").expect("test harness only"),
        );
        let index: usize = std::env::var("YAPFAST_TEST_INDEX")
            .unwrap()
            .parse()
            .unwrap();
        let count: usize = std::env::var("YAPFAST_TEST_COUNT")
            .unwrap()
            .parse()
            .unwrap();
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        crate::logging::init();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(tokio::task::LocalSet::new().run_until(async move {
            let manifest: serde_json::Value =
                serde_json::from_slice(&std::fs::read(root.join("rooms.json")).unwrap()).unwrap();
            let voice_id = OwnedRoomId::try_from(manifest["voice"].as_str().unwrap()).unwrap();
            let session = Session::restore(account.clone()).await.unwrap();
            let sync = session.start_sync().await.unwrap();
            sync.wait_until_ready().await.unwrap();
            // Frames that arrived before their key, and frames a held key could not decrypt.
            let missing = std::rc::Rc::new(AtomicUsize::new(0));
            let failures = std::rc::Rc::new(AtomicUsize::new(0));
            let decrypting = std::rc::Rc::new(std::cell::RefCell::new(HashSet::<String>::new()));
            let mut status = serde_json::json!({"phase": "ready"});
            let write = |status: &serde_json::Value| {
                let temporary = account.join("status.json.tmp");
                std::fs::write(&temporary, status.to_string()).unwrap();
                std::fs::rename(temporary, account.join("status.json")).unwrap();
            };
            write(&status);
            let commands = account.join("commands");
            let mut handled = 0;
            let mut call: Option<Call> = None;
            let mut tone = None;
            let mut decoders: Vec<tokio::task::JoinHandle<()>> = Vec::new();
            let started = std::time::Instant::now();
            'run: loop {
                tokio::time::sleep(Duration::from_millis(100)).await;
                if let Some(active) = call.as_mut() {
                    while let Ok(event) = active.events().try_recv() {
                        if let RoomEvent::TrackSubscribed {
                            track: RemoteTrack::Audio(track),
                            ..
                        } = event
                        {
                            let mut stream =
                                livekit::webrtc::audio_stream::native::NativeAudioStream::new(
                                    track.rtc_track(),
                                    48_000,
                                    1,
                                );
                            decoders.push(tokio::spawn(async move {
                                while stream.next().await.is_some() {}
                            }));
                        }
                    }
                    let present: Vec<String> = active
                        .session()
                        .room()
                        .remote_participants()
                        .keys()
                        .map(ToString::to_string)
                        .collect();
                    let keyed: Vec<&String> = present
                        .iter()
                        .filter(|identity| active.imported_key_for(identity))
                        .collect();
                    status["present"] = serde_json::json!(present);
                    status["keyed"] = serde_json::json!(keyed);
                    status["missing_key_events"] =
                        serde_json::json!(missing.load(Ordering::Relaxed));
                    status["decryption_failures"] =
                        serde_json::json!(failures.load(Ordering::Relaxed));
                    status["decrypting"] =
                        serde_json::json!(decrypting.borrow().iter().collect::<Vec<_>>());
                    write(&status);
                }
                let text = std::fs::read_to_string(&commands).unwrap_or_default();
                let lines: Vec<String> = text.lines().map(str::to_owned).collect();
                for command in lines.iter().skip(handled) {
                    match command.as_str() {
                        "join" => {
                            let room = session.client.get_room(&voice_id).unwrap();
                            let began = std::time::Instant::now();
                            let joined = Call::join(
                                &room,
                                CallOptions {
                                    livekit_service_url_fallback: Some(
                                        "http://localhost:16080".into(),
                                    ),
                                    sticky_duration_ms: Some(STICKY_DURATION_MS),
                                    ..Default::default()
                                },
                            )
                            .await
                            .unwrap();
                            status["join_ms"] =
                                serde_json::json!(began.elapsed().as_millis() as u64);
                            status["identity"] = serde_json::json!(joined.local_identity());
                            status["member_id"] = serde_json::json!(joined.membership_id());
                            let mut events = joined.subscribe_call_events();
                            let (missing, counter, decrypting) =
                                (missing.clone(), failures.clone(), decrypting.clone());
                            decoders.push(tokio::task::spawn_local(async move {
                                while let Ok(event) = events.recv().await {
                                    if let CallEvent::FrameEncryptionState {
                                        member_id,
                                        state,
                                        ..
                                    } = event
                                    {
                                        match state {
                                            matrix_rtc_media::FrameEncryptionState::Ok => {}
                                            matrix_rtc_media::FrameEncryptionState::MissingKey => {
                                                missing.fetch_add(1, Ordering::Relaxed);
                                            }
                                            _ => {
                                                counter.fetch_add(1, Ordering::Relaxed);
                                            }
                                        }
                                        if matches!(
                                            state,
                                            matrix_rtc_media::FrameEncryptionState::Ok
                                        ) {
                                            decrypting.borrow_mut().insert(member_id);
                                        } else {
                                            decrypting.borrow_mut().remove(&member_id);
                                        }
                                    }
                                }
                            }));
                            let speaking = move || {
                                (started.elapsed().as_secs() / 3) as usize % count.max(1) == index
                            };
                            tone = Some(
                                crate::calls::publish_tone_while(
                                    &joined,
                                    300.0 + 40.0 * index as f64,
                                    true,
                                    speaking,
                                )
                                .await
                                .unwrap(),
                            );
                            call = Some(joined);
                            status["phase"] = serde_json::json!("joined");
                        }
                        "leave" => {
                            tone = None;
                            decoders.drain(..).for_each(|task| task.abort());
                            decrypting.borrow_mut().clear();
                            if let Some(active) = call.take() {
                                let began = std::time::Instant::now();
                                let _ =
                                    tokio::time::timeout(Duration::from_secs(30), active.leave())
                                        .await;
                                status["leave_ms"] =
                                    serde_json::json!(began.elapsed().as_millis() as u64);
                            }
                            status["phase"] = serde_json::json!("ready");
                            status["present"] = serde_json::json!([]);
                            status["keyed"] = serde_json::json!([]);
                            status["decrypting"] = serde_json::json!([]);
                            write(&status);
                        }
                        "exit" => break 'run,
                        other => panic!("unknown participant command {other}"),
                    }
                }
                handled = lines.len();
            }
            drop(tone);
            if let Some(active) = call.take() {
                let _ = tokio::time::timeout(Duration::from_secs(10), active.leave()).await;
            }
            sync.stop().await;
            status["phase"] = serde_json::json!("stopped");
            write(&status);
        }));
    }

    #[test]
    #[ignore = "requires scripts/test-voice and scripts/test-stack up; uses virtual audio only"]
    fn persistent_encrypted_voice_and_native_audio() {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        let runtime = crate::runtime::BackgroundRuntime::new().unwrap();
        runtime.block_on(async {
            let root = crate::voice_demo::provision().await.unwrap();
            let manifest: serde_json::Value =
                serde_json::from_slice(&std::fs::read(root.join("rooms.json")).unwrap()).unwrap();
            let voice_id = manifest["voice"].as_str().unwrap().to_string();
            let text_id = manifest["text"].as_str().unwrap();
            let alice = Session::restore(root.join("alice")).await.unwrap();
            assert!(
                require_voice_channel(
                    &alice
                        .client
                        .get_room(&OwnedRoomId::try_from(text_id).unwrap())
                        .unwrap()
                )
                .await
                .is_err()
            );
            let mut plain = matrix_sdk::ruma::api::client::room::create_room::v3::Request::new();
            plain.creation_content = Some(
                matrix_sdk::ruma::serde::Raw::from_json_string(
                    serde_json::json!({"type":"org.matrix.msc3417.call"}).to_string(),
                )
                .unwrap(),
            );
            let plain = alice.client.create_room(plain).await.unwrap();
            assert!(require_voice_channel(&plain).await.is_err());
            // Even an encrypted room must have an encrypted RTC slot.
            let voice_room = alice
                .client
                .get_room(&OwnedRoomId::try_from(voice_id.clone()).unwrap())
                .unwrap();
            matrix_rtc_livekit::open_slot(&alice.client, &voice_id, "m.call#ROOM", "m.call", None)
                .await
                .unwrap();
            assert!(require_voice_channel(&voice_room).await.is_err());
            matrix_rtc_livekit::open_slot(
                &alice.client,
                &voice_id,
                "m.call#ROOM",
                "m.call",
                Some(matrix_rtc_core::SlotEncryption {
                    encryption_type: "m.per_member".into(),
                    extra: Default::default(),
                }),
            )
            .await
            .unwrap();
            let alice_sync = alice.start_sync().await.unwrap();
            alice_sync.wait_until_ready().await.unwrap();
            let mut first = Controller::start(alice.lease()).unwrap();
            first.command(Command::Join(voice_id.clone()));
            let state = wait(&mut first, |state| {
                state.phase == Phase::Connected && state.participants.len() == 1
            })
            .await;
            assert!(state.muted);
            virtual_devices(&mut first).await;
            // Hold authorization open after Bob publishes membership. Leave
            // must cancel that request and remove Bob before a fresh join.
            let stalled = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let url = format!("http://{}", stalled.local_addr().unwrap());
            let mut cancelling = ReceiverProcess(std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "matrix_voice::tests::cancel_pending_join", "--ignored", "--nocapture"])
                .env("YAPFAST_TEST_VOICE_ROOT", &root).env("YAPFAST_RTC_AUTHORIZATION_URL", url).spawn().unwrap());
            let (pending_request, _) = tokio::time::timeout(Duration::from_secs(25), stalled.accept()).await.unwrap().unwrap();
            wait(&mut first, |state| state.participants.len() == 2).await;
            receiver_command(&root, "cancel").await;
            assert!(cancelling.0.wait().unwrap().success());
            wait(&mut first, |state| state.participants.len() == 1).await;
            drop(pending_request);
            let cancel_milliseconds: u64 = std::fs::read_to_string(root.join("cancel-milliseconds")).unwrap().parse().unwrap();
            std::fs::remove_file(root.join("receiver-command")).unwrap();
            std::fs::remove_file(root.join("receiver-ack")).unwrap();
            // Longer than a heartbeat; there is no timeout based on being alone.
            let wait_alone = if std::env::var("YAPFAST_TEST_FAST").as_deref() == Ok("1") { 1 } else { 30 };
            tokio::time::sleep(Duration::from_secs(wait_alone)).await;
            assert_eq!(first.states.borrow().phase, Phase::Connected);
            assert_eq!(first.states.borrow().participants.len(), 1);
            // Each client needs its own process: LiveKit shares an ADM within
            // one process, unlike two friends running Yapfast on separate machines.
            let mut receiver = ReceiverProcess(std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "matrix_voice::tests::native_receiver", "--ignored", "--nocapture"])
                .env("YAPFAST_TEST_VOICE_ROOT", &root).spawn().unwrap());
            receiver_ack(&root, "ready").await;
            wait(&mut first, |state| state.participants.len() == 2).await;
            first.command(Command::Mute(false));
            wait(&mut first, |state| !state.muted).await;
            tokio::time::sleep(Duration::from_secs(2)).await;
            let signal = output_energy().await;
            assert!(
                signal > 50.0,
                "native encrypted microphone did not reach virtual speakers: {signal}"
            );
            receiver_command(&root, "deafen").await;
            tokio::time::sleep(Duration::from_secs(1)).await;
            let deafened = output_energy().await;
            assert!(
                deafened < signal * 0.05,
                "deafen did not silence output: {deafened} vs {signal}"
            );
            receiver_command(&root, "undeafen").await;
            assert!(output_energy().await > signal * 0.25);
            // In push to talk the microphone is silent until the key is held.
            crate::push_to_talk::set_on(true);
            tokio::time::sleep(Duration::from_secs(1)).await;
            let key_up = output_energy().await;
            assert!(
                key_up < signal * 0.05,
                "push to talk sent with the key up: {key_up} vs {signal}"
            );
            crate::push_to_talk::hold(true);
            tokio::time::sleep(Duration::from_secs(1)).await;
            let key_held = output_energy().await;
            assert!(
                key_held > signal * 0.25,
                "push to talk didn't send with the key held: {key_held} vs {signal}"
            );
            crate::push_to_talk::hold(false);
            crate::push_to_talk::set_on(false);
            first.command(Command::Mute(true));
            wait(&mut first, |state| state.muted).await;
            tokio::time::sleep(Duration::from_secs(1)).await;
            assert!(
                output_energy().await < signal * 0.05,
                "mute did not silence captured audio"
            );
            receiver_command(&root, "unmute").await;
            let reverse = output_energy().await;
            assert!(reverse > 50.0, "reverse native audio did not reach speakers: {reverse}");
            receiver_command(&root, "mute").await;
            receiver_command(&root, "leave").await;
            assert!(receiver.0.wait().unwrap().success());
            wait(&mut first, |state| state.participants.len() == 1).await;
            assert_eq!(first.states.borrow().phase, Phase::Connected);
            first.command(Command::Leave);
            wait(&mut first, |state| state.phase == Phase::Disconnected).await;
            first.stop().await;
            alice_sync.stop().await;
            std::fs::write(
                root.join("voice-result.json"),
                serde_json::to_vec_pretty(&serde_json::json!({
                    "virtual_devices":true, "physical_hardware":false, "wait_alone_seconds":wait_alone,
                    "cancel_stalled_join_milliseconds":cancel_milliseconds, "cancelled_membership_removed":true,
                    "joined_muted":true, "late_join":true, "native_encrypted_audio":true,
                    "received_440hz_amplitude":signal, "reverse_440hz_amplitude":reverse, "deafened_amplitude":deafened,
                    "push_to_talk_key_up_amplitude":key_up, "push_to_talk_key_held_amplitude":key_held,
                    "mute":true, "deafen":true, "clean_leave":true
                }))
                .unwrap(),
            )
            .unwrap();
            println!(
                "Voice controller and native virtual-device audio passed; report {}",
                root.join("voice-result.json").display()
            );
        });
    }
}
