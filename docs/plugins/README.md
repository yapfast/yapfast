# Plugins

A plugin is a WebAssembly module and a manifest, `yapfast-plugin.json`, read
from a git repository and pinned there to one commit. Yapfast runs the module in
a helper process of its own, [`yapfast-plugins`](../../crates/yapfast-plugin-host),
and talks to it over a pipe of JSON frames. The helper holds no Discord token,
no Matrix session, no network stack and no filesystem; a guest asks Yapfast for
everything through one host function, and the parent answers it. What a plugin
may do is written in its manifest and agreed to at install, line by line:
[permissions](permissions.md).

Plugins are behind a Cargo feature, `plugins`, and the helper is a separate
binary, so a build without them links neither wasmtime nor `gix`.

## The lifecycle

| State | What exists |
| --- | --- |
| Installed | Files under `store/`, and a line in the lock file. Nothing in memory. |
| Enabled | One registry entry in the parent: the manifest's contributions, its permissions and its subscription bits. The helper needn't be running. |
| Active | An instance in the helper, with the pages of memory its manifest asked for. |
| Faulted | It trapped, ran out of memory or missed a deadline. Its instance is gone and it is re-instantiated on its next trigger. |
| Suspended | Three faults in a minute. It stays off until the person enables it again. |

```mermaid
stateDiagram-v2
    [*] --> Installed
    Installed --> Enabled: enabled
    Enabled --> Installed: disabled
    Enabled --> Active: first trigger
    Active --> Enabled: idle eviction
    Active --> Faulted: trap, out of memory or a deadline
    Faulted --> Active: next trigger
    Faulted --> Suspended: third fault in a minute
    Suspended --> Enabled: enabled by hand
```

Enabling a plugin adds its registry entry and checks its dependencies; it starts
no process. A plugin names what wakes it, `startup`, `command`, `menu` or
`event:message_create` and the like, and is instantiated when one of those
happens. A person waiting on the result is what forces the two exceptions: a
plugin that intercepts outgoing messages is activated at startup, since the first
message must not go out before it is ready, and so is any plugin whose manifest
says `startup`.

**The helper exists only while an instance does.** The parent starts it on the
first activation, and it exits when the last instance is evicted, so wasmtime's
compile threads and a guest's linear memory never outlive the plugins that
needed them. With nothing enabled there is no helper, no thread and no timer:
`AppState` carries no plugin state at all, and the tap points in the send path,
the event path and the menus cost one check each. A plugin that has been
installed but never triggered costs its registry entry.

An instance is evicted after five minutes with no calls, at once when the window
closes to the tray, and when the helper's memory budget is full. A plugin holding
a timer or an open request isn't idle. The contract that follows, and that a
plugin's author has to write to: an instance may vanish between any two calls,
so anything that must survive goes in storage.

A run started with `--no-plugins` reads no plugin and starts no helper, which is
how someone gets their window back when a plugin has made it unusable; the next
run without the flag has its plugins again, since nothing was uninstalled.

## What a plugin can add

Everything the interface shows before a plugin runs is in its manifest, so a
plugin that has never been activated still appears:

- commands in the composer's `/` list, with typed and required options;
- rows in a message, person, channel, server, role, emoji, sticker, image,
  stream, game, composer or attachment menu, with a closed set of `when`
  conditions and no expression language;
- settings the plugin's page draws for it, as strings, numbers, sliders,
  switches, choices and lists of records;
- named switches, `tweaks`, for parts of Yapfast that offer them.

Once it is running it can subscribe to events and answer with data to draw: text
and controls beside a message, or inside a slot the interface offers. The
interface is compiled Slint, so a plugin adds no widget; it sends a tree and
precompiled components draw it. Calling another plugin by name, and publishing to
whoever subscribed, are named by the API from its first version and answer
`unsupported` until the runtime has them.

What it can't do is become Yapfast. A guest gets no Discord or Matrix token, no
raw authenticated request, no direct socket and no filesystem; network access is
one gated HTTPS fetch to the hosts its manifest names, and nothing else. It can't
answer with a shape the ABI doesn't define, and a host method this build doesn't
have answers `unsupported`, so a plugin feature-detects instead of failing to
load.

## Where the files live

Everything is in the data directory (the one `--data-dir` names), under
`plugins/`:

```text
<data>/plugins/
  lock.json               what is installed, with its pinned commit and hashes
  grants.json             what the person allowed, per plugin and per host
  store/<sha256>/         content-addressed: plugin.wasm and manifest.json
  data/<id>/              the plugin's own storage.json and settings.json
  cache/                  wasmtime's compile cache, safe to delete
```

`store/` is content-addressed because a git object name is a SHA-1 of the commit,
not of the bytes that run: the lock file records the commit it was installed
from, the SHA-256 of the `.wasm` and the SHA-256 of the manifest it was reviewed
with, and the module is hashed before its path is handed to anything. The current
and previous version of each plugin stay in `store/`, so a rollback needs no
fetch. With no plugin enabled, none of it exists: Yapfast creates the directory
when it first installs a plugin and not before.

## The other pages

| Page | What it covers |
| --- | --- |
| [Manifest](manifest.md) | Every field of `yapfast-plugin.json`, its limits and what validation refuses |
| [ABI](abi.md) | The pipe frames, the guest's exports and imports, and the JSON each one carries |
| [Permissions](permissions.md) | Every capability and its sentence, the network gate, grants and revoking them |
| [The helper](host.md) | The helper's threads, lanes, deadlines, faults, eviction, cache and crashes |
| [Writing a plugin](pdk.md) | The Rust PDK, a plugin end to end, and the eviction contract |
| [Testing](testing.md) | The `.wat` fixtures, the helper and gate tests, and measuring memory |

## What isn't wired yet

The core is in place: the helper and its scheduler, the Extism runtime, the bridge
with its registry and store, the content-addressed store and the `fetch`
subcommand, permissions, grants and the network gate, the application's actions
and taps, the Rust PDK, and the fixtures and tests for all of it.

Four things are deliberately not there yet:

- **The interface.** No Plugins page, consent dialog, slot or widget exists. The
  Slint adapter has to call `Application::open_plugins` at startup and handle
  `Effect::Plugin` and `Event::Plugin` before a build with the feature on can
  compile — `src/ui.rs`'s `execute` match is exhaustive, so it refuses to build
  until then. The site is `src/ui/refresh.rs`'s `Reach` for a repaint.
- **A transport for the gate.** `plugins/network.rs` defines `Transport` and
  `Resolver` and leaves them to whoever owns the HTTP client. Until one is
  installed, `http.fetch` answers `unsupported`.
- **Chunked response bodies.** A body larger than one frame is refused rather
  than streamed to a file, because the answer has no field naming the file.
- **Matrix messages.** Selection, voice, focus and account-ready events are
  tapped where both networks pass; a Matrix message isn't tapped yet, so only
  Discord's gateway feeds `message_create`, `message_update` and
  `message_delete`.
