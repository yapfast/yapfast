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
declare isn't shown, because the plugin can't use it. One row below says it is
not shown at all: nothing in this build is behind `microphone` yet
([its design](audio-processing.md)), so a manifest may still name it and reading
it leaves it out.

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
| `uploads.intercept` | Rename, withhold, read or replace files before upload, or send them to its own host | Can read and change files you upload, or send them somewhere else |
| `media.read` | Bytes of attachments, avatars and emoji | Reads attachments and pictures |
| `files.save` | Hand the person a file to save, through the system's save dialog | Saves files where you choose |
| `voice.read` | The call: who is in it, who speaks, mute, deafen, screens and cameras | Sees your calls |
| `audio.play`, `audio.speak` | Play a sound, speak text through the OS | Plays sounds and speech |
| `microphone` | Hear or process the microphone | Not shown: no path is behind it yet, see [the design](audio-processing.md) |
| `notifications` | OS notifications | Shows notifications |
| `clipboard.write` | Write to the clipboard on a gesture | Writes to your clipboard |
| `ui.modify` | Change, hide or replace named parts of the interface | Can change Yapfast's interface |
| `network` | HTTPS to the hosts the manifest lists, and to no other | Uses network |
| `network.local` | One exact loopback port, which the manifest names | Connects to an app on this computer |
| `open.app` | Open other applications through listed URI schemes | Opens other apps |
| `messages.attach` | Send files it made, or its own assets, with the messages it sends | Sends files it makes with the messages it sends, as you |
| `activity.set` | An activity of its own on the account's presence | Shows an activity of its own on your profile, which everyone who sees you on Discord sees |
| `profile.write` | The bio, the pronouns and the custom status, asked about the first time | Can change your bio, pronouns and custom status; you're asked the first time |
| `keybinds` | Named actions the person binds to keys in Settings → Keybinds | Offers actions you can bind to keys; it never sees your keys |
| `sounds.replace` | Its own sound assets in place of Yapfast's sound events | Replaces Yapfast's sounds with its own |
| `messages.search` | Discord's search, as the search box asks it | Searches your messages through Discord's search, as the search box does |
| `drafts.read` | The drafts the person left unsent, and `composer_change` as they type | Reads what you type before you send it, and the drafts you left |
| `account.sessions` | The devices signed in to the account, read-only | Sees the devices signed in to your account and where they are |
| `spotify.control` | The linked Spotify's playback: what plays, and its controls | Sees and controls what your Spotify plays |
| `links.offer` | Be asked about a link the person clicks on one of the `link-hosts` it lists, and offer an app to open it in, through one of its `schemes` | Offers to open links you click in other apps |
| `window.title` | A few words after the window's title | Adds a few words to the window's title |
| `switcher.rank` | See the quick switcher's search and what it found, and reorder or leave out places | Sees what you search in the quick switcher, and can reorder it |
| `app.timing` | `startup_timing`, once a session | Reads how long Yapfast took to start |
| `relationships.read` | `relationship_add`, `relationship_remove`, `relationship_request` and `relationships.affinities` | Sees friends being added and removed, friend requests, and who Discord ranks you closest to |
| `images.web` | Pictures from the HTTPS hosts in `network`, in what it draws | Shows pictures from the sites it lists |
| `overlay` | A layer of its own pictures over the window, which clicks go through, and frames at the pace it asks | Draws moving pictures over Yapfast's window; clicks go through them |
| `oauth.login` | Sign the person in to the services its `oauth` names, in their browser, with its own app, and use the account through `http.fetch` | Asks you to sign in to another site in your browser, and uses that account |
| `activity.read` | `game_start` and `game_stop`, the game Yapfast sees this account playing | Sees which game you're playing, even when you hide it |

One of those sentences is shown in the dialog's danger tone:
`messages.send.background`, the one that lets a plugin act as the person without
them. The lines that reach something people care about are a step below it, in
bold: `messages.read`, `messages.intercept`, `messages.send`, `messages.retain`,
`servers.manage`, `uploads.intercept`, `ui.modify`, `messages.attach`,
`profile.write`, `messages.search`, `drafts.read` and `account.sessions`.

