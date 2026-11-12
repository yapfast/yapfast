//! The Yapfast plugin development kit.
//!
//! A Yapfast plugin is a WebAssembly module that a separate helper process
//! runs. The module is small and deliberately dumb: it exports one function per
//! kind of call, imports one request-and-answer host function and one
//! fire-and-forget one, and speaks JSON to both. This crate is the Rust side of
//! that: the exports are generated, the imports are typed, and the JSON is
//! never written by hand.
//!
//! ```ignore
//! use yapfast_pdk::prelude::*;
//!
//! #[derive(Default)]
//! struct Shout;
//!
//! impl Plugin for Shout {
//!     fn command(&mut self, _ctx: &Context<'_>, command: Command) -> Result<Reply> {
//!         let text = command.required_string("text")?;
//!         storage::set("last", &text)?;
//!         Ok(Reply::notice(text.to_uppercase()))
//!     }
//! }
//!
//! yapfast_plugin!(Shout);
//! ```
//!
//! Two things shape a plugin more than the API does.
//!
//! An instance may be evicted between any two calls: the helper drops idle
//! ones, and memory can't be given back any other way. Fields are a cache, and
//! anything that must be there later goes in [`storage`]. Anything a plugin has
//! to know before it is told lives there too, since an activation is the only
//! notice it gets that it was evicted.
//!
//! Memory only grows. WebAssembly memory can't shrink, so an instance that
//! spiked holds that footprint until it is evicted; buffer as little as
//! possible, and prefer a language runtime that doesn't keep a heap it has
//! finished with. Rust is that language.
//!
//! Building for `wasm32-unknown-unknown` is the only supported target: the
//! module runs in Extism, which supplies the imports.

// The only unsafe code is the two Extism host imports in `call`, which are
// unsafe because they cross into the host; the allow is written there, next to
// its safety argument.
#![deny(unsafe_code)]

pub mod app;
pub mod call;
pub mod command;
pub mod event;
pub mod hook;
pub mod http;
pub mod log;
pub mod plugin;
pub mod settings;
pub mod storage;
pub mod timers;
pub mod wire;

/// The modules and types a plugin's `lib.rs` wants, in one line.
///
/// `use yapfast_pdk::prelude::*;` brings in the trait to implement, the types
/// its handlers are given and answer with, the macro that exports them, the
/// call modules, and [`mod@json`] for a payload a plugin defines itself.
pub mod prelude {
    pub use crate::Error;
    pub use crate::Result;
    pub use crate::json;
    pub use crate::wire::method;
    pub use crate::yapfast_plugin;
    pub use crate::{app, call, http, log, settings, storage, timers};

    pub use crate::call::{CallResult, HostReturn, Pending};
    pub use crate::command::{Command, Reply};
    pub use crate::event::{Event, EventKind, Network};
    pub use crate::hook::{Draft, HookKind, HookOutcome, HookRequest, HookSubject};
    pub use crate::plugin::{Activation, Call, Context, Plugin};
}

/// The crate's error type, which is what a handler returns when it can't do
/// what it meant to.
pub use extism_pdk::Error;

/// The crate's result type, so a handler writes `Result<Reply>`.
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// JSON, re-exported so a plugin that has this crate needs no separate
/// dependency for its own payloads: `json::Value` for the type, `json!` for a
/// literal.
pub use serde_json as json;
/// The `json!` macro, under the same name as the crate it builds values of.
pub use serde_json::json;

/// Extism's own crate, for the pieces a plugin occasionally needs directly: the
/// export attribute the macro uses, and `FnResult`.
pub use extism_pdk;
