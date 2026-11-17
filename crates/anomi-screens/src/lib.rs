//! The screens of Anomi's iced interface: one folder per area of the window,
//! one module per component. A screen is a function of its view-model:
//! `fn view(vm: &Area, motion: Motion) -> impl View<Intent> + '_`. It borrows
//! what it shows, asks for things with its area's `Intent`, and holds no
//! state of its own beyond what its widgets keep in iced's tree.
//!
//! [`window`] lays out the main window from its areas, those here and those
//! of the area crates (`anomi-chat`, `anomi-people`, `anomi-voice`,
//! `anomi-settings`). F01 keeps this list and `shots.rs`; see
//! docs/iced/README.md for the rules every screen follows.

pub mod account;
pub mod browse;
pub mod chrome;
pub mod conversation;
pub mod header;
pub mod inbox;
pub mod new_dm;
pub mod places;
pub mod rail;
pub mod shell;
pub mod sidebar;
pub mod sign_in;
pub mod switcher;

#[cfg(feature = "shots")]
pub mod shots;

pub use shell::window;
