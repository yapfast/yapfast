//! How calls start and sound: the switches, the input profile over the
//! microphone's processing, the choices of noise suppression and of waiting
//! alone, the soundboard's volume, the input mode with its key and release
//! delay, and the gate's sensitivity (`ui/settings.slint:971-1148`).

use iced::widget::column;

use anomi_theme::prelude::*;
use anomi_vm::settings::voice::{Intent, Voice};
use anomi_widgets::slider::{Reading, Sensitivity, Volume, sensitivity, volume};
use yapfast_lang::tr;

use super::{GROUP, note};
use crate::kit::{choices, heading, recorder, switch, words};

/// The push to talk keys' well (`KeyRecorder { width: 200px }`).
const TALK_KEYS: f32 = 200.0;

/// A heading over a row of choices and what they mean.
fn group<'a>(
    title: &'static str,
    options: &'static [&'static str],
    current: usize,
    pick: fn(usize) -> Intent,
    about: &'a str,
    motion: Motion,
) -> Element<'a, Intent> {
    column![
        heading(title),
        choices(options, current, motion)
            .wrap(true)
            .on_pick(pick)
            .view(),
        note(about),
    ]
    .spacing(GROUP)
    .boxed()
}

/// Whether joining turns the microphone on.
pub fn unmute(page: &Voice, motion: Motion) -> Element<'_, Intent> {
    switch(
        tr!("Turn on your microphone when you join a call"),
        page.unmute_on_join,
        motion,
    )
    .description(tr!(
        "Otherwise you join muted, and unmute when you're ready."
    ))
    .on_toggle(Intent::UnmuteOnJoin)
    .view()
}

/// The input profile the switches under it make, as Discord's: choosing a
/// preset sets them.
pub fn profile(page: &Voice, motion: Motion) -> Element<'_, Intent> {
    group(
        tr!("INPUT PROFILE"),
        words(&[
            tr!("Voice Isolation"),
            tr!("Studio"),
            yapfast_lang::text_in("InputProfile", "Custom"),
        ]),
        page.profile().index(),
        Intent::Profile,
        tr!(
            "Voice Isolation turns on everything below that cleans your voice. Studio turns it all off and sends your microphone as it is. Change a switch yourself for Custom."
        ),
        motion,
    )
}

/// Leveling, which every call has.
pub fn level(page: &Voice, motion: Motion) -> Element<'_, Intent> {
    switch(tr!("Even out your voice"), page.level_voice, motion)
        .description(tr!(
            "Keeps your voice steady as you lean towards the microphone or away from it. Whispers and shouts still come through as they are."
        ))
        .on_toggle(Intent::LevelVoice)
        .view()
}

/// How long a DM call waits with no one else in it.
pub fn alone(page: &Voice, motion: Motion) -> Element<'_, Intent> {
    group(
        tr!("LEAVE A DM CALL WHEN YOU'RE ALONE"),
        words(&[
            tr!("Never"),
            tr!("3 min"),
            tr!("10 min"),
            tr!("30 min"),
            tr!("1 hour"),
        ]),
        page.alone_wait,
        Intent::AloneWait,
        tr!(
            "Once everyone else has left a DM or group call, or nobody has answered yours, you leave it after this long. Server voice channels keep you until you leave."
        ),
        motion,
    )
}

/// What takes the noise out, and how far.
pub fn noise(page: &Voice, motion: Motion) -> Vec<Element<'_, Intent>> {
    let mut parts = vec![group(
        tr!("NOISE SUPPRESSION IN DISCORD CALLS"),
        words(&["DeepFilterNet", "RNNoise", tr!("Off")]),
        page.noise_suppression,
        Intent::NoiseSuppression,
        tr!(
            "DeepFilterNet also takes out typing and other sudden noise. RNNoise uses less processing. Off sends your microphone as it is, for a quiet room. A call going on switches straight away."
        ),
        motion,
    )];
    if page.noise_suppression != 2 {
        parts.push(group(
            tr!("NOISE SUPPRESSION STRENGTH"),
            // "Medium" and "Light" mean other things elsewhere (a font's weight,
            // the light theme), so the strength asks in its own context.
            words(&[
                tr!("Full"),
                yapfast_lang::text_in("VoiceSettings", "Medium"),
                yapfast_lang::text_in("VoiceSettings", "Light"),
            ]),
            page.noise_strength,
            Intent::NoiseStrength,
            tr!(
                "Full takes out the most noise, but can cut off giggles and breathy laughs. Medium and Light leave a little of the room's sound in, which can sound more natural, and let a little more noise through."
            ),
            motion,
        ));
    }
    parts
}

