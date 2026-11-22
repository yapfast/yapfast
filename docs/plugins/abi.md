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

The header is 13 bytes and the payload is JSON. An event batch is forwarded
verbatim: the parent writes each event once for every plugin that wants it, and
the helper reads only the frame's tag and plugin and hands the guest the events
and topics as the bytes they came in, so an event costs a copy and not a parse
and a re-serialize. Every other message is decoded into its type by the side
that acts on it. `id` pairs a request with its answer, in either
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
| `limits` | `memory_mib`, `idle_minutes` | What the person allows the helper: the whole memory budget in MiB and the idle timeout in minutes, 256 and five by default. Sent right after `Ready` and again whenever the person changes one, with id `0` and no answer; a number that isn't an offered step is snapped to the nearest one |
| `load` | `plugin`, `wasm`, `memory_pages`, `wasi` | Load a module and its limits, without activating it |
| `unload` | `plugin` | Drop an instance, keeping nothing |
| `activate` | `plugin`, `load` | Load the module when `load` is there, then instantiate and call `yf_activate`: one request for the whole phase, so the helper is never between a module and its instance. A `load` that replaces a running version carries the hand-off its `yf_deactivate` left, which is how a plugin's state crosses an update |
| `deactivate` | `plugin` | Call `yf_deactivate`, which may hand state to the next activation |
| `kick` | `plugin` | Run an instance again with nothing new: an eviction clock's trigger |
| `event` | `plugin`, `events` | A batch of events, already filtered and serialized |
| `hook` | `chain`, `request` | One hook, for the whole chain at once |
| `decorate` | `plugin`, `request` | Ask a plugin what it draws under a message on screen, with `yf_decorate` |
| `code_block` | `plugin`, `request` | Ask a plugin to colour one code block on screen, with `yf_code_block` |
| `identity` | `plugin`, `request` | Ask a plugin how one (person, server) looks, with `yf_identity` |
| `member_decorate` | `plugin`, `request` | Ask a plugin what it marks one member list row with, with `yf_member_decorate` |
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
| `identity` | `plugin`, `person`, `server`, `identity` | How a plugin says one person looks, or nothing |
| `member_decorated` | `plugin`, `member`, `badge` | What a plugin marks one member list row with, or nothing; unchecked, so the parent checks it |
| `code_block` | `plugin`, `block`, `spans` | The spans a plugin gave one code block, or nothing |
| `event_done` | `plugin` | An event batch finished |
| `status` | `plugin`, `status` | A plugin's state changed, for the page and the registry |
| `fault` | `plugin`, `fault` | An instance was discarded, and why |
| `log` | `plugin`, `record` | One line of a plugin's log |
| `usage` | `plugin`, `cpu_micros` | The processor time a plugin's code has taken since the helper started, for the devtools; at most once a second a plugin, after a call it ran |
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
on the first message. Five more exports are the interface's half of the API, and
optional: `yf_decorate` draws under a message, `yf_member_decorate` marks people
in the member list, `yf_identity` says how one person looks, `yf_code_block`
colours one block, and `yf_input` is told what the person did with something the
plugin drew. A module that declares one is asked for it and one that doesn't is
left alone — the helper reads the export section and calls only what is there,
so a missing one is a plugin that chose not to listen rather than a fault — so a
plugin that draws nothing, marks nobody, says nothing about how people look,
colours no code and ignores clicks exports none of the five.

### The payloads

Every one is JSON, because every PDK language has it and a payload can be read in
a log.

| Call | Argument | Answer |
| --- | --- | --- |
| `yf_activate` | `ActivatePayload { api, plugin, state }`: the API the helper speaks, the id, and what the last `yf_deactivate` handed on | Nothing |
| `yf_deactivate` | Nothing | `DeactivatePayload { state }`, or `null` for none. Cut to 256 KiB |
| `yf_event` | `EventBatch { events, results }` | Nothing |
| `yf_hook` | `HookRequest { hook, subject }` | `HookOutcome` |
| `yf_command` | `CommandInvocation { name, options, room }`, `room` the conversation it was typed in | `CommandOutcome` |
| `yf_decorate` | `DecorateRequest { room, message, author, body, own }` | A widget tree, an object with `tree`, `annotation`, `marks`, `hide` and `hide_media`, or `null` for none |
| `yf_member_decorate` | `DecorateMemberRequest { server, room, member, name, own, bot, roles, role_names, owner, permissions }`: `role_names` in `roles`' order, `owner` whether they own the server, `permissions` their server-wide bits from @everyone and their roles as Discord writes them | `MemberBadge { text, tone }`, or `null` for none |
| `yf_identity` | `IdentityRequest { person, server, name, username, own }` | An `Identity`, or `null` for none |
| `yf_code_block` | `CodeBlockRequest { block, language, text }` | A list of spans, or `null` for none |
| `yf_input` | `InputEvent`, one of `menu`, `widget`, `dismissed` or `keybind` | Nothing |
| `yf_call` | `HostCall { method, payload }` | `HostReturn` |
| `yf_post` | `HostCall` | Nothing |

A `HostCall` is at most 1 MiB less 16 KiB, so it fits one frame to the parent,
and a plugin makes at most 1,024 calls and posts in a second. Past either, the
host function traps: the instance is discarded and the plugin takes a strike.

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

