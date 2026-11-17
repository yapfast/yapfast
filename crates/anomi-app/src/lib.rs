//! Anomi's iced interface on the real backend: the one crate of the
//! interface that knows `yapfast-core`.
//!
//! - [`app`]: the program's state ([`app::App`]) and its `update`, `view`
//!   and `subscription`.
//! - [`message`]: what reaches `update`.
//! - [`backend`] and [`effects`]: the core's `Effect`s run as iced `Task`s,
//!   and its events brought back as messages.
//! - [`project`]: the core's `AppState` shown as the screens' view-models,
//!   an area at a time and only when what the area shows changed.
//! - [`intent`]: what the screens ask, turned into the core's `Action`s.
//! - [`layout`], [`language`], [`pictures`], [`control`], [`window`]: the
//!   settings file, the language, picture buffers, the test session's
//!   commands and the window.
//! - [`clocks`], [`keys`], [`media`], [`platform`]: clocks, the window's keys,
//!   video and voice messages, and what the window needs of the desktop,
//!   each its task's (docs/iced/tasks/).
//!
//! See docs/iced/README.md for the flow from a click to the core and back.

pub mod app;
pub mod backend;
pub mod cli;
pub mod clocks;
pub mod control;
pub mod effects;
pub mod executor;
pub mod intent;
pub mod keys;
pub mod language;
pub mod layout;
pub mod logging;
pub mod media;
pub mod message;
pub mod pictures;
pub mod platform;
pub mod project;
pub mod run;
pub mod window;
