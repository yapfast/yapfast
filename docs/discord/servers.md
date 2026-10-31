# Servers

## Permissions and roles

What the account may do in a server is its @everyone role's permissions
and those of its roles, everything for the owner and administrators
(`Server::server_permissions`); in a channel, the channel's overwrites for
@everyone, the roles and the member then apply
(`Server::channel_permissions`). Acting on another member, to kick, ban,
time out, rename or change their roles, also needs to outrank them: nobody
outranks the owner, the owner outranks everyone else, and otherwise the
highest role must be higher (`Server::outranks`). A role can be given or
taken by those who may manage roles and whose highest role is above it,
or by the owner, but never @everyone or one an integration manages
(`Server::may_grant`). The core asks through `Application::may`,
`outranks` and `may_grant`, and `server_roles` lists a server's roles,
highest first, with their colours, from the server the latest snapshot
shares. A category keeps its overwrites too, so `may` answers for one by
its room id, and the channel list leaves out an empty category its
permissions hide. Members' roles are learned with their names; member list rows and
`ChatMember::roles` share each member's list with them rather than copying
it.

## A server member's menu

Right-clicking someone in a server channel's member list, or a message's
author there, opens their menu in the order of Discord's: Profile, Mention,
Message, Call, Add note and the friend nickname; Change nickname, [Invite to
server](#invite-to-server), friendship, Ignore and Block; Roles; in red,
Time out, Kick and Ban, each naming them; then Copy ID, last as in
Discord's `GuildChannelUserContextMenu`. In a voice channel it has more; see
[Someone's menu in a voice channel](#someones-menu-in-a-voice-channel).
What shows depends on this account's permissions and on its
highest role against theirs, checked when the menu opens
(`application/members.rs`). The member's roles and timeout come from the
service's names, which each snapshot shares as `SharedNames` rather than
copying them.

- Mention puts `@Name ` in the composer, which sending turns into `<@id>`,
  when the channel can be written in.
- Message opens the DM with them, or starts one with any member through `POST
  /users/@me/channels`. Call opens the DM and starts its call; with a friend
  Yapfast has no DM with, it starts the DM first and calls once it opens.
- Add note works for any member, and the friend nickname for friends, without
  a DM.
- Change nickname: another's needs Manage Nicknames and outranking them,
  and is `PATCH /guilds/{id}/members/{user}` with `{"nick": ...}`; empty
  resets it. One's own entry offers Edit server profile in its place, as
  Discord's menu does, which opens the [per-server profile
  editor](people.md#editing-ones-server-profile).
- Roles needs Manage Roles. Its submenu lists every role but @everyone,
  highest first, ticked where they hold it. Roles at or above this account's
  highest, and those an integration manages, show greyed, as Discord shows
  them. Ticking one is `PUT` or `DELETE
  /guilds/{id}/members/{user}/roles/{role}`; the box changes at once and the
  menu stays open.
- Time out needs Moderate Members and outranking them, and isn't offered for
  administrators, whom Discord doesn't let anyone time out. Its dialog offers
  Discord's lengths, 60 seconds to a week, and a reason, then `PATCH
  /guilds/{id}/members/{user}` with `communication_disabled_until`. Someone
  timed out is offered Remove timeout instead, which sends null, and the
  member list marks them with a red clock beside their name.
- Kick needs Kick Members and outranking them: a confirmation with a reason,
  then `DELETE /guilds/{id}/members/{user}`.
- Ban needs Ban Members and outranking them: how far back their messages go
  (none, the last hour, 6, 12 or 24 hours, 3 or 7 days) and a reason, then
  `PUT /guilds/{id}/bans/{user}` with `delete_message_seconds`.
- Reasons go to the server's audit log in the `X-Audit-Log-Reason` header,
  URL-encoded and cut to the 512 characters Discord keeps.
- One's own entry has Profile, Change nickname, Roles and Copy ID. Nobody
  outranks the owner, so nobody kicks, bans or times out the owner.

## Someone's menu in a voice channel

Right-clicking someone in a server's call, or under a voice channel in the
channel list, opens their volumes above their menu as a member
of the server, with what Discord's client adds there, in its order:

- After Add note: Mute, a box that mutes them for this account alone and
  keeps their volume for when it's unticked, and Mute Soundboard, a box
  that silences the soundboard sounds they play, for this account alone;
  Yapfast remembers both, as Discord's client keeps them locally. One's own
  entry, in the call Yapfast is in there, has Mute and Deafen boxes for this
  account's own instead, and no volume.
- After Roles: Move to, a submenu of the server's other voice channels.
  Someone else's lists those this account may move them to: it needs Move
  Members in their channel and in the other, which it or they may join. Its
  own lists those it may join, and choosing one joins it there, as Discord's
  client does. Choosing one for someone else is `PATCH
  /guilds/{id}/members/{user}` with `{"channel_id": ...}`.
- In red, before Time out: Server mute and Server deafen, boxes ticked from
  their voice state's `mute` and `deaf`, which need Mute Members and Deafen
  Members, and send `{"mute": ...}` or `{"deaf": ...}`; then Disconnect,
  which needs Move Members and sends `{"channel_id": null}`, without asking,
  as Discord's doesn't. Neither needs outranking them, as on Discord.

The tracker keeps a voice state's own `self_mute` and `self_deaf` apart from
the server's `mute` and `deaf`, so the channel list and the call show a
server mute or deafen in red, as Discord's client does. Discord's Disable
Video and Apps aren't offered yet. In a DM's call or a
Matrix room's, the right click still sets only the volume.

Discord answers each change with GUILD_MEMBER_UPDATE, or for a kick with
GUILD_MEMBER_REMOVE, and for a ban with GUILD_BAN_ADD as well, and sends the
member list's subscribers its operations. Yapfast learns the member's
nickname, roles and `communication_disabled_until`, shows the open member
list again when it lists them, and renders the open channel again, so names,
colours and the list follow. A member who leaves is forgotten. A nickname,
timeout, kick or ban that fails says why in its dialog; a role says so in a
notice.

Discord's menu also has Invite to listen (Spotify), Apps and Open in
moderator view, which Yapfast leaves out for now.

## Invites

Invite to server, in a server's menu or its header, and Invite to channel,
in a channel's, make an invite with `POST /channels/{id}/invites` and a
body of `max_age`, in seconds or 0 for good, `max_uses`, 0 for any number,
and `temporary` false; the answer's `code` makes `https://discord.gg/{code}`.
The dialog asks for one at once with the defaults of Discord's dialog,
7 days and any number of uses, and makes another when asked with its
other choices. Discord answers 204, with no invite, when a server paused
invites, which the dialog says. A channel's invite lands people in it. A
server's lands them in the open channel when that's in the server, else
in the first of its channels in Discord's order; each only where the
account may see the channel and create invites (`CREATE_INSTANT_INVITE`),
and the choice shows only then. docs.discord.food doesn't say which
channel Discord's client picks for a server, so Yapfast takes the one it
would open.

## Invite to server

Someone's menu has an Invite to server submenu, as Discord's has, in a
server member's menu after Change nickname, and in a DM's, a group DM
member's and a friend's before friendship, Ignore and Block. It lists, in
the rail's order, the Discord servers where this account may create invites
(`CREATE_INSTANT_INVITE` on the server) and has a channel to invite to,
leaving out only the server they're shown as a member of. Discord's client
lists the rest whether they're in them or not, as it doesn't ask which
servers it shares with them, and Yapfast does the same. Nobody is offered it
for themselves, a bot, or someone blocked or ignored. Yapfast knows bots from
the `bot` field of user objects, kept with the service's names. The list is
built when the menu opens, from the servers the snapshot shares.

Choosing a server picks its channel as Discord's client does: of the text
and announcement channels this account may see and invite to, the first by
`position`, not the channel list's order, which goes by category first;
failing those, such a voice channel. Then three requests:

- `POST /channels/{id}/invites` with `{"max_uses": 1, "unique": true}`, so
  it works once and lasts Discord's default day, and an
  `X-Context-Properties` location of "User Invite Context Menu".
- `POST /users/@me/channels` with their id, which finds the DM or starts it.
- `POST /channels/{dm}/messages` with the link, `https://discord.gg/{code}`,
  as its content, and a location of "context_menu".

Discord's client shows nothing more: the DM's CHANNEL_CREATE and the
message's MESSAGE_CREATE list the DM and show the link in it. Where
Discord's says nothing on a failure, Yapfast says why in a notice. Discord's
client also offers a server without the permission when it has a vanity
URL, and sends that instead; Yapfast doesn't keep servers' vanity codes, so it
leaves those out.

## Joining by invite

The "+" at the foot of the server rail opens Join a Server, which takes an
invite link or a bare code, a server's vanity code too, as Discord's dialog
does: `discord.gg/{code}`, or `/invite/{code}` on discord.com, discordapp.com,
canary and ptb, with any query left off; otherwise the last part after a `/`
(`invite.rs`). Clicking an invite link in a message opens the same dialog
without the field, even when the link's text says something else, instead
of the browser. Either looks the invite up first and shows its server,
icon, and how many members it has and how many are online, or the channel
when it has fewer than five members and none online, as Discord's invite
card does; nothing joins until the Join Server click. Discord's docs warn
that accounts joining many servers in a short time get flagged, so Yapfast
never joins by itself.

- `GET /invites/{code}?with_counts=true&with_expiration=true&with_permissions=true`,
  as Discord's client resolves an invite, with `&inputValue=` and what was
  typed into the dialog when it was. An invite that isn't to a server's
  channel, such as a friend invite, is refused.
- `POST /invites/{code}` with `{"session_id": ...}`, this gateway session's
  id from READY, and an `X-Context-Properties` of the location, "Join Guild"
  from the dialog or "Desktop Invite Modal" from a link, with
  `location_guild_id`, `location_channel_id` and `location_channel_type`
  naming the invite's server and channel, as Discord's client sends them.

Discord often asks for a captcha before joining. Yapfast shows it as linking
does (`application/join.rs`), and the dialog says so and can open it again. The
server's GUILD_CREATE lists it, and Yapfast opens the invite's channel once
it's listed. An invite to a server the account is in opens the server
instead of joining, at once for a link; the dialog says so and its button
opens it. Discord's refusals come back in the dialog's words: an invalid or
expired invite (10006, 50020), a ban (40007), too many servers (30001), a
full server (30019) and paused invites (40069).

When the answer has `show_verification_form`, the server asks new members
to fill in its form, such as accepting its rules, before they may talk.
Yapfast doesn't show forms or onboarding questions yet, so the dialog says
so and opens the channel in Discord's web app, where the form waits. A voice
channel's invite joins the server and opens the channel without joining the
call. Discord's client also sends `invite_instance_id` from a message's
invite card, which Yapfast doesn't show yet, so it leaves that out.

## Event links

Clicking a link to a server's scheduled event, `/events/{server}/{event}`,
opens a dialog with the event's name, when it starts and ends, where it
happens, who made it, how many are interested and what it says, with
Discord's Interested button, which becomes Remove interest once pressed. A
link to one occurrence of a recurring event, with a third id, shows the
whole series, and interest in it is for the series (`link.rs`,
`application/scheduled_event.rs`). Links to a server the account isn't in,
the guildless `/events/{event}`, and any while Discord is offline open in
the browser.

- `GET /guilds/{id}/scheduled-events/{event}?with_user_count=true` for the
  event and its count, and `GET /users/@me/scheduled-events?guild_ids={id}`
  for whether this account is interested in the series: a subscription with
  `response` 1 and no `guild_scheduled_event_exception_id`, as one for a
  single occurrence has.
- `PUT /guilds/{id}/scheduled-events/{event}/users/@me` with
  `{"response": 1}`, as Discord's client sends it, or `DELETE` on the same
  path, from the click only. An ended or canceled event offers no button.

Yapfast doesn't keep a server's events or follow their gateway events; the
dialog shows them as they were when it opened.

## Channels and categories

Where the account may manage channels (`MANAGE_CHANNELS`, on the server for one
made there, on the category for one made in it, on the channel for the rest),
Create channel and Create category make one with `POST /guilds/{id}/channels`
and a body of its `name`, `type` (0 text, 2 voice, 4 category), `parent_id` for
a category, and for text its `topic`, `rate_limit_per_user` and `nsfw`. Edit
channel opens the channel's or category's [settings](server-settings.md#channel-settings).
Duplicate channel reads the channel and makes another with its type, category,
permission overwrites, topic, age restriction, slowmode and voice settings
under the name given, as Discord's client does. Delete channel uses `DELETE
/channels/{id}`; deleting a category leaves its channels outside any, each with
a CHANNEL_UPDATE before the CHANNEL_DELETE. Yapfast changes its list only from
CHANNEL_CREATE, UPDATE and DELETE, which the service already applies to the one
server they name, so the sidebar rebuilds that server's rows and keeps the
others. Names are 1 to 100 characters and topics up to 1024, checked before
asking.

## A channel's menu

Right-clicking a server's channel offers, in order, as Discord's menu
does: Mark as read when it has unread messages, Join voice for a voice
channel, then Invite to channel, Pin channel to top or Unpin channel, the
channel list's choice while it [lists only the channels
followed](#following-channels), and Copy link, then Mute channel and
Notification settings for a text channel, then Edit channel, Duplicate
channel, Create channel and Delete channel where the account may manage
channels, and Copy ID. A thread's menu has none of inviting, pinning,
following or managing.

### Pinning a channel

Pin channel to top, as Discord's client has it, moves a text or voice
channel out of its category into a section at the top of the server's
channel list, headed "Pinned channels", above every category and the
channels outside any. The channel shows only there, with its threads under
it. Pinned channels list by their own `position`, voice channels after
text ones, not in the order they were pinned. The section has no menu and
doesn't fold, and a line ends it. Once a channel is pinned its menu says
Unpin channel. Discord's client sets no limit on how many, and offers it
for every channel but threads and categories.

The pin belongs to the account, so it follows it to every client: it's
the `FAVORITED` flag, `1 << 11`, of the channel's override in the user
guild settings, the same `channel_overrides` that hold its mute and level
(see [Notification settings](messages.md#notification-settings)). Discord's client
pins with `PATCH /users/@me/guilds/settings` and a body of
`{"guilds": {"<guild>": {"channel_overrides": {"<channel>": {"flags":
6144}}}}}`: the override's flags with `FAVORITED` set, and with
`OPT_IN_ENABLED`, `1 << 12`, too if it wasn't. Unpinning sends the flags
with `FAVORITED` cleared and the rest as they were. Discord answers with
each server's settings, and USER_GUILD_SETTINGS_UPDATE tells every session.
`yapfast_discord::notifications` reads the flags with each override and
writes the body (`Override::pin_flags`, `flags_body`); Yapfast changes the
list when the update comes, as it does for a mute.

Yapfast works out the pinned channels when it lists a server's rooms, which
happens only when the server's snapshot changes, and keeps them in the
server's `ChatServer::pinned`; the sidebar rebuilds only when the servers
it hashes change. Discord's "Move to…", which reorders the server's
channels for everyone, and its Favorites server, kept in the settings
protobuf, are different features; Discord's client only hides "Move to…"
and dragging for a pinned channel, which Yapfast has neither of.

### Following channels

A Community server's members may list only the channels they follow, as
Discord's client has them do from the server's menu by unticking Show
All Channels, from a channel's menu, and from [Browse
Channels](#browse-channels). Discord keeps it with the account, so its list
matches Discord's on every client. Its server menu, and the dropdown of its
header, tick Show All Channels above Hide muted channels; a server that
isn't a Community server (`COMMUNITY` among its `features`) has neither the
choice nor the filter, as in Discord's client.

With Show All Channels off, the server's own `flags` in the user guild
settings hold `OPT_IN_CHANNELS_ON`, `1 << 14`, and the list shows a text
channel only if its override, or its category's, holds `OPT_IN_ENABLED`,
`1 << 12`; its threads go with it. One left out still shows while it, or a
thread of it, is open or mentions the account, and a pinned channel shows
in the pinned section all the same. A channel left out marks the server on
the rail only with its mentions, not its unread messages. Voice channels
all show. Discord's
client differs in where it puts some of these: a channel left out that's
open or mentions the account shows in a "Suggested" section at the top,
which also suggests new channels, and the voice channels not followed show
in a section of their own after the categories. Yapfast keeps each in its
category.

A channel's menu then offers Remove from channel list or Add to channel
list, after pinning, by its own flag, or Unfollow channel category when its
category is followed, which removes the category; a category's menu offers
Add to channel list or Remove from channel list after Mark as read.
Discord's client writes them as it pins (`PATCH
/users/@me/guilds/settings` with the override's `flags`): adding sets
`OPT_IN_ENABLED`, and removing clears it and `FAVORITED`, unpinning the
channel. Show All Channels is `PATCH /users/@me/guilds/{guild}/settings`
with the server's `flags`, `OPT_IN_CHANNELS_ON` cleared or set and the rest
as they were. Turned off with no channel or category followed, the same
body follows every text and voice channel the account sees, in
`channel_overrides`, so the list doesn't empty; Discord's client does that
too unless the account finished the server's onboarding, and adds the
onboarding's default channels, which Yapfast doesn't fetch.

`ChatRoom::unlisted` marks a channel left out, and each thread of it, worked
out with the pinned channels when the server's snapshot changes, rather than a
list of their ids on the server. The channel list drops them as it drops muted
channels in a server that hides them (`AppState::scoped_rooms`).

### Browse Channels

A Community server's channel list starts with Browse channels, as
Discord's client has it, which opens Browse Channels in place of the
conversation: every text and voice channel the account sees, but threads,
under its category's heading in the channel list's order, with a search of
their names. Each channel shows its icon, name and topic, a button that
opens it (View, or Join for a voice channel, Open for a forum) and a box
ticked while the list follows it; clicking the row ticks or clears it. A
category's heading has a Follow category switch, and the channels of a
followed category show ticked and can't change by themselves, as Discord's
tooltip says: "You must unfollow this category to add or remove specific
channels."

Ticking or clearing writes what a channel's menu writes, but it also
turns Show All Channels off, as Discord's client does, in one `PATCH
/users/@me/guilds/{guild}/settings` with the server's `flags` and the
channel's. With Show All Channels on and nothing followed every box shows
ticked, and clearing one follows every other channel the account sees.
Discord's client lets a page of changes wait and sends them when it
closes; Yapfast sends each at once and ticks the box when Discord confirms
it. Its "Channels & Roles" tab of onboarding questions, the Create button
and the channel's subtitle of active threads and when it was last active
are left out, and View opens the channel rather than showing it beside the
list.

`Application::open_browse` keeps the rows in `AppState::browse` only while
it shows, and works them out again from each snapshot.

## Threads

A text channel's threads (types 10, 11 and 12) are channels of their own,
with a `parent_id` and `thread_metadata`. A user account's READY and
GUILD_CREATE carry only the active threads it's in, each with its own
`member`; THREAD_CREATE, UPDATE, DELETE and LIST_SYNC change them, and
THREAD_MEMBER_UPDATE and THREAD_MEMBERS_UPDATE say when the account joins or
leaves one. The service keeps them in the server's snapshot, newest first,
with the thread's owner, privacy, archiving, locking, message count (which
leaves out the message a thread was started from) and the account's
membership: when it joined, its mute and its notification level (flags 2,
4 and 8 for all messages, mentions only and nothing). A thread takes its
permission overwrites from its parent: it shows with View Channel, a
private one only to its members or with Manage Threads, and writing in it
takes Send Messages in Threads, which a locked thread also asks Manage
Threads for.

The sidebar lists each active thread the account is in under its channel,
joined to it by a line, with its own unread state and mentions, read at
join time until Discord's read state says otherwise. A thread notifies
as its own level says, else as its channel does; one the account isn't in
notifies only of mentions, and a muted thread not at all. Opening one shows
it in place of the channel, its header reading "general › Plans" with the
channel's name a link back, and the channel's member list beside it.

As in Discord's client (`ActiveJoinedThreadsStore`), a thread goes quiet
its `auto_archive_duration` after its newest message, its start or its
`archive_timestamp`, whichever came last, though Discord may archive it
much later. A quiet thread lists only while it's unread and not muted,
mentions the account, or is on screen. The service wakes when the next
thread goes quiet and publishes a snapshot, so the row goes with nothing
else happening (`Tracker::next_quiet`).

A message that started a thread carries it as `thread`, and shows a chip with
its name and count under it; the THREAD_CREATED notice (type 18) links to the
thread through its `message_reference`, and a thread's first message,
THREAD_STARTER_MESSAGE (type 21), shows the message it was started from.
Clicking a chip opens the thread beside the channel, as Discord does, in place
of the member list: its messages, its history and a composer of its own, which
replies, edits and reacts as the timeline does, and takes files from its + and
pastes as the channel's does. Files dragged over it go with its message, and
the card saying where moves over it; the rest of the window takes them for the
channel. Wayland's data device says where the pointer is; winit doesn't on X11
and Windows, so Yapfast asks the X server, or Windows, while files hover. Its
button shows it in place of the channel, and closing it, or opening another
channel, brings the member list back. A thread the account isn't in is read
with `GET /channels/{id}` and listed while it shows, so it reads and marks read
like any other; writing in it joins it, as Discord does.

A thread's menu offers Join thread, or Leave thread for one the account is in,
after Mark as read, as Discord's client does (its `thread-context` menu). So
does the More button (…) in a thread's header, after its member list, and in
the header of a thread beside its channel: the same menu less Mark as read, as
Discord's "Toolbar Overflow" and "Sidebar Overflow" menus start with joining or
leaving; their other items, such as closing or locking the thread, Yapfast
doesn't have yet. Discord's client checks nothing else: not archiving, locking
or permissions. It sends `POST /channels/{id}/thread-members/@me` to join,
though docs.discord.food says PUT, and `DELETE` to leave, each with the menu's
`location` ("Context Menu", "Toolbar Overflow" or "Sidebar Overflow") and no
body; Discord answers with THREAD_MEMBERS_UPDATE. A thread left stays on
screen, as in Discord's client, and stops listing in the sidebar once it's off
it.

### A channel's list of threads

The Threads button comes first in a text or announcement channel's header,
and in a thread's for its channel, as in Discord's client, which no
permission hides. It opens the channel's threads under it, as Discord's
popout does (`application/threads.rs`, `ThreadsPopover`): "N joined
threads", the active ones the account is in, then "N other active
threads", then "Older threads", the archived ones, each section with the
newest message first. An active thread's row shows its newest message, its
author's avatar and name in their role colour, and how long ago ("5m ago",
">30d ago"), or "No recent messages"; an archived one's shows "Started by"
its owner and when it was last active. On the right are a few of its
members' avatars and how many are in it. Clicking a row opens the thread in
place of the channel, and Shift-click beside it, as Discord's does; a
thread the account isn't in opens as any other does, without joining it.
Sized as in Discord's client (its CSS and list in the web bundle): each
thread is a card 72px tall with 8px under it, the first section's heading
32px tall and later ones 64px, and the popout 35% of the window wide, from
480px to 600px, and as tall as its rows, from 400px to 80% of the window.

What fills it, as Discord's web client does it: the active threads come
from the gateway, never a request. Subscribing to a server with `threads`
(op 37), which Yapfast does for the server on screen, brings THREAD_LIST_SYNC
with its active threads the account may see and each one's newest message
(`most_recent_messages`). The service keeps each thread's newest message
(its text cut to 200 bytes), its member count and preview ids in the
server's snapshot, and follows them with MESSAGE_CREATE and
THREAD_MEMBERS_UPDATE; the list reads them from the snapshot as it shows,
without copying the server. Archived threads come only with Read Message
History, when the list opens and then a page of 25 at a time, from `GET /channels/{id}/threads/search` with
`archived=true&sort_by=last_message_time&sort_order=desc&limit=25`
and an `offset`, as Discord's client asks; it never calls the
`/threads/archived/` endpoints. The next page loads only as a scroll by
hand ends within 200px of the list's end, as Discord's client does it, so
a list too short to scroll loads no more. Archived threads the snapshot
knows are merged in where the pages reach, and none show before the first
page answers. Search, which also needs Read Message
History, runs on Enter with `name=` and no `archived`, so it finds active
and archived threads, and its results replace the sections until it's
cleared; the first search of a channel can answer 202 while Discord indexes
it, and Yapfast asks again. A private thread shows only to its members and
those with Manage Threads. The pages, the search's results and the channel
objects the list found, from which a thread opens without another `GET
/channels/{id}`, are dropped when the list closes or another room opens.

### Starting a thread

A thread starts as in Discord's client, from three places: Create Thread in
a message's menu, the list of threads' Create button (and its empty
list's Create Thread), and Create Thread in the composer's + menu, which
then also holds Upload a File. Each opens Discord's "New Thread" form beside
the channel, in place of its member list (`application/threads.rs`,
`NewThreadPanel` in `ui/thread.slint`): Thread Name, up to 100 characters;
Private Thread, "Only people you invite and moderators can see"; and the
first message, "Enter a message to start the conversation!", which takes
the channel composer's draft. From a message the form shows that message,
the name is optional and the thread public; an empty name becomes
Discord's default: the message's embed title, else its first line without
leading `#`, `-` or spaces, grown word by word up to 40 characters and
then "...", else "Thread". Without a message a name is required ("Thread
Name is required"), and a first message always is ("Starter Message is
required"). Closing a form with something written asks "Are you sure you
want to go?" first.

Discord's client offers a public thread with Create Public Threads and
Read Message History, and a message's Create Thread only on a delivered
message without a thread, whose menu offers View Thread instead; a
private one takes Create Private Threads, in a text channel only, and
where only that is allowed, the form starts one. Creating sends `POST
/channels/{id}/messages/{message}/threads` or `POST
/channels/{id}/threads` with the `name`, the `type` (11, 12 for a
private one, 10 in an announcement channel), the channel's
`default_auto_archive_duration`, else 4320 minutes, and the `location`
Discord's client gives ("Message", "Thread Browser Toolbar", "Thread
Browser Empty State", "Plus Button"). Discord then sends THREAD_CREATE
with `newly_created` and THREAD_MEMBERS_UPDATE with the account in it,
updates the message, which now carries the thread, and posts a
THREAD_CREATED notice in the channel for a public thread started without a
message or from an older one. As Discord's client does, Yapfast opens the
thread beside the channel and then sends the form's first message as an
ordinary message in it.

## Forums

A forum (type 15) lists among the text channels, by its position, with its
own icon, and shows its posts in place of messages, as Discord's client
does (`application/forum.rs`, `ForumPanel`). A media channel (type 16) has
a forum's fields and is one here, with a forum's icon and its list of
cards, where Discord's client shows a gallery. Its posts are public threads
whose parent is the forum, so everything above holds for them: posts the
account is in list under the forum in the sidebar, the active ones come
with THREAD_LIST_SYNC once the server is subscribed (with `threads`, but
no channel, since a forum has no member list), and archived ones come a
page of 25 at a time from `GET /channels/{id}/threads/search`, as a
channel's list of threads asks. The service keeps the forum's tags
(`available_tags`), whether a post must take one (flag 16), its default
sort order and its default reaction, and each post's `applied_tags` and
whether it's pinned (flag 2). A post's first message has the post's own id
and isn't counted in its `message_count`.

The forum shows its active posts, the pinned one first, then by latest
message, or by when they were made if the forum sorts by creation, then
"Older posts", the archived ones by latest message, then what a search of
their titles finds while one shows. Each card shows, as Discord's list
layout does: a pin for a pinned post and its first three tags, "+N" for
more; its title, bright when the account opened it and has messages to
read there, and "NEW" for a post made since the forum was last read that
the account never opened (it has no read state of its own); the first
message's author in their role colour and its text, or "Original message
was deleted"; the reaction most given, with super reactions, or the
forum's default reaction without a count; the replies; how long ago it was
last active, or made; and its first picture, a 72px thumbnail from a file
or an embed. Clicking a card opens the post beside the forum, as Discord's
does, and Shift-click in its place.

First messages, as Discord's client gets them: an archived page or a
search brings its posts' `first_messages`; the active posts' come from
`POST /channels/{id}/post-data` with `{"thread_ids": [...]}`, at most ten at
a time, which answers with each post's `first_message`, `most_recent_message`
and `owner`. Yapfast asks for the first 25 cards' when the forum opens and
for 25 more each time the list scrolls near its end, one request at a time,
as Discord's client asks for the cards it shows. The cards, pages and first
messages are kept only while the forum shows. Opening the forum marks it
read, which acknowledges its newest message, as Discord's client does; a
forum's newest message is its newest post, so a post made since makes it
unread (THREAD_CREATE with `newly_created`).

New Post, beside the search, opens the form for a post in place of the
list, as Discord's client opens it at the top of a forum
(`application/new_post.rs`, `NewPostForm`). Discord's client offers it
with Send Messages on the forum, which is all posting takes. The form has
a title of at most 100 characters; up to five of the forum's tags, its
moderated ones only with Manage Threads; the forum's topic as its "Post
Guidelines"; and a first message, whose files are the forum's staged ones,
so files dropped on a forum open the form. Posting asks for a title, then
a message or a file, then a tag if the forum requires one (flag 16), with
Discord's client's words. The post goes as Discord's client sends it: the
files are uploaded to the forum, then `POST
/channels/{id}/threads?use_nested_fields=true` takes the `name`, the
forum's `default_auto_archive_duration` (three days without one),
`applied_tags`, and the first message nested as `message`, with its
`content` and `attachments`. Discord sends THREAD_CREATE, then the first
message, whose id is the post's; the post opens beside the forum. What was
written stays while other rooms show; closing the form after writing asks
first.

## Leaving a server

Leave server, in a server's menu or at the end of its header's dropdown, asks
first, with Discord's words, then sends `DELETE /users/@me/guilds/{id}` with
`{"lurking": false}`; Discord answers 204 and sends GUILD_DELETE, without
`unavailable`, and GUILD_MEMBER_REMOVE. The tracker already drops a server on
such a GUILD_DELETE, and Yapfast goes to the DMs if it was open. The owner
isn't offered it: Discord has them transfer or delete the server instead.

A server's privacy settings, which Discord's menu also has, are left for
later: they live in the settings protobuf's guild entries, which Yapfast
would have to read and write back whole, beyond a menu's reach.
