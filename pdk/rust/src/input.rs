//! What the person did with something a plugin drew.
//!
//! A plugin that puts a row in a menu, a widget in a slot or a notice on the
//! screen hears about it through [`Plugin::input`](crate::Plugin::input), which
//! is given one of these. The ABI's shape is
//! [`docs/plugins/interface.md`](../../../docs/plugins/interface.md).

pub use crate::wire::InputEvent as Input;
