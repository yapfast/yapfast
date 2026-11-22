// Copyright 2026 Element Creations Ltd.
//
// SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-Element-Commercial
// Please see LICENSE in the repository root for full details.

//! The call engine: one flat participant roster over N transport connections.
//!
//! The engine consumes the core's membership snapshots (the signalling truth
//! about who is in the call) and the call layer's raised hands and reactions,
//! and reconciles them with the media of a [`MediaPool`] — which owns the
//! MSC4195 connections and maps transport identities back to `member_id`s —
//! into the [`Participant`] roster, the tiles and the unified [`CallEvent`]
//! stream.
//!
//! The pool is driven from the engine's actor rather than running beside it,
//! so a member's join always lands on the roster before their media does, and
//! the roster and the event stream never disagree about it. Only the *own*
//! focus ending (adopted via [`CallEngine::adopt_own_connection`]) ends the
//! call; a peer focus is the pool's to reconnect.
//!
//! The roster is the whole call; joins, leaves, hands and reactions on the
//! event stream are only the changes made while we are in it, so the roster
//! we arrive to, and the hands already up in it, are never announced.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use matrix_rtc_call::{
    ConnectionLossReason, MediaStatusReporter, RaisedHand, ReceivedReaction, SignallingHealth,
};
use matrix_rtc_core::{DiscardedKey, JoinedMembership, LeaveReason};
use tokio::sync::{broadcast, mpsc, oneshot, watch};

use matrix_rtc_core::executor as rt;
use matrix_rtc_transport::pool::{
    MediaPool, OwnConnectionLoss, PoolConfig, PoolEvent, PoolMessage, RemoteTracks,
};
use matrix_rtc_transport::{
    ConnectionContext, ConnectionEvent, ConnectionState, FrameEncryptionState, LocalTrackHandle,
    MediaConstraints, MediaStreamKind, MediaTransport, PublishOptions, ReceiveStats,
    RemoteTrackHandle, TransportConnection, TransportError,
};

use crate::event::{CallEvent, FrameEncryptionDiagnostic, SpeakingMember};
use crate::interruptions::Interruptions;
use crate::participant::{Participant, StreamState, UnableToDecrypt};
use crate::tile::{
    CallTile, DetailWindow, LocalState, TileId, TileRoster, Tiles, derive_tiles, window,
};

/// Capacity of the broadcast [`CallEvent`] channel. Subscribers that fall
/// further behind than this observe a `Lagged` error and miss events; they
/// should resynchronise from [`CallEngine::participants`].
const EVENT_CHANNEL_CAPACITY: usize = 256;

/// How long a member's frames must fail to decrypt, without a break, before
/// they are shown unable to decrypt, unless [`StabilityConfig::key_grace`]
/// says otherwise (034 C6). Senders switch to a new key 5 s
/// after sending it, and on matrix.org keys took up to 8 s to arrive, so a
/// rotation can drop frames for 3 s; that is not a fault to draw, and a grace
/// barely past it would flash the state for a split second.
pub const KEY_GRACE: Duration = Duration::from_secs(4);

/// How long a participant may be on an SFU without a membership we have
/// received, while our signalling is healthy, before it is listed as
/// unverified (spec 038 R25). Long enough for a membership that is only
/// propagating, of the same order as 034's fallback grace.
pub const UNVERIFIED_GRACE: Duration = Duration::from_secs(10);

/// What a member's video stream is asked for while we cannot decrypt it: kept
/// flowing at the lowest layer even when the host says it is not visible. A
/// host hides the video exactly when it shows the key error, and a paused
/// stream sends no frames, so a key that arrives later could never be seen to
/// work: the member would stay unable to decrypt until something else, such as
/// their unmuting, sent frames. A stream the host released (`enabled: false`)
/// or a bandwidth-saver choice is left as asked.
fn probe_undecryptable(
    kind: MediaStreamKind,
    constraints: MediaConstraints,
    undecryptable: bool,
) -> MediaConstraints {
    let video = matches!(kind, MediaStreamKind::Camera | MediaStreamKind::ScreenShare);
    if !undecryptable || !video || !constraints.enabled || constraints.visible {
        return constraints;
    }
    MediaConstraints {
        visible: true,
        detail: matrix_rtc_transport::VideoDetail::Quality(matrix_rtc_transport::QualityLimit::Low),
        ..constraints
    }
}

/// Why a remote member's media cannot be shown now: sent in clear at once, a
/// key failure only once it has lasted `grace`, and only while they publish
/// something. A member with no streams, such as a client that died without
/// leaving, has nothing to fail to decrypt, whatever key state it left behind.
fn unable_to_decrypt(
    member_id: &str,
    publishing: bool,
    grace: Duration,
    sent_in_clear: &HashSet<String>,
    undecryptable_since: &HashMap<String, rt::Instant>,
    encryption_states: &HashMap<String, FrameEncryptionState>,
) -> Option<UnableToDecrypt> {
    if sent_in_clear.contains(member_id) {
        return Some(UnableToDecrypt::NotEncrypted);
    }
    if !publishing {
        return None;
    }
    if undecryptable_since
        .get(member_id)
        .is_none_or(|since| since.elapsed() < grace)
    {
        return None;
    }
    match encryption_states.get(member_id) {
        Some(FrameEncryptionState::MissingKey) => Some(UnableToDecrypt::MissingKey),
        _ => Some(UnableToDecrypt::BadKey),
    }
}

/// How much the tile order (spec 002 R10, R11) and a tile's key failures
/// (034 C6) are damped. Configurable because the values are a product
/// decision, not a protocol one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StabilityConfig {
    /// Sustained voice before a member ranks as speaking; the tile flag is not delayed.
    pub promote: Duration,
    /// Silence before a speaking member stops ranking as one.
    pub demote: Duration,
    /// Reorders inside this window are delivered as one.
    pub coalesce: Duration,
    /// How long a member's frames must fail to decrypt, without a break,
    /// before they are shown unable to decrypt; [`KEY_GRACE`] by default.
    pub key_grace: Duration,
}

impl Default for StabilityConfig {
    fn default() -> Self {
        Self {
            // Symmetric: a longer demote than promote latches a tile at the
            // top of the order well after the speaker stopped, which reads as
            // a stuck UI rather than a damped one.
            promote: Duration::from_millis(1500),
            demote: Duration::from_millis(1500),
            coalesce: Duration::from_millis(300),
            key_grace: KEY_GRACE,
        }
    }
}

/// Raw speaking state of one member, with the generation of the hysteresis
/// timer that would make it effective (R10). A bump makes an older timer stale.
struct SpeakerState {
    raw: bool,
    generation: u64,
}

pub struct EngineConfig {
    /// Transport backends, in descending order of preference.
    pub transports: Vec<Arc<dyn MediaTransport>>,
    /// `member.id` of our own membership, used to flag the local participant.
    pub own_member_id: String,
    /// Call-scoped context passed to [`MediaTransport::connect`].
    pub ctx: ConnectionContext,
    /// Connection key of the focus we publish on. The engine never opens this
    /// one itself — the caller establishes it (so join can fail fast) and
    /// hands it over via [`CallEngine::adopt_own_connection`].
    pub own_connection_key: Option<String>,
    /// The call layer's raised-hand snapshots
    /// (`RtcCall::subscribe_raised_hands`), merged onto the roster as
    /// [`Participant::hand_raised_at_ms`] and
    /// [`Participant::hand_raised_position`], and announced as
    /// [`CallEvent::HandRaised`] / [`CallEvent::HandLowered`] while we are in
    /// the call. `None` leaves every hand down.
    pub raised_hands: Option<watch::Receiver<Vec<RaisedHand>>>,
    /// The call layer's emoji reactions
    /// (`RtcCall::subscribe_reactions`),
    /// forwarded as [`CallEvent::Reaction`] for members on the roster while
    /// we are in the call. `None` reports no reactions.
    pub reactions: Option<broadcast::Receiver<ReceivedReaction>>,
    /// `RtcSession::subscribe_ended`: once `Some`, the engine ends the call
    /// and closes every connection, the own-focus one included.
    pub ended: Option<watch::Receiver<Option<LeaveReason>>>,
    /// The call's status (`RtcCall::media_status_reporter`), told when our
    /// media degrades, is lost or is shut down. `None` reports nothing.
    pub media_status: Option<MediaStatusReporter>,
    /// Damping of the tile order; see [`StabilityConfig`].
    pub stability: StabilityConfig,
    /// The client's signalling health (`RtcCall::subscribe_signalling_health`),
    /// copied into [`Interruptions::signalling`]. `None` reports it reachable.
    pub signalling: Option<watch::Receiver<SignallingHealth>>,
    /// Whether our membership has lapsed for the other members
    /// (`RtcCall::subscribe_own_membership_lapsed`, 034 R17): our
    /// publications are paused at the transport while it is. `None` relies
    /// on the snapshots alone.
    pub own_membership_lapsed: Option<watch::Receiver<bool>>,
    /// How long our own focus may stay unreachable before the call ends
    /// (034 R24); [`matrix_rtc_transport::pool::OWN_RECONNECT_BUDGET`] by
    /// default.
    pub own_reconnect_budget: Duration,
    /// Whether our media may move to a peer's focus while ours is down
    /// (034 R25): see [`PoolConfig::sfu_fallback`]. Our membership is
    /// republished naming it through [`Self::media_status`].
    pub sfu_fallback: bool,
}

/// Everything the engine can be told from outside its actor task.
enum ActorMessage {
    /// Take ownership of the caller-established own-focus connection.
    AdoptConnection {
        connection: Box<dyn TransportConnection>,
        events: mpsc::UnboundedReceiver<ConnectionEvent>,
    },
    /// Take the own-focus handle unconnected and connect it.
    ConnectOwn {
        connection: Box<dyn TransportConnection>,
    },
    /// A media decryption key was imported for a transport identity.
    KeyImported {
        identity: String,
        key_index: u8,
    },
    KeyDiscarded(DiscardedKey),
    /// A local publication went up; the roster must show it and peers were
    /// already told by the transport. `muted` is the state it was published
    /// with, which the transport announced along with the track.
    LocalPublished {
        kind: MediaStreamKind,
        muted: bool,
        track: Arc<dyn LocalTrackHandle>,
    },
    /// A local retraction resolved at the transport; the roster must drop it.
    /// `track` identifies which publication was retracted, so a same-kind
    /// publish that landed while the retraction was in flight is not the one
    /// removed.
    LocalUnpublished {
        kind: MediaStreamKind,
        track: Arc<dyn LocalTrackHandle>,
    },
    SetLocalMuted {
        kind: MediaStreamKind,
        muted: bool,
        respond: oneshot::Sender<Result<(), TransportError>>,
    },
    /// Retract one of our own publications.
    Unpublish {
        kind: MediaStreamKind,
        respond: oneshot::Sender<Result<(), TransportError>>,
    },
    /// Publish a local track on the own-focus connection.
    Publish {
        options: PublishOptions,
        respond: oneshot::Sender<Result<Arc<dyn LocalTrackHandle>, TransportError>>,
    },
    /// Store new constraints for one stream; the pool debounces them.
    SetConstraints {
        member_id: String,
        kind: MediaStreamKind,
        constraints: MediaConstraints,
    },
    /// A hysteresis timer elapsed; apply if it is still the current one (R10).
    SpeakerTimer {
        member_id: String,
        generation: u64,
    },
    /// A member's key grace may have run out; the turn's sync reads it.
    KeyGrace,
    /// A participant's unverified grace may have run out; the turn's sync
    /// reads it (spec 038 R25).
    UnverifiedGrace,
    /// The coalesce window closed; publish the new order if it is still the
    /// current window (R11).
    FlushReorder {
        generation: u64,
    },
    /// Which tiles get full records; see [`CallEngine::set_detail_window`].
    SetDetailWindow(DetailWindow),
    /// Rank only above this many tiles; see [`CallEngine::set_ranking_threshold`].
    SetRankingThreshold(u32),
    /// Close every pooled connection and stop.
    Shutdown {
        ack: oneshot::Sender<()>,
    },
}

/// A cheap, cloneable handle for feeding the engine from other components
/// (e.g. a key bridge reporting imports), without owning the engine.
#[derive(Clone)]
pub struct EngineHandle {
    messages: mpsc::UnboundedSender<ActorMessage>,
}

impl EngineHandle {
    /// See [`CallEngine::notify_key_imported`]. No-op once the engine is gone.
    pub fn notify_key_imported(&self, identity: impl Into<String>, key_index: u8) {
        let _ = self.messages.send(ActorMessage::KeyImported {
            identity: identity.into(),
            key_index,
        });
    }

    /// See [`CallEngine::notify_key_discarded`]. No-op once the engine is gone.
    pub fn notify_key_discarded(&self, report: DiscardedKey) {
        let _ = self.messages.send(ActorMessage::KeyDiscarded(report));
    }
}

/// One MatrixRTC call as a flat set of participants with media streams.
///
/// Created per joined slot; dropping the engine stops its background task
/// (pooled connections then disconnect as they are dropped — call
/// [`CallEngine::shutdown`] first for an orderly close).
pub struct CallEngine {
    messages: mpsc::UnboundedSender<ActorMessage>,
    events_tx: broadcast::Sender<CallEvent>,
    /// The receiver created with the channel, handed to the first
    /// [`Self::subscribe_events`]; see there.
    first_events_rx: Mutex<Option<broadcast::Receiver<CallEvent>>>,
    participants_rx: watch::Receiver<Vec<Participant>>,
    tiles_rx: watch::Receiver<TileRoster>,
    local_rx: watch::Receiver<Option<LocalState>>,
    interruptions_rx: watch::Receiver<Interruptions>,
    unverified_rx: watch::Receiver<Vec<String>>,
    tracks: RemoteTracks,
    task: rt::JoinHandle<()>,
}

impl Drop for CallEngine {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl CallEngine {
    /// Start the engine over the core's membership snapshot channel
    /// (`RtcSession::subscribe_membership_snapshots`). The current snapshot is
    /// applied immediately.
    ///
    /// Must be called within a tokio runtime off `wasm32` (spawns the actor
    /// task, which is `Send` there — no `LocalSet` needed). On `wasm32` the
    /// actor runs on the JS microtask queue via `spawn_local`.
    pub fn new(
        config: EngineConfig,
        memberships: watch::Receiver<Vec<JoinedMembership>>,
    ) -> CallEngine {
        let (messages_tx, messages_rx) = mpsc::unbounded_channel();
        let (events_tx, first_events_rx) = broadcast::channel(EVENT_CHANNEL_CAPACITY);
        let (participants_tx, participants_rx) = watch::channel(Vec::new());
        let (tiles_tx, tiles_rx) = watch::channel(TileRoster::default());
        let (local_tx, local_rx) = watch::channel(None);
        let own_connection_key = config.own_connection_key.clone();
        let (pool, pool_inbox) = MediaPool::new(PoolConfig {
            transports: config.transports,
            ctx: config.ctx,
            own_connection_key: config.own_connection_key,
            own_reconnect_budget: config.own_reconnect_budget,
            sfu_fallback: config.sfu_fallback,
        });
        let tracks = pool.remote_tracks();
        // Present from the first value: our own SFU before our tile exists.
        let (unverified_tx, unverified_rx) = watch::channel(Vec::new());
        let (interruptions_tx, interruptions_rx) = watch::channel(Interruptions {
            own_media: pool.own_connection_state(),
            ..Interruptions::default()
        });

        let actor = Actor {
            pool,
            own_connection_key,
            media_status: config.media_status,
            own_member_id: config.own_member_id,
            events_tx: events_tx.clone(),
            participants_tx,
            tiles_tx,
            local_tx,
            interruptions_tx,
            unverified_tx,
            signalling: SignallingHealth::Reachable,
            core_lapsed: false,
            observed_lapse: false,
            window: DetailWindow::default(),
            ranking_threshold: 0,
            speaking: HashSet::new(),
            speakers: HashMap::new(),
            last_order: Vec::new(),
            pending_reorder: None,
            reorder_generation: 0,
            stability: config.stability,
            messages_tx: messages_tx.clone(),
            roster: Vec::new(),
            local_tracks: HashMap::new(),
            encryption_states: HashMap::new(),
            undecryptable_since: HashMap::new(),
            unmapped_since: HashMap::new(),
            host_constraints: HashMap::new(),
            sent_in_clear: HashSet::new(),
            installed_keys: HashMap::new(),
            raised_hands: Vec::new(),
            arrived: false,
            ended: false,
        };
        let task = rt::spawn(actor.run(
            memberships,
            messages_rx,
            pool_inbox,
            ActorInputs {
                raised_hands: config.raised_hands,
                reactions: config.reactions,
                ended: config.ended,
                signalling: config.signalling,
                lapsed: config.own_membership_lapsed,
            },
        ));

        CallEngine {
            messages: messages_tx,
            events_tx,
            first_events_rx: Mutex::new(Some(first_events_rx)),
            participants_rx,
            tiles_rx,
            local_rx,
            interruptions_rx,
            unverified_rx,
            tracks,
            task,
        }
    }

    /// A cloneable handle for feeding the engine from other components.
    pub fn handle(&self) -> EngineHandle {
        EngineHandle {
            messages: self.messages.clone(),
        }
    }

    /// Subscribe to the unified call event stream.
    ///
    /// The first subscriber sees every event since the engine was created
    /// (up to the channel capacity, past which it reads `Lagged`); later ones
    /// see events from their subscription on. Hosts emit events before they
    /// get to subscribe — the held-key replay surfaces `KeyImported`s while
    /// media is still connecting — so a plain broadcast subscription would
    /// make each host's correctness depend on its call order.
    pub fn subscribe_events(&self) -> broadcast::Receiver<CallEvent> {
        self.first_events_rx
            .lock()
            .expect("engine first-receiver mutex poisoned")
            .take()
            .unwrap_or_else(|| self.events_tx.subscribe())
    }

    /// The current participant roster.
    pub fn participants(&self) -> Vec<Participant> {
        self.participants_rx.borrow().clone()
    }

    /// Watch the participant roster; the receiver always holds the latest
    /// snapshot.
    pub fn subscribe_participants(&self) -> watch::Receiver<Vec<Participant>> {
        self.participants_rx.clone()
    }

    /// The tile roster: every remote tile in rank order, with full records for
    /// the declared detail window. See [`TileRoster`].
    pub fn tiles(&self) -> TileRoster {
        self.tiles_rx.borrow().clone()
    }

    /// Watch the tile roster; the receiver always holds the latest snapshot.
    pub fn subscribe_tiles(&self) -> watch::Receiver<TileRoster> {
        self.tiles_rx.clone()
    }

    /// Our own tile and screen-sharing state, beside the roster. `None` until
    /// our own membership is on the roster.
    pub fn local_state(&self) -> Option<LocalState> {
        self.local_rx.borrow().clone()
    }

    /// Watch our local state.
    pub fn subscribe_local_state(&self) -> watch::Receiver<Option<LocalState>> {
        self.local_rx.clone()
    }

    /// What is interrupting the call now, cause by cause (034 R38).
    pub fn interruptions(&self) -> Interruptions {
        self.interruptions_rx.borrow().clone()
    }

    /// Watch [`Self::interruptions`]. For any one change it moves after the
    /// participants, tiles and local state, so a reader woken by it reads
    /// those already agreeing with it.
    pub fn subscribe_interruptions(&self) -> watch::Receiver<Interruptions> {
        self.interruptions_rx.clone()
    }

    /// Participants on an SFU whose membership we have not received, by
    /// transport identity, sorted (spec 038 R25): each one unmapped for
    /// [`UNVERIFIED_GRACE`] while our signalling is healthy. Kept apart from
    /// [`Self::interruptions`], which a healthy call leaves empty; while
    /// signalling is not healthy, they are an interruption cause instead
    /// (034 R16) and this is empty.
    pub fn unverified_participants(&self) -> Vec<String> {
        self.unverified_rx.borrow().clone()
    }

    /// Watch [`Self::unverified_participants`].
    pub fn subscribe_unverified_participants(&self) -> watch::Receiver<Vec<String>> {
        self.unverified_rx.clone()
    }

    /// Declare which tiles get full records: ranks `[offset, offset + len)`
    /// plus `also`, wherever those rank. The default is everything. Declare
    /// what you compose, not what is visible.
    pub fn set_detail_window(&self, offset: u32, len: u32, also: impl IntoIterator<Item = TileId>) {
        let _ = self
            .messages
            .send(ActorMessage::SetDetailWindow(DetailWindow {
                offset,
                len,
                also: also.into_iter().collect(),
            }));
    }

    /// Rank tiles by what members are doing only when there are more than
    /// `tiles` remote tiles; at or below it they keep join order, heroes
    /// first, so a small call does not shuffle. A hint, not a layout
    /// contract: what fits on screen is a good value (1 in spotlight).
    /// Default 0: always rank.
    pub fn set_ranking_threshold(&self, tiles: u32) {
        let _ = self.messages.send(ActorMessage::SetRankingThreshold(tiles));
    }

    /// Hand the engine our own focus's handle before it is connected: the
    /// engine connects it, and reconnects it whenever it is lost, within the
    /// reconnect budget (034 R24). The call status stays connecting until it
    /// is first up; publications made meanwhile go out once it is.
    pub fn connect_own(&self, connection: Box<dyn TransportConnection>) {
        let _ = self.messages.send(ActorMessage::ConnectOwn { connection });
    }

    /// Hand the caller-established own-focus connection to the engine, which
    /// consumes its events and owns its lifetime from here (except closing —
    /// the caller keeps that, so a clean leave can report the close result).
    pub fn adopt_own_connection(
        &self,
        connection: Box<dyn TransportConnection>,
        events: mpsc::UnboundedReceiver<ConnectionEvent>,
    ) {
        let _ = self
            .messages
            .send(ActorMessage::AdoptConnection { connection, events });
    }

    /// Report that a media decryption key for `identity` was imported, so the
    /// engine can surface [`CallEvent::KeyImported`] against the right member.
    pub fn notify_key_imported(&self, identity: impl Into<String>, key_index: u8) {
        let _ = self.messages.send(ActorMessage::KeyImported {
            identity: identity.into(),
            key_index,
        });
    }

    /// Report that a media key was refused, so the engine can surface
    /// [`CallEvent::KeyDiscarded`] with the reason.
    ///
    /// Keyed by `member_id` rather than a transport identity: the core refuses a
    /// key before any identity derivation applies to it, and a refused key may
    /// well name a member the transport has never seen.
    pub fn notify_key_discarded(&self, report: DiscardedKey) {
        let _ = self.messages.send(ActorMessage::KeyDiscarded(report));
    }

    /// Publish a local track on the own-focus connection (see
    /// [`PublishOptions`]); push captured frames into the returned handle.
    /// Fails while that connection is not up.
    pub async fn publish(
        &self,
        options: PublishOptions,
    ) -> Result<Arc<dyn LocalTrackHandle>, TransportError> {
        let (respond, response) = oneshot::channel();
        self.messages
            .send(ActorMessage::Publish { options, respond })
            .map_err(|_| TransportError::Closed("the engine is gone".into()))?;
        response
            .await
            .map_err(|_| TransportError::Closed("the engine is gone".into()))?
    }

    /// Mute or unmute one of our own publications.
    ///
    /// Peers are told, so their UI can show it — which is the difference
    /// between a muted sender and one that has simply stopped sending. The
    /// roster and the event stream are updated too, so a host can render its own
    /// state from the same source it renders everyone else's rather than
    /// shadowing it separately.
    ///
    /// Errors if nothing of that kind is published.
    pub async fn set_local_muted(
        &self,
        kind: MediaStreamKind,
        muted: bool,
    ) -> Result<(), TransportError> {
        let (respond, response) = oneshot::channel();
        self.messages
            .send(ActorMessage::SetLocalMuted {
                kind,
                muted,
                respond,
            })
            .map_err(|_| TransportError::Closed("the engine is gone".into()))?;
        response
            .await
            .map_err(|_| TransportError::Closed("the engine is gone".into()))?
    }

