//! Yapfast plugins in AssemblyScript.
//!
//! The ABI is five exports and two imports around JSON, and this file is the
//! whole of the AssemblyScript side of it: marshalling through Extism's kernel,
//! the host calls, and the exports that reach a plugin's own handlers.
//!
//! A plugin extends [Plugin], writes the handlers it needs, and registers one
//! instance. Handlers it doesn't write do nothing, which is why a plugin that
//! adds one command is one class long.
//!
//! Extism's handles are opaque: a handle names memory Extism keeps for the guest,
//! and only the kernel's `store` and `load` may read or write through one. A
//! handle cast to an ArrayBuffer offset compiles and reads zeroes, which is why
//! every copy below is a byte at a time through the kernel.

import { field, items, quote } from "./json";

@external("extism:host/env", "input_length") declare function inputLength(): u64;
@external("extism:host/env", "input_load_u8") declare function inputLoadU8(at: u64): u8;
@external("extism:host/env", "alloc") declare function allocate(bytes: u64): u64;
@external("extism:host/env", "free") declare function release(handle: u64): void;
@external("extism:host/env", "length") declare function handleLength(handle: u64): u64;
@external("extism:host/env", "store_u8") declare function storeU8(handle: u64, value: u8): void;
@external("extism:host/env", "load_u8") declare function loadU8(handle: u64): u8;
@external("extism:host/env", "output_set") declare function outputSet(handle: u64, bytes: u64): void;
@external("extism:host/env", "error_set") declare function errorSet(handle: u64): void;
@external("extism:host/user", "yf_call") declare function callHost(request: u64): u64;
@external("extism:host/user", "yf_post") declare function postHost(message: u64): void;

/// What a plugin writes: one instance, registered once.
export abstract class Plugin {
  /// The instance is new, or was evicted and came back.
  activate(payload: string): void {}
  /// The instance is going away. Answer with what the next activation gets.
  deactivate(): string | null {
    return null;
  }
  /// One event: `kind` says what it is.
  event(envelope: string): void {}
  /// The answer to a call this plugin left pending.
  result(call: u64, answer: string): void {}
  /// Before something goes out. Answer with the ABI's outcome.
  hook(request: string): string {
    return '"unchanged"';
  }
  /// A command a person typed. Answer with the ABI's outcome.
  command(invocation: string): string {
    return '"none"';
  }
}

let plugin: Plugin | null = null;

/// Tells the PDK which instance the exports reach.
export function register(instance: Plugin): void {
  plugin = instance;
}

/// This call's payload.
function takeInput(): string {
  const bytes = inputLength();
  const buffer = new ArrayBuffer(<i32>bytes);
  const out = Uint8Array.wrap(buffer);
  for (let at: u64 = 0; at < bytes; at++) {
    out[<i32>at] = inputLoadU8(at);
  }
  return String.UTF8.decode(buffer);
}

function giveOutput(text: string): void {
  writeTo(0, text, true);
}

/// Copies `text`'s UTF-8 bytes into memory Extism holds, or sets it as output.
function writeTo(handle: u64, text: string, output: bool): void {
  const buffer = String.UTF8.encode(text);
  const bytes = buffer.byteLength;
  const at = output ? allocate(bytes) : handle;
  if (at === 0) {
    log("error", "this instance has no memory left for an answer");
    return;
  }
  const source = Uint8Array.wrap(buffer);
  for (let i = 0; i < bytes; i++) storeU8(at + <u64>i, source[i]);
  if (output) outputSet(at, <u64>bytes);
}

function readFrom(handle: u64): string {
  const bytes = handleLength(handle);
  const buffer = new ArrayBuffer(<i32>bytes);
  const out = Uint8Array.wrap(buffer);
  for (let at: u64 = 0; at < bytes; at++) out[<i32>at] = loadU8(handle + at);
  return String.UTF8.decode(buffer);
}

/// One host call, answered with the `HostReturn` the ABI lists.
///
/// The answer may be `"unsupported"` for a method this build doesn't have, which
/// is how a plugin asks before it relies on one.
export function call(method: string, payload: string): string {
  const request = `{"method":${quote(method)},"payload":${payload}}`;
  const bytes = String.UTF8.encode(request);
  const length = <u64>bytes.byteLength;
  const handle = allocate(length);
  if (handle === 0) return "";
  writeTo(handle, request, false);
  const answer = callHost(handle);
  release(handle);
  if (answer === 0) return "";
  const read = readFrom(answer);
  release(answer);
  return read;
}

/// One call with no answer.
export function post(method: string, payload: string): void {
  const request = `{"method":${quote(method)},"payload":${payload}}`;
  const length = <u64>String.UTF8.byteLength(request);
  const handle = allocate(length);
  if (handle === 0) return;
  writeTo(handle, request, false);
  postHost(handle);
  release(handle);
}

/// A line in the plugin's own log, which its page shows.
export function log(level: string, message: string): void {
  post("log", `{"level":${quote(level)},"message":${quote(message)}}`);
}

/// Reads one of the plugin's own keys. The answer is JSON, or `"null"`.
export function storageGet(key: string): string {
  return call("storage.get", `{"key":${quote(key)}}`);
}

/// Writes one key. `value` is JSON.
export function storageSet(key: string, value: string): bool {
  const answer = call("storage.set", `{"key":${quote(key)},"value":${value}}`);
  return answer.indexOf('"ok"') >= 0;
}

/// Told when a guest aborts, which is what a failed assertion does here.
export function abort(message: string | null, fileName: string | null, line: u32, column: u32): void {
  const shown: string = message === null ? "no reason given" : <string>message;
  log("error", `the plugin stopped: ${shown}`);
}

// ---------------------------------------------------------------------------
// What a plugin's exports call
// ---------------------------------------------------------------------------
//
// AssemblyScript exports the functions of the file `asc` is pointed at, not of a
// library it imports, so a plugin names the five exports itself and each one is
// a line: `export function yf_activate(): void { pdkActivate(); }`.

/// The body of `yf_activate`, for a plugin's own export to call.
export function pdkActivate(): void {
  const payload = takeInput();
  if (plugin !== null) plugin!.activate(payload);
}

export function pdkDeactivate(): void {
  const handed: string | null = plugin === null ? null : plugin!.deactivate();
  const state: string = handed === null ? "" : <string>handed;
  giveOutput(state.length === 0 ? "null" : `{"state":${state}}`);
}

export function pdkEvent(): void {
  const batch = takeInput();
  if (plugin === null) return;
  // Answers first: a plugin reads what a call it left pending answered before
  // the event that asked is looked at again.
  const results = items(field(batch, "results"));
  for (let i = 0; i < results.length; i++) {
    const call = field(results[i], "call");
    const answer = field(results[i], "answer");
    if (call === null || answer === null) continue;
    plugin!.result(<u64>parseInt(call, 10), answer);
  }
  const events = items(field(batch, "events"));
  for (let i = 0; i < events.length; i++) {
    plugin!.event(events[i]);
  }
}

export function pdkHook(): void {
  const request = takeInput();
  const answer = plugin === null ? '"unchanged"' : plugin!.hook(request);
  giveOutput(answer);
}

export function pdkCommand(): void {
  const invocation = takeInput();
  const answer = plugin === null ? '"none"' : plugin!.command(invocation);
  giveOutput(answer);
}
