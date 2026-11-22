// Copyright 2026 Element Creations Ltd.
//
// SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-Element-Commercial
// Please see LICENSE in the repository root for full details.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use matrix_rtc_call::{
    CallEnd, CallJoinOptions, CallStatus, ConnectionLossReason, JoinTransport, RtcClient, RtcRoom,
};
use matrix_rtc_core::testing::MockBackend;
use matrix_rtc_core::{
    EventEncryption, EventIn, IdentitySubject, JoinedMembership, KEY_MESSAGE_TYPE,
    LeaveSessionParams, RoomOptions, RtcTransport, SLOT_EVENT_TYPE, ToDeviceMessageIn,
};
use serde_json::json;
use tokio::sync::mpsc;

use super::*;
use matrix_rtc_transport::{ConnectionEvent, FrameKeyRing, TransportConnection};

const ROOM: &str = "!room:example.org";
const SLOT: &str = "m.call#room";
const BOB: &str = "@bob:example.org";
const FOCUS: &str = "wss://sfu.example.org";
const PEER_FOCUS: &str = "wss://sfu-b.example.org";

/// Not the default derivation, so a key imported under the raw `member_id`
/// fallback (the mapper installed too late) shows.
fn mapper() -> RtcIdentityMapper {
    Arc::new(|subject: &IdentitySubject<'_>| {
        Some(identity(
            subject.user_id,
            subject.device_id?,
            subject.member_id,
        ))
    })
}

fn identity(user: &str, device: &str, member: &str) -> String {
    format!("{user}|{device}|{member}")
}

fn bob_identity() -> String {
    identity(BOB, "BOBDEV", "bob-1")
}

/// Every key the ring was handed, by identity.
#[derive(Default)]
struct RecordingRing(Mutex<Vec<String>>);

#[async_trait]
impl FrameKeyRing for RecordingRing {
    fn ring_size(&self) -> u16 {
        16
    }

    async fn set_key(&self, identity: &str, _index: u8, _key: Vec<u8>) -> bool {
        self.0.lock().unwrap().push(identity.to_owned());
        true
    }
}

/// How the own focus answers each connect, in order; `Ok` once they run out.
#[derive(Clone, Copy, Debug)]
enum Answer {
    Fail,
    Refuse,
}

/// The SFUs, shared by the transport and its handles. `answers` are our own
/// focus's; a peer's focus always connects.
#[derive(Default)]
struct OwnFocus {
    connects: Mutex<Vec<String>>,
    answers: Mutex<std::collections::VecDeque<Answer>>,
    /// Each focus's live connection events, for a test to play the SFU.
    events: Mutex<std::collections::HashMap<String, mpsc::UnboundedSender<ConnectionEvent>>>,
    /// The foci our own handle took over.
    takeovers: Mutex<Vec<String>>,
}

#[derive(Clone)]
struct FakeConnection {
    key: String,
    local_key_indexes: Arc<Mutex<Vec<u8>>>,
    own: Arc<OwnFocus>,
}

#[async_trait]
impl TransportConnection for FakeConnection {
    fn connection_key(&self) -> &str {
        &self.key
    }

    fn set_local_key_index(&self, key_index: u8) {
        self.local_key_indexes.lock().unwrap().push(key_index);
    }

    async fn reconnect(
        &self,
        _fresh_token: bool,
    ) -> Result<mpsc::UnboundedReceiver<ConnectionEvent>, TransportError> {
        self.own.connects.lock().unwrap().push(self.key.clone());
        let answer = if self.key == FOCUS {
            self.own.answers.lock().unwrap().pop_front()
        } else {
            None
        };
        match answer {
            Some(Answer::Fail) => Err(TransportError::Connect("unreachable".into())),
            Some(Answer::Refuse) => Err(TransportError::Refused("403".into())),
            None => {
                let (tx, rx) = mpsc::unbounded_channel();
                self.own.events.lock().unwrap().insert(self.key.clone(), tx);
                Ok(rx)
            }
        }
    }

    fn hand_over(&self) -> Option<Box<dyn std::any::Any>> {
        Some(Box::new(self.key.clone()))
    }

