//! Text in each script DM Sans lacks draws with the system's font for it,
//! with no missing-glyph boxes, once the fonts the shaper asked for are
//! loaded. A test binary of its own: a face loaded here is tried for every
//! character after the fallbacks, which would change what the unit tests'
//! fonts draw.
#![cfg(all(unix, not(target_os = "macos")))]

use iced::advanced::graphics::text::{cosmic_text, font_system};
use yapfast_iced_kit::fonts;

/// A sample of each script, the language fontconfig knows it by, and
/// whether it reads right to left.
const SAMPLES: &[(&str, &str, bool)] = &[
    ("ياسمين", "ar", true),
    ("שלום עולם", "he", true),
    ("湊", "zh-cn", false),
    ("みなと", "ja", false),
    ("カタカナ", "ja", false),
    ("한국어", "ko", false),
    ("नमस्ते", "hi", false),
    ("สวัสดี", "th", false),
    ("Привет", "ru", false),
    ("Καλημέρα", "el", false),
];

/// Each line of `text` in DM Sans: whether it runs right to left, and its
/// glyph ids.
fn shape(text: &str) -> Vec<(bool, Vec<u16>)> {
    use cosmic_text::{Attrs, Buffer, Metrics, Shaping};
    let mut system = font_system().write().expect("font system");
    let raw = system.raw();
    let mut buffer = Buffer::new(raw, Metrics::new(16.0, 20.0));
    let attrs = Attrs::new().family(cosmic_text::Family::Name("DM Sans"));
    buffer.set_text(raw, text, &attrs, Shaping::Advanced, None);
    buffer
        .layout_runs()
        .map(|run| {
            (
                run.rtl,
                run.glyphs.iter().map(|glyph| glyph.glyph_id).collect(),
            )
        })
        .collect()
}

/// Whether the system has a font fontconfig says covers `language`; a
/// script without one is skipped.
fn system_covers(language: &str) -> bool {
    std::process::Command::new("fc-match")
        .args(["-f", "%{lang}", &format!(":lang={language}")])
        .env_remove("LD_LIBRARY_PATH")
        .output()
        .is_ok_and(|output| {
            String::from_utf8_lossy(&output.stdout)
                .split('|')
                .any(|known| known == language)
        })
}

#[test]
fn every_script_draws_without_boxes_in_its_direction() {
    fonts::install(&[]);
    let samples: Vec<_> = SAMPLES
        .iter()
        .filter(|(text, language, _)| {
            let covered = system_covers(language);
            if !covered {
                eprintln!("no system font for {language}; {text} not checked");
            }
            covered
        })
        .collect();
    for (text, _, _) in &samples {
        shape(text);
    }
    // The scripts are queued as they're shaped, and characters still
    // missing after that as they're shaped again.
    fonts::load_wanted();
    for (text, _, _) in &samples {
        shape(text);
    }
    fonts::load_wanted();
    for (text, language, rtl) in &samples {
        let lines = shape(text);
        assert!(
            lines
                .iter()
                .all(|(_, glyphs)| glyphs.iter().all(|&glyph| glyph != 0)),
            "{language}: {text} has boxes: {lines:?}"
        );
        assert!(
            lines.iter().all(|(line_rtl, _)| line_rtl == rtl),
            "{language}: {text} runs the wrong way: {lines:?}"
        );
    }
    // Arabic letters join: س alone and س before another take different
    // glyphs, as the shaper picks each letter's form.
    if samples.iter().any(|(_, language, _)| *language == "ar") {
        let alone = shape("س")[0].1.clone();
        let joined = shape("سس")[0].1.clone();
        assert_eq!(alone.len(), 1);
        assert!(
            joined.iter().all(|glyph| *glyph != alone[0]),
            "joined {joined:?}, alone {alone:?}"
        );
    }
}
