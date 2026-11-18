//! The paragraph across a change of the person's font
//! (`anomi_theme::fonts::custom`), drawn as the window draws it after one: a
//! paragraph laid out before the change keeps its state, is laid out again,
//! and is drawn by a renderer made anew (`fonts::redrawn`). Its text draws in
//! the new font at once and in DM Sans again after a reset, and Russian in a
//! font without it draws from the fallback. A test binary of its own, as
//! loading a font changes the process's font system under every other
//! test's text; its tests take turns.

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, PoisonError};

use iced::advanced::layout::{Layout, Limits};
use iced::advanced::renderer::{self, Headless};
use iced::advanced::widget::{Tree, Widget};
use iced::{Color, Rectangle, Size, mouse};

use anomi_theme::fonts::custom::{Loaded, Stored, import};
use anomi_theme::{Renderer, Theme, fonts, typography};
use anomi_vm::text::{Text, plain};
use anomi_widgets::paragraph::{Paragraph, paragraph};

const WIDTH: f32 = 480.0;

/// Atkinson Hyperlegible Next's bold, which has no Cyrillic, and draws
/// thicker than DM Sans's regular.
const LATIN_ONLY: &str = "AtkinsonHyperlegibleNext-Bold.ttf";

fn turn() -> MutexGuard<'static, ()> {
    static TURN: Mutex<()> = Mutex::new(());
    TURN.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The person's font as Appearance adds it: copied into a folder of its own
/// (under cargo's scratch folder for tests), then loaded in DM Sans's place.
fn load(name: &str) -> (PathBuf, Loaded) {
    let folder = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("fonts-{name}"));
    let file = PathBuf::from(format!(
        "{}/../../ui/fonts/{name}",
        env!("CARGO_MANIFEST_DIR")
    ));
    let stored: Vec<Stored> = import(&folder, &[file])
        .into_iter()
        .map(|imported| imported.expect("copied"))
        .collect();
    let loaded = Loaded::load(&folder, Loaded::default(), &stored);
    assert!(loaded.replaces(), "{name} draws in DM Sans's place");
    (folder, loaded)
}

/// Takes the person's font out, DM Sans back, and the copies away.
fn reset(folder: PathBuf, loaded: Loaded) {
    let none = Loaded::load(&folder, loaded, &[]);
    assert!(!none.replaces());
    let _ = std::fs::remove_dir_all(folder);
}

/// A renderer made as the window makes one: its caches empty.
fn renderer() -> Renderer {
    let settings = renderer::Settings {
        font: typography::UI,
        ..renderer::Settings::default()
    };
    let mut future = std::pin::pin!(<Renderer as Headless>::new(settings, None));
    let mut context = std::task::Context::from_waker(std::task::Waker::noop());
    loop {
        if let std::task::Poll::Ready(renderer) = future.as_mut().poll(&mut context) {
            return renderer.expect("a headless renderer");
        }
    }
}

/// Lays `view` out on `tree`, as the frame after an update does, and draws
/// it with a renderer made anew: how many pixels its text lit on the black.
fn lit(view: &mut Paragraph<'_, ()>, tree: &mut Tree) -> usize {
    let mut renderer = renderer();
    view.diff(tree);
    let room = Limits::new(Size::ZERO, Size::new(WIDTH, f32::INFINITY));
    view.layout(tree, &renderer, &room);
    let size = tree.size;
    assert!(size.height > 0.0);
    view.draw(
        tree,
        &mut renderer,
        &Theme::built_in(None),
        &renderer::Style {
            text_color: Color::WHITE,
        },
        Layout::new(size),
        mouse::Cursor::Unavailable,
        &Rectangle::with_size(size),
    );
    let physical = Size::new(WIDTH as u32, size.height.ceil() as u32);
    let pixels = renderer.screenshot(physical, 1.0, Color::BLACK);
    pixels
        .chunks(4)
        .filter(|pixel| pixel[..3].iter().any(|&channel| channel > 64))
        .count()
}

/// A paragraph of `text` and its tree, laid out and drawn once.
fn drawn(text: &Text) -> (Paragraph<'_, ()>, Tree, usize) {
    let mut view = paragraph::<()>(text);
    let mut tree = Tree::new(&view);
    let first = lit(&mut view, &mut tree);
    (view, tree, first)
}

#[test]
fn a_paragraph_draws_in_the_new_font_at_once_and_in_dm_sans_after_a_reset() {
    let _turn = turn();
    fonts::install();
    let text = plain("The quick brown fox jumps over the lazy dog");
    let (mut view, mut tree, sans) = drawn(&text);
    assert!(sans > 300, "{sans} pixels in DM Sans");

    let (folder, loaded) = load(LATIN_ONLY);
    // The same paragraph, its shapes kept from before: shaped again in the
    // bold, which lights more, not drawn from faces taken out (none).
    let bold = lit(&mut view, &mut tree);
    assert!(bold > sans + sans / 5, "{sans} in DM Sans, then {bold}");

    reset(folder, loaded);
    let again = lit(&mut view, &mut tree);
    assert_eq!(again, sans, "DM Sans again, as before");
}

#[test]
fn russian_in_a_font_without_it_draws_from_the_fallback() {
    let _turn = turn();
    fonts::install();
    let (folder, loaded) = load(LATIN_ONLY);
    let latin = plain("Hello");
    let mixed = plain("Hello, Привет мир");
    let (_, _, hello) = drawn(&latin);
    let (mut view, mut tree, both) = drawn(&mixed);
    // Each Russian letter from a font that has it, the Latin from theirs.
    assert!(hello > 100, "{hello} pixels of Latin");
    assert!(
        both > hello * 2,
        "{hello} for Hello, {both} with the Russian"
    );
    reset(folder, loaded);
    let sans = lit(&mut view, &mut tree);
    assert!(
        sans > 0 && sans != both,
        "{both} in their font, {sans} after"
    );
}
