//! The caption and bar buttons' colours: task F18's unit tests.

use iced::Color;

use anomi_theme::{Motion, Palette, builtin};
use anomi_widgets::icon::Icon;

use super::button::{ACTIVE_ALPHA, HOVER_ALPHA, Kind, button};

fn palette() -> Palette {
    builtin::BUILT_IN[0].palette
}

#[test]
fn close_fills_with_danger_and_inks_on_danger() {
    let palette = palette();
    let close = button::<()>(Icon::X, Kind::Close, Motion::default());
    let (rest, ink) = close.colours(&palette, 0.0, 0.0, 0.0, 1.0);
    assert_eq!((rest.a, ink), (0.0, palette.secondary));
    let (hot, ink) = close.colours(&palette, 1.0, 1.0, 0.0, 1.0);
    assert_eq!((hot, ink), (palette.danger, palette.on_danger));
    // Focus lights the ground but leaves the ink.
    let (focus, ink) = close.colours(&palette, 1.0, 0.0, 0.0, 1.0);
    assert_eq!((focus, ink), (palette.danger, palette.secondary));
}

#[test]
fn a_bar_button_lifts_by_the_overlay_and_dims_when_disabled() {
    let palette = palette();
    let bar = button::<()>(Icon::Inbox, Kind::Bar { active: false }, Motion::default());
    assert_eq!(bar.colours(&palette, 1.0, 1.0, 0.0, 1.0).0.a, HOVER_ALPHA);
    let on = button::<()>(Icon::Inbox, Kind::Bar { active: true }, Motion::default());
    assert_eq!(
        on.colours(&palette, 0.0, 0.0, 1.0, 1.0),
        (
            Color {
                a: ACTIVE_ALPHA,
                ..palette.overlay
            },
            palette.text
        )
    );
    let (ground, ink) = bar.colours(&palette, 1.0, 1.0, 0.0, 0.0);
    assert_eq!((ground.a, ink), (0.0, palette.faint));
    // Halfway between disabled and enabled, the ink is between too.
    let (_, half) = bar.colours(&palette, 0.0, 0.0, 0.0, 0.5);
    assert_ne!(half, palette.faint);
    assert_ne!(half, palette.secondary);
}
