# Ping

The first Yapfast plugin: one command, no capability, and nothing but an answer.

```sh
cargo build --release --target wasm32-unknown-unknown
cp target/wasm32-unknown-unknown/release/ping.wasm plugin.wasm
```

`plugin.wasm` beside `yapfast-plugin.json` is what an install copies into the
store. With the feature on, put this folder in the data directory's plugin store
and enable `com.example.ping`; the helper is started by the first `/ping` and
exits when the last instance goes.

## What it does

`/ping` answers `pong`, in the room it was typed in and only to the person who
typed it. That is a command's answer, not a message: `Reply::notice` is a line
the person reads, so the plugin needs no capability at all, and nothing leaves
the machine.

The notice is worth contrasting with a message, because the difference is the
scope of what a plugin can do here today:

| What it wants to do | Today |
| --- | --- |
| Answer a command | Works, and is what this plugin does |
| Send a message into a room | `messages.send` is answered `unsupported` in this build; the API names it and the runtime has no handler |
| Draw a row in a context menu | The manifest declares it (below); nothing dispatches it yet |

## The context-menu declaration

`yapfast-plugin.json` carries a `menus` entry:

```json
{ "target": "message", "label": "Ping this message", "action": "ping-message" }
```

That is the shape the API is built around, and it is deliberately a manifest
field rather than code: the interface can show the row without loading any
WebAssembly, which is what keeps the helper process out of every menu that has
one row in it. What is missing is the interface half — nothing in this build
consumes `MenuDecl`, so the row is declared and not yet drawn, and no action
reaches the plugin. The `menu` handler is where it will answer when it does.

This plugin declares the row anyway on purpose: the first plugin is a good place
to have the thing that doesn't work yet written down in the form it will work in,
rather than discovered later.

## The manifest, field by field

| Field | Why |
| --- | --- |
| `activation.on: ["command", "menu"]` | Two ways to wake it: typing `/ping`, and the menu row once it is dispatched |
| `permissions.capabilities: []` | It sends nothing and reads nothing; an empty list is a promise the review dialog can show |
| `commands` | The name and description the `/` list draws, with no options |
| `menus` | The context-menu row, above |
