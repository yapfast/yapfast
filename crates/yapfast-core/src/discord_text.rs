//! A Discord text channel in Yapfast's conversation view. Yapfast keeps the open
//! channel's loaded messages in memory, and those of channels open lately,
//! so going back to one shows it at once; history loads from Discord and
//! live changes arrive through the gateway. See docs/discord/README.md.
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use tokio::{
    sync::{
        broadcast::{self, error::RecvError},
        mpsc::{UnboundedReceiver, UnboundedSender},
        watch,
    },
    task::JoinSet,
};
use yapfast_discord::{
    member_list::ListedMember,
    message::{Message, Page, Reply as DiscordReply, Shown},
    service::{Discord, Live, Snapshot, Status},
};

use crate::{
    chat::{
        Attachment, Beside, ButtonStyle, Card, CardButton, CardRow, ChatMember, ChatMessage,
        Command, Contact, CustomEmoji, Delivery, Direction, DiscordCommand, Embed, EmbedField,
        Event, History, MAX_PREVIEW_DOWNLOAD_BYTES, MentionGroup, Notification, OutgoingFile,
        Picture, Placement, Poll, PollAnswer, Profile, ProfileFriend, ProfileRole, ProfileServer,
        Reaction, Reply, Role, SearchHit, ThreadLink, Topic, Video, VideoCommand, decode_preview,
        decode_view, plain_snippet,
    },
    discord::{self, KEPT_CHANNELS, KEPT_MESSAGES},
    mention::{Mentioned, Target},
    network::Network,
};

mod account;
mod audit_log;
mod channels;
mod commands;
mod expressions;
mod gifs;
mod open_channel;
mod overwrites;
mod own_profile;
mod ownership;
mod people;
mod render;
mod roles;
mod server_settings;

pub use channels::Channels;
pub use commands::{command, finished, live};
pub use open_channel::{Load, OpenChannel};

/// Discord's message length limit without Nitro, in characters.
pub const MAX_CHARS: usize = 2000;
/// How long someone shows as typing after Discord says they started.
const TYPING: Duration = Duration::from_secs(10);
/// How often Discord hears that this account is still typing.
const TYPING_AGAIN: Duration = Duration::from_secs(8);
/// How long an open channel waits for its member list before saying none came.
const MEMBERS_WAIT: Duration = Duration::from_secs(8);
/// How often changed channels' newest messages go to disk; see
/// [`yapfast_discord::cache`].
const SAVE_EVERY: Duration = Duration::from_secs(20);
/// Saving a file holds it in memory first.
const MAX_SAVE_BYTES: usize = 100 * 1024 * 1024;
/// The largest video that plays, past Discord's 500 MB limit for Nitro.
const MAX_VIDEO_BYTES: u64 = 1024 * 1024 * 1024;

/// Work for the open channel that finished in the background.
pub enum Done {
    History {
        room: String,
        load: Load,
        /// [`OpenChannel::generation`] when it was asked for.
        generation: u64,
        result: Result<Page, String>,
    },
    Sent {
        room: String,
        nonce: String,
        result: Result<Message, String>,
    },
    Edited {
        room: String,
        item: String,
        result: Result<Message, String>,
    },
    Deleted {
        room: String,
        id: String,
        result: Result<(), String>,
    },
    Pinned {
        room: String,
        id: String,
        pinned: bool,
        result: Result<(), String>,
    },
    Picture {
        room: String,
        url: String,
        picture: Option<Arc<Picture>>,
    },
    Image {
        room: String,
        item: String,
        index: usize,
        picture: Result<Arc<Picture>, &'static str>,
    },
    Saved {
        room: String,
        name: String,
        result: Result<(), &'static str>,
    },
    Copied {
        room: String,
        picture: Result<Vec<u8>, &'static str>,
    },
    /// An action failed; the reason is for the person.
    Failed { room: String, message: String },
    /// Someone may have stopped typing.
    Typing { room: String },
    /// A channel opened a while ago; its member list should have come by now.
    MembersLate { room: String },
}

/// What woke the Discord side of the chat actor.
pub(crate) enum Wake {
    Snapshot(Option<Arc<Snapshot>>),
    Live(Option<Result<Live, RecvError>>),
    Finished(Done),
    /// Time to put changed channels on disk.
    Save,
    Video(crate::playback::Wake),
}

