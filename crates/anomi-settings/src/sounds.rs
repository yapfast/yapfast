//! The Sounds settings page: which set plays, how loud all the sounds are,
//! and, when asked, how loud each one is, with a button to hear it. The
//! Slint side: `SoundSettings` (`ui/settings.slint:1821`).

use iced::widget::{column, container, row};
use iced::{Length, alignment};

use anomi_theme::prelude::*;
use anomi_vm::icon::Icon;
use anomi_vm::settings::sounds::{Intent, Sounds};
use anomi_widgets::button::icon_button;
use anomi_widgets::slider::{Volume, volume};
use yapfast_lang::tr;

use crate::kit::{heading, picker, switch};

/// Between the parts of the page and of the list of sounds (`spacing: 16px`,
/// `12px`), and between a group's heading and its control (`8px`).
const SPACING: f32 = 16.0;
const LIST: f32 = 12.0;
const GROUP: f32 = 8.0;
/// The picker's width.
const PICKER: f32 = 260.0;

/// The sets, in the order `yapfast_core::sounds` numbers them.
fn sets() -> [&'static str; 6] {
    [
        tr!("New Kalimba"),
        tr!("Felt piano"),
        tr!("Soft marimba"),
        tr!("Mellow Rhodes"),
        tr!("Glow"),
        tr!("Flute"),
    ]
}

/// The sounds' names, in the order of `yapfast_core::sounds::Sound::ALL`.
fn names() -> [&'static str; 12] {
    [
        tr!("Someone joins the call"),
        tr!("Someone leaves the call"),
        tr!("Muting"),
        tr!("Unmuting"),
        tr!("Deafening"),
        tr!("Undeafening"),
        tr!("A message"),
        tr!("A mention"),
        tr!("Someone starts watching a stream"),
        tr!("Someone stops watching a stream"),
        tr!("Someone starts streaming"),
        tr!("Someone stops streaming"),
    ]
}

pub fn view(page: &Sounds, motion: Motion) -> impl View<Intent> + '_ {
    let mut parts: Vec<Element<'_, Intent>> = vec![
        typography::label(tr!(
            "Yapfast plays these as you and others join and leave a call, as you mute and deafen, as others start and stop streaming, as they start and stop watching your stream or the one you watch, and for messages that notify, except in the conversation in front of you. They play on the speakers chosen for calls."
        ))
        .width(Length::Fill)
        .boxed(),
        column![
            heading(tr!("SOUND SET")),
            // A list, as Discord offers its sound packs: six names side by
            // side run wider than the page in a translation.
            container(
                container(picker(
                    &sets(),
                    page.set,
                    Some(Intent::Set as fn(usize) -> Intent),
                ))
                .width(PICKER)
            )
            .width(Length::Fill)
            .align_x(alignment::Horizontal::Left),
        ]
        .spacing(GROUP)
        .boxed(),
        volume(
            Volume::new(tr!("Volume"), page.volume),
            Intent::Volume,
            Intent::VolumeSettled,
        ),
        switch(tr!("Adjust each sound"), page.adjust_each, motion)
            .description(tr!(
                "Set how loud each one is, as a share of the volume above. 0% turns a sound off."
            ))
            .on_toggle(Intent::AdjustEach)
            .view(),
    ];
    if page.adjust_each {
        parts.push(each(page, motion));
    }
    column(parts).spacing(SPACING).width(Length::Fill)
}

/// A slider for each sound with a button that plays it.
fn each(page: &Sounds, motion: Motion) -> Element<'_, Intent> {
    let rows = names().into_iter().enumerate().map(|(index, name)| {
        let share = page.each.get(index).copied().unwrap_or(100);
        let play = tr!("Play {}", name);
        row![
            volume(
                Volume::up_to(name, share, 100),
                move |value| Intent::Each(index, value),
                move |value| Intent::EachSettled(index, value),
            ),
            container(icon_button(Icon::Play, play, motion).on_press(Intent::Play(index)))
                .align_y(alignment::Vertical::Center),
        ]
        .spacing(LIST)
        .align_y(alignment::Vertical::Center)
        .into()
    });
    column(rows.collect::<Vec<Element<'_, Intent>>>())
        .spacing(LIST)
        .width(Length::Fill)
        .boxed()
}

/// Its shots for `anomi-shots`: `settings/sounds.png` as the Slint app showed
/// it, and the page with a slider for each sound.
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    use anomi_vm::fixtures::settings::sounds::{each, page};

    use crate::shots::{GOLDEN, Shot};

    [
        ("sounds", page as fn() -> anomi_vm::Vm),
        ("sounds_each", each),
    ]
    .into_iter()
    .map(|(name, vm)| Shot {
        area: "settings",
        name,
        size: GOLDEN,
        theme: "bonfire",
        vm,
    })
    .collect()
}
