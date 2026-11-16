//! What the screens of the iced interface show, and what they ask for.
//!
//! Each area of the window has a module here: its view-model, the plain data
//! its screen draws, and its `Intent`, what a click or a keystroke on it asks.
//! `anomi-app` fills the view-models from the core's `AppState` (as
//! `src/ui/refresh.rs` fills the Slint window) and turns intents into the
//! core's `Action`s. Nothing here knows the core, so the screens type-check
//! without it.
//!
//! An area's module owns its types; a type two areas share goes in [`kind`].
//! See docs/iced/README.md.

pub mod account;
pub mod conversation;
pub mod kind;
pub mod rail;
pub mod shell;
pub mod sidebar;

#[cfg(feature = "fixtures")]
pub mod fixtures;

/// Everything the main window shows, by area.
#[derive(Clone, Debug, Default)]
pub struct Vm {
    pub shell: shell::Shell,
    pub rail: rail::Rail,
    pub sidebar: sidebar::Sidebar,
    pub account: account::Account,
    pub conversation: conversation::Conversation,
}

/// What a screen asks for, by area. `anomi-app` routes each to the area's
/// handler, which changes the view-model's own state or dispatches a core
/// `Action`.
#[derive(Clone, Debug, PartialEq)]
pub enum Intent {
    Rail(rail::Intent),
    Sidebar(sidebar::Intent),
    Account(account::Intent),
}
