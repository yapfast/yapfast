//! A plugin's calls about the account itself that Discord or Spotify answer:
//! its own activity, the profile, a search, the signed-in devices and the
//! linked Spotify. Each becomes a [`DiscordCommand::PluginAsk`] and its answer
//! comes back as [`Event::PluginAnswered`]; the drafts are read here and now.
//!
//! [`DiscordCommand::PluginAsk`]: crate::chat::DiscordCommand::PluginAsk

use super::*;
use crate::chat::{DiscordCommand, PluginAsk};
use crate::plugins::state::AppCall;

/// The activity a plugin shows on the account, for the window to say so.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActivityShown {
    pub plugin: PluginId,
    /// The plugin's name.
    pub by: String,
    /// The activity's own name.
    pub name: String,
}

/// A plugin recording the microphone, as the red bar names it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordingShown {
    pub plugin: PluginId,
    /// The plugin's name.
    pub by: String,
    /// When it started, in Unix milliseconds, for the bar's clock.
    pub since_ms: u64,
}

/// Whether a call is answered once Discord or Spotify has, rather than now.
pub(super) fn answered_later(request: &AppCall) -> bool {
    matches!(
        request,
        AppCall::SetPluginActivity { .. }
            | AppCall::SetProfile(_)
            | AppCall::Search(_)
            | AppCall::Sessions
            | AppCall::Affinities(_)
            | AppCall::Spotify(_)
            | AppCall::SendVoice { .. }
            | AppCall::Messages { .. }
            | AppCall::Badges { .. }
            | AppCall::MediaFetch { .. }
            | AppCall::ServerDetails { .. }
            | AppCall::FavouriteGifs
    )
}

impl Application {
    /// Hands a call Discord or Spotify answers to the account's session, or
    /// answers now why it can't go.
    pub(super) fn plugins_ask(
        &mut self,
        plugin: PluginId,
        call: u64,
        request: AppCall,
    ) -> Vec<AppEffect> {
        let mut effects = Vec::new();
        let ask = match request {
            AppCall::SetPluginActivity { activity } => {
                self.state.plugin_activity = activity.as_ref().map(|activity| ActivityShown {
                    plugin: plugin.clone(),
                    by: self
                        .state
                        .plugins
                        .as_ref()
                        .and_then(|plugins| plugins.name(&plugin))
                        .unwrap_or_else(|| plugin.to_string()),
                    name: activity.name.trim().to_owned(),
                });
                Some(PluginAsk::Activity(activity))
            }
            AppCall::SetProfile(profile) => {
                // The custom status goes the way the person's own does; the
                // bio and the pronouns are one PATCH of the profile.
                if let Some(status) = profile.custom_status {
                    match self.plugin_status(status.emoji, status.text, None) {
                        Ok(status) => effects.extend(status),
                        Err(message) => {
                            self.plugins_reply(&plugin, call, HostReturn::Error { message });
                            return Vec::new();
                        }
                    }
                }
                if profile.bio.is_none() && profile.pronouns.is_none() {
                    self.plugins_reply(&plugin, call, HostReturn::Ok(Value::Bool(true)));
                    return effects;
                }
                Some(PluginAsk::Profile {
                    bio: profile.bio,
                    pronouns: profile.pronouns,
                })
            }
            AppCall::Search(request) => Some(PluginAsk::Search(request)),
            AppCall::Sessions => Some(PluginAsk::Sessions),
            AppCall::Affinities(request) => Some(PluginAsk::Affinities(request)),
            AppCall::Badges { person, server } => Some(PluginAsk::Badges { person, server }),
            AppCall::MediaFetch { url } => Some(PluginAsk::Media { url }),
            AppCall::ServerDetails { server } => Some(PluginAsk::ServerDetails { server }),
            AppCall::FavouriteGifs => Some(PluginAsk::FavouriteGifs),
            AppCall::Messages {
                room,
                message,
                before,
                limit,
            } => Some(PluginAsk::Messages {
                room,
                message,
                before,
                limit,
            }),
            AppCall::SendVoice {
                room,
                path,
                duration_ms,
                waveform,
            } => Some(PluginAsk::Voice {
                room,
                path,
                duration_ms,
                waveform,
            }),
            AppCall::Spotify(action) => Some(PluginAsk::Spotify(action)),
            other => {
                let (more, answer) = self.plugins_call(&plugin, other);
                self.plugins_reply(&plugin, call, answer);
                return more;
            }
        };
        if let Err(message) = self.ready_for_discord() {
            self.plugins_reply(&plugin, call, HostReturn::Error { message });
            return effects;
        }
        if let Some(ask) = ask {
            effects.push(AppEffect::Backend(Command::Discord(
                DiscordCommand::PluginAsk {
                    plugin: plugin.to_string(),
                    call,
                    ask: Box::new(ask),
                },
            )));
        }
        effects
    }