    fn take_over(&self, handover: Box<dyn std::any::Any>) -> Result<(), TransportError> {
        let key = handover.downcast::<String>().expect("a fake's handover");
        self.own.takeovers.lock().unwrap().push(*key);
        Ok(())
    }

    async fn close(&self) -> Result<(), TransportError> {
        Ok(())
    }
}

#[derive(Default)]
struct FakeTransport {
    own: Arc<OwnFocus>,
}

impl FakeTransport {
    fn answering(answers: impl IntoIterator<Item = Answer>) -> Self {
        let transport = Self::default();
        transport.own.answers.lock().unwrap().extend(answers);
        transport
    }

    /// Our own focus's connects.
    fn connects(&self) -> Vec<String> {
        let connects = self.own.connects.lock().unwrap();
        connects
            .iter()
            .filter(|key| *key == FOCUS)
            .cloned()
            .collect()
    }

    /// Our own focus's live connection.
    fn events(&self) -> mpsc::UnboundedSender<ConnectionEvent> {
        self.own.events.lock().unwrap()[FOCUS].clone()
    }
}

#[async_trait]
impl MediaTransport for FakeTransport {
    fn transport_type(&self) -> &'static str {
        "livekit"
    }

    fn connection_key(&self, transport: &RtcTransport, _publisher_user_id: &str) -> Option<String> {
        match transport {
            RtcTransport::LiveKit(livekit) => livekit.url().map(str::to_owned),
            RtcTransport::Unsupported(_) => None,
        }
    }

    fn remote_identity(&self, member: &JoinedMembership) -> Option<String> {
        let device = member.origin.sender_device_id()?;
        Some(identity(&member.sender, device, &member.member_id))
    }

    async fn connect(
        &self,
        connection_key: &str,
        ctx: &ConnectionContext,
    ) -> Result<
        (
            Box<dyn TransportConnection>,
            mpsc::UnboundedReceiver<ConnectionEvent>,
        ),
        TransportError,
    > {
        let (connection, events) = self.connect_own(connection_key, ctx).await?;
        Ok((Box::new(connection), events))
    }
}

#[async_trait]
impl OwnFocusTransport for FakeTransport {
    type Connection = FakeConnection;

    async fn connect_own(
        &self,
        connection_key: &str,
        ctx: &ConnectionContext,
    ) -> Result<(FakeConnection, mpsc::UnboundedReceiver<ConnectionEvent>), TransportError> {
        let connection = self.prepare_own(connection_key, ctx);
        let events = connection.reconnect(false).await?;
        Ok((connection, events))
    }

    fn prepare_own(&self, connection_key: &str, _ctx: &ConnectionContext) -> FakeConnection {
        FakeConnection {
            key: connection_key.to_owned(),
            local_key_indexes: Arc::default(),
            own: self.own.clone(),
        }
    }
}

/// An encrypted room with an open per-member-encrypted slot and Bob in it, the way a homeserver would feed it.
async fn open(client: &RtcClient<MockBackend>, mock: &MockBackend) -> RtcRoom<MockBackend> {
    open_with_bob_on(client, mock, FOCUS).await
}

/// [`open`], with Bob publishing on `bob_focus`.
async fn open_with_bob_on(
    client: &RtcClient<MockBackend>,
    mock: &MockBackend,
    bob_focus: &str,
) -> RtcRoom<MockBackend> {
    let room = client
        .room(ROOM, RoomOptions::default())
        .await
        .expect("the room opens");
    let sink = mock
        .room_subscription(ROOM)
        .expect("subscribed")
        .sink
        .clone();
    sink.on_encryption(true);
    sink.on_state_events(
        SLOT_EVENT_TYPE.to_owned(),
        vec![EventIn {
            event_id: "$slot".to_owned(),
            sender: BOB.to_owned(),
            event_type: SLOT_EVENT_TYPE.to_owned(),
            state_key: Some(SLOT.to_owned()),
            origin_server_ts: 1,
            content: json!({
                "status": "open",
                "application": { "type": "m.call" },
                "encryption": { "type": "m.per_member" },
            }),
            encryption: EventEncryption::Cleartext,
        }],
    );
    sink.on_joined_members(vec![mock.user_id.clone(), BOB.to_owned()]);
    sink.on_sticky_events(vec![EventIn {
        event_id: "$bob".to_owned(),
        sender: BOB.to_owned(),
        event_type: "m.rtc.member".to_owned(),
        state_key: None,
        origin_server_ts: 2,
        content: json!({
            "slot_id": SLOT,
            "msc4354_sticky_key": "bob-1",
            "member": { "id": "bob-1", "membership": "join" },
            "application": { "type": "m.call" },
            "transports": { "published": [{ "type": "m.livekit", "url": bob_focus }] },
        }),
        encryption: EventEncryption::encrypted(Some("BOBDEV".to_owned()), Some(true)),
    }]);
    room.seeded().await;
    room
}

