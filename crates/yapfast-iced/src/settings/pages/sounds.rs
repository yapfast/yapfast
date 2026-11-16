//! Sounds: the sound set and each sound on or off.

use iced::{
    Alignment, Element, Length,
    widget::{column, container, row},
};
use yapfast_core::sounds::{Sound, SoundSet};

use super::{Pref, pref};
use crate::{
    app::{App, Message},
    controls::slider,
    form::{self},
    pictures::Icon,
};

pub(super) const SOUND_SETS: [&str; 6] = [
    "New Kalimba",
    "Felt piano",
    "Soft marimba",
    "Mellow Rhodes",
    "Glow",
    "Flute",
];

/// Each of Yapfast's sounds, in `Sound::ALL`'s order.
pub(super) const SOUND_NAMES: [&str; 12] = [
    "Someone joins the call",
    "Someone leaves the call",
    "Muting",
    "Unmuting",
    "Deafening",
    "Undeafening",
    "A message",
    "A mention",
    "Someone starts watching a stream",
    "Someone stops watching a stream",
    "Someone starts streaming",
    "Someone stops streaming",
];

impl App {
    pub(in crate::settings) fn sounds_page(&self) -> Element<'_, Message> {
        let theme = &self.theme;
        let voice = &self.layout.voice;
        let set = SoundSet::ALL
            .iter()
            .position(|set| *set == voice.sound_set)
            .unwrap_or(0);
        let volume = u32::from(voice.sound_volume.unwrap_or(100));
        let mut parts = vec![
            form::title(theme, "Sounds"),
            form::note(
                theme,
                "Yapfast plays these as you and others join and leave a call, as you mute and deafen, as others start and stop streaming, as they start and stop watching your stream or the one you watch, and for messages that notify, except in the conversation in front of you. They play on the speakers chosen for calls.",
            ),
            form::group([
                form::heading(theme, "SOUND SET"),
                crate::controls::choices_within(theme, &SOUND_SETS, set, 520.0, |index| {
                    pref(Pref::SoundSet(index))
                }),
            ]),
            slider("Volume", volume, 200, |value| {
                pref(Pref::SoundVolume(value, false))
            })
            .on_release(pref(Pref::SoundVolume(volume, true)))
            .view(theme),
            self.flip(
                "Adjust each sound",
                "Set how loud each one is, as a share of the volume above. 0% turns a sound off.",
                voice.adjust_each_sound,
                Pref::AdjustEach,
            ),
        ];
        if voice.adjust_each_sound {
            let mut each = column![].spacing(12);
            for (index, (name, sound)) in SOUND_NAMES.iter().zip(Sound::ALL).enumerate() {
                let value = u32::from(voice.each_sound.get(&sound).copied().unwrap_or(100));
                each = each.push(
                    row![
                        container(
                            slider(*name, value, 100, move |value| {
                                pref(Pref::EachSound(index, value, false))
                            })
                            .on_release(pref(Pref::EachSound(index, value, true)))
                            .view(theme)
                        )
                        .width(Length::Fill),
                        self.look()
                            .icon_button(Icon::Play, "Play", pref(Pref::PlaySound(index)))
                            .size(32.0, 18)
                    ]
                    .spacing(12)
                    .align_y(Alignment::Center),
                );
            }
            parts.push(each.into());
        }
        form::page(parts)
    }
}
