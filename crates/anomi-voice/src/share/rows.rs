//! The menu's choices as rows, text translated: `MenuItem`s of
//! `ui/voice.slint:1456`-`:1560`. A row's view borrows its row, so
//! [`built`] stores them in the view-model (`Share::rows`); the projection
//! calls it after setting the rest, and so do the shots and tests. The
//! notes under a row stay on one line, elided: `Row::wraps` draws at a
//! narrow width at this commit (reported), and at 280 px they fit. While
//! live, the stream's menu's rows too (`Share::live`), its submenus with
//! radio marks under the groups' titles, as Discord's client has them.

use yapfast_lang::tr;

use anomi_vm::menu::Row;
use anomi_vm::voice::share::{FRAME_RATES, Intent, Live, Part, Resolution, Rows, Share, Sound};

/// What the menu calls a resolution.
pub fn resolution(resolution: Resolution) -> &'static str {
    match resolution {
        Resolution::P720 => "720p",
        Resolution::P1080 => "1080p",
        Resolution::Source => tr!("Source"),
    }
}

/// `share` with its rows made from its picks.
pub fn built(mut share: Share) -> Share {
    let pick = |index: usize, text: &str| {
        Row::new(text, Intent::Source(index)).checked(share.source == Some(index))
    };
    let windows = (share.windows.iter().enumerate())
        .map(|(at, title)| pick(share.screens.len() + at, title))
        .collect();
    let dialog = share.asks_dialog().then(|| {
        pick(share.dialog_index(), tr!("Screen or window"))
            .subtitle(tr!("Picked in the system's dialog"))
    });
    let cameras = (share.cameras.iter().enumerate())
        .map(|(at, title)| pick(share.first_camera() + at, title))
        .collect();
    let rows = Rows {
        windows,
        dialog,
        cameras,
        sound: sound(&share),
        apps: (share.sound_apps.iter().enumerate())
            .map(|(at, app)| {
                Row::new(app.as_str(), Intent::Sound(Sound::App(at)))
                    .checked(share.sound == Sound::App(at))
            })
            .collect(),
        resolutions: Resolution::ALL
            .into_iter()
            .map(|choice| {
                Row::new(resolution(choice), Intent::Resolution(choice))
                    .checked(share.resolution == choice)
            })
            .collect(),
        rates: FRAME_RATES
            .into_iter()
            .map(|fps| Row::new(tr!("{} fps", fps), Intent::Fps(fps)).checked(share.fps == fps))
            .collect(),
        quality: Some(
            Row::new(
                tr!("Higher quality"),
                Intent::HighQuality(!share.high_quality),
            )
            .subtitle(tr!("Sharper, at twice the CPU"))
            .checkbox(share.high_quality),
        ),
    };
    share.rows = rows;
    if let Some(mut live) = share.live.take() {
        stream(&share, &mut live);
        share.live = Some(live);
    }
    share
}

/// What the stream goes out at, as the stream's menu and the dock say it:
/// "1080p · 60 fps".
pub fn quality(resolution: Resolution, fps: u32) -> String {
    tr!("{} · {} fps", self::resolution(resolution), fps)
}

/// The stream's menu's rows, from the picks: "Stream Quality" with the
/// frame rates, the resolutions and the encoding beside it, "Sound" with
/// its choices, "Change Windows" and "Stop Streaming".
fn stream(share: &Share, live: &mut Live) {
    let mut rates = vec![Row::heading(tr!("FRAME RATE"))];
    rates.extend(FRAME_RATES.into_iter().map(|fps| {
        Row::new(tr!("{} fps", fps), Intent::Fps(fps))
            .radio(share.fps == fps)
            .plain()
    }));
    rates.push(Row::heading(tr!("RESOLUTION")).separated());
    rates.extend(Resolution::ALL.into_iter().map(|choice| {
        Row::new(resolution(choice), Intent::Resolution(choice))
            .radio(share.resolution == choice)
            .plain()
    }));
    rates.push(Row::heading(tr!("ENCODING")).separated());
    rates.push(
        Row::new(
            tr!("Higher quality"),
            Intent::HighQuality(!share.high_quality),
        )
        .subtitle(tr!("Sharper, at twice the CPU"))
        .checkbox(share.high_quality)
        .plain(),
    );
    let choice = |text: &str, sound: Sound| {
        Row::new(text, Intent::Sound(sound))
            .radio(share.sound == sound)
            .plain()
    };
    let mut sounds = vec![
        choice(tr!("All applications"), Sound::All).subtitle(tr!("Except Yapfast's own sound")),
        choice(tr!("No sound"), Sound::None),
    ];
    for (at, app) in share.sound_apps.iter().enumerate() {
        let row = choice(app.as_str(), Sound::App(at));
        sounds.push(if at == 0 { row.separated() } else { row });
    }
    let heard = match share.sound {
        Sound::All => tr!("All applications").to_owned(),
        Sound::None => tr!("No sound").to_owned(),
        Sound::App(at) => share.sound_apps.get(at).cloned().unwrap_or_default(),
    };
    let opens = |part| Intent::Hover(Some(part));
    live.quality = Row::new(tr!("Stream Quality"), opens(Part::Quality))
        .subtitle(quality(share.resolution, share.fps))
        .submenu(rates)
        .plain();
    live.sound = Row::new(tr!("ShareMenu" => "Sound"), opens(Part::Sound))
        .subtitle(heard)
        .submenu(sounds)
        .plain();
    live.change = Row::new(tr!("Change Windows"), Intent::Change)
        .disabled(!live.changeable)
        .plain();
    live.stop = Row::new(tr!("Stop Streaming"), Intent::Stop)
        .danger()
        .plain();
    live.settle();
}

/// All applications, No sound and, where there are windows to choose one
/// from, One application, which opens the list.
fn sound(share: &Share) -> Vec<Row<Intent>> {
    let choice = |text: &'static str, sound: Sound| {
        Row::new(text, Intent::Sound(sound)).checked(share.sound == sound)
    };
    let mut rows = vec![
        choice(tr!("All applications"), Sound::All).subtitle(tr!("Except Yapfast's own sound")),
        choice(tr!("No sound"), Sound::None),
    ];
    if !share.sound_apps.is_empty() {
        let chosen = match share.sound {
            Sound::App(at) => share.sound_apps.get(at).cloned(),
            _ => None,
        };
        rows.push(
            Row::new(tr!("One application"), Intent::Apps(!share.apps_open))
                .subtitle(chosen.unwrap_or_else(|| tr!("Chosen by its window").into()))
                .checked(matches!(share.sound, Sound::App(_))),
        );
    }
    rows
}