fn bob_key() -> ToDeviceMessageIn {
    ToDeviceMessageIn {
        sender: BOB.to_owned(),
        event_type: KEY_MESSAGE_TYPE.to_owned(),
        content: json!({
            "room_id": ROOM,
            "member_id": "bob-1",
            "media_key": { "index": 0, "key": "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8" },
        }),
        encryption: EventEncryption::encrypted(Some("BOBDEV".to_owned()), Some(true)),
    }
}

fn mock() -> Arc<MockBackend> {
    let mock = Arc::new(MockBackend::new());
    *mock.transports.lock().unwrap() = Ok(json!([{ "type": "m.livekit", "url": FOCUS }]));
    mock
}

fn options(mock: &MockBackend) -> AttachOptions {
    AttachOptions {
        own_user_id: mock.user_id.clone(),
        own_device_id: mock.device_id.clone(),
        identity_mapper: mapper(),
        stability: StabilityConfig::default(),
        own_reconnect_budget: matrix_rtc_transport::pool::OWN_RECONNECT_BUDGET,
        sfu_fallback: false,
    }
}

async fn settle() {
    tokio::time::sleep(Duration::from_millis(50)).await;
}

#[tokio::test]
async fn a_key_held_before_attaching_is_imported_under_the_mapped_identity_and_reported() {
    let mock = mock();
    let client = RtcClient::new(mock.clone());
    let room = open(&client, &mock).await;
    let call = room.join_call(CallJoinOptions::new()).await.expect("join");

    // Bob's key arrives between the join and the attach, with nobody listening.
    mock.to_device_subscription()
        .unwrap()
        .sink
        .on_to_device_message(bob_key());
    settle().await;

    let ring = Arc::new(RecordingRing::default());
    let handler = Arc::new(MediaKeyHandler::with_ring(ring.clone()));
    let transport = Arc::new(FakeTransport::default());
    let mut attached = attach_media(&call, transport.clone(), handler, options(&mock))
        .await
        .expect("attaches");
    settle().await;

    assert!(
        ring.0.lock().unwrap().contains(&bob_identity()),
        "the replay derives through the mapper, installed first: {:?}",
        ring.0.lock().unwrap(),
    );
    let mut imported = false;
    while let Ok(event) = attached.events.try_recv() {
        imported |=
            matches!(event, CallEvent::KeyImported { member_id, .. } if member_id == "bob-1");
    }
    assert!(imported, "the replayed key reaches the event stream");

    assert_eq!(transport.connects(), [FOCUS]);
    assert!(attached.own_connection.is_some());
    assert_eq!(
        attached.own_identity,
        identity(&mock.user_id, &mock.device_id, call.member_id())
    );
}

#[tokio::test]
async fn our_sender_adopts_the_key_index_we_are_already_on() {
    let mock = mock();
    let client = RtcClient::new(mock.clone());
    let room = open(&client, &mock).await;
    let call = room.join_call(CallJoinOptions::new()).await.expect("join");
    let handler = Arc::new(MediaKeyHandler::new());

    let attached = attach_media(
        &call,
        Arc::new(FakeTransport::default()),
        handler.clone(),
        options(&mock),
    )
    .await
    .expect("attaches");

    let connection = attached.own_connection.expect("publishes on the focus");
    // The join distributed our first key; the replay recorded it under our
    // mapped identity before the connect.
    let own = handler
        .key_for(&attached.own_identity)
        .expect("our own key was replayed");
    assert_eq!(
        *connection.local_key_indexes.lock().unwrap(),
        [own.key_index]
    );
}