A message has one id everywhere: the timeline's, `d:<id>` for Discord, in a
message event, `message_delete`, a reaction event's `message`, `reply_to`,
`messages.get` and `messages.history`, a decoration's and a menu row's subject,
and what `messages.edit`, `delete`, `react`, `jump_to` and the rest take (a bare
Discord id is still read as its `d:` form). A message's payload is the message as `messages.get` answers it, each file in
`attachments` with its name, size, type and `media`, the address `media.get`
reads. `message_delete`'s is `{ "id": … }`, with `"message"` too, the message as
it was, when a conversation Yapfast holds had it loaded, which is what a logger
keeps.

`topics` is there only when something was published to a topic the plugin
subscribed to; a batch with none has no field at all. Each entry is one
`plugin.publish`, and a subscriber handles them after the batch's events.

The top-level fields are what a subscription filter reads, so the parent filters
before the helper is woken and a plugin that didn't subscribe is never sent a
payload. A field over 64 KiB is cut and flagged rather than the event being
dropped: the payload becomes `{ "truncated": true, "bytes": 65540, "text": "…" }`,
so a plugin still sees that its message was long. Typing, presence, speaking
and the member list's comings and goings are opt-in and coalesced, so a burst is
one call with several events rather than one call each.

| Event | Payload | Tapped where |
| --- | --- | --- |
| `typing` | `{ "name": … }` | Discord's gateway, for any channel the account may read |
| `presence` | `{ "person": …, "name": …, "presence": "online"\|"idle"\|"dnd"\|"offline", "friend"? }` | The open room's member list, once per person whose presence moved; the first list of a conversation says nothing, and someone who left is not reported. A friend's presence comes too, wherever they are, with no room and `"friend": true`, from the friends list Yapfast keeps |
| `speaking` | `{ "person": …, "speaking": true\|false }` | The call's own voice state, once per person whose voice started or stopped, which is what `discord_audio` hears in its frames |
| `member_join` | `{ "person": …, "name": … }` | The open room's member list, once per person who appeared in it since its last list; the first list of a conversation says nothing |
| `member_leave` | `{ "person": … }` | The same list, once per person who left it |
| `spotify_state` | `spotify.now`'s answer, or `{ "playing": false }` | The song the session shares for the linked Spotify, once per change |
| `composer_change` | `{ "room": …, "text": … }` | `drafts.read`. A composer's text after each edit the person makes, as the draft Yapfast keeps for that conversation; a burst of keys is one call |

`voice_state` isn't coalesced: it is sent when the call changes as a whole —
joined, left, a phase, this account's mute or deafen, its screen or camera, the
stream it watches, or anyone's arrival, departure, mute, deafen, screen or
camera — and carries the call as `voice.state` answers it, below. A call that
ended is `{ "room": null, "people": [] }`. Who is speaking is `speaking`'s, so a
busy call doesn't resend the whole call four times a second.

These are sent as they happen:

| Event | Payload | Needs, and tapped where |
| --- | --- | --- |
| `startup_timing` | `{ "window_ms": …, "ready_ms": … }` | `app.timing`. Once a session, when Discord's gateway first says the account is online: milliseconds from the process starting to its window being built and to that moment, each `null` when it wasn't measured. Time spent signing in counts, so a start that waited on a person reads long |
| `relationship_add` | `{ "person": …, "name": … }` | `relationships.read`. The friends list Yapfast keeps from Discord's `RELATIONSHIP_ADD`, `UPDATE` and `REMOVE` (`yapfast-discord/src/service/tracker.rs`), once per person who became a friend, an accepted request included |
| `relationship_remove` | `{ "person": …, "name": …, "was": "friend"\|"request" }` | The same lists, once per friend or request that went |
| `relationship_request` | `{ "person": …, "name": …, "incoming": true\|false }` | The same lists, once per friend request that arrived or was sent |
| `server_join` | `{ "server": …, "name": … }` | `servers.read`. The server list Yapfast keeps from Discord (`GUILD_CREATE` for a server joined, docs.discord.food, gateway events), once per Discord server that appeared in it while Discord was online |
| `server_remove` | `{ "server": …, "name": … }` | The same list, once per server that left it: left, kicked, banned or deleted (`GUILD_DELETE` without `unavailable`). An outage keeps the server listed as unavailable, so it says nothing, and neither does signing in or a reconnect |
| `group_dm_remove` | `{ "room": …, "name": … }` | `servers.read`. The DM list, once per group DM that left it: left, removed or deleted |
| `game_start` | `{ "name": …, "started_ms": … }` | `activity.read`. The game Yapfast sees this account playing (its own game detection and Discord's), once per game that started, `started_ms` when the game says; whether the game is shared on the profile doesn't matter |
| `game_stop` | `{ "name": … }` | The same, once per game that stopped; one game changing to another is a stop and a start |

The relationship events compare the friends list and the requests when the
people lists change, and nothing while no plugin listens; the first read after a
plugin starts listening says nothing, as the member list's does. The server, group
and game events work the same way on the server list, the DM list and the game.

`member_join` and `member_leave` read the member list Yapfast holds for the
conversation on screen: in a DM, a group or a small server that is everyone in
it, and in a large server it is the part of the list Discord has sent, so a
plugin should read them as "appeared in" and "left" the list rather than as a
server's joins. Both need `people.read`, are opt-in and coalesced like presence,
and cost nothing while nobody subscribed.

`presence` costs one comparison per member of a list that arrived, and `speaking`
one per participant of a voice state that changed; neither builds an event while
no plugin declared the capability its subscription needs.

A hook's subject is a draft about to be sent, a file on its way up, a typing
indicator about to go, or an activity about to be published — the last of which
nothing dispatches yet: Yapfast does publish an activity, the game it detects
merged into its own presence, but that happens in the Discord client's session,
which has no bridge to ask, so `activity_pre_publish` is named in the ABI and
`HookKind::DISPATCHED` says it is the one hook no chain runs. Wiring it means
asking the chain where the session builds its presence. The answer is
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
                               "content_type": "image/jpeg", "upload": 7 } } }
