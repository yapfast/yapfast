//! What a plugin asks the application to do, and how the answer is shaped.
//!
//! A guest's host call is one JSON request; the ones only the application can
//! carry out reach it as [`super::News::Notice`]`(Notice::Call)`, and this is
//! where each is done. Everything here is gated on the call's capability and
//! grant before it touches anything, and nothing here waits on a plugin: the
//! answer goes back through the bridge under the guest's own call id.

use super::*;

/// The largest file a plugin sends with a message: Discord's own cap for an
/// account without Nitro, so what goes is what Discord takes.
const PLUGIN_FILE_LIMIT: u64 = yapfast_plugin_api::call::SendMessage::MAX_FILE_BYTES;

impl Application {
    /// Carries out a call only the application can, and says what it answered.
    ///
    /// Each one wraps something Yapfast already does: the token, the rate
    /// limiter and the confirmation of a link stay where they are, and the
    /// plugin only names what it wants.
    pub(super) fn plugins_call(
        &mut self,
        plugin: &yapfast_plugin_api::PluginId,
        request: crate::plugins::state::AppCall,
    ) -> (Vec<AppEffect>, yapfast_plugin_api::call::HostReturn) {
        use crate::plugins::state::AppCall;
        let done = yapfast_plugin_api::call::HostReturn::Ok(serde_json::Value::Bool(true));
        let request = with_items(request);
        match request {
            AppCall::Send {
                room,
                body,
                mut files,
                made,
                agent,
            } => {
                if !self.state.rooms.iter().any(|listed| listed.id == room) {
                    return (
                        Vec::new(),
                        yapfast_plugin_api::call::HostReturn::Error {
                            message: format!("{room} isn't a conversation this account can see"),
                        },
                    );
                }
                let name = self
                    .state
                    .plugins
                    .as_ref()
                    .and_then(|plugins| plugins.name(plugin))
                    .unwrap_or_else(|| plugin.to_string());
                let now = crate::clock::now_ms();
                if agent {
                    self.state.plugin_agent = Some(super::AgentShown {
                        plugin: plugin.clone(),
                        name: name.clone(),
                        at: now,
                    });
                }
                // A plugin's own files are written where the upload reads them,
                // and go as a person's attached files do.
                if !made.is_empty() {
                    match self.plugins_write_made(plugin, made) {
                        Ok(written) => files.extend(written),
                        Err(message) => return (Vec::new(), HostReturn::Error { message }),
                    }
                }
                // Said under the message once the timeline shows it, so the
                // person can tell what a plugin sent from what they typed.
                self.plugin_sent_pending
                    .push((room.clone(), body.clone(), name, now));
                match self.plugins_send_a_message(&room, body, files) {
                    Ok(effects) => (effects, done),
                    Err(message) => (Vec::new(), HostReturn::Error { message }),
                }
            }
            AppCall::Edit {
                room,
                message,
                body,
            } => (
                vec![AppEffect::Backend(Command::Edit {
                    room,
                    item: message,
                    body,
                })],
                done,
            ),
            AppCall::Delete { room, message } => (
                vec![AppEffect::Backend(Command::Delete {
                    room,
                    item: message,
                })],
                done,
            ),
            AppCall::React {
                room,
                message,
                emoji,
            } => (
                vec![AppEffect::Backend(Command::React {
                    room,
                    item: message,
                    key: emoji,
                })],
                done,
            ),
            AppCall::JumpTo { room, message } => (
                self.dispatch(AppAction::JumpInRoom {
                    room,
                    item: message,
                }),
                done,
            ),
            AppCall::SetSetting { key, value } => {
                let answer = match self.state.plugins.as_mut() {
                    Some(plugins) => match plugins.set_own_setting(plugin, &key, &value) {
                        Ok(()) => done,
                        Err(message) => yapfast_plugin_api::call::HostReturn::Error { message },
                    },
                    None => yapfast_plugin_api::call::HostReturn::Unsupported,
                };
                (Vec::new(), answer)
            }
            AppCall::RetainStyle(style) => {
                let look = &mut self.state.retained_look;
                for (field, asked) in [
                    (&mut look.red_text, style.red_text),
                    (&mut look.red_ground, style.red_ground),
                    (&mut look.strike, style.strike),
                    (&mut look.faded, style.faded),
                    (&mut look.tag, style.tag),
                ] {
                    if let Some(asked) = asked {
                        *field = asked;
                    }
                }
                (Vec::new(), done)
            }
            AppCall::GetDraft { room, detail } => {
                let body = self.state.drafts.get(&room).cloned().unwrap_or_default();
                if !detail {
                    return (
                        Vec::new(),
                        yapfast_plugin_api::call::HostReturn::Ok(serde_json::Value::String(body)),
                    );
                }
                // The caret as the window said it, for this room and inside
                // this text; the end of the draft otherwise.
                let (caret, selection) = self
                    .plugin_caret
                    .as_ref()
                    .filter(|(shown, caret, _)| *shown == room && *caret <= body.len())
                    .map_or((body.len(), None), |(_, caret, selection)| {
                        (
                            *caret,
                            selection.filter(|(start, end)| start < end && *end <= body.len()),
                        )
                    });
                (
                    Vec::new(),
                    yapfast_plugin_api::call::HostReturn::Ok(serde_json::json!({
                        "body": body,
                        "caret": caret,
                        "selection": selection.map(|(start, end)| [start, end]),
                    })),
                )
            }
            AppCall::SetDraft { room, body } => (self.set_draft_in(room, body), done),
            AppCall::Insert { room, text } => {
                // Insert is what a widget does to the composer: after whatever
                // the person has already written, never in place of it.
                let mut draft = self.state.drafts.get(&room).cloned().unwrap_or_default();
                draft.push_str(&text);
                (self.set_draft_in(room, draft), done)
            }
            AppCall::Notify { text, click } => {
                if let Some(room) = click.as_ref().and_then(|click| click.room.as_deref())
                    && !self.listed_place(room)
                {
                    return (Vec::new(), not_listed(room));
                }
                // Titled with the plugin's name, as everything it shows is, and
                // tagged with it, so a click on it can only be this plugin's.
                let name = self
                    .state
                    .plugins
                    .as_ref()
                    .and_then(|plugins| plugins.name(plugin))
                    .unwrap_or_else(|| plugin.to_string());
                let room = click
                    .as_ref()
                    .and_then(|click| click.room.clone())
                    .or_else(|| self.state.selected.clone())
                    .unwrap_or_default();
                (
                    vec![AppEffect::Notify(crate::chat::Alert {
                        room,
                        title: format!("{name} plugin"),
                        body: text,
                        picture: None,
                        call: false,
                        mention: false,
                        plugin: Some(crate::chat::PluginAlert {
                            plugin: plugin.to_string(),
                            click: click.is_some(),
                            id: click
                                .as_ref()
                                .map(|click| click.id.clone())
                                .unwrap_or_default(),
                            room: click.and_then(|click| click.room),
                        }),
                    })],
                    done,
                )
            }
            AppCall::Copy { text } => (vec![AppEffect::CopyText(text)], done),
            // The link goes through the leaving dialog a masked message link
            // does: a plugin asks, and the person sees where it goes and
            // chooses. This used to open the page straight away.
            AppCall::Open { url } => {
                let name = self
                    .state
                    .plugins
                    .as_ref()
                    .and_then(|plugins| plugins.name(plugin))
                    .unwrap_or_else(|| plugin.to_string());
                let plugin = format!("{name} ({plugin})");
                (vec![AppEffect::ConfirmOpen { url, plugin }], done)
            }
            // The curated actions of PLAN §17.2. Each goes where the person's
            // own click goes, so the token, the rate limiter and this
            // account's permissions stay where they are.
            AppCall::SetStatus { status } => match self.ready_for_discord() {
                Ok(()) => (self.dispatch(AppAction::SetStatus(status)), done),
                Err(message) => (Vec::new(), HostReturn::Error { message }),
            },
            AppCall::SetCustomStatus {
                emoji,
                text,
                clear_after,
            } => answered(self.plugin_status(emoji, text, clear_after)),
            // The game Discord sees this account play; a plugin's own
            // activity is `activity.set`, which `plugins_ask` carries out.
            AppCall::SetActivity { enabled } => match self.ready_for_discord() {
                Ok(()) => (self.dispatch(AppAction::ShowGame(enabled)), done),
                Err(message) => (Vec::new(), HostReturn::Error { message }),
            },
            AppCall::MarkRead { room } => match self.listed_place(&room) {
                true => (self.dispatch(AppAction::MarkRead(room)), done),
                false => (Vec::new(), not_listed(&room)),
            },
            AppCall::SetMute { room, muting } => {
                if !self.listed_place(&room) {
                    return (Vec::new(), not_listed(&room));
                }
                // A conversation is muted through its own menu, and a channel
                // or a server through its notifications, as each does when a
                // person picks Mute.
                let mut effects = self.dispatch(AppAction::MuteDm {
                    room: room.clone(),
                    muting: Self::plugin_muting(muting),
                });
                if effects.is_empty() {
                    effects = self.dispatch(AppAction::ChangeNotifications {
                        place: room,
                        change: crate::chat::NotificationChange::Mute(Self::plugin_muting(muting)),
                    });
                }
                (effects, done)
            }
            AppCall::OpenRoom { room } => match self.listed_place(&room) {
                true => (self.dispatch(AppAction::Open(room)), done),
                false => (Vec::new(), not_listed(&room)),
            },
            AppCall::StartReply { room, message } => {
                answered(self.plugin_compose(&room, &message, false))
            }
            AppCall::StartEdit { room, message } => {
                answered(self.plugin_compose(&room, &message, true))
            }
            AppCall::Reactors { room, message } => {
                // The reactions a message has are held for the conversation on
                // screen, which is the one whose messages Yapfast keeps.
                if self.state.message(&message).is_none()
                    || self.state.room_of(&message).map(String::as_str) != Some(room.as_str())
                {
                    return (
                        Vec::new(),
                        HostReturn::Error {
                            message: format!(
                                "{message} isn't a message the open conversation holds"
                            ),
                        },
                    );
                }
                let reactions: Vec<ReactionCount> = self
                    .state
                    .message(&message)
                    .map(|held| {
                        held.reactions
                            .iter()
                            .map(|reaction| ReactionCount {
                                emoji: reaction.key.clone(),
                                count: reaction.count as u64,
                                me: reaction.mine,
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                match serde_json::to_value(reactions) {
                    Ok(value) => (Vec::new(), HostReturn::Ok(value)),
                    Err(error) => (
                        Vec::new(),
                        HostReturn::Error {
                            message: format!("could not write the reactions: {error}"),
                        },
                    ),
                }
            }
            AppCall::PauseInvites {
                server,
                invites,
                dms,
                hours,
            } => answered(self.plugin_pause_invites(&server, invites, dms, hours)),
            AppCall::AddEmoji {
                server,
                name,
                image,
            } => answered(self.plugin_add_emoji(&server, &name, &image)),
            AppCall::AddSticker {
                server,
                name,
                tags,
                description,
                image,
            } => answered(self.plugin_add_sticker(&server, &name, &tags, &description, &image)),
            AppCall::PlaySound { sound, volume } => {
                match crate::plugins::audio::play(sound, volume) {
                    // Whose sound it is shows while it plays, so a noise from
                    // nowhere always has a name.
                    Ok(()) => {
                        self.state.plugin_playing = Some(PlayingShown {
                            plugin: plugin.clone(),
                            name: self.plugin_name(plugin),
                            at: crate::clock::now_ms(),
                        });
                        (vec![AppEffect::Plugin(Effect::Slots)], done)
                    }
                    Err(error) => (
                        Vec::new(),
                        HostReturn::Error {
                            message: error.to_string(),
                        },
                    ),
                }
            }
            // Answered when the dialog closes, by `plugins_save_file`; a call
            // that reaches here wasn't routed there.
            AppCall::SaveFile { .. } => (
                Vec::new(),
                HostReturn::Error {
                    message: "a save is answered by the dialog it opens".to_owned(),
                },
            ),
            AppCall::Drafts => (Vec::new(), self.plugins_drafts()),
            // Answered once Discord or Spotify has, by `plugins_ask`; a call
            // that reaches here wasn't routed there.
            AppCall::RecordStop => match &self.plugin_recorder {
                Some(recording) if recording.plugin == *plugin => {
                    recording.stop_for_plugin();
                    (Vec::new(), done)
                }
                _ => (
                    Vec::new(),
                    HostReturn::Error {
                        message: "this plugin isn't recording".to_owned(),
                    },
                ),
            },
            AppCall::Record { .. } => (
                Vec::new(),
                HostReturn::Error {
                    message: "a recording is answered when it ends".to_owned(),
                },
            ),
            AppCall::SetPluginActivity { .. }
            | AppCall::SetProfile(_)
            | AppCall::Search(_)
            | AppCall::Sessions
            | AppCall::Affinities(_)
            | AppCall::SendVoice { .. }
            | AppCall::Messages { .. }
            | AppCall::Badges { .. }
            | AppCall::MediaFetch { .. }
            | AppCall::ServerDetails { .. }
            | AppCall::FavouriteGifs
            | AppCall::Servers
            | AppCall::Profile { .. }
            | AppCall::Relations { .. }
            | AppCall::Spotify(_) => (
                Vec::new(),
                HostReturn::Error {
                    message: "this call is answered by Discord, later".to_owned(),
                },
            ),
            AppCall::VoiceSettings => (Vec::new(), HostReturn::Ok(self.plugin_voice_settings())),
            AppCall::VoiceSet(set) => self.plugin_voice_set(set),
            AppCall::PlayRecording { path, volume } => {
                let played = match path {
                    Some(path) => crate::plugins::audio::play_recording(&path, volume),
                    None => {
                        crate::plugins::audio::stop_recording_playback();
                        Ok(())
                    }
                };
                match played {
                    Ok(()) => (Vec::new(), done),
                    Err(error) => (
                        Vec::new(),
                        HostReturn::Error {
                            message: format!("{error:#}"),
                        },
                    ),
                }
            }
            AppCall::Speak {
                text,
                voice,
                rate,
                volume,
            } => match crate::plugins::audio::speak(
                &text,
                &crate::plugins::audio::Voice {
                    name: voice,
                    rate,
                    volume,
                },
            ) {
                Ok(()) => (Vec::new(), done),
                Err(error) => (
                    Vec::new(),
                    HostReturn::Error {
                        message: error.to_string(),
                    },
                ),
            },
        }
    }

    /// `voice.settings`: the microphone and speakers chosen and listed, their
    /// volumes as the page's sliders show them, and the noise suppressor.
    fn plugin_voice_settings(&self) -> serde_json::Value {
        use yapfast_plugin_api::call::{VoiceDevice, VoiceSettings};
        let voice = &self.state.voice;
        let devices = |list: &[crate::voice::Device]| {
            list.iter()
                .map(|device| VoiceDevice {
                    id: device.id.clone(),
                    name: device.name.clone(),
                })
                .collect()
        };
        let slider = |gain: f32| crate::volume::to_slider(gain).round().clamp(0.0, 200.0) as u8;
        let settings = VoiceSettings {
            input_volume: slider(crate::volume::microphone()),
            output_volume: slider(crate::volume::speakers()),
            microphone: voice.microphone.clone(),
            speaker: voice.speaker.clone(),
            microphones: devices(&voice.microphones),
            speakers: devices(&voice.speakers),
            noise_suppression: suppressor_name(crate::noise::chosen()).to_owned(),
        };
        serde_json::to_value(settings).unwrap_or_default()
    }

    /// `voice.set`: a device picked as the person picks one, and the volumes
    /// and the suppressor handed to the window, which keeps them as its page
    /// does and plays them.
    fn plugin_voice_set(
        &mut self,
        set: yapfast_plugin_api::call::VoiceSet,
    ) -> (Vec<AppEffect>, HostReturn) {
        let listed =
            |list: &[crate::voice::Device], id: &str| list.iter().any(|device| device.id == id);
        if let Some(id) = &set.microphone
            && !listed(&self.state.voice.microphones, id)
        {
            return (
                Vec::new(),
                HostReturn::Error {
                    message: format!("no microphone has the id {id}"),
                },
            );
        }
        if let Some(id) = &set.speaker
            && !listed(&self.state.voice.speakers, id)
        {
            return (
                Vec::new(),
                HostReturn::Error {
                    message: format!("no speaker has the id {id}"),
                },
            );
        }
        let mut effects = Vec::new();
        if let Some(id) = set.microphone {
            effects.extend(self.dispatch(AppAction::Voice(crate::voice::Command::Microphone(id))));
        }
        if let Some(id) = set.speaker {
            effects.extend(self.dispatch(AppAction::Voice(crate::voice::Command::Speaker(id))));
        }
        let suppressor = set.noise_suppression.as_deref().map(|name| match name {
            "rnnoise" => crate::noise::Suppressor::RNNoise,
            "off" => crate::noise::Suppressor::Off,
            _ => crate::noise::Suppressor::DeepFilterNet,
        });
        if set.input_volume.is_some() || set.output_volume.is_some() || suppressor.is_some() {
            effects.push(AppEffect::Plugin(Effect::VoiceSettings(VoiceChange {
                input_volume: set.input_volume,
                output_volume: set.output_volume,
                suppressor,
            })));
        }
        (effects, HostReturn::Ok(serde_json::Value::Bool(true)))
    }

    /// Whether an id names something the sidebar lists: a conversation, a
    /// server, a folder or a category. A plugin can only act on a place the
    /// person can see.
    pub(crate) fn listed_place(&self, place: &str) -> bool {
        self.state.rooms.iter().any(|room| room.id == place)
            || self.state.servers.iter().any(|server| server.id == place)
            || self.state.is_folder(place)
            || self.state.is_category(place)
    }

    /// Whether this Yapfast can change a Discord account at all: it is linked,
    /// and the window isn't showing the preview that has no account behind it.
    pub(super) fn ready_for_discord(&self) -> Result<(), String> {
        if self.state.preview || self.state.discord_user.is_none() {
            return Err("this Yapfast has no Discord account signed in".to_owned());
        }
        Ok(())
    }

    /// The custom status a plugin asked for, sent without the person's status
    /// editor being open: that editor is a dialog nobody opened here.
    pub(super) fn plugin_status(
        &self,
        emoji: Option<String>,
        text: String,
        clear_after: Option<u64>,
    ) -> Result<Vec<AppEffect>, String> {
        self.ready_for_discord()?;
        let text = text.trim().to_owned();
        let emoji = emoji.filter(|emoji| !emoji.is_empty());
        let command = |status: Option<crate::chat::CustomStatus>| {
            AppEffect::Backend(Command::Discord(
                crate::chat::DiscordCommand::SetCustomStatus(status),
            ))
        };
        if text.is_empty() && emoji.is_none() {
            return Ok(vec![command(None)]);
        }
        let (emoji, emoji_id) = match emoji {
            Some(name) if name.contains(':') => {
                let found = self.status_emoji(&name).map_err(str::to_owned)?;
                (
                    Some(found.text.trim_matches(':').to_owned()),
                    Some(found.id),
                )
            }
            emoji => (emoji, None),
        };
        Ok(vec![command(Some(crate::chat::CustomStatus {
            text,
            emoji,
            emoji_id,
            expires_at_ms: clear_after
                .map(|seconds| crate::clock::now_ms().saturating_add(seconds.saturating_mul(1000))),
        }))])
    }

    /// Starts a reply to, or an edit of, a message the open conversation holds.
    pub(super) fn plugin_compose(
        &mut self,
        room: &str,
        message: &str,
        edit: bool,
    ) -> Result<Vec<AppEffect>, String> {
        if self.state.message(message).is_none() {
            return Err(format!(
                "{message} isn't a message the open conversation holds"
            ));
        }
        let Some(open) = self.state.room_of(message) else {
            return Err(format!(
                "{message} isn't a message the open conversation holds"
            ));
        };
        if open != room {
            return Err(format!("{message} is in {open}, not {room}"));
        }
        let message = message.to_owned();
        Ok(if edit {
            self.dispatch(AppAction::Edit(message))
        } else {
            self.dispatch(AppAction::Reply(message))
        })
    }

    /// `discord.set_mute`'s length of time, as the application mutes with: the
    /// wire carries it as a plugin's call does, and the room's own switch takes
    /// this application's.
    pub(super) fn plugin_muting(mute: yapfast_plugin_api::call::Mute) -> crate::chat::Muting {
        match mute {
            yapfast_plugin_api::call::Mute::Off => crate::chat::Muting::Off,
            yapfast_plugin_api::call::Mute::Always => crate::chat::Muting::Always,
            yapfast_plugin_api::call::Mute::Minutes(minutes) => {
                crate::chat::Muting::Minutes(minutes)
            }
        }
    }

    /// Pauses a server's invites, or resumes them, as Security Actions does.
    pub(super) fn plugin_pause_invites(
        &self,
        server: &str,
        invites: bool,
        dms: bool,
        hours: u8,
    ) -> Result<Vec<AppEffect>, String> {
        if !self.state.servers.iter().any(|listed| listed.id == server) {
            return Err(format!("{server} isn't a server this account is in"));
        }
        // The lengths Discord's own dialog offers; anything else takes the
        // longest, which is what pausing for good looks like there.
        let offered = crate::application::server_settings::PAUSE_HOURS;
        let hours = if offered.contains(&hours) {
            hours
        } else {
            offered[offered.len() - 1]
        };
        if !self.may(server, crate::chat::Permission::ManageServer) {
            return Err(format!(
                "you don't have Manage Server in {server}, so a plugin can't pause its invites"
            ));
        }
        Ok(vec![AppEffect::Backend(Command::Discord(
            crate::chat::DiscordCommand::Invites {
                server: server.to_owned(),
                action: crate::chat::InvitesAction::Pause {
                    invites,
                    dms,
                    hours,
                },
            },
        ))])
    }

    /// Adds a server's emoji, through the reader a person's own picture goes
    /// through, and then the upload a person's does.
    #[cfg(feature = "discord")]
    pub(super) fn plugin_add_emoji(
        &self,
        server: &str,
        name: &str,
        image: &[u8],
    ) -> Result<Vec<AppEffect>, String> {
        self.may_add_expressions(server)?;
        let name = crate::application::emoji_name(name.trim());
        if name.chars().count() < 2 {
            return Err("an emoji's name is at least two characters".to_owned());
        }
        let picture = with_brought_picture(image, crate::discord_text::expressions::read_emoji)?;
        Ok(vec![AppEffect::Backend(Command::Discord(
            crate::chat::DiscordCommand::Emoji {
                server: server.to_owned(),
                action: crate::chat::EmojiAction::Upload { name, picture },
            },
        ))])
    }

    #[cfg(not(feature = "discord"))]
    pub(super) fn plugin_add_emoji(
        &mut self,
        _server: &str,
        _name: &str,
        _image: &[u8],
    ) -> Result<Vec<AppEffect>, String> {
        Err("this build has no Discord, so it has no emoji to add".to_owned())
    }

    /// The same for a server's stickers.
    #[cfg(feature = "discord")]
    pub(super) fn plugin_add_sticker(
        &self,
        server: &str,
        name: &str,
        tags: &str,
        description: &str,
        image: &[u8],
    ) -> Result<Vec<AppEffect>, String> {
        self.may_add_expressions(server)?;
        let name = name.trim().to_owned();
        if name.is_empty() {
            return Err("a sticker needs a name".to_owned());
        }
        let file = with_brought_picture(image, crate::discord_text::expressions::read_sticker)?;
        Ok(vec![AppEffect::Backend(Command::Discord(
            crate::chat::DiscordCommand::Stickers {
                server: server.to_owned(),
                action: Box::new(crate::chat::StickerAction::Upload {
                    form: crate::chat::StickerForm {
                        name,
                        tags: tags.trim().to_owned(),
                        description: description.trim().to_owned(),
                    },
                    file,
                }),
            },
        ))])
    }

    #[cfg(not(feature = "discord"))]
    pub(super) fn plugin_add_sticker(
        &mut self,
        _server: &str,
        _name: &str,
        _tags: &str,
        _description: &str,
        _image: &[u8],
    ) -> Result<Vec<AppEffect>, String> {
        Err("this build has no Discord, so it has no stickers to add".to_owned())
    }

    /// Whether this account may add an emoji or a sticker to a server: what
    /// Emoji's and Stickers' own buttons ask for.
    pub(super) fn may_add_expressions(&self, server: &str) -> Result<(), String> {
        if !self.state.servers.iter().any(|listed| listed.id == server) {
            return Err(format!("{server} isn't a server this account is in"));
        }
        let rules = self.expression_rules(server);
        if !(rules.manage || rules.create) {
            return Err(format!(
                "you may not add expressions in {server}, so a plugin can't either"
            ));
        }
        Ok(())
    }

    /// Writes the files a plugin made where the upload reads them:
    /// `<data>/plugins-attach/<plugin>/<n>/<name>`, a folder per send so two
    /// files of one name don't meet. A send's folder is removed once it is ten
    /// minutes old, by the next send, which is long after its upload read it.
    pub(super) fn plugins_write_made(
        &self,
        plugin: &yapfast_plugin_api::PluginId,
        made: Vec<crate::plugins::state::MadeFile>,
    ) -> Result<Vec<std::path::PathBuf>, String> {
        let Some(root) = self
            .state
            .plugins
            .as_ref()
            .map(|plugins| plugins.attach_dir().join(plugin.as_str()))
        else {
            return Err("the plugin system isn't running".to_owned());
        };
        if let Ok(old) = std::fs::read_dir(&root) {
            let stale = std::time::Duration::from_secs(10 * 60);
            for entry in old.flatten() {
                let old_enough = entry
                    .metadata()
                    .and_then(|metadata| metadata.modified())
                    .ok()
                    .and_then(|modified| modified.elapsed().ok())
                    .is_some_and(|age| age > stale);
                if old_enough {
                    let _ = std::fs::remove_dir_all(entry.path());
                }
            }
        }
        let folder = root.join(crate::clock::now_ms().to_string());
        std::fs::create_dir_all(&folder)
            .map_err(|error| format!("couldn't keep the plugin's files: {error}"))?;
        made.into_iter()
            .map(|file| {
                let name = if file.spoiler {
                    format!("SPOILER_{}", file.name)
                } else {
                    file.name
                };
                let path = folder.join(&name);
                std::fs::write(&path, &file.bytes)
                    .map_err(|error| format!("couldn't keep {name}: {error}"))?;
                Ok(path)
            })
            .collect()
    }

    /// Sends a plugin's text in a room, leaving the person's own draft where it
    /// was.
    pub(super) fn plugins_send_a_message(
        &mut self,
        room: &str,
        body: String,
        files: Vec<std::path::PathBuf>,
    ) -> Result<Vec<AppEffect>, String> {
        // The plugin's files, read the way a file the person adds is, in
        // place of whatever the person had staged, which waits for them.
        let staged: Vec<crate::chat::Staged> = files
            .into_iter()
            .map(|path| crate::chat::Staged::read(path, PLUGIN_FILE_LIMIT))
            .collect::<Result<_, _>>()?;
        let saved_files = self.state.staged.remove(room);
        if !staged.is_empty() {
            self.state.staged.insert(room.to_owned(), staged);
        }
        let saved = self.state.drafts.get(room).cloned();
        self.state.drafts.insert(room.to_owned(), body);
        self.plugin_sending = true;
        let effects = self.send_composed_in(room.to_owned());
        self.plugin_sending = false;
        self.state.staged.remove(room);
        if let Some(files) = saved_files {
            self.state.staged.insert(room.to_owned(), files);
        }
        match saved {
            Some(draft) => {
                self.state.drafts.insert(room.to_owned(), draft);
            }
            None => {
                self.state.drafts.remove(room);
            }
        }
        Ok(effects)
    }
}

/// A message as the timeline names it, `d:<id>` for Discord: the form every
/// payload gives a message in, which a plugin that kept a bare Discord id
/// from an older build still reaches.
pub(crate) fn message_item(room: &str, message: &str) -> String {
    if crate::discord::parse_room(room).is_some()
        && !message.is_empty()
        && message.bytes().all(|byte| byte.is_ascii_digit())
    {
        format!("d:{message}")
    } else {
        message.to_owned()
    }
}

/// A call about a message, with its message in the timeline's form.
fn with_items(request: crate::plugins::state::AppCall) -> crate::plugins::state::AppCall {
    use crate::plugins::state::AppCall;
    match request {
        AppCall::Edit {
            room,
            message,
            body,
        } => AppCall::Edit {
            message: message_item(&room, &message),
            room,
            body,
        },
        AppCall::Delete { room, message } => AppCall::Delete {
            message: message_item(&room, &message),
            room,
        },
        AppCall::React {
            room,
            message,
            emoji,
        } => AppCall::React {
            message: message_item(&room, &message),
            room,
            emoji,
        },
        AppCall::JumpTo { room, message } => AppCall::JumpTo {
            message: message_item(&room, &message),
            room,
        },
        AppCall::StartReply { room, message } => AppCall::StartReply {
            message: message_item(&room, &message),
            room,
        },
        AppCall::StartEdit { room, message } => AppCall::StartEdit {
            message: message_item(&room, &message),
            room,
        },
        AppCall::Reactors { room, message } => AppCall::Reactors {
            message: message_item(&room, &message),
            room,
        },
        other => other,
    }
}

/// A noise suppressor by the name `voice.settings` and `voice.set` use.
fn suppressor_name(suppressor: crate::noise::Suppressor) -> &'static str {
    match suppressor {
        crate::noise::Suppressor::DeepFilterNet => "deepfilternet",
        crate::noise::Suppressor::RNNoise => "rnnoise",
        crate::noise::Suppressor::Off => "off",
    }
}
