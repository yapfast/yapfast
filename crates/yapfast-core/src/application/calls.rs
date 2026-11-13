//! Joining and switching calls, and answering them, and the sounds they make.

use super::*;
use crate::sounds::Sound;

/// The sound a change in the call makes: this account joining or leaving it,
/// or, while it's there, someone else connecting or going, or starting or
/// stopping a stream. People arriving together make one sound. Reconnecting makes none,
/// and nor do those a DM call is still ringing, until they answer.
pub(super) fn call_sounds(before: &voice::State, after: &voice::State) -> Vec<Effect> {
    let here = |state: &voice::State| matches!(state.phase, Phase::Connected | Phase::Reconnecting);
    let others = |state: &voice::State| -> HashSet<String> {
        state
            .participants
            .iter()
            .filter(|person| !person.local && person.connected)
            .map(|person| person.id.clone())
            .collect()
    };
    let sound =
        if after.phase == Phase::Connected && (!here(before) || before.room != after.room) {
            Some(Sound::Join)
        } else if here(before) && !here(after) {
            Some(Sound::Leave)
        } else if before.phase == Phase::Connected && after.phase == Phase::Connected {
            let (before, after) = (others(before), others(after));
            if !after.is_subset(&before) {
                Some(Sound::Join)
            } else if !before.is_subset(&after) {
                Some(Sound::Leave)
            } else {
                None
            }
        } else {
            None
        }
        .or_else(|| stream_sound(before, after))
        .or_else(|| viewer_sound(before, after));
    sound.map(Effect::Sound).into_iter().collect()
}

/// Someone else already in the call going live, or ending their stream.
/// Those streaming as this account arrives, or as they arrive, make only the
/// sound of joining, and those leaving mid-stream the sound of leaving, which
/// come first.
fn stream_sound(before: &voice::State, after: &voice::State) -> Option<Sound> {
    if before.phase != Phase::Connected
        || after.phase != Phase::Connected
        || before.room != after.room
    {
        return None;
    }
    fn streaming(state: &voice::State) -> HashSet<&str> {
        state
            .participants
            .iter()
            .filter(|person| !person.local && person.connected && person.streaming)
            .map(|person| person.id.as_str())
            .collect()
    }
    let (before, after) = (streaming(before), streaming(after));
    if !after.is_subset(&before) {
        Some(Sound::StreamStart)
    } else if !before.is_subset(&after) {
        Some(Sound::StreamStop)
    } else {
        None
    }
}

/// Someone else starting or stopping watching this account's stream, or the
/// one it watches, while that stream goes on.
fn viewer_sound(before: &voice::State, after: &voice::State) -> Option<Sound> {
    let me = after
        .participants
        .iter()
        .find(|person| person.local)
        .map(|person| person.id.as_str());
    // Each viewer by the stream they watch: this account's, as none, or the
    // streamer's id.
    let viewers = |state: &voice::State| -> HashSet<(Option<String>, String)> {
        let mine = state
            .sharing
            .iter()
            .flat_map(|sharing| sharing.viewers.iter().map(|viewer| (None, viewer)));
        let watched = state.watching.iter().flat_map(|watching| {
            watching
                .viewers
                .iter()
                .map(|viewer| (Some(watching.user.clone()), viewer))
        });
        mine.chain(watched)
            .filter(|(_, viewer)| Some(viewer.as_str()) != me)
            .map(|(stream, viewer)| (stream, viewer.clone()))
            .collect()
    };
    let streams = |state: &voice::State| -> HashSet<Option<String>> {
        let mine = state.sharing.as_ref().map(|_| None);
        let watched = state
            .watching
            .as_ref()
            .map(|watching| Some(watching.user.clone()));
        mine.into_iter().chain(watched).collect()
    };
    let lasting = streams(before);
    let lasting: HashSet<_> = lasting.intersection(&streams(after)).cloned().collect();
    let (before, after) = (viewers(before), viewers(after));
    let changed = |from: &HashSet<(Option<String>, String)>, to: &HashSet<_>| {
        from.difference(to)
            .any(|(stream, _)| lasting.contains(stream))
    };
    if changed(&after, &before) {
        Some(Sound::ViewerJoin)
    } else if changed(&before, &after) {
        Some(Sound::ViewerLeave)
    } else {
        None
    }
}