```

Two hooks are asked by the interface, and neither can stop anything:

```json
{ "hook": "link_open",
  "subject": { "link": { "url": "https://open.spotify.com/track/1" } } }
{ "hook": "switcher_rank",
  "subject": { "switcher": { "query": "ge", "places": [
      { "id": "d:1/2", "name": "general", "kind": "channel" },
      { "id": "d:dm/3", "name": "Ann", "kind": "dm" } ] } } }
```

`link_open` is asked when the person clicks a web link on a host the plugin's
`link-hosts` lists, each such plugin on its own, with 50 ms each and 150 ms in
all: the answer is `changed` with the same `url` and an `offer`, `{ "label":
"Open in Spotify", "uri": "spotify:track:1" }`, whose words are 1 to 32
characters on one line and whose scheme the manifest's `schemes` lists. The
leaving dialog shows at once and the offer joins it beside Visit site, under the
plugin's name; an offer about another link, or later than that, isn't shown.
`switcher_rank` is asked of the first plugin with `switcher.rank` for each
search, with the first 50 places in Yapfast's order: the answer is `changed`
with the same `query` and the places in its own order, any left out being
hidden. One that adds a place or names one twice is ignored, and the list keeps
Yapfast's order until the answer comes, within 60 ms.

An indicator has nothing to change, so `changed` is refused for it: a plugin
either lets it go or, with `typing.intercept`, withholds it with `block`. A file
is answered by `changed` with the `attachment` it should be uploaded as — a new
`name`, and a `content_type` or a byte count it may narrow but not raise — or by
`block` with the reason the person sees where the file would have gone. Both
chains answer under a deadline of their own, so the composer never waits longer
than a table's worth of them ([the helper](host.md#deadlines)).

### A file's bytes

`upload` names the file while the chain holds it. A plugin in the chain may read
it, inside its hook, and a plugin that wants more time answers `changed` with
the same `attachment` and `"hold": true`: the chain ends there, the composer
says "*Name* is working on cat.JPG." and goes on without the file, and the file
is that plugin's until it says what goes. Nothing about the file is ever whole
in memory: a read is one piece off the disk, a write goes straight to a file of
its own, and an upload to another host is streamed from the disk by the network
gate.

| Call | Payload | Answer |
| --- | --- | --- |
| `uploads.read` | `{ upload, offset?, length?, replacement? }`, at most 512 KiB a piece | `{ data_base64, offset, bytes, eof }`, `bytes` the whole file's size |
| `uploads.write` | `{ upload, offset, data_base64 }`, each piece where the last ended | `{ written }`, the bytes written so far |
| `uploads.finish` | `{ upload, send: "original" \| "replacement" \| "nothing", name?, insert? }` | `true` |

`uploads.write` is for a plugin that held the file, and writes the file that
goes in its place, which is held to the room's own limit (Discord's for the
account and the conversation); `replacement: true` reads that file back.
`uploads.finish` ends the hold: the person's file, the plugin's or nothing goes
with the message, under `name` if it gives one, and `insert` is text put into
that conversation's composer, after what is there: the link to the file on the
plugin's own host, say. A plugin that is turned off, suspended or uninstalled
with a file held lets it go as the person picked it.

`http.fetch` takes the held file as its body with `body_upload` in place of
`body_base64`:

```json
{ "method": "POST", "url": "https://catbox.moe/user/api.php",
  "body_upload": { "upload": 7,
                   "form": { "field": "fileToUpload",
                             "fields": [["reqtype", "fileupload"]] } } }
```

Without `form` the file is the whole body; with it, it is one part of a
`multipart/form-data` form with a boundary and a `Content-Type` made here, the
text `fields` first, `filename` the upload's name unless given and its part's
type `application/octet-stream` unless `content_type` says. `replacement: true`
sends the plugin's written file. The length is sent as `Content-Length`, the
request goes through the same gate as any other, and the host still has to be
in the manifest's `network` (or allowed in a dialog). The answer is the host's,
as `http.fetch` answers any request.

A command's answer is `none`, `notice` with a line for the person to see in the
room they typed it in, `value` with the plugin's own JSON, or `failed` with a
message.

`yf_decorate` is asked by the parent for a message on screen, and never by a
guest's own call: it answers the widget tree `interface.md` describes, or
`null` for "nothing under this one". The parent checks the tree, so a plugin
that answers a shape the widget set doesn't have simply draws nothing.

The answer may also keep the row or its pictures out of sight, which is what
`messages.filter` is for:

```json
{ "hide_media": true }
{ "blur_media": true }
{ "hide": true, "tree": { "rows": [ { "cells": [ { "kind": "muted", "text": "hidden" } ] } ] } }
```

A bare tree is still an answer, so a plugin that only draws says what it always
said. The object form may also carry a line under the text and styles over its
words, both of which need `messages.read`:

```json
{ "annotation": { "text": "hello (from French)", "tone": "muted" },
  "marks": [{ "text": "bonjour", "style": "highlight" }],
  "tree": { "rows": [ { "cells": [ { "kind": "badge", "text": "fr → en" } ] } ] } }
