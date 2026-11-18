//! Sounds: `Effect::Sound`, and a message's own (`effects::notify`), played
//! through the core's audio as the person's settings say (`sound`, `play`
//! and `sound_gain` in `src/ui.rs`): on the instrument chosen, at the volume
//! chosen and each sound's own, on the call's speakers. Deafened in a call,
//! only deafening and undeafening sound. A sound plays on a thread of the
//! core's own and asks nothing of the window.

use iced::Task;
use yapfast_core::application::Effect;
use yapfast_core::sounds::{self, Sound};
use yapfast_core::voice;

use crate::app::App;
use crate::layout::Saved;
use crate::message::Message;

/// The sounds' volume as it starts: as they're made, 12 dB under full scale
/// at the loudest (`DEFAULT_SOUND_VOLUME`, `src/ui.rs`).
const VOLUME: u8 = 100;

pub(super) fn run(app: &mut App, effect: Effect) -> Task<Message> {
    match effect {
        Effect::Sound(sound) => play(app, sound),
        effect => return super::waiting(&effect),
    }
    Task::none()
}

/// Plays `sound`, unless deafened in a call.
pub(crate) fn play(app: &App, sound: Sound) {
    let voice = &app.core.state.voice;
    if !heard(voice, sound) {
        return;
    }
    let gain = gain(&app.layout, sound);
    sounds::play(app.layout.sound_set, sound, gain, voice.speaker.as_deref());
}

/// Whether `sound` is heard: deafened in a call, only the sounds of
/// deafening and undeafening are.
fn heard(voice: &voice::State, sound: Sound) -> bool {
    !(voice.active() && voice.deafened) || matches!(sound, Sound::Deafen | Sound::Undeafen)
}

/// How loud `sound` plays, as a gain: the sounds' volume, then its own
/// where the person adjusts each sound.
pub(crate) fn gain(saved: &Saved, sound: Sound) -> f32 {
    let own = (saved.each_sound.get(&sound).copied())
        .filter(|_| saved.adjust_each_sound)
        .unwrap_or(100);
    sounds::gain(saved.sound_volume.unwrap_or(VOLUME)) * sounds::gain(own)
}

#[cfg(test)]
mod tests {
    use yapfast_core::sounds::Sound;

    use crate::layout::Saved;

    #[test]
    fn a_sound_turned_down_to_nothing_plays_at_no_gain() {
        let mut saved = Saved {
            sound_volume: Some(50),
            ..Saved::default()
        };
        assert_eq!(super::gain(&saved, Sound::Message), 0.25);
        saved.each_sound.insert(Sound::Message, 0);
        // Each sound's own volume counts only while the person adjusts them.
        assert_eq!(super::gain(&saved, Sound::Message), 0.25);
        saved.adjust_each_sound = true;
        assert_eq!(super::gain(&saved, Sound::Message), 0.0);
        assert_eq!(super::gain(&saved, Sound::Mention), 0.25);
    }

    #[test]
    fn deafened_in_a_call_only_deafening_sounds() {
        let mut voice = yapfast_core::voice::State {
            deafened: true,
            ..Default::default()
        };
        // Out of a call, deafened means the next call joins so.
        assert!(super::heard(&voice, Sound::Message));
        voice.phase = yapfast_core::voice::Phase::Connected;
        assert!(!super::heard(&voice, Sound::Message));
        assert!(!super::heard(&voice, Sound::Mute));
        assert!(super::heard(&voice, Sound::Undeafen));
    }
}
