use super::*;

/// One event, serialized once and shared by every plugin that wants it.
///
/// The wire payload of an event batch embeds these bytes rather than the event's
/// own shape, so an event four plugins subscribed to is written once and copied
/// nowhere: what differs between the frames is the plugin they name.
#[derive(Clone)]
pub(super) struct SharedEvent {
    pub(super) json: Arc<[u8]>,
    pub(super) kind: EventKind,
}

impl SharedEvent {
    /// Serializes an event once, for every plugin that wants it.
    pub(super) fn new(event: EventEnvelope) -> Option<Arc<SharedEvent>> {
        match serde_json::to_vec(&event) {
            Ok(json) => Some(Arc::new(SharedEvent {
                json: Arc::from(json),
                kind: event.kind,
            })),
            Err(error) => {
                // Nothing inside an `EventEnvelope` can fail to serialize today;
                // if that changes, dropping the event beats waking a plugin with
                // a broken frame.
                tracing::warn!(%error, "an event couldn't be serialized");
                None
            }
        }
    }
}

/// One `plugin.publish`, serialized once for every plugin that subscribed to
/// its topic.
///
/// The bytes are the envelope, as the events' are: what differs between the
/// frames of two subscribers is the message around them, not the topic.
#[derive(Clone)]
pub(super) struct SharedTopic {
    pub(super) json: Arc<[u8]>,
}

impl SharedTopic {
    /// The envelope for one publish, cut and flagged as an event's payload is.
    pub(super) fn new(topic: TopicEvent) -> Option<Arc<SharedTopic>> {
        match serde_json::to_vec(&topic.truncated()) {
            Ok(json) => Some(Arc::new(SharedTopic {
                json: Arc::from(json),
            })),
            Err(error) => {
                tracing::warn!(%error, "a published topic couldn't be serialized");
                None
            }
        }
    }
}

/// The payload of an `Event` frame: a batch's events, around the plugin they are
/// for.
///
/// This is written by hand because the events are already bytes: serializing a
/// `ToHelper::Event` per plugin would copy every event once per subscriber, which
/// on a busy account is the difference between one serialization and four. The
/// test below reads it back with `serde_json` to keep it honest.
pub(super) fn event_payload(plugin: &PluginId, events: &[Arc<SharedEvent>]) -> Vec<u8> {
    frame_payload(plugin, events, &[])
}

/// The payload of one event batch, with the topics published to the same plugin
/// in the same call.
///
/// `topics` is left out when it is empty, so an event nobody published for costs
/// the frame it always did.
pub(super) fn frame_payload(
    plugin: &PluginId,
    events: &[Arc<SharedEvent>],
    topics: &[Arc<SharedTopic>],
) -> Vec<u8> {
    let mut payload = Vec::with_capacity(
        64 + events
            .iter()
            .map(|event| event.json.len() + 1)
            .sum::<usize>()
            + topics
                .iter()
                .map(|topic| topic.json.len() + 1)
                .sum::<usize>(),
    );
    payload.extend_from_slice(b"{\"t\":\"event\",\"plugin\":");
    match serde_json::to_vec(plugin) {
        Ok(id) => payload.extend_from_slice(&id),
        Err(_) => payload.extend_from_slice(b"null"),
    }
    payload.extend_from_slice(b",\"events\":[");
    for (index, event) in events.iter().enumerate() {
        if index > 0 {
            payload.push(b',');
        }
        payload.extend_from_slice(&event.json);
    }
    payload.push(b']');
    if !topics.is_empty() {
        payload.extend_from_slice(b",\"topics\":[");
        for (index, topic) in topics.iter().enumerate() {
            if index > 0 {
                payload.push(b',');
            }
            payload.extend_from_slice(&topic.json);
        }
        payload.push(b']');
    }
    payload.push(b'}');
    payload
}
