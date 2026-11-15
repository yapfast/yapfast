//! Streams being started or watched: each waits for Discord to name its
//! server, or gives up after a while.

use std::{collections::HashMap, pin::Pin};

use anyhow::{Result, anyhow};
use serde_json::Value;
use tokio::{
    sync::oneshot,
    time::{Duration, Instant, Sleep},
};
use yapfast_lang::tr;

use super::{server, tracker::Tracker};
use crate::{
    event::Event,
    gateway::{Dispatch, Gateway},
    stream::{self, Opening, StreamServer},
};

/// How long a stream waits for Discord to assign its server.
const OPEN_TIMEOUT: Duration = Duration::from_secs(15);

/// The streams waiting for their server, by stream key.
pub(super) struct StreamOpenings {
    waiting: HashMap<String, PendingStream>,
    deadline: Pin<Box<Sleep>>,
}

/// A stream started or watched, waiting for its server.
struct PendingStream {
    guild: String,
    reply: oneshot::Sender<Result<StreamServer>>,
    opening: Opening,
}

impl StreamOpenings {
    pub(super) fn new() -> Self {
        Self {
            waiting: HashMap::new(),
            deadline: Box::pin(tokio::time::sleep(OPEN_TIMEOUT)),
        }
    }

    pub(super) fn waiting(&self) -> bool {
        !self.waiting.is_empty()
    }

    /// Starts sharing to the voice channel this account is in.
    pub(super) async fn go_live(
        &mut self,
        gateway: &mut Gateway,
        tracker: &Tracker,
        guild: String,
        channel: String,
        reply: oneshot::Sender<Result<StreamServer>>,
    ) -> Result<()> {
        if tracker
            .voice_channel(&guild)
            .is_none_or(|current| current != channel)
        {
            let _ = reply.send(Err(anyhow!(tr!("Join the call before going live."))));
            return Ok(());
        }
        let key = stream::key(&guild, &channel, &tracker.me);
        self.wait(
            key.clone(),
            guild.clone(),
            reply,
            tr!("Going live again replaced this attempt."),
        );
        // Unpaused straight away, as Discord-video-stream
        // does: viewers of a paused stream get no video.
        match gateway.create_stream(server(&guild), &channel).await {
            Ok(()) => gateway.pause_stream(&key, false).await,
            Err(error) => Err(error),
        }
    }

    /// Starts watching a stream in the voice channel this account is in.
    pub(super) async fn watch(
        &mut self,
        gateway: &mut Gateway,
        tracker: &Tracker,
        stream_key: String,
        reply: oneshot::Sender<Result<StreamServer>>,
    ) -> Result<()> {
        match stream::parse(&stream_key) {
            Some((guild, channel, _)) if tracker.voice_channel(guild) == Some(channel) => {
                let guild = guild.to_owned();
                self.wait(
                    stream_key.clone(),
                    guild,
                    reply,
                    tr!("Watching again replaced this attempt."),
                );
                gateway.watch_stream(&stream_key).await
            }
            Some(_) => {
                let _ = reply.send(Err(anyhow!(tr!("Join the call to watch its streams."))));
                Ok(())
            }
            None => {
                let _ = reply.send(Err(anyhow!(tr!(
                    "That isn't a stream Yapfast knows how to watch."
                ))));
                Ok(())
            }
        }
    }

    fn wait(
        &mut self,
        key: String,
        guild: String,
        reply: oneshot::Sender<Result<StreamServer>>,
        replaced: &str,
    ) {
        self.deadline.as_mut().reset(Instant::now() + OPEN_TIMEOUT);
        let waiting = PendingStream {
            guild,
            reply,
            opening: Opening::default(),
        };
        if let Some(previous) = self.waiting.insert(key, waiting) {
            let _ = previous.reply.send(Err(anyhow!("{replaced}")));
        }
    }

    /// Ends this account's stream, or stops watching someone else's.
    pub(super) async fn end(&mut self, gateway: &mut Gateway, stream_key: &str) -> Result<()> {
        if let Some(waiting) = self.waiting.remove(stream_key) {
            let _ = waiting.reply.send(Err(anyhow!(tr!(
                "The stream was stopped before it started."
            ))));
        }
        gateway.delete_stream(stream_key).await
    }

