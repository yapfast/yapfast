//! The drop-down of a settings page (`Picker`, `ui/settings.slint:203`), which
//! is the std-widgets Fluent `ComboBox` (`crates/yapfast-ui/build.rs:18`): its
//! face and the chosen text. The caller sets `.padding(..)` to reach
//! `metrics::CONTROL_HEIGHT`, and `.handle(pick_list::Handle::..)` to Fluent's
//! 12 px chevron in the text-secondary colour (`combobox.slint:84-86`); a
//! pick_list Style cannot set the handle, and iced's default is a filled
//! triangle. The list it opens is [`super::menu`],
//! which iced opens below the field, or above it when there is less room
//! below, so the field stays visible while the list is open: it keeps its rest
//! or hover face, which is why `Opened` maps to those faces and adds no ring.
//!
//! Fluent follows only dark or light, never the palette: the values below are
//! slint's `internal/compiler/widgets/fluent/styling.slint:40-62` and
//! `combobox.slint:62`, dark first.

use iced::widget::pick_list::{Catalog, Status, Style, StyleFn};
use iced::{Background, Border, Color};

use crate::Theme;

impl Catalog for Theme {
    type Class<'a> = StyleFn<'a, Self>;

    fn default<'a>() -> <Self as Catalog>::Class<'a> {
        Box::new(default)
    }

    fn style(&self, class: &<Self as Catalog>::Class<'_>, status: Status) -> Style {
        class(self, status)
    }
}

const fn grey(value: u8, alpha: f32) -> Color {
    Color {
        r: value as f32 / 255.0,
        g: value as f32 / 255.0,
        b: value as f32 / 255.0,
        a: alpha,
    }
}

const WHITE: u8 = 0xff;
const BLACK: u8 = 0;
const PAPER: u8 = 0xf9;

const DARK_BACKGROUND: Color = grey(WHITE, 0.06);
const LIGHT_BACKGROUND: Color = grey(WHITE, 0.70);
const DARK_HOVERED: Color = grey(WHITE, 0.08);
const LIGHT_HOVERED: Color = grey(PAPER, 0.5);
const DARK_DISABLED: Color = grey(WHITE, 0.04);
const LIGHT_DISABLED: Color = grey(PAPER, 0.3);
/// `#00000012` and `#0000000F`.
const DARK_BORDER: Color = Color::from_rgba(0.0, 0.0, 0.0, 0x12 as f32 / 255.0);
const LIGHT_BORDER: Color = Color::from_rgba(0.0, 0.0, 0.0, 0x0f as f32 / 255.0);
/// Fluent's disabled border, `combobox.slint:34`, `styling.slint:46`:
/// `#FFFFFF14` and `#00000073`.
const DARK_BORDER_DISABLED: Color = grey(WHITE, 0x14 as f32 / 255.0);
const LIGHT_BORDER_DISABLED: Color = grey(BLACK, 0x73 as f32 / 255.0);
const DARK_TEXT: Color = grey(WHITE, 1.0);
const LIGHT_TEXT: Color = grey(BLACK, 0.9);
const DARK_TEXT_DISABLED: Color = grey(WHITE, 0.37);
const LIGHT_TEXT_DISABLED: Color = grey(BLACK, 0.37);
const DARK_HANDLE: Color = grey(WHITE, 0.79);
const LIGHT_HANDLE: Color = grey(BLACK, 0.6);
/// The corner radius, `combobox.slint:62`.
const RADIUS: f32 = 3.0;

pub fn default(theme: &Theme, status: Status) -> Style {
    let palette = theme.palette();
    let dark = palette.dark;
    let pick = |dark_value: Color, light_value: Color| if dark { dark_value } else { light_value };
    let disabled = status == Status::Disabled;
    let face = match status {
        Status::Active | Status::Opened { is_hovered: false } => {
            pick(DARK_BACKGROUND, LIGHT_BACKGROUND)
        }
        Status::Hovered | Status::Opened { is_hovered: true } => pick(DARK_HOVERED, LIGHT_HOVERED),
        Status::Disabled => pick(DARK_DISABLED, LIGHT_DISABLED),
    };
    let text = if disabled {
        pick(DARK_TEXT_DISABLED, LIGHT_TEXT_DISABLED)
    } else {
        pick(DARK_TEXT, LIGHT_TEXT)
    };
    Style {
        text_color: text,
        placeholder_color: palette.placeholder(),
        handle_color: if disabled {
            text
        } else {
            pick(DARK_HANDLE, LIGHT_HANDLE)
        },
        background: Background::Color(face),
        border: Border::default()
            .rounded(RADIUS)
            .width(1)
            .color(match (disabled, dark) {
                (false, true) => DARK_BORDER,
                (false, false) => LIGHT_BORDER,
                (true, true) => DARK_BORDER_DISABLED,
                (true, false) => LIGHT_BORDER_DISABLED,
            }),
    }
}