/// The linked Discord account in the chat actor: its connection and the open
/// channel. The chat actor owns it beside Matrix and voice.
pub(crate) struct Chat {
    discord: Discord,
    // Each becomes None once the connection is gone.
    states: Option<watch::Receiver<Arc<Snapshot>>>,
    live: Option<broadcast::Receiver<Live>>,
    online: bool,
    channels: Channels,
    done: UnboundedSender<Done>,
    finished: UnboundedReceiver<Done>,
    pictures: discord::Pictures,
    saving: tokio::time::Interval,
    /// The video playing in the open channel.
    video: Option<crate::playback::Playback>,
    /// How loud the last video played, and whether muted: the next plays
    /// the same.
    video_sound: (f32, bool),
}

impl Drop for Chat {
    /// What changed since the last save goes to disk before Yapfast closes.
    fn drop(&mut self) {
        for channel in self
            .channels
            .open
            .iter_mut()
            .chain(&mut self.channels.recent)
        {
            if let Some((id, messages)) = channel.take_changes() {
                yapfast_discord::cache::save_messages(&id, messages);
            }
        }
    }
}

impl Chat {
    /// Connects when this device has a linked account; without one, there's
    /// nothing to start.
    pub(crate) fn start() -> anyhow::Result<Option<Self>> {
        let token = match yapfast_discord::token::load() {
            Ok(token) => token,
            Err(error) if error.is::<yapfast_discord::token::NotLinked>() => return Ok(None),
            Err(error) => return Err(error),
        };
        let discord = Discord::start(token)?;
        let (done, finished) = tokio::sync::mpsc::unbounded_channel();
        Ok(Some(Self {
            states: Some(discord.snapshots()),
            live: Some(discord.live()),
            discord,
            online: false,
            channels: Channels::default(),
            done,
            finished,
            pictures: discord::Pictures::new(),
            saving: {
                let mut saving =
                    tokio::time::interval_at(tokio::time::Instant::now() + SAVE_EVERY, SAVE_EVERY);
                saving.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
                saving
            },
            video: None,
            video_sound: (1.0, false),
        }))
    }

    pub(crate) fn service(&self) -> Discord {
        self.discord.clone()
    }

    pub(crate) fn command(&mut self, command: Command, events: &UnboundedSender<Event>) {
        match command {
            Command::Discord(DiscordCommand::Pictures(urls)) => {
                self.pictures.fetch(&self.discord, urls, events);
                return;
            }
            Command::PlayVideo {
                room,
                item,
                index,
                width,
                height,
                speakers,
            } => {
                self.play_video(room, item, index, (width, height), speakers, events);
                return;
            }
            Command::Video(VideoCommand::Stop) => {
                self.stop_video(events);
                return;
            }
            Command::Video(command) => {
                if let Some(playback) = &mut self.video {
                    playback.command(command);
                    self.video_sound = (playback.video.volume, playback.video.muted);
                    let _ = events.send(Event::Video(Some(playback.video.clone())));
                }
                return;
            }
            // A video plays only while its channel is open.
            Command::Open(_) | Command::CloseRoom => self.stop_video(events),
            _ => {}
        }
        self::command(
            command,
            Ok(&self.discord),
            &mut self.channels,
            &self.done,
            events,
        );
    }

    fn stop_video(&mut self, events: &UnboundedSender<Event>) {
        if self.video.take().is_some() {
            let _ = events.send(Event::Video(None));
        }
    }

    /// Plays an attachment's video in place of its poster, as it downloads,
    /// in place of any other.
    fn play_video(
        &mut self,
        room: String,
        item: String,
        index: usize,
        frame: (u32, u32),
        speakers: Option<String>,
        events: &UnboundedSender<Event>,
    ) {
        self.video = None;
        let mut video = Video {
            room,
            item,
            index,
            frames: None,
            duration_ms: None,
            size: None,
            loading: true,
            playing: true,
            ended: false,
            sound: false,
            muted: self.video_sound.1,
            volume: self.video_sound.0,
            error: None,
        };
        let found = self
            .channels
            .open
            .as_ref()
            .filter(|channel| channel.room == video.room)
            .and_then(|channel| channel.attachment(&video.item, index))
            .and_then(|attachment| Some((attachment.video_url.clone()?, attachment.size)));
        let Some((url, size)) = found else {
            video.loading = false;
            video.error = Some("This video is no longer available.".into());
            let _ = events.send(Event::Video(Some(video)));
            return;
        };
        let directory = yapfast_discord::cache::videos()
            .unwrap_or_else(|| std::env::temp_dir().join("yapfast-videos"));
        let discord = self.discord.clone();
        let download = move |mut file: std::fs::File| async move {
            discord
                .download_into(&url, &mut file, MAX_VIDEO_BYTES)
                .await
        };
        match crate::playback::Playback::start(
            video.clone(),
            &directory,
            size,
            frame,
            speakers,
            download,
        ) {
            Ok(playback) => {
                let _ = events.send(Event::Video(Some(playback.video.clone())));
                self.video = Some(playback);
            }
            Err(error) => {
                tracing::warn!(error = format!("{error:#}"), "could not play a video");
                video.loading = false;
                video.error = Some("Yapfast couldn't play this video.".into());
                let _ = events.send(Event::Video(Some(video)));
            }
        }
    }