    /// Whether a stream server update is for a stream waiting here, rather
    /// than one moving a stream under way.
    pub(super) fn awaits(&self, data: &Value) -> bool {
        data["stream_key"]
            .as_str()
            .is_some_and(|key| self.waiting.contains_key(key))
    }

    /// Passes a stream dispatch to the streams being opened, and answers those
    /// that are now ready or that Discord refused.
    pub(super) fn apply(&mut self, tracker: &Tracker, event: &Dispatch) {
        let data = &event.data;
        let Some(key) = data["stream_key"].as_str() else {
            return;
        };
        let Some(mut waiting) = self.waiting.remove(key) else {
            return;
        };
        if event.event == Event::StreamDelete {
            let reason = match data["reason"].as_str() {
                Some("stream_full") => tr!("The stream is full."),
                Some("unauthorized") => tr!("You aren't allowed to stream or watch here."),
                _ => tr!("The stream ended before it started."),
            };
            let _ = waiting.reply.send(Err(anyhow!(reason)));
            return;
        }
        waiting.opening.apply(key, &event.event, data);
        self.finish(tracker, key, waiting);
    }

    /// Answers the streams whose server Discord already named once this
    /// account's voice session is known too, as when it arrives after them.
    pub(super) fn voice_changed(&mut self, tracker: &Tracker) {
        let keys: Vec<String> = self.waiting.keys().cloned().collect();
        for key in keys {
            if let Some(waiting) = self.waiting.remove(&key) {
                self.finish(tracker, &key, waiting);
            }
        }
    }

    /// Answers a stream once its server and this account's voice session are
    /// known, or keeps it waiting.
    fn finish(&mut self, tracker: &Tracker, key: &str, waiting: PendingStream) {
        let session = tracker.voice_session(&waiting.guild);
        match session.and_then(|session| waiting.opening.server(key, &tracker.me, session)) {
            Some(server) => {
                let _ = waiting.reply.send(Ok(server));
            }
            None => {
                self.waiting.insert(key.to_owned(), waiting);
            }
        }
    }

    /// Gives up on every stream once Discord took too long to assign one.
    pub(super) async fn timed_out(&mut self) {
        self.deadline.as_mut().await;
        self.fail(tr!("Discord didn't assign a stream server. Try again."));
    }

    pub(super) fn fail(&mut self, message: &str) {
        for (_, waiting) in self.waiting.drain() {
            let _ = waiting.reply.send(Err(anyhow!("{message}")));
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[tokio::test]
    async fn a_stream_opens_when_the_voice_session_comes_last() {
        let ready = json!({
            "user": {"id": "1", "username": "me"},
            "guilds": [{"id": "10", "properties": {"name": "Test"}, "channels": [
                {"id": "11", "name": "Lounge", "type": 2, "position": 0},
            ]}],
        });
        let mut tracker = Tracker::from_ready("1".into(), &ready, Default::default());
        let mut openings = StreamOpenings::new();
        let (reply, mut answer) = oneshot::channel();
        let key = "guild:10:11:2";
        openings.wait(key.into(), "10".into(), reply, "replaced");
        for (event, data) in [
            (
                Event::StreamCreate,
                json!({"stream_key": key, "rtc_server_id": "99"}),
            ),
            (
                Event::StreamServerUpdate,
                json!({"stream_key": key, "token": "t", "endpoint": "e"}),
            ),
        ] {
            openings.apply(&tracker, &Dispatch { event, data });
        }
        assert!(answer.try_recv().is_err(), "no voice session yet");
        tracker.apply(
            &Event::VoiceStateUpdate,
            &json!({"guild_id": "10", "user_id": "1", "channel_id": "11", "session_id": "s"}),
        );
        openings.voice_changed(&tracker);
        let server = answer.try_recv().expect("answered").expect("opened");
        assert_eq!(
            (server.session_id.as_str(), server.rtc_server_id.as_str()),
            ("s", "99")
        );
        assert!(!openings.waiting());
    }
}
