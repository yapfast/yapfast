//! {{project-name}}: a Yapfast plugin.
//!
//! A plugin implements [`Plugin`] and exports its handlers with
//! [`yapfast_plugin!`](yapfast_pdk::yapfast_plugin). Handlers it doesn't
//! implement do nothing, so the only code here is the one command the manifest
//! declares.

use yapfast_pdk::prelude::*;

/// The plugin's state for as long as one WebAssembly instance lives.
///
/// It is a cache and nothing more: the helper may drop the instance between any
/// two calls, so anything that must be there next time goes in `storage`.
#[derive(Default)]
struct Greeter;

impl Plugin for Greeter {
    fn command(&mut self, _ctx: &Context<'_>, command: Command) -> Result<Reply> {
        // The manifest declares `name` as an optional string, so the composer
        // has already checked it and the handler only supplies a default.
        let name = command.string("name").unwrap_or("world");
        Ok(Reply::notice(format!("Hello, {name}!")))
    }
}

yapfast_plugin!(Greeter);
