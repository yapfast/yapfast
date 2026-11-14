# Permissions

A manifest declares what a plugin means to do, the review dialog shows that at
install, and both Yapfast and the helper refuse anything the manifest didn't
declare. A capability is a group of calls, not a person or a token: every call
the parent answers is checked again where it runs, whether the guest asked for
it directly or arrived through a hook, an event or a timer. There is one place a
missing capability turns into a refusal instead of a call.

The declarations are in the manifest's `permissions` object, described in
[the manifest](manifest.md). The names are public API: they are written in
manifests, kept in `grants.json`, and shown at install.

## The capabilities

The dialog lists them in this order, which is the contract crate's order and not
the order the manifest happens to write. A capability the manifest doesn't
declare isn't shown, because the plugin can't use it. Two rows below say they are
not shown at all: nothing in this build is behind them, so a manifest may still
name them and reading it leaves them out.

The last column is the dialog's own wording, a sentence each except the two
audio capabilities, which share one line.

| Name | Grants | Shown as |
| --- | --- | --- |
| `storage` | Its own key-value store | Keeps its own data |
| `messages.read` | Message events and history queries | Reads your messages |
| `messages.intercept` | Change or cancel outgoing messages | Can change what you send |
| `messages.send` | Send, within limits | Can send messages as you |
| `messages.send.background` | Send without a recent user gesture | Sends messages automatically, without you doing anything. Automated sending can get your Discord account banned. |
| `messages.retain` | Keep deleted messages visible | Keeps messages after they're deleted |
| `messages.filter` | Hide messages from view | Can hide messages from you |
| `people.read` | Profiles, members, friends, presence, and how people look | Reads people's profiles |
| `servers.read` | Servers, channels, roles, permissions | Reads server details |
| `servers.manage` | Actions the person already has permission for, on their servers | Can change servers you manage |
| `presence.write` | Status, custom status, activity | Changes your status |
| `notifications.manage` | Mute, mark as read | Can mute and mark things read |
| `typing.intercept` | Stop the typing indicator being sent | Can hide that you're typing |
| `uploads.intercept` | Rename or strip files before upload | Can change files you upload |
| `media.read` | Bytes of attachments, avatars and emoji | Reads attachments and pictures |
| `files.save` | Save through the native dialog | Not shown: no save dialog is behind it |
| `voice.read` | Voice state and who is speaking | Sees your calls |
| `audio.play`, `audio.speak` | Play a sound, speak text through the OS | Plays sounds and speech |
| `microphone` | Record from the microphone | Not shown: no recording path is behind it |
| `notifications` | OS notifications | Shows notifications |
| `clipboard.write` | Write to the clipboard on a gesture | Writes to your clipboard |
| `ui.modify` | Change, hide or replace named parts of the interface | Can change Yapfast's interface |
| `network` | HTTPS to the hosts the manifest lists, and to no other | Uses network |
| `network.local` | One exact loopback port, which the manifest names | Connects to an app on this computer |
| `open.app` | Open other applications through listed URI schemes | Opens other apps |

One of those sentences is shown in the dialog's danger tone:
`messages.send.background`, the one that lets a plugin act as the person without
them. The lines that reach something people care about are a step below it, in
bold: `messages.read`, `messages.intercept`, `messages.send`, `messages.retain`,
`servers.manage`, `uploads.intercept` and `ui.modify`.