    /// Starts a plugin's recording, which answers its call when it ends: one
    /// at a time, from the microphone chosen for calls, with the red bar up
    /// while it runs.
    pub(super) fn plugins_record(
        &mut self,
        plugin: PluginId,
        call: u64,
        format: yapfast_plugin_api::call::RecordingFormat,
        max_seconds: Option<u32>,
    ) -> Vec<AppEffect> {
        if let Some(shown) = &self.state.plugin_recording {
            let message = format!("{} is recording the microphone already", shown.by);
            self.plugins_reply(&plugin, call, HostReturn::Error { message });
            return Vec::new();
        }
        let Some(plugins) = self.state.plugins.as_mut() else {
            return Vec::new();
        };
        let folder = crate::plugins::recorder::folder(plugins.attach_dir());
        let Some(answerer) = plugins.answerer() else {
            return Vec::new();
        };
        match crate::plugins::recorder::start(
            plugin.clone(),
            call,
            format,
            max_seconds,
            self.state.voice.microphone.clone(),
            &folder,
            answerer,
        ) {
            Ok(recording) => {
                self.state.plugin_recording = Some(RecordingShown {
                    by: self.plugin_name(&plugin),
                    plugin,
                    since_ms: crate::clock::now_ms(),
                });
                self.plugin_recorder = Some(recording);
                vec![AppEffect::Plugin(Effect::Recording)]
            }
            Err(error) => {
                self.plugins_reply(
                    &plugin,
                    call,
                    HostReturn::Error {
                        message: format!("{error:#}"),
                    },
                );
                Vec::new()
            }
        }
    }

    /// A recording ended, or its plugin stopped: the bar goes.
    pub(super) fn plugins_recording_ended(&mut self, plugin: &PluginId) -> Vec<AppEffect> {
        if let Some(recording) = &self.plugin_recorder
            && recording.plugin == *plugin
        {
            recording.stop_for_plugin();
            self.plugin_recorder = None;
        }
        if self
            .state
            .plugin_recording
            .as_ref()
            .is_some_and(|shown| shown.plugin == *plugin)
        {
            self.state.plugin_recording = None;
            return vec![AppEffect::Plugin(Effect::Recording)];
        }
        Vec::new()
    }

    /// Takes a plugin's activity off the account, when it is turned off or
    /// suspended: nobody answers for an activity it no longer runs.
    pub(super) fn plugins_activity_gone(&mut self, plugin: &PluginId) -> Option<AppEffect> {
        if self
            .state
            .plugin_activity
            .as_ref()
            .is_none_or(|shown| shown.plugin != *plugin)
        {
            return None;
        }
        self.state.plugin_activity = None;
        self.ready_for_discord().ok()?;
        // No call waits on this, so the answer goes to no plugin.
        Some(AppEffect::Backend(Command::Discord(
            DiscordCommand::PluginAsk {
                plugin: String::new(),
                call: 0,
                ask: Box::new(PluginAsk::Activity(None)),
            },
        )))
    }

    /// Answers one of a plugin's calls.
    pub(super) fn plugins_reply(&mut self, plugin: &PluginId, call: u64, answer: HostReturn) {
        if let Some(plugins) = self.state.plugins.as_mut() {
            plugins.answer_call(plugin, call, answer);
        }
    }

    /// Discord or Spotify answered a plugin's ask.
    pub(in crate::application) fn plugins_answered(
        &mut self,
        plugin: &str,
        call: u64,
        answer: HostReturn,
    ) {
        let Ok(plugin) = PluginId::parse(plugin) else {
            return;
        };
        // An activity Discord didn't take isn't one the account shows.
        if matches!(answer, HostReturn::Error { .. })
            && self
                .state
                .plugin_activity
                .as_ref()
                .is_some_and(|shown| shown.plugin == plugin)
        {
            self.state.plugin_activity = None;
        }
        self.plugins_reply(&plugin, call, answer);
    }

    /// What the linked Spotify plays changed: the plugins subscribed to
    /// `spotify_state` hear it, and nothing is built while none is.
    pub(in crate::application) fn plugins_spotify_shared(&mut self, now: Value) {
        let wanted = self
            .state
            .plugins
            .as_ref()
            .is_some_and(|plugins| plugins.subscribes(EventKind::SpotifyState));
        if !wanted {
            return;
        }
        self.plugins_observe(envelope(
            EventKind::SpotifyState,
            PluginNetwork::Discord,
            None,
            None,
            true,
            now,
        ));
    }

    /// The drafts the person left unsent, for `drafts.list`.
    pub(super) fn plugins_drafts(&self) -> HostReturn {
        let drafts: Vec<yapfast_plugin_api::account::DraftView> = self
            .state
            .drafts
            .iter()
            .filter(|(_, body)| !body.trim().is_empty())
            .map(|(room, body)| yapfast_plugin_api::account::DraftView {
                room: room.clone(),
                body: body.clone(),
            })
            .collect();
        match serde_json::to_value(drafts) {
            Ok(value) => HostReturn::Ok(value),
            Err(error) => HostReturn::Error {
                message: error.to_string(),
            },
        }
    }
}