/// The soundboard's volume, kept for every device of the account.
pub fn soundboard(page: &Voice) -> Element<'_, Intent> {
    column![
        volume(
            Volume::up_to(tr!("Soundboard volume"), page.soundboard_volume, 100),
            Intent::SoundboardVolume,
            Intent::SoundboardVolumeSettled,
        ),
        note(tr!(
            "How loud soundboard sounds play in Discord calls, yours and everyone else's. Discord keeps it for all your devices."
        )),
    ]
    .spacing(GROUP)
    .boxed()
}

/// The switches for the room's echo and Windows' effects.
pub fn room(page: &Voice, motion: Motion) -> Vec<Element<'_, Intent>> {
    let mut parts = vec![
        switch(
            tr!("Take out your room's echo in Discord calls"),
            page.dereverb,
            motion,
        )
        .description(tr!(
            "For a bare or echoey room that makes your voice sound far away. Uses a little more processing, and your voice goes out 10 ms later."
        ))
        .on_toggle(Intent::Dereverb)
        .view(),
    ];
    if page.windows_effects_choice {
        parts.push(
            switch(
                tr!("Let Windows process your microphone in Discord calls"),
                page.windows_effects,
                motion,
            )
            .description(tr!(
                "Runs the effects Windows and your sound driver add to the microphone, such as their own noise suppression. Off, Yapfast skips them, as they can muffle your voice on top of Yapfast's. Turn it on if your microphone sounds worse without them, as some laptop microphones can."
            ))
            .on_toggle(Intent::WindowsEffects)
            .view(),
        );
    }
    parts
}

/// Voice activity or push to talk, and what push to talk needs.
pub fn input(page: &Voice, motion: Motion) -> Vec<Element<'_, Intent>> {
    let about = if page.input_mode == 0 {
        tr!("Yapfast sends your voice when it hears you speak.")
    } else {
        tr!("Yapfast sends your microphone only while you hold a key.")
    };
    let mut parts = vec![group(
        tr!("INPUT MODE"),
        words(&[tr!("Voice activity"), tr!("Push to talk")]),
        page.input_mode,
        Intent::InputMode,
        about,
        motion,
    )];
    if page.input_mode == 1 {
        parts.push(talk_key(page, motion));
        parts.push(release_delay(page));
    } else if page.discord {
        parts.push(gate(page));
    }
    parts
}

fn talk_key(page: &Voice, motion: Motion) -> Element<'_, Intent> {
    let about = if page.talk_recording {
        tr!("Press the keys to hold while you talk, then let go. Escape stops without changing it.")
    } else if page.talk_key.is_empty() {
        tr!("Push to talk is on, but no keybind is set, so your microphone sends nothing.")
    } else {
        page.talk_note.as_str()
    };
    column![
        heading(tr!("PUSH TO TALK KEYBIND")),
        recorder(
            &page.talk_key,
            page.talk_recording,
            &page.recorded_keys,
            Intent::RecordTalkKey,
            TALK_KEYS,
            motion,
        ),
        note(about),
    ]
    .spacing(GROUP)
    .boxed()
}

fn release_delay(page: &Voice) -> Element<'_, Intent> {
    let ms = page.talk_delay;
    let shown = if ms >= 1000 {
        tr!("{} s", format!("{:.2}", f64::from(ms) / 1000.0))
    } else {
        tr!("{} ms", ms)
    };
    column![
        volume(
            Volume::up_to(tr!("Push to talk release delay"), ms, 2000)
                .shown(shown)
                .step(10)
                .full_mark(false),
            Intent::TalkDelay,
            Intent::TalkDelaySettled,
        ),
        note(tr!(
            "How long your microphone keeps sending after you let go of the key, so the end of a word isn't cut off."
        )),
    ]
    .spacing(GROUP)
    .boxed()
}

/// The gate's handle over the microphone's live level.
fn gate(page: &Voice) -> Element<'_, Intent> {
    let about = if page.gate_note.is_empty() {
        tr!(
            "Yapfast sends your voice when it's louder than the handle, and other sounds when they're 18 dB louder still. The bar is your microphone after noise suppression and at its volume, as Yapfast hears it to decide."
        )
    } else {
        page.gate_note.as_str()
    };
    column![
        sensitivity(
            Sensitivity {
                label: tr!("Input sensitivity"),
                value: page.sensitivity,
                level: Reading::Live(page.gate.clone()),
                sending: page.gate_sending,
            },
            Intent::Sensitivity,
            Intent::SensitivitySettled,
        ),
        note(about),
    ]
    .spacing(GROUP)
    .boxed()
}