    /// Retract one of our own publications at the transport.
    ///
    /// Where a mute keeps the publication up so peers can tell a deliberate
    /// off from a wedged sender, unpublish removes it: peers drop the stream
    /// (their transport reports it removed) instead of rendering an empty
    /// tile — what a stopped screen share needs, since a screen has no "off"
    /// state worth showing. On success the own roster entry drops the stream
    /// and [`CallEvent::StreamStopped`] is emitted, and the handle obtained
    /// from [`CallEngine::publish`] is dead: `capture_*` calls on it error.
    ///
    /// On failure the publication stays on the roster and stays usable
    /// (mute, capture, retry this call): the retraction can fail while the
    /// room is live, and dropping local state then would leave a still-live
    /// publication nothing can retract. Errors if nothing of that kind is
    /// published.
    pub async fn unpublish(&self, kind: MediaStreamKind) -> Result<(), TransportError> {
        let (respond, response) = oneshot::channel();
        self.messages
            .send(ActorMessage::Unpublish { kind, respond })
            .map_err(|_| TransportError::Closed("the engine is gone".into()))?;
        response
            .await
            .map_err(|_| TransportError::Closed("the engine is gone".into()))?
    }

    /// Set the subscription constraints for one stream of one participant.
    ///
    /// Applied after a short debounce (rapid changes coalesce, e.g. while
    /// scrolling a participant grid) and re-applied automatically whenever
    /// the stream (re)appears. Constraints are keyed by membership: they die
    /// when the member leaves.
    pub fn set_constraints(
        &self,
        member_id: impl Into<String>,
        kind: MediaStreamKind,
        constraints: MediaConstraints,
    ) {
        let _ = self.messages.send(ActorMessage::SetConstraints {
            member_id: member_id.into(),
            kind,
            constraints,
        });
    }

    /// The handle for a participant's subscribed stream, to open frame
    /// streams from. `None` while no such stream is up (see
    /// [`CallEvent::StreamStarted`] / [`CallEvent::StreamStopped`]).
    pub fn remote_track(
        &self,
        member_id: &str,
        kind: MediaStreamKind,
    ) -> Option<Arc<dyn RemoteTrackHandle>> {
        self.tracks.get(member_id, kind)
    }

    /// Receive-side RTP counters for a participant's stream.
    ///
    /// `None` while no such stream is subscribed, or when the transport
    /// reports no counters. Because the receive path emits frames whether or
    /// not RTP arrives, this is how a caller distinguishes "nothing is
    /// arriving" from "arriving but not decoding" — sample twice and compare,
    /// the fields are cumulative totals. See [`ReceiveStats`].
    pub async fn receive_stats(
        &self,
        member_id: &str,
        kind: MediaStreamKind,
    ) -> Option<ReceiveStats> {
        // `remote_track` hands back an owned `Arc`, so the track-map lock is
        // released before the transport's (potentially slow) stats round trip.
        let track = self.remote_track(member_id, kind)?;
        track.receive_stats().await
    }

    /// [`CallEngine::receive_stats`] for many streams at once.
    ///
    /// One entry per element of `streams`, in the same order — `None` under
    /// the same conditions as the single call. The track map is locked once,
    /// for the lookups only; the transports' stats round trips then run
    /// concurrently, so a batch costs about one collection rather than one
    /// per stream. Bound the request to what is drawn (contract C12) and call
    /// this once per sample.
    pub async fn receive_stats_for(
        &self,
        streams: &[(String, MediaStreamKind)],
    ) -> Vec<Option<ReceiveStats>> {
        if streams.is_empty() {
            return Vec::new();
        }
        let tracks = self.tracks.get_many(streams);
        // Unbounded on purpose: libwebrtc serialises stats collection on its
        // own thread. `futures_util::stream::iter(..).buffered(n)` is the
        // drop-in if a call size ever makes that a problem.
        futures_util::future::join_all(tracks.into_iter().map(|track| async move {
            match track {
                Some(track) => track.receive_stats().await,
                None => None,
            }
        }))
        .await
    }

    /// Close every pooled peer-focus connection, and report media detached to
    /// the call's status. The adopted own-focus connection is not closed here — its owner (the
    /// caller of [`CallEngine::adopt_own_connection`]) closes it and gets the
    /// result. Resolves once the engine has processed the shutdown.
    pub async fn shutdown(&self) {
        let (ack, done) = oneshot::channel();
        if self.messages.send(ActorMessage::Shutdown { ack }).is_ok() {
            let _ = done.await;
        }
    }
}

struct Actor {
    /// The call's media connections, and the identity mapping over them.
    pool: MediaPool,
    /// The focus we publish on, if any.
    own_connection_key: Option<String>,
    media_status: Option<MediaStatusReporter>,
    own_member_id: String,
    events_tx: broadcast::Sender<CallEvent>,
    participants_tx: watch::Sender<Vec<Participant>>,
    tiles_tx: watch::Sender<TileRoster>,
    local_tx: watch::Sender<Option<LocalState>>,
    interruptions_tx: watch::Sender<Interruptions>,
    /// See [`CallEngine::unverified_participants`].
    unverified_tx: watch::Sender<Vec<String>>,
    /// Copied into the interruptions record.
    signalling: SignallingHealth,
    /// The core's prediction that our membership has lapsed (034 R17).
    core_lapsed: bool,
    /// A snapshot without us arrived after we had arrived, while the call is
    /// live: the lapse observed, which beats the prediction.
    observed_lapse: bool,
    /// Which tiles get full records. Defaults to everything.
    window: DetailWindow,
    /// At or below this many remote tiles, activity does not reorder them.
    /// 0 always ranks.
    ranking_threshold: u32,
    /// Members currently counted as speaking for the order — the damped set
    /// the ranking reads (R10). `speakers` holds the raw state driving it,
    /// which is what a tile's own `speaking` flag reports (R11).
    speaking: HashSet<String>,
    /// Raw speaking state per member and its in-flight timer's generation.
    speakers: HashMap<String, SpeakerState>,
    /// The tile order consumers were last given (R11). Departed ids leave it
    /// at once; the new order replaces it when the coalesce window closes.
    last_order: Vec<TileId>,
    /// Generation of the open coalesce window, if one is open.
    pending_reorder: Option<u64>,
    reorder_generation: u64,
    stability: StabilityConfig,
    /// Handed to publishes and timers so everything funnels into the same
    /// mailbox.
    messages_tx: mpsc::UnboundedSender<ActorMessage>,
    roster: Vec<Participant>,
    /// Last frame-encryption state reported per member.
    ///
    /// Only so the log line can name the transition. A bare "is MissingKey"
    /// cannot be read: a momentary drop during a rotation and a permanent failure
    /// produce the same line, and only the previous state separates them.
    encryption_states: HashMap<String, FrameEncryptionState>,
    /// Since when each member's frames have failed without a break; shown
    /// unable to decrypt once that is `stability.key_grace` old.
    undecryptable_since: HashMap<String, rt::Instant>,
    /// When each transport identity mapping to no membership was first seen
    /// so (spec 038 R25).
    unmapped_since: HashMap<String, rt::Instant>,
    /// The constraints the host last set per stream, before
    /// [`probe_undecryptable`] adjusts them for a member we cannot decrypt.
    host_constraints: HashMap<(String, MediaStreamKind), MediaConstraints>,
    /// Members who published a track declared unencrypted, which the
    /// transport refused (034 C6); until they leave.
    sent_in_clear: HashSet<String>,
    /// Our own live publications, so a mute can reach the transport and the
    /// roster can be corrected in one place, and so a membership landing after
    /// them can be seeded from them.
    local_tracks: HashMap<MediaStreamKind, LocalPublication>,
    /// Key indices installed per member, in the order they were imported.
    ///
    /// Kept so a frame-encryption failure can say whether *any* key reached this
    /// participant. Without it a `MissingKey` is unattributable from outside: a
    /// key that never arrived and a rotation still in flight look identical.
    installed_keys: HashMap<String, Vec<u8>>,
    /// The raised hands as the core last reported them, oldest first: its
    /// order is the queue that [`Participant::hand_raised_position`] numbers.
    /// Kept for members not yet on the roster too: the core learns of a hand
    /// and of the membership it belongs to through different channels, and a
    /// hand that arrives first must not be lost.
    raised_hands: Vec<RaisedHand>,
    /// Whether a snapshot has held our own membership yet. The roster we
    /// find on arrival was there before us and is not news; every change
    /// after it is, through a lapse of our own membership too (034 R16): a
    /// member who joined while our homeserver was unreachable arrives in the
    /// catch-up set and is announced then. Our own entry is never announced.
    arrived: bool,
    ended: bool,
}

/// What the actor follows besides the memberships and its own mailboxes,
/// each optional (see [`EngineConfig`]).
struct ActorInputs {
    raised_hands: Option<watch::Receiver<Vec<RaisedHand>>>,
    reactions: Option<broadcast::Receiver<ReceivedReaction>>,
    ended: Option<watch::Receiver<Option<LeaveReason>>>,
    signalling: Option<watch::Receiver<SignallingHealth>>,
    lapsed: Option<watch::Receiver<bool>>,
}

/// One of our own live publications.
///
/// The mute state is kept here rather than read back from the handle:
/// [`LocalTrackHandle`] cannot be asked, and it is needed to seed our roster
/// entry when our membership lands after the publish.
struct LocalPublication {
    track: Arc<dyn LocalTrackHandle>,
    /// The user's mute: what the roster shows.
    muted: bool,
    /// What the transport was last told: the user's mute, or the lapse's
    /// pause on top of it (034 R17).
    applied_muted: bool,
}

/// Waits for the raised-hands watch to change; never resolves when there is
/// no such channel, so the `select!` branch simply never fires.
async fn hands_changed(
    hands: &mut Option<watch::Receiver<Vec<RaisedHand>>>,
) -> Result<(), watch::error::RecvError> {
    match hands {
        Some(hands) => hands.changed().await,
        None => std::future::pending().await,
    }
}

/// The next value of an optional watch; `None` once its sender is gone.
/// Never resolves without a receiver.
async fn watch_changed<T: Clone>(receiver: &mut Option<watch::Receiver<T>>) -> Option<T> {
    let Some(receiver) = receiver else {
        return std::future::pending().await;
    };
    receiver.changed().await.ok()?;
    Some(receiver.borrow_and_update().clone())
}

/// Resolves once the participation has ended; `None` once the channel closes.
/// Never resolves when there is no channel.
async fn participation_ended(
    ended: &mut Option<watch::Receiver<Option<LeaveReason>>>,
) -> Option<LeaveReason> {
    let Some(ended) = ended else {
        return std::future::pending().await;
    };
    let reason = ended.wait_for(Option::is_some).await.ok()?;
    reason.clone()
}

/// The next reaction, skipping over any the receiver lagged past; `None`
/// once the channel is closed. Never resolves when there is no channel.
async fn next_reaction(
    reactions: &mut Option<broadcast::Receiver<ReceivedReaction>>,
) -> Option<ReceivedReaction> {
    let Some(reactions) = reactions else {
        return std::future::pending().await;
    };
    loop {
        match reactions.recv().await {
            Ok(reaction) => return Some(reaction),
            // A reaction is a three-second visual; the ones we fell behind on
            // are not worth showing late.
            Err(broadcast::error::RecvError::Lagged(skipped)) => {
                log::debug!("skipped {skipped} reaction(s) the engine fell behind on");
            }
            Err(broadcast::error::RecvError::Closed) => return None,
        }
    }
}

impl Actor {
    async fn run(
        mut self,
        mut memberships: watch::Receiver<Vec<JoinedMembership>>,
        mut messages: mpsc::UnboundedReceiver<ActorMessage>,
        mut pool_inbox: mpsc::UnboundedReceiver<PoolMessage>,
        inputs: ActorInputs,
    ) {
        let ActorInputs {
            mut raised_hands,
            mut reactions,
            mut ended,
            mut signalling,
            mut lapsed,
        } = inputs;
        // The watch channels always have a value; start from it.
        if let Some(signalling) = signalling.as_mut() {
            let health = *signalling.borrow_and_update();
            self.set_signalling(health);
        }
        if let Some(lapsed) = lapsed.as_mut() {
            self.core_lapsed = *lapsed.borrow_and_update();
        }
        let initial = memberships.borrow_and_update().clone();
        self.apply_snapshot(initial);
        if let Some(hands) = raised_hands.as_mut() {
            let initial = hands.borrow_and_update().clone();
            self.apply_raised_hands(initial, false);
        }

        self.sync_media_state();
        let mut memberships_open = true;
        loop {
            tokio::select! {
                changed = watch_changed(&mut signalling) => match changed {
                    Some(health) => self.set_signalling(health),
                    None => signalling = None,
                },
                changed = watch_changed(&mut lapsed) => match changed {
                    Some(core_lapsed) => self.core_lapsed = core_lapsed,
                    None => lapsed = None,
                },
                changed = memberships.changed(), if memberships_open => match changed {
                    Ok(()) => {
                        let snapshot = memberships.borrow_and_update().clone();
                        self.apply_snapshot(snapshot);
                    }
                    // The core session is gone; media may outlive it briefly,
                    // so keep serving connection events.
                    Err(_) => memberships_open = false,
                },
                changed = hands_changed(&mut raised_hands) => match changed {
                    Ok(()) => {
                        let snapshot = raised_hands
                            .as_mut()
                            .map(|hands| hands.borrow_and_update().clone())
                            .unwrap_or_default();
                        self.apply_raised_hands(snapshot, true);
                    }
                    Err(_) => raised_hands = None,
                },
                reaction = next_reaction(&mut reactions) => match reaction {
                    Some(reaction) => self.apply_reaction(reaction),
                    None => reactions = None,
                },
                left = participation_ended(&mut ended) => {
                    ended = None;
                    if let Some(reason) = left {
                        self.end_on_leave(reason).await;
                    }
                }
                // Never closes: the pool holds a sender for its own timers.
                Some(message) = pool_inbox.recv() => {
                    let events = self.pool.handle(message);
                    self.apply_pool_events(events);
                }
                message = messages.recv() => match message {
                    Some(ActorMessage::Shutdown { ack }) => {
                        if let Some(status) = &self.media_status {
                            status.detached();
                        }
                        self.end();
                        let _ = ack.send(());
                        break;
                    }
                    Some(message) => self.handle_message(message),
                    // All senders gone: the engine handle was dropped.
                    None => break,
                },
            }
            self.sync_media_state();
        }
    }

    // ---- interruptions -----------------------------------------------------

    /// Whether our membership has lapsed, predicted or observed (034 R17).
    fn lapsed(&self) -> bool {
        (self.core_lapsed || self.observed_lapse) && !self.ended
    }

    /// Brings what every change can move into line, once per actor turn: each
    /// roster entry's connection and decryptability, the pause of our
    /// publications, and last the interruptions record, so it never runs
    /// ahead of the roster and tiles it describes (034 R38).
    fn sync_media_state(&mut self) {
        let own_media = self.pool.own_connection_state();
        let mut changed = false;
        for participant in &mut self.roster {
            let (connection, unable_to_decrypt) = if participant.is_local {
                (own_media, None)
            } else {
                (
                    self.pool
                        .member_connection(&participant.member_id)
                        .unwrap_or_default(),
                    unable_to_decrypt(
                        &participant.member_id,
                        !participant.streams.is_empty(),
                        self.stability.key_grace,
                        &self.sent_in_clear,
                        &self.undecryptable_since,
                        &self.encryption_states,
                    ),
                )
            };
            if participant.connection != connection
                || participant.unable_to_decrypt != unable_to_decrypt
            {
                participant.connection = connection;
                participant.unable_to_decrypt = unable_to_decrypt;
                changed = true;
            }
        }
        if changed {
            self.publish_roster();
        }
        self.apply_pause();

        let remote = || self.roster.iter().filter(|p| !p.is_local);
        let mut record = Interruptions {
            signalling: self.signalling,
            own_membership_lapsed: self.lapsed(),
            own_media,
            members_reconnecting: remote()
                .filter(|p| p.connection == ConnectionState::Reconnecting)
                .map(|p| p.member_id.clone())
                .collect(),
            members_unreachable: remote()
                .filter(|p| p.connection == ConnectionState::Unreachable)
                .map(|p| p.member_id.clone())
                .collect(),
            unable_to_decrypt: remote()
                .filter(|p| p.unable_to_decrypt.is_some())
                .map(|p| p.member_id.clone())
                .collect(),
            unmappable: Vec::new(),
        };
        let unverified = self.split_unmapped(&mut record);
        record.members_reconnecting.sort();
        record.members_unreachable.sort();
        record.unable_to_decrypt.sort();
        self.interruptions_tx.send_if_modified(|current| {
            if *current == record {
                return false;
            }
            log::info!("interruptions: {record:?}");
            *current = record;
            true
        });
        self.unverified_tx.send_if_modified(|current| {
            if *current == unverified {
                return false;
            }
            log::info!("unverified participants: {unverified:?}");
            *current = unverified;
            true
        });
    }

    /// Splits the participants mapping to no membership (spec 038 R25):
    /// while signalling is healthy, those unmapped for [`UNVERIFIED_GRACE`]
    /// are returned as unverified and the interruption record holds none;
    /// otherwise every one is an interruption cause (034 R16) and none is
    /// unverified. Arms a wake-up for each newly unmapped participant.
    fn split_unmapped(&mut self, record: &mut Interruptions) -> Vec<String> {
        let unmapped = self.pool.unmappable();
        let now = rt::Instant::now();
        self.unmapped_since
            .retain(|identity, _| unmapped.contains(identity));
        for identity in &unmapped {
            if !self.unmapped_since.contains_key(identity) {
                self.unmapped_since.insert(identity.clone(), now);
                let messages = self.messages_tx.clone();
                rt::spawn(async move {
                    rt::sleep(UNVERIFIED_GRACE).await;
                    let _ = messages.send(ActorMessage::UnverifiedGrace);
                });
            }
        }
        if self.signalling != SignallingHealth::Reachable {
            record.unmappable = unmapped;
            return Vec::new();
        }
        unmapped
            .into_iter()
            .filter(|identity| {
                self.unmapped_since
                    .get(identity)
                    .is_some_and(|since| now.duration_since(*since) >= UNVERIFIED_GRACE)
            })
            .collect()
    }

    /// While our membership has lapsed, every publication is muted at the
    /// transport, peers having dropped us: the user's own mute is kept apart
    /// in `LocalPublication::muted`, so the roster never moves and a track
    /// comes back unmuted only if the user had not muted it (034 R17).
    fn apply_pause(&mut self) {
        let paused = self.lapsed();
        for (kind, publication) in &mut self.local_tracks {
            let effective = publication.muted || paused;
            if publication.applied_muted == effective {
                continue;
            }
            match publication.track.set_muted(effective) {
                Ok(()) => {
                    log::info!(
                        "{kind:?} {} at the transport ({})",
                        if effective { "paused" } else { "resumed" },
                        if paused {
                            "our membership lapsed"
                        } else {
                            "our membership is back"
                        },
                    );
                    publication.applied_muted = effective;
                }
                Err(error) => log::warn!("could not pause/resume {kind:?}: {error}"),
            }
        }
    }

    fn handle_message(&mut self, message: ActorMessage) {
        match message {
            ActorMessage::AdoptConnection { connection, events } => {
                self.pool.adopt_own_connection(connection, events);
            }
            ActorMessage::ConnectOwn { connection } => {
                let events = self.pool.connect_own(connection);
                self.apply_pool_events(events);
            }
            ActorMessage::KeyImported {
                identity,
                key_index,
            } => {
                let events = self.pool.key_imported(identity, key_index);
                self.apply_pool_events(events);
            }
            ActorMessage::KeyDiscarded(report) => {
                log::warn!(
                    "media key index {:?} for member {} refused: {} (from {}/{})",
                    report.key_index,
                    report.member_id,
                    report.reason,
                    report.sender_user_id.as_deref().unwrap_or("<unattributed>"),
                    report.sender_device_id.as_deref().unwrap_or("<unknown>"),
                );
                self.emit(CallEvent::KeyDiscarded {
                    member_id: report.member_id,
                    key_index: report.key_index,
                    sender_user_id: report.sender_user_id,
                    sender_device_id: report.sender_device_id,
                    reason: report.reason,
                });
            }
            ActorMessage::Publish { options, respond } => {
                // Local tracks always go to the focus we announced in our
                // membership — that is where peers subscribe to us. While it
                // is down the handle takes them, and they go out once it is up.
                match self.pool.own_handle() {
                    Some(connection) => {
                        // The publish itself is awaited off-actor so a slow SFU
                        // does not stall every other message; the actor learns
                        // the outcome through `LocalPublished`, which is what
                        // puts the stream on our own roster entry. Without that
                        // a host sees no event for its own microphone and has to
                        // shadow the state to render itself truthfully.
                        let kind = options.kind;
                        let muted = options.muted;
                        let messages = self.messages_tx.clone();
                        rt::spawn(async move {
                            let outcome = connection.publish(options).await;
                            if let Ok(track) = &outcome {
                                let _ = messages.send(ActorMessage::LocalPublished {
                                    kind,
                                    muted,
                                    track: Arc::clone(track),
                                });
                            }
                            let _ = respond.send(outcome);
                        });
                    }
                    None => {
                        let _ = respond.send(Err(TransportError::Closed(
                            "the own-focus connection is not up".into(),
                        )));
                    }
                }
            }
            ActorMessage::LocalPublished { kind, muted, track } => {
                self.local_tracks.insert(
                    kind,
                    LocalPublication {
                        track,
                        muted,
                        applied_muted: muted,
                    },
                );
                let own_member_id = self.own_member_id.clone();
                self.add_local_stream(&own_member_id, kind);
            }
            ActorMessage::SetLocalMuted {
                kind,
                muted,
                respond,
            } => {
                // A mute set while paused holds, and only reaches the
                // transport once the pause would not override it (034 R17).
                let paused = self.lapsed();
                let outcome = match self.local_tracks.get(&kind) {
                    Some(publication) => publication.track.set_muted(muted || paused),
                    None => Err(TransportError::Unsupported(format!(
                        "nothing of kind {kind:?} is published locally"
                    ))),
                };
                if outcome.is_ok() {
                    // Remembered, not just pushed at the roster: peers have
                    // been told, and our roster entry may not exist yet (or may
                    // be rebuilt later), in which case this is the only record
                    // `add_member` can seed it from.
                    if let Some(publication) = self.local_tracks.get_mut(&kind) {
                        publication.muted = muted;
                        publication.applied_muted = muted || paused;
                    }
                    let own_member_id = self.own_member_id.clone();
                    self.set_member_muted(&own_member_id, kind, muted);
                }
                let _ = respond.send(outcome);
            }
            ActorMessage::Unpublish { kind, respond } => {
                let Some(publication) = self.local_tracks.get(&kind) else {
                    let _ = respond.send(Err(TransportError::Unsupported(format!(
                        "nothing of kind {kind:?} is published locally"
                    ))));
                    return;
                };
                // Awaited off-actor like the publish itself, and the actor
                // learns the outcome through `LocalUnpublished`: a retraction
                // can fail while the room is live (the room-closed case comes
                // back as success from the transport), and clearing local
                // state on a failed attempt would leave a still-live
                // publication that nothing can mute or retract anymore.
                let track = Arc::clone(&publication.track);
                let messages = self.messages_tx.clone();
                rt::spawn(async move {
                    let outcome = track.unpublish().await;
                    if outcome.is_ok() {
                        let _ = messages.send(ActorMessage::LocalUnpublished { kind, track });
                    }
                    let _ = respond.send(outcome);
                });
            }
            ActorMessage::LocalUnpublished { kind, track } => {
                if self
                    .local_tracks
                    .get(&kind)
                    .is_some_and(|current| Arc::ptr_eq(&current.track, &track))
                {
                    self.local_tracks.remove(&kind);
                    let own_member_id = self.own_member_id.clone();
                    self.remove_stream(&own_member_id, kind);
                }
            }
            ActorMessage::SetConstraints {
                member_id,
                kind,
                constraints,
            } => {
                self.host_constraints
                    .insert((member_id.clone(), kind), constraints);
                let undecryptable = self.is_undecryptable(&member_id);
                self.pool.set_constraints(
                    member_id,
                    kind,
                    probe_undecryptable(kind, constraints, undecryptable),
                );
            }
            ActorMessage::SpeakerTimer {
                member_id,
                generation,
            } => {
                // Only the newest timer applies; an older one was superseded.
                let raw = match self.speakers.get(&member_id) {
                    Some(state) if state.generation == generation => state.raw,
                    _ => return,
                };
                let changed = if raw {
                    self.speaking.insert(member_id)
                } else {
                    self.speaking.remove(&member_id)
                };
                if changed {
                    self.publish_tiles();
                }
            }
            // Nothing to do here: the sync after every message applies it.
            ActorMessage::KeyGrace | ActorMessage::UnverifiedGrace => {}
            ActorMessage::FlushReorder { generation } => {
                if self.pending_reorder != Some(generation) {
                    return;
                }
                self.pending_reorder = None;
                self.publish_tiles_reordered();
            }
            ActorMessage::SetDetailWindow(window) => {
                self.window = window;
                self.publish_tiles();
            }
            ActorMessage::SetRankingThreshold(threshold) => {
                self.ranking_threshold = threshold;
                self.publish_tiles();
            }
            // Handled in the run loop (it must break).
            ActorMessage::Shutdown { ack } => {
                let _ = ack.send(());
            }
        }
    }

