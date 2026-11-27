//! Sounds: the sound set, volumes and personal replacements.

use std::sync::Arc;

use iced::{
    Alignment, Element, Task,
    widget::{self, column, row, text},
};
use yapfast_core::{
    ring::{Ringer, Tone},
    sounds::{Sound, SoundSet},
    themes::media::{SOUND_EXTENSIONS, SoundSlot},
};

use super::{Pref, pref, try_sound};
use crate::settings::{Event as SettingsEvent, MARGIN, NAV, Page, Settings};
use crate::{
    app::Message,
    controls::slider,
    ctx::Cx,
    custom_sounds::{self, Choice},
    form,
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

#[derive(Default)]
pub(in crate::settings) struct Editor {
    menu: Option<SoundSlot>,
    picking: Option<(widget::Id, iced::task::Handle)>,
    error: Option<(SoundSlot, String)>,
    ring_preview: Option<Ringer>,
}

#[derive(Clone, Debug)]
pub enum Event {
    Enable(bool),
    Menu(Option<SoundSlot>),
    Select(SoundSlot, Choice),
    Preview(SoundSlot),
    Choose(SoundSlot),
    Chosen(widget::Id, SoundSlot, Option<Result<Arc<[i16]>, String>>),
    Remove(SoundSlot),
}

fn message(event: Event) -> Message {
    crate::app::settings(SettingsEvent::Sounds(event))
}

impl Settings {
    pub(in crate::settings) fn sound_event(
        &mut self,
        event: Event,
        cx: &mut Cx<'_>,
    ) -> Task<Message> {
        let files = &mut self.sound_editor;
        match event {
            Event::Enable(on) => {
                cx.layout.voice.custom_sound_set = Some(on);
                cx.keep("custom_sound_set", on);
                cx.layout.voice.apply_rings();
                *files = Editor::default();
            }
            Event::Menu(slot) => files.menu = slot,
            Event::Select(slot, choice) => {
                files.menu = None;
                files.error = None;
                select(cx, slot, choice);
                preview(cx, files, slot);
            }
            Event::Preview(slot) => preview(cx, files, slot),
            Event::Choose(slot) => {
                if files.picking.is_some() {
                    return Task::none();
                }
                let pick = widget::Id::unique();
                let id = pick.clone();
                let (task, handle) = Task::perform(
                    async move {
                        let picked = rfd::AsyncFileDialog::new()
                            .set_title("Choose a custom sound")
                            .add_filter("Audio", &SOUND_EXTENSIONS)
                            .pick_file()
                            .await?;
                        Some(
                            tokio::task::spawn_blocking(move || {
                                custom_sounds::decode(slot, picked.path())
                            })
                            .await
                            .unwrap_or_else(|_| Err("Couldn't read the selected sound.".into())),
                        )
                    },
                    move |result| message(Event::Chosen(id.clone(), slot, result)),
                )
                .abortable();
                files.picking = Some((pick, handle.abort_on_drop()));
                files.menu = None;
                files.error = None;
                cx.touch();
                return task;
            }
            Event::Chosen(pick, slot, result) => {
                if files.picking.as_ref().is_none_or(|(id, _)| *id != pick) {
                    return Task::none();
                }
                files.picking = None;
                if let Some(result) = result {
                    match result.and_then(|samples| {
                        custom_sounds::save(cx.client.root(), slot, &samples)
                            .map_err(|error| format!("Couldn't save the sound. {error}"))?;
                        Ok(samples)
                    }) {
                        Ok(samples) => {
                            cx.layout.voice.custom_sounds.insert(slot, samples);
                            select(cx, slot, Choice::File);
                            preview(cx, files, slot);
                        }
                        Err(error) => files.error = Some((slot, error)),
                    }
                }
            }
            Event::Remove(_) if files.picking.is_some() => return Task::none(),
            Event::Remove(slot) => match custom_sounds::remove(cx.client.root(), slot) {
                Ok(()) => {
                    let was_in_use = cx.layout.voice.sound_choice(slot) == Choice::File;
                    cx.layout.voice.custom_sounds.remove(&slot);
                    if was_in_use {
                        select(cx, slot, Choice::Default);
                    }
                    files.ring_preview = None;
                    files.error = None;
                }
                Err(error) => {
                    files.error = Some((slot, format!("Couldn't remove the sound. {error}")));
                }
            },
        }
        cx.touch();
        Task::none()
    }
}

fn select(cx: &mut Cx<'_>, slot: SoundSlot, choice: Choice) {
    let voice = &mut cx.layout.voice;
    if choice == Choice::Default && !voice.custom_sounds.contains_key(&slot) {
        voice.custom_sound_choices.remove(&slot);
    } else {
        voice.custom_sound_choices.insert(slot, choice);
    }
    cx.keep(
        "custom_sound_choices",
        &cx.layout.voice.custom_sound_choices,
    );
    if matches!(slot, SoundSlot::Ring | SoundSlot::Ringback) {
        cx.layout.voice.apply_rings();
    }
}

fn preview(cx: &Cx<'_>, files: &mut Editor, slot: SoundSlot) {
    files.ring_preview = None;
    let tone = match slot {
        SoundSlot::Ring => Tone::Incoming,
        SoundSlot::Ringback => Tone::Outgoing,
        _ => {
            if let Some(sound) = slot.app_sound() {
                try_sound(cx, sound);
            }
            return;
        }
    };
    match Ringer::preview(tone, cx.state().voice.speaker.as_deref()) {
        Ok(preview) => files.ring_preview = Some(preview),
        Err(error) => files.error = Some((slot, format!("Couldn't play the sound. {error}"))),
    }
}

impl<'a> Page<'a> {
    pub(in crate::settings) fn sounds_page(self) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let voice = &self.cx.layout.voice;
        let set = SoundSet::ALL
            .iter()
            .position(|set| *set == voice.sound_set)
            .unwrap_or(0);
        let volume = u32::from(voice.sound_volume.unwrap_or(100));
        let custom = voice.uses_custom_sounds();
        let mut parts = vec![
            form::title(theme, "Sounds"),
            form::note(
                theme,
                "Call tones and sounds for messages, voice and streams play on the speakers chosen for calls.",
            ),
            form::group([
                form::heading(theme, "SOUND SET"),
                crate::controls::choices_within(theme, &SOUND_SETS, set, 520.0, |index| {
                    pref(Pref::SoundSet(index))
                }),
            ]),
            slider("Event volume", volume, 200, |value| {
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
            form::switch(
                "Custom sound set",
                custom,
                Some(message(Event::Enable(!custom))),
            )
            .description("Choose a sound from any set, or select Custom to pick a file.")
            .view(self.cx.look),
        ];
        if !custom && !voice.adjust_each_sound {
            return form::page(parts);
        }
        let mut each = column![].spacing(12);
        if custom {
            each = each.push(form::note(
                theme,
                "WAV, Ogg, Opus and MP3 files can be up to 1 MiB, with a 4-second limit for events and a 6-second limit for call tones.",
            ));
            for (slot, name) in [
                (SoundSlot::Ring, "Incoming call"),
                (SoundSlot::Ringback, "Outgoing call"),
            ] {
                each =
                    each.push(self.sound_row(slot, text(name).size(14).color(theme.text).into()));
            }
        }
        for (index, (&name, sound)) in SOUND_NAMES.iter().zip(Sound::ALL).enumerate() {
            let label: Element<'_, Message> = if voice.adjust_each_sound {
                let value = u32::from(voice.each_sound.get(&sound).copied().unwrap_or(100));
                slider(name, value, 100, move |value| {
                    pref(Pref::EachSound(index, value, false))
                })
                .on_release(pref(Pref::EachSound(index, value, true)))
                .view(theme)
            } else {
                text(name).size(14).color(theme.text).into()
            };
            each = each.push(self.sound_row(SoundSlot::of_sound(sound), label));
        }
        parts.push(each.into());
        form::page(parts)
    }

    fn sound_row(&self, slot: SoundSlot, label: Element<'a, Message>) -> Element<'a, Message> {
        let look = self.cx.look;
        let voice = &self.cx.layout.voice;
        let files = &self.s.sound_editor;
        let play = look
            .icon_button(Icon::Play, "Play", message(Event::Preview(slot)))
            .size(32.0, 18);
        if !voice.uses_custom_sounds() {
            return form::beside(label, play);
        }

        let has_file = voice.custom_sounds.contains_key(&slot);
        let can_change = files.picking.is_none();
        let open = files.menu == Some(slot);
        let mut options = vec![("Default", Some(Choice::Default)), ("Custom", None)];
        if has_file {
            options.push(("Uploaded file", Some(Choice::File)));
        }
        options.extend(
            SOUND_SETS
                .into_iter()
                .zip(SoundSet::ALL.map(|set| Some(Choice::Set(set)))),
        );
        let selected = Some(voice.sound_choice(slot));
        let current = options.iter().position(|(_, choice)| *choice == selected);
        let source = form::dropdown(
            options.iter().map(|(name, _)| (*name).to_owned()).collect(),
            current,
            open,
            message(Event::Menu((!open).then_some(slot))),
            move |index| {
                message(match options[index].1 {
                    Some(choice) => Event::Select(slot, choice),
                    None => Event::Choose(slot),
                })
            },
        )
        .width(136.0)
        .enabled(can_change)
        .view(look);
        let mut actions = row![].spacing(4).align_y(Alignment::Center);
        if has_file {
            actions = actions.push(
                look.icon_button(
                    Icon::X,
                    "Remove uploaded file",
                    can_change.then(|| message(Event::Remove(slot))),
                )
                .size(32.0, 18),
            );
        }
        let actions = actions.push(source).push(play);
        let page_width = look.window.width - NAV - 2.0 * (MARGIN + form::PAGE_SIDE);
        let row = if page_width < 370.0 {
            column![label, actions.wrap().vertical_spacing(4)]
                .spacing(6)
                .into()
        } else {
            form::beside(label, actions)
        };
        if let Some((failed, error)) = &files.error
            && *failed == slot
        {
            return column![row, form::error(look.theme, error.as_str())]
                .spacing(4)
                .into();
        }
        row
    }
}
