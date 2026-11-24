//! Joining a voice channel: the join waits for Discord to say both the
//! voice session and the voice server, or gives up after a while.

use std::pin::Pin;

use anyhow::{Result, anyhow};
use serde_json::Value;
use tokio::{
    sync::oneshot,
    time::{Duration, Instant, Sleep},
};

use super::{VoiceServer, server, tracker::Tracker};
use crate::{event::Event, gateway::Dispatch};

const JOIN_TIMEOUT: Duration = Duration::from_secs(15);

/// The voice join under way, if any.
pub(super) struct VoiceJoin {
    pending: Option<Pending>,
    deadline: Pin<Box<Sleep>>,
}

struct Pending {
    guild: String,
    channel: String,
    reply: oneshot::Sender<Result<VoiceServer>>,
    session_id: Option<String>,
    server: Option<(String, String)>,
    /// What came while it waited, for the log if it times out, never for
    /// the join itself: this session's voice state in the join's channel,
    /// this session's for no channel or another (a leave just before),
    /// this account's other sessions', and voice servers still being
    /// allocated (no endpoint).
    in_channel: bool,
    elsewhere: u32,
    other_sessions: u32,
    allocating: u32,
}

impl Pending {
    /// Whether a voice state or voice server event is about this join: in
    /// its server, or for a DM call, its channel, as those carry no server.
    fn answers(&self, data: &Value) -> bool {
        match server(&self.guild) {
            Some(guild) => data["guild_id"] == guild,
            None => data["guild_id"].is_null() && data["channel_id"] == self.channel.as_str(),
        }
    }

    /// What Discord said while the join waited, so a join that times out
    /// says which half never came. No ids.
    fn heard(&self) -> String {
        let said = |came: bool| if came { "came" } else { "never came" };
        format!(
            "its voice state {}, the voice server {}; {} of this session's voice \
             state(s) for no channel or another, {} from other sessions, {} voice \
             server(s) still allocating",
            said(self.in_channel),
            said(self.server.is_some()),
            self.elsewhere,
            self.other_sessions,
            self.allocating,
        )
    }
}

impl VoiceJoin {
    pub(super) fn new() -> Self {
        Self {
            pending: None,
            deadline: Box::pin(tokio::time::sleep(JOIN_TIMEOUT)),
        }
    }

    /// Waits for the server of a join just asked for, replacing any other.
    pub(super) fn start(
        &mut self,
        guild: String,
        channel: String,
        reply: oneshot::Sender<Result<VoiceServer>>,
    ) {
        self.deadline.as_mut().reset(Instant::now() + JOIN_TIMEOUT);
        let pending = Pending {
            guild,
            channel,
            reply,
            session_id: None,
            server: None,
            in_channel: false,
            elsewhere: 0,
            other_sessions: 0,
            allocating: 0,
        };
        if let Some(previous) = self.pending.replace(pending) {
            let _ = previous
                .reply
                .send(Err(anyhow!("Another voice join replaced this one.")));
        }
    }

    /// Stops waiting for a join in `guild`, which this account left;
    /// returns whether one was under way.
    pub(super) fn leave(&mut self, guild: &str) -> bool {
        let joining = self
            .pending
            .as_ref()
            .is_some_and(|pending| pending.guild == guild);
        if joining {
            self.pending = None;
        }
        joining
    }

    pub(super) fn waiting(&self) -> bool {
        self.pending.is_some()
    }

    /// Whether a voice server update is the one this join waits for, rather
    /// than one moving a call under way.
    pub(super) fn awaits(&self, data: &Value) -> bool {
        self.pending
            .as_ref()
            .is_some_and(|pending| pending.answers(data))
    }

    /// Takes in a dispatch, and answers the join once Discord said all it
    /// needs. Only a voice state of this gateway session answers it, not
    /// one of this account's other clients.
    pub(super) fn apply(&mut self, tracker: &Tracker, event: &Dispatch) {
        let Some(waiting) = self.pending.as_mut() else {
            return;
        };
        let (me, data) = (tracker.me.as_str(), &event.data);
        match event.event {
            Event::VoiceStateUpdate
                if data["user_id"] == me
                    && waiting.answers(data)
                    && tracker
                        .session
                        .as_deref()
                        .is_none_or(|session| data["session_id"] == session) =>
            {
                waiting.session_id = data["session_id"].as_str().map(str::to_owned);
                if data["channel_id"] == waiting.channel.as_str() {
                    waiting.in_channel = true;
                } else {
                    waiting.elsewhere += 1;
                }
            }
            Event::VoiceStateUpdate if data["user_id"] == me && waiting.answers(data) => {
                waiting.other_sessions += 1;
            }
            // The endpoint is null while Discord is still allocating a server.
            Event::VoiceServerUpdate if waiting.answers(data) => {
                if let (Some(token), Some(endpoint)) =
                    (data["token"].as_str(), data["endpoint"].as_str())
                {
                    waiting.server = Some((token.into(), endpoint.into()));
                } else {
                    waiting.allocating += 1;
                }
            }
            _ => {}
        }
        if let (Some(session_id), Some((token, endpoint))) = (&waiting.session_id, &waiting.server)
        {
            let server = VoiceServer {
                user_id: me.to_owned(),
                session_id: session_id.clone(),
                token: token.clone(),
                endpoint: endpoint.clone(),
            };
            if let Some(done) = self.pending.take() {
                let _ = done.reply.send(Ok(server));
            }
        }
    }

