//! The menu's choices as rows, text translated: `MenuItem`s of
//! `ui/voice.slint:1456`-`:1560`. A row's view borrows its row, so
//! [`built`] stores them in the view-model (`Share::rows`); the projection
//! calls it after setting the rest, and so do the shots and tests. The
//! notes under a row stay on one line, elided: `Row::wraps` draws at a
//! narrow width at this commit (reported), and at 280 px they fit.

use yapfast_lang::tr;

use anomi_vm::menu::Row;
use anomi_vm::voice::share::{FRAME_RATES, Intent, Resolution, Rows, Share, Sound};

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
        quality: (!share.switching).then(|| {
            Row::new(
                tr!("Higher quality"),
                Intent::HighQuality(!share.high_quality),
            )
            .subtitle(tr!("Sharper, at twice the CPU"))
            .checkbox(share.high_quality)
        }),
    };
    share.rows = rows;
    share
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
