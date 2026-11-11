# The ABI

There are two boundaries. Yapfast and the helper speak a frame protocol over the
helper's stdin and stdout; the helper and a guest speak JSON through five exports
and two imports of the ABI's own, on top of the ones Extism supplies to a guest.
Both boundaries are described by
[`crates/yapfast-plugin-api`](../../crates/yapfast-plugin-api), which carries
`serde` and nothing else, because both processes build it.

## Frames on the pipe

One frame is a fixed header and a payload:

```text
u32 payload length (little-endian)
u8  kind
u64 id (little-endian)
payload bytes
```

The header is 13 bytes and the payload is JSON, forwarded verbatim: neither side
decodes a message it is only relaying, so a payload costs one copy and not a
parse and a re-serialize. `id` pairs a request with its answer, in either
direction, and is 0 when nothing pairs with it; events and log lines carry 0.
The length counts the payload alone. A frame is at most 1 MiB, and a length above
that is a broken stream rather than a big message, so reading it fails instead of
allocating. Writing one larger is refused rather than truncated, since the peer
would read a short frame and lose the stream. A pipe closed between frames is a
clean end, which is how the helper is told to exit; a pipe closed inside a frame
is an error, and so is a kind this build doesn't know.

| Kind | Value | Carries |
| --- | --- | --- |
| `Hello` | 1 | The first frame either way: the API this side speaks |
| `Request` | 2 | Something the other side answers with `Reply` |
| `Reply` | 3 | The answer to a `Request`, or to a guest's host call |
| `Event` | 4 | A batch of events for a guest, carrying no answer |
| `Report` | 5 | What the helper says as it happens: status, fault, exit |
| `Log` | 6 | One line of a plugin's log, ring-buffered by the parent |
| `Bye` | 7 | The helper is going away, and why |

### Yapfast to the helper

Every payload is a JSON object with a `t` naming the variant. The first is
`Hello`, and the helper answers `Ready` with its own version.

| `t` | Fields | Meaning |
| --- | --- | --- |
| `hello` | `api` | Which API the parent speaks |
| `load` | `plugin`, `wasm`, `memory_pages`, `wasi` | Load a module and its limits, without activating it |
| `unload` | `plugin` | Drop an instance, keeping nothing |
| `activate` | `plugin`, `load` | Load the module when `load` is there, then instantiate and call `yf_activate`: one request for the whole phase, so the helper is never between a module and its instance. A `load` that replaces a running version carries the hand-off its `yf_deactivate` left, which is how a plugin's state crosses an update |
| `deactivate` | `plugin` | Call `yf_deactivate`, which may hand state to the next activation |
| `kick` | `plugin` | Run an instance again with nothing new: an eviction clock's trigger |
| `event` | `plugin`, `events` | A batch of events, already filtered and serialized |
| `hook` | `chain`, `request` | One hook, for the whole chain at once |
| `command` | `plugin`, `command` | A command the person typed, with its options |
| `invoke` | `plugin`, `function`, `args_base64` | One export of one plugin, called for another: `plugin.call`. Answered with `invoked` |
| `return` | `plugin`, `call`, `answer` | The answer to a guest's host call |
| `evict` | `plugin` | Evict now: the idle clock ran out or the memory budget is full |
| `shutdown` | | The parent is going away; exit |

A hook's `chain` is the plugin ids in activation order, dependencies first and
then by id, so the whole chain is one exchange with the parent rather than one
round trip per plugin. Each plugin in it sees the previous one's subject.

The helper runs that chain one plugin at a time, and gives each its own
deadline: 25 ms for `message_pre_send`, and the chain as a whole 100 ms. A
plugin that runs past its own is cancelled, thrown away and left out of the
answer with a `deadline` fault against it, and the chain goes on with the
subject as it was. `by` names the plugin whose answer the chain ended with, so
the parent can ask what its manifest allows before it acts on a `block`.

A hook's deadlines are the parent's as well as the helper's: it starts the
clock when the person pressed send, so the helper's start, the hello and the
first compile of a module all come out of the same 100 ms. A chain that has
spent its budget before the helper has it is answered unchanged rather than
given a fresh one.

### The helper to Yapfast

