//! Yapfast plugins in Zig.
//!
//! The ABI is five exports and two imports around JSON, and this file is the
//! whole of the Zig side of it: marshalling through Extism's kernel, the host
//! calls, and the exports that reach a plugin's own handlers.
//!
//! A plugin is one file that imports this one and writes the handlers it needs:
//! `activate`, `deactivate`, `event`, `result`, `hook` and `command`. The export
//! wrappers here ask whether a handler exists at compile time, so a plugin that
//! adds one command writes one function and nothing else.
//!
//! There is no allocator. Payloads are copied into a static buffer, JSON is
//! parsed with a fixed buffer over a little more, and the answer is built in a
//! third. An instance's memory is capped by its manifest and never given back,
//! so passing on a value instead of keeping it is what keeps a plugin alive.

const std = @import("std");

/// Extism's kernel and the helper's two calls.
///
/// A handle is opaque: it names memory Extism keeps for the guest, and only
/// these functions may read or write through one. Casting a handle to a pointer
/// compiles and writes bytes nobody will ever look at, which is why every copy
/// here goes through `store` and `load`.
pub const kernel = struct {
    pub extern "extism:host/env" fn input_length() u64;
    pub extern "extism:host/env" fn input_load_u8(u64) u8;
    pub extern "extism:host/env" fn input_load_u64(u64) u64;
    pub extern "extism:host/env" fn alloc(u64) u64;
    pub extern "extism:host/env" fn free(u64) void;
    pub extern "extism:host/env" fn length(u64) u64;
    pub extern "extism:host/env" fn store_u8(u64, u8) void;
    pub extern "extism:host/env" fn load_u8(u64) u8;
    pub extern "extism:host/env" fn store_u64(u64, u64) void;
    pub extern "extism:host/env" fn load_u64(u64) u64;
    pub extern "extism:host/env" fn output_set(u64, u64) void;
    pub extern "extism:host/env" fn error_set(u64) void;
    pub extern "extism:host/user" fn yf_call(u64) u64;
    pub extern "extism:host/user" fn yf_post(u64) void;
};

/// Sizes a plugin can raise before it includes this file.
pub const input_max = 96 * 1024;
pub const answer_max = 32 * 1024;
pub const output_max = 96 * 1024;

var input_buffer: [input_max]u8 = undefined;
var answer_buffer: [answer_max]u8 = undefined;
var output_buffer: [output_max]u8 = undefined;
var json_arena: [16 * 1024]u8 = undefined;

/// The payload of the call being handled.
pub fn input() []const u8 {
    const length = kernel.input_length();
    if (length > input_max) return error_abort("the payload is larger than the PDK's input buffer");
    var at: u64 = 0;
    while (at + 8 <= length) : (at += 8) {
        const word = kernel.input_load_u64(at);
        var i: u6 = 0;
        while (i < 8) : (i += 1) {
            input_buffer[@intCast(at + i)] = @truncate(word >> (@as(u6, i) * 8));
        }
    }
    while (at < length) : (at += 1) input_buffer[@intCast(at)] = kernel.input_load_u8(at);
    return input_buffer[0..@intCast(length)];
}

fn error_abort(reason: []const u8) []const u8 {
    // A caller that can't be given its payload is answered with nothing, and the
    // reason goes to the plugin's log where a person can see it.
    log(.@"error", reason);
    return input_buffer[0..0];
}

fn write(handle: u64, bytes: []const u8) void {
    var at: usize = 0;
    while (at + 8 <= bytes.len) : (at += 8) {
        var word: u64 = 0;
        for (0..8) |i| word |= @as(u64, bytes[at + i]) << @intCast(i * 8);
        kernel.store_u64(handle + at, word);
    }
    while (at < bytes.len) : (at += 1) kernel.store_u8(handle + at, bytes[at]);
}

fn read(handle: u64, out: []u8) void {
    var at: usize = 0;
    while (at + 8 <= out.len) : (at += 8) {
        const word = kernel.load_u64(handle + at);
        for (0..8) |i| out[at + i] = @truncate(word >> @intCast(i * 8));
    }
    while (at < out.len) : (at += 1) out[at] = kernel.load_u8(handle + at);
}