    // ---- roster ----------------------------------------------------------

    fn apply_snapshot(&mut self, snapshot: Vec<JoinedMembership>) {
        // Announced once we have arrived, and never after the call ended for
        // us (see `arrived`).
        let announce = self.arrived && !self.ended;
        let with_us = snapshot
            .iter()
            .any(|member| member.member_id == self.own_member_id);
        self.arrived |= with_us;
        self.observed_lapse = self.arrived && !with_us;
        let own_member_id = self.own_member_id.clone();
        let announce_for =
            |member_id: &str, count: u32| (announce && member_id != own_member_id).then_some(count);

        // Departures first, so a member_id rejoining in the same snapshot
        // (new membership, same key space) starts clean.
        let current_ids: Vec<String> = snapshot
            .iter()
            .map(|member| member.member_id.clone())
            .collect();
        // The roster becomes exactly the snapshot's members, so this is the
        // count after the change, whichever event carries it.
        let participant_count = current_ids.iter().collect::<HashSet<_>>().len() as u32;
        let departed: Vec<Participant> = self
            .roster
            .iter()
            .filter(|participant| !current_ids.contains(&participant.member_id))
            .cloned()
            .collect();
        for participant in departed {
            self.remove_member(
                &participant.member_id,
                announce_for(&participant.member_id, participant_count),
            );
        }

        for member in &snapshot {
            if self.roster.iter().any(|p| p.member_id == member.member_id) {
                continue;
            }
            self.add_member(member, announce_for(&member.member_id, participant_count));
        }

        // After the roster, so media the pool releases for a member who just
        // joined (buffered tracks and keys) finds their entry.
        let events = self.pool.apply_snapshot(&snapshot);
        self.apply_pool_events(events);
        self.refresh_hand_positions();
        self.publish_roster();

        log::debug!("roster reconciled: {} participant(s)", self.roster.len());
    }

    /// Signalling health, kept for the interruptions record and told to the
    /// pool, whose fallback needs it (034 R25).
    fn set_signalling(&mut self, health: SignallingHealth) {
        self.signalling = health;
        let events = self
            .pool
            .set_signalling_reachable(health == SignallingHealth::Reachable);
        self.apply_pool_events(events);
    }

    /// Put what the pool reports onto the roster and the event stream.
    fn apply_pool_events(&mut self, events: Vec<PoolEvent>) {
        for event in events {
            match event {
                PoolEvent::StreamAdded { member_id, kind } => self.add_stream(&member_id, kind),
                PoolEvent::StreamRemoved { member_id, kind } => {
                    self.remove_stream(&member_id, kind);
                }
                PoolEvent::StreamMuted {
                    member_id,
                    kind,
                    muted,
                } => self.set_member_muted(&member_id, kind, muted),
                PoolEvent::ActiveSpeakers { speakers } => {
                    let speakers: Vec<SpeakingMember> = speakers
                        .into_iter()
                        .map(|speaker| SpeakingMember {
                            member_id: speaker.member_id,
                            level: speaker.level,
                        })
                        .collect();
                    // Not sampled here: the SFU decides who is speaking and
                    // sends one update for the whole call per
                    // `update_interval` (LiveKit default 500 ms), smoothed over
                    // two intervals. That bounds how often the raw set — and
                    // with it the tile flag and the roster — can change.
                    //
                    // Timers are armed before the event goes out, so a consumer
                    // woken by it finds the hysteresis already running.
                    let raw: HashSet<String> =
                        speakers.iter().map(|s| s.member_id.clone()).collect();
                    self.apply_raw_speakers(raw);
                    self.emit(CallEvent::ActiveSpeakers { speakers });
                }
                PoolEvent::EncryptionStateChanged { member_id, state } => {
                    self.encryption_state_changed(member_id, state);
                }
                PoolEvent::SentInClear { member_id } => {
                    log::warn!("{member_id} sent media in clear; it is not played");
                    self.sent_in_clear.insert(member_id);
                }
                PoolEvent::KeyImported {
                    member_id,
                    identity,
                    key_index,
                } => {
                    self.record_installed_key(&member_id, key_index);
                    self.emit(CallEvent::KeyImported {
                        member_id,
                        identity,
                        key_index,
                    });
                }
                PoolEvent::UnknownParticipant { identity } => {
                    self.emit(CallEvent::UnknownParticipant { identity });
                }
                // Read back per member by `sync_media_state` at the end of
                // the turn.
                PoolEvent::ConnectionState {
                    connection_key,
                    state,
                } => {
                    log::info!("media connection {connection_key} is {state:?}");
                    // Our own focus up: the call's media is (034 R24).
                    if state == ConnectionState::Connected
                        && self.own_connection_key.as_deref() == Some(connection_key.as_str())
                        && let Some(status) = &self.media_status
                    {
                        status.connected();
                    }
                }
                PoolEvent::Degraded(degraded) => {
                    if let Some(status) = &self.media_status {
                        status.degraded(degraded);
                    }
                }
                PoolEvent::OwnConnectionLost { reason, message } => {
                    if let Some(status) = &self.media_status {
                        status.lost(
                            match reason {
                                OwnConnectionLoss::Budget => ConnectionLossReason::Budget,
                                OwnConnectionLoss::Refused => ConnectionLossReason::Refused,
                                OwnConnectionLoss::NeverConnected => {
                                    ConnectionLossReason::NeverConnected
                                }
                                OwnConnectionLoss::TokenEndpointUnsupported => {
                                    ConnectionLossReason::TokenEndpointUnsupported
                                }
                            },
                            message,
                        );
                    }
                    self.end();
                }
                PoolEvent::OwnFocusMoved {
                    connection_key,
                    transport,
                } => {
                    log::info!("our media moved to {connection_key}");
                    self.own_connection_key = Some(connection_key);
                    if let Some(status) = &self.media_status {
                        status.moved(transport);
                    }
                }
            }
        }
    }

    fn is_undecryptable(&self, member_id: &str) -> bool {
        self.encryption_states
            .get(member_id)
            .is_some_and(FrameEncryptionState::is_failure)
    }

    fn encryption_state_changed(&mut self, member_id: String, state: FrameEncryptionState) {
        let diagnostic = self.encryption_diagnostic(&member_id, state);
        let previous = self.encryption_states.insert(member_id.clone(), state);
        // Crossing between decrypting and not changes what the member's video
        // streams are asked for (see `probe_undecryptable`).
        if previous.is_some_and(|p| p.is_failure()) != state.is_failure() {
            let streams: Vec<_> = self
                .host_constraints
                .iter()
                .filter(|((member, _), _)| *member == member_id)
                .map(|((_, kind), constraints)| (*kind, *constraints))
                .collect();
            for (kind, constraints) in streams {
                self.pool.set_constraints(
                    member_id.clone(),
                    kind,
                    probe_undecryptable(kind, constraints, state.is_failure()),
                );
            }
        }
        if !state.is_failure() {
            self.undecryptable_since.remove(&member_id);
        } else if !self.undecryptable_since.contains_key(&member_id) {
            self.undecryptable_since
                .insert(member_id.clone(), rt::Instant::now());
            let messages = self.messages_tx.clone();
            let grace = self.stability.key_grace;
            rt::spawn(async move {
                rt::sleep(grace).await;
                let _ = messages.send(ActorMessage::KeyGrace);
            });
        }
        if state.is_failure() {
            log::warn!(
                "frame encryption {state:?} for {member_id} (was {previous:?}), {diagnostic:?}"
            );
        } else {
            log::info!("frame encryption {state:?} for {member_id} (was {previous:?})");
        }
        self.emit(CallEvent::FrameEncryptionState {
            member_id,
            state,
            diagnostic,
        });
    }

    /// The streams `member_id` should start out with: for our own membership,
    /// our live publications and the mute state we last applied to them;
    /// nothing for anyone else, whose media attaches through `add_stream`.
    ///
    /// Sorted, because `local_tracks` is a `HashMap` and the events derived
    /// from this must come out in a stable order.
    fn own_published_streams(&self, member_id: &str) -> Vec<StreamState> {
        if member_id != self.own_member_id {
            return Vec::new();
        }
        let mut streams: Vec<StreamState> = self
            .local_tracks
            .iter()
            .map(|(kind, publication)| StreamState {
                kind: *kind,
                muted: publication.muted,
            })
            .collect();
        streams.sort_by_key(|stream| stream.kind);
        streams
    }

    /// `announce` carries the participant count when the join is news.
    fn add_member(&mut self, member: &JoinedMembership, announce: Option<u32>) {
        let reachable = self.pool.is_reachable(member);
        log::debug!(
            "member {} ({}) joined the roster, reachable={reachable}",
            member.member_id,
            member.sender,
        );

        // Seeded silently: a member arriving with their hand up did not just
        // raise it. The position is set once the roster is complete.
        let hand_raised_at_ms = self.hand_raised_at(&member.member_id);
        // Our own publications outlive any single membership: they can go up
        // before the first one lands (the SFU connection is adopted
        // independently of the snapshot) and they survive one being re-created.
        // This entry is built from scratch, so it has to be re-seeded from
        // them — nothing publishes again later to correct it, and a host would
        // render itself as camera-off while the track is live at the SFU.
        let own_streams = self.own_published_streams(&member.member_id);

        self.roster.push(Participant {
            member_id: member.member_id.clone(),
            user_id: member.sender.clone(),
            device_id: member.origin.sender_device_id().map(str::to_owned),
            is_local: member.member_id == self.own_member_id,
            // Settled by `sync_media_state` at the end of this turn, before
            // anything is read from the roster.
            connection: if reachable {
                ConnectionState::Connected
            } else {
                ConnectionState::Unreachable
            },
            unable_to_decrypt: None,
            streams: own_streams.clone(),
            hand_raised_at_ms,
            hand_raised_position: None,
            // Pinned here: a member is added once, so a sticky refresh's newer
            // `origin_server_ts` never moves them. `membership_ts` wins where
            // the dialect states one (the legacy `created_ts`).
            joined_at_ms: member.membership_ts.or(member.origin_server_ts),
        });
        if let Some(participant_count) = announce {
            self.emit(CallEvent::ParticipantJoined {
                member_id: member.member_id.clone(),
                user_id: member.sender.clone(),
                participant_count,
            });
        }
        // After `ParticipantJoined`, so a host sees the same order either way
        // round: the membership-first case emits the join, then the stream.
        for stream in own_streams {
            self.emit(CallEvent::StreamStarted {
                member_id: member.member_id.clone(),
                kind: stream.kind,
            });
            if stream.muted {
                self.emit(CallEvent::StreamMuted {
                    member_id: member.member_id.clone(),
                    kind: stream.kind,
                });
            }
        }
    }

    // ---- reactions --------------------------------------------------------

    /// Applies the core's raised-hand set: the roster entries and their queue
    /// positions change, and each hand that went up or down is announced when
    /// it is news (see [`CallEvent::HandRaised`]). Hands of members not on the
    /// roster yet are kept for `add_member`. `announce` is false for the set
    /// we attach to: those hands were up before us.
    fn apply_raised_hands(&mut self, hands: Vec<RaisedHand>, announce: bool) {
        self.raised_hands = hands;
        let announce = announce && self.arrived && !self.ended;

        let mut changed = false;
        let mut events = Vec::new();
        for participant in &mut self.roster {
            let hand = self
                .raised_hands
                .iter()
                .find(|hand| hand.member_id == participant.member_id);
            let now = hand.map(|hand| hand.raised_at_ms);
            if participant.hand_raised_at_ms == now {
                continue;
            }
            let was_up = participant.hand_raised_at_ms.is_some();
            participant.hand_raised_at_ms = now;
            changed = true;
            if !announce {
                continue;
            }
            match hand {
                // Only a hand seen going up: one found by the relations
                // backfill was already up when we looked. A hand already up
                // whose time merely moved is no news either.
                Some(hand) if !was_up && !hand.from_backfill => {
                    events.push(CallEvent::HandRaised {
                        member_id: participant.member_id.clone(),
                        raised_at_ms: hand.raised_at_ms,
                    });
                }
                Some(_) => {}
                None => events.push(CallEvent::HandLowered {
                    member_id: participant.member_id.clone(),
                }),
            }
        }
        if !changed {
            return;
        }
        self.refresh_hand_positions();
        // Roster first, events second. This actor runs on a worker thread while
        // a consumer may sit on another, and a consumer woken by `HandRaised`
        // reads `participants()` straight away — it must see the hand there.
        self.publish_roster();
        for event in events {
            self.emit(event);
        }
    }

    fn hand_raised_at(&self, member_id: &str) -> Option<u64> {
        self.raised_hands
            .iter()
            .find(|hand| hand.member_id == member_id)
            .map(|hand| hand.raised_at_ms)
    }

    /// Numbers the roster's raised hands 1, 2, … in the core's queue order,
    /// skipping hands of members not on the roster. O(roster + hands), on a
    /// membership snapshot or a hand change only.
    fn refresh_hand_positions(&mut self) {
        let index: HashMap<&str, usize> = self
            .roster
            .iter()
            .enumerate()
            .map(|(i, participant)| (participant.member_id.as_str(), i))
            .collect();
        let mut positions = vec![None; self.roster.len()];
        let mut next = 1;
        for hand in &self.raised_hands {
            if let Some(&i) = index.get(hand.member_id.as_str()) {
                positions[i] = Some(next);
                next += 1;
            }
        }
        for (participant, position) in self.roster.iter_mut().zip(positions) {
            participant.hand_raised_position = position;
        }
    }

    /// Reports a reaction from a member on the roster, while we are in the
    /// call. One from a member the roster does not hold is dropped: the core
    /// validated it against a membership, so this is a hand-off race, not a
    /// stranger.
    fn apply_reaction(&mut self, reaction: ReceivedReaction) {
        if !self.arrived || self.ended {
            log::debug!(
                "dropping reaction from {}: we are not in the call",
                reaction.member_id,
            );
            return;
        }
        if !self
            .roster
            .iter()
            .any(|participant| participant.member_id == reaction.member_id)
        {
            log::debug!(
                "dropping reaction from {} ({}): not on the roster yet",
                reaction.member_id,
                reaction.sender,
            );
            return;
        }
        self.emit(CallEvent::Reaction {
            member_id: reaction.member_id,
            emoji: reaction.emoji,
            name: reaction.name,
            sound: reaction.sound.asset_name().to_owned(),
        });
    }

    fn record_installed_key(&mut self, member_id: &str, key_index: u8) {
        let installed = self.installed_keys.entry(member_id.to_owned()).or_default();
        if !installed.contains(&key_index) {
            installed.push(key_index);
        }
    }

    /// What we can say about a frame-encryption state, from what we installed.
    fn encryption_diagnostic(
        &self,
        member_id: &str,
        state: FrameEncryptionState,
    ) -> FrameEncryptionDiagnostic {
        if !state.is_failure() {
            return FrameEncryptionDiagnostic::NotApplicable;
        }
        match self.installed_keys.get(member_id) {
            Some(indices) if !indices.is_empty() => FrameEncryptionDiagnostic::KeysInstalled {
                key_indices: indices.clone(),
            },
            _ => FrameEncryptionDiagnostic::NoKeyInstalled,
        }
    }

    /// `announce` carries the participant count when the departure is news.
    fn remove_member(&mut self, member_id: &str, announce: Option<u32>) {
        self.roster
            .retain(|participant| participant.member_id != member_id);
        // The pool forgets their identity, buffers, constraints and tracks
        // when it applies the same snapshot.
        self.installed_keys.remove(member_id);
        self.encryption_states.remove(member_id);
        self.undecryptable_since.remove(member_id);
        self.host_constraints
            .retain(|(member, _), _| member != member_id);
        self.sent_in_clear.remove(member_id);
        self.speaking.remove(member_id);
        self.speakers.remove(member_id);
        if let Some(participant_count) = announce {
            self.emit(CallEvent::ParticipantLeft {
                member_id: member_id.to_owned(),
                participant_count,
            });
        }
    }

    // ---- streams ----------------------------------------------------------

    /// Put one of our own publications on our roster entry.
    ///
    /// Deliberately not `add_stream`: there is no `RemoteTrackHandle` for our
    /// own media (nothing subscribes us to ourselves) and no constraints to
    /// push, so the two share only the roster bookkeeping.
    fn add_local_stream(&mut self, member_id: &str, kind: MediaStreamKind) {
        // The state the publication actually went up with, not `false`: a track
        // can be published already muted, and saying otherwise here would
        // contradict what the SFU was told in the same request.
        let muted = self
            .local_tracks
            .get(&kind)
            .is_some_and(|publication| publication.muted);
        let Some(participant) = self.roster.iter_mut().find(|p| p.member_id == member_id) else {
            // Our own membership has not come back through the sticky map yet.
            // The publication is live at the transport regardless, and it is
            // already in `local_tracks`, which `add_member` seeds our roster
            // entry from once the membership lands.
            log::debug!("published {kind:?} before our own membership is known");
            return;
        };
        if participant.streams.iter().any(|stream| stream.kind == kind) {
            return;
        }
        participant.streams.push(StreamState { kind, muted });
        self.emit(CallEvent::StreamStarted {
            member_id: member_id.to_owned(),
            kind,
        });
        if muted {
            self.emit(CallEvent::StreamMuted {
                member_id: member_id.to_owned(),
                kind,
            });
        }
        self.publish_roster();
    }

    /// A remote stream the pool subscribed; its handle is in the pool's
    /// [`RemoteTracks`].
    fn add_stream(&mut self, member_id: &str, kind: MediaStreamKind) {
        let Some(participant) = self.roster.iter_mut().find(|p| p.member_id == member_id) else {
            return;
        };
        if participant.streams.iter().any(|stream| stream.kind == kind) {
            // Already on the roster; not a new stream.
            return;
        }
        participant.streams.push(StreamState { kind, muted: false });
        self.emit(CallEvent::StreamStarted {
            member_id: member_id.to_owned(),
            kind,
        });
        self.publish_roster();
    }

    fn remove_stream(&mut self, member_id: &str, kind: MediaStreamKind) {
        let Some(participant) = self.roster.iter_mut().find(|p| p.member_id == member_id) else {
            return;
        };
        let had_stream = participant.streams.iter().any(|stream| stream.kind == kind);
        participant.streams.retain(|stream| stream.kind != kind);
        if had_stream {
            self.emit(CallEvent::StreamStopped {
                member_id: member_id.to_owned(),
                kind,
            });
            self.publish_roster();
        }
    }

    /// Mute state keyed by member rather than transport identity, so it serves
    /// our own publications too — those never arrive as transport events, since
    /// nothing subscribes us to ourselves.
    fn set_member_muted(&mut self, member_id: &str, kind: MediaStreamKind, muted: bool) {
        let Some(stream) = self
            .roster
            .iter_mut()
            .find(|p| p.member_id == member_id)
            .and_then(|p| p.streams.iter_mut().find(|stream| stream.kind == kind))
        else {
            return;
        };
        if stream.muted == muted {
            return;
        }
        stream.muted = muted;
        let member_id = member_id.to_owned();
        let event = if muted {
            CallEvent::StreamMuted { member_id, kind }
        } else {
            CallEvent::StreamUnmuted { member_id, kind }
        };
        self.emit(event);
        self.publish_roster();
    }

    // ---- end ---------------------------------------------------------------

    /// Close all pooled peer-focus connections, once. The adopted own-focus
    /// connection is never closed by the engine (its owner closes it; at this
    /// point it is usually already gone). Why the call ended is the call's
    /// status to say.
    fn end(&mut self) {
        if self.ended {
            return;
        }
        self.ended = true;
        self.pool.close();
    }

    /// Unlike a media-only shutdown, also closes the own-focus connection.
    /// Awaited, so a shutdown queued behind it finds it closed.
    async fn end_on_leave(&mut self, reason: LeaveReason) {
        if self.ended {
            return;
        }
        log::info!("call: the session ended ({:?}); ending media", reason.code);
        let own = self.pool.own_connection();
        self.end();
        if let Some(own) = own
            && let Err(error) = own.close().await
        {
            log::warn!("call: own focus did not close cleanly after the leave: {error}");
        }
    }

    fn emit(&self, event: CallEvent) {
        // Err means no subscriber right now; the roster watch still updates.
        let _ = self.events_tx.send(event);
    }

    fn publish_roster(&mut self) {
        let _ = self.participants_tx.send(self.roster.clone());
        self.publish_tiles();
    }

    /// Members speaking now as the SFU reports them: what a tile's flag
    /// reports. Undamped here, but already smoothed and rate-limited by the
    /// server (see the `ActiveSpeakers` handler).
    fn raw_speaking(&self) -> HashSet<String> {
        self.speakers
            .iter()
            .filter(|(_, state)| state.raw)
            .map(|(member_id, _)| member_id.clone())
            .collect()
    }

    /// Feeds the transport's speaking snapshot into the hysteresis (R10).
    ///
    /// A member whose raw state moved away from their effective state gets a
    /// timer — `promote` to start counting as speaking, `demote` to stop — and
    /// a move back before it fires makes that timer stale. Two people trading
    /// half-second bursts therefore never trade places.
    ///
    /// The raw state itself is published at once: the flag on a tile is not
    /// damped (R11), only where the tile ranks.
    fn apply_raw_speakers(&mut self, raw: HashSet<String>) {
        let mut raw_changed = false;
        let ids: HashSet<String> = self
            .speakers
            .keys()
            .cloned()
            .chain(raw.iter().cloned())
            .collect();
        for member_id in ids {
            let is_raw = raw.contains(&member_id);
            let state = self
                .speakers
                .entry(member_id.clone())
                .or_insert(SpeakerState {
                    raw: false,
                    generation: 0,
                });
            if state.raw == is_raw {
                continue;
            }
            state.raw = is_raw;
            state.generation += 1;
            raw_changed = true;
            let generation = state.generation;
            if is_raw == self.speaking.contains(&member_id) {
                // Back to matching the effective state: the in-flight timer is
                // stale now, and there is nothing to arm.
                continue;
            }
            let delay = if is_raw {
                self.stability.promote
            } else {
                self.stability.demote
            };
            let messages = self.messages_tx.clone();
            rt::spawn(async move {
                rt::sleep(delay).await;
                let _ = messages.send(ActorMessage::SpeakerTimer {
                    member_id,
                    generation,
                });
            });
        }
        if raw_changed {
            self.publish_tiles();
        }
    }