| `t` | Fields | Meaning |
| --- | --- | --- |
| `ready` | `api`, `version` | The answer to `hello` |
| `call` | `plugin`, `call`, `request` | A guest wants something from Yapfast: its own number for the call, and the method with its payload. The answer is a `return` frame with that number |
| `activated` | `plugin`, `state`, `memory_pages` | The instance is up, with what the last `yf_deactivate` handed on |
| `deactivated` | `plugin`, `state` | The instance is gone, with its hand-off |
| `hook_done` | `outcome`, `failures`, `by` | A chain's final answer, the plugins that failed on the way, and the plugin whose answer it is |
| `command_done` | `plugin`, `outcome` | A command's answer |
| `invoked` | `answer_base64`, `fault` | The export's answer, or the fault that stopped it |
| `event_done` | `plugin` | An event batch finished |
| `status` | `plugin`, `status` | A plugin's state changed, for the page and the registry |
| `fault` | `plugin`, `fault` | An instance was discarded, and why |
| `log` | `plugin`, `record` | One line of a plugin's log |
| `bye` | `reason` | The helper is exiting, and why |

`status` is one of `installed`, `enabled`, `active`,
`faulted { reason, memory_pages }` and `suspended { reason }`. A fault names its
kind, one of `trap`, `out_of_memory`, `deadline`, `bad_module`,
`activate_failed` and `bad_answer`, and carries a sentence a person can read:
`ran out of its 16 MiB`. A log record is its level, its target, its message and
the milliseconds since the Unix epoch.

## The guest's side

The guest ABI is deliberately small, so growing the API never changes the link
surface. The method is a name inside the payload, an unknown method answers
`unsupported`, and `api` in the manifest changes only for a breaking change.

