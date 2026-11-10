//! The plugin helper's parts, as a library the binary and the tests share.
//!
//! The binary is a thin argument parser in front of [`host::run`] and
//! [`fetch::run`]; the scheduler and the runtime are tested through these
//! modules, with a fake backend in place of Extism where the test is about
//! ordering rather than WebAssembly.

pub mod backend;
pub mod environment;
pub mod fetch;
pub mod host;
pub mod logging;
pub mod pipe;
pub mod runtime;
pub mod scheduler;
