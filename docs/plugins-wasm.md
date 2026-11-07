# Plugins: WebAssembly sandbox (follow-up research)

Status: research only. v1 (`plugins.md`) runs bundled plugins in-process
behind capability checks. This doc records what a WASM sandbox would add
and what it must decide before privileged native plugins are allowed.

## Why

In-process plugins share the app's memory, credentials and network stack.
Capability checks stop a plugin from *calling* an operation it did not
declare, but a native plugin can still misbehave in ways checks cannot
see: read memory it should not, crash the window, or hang the event loop.
A WASM sandbox would run third-party plugins in a linear memory the host
controls, with every import going through the same `Host` capability
checks — so the manifest and enforcement model from v1 carries over, and
the privileged operations (`credentials`, `filesystem`, `network`,
`process`, `discord_api`) could be implemented as mediated imports instead
of `UnavailableInV1`.

## Candidates

| Runtime | Notes |
|---|---|
| `wasmtime` | Mature, component-model support, fuel-based metering. Heaviest dependency; check binary size and startup cost against `docs/measurements/README.md`. |
| `wasmer` | Similar feature set, different JIT/AOT story. |
| `wasmi` / `wasmer` interpreted modes | No JIT: slower per instruction, much simpler to ship, deterministic metering. Likely enough for notification filters and small UI contributions. |
| `boa` / JS engines | Wrong abstraction: plugins are Rust/native, not scripts. Skip. |

Recommendation for a first spike: a small interpreted runtime behind the
existing `Plugin` trait, so the manager, manifest validation and host
checks are reused unchanged and only the plugin body changes.

## Decisions to make

1. **ABI.** A JSON-shaped message protocol over the existing hooks
   (`filter_notification`, `theme_contributions`, `compact_density`), or a
   component-model interface? JSON is debuggable and matches the config
   format; the component model is more typed. Spike both, prefer JSON.
2. **Metering.** Fuel per hook call, so a looping plugin cannot hang the
   window. Pick a budget from a filter-plugin baseline measurement.
3. **Imports.** The host exposes exactly the v1 `Host` methods; privileged
   imports exist only when the manifest declares them *and* the person
   opts in, with per-call prompts for `credentials` and `process`.
4. **Filesystem.** WASI preopens scoped to a per-plugin data directory;
   no ambient access, ever.
5. **Packaging.** Plugins as `.wasm` files with the manifest embedded or
   beside them, installed into the data directory and listed in
   `layout.json` like bundled ones. Signature verification is out of scope
   for the spike; the sandbox, not the signature, is the boundary.
6. **Migration.** Bundled plugins stay native (they are trusted and need
   the fast path); third-party plugins load as WASM. The `PluginManager`
   then holds both behind one trait.

## Exit criteria

- A sample filter plugin written in Rust, compiled to WASM, runs through
  the same manager and passes the same capability tests as v1.
- A plugin that tries an undeclared import fails closed, with no host
  state changed.
- Startup and per-notification overhead measured and recorded in
  `docs/measurements/`.
