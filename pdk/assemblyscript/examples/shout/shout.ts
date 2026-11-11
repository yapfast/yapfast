//! `/shout`: send the text the person gave in capitals.
//!
//! The smallest useful plugin: one command, one host call, no state. It shows the
//! three things every plugin does — read what it was given, do something through
//! the host, and say what the person should see.

import { path, text, quote } from "../../assembly/json";
import {
  Plugin,
  register,
  call,
  pdkActivate,
  pdkDeactivate,
  pdkEvent,
  pdkHook,
  pdkCommand,
} from "../../assembly/index";

// The five exports Yapfast calls: AssemblyScript exports this file's functions,
// so each one names the PDK's.
export function yf_activate(): void {
  pdkActivate();
}
export function yf_deactivate(): void {
  pdkDeactivate();
}
export function yf_event(): void {
  pdkEvent();
}
export function yf_hook(): void {
  pdkHook();
}
export function yf_command(): void {
  pdkCommand();
}

class Shout extends Plugin {
  /// The command the manifest declares. Yapfast checked `text` against the
  /// manifest before calling, so it is a string and it is there.
  command(invocation: string): string {
    const found = text(path(invocation, "options.text"));
    if (found === null) return '{"failed":{"message":"say what to shout"}}';

    // `messages.send` needs the capability of the same name, which the manifest
    // declares. The helper sends into the room the command was typed in.
    const answer = call("messages.send", `{"body":${quote(found.toUpperCase())},"reply":true}`);
    // A send is answered later, on this instance's next event, so `pending` is
    // success here and anything else is worth telling the person about.
    if (answer.indexOf('"pending"') >= 0 || answer.indexOf('"ok"') >= 0) return '"none"';
    return '{"failed":{"message":"Yapfast wouldn\'t send that"}}';
  }
}

register(new Shout());