#[tokio::test]
async fn a_receive_only_call_connects_no_own_focus() {
    let mock = mock();
    let client = RtcClient::new(mock.clone());
    let room = open(&client, &mock).await;
    let call = room
        .join_call(CallJoinOptions::new().transport(JoinTransport::ReceiveOnly))
        .await
        .expect("join");
    let transport = Arc::new(FakeTransport::default());

    let attached = attach_media(
        &call,
        transport.clone(),
        Arc::new(MediaKeyHandler::new()),
        options(&mock),
    )
    .await
    .expect("attaches");

    assert!(attached.own_connection.is_none());
    assert!(transport.connects().is_empty());
}

#[tokio::test]
async fn a_call_that_is_over_does_not_attach() {
    let mock = mock();
    let client = RtcClient::new(mock.clone());
    let room = open(&client, &mock).await;
    let call = room.join_call(CallJoinOptions::new()).await.expect("join");
    call.leave(LeaveSessionParams::new()).await.expect("leave");

    let result = attach_media(
        &call,
        Arc::new(FakeTransport::default()),
        Arc::new(MediaKeyHandler::new()),
        options(&mock),
    )
    .await;
    assert!(matches!(result, Err(AttachError::NotJoined(_))));
}

/// 034 R24, C10: the attach never waits for our own focus; a refusal ends
/// the call, after its leave, with the reason.
#[tokio::test]
async fn a_refusing_own_focus_ends_the_call_after_its_leave() {
    let mock = mock();
    let client = RtcClient::new(mock.clone());
    let room = open(&client, &mock).await;
    let call = room.join_call(CallJoinOptions::new()).await.expect("join");
    let leaves_before = sticky_leaves(&mock);
    let transport = Arc::new(FakeTransport::answering([Answer::Refuse]));

    let _attached = attach_media(
        &call,
        transport,
        Arc::new(MediaKeyHandler::new()),
        options(&mock),
    )
    .await
    .expect("attaches without waiting for the SFU");
    settle().await;

    assert!(matches!(
        call.status(),
        CallStatus::Ended {
            reason: CallEnd::ConnectionClosed {
                reason: ConnectionLossReason::Refused,
                ..
            }
        }
    ));
    assert!(!call.is_live());
    assert_eq!(sticky_leaves(&mock), leaves_before + 1, "one leave issued");
}

/// Our sticky leaves sent so far.
fn sticky_leaves(mock: &MockBackend) -> usize {
    mock.sticky_events
        .lock()
        .unwrap()
        .iter()
        .filter(|(_, _, content, _)| content["member"]["membership"] == "leave")
        .count()
}

/// The status follows our own focus: connecting until it is first up,
/// degraded while it reconnects, and a loss the transport gives up on is
/// reconnected rather than ended.
#[tokio::test]
async fn our_own_focus_drives_the_call_status() {
    let mock = mock();
    let client = RtcClient::new(mock.clone());
    let room = open(&client, &mock).await;
    let call = room.join_call(CallJoinOptions::new()).await.expect("join");
    let transport = Arc::new(FakeTransport::default());
    assert_eq!(call.status(), CallStatus::Connecting);

    let _attached = attach_media(
        &call,
        transport.clone(),
        Arc::new(MediaKeyHandler::new()),
        options(&mock),
    )
    .await
    .expect("attaches");
    settle().await;
    let CallStatus::Connected {
        since_ms,
        degraded: false,
    } = call.status()
    else {
        panic!("connected once our focus is up, not {:?}", call.status());
    };

    transport
        .events()
        .send(ConnectionEvent::Reconnecting)
        .unwrap();
    settle().await;
    assert_eq!(
        call.status(),
        CallStatus::Connected {
            since_ms,
            degraded: true
        }
    );

    transport
        .events()
        .send(ConnectionEvent::Closed {
            message: "gone".to_owned(),
        })
        .unwrap();
    settle().await;
    assert_eq!(
        call.status(),
        CallStatus::Connected {
            since_ms,
            degraded: false
        },
        "reconnected behind the same handle"
    );
    assert_eq!(transport.connects().len(), 2);
}