| Direction | Name | Purpose |
| --- | --- | --- |
| Guest export | `yf_activate` | Start, given [`ActivatePayload`](#the-payloads) |
| | `yf_deactivate` | Stop, and hand state to the next activation |
| | `yf_event` | A batch of events and of finished host calls |
| | `yf_hook` | A hook, for a plugin that intercepts |
| | `yf_command` | A command the person typed |
| Host import | `yf_call(request) -> answer` | Ask Yapfast something, and get its answer |
| | `yf_post(message)` | Say something and don't wait |

A guest that doesn't export one of those five is a `bad_module` fault at
activation, so a PDK that forgets `yf_event` fails early and clearly rather than
on the first message. `yf_decorate` and `yf_ui_event` are for drawing beside a
row, which is the interface half of the API and not in this build; a module that
declares them is left alone.

### The payloads

Every one is JSON, because every PDK language has it and a payload can be read in
a log.

| Call | Argument | Answer |
| --- | --- | --- |
| `yf_activate` | `ActivatePayload { api, plugin, state }`: the API the helper speaks, the id, and what the last `yf_deactivate` handed on | Nothing |
| `yf_deactivate` | Nothing | `DeactivatePayload { state }`, or `null` for none. Cut to 256 KiB |
| `yf_event` | `EventBatch { events, results }` | Nothing |
| `yf_hook` | `HookRequest { hook, subject }` | `HookOutcome` |
| `yf_command` | `CommandInvocation { name, options }` | `CommandOutcome` |
| `yf_call` | `HostCall { method, payload }` | `HostReturn` |
| `yf_post` | `HostCall` | Nothing |

An event batch arrives as:

```json
{
  "events": [
    {
      "kind": "message_create",
      "network": "discord",
      "at": 1767225600000,
      "room": "room-1",
      "author": "person-1",
      "own": false,
      "payload": { "body": "hi", "id": "message-1" }
    }
  ],
  "results": [{ "call": 7, "answer": { "ok": { "status": 200 } } }],
  "topics": [
    {
      "topic": "word-filter.words",
      "from": "com.example.base",
      "at": 1767225600123,
      "payload": { "words": ["spam"] }
    }
  ]
}
```

`topics` is there only when something was published to a topic the plugin
subscribed to; a batch with none has no field at all. Each entry is one
`plugin.publish`, and a subscriber handles them after the batch's events.

The top-level fields are what a subscription filter reads, so the parent filters
before the helper is woken and a plugin that didn't subscribe is never sent a
payload. A field over 64 KiB is cut and flagged rather than the event being
dropped: the payload becomes `{ "truncated": true, "bytes": 65540, "text": "…" }`,
so a plugin still sees that its message was long. Typing, presence and speaking
are opt-in and coalesced, so a burst is one call with several events rather than
one call each.

A hook's subject is a draft about to be sent, a file on its way up, a typing
indicator about to go, or an activity about to be published. The answer is
`unchanged`, `changed` with the new subject, which the next plugin in the chain
sees, or `block` with a reason. A `block` stops the message only from a plugin
whose manifest says `"on_failure": "block"`, which needs `messages.intercept`;
from any other it is the fail-open answer and the message goes as it was.

```json
{ "hook": "message_pre_send",
  "subject": { "draft": { "room": "room-1", "body": "hi", "attachments": [] } } }
{ "hook": "typing_pre_send",
  "subject": { "typing": { "room": "room-1" } } }
{ "hook": "attachment_pre_upload",
  "subject": { "attachment": { "room": "room-1", "name": "cat.JPG", "bytes": 2048,
                               "content_type": "image/jpeg" } } }
```

An indicator has nothing to change, so `changed` is refused for it: a plugin
either lets it go or, with `typing.intercept`, withholds it with `block`. A file
is answered by `changed` with the `attachment` it should be uploaded as — a new
`name`, and a `content_type` or a byte count it may narrow but not raise — or by
`block` with the reason the person sees where the file would have gone. Both
chains answer under a deadline of their own, so the composer never waits longer
than the plan's table for one.

A command's answer is `none`, `notice` with a line for the person to see in the
room they typed it in, `value` with the plugin's own JSON, or `failed` with a
message.

A settings change arrives as the answer to the `settings.on_change` call that
asked for it: the keys that changed, with the values they hold now, and the
input of a widget one of its settings draws when that was what the person used.

```json
{ "changed": { "mode": "hide" } }
{ "widget": { "setting": "preview", "id": "reset", "value": "clicked" } }
```

### Host calls

`yf_call` takes a method name and a payload, and its answer is one of:

| Answer | Meaning |
| --- | --- |
| `{ "ok": … }` | The method ran, and the payload is the method's own answer |
| `"unsupported"` | This build doesn't have this method, so a plugin can feature-detect |
| `{ "denied": { "reason": … } }` | Yapfast refused: a missing capability, a host not granted, or a cap reached |
| `{ "pending": { "tag": … } }` | The answer is coming as a later event tagged with that value |
| `{ "error": { "message": … } }` | The call was understood and failed |

The methods are names, so adding one never changes the link surface. `storage`,
`settings`, `http`, `timers`, `app`, `account`, the `rooms`, `messages` and
`people` reads, the `ui` calls and the `plugin` group have typed payloads in
the contract crate, because they are the calls the runtime answers. Every
other name the ABI lists —
`messages.send`, `edit`, `delete`, `react` and `jump_to`, the `composer` calls,
`notify.send`, `clipboard.write` and `open.url` — is named from the first version
so a plugin can feature-detect it, and answers `unsupported` until the runtime
has it.

### Calls between plugins

```json
{ "method": "plugin.call",
  "payload": { "plugin": "com.example.base", "function": "lookup",
               "args_base64": "aGk=" } }
```

A call runs one export of the named plugin and answers with what it returned:
`{ "ok": { "answer_base64": "…" } }`, `{ "ok": null }` when the export returned
nothing, or `{ "error": { "message": … } }` for a fault or a deadline of two
seconds. The bytes are the two plugins' own — the ABI only carries them — so a
pair that agreed on JSON, MessagePack or a single string all work. The callee is
started first if it was idle, and an export the ABI reserves (`yf_activate` and
the other four) can't be reached this way: the parent owns those phases.

The caller must declare the callee as a `dependencies` or
`optional_dependencies` entry, and no other call is routed. That is what makes a
loop of calls impossible; a call that would come back around anyway, such as a
callee calling its own caller, is refused with the chain that would have made the
loop (`a → b → a`).

`plugin.publish` and `plugin.subscribe` are the other direction: asynchronous,
and never waited on.

```json
{ "method": "plugin.publish",
  "payload": { "topic": "word-filter.words", "payload": { "words": ["spam"] } } }
{ "method": "plugin.subscribe", "payload": { "topic": "word-filter.words" } }
{ "method": "plugin.subscribe",
  "payload": { "topic": "word-filter.words", "cancel": true } }
```

A publish answers at once with the number of plugins it was handed to. It is
delivered in the subscriber's next `yf_event` batch, under `topics`, in the same
quarter-second window typing waits in, and the receiver pays one event batch's
budget for it. A publisher never receives its own topic. A topic's name is 1 to
128 bytes and a plugin may hold 32 of them; the payload is cut and flagged at
64 KiB as an event's is.

`settings.get` answers one of the plugin's settings as the person set it, and
`settings.on_change` is how a plugin hears about a change while it runs: the
call is answered `pending`, and every change after it arrives as the answer to
that call, in the guest's next event batch, so the plugin sees it without being
restarted. A plugin that isn't running isn't woken for one: it reads its values
with `settings.get` when it next starts.

A read answers from what the application already knows, never by asking a
network: `app.version`, `app.locale` and `app.theme_mode` give one object;
`account.me` the account signed in; `rooms.selected`, `rooms.get` and
`rooms.list` the sidebar's own list; `messages.get` and `messages.history` the
conversation on screen, because that is the timeline Yapfast keeps; and
`people.get` and `people.member` the account and the open room's members. A
question about a room that isn't open is answered with an error saying so, and
`rooms.selected` names the one that is. Each read needs the capability
[`permissions.md`](permissions.md) gives it, and one answer is one frame: a page
stops at 1 MiB rather than being cut.

The `ui` calls take a widget tree, the rows of cells
[`interface.md`](interface.md#widget-trees) describes, as JSON:

| Method | Payload | Answer |
| --- | --- | --- |
| `ui.set_slot` | `{ "slot": "banner", "tree": { … } }` | `{ "drawn": true }`, or `{ "drawn": false, "reason": … }` for a slot this build has no place for, a tree over its cap, or one that isn't a tree |
| `ui.clear_slot` | `{ "slot": "banner" }` | `{ "drawn": true }` |
| `ui.notice` | `{ "text": "…" }`, markdown | `{ "drawn": true }`; an empty line is an error |
| `ui.toast` | `{ "text": "…" }`, plain text | `{ "drawn": true }` |
| `ui.open_dialog` | `{ "tree": { … } }` | `{ "drawn": true }`; a second dialog from one plugin replaces the first |

A drawing doesn't wait for the interface: the call is answered as soon as the
tree is checked, and what reaches the screen is coalesced to a frame and capped
at ten a second per plugin. The slots a plugin may name are `banner`, `toast`,
`composer`, `message.actions` and `dialog`; `message` answers that what a plugin
draws under a message is a `message.decorate` answer, not a slot. None of them
needs a capability: §9.1 of the plan gives every plugin its own slots.

Every host call is a request the parent answers, because the parent holds the
token, the network gate and the consent dialogs, and the guest's function is
synchronous while a person may need to be asked something. So the guest's
function never waits for a person: it gets `pending` and returns.

### How a pending call finishes

1. The guest calls `yf_call` with, say, `http.fetch` and a body it wants posted.
2. The answer is `{ "pending": { "tag": "lookup" } }`. The host function also
   gives the call its numeric id, which the plugin keeps if it cares to.
3. The helper sends `call` to the parent and the guest returns from its function.
   Its worker is free, and nothing is held open while a dialog is up or a request
   is in flight.
4. The parent performs the request through the network gate, or asks the person,
   and sends `return` with that call id and the answer.
5. The helper passes the answer to the instance, and the guest's next `yf_event`
   call carries it in `results` as `{ "call": 7, "answer": … }`.

That is why an event function has to handle its own replies, and why a plugin
that wants to look synchronous uses a PDK that wraps this in futures. A call
answered later still counts as work: an instance with one in flight is not idle,
and is not evicted under it.

## How the ABI changes

A change in
[`yapfast-plugin-api`](../../crates/yapfast-plugin-api) is a change to the plugin
ABI, so it follows two rules.

Additive changes keep `API_VERSION`, which is 1. A new host method is a name in
a payload, a new event kind is a value in `kind`, and a new optional field is one
the other side may not send; none of them changes what a written plugin links
against, and a plugin that doesn't know one either ignores it or gets
`unsupported`. Anything that breaks a written plugin, such as changing what an
existing payload means or removing a field, raises the version. A manifest that
asks for a higher version than this build speaks is refused with both numbers,
rather than loaded and left to fail at instantiation.

The frame protocol has no version of its own: the parent and the helper ship in
one build and are started together, so the two sides of that pipe always agree.
`api` versions the guest ABI, which is the boundary a plugin written by somebody
else crosses.

Bodies that aren't text cross the pipe as base64, standard alphabet with padding.
It costs a third more than the bytes themselves, which is nothing beside a 1 MiB
frame, and every language has it. An HTTP response is streamed to a file and read
in chunks rather than carried in one answer when it is large.
