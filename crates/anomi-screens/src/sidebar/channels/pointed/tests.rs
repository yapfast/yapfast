//! How a row's look reads the palette in each of its states.

use anomi_theme::Theme;

use super::{Fill, Ink, Look, blend};

fn palette() -> anomi_theme::Palette {
    *Theme::built_in(None).palette()
}

#[test]
fn a_chosen_row_is_active_and_a_pointed_one_lights_up() {
    let palette = palette();
    let chosen = Look {
        selected: true,
        ..Look::default()
    };
    assert_eq!(chosen.fill(&palette, 0.0), palette.active);
    let rest = Look::default();
    assert_eq!(rest.fill(&palette, 0.0).a, 0.0, "nothing at rest");
    assert_eq!(rest.fill(&palette, 1.0), palette.hover);
    let heading = Look {
        fill: Fill::None,
        ..Look::default()
    };
    assert_eq!(heading.fill(&palette, 1.0).a, 0.0);
}

#[test]
fn a_channels_ink_follows_its_state() {
    let palette = palette();
    let none = iced::Color::BLACK;
    let rest = Look::default();
    assert_eq!(rest.ink(&palette, 0.0, none), palette.muted);
    assert_eq!(rest.ink(&palette, 1.0, none), palette.secondary);
    let unread = Look {
        unread: true,
        ..Look::default()
    };
    assert_eq!(unread.ink(&palette, 0.0, none), palette.text);
    let locked = Look {
        locked: true,
        unread: true,
        ..Look::default()
    };
    assert_eq!(locked.ink(&palette, 1.0, none), palette.faint);
    let chosen = Look {
        selected: true,
        ..locked
    };
    assert_eq!(chosen.ink(&palette, 0.0, none), palette.muted);
    let inherit = Look {
        ink: Ink::Inherit,
        ..Look::default()
    };
    assert_eq!(inherit.ink(&palette, 0.5, none), none);
}

#[test]
fn a_muted_row_is_half_as_strong_until_pointed_at_or_chosen() {
    let muted = Look {
        rest: 0.5,
        ..Look::default()
    };
    assert_eq!(muted.strength(0.0), 0.5);
    assert_eq!(muted.strength(1.0), 1.0);
    let chosen = Look {
        selected: true,
        ..muted
    };
    assert_eq!(chosen.strength(0.0), 1.0);
    let dimmed = Look { dim: 0.3, ..muted };
    assert!((dimmed.strength(1.0) - 0.3).abs() < 1e-6);
}

#[test]
fn a_blend_goes_from_one_colour_to_the_other() {
    let (black, white) = (iced::Color::BLACK, iced::Color::WHITE);
    assert_eq!(blend(black, white, 0.0), black);
    assert_eq!(blend(black, white, 1.0), white);
    assert_eq!(blend(black, white, 0.5).r, 0.5);
}
