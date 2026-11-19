//! The small style helpers every view uses. The colours are the theme's
//! ([`crate::theme::Theme`]); the few drawn the same in every theme are
//! here.

use iced::{Background, Border, Color, border, widget::container};

use crate::theme::Theme;

/// What is drawn in the same colour whatever the theme:
/// Discord's grey for a member list's headings, its wash for a member row
/// under the pointer (`#97979f1f`) or whose card is open (`#97979f33`),
/// the chips of server tags, and the crown and the APP tag.
pub const HEADING: u32 = 0x81828a;
pub const MEMBER_HOVER: Color = grey(0x1f);
pub const MEMBER_OPEN: Color = grey(0x33);
pub const CHIP: Color = grey(0x33);
pub const CROWN: u32 = 0xf0b232;
pub const BLURPLE: u32 = 0x5865f2;
/// Black at 70%: the ground of marks and notes over pictures.
pub const SMOKE: Color = Color {
    r: 0.0,
    g: 0.0,
    b: 0.0,
    a: 0xb3 as f32 / 255.0,
};

const fn grey(alpha: u8) -> Color {
    Color {
        r: 0x97 as f32 / 255.0,
        g: 0x97 as f32 / 255.0,
        b: 0x9f as f32 / 255.0,
        a: alpha as f32 / 255.0,
    }
}

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
pub fn blend(under: Color, over: Color) -> Color {
    let mix = |a: f32, b: f32| a * (1.0 - over.a) + b * over.a;
    Color::from_rgb(
        mix(under.r, over.r),
        mix(under.g, over.g),
        mix(under.b, over.b),
    )
}

/// A container filled with one colour.
pub fn fill(color: Color) -> container::Style {
    container::Style {
        background: Some(Background::Color(color)),
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

/// A 1 px outline in `color`, its corners rounded by `radius`.
pub fn edge(color: Color, radius: impl Into<border::Radius>) -> Border {
    Border {
        color,
        width: 1.0,
        radius: radius.into(),
    }
}

/// No outline, its corners rounded by `radius`: the border of a button's,
/// a field's or a quad's style.
pub fn round(radius: impl Into<border::Radius>) -> Border {
    Border::default().rounded(radius)
}

/// A container filled with `ground` in a 1 px `outline`, its corners
/// rounded by `radius`: a card, a bar, a field.
pub fn outlined(
    ground: Color,
    outline: Color,
    radius: impl Into<border::Radius>,
) -> container::Style {
    let radius = radius.into();
    rounded(ground, radius).border(edge(outline, radius))
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

/// A card's shadow, over the window or a dimmed one: the sign-in card, a
/// dialog.
pub fn card_shadow() -> iced::Shadow {
    iced::Shadow {
        color: Color {
            a: 0.4,
            ..Color::BLACK
        },
        offset: iced::Vector::new(0.0, 12.0),
        blur_radius: 32.0,
    }
}

/// The shadow of a window over a card: the Display Name Style's, over the
/// profile editor.
pub fn window_shadow() -> iced::Shadow {
    iced::Shadow {
        color: Color {
            a: 0.7,
            ..Color::BLACK
        },
        offset: iced::Vector::new(0.0, 16.0),
        blur_radius: 48.0,
    }
}

/// The raised panels' shadow: menus, the picker, suggestions; and the
/// settings' cards.
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

/// A raised panel: `ground` in a 1 px `edge` with `radius` corners, over
/// the raised shadow, as menus, suggestions, the picker and cards are.
pub fn panel(ground: Color, edge: Color, radius: f32) -> container::Style {
    container::Style {
        background: Some(ground.into()),
        border: self::edge(edge, radius),
        shadow: raised_shadow(),
        ..Default::default()
    }
}

/// The shadow of the pill floating over the timeline's end: Jump to present.
pub fn pill_shadow() -> iced::Shadow {
    iced::Shadow {
        color: Color::from_rgba8(0, 0, 0, 0x40 as f32 / 255.0),
        offset: iced::Vector::new(0.0, 2.0),
        blur_radius: 8.0,
    }
}

/// The shadow of a message's hover bar, over its row's top edge.
pub fn hover_bar_shadow() -> iced::Shadow {
    iced::Shadow {
        color: Color::from_rgba8(0, 0, 0, 0x59 as f32 / 255.0),
        offset: iced::Vector::new(0.0, 2.0),
        blur_radius: 10.0,
    }
}

/// A list's scroll bar's inks: faint on a dark theme, darker on a light
/// one, and plainer under the pointer.
pub fn scrollbar(theme: &Theme) -> crate::widgets::Scroller {
    if theme.dark {
        crate::widgets::Scroller::DARK
    } else {
        crate::widgets::Scroller::LIGHT
    }
}