fn copyIn(bytes: []const u8) ?u64 {
    const handle = kernel.alloc(bytes.len);
    if (handle == 0) return null;
    write(handle, bytes);
    return handle;
}

fn takeInput() []const u8 {
    const bytes = input();
    return bytes;
}

fn giveOutput(bytes: []const u8) void {
    const handle = copyIn(bytes) orelse {
        log(.@"error", "this instance has no memory left for an answer");
        return;
    };
    kernel.output_set(handle, bytes.len);
}

/// One host call: its answer is the `HostReturn` the ABI lists.
///
/// Returns the answer's bytes, which live in the PDK's buffer until the next
/// call, or an empty slice when the call couldn't be made at all.
pub fn call(method: []const u8, payload: []const u8) []const u8 {
    var request: [answer_max]u8 = undefined;
    const object = std.fmt.bufPrint(&request, "{{\"method\":{s},\"payload\":{s}}}", .{
        jsonQuote(request[answer_max - 20 ..], method),
        payload,
    }) catch return "";
    const handle = copyIn(object) orelse return "";
    const answer = kernel.yf_call(handle);
    kernel.free(handle);
    if (answer == 0) return "";
    const length = kernel.length(answer);
    if (length > answer_buffer.len) {
        kernel.free(answer);
        return "";
    }
    read(answer, answer_buffer[0..@intCast(length)]);
    kernel.free(answer);
    return answer_buffer[0..@intCast(length)];
}

/// One call with no answer.
pub fn post(method: []const u8, payload: []const u8) void {
    var request: [answer_max]u8 = undefined;
    const object = std.fmt.bufPrint(&request, "{{\"method\":{s},\"payload\":{s}}}", .{
        jsonQuote(request[answer_max - 20 ..], method),
        payload,
    }) catch return;
    const handle = copyIn(object) orelse return;
    kernel.yf_post(handle);
    kernel.free(handle);
}

fn jsonQuote(out: []u8, text: []const u8) []const u8 {
    var at: usize = 0;
    if (at < out.len) {
        out[at] = '"';
        at += 1;
    }
    for (text) |byte| {
        if (at + 1 < out.len) {
            out[at] = if (byte == '"' or byte == '\\') '-' else byte;
            at += 1;
        }
    }
    if (at < out.len) {
        out[at] = '"';
        at += 1;
    }
    return out[0..at];
}

pub const Level = enum { trace, debug, info, warn, @"error" };

/// A line in the plugin's own log, which its page shows.
pub fn log(level: Level, message: []const u8) void {
    var payload: [1024]u8 = undefined;
    const body = std.fmt.bufPrint(&payload, "{{\"level\":\"{s}\",\"message\":{s}}}", .{
        @tagName(level),
        jsonQuote(payload[900..], message),
    }) catch return;
    post("log", body);
}

/// Reads a key from the plugin's storage. The answer is JSON, or `"null"`.
pub fn storageGet(key: []const u8) []const u8 {
    var payload: [512]u8 = undefined;
    const body = std.fmt.bufPrint(&payload, "{{\"key\":{s}}}", .{
        jsonQuote(payload[256..], key),
    }) catch return "null";
    return call("storage.get", body);
}

/// Writes a key. `value` is JSON.
pub fn storageSet(key: []const u8, value: []const u8) bool {
    var payload: [1024]u8 = undefined;
    const body = std.fmt.bufPrint(&payload, "{{\"key\":{s},\"value\":{s}}}", .{
        jsonQuote(payload[128..], key),
        value,
    }) catch return false;
    const answer = call("storage.set", body);
    return std.mem.indexOf(u8, answer, "\"ok\"") != null;
}

/// Parses JSON with a fixed buffer, so nothing here allocates.
pub fn parse(comptime T: type, bytes: []const u8) ?std.json.Parsed(T) {
    var fba = std.heap.FixedBufferAllocator.init(&json_arena);
    return std.json.parseFromSlice(T, fba.allocator(), bytes, .{ .ignore_unknown_fields = true }) catch null;
}

