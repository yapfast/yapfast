//! Events: what a plugin is told about as it happens.
//!
//! An event arrives with the fields every event carries at the top and the
//! kind's own JSON in its payload, so a plugin that only cares about which room
//! something happened in never decodes the rest. The typed accessors here
//! decode the payloads this API version defines and answer `None` for
//! anything else, including a payload the helper had to truncate.

use serde_json::Value;

use crate::wire;

pub use crate::wire::{AttachmentView, EventKind, MessageView, Network, PersonView, RoomView};

/// The fields every event carries, copied out of the envelope.
#[derive(Clone, Debug, PartialEq)]
pub struct Envelope {
    /// What happened.
    pub kind: EventKind,
    /// Which network it happened on.
    pub network: Network,
    /// Milliseconds since the Unix epoch.
    pub at: u64,
    /// The room it happened in, if it happened in one.
    pub room: Option<String>,
    /// The person it is about, if it is about someone.
    pub author: Option<String>,
    /// Whether the account did it itself.
    pub own: bool,
    /// The kind's own fields; `{"truncated": true, ...}` when they were too
    /// large to send whole.
    pub payload: Value,
}

/// One event, decoded.
///
/// Kinds this API version names have their own variant, so a handler can match
/// the ones it cares about; a kind a later version adds arrives as
/// [`Event::Other`] with its name in [`Event::kind`].
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// A message was sent.
    MessageCreate(Envelope),
    /// A message was edited.
    MessageUpdate(Envelope),
    /// A message was deleted.
    MessageDelete(Envelope),
    /// Someone reacted to a message.
    ReactionAdd(Envelope),
    /// A reaction was taken away.
    ReactionRemove(Envelope),
    /// The person opened a different room.
    RoomSelected(Envelope),
    /// Any other kind: voice state, focus, presence, typing, or one added
    /// after this PDK was built.
    Other(Envelope),
}

impl Event {
    /// The fields every event carries.
    pub fn envelope(&self) -> &Envelope {
        match self {
            Event::MessageCreate(envelope)
            | Event::MessageUpdate(envelope)
            | Event::MessageDelete(envelope)
            | Event::ReactionAdd(envelope)
            | Event::ReactionRemove(envelope)
            | Event::RoomSelected(envelope)
            | Event::Other(envelope) => envelope,
        }
    }

    /// What happened.
    pub fn kind(&self) -> &EventKind {
        &self.envelope().kind
    }

    /// Which network it happened on.
    pub fn network(&self) -> Network {
        self.envelope().network
    }

    /// Milliseconds since the Unix epoch.
    pub fn at(&self) -> u64 {
        self.envelope().at
    }

    /// The room it happened in, if it happened in one.
    pub fn room(&self) -> Option<&str> {
        self.envelope().room.as_deref()
    }

    /// The person it is about, if it is about someone.
    pub fn author(&self) -> Option<&str> {
        self.envelope().author.as_deref()
    }

    /// Whether the account did it itself.
    pub fn is_own(&self) -> bool {
        self.envelope().own
    }

    /// The kind's own fields, as the sender shaped them.
    pub fn payload(&self) -> &Value {
        &self.envelope().payload
    }

    /// The message this event is about, when its payload is one.
    ///
    /// `None` for kinds that carry something else, and for a payload that was
    /// cut because it was larger than one event may carry.
    pub fn message(&self) -> Option<MessageView> {
        serde_json::from_value(self.envelope().payload.clone()).ok()
    }

    /// The room this event is about, when its payload is one.
    pub fn room_view(&self) -> Option<RoomView> {
        serde_json::from_value(self.envelope().payload.clone()).ok()
    }
}

impl From<wire::EventEnvelope> for Event {
    fn from(envelope: wire::EventEnvelope) -> Self {
        let kind = envelope.kind.clone();
        let envelope = Envelope {
            kind: kind.clone(),
            network: envelope.network,
            at: envelope.at,
            room: envelope.room,
            author: envelope.author,
            own: envelope.own,
            payload: envelope.payload,
        };
        match kind {
            EventKind::MessageCreate => Event::MessageCreate(envelope),
            EventKind::MessageUpdate => Event::MessageUpdate(envelope),
            EventKind::MessageDelete => Event::MessageDelete(envelope),
            EventKind::ReactionAdd => Event::ReactionAdd(envelope),
            EventKind::ReactionRemove => Event::ReactionRemove(envelope),
            EventKind::RoomSelected => Event::RoomSelected(envelope),
            _ => Event::Other(envelope),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn envelope(kind: &str, payload: Value) -> wire::EventEnvelope {
        serde_json::from_value(serde_json::json!({
            "kind": kind,
            "network": "discord",
            "at": 5,
            "room": "room-1",
            "own": false,
            "payload": payload,
        }))
        .expect("an envelope the app writes")
    }

    #[test]
    fn a_named_kind_decodes_to_its_variant() {
        let event = Event::from(envelope(
            "message_create",
            serde_json::json!({"body": "hi"}),
        ));
        assert!(matches!(event, Event::MessageCreate(_)));
        assert_eq!(event.kind(), &EventKind::MessageCreate);
        assert_eq!(event.room(), Some("room-1"));
        assert_eq!(event.payload()["body"], "hi");
    }

    #[test]
    fn a_message_payload_decodes_and_a_truncated_one_does_not() {
        let message = Event::from(envelope(
            "message_create",
            serde_json::json!({
                "id": "m1",
                "room": "room-1",
                "author": {"id": "u1", "name": "ada"},
                "body": "hi",
                "at": 5,
            }),
        ));
        assert_eq!(message.message().expect("a message view").body, "hi");

        let cut = Event::from(envelope(
            "message_create",
            serde_json::json!({"truncated": true, "bytes": 999999, "text": "{\"body\":"}),
        ));
        assert!(cut.message().is_none());
        assert_eq!(cut.payload()["truncated"], true);
    }

    #[test]
    fn a_kind_from_a_later_version_is_kept_not_dropped() {
        let event = Event::from(envelope("stream_started", Value::Null));
        assert!(matches!(event, Event::Other(_)));
        assert_eq!(event.kind().to_string(), "stream_started");
    }
}
