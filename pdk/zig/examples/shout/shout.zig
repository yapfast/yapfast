//! `/shout`: send the text the person gave in capitals.
//!
//! The smallest useful plugin: one command, one host call, no state. It shows the
//! three things every plugin does — read what it was given, do something through
//! the host, and say what the person should see.

const std = @import("std");
const yf = @import("yapfast");

// The five exports Yapfast calls.
comptime {
    yf.exports();
}

/// The instance's own memory, reused: a plugin that returns the bytes it was
/// given is a plugin that stays inside its page limit.
var shouted: [1024]u8 = undefined;
var send_payload: [1200]u8 = undefined;

/// The command the manifest declares. Yapfast checked `text` against the
/// manifest before calling, so it is a string and it is there.
pub fn command(invocation: []const u8) ?[]const u8 {
    const parsed = yf.parse(std.json.Value, invocation) orelse return "\"none\"";
    defer parsed.deinit();

    const options = switch (parsed.value) {
        .object => |object| object.get("options") orelse return "\"none\"",
        else => return "\"none\"",
    };
    const text = switch (options) {
        .object => |object| switch (object.get("text") orelse return "\"none\"") {
            .string => |string| string,
            else => return "\"none\"",
        },
        else => return "\"none\"",
    };
    if (text.len > shouted.len) return "\"none\"";
    for (text, 0..) |byte, at| {
        shouted[at] = if (byte >= 'a' and byte <= 'z') byte - ('a' - 'A') else byte;
    }

    // `messages.send` needs the capability of the same name, which the manifest
    // declares. The helper sends into the room the command was typed in.
    const body = std.fmt.bufPrint(&send_payload, "{{\"body\":{f},\"reply\":true}}", .{
        std.json.fmt(shouted[0..text.len], .{}),
    }) catch return "\"none\"";
    const answer = yf.call("messages.send", body);
    // A send is answered later, on this instance's next event, so `pending` is
    // success here and anything else is worth telling the person about.
    if (std.mem.indexOf(u8, answer, "\"pending\"") != null or
        std.mem.indexOf(u8, answer, "\"ok\"") != null)
    {
        return "\"none\"";
    }
    return "{\"failed\":{\"message\":\"Yapfast wouldn't send that\"}}";
}