// ---------------------------------------------------------------------------
// The exports Yapfast calls
// ---------------------------------------------------------------------------
//
// Each reaches the plugin's handler when it has one. A plugin that answers a
// hook but not a command is a plugin with one handler.

const root = @import("root");

/// Whether the plugin's own file declares this handler.
fn hasIt(comptime name: []const u8) bool {
    return @hasDecl(root, name);
}

/// Names the five exports Yapfast calls.
///
/// A plugin calls this once, at comptime — `comptime { yapfast.exports(); }` —
/// and the names are written into the module it is building. Both halves are
/// needed: `@export` gives the function a global symbol of that name, and the
/// build passes the same name to wasm-ld as `--export=`, because a WebAssembly
/// module's export section is written by the linker and not by the compiler.
pub fn exports() void {
    @export(&yf_activate, .{ .name = "yf_activate" });
    @export(&yf_deactivate, .{ .name = "yf_deactivate" });
    @export(&yf_event, .{ .name = "yf_event" });
    @export(&yf_hook, .{ .name = "yf_hook" });
    @export(&yf_command, .{ .name = "yf_command" });
}

// The five functions below are the module's exports, by these names: the build
// passes each one to wasm-ld as `--export=`, which is how a WebAssembly module
// names an export at all. `export fn` alone writes nothing to the export
// section, which is the one thing about this that surprises everybody.

fn yf_activate() callconv(.c) void {
    const payload = takeInput();
    if (comptime hasIt("activate")) root.activate(payload);
}

fn yf_deactivate() callconv(.c) void {
    if (comptime hasIt("deactivate")) {
        const state = root.deactivate();
        var out: [answer_max]u8 = undefined;
        const answer = std.fmt.bufPrint(&out, "{{\"state\":{s}}}", .{state}) catch "null";
        giveOutput(answer);
        return;
    }
    giveOutput("null");
}

fn yf_event() callconv(.c) void {
    const batch = takeInput();
    // Answers first, then events: a plugin reads what a call it left pending
    // answered before the event that asked is looked at again.
    if (comptime hasIt("result")) {
        var buffer: [json_arena.len]u8 = undefined;
        if (parse(std.json.Value, batch)) |parsed| {
            _ = &buffer;
            if (field(parsed.value, "results")) |results| {
                switch (results) {
                    .array => |items| for (items.items) |item| {
                        const id = switch (field(item, "call") orelse continue) {
                            .integer => |number| @as(u64, @intCast(number)),
                            else => continue,
                        };
                        root.result(id, item);
                    },
                    else => {},
                }
            }
            parsed.deinit();
        }
    }
    if (comptime hasIt("event")) {
        if (parse(std.json.Value, batch)) |parsed| {
            if (field(parsed.value, "events")) |events| {
                switch (events) {
                    .array => |items| for (items.items) |item| root.event(item),
                    else => {},
                }
            }
            parsed.deinit();
        }
    }
    if (comptime hasIt("topic")) {
        if (parse(std.json.Value, batch)) |parsed| {
            if (field(parsed.value, "topics")) |topics| {
                switch (topics) {
                    .array => |items| for (items.items) |item| root.topic(item),
                    else => {},
                }
            }
            parsed.deinit();
        }
    }
}

fn field(value: std.json.Value, key: []const u8) ?std.json.Value {
    return switch (value) {
        .object => |object| object.get(key),
        else => null,
    };
}

fn yf_hook() callconv(.c) void {
    const request = takeInput();
    if (comptime hasIt("hook")) {
        const answer = root.hook(request) orelse "unchanged";
        giveOutput(answer);
        return;
    }
    giveOutput("\"unchanged\"");
}

fn yf_command() callconv(.c) void {
    const invocation = takeInput();
    if (comptime hasIt("command")) {
        const answer = root.command(invocation) orelse return giveOutput("\"none\"");
        giveOutput(answer);
        return;
    }
    giveOutput("\"none\"");
}