impl Application {
    pub(super) fn request_join_voice(&mut self, id: String) -> Vec<Effect> {
        if !self.state.can_join_voice(&id) {
            return vec![];
        }
        if self.state.voice.active() {
            if self.state.voice.room.as_ref() != Some(&id) {
                self.state.switch_voice = Some(id);
            }
            vec![]
        } else {
            self.voice_command(voice::Command::Join(id))
        }
    }

    /// Joins a room's call and, once it is connected, turns the camera on,
    /// as Discord's video call button does; in a call already, only the
    /// camera goes on.
    pub(super) fn request_video_call(&mut self, id: String) -> Vec<Effect> {
        if !self.state.can_join_voice(&id) {
            return vec![];
        }
        if self.state.voice.active() && self.state.voice.room.as_ref() == Some(&id) {
            if self.state.voice.camera_phase == voice::CameraPhase::Off {
                return self.voice_command(voice::Command::Video(true));
            }
            return vec![];
        }
        self.state.pending_camera = Some(id.clone());
        self.request_join_voice(id)
    }

    /// Turns the camera on for the call [`AppState::pending_camera`] waits
    /// for, once it connects.
    pub(super) fn film_when_connected(&mut self) -> Vec<Effect> {
        let voice = &self.state.voice;
        let ready = self.state.pending_camera.as_ref().is_some_and(|room| {
            voice.phase == Phase::Connected && voice.room.as_ref() == Some(room)
        });
        if ready {
            self.state.pending_camera = None;
            return self.voice_command(voice::Command::Video(true));
        }
        vec![]
    }

    pub(super) fn accept_call(&mut self, id: String) -> Vec<Effect> {
        if !self.state.can_join_voice(&id) {
            return vec![];
        }
        // What's on screen stays, as in Discord's client: the call shows in
        // the voice dock, and the DM opens from there or from its row.
        let mut effects = vec![];
        if self.state.voice.room.as_ref() != Some(&id) || !self.state.voice.active() {
            if self.state.voice.active() {
                effects.extend(self.voice_command(voice::Command::Leave));
            }
            effects.extend(self.voice_command(voice::Command::Join(id)));
        }
        effects
    }

    pub(super) fn confirm_voice_switch(&mut self) -> Vec<Effect> {
        let Some(id) = self.state.switch_voice.take() else {
            return vec![];
        };
        if !self.state.can_join_voice(&id) {
            self.state.pending_watch = None;
            return vec![];
        }
        // Leaving the call it's in keeps the stream it switches to watch.
        let watch = self.state.pending_watch.take();
        let mut effects = if self.state.voice.active() {
            self.voice_command(voice::Command::Leave)
        } else {
            vec![]
        };
        self.state.pending_watch = watch;
        effects.extend(self.voice_command(voice::Command::Join(id)));
        effects.extend(self.watch_when_connected());
        effects
    }

    /// Someone in a voice channel's list was clicked: shows the channel,
    /// and watches them if they share their screen. Watching from outside
    /// the call joins it once asked, as [`AppState::switch_voice`] does, and
    /// [`Self::watch_when_connected`] then watches.
    pub(super) fn show_voice_person(&mut self, room: String, user: String) -> Vec<Effect> {
        let voice = &self.state.voice;
        let here = voice.active() && voice.room.as_ref() == Some(&room);
        let call = voice
            .participants
            .iter()
            .find(|person| here && person.id == user);
        let listed = self
            .state
            .rooms
            .iter()
            .find(|listed| listed.id == room)
            .and_then(|listed| listed.people.iter().find(|person| person.id == user));
        let me = call.is_some_and(|person| person.local)
            || self.state.discord_user.as_ref() == Some(&user);
        let streaming = call.is_some_and(|person| person.streaming)
            || listed.is_some_and(|person| person.streaming);
        let mut effects = self.dispatch(Action::Open(room.clone()));
        if me || !streaming || self.state.selected.as_ref() != Some(&room) {
            return effects;
        }
        if here {
            self.state.pending_watch = Some((room, user));
            effects.extend(self.watch_when_connected());
        } else if self.state.can_join_voice(&room) {
            self.state.switch_voice = Some(room.clone());
            self.state.pending_watch = Some((room, user));
        }
        effects
    }

