//! The list a pick list or a combo box opens. In Slint it is Fluent's popup,
//! not the context menu: `MenuBorder` and `ListItem` in slint
//! `internal/compiler/widgets/fluent/components.slint:21-105`. Like the field
//! (`super::pick_list`) it follows only dark or light, never the palette, so
//! the values below are Fluent's, dark first.

use iced::widget::overlay::menu::{Catalog, Style, StyleFn};
use iced::{Background, Border, Color, Shadow};

use crate::{Theme, rgb, rgba};

impl Catalog for Theme {
    type Class<'a> = StyleFn<'a, Self>;

    fn default<'a>() -> <Self as Catalog>::Class<'a> {
        Box::new(default)
    }

    fn style(&self, class: &<Self as Catalog>::Class<'_>) -> Style {
        class(self)
    }
}

/// `alternate-background`.
const DARK_BACKGROUND: Color = rgb(0x2c2c2c);
const LIGHT_BACKGROUND: Color = rgb(0xf0f0f0);
/// `control-background-stroke-flyout`, the 1 px border.
const DARK_BORDER: Color = rgba(0x0000_0033);
const LIGHT_BORDER: Color = rgba(0x0000_000f);
/// `control-foreground`.
const DARK_TEXT: Color = rgb(0xffffff);
const LIGHT_TEXT: Color = rgba(0x0000_00e6);
/// `subtle-secondary`, the pointer's row.
const DARK_SELECTED: Color = rgba(0xffff_ff0f);
const LIGHT_SELECTED: Color = rgba(0x0000_000a);
/// `text-secondary`, the pointer's row's text.
const DARK_SELECTED_TEXT: Color = rgba(0xffff_ffc9);
const LIGHT_SELECTED_TEXT: Color = rgba(0x0000_0099);
/// The popup's corner radius.
const RADIUS: f32 = 7.0;

pub fn default(theme: &Theme) -> Style {
    let dark = theme.palette().dark;
    let pick = |dark_value: Color, light_value: Color| if dark { dark_value } else { light_value };
    Style {
        background: Background::Color(pick(DARK_BACKGROUND, LIGHT_BACKGROUND)),
        border: Border::default()
            .rounded(RADIUS)
            .width(1)
            .color(pick(DARK_BORDER, LIGHT_BORDER)),
        text_color: pick(DARK_TEXT, LIGHT_TEXT),
        selected_text_color: pick(DARK_SELECTED_TEXT, LIGHT_SELECTED_TEXT),
        selected_background: Background::Color(pick(DARK_SELECTED, LIGHT_SELECTED)),
        // iced draws the quad inside the menu's own layer, which clips a
        // shadow to the rectangle and squares its corners; the popup's real
        // shadow belongs to F06's layer.
        shadow: Shadow::default(),
    }
}