```

An annotation is one line of at most 280 characters in `normal`, `muted`,
`accent`, `success` or `warning`; a mark names 1 to 200 characters of the text
and one of `bold`, `italic`, `strike` or `highlight`, and a message takes 16. The
parent checks both where they arrive and drops what doesn't check out, keeping
the rest. What they can't reach — the author, the time, Yapfast's badges — and
how the answers are kept per message and per plugin version is in
[the interface](interface.md#decorating-messages). `hide` takes the whole row out of view and `hide_media` takes its pictures
and files and leaves its text: both are display only, nothing is deleted and
nothing is sent, and both need `messages.filter`. A plugin without that
capability has the flags dropped and keeps whatever it drew, so a plugin that
declares `decorate` alone can draw under a row and cannot make it go away.

`messages.retain` is the other half of showing what went: a plugin that declares
it keeps a deleted row on screen, flagged `deleted` with its text, while the
backend's new timeline no longer holds it. The plugin is told about the deletion
by the `message_delete` event, which is where the row it stores comes from. The
merge is the parent's (`plugins/retention.rs`), so a plugin keeps the row by
declaring the capability and needs no call.

`yf_identity` is asked once per (person, server) and not once per message, so
the answer is a description of a person and not of a row:

```json
{ "person": "person-1", "server": "server-1", "name": "Ada",
  "username": "ada", "own": false }
```

```json
{ "display_name": "Ada Lovelace", "name_colour": 11580159,
  "avatar": "https://example.test/ada.png",
  "banner": "https://example.test/banner.png",
  "decoration": "https://example.test/decoration.png",
  "pronouns": "she/her", "badges": ["early", "bug-hunter"] }
