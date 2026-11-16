//! The screens of Anomi's iced interface: one folder per area of the window,
//! one module per component. A screen is a function of its view-model:
//! `fn view(vm: &AreaVm, ...) -> impl View<Intent> + '_`. It borrows what it
//! shows, asks for things with `anomi_vm::Intent`, and holds no state of its
//! own beyond what its widgets keep in iced's tree.
//!
//! [`window`] lays out the main window from its areas. See
//! docs/iced/README.md for the rules every screen follows.

pub mod account;
pub mod conversation;
pub mod rail;
pub mod shell;
pub mod sidebar;

#[cfg(feature = "shots")]
pub mod shots;

pub use shell::window;
