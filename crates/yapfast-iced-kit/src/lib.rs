//! The pieces Yapfast's iced client builds its screens from, knowing
//! nothing of the core or of the app: the theme's colours and the styles
//! drawn from them, the fonts, the icons, the controls, forms, sheets and
//! dialog frames, the call's tiles, the picture viewer's frame, and the
//! client's own widgets. Each is generic over the message it sends; the
//! app uses them with its one `Message`. See docs/client/kit.md.

pub mod canvas;
pub mod controls;
pub mod dialog;
pub mod elide;
pub mod field;
pub mod fonts;
pub mod form;
pub mod frames;
pub mod icons;
pub mod look;
pub mod piece;
pub mod popover;
pub mod renderer;
pub mod reorder;
pub mod sheet;
pub mod style;
pub mod theme;
pub mod tile;
pub mod timer;
pub mod viewer;
pub mod widgets;

#[cfg(test)]
mod testing;