`messages.send` covers a send a person asked for. `messages.send.background`
covers everything else, and it is a deliberate exception to the rule the rest of
Yapfast keeps, in [the Discord notes](../discord/README.md#staying-within-discords-limits):
never act without a user action behind it. A plugin's own consent is what pays for
that exception, which is why the capability is the one the dialog shows in its
danger tone.

## What an event needs

Subscribing isn't a capability of its own: an event arrives only if the plugin
holds the capability its contents need, so a plugin that subscribed to messages
without `messages.read` receives none of them. A hook is the same: a plugin takes
part in `message_pre_send` only if its manifest holds `messages.intercept`,
because intercepting a send is not something a plugin can do by accident.

| Event | Needs |
| --- | --- |
| `message_create`, `message_update`, `message_delete`, `reaction_add`, `reaction_remove`, `typing` | `messages.read` |
| `room_selected` | `servers.read` |
| `voice_state`, `speaking` | `voice.read` |
| `presence` | `people.read` |
| `app_focus`, `account_ready` | Nothing, since they say nothing about a person |

A read is the same: what it answers is what the capability names.

| Call | Needs |
| --- | --- |
| `app.version`, `app.locale`, `app.theme_mode`, `account.me` | Nothing: they say what Yapfast and the person's own account are, and no other person's data |
| `rooms.selected`, `rooms.get`, `rooms.list` | `servers.read` |
| `messages.get`, `messages.history` | `messages.read` |
| `people.get`, `people.member` | `people.read` |
| `media.get` | `media.read` |
| The `decorate` reply's `hide` and `hide_media` | `messages.filter` |
| Keeping a deleted row on screen | `messages.retain`, declared and nothing more: the parent merges the row |
| `identity.decorate`, the `identity` activation trigger | `people.read` |
| `code_block.render`, the `code` activation trigger | `messages.read`: a block is message content |

`media.get` answers bytes of what the picture store already keeps: an
attachment, an avatar or an emoji a conversation, a member list or a profile has
shown. It never fetches anything, because a fetch on a plugin's behalf would be a
request the network gate never saw and the manifest never declared, and it takes
no token. A picture the store doesn't hold is an error saying so, and one larger
than a frame carries is `denied` with its size and the cap in the sentence.

A subscription's own filter, which room and whether the account's own messages
count, applies in the parent before the helper is woken, so an event nobody wants
is never serialized and its contents never leave Yapfast. That filter and the
capability are two separate gates.

## Sending

A plugin may send five messages in ten seconds, counted over a sliding window, so
a plugin that sends five and waits nine seconds doesn't then get five more at the
turn of a window. The refusal says how long to wait, and that is the whole rate
limit a plugin meets: it never argues with Discord's own buckets, because it
never talks to Discord.

A send counts as the person's while it follows something they did. A command
typed in the composer, a menu row chosen or a button pressed records that the
person asked the plugin for something, and for five seconds after that
`messages.send` alone covers what the plugin sends. Past five seconds, or with no
gesture at all, the send is the plugin acting on its own and needs
`messages.send.background`.

## What each call needs

| Call | Capability |
| --- | --- |
| `messages.send`, `edit`, `delete`, `react` | `messages.send`; a send without a recent gesture also needs `messages.send.background` |
| `messages.jump_to`, `composer.*` | none: navigation and the person's own composer |
| `notify.send` | `notifications` |
| `clipboard.write` | `clipboard.write` |
| `open.url` | `open.app` |
| `discord.set_status`, `set_custom_status`, `set_activity`, `clear_activity` | `presence.write` |
| `discord.mark_read`, `set_mute` | `notifications.manage` |
| `discord.reactors` | `messages.read`: it answers what people gave to a message |
| `discord.pause_invites`, `add_emoji`, `add_sticker` | `servers.manage`, and the person's own roles on that server |
| `audio.play` | `audio.play` |
| `audio.speak` | `audio.speak` |
| `discord.open_room`, `start_reply`, `start_edit` | none, and a recent user gesture: without one they answer `denied` |

A curated action wraps something Yapfast already does, so the capability is the
first of two gates and the person's own permissions are the second: a plugin may
ask a server to pause its invites, and it happens only if this account has Manage
Server there. Yapfast never acts with anything a plugin didn't declare.

## The network gate

A guest never opens a socket. `http.fetch` crosses the pipe, and the parent's own
HTTP stack performs the request through a gate that decides it there and then.
Five things are refused outright, and the same checks run for every hop of a
redirect chain, at most five hops deep: a server that answers 302 is as able to
point a request at a private address as a plugin is.

- **HTTPS only.** A plain `http` URL, and any other scheme, is refused.
- **Discord's own hosts.** `discord.com`, `discordapp.com`, `discordapp.net`,
  `discord.gg`, `discord.dev` and `discord.media`, plus the gateway, voice and
  CDN names under them. A plugin reaches Discord through Yapfast's curated
  actions and `media.get`, never with the account's credentials.
- **The host must be declared.** It has to match an entry in the manifest's
  `network` list, or a grant the person gave. `api.example.com` is exact;
  `*.cdn.example.com` matches `cdn.example.com` and the names below it. A bare
  `*` is refused when the manifest is validated, since a plugin that may reach
  anywhere says nothing useful at install.
- **An address that isn't public.** Loopback, an RFC 1918 network, a carrier's
  range, link-local, a cloud metadata address, the unspecified address,
  broadcast or multicast, and an IPv4 address written the IPv6 way: each of
  those reaches a machine that never expected a plugin to be able to reach it.
  The name is resolved here, the addresses are checked, and the connection goes
  to those addresses and no other, so a second lookup that answers differently
  finds nobody left to connect anywhere.
- **Past the caps.** A response is read under 4 MiB unless the manifest asked for
  more, up to 16 MiB, and past it the stream is dropped rather than read to the
  end. A plugin may have 4 requests in flight while the whole helper has 16.

A request body is at most 768 KiB undone from its base64, because it arrives
inside one frame. A response body above 752 KiB is written to
`<data>/plugins-http/` as it arrives rather than carried in the answer.

The gate is handed a transport by the application,
[`transport.rs`](../../crates/yapfast-plugins/src/transport.rs), because
`yapfast-plugins` has no HTTP client and no trust roots of its own:
[`network.rs`](../../crates/yapfast-plugins/src/network.rs) defines `Transport`
and `Resolver` and leaves them to the crate that owns the client. One seam is open: the answer to a body that went to a
file has its length in `bytes`, no `body_base64`, no field naming the file and no
reason saying why, so a plugin can tell that there was a body and not what it
said, and nothing deletes the file afterwards. Only an answer that runs past the
cap tidies up after itself, by removing the file it had started. A chunked-body
frame is what would carry one to a guest. The methods are `GET`, `HEAD`, `POST`,
`PUT`, `PATCH`, `DELETE` and `OPTIONS`, and a fixed set because `CONNECT` would
ask a server for a tunnel and `TRACE` would echo the request back.

Loopback is the one deliberate exception, and it is its own capability: a plugin
that declared `network.local` and named one port in `local-port` may reach that
port on this computer and nothing else, which is how a plugin talks to a program
someone runs beside Yapfast. Every address the name resolves to must be
loopback, and the port must be the one named.

Nothing of Yapfast's travels with the request. The `User-Agent` is the fixed line
`Yapfast-Plugin/1`, never Discord's client, and a plugin can't set `host`,
`user-agent`, the connection-level headers, `cookie` or `proxy-authorization`:
those are the transport's, and a plugin holds no session for them to belong to.
`Authorization` is sent only if the plugin wrote it. One connection is opened per
request, with no cookie jar, no pool and no proxy, so a plugin can't touch
Yapfast's Discord session or another plugin's.

What a plugin does reach through the gate is what it declared and what was
agreed to. Installing a plugin from a git URL is not a review: the dialog says so,
and the person's own answer is the only thing standing between a declared host
and the plugin's request.

## Grants

A host in the manifest is granted by accepting the install. A host that isn't
declared is a request the person answers the first time the plugin tries it, in
a dialog that sits over whatever is on screen:

> Word Filter wants to connect to api.example.com

| Choice | Effect |
| --- | --- |
| Allow | For this session |
| Allow, and don't ask again | Written to `grants.json` |
| Deny | For this session |
| Deny, and don't ask again | Written to `grants.json` |

Closing the dialog without choosing is a Deny: the plugin asked, and nobody said
yes. The question belongs to the application and not to the Settings page, so it
is asked wherever the person is and survives the page closing.

### What the plugin sees

`http.fetch` answers `pending` at once, and the guest's call is finished later,
under the same call id: the response on an allow, and a refusal with the reason
on a deny. The guest holds nothing while it waits.

The parent does. The request is kept, with no socket and no place in the gate's
concurrency limits, and it is made again through the gate once there is an
answer: every check runs a second time, and the second run is the one that
reaches a server.

A request let go because nothing will answer it any more — the plugin was turned
off or removed, the helper stopped, Yapfast is exiting, or the host's grant was
taken back — is refused with the reason saying so, so none waits forever.

### Asking is bounded

A plugin that keeps asking is asking a person who has already said no:

- one dialog per plugin at a time, so the person answers about one server at a
  time. A request for a host whose dialog is already open joins it, and one for
  another host is refused with a reason telling the plugin to ask again, rather
  than queued behind a question the plugin can't see;
- four of a plugin's requests wait at once at most, so a plugin that asks in a
  loop holds a handful of bodies and no more;
- after three refusals in a session, that plugin's further requests are refused
  without a dialog, and its row says how many were refused without asking.
  Turning it off and on asks again.

`grants.json` holds hosts, and only hosts: never a URL, never anything sent to
one. An allow for `api.example.com` says which server may be contacted and
nothing about what may be sent there. The file is versioned, and a version this
build doesn't write, or a file that isn't the shape it writes, is moved aside as
`grants.json.invalid` with its contents intact and a line in the log, and the app
starts with nothing granted: a grants file that can't be read is a reason to ask
again, never a reason to guess. Answers are written beside the file and renamed
over it, so a crash mid-write leaves the previous answers rather than half a
file.

Session answers live in memory and vanish when the app exits.

## Taking a grant back

The plugin's page lists the hosts it has been allowed, and each one can be
revoked there. Revoking removes the grant from the file and from this session,
and refuses the host for the rest of the session, so it takes effect on the next
request and not on the next start: the gate asks on every request rather than
remembering an answer, a request already in flight finds nothing that would let
it through, and a request waiting on a dialog about that host is refused at
once. Turning a plugin off forgets its session answers and closes its dialog.

Disabling a plugin, or turning the plugin system off, stops all of this: no
helper, no request, no dialog.

## What is called out at install

Two things in the dialog are more than a list of lines.

The first is a plugin that reads messages and reaches a network, which is the
real threat a plugin poses: it can send what it reads to a server it names.
Listing that as two unrelated lines hides it, so the dialog says instead:

> It reads your messages and connects to api.example.com, so it could send your
> messages to that server.

The hosts are the ones allowed, a declared one being granted by the install itself
and the rest by the popup, joined as a list: "that server" for one and "those
servers" for more, and no line at all when none is allowed. Reading here means
`messages.read` and nothing else, so a plugin that holds `messages.send` or
`messages.intercept` without it gets no such line either.

The second is the danger tone's sentence, above: a plugin that sends without the
person says so in a line that doesn't look like the others, and the bold lines
below it are the ones that reach something people care about.