    /// Gives up on the join once Discord took too long to assign a server.
    pub(super) async fn timed_out(&mut self) {
        self.deadline.as_mut().await;
        if let Some(waiting) = &self.pending {
            tracing::warn!(
                heard = waiting.heard(),
                "Discord didn't answer a voice join in time"
            );
        }
        self.fail("Discord didn't assign a voice server. Try again.");
    }

    pub(super) fn fail(&mut self, message: &str) {
        if let Some(pending) = self.pending.take() {
            let _ = pending.reply.send(Err(anyhow!("{message}")));
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::ready::PRIVATE;

    #[tokio::test]
    async fn takes_this_sessions_voice_session_not_another_clients() {
        let tracker = Tracker::from_ready(
            "1".into(),
            &json!({"session_id": "here", "user": {"id": "1"}}),
            Default::default(),
        );
        let (reply, mut answer) = oneshot::channel();
        let mut join = VoiceJoin::new();
        join.start("10".into(), "11".into(), reply);
        let dispatch = |event, data| Dispatch { event, data };
        let state = |session: &str| json!({"guild_id": "10", "user_id": "1", "channel_id": "11", "session_id": session});
        join.apply(&tracker, &dispatch(Event::VoiceStateUpdate, state("phone")));
        join.apply(
            &tracker,
            &dispatch(
                Event::VoiceServerUpdate,
                json!({"guild_id": "10", "token": "t", "endpoint": "e"}),
            ),
        );
        assert!(join.waiting(), "the phone's voice state doesn't answer it");
        join.apply(&tracker, &dispatch(Event::VoiceStateUpdate, state("here")));
        let server = answer.try_recv().unwrap().unwrap();
        assert_eq!(server.session_id, "here");
        assert!(!join.leave("10"), "nothing is under way once answered");
    }

    #[test]
    fn answers_a_dm_join_by_its_channel() {
        let (reply, _) = oneshot::channel();
        let waiting = Pending {
            guild: PRIVATE.into(),
            channel: "20".into(),
            reply,
            session_id: None,
            server: None,
            in_channel: false,
            elsewhere: 0,
            other_sessions: 0,
            allocating: 0,
        };
        assert!(waiting.answers(&json!({"guild_id": null, "channel_id": "20"})));
        assert!(!waiting.answers(&json!({"guild_id": null, "channel_id": "21"})));
        assert!(!waiting.answers(&json!({"guild_id": "10", "channel_id": "20"})));
    }

    /// A join that times out says what came meanwhile: here the leave's own
    /// reply from this session (no channel, as a switch within a server
    /// sends), another session's voice state and a voice server still being
    /// allocated, but neither half of this join's answer.
    #[tokio::test]
    async fn a_join_that_times_out_says_what_came() {
        let tracker = Tracker::from_ready(
            "1".into(),
            &json!({"session_id": "here", "user": {"id": "1"}}),
            Default::default(),
        );
        let (reply, _answer) = oneshot::channel();
        let mut join = VoiceJoin::new();
        join.start("10".into(), "11".into(), reply);
        let dispatch = |event, data| Dispatch { event, data };
        let state = |session: &str, channel: Value| json!({"guild_id": "10", "user_id": "1", "channel_id": channel, "session_id": session});
        join.apply(
            &tracker,
            &dispatch(Event::VoiceStateUpdate, state("here", Value::Null)),
        );
        join.apply(
            &tracker,
            &dispatch(Event::VoiceStateUpdate, state("phone", json!("11"))),
        );
        join.apply(
            &tracker,
            &dispatch(
                Event::VoiceServerUpdate,
                json!({"guild_id": "10", "token": "t", "endpoint": null}),
            ),
        );
        assert!(join.waiting());
        let heard = join.pending.as_ref().unwrap().heard();
        assert_eq!(
            heard,
            "its voice state never came, the voice server never came; 1 of this \
             session's voice state(s) for no channel or another, 1 from other \
             sessions, 1 voice server(s) still allocating"
        );
        assert!(
            !heard.contains("here") && !heard.contains("phone"),
            "no ids"
        );
        // This session's state in the channel is its voice state.
        join.apply(
            &tracker,
            &dispatch(Event::VoiceStateUpdate, state("here", json!("11"))),
        );
        assert!(
            join.pending
                .as_ref()
                .unwrap()
                .heard()
                .starts_with("its voice state came")
        );
    }
}