    /// Derives and publishes the tile roster and our local state. Reached from
    /// every roster publish, and from the changes that do not touch the
    /// roster: the raw and the effective speaking sets, and the detail window.
    ///
    /// R11: a tile's own state is never delayed, and reordering is coalesced.
    /// When the order moved, consumers get the fresh state now — in the order
    /// they already hold, minus anyone who left — and the new order when the
    /// window closes, so a burst of rank changes lands as one reorder.
    fn publish_tiles(&mut self) {
        let Tiles { remote, own } = derive_tiles(
            &self.roster,
            &self.raw_speaking(),
            &self.speaking,
            self.ranking_threshold,
        );
        self.publish_local(own);

        let order: Vec<TileId> = remote.iter().map(CallTile::id).collect();
        if order == self.last_order {
            self.send_tiles(window(&remote, &self.window));
            return;
        }
        if self.last_order.is_empty() {
            // Populating an empty order is not a reorder: nothing a consumer
            // holds can jump. Publish at once, or every call would open on a
            // blank window for the length of the coalesce delay.
            self.last_order = order;
            self.send_tiles(window(&remote, &self.window));
            return;
        }

        if self.pending_reorder.is_none() {
            self.reorder_generation += 1;
            let generation = self.reorder_generation;
            self.pending_reorder = Some(generation);
            let delay = self.stability.coalesce;
            let messages = self.messages_tx.clone();
            rt::spawn(async move {
                rt::sleep(delay).await;
                let _ = messages.send(ActorMessage::FlushReorder { generation });
            });
        }
        // Departed tiles leave the held order at once; joiners wait for it.
        let by_id: HashMap<TileId, &CallTile> =
            remote.iter().map(|tile| (tile.id(), tile)).collect();
        self.last_order.retain(|id| by_id.contains_key(id));
        let held: Vec<CallTile> = self.last_order.iter().map(|id| by_id[id].clone()).collect();
        self.send_tiles(window(&held, &self.window));
    }

    /// The coalesce window closed: publish the current order (R11).
    fn publish_tiles_reordered(&mut self) {
        let Tiles { remote, own } = derive_tiles(
            &self.roster,
            &self.raw_speaking(),
            &self.speaking,
            self.ranking_threshold,
        );
        self.publish_local(own);
        self.last_order = remote.iter().map(CallTile::id).collect();
        self.send_tiles(window(&remote, &self.window));
    }

    fn publish_local(&self, own: Option<CallTile>) {
        let local = own.map(|tile| LocalState {
            // Publication state, not intent: up and unmuted, however it ends.
            is_screen_sharing: self
                .roster
                .iter()
                .find(|p| p.is_local)
                .and_then(|p| {
                    p.streams
                        .iter()
                        .find(|s| s.kind == MediaStreamKind::ScreenShare)
                })
                .is_some_and(|s| !s.muted),
            tile,
        });
        self.local_tx.send_if_modified(|current| {
            if *current == local {
                return false;
            }
            *current = local;
            true
        });
    }

    fn send_tiles(&self, roster: TileRoster) {
        self.tiles_tx.send_if_modified(|current| {
            if *current == roster {
                return false;
            }
            *current = roster;
            true
        });
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex as StdMutex;
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    use std::time::Duration;

    use futures_core::stream::BoxStream;
    use futures_util::StreamExt;
    use matrix_rtc_call::{CallEnd, CallStatus};
    use matrix_rtc_core::{EventOrigin, LeaveCode, LiveKitTransport, RtcTransport};

    use crate::event::FrameEncryptionDiagnostic;
    use matrix_rtc_core::KeyRejection;
    use tokio::sync::mpsc::UnboundedSender;

    use super::*;
    use matrix_rtc_transport::pool::{CONSTRAINTS_DEBOUNCE, IDLE_GRACE};
    use matrix_rtc_transport::{
        AudioFrame, LocalTrackHandle, OwnMemberClaims, SpeakingParticipant, VideoSourceConfig,
    };

    const OWN_FOCUS: &str = "https://sfu.example.org";
    const PEER_FOCUS: &str = "https://sfu-b.example.org";

    /// Shared observable state of the fake transport: which keys got
    /// connected/closed, and the live event senders per key.
    #[derive(Default)]
    struct TransportState {
        connects: StdMutex<Vec<String>>,
        closes: StdMutex<Vec<String>>,
        senders: StdMutex<HashMap<String, UnboundedSender<ConnectionEvent>>>,
        /// Number of connect attempts (per key) that fail before succeeding.
        fail_attempts: AtomicU32,
        /// `(connection_key, kind, muted)` of every publish call.
        published: StdMutex<Vec<(String, MediaStreamKind, bool)>>,
        /// `(identity, kind, resolved)` of every apply_constraints call.
        applied: StdMutex<
            Vec<(
                String,
                MediaStreamKind,
                matrix_rtc_transport::ResolvedConstraints,
            )>,
        >,
        /// `(kind, muted)` of every local mute call that reached the transport.
        local_mutes: StdMutex<Vec<(MediaStreamKind, bool)>>,
        /// Kind of every unpublish call that reached the transport.
        unpublished: StdMutex<Vec<MediaStreamKind>>,
        /// Makes the next unpublish calls fail at the transport.
        fail_unpublish: AtomicBool,
    }

    struct FakeTransport {
        state: Arc<TransportState>,
    }

    #[async_trait::async_trait]
    impl MediaTransport for FakeTransport {
        fn transport_type(&self) -> &'static str {
            "fake"
        }

        fn connection_key(
            &self,
            transport: &RtcTransport,
            _publisher_user_id: &str,
        ) -> Option<String> {
            match transport {
                RtcTransport::LiveKit(livekit) => livekit.livekit_service_url().map(str::to_owned),
                RtcTransport::Unsupported(_) => None,
            }
        }

        fn remote_identity(&self, member: &JoinedMembership) -> Option<String> {
            Some(format!("id-{}", member.member_id))
        }

        async fn connect(
            &self,
            connection_key: &str,
            _ctx: &ConnectionContext,
        ) -> Result<
            (
                Box<dyn TransportConnection>,
                mpsc::UnboundedReceiver<ConnectionEvent>,
            ),
            TransportError,
        > {
            self.state
                .connects
                .lock()
                .unwrap()
                .push(connection_key.to_owned());
            if self.state.fail_attempts.load(Ordering::SeqCst) > 0 {
                self.state.fail_attempts.fetch_sub(1, Ordering::SeqCst);
                return Err(TransportError::Connect("fake failure".into()));
            }
            let (connection, events) = fake_connection(&self.state, connection_key);
            Ok((connection, events))
        }
    }

    struct FakeConnection {
        key: String,
        state: Arc<TransportState>,
    }

    #[async_trait::async_trait]
    impl TransportConnection for FakeConnection {
        fn connection_key(&self) -> &str {
            &self.key
        }

        async fn publish(
            &self,
            options: PublishOptions,
        ) -> Result<Arc<dyn LocalTrackHandle>, TransportError> {
            self.state.published.lock().unwrap().push((
                self.key.clone(),
                options.kind,
                options.muted,
            ));
            Ok(Arc::new(FakeLocalTrack::new(
                options.kind,
                Arc::clone(&self.state),
            )))
        }

        async fn apply_constraints(
            &self,
            identity: &str,
            kind: MediaStreamKind,
            resolved: matrix_rtc_transport::ResolvedConstraints,
        ) -> Result<(), TransportError> {
            self.state
                .applied
                .lock()
                .unwrap()
                .push((identity.to_owned(), kind, resolved));
            Ok(())
        }

        async fn close(&self) -> Result<(), TransportError> {
            self.state.closes.lock().unwrap().push(self.key.clone());
            Ok(())
        }
    }

    struct FakeLocalTrack {
        kind: MediaStreamKind,
        state: Arc<TransportState>,
    }

    impl FakeLocalTrack {
        fn new(kind: MediaStreamKind, state: Arc<TransportState>) -> Self {
            Self { kind, state }
        }
    }

    #[async_trait::async_trait]
    impl LocalTrackHandle for FakeLocalTrack {
        fn kind(&self) -> MediaStreamKind {
            self.kind
        }

        fn set_muted(&self, muted: bool) -> Result<(), TransportError> {
            self.state
                .local_mutes
                .lock()
                .unwrap()
                .push((self.kind, muted));
            Ok(())
        }

        async fn unpublish(&self) -> Result<(), TransportError> {
            if self.state.fail_unpublish.load(Ordering::SeqCst) {
                return Err(TransportError::Closed("fake unpublish failure".into()));
            }
            self.state.unpublished.lock().unwrap().push(self.kind);
            Ok(())
        }
    }

    fn fake_connection(
        state: &Arc<TransportState>,
        key: &str,
    ) -> (
        Box<dyn TransportConnection>,
        mpsc::UnboundedReceiver<ConnectionEvent>,
    ) {
        let (tx, rx) = mpsc::unbounded_channel();
        state.senders.lock().unwrap().insert(key.to_owned(), tx);
        (
            Box::new(FakeConnection {
                key: key.to_owned(),
                state: state.clone(),
            }),
            rx,
        )
    }

    struct FakeTrack {
        kind: MediaStreamKind,
        /// What `receive_stats` reports; `None` models a transport with no
        /// counters (the trait default).
        stats: Option<ReceiveStats>,
        /// A barrier every `receive_stats` waits at before answering, so a
        /// test can prove several were in flight together.
        gate: Option<Arc<tokio::sync::Barrier>>,
    }

    impl FakeTrack {
        fn new(kind: MediaStreamKind) -> Self {
            Self {
                kind,
                stats: None,
                gate: None,
            }
        }

        fn with_stats(kind: MediaStreamKind, stats: ReceiveStats) -> Self {
            Self {
                kind,
                stats: Some(stats),
                gate: None,
            }
        }
    }

    #[async_trait::async_trait]
    impl RemoteTrackHandle for FakeTrack {
        fn kind(&self) -> MediaStreamKind {
            self.kind
        }

        fn audio_frames(&self) -> Option<BoxStream<'static, AudioFrame>> {
            let frame = AudioFrame {
                data: vec![1, -1],
                sample_rate: 48_000,
                num_channels: 1,
                samples_per_channel: 2,
            };
            Some(futures_util::stream::iter([frame]).boxed())
        }

        async fn receive_stats(&self) -> Option<ReceiveStats> {
            if let Some(gate) = &self.gate {
                gate.wait().await;
            }
            self.stats.clone()
        }
    }

    fn member_on(member_id: &str, user_id: &str, focus: &str) -> JoinedMembership {
        JoinedMembership {
            room_id: "!room:example.org".to_owned(),
            slot_id: "m.call#room".to_owned(),
            sender: user_id.to_owned(),
            origin: EventOrigin::encrypted(Some("DEVICE".to_owned())),
            sticky_key: member_id.to_owned(),
            member_id: member_id.to_owned(),
            membership_event_id: None,
            membership_ts: None,
            origin_server_ts: None,
            application: "m.call".into(),
            transports: vec![RtcTransport::LiveKit(LiveKitTransport::service(
                focus.to_owned(),
            ))],
            can_subscribe: vec!["livekit".to_owned()],
        }
    }

    fn member(member_id: &str, user_id: &str) -> JoinedMembership {
        member_on(member_id, user_id, OWN_FOCUS)
    }

    struct Fixture {
        engine: CallEngine,
        events: broadcast::Receiver<CallEvent>,
        memberships: watch::Sender<Vec<JoinedMembership>>,
        raised_hands: watch::Sender<Vec<RaisedHand>>,
        reactions: broadcast::Sender<ReceivedReaction>,
        ended: watch::Sender<Option<LeaveReason>>,
        signalling: watch::Sender<SignallingHealth>,
        lapsed: watch::Sender<bool>,
        /// The status a call would show, our media already connected.
        status: watch::Receiver<CallStatus>,
        state: Arc<TransportState>,
    }

    fn fixture() -> Fixture {
        fixture_with(StabilityConfig::default())
    }

    fn fixture_with(stability: StabilityConfig) -> Fixture {
        let mut fx = unsubscribed_fixture(stability);
        fx.events = fx.engine.subscribe_events();
        fx
    }

    /// A fixture whose `events` is a placeholder: nothing has subscribed to
    /// the engine yet, so the first `subscribe_events` is still to come.
    fn unsubscribed_fixture(stability: StabilityConfig) -> Fixture {
        let state = Arc::new(TransportState::default());
        let (memberships, memberships_rx) = watch::channel(Vec::new());
        let (raised_hands, raised_hands_rx) = watch::channel(Vec::new());
        let (reactions, reactions_rx) = broadcast::channel(8);
        let (ended, ended_rx) = watch::channel(None);
        let (signalling, signalling_rx) = watch::channel(SignallingHealth::Reachable);
        let (lapsed, lapsed_rx) = watch::channel(false);
        let (media_status, status) = MediaStatusReporter::standalone();
        media_status.connected();
        let engine = CallEngine::new(
            EngineConfig {
                transports: vec![Arc::new(FakeTransport {
                    state: state.clone(),
                })],
                own_member_id: "own".to_owned(),
                ctx: ConnectionContext {
                    room_id: "!room:example.org".to_owned(),
                    slot_id: "m.call#room".to_owned(),
                    member: OwnMemberClaims {
                        member_id: "own".to_owned(),
                        user_id: "@alice:example.org".to_owned(),
                        device_id: "DEVICE".to_owned(),
                    },
                    e2ee: true,
                },
                own_connection_key: Some(OWN_FOCUS.to_owned()),
                raised_hands: Some(raised_hands_rx),
                reactions: Some(reactions_rx),
                ended: Some(ended_rx),
                media_status: Some(media_status),
                stability,
                signalling: Some(signalling_rx),
                own_membership_lapsed: Some(lapsed_rx),
                own_reconnect_budget: matrix_rtc_transport::pool::OWN_RECONNECT_BUDGET,
                sfu_fallback: false,
            },
            memberships_rx,
        );
        Fixture {
            engine,
            events: broadcast::channel(1).1,
            memberships,
            raised_hands,
            reactions,
            ended,
            signalling,
            lapsed,
            status,
            state,
        }
    }

    fn degraded(fx: &Fixture) -> Option<bool> {
        match *fx.status.borrow() {
            CallStatus::Connected { degraded, .. } => Some(degraded),
            _ => None,
        }
    }

    /// Events emitted before anyone subscribes reach the first subscriber.
    /// Hosts replay held keys while media is still connecting, before they
    /// subscribe; with a plain broadcast those `KeyImported`s were dropped, and
    /// each host had to get its call order right to see them.
    #[tokio::test]
    async fn the_first_subscriber_sees_events_from_before_it_subscribed() {
        let fx = unsubscribed_fixture(StabilityConfig::default());

        in_call(&fx).await;
        fx.memberships
            .send(vec![
                member("own", "@alice:example.org"),
                member("bob", "@bob:example.org"),
            ])
            .unwrap();
        fx.engine.notify_key_imported("id-bob", 0);
        wait_until(|| fx.engine.participants().len() == 2).await;
        tokio::task::yield_now().await;

        let mut events = fx.engine.subscribe_events();
        assert_eq!(
            next_event(&mut events).await,
            CallEvent::ParticipantJoined {
                member_id: "bob".to_owned(),
                user_id: "@bob:example.org".to_owned(),
                participant_count: 2,
            }
        );
        assert_eq!(
            next_event(&mut events).await,
            CallEvent::KeyImported {
                member_id: "bob".to_owned(),
                identity: "id-bob".to_owned(),
                key_index: 0,
            }
        );
    }

    fn participant(fx: &Fixture, member_id: &str) -> Participant {
        fx.engine
            .participants()
            .into_iter()
            .find(|p| p.member_id == member_id)
            .unwrap()
    }

    fn reaction_from(member_id: &str) -> ReceivedReaction {
        ReceivedReaction {
            member_id: member_id.to_owned(),
            sender: format!("@{member_id}:example.org"),
            emoji: "👏".to_owned(),
            name: "clapping".to_owned(),
            sound: matrix_rtc_call::ReactionSound::Named("clap".to_owned()),
        }
    }

    fn clapped(member_id: &str) -> CallEvent {
        CallEvent::Reaction {
            member_id: member_id.to_owned(),
            emoji: "👏".to_owned(),
            name: "clapping".to_owned(),
            sound: "clap".to_owned(),
        }
    }

    /// The hands up when we arrive, and the hand a member arrives with, are
    /// state on the roster, not news. The core accepts a live hand only once
    /// it knows the membership, so a hand that was up before we saw the member
    /// comes from the relations backfill. The reaction after them is the first
    /// event: nothing about a hand came before it.
    #[tokio::test]
    async fn hands_already_up_are_on_the_roster_but_not_announced() {
        let mut fx = fixture();
        let backfilled = |member_id: &str, raised_at_ms: u64| RaisedHand {
            from_backfill: true,
            ..hand(member_id, raised_at_ms)
        };

        // Known before the membership: parked, then applied.
        fx.raised_hands.send(vec![backfilled("bob", 42)]).unwrap();
        fx.memberships
            .send(vec![
                member("own", "@alice:example.org"),
                member("bob", "@bob:example.org"),
            ])
            .unwrap();
        rostered(&fx, &["own", "bob"]).await;
        wait_until(|| participant(&fx, "bob").hand_raised_at_ms == Some(42)).await;
        assert_eq!(participant(&fx, "bob").hand_raised_position, Some(1));

        // Carol arrives with her hand up.
        fx.raised_hands
            .send(vec![backfilled("bob", 42), backfilled("carol", 50)])
            .unwrap();
        fx.memberships
            .send(vec![
                member("own", "@alice:example.org"),
                member("bob", "@bob:example.org"),
                member("carol", "@carol:example.org"),
            ])
            .unwrap();
        assert_eq!(
            next_event(&mut fx.events).await,
            CallEvent::ParticipantJoined {
                member_id: "carol".to_owned(),
                user_id: "@carol:example.org".to_owned(),
                participant_count: 3,
            }
        );
        wait_until(|| participant(&fx, "carol").hand_raised_position == Some(2)).await;

        fx.reactions.send(reaction_from("bob")).unwrap();
        assert_eq!(next_event(&mut fx.events).await, clapped("bob"));
    }

    /// In the call, a hand seen going up is announced; one the relations
    /// backfill found is not, nor a hand that stays up under a new annotation.
    /// Lowering is announced and renumbers the queue.
    #[tokio::test]
    async fn a_hand_raised_in_the_call_is_announced() {
        let mut fx = fixture();
        in_call(&fx).await;
        fx.memberships
            .send(vec![
                member("own", "@alice:example.org"),
                member("bob", "@bob:example.org"),
                member("carol", "@carol:example.org"),
            ])
            .unwrap();
        let _ = next_event(&mut fx.events).await; // ParticipantJoined bob
        let _ = next_event(&mut fx.events).await; // ParticipantJoined carol

        let bob_backfilled = RaisedHand {
            from_backfill: true,
            ..hand("bob", 10)
        };
        fx.raised_hands.send(vec![bob_backfilled.clone()]).unwrap();
        wait_until(|| participant(&fx, "bob").hand_raised_position == Some(1)).await;

        fx.raised_hands
            .send(vec![bob_backfilled, hand("carol", 20)])
            .unwrap();
        assert_eq!(
            next_event(&mut fx.events).await,
            CallEvent::HandRaised {
                member_id: "carol".to_owned(),
                raised_at_ms: 20,
            },
            "bob's backfilled hand was never announced"
        );
        assert_eq!(participant(&fx, "carol").hand_raised_position, Some(2));

        fx.raised_hands.send(vec![hand("carol", 20)]).unwrap();
        assert_eq!(
            next_event(&mut fx.events).await,
            CallEvent::HandLowered {
                member_id: "bob".to_owned(),
            }
        );
        let carol = participant(&fx, "carol");
        assert_eq!(carol.hand_raised_position, Some(1), "the queue renumbers");
        assert_eq!(participant(&fx, "bob").hand_raised_position, None);

        // Carol's refresh: the hand moves to a new annotation, same time.
        fx.raised_hands
            .send(vec![RaisedHand {
                reaction_event_id: "$carol-again".to_owned(),
                ..hand("carol", 20)
            }])
            .unwrap();
        fx.reactions.send(reaction_from("carol")).unwrap();
        assert_eq!(next_event(&mut fx.events).await, clapped("carol"));
    }

    /// A hand of a member not on the roster yet takes no place in the queue,
    /// and a departure renumbers the rest.
    #[tokio::test]
    async fn positions_count_the_roster_only() {
        let fx = fixture();
        fx.raised_hands
            .send(vec![hand("dave", 1), hand("bob", 2), hand("carol", 3)])
            .unwrap();
        fx.memberships
            .send(vec![
                member("own", "@alice:example.org"),
                member("bob", "@bob:example.org"),
                member("carol", "@carol:example.org"),
            ])
            .unwrap();
        rostered(&fx, &["own", "bob", "carol"]).await;
        wait_until(|| participant(&fx, "carol").hand_raised_position == Some(2)).await;
        assert_eq!(participant(&fx, "bob").hand_raised_position, Some(1));
        assert_eq!(participant(&fx, "own").hand_raised_position, None);

        fx.memberships
            .send(vec![
                member("own", "@alice:example.org"),
                member("dave", "@dave:example.org"),
                member("carol", "@carol:example.org"),
            ])
            .unwrap();
        rostered(&fx, &["dave"]).await;
        assert_eq!(participant(&fx, "dave").hand_raised_position, Some(1));
        assert_eq!(participant(&fx, "carol").hand_raised_position, Some(2));
    }

    /// Until our membership first lands, hands and reactions still land on
    /// the roster (or are dropped) silently.
    #[tokio::test]
    async fn no_hand_or_reaction_is_announced_before_our_membership_lands() {
        let mut fx = fixture();
        fx.memberships
            .send(vec![member("bob", "@bob:example.org")])
            .unwrap();
        rostered(&fx, &["bob"]).await;

        fx.raised_hands.send(vec![hand("bob", 5)]).unwrap();
        wait_until(|| participant(&fx, "bob").hand_raised_at_ms == Some(5)).await;
        fx.reactions.send(reaction_from("bob")).unwrap();
        fx.raised_hands.send(Vec::new()).unwrap();
        wait_until(|| participant(&fx, "bob").hand_raised_at_ms.is_none()).await;
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(fx.events.try_recv().is_err());
    }

    async fn next_event(events: &mut broadcast::Receiver<CallEvent>) -> CallEvent {
        tokio::time::timeout(Duration::from_secs(5), events.recv())
            .await
            .expect("timed out waiting for a call event")
            .expect("event channel closed")
    }

    /// Land our own membership: the roster every later change is announced
    /// against.
    async fn in_call(fx: &Fixture) {
        fx.memberships
            .send(vec![member("own", "@alice:example.org")])
            .unwrap();
        rostered(fx, &["own"]).await;
    }

    /// Wait until every one of `member_ids` is on the roster. The roster we
    /// arrive to is not announced, so this, not the event stream, is how a
    /// test sees a snapshot land.
    async fn rostered(fx: &Fixture, member_ids: &[&str]) {
        wait_until(|| {
            let roster = fx.engine.participants();
            member_ids
                .iter()
                .all(|id| roster.iter().any(|p| p.member_id == *id))
        })
        .await;
    }

