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
            // `messages.attach` is `messages.send` with files: the same
            // payload, the same rails below, and its own capability.
            call::method::MESSAGES_SEND | call::method::MESSAGES_ATTACH => {
                match request.parse::<SendMessage>() {
                    Err(error) => return Some(wrong_payload(method, error)),
                    Ok(sent) => {
                        if sent.files.len() + sent.attachments.len() > SendMessage::MAX_FILES {
                            return Some(HostReturn::Error {
                                message: format!(
                                    "a message carries at most {} files, as Discord takes them",
                                    SendMessage::MAX_FILES
                                ),
                            });
                        }
                        let files = if sent.files.is_empty() {
                            Vec::new()
                        } else {
                            match self.attached(shared, plugin, &sent.files) {
                                Ok(files) => files,
                                Err(answer) => return Some(answer),
                            }
                        };
                        let made = if sent.attachments.is_empty() {
                            Vec::new()
                        } else {
                            match made_files(shared, plugin, &sent.attachments) {
                                Ok(made) => made,
                                Err(answer) => return Some(answer),
                            }
                        };
                        Ok(AppCall::Send {
                            room: sent.room,
                            body: sent.body,
                            files,
                            made,
                            agent: false,
                        })
                    }
                }
            }
            call::method::ACTIVITY_SET => {
                match request.parse::<yapfast_plugin_api::account::ActivitySet>() {
                    Err(error) => return Some(wrong_payload(method, error)),
                    Ok(activity) => match activity.problems() {
                        Ok(()) => Ok(AppCall::SetPluginActivity {
                            activity: Some(Box::new(activity)),
                        }),
                        Err(message) => return Some(HostReturn::Error { message }),
                    },
                }
            }
            call::method::ACTIVITY_CLEAR => Ok(AppCall::SetPluginActivity { activity: None }),
            call::method::PROFILE_SET => {
                match request.parse::<yapfast_plugin_api::account::ProfileSet>() {
                    Err(error) => return Some(wrong_payload(method, error)),
                    Ok(profile) => match profile.problems() {
                        Ok(()) => Ok(AppCall::SetProfile(profile)),
                        Err(message) => return Some(HostReturn::Error { message }),
                    },
                }
            }
            call::method::MESSAGES_SEARCH => request
                .parse::<yapfast_plugin_api::account::SearchRequest>()
                .map(AppCall::Search),
            call::method::DRAFTS_LIST => Ok(AppCall::Drafts),
            call::method::ACCOUNT_SESSIONS => Ok(AppCall::Sessions),
            call::method::MICROPHONE_START => {
                // No payload is an Opus recording until it is stopped.
                if request.payload.is_null() {
                    Ok(AppCall::Record {
                        format: Default::default(),
                        max_seconds: None,
                    })
                } else {
                    request
                        .parse::<yapfast_plugin_api::call::MicrophoneStart>()
                        .map(|start| AppCall::Record {
                            format: start.format,
                            max_seconds: start.max_seconds,
                        })
                }
            }
            call::method::MICROPHONE_STOP => Ok(AppCall::RecordStop),
            call::method::VOICE_SETTINGS => Ok(AppCall::VoiceSettings),
            call::method::VOICE_SET => match request.parse::<call::VoiceSet>() {
                Ok(set) => match set.check() {
                    Ok(()) => Ok(AppCall::VoiceSet(set)),
                    Err(message) => return Some(HostReturn::Error { message }),
                },
                Err(error) => Err(error),
            },
            call::method::MICROPHONE_PLAY => match request.parse::<call::MicrophonePlay>() {
                Err(error) => return Some(wrong_payload(method, error)),
                Ok(play) if play.stop || play.recording.is_none() => Ok(AppCall::PlayRecording {
                    path: None,
                    volume: None,
                }),
                Ok(play) => match shared.recording(plugin, play.recording.unwrap_or_default()) {
                    Ok((path, _, _)) => Ok(AppCall::PlayRecording {
                        path: Some(path),
                        volume: play.volume.map(|volume| volume.min(100)),
                    }),
                    Err(message) => return Some(HostReturn::Error { message }),
                },
            },
            call::method::SERVERS_DETAILS => {
                request
                    .parse::<call::ServerDetails>()
                    .map(|asked| AppCall::ServerDetails {
                        server: asked.server,
                    })
            }
            call::method::GIFS_FAVOURITES => Ok(AppCall::FavouriteGifs),
            call::method::SERVERS_LIST => Ok(AppCall::Servers),
            call::method::PEOPLE_PROFILE => {
                request
                    .parse::<call::PeopleBadges>()
                    .map(|asked| AppCall::Profile {
                        person: asked.person,
                        server: asked.server,
                    })
            }
            call::method::PEOPLE_FRIENDS => Ok(AppCall::Relations { blocked: false }),
            call::method::PEOPLE_BLOCKED => Ok(AppCall::Relations { blocked: true }),
            call::method::SETTINGS_SET => {
                request
                    .parse::<call::SettingSet>()
                    .map(|set| AppCall::SetSetting {
                        key: set.key,
                        value: set.value,
                    })
            }
            call::method::MESSAGES_RETAIN_STYLE => request
                .parse::<call::RetainStyle>()
                .map(AppCall::RetainStyle),
            // A picture the store doesn't have yet, from Discord's CDN: the
            // application fetches it, through its own session, and answers
            // later. Anything the store holds, or any other address, is
            // answered here as before.
            call::method::MEDIA_GET => Ok(AppCall::MediaFetch {
                url: media_to_fetch(shared, request)?,
            }),
            call::method::PEOPLE_BADGES => {
                request
                    .parse::<call::PeopleBadges>()
                    .map(|asked| AppCall::Badges {
                        person: asked.person,
                        server: asked.server,
                    })
            }
            call::method::MESSAGES_VOICE => {
                match request.parse::<yapfast_plugin_api::call::VoiceSend>() {
                    Err(error) => return Some(wrong_payload(method, error)),
                    Ok(voice) => match shared.recording(plugin, voice.recording) {
                        Ok((path, duration_ms, waveform)) => Ok(AppCall::SendVoice {
                            room: voice.room,
                            path,
                            duration_ms,
                            waveform,
                        }),
                        Err(message) => return Some(HostReturn::Error { message }),
                    },
                }
            }
            call::method::RELATIONSHIPS_AFFINITIES => {
                // No payload is the plain list of people.
                if request.payload.is_null() {
                    Ok(AppCall::Affinities(Default::default()))
                } else {
                    request
                        .parse::<yapfast_plugin_api::account::AffinitiesRequest>()
                        .map(AppCall::Affinities)
                }
            }
            spotify if call::method::SPOTIFY.contains(&spotify) => {
                match yapfast_plugin_api::account::SpotifyAction::of(spotify, &request.payload) {
                    Ok(action) => Ok(AppCall::Spotify(action)),
                    Err(message) => return Some(HostReturn::Error { message }),
                }
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
            call::method::COMPOSER_GET_DRAFT => {
                request
                    .parse::<DraftRequest>()
                    .map(|draft| AppCall::GetDraft {
                        room: draft.room,
                        detail: draft.detail,
                    })
            }
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
            call::method::NOTIFY_SEND => {
                request.parse::<NotifySend>().map(|notify| AppCall::Notify {
                    text: notify.text,
                    click: crate::ui::Click::asked(notify.room, notify.id).map(Box::new),
                })
            }
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
                    muting: set.muting,
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
                Ok(emoji) => match carried(
                    shared,
                    plugin,
                    &emoji.image_base64,
                    emoji.asset.as_deref(),
                    MAX_ADDED_EMOJI_BYTES,
                    Carried::Picture,
                ) {
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
                Ok(sticker) => match carried(
                    shared,
                    plugin,
                    &sticker.image_base64,
                    sticker.asset.as_deref(),
                    MAX_ADDED_STICKER_BYTES,
                    Carried::Picture,
                ) {
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
                Ok(play) => match carried(
                    shared,
                    plugin,
                    &play.sound_base64,
                    play.asset.as_deref(),
                    MAX_PLAYED_SOUND_BYTES,
                    Carried::Sound,
                ) {
                    Ok(sound) => Ok(AppCall::PlaySound {
                        sound,
                        volume: play.volume,
                    }),
                    Err(message) => return Some(HostReturn::Error { message }),
                },
            },
            call::method::AUDIO_SPEAK => request.parse::<Speak>().map(|speak| AppCall::Speak {
                text: speak.text,
                voice: speak.voice,
                rate: speak.rate,
                volume: speak.volume,
            }),
            // The bytes are the plugin's and the folder is the person's: the
            // name is checked to be a name and no path, the bytes to fit one
            // frame, and the application asks where they go.
            call::method::FILES_SAVE => match request.parse::<SaveFile>() {
                Err(error) => return Some(wrong_payload(method, error)),
                Ok(save) => {
                    let name = match saved_name(&save.name) {
                        Ok(name) => name,
                        Err(message) => return Some(HostReturn::Error { message }),
                    };
                    let bytes = match (save.text, save.bytes_base64) {
                        (Some(text), None) => Ok(text.into_bytes()),
                        (None, Some(encoded)) => brought(&encoded, limits::MAX_SAVED_FILE_BYTES),
                        _ => Err(
                            "`files.save` takes `text` or `bytes_base64`, and one of them"
                                .to_owned(),
                        ),
                    };
                    match bytes {
                        Ok(bytes) if bytes.len() > limits::MAX_SAVED_FILE_BYTES => {
                            return Some(HostReturn::Error {
                                message: format!(
                                    "`files.save` writes at most {} bytes, and this file is {}",
                                    limits::MAX_SAVED_FILE_BYTES,
                                    bytes.len()
                                ),
                            });
                        }
                        Ok(bytes) => Ok(AppCall::SaveFile { name, bytes }),
                        Err(message) => return Some(HostReturn::Error { message }),
                    }
                }
            },
            _ => return None,
        };
        let mut app = match parsed {
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
            && let Err(denial) = crate::permissions::check(&permissions, capability)
        {
            return Some(HostReturn::Denied {
                reason: denial.reason().to_owned(),
            });
        }
        // A plugin's files go with a send, under the send's own rails below.
        if let AppCall::Send { made, .. } = &app
            && !made.is_empty()
            && let Err(denial) = crate::permissions::check(&permissions, Capability::MessagesAttach)
        {
            return Some(HostReturn::Denied {
                reason: denial.reason().to_owned(),
            });
        }
        // Changing what everyone sees on the account is asked about the first
        // time, in the dialog the powerful tier's first use opens, though it
        // doesn't wait for that tier's switch.
        if matches!(app, AppCall::SetProfile(_))
            && let Err(answer) =
                self.powerful_gate(shared, plugin, &permissions, Capability::ProfileWrite)
        {
            return Some(answer);
        }
        // The microphone is asked about the first time the same way, and the
        // red bar shows for as long as each recording runs.
        if matches!(app, AppCall::Record { .. })
            && let Err(answer) =
                self.powerful_gate(shared, plugin, &permissions, Capability::Microphone)
        {
            return Some(answer);
        }
        // §9.1's guard rails for sending: a person's action behind it, or the
        // capability that says the plugin sends on its own, and five sends in
        // any ten seconds either way. Editing, deleting and reacting act as
        // the person under the same `messages.send`, so they share the rails:
        // without them a plugin could delete every message a moderator can
        // reach, or react in a loop, with nobody at the keyboard.
        if matches!(
            app,
            AppCall::Send { .. }
                | AppCall::SendVoice { .. }
                | AppCall::Edit { .. }
                | AppCall::Delete { .. }
                | AppCall::React { .. }
        ) {
            let mut agent = false;
            // Editing, deleting or reacting to the person's own message just
            // after they sent one through this plugin's pre-send hook stands
            // behind that send, as a hook that tidies up after itself does.
            let own_after_send = shared.sent_through(plugin)
                && match &app {
                    AppCall::Edit { room, message, .. }
                    | AppCall::Delete { room, message }
                    | AppCall::React { room, message, .. } => shared
                        .world
                        .message(room, message)
                        .is_some_and(|view| view.author.own),
                    _ => false,
                };
            if !own_after_send
                && !shared.has_gesture(plugin)
                && !permissions.allows(Capability::MessagesSendBackground)
            {
                // An agent's send: no keystroke behind it, and the person said
                // in a red dialog that this plugin may, at the rate its
                // manifest names.
                if !permissions.allows(Capability::MessagesSendAgent) {
                    return Some(HostReturn::Denied {
                        reason: format!(
                            "{plugin} wants to send without you doing anything: that needs \
                             `messages.send.background` or `messages.send.agent`, and automated \
                             sending can get a Discord account banned"
                        ),
                    });
                }
                if let Err(answer) =
                    self.powerful_gate(shared, plugin, &permissions, Capability::MessagesSendAgent)
                {
                    return Some(answer);
                }
                if !shared.may_send_as_agent(plugin, permissions.agent_per_minute()) {
                    return Some(HostReturn::Denied {
                        reason: format!(
                            "{plugin} is sending as an agent more than {} times a minute, the \
                             rate its manifest names",
                            permissions.agent_per_minute()
                        ),
                    });
                }
                agent = true;
            }
            if !shared.may_send(plugin) {
                return Some(HostReturn::Denied {
                    reason: format!(
                        "{plugin} is sending more than {SEND_LIMIT} messages in {window} seconds",
                        window = SEND_WINDOW.as_secs()
                    ),
                });
            }
            if agent {
                if let AppCall::Send { agent: marked, .. } = &mut app {
                    *marked = true;
                }
                shared.announce(Notice::AgentSent {
                    plugin: plugin.clone(),
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
        if needs_gesture(&app) && !shared.has_gesture(plugin) {
            let what = if matches!(app, AppCall::SaveFile { .. }) {
                "open a save dialog"
            } else {
                "move the window or write the composer"
            };
            return Some(HostReturn::Denied {
                reason: format!(
                    "{plugin} wants to {what} without you doing anything: that needs a click \
                     of yours first"
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
        if crate::devtools::on() {
            let sent = serde_json::to_vec(&request.payload).map_or(0, |bytes| bytes.len());
            crate::devtools::called(plugin.as_str(), call, &request.method, sent);
        }
        if request.method == call::method::PLUGIN_CALL {
            self.call_plugin(shared, call, plugin, request);
            return;
        }
        // A powerful ability's first use waits for the person's answer and
        // then goes on, rather than being refused and left to be tried again.
        if let Some(capability) = self.powerful_unasked(shared, &plugin, &request) {
            self.wait_for_powerful(shared, plugin, capability, call, request);
            return;
        }
        if request.method == call::method::HTTP_FETCH {
            self.fetch(shared, call, plugin, request);
            return;
        }
        // A message or history of a conversation the window doesn't hold is
        // asked of Discord, later; one it holds is answered below, now.
        if let Some(app) = elsewhere(shared, &request) {
            let permissions = shared
                .registry()
                .entry(&plugin)
                .map(|entry| entry.manifest.permissions.clone())
                .unwrap_or_default();
            if let Err(denial) = crate::permissions::check(&permissions, Capability::MessagesRead) {
                self.reply(
                    &plugin,
                    call,
                    HostReturn::Denied {
                        reason: denial.reason().to_owned(),
                    },
                );
                return;
            }
            shared.announce(Notice::Call {
                plugin,
                call,
                request: app,
            });
            return;
        }
        if request.method == call::method::OAUTH_LOGIN
            || request.method == call::method::OAUTH_TOKEN
        {
            self.oauth(shared, call, plugin, request);
            return;
        }
        // The powerful tier's two calls that answer later: a picture decoded
        // off this task, and an endpoint whose requests are answers to come.
        if request.method == call::method::FS_THUMBNAIL {
            self.thumbnail(shared, call, plugin, request);
            return;
        }
        if request.method == call::method::NET_LISTEN {
            self.listen(shared, call, plugin);
            return;
        }
        if request.method == call::method::NET_CONNECT {
            self.connect(shared, call, plugin, request);
            return;
        }
        // A `media.get` sent on to the application (see `forward_call`).
        let fetched =
            request.method == call::method::MEDIA_GET && media_to_fetch(shared, &request).is_some();
        // The calls only the application can answer go to it and are answered
        // later through `PluginState::answer_call`, so the interface thread is
        // never waited on and the guest is never left thinking nothing happened.
        if let Some(answer) = self.forward_call(shared, call, &plugin, &request) {
            self.reply(&plugin, call, answer);
        }
        if is_app_call(request.method.as_str()) || fetched {
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

/// What kind of asset a call that carries bytes may name instead.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Carried {
    Picture,
    Sound,
}

/// The bytes a call brought base64, or the ones of the asset it named instead:
/// one of the two, never both, and an asset of the kind the call uses.
pub(super) fn carried(
    shared: &Shared,
    plugin: &PluginId,
    encoded: &str,
    asset: Option<&str>,
    cap: usize,
    kind: Carried,
) -> Result<Vec<u8>, String> {
    let Some(name) = asset else {
        return brought(encoded, cap);
    };
    if !encoded.is_empty() {
        return Err("this call names an asset and carries bytes too: one or the other".to_owned());
    }
    let (found, bytes) = shared.asset(plugin, name)?;
    let fits = match kind {
        Carried::Picture => found.is_picture(),
        Carried::Sound => found.is_sound(),
    };
    if !fits {
        return Err(format!(
            "the asset `{name}` is {found}, which this call doesn't take"
        ));
    }
    if bytes.len() > cap {
        return Err(format!(
            "the asset `{name}` is {} bytes, and {cap} is the most this call takes",
            bytes.len()
        ));
    }
    Ok(bytes)
}

/// The files a plugin made, that its package carries or that it fetched, for
/// a send: each a file name and no path, and one of bytes, an asset or a kept
/// body. Bytes come inside the call's one frame, so the frame is their cap; an
/// asset is read from the store, checked again as it is read; a body is read
/// from where `http.fetch` kept it, the plugin's own and at most a plugin's
/// largest file.
fn made_files(
    shared: &Shared,
    plugin: &PluginId,
    attachments: &[call::Attachment],
) -> Result<Vec<crate::bridge::messages::MadeFile>, HostReturn> {
    attachments
        .iter()
        .map(|attachment| {
            let name = saved_name(&attachment.name).map_err(|message| HostReturn::Error {
                message: message.replace("`files.save` suggests", "an attachment has"),
            })?;
            let bytes = match (&attachment.bytes_base64, &attachment.asset, attachment.body) {
                (Some(encoded), None, None) => brought(encoded, frame::MAX_FRAME),
                (None, Some(asset), None) => shared.asset(plugin, asset).map(|(_, bytes)| bytes),
                (None, None, Some(body)) => {
                    shared.body_bytes(plugin, body, SendMessage::MAX_FILE_BYTES)
                }
                _ => Err(format!(
                    "the attachment `{name}` takes `bytes_base64`, `asset` or `body`, and one of them"
                )),
            }
            .map_err(|message| HostReturn::Error { message })?;
            Ok(crate::bridge::messages::MadeFile {
                name,
                bytes,
                spoiler: attachment.spoiler,
            })
        })
        .collect()
}

impl Session {
    /// The files a send carries, from the plugin's folder: `messages.attach`,
    /// at most ten, each resolved inside the folder and a file.
    fn attached(
        &self,
        shared: &Shared,
        plugin: &PluginId,
        named: &[String],
    ) -> Result<Vec<std::path::PathBuf>, HostReturn> {
        let permissions = shared
            .registry()
            .entry(plugin)
            .map(|entry| entry.manifest.permissions.clone())
            .unwrap_or_default();
        crate::permissions::check(&permissions, Capability::MessagesAttach).map_err(|denial| {
            HostReturn::Denied {
                reason: denial.reason().to_owned(),
            }
        })?;
        if named.len() > SendMessage::MAX_FILES {
            return Err(HostReturn::Error {
                message: format!("a message carries at most {} files", SendMessage::MAX_FILES),
            });
        }
        let root = self.files_root(shared, plugin)?;
        named
            .iter()
            .map(|path| {
                root.file(path)
                    .map_err(|message| HostReturn::Error { message })
            })
            .collect()
    }
}

/// `messages.get` or `messages.history` about a Discord conversation that
/// isn't the one the window holds, as the call the application asks Discord.
fn elsewhere(shared: &Shared, request: &HostCall) -> Option<AppCall> {
    let held = |room: &str| shared.world.history(room, None, Some(1)).is_some();
    let discord = |room: &str| room.starts_with("discord:");
    match request.method.as_str() {
        call::method::MESSAGES_GET => {
            let asked: MessageGet = request.parse().ok()?;
            let wanted = discord(&asked.room) && !held(&asked.room);
            wanted.then_some(AppCall::Messages {
                room: asked.room,
                message: Some(asked.message),
                before: None,
                limit: None,
            })
        }
        call::method::MESSAGES_HISTORY => {
            let asked: call::MessageHistory = request.parse().ok()?;
            let wanted = discord(&asked.room) && !held(&asked.room);
            wanted.then_some(AppCall::Messages {
                room: asked.room,
                message: None,
                before: asked.before,
                limit: asked.limit,
            })
        }
        _ => None,
    }
}

/// The address `media.get` asks for, when the store has nothing kept for it
/// and it is on Discord's CDN, which the application can fetch: an attachment
/// no conversation has shown yet, a text file, a sticker.
fn media_to_fetch(shared: &Shared, request: &HostCall) -> Option<String> {
    let asked: MediaGet = request.parse().ok()?;
    let host = asked
        .url
        .strip_prefix("https://")?
        .split(['/', '?', '#'])
        .next()?;
    if !matches!(host, "cdn.discordapp.com" | "media.discordapp.net") {
        return None;
    }
    let pictures = shared
        .pictures
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()?;
    pictures.bytes(&asked.url).is_none().then_some(asked.url)
}