    /// Watches the stream [`AppState::pending_watch`] waits for, once its
    /// call is connected and they show sharing in it.
    pub(super) fn watch_when_connected(&mut self) -> Vec<Effect> {
        let voice = &self.state.voice;
        let ready = self
            .state
            .pending_watch
            .as_ref()
            .is_some_and(|(room, user)| {
                voice.phase == Phase::Connected
                    && voice.room.as_ref() == Some(room)
                    && voice
                        .participants
                        .iter()
                        .any(|person| &person.id == user && person.streaming)
            });
        match self.state.pending_watch.take_if(|_| ready) {
            Some((_, user)) => self.voice_command(voice::Command::Watch(user)),
            None => vec![],
        }
    }

    /// A call command, with the sound of muting or deafening in a call.
    pub(super) fn voice_command(&mut self, command: voice::Command) -> Vec<Effect> {
        let connected = self.state.voice.phase == Phase::Connected;
        let sound = match &command {
            _ if !connected => None,
            voice::Command::Deafen(true) => Some(Sound::Deafen),
            // Unmuting undeafens too.
            voice::Command::Deafen(false) | voice::Command::Mute(false)
                if self.state.voice.deafened =>
            {
                Some(Sound::Undeafen)
            }
            voice::Command::Mute(true) => Some(Sound::Mute),
            voice::Command::Mute(false) => Some(Sound::Unmute),
            _ => None,
        };
        let mut effects = self.voice_command_quietly(command);
        effects.extend(sound.map(Effect::Sound));
        effects
    }

    pub(super) fn voice_command_quietly(&mut self, command: voice::Command) -> Vec<Effect> {
        if matches!(command, voice::Command::Leave) {
            self.state.switch_voice = None;
            self.state.pending_watch = None;
            self.state.pending_camera = None;
        }
        if !self.state.preview {
            return vec![Effect::Backend(Command::Voice(command))];
        }
        let before = self.state.voice.clone();
        match command {
            voice::Command::Join(id) => {
                if let Some(room) = self
                    .state
                    .rooms
                    .iter()
                    .find(|room| room.id == id && room.voice)
                {
                    self.state.voice = voice::State {
                        phase: Phase::Connected,
                        room: Some(id),
                        name: room.name.clone(),
                        participants: vec![voice::Participant {
                            id: "preview-you".into(),
                            user: "You".into(),
                            person: "preview-you".into(),
                            local: true,
                            muted: true,
                            deafened: false,
                            server_muted: false,
                            server_deafened: false,
                            speaking: false,
                            connected: true,
                            problem: None,
                            avatar: None,
                            streaming: false,
                            video: None,
                        }],
                        can_film: true,
                        ..self.preview_voice()
                    };
                }
            }
            voice::Command::Leave => {
                let filming = self.state.voice.camera_phase != voice::CameraPhase::Off;
                self.state.voice = self.preview_voice();
                if filming {
                    let mut effects = call_sounds(&before, &self.state.voice);
                    effects.push(Effect::PreviewCamera(None));
                    return effects;
                }
            }
            voice::Command::Video(on) if self.state.voice.active() => {
                let feed = on.then(crate::picture::Feed::new);
                for person in &mut self.state.voice.participants {
                    if person.local {
                        person.video = feed.clone();
                    }
                }
                self.state.voice.camera_phase = if on {
                    voice::CameraPhase::On
                } else {
                    voice::CameraPhase::Off
                };
                let camera = self.state.voice.camera.clone();
                return vec![Effect::PreviewCamera(feed.map(|feed| (camera, feed)))];
            }
            voice::Command::Devices => return vec![Effect::ListDevices],
            voice::Command::Microphone(id) => self.state.voice.microphone = Some(id),
            voice::Command::Speaker(id) => self.state.voice.speaker = Some(id),
            voice::Command::Camera(id) => self.state.voice.camera = Some(id),
            voice::Command::Mute(muted) => {
                if !muted {
                    self.state.voice.deafened = false;
                }
                self.state.voice.muted = muted;
            }
            voice::Command::Deafen(deafened) => {
                self.state.voice.deafened = deafened;
                if deafened {
                    self.state.voice.muted = true;
                }
            }
            _ => {}
        }
        for participant in &mut self.state.voice.participants {
            participant.muted = self.state.voice.muted;
        }
        call_sounds(&before, &self.state.voice)
    }
}
