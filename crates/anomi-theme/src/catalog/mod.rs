//! iced's built-in widgets styled for [`Theme`]: one module per widget, each
//! implementing that widget's `Catalog` with the default look the interface
//! wants, plus the named styles several places share.
//!
//! A widget takes another look with `.style(fn)`, where the function reads
//! `theme.palette()`; a look one widget alone uses lives beside it in
//! `anomi-widgets`. A built-in widget not listed here doesn't compile with
//! [`Theme`] until its module is added (docs/iced/tasks/foundation.md, F02).

pub mod button;
pub mod checkbox;
pub mod combo_box;
pub mod container;
pub mod float;
pub mod menu;
pub mod pick_list;
pub mod progress_bar;
pub mod qr_code;
pub mod radio;
pub mod rule;
pub mod scrollable;
pub mod slider;
pub mod svg;
pub mod text;
pub mod text_editor;
pub mod text_input;
pub mod toggler;

/// Implements a widget's `Catalog` whose style takes a status, with boxed
/// closures for classes and `$default` as the default look.
macro_rules! catalog_with_status {
    ($widget:ident, $default:path) => {
        impl iced::widget::$widget::Catalog for $crate::Theme {
            type Class<'a> = iced::widget::$widget::StyleFn<'a, Self>;

            fn default<'a>() -> Self::Class<'a> {
                Box::new($default)
            }

            fn style(
                &self,
                class: &Self::Class<'_>,
                status: iced::widget::$widget::Status,
            ) -> iced::widget::$widget::Style {
                class(self, status)
            }
        }
    };
}

/// The same for a widget whose style takes no status.
macro_rules! catalog {
    ($widget:ident, $default:path) => {
        impl iced::widget::$widget::Catalog for $crate::Theme {
            type Class<'a> = iced::widget::$widget::StyleFn<'a, Self>;

            fn default<'a>() -> Self::Class<'a> {
                Box::new($default)
            }

            fn style(&self, class: &Self::Class<'_>) -> iced::widget::$widget::Style {
                class(self)
            }
        }
    };
}

pub(crate) use {catalog, catalog_with_status};
