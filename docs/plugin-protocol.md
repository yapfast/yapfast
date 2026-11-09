# The plugin helper's wire protocol

The parent and the plugin helper (`yapfast-plugins`) are separate processes.
This is the M0 protocol between them: the framing, the limits, and the JSON
envelopes they exchange. The types live in
[`crates/yapfast-plugin-api`](../crates/yapfast-plugin-api).

M0 is two crates: `yapfast-plugin-api`, the types below, and
`yapfast-plugin-host`, the helper binary. Both are workspace members that are
not in `default-members`, so a normal build of the app does not compile them;
the helper's own `host` feature gates Extism and module loading. The app-side
`plugins` feature, which wires the parent into Yapfast, arrives with a later
milestone; M0 uses the protocol to load a module, call into it and unload it
while the plan's measurements are taken.

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

The length counts the payload only, not the header. The header is 13 bytes
(4 + 1 + 8). A whole frame is at most 1 MiB (1,048,576 bytes), so a payload is
at most 1,048,563 bytes.

Frame id 0 is reserved for `ready`. Every request carries a non-zero id, and
every response that answers one copies that id. `ready` is unsolicited: it has
no request to answer, so it is the only frame at id 0. A request at id 0 would
never be answered; an answer at id 0 would pair with nothing. Ids are chosen by
the parent and may repeat once their exchange is done.

An unknown kind is a protocol error, and fatal in this M0 version. The header
still states the payload's length, so the frame could physically be skipped;
the receiver does not skip it, because a kind it does not know means the peer
speaks a version whose frames it would misinterpret. Refusing it is policy, not
an inability to find the frame's end. Any protocol error closes the stream:
both sides stop instead of guessing.

## Kinds

The discriminants are the wire format and must not change. Adding a kind takes
the next free byte.

| Byte | Kind | Direction | Meaning |
| --- | --- | --- | --- |
| 1 | `ready` | helper → parent | The helper is up and takes requests |
| 2 | `load` | parent → helper | Instantiate a module |
| 3 | `loaded` | helper → parent | The module is instantiated |
| 4 | `call` | parent → helper | Call an export on a loaded plugin |
| 5 | `reply` | helper → parent | The export's answer |
| 6 | `unload` | parent → helper | Drop an instance |
| 7 | `unloaded` | helper → parent | The instance is gone |
| 8 | `shutdown` | parent → helper | The helper should exit |
| 9 | `error` | either → other | A request that could not be served |

`shutdown` has no success answer: the helper closes the stream and exits. Its
payload is still the JSON envelope `{"type": "shutdown"}`; a bare frame with no
envelope is not a shutdown.

`error` is a response envelope in either direction: the helper reports a call it
could not answer, and the parent reports a request it could not serve. It is
never a request, so it carries the id of the exchange that failed.

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
refused. In code a call's or reply's payload is a `Payload`, which holds the
bytes: decoding a frame fills it, and `to_frame` writes those same bytes back.

### Requests

```json
{"type": "load", "plugin": "echo", "module": "C:\\plugins\\echo.wasm"}
{"type": "call", "plugin": "echo", "method": "shout", "payload": {"text": "hi"}}
{"type": "unload", "plugin": "echo"}
{"type": "shutdown"}
```

`module` is always an absolute path: the helper does not resolve paths of its
own, so the file the parent checked is the file the helper loads.

`method` is the guest export M0 calls, and `payload` is that export's argument
bytes. M0 has no guest-side method envelope: one call is one export call. Later
milestones add `yf_call`, whose method is a name inside the payload; that is a
different thing from this field, and the plan's `api` changes when it lands.

`payload` is the call's JSON, forwarded verbatim. Decoding a frame and writing
it out again must reproduce the payload's bytes exactly: key order, whitespace,
exponent spelling and integers past `u64` included. It is not reserialised
through `serde_json::Value`, which would reorder keys and rewrite numbers. A
payload built in code from a `Value` has no source bytes, so only its meaning is
claimed for it.

A call without `payload` reads as `null`.

### Responses

```json
{"type": "ready"}
{"type": "loaded"}
{"type": "reply", "payload": {"text": "HI"}}
{"type": "unloaded"}
{"type": "error", "message": "the module trapped"}
```

`reply.payload` is the export's answer JSON, verbatim, with the same byte rule
as a call's payload; an export that answers nothing uses `null`.
`error.message` is one sentence for the other side's log and whatever the user
is shown; it is not parsed.

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
| `ProtocolError::ReservedId` | A request, or a response other than `ready`, used id 0 |
| `ProtocolError::ReadyWithId` | A `ready` used an id other than 0 |
| `ProtocolError::KindMismatch` | The frame's kind and its JSON's `type` disagree |
| `ProtocolError::Frame` | The envelope does not fit a frame |

## Not here yet

M0 has no version field and no handshake beyond `ready`. Extism, permissions,
the HTTP proxy and guest imports arrive in later milestones; the guest ABI
(`yf_activate`, `yf_call` and the rest) is described in
[the plan](../PLAN.md#102-wire-format-and-abi).
