//! What a plugin asks the application to do, and how the answer is shaped.
//!
//! A guest's host call is one JSON request; the ones only the application can
//! carry out reach it as [`super::News::Notice`]`(Notice::Call)`, and this is
//! where each is done. Everything here is gated on the call's capability and
//! grant before it touches anything, and nothing here waits on a plugin: the
//! answer goes back through the bridge under the guest's own call id.

use super::*;

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
        match request {
            AppCall::Send { room, body } => {
                if !self.state.rooms.iter().any(|listed| listed.id == room) {
                    return (
                        Vec::new(),
                        yapfast_plugin_api::call::HostReturn::Error {
                            message: format!("{room} isn't a conversation this account can see"),
                        },
                    );
                }
                let effects = self.plugins_send_a_message(&room, body);
                (effects, done)
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
            AppCall::GetDraft { room } => (
                Vec::new(),
                yapfast_plugin_api::call::HostReturn::Ok(serde_json::Value::String(
                    self.state.drafts.get(&room).cloned().unwrap_or_default(),
                )),
            ),
            AppCall::SetDraft { room, body } => (self.set_draft_in(room, body), done),
            AppCall::Insert { room, text } => {
                // Insert is what a widget does to the composer: after whatever
                // the person has already written, never in place of it.
                let mut draft = self.state.drafts.get(&room).cloned().unwrap_or_default();
                draft.push_str(&text);
                (self.set_draft_in(room, draft), done)
            }
            AppCall::Notify { text } => (
                vec![AppEffect::Notify(crate::chat::Alert {
                    room: self.state.selected.clone().unwrap_or_default(),
                    title: plugin.to_string(),
                    body: text,
                    picture: None,
                    call: false,
                    mention: false,
                })],
                done,
            ),
            AppCall::Copy { text } => (vec![AppEffect::CopyText(text)], done),
            // The link goes through the same confirmation a message's does: a
            // plugin asks, and the person still sees where it goes.
            AppCall::Open { url } => (vec![AppEffect::OpenPage(url)], done),
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
            // The only activity Yapfast has is the game Discord sees this
            // account play, so that is the one a plugin can show or clear.
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
                    Ok(()) => (Vec::new(), done),
                    Err(error) => (
                        Vec::new(),
                        HostReturn::Error {
                            message: error.to_string(),
                        },
                    ),
                }
            }
            AppCall::Speak { text } => match crate::plugins::audio::speak(&text) {
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

    /// Whether an id names something the sidebar lists: a conversation, a
    /// server, a folder or a category. A plugin can only act on a place the
    /// person can see.
    pub(super) fn listed_place(&self, place: &str) -> bool {
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
        &mut self,
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
        &mut self,
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
        &mut self,
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
        &mut self,
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

    /// Sends a plugin's text in a room, leaving the person's own draft where it
    /// was.
    pub(super) fn plugins_send_a_message(&mut self, room: &str, body: String) -> Vec<AppEffect> {
        let saved = self.state.drafts.get(room).cloned();
        self.state.drafts.insert(room.to_owned(), body);
        self.plugin_sending = true;
        let effects = self.send_composed_in(room.to_owned());
        self.plugin_sending = false;
        match saved {
            Some(draft) => {
                self.state.drafts.insert(room.to_owned(), draft);
            }
            None => {
                self.state.drafts.remove(room);
            }
        }
        effects
    }
}