/// 034 R26: when our own focus is given up, publishing stops, the leave is
/// issued, and only then is the end reported, without waiting for the
/// leave's outcome.
#[tokio::test(start_paused = true)]
async fn a_lost_connection_stops_publishing_and_issues_the_leave_before_ended() {
    let mock = mock();
    let client = RtcClient::new(mock.clone());
    let room = open(&client, &mock).await;
    let call = room.join_call(CallJoinOptions::new()).await.expect("join");
    let transport = Arc::new(FakeTransport::default());
    let _attached = attach_media(
        &call,
        transport.clone(),
        Arc::new(MediaKeyHandler::new()),
        options(&mock),
    )
    .await
    .expect("attaches");
    settle().await;
    transport
        .own
        .answers
        .lock()
        .unwrap()
        .extend(std::iter::repeat_n(Answer::Fail, 100));
    mock.script(
        matrix_rtc_core::testing::MockMethod::StickyEvent,
        [matrix_rtc_core::testing::MockOutcome::Hang],
    );
    let calls_before = mock
        .calls_to(matrix_rtc_core::testing::MockMethod::StickyEvent)
        .len();
    let mut ended = call.subscribe_ended();
    let mut status = call.subscribe_status();

    transport
        .events()
        .send(ConnectionEvent::Closed {
            message: "gone".to_owned(),
        })
        .unwrap();
    // Nothing ends before the budget.
    tokio::time::sleep(Duration::from_secs(59)).await;
    assert!(call.is_live());
    status
        .wait_for(|status| matches!(status, CallStatus::Ended { .. }))
        .await
        .unwrap();

    assert!(
        ended.borrow_and_update().is_some(),
        "publishing stopped first"
    );
    assert_eq!(
        mock.calls_to(matrix_rtc_core::testing::MockMethod::StickyEvent)
            .len(),
        calls_before + 1,
        "the leave was issued before the end, and the end did not wait for it"
    );
    assert!(matches!(
        call.status(),
        CallStatus::Ended {
            reason: CallEnd::ConnectionClosed {
                reason: ConnectionLossReason::Budget,
                ..
            }
        }
    ));
}

/// 034 R25: our own focus down for ten seconds while Bob's is up moves our
/// media onto Bob's, and our membership is republished naming it under the
/// same member id; the call outlives the budget.
#[tokio::test(start_paused = true)]
async fn own_sfu_loss_with_fallback_republishes_our_membership_on_the_peer_focus() {
    let mock = mock();
    let client = RtcClient::new(mock.clone());
    let room = open_with_bob_on(&client, &mock, PEER_FOCUS).await;
    let call = room.join_call(CallJoinOptions::new()).await.expect("join");
    let transport = Arc::new(FakeTransport::default());
    let _attached = attach_media(
        &call,
        transport.clone(),
        Arc::new(MediaKeyHandler::new()),
        AttachOptions {
            sfu_fallback: true,
            ..options(&mock)
        },
    )
    .await
    .expect("attaches");
    settle().await;
    transport
        .own
        .answers
        .lock()
        .unwrap()
        .extend(std::iter::repeat_n(Answer::Fail, 100));
    let member_id = call.member_id().to_owned();

    transport
        .events()
        .send(ConnectionEvent::Closed {
            message: "gone".to_owned(),
        })
        .unwrap();
    tokio::time::sleep(Duration::from_secs(9)).await;
    assert!(transport.own.takeovers.lock().unwrap().is_empty());
    tokio::time::sleep(Duration::from_secs(61)).await;

    assert_eq!(*transport.own.takeovers.lock().unwrap(), [PEER_FOCUS]);
    let (_, _, content, _) = mock.last_sticky_event().expect("republished");
    assert_eq!(
        content["transports"]["published"],
        json!([{ "type": "m.livekit", "url": PEER_FOCUS, "server_name": "example.org" }])
    );
    assert_eq!(content["member"]["id"], json!(member_id));
    assert_eq!(content["member"]["membership"], "join");
    assert!(call.is_live(), "no end at the budget");
    assert!(matches!(
        call.status(),
        CallStatus::Connected {
            degraded: false,
            ..
        }
    ));
}
