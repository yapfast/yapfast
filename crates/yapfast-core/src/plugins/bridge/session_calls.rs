use super::*;

impl Session {
    /// Answers a host call that needs no waiting.
    /// Hands a call only the application can carry out to it, or answers why it
    /// can't.
    ///
    /// `None` means the call was passed on: the application answers it with
    /// [`Bridge::answer_call`]. `Some` is the refusal or the bad payload, which
    /// is answered here and now, because neither can change by waiting.
    pub(super) fn forward_call(
        &mut self,
        shared: &Shared,
        call: u64,
        plugin: &PluginId,
        request: &HostCall,
    ) -> Option<HostReturn> {
        let method = request.method.as_str();
        let parsed: Result<AppCall, serde_json::Error> = match method {
            call::method::MESSAGES_SEND => {
                request.parse::<SendMessage>().map(|sent| AppCall::Send {
                    room: sent.room,
                    body: sent.body,
                })
            }
            call::method::MESSAGES_EDIT => {
                request.parse::<EditMessage>().map(|edit| AppCall::Edit {
                    room: edit.room,
                    message: edit.message,
                    body: edit.body,
                })
            }
            call::method::MESSAGES_DELETE => {
                request
                    .parse::<MessageRef>()
                    .map(|deleted| AppCall::Delete {
                        room: deleted.room,
                        message: deleted.message,
                    })
            }
            call::method::MESSAGES_REACT => {
                request.parse::<ReactMessage>().map(|react| AppCall::React {
                    room: react.room,
                    message: react.message,
                    emoji: react.emoji,
                })
            }
            call::method::MESSAGES_JUMP_TO => {
                request.parse::<MessageRef>().map(|jump| AppCall::JumpTo {
                    room: jump.room,
                    message: jump.message,
                })
            }
            call::method::COMPOSER_GET_DRAFT => request
                .parse::<DraftRequest>()
                .map(|draft| AppCall::GetDraft { room: draft.room }),
            call::method::COMPOSER_SET_DRAFT => {
                request
                    .parse::<DraftRequest>()
                    .map(|draft| AppCall::SetDraft {
                        room: draft.room,
                        body: draft.body.unwrap_or_default(),
                    })
            }
            call::method::COMPOSER_INSERT => {
                request.parse::<InsertText>().map(|insert| AppCall::Insert {
                    room: insert.room,
                    text: insert.text,
                })
            }
            call::method::NOTIFY_SEND => request
                .parse::<NotifySend>()
                .map(|notify| AppCall::Notify { text: notify.text }),
            call::method::CLIPBOARD_WRITE => request
                .parse::<CopyText>()
                .map(|copy| AppCall::Copy { text: copy.text }),
            call::method::OPEN_URL => request
                .parse::<OpenUrl>()
                .map(|open| AppCall::Open { url: open.url }),
            call::method::DISCORD_SET_STATUS => request
                .parse::<SetStatus>()
                .map(|set| AppCall::SetStatus { status: set.status }),
            call::method::DISCORD_SET_CUSTOM_STATUS => {
                request
                    .parse::<SetCustomStatus>()
                    .map(|set| AppCall::SetCustomStatus {
                        emoji: set.emoji,
                        text: set.text,
                        clear_after: set.clear_after,
                    })
            }
            call::method::DISCORD_SET_ACTIVITY => {
                request
                    .parse::<SetActivity>()
                    .map(|set| AppCall::SetActivity {
                        enabled: set.enabled,
                    })
            }
            // Nothing to read: clearing an activity has no argument.
            call::method::DISCORD_CLEAR_ACTIVITY => Ok(AppCall::SetActivity { enabled: false }),
            call::method::DISCORD_MARK_READ => request
                .parse::<RoomGet>()
                .map(|room| AppCall::MarkRead { room: room.room }),
            call::method::DISCORD_SET_MUTE => {
                request.parse::<SetMute>().map(|set| AppCall::SetMute {
                    room: set.room,
                    muting: muting(set.muting),
                })
            }
            call::method::DISCORD_OPEN_ROOM => request
                .parse::<RoomGet>()
                .map(|room| AppCall::OpenRoom { room: room.room }),
            call::method::DISCORD_START_REPLY => {
                request
                    .parse::<MessageRef>()
                    .map(|asked| AppCall::StartReply {
                        room: asked.room,
                        message: asked.message,
                    })
            }
            call::method::DISCORD_START_EDIT => {
                request
                    .parse::<MessageRef>()
                    .map(|asked| AppCall::StartEdit {
                        room: asked.room,
                        message: asked.message,
                    })
            }
            call::method::DISCORD_REACTORS => {
                request
                    .parse::<MessageRef>()
                    .map(|asked| AppCall::Reactors {
                        room: asked.room,
                        message: asked.message,
                    })
            }
            call::method::DISCORD_PAUSE_INVITES => {
                request
                    .parse::<PauseInvites>()
                    .map(|asked| AppCall::PauseInvites {
                        server: asked.server,
                        invites: asked.invites,
                        dms: asked.dms,
                        hours: asked.hours,
                    })
            }
            // A picture a plugin brings is base64 inside the one frame the
            // call arrived in, so a picture over the cap is refused here
            // rather than carried, decoded and remembered.
            call::method::DISCORD_ADD_EMOJI => match request.parse::<AddEmoji>() {
                Err(error) => return Some(wrong_payload(method, error)),
                Ok(emoji) => match brought(&emoji.image_base64, MAX_ADDED_EMOJI_BYTES) {
                    Ok(image) => Ok(AppCall::AddEmoji {
                        server: emoji.server,
                        name: emoji.name,
                        image,
                    }),
                    Err(message) => return Some(HostReturn::Error { message }),
                },
            },
            call::method::DISCORD_ADD_STICKER => match request.parse::<AddSticker>() {
                Err(error) => return Some(wrong_payload(method, error)),
                Ok(sticker) => match brought(&sticker.image_base64, MAX_ADDED_STICKER_BYTES) {
                    Ok(image) => Ok(AppCall::AddSticker {
                        server: sticker.server,
                        name: sticker.name,
                        tags: sticker.tags,
                        description: sticker.description,
                        image,
                    }),
                    Err(message) => return Some(HostReturn::Error { message }),
                },
            },
            call::method::AUDIO_PLAY => match request.parse::<PlaySound>() {
                Err(error) => return Some(wrong_payload(method, error)),
                Ok(play) => match brought(&play.sound_base64, MAX_PLAYED_SOUND_BYTES) {
                    Ok(sound) => Ok(AppCall::PlaySound {
                        sound,
                        volume: play.volume,
                    }),
                    Err(message) => return Some(HostReturn::Error { message }),
                },
            },
            call::method::AUDIO_SPEAK => request
                .parse::<Speak>()
                .map(|speak| AppCall::Speak { text: speak.text }),
            _ => return None,
        };
        let app = match parsed {
            Ok(app) => app,
            Err(error) => return Some(wrong_payload(method, error)),
        };
        let registry = shared.registry();
        let Some(entry) = registry.entry(plugin) else {
            return Some(HostReturn::Error {
                message: format!("{plugin} isn't installed"),
            });
        };
        let permissions = entry.manifest.permissions.clone();
        if let Some(capability) = app_capability(&app)
            && let Err(denial) = crate::plugins::permissions::check(&permissions, capability)
        {
            return Some(HostReturn::Denied {
                reason: denial.reason().to_owned(),
            });
        }
        // §9.1's guard rails for sending: a person's action behind it, or the
        // capability that says the plugin sends on its own, and five sends in
        // any ten seconds either way.
        if matches!(app, AppCall::Send { .. }) {
            if !shared.has_gesture() && !permissions.allows(Capability::MessagesSendBackground) {
                return Some(HostReturn::Denied {
                    reason: format!(
                        "{plugin} wants to send without you doing anything: that needs                          `messages.send.background`, and automated sending can get a Discord                          account banned"
                    ),
                });
            }
            if !shared.may_send(plugin) {
                return Some(HostReturn::Denied {
                    reason: format!(
                        "{plugin} is sending more than {SEND_LIMIT} messages in {window} seconds",
                        window = SEND_WINDOW.as_secs()
                    ),
                });
            }
        }
        // §9.1's scheme list, checked where the manifest is in hand.
        if let Err(reason) = opened_scheme_allowed(&permissions, &app) {
            return Some(HostReturn::Denied {
                reason: format!("{plugin} wants to open {reason}"),
            });
        }
        // A navigation or compose action needs no capability, and does need a
        // person's own hand behind it: a plugin must not move the window or
        // start a reply while nobody is there.
        if needs_gesture(&app) && !shared.has_gesture() {
            return Some(HostReturn::Denied {
                reason: format!(
                    "{plugin} wants to move the window or write the composer \
                     without you doing anything: that needs a click of yours first"
                ),
            });
        }
        shared.announce(Notice::Call {
            plugin: plugin.clone(),
            call,
            request: app,
        });
        None
    }

