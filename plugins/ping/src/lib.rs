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
//! WebAssembly. Nothing dispatches a row yet — `MenuDecl` has no consumer in
//! this build — so the handler below answers the command and the menu row
//! waits for the interface half. When it arrives, the row's action comes to
//! the same plugin, and `menu` is where this plugin answers it.

use yapfast_pdk::prelude::*;

/// The plugin's handlers. A plugin is one type, created once per instance.
#[derive(Default)]
struct Ping;

impl Plugin for Ping {
    fn command(&mut self, _ctx: &Context<'_>, command: Command) -> Result<Reply> {
        // Nothing to read: `/ping` takes no options, so the manifest declares
        // none and the composer rejects a `/ping something` before it gets here.
        let _ = command;
        // `Reply::notice` is a line for the person who typed it, in the room
        // they typed it in. Nothing is sent, so nothing needs a capability.
        Ok(Reply::notice("pong"))
    }
}

// Exports `yf_activate`, `yf_deactivate`, `yf_event`, `yf_hook` and
// `yf_command`; the four handlers not written above do nothing, since this
// plugin answers one command and nothing else.
yapfast_plugin!(Ping);