```

Every field of the answer is optional: a plugin that changes only the name
answers only the name, and the rest stays as Yapfast had it. `null`, or an
answer with nothing in it, says nobody changed this person, which the parent
keeps as an answer so it isn't asked again. A name, a pronoun line or a badge
is one line of at most 64 characters, there are at most 8 badges, a colour is
`0xRRGGBB`, and a picture address is `https` and at most 2048 characters: a
field that isn't is dropped, and the rest of the answer still applies.

`yf_input` is the click, not the question: the interface has already changed — a
toggle took its value, a notice left the screen — so there is nothing to answer.
`menu` carries the row's `action` and what the menu was about, `keybind` the
keybind action the person's keys ran, `widget` the slot,
the widget's id, its new value and what it was drawn on, and `dismissed` the slot
the notice was in. A plugin that wants the person to see something says it with
`ui.notice` or by drawing again.

`yf_code_block` is asked once per code block, named by the hash of its content
and language, and only for the blocks of the messages on screen:

```json
{ "block": "9f2c…", "language": "rust", "text": "let x = 1;" }
```

```json
[{ "text": "let", "tone": "keyword" }, { "text": " x = 1;", "tone": "plain" }]
```

A tone is one of `plain`, `comment`, `keyword`, `string`, `number`, `function`,
`type`, `variable`, `operator`, `punctuation` and `error`, and never a colour:
how a tone looks is the interface's to decide, the way a widget cell's is. The
answer must **spell the block exactly**, character for character, in at most 512
spans, none of them empty; a set of spans that doesn't is refused where it
arrives and the block is drawn plain. A plugin colours a block and never changes
what it says. `null`, or nothing, is an answer too, and is kept: the block is
plain and isn't asked about again.

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

The calls the application carries out answer `true` when they did, and a
`denied` or `error` when they didn't. They reach the application's own actions,
so nothing waits on a person:

| Call | Payload | What it does |
| --- | --- | --- |
| `messages.send` | `{ room, body }` | Sends, within the 5-per-10-s cap, with a gesture behind it or `messages.send.background` |
| `messages.edit` | `{ room, message, body }` | Edits one message |
| `messages.delete` | `{ room, message }` | Deletes one message |
| `messages.react` | `{ room, message, emoji }` | Reacts with the emoji as Discord writes it |
| `messages.jump_to` | `{ room, message }` | Shows that message in its room |
| `composer.get_draft` | `{ room }` | The person's draft there, as a string |
| `composer.set_draft` | `{ room, body }` | Replaces it, which the composer shows at once |
| `composer.insert` | `{ room, text }` | Writes after it, never over it |
| `notify.send` | `{ text }` | One OS notification, titled with the plugin |
| `clipboard.write` | `{ text }` | Puts text on the clipboard |
| `open.url` | `{ url }` | Opens a page the way a message's link does |
| `media.get` | `{ url }` | The bytes of an attachment, an avatar or an emoji the picture store already keeps; `{ "bytes": 1234, "bytes_base64": "…" }`, and never a fetch |

The curated actions named `discord.*` are the same kind of call: each wraps
something Yapfast already does for the person, so the token, the rate limiter and
the account's own permissions stay here. A plugin names what it wants and never
reaches REST.

| Call | Payload | Answer | Does |
| --- | --- | --- | --- |
| `discord.set_status` | `{ status }` | `true` | Sets `online`, `idle`, `dnd` or `invisible`, as the status menu does |
| `discord.set_custom_status` | `{ text, emoji?, clear_after? }` | `true` | Sets the line under the name; `clear_after` is seconds. Empty text and no emoji clears it |
| `discord.set_activity` | `{ enabled }` | `true` | Shows or hides the game Discord sees you play; a plugin's own activity is `activity.set`, below |
| `discord.clear_activity` | none | `true` | The same as `{ enabled: false }` |
| `discord.mark_read` | `{ room }` | `true` | Marks a conversation, server, category or folder read, as its menu does |
| `discord.set_mute` | `{ room, muting }` | `true` | Mutes a conversation, channel or server; `muting` is `"off"`, `"always"` or `{ "minutes": n }` |
| `discord.open_room` | `{ room }` | `true` | Opens it where a click in the sidebar does |
| `discord.start_reply` | `{ room, message }` | `true` | Puts the composer in reply to a message of the room on screen |
| `discord.start_edit` | `{ room, message }` | `true` | The same for an edit, when it's this account's to edit |
| `discord.reactors` | `{ room, message }` | `[{ emoji, count, me }]` | The reactions the open timeline holds for a message, and how many gave each |
| `discord.pause_invites` | `{ server, invites, dms, hours }` | `true` | Pauses or resumes a server's invites and DMs, for one of the lengths Discord offers |
| `discord.add_emoji` | `{ server, name, image_base64 }` | `true` | Adds an emoji from a PNG, JPEG or GIF |
| `discord.add_sticker` | `{ server, name, tags?, description?, image_base64 }` | `true` | Adds a sticker from a PNG, JPEG or GIF |

A room, server or message the person can't see is an `error`, not a silent
nothing: Yapfast answers about what it holds. A server action the person has no
permission for is an `error` too — `servers.manage` is what lets a plugin ask,
and the person's own roles are what lets it happen.

`discord.open_room`, `start_reply` and `start_edit` need no capability and do
need a recent user gesture: a plugin can't move the window or write the composer
while nobody is there. Without one they answer `denied`.

### A plugin's own sound, its voice and its assets

| Call | Payload | Answer | Does |
| --- | --- | --- | --- |
| `audio.play` | `{ sound_base64, volume? }` or `{ asset, volume? }` | `true` | Plays a sound file on the speakers, at `volume` percent: one the call carries, or one of the package's sound assets by name |
| `audio.speak` | `{ text, voice?, rate?, volume? }` | `true` | Says up to 1000 characters in the OS's speech: `voice` one of its voices by name (spd-say `-y`, `say -v`, SAPI `SelectVoice`), `rate` percent of its own pace from 50 to 200, `volume` 0 to 100 |
| `assets.list` | none | `[{ name, kind, bytes }]` | The pictures and sounds the package carries; `kind` is `png`, `jpeg`, `gif`, `webp`, `wav`, `ogg` or `mp3` |
| `assets.get` | `{ name }` | `{ name, kind, bytes, bytes_base64 }` | One of them, read from the store and checked again as it is read |

An asset is the plugin's own and needs no capability to read: the review named
what the package carries, and a plugin reaches its own and no other's, by the
name its manifest gave it and never by a path. Playing one is still
`audio.play`'s. `discord.add_emoji` and `discord.add_sticker` take `asset` in
place of `image_base64` too, which is how a plugin's picture becomes an emoji a
person can react with: the reaction itself is Discord's, so it is an emoji on a
server the person manages, added with `servers.manage`, or nothing. A call that
names an asset of the wrong kind — a picture to `audio.play` — or names one and
carries bytes as well is an `error`.

The sound is decoded before the call answers, so what isn't a sound is an
`error`; then it plays on a thread of its own, as Yapfast's own sounds do, and
another sound overlaps it. A WAV file is read here; in a build with Discord's
media stack the MP3 and Ogg the soundboard plays are read too. A sound is at
most 512 KB, plays for at most 10 seconds, and is cut where that ends.

Speech is the operating system's: `spd-say` on Linux, `say` on macOS, and SAPI
through PowerShell on Windows. The words are one argument of the program, or a
file the script reads, so nothing a plugin writes can become a command. The
answer says the voice was asked to speak, not that it finished.

### The call, and the save dialog

| Call | Payload | Answer | Needs |
| --- | --- | --- | --- |
| `voice.state` | none | `VoiceView`, or `null` outside a call | `voice.read` |
| `files.save` | `{ name, text }` or `{ name, bytes_base64 }` | `pending`, then `{ saved, name? }` | `files.save`, and a recent user gesture |
| `window.set_title` | `{ suffix }`, or `{}` to take it away | `true` | `window.title` |

`voice.state` is the call as the call panel shows it, and nothing the panel
doesn't: the room and its name, the server, the phase (`joining`, `connected` or
`reconnecting`), this account's `muted`, `deafened`, `streaming` and `video`,
whose stream it `watching`, and `people`, at most 99, each with `id`, `name`, `own`,
`muted`, `deafened`, `server_muted`, `speaking`, `streaming` and `video`. It is
read-only: nothing in it mutes, moves or hears anyone, and no audio, picture or
device leaves through it. The `voice_state` event carries the same object.

`window.set_title` puts up to 32 characters on one line after the window's
title, after a dot: *(3) Yapfast · ready in 1.2 s*. One plugin's words show at
a time, the first to set them keeping the title until it takes them away or
stops.

`files.save` hands the person a file. The parent writes it, never the plugin: the
system's save dialog opens titled *Save a file for* and the plugin's name, with
the suggested name filled in, and the bytes go where the person chose. The name
is a file name of at most 128 characters — no `/`, `\`, `:`, leading dot or
control character — and the bytes are at most 768 KiB, since they arrive in one
frame. A plugin has one dialog open at a time, and opens one only behind a
person's click, as `clipboard.write` does. The answer comes when the dialog
closes: `{ "saved": true, "name": "notes.txt" }`, `{ "saved": false }` for a
cancel, or an `error` if the write failed. It names the file and never the
folder, which would tell a plugin the person's account name and how their disk
is laid out. Nothing else in the API writes outside the plugin's own storage.

### The powerful tier's calls

Each needs its capability, the switch in Advanced and the person's first-use
answer ([permissions](permissions.md#the-powerful-tier)); the first use waits
for that answer and then goes on, or is `denied` with the person's no.

| Call | Payload | Answer |
| --- | --- | --- |
| `fs.list` | `{ path }`, `""` for the folder itself | `[{ path, dir, bytes, modified? }]`, sorted, at most 1,000 |
| `fs.stat` | `{ path }` | `{ path, dir, bytes, modified? }` |
| `fs.read` | `{ path, offset?, length? }` | `{ size, offset, bytes_base64 }`, at most 512 KiB a call |
| `fs.write` | `{ path, text \| bytes_base64, append? }` | `{ written }`, at most 768 KiB a call, for a folder given to write |
| `fs.thumbnail` | `{ path, size? }` | `{ width, height, png_base64 }`, the longer side at most 256 px, from a picture of at most 32 MiB, decoded off every thread a person waits on |
| `net.listen` | none | `{ listening, port, url }`, then one more answer per request: `{ request, method, path, headers, body_base64 }` |
| `net.respond` | `{ request, status?, headers?, text \| body_base64 }` | `true`, or an `error` for a request no longer waiting |
| `net.connect` | `{ host, port, tls?, websocket? }` | `pending`, then one answer per piece of news: `{ news: "connected", socket, address }`, `{ news: "data", socket, data_base64 }`, `{ news: "text", socket, text }` and last `{ news: "closed", socket, reason }` |
| `net.send` | `{ socket, text \| data_base64 }` | `true`, or an `error` for a connection that closed |
| `net.close` | `{ socket }` | `true`; the `closed` news follows |

### The microphone

A plugin with `microphone` records it, after the person said yes the first
time, in the dialog the powerful tier's first use opens (without its switch),
and for as long as a recording runs Yapfast shows a red bar at the top of the
window naming the plugin, with how long it has run and Stop. The bar is
Yapfast's own, over everything a plugin draws; no `ui.modify` reaches it.

| Call | Payload | Answer |
| --- | --- | --- |
| `microphone.start` | `{ format?: "opus" \| "pcm", max_seconds? }` | `pending`, then, when it ends, `{ recording, format, duration_ms, bytes, waveform, stopped_by }`; `stopped_by` is `plugin`, `person` or `length` |
| `microphone.stop` | none | `true` |
| `microphone.read` | `{ body, offset?, length? }`, `body` the `recording` | `{ data_base64, offset, bytes, eof }`, 512 KiB at most, as `http.read` |
| `messages.voice` | `{ room, recording }` | `pending`, then `true` once Discord took the voice message |

One recording runs at a time, from the microphone chosen for calls, on a
thread of its own that writes the file as it goes, 20 ms at a time: `opus` is
Ogg Opus at 48 kHz mono and 32 kbit/s, what Discord's clients send as a voice
message (docs.discord.food, resources/message, Voice Messages), and `pcm` a WAV
of 16-bit 48 kHz mono samples. `waveform` is the loudest sample of each 100 ms,
one byte a point and at most 256 points, averaged down for a longer recording,
base64: Discord's voice message waveform. A plugin that stops, or is turned
off, ends its recording.

`messages.voice` sends an Opus recording of the plugin's own as Discord's
clients send a voice message: the file uploaded as `voice-message.ogg`
(`POST /channels/{id}/attachments`, then the upload), and a message with the
`IS_VOICE_MESSAGE` flag (`1 << 13`), no content, and the one attachment with its
`duration_secs` and `waveform`. It is a send, under `messages.send` and the
send's rails: a click behind it, or `messages.send.background`, and five sends
in ten seconds. Discord itself refuses one where the account lacks
`SEND_VOICE_MESSAGES`, and says so.

### Signing in to a service

A plugin with `oauth.login` signs the person in to a service its manifest's
`oauth` names, with an app of its author's own: Spotify's Web API, for one, in
place of the token Discord lends (`spotify.control`).

| Call | Payload | Answer |
| --- | --- | --- |
| `oauth.login` | `{ provider }` | `pending`, then `{ provider, access_token, token_type, expires_at_ms, scopes }` once the person signed in, or an `error` saying why not |
| `oauth.token` | `{ provider }` | `pending`, then the same shape: the kept token, renewed first when it has less than a minute left |
| `oauth.logout` | `{ provider }` | Whether a sign-in was kept |

`oauth.login` stands behind a click of the person's. Yapfast runs
Authorization Code with PKCE (RFC 7636), the flow for an app with no secret:
it listens for one request on `http://127.0.0.1:PORT/callback`, the leaving
dialog shows the service's page with the plugin named, the browser opens it on
the person's say, and the code that comes back (with its `state` checked) is
traded for tokens in the parent. The person has five minutes. The refresh
token is kept in the plugin's data folder, readable by this account only, and
never reaches the plugin, which is given access tokens and calls the service's
API itself through `http.fetch` with `Authorization: Bearer …`, the API's host
in its `network`. A renewal that brings no new refresh token keeps the old one,
as Spotify's docs say may happen. Uninstalling with the data removes the kept
sign-in; signing out of the app itself is done on the service's site.

