# The plugin helper's wire protocol

The parent and the plugin helper (`yapfast-plugins`) are separate processes.
This is the M0 protocol between them: the framing, the limits, and the JSON
envelopes they exchange. The types live in
[`crates/yapfast-plugin-api`](../crates/yapfast-plugin-api).

Until M8, the code sits behind the `plugins` feature, which is off by default.
M0 uses the protocol to load a module, call into it and unload it, while the
plan's measurements are taken.

## Transport

The helper reads requests on stdin and writes responses on stdout. One frame
follows another with nothing between them. A frame is a 13-byte header and a
JSON payload:

| Offset | Bytes | Field | Encoding |
| --- | --- | --- | --- |
| 0 | 4 | payload length | `u32`, little endian |
| 4 | 1 | kind | `u8`, see the kinds below |
| 5 | 8 | id | `u64`, little endian |
| 13 | *length* | payload | UTF-8 JSON |

The length counts the payload only, not the header. A whole frame is at most
1 MiB (1,048,576 bytes), so a payload is at most 1,048,563 bytes.

The id pairs a response with its request: the helper copies the request's id
into its answer. `ready` is unsolicited and uses 0. Ids are chosen by the
parent and may repeat once their exchange is done.

An unknown kind is a protocol error: the receiver cannot tell where its
payload ends, so the stream cannot be resynchronised. Fatal errors end the
stream; both sides exit instead of guessing.

## Kinds

The discriminants are the wire format and must not change. Adding a kind takes
the next free byte.

| Byte | Kind | Direction | Meaning |
| --- | --- | --- | --- |
| 1 | `ready` | helper → parent | The helper is up and takes requests |
| 2 | `load` | parent → helper | Instantiate a module |
| 3 | `loaded` | helper → parent | The module is instantiated |
| 4 | `call` | parent → helper | Call a method on a loaded plugin |
| 5 | `reply` | helper → parent | The method's answer |
| 6 | `unload` | parent → helper | Drop an instance |
| 7 | `unloaded` | helper → parent | The instance is gone |
| 8 | `shutdown` | parent → helper | The helper should exit |
| 9 | `error` | either → other | A request that could not be served |

`shutdown` has no success answer: the helper closes the stream and exits.

## Reading and writing

A frame's payload buffer is allocated only after its declared length has been
checked against the limit, so a peer cannot make the other side reserve memory
it would then refuse. A refused frame leaves the stream in an unknown state.

End of stream before a frame's first byte is a clean stop, as when the parent
closes stdin to let the helper finish. End of stream after some of a header or
payload is `UnexpectedEof`: the peer died mid-frame, and the receiver reports
that rather than a clean stop.

Every write flushes. A pipe has no autoflush, and without the flush a written
frame would sit in the writer while the other process waits for it.

## Envelopes

The payload is JSON with a `type` tag naming its envelope. Requests and
responses are distinct enums, so a frame's kind must agree with its payload's
`type`; a frame whose kind names a request but whose JSON names a response is
refused.

### Requests

```json
{"type": "load", "plugin": "echo", "module": "C:\\plugins\\echo.wasm"}
{"type": "call", "plugin": "echo", "method": "shout", "payload": {"text": "hi"}}
{"type": "unload", "plugin": "echo"}
{"type": "shutdown"}
```

`module` is always an absolute path: the helper does not resolve paths of its
own, so the file the parent checked is the file the helper loads.

`payload` is the call's JSON, passed to the plugin verbatim. A call without one
reads as `null`.

### Responses

```json
{"type": "ready"}
{"type": "loaded"}
{"type": "reply", "payload": {"text": "HI"}}
{"type": "unloaded"}
{"type": "error", "message": "the module trapped"}
```

`reply.payload` is the plugin's JSON answer, verbatim; a method that answers
nothing uses `null`. `error.message` is one sentence for the parent's log and
whatever the user is shown; it is not parsed.

## Errors

Encoding, decoding and framing fail with one of three errors:

| Error | Cause |
| --- | --- |
| `FrameError::TooLarge` | The payload would make the frame larger than 1 MiB |
| `ReadError::FrameTooLarge` | A header declared a payload past the limit |
| `ReadError::UnknownKind` | The kind byte names no kind |
| `ReadError::UnexpectedEof` | The stream ended mid-header or mid-payload |
| `ReadError::Io` | The reader itself failed |
| `ProtocolError::Json` | The payload is not the JSON its kind promises |
| `ProtocolError::RelativeModulePath` | A `load` named a relative module |
| `ProtocolError::NotRequest` / `NotResponse` | The frame's kind is the other direction |
| `ProtocolError::KindMismatch` | The frame's kind and its JSON's `type` disagree |
| `ProtocolError::Frame` | The envelope does not fit a frame |

## Not here yet

M0 has no version field and no handshake beyond `ready`. Extism, permissions,
the HTTP proxy and guest imports arrive in later milestones; the guest ABI
(`yf_activate`, `yf_call` and the rest) is described in
[the plan](../PLAN.md#102-wire-format-and-abi).
