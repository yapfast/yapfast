//! `/hello`: the smallest whole plugin, and the shape to start from.
//!
//! Yapfast calls five exports on every plugin, and this file names them; what
//! the plugin *does* is the class below them. Copy this folder, rename the
//! command in `yapfast-plugin.json`, and write `command`.

import { path, text, quote } from "../assembly/json";
import {
  Plugin,
  register,
  pdkActivate,
  pdkDeactivate,
  pdkEvent,
  pdkHook,
  pdkCommand,
} from "../assembly/index";

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

class Hello extends Plugin {
  /// The command the manifest declares. Yapfast checked the options against the
  /// manifest before calling, so an option that is there is the type it says.
  command(invocation: string): string {
    const found = text(path(invocation, "options.name"));
    const who = found === null ? "you" : found;
    // A command answers with the one line the person sees; `quote` is the PDK's
    // JSON string writer, so a name with a quote in it doesn't break the frame.
    return quote(`hello, ${who}`);
  }
}

register(new Hello());
