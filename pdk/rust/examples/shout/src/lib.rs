//! `/shout`: send the text the person gave in capitals.
//!
//! The smallest useful plugin: one command, one host call, no state. It shows
//! the three things every plugin does — read what it was given, do something
//! through the host, and say what the person should see.

use yapfast_pdk::prelude::*;

/// The plugin's handlers. A plugin is one type, created once per instance.
#[derive(Default)]
struct Shout;

impl Plugin for Shout {
    fn command(&mut self, _ctx: &Context<'_>, command: Command) -> Result<Reply> {
        // The composer has already checked that `text` is a string and is
        // there, because the manifest says it is required; `required_string`
        // only turns the absence into a message a person can read.
        let text = command.required_string("text")?;
        let body = text.to_uppercase();
        // `messages.send` needs the capability of the same name, which this
        // plugin's manifest declares. The helper sends into the room the
        // command was typed in: nothing else names a room.
        call::ok(
            method::MESSAGES_SEND,
            json!({
                "body": body,
                "reply": true,
            }),
        )?;
        Ok(Reply::None)
    }
}

// Exports `yf_activate`, `yf_deactivate`, `yf_event`, `yf_hook` and
// `yf_command`; the four handlers not written above are no-ops, since this
// plugin does nothing but answer one command.
yapfast_plugin!(Shout);
