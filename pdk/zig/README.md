# Plugins in Zig

A plugin is a WebAssembly module Extism runs, and the ABI between it and Yapfast
is five exports and two imports around JSON. This folder is the whole Zig side of
it: no WASI, no allocator of its own, and nothing to install beyond a `zig` on
your PATH.

| File | Is |
| --- | --- |
| `src/yapfast.zig` | The PDK: the kernel's imports, marshalling, host calls, and the five exports |
| `template` | **Start here:** the plugin to copy — a manifest, one handler and a build script |
| `examples/shout` | A whole plugin: one command, one host call, no state |

```sh
cd examples/shout && ./build.sh && ls -l plugin.wasm
```

## What a plugin writes

A plugin is one file that imports the PDK and writes the handlers it needs:
`activate`, `deactivate`, `event`, `result`, `hook` and `command`. The exports ask
whether a handler exists at compile time, so a plugin that adds one command is
one function long:

```zig
const yf = @import("yapfast");

pub fn command(invocation: []const u8) ?[]const u8 {
    return "\"none\"";
}

comptime {
    yf.exports();
}
```

`yf.exports()` is not optional. A WebAssembly module's export section is written
by wasm-ld, not by the compiler: it needs both `@export(&f, .{ .name = "yf_activate" })`
and `--export=yf_activate` on the command line, and `export fn` alone produces a
module whose only export is `memory`. `build.sh` passes the five names; without
them the helper refuses the module with "doesn't export yf_activate, …".

## The PDK is a module of its own

Zig resolves `@import` inside the module it is building, so the PDK is added as
its own module and imported by name:

```sh
zig build-exe \
    -target wasm32-freestanding -O ReleaseSmall -fno-entry --export-memory \
    -femit-bin=plugin.wasm \
    --export=yf_activate --export=yf_deactivate --export=yf_event \
    --export=yf_hook --export=yf_command \
    --dep yapfast -Mroot=shout.zig -Myapfast=../../src/yapfast.zig
```

That is what `build.sh` runs. `-fno-entry` says there is no `_start`; nothing runs
but what Yapfast calls.

## Memory

No allocator is required by the PDK: a payload is copied into a static buffer, the
answer is built in another, and JSON is parsed with a fixed buffer over a third.
An instance's memory is capped by its manifest and never given back, so a plugin
that returns the bytes it was given is a plugin that stays alive longer. A plugin
that wants a heap can have one — `std.heap.wasm_allocator` — but the example
doesn't, and neither does the template.

## Host calls

`yf.call(method, payload)` is one call, answered with the ABI's `HostReturn`:
`{"ok": …}`, `"unsupported"`, `{"denied": {"reason": …}}`, `{"pending": {"tag": …}}`
or `{"error": {"message": …}}`. `yf.storageGet`/`yf.storageSet` and `yf.log` are
the two shortcuts worth having; anything else is `call` with a method name, which
is how a plugin reaches a method newer than the PDK it was built against without
failing to link.

A call can answer later, and `result` is where that answer arrives before the
events of the next batch. That is why an event handler has to handle its own
replies.

## Handles are opaque

The kernel's memory isn't addressable from a Zig slice. A handle comes back from
`alloc` and only `store_u64`, `store_u8`, `load_u64`, `load_u8` and `length` may
touch what it names: casting one to a pointer compiles, writes bytes the host
never reads, and every host call then answers nothing. The PDK marshals eight
bytes at a time through the kernel for that reason, and `src/yapfast.zig` says so
where a plugin author will see it.