    /// Poll until `predicate` holds (paused-clock tests auto-advance timers).
    async fn wait_until(mut predicate: impl FnMut() -> bool) {
        tokio::time::timeout(Duration::from_secs(60), async {
            while !predicate() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("condition not reached in time");
    }

    /// Let the actor apply everything queued so far.
    ///
    /// `publish` resolves as soon as the transport answered, while the
    /// `LocalPublished` that puts the stream on the roster is still queued —
    /// and the membership watch is a different `select!` branch, so a test that
    /// publishes and then feeds a snapshot would race the two. Muting a kind
    /// that was never published is answered without touching any state, so its
    /// reply proves every message queued before it has been applied.
    async fn settle(engine: &CallEngine) {
        engine
            .set_local_muted(MediaStreamKind::Data, true)
            .await
            .expect_err("nothing of that kind is published");
    }

    /// Adopt an own-focus connection into the engine, returning its sender.
    fn adopt(fx: &Fixture) -> UnboundedSender<ConnectionEvent> {
        let (connection, events) = fake_connection(&fx.state, OWN_FOCUS);
        fx.engine.adopt_own_connection(connection, events);
        wait_sender(fx, OWN_FOCUS)
    }

    fn wait_sender(fx: &Fixture, key: &str) -> UnboundedSender<ConnectionEvent> {
        fx.state
            .senders
            .lock()
            .unwrap()
            .get(key)
            .expect("connection sender should exist")
            .clone()
    }

    fn peer_sender(fx: &Fixture, key: &str) -> Option<UnboundedSender<ConnectionEvent>> {
        fx.state.senders.lock().unwrap().get(key).cloned()
    }

    fn connect_count(fx: &Fixture, key: &str) -> usize {
        fx.state
            .connects
            .lock()
            .unwrap()
            .iter()
            .filter(|k| *k == key)
            .count()
    }

    fn closed(fx: &Fixture, key: &str) -> bool {
        fx.state.closes.lock().unwrap().iter().any(|k| k == key)
    }

    #[tokio::test]
    async fn snapshot_builds_the_roster() {
        let mut fx = fixture();
        fx.memberships
            .send(vec![
                member("own", "@alice:example.org"),
                member("bob", "@bob:example.org"),
            ])
            .unwrap();

        // The roster we arrive to is not news: no join is announced for it,
        // ours included.
        rostered(&fx, &["own", "bob"]).await;
        assert!(fx.events.try_recv().is_err());

        let participants = fx.engine.participants();
        assert_eq!(participants.len(), 2);
        let own = participants.iter().find(|p| p.member_id == "own").unwrap();
        assert!(own.is_local);
        let bob = participants.iter().find(|p| p.member_id == "bob").unwrap();
        assert!(!bob.is_local);
        assert_eq!(bob.connection, ConnectionState::Connected);
        assert_eq!(bob.device_id.as_deref(), Some("DEVICE"));

        // Everyone is on the own focus, which awaits adoption: the engine
        // must not have opened a connection itself.
        assert_eq!(connect_count(&fx, OWN_FOCUS), 0);
    }

    /// Another device of ours is a participant of its own, and its arrival
    /// is news like anyone else's.
    #[tokio::test]
    async fn a_second_device_of_ours_is_announced() {
        let mut fx = fixture();
        in_call(&fx).await;

        fx.memberships
            .send(vec![
                member("own", "@alice:example.org"),
                member("own-2", "@alice:example.org"),
            ])
            .unwrap();
        assert_eq!(
            next_event(&mut fx.events).await,
            CallEvent::ParticipantJoined {
                member_id: "own-2".to_owned(),
                user_id: "@alice:example.org".to_owned(),
                participant_count: 2,
            }
        );
    }

    /// One snapshot can carry a departure and an arrival: both are announced,
    /// each with the count after the whole change.
    #[tokio::test]
    async fn a_swap_announces_both_with_the_count_after_it() {
        let mut fx = fixture();
        in_call(&fx).await;
        fx.memberships
            .send(vec![
                member("own", "@alice:example.org"),
                member("bob", "@bob:example.org"),
            ])
            .unwrap();
        let _ = next_event(&mut fx.events).await; // ParticipantJoined

        fx.memberships
            .send(vec![
                member("own", "@alice:example.org"),
                member("carol", "@carol:example.org"),
            ])
            .unwrap();
        assert_eq!(
            next_event(&mut fx.events).await,
            CallEvent::ParticipantLeft {
                member_id: "bob".to_owned(),
                participant_count: 2,
            }
        );
        assert_eq!(
            next_event(&mut fx.events).await,
            CallEvent::ParticipantJoined {
                member_id: "carol".to_owned(),
                user_id: "@carol:example.org".to_owned(),
                participant_count: 2,
            }
        );
    }

    /// 034 R16: our membership lapses while bob is with us; the catch-up set
    /// brings us back with carol, who joined meanwhile. carol is announced as
    /// a join, bob is not re-announced, and our own entry never is.
    #[tokio::test]
    async fn a_member_found_in_the_catch_up_is_announced_as_a_join() {
        let mut fx = fixture();
        in_call(&fx).await;
        fx.memberships
            .send(vec![
                member("own", "@alice:example.org"),
                member("bob", "@bob:example.org"),
            ])
            .unwrap();
        let _ = next_event(&mut fx.events).await; // ParticipantJoined

        // Our membership lapses: the roster follows, nothing about us is said.
        fx.memberships
            .send(vec![member("bob", "@bob:example.org")])
            .unwrap();
        wait_until(|| fx.engine.participants().len() == 1).await;
        assert!(fx.events.try_recv().is_err());

        fx.memberships
            .send(vec![
                member("own", "@alice:example.org"),
                member("bob", "@bob:example.org"),
                member("carol", "@carol:example.org"),
            ])
            .unwrap();
        assert_eq!(
            next_event(&mut fx.events).await,
            CallEvent::ParticipantJoined {
                member_id: "carol".to_owned(),
                user_id: "@carol:example.org".to_owned(),
                participant_count: 3,
            }
        );
        rostered(&fx, &["own", "bob", "carol"]).await;
        assert!(fx.events.try_recv().is_err(), "nothing else");
    }

    /// The roster found before our own membership first lands was there
    /// before us: none of it is announced.
    #[tokio::test]
    async fn the_roster_before_our_membership_lands_is_not_announced() {
        let mut fx = fixture();
        fx.memberships
            .send(vec![member("bob", "@bob:example.org")])
            .unwrap();
        wait_until(|| fx.engine.participants().len() == 1).await;
        fx.memberships
            .send(vec![
                member("own", "@alice:example.org"),
                member("bob", "@bob:example.org"),
            ])
            .unwrap();
        rostered(&fx, &["own", "bob"]).await;
        assert!(fx.events.try_recv().is_err());
    }

    /// Once the call has ended for us, the roster may still be torn down by
    /// later snapshots, but none of it is announced.
    #[tokio::test(start_paused = true)]
    async fn nothing_is_announced_after_the_call_ended() {
        let mut fx = fixture();
        in_call(&fx).await;
        fx.memberships
            .send(vec![
                member("own", "@alice:example.org"),
                member("bob", "@bob:example.org"),
            ])
            .unwrap();
        let _ = next_event(&mut fx.events).await; // ParticipantJoined

        fx.ended
            .send_replace(Some(LeaveReason::new(LeaveCode::SlotClosed)));
        settle(&fx.engine).await;

        fx.raised_hands.send(vec![hand("bob", 5)]).unwrap();
        wait_until(|| participant(&fx, "bob").hand_raised_at_ms == Some(5)).await;
        fx.reactions.send(reaction_from("bob")).unwrap();
        fx.memberships
            .send(vec![member("own", "@alice:example.org")])
            .unwrap();
        wait_until(|| fx.engine.participants().len() == 1).await;
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(fx.events.try_recv().is_err());
    }

    #[tokio::test]
    async fn member_without_supported_transport_is_unreachable() {
        let fx = fixture();
        let mut eve = member("eve", "@eve:example.org");
        eve.transports = vec![RtcTransport::Unsupported(
            matrix_rtc_core::UnsupportedTransport {
                transport_type: "p2p".to_owned(),
                extra_fields: Default::default(),
            },
        )];
        fx.memberships.send(vec![eve]).unwrap();

        rostered(&fx, &["eve"]).await;
        let participants = fx.engine.participants();
        assert_eq!(participants[0].connection, ConnectionState::Unreachable);
    }

    #[tokio::test]
    async fn track_maps_to_member_and_yields_frames() {
        let mut fx = fixture();
        fx.memberships
            .send(vec![member("bob", "@bob:example.org")])
            .unwrap();
        rostered(&fx, &["bob"]).await;

        let connection = adopt(&fx);
        connection
            .send(ConnectionEvent::TrackAdded {
                identity: "id-bob".to_owned(),
                kind: MediaStreamKind::Microphone,
                track: Arc::new(FakeTrack::new(MediaStreamKind::Microphone)),
            })
            .unwrap();

        assert_eq!(
            next_event(&mut fx.events).await,
            CallEvent::StreamStarted {
                member_id: "bob".to_owned(),
                kind: MediaStreamKind::Microphone,
            }
        );

        let track = fx
            .engine
            .remote_track("bob", MediaStreamKind::Microphone)
            .expect("track handle should be available");
        let frames: Vec<AudioFrame> = track.audio_frames().unwrap().collect().await;
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].sample_rate, 48_000);

        // Mute, then removal.
        connection
            .send(ConnectionEvent::TrackMuted {
                identity: "id-bob".to_owned(),
                kind: MediaStreamKind::Microphone,
            })
            .unwrap();
        assert_eq!(
            next_event(&mut fx.events).await,
            CallEvent::StreamMuted {
                member_id: "bob".to_owned(),
                kind: MediaStreamKind::Microphone,
            }
        );
        connection
            .send(ConnectionEvent::TrackRemoved {
                identity: "id-bob".to_owned(),
                kind: MediaStreamKind::Microphone,
            })
            .unwrap();
        assert_eq!(
            next_event(&mut fx.events).await,
            CallEvent::StreamStopped {
                member_id: "bob".to_owned(),
                kind: MediaStreamKind::Microphone,
            }
        );
        assert!(
            fx.engine
                .remote_track("bob", MediaStreamKind::Microphone)
                .is_none()
        );
    }

    #[tokio::test]
    async fn early_media_waits_for_its_membership() {
        let mut fx = fixture();
        in_call(&fx).await;
        let connection = adopt(&fx);

        // Track and key arrive before the sticky membership.
        connection
            .send(ConnectionEvent::TrackAdded {
                identity: "id-bob".to_owned(),
                kind: MediaStreamKind::Camera,
                track: Arc::new(FakeTrack::new(MediaStreamKind::Camera)),
            })
            .unwrap();
        fx.engine.notify_key_imported("id-bob", 3);

        fx.memberships
            .send(vec![
                member("own", "@alice:example.org"),
                member("bob", "@bob:example.org"),
            ])
            .unwrap();

        assert_eq!(
            next_event(&mut fx.events).await,
            CallEvent::ParticipantJoined {
                member_id: "bob".to_owned(),
                user_id: "@bob:example.org".to_owned(),
                participant_count: 2,
            }
        );
        // The buffered track and key both flush on the membership; their
        // relative order depends on when the connection forwarder ran.
        let flushed = [
            next_event(&mut fx.events).await,
            next_event(&mut fx.events).await,
        ];
        assert!(flushed.contains(&CallEvent::StreamStarted {
            member_id: "bob".to_owned(),
            kind: MediaStreamKind::Camera,
        }));
        assert!(flushed.contains(&CallEvent::KeyImported {
            member_id: "bob".to_owned(),
            identity: "id-bob".to_owned(),
            key_index: 3,
        }));
    }

    /// More than one key can be imported before a membership lands — a first key
    /// plus a rotation, which is exactly what a rejoin produces. Keeping only the
    /// latest index left the host believing the transport held one key when it
    /// held several, and unable to tell which.
    #[tokio::test]
    async fn every_early_key_surfaces_once_the_membership_lands() {
        let mut fx = fixture();
        in_call(&fx).await;

        fx.engine.notify_key_imported("id-bob", 0);
        fx.engine.notify_key_imported("id-bob", 1);
        // A repeat of an index already buffered is still just one import.
        fx.engine.notify_key_imported("id-bob", 1);

        fx.memberships
            .send(vec![
                member("own", "@alice:example.org"),
                member("bob", "@bob:example.org"),
            ])
            .unwrap();

        assert_eq!(
            next_event(&mut fx.events).await,
            CallEvent::ParticipantJoined {
                member_id: "bob".to_owned(),
                user_id: "@bob:example.org".to_owned(),
                participant_count: 2,
            }
        );
        assert_eq!(
            next_event(&mut fx.events).await,
            CallEvent::KeyImported {
                member_id: "bob".to_owned(),
                identity: "id-bob".to_owned(),
                key_index: 0,
            }
        );
        assert_eq!(
            next_event(&mut fx.events).await,
            CallEvent::KeyImported {
                member_id: "bob".to_owned(),
                identity: "id-bob".to_owned(),
                key_index: 1,
            }
        );
    }

    /// A departure must take its identity-keyed buffers with it, or a key
    /// imported for the member who left resurfaces against whoever next occupies
    /// that identity.
    #[tokio::test]
    async fn a_departure_forgets_the_keys_buffered_against_its_identity() {
        let mut fx = fixture();
        in_call(&fx).await;

        fx.memberships
            .send(vec![
                member("own", "@alice:example.org"),
                member("bob", "@bob:example.org"),
            ])
            .unwrap();
        assert_eq!(
            next_event(&mut fx.events).await,
            CallEvent::ParticipantJoined {
                member_id: "bob".to_owned(),
                user_id: "@bob:example.org".to_owned(),
                participant_count: 2,
            }
        );

        // Bob leaves, and a key for his identity lands late.
        fx.memberships
            .send(vec![member("own", "@alice:example.org")])
            .unwrap();
        assert_eq!(
            next_event(&mut fx.events).await,
            CallEvent::ParticipantLeft {
                member_id: "bob".to_owned(),
                participant_count: 1,
            }
        );
        fx.engine.notify_key_imported("id-bob", 7);

        // Bob rejoins under a fresh member id, as MSC4143 requires. The stale
        // index must not be attributed to the new participation.
        fx.memberships
            .send(vec![
                member("own", "@alice:example.org"),
                member("bob-2", "@bob:example.org"),
            ])
            .unwrap();
        assert_eq!(
            next_event(&mut fx.events).await,
            CallEvent::ParticipantJoined {
                member_id: "bob-2".to_owned(),
                user_id: "@bob:example.org".to_owned(),
                participant_count: 2,
            }
        );
    }

    #[tokio::test]
    async fn a_decryption_failure_is_surfaced_against_the_membership() {
        let mut fx = fixture();
        fx.memberships
            .send(vec![member("bob", "@bob:example.org")])
            .unwrap();
        rostered(&fx, &["bob"]).await;

        let connection = adopt(&fx);
        connection
            .send(ConnectionEvent::EncryptionStateChanged {
                identity: "id-bob".to_owned(),
                state: FrameEncryptionState::MissingKey,
            })
            .unwrap();

        // The host learns which *member* is undecryptable, not which opaque
        // transport identity — and that no key ever reached them, which is the
        // difference between a signalling bug and a rotation in flight.
        assert_eq!(
            next_event(&mut fx.events).await,
            CallEvent::FrameEncryptionState {
                member_id: "bob".to_owned(),
                state: FrameEncryptionState::MissingKey,
                diagnostic: FrameEncryptionDiagnostic::NoKeyInstalled,
            }
        );
    }

    /// The same failure means something different once a key *has* been
    /// installed: the frames are carrying an index we were not given, so the key
    /// path is working and a rotation is simply in flight.
    #[tokio::test]
    async fn a_failure_after_an_import_reports_the_keys_it_holds() {
        let mut fx = fixture();
        fx.memberships
            .send(vec![member("bob", "@bob:example.org")])
            .unwrap();
        rostered(&fx, &["bob"]).await;

        fx.engine.notify_key_imported("id-bob", 4);
        assert_eq!(
            next_event(&mut fx.events).await,
            CallEvent::KeyImported {
                member_id: "bob".to_owned(),
                identity: "id-bob".to_owned(),
                key_index: 4,
            }
        );

        let connection = adopt(&fx);
        connection
            .send(ConnectionEvent::EncryptionStateChanged {
                identity: "id-bob".to_owned(),
                state: FrameEncryptionState::MissingKey,
            })
            .unwrap();

        assert_eq!(
            next_event(&mut fx.events).await,
            CallEvent::FrameEncryptionState {
                member_id: "bob".to_owned(),
                state: FrameEncryptionState::MissingKey,
                diagnostic: FrameEncryptionDiagnostic::KeysInstalled {
                    key_indices: vec![4]
                },
            }
        );
    }

    /// A refused key must reach the host, with the reason and the device that
    /// sent it. It is reported against a `member_id` and needs no membership or
    /// transport identity: the core refuses the key before either applies, and a
    /// key naming a member we have never seen is exactly the case worth reporting.
    #[tokio::test]
    async fn a_refused_key_surfaces_with_its_reason() {
        let mut fx = fixture();

        fx.engine.notify_key_discarded(DiscardedKey {
            member_id: "bob".to_owned(),
            key_index: Some(2),
            sender_user_id: Some("@bob:example.org".to_owned()),
            sender_device_id: Some("BOBDEV".to_owned()),
            reason: KeyRejection::SenderNotConfirmed,
        });

        assert_eq!(
            next_event(&mut fx.events).await,
            CallEvent::KeyDiscarded {
                member_id: "bob".to_owned(),
                key_index: Some(2),
                sender_user_id: Some("@bob:example.org".to_owned()),
                sender_device_id: Some("BOBDEV".to_owned()),
                reason: KeyRejection::SenderNotConfirmed,
            }
        );
    }

    /// A recovery carries no diagnostic — there is nothing to explain, and an
    /// `Ok` that still named a reason would read as a lingering fault.
    #[tokio::test]
    async fn a_recovered_stream_reports_no_diagnostic() {
        let mut fx = fixture();
        fx.memberships
            .send(vec![member("bob", "@bob:example.org")])
            .unwrap();
        rostered(&fx, &["bob"]).await;

        let connection = adopt(&fx);
        connection
            .send(ConnectionEvent::EncryptionStateChanged {
                identity: "id-bob".to_owned(),
                state: FrameEncryptionState::Ok,
            })
            .unwrap();

        assert_eq!(
            next_event(&mut fx.events).await,
            CallEvent::FrameEncryptionState {
                member_id: "bob".to_owned(),
                state: FrameEncryptionState::Ok,
                diagnostic: FrameEncryptionDiagnostic::NotApplicable,
            }
        );
    }

    /// "Who is talking" and "how loud" arrive in the same transport event, so
    /// they must leave together: a host that gets only the identities has to
    /// meter the PCM itself, decoding audio it may not otherwise need to answer
    /// a question the SFU already answered.
    #[tokio::test]
    async fn active_speakers_carry_their_audio_level() {
        let mut fx = fixture();
        fx.memberships
            .send(vec![member("bob", "@bob:example.org")])
            .unwrap();
        rostered(&fx, &["bob"]).await;

        let connection = adopt(&fx);
        connection
            .send(ConnectionEvent::ActiveSpeakers {
                speakers: vec![
                    SpeakingParticipant {
                        identity: "id-bob".to_owned(),
                        level: 0.75,
                    },
                    // No membership maps to this one, so it cannot be attributed
                    // and is dropped rather than reported against nobody.
                    SpeakingParticipant {
                        identity: "id-ghost".to_owned(),
                        level: 0.9,
                    },
                ],
            })
            .unwrap();

        assert_eq!(
            next_event(&mut fx.events).await,
            CallEvent::ActiveSpeakers {
                speakers: vec![SpeakingMember {
                    member_id: "bob".to_owned(),
                    level: 0.75,
                }],
            }
        );
    }

    #[tokio::test]
    async fn an_encryption_state_for_an_unmapped_identity_is_dropped() {
        let mut fx = fixture();
        let connection = adopt(&fx);
        connection
            .send(ConnectionEvent::EncryptionStateChanged {
                identity: "id-nobody".to_owned(),
                state: FrameEncryptionState::DecryptionFailed,
            })
            .unwrap();
        // Followed by something we can wait for, to prove the first produced
        // no event rather than merely being slower.
        connection
            .send(ConnectionEvent::ActiveSpeakers {
                speakers: Vec::new(),
            })
            .unwrap();

        assert_eq!(
            next_event(&mut fx.events).await,
            CallEvent::ActiveSpeakers {
                speakers: Vec::new(),
            }
        );
    }

    #[tokio::test]
    async fn receive_stats_come_from_the_subscribed_track() {
        let mut fx = fixture();
        fx.memberships
            .send(vec![member("bob", "@bob:example.org")])
            .unwrap();
        rostered(&fx, &["bob"]).await;

        // RTP arriving, nothing decoding: the exact case that is invisible at
        // the frame level, because frames are produced either way.
        let reported = ReceiveStats {
            packets_received: 900,
            bytes_received: 120_000,
            frames_decoded: 0,
            concealed_samples: 48_000,
            total_samples_received: 48_000,
            ..ReceiveStats::default()
        };
        let connection = adopt(&fx);
        connection
            .send(ConnectionEvent::TrackAdded {
                identity: "id-bob".to_owned(),
                kind: MediaStreamKind::Microphone,
                track: Arc::new(FakeTrack::with_stats(
                    MediaStreamKind::Microphone,
                    reported.clone(),
                )),
            })
            .unwrap();
        let _ = next_event(&mut fx.events).await; // StreamStarted

        assert_eq!(
            fx.engine
                .receive_stats("bob", MediaStreamKind::Microphone)
                .await,
            Some(reported)
        );
    }

    #[tokio::test]
    async fn the_receive_stats_future_is_send() {
        // uniffi's async exports require `Future + Send`. A `!Send` future
        // here would only break the FFI crate, which cannot be built without
        // libwebrtc — so assert it in the crate that can.
        fn assert_send<T: Send>(_: T) {}
        let fx = fixture();
        assert_send(fx.engine.receive_stats("bob", MediaStreamKind::Microphone));
    }

    fn stats_with(packets_received: u64) -> ReceiveStats {
        ReceiveStats {
            packets_received,
            ..ReceiveStats::default()
        }
    }

    /// Own connection adopted and one fake track per `(member, kind)` added,
    /// each answering `receive_stats` with the given counters.
    async fn with_tracks(fx: &Fixture, tracks: Vec<(&str, MediaStreamKind, FakeTrack)>) {
        let connection = adopt(fx);
        for (member_id, kind, track) in tracks {
            connection
                .send(ConnectionEvent::TrackAdded {
                    identity: format!("id-{member_id}"),
                    kind,
                    track: Arc::new(track),
                })
                .unwrap();
            wait_until(|| fx.engine.remote_track(member_id, kind).is_some()).await;
        }
    }

    #[tokio::test]
    async fn batched_receive_stats_follow_request_order_with_none_for_unknown() {
        use MediaStreamKind::{Camera, Microphone, ScreenShare};
        let fx = fixture();
        fx.memberships
            .send(vec![
                member("bob", "@bob:example.org"),
                member("carol", "@carol:example.org"),
            ])
            .unwrap();
        wait_until(|| fx.engine.participants().len() == 2).await;
        with_tracks(
            &fx,
            vec![
                (
                    "bob",
                    Microphone,
                    FakeTrack::with_stats(Microphone, stats_with(1)),
                ),
                ("bob", Camera, FakeTrack::with_stats(Camera, stats_with(2))),
                (
                    "carol",
                    Microphone,
                    FakeTrack::with_stats(Microphone, stats_with(3)),
                ),
            ],
        )
        .await;

        let request = [
            ("carol".to_owned(), Microphone),
            ("bob".to_owned(), Camera),
            ("dave".to_owned(), Microphone), // unknown member
            ("bob".to_owned(), Microphone),
            ("bob".to_owned(), ScreenShare),  // unsubscribed kind
            ("carol".to_owned(), Microphone), // duplicate: answered twice
        ];
        let results = fx.engine.receive_stats_for(&request).await;

        assert_eq!(
            results,
            vec![
                Some(stats_with(3)),
                Some(stats_with(2)),
                None,
                Some(stats_with(1)),
                None,
                Some(stats_with(3)),
            ]
        );
    }

    /// Three tracks share a barrier of three: only if all three
    /// `receive_stats` are in flight together does any of them return. A
    /// sequential loop deadlocks on the first, and the timeout says so.
    #[tokio::test]
    async fn batched_receive_stats_are_awaited_concurrently() {
        use MediaStreamKind::Microphone;
        let fx = fixture();
        fx.memberships
            .send(vec![
                member("a", "@a:example.org"),
                member("b", "@b:example.org"),
                member("c", "@c:example.org"),
            ])
            .unwrap();
        wait_until(|| fx.engine.participants().len() == 3).await;
        let gate = Arc::new(tokio::sync::Barrier::new(3));
        let gated = |n: u64| FakeTrack {
            kind: Microphone,
            stats: Some(stats_with(n)),
            gate: Some(gate.clone()),
        };
        with_tracks(
            &fx,
            vec![
                ("a", Microphone, gated(1)),
                ("b", Microphone, gated(2)),
                ("c", Microphone, gated(3)),
            ],
        )
        .await;

        let request: Vec<_> = ["a", "b", "c"]
            .into_iter()
            .map(|m| (m.to_owned(), Microphone))
            .collect();
        let results = tokio::time::timeout(
            Duration::from_secs(2),
            fx.engine.receive_stats_for(&request),
        )
        .await
        .expect("stats were awaited one at a time: a barrier of three never released");

        assert_eq!(
            results,
            vec![
                Some(stats_with(1)),
                Some(stats_with(2)),
                Some(stats_with(3))
            ]
        );
    }

