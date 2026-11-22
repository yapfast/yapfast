# MCP Bridge, an example plugin (design)

An MCP server inside Yapfast, so a local MCP client — Claude Desktop, Claude
Code, or anything else that speaks the Model Context Protocol — can read the
conversations you can see and, if you allow it, send messages for you. This
folder holds the manifest the plugin would ship with and the design; the module
itself is the next step, written against the Rust PDK in `pdk/rust`.

## What it is made of

| Piece | Where it comes from |
| --- | --- |
| The endpoint | `net.listen` on `127.0.0.1:8765` (`listen-port`), behind the token on the plugin's page |
| Listing places | `rooms.list`, `rooms.get` (`servers.read`) |
| Reading | `messages.history`, `messages.get` (`messages.read`) |
| Sending | `messages.send` with `messages.send.agent`, at six a minute |

Every one of those is a capability the install review lists; `net.listen` and
`messages.send.agent` are powerful, so they are said in red, need *Allow
powerful plugins* in Advanced, and each asks once more the first time it is
used ([permissions](../../docs/plugins/permissions.md#the-powerful-tier)).

## The protocol

MCP's Streamable HTTP transport without server-sent events: every JSON-RPC
request is one `POST /mcp`, answered with one JSON body, which is exactly what
`net.listen` hands a plugin. The plugin's `yf_activate` calls `net.listen` once;
each request arrives as another answer to that call:

```json
{ "request": 3, "method": "POST", "path": "/mcp",
  "headers": [["content-type", "application/json"]],
  "body_base64": "eyJqc29ucnBjIjoiMi4wIiwiaWQiOjEsIm1ldGhvZCI6InRvb2xzL2xpc3QifQ==" }
```

The plugin decodes the JSON-RPC, does the work, and answers with
`net.respond { request: 3, text: "{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":…}" }`.
A request that needs a host call that answers later — `messages.send` waits
for the application — is answered from the `yf_event` that carries that
result, by the request number the plugin kept. Yapfast answers 504 for it if
nothing comes in 30 seconds.

`initialize` answers the server's name and the `tools` capability, and
`tools/list` names four tools:

| Tool | Arguments | Does |
| --- | --- | --- |
| `list_conversations` | `limit?` | `rooms.list`: id, name, server, whether a DM |
| `read_messages` | `room`, `before?`, `limit?` (≤ 100) | `messages.history` of the conversation on screen; any other answers that Yapfast keeps the open one, so the tool says to open it first |
| `get_message` | `room`, `message` | `messages.get` |
| `send_message` | `room`, `body` | `messages.send` as an agent |

## Connecting Claude

The plugin's page shows *Listening on http://127.0.0.1:8765/* and *Copy token*.
In an MCP client's configuration:

```json
{ "mcpServers": { "yapfast": {
    "type": "http",
    "url": "http://127.0.0.1:8765/mcp",
    "headers": { "Authorization": "Bearer yf_…" } } } }
```

## What keeps it honest

- Only this computer can connect: the endpoint binds loopback and nothing else,
  and a `Host` that isn't loopback is refused, so a web page can't drive it
  through a browser.
- Nothing connects without the token, which is shown only on the plugin's page
  and changes when `net.listen` is revoked.
- Every send is the agent's: at six a minute and five in any ten seconds, with
  the red strip over the conversation for as long as it sends and **Stop** on it,
  and *Sent by MCP Bridge, a plugin, not typed by you* under each message.
- It reads what Yapfast shows, never Discord directly: no token, no request of
  its own to Discord, and only the open conversation's history, as every
  plugin reads.
