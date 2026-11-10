//! The JSON a guest's exported functions receive, and return.
//!
//! The guest ABI is deliberately small, so growing the API never changes the
//! link surface: six exports and two imports, with a method name inside every
//! payload. An unknown method answers `unsupported`, so a plugin can
//! feature-detect rather than fail to link, and `api` in the manifest changes
//! only for a breaking change.
//!
//! All of it is JSON in the first version, because every PDK language has it
//! and a payload can be read in a log.

use serde::{Deserialize, Serialize};

use crate::call::{HostCall, HostReturn};
use crate::event::EventEnvelope;
use crate::ids::PluginId;

/// What `yf_activate` is given.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivatePayload {
    /// The API version the helper speaks.
    pub api: u32,
    pub plugin: PluginId,
    /// What the last `yf_deactivate` handed on, if anything.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
}

/// What `yf_deactivate` returns, as JSON: its hand-off, or `null`.
///
/// A guest that returns nothing is the same as `null`. The hand-off is cut to
/// `limits::MAX_HANDOFF_BYTES`, and anything durable belongs in storage, since
/// an instance may vanish between any two calls.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeactivatePayload {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
}

/// What `yf_event` is given, in one call.
///
/// Events are batched, so a burst of typing is one call. Results of host calls
/// the guest made that were answered later travel in the same call, keyed by
/// the id `yf_call` answered with `pending`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct EventBatch {
    #[serde(default)]
    pub events: Vec<EventEnvelope>,
    #[serde(default)]
    pub results: Vec<CallResult>,
}

/// The answer to one host call the guest left pending.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CallResult {
    /// The id the host function returned when it answered `pending`.
    pub call: u64,
    pub answer: HostReturn,
}

/// What `yf_post` is given: a call with no answer.
pub type PostPayload = HostCall;

impl EventBatch {
    /// A batch of one, which is what most events arrive as.
    pub fn one(event: EventEnvelope) -> Self {
        EventBatch {
            events: vec![event],
            results: Vec::new(),
        }
    }

    /// Whether there is anything to call the guest for.
    pub fn is_empty(&self) -> bool {
        self.events.is_empty() && self.results.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::EventKind;
    use crate::event::Network;

    #[test]
    fn an_activation_carries_the_api_and_the_hand_off() {
        let payload = ActivatePayload {
            api: crate::API_VERSION,
            plugin: PluginId::parse("com.example.shout").unwrap(),
            state: Some("{\"seen\":3}".into()),
        };
        let text = serde_json::to_string(&payload).unwrap();
        assert_eq!(
            serde_json::from_str::<ActivatePayload>(&text).unwrap(),
            payload
        );
    }

    #[test]
    fn an_event_batch_carries_results_too() {
        let batch = EventBatch {
            events: vec![EventEnvelope {
                kind: EventKind::MessageCreate,
                network: Network::Discord,
                at: 0,
                room: Some("room-1".into()),
                author: None,
                own: false,
                payload: serde_json::json!({ "body": "hi" }),
            }],
            results: vec![CallResult {
                call: 7,
                answer: HostReturn::Unsupported,
            }],
        };
        let text = serde_json::to_string(&batch).unwrap();
        assert_eq!(serde_json::from_str::<EventBatch>(&text).unwrap(), batch);
        assert!(!batch.is_empty());
        assert!(EventBatch::default().is_empty());
    }
}