    /// Answers a guest's host call.
    ///
    /// `call` is the guest's own number for it, which the runtime handed out
    /// when `yf_call` answered `pending`: the answer goes back under it, and the
    /// runtime delivers it to the guest in its next event batch.
    ///
    /// `http.fetch` goes beside the conversation instead of inside it: a plugin
    /// waiting on the network must not stop events reaching the others.
    pub(super) fn on_call(
        &mut self,
        shared: &Arc<Shared>,
        call: u64,
        plugin: PluginId,
        request: HostCall,
    ) {
        if request.method == call::method::PLUGIN_CALL {
            self.call_plugin(shared, call, plugin, request);
            return;
        }
        if request.method == call::method::HTTP_FETCH {
            self.fetch(shared, call, plugin, request);
            return;
        }
        // The calls only the application can answer go to it and are answered
        // later through `PluginState::answer_call`, so the interface thread is
        // never waited on and the guest is never left thinking nothing happened.
        if let Some(answer) = self.forward_call(shared, call, &plugin, &request) {
            self.reply(&plugin, call, answer);
        }
        if is_app_call(request.method.as_str()) {
            return;
        }
        // Being told of a change is the one call whose answer only exists
        // later: the guest is answered `pending` now, and every change after it
        // travels under this id.
        if request.method == call::method::SETTINGS_ON_CHANGE {
            self.watch_settings(&plugin, call);
            return;
        }
        let answer = self.answer(shared, &plugin, request);
        self.reply(&plugin, call, answer);
    }
}