`messages.send` covers a send a person asked for. `messages.send.background`
covers everything else, and it is a deliberate exception to the rule the rest of
Yapfast keeps, in [the Discord notes](../discord/README.md#staying-within-discords-limits):
never act without a user action behind it. A plugin's own consent is what pays for
that exception, which is why the capability is the one the dialog shows in its
danger tone.

## The powerful tier

Yapfast's maintainers decided that some plugins worth having need more than
the sandbox gives: an MCP plugin that lets a local Claude read channels and send
for you, or a media picker over your own folders. Those abilities are a tier of
their own, and every one of them is behind four things at once:

1. **The manifest declares it**, and the install review shows each one as a red
   sentence in plain words — *This plugin can read every file in a folder you
   choose*, *This plugin can send your data to any website* — in a red box the
   person has to tick, *I understand what this plugin will be able to do*,
   before Install does anything.
2. **A switch in Settings → Plugins → Advanced**, *Allow powerful plugins*, off
   until the person turns it on. While it is off every powerful call is refused
   with a sentence saying where the switch is, and turning it off stops every
   local endpoint at once.
3. **A red dialog the first time each capability is used**, by each plugin,
   wherever the person is: who asks, and the same sentence. For `fs.folder` its
   button is *Choose folder*, and the folder the person picks is the only one
   the plugin ever sees. The first call waits for the answer, `pending` like any
   call answered later, and goes ahead the moment the person says yes, through
   every gate again; a no answers it `denied`, saying so. One dialog per
   (plugin, capability) is on screen at a time, and calls made while it is up
   wait on the same answer, sixteen of a plugin's at most, since each holds its
   payload in memory; past that a call is `denied` saying the person is being
   asked.
4. **One-click revoke** on the plugin's page: every powerful line has its own
   Revoke, and the next use asks again. Revoking `fs.folder` forgets the folder,
   and revoking `net.listen` stops the endpoint and retires its token.

A plugin with any powerful capability carries a red **POWERFUL** marker on its
row for as long as it is installed. The answers live in `powerful.json` beside
the lock file, written atomically; a file this build can't read is moved aside
and nothing powerful is allowed. The marketplace lists no powerful plugin as
*Reviewed* unless a maintainer has read its source, as for every listing.

| Name | Grants | Shown as |
| --- | --- | --- |
| `fs.folder` | Read, or with `"files": { "write": true }` read and write, one folder the person picks | Can read the files in a folder you choose |
| `fs.full` | The same over the home folder, or with `"scope": "disk"` the whole disk | Can read every file in your home folder |
| `net.any` | `http.fetch` to any public HTTPS host, not only the listed ones | Can send your data to any website |
| `net.listen` | An HTTP endpoint on `127.0.0.1` and `::1`, behind a token | Lets apps on this computer connect to it |
| `messages.send.agent` | Sending without a keystroke, at `agent-per-minute` (1 to 20, six unsaid) | Sends messages as you without you pressing a key |
| `net.socket` | Raw TCP, TLS over it, or a WebSocket, out; with `"socket-local": true` this computer and its network too | Can open raw connections to any server |

`messages.attach` is not powerful: it sends files the plugin made, or its own
assets, with a message, on the rails every send takes
([the account's own calls](#the-accounts-own-calls)). Sending a file from a
folder by its path (`files`) needs `fs.folder` or `fs.full` as well, which is
the tier.

### Files

The guest still has no filesystem: the helper's OS sandbox refuses every path,
and keeping it that way is worth more than WASI's preopened directories, which
would mean punching a hole in that sandbox per plugin and restarting the helper
whenever a folder changes. The parent reads and writes on the plugin's behalf
instead, through `fs.list`, `fs.stat`, `fs.read`, `fs.write` and
`fs.thumbnail` ([the ABI](abi.md#the-powerful-tiers-calls)), and every path goes
through one resolver (`yapfast-plugins/src/files.rs`):

- a path is plain parts joined by `/`; `..`, a root, a drive, a backslash or a
  NUL is refused before anything is opened;
- what it names is resolved by the operating system and must still be inside
  the folder, so a symbolic link that leads out is refused like `..`, and a
  listing leaves such links out;
- a write goes through the folder it lands in, resolved the same way, and is
  refused where its own name is a link or a folder, and anywhere for a plugin
  given the folder to read;
- a read answers at most 512 KiB at a time, a write takes at most 768 KiB, and
  a listing at most 1,000 entries.

### Networking

`net.any` widens `http.fetch` and nothing else: HTTPS only, Discord's hosts
refused, every address checked to be public, the caps — the whole gate stands,
less the host list. A plugin that also lists hosts keeps reaching those while
`net.any` waits for its answer.

`net.listen` serves on loopback and nowhere else. The two addresses are
constants in `listen.rs`, so no manifest, setting or guest can make it listen on
`0.0.0.0` or a network a stranger is on, and a peer that isn't loopback is not
served. Every request needs the plugin's token, `Authorization: Bearer <token>`:
244 random bits, made once, kept in `powerful.json` so the address survives a
restart, and shown on the plugin's page with *Copy token*. A request without it
is answered 401 and the plugin never hears of it, and the token is taken out of
the headers the plugin is handed. The `Host` header must name loopback too, so a
web page that rebinds a name of its own to 127.0.0.1 can't reach the endpoint
through a browser. One request a connection, a head of 16 KiB, a body of 1 MiB,
eight at a time, an answer in 30 seconds or a 504.

`net.socket` opens a connection for the plugin in the parent
([`socket.rs`](../../crates/yapfast-plugins/src/socket.rs)): `net.connect
{ host, port, tls?, websocket? }` is answered `pending`, and every answer after
that is the connection's news, `connected`, `data`, `text` (a WebSocket's text
message) and `closed` with its reason, which always comes last. `net.send
{ socket, text | data_base64 }` and `net.close { socket }` go the other way. TLS
trusts the system's roots, as Discord's own WebSockets do, and a WebSocket is
`wss` over it or `ws` without.

The tier's rule is that a plugin is free to do what it declared and the person
is warned rather than the plugin capped: so `net.socket` puts no limit on how
much a connection carries, how fast or for how long. The red install box says the
plugin can open raw connections to any server, and with `socket-local` that it
reaches apps on this computer too. What stays is what keeps Yapfast standing or
is part of the tier's line:

- every address a name resolves to is checked before any is dialled, and
  loopback, a private network, link-local and the rest of the gate's list
  (above) are reached only with `"socket-local": true` in the manifest;
- Discord's own hosts are refused, as the gate refuses them, since Discord is
  reached through Yapfast;
- 64 connections a plugin at once (`limits::MAX_SOCKETS`), each a task and a
  buffer in Yapfast; 48 KiB a piece and 512 KiB a WebSocket message, so a piece
  fits one answer on the pipe; 30 seconds to open, after which its task is let
  go.

Turning the plugin off, revoking `net.socket`, or turning powerful plugins off
closes every connection it holds.

### Sending as an agent

`messages.send` normally stands behind a person's click: five seconds after
one, five sends in ten seconds. `messages.send.background` lifts the click and
keeps the five-in-ten. `messages.send.agent` is the agent's version, for a
plugin a local assistant drives: it lifts the click once the person has allowed
it in its red dialog, keeps the five-in-ten, and adds its own rate, the
manifest's `agent-per-minute`, 1 to 20. While a plugin sends that way a red
strip over the conversation says *MCP Bridge is sending messages for you,
without you pressing a key*, with **Stop**, which takes the capability back at
once; it stays for two minutes after the last send. Every message any plugin
sends says so under it — *Sent by MCP Bridge, a plugin, not typed by you* —
drawn by Yapfast and not by the plugin.

### Still never

Even in this tier, a plugin never gets the Discord token, never sees or
rewrites the traffic between Yapfast and Discord, and never runs native code.
Those are the three things that would put the account itself, the person's
standing with Discord, or the computer outside every wall at once, and no
dialog makes them safe: a token can be copied once and used forever, a
rewritten request is a client that lies about itself on the person's account,
and native code is outside the sandbox the rest of this page describes. The
README says it in [what a plugin will never do](README.md#what-a-plugin-will-never-do).

## What an event needs

Subscribing isn't a capability of its own: an event arrives only if the plugin
holds the capability its contents need, so a plugin that subscribed to messages
without `messages.read` receives none of them. A hook is the same: a plugin takes
part in `message_pre_send` only if its manifest holds `messages.intercept`,
because intercepting a send is not something a plugin can do by accident.
`link_open` asks the plugins with `links.offer`, and only about a link to one of
the hosts their `link-hosts` list; `switcher_rank` asks the first plugin with
`switcher.rank`.

| Event | Needs |
| --- | --- |
| `message_create`, `message_update`, `message_delete`, `reaction_add`, `reaction_remove`, `typing` | `messages.read` |
| `room_selected`, `server_join`, `server_remove`, `group_dm_remove` | `servers.read` |
| `game_start`, `game_stop` | `activity.read` |
| `composer_change` | `drafts.read` |
| `voice_state`, `speaking` | `voice.read` |
| `presence`, `member_join`, `member_leave` | `people.read` |
| `relationship_add`, `relationship_remove`, `relationship_request` | `relationships.read` |
| `startup_timing` | `app.timing` |
| `app_focus`, `account_ready` | Nothing, since they say nothing about a person |

A read is the same: what it answers is what the capability names.

| Call | Needs |
| --- | --- |
| `app.version`, `app.locale`, `app.theme_mode`, `account.me` | Nothing: they say what Yapfast and the person's own account are, and no other person's data |
| `rooms.selected`, `rooms.get`, `rooms.list` | `servers.read` |
| `messages.get`, `messages.history` | `messages.read` |
| `people.get`, `people.member` | `people.read` |
| `media.get` | `media.read` |
| `voice.state` | `voice.read` |
| `assets.list`, `assets.get` | Nothing: they read what the plugin's own package carries, which the review listed |
| The `decorate` reply's `annotation` and `marks` | `messages.read`: they are about what the message says, and without it there is no text to style |
| The `decorate` reply's `hide` and `hide_media` | `messages.filter` |
| Keeping a deleted row on screen | `messages.retain`, declared and nothing more: the parent merges the row |
| `identity.decorate`, the `identity` activation trigger | `people.read` |
| `code_block.render`, the `code` activation trigger | `messages.read`: a block is message content |
| The `body` of a `decorate` request | `messages.read`: without it the body is empty |

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
typed in the composer, a menu row chosen or a widget used records that the
person asked that plugin for something, and for five seconds after that
`messages.send` alone covers what the plugin sends. Past five seconds, or with no
gesture at all, the send is the plugin acting on its own and needs
`messages.send.background`. A gesture is one plugin's: typing a message, or
using another plugin's button, is no gesture for this one.

## What each call needs

| Call | Capability |
| --- | --- |
| `messages.send`, `edit`, `delete`, `react` | `messages.send`; without a recent gesture any of them also needs `messages.send.background`, and together they share the limit of five in ten seconds |
| `composer.get_draft` | none: the person's own composer |
| `messages.jump_to`, `composer.set_draft`, `composer.insert` | none, and a recent user gesture: without one they answer `denied` |
| `notify.send` | `notifications` |
| `clipboard.write` | `clipboard.write`, and a recent user gesture |
| `open.url` | `open.app`, and a recent user gesture; each link then waits in the leaving-link dialog, with the theme unmodified, until the person chooses to open it |
| `discord.set_status`, `set_custom_status`, `set_activity`, `clear_activity` | `presence.write` |
| `discord.mark_read`, `set_mute` | `notifications.manage` |
| `discord.reactors` | `messages.read`: it answers what people gave to a message |
| `discord.pause_invites`, `add_emoji`, `add_sticker` | `servers.manage`, and the person's own roles on that server |
| `audio.play` | `audio.play`, whether the sound is carried in the call or is one of the package's assets |
| `audio.speak` | `audio.speak` |
| `files.save` | `files.save`, and a recent user gesture; one dialog per plugin at a time, titled with the plugin's name |
| `messages.attach`, and `messages.send` with `attachments` | `messages.send` and `messages.attach`, and the send's own gesture and five in ten seconds |
| `activity.set`, `activity.clear` | `activity.set` |
| `profile.set` | `profile.write`, and the person's yes the first time |
| `messages.search` | `messages.search` |
| `drafts.list` | `drafts.read` |
| `account.sessions` | `account.sessions` |
| `spotify.now`, `play`, `pause`, `next`, `previous`, `seek`, `volume`, `shuffle`, `repeat` | `spotify.control` |
| The `spotify_state` event | `spotify.control` |
| A keybind action in the manifest, and the input it sends | `keybinds` |
| A sound the manifest's `sounds` names | `sounds.replace` |
| `window.set_title` | `window.title` |
| `net.connect`, `net.send`, `net.close` | `net.socket`, the switch and the person's first yes |
| `discord.open_room`, `start_reply`, `start_edit` | none, and a recent user gesture: without one they answer `denied` |

A curated action wraps something Yapfast already does, so the capability is the
first of two gates and the person's own permissions are the second: a plugin may
ask a server to pause its invites, and it happens only if this account has Manage
Server there. Yapfast never acts with anything a plugin didn't declare.

## The account's own calls

These calls change or read the account itself, so each has a capability the
review shows in words a person reads, and nothing past it: the only limits are
the ones Discord and Spotify keep themselves, and the ones that keep Yapfast
running. A plugin meets what the service would refuse anyway, and nothing
Yapfast made up; the review line is the warning. Each source below was read
when this was written (November 2026).

**Files with a message.** `messages.attach` sends the plugin's own bytes, or
its assets by name, with a message, as a person's attached files go: the send
needs a click of the person's behind it or `messages.send.background`, it
shares the five sends in ten seconds, and the message says *Sent by* the
plugin under it, drawn by Yapfast. A message carries ten files, as Discord
takes them, and each up to Discord's 10 MiB for an account without Nitro.
Bytes arrive inside the call's frame, so one call carries at most 1 MiB of
them; an asset is read from the store and isn't in the frame. A file's name is
a name and no path, checked as `files.save`'s is. The parent writes the bytes
under `<data>/plugins-attach/<plugin>/` for the upload to read, and removes a
send's files once they are ten minutes old.

**An activity.** `activity.set` adds the plugin's own activity to the
presence, after the custom status, the game and the song, in the shape of
Discord's Activity Object ([docs.discord.food, presence](https://docs.discord.food/resources/presence#activity-object)):
a name of 1 to 128 characters, `details` and `state` of at most 128, `playing`,
`listening`, `watching` or `competing` (types 0, 2, 3 and 5), timestamps in
milliseconds, at most two buttons sent as their labels with the addresses in
`metadata.button_urls`, the convention Discord's own clients use (the page
says it is not enforced). A picture goes through a Discord application named
in `application_id`: a web address becomes an `mp:` key through
`POST /applications/{id}/external-assets`, Discord's media proxy, as the Rich
Presence bridge does it for a game, and any other name is looked up in that
application's assets. How often it changes is Discord's: the gateway takes
five presence updates in twenty seconds and Yapfast's gateway already holds
the rest back to the latest (`yapfast-discord/src/gateway.rs`). While the
account is invisible no plugin activity is sent, as no song is. One plugin's
activity shows at a time, the last one set; turning the plugin off takes it
away, and its row in Settings → Plugins says what it shows. Unverified: whether
Discord shows buttons on an activity with no `application_id`.

**The profile.** `profile.set` changes the bio (at most 190 characters) and the
pronouns (at most 40) with `PATCH /users/@me/profile`
([docs.discord.food, user](https://docs.discord.food/resources/user#modify-user-profile)),
the request Yapfast's own profile editor makes, and the custom status the way
the status menu sets it. It is asked about the first time each plugin uses it,
in the dialog the powerful tier's first use opens, without waiting for that
tier's switch, and the first call waits for the answer as the tier's do; the
plugin's page has Revoke for it.

**Search, drafts, devices.** `messages.search` is the search box's own search
of a conversation or a server (Discord's page of 25), and Discord's search rate
limit is the account's. `drafts.list` answers the drafts held in the composer.
`account.sessions` lists the devices signed in, from `GET /auth/sessions`
([docs.discord.food, authentication](https://docs.discord.food/authentication#auth-session-object)),
up to 50, and never signs any out.

**Affinities.** `relationships.affinities`, under `relationships.read`, reads
Discord's own scores of who the account is closest to, the ones
ImplicitRelationships sorts by, read-only and asked of Discord each time
(Yapfast keeps none): `GET /users/@me/affinities/users` (`{ user_id, affinity }`
each), `GET /users/@me/affinities/v2/users` for `"detailed": true` (the DM,
call, server-message and overall probabilities and ranks, `is_friend` and the
two segments), and `GET /users/@me/affinities/guilds` for `"of": "servers"`
([docs.discord.food, user](https://docs.discord.food/resources/user#get-user-affinities)).
People are named by their Discord user id, as the relationship events name
them, and servers as `rooms.list` does; each list is highest first.

**Spotify.** Discord lends the account's linked Spotify an access token
(`GET /users/@me/connections/spotify/{id}/access-token`), which is how Yapfast
already shows "Listening to Spotify" (`yapfast-discord/src/spotify.rs`) and how
Vencord's SpotifyControls controls playback. Yapfast makes the requests to
`api.spotify.com/v1/me/player` itself, and the token never leaves
`yapfast-discord`: a plugin names what it wants. Spotify's reference says of
every control, "This API only works for users who have Spotify Premium"
([developer.spotify.com](https://developer.spotify.com/documentation/web-api/reference/start-a-users-playback));
a free account's call is answered with an error saying Premium is needed
(`PREMIUM_REQUIRED`, the reason Spotify's forum shows in its 403; the
reason's name is not on the current reference pages, so a 403 that says
Premium is read the same way). No device playing is an error of its own, and
Spotify's own 429 is passed back with its wait. `spotify_state` follows the
song the session already shares, so it needs no request and is quiet while
"Display Spotify as your status" is off.

**Keybinds and sounds.** A manifest names its keybind actions; the person binds
keys to them in Settings → Keybinds, where each shows as *Plugin: action*.
Keys another keybind or push to talk already use are refused for a plugin's
action, and the plugin is told only that its action ran. A sound the manifest
names replaces Yapfast's for that event, at the volume and on the speakers
Yapfast's own would play; themes have no sound packs in this build, so this is
the one path a sound is replaced by.

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
  broadcast or multicast, the reserved, benchmarking and IETF ranges, a Teredo
  tunnel, and an IPv4 address written the IPv6 way, including through NAT64 or
  6to4, which are checked as the IPv4 address they reach: each of those reaches
  a machine that never expected a plugin to be able to reach it.
  The name is resolved here, the addresses are checked, and the connection goes
  to those addresses and no other, so a second lookup that answers differently
  finds nobody left to connect anywhere.
- **Past the caps.** A response is read under 4 MiB unless the manifest asked for
  more, up to 16 MiB, and past it the stream is dropped rather than read to the
  end. A plugin may have 4 requests in flight while the whole helper has 16, and
  a request that hasn't connected in 10 seconds, or finished in 180, is dropped.

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
yes. So is leaving it unanswered: after a minute (`dialogs::CONSENT_WAIT`) it
denies for this time only, keeps nothing in `grants.json`, and says so while it
waits, counting down (*If you don't answer, it's denied in 42 s, for this time
only.*). A plugin's request never waits on a person who walked away, and a yes is
never given for them. The question belongs to the application and not to the
Settings page, so it is asked wherever the person is and survives the page
closing. It is drawn in Yapfast's own look whatever colours plugins set.

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
it through, an answer still arriving from that host stops at its next piece,
and a request waiting on a dialog about that host is refused at once. Turning a plugin off forgets its session answers and closes its dialog.
Removing a plugin forgets its stored answers too, so whatever is installed under
that id next is asked again.

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