    #[tokio::test]
    async fn batched_receive_stats_for_an_empty_request_are_empty() {
        let fx = fixture();
        assert!(fx.engine.receive_stats_for(&[]).await.is_empty());
    }

    #[tokio::test]
    async fn the_batched_receive_stats_future_is_send() {
        fn assert_send<T: Send>(_: T) {}
        let fx = fixture();
        let request = [("bob".to_owned(), MediaStreamKind::Microphone)];
        assert_send(fx.engine.receive_stats_for(&request));
    }

    #[tokio::test]
    async fn receive_stats_are_none_without_a_subscribed_stream() {
        let fx = fixture();
        assert_eq!(
            fx.engine
                .receive_stats("bob", MediaStreamKind::Microphone)
                .await,
            None
        );
    }

    #[tokio::test]
    async fn receive_stats_are_none_when_the_transport_reports_nothing() {
        let mut fx = fixture();
        fx.memberships
            .send(vec![member("bob", "@bob:example.org")])
            .unwrap();
        rostered(&fx, &["bob"]).await;

        let connection = adopt(&fx);
        connection
            .send(ConnectionEvent::TrackAdded {
                identity: "id-bob".to_owned(),
                kind: MediaStreamKind::Microphone,
                // `FakeTrack::new` leaves the trait default in place.
                track: Arc::new(FakeTrack::new(MediaStreamKind::Microphone)),
            })
            .unwrap();
        let _ = next_event(&mut fx.events).await;

        assert_eq!(
            fx.engine
                .receive_stats("bob", MediaStreamKind::Microphone)
                .await,
            None
        );
    }

    #[tokio::test]
    async fn departure_cleans_up_roster_and_tracks() {
        let mut fx = fixture();
        in_call(&fx).await;
        fx.memberships
            .send(vec![
                member("own", "@alice:example.org"),
                member("bob", "@bob:example.org"),
            ])
            .unwrap();
        rostered(&fx, &["bob"]).await;

        let connection = adopt(&fx);
        connection
            .send(ConnectionEvent::TrackAdded {
                identity: "id-bob".to_owned(),
                kind: MediaStreamKind::Microphone,
                track: Arc::new(FakeTrack::new(MediaStreamKind::Microphone)),
            })
            .unwrap();
        let _ = next_event(&mut fx.events).await; // ParticipantJoined
        let _ = next_event(&mut fx.events).await; // StreamStarted

        fx.memberships
            .send(vec![member("own", "@alice:example.org")])
            .unwrap();
        assert_eq!(
            next_event(&mut fx.events).await,
            CallEvent::ParticipantLeft {
                member_id: "bob".to_owned(),
                participant_count: 1,
            }
        );
        assert_eq!(fx.engine.participants().len(), 1);
        assert!(
            fx.engine
                .remote_track("bob", MediaStreamKind::Microphone)
                .is_none()
        );
    }

    /// 034 R24: a closed own connection is reconnected, and only a budget
    /// spent ends the call; never before 60 s.
    #[tokio::test(start_paused = true)]
    async fn the_reconnect_budget_ends_the_call_after_sixty_seconds() {
        let fx = fixture();
        let connection = adopt(&fx);
        connection
            .send(ConnectionEvent::Closed {
                message: "server shutting down".to_owned(),
            })
            .unwrap();
        tokio::time::sleep(Duration::from_secs(59)).await;
        assert!(matches!(*fx.status.borrow(), CallStatus::Connected { .. }));

        wait_until(|| {
            matches!(
                &*fx.status.borrow(),
                CallStatus::Ended {
                    reason: CallEnd::ConnectionClosed {
                        reason: matrix_rtc_call::ConnectionLossReason::Budget,
                        ..
                    },
                }
            )
        })
        .await;
        assert_eq!(
            fx.engine.interruptions().own_media,
            ConnectionState::Unreachable
        );
    }

    #[tokio::test]
    async fn reconnect_cycle_reports_degraded_media() {
        let fx = fixture();
        let connection = adopt(&fx);
        connection.send(ConnectionEvent::Reconnecting).unwrap();
        wait_until(|| degraded(&fx) == Some(true)).await;
        connection.send(ConnectionEvent::Reconnecting).unwrap(); // deduplicated
        connection.send(ConnectionEvent::Reconnected).unwrap();
        wait_until(|| degraded(&fx) == Some(false)).await;
    }

    #[tokio::test(start_paused = true)]
    async fn peer_focus_gets_its_own_connection() {
        let mut fx = fixture();
        fx.memberships
            .send(vec![
                member("own", "@alice:example.org"),
                member_on("bob", "@bob:example.org", PEER_FOCUS),
            ])
            .unwrap();
        rostered(&fx, &["own", "bob"]).await;

        // The engine connects to bob's focus by itself...
        wait_until(|| peer_sender(&fx, PEER_FOCUS).is_some()).await;
        assert_eq!(connect_count(&fx, PEER_FOCUS), 1);
        // ...but never to the own focus (that one is adopted).
        assert_eq!(connect_count(&fx, OWN_FOCUS), 0);

        // Media flowing over the peer connection maps onto the roster.
        peer_sender(&fx, PEER_FOCUS)
            .unwrap()
            .send(ConnectionEvent::TrackAdded {
                identity: "id-bob".to_owned(),
                kind: MediaStreamKind::Microphone,
                track: Arc::new(FakeTrack::new(MediaStreamKind::Microphone)),
            })
            .unwrap();
        assert_eq!(
            next_event(&mut fx.events).await,
            CallEvent::StreamStarted {
                member_id: "bob".to_owned(),
                kind: MediaStreamKind::Microphone,
            }
        );
    }

    #[tokio::test(start_paused = true)]
    async fn failed_peer_connect_retries_with_backoff() {
        let fx = fixture();
        fx.state.fail_attempts.store(2, Ordering::SeqCst);

        fx.memberships
            .send(vec![member_on("bob", "@bob:example.org", PEER_FOCUS)])
            .unwrap();

        // Two failures, then success on the third attempt (timers auto-advance
        // under the paused clock).
        wait_until(|| peer_sender(&fx, PEER_FOCUS).is_some()).await;
        assert_eq!(connect_count(&fx, PEER_FOCUS), 3);
    }

    #[tokio::test(start_paused = true)]
    async fn peer_connection_loss_drops_streams_and_reconnects() {
        let mut fx = fixture();
        fx.memberships
            .send(vec![member_on("bob", "@bob:example.org", PEER_FOCUS)])
            .unwrap();
        rostered(&fx, &["bob"]).await;

        wait_until(|| peer_sender(&fx, PEER_FOCUS).is_some()).await;
        peer_sender(&fx, PEER_FOCUS)
            .unwrap()
            .send(ConnectionEvent::TrackAdded {
                identity: "id-bob".to_owned(),
                kind: MediaStreamKind::Microphone,
                track: Arc::new(FakeTrack::new(MediaStreamKind::Microphone)),
            })
            .unwrap();
        let _ = next_event(&mut fx.events).await; // StreamStarted

        // The peer focus dies: bob's streams stop, media degrades, the call
        // does NOT end, and a reconnect follows.
        peer_sender(&fx, PEER_FOCUS)
            .unwrap()
            .send(ConnectionEvent::Closed {
                message: "gone".to_owned(),
            })
            .unwrap();

        assert_eq!(
            next_event(&mut fx.events).await,
            CallEvent::StreamStopped {
                member_id: "bob".to_owned(),
                kind: MediaStreamKind::Microphone,
            }
        );
        wait_until(|| degraded(&fx) == Some(true)).await;
        wait_until(|| connect_count(&fx, PEER_FOCUS) >= 2).await;
        wait_until(|| degraded(&fx) == Some(false)).await;
        assert!(
            fx.engine
                .participants()
                .iter()
                .any(|p| p.member_id == "bob")
        );
    }

