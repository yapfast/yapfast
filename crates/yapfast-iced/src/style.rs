//! The window's colours and the small style helpers every view uses.
//!
//! These are the port's stand-ins for the Slint client's dark Discord look
//! (`ui/theme.slint`), as constants. M0's theme step replaces them with one
//! struct of `src/ui/theme.rs`'s tokens that the widgets read.

use iced::{
    Background, Border, Color, Theme, border,
    widget::{container, scrollable},
};

pub const BACKGROUND: u32 = 0x1a1a1e;
pub const PANEL: u32 = 0x131316;
pub const LINE: u32 = 0x26262c;
pub const TEXT: u32 = 0xdbdee1;
pub const MUTED: u32 = 0x949ba4;
pub const SELECTED: u32 = 0x35373c;
pub const ACCENT: u32 = 0x5865f2;
pub const FIELD: u32 = 0x232428;

/// The Slint client's colours for people, with its Discord look on.
pub const SECONDARY: u32 = 0xb5b8c1;
pub const ACTIVITY: u32 = 0x94959c;
pub const HEADING: u32 = 0x81828a;
pub const LIVE: u32 = 0x3ecf7e;
/// The member list's ground, and the row under the pointer (`#97979f1f`).
pub const MEMBER_GROUND: u32 = 0x1a1a1e;
pub const MEMBER_HOVER: Color = Color {
    r: 0x97 as f32 / 255.0,
    g: 0x97 as f32 / 255.0,
    b: 0x9f as f32 / 255.0,
    a: 0x1f as f32 / 255.0,
};
pub const CARD_GROUND: u32 = 0x121214;
pub const CARD_EDGE: u32 = 0x303036;
pub const CARD_INK: u32 = 0xeef0f4;
pub const RAISED: u32 = 0x28282e;

/// Menus, the tooltip, suggestions and the modal.
pub const MENU: u32 = 0x28282d;
pub const MENU_LINE: u32 = 0x35353b;
pub const TOOLTIP: u32 = 0x0b0c0e;
pub const SECONDARY_INK: u32 = 0xb5b8c1;
pub const DANGER: u32 = 0xf0545e;
pub const DANGER_TEXT: u32 = 0xfa777c;
pub const EDGE: u32 = 0x0c0d0f;
/// A lit row in a list of suggestions or categories.
pub const LIT: u32 = 0x2c2d35;
/// Text selected in the timeline and the composer.
pub const SELECTION: Color = Color {
    r: 0x58 as f32 / 255.0,
    g: 0x65 as f32 / 255.0,
    b: 0xf2 as f32 / 255.0,
    a: 0.35,
};

/// A colour from 0xRRGGBB.
pub fn hex(rgb: u32) -> Color {
    Color::from_rgb8((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

/// `color` with its alpha scaled by `alpha`.
pub fn faded(color: Color, alpha: f32) -> Color {
    Color {
        a: color.a * alpha,
        ..color
    }
}

/// `over` laid on the opaque colour `under`.
pub fn blend(under: u32, over: Color) -> Color {
    let under = hex(under);
    let mix = |a: f32, b: f32| a * (1.0 - over.a) + b * over.a;
    Color::from_rgb(
        mix(under.r, over.r),
        mix(under.g, over.g),
        mix(under.b, over.b),
    )
}

/// White, see-through, for what the pointer lights.
pub fn white(alpha: f32) -> Color {
    Color {
        a: alpha,
        ..Color::WHITE
    }
}

/// A container filled with one colour.
pub fn fill(rgb: u32) -> container::Style {
    container::Style {
        background: Some(Background::Color(hex(rgb))),
        ..Default::default()
    }
}

/// A container filled with `color`, its corners rounded by `radius`.
pub fn rounded(color: Color, radius: impl Into<border::Radius>) -> container::Style {
    container::Style {
        background: Some(color.into()),
        border: Border::default().rounded(radius),
        ..Default::default()
    }
}

/// Each corner its own radius: top left, top right, bottom right, bottom left.
pub fn corners([top_left, top_right, bottom_right, bottom_left]: [f32; 4]) -> border::Radius {
    border::Radius {
        top_left,
        top_right,
        bottom_right,
        bottom_left,
    }
}

/// The raised panels' shadow: menus, the picker, suggestions, the modal.
pub fn raised_shadow() -> iced::Shadow {
    iced::Shadow {
        color: Color {
            a: 0.5,
            ..Color::BLACK
        },
        offset: iced::Vector::new(0.0, 8.0),
        blur_radius: 24.0,
    }
}

/// A thin scroller on no rail, in the window's colours.
pub fn scrollbar(theme: &Theme, status: scrollable::Status) -> scrollable::Style {
    let mut style = scrollable::default(theme, status);
    for rail in [&mut style.vertical_rail, &mut style.horizontal_rail] {
        rail.background = None;
        rail.border = Border::default();
        rail.scroller.background = hex(SELECTED).into();
        rail.scroller.border = Border::default().rounded(4);
    }
    style
}
