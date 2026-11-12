# Plugins in AssemblyScript

A plugin is a WebAssembly module Extism runs, and the ABI between it and Yapfast
is five exports and two imports around JSON. This folder is the AssemblyScript
side of it: no WASI, and one dev dependency (`assemblyscript`) for the compiler.

**This is the PDK a new plugin should use.** It is the cheapest language
measured — one page (64 KiB) of linear memory and an 11.4 KiB module, against
the Rust PDK's 17 pages and 160.8 KiB — and it builds in a second with nothing
but `npm`. The comparisons are in
[the measurements](../docs/measurements/README.md#the-other-collected-languages-measured-the-same-way),
and the recommendation is in [writing a plugin](../docs/plugins/pdk.md).

| File | Is |
| --- | --- |
| `assembly/index.ts` | The PDK: the kernel's imports, marshalling, host calls, the `Plugin` base class and the handlers it reaches |
| `assembly/json.ts` | Reading a payload's fields, and writing an answer's |
| `template` | **Start here:** the plugin to copy, `/hello`, with its manifest, build script and README |
| `examples/shout` | A whole plugin: one command, one host call, no state |

```sh
npm install
cd examples/shout && ./build.sh && ls -l plugin.wasm
```

## What a plugin writes

A plugin extends `Plugin`, writes the handlers it needs, registers one instance,
and names the five exports — AssemblyScript exports the functions of the file
`asc` is pointed at, and not those of a library it imports, so without the
`export function` lines the module has one export and the helper refuses it.

```ts
import { Plugin, register, pdkActivate, pdkCommand /* … */ } from "../../assembly/index";

class Ping extends Plugin {
  command(_invocation: string): string {
    return '{"notice":{"text":"pong"}}';
  }
}

register(new Ping());

export function yf_activate(): void { pdkActivate(); }
// …and the other four
```

`build.sh` passes `--baseDir` as the PDK's folder so both the plugin and the PDK
are inside the project `asc` compiles.

## Memory

AssemblyScript has a heap and a garbage collector, so this is the one of the four
PDKs that carries a runtime: the example is 17.7 KiB against C's 11.9 KiB. An
instance's memory is capped by its manifest and never given back, so a plugin
that touches less memory holds its instance for longer — and a plugin written
here should say so in its manifest if it means to hold a lot.

## Host calls

`call(method, payload)` is one call, answered with the ABI's `HostReturn`:
`{"ok": …}`, `"unsupported"`, `{"denied": {"reason": …}}`, `{"pending": {"tag": …}}`
or `{"error": {"message": …}}`. `storageGet`, `storageSet` and `log` are the
shortcuts; everything else is `call` with a method name, which is how a plugin
reaches a method newer than the PDK.

A call can answer later, and `result` is where that answer arrives, in the batch
before the events it belongs with.

## Handles are opaque

A handle names memory Extism keeps for the guest. Only the kernel's `store_u8`,
`load_u8` and `length` may touch what it names: an `ArrayBuffer` over a handle's
offset compiles, reads zeroes, and every host call then answers nothing. The PDK
copies through the kernel for that reason.

## These are AssemblyScript, not TypeScript

`assembly/*.ts` uses `@external("extism:host/env", "alloc") declare function …`,
which is AssemblyScript's syntax and invalid TypeScript. `tsconfig.json` here
lists no files for that reason: an editor that tries to check them as TypeScript
reports a dozen errors that are not errors.