    #[tokio::test(start_paused = true)]
    async fn idle_peer_connection_closes_after_grace() {
        let fx = fixture();
        fx.memberships
            .send(vec![member_on("bob", "@bob:example.org", PEER_FOCUS)])
            .unwrap();
        rostered(&fx, &["bob"]).await;
        wait_until(|| peer_sender(&fx, PEER_FOCUS).is_some()).await;

        // Bob leaves; the connection lingers for the grace period, then closes.
        fx.memberships.send(Vec::new()).unwrap();
        wait_until(|| fx.engine.participants().is_empty()).await;
        assert!(!closed(&fx, PEER_FOCUS));
        wait_until(|| closed(&fx, PEER_FOCUS)).await;
        assert_eq!(connect_count(&fx, PEER_FOCUS), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn returning_member_cancels_idle_close() {
        let fx = fixture();
        let bob = || member_on("bob", "@bob:example.org", PEER_FOCUS);
        fx.memberships.send(vec![bob()]).unwrap();
        rostered(&fx, &["bob"]).await;
        wait_until(|| peer_sender(&fx, PEER_FOCUS).is_some()).await;

        // Bob flaps: leaves and returns within the grace period.
        fx.memberships.send(Vec::new()).unwrap();
        wait_until(|| fx.engine.participants().is_empty()).await;
        tokio::time::sleep(IDLE_GRACE / 2).await;
        fx.memberships.send(vec![bob()]).unwrap();
        rostered(&fx, &["bob"]).await;

        // Well past the original grace deadline the connection is still the
        // first one, never closed or re-opened.
        tokio::time::sleep(IDLE_GRACE * 2).await;
        assert!(!closed(&fx, PEER_FOCUS));
        assert_eq!(connect_count(&fx, PEER_FOCUS), 1);
    }

    /// A host must be able to render itself from the same roster it renders
    /// everyone else from. Publishing raised no event, so its own entry lacked
    /// the microphone it was actively capturing — and alone in a call nothing
    /// later prompted a re-read to correct it.
    #[tokio::test]
    async fn publishing_puts_the_stream_on_our_own_roster_entry() {
        let mut fx = fixture();
        fx.memberships
            .send(vec![member("own", "@own:example.org")])
            .unwrap();
        rostered(&fx, &["own"]).await;
        let _connection = adopt(&fx);

        fx.engine
            .publish(PublishOptions::microphone())
            .await
            .expect("publish should succeed");

        assert_eq!(
            next_event(&mut fx.events).await,
            CallEvent::StreamStarted {
                member_id: "own".to_owned(),
                kind: MediaStreamKind::Microphone,
            }
        );
        let own = fx
            .engine
            .participants()
            .into_iter()
            .find(|p| p.is_local)
            .expect("we are on our own roster");
        assert_eq!(own.streams.len(), 1);
        assert_eq!(own.streams[0].kind, MediaStreamKind::Microphone);
        assert!(!own.streams[0].muted);
    }

    /// The reverse order of the test above, and the normal one: the SFU
    /// connection is adopted and published on before our own membership has
    /// round-tripped through the homeserver. The publication is live either
    /// way, so our roster entry has to show it once the membership lands —
    /// nothing publishes again later to correct it.
    #[tokio::test]
    async fn publishing_before_our_membership_lands_still_rosters_the_stream() {
        let mut fx = fixture();
        let _connection = adopt(&fx);

        fx.engine
            .publish(PublishOptions::microphone())
            .await
            .expect("publish should succeed");
        settle(&fx.engine).await;

        fx.memberships
            .send(vec![member("own", "@own:example.org")])
            .unwrap();

        // Our own arrival is not announced; its streams still are.
        assert_eq!(
            next_event(&mut fx.events).await,
            CallEvent::StreamStarted {
                member_id: "own".to_owned(),
                kind: MediaStreamKind::Microphone,
            }
        );
        let own = fx
            .engine
            .participants()
            .into_iter()
            .find(|p| p.is_local)
            .expect("we are on our own roster");
        assert_eq!(
            own.streams,
            vec![StreamState {
                kind: MediaStreamKind::Microphone,
                muted: false,
            }],
        );
    }

    /// A mute applied while our membership is still in flight reaches the
    /// transport, and a mute is signalling rather than media — peers see it
    /// whether or not they can decrypt our frames yet. Seeding the roster entry
    /// as unmuted when the membership lands would therefore contradict what the
    /// SFU is already telling everyone.
    #[tokio::test]
    async fn muting_before_our_membership_lands_is_reflected_when_it_does() {
        let mut fx = fixture();
        let _connection = adopt(&fx);

        fx.engine
            .publish(PublishOptions::microphone())
            .await
            .expect("publish should succeed");
        fx.engine
            .set_local_muted(MediaStreamKind::Microphone, true)
            .await
            .expect("muting a live publication should succeed");

        fx.memberships
            .send(vec![member("own", "@own:example.org")])
            .unwrap();

        // Our own arrival is not announced; its streams still are.
        assert_eq!(
            next_event(&mut fx.events).await,
            CallEvent::StreamStarted {
                member_id: "own".to_owned(),
                kind: MediaStreamKind::Microphone,
            }
        );
        assert_eq!(
            next_event(&mut fx.events).await,
            CallEvent::StreamMuted {
                member_id: "own".to_owned(),
                kind: MediaStreamKind::Microphone,
            }
        );
        let own = fx
            .engine
            .participants()
            .into_iter()
            .find(|p| p.is_local)
            .expect("we are on our own roster");
        assert_eq!(
            own.streams,
            vec![StreamState {
                kind: MediaStreamKind::Microphone,
                muted: true,
            }],
        );
    }

    /// Our membership can leave the roster and come back mid-call — a sticky
    /// entry that expired a moment before its refresh landed. The publication
    /// never went anywhere, so the rebuilt entry must carry it again.
    #[tokio::test]
    async fn our_streams_survive_our_membership_being_re_created() {
        let mut fx = fixture();
        fx.memberships
            .send(vec![member("own", "@own:example.org")])
            .unwrap();
        rostered(&fx, &["own"]).await;
        let _connection = adopt(&fx);

        fx.engine
            .publish(PublishOptions::microphone())
            .await
            .expect("publish should succeed");
        let _ = next_event(&mut fx.events).await; // StreamStarted

        // Neither our lapse nor our return is announced.
        fx.memberships.send(Vec::new()).unwrap();
        wait_until(|| fx.engine.participants().is_empty()).await;

        fx.memberships
            .send(vec![member("own", "@own:example.org")])
            .unwrap();
        assert_eq!(
            next_event(&mut fx.events).await,
            CallEvent::StreamStarted {
                member_id: "own".to_owned(),
                kind: MediaStreamKind::Microphone,
            }
        );
        let own = fx
            .engine
            .participants()
            .into_iter()
            .find(|p| p.is_local)
            .expect("we are back on our own roster");
        assert_eq!(
            own.streams,
            vec![StreamState {
                kind: MediaStreamKind::Microphone,
                muted: false,
            }],
        );
    }

    /// A lobby that joins muted publishes muted: the mute is announced with the
    /// track rather than applied after, so there is no window with a live
    /// unmuted microphone at the SFU. Both the transport and our own roster
    /// entry have to start out that way.
    #[tokio::test]
    async fn publishing_muted_tells_the_transport_and_starts_muted_on_the_roster() {
        let mut fx = fixture();
        fx.memberships
            .send(vec![member("own", "@own:example.org")])
            .unwrap();
        rostered(&fx, &["own"]).await;
        let _connection = adopt(&fx);

        fx.engine
            .publish(PublishOptions::microphone().muted())
            .await
            .expect("publish should succeed");

        assert_eq!(
            fx.state.published.lock().unwrap().as_slice(),
            &[(OWN_FOCUS.to_owned(), MediaStreamKind::Microphone, true)],
            "the mute must travel with the publish, or the track is live unmuted first",
        );
        assert_eq!(
            next_event(&mut fx.events).await,
            CallEvent::StreamStarted {
                member_id: "own".to_owned(),
                kind: MediaStreamKind::Microphone,
            }
        );
        assert_eq!(
            next_event(&mut fx.events).await,
            CallEvent::StreamMuted {
                member_id: "own".to_owned(),
                kind: MediaStreamKind::Microphone,
            }
        );
        assert_eq!(
            fx.engine
                .participants()
                .into_iter()
                .find(|p| p.is_local)
                .expect("we are on our own roster")
                .streams,
            vec![StreamState {
                kind: MediaStreamKind::Microphone,
                muted: true,
            }],
        );
        assert!(
            fx.state.local_mutes.lock().unwrap().is_empty(),
            "a pre-muted publish needs no follow-up mute call",
        );
    }

    /// The same publish with our membership still in flight: the seeded entry
    /// has to carry the mute too, or the lobby's choice is invisible until
    /// something else nudges the roster.
    #[tokio::test]
    async fn publishing_muted_before_our_membership_lands_stays_muted() {
        let mut fx = fixture();
        let _connection = adopt(&fx);

        fx.engine
            .publish(PublishOptions::microphone().muted())
            .await
            .expect("publish should succeed");
        settle(&fx.engine).await;

        fx.memberships
            .send(vec![member("own", "@own:example.org")])
            .unwrap();

        rostered(&fx, &["own"]).await;
        assert_eq!(
            next_event(&mut fx.events).await,
            CallEvent::StreamStarted {
                member_id: "own".to_owned(),
                kind: MediaStreamKind::Microphone,
            }
        );
        assert_eq!(
            next_event(&mut fx.events).await,
            CallEvent::StreamMuted {
                member_id: "own".to_owned(),
                kind: MediaStreamKind::Microphone,
            }
        );
        assert_eq!(
            fx.engine
                .participants()
                .into_iter()
                .find(|p| p.is_local)
                .expect("we are on our own roster")
                .streams,
            vec![StreamState {
                kind: MediaStreamKind::Microphone,
                muted: true,
            }],
        );
    }

    /// Unmuting a publication that went up muted is a plain flag flip — the
    /// point of publishing muted rather than not publishing at all.
    #[tokio::test]
    async fn unmuting_a_pre_muted_publication_needs_no_republish() {
        let mut fx = fixture();
        fx.memberships
            .send(vec![member("own", "@own:example.org")])
            .unwrap();
        rostered(&fx, &["own"]).await;
        let _connection = adopt(&fx);

        fx.engine
            .publish(PublishOptions::microphone().muted())
            .await
            .expect("publish should succeed");
        let _ = next_event(&mut fx.events).await; // StreamStarted
        let _ = next_event(&mut fx.events).await; // StreamMuted

        fx.engine
            .set_local_muted(MediaStreamKind::Microphone, false)
            .await
            .expect("unmuting a live publication should succeed");

        assert_eq!(
            next_event(&mut fx.events).await,
            CallEvent::StreamUnmuted {
                member_id: "own".to_owned(),
                kind: MediaStreamKind::Microphone,
            }
        );
        assert_eq!(
            *fx.state.local_mutes.lock().unwrap(),
            vec![(MediaStreamKind::Microphone, false)],
        );
        assert_eq!(
            fx.state.published.lock().unwrap().len(),
            1,
            "unmuting must not republish",
        );
    }

    /// Muting must reach the transport (so peers are told, rather than seeing a
    /// sender that merely stopped) *and* the roster, so the host does not have to
    /// keep its own copy of the answer.
    #[tokio::test]
    async fn muting_ourselves_reaches_the_transport_and_the_roster() {
        let mut fx = fixture();
        fx.memberships
            .send(vec![member("own", "@own:example.org")])
            .unwrap();
        rostered(&fx, &["own"]).await;
        let _connection = adopt(&fx);

        fx.engine
            .publish(PublishOptions::microphone())
            .await
            .expect("publish should succeed");
        let _ = next_event(&mut fx.events).await; // StreamStarted

        fx.engine
            .set_local_muted(MediaStreamKind::Microphone, true)
            .await
            .expect("muting a live publication should succeed");

        assert_eq!(
            next_event(&mut fx.events).await,
            CallEvent::StreamMuted {
                member_id: "own".to_owned(),
                kind: MediaStreamKind::Microphone,
            }
        );
        assert_eq!(
            *fx.state.local_mutes.lock().unwrap(),
            vec![(MediaStreamKind::Microphone, true)],
            "the mute must reach the transport, or peers just see a stalled sender",
        );
        assert!(
            fx.engine
                .participants()
                .into_iter()
                .find(|p| p.is_local)
                .expect("we are on our own roster")
                .streams[0]
                .muted
        );
    }

    /// Muting something we never published is an error, not a silent no-op that
    /// leaves the host believing it is muted.
    #[tokio::test]
    async fn muting_an_unpublished_kind_fails() {
        let fx = fixture();
        let _connection = adopt(&fx);

        assert!(
            fx.engine
                .set_local_muted(MediaStreamKind::Camera, true)
                .await
                .is_err()
        );
    }

    /// Unpublishing must reach the transport (so peers drop the stream rather
    /// than rendering an empty tile — the stopped-screen-share case) *and*
    /// remove it from our own roster entry.
    #[tokio::test]
    async fn unpublishing_removes_the_stream_from_our_roster_and_reaches_the_transport() {
        let mut fx = fixture();
        fx.memberships
            .send(vec![member("own", "@own:example.org")])
            .unwrap();
        rostered(&fx, &["own"]).await;
        let _connection = adopt(&fx);

        fx.engine
            .publish(PublishOptions::screen_share(VideoSourceConfig {
                width: 1920,
                height: 1080,
            }))
            .await
            .expect("publish should succeed");
        let _ = next_event(&mut fx.events).await; // StreamStarted

        fx.engine
            .unpublish(MediaStreamKind::ScreenShare)
            .await
            .expect("unpublishing a live publication should succeed");

        assert_eq!(
            next_event(&mut fx.events).await,
            CallEvent::StreamStopped {
                member_id: "own".to_owned(),
                kind: MediaStreamKind::ScreenShare,
            }
        );
        assert_eq!(
            *fx.state.unpublished.lock().unwrap(),
            vec![MediaStreamKind::ScreenShare],
            "the unpublish must reach the transport, or peers keep the stream",
        );
        assert!(
            fx.engine
                .participants()
                .into_iter()
                .find(|p| p.is_local)
                .expect("we are on our own roster")
                .streams
                .is_empty()
        );
    }

    /// Unpublishing something we never published is an error, mirroring mute.
    #[tokio::test]
    async fn unpublishing_an_unpublished_kind_fails() {
        let fx = fixture();
        let _connection = adopt(&fx);

        assert!(
            fx.engine
                .unpublish(MediaStreamKind::ScreenShare)
                .await
                .is_err()
        );
    }

    /// A fresh publish of the same kind after an unpublish must come back on
    /// the roster — nothing of the retracted publication may linger.
    #[tokio::test]
    async fn republishing_after_unpublish_puts_the_stream_back() {
        let mut fx = fixture();
        fx.memberships
            .send(vec![member("own", "@own:example.org")])
            .unwrap();
        rostered(&fx, &["own"]).await;
        let _connection = adopt(&fx);

        fx.engine
            .publish(PublishOptions::microphone())
            .await
            .expect("publish should succeed");
        let _ = next_event(&mut fx.events).await; // StreamStarted
        fx.engine
            .unpublish(MediaStreamKind::Microphone)
            .await
            .expect("unpublish should succeed");
        let _ = next_event(&mut fx.events).await; // StreamStopped

        fx.engine
            .publish(PublishOptions::microphone())
            .await
            .expect("re-publishing after an unpublish should succeed");

        assert_eq!(
            next_event(&mut fx.events).await,
            CallEvent::StreamStarted {
                member_id: "own".to_owned(),
                kind: MediaStreamKind::Microphone,
            }
        );
        assert_eq!(fx.state.published.lock().unwrap().len(), 2);
        let own = fx
            .engine
            .participants()
            .into_iter()
            .find(|p| p.is_local)
            .expect("we are on our own roster");
        assert_eq!(own.streams.len(), 1);
        assert_eq!(own.streams[0].kind, MediaStreamKind::Microphone);
    }

    /// A failed retraction keeps the publication: the transport already maps
    /// the room-closed case to success, so a failure means the room is live
    /// and the track is still published at the SFU. Dropping local state then
    /// would leave a stream peers keep rendering but that nothing can mute or
    /// retract anymore — for a screen share, a privacy bug.
    #[tokio::test]
    async fn unpublish_keeps_the_publication_when_the_transport_errors() {
        let mut fx = fixture();
        fx.memberships
            .send(vec![member("own", "@own:example.org")])
            .unwrap();
        rostered(&fx, &["own"]).await;
        let _connection = adopt(&fx);

        fx.engine
            .publish(PublishOptions::microphone())
            .await
            .expect("publish should succeed");
        let _ = next_event(&mut fx.events).await; // StreamStarted

        fx.state.fail_unpublish.store(true, Ordering::SeqCst);
        assert!(
            fx.engine
                .unpublish(MediaStreamKind::Microphone)
                .await
                .is_err()
        );

        // The stream is still on our roster and the publication still usable.
        assert_eq!(
            fx.engine
                .participants()
                .into_iter()
                .find(|p| p.is_local)
                .expect("we are on our own roster")
                .streams
                .len(),
            1
        );
        fx.engine
            .set_local_muted(MediaStreamKind::Microphone, true)
            .await
            .expect("the engine still holds the publication");
        let _ = next_event(&mut fx.events).await; // StreamMuted

        // A retry once the transport recovers retracts it for real.
        fx.state.fail_unpublish.store(false, Ordering::SeqCst);
        fx.engine
            .unpublish(MediaStreamKind::Microphone)
            .await
            .expect("the retry should succeed");
        assert_eq!(
            next_event(&mut fx.events).await,
            CallEvent::StreamStopped {
                member_id: "own".to_owned(),
                kind: MediaStreamKind::Microphone,
            }
        );
        assert_eq!(
            *fx.state.unpublished.lock().unwrap(),
            vec![MediaStreamKind::Microphone],
        );
    }

    #[tokio::test]
    async fn publish_targets_the_own_focus_connection() {
        let fx = fixture();

        // Before the own connection is adopted, publishing fails.
        assert!(
            fx.engine
                .publish(PublishOptions::microphone())
                .await
                .is_err()
        );

        let _own = adopt(&fx);
        let handle = fx
            .engine
            .publish(PublishOptions::microphone())
            .await
            .expect("publish should succeed once the own focus is up");
        assert_eq!(handle.kind(), MediaStreamKind::Microphone);
        assert_eq!(
            fx.state.published.lock().unwrap().as_slice(),
            &[(OWN_FOCUS.to_owned(), MediaStreamKind::Microphone, false)]
        );
    }

    #[tokio::test(start_paused = true)]
    async fn constraints_are_debounced_and_apply_the_latest_value() {
        let fx = fixture();
        fx.memberships
            .send(vec![member_on("bob", "@bob:example.org", PEER_FOCUS)])
            .unwrap();
        rostered(&fx, &["bob"]).await;
        wait_until(|| peer_sender(&fx, PEER_FOCUS).is_some()).await;

        // Two rapid updates: only the final state reaches the transport.
        fx.engine.set_constraints(
            "bob",
            MediaStreamKind::Camera,
            MediaConstraints {
                visible: false,
                ..Default::default()
            },
        );
        fx.engine.set_constraints(
            "bob",
            MediaStreamKind::Camera,
            MediaConstraints {
                visible: true,
                detail: matrix_rtc_transport::VideoDetail::Quality(
                    matrix_rtc_transport::QualityLimit::Low,
                ),
                ..Default::default()
            },
        );

        wait_until(|| !fx.state.applied.lock().unwrap().is_empty()).await;
        // Give any stale timer a chance to (wrongly) fire as well.
        tokio::time::sleep(CONSTRAINTS_DEBOUNCE * 3).await;

        let applied = fx.state.applied.lock().unwrap().clone();
        assert_eq!(applied.len(), 1, "debounce should coalesce to one apply");
        let (identity, kind, resolved) = &applied[0];
        assert_eq!(identity, "id-bob");
        assert_eq!(*kind, MediaStreamKind::Camera);
        assert_eq!(resolved.demand, matrix_rtc_transport::StreamDemand::Active);
        assert_eq!(
            resolved.detail,
            matrix_rtc_transport::VideoDetail::Quality(matrix_rtc_transport::QualityLimit::Low)
        );
    }

    #[tokio::test(start_paused = true)]
    async fn constraints_reapply_on_stream_start_and_reconnect() {
        let fx = fixture();
        fx.memberships
            .send(vec![member_on("bob", "@bob:example.org", PEER_FOCUS)])
            .unwrap();
        rostered(&fx, &["bob"]).await;
        wait_until(|| peer_sender(&fx, PEER_FOCUS).is_some()).await;

        fx.engine.set_constraints(
            "bob",
            MediaStreamKind::Camera,
            MediaConstraints {
                low_bandwidth: true,
                ..Default::default()
            },
        );
        wait_until(|| fx.state.applied.lock().unwrap().len() == 1).await;

        // The stream appearing re-applies immediately (fresh subscriptions
        // start from server defaults)...
        peer_sender(&fx, PEER_FOCUS)
            .unwrap()
            .send(ConnectionEvent::TrackAdded {
                identity: "id-bob".to_owned(),
                kind: MediaStreamKind::Camera,
                track: Arc::new(FakeTrack::new(MediaStreamKind::Camera)),
            })
            .unwrap();
        wait_until(|| fx.state.applied.lock().unwrap().len() == 2).await;
        // low_bandwidth folds video to a pause (subscription kept).
        assert_eq!(
            fx.state.applied.lock().unwrap()[1].2.demand,
            matrix_rtc_transport::StreamDemand::Paused
        );

        // ...and so does a transport-level reconnect.
        peer_sender(&fx, PEER_FOCUS)
            .unwrap()
            .send(ConnectionEvent::Reconnected)
            .unwrap();
        wait_until(|| fx.state.applied.lock().unwrap().len() == 3).await;
    }

    /// A host hides the video of a member it shows unable to decrypt; the stream
    /// keeps flowing at the lowest layer meanwhile, so a key that arrives later
    /// can be seen to work, and the host's own constraints come back with it.
    #[tokio::test(start_paused = true)]
    async fn an_undecryptable_members_hidden_video_keeps_flowing_until_it_decrypts() {
        let fx = fixture();
        fx.memberships
            .send(vec![member_on("bob", "@bob:example.org", PEER_FOCUS)])
            .unwrap();
        rostered(&fx, &["bob"]).await;
        wait_until(|| peer_sender(&fx, PEER_FOCUS).is_some()).await;
        let last = |fx: &Fixture| fx.state.applied.lock().unwrap().last().map(|(_, _, r)| *r);
        let encryption = |state| ConnectionEvent::EncryptionStateChanged {
            identity: "id-bob".to_owned(),
            state,
        };

        peer_sender(&fx, PEER_FOCUS)
            .unwrap()
            .send(encryption(FrameEncryptionState::MissingKey))
            .unwrap();
        fx.engine.set_constraints(
            "bob",
            MediaStreamKind::Camera,
            MediaConstraints {
                visible: false,
                ..Default::default()
            },
        );
        wait_until(|| !fx.state.applied.lock().unwrap().is_empty()).await;
        let probing = last(&fx).unwrap();
        assert_eq!(probing.demand, matrix_rtc_transport::StreamDemand::Active);
        assert_eq!(
            probing.detail,
            matrix_rtc_transport::VideoDetail::Quality(matrix_rtc_transport::QualityLimit::Low)
        );

        let applied = fx.state.applied.lock().unwrap().len();
        peer_sender(&fx, PEER_FOCUS)
            .unwrap()
            .send(encryption(FrameEncryptionState::Ok))
            .unwrap();
        wait_until(|| fx.state.applied.lock().unwrap().len() > applied).await;
        assert_eq!(
            last(&fx).unwrap().demand,
            matrix_rtc_transport::StreamDemand::Paused,
            "decrypting again, the host's hidden stream pauses as it asked"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn shutdown_detaches_media_and_closes_peer_connections() {
        let fx = fixture();
        fx.memberships
            .send(vec![member_on("bob", "@bob:example.org", PEER_FOCUS)])
            .unwrap();
        rostered(&fx, &["bob"]).await;
        wait_until(|| peer_sender(&fx, PEER_FOCUS).is_some()).await;
        let own = adopt(&fx);

        fx.engine.shutdown().await;
        assert_eq!(*fx.status.borrow(), CallStatus::Connecting);
        wait_until(|| closed(&fx, PEER_FOCUS)).await;
        // The adopted own connection is left to its owner.
        assert!(!closed(&fx, OWN_FOCUS));
        drop(own);
    }

    #[tokio::test(start_paused = true)]
    async fn a_closed_slot_ends_the_call_and_closes_the_own_connection() {
        let fx = fixture();
        let _own = adopt(&fx);
        settle(&fx.engine).await;

        fx.ended
            .send_replace(Some(LeaveReason::new(LeaveCode::SlotClosed)));

        wait_until(|| closed(&fx, OWN_FOCUS)).await;
        // The host's own teardown afterwards is harmless.
        fx.engine.shutdown().await;
    }

    #[tokio::test(start_paused = true)]
    async fn a_host_leave_ends_the_call_and_closes_the_own_connection() {
        let fx = fixture();
        let _own = adopt(&fx);
        settle(&fx.engine).await;

        fx.ended
            .send_replace(Some(LeaveReason::new(LeaveCode::Leave)));

        wait_until(|| closed(&fx, OWN_FOCUS)).await;
    }

    // ---- tiles (spec 002) ---------------------------------------------------

    fn order_ids(fx: &Fixture) -> Vec<String> {
        fx.engine
            .tiles()
            .order
            .iter()
            .map(|r| r.id.member_id.clone())
            .collect()
    }

    /// Tiles are a projection of the roster: they appear and disappear with
    /// memberships, through the same publish.
    #[tokio::test]
    async fn tiles_ride_on_the_roster() {
        let fx = fixture();
        fx.memberships
            .send(vec![
                member("own", "@alice:example.org"),
                member("bob", "@bob:example.org"),
            ])
            .unwrap();
        wait_until(|| order_ids(&fx) == ["bob"]).await;

        fx.memberships
            .send(vec![member("own", "@alice:example.org")])
            .unwrap();
        wait_until(|| order_ids(&fx).is_empty()).await;
    }

    /// R7, C3: our own tile is published beside the list, never in it, and is
    /// never a hero. `None` until our membership is on the roster.
    #[tokio::test]
    async fn own_tile_is_beside_the_list_not_in_it() {
        let fx = fixture();
        assert!(fx.engine.local_state().is_none());

        fx.memberships
            .send(vec![
                member("own", "@alice:example.org"),
                member("bob", "@bob:example.org"),
            ])
            .unwrap();
        wait_until(|| fx.engine.local_state().is_some()).await;

        let local = fx.engine.local_state().unwrap();
        assert_eq!(local.tile.member_id, "own");
        assert!(!local.tile.hero);
        assert!(!local.is_screen_sharing);
        assert!(order_ids(&fx).iter().all(|id| id != "own"));
    }

    /// C12: the window bounds the full records, never the order. The default
    /// is everything; an explicitly named tile appears at its rank position.
    #[tokio::test]
    async fn detail_window_bounds_the_records_not_the_order() {
        let fx = fixture();
        fx.memberships
            .send(vec![
                member("own", "@alice:example.org"),
                member("a", "@a:example.org"),
                member("b", "@b:example.org"),
                member("c", "@c:example.org"),
                member("d", "@d:example.org"),
            ])
            .unwrap();
        wait_until(|| fx.engine.tiles().order.len() == 4).await;
        assert_eq!(
            fx.engine.tiles().detail.len(),
            4,
            "default window is everything"
        );

        let last = fx.engine.tiles().order[3].id.clone();
        fx.engine.set_detail_window(0, 2, [last.clone()]);
        wait_until(|| fx.engine.tiles().detail.len() == 3).await;

        let roster = fx.engine.tiles();
        assert_eq!(roster.order.len(), 4, "order is never truncated");
        let detail: Vec<TileId> = roster.detail.iter().map(|t| t.id()).collect();
        assert_eq!(
            detail,
            vec![roster.order[0].id.clone(), roster.order[1].id.clone(), last],
            "range first, then the named tile at its own rank position"
        );
    }

    /// R14, C8: the screen-sharing flag follows the publication — up **and
    /// unmuted** — not our intent, so it goes false however the share ends.
    #[tokio::test]
    async fn screen_sharing_follows_the_publication_up_and_unmuted() {
        let fx = fixture();
        fx.memberships
            .send(vec![member("own", "@own:example.org")])
            .unwrap();
        wait_until(|| fx.engine.local_state().is_some()).await;
        let _connection = adopt(&fx);
        let sharing = || fx.engine.local_state().unwrap().is_screen_sharing;
        assert!(!sharing());

        fx.engine
            .publish(PublishOptions::screen_share(VideoSourceConfig {
                width: 1920,
                height: 1080,
            }))
            .await
            .expect("publish should succeed");
        wait_until(sharing).await;

        // Muted is "not sharing": one consumer publishes muted and unmutes
        // only once capture is live, and the flag must not lead the picture.
        fx.engine
            .set_local_muted(MediaStreamKind::ScreenShare, true)
            .await
            .expect("mute should succeed");
        wait_until(|| !sharing()).await;
        fx.engine
            .set_local_muted(MediaStreamKind::ScreenShare, false)
            .await
            .expect("unmute should succeed");
        wait_until(sharing).await;

        fx.engine
            .unpublish(MediaStreamKind::ScreenShare)
            .await
            .expect("unpublish should succeed");
        wait_until(|| !sharing()).await;
    }

    // ---- stability (spec 002 R10, R11) ---------------------------------------
    //
    // Paused clock. `after(ms)` advances it: the clock only moves once every
    // task is idle, so the actor has always drained its mailbox — and handled
    // any timer that fired on the way — by the time it returns.

    async fn after(ms: u64) {
        tokio::time::sleep(Duration::from_millis(ms)).await;
    }

    fn speakers(ids: &[&str]) -> ConnectionEvent {
        ConnectionEvent::ActiveSpeakers {
            speakers: ids
                .iter()
                .map(|id| SpeakingParticipant {
                    identity: format!("id-{id}"),
                    level: 0.5,
                })
                .collect(),
        }
    }

    fn speaking(fx: &Fixture, member_id: &str) -> bool {
        fx.engine
            .tiles()
            .detail
            .iter()
            .any(|t| t.member_id == member_id && t.speaking)
    }

    fn mic_muted(fx: &Fixture, member_id: &str) -> bool {
        fx.engine
            .tiles()
            .detail
            .iter()
            .find(|t| t.member_id == member_id)
            .is_some_and(|t| t.microphone_muted)
    }

    fn hand(member_id: &str, raised_at_ms: u64) -> RaisedHand {
        RaisedHand {
            member_id: member_id.to_owned(),
            sender: format!("@{member_id}:example.org"),
            reaction_event_id: format!("${member_id}"),
            raised_at_ms,
            from_backfill: false,
        }
    }

    /// Own + a, b, c on the roster and published, in that order.
    async fn with_abc(fx: &Fixture) {
        fx.memberships
            .send(vec![
                member("own", "@alice:example.org"),
                member("a", "@a:example.org"),
                member("b", "@b:example.org"),
                member("c", "@c:example.org"),
            ])
            .unwrap();
        wait_until(|| order_ids(fx) == ["a", "b", "c"]).await;
    }

    /// Records every order consumers are given from now on.
    fn observe_orders(fx: &Fixture) -> Arc<Mutex<Vec<Vec<String>>>> {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let mut rx = fx.engine.subscribe_tiles();
        rx.borrow_and_update();
        let sink = seen.clone();
        tokio::spawn(async move {
            while rx.changed().await.is_ok() {
                let ids: Vec<String> = rx
                    .borrow()
                    .order
                    .iter()
                    .map(|r| r.id.member_id.clone())
                    .collect();
                sink.lock().unwrap().push(ids);
            }
        });
        seen
    }

    /// Own + a, b, c published, with the own connection adopted so transport
    /// speaker events reach the engine.
    async fn with_abc_connected(fx: &Fixture) -> UnboundedSender<ConnectionEvent> {
        with_abc(fx).await;
        adopt(fx)
    }

    // The flag is raw (R11) and only the rank is damped (R10). A promotion
    // lands after `promote` plus the coalesce window, since it is a reorder.

    #[tokio::test(start_paused = true)]
    async fn r10_speaking_is_reported_at_once_and_ranked_only_after_sustained_voice() {
        let fx = fixture();
        let connection = with_abc_connected(&fx).await;
        connection.send(speakers(&["c"])).unwrap();
        after(50).await;
        assert!(speaking(&fx, "c"), "the flag is not damped");
        assert_eq!(order_ids(&fx), ["a", "b", "c"], "the rank is");
        after(1400).await;
        assert_eq!(
            order_ids(&fx),
            ["a", "b", "c"],
            "1.5 s of voice is required"
        );
        after(400).await;
        assert_eq!(order_ids(&fx), ["c", "a", "b"]);
    }

    #[tokio::test(start_paused = true)]
    async fn r10_a_speaker_stops_at_once_and_keeps_their_rank_through_the_cooldown() {
        let fx = fixture();
        let connection = with_abc_connected(&fx).await;
        connection.send(speakers(&["c"])).unwrap();
        after(2000).await;
        assert_eq!(order_ids(&fx), ["c", "a", "b"]);
        connection.send(speakers(&[])).unwrap();
        after(50).await;
        assert!(!speaking(&fx, "c"), "the flag is not damped");
        assert_eq!(order_ids(&fx), ["c", "a", "b"], "the rank is");
        after(1400).await;
        assert_eq!(
            order_ids(&fx),
            ["c", "a", "b"],
            "still ranked inside the cooldown"
        );
        after(400).await;
        assert_eq!(order_ids(&fx), ["a", "b", "c"]);
    }

    #[tokio::test(start_paused = true)]
    async fn r10_a_burst_shorter_than_the_promotion_never_moves_the_tile() {
        let fx = fixture();
        let connection = with_abc_connected(&fx).await;
        let seen = observe_orders(&fx);
        connection.send(speakers(&["c"])).unwrap();
        after(500).await;
        assert!(speaking(&fx, "c"));
        connection.send(speakers(&[])).unwrap();
        after(2500).await;
        assert!(!speaking(&fx, "c"));
        assert!(
            seen.lock()
                .unwrap()
                .iter()
                .all(|order| order.first().map(String::as_str) == Some("a")),
            "the promotion timer went stale when the voice stopped"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn r10_the_thresholds_are_configurable() {
        let fx = fixture_with(StabilityConfig {
            promote: Duration::from_millis(500),
            ..StabilityConfig::default()
        });
        let connection = with_abc_connected(&fx).await;
        connection.send(speakers(&["c"])).unwrap();
        after(400).await;
        assert_eq!(order_ids(&fx), ["a", "b", "c"]);
        after(500).await;
        assert_eq!(order_ids(&fx), ["c", "a", "b"]);
    }

    #[tokio::test(start_paused = true)]
    async fn r11_the_first_order_is_not_coalesced() {
        let fx = fixture();
        fx.memberships
            .send(vec![
                member("own", "@alice:example.org"),
                member("bob", "@bob:example.org"),
            ])
            .unwrap();
        after(50).await;
        assert_eq!(
            order_ids(&fx),
            ["bob"],
            "populating an empty order is not a reorder"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn r11_rank_changes_inside_the_window_land_as_one_reorder() {
        let fx = fixture();
        with_abc(&fx).await;
        let seen = observe_orders(&fx);

        // Two rank changes 100 ms apart: c raises a hand, then b.
        fx.raised_hands.send(vec![hand("c", 1)]).unwrap();
        after(100).await;
        fx.raised_hands
            .send(vec![hand("c", 1), hand("b", 2)])
            .unwrap();
        after(400).await;

        let mut orders = seen.lock().unwrap().clone();
        orders.dedup();
        assert_eq!(
            orders,
            vec![vec!["a", "b", "c"], vec!["c", "b", "a"]],
            "the old order held with fresh state, then one reorder — never [c, a, b] in between"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn join_order_is_the_first_event_seen_and_a_refresh_does_not_move_it() {
        let fx = fixture();
        let at = |id: &str, ts: u64| JoinedMembership {
            origin_server_ts: Some(ts),
            ..member(id, &format!("@{id}:example.org"))
        };
        // member_id order would be [a, b]; join order is [b, a].
        fx.memberships
            .send(vec![at("a", 2000), at("b", 1000)])
            .unwrap();
        wait_until(|| order_ids(&fx) == ["b", "a"]).await;

        // b's sticky refresh carries a later timestamp.
        fx.memberships
            .send(vec![at("a", 2000), at("b", 3000)])
            .unwrap();
        after(400).await;
        assert_eq!(order_ids(&fx), ["b", "a"]);
    }

    #[tokio::test(start_paused = true)]
    async fn under_the_ranking_threshold_a_hand_does_not_reorder_until_it_drops() {
        let fx = fixture();
        fx.engine.set_ranking_threshold(3);
        with_abc(&fx).await;
        let seen = observe_orders(&fx);

        fx.raised_hands.send(vec![hand("c", 1)]).unwrap();
        after(400).await;
        // Spotlight: one tile shown, so the three are ranked again.
        fx.engine.set_ranking_threshold(1);
        after(400).await;

        let mut orders = seen.lock().unwrap().clone();
        orders.dedup();
        assert_eq!(orders, vec![vec!["a", "b", "c"], vec!["c", "a", "b"]]);
    }

    #[tokio::test(start_paused = true)]
    async fn r11_state_is_never_delayed_by_the_coalesce_window() {
        let fx = fixture();
        with_abc(&fx).await;
        let connection = adopt(&fx);
        connection
            .send(ConnectionEvent::TrackAdded {
                identity: "id-b".to_owned(),
                kind: MediaStreamKind::Microphone,
                track: Arc::new(FakeTrack::new(MediaStreamKind::Microphone)),
            })
            .unwrap();
        wait_until(|| !mic_muted(&fx, "b")).await;

        // c raises a hand: the order will change, so a window opens.
        fx.raised_hands.send(vec![hand("c", 1)]).unwrap();
        after(100).await;
        // b mutes inside it.
        connection
            .send(ConnectionEvent::TrackMuted {
                identity: "id-b".to_owned(),
                kind: MediaStreamKind::Microphone,
            })
            .unwrap();
        after(50).await;
        assert_eq!(
            order_ids(&fx),
            ["a", "b", "c"],
            "the order waits for the window"
        );
        assert!(mic_muted(&fx, "b"), "the mute does not");

        after(200).await;
        assert_eq!(order_ids(&fx), ["c", "a", "b"]);
    }

    #[tokio::test(start_paused = true)]
    async fn r11_a_member_leaving_inside_the_window_leaves_the_order_at_once() {
        let fx = fixture();
        with_abc(&fx).await;
        fx.raised_hands.send(vec![hand("c", 1)]).unwrap();
        after(100).await;
        fx.memberships
            .send(vec![
                member("own", "@alice:example.org"),
                member("a", "@a:example.org"),
                member("c", "@c:example.org"),
            ])
            .unwrap();
        after(50).await;
        assert_eq!(
            order_ids(&fx),
            ["a", "c"],
            "b is gone now; c has not moved yet"
        );
        after(200).await;
        assert_eq!(order_ids(&fx), ["c", "a"]);
    }

    /// 034: connection state, decryptability, the lapse's pause and the
    /// interruptions record.
    mod resilience {
        use super::*;
        use crate::tile::TileKind;

        fn record(fx: &Fixture) -> Interruptions {
            fx.engine.interruptions()
        }

        fn unsupported(member_id: &str) -> JoinedMembership {
            let mut member = member(member_id, &format!("@{member_id}:example.org"));
            member.transports = vec![RtcTransport::Unsupported(
                matrix_rtc_core::UnsupportedTransport {
                    transport_type: "p2p".to_owned(),
                    extra_fields: Default::default(),
                },
            )];
            member
        }

        fn tile(fx: &Fixture, member_id: &str) -> CallTile {
            fx.engine
                .tiles()
                .detail
                .into_iter()
                .find(|tile| tile.member_id == member_id && tile.kind == TileKind::Person)
                .expect("a tile")
        }

        async fn with_bob_on_a_peer_focus(fx: &Fixture) -> UnboundedSender<ConnectionEvent> {
            adopt(fx);
            fx.memberships
                .send(vec![
                    member("own", "@alice:example.org"),
                    member_on("bob", "@bob:example.org", PEER_FOCUS),
                    unsupported("carol"),
                ])
                .unwrap();
            rostered(fx, &["own", "bob", "carol"]).await;
            wait_until(|| peer_sender(fx, PEER_FOCUS).is_some()).await;
            peer_sender(fx, PEER_FOCUS).unwrap()
        }

        /// R28: each tile carries its member's connection; `degraded` stays.
        #[tokio::test]
        async fn each_members_tile_carries_its_connection_state() {
            let fx = fixture();
            let peer = with_bob_on_a_peer_focus(&fx).await;
            wait_until(|| record(&fx).members_unreachable == ["carol"]).await;
            assert_eq!(tile(&fx, "bob").connection, ConnectionState::Connected);
            assert_eq!(tile(&fx, "carol").connection, ConnectionState::Unreachable);

            peer.send(ConnectionEvent::Reconnecting).unwrap();
            wait_until(|| record(&fx).members_reconnecting == ["bob"]).await;
            assert_eq!(tile(&fx, "bob").connection, ConnectionState::Reconnecting);
            assert_eq!(
                participant(&fx, "bob").connection,
                ConnectionState::Reconnecting
            );
            assert_eq!(
                fx.engine.local_state().unwrap().tile.connection,
                ConnectionState::Connected
            );
            assert_eq!(degraded(&fx), Some(true));

            peer.send(ConnectionEvent::Reconnected).unwrap();
            wait_until(|| record(&fx).members_reconnecting.is_empty()).await;
            assert_eq!(tile(&fx, "bob").connection, ConnectionState::Connected);
            assert_eq!(degraded(&fx), Some(false));
        }

        /// R23: the own tile and the record both carry our own SFU's state.
        #[tokio::test]
        async fn the_own_tile_reports_our_connection_state() {
            let fx = fixture();
            in_call(&fx).await;
            let own = adopt(&fx);
            wait_until(|| record(&fx).own_media == ConnectionState::Connected).await;

            own.send(ConnectionEvent::Reconnecting).unwrap();
            wait_until(|| record(&fx).own_media == ConnectionState::Reconnecting).await;
            assert_eq!(
                fx.engine.local_state().unwrap().tile.connection,
                ConnectionState::Reconnecting
            );
            own.send(ConnectionEvent::Reconnected).unwrap();
            wait_until(|| record(&fx).own_media == ConnectionState::Connected).await;
            assert_eq!(
                fx.engine.local_state().unwrap().tile.connection,
                ConnectionState::Connected
            );
        }

        /// R23, C5: `own_media` is in the record from its first value, before
        /// our own tile exists.
        #[tokio::test]
        async fn own_media_is_in_the_first_record_before_our_membership_lands() {
            let fx = fixture();
            assert_eq!(
                record(&fx).own_media,
                ConnectionState::Connecting,
                "not adopted"
            );
            let own = adopt(&fx);
            wait_until(|| record(&fx).own_media == ConnectionState::Connected).await;
            own.send(ConnectionEvent::Reconnecting).unwrap();
            wait_until(|| record(&fx).own_media == ConnectionState::Reconnecting).await;
            assert!(fx.engine.local_state().is_none());
        }

        /// R33: an undecryptable member is flagged; `has_video` and the order
        /// do not move.
        #[tokio::test(start_paused = true)]
        async fn an_undecryptable_member_is_flagged_and_has_video_is_untouched() {
            let fx = fixture();
            in_call(&fx).await;
            let own = adopt(&fx);
            fx.memberships
                .send(vec![
                    member("own", "@alice:example.org"),
                    member("bob", "@bob:example.org"),
                    member("carol", "@carol:example.org"),
                ])
                .unwrap();
            rostered(&fx, &["own", "bob", "carol"]).await;
            own.send(ConnectionEvent::TrackAdded {
                identity: "id-bob".to_owned(),
                kind: MediaStreamKind::Camera,
                track: Arc::new(FakeTrack::new(MediaStreamKind::Camera)),
            })
            .unwrap();
            wait_until(|| tile(&fx, "bob").has_video).await;
            let order = order_ids(&fx);

            own.send(ConnectionEvent::EncryptionStateChanged {
                identity: "id-bob".to_owned(),
                state: FrameEncryptionState::MissingKey,
            })
            .unwrap();
            wait_until(|| record(&fx).unable_to_decrypt == ["bob"]).await;
            assert_eq!(
                tile(&fx, "bob").unable_to_decrypt,
                Some(UnableToDecrypt::MissingKey)
            );
            assert!(tile(&fx, "bob").has_video);
            assert_eq!(order_ids(&fx), order);

            own.send(ConnectionEvent::EncryptionStateChanged {
                identity: "id-bob".to_owned(),
                state: FrameEncryptionState::Ok,
            })
            .unwrap();
            wait_until(|| record(&fx).unable_to_decrypt.is_empty()).await;
            assert_eq!(tile(&fx, "bob").unable_to_decrypt, None);
        }

        /// C6: frames failing for less than `KEY_GRACE` without a break, as
        /// around a key rotation, never show the member undecryptable; each
        /// spell counts from its own start.
        #[tokio::test(start_paused = true)]
        async fn a_key_rotation_blip_is_not_shown_undecryptable() {
            let fx = fixture();
            in_call(&fx).await;
            let own = adopt(&fx);
            fx.memberships
                .send(vec![
                    member("own", "@alice:example.org"),
                    member("bob", "@bob:example.org"),
                ])
                .unwrap();
            rostered(&fx, &["own", "bob"]).await;
            own.send(ConnectionEvent::TrackAdded {
                identity: "id-bob".to_owned(),
                kind: MediaStreamKind::Camera,
                track: Arc::new(FakeTrack::new(MediaStreamKind::Camera)),
            })
            .unwrap();
            let state = |state| {
                own.send(ConnectionEvent::EncryptionStateChanged {
                    identity: "id-bob".to_owned(),
                    state,
                })
                .unwrap();
            };

            state(FrameEncryptionState::MissingKey);
            tokio::time::sleep(KEY_GRACE - Duration::from_millis(500)).await;
            state(FrameEncryptionState::Ok);
            state(FrameEncryptionState::MissingKey);
            tokio::time::sleep(KEY_GRACE - Duration::from_millis(500)).await;
            assert_eq!(
                tile(&fx, "bob").unable_to_decrypt,
                None,
                "a new spell, not yet past"
            );
            assert!(record(&fx).unable_to_decrypt.is_empty());

            tokio::time::sleep(Duration::from_millis(600)).await;
            assert_eq!(
                tile(&fx, "bob").unable_to_decrypt,
                Some(UnableToDecrypt::MissingKey)
            );
            assert_eq!(record(&fx).unable_to_decrypt, ["bob"]);
        }

        /// C6: a host can set the key grace.
        #[tokio::test(start_paused = true)]
        async fn the_key_grace_is_configurable() {
            let fx = fixture_with(StabilityConfig {
                key_grace: Duration::from_secs(10),
                ..StabilityConfig::default()
            });
            in_call(&fx).await;
            let own = adopt(&fx);
            fx.memberships
                .send(vec![
                    member("own", "@alice:example.org"),
                    member("bob", "@bob:example.org"),
                ])
                .unwrap();
            rostered(&fx, &["own", "bob"]).await;
            own.send(ConnectionEvent::TrackAdded {
                identity: "id-bob".to_owned(),
                kind: MediaStreamKind::Camera,
                track: Arc::new(FakeTrack::new(MediaStreamKind::Camera)),
            })
            .unwrap();
            own.send(ConnectionEvent::EncryptionStateChanged {
                identity: "id-bob".to_owned(),
                state: FrameEncryptionState::MissingKey,
            })
            .unwrap();
            tokio::time::sleep(Duration::from_secs(9)).await;
            assert_eq!(tile(&fx, "bob").unable_to_decrypt, None);
            tokio::time::sleep(Duration::from_secs(2)).await;
            assert_eq!(
                tile(&fx, "bob").unable_to_decrypt,
                Some(UnableToDecrypt::MissingKey)
            );
        }

        /// R33: a key that does not decrypt is told apart from a missing one.
        #[tokio::test(start_paused = true)]
        async fn a_bad_key_is_told_apart_from_a_missing_one() {
            let fx = fixture();
            in_call(&fx).await;
            let own = adopt(&fx);
            fx.memberships
                .send(vec![
                    member("own", "@alice:example.org"),
                    member("bob", "@bob:example.org"),
                ])
                .unwrap();
            rostered(&fx, &["own", "bob"]).await;
            own.send(ConnectionEvent::TrackAdded {
                identity: "id-bob".to_owned(),
                kind: MediaStreamKind::Camera,
                track: Arc::new(FakeTrack::new(MediaStreamKind::Camera)),
            })
            .unwrap();
            own.send(ConnectionEvent::EncryptionStateChanged {
                identity: "id-bob".to_owned(),
                state: FrameEncryptionState::DecryptionFailed,
            })
            .unwrap();
            wait_until(|| record(&fx).unable_to_decrypt == ["bob"]).await;
            assert_eq!(
                tile(&fx, "bob").unable_to_decrypt,
                Some(UnableToDecrypt::BadKey)
            );
        }

        /// C6: a member who publishes nothing has nothing to fail to decrypt.
        /// Their tracks gone, as when a client dies without leaving, the key
        /// state it left behind no longer shows on their tile.
        #[tokio::test(start_paused = true)]
        async fn a_member_without_media_is_never_shown_undecryptable() {
            let fx = fixture();
            in_call(&fx).await;
            let own = adopt(&fx);
            fx.memberships
                .send(vec![
                    member("own", "@alice:example.org"),
                    member("bob", "@bob:example.org"),
                ])
                .unwrap();
            rostered(&fx, &["own", "bob"]).await;
            own.send(ConnectionEvent::TrackAdded {
                identity: "id-bob".to_owned(),
                kind: MediaStreamKind::Camera,
                track: Arc::new(FakeTrack::new(MediaStreamKind::Camera)),
            })
            .unwrap();
            own.send(ConnectionEvent::EncryptionStateChanged {
                identity: "id-bob".to_owned(),
                state: FrameEncryptionState::MissingKey,
            })
            .unwrap();
            wait_until(|| record(&fx).unable_to_decrypt == ["bob"]).await;

            own.send(ConnectionEvent::TrackRemoved {
                identity: "id-bob".to_owned(),
                kind: MediaStreamKind::Camera,
            })
            .unwrap();
            wait_until(|| record(&fx).unable_to_decrypt.is_empty()).await;
            assert_eq!(tile(&fx, "bob").unable_to_decrypt, None);
        }

        /// C6: a track declared unencrypted is never added, and its member
        /// reads `NotEncrypted` at once, whatever their frame cryptor says,
        /// until they leave.
        #[tokio::test(start_paused = true)]
        async fn a_track_sent_in_clear_is_refused_and_flagged_at_once() {
            let fx = fixture();
            in_call(&fx).await;
            let own = adopt(&fx);
            fx.memberships
                .send(vec![
                    member("own", "@alice:example.org"),
                    member("bob", "@bob:example.org"),
                ])
                .unwrap();
            rostered(&fx, &["own", "bob"]).await;
            own.send(ConnectionEvent::TrackNotEncrypted {
                identity: "id-bob".to_owned(),
                kind: MediaStreamKind::Camera,
            })
            .unwrap();
            own.send(ConnectionEvent::EncryptionStateChanged {
                identity: "id-bob".to_owned(),
                state: FrameEncryptionState::Ok,
            })
            .unwrap();
            wait_until(|| record(&fx).unable_to_decrypt == ["bob"]).await;
            settle(&fx.engine).await;
            assert_eq!(
                tile(&fx, "bob").unable_to_decrypt,
                Some(UnableToDecrypt::NotEncrypted)
            );
            assert!(!tile(&fx, "bob").has_video, "never added");
            assert_eq!(record(&fx).unable_to_decrypt, ["bob"]);

            fx.memberships
                .send(vec![member("own", "@alice:example.org")])
                .unwrap();
            wait_until(|| record(&fx).unable_to_decrypt.is_empty()).await;
        }

        /// R33: nothing of ours is decrypted, so our own tile always is.
        #[tokio::test]
        async fn the_own_tile_is_always_decryptable() {
            let mut fx = fixture();
            in_call(&fx).await;
            let own = adopt(&fx);
            own.send(ConnectionEvent::EncryptionStateChanged {
                identity: "id-own".to_owned(),
                state: FrameEncryptionState::MissingKey,
            })
            .unwrap();
            let _ = next_event(&mut fx.events).await; // FrameEncryptionState, logged
            settle(&fx.engine).await;
            assert_eq!(
                fx.engine.local_state().unwrap().tile.unable_to_decrypt,
                None
            );
            assert!(record(&fx).unable_to_decrypt.is_empty());
        }

        async fn published(fx: &Fixture, options: PublishOptions) {
            fx.engine.publish(options).await.expect("publish");
            settle(&fx.engine).await;
        }

        fn mutes(fx: &Fixture) -> Vec<(MediaStreamKind, bool)> {
            fx.state.local_mutes.lock().unwrap().clone()
        }

        /// R17: a lapse pauses publishing at the transport; the user's mute is
        /// kept apart and holds when the lapse clears.
        #[tokio::test]
        async fn a_lapsed_membership_pauses_publishing_and_keeps_the_users_mute() {
            let fx = fixture();
            in_call(&fx).await;
            adopt(&fx);
            published(&fx, PublishOptions::microphone()).await;
            published(
                &fx,
                PublishOptions::camera(VideoSourceConfig {
                    width: 640,
                    height: 480,
                }),
            )
            .await;

            fx.lapsed.send(true).unwrap();
            wait_until(|| record(&fx).own_membership_lapsed).await;
            let mut paused = mutes(&fx);
            paused.sort();
            assert_eq!(
                paused,
                [
                    (MediaStreamKind::Microphone, true),
                    (MediaStreamKind::Camera, true)
                ]
                .into_iter()
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>()
            );

            // The user mutes the microphone during the lapse.
            fx.engine
                .set_local_muted(MediaStreamKind::Microphone, true)
                .await
                .expect("mute");
            fx.state.local_mutes.lock().unwrap().clear();

            fx.lapsed.send(false).unwrap();
            wait_until(|| !record(&fx).own_membership_lapsed).await;
            assert_eq!(
                mutes(&fx),
                [(MediaStreamKind::Camera, false)],
                "only what the user had not muted resumes"
            );
            fx.engine
                .set_local_muted(MediaStreamKind::Microphone, false)
                .await
                .expect("unmute");
            assert_eq!(
                mutes(&fx).last(),
                Some(&(MediaStreamKind::Microphone, false))
            );
        }

        /// R17, C7: the pause never moves the own tile or the share flag.
        #[tokio::test]
        async fn the_pause_never_moves_the_own_tile_or_the_share_flag() {
            let fx = fixture();
            in_call(&fx).await;
            adopt(&fx);
            published(
                &fx,
                PublishOptions::camera(VideoSourceConfig {
                    width: 640,
                    height: 480,
                }),
            )
            .await;
            published(
                &fx,
                PublishOptions::screen_share(VideoSourceConfig {
                    width: 1920,
                    height: 1080,
                }),
            )
            .await;
            let before = fx.engine.local_state().unwrap();
            assert!(before.tile.has_video && before.is_screen_sharing);

            fx.lapsed.send(true).unwrap();
            wait_until(|| mutes(&fx).len() == 2).await;
            settle(&fx.engine).await;
            assert_eq!(fx.engine.local_state().unwrap(), before);
        }

        /// R17: a snapshot without us after we arrived is the lapse observed.
        #[tokio::test]
        async fn a_snapshot_without_us_is_a_lapse_observed() {
            let fx = fixture();
            in_call(&fx).await;
            fx.memberships
                .send(vec![member("bob", "@bob:example.org")])
                .unwrap();
            wait_until(|| record(&fx).own_membership_lapsed).await;
            in_call(&fx).await;
            wait_until(|| !record(&fx).own_membership_lapsed).await;
        }

        /// R38: each cause in its own field; clearing one clears only it.
        #[tokio::test(start_paused = true)]
        async fn interruptions_list_every_cause_separately() {
            let fx = fixture();
            let peer = with_bob_on_a_peer_focus(&fx).await;
            let own = wait_sender(&fx, OWN_FOCUS);
            peer.send(ConnectionEvent::TrackAdded {
                identity: "id-bob".to_owned(),
                kind: MediaStreamKind::Camera,
                track: Arc::new(FakeTrack::new(MediaStreamKind::Camera)),
            })
            .unwrap();
            peer.send(ConnectionEvent::Reconnecting).unwrap();
            own.send(ConnectionEvent::RemoteJoined {
                identity: "id-stranger".to_owned(),
            })
            .unwrap();
            peer.send(ConnectionEvent::EncryptionStateChanged {
                identity: "id-bob".to_owned(),
                state: FrameEncryptionState::DecryptionFailed,
            })
            .unwrap();
            fx.signalling
                .send(SignallingHealth::Unreachable { since_ms: 5 })
                .unwrap();
            fx.lapsed.send(true).unwrap();

            let all = Interruptions {
                signalling: SignallingHealth::Unreachable { since_ms: 5 },
                own_membership_lapsed: true,
                own_media: ConnectionState::Connected,
                members_reconnecting: vec!["bob".to_owned()],
                members_unreachable: vec!["carol".to_owned()],
                unable_to_decrypt: vec!["bob".to_owned()],
                unmappable: vec!["id-stranger".to_owned()],
            };
            wait_until(|| record(&fx) == all).await;

            fx.signalling.send(SignallingHealth::Reachable).unwrap();
            wait_until(|| record(&fx).signalling == SignallingHealth::Reachable).await;
            // Spec 038 R25: with signalling healthy, a participant with no
            // membership is no interruption; it is listed apart once its
            // grace has run.
            assert_eq!(
                record(&fx),
                Interruptions {
                    signalling: SignallingHealth::Reachable,
                    unmappable: Vec::new(),
                    ..all
                }
            );
        }

        /// R38: a reader woken by the record finds the tiles already agreeing.
        #[tokio::test]
        async fn the_record_follows_the_roster_within_one_change() {
            let fx = fixture();
            let peer = with_bob_on_a_peer_focus(&fx).await;
            let mut records = fx.engine.subscribe_interruptions();
            records.borrow_and_update();
            peer.send(ConnectionEvent::Reconnecting).unwrap();
            loop {
                records.changed().await.unwrap();
                if records.borrow_and_update().members_reconnecting == ["bob"] {
                    break;
                }
            }
            assert_eq!(tile(&fx, "bob").connection, ConnectionState::Reconnecting);
        }

        /// Spec 038 R25: with signalling healthy, an identity on an SFU with
        /// no membership is listed as unverified once it has been so for
        /// the grace, apart from the interruptions, until it maps or leaves.
        #[tokio::test(start_paused = true)]
        async fn an_unmapped_participant_is_unverified_after_the_grace_until_it_maps_or_leaves() {
            let fx = fixture();
            in_call(&fx).await;
            let own = adopt(&fx);
            for identity in ["id-dave", "id-erin"] {
                own.send(ConnectionEvent::RemoteJoined {
                    identity: identity.to_owned(),
                })
                .unwrap();
            }
            tokio::time::sleep(UNVERIFIED_GRACE - Duration::from_secs(1)).await;
            assert!(
                fx.engine.unverified_participants().is_empty(),
                "inside the grace"
            );
            assert!(
                record(&fx).unmappable.is_empty(),
                "never an interruption here"
            );

            tokio::time::sleep(Duration::from_secs(2)).await;
            wait_until(|| fx.engine.unverified_participants() == ["id-dave", "id-erin"]).await;
            assert!(
                record(&fx).is_empty(),
                "a healthy call shows no interruption"
            );

            fx.memberships
                .send(vec![
                    member("own", "@alice:example.org"),
                    member("dave", "@dave:example.org"),
                ])
                .unwrap();
            wait_until(|| fx.engine.unverified_participants() == ["id-erin"]).await;
            own.send(ConnectionEvent::RemoteLeft {
                identity: "id-erin".to_owned(),
            })
            .unwrap();
            wait_until(|| fx.engine.unverified_participants().is_empty()).await;
        }

        /// 034 R16, as 038 R25 narrows it: while signalling is unhealthy, an
        /// unmapped participant is an interruption cause at once, and nobody
        /// is listed unverified.
        #[tokio::test(start_paused = true)]
        async fn with_signalling_unhealthy_an_unmapped_participant_is_an_interruption() {
            let fx = fixture();
            in_call(&fx).await;
            let own = adopt(&fx);
            fx.signalling
                .send(SignallingHealth::Unreachable { since_ms: 5 })
                .unwrap();
            own.send(ConnectionEvent::RemoteJoined {
                identity: "id-dave".to_owned(),
            })
            .unwrap();
            wait_until(|| record(&fx).unmappable == ["id-dave"]).await;
            tokio::time::sleep(UNVERIFIED_GRACE * 2).await;
            assert!(fx.engine.unverified_participants().is_empty());
            assert_eq!(record(&fx).unmappable, ["id-dave"]);
        }

        /// R31: a peer's focus down for an hour never ends the call.
        #[tokio::test(start_paused = true)]
        async fn a_peers_focus_down_for_an_hour_never_ends_the_call() {
            let fx = fixture();
            let peer = with_bob_on_a_peer_focus(&fx).await;
            fx.state.fail_attempts.store(u32::MAX, Ordering::SeqCst);
            peer.send(ConnectionEvent::Closed {
                message: "gone".to_owned(),
            })
            .unwrap();
            tokio::time::sleep(Duration::from_secs(3_600)).await;
            assert_eq!(
                degraded(&fx),
                Some(true),
                "connected, degraded, never ended"
            );
            assert_eq!(record(&fx).members_reconnecting, ["bob"]);
        }

        /// R24, C10: our own focus is taken before it is up; a publication
        /// made meanwhile goes through its handle rather than failing, and
        /// our media reads connecting until it is first up.
        #[tokio::test]
        async fn publishing_before_our_own_focus_is_up_goes_through_its_handle() {
            let fx = fixture();
            let (connection, _events) = fake_connection(&fx.state, OWN_FOCUS);
            fx.engine.connect_own(connection);
            fx.engine
                .publish(PublishOptions::microphone())
                .await
                .expect("the handle takes it");
            assert_eq!(fx.state.published.lock().unwrap().len(), 1);
            assert_eq!(record(&fx).own_media, ConnectionState::Connecting);
        }

        /// R12: signalling unreachable for five minutes never touches our own
        /// connection.
        #[tokio::test(start_paused = true)]
        async fn signalling_loss_does_not_touch_the_own_connection() {
            let fx = fixture();
            in_call(&fx).await;
            adopt(&fx);
            fx.signalling
                .send(SignallingHealth::Unreachable { since_ms: 1 })
                .unwrap();
            tokio::time::sleep(Duration::from_secs(300)).await;
            assert!(!closed(&fx, OWN_FOCUS));
            assert_eq!(connect_count(&fx, OWN_FOCUS), 0);
            assert_eq!(degraded(&fx), Some(false));
            assert_eq!(record(&fx).own_media, ConnectionState::Connected);
        }
    }
}
