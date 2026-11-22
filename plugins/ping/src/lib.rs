//! `/ping`: the first Yapfast plugin, and the smallest one that does something.
//!
//! It adds one command. Someone types `/ping`, and the answer they see is
//! `pong` — in the room they typed in, and only to them: a command's answer is
//! a line the person reads, not a message that goes out, which is why this
//! plugin asks for no capability at all.
//!
//! It also declares a context-menu row, which is the part of the API this
//! plugin was written to exercise. The manifest carries it (`menus` in
//! `yapfast-plugin.json`), and it is the declaration that a plugin's presence
//! in a menu is meant to be: the interface can list it without loading any
//! WebAssembly. The row is drawn, and a click on it is dispatched as
//! `Input::Menu`, which the application delivers with `yf_input`. The PDK
//! exports that for every plugin, and this one keeps the default `input`, which
//! does nothing, so the click is heard and ignored; the handler below answers
//! the command, which is what this plugin is for.

use yapfast_pdk::prelude::*;

/// The plugin's handlers. A plugin is one type, created once per instance.
#[derive(Default)]
struct Ping;

impl Plugin for Ping {
    fn command(&mut self, _ctx: &Context<'_>, command: Command) -> Result<Reply> {
        // Nothing to read: `/ping` takes no options, so the manifest declares
        // none and the composer rejects a `/ping something` before it gets here.
        let _ = command;
        // What it answers is a setting, read where it is used: the call is
        // answered while this function runs, so nothing is cached at
        // activation and a change on the plugin's page counts at once.
        let reply = settings::get::<String>("reply")?
            .filter(|reply| !reply.trim().is_empty())
            .unwrap_or_else(|| "pong".to_owned());
        // `Reply::notice` is a line for the person who typed it, in the room
        // they typed it in. Nothing is sent, so nothing needs a capability.
        Ok(Reply::notice(reply))
    }
}

// Exports `yf_activate`, `yf_deactivate`, `yf_event`, `yf_hook` and
// `yf_command`; the four handlers not written above do nothing, since this
// plugin answers one command and nothing else.
yapfast_plugin!(Ping);
