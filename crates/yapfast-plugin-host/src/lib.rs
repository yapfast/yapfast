//! The helper process: it serves the plugin wire protocol on stdin and stdout
//! and runs plugin modules for Yapfast.
//!
//! The crate is in two layers, so the protocol can be tested without an engine:
//!
//! - [`protocol`] reads requests and writes responses, and knows nothing about
//!   WebAssembly. [`protocol::Backend`] is the one thing it asks for.
//! - [`runtime`] implements that with Extism. It sits behind the `host` feature,
//!   which is also what builds the `yapfast-plugins` binary, so a default
//!   workspace build links neither Extism nor wasmtime.
//!
//! [`module`] holds the checks a module file has to pass before Extism sees it.
//! [`environment`] is what the helper drops from its own environment before
//! Extism can read it.

pub mod environment;
pub mod module;
pub mod protocol;
#[cfg(feature = "host")]
pub mod runtime;