    /// Waits for Discord. Cancel safe: the chat actor handles what it returns.
    pub(crate) async fn wait(&mut self) -> Wake {
        tokio::select! {
            snapshot = changed(&mut self.states) => Wake::Snapshot(snapshot),
            event = next_live(&mut self.live) => Wake::Live(event),
            Some(finished) = self.finished.recv() => Wake::Finished(finished),
            _ = self.saving.tick() => Wake::Save,
            wake = next_video(&mut self.video) => Wake::Video(wake),
        }
    }

    pub(crate) fn handle(&mut self, wake: Wake, events: &UnboundedSender<Event>) {
        match wake {
            Wake::Snapshot(Some(snapshot)) => {
                let online = snapshot.status == Status::Online;
                // Live events stop with the connection: kept channels load
                // their latest page again when opened, and messages sent
                // while disconnected arrive with the open one's.
                if !online && self.online {
                    self.channels.missed();
                }
                if online
                    && !self.online
                    && let Some(channel) = &mut self.channels.open
                {
                    channel.refresh(&self.discord, &self.done);
                }
                self.online = online;
                // A thread a message leads to shows its new count on the link.
                for channel in self
                    .channels
                    .open
                    .iter_mut()
                    .chain(&mut self.channels.aside)
                {
                    if channel.loaded && channel.links_changed(&snapshot) {
                        render::publish(channel, &self.discord, &self.done, events);
                    }
                }
                let _ = events.send(Event::Discord(snapshot));
                if let Some(channel) = &mut self.channels.open {
                    render::send_topic(channel, &self.discord, events, false);
                }
            }
            Wake::Live(Some(event)) => {
                live(event, &self.discord, &mut self.channels, &self.done, events);
            }
            Wake::Finished(done) => {
                finished(
                    done,
                    Some(&self.discord),
                    &mut self.channels,
                    &self.done,
                    events,
                );
            }
            Wake::Save => self.channels.save(),
            Wake::Video(wake) => {
                if let Some(playback) = &mut self.video {
                    playback.handle(wake);
                    let _ = events.send(Event::Video(Some(playback.video.clone())));
                }
            }
            Wake::Snapshot(None) | Wake::Live(None) => {}
        }
    }
}

/// What the playing video next does, or never without one.
async fn next_video(video: &mut Option<crate::playback::Playback>) -> crate::playback::Wake {
    match video {
        Some(playback) => playback.wait().await,
        None => std::future::pending().await,
    }
}

/// The next snapshot, or never after the connection is gone.
async fn changed(states: &mut Option<watch::Receiver<Arc<Snapshot>>>) -> Option<Arc<Snapshot>> {
    let Some(receiver) = states else {
        return std::future::pending().await;
    };
    if receiver.changed().await.is_err() {
        *states = None;
        return None;
    }
    Some(receiver.borrow_and_update().clone())
}

/// The next message event, or never after the connection is gone.
async fn next_live(
    live: &mut Option<broadcast::Receiver<Live>>,
) -> Option<Result<Live, RecvError>> {
    let Some(receiver) = live else {
        return std::future::pending().await;
    };
    match receiver.recv().await {
        Err(RecvError::Closed) => {
            *live = None;
            None
        }
        event => Some(event),
    }
}

/// Why Discord commands fail when no account is linked.
pub(crate) const NOT_LINKED: &str = "Discord isn't linked on this device.";

/// Answers a command for Discord while Discord isn't running, which `why`
/// explains: it fails, so nothing waits on it.
pub(crate) fn without(command: Command, why: &str, events: &UnboundedSender<Event>) {
    let (done, _) = tokio::sync::mpsc::unbounded_channel();
    self::command(command, Err(why), &mut Channels::default(), &done, events);
}

#[cfg(test)]
mod tests;