Spotify's own rules apply to the author's app
([developer.spotify.com](https://developer.spotify.com/documentation/web-api/concepts/redirect_uri)):
the redirect URI is registered as `http://127.0.0.1/callback` (`localhost`
isn't allowed); an app in development mode needs its owner on Premium and
lists the people allowed to sign in to it; and the player's controls need
Premium on the account signed in.

### The account's own calls

Each has its capability, and each is answered later: `pending`, then the
answer in a later batch, because Discord or Spotify answers it
([permissions](permissions.md#the-accounts-own-calls) has the limits, which are
the services' own, and the sources).

| Call | Payload | Answer |
| --- | --- | --- |
| `messages.attach` | `{ room, body, attachments }`, the same as `messages.send` | `true` |
| `activity.set` | `{ name, kind?, details?, state?, start_ms?, end_ms?, application_id?, large_image?, large_text?, small_image?, small_text?, buttons?, url?, party?: { id?, size?: [now, most] } }`; `kind` is `playing`, `streaming` (with `url`, a Twitch or YouTube address, as Discord shows streams), `listening`, `watching` or `competing` | `true` once Discord took its pictures |
| `activity.clear` | none | `true` |
| `profile.set` | `{ bio?, pronouns?, custom_status?: { text, emoji? } }` | `true`; the first time it waits for the person's answer, `pending`, then goes on or is `denied` with their no |
| `messages.search` | `{ place, query, offset? }` | `{ total, hits: [{ room, message, author, body, at }], indexing }` |
| `drafts.list` | none | `[{ room, body }]` |
| `account.sessions` | none | `[{ id_hash, last_used?, os?, platform?, location? }]` |
| `people.badges` | `{ person, server? }`, with `people.read` | `[{ id, description, icon, link? }]`: the person's Discord badges, from their profile in that server or outside any, as Discord's profile gives them |
| `relationships.affinities` | `{ of?: "users" \| "servers", detailed? }`, or none for the people | `{ users: [{ person, affinity, is_friend?, user_segment?, other_user_segment?, dm_probability?, dm_rank?, vc_probability?, vc_rank?, server_message_probability?, server_message_rank?, communication_probability?, communication_rank? }] }` or `{ servers: [{ server, affinity }] }`, highest first |
| `spotify.now` | none | `{ playing, track?, artists, album?, album_art?, uri?, duration_ms?, position_ms?, shuffle?, repeat?, volume?, device? }`, or `null` |
| `spotify.play`, `pause`, `next`, `previous` | none | `true` |
| `spotify.seek` | `{ position_ms }` | `true` |
| `spotify.volume` | `{ percent }`, 0 to 100 | `true` |
| `spotify.shuffle` | `{ on }` | `true` |
| `spotify.repeat` | `{ mode }`: `off`, `track` or `context` | `true` |

An attachment is `{ name, bytes_base64 }` or `{ name, asset }`, never both, and
`spoiler: true` covers it until clicked. `messages.send` takes `attachments` as
well, with `messages.attach` declared. `place` is a room's id or a server's, and
`query` takes the search box's filters. A Spotify call that fails says why: no
Spotify linked, Premium needed, nothing playing on any device, or Spotify's own
wait.

`spotify_state` is an event, with `spotify.control`: what the session shares
for the linked Spotify changed, and the payload is `spotify.now`'s shape, or
`{ "playing": false }` when nothing plays. A keybind arrives at `yf_input` as
`{ "kind": "keybind", "action": "next" }`, the action as the manifest's
`keybinds` names it; it is a person's gesture, as a menu row is.

A path is inside the folder the plugin was given, with `/` between its parts.
`messages.send` takes `files`, paths in that folder, with `messages.attach`: at
most ten, each at most 10 MiB, sent as a person's attached files are and never
in place of what the person has attached.

A send a plugin asks for isn't run through `message.pre_send`: that chain is for
what the person wrote, and a plugin that both intercepts and sends would meet
itself.

The methods are names, so adding one never changes the link surface. Every name
the ABI lists has a typed payload in the contract crate: `storage`, `settings`,
`http`, `timers`, `app`, `account`, the `rooms`, `messages` and `people` reads,
the `ui` calls, `media.get`, the application's own actions (`messages.send`,
`edit`, `delete`, `react` and `jump_to`, the `composer` calls, `notify.send`,
`clipboard.write` and `open.url`), the `discord` ones, the `audio` ones,
`voice.state`, `files.save`, `assets.list` and `assets.get`, the account's own
calls (`crate::account`), and the `plugin` group. A name this build doesn't have answers `unsupported` rather than
failing to link, which is how a plugin feature-detects what the Yapfast in front
of it can do.

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
conversation on screen from the timeline Yapfast keeps, and any other Discord
conversation the account can read by asking Discord as its client asks (a page
of at most 100, `before` a message's id), answered later; and
`people.get` and `people.member` the account, the open room's members and, for `people.get`, the friends. `rooms.selected` names the conversation that is open. Each read needs the capability
[`permissions.md`](permissions.md) gives it, and one answer is one frame: a page
stops at 1 MiB rather than being cut.

The `ui` calls take a widget tree, the rows of cells
[`interface.md`](interface.md#widget-trees) describes, as JSON:

| Method | Payload | Answer |
| --- | --- | --- |
| `ui.set_slot` | `{ "slot": "banner", "tree": { … } }`, and `"part": "now-playing"` for one of the names the manifest's `parts` gives that slot | `{ "drawn": true }`, or `{ "drawn": false, "reason": … }` for a slot this build has no place for, a tree over its cap, one that isn't a tree, or a part the manifest doesn't give that slot |
| `ui.clear_slot` | `{ "slot": "banner" }` | `{ "drawn": true }` |
| `ui.notice` | `{ "text": "…" }`, markdown | `{ "drawn": true }`; an empty line is an error |
| `ui.toast` | `{ "text": "…" }`, plain text | `{ "drawn": true }` |
| `ui.open_dialog` | `{ "tree": { … }, "subject"? }`, `subject` what the dialog is about (a message or person a menu row asked about), which its widgets' inputs name; the open conversation when left out | `{ "drawn": true }`; a second dialog from one plugin replaces the first |
| `overlay.set` | `{ sprites: [{ id, image, x, y, width?, height?, crop?, opacity?, clickable? }], fps? }`, with `overlay` | `true`; frames arrive at `yf_input` as `overlay_frame` ([the overlay](interface.md#the-overlay)) |
| `overlay.clear` | `{}` | `true` |
| `ui.redecorate` | `{}` | `true`: what this plugin drew under messages is forgotten and the rows on screen are asked again, for an answer that changed (a review that loaded, a setting of its own) |
| `identity.refresh` | `{ person? }` | `true`: what plugins said about that person, or everyone, is forgotten, and a card showing them asks again |

A drawing doesn't wait for the interface: the call is answered as soon as the
tree is checked, and what reaches the screen is coalesced to a frame and capped
at ten a second per plugin. The slots a plugin may name are `banner`, `toast`,
`dialog` and `profile.badges`; `composer` and `message.actions` take buttons only,
and `message` answers that what a plugin draws under a message is a
`message.decorate` answer, not a slot. None of them needs a capability: a slot is
a plugin's own surface.

### What a plugin keeps, and when it wakes

`storage` is the plugin's own key-value store: the only thing that survives an
instance being evicted, and the only thing counted against its `storage_bytes`
quota. It needs the `storage` capability.

| Method | Payload | Answer |
| --- | --- | --- |
| `storage.get` | `{ key }` | The value held there, which is the plugin's own JSON, or `null` for a key it doesn't have |
| `storage.set` | `{ key, value }` | `true`, or `denied` with the quota's sentence when the store would go past it |
| `storage.delete` | `{ key }` | `true`, whether or not the key was there |
| `storage.list` | `{ prefix }` | The keys it holds under that prefix, which is every key when it's empty |
| `storage.clear` | `{}` | `true`: every key it held is gone, and its quota is free again |

One value is one answer, so a value too big for a frame is refused with a
sentence saying its size and what one answer carries, rather than read back as
`null` and taken for an absent key.

`timers` wake a plugin with nothing to trigger it:

| Method | Payload | Answer |
| --- | --- | --- |
| `timers.after` | `{ interval_ms, tag }` | `true`: wakes it once, after that long |
| `timers.every` | `{ interval_ms, tag }` | `true`: wakes it every interval, which is at least 1000 ms |
| `timers.cancel` | `{ tag }` | Whether a timer with that tag was there |

A timer belongs to the helper and not to the instance, so it survives an eviction
and an instance with one pending isn't evicted under it. A plugin may hold 16.
What arrives when one fires is an `yf_event` call whose `timers` names it by
its `tag`, `{ "events": [], "results": [], "timers": ["refresh"] }`; timers that
fire while the plugin is busy ride in one call, each named once, and a tick is
never dropped to make room in a full lane. The PDK calls `timer` with each tag.
`app.now` answers the time in Unix milliseconds, since a plugin has no clock of
its own.

Every host call is a request the parent answers, because the parent holds the
token, the network gate and the consent dialogs. Most of those answers take the
parent well under a millisecond — a setting, a stored value, the open room, a
slot drawn — so the guest's `yf_call` waits for the answer and returns it in the
same call: a PDK's `settings::get` or `storage::get` returns its value, inside a
hook as anywhere else. The wait is at most 250 ms and never past the deadline of
the export it is in, less 3 ms for the guest to use the answer (a
`message_pre_send` hook has 25 ms), and an answer that hasn't come by then makes
the call `pending` after all, finished the way below. A plugin reading a
setting should still handle `pending`, which means Yapfast was busy, not that
the setting is missing.

The calls that wait on the network, a person or another plugin, or that answer
more than once, are `pending` at once and never hold the guest's thread:
`http.fetch`, `plugin.call`, `settings.on_change`, `files.save`,
`fs.thumbnail`, `net.listen`, `net.connect` and the account's own calls
([below](#the-accounts-own-calls)). The guest's function never waits for a
person.

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
   call carries it in `results` as `{ "call": 7, "answer": … }`. Answers that
   arrived while the plugin was busy come in the same call, in the order the
   parent sent them, so a guest reads every entry of `results`, not the first.

That is why an event function has to handle the replies of the calls that
answer later.

An `http.fetch` answer whose body is larger than one frame carries, in place of
`body_base64`, a `body_id` with `bytes`, its size: the body is kept on disk,
and `http.read` `{ body, offset?, length? }` answers it 512 KiB at a time as
`{ data_base64, offset, bytes, eof }` (the PDK's `http::body` reads it whole).
`http.release` `{ body }` lets it go; one left unread goes after ten minutes. A call
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
