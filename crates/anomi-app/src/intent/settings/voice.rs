//! Voice & video's intents: the devices, how calls are joined and left,
//! noise suppression, the gate, push to talk, the volumes and the entrance
//! sound, each kept and handed to the call's audio at once, as
//! `on_preferences_changed` and the device callbacks do
//! (`src/ui/callbacks.rs:2303`, `:3604`-`:3720`). The gate's meter and the
//! microphone test follow the page in [`super::gate`].

use iced::Task;
use yapfast_core::application::Action;
use yapfast_core::voice::Command;

use anomi_vm::settings::voice::Intent;

use crate::app::App;
use crate::layout::InputMode;
use crate::message::Message;
use crate::platform::keybinds::{self, record};
use crate::project::Reach;
use crate::project::settings::pages::WAITS;
use crate::project::settings::voice::{STRENGTHS, SUPPRESSORS, entrance_servers};

/// A slider's value as the call's gain.
fn gain(value: i32) -> f32 {
    yapfast_core::volume::from_slider(value.clamp(0, 200) as f32)
}

pub(super) fn handle(app: &mut App, intent: Intent) -> Task<Message> {
    app.vm.settings.voice.apply(&intent);
    let voice = &app.core.state.voice;
    let device =
        |list: &[yapfast_core::voice::Device], at: usize| list.get(at).map(|d| d.id.clone());
    let task = match intent {
        Intent::Microphone(at) => match device(&voice.microphones, at) {
            Some(id) => app.dispatch(Action::Voice(Command::Microphone(id))),
            None => Task::none(),
        },
        Intent::Speaker(at) => match device(&voice.speakers, at) {
            Some(id) => app.dispatch(Action::Voice(Command::Speaker(id))),
            None => Task::none(),
        },
        Intent::Camera(at) => match device(&voice.cameras, at) {
            Some(id) => app.dispatch(Action::Voice(Command::Camera(id))),
            None => Task::none(),
        },
        Intent::Refresh => app.dispatch(Action::Voice(Command::Devices)),
        Intent::RecordTalkKey(on) => {
            record::record(app, on.then_some(record::Target::Talk));
            Task::none()
        }
        Intent::EntranceList(open) if open => app.dispatch(Action::LoadSoundCatalog),
        Intent::EntranceServer(at) => {
            let server = at.checked_sub(1).and_then(|at| {
                entrance_servers(&app.core)
                    .nth(at)
                    .map(|(guild, _)| guild.to_owned())
            });
            let kept = &mut app.shown.settings.kept;
            kept.entrance = server;
            kept.changed();
            app.show(Reach::SETTINGS);
            Task::none()
        }
        Intent::Entrance(id) => {
            let server = app.shown.settings.kept.entrance.clone();
            let sound = (!id.is_empty()).then_some(id);
            app.dispatch(Action::SetJoinSound { server, sound })
        }
        Intent::SoundboardVolumeSettled(volume) => {
            app.dispatch(Action::SoundboardVolume(volume.clamp(0, 100) as f32))
        }
        Intent::NoiseSuppression(at) => {
            crate::intent::voice::noise_suppression(app, SUPPRESSORS[at.min(2)]);
            Task::none()
        }
        intent => {
            setting(app, intent);
            Task::none()
        }
    };
    app.show(Reach::SETTINGS);
    task
}

/// A setting the layout keeps and the call's audio takes at once.
fn setting(app: &mut App, intent: Intent) {
    use yapfast_core::{leveling, noise, volume};
    let layout = &mut app.layout;
    match intent {
        Intent::UnmuteOnJoin(on) => layout.edit(|saved| saved.unmute_on_join = Some(on)),
        Intent::LevelVoice(on) => {
            layout.edit(|saved| saved.level_voice = Some(on));
            leveling::set_enabled(on);
        }
        Intent::AloneWait(at) => {
            layout.edit(|saved| saved.alone_timeout = Some(WAITS[at.min(WAITS.len() - 1)]));
        }
        Intent::NoiseStrength(at) => {
            let strength = STRENGTHS[at.min(2)];
            layout.edit(|saved| saved.noise_strength = strength);
            noise::set_strength(strength);
        }
        Intent::Dereverb(on) => {
            layout.edit(|saved| saved.reduce_room_echo = on);
            noise::set_dereverb(on);
        }
        Intent::WindowsEffects(on) => {
            layout.edit(|saved| saved.windows_microphone_effects = on);
            yapfast_core::set_windows_effects(on);
        }
        Intent::InputMode(at) => {
            let mode = if at == 1 {
                InputMode::PushToTalk
            } else {
                InputMode::VoiceActivity
            };
            layout.edit(|saved| saved.input_mode = mode);
            keybinds::carry(&app.layout);
            keybinds::watch(app);
            record::show(app);
        }
        // Under a drag the page shows the value, and the call hears it.
        Intent::TalkDelay(_) => {}
        Intent::TalkDelaySettled(ms) => {
            layout.edit(|saved| saved.push_to_talk_release_delay = Some(ms.max(0) as u32));
            keybinds::carry(&app.layout);
        }
        Intent::Sensitivity(db) => noise::set_sensitivity(db.clamp(-80, 0) as f32),
        Intent::SensitivitySettled(db) => {
            let db = db.clamp(-80, 0);
            layout.edit(|saved| saved.input_sensitivity = Some(db));
            noise::set_sensitivity(db as f32);
        }
        Intent::MicrophoneVolume(value) => volume::set_microphone(gain(value)),
        Intent::MicrophoneVolumeSettled(value) => {
            layout.edit(|saved| saved.microphone_volume = Some(value.clamp(0, 200) as u8));
            volume::set_microphone(gain(value));
        }
        Intent::SpeakerVolume(value) => volume::set_speakers(gain(value)),
        Intent::SpeakerVolumeSettled(value) => {
            layout.edit(|saved| saved.speaker_volume = Some(value.clamp(0, 200) as u8));
            volume::set_speakers(gain(value));
        }
        // The microphone test follows the page (`gate`); the camera's test,
        // the list closing and the soundboard's slider under a drag show in
        // the page alone.
        intent => tracing::debug!(?intent, "the page alone"),
    }
}
