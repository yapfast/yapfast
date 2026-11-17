//! What the window needs of the desktop: the tray, the window itself, a
//! single instance, where it opens, dropped and pasted files, links and
//! global keybinds. One module each, its I task's (I14, I15, I18).

pub mod drop;
pub mod instance;
pub mod keybinds;
pub mod links;
pub mod paste;
pub mod placement;
pub mod tray;
pub mod window;
