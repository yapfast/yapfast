//! `/hello`: the smallest whole plugin in Zig, and the shape to start from.
//!
//! A Zig plugin defines only the handlers it has; the PDK asks at compile time
//! whether `command`, `event` and the rest exist. Copy this folder, rename the
//! command in `yapfast-plugin.json`, and write `command`.

const std = @import("std");
const yf = @import("yapfast");

// The five exports Yapfast calls.
comptime {
    yf.exports();
}

/// The instance's own memory, reused: a plugin that answers into a static
/// buffer is a plugin that stays inside its page limit.
var text: [512]u8 = undefined;
var answer: [1024]u8 = undefined;

/// The command the manifest declares. Yapfast checked `name` against the
/// manifest before calling, so when it is there it is a string.
pub fn command(invocation: []const u8) ?[]const u8 {
    const parsed = yf.parse(std.json.Value, invocation) orelse return "\"none\"";
    defer parsed.deinit();

    const name = blk: {
        const options = switch (parsed.value) {
            .object => |object| object.get("options") orelse break :blk "you",
            else => break :blk "you",
        };
        const found = switch (options) {
            .object => |object| object.get("name") orelse break :blk "you",
            else => break :blk "you",
        };
        break :blk switch (found) {
            .string => |string| string,
            else => "you",
        };
    };

    const greeting = std.fmt.bufPrint(&text, "hello, {s}", .{name}) catch return "\"none\"";
    // A command answers with JSON; `notice` is the one line the person sees.
    return std.fmt.bufPrint(&answer, "{{\"notice\":{{\"text\":{f}}}}}", .{
        std.json.fmt(greeting, .{}),
    }) catch return "\"none\"";
}
