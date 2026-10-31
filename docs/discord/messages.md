# Channels and messages

Yapfast shows Discord live from its own Discord connection. Every server, channel
and DM is visible without copying anything into Matrix.

Yapfast lists the linked account's Discord servers in the rail with their
channels. Channels the account's roles and channel overwrites keep it out of
show greyed out with a lock, as in Discord; channels hidden from it appear the
same way only with the "Show channels you can't access" setting, which is off
by default. Server text channels are listed under their Discord categories and
open in Yapfast's timeline. Clicking a category's heading collapses it, leaving
the open channel, unread ones and voice channels with people in them, as in
Discord; Yapfast remembers which categories are collapsed, per server. History
loads from Discord 50 messages at a time (`GET /channels/{id}/messages` with
`before` the oldest loaded message and `limit=50`) as the timeline scrolls near
its top, one page at a time per channel, until a short page shows the channel's
first message is there; and new messages, edits, deletions and reactions arrive
live. A jump to a message that isn't loaded shows the messages around it; see
[jumping to a message](#jumping-to-a-message). Opening a channel subscribes to
its server, as the web client does, because Discord sends large servers'
messages only to subscribers. Yapfast sends, replies, edits, deletes, reacts
and sends files through Discord's cloud uploads. Mentions and timestamps show
as text, custom emoji and stickers as pictures, every attachment shows, images
with previews scaled by Discord's media proxy, and channels the account can't
send in are read-only. Bots and webhooks have a BOT tag after their name. An
author's name takes their highest coloured role's colour, once their roles are
known: live messages carry them, and for history Yapfast asks Discord for the
authors' member profiles.

Yapfast keeps the open channel's messages in memory, and those of the 40
channels open before it, so going back to one shows it at once. A closed
channel keeps its newest 200 messages as metadata and follows new messages,
edits, deletions and reactions while it's kept. Opening it again loads the
latest page in the background and fits it in: messages kept within the page's
span that it lacks were deleted meanwhile, and when a full page starts after
everything kept, more came than a page holds, so only the page stays. The UI
keeps the same 40 channels' last timeline and member list, so going back shows
them before the chat side answers, without "Loading messages...". Channels also
go to disk, as the startup cache below describes, so a channel opened after a
restart shows its saved messages while the latest page loads and fits in the
same way.

All channels share one 8 MiB cache for decoded preview pixels, with least
recently used eviction. Only messages in or near the timeline viewport
request previews, and previews show as loading until then; evicted previews
can be decoded again from the disk cache. The UI drops decoded preview
handles when closing a channel, so remembered UI timelines do not pin pixels
outside the shared cache.

The open channel renders each message for the timeline once, with its
author's name, colour and avatar and its mentions as names, and keeps it. A
new message, an edit, a reaction or a preview that arrived renders only that
message again; a change to someone's name, avatar or roles, to the
server's roles or channel names, or to who's blocked, renders them all. A closed channel drops its rendered messages,
since they hold its previews.

Unread dots and mention counts follow Discord's read states, mutes and
@everyone and role suppression, including passive updates from large servers.
A message that calls on this account is highlighted in the timeline, as in
Discord: one that mentions it, one of its roles in the server, or @everyone
or @here, from a message's `mentions`, `mention_roles` and
`mention_everyone`, which count only what the author may mention. The
account's settings for the server can suppress @everyone and @here, and
roles. A reply to its message is highlighted too, whether it pings or not,
but only while it's new: after the last message read when the channel or
thread opened, by snowflake. A ping reply puts the account in `mentions`,
so it's a mention only when its `content` names it, `<@id>` or `<@!id>`.
Each server in the rail shows its mentions as a count, and the DM button
counts DM mentions and the friend requests waiting for an answer.
Reading an unread channel marks it read, as Discord's client does: once its
newest message is in view, with the window in front and the "New" divider
placed, so opening a channel at its divider and leaving it doesn't. A
channel whose new messages don't show, deleted, of a kind Yapfast leaves out
or hidden as blocked, has no divider once its newest page has come, and reads
then. With the "Mark messages read on Discord" setting, on by default,
Yapfast acknowledges it on Discord within a second.
Reads within that second go together: one channel as its own
acknowledgement, several, as when a server is marked read from its menu, in
one `POST /read-states/ack-bulk` of up to 100 channels, as the official
client does.
Before it does, Yapfast notes the channel's last read message, from READY's read
state or the last acknowledgement since, and the "New" divider goes above the
first message after it from someone else. Snowflakes order messages, so the
divider finds its place even when the last read message was deleted. When
every loaded message is unread, the 50 around the last read one load in
place of the newest, as a window of older messages that opens at the
divider; when even those are all newer, the divider goes above the oldest.
The open text channel lists its members on the right as Discord's sidebar
does: under hoisted roles, then online, then offline, with offline members
faded. Each shows a status dot for online, idle or do not disturb, their name
in their highest coloured role's colour, and their custom status or what
they're playing below it. Yapfast shows the list's first 100 rows, headings
included. Discord keeps one list for all channels with the same readers, and
its id says who they are: `everyone` when @everyone sees the channel and no
overwrite hides it, else a MurmurHash3 of the overwrites that allow or deny
seeing it, as Discord's client computes it. Yapfast keys its lists by that id, so
moving between such channels shows the same list and asks Discord for
nothing, and an update applies only to the list it names. Yapfast keeps the last
40 lists. An INVALIDATE, by which Discord stops sending rows, leaves them as
they were, to show when their channel is again. One Discord still sends, or
sent within 10 minutes, shows without asking again. An older one shows at
once while Yapfast subscribes, and Discord's sync replaces it. A list
on screen that Discord doesn't send is asked for again once it's 10 minutes
old. When no list comes within 8 seconds of opening a channel, the panel
says Discord didn't send one instead of loading on, and the header says
"Members" until a count is known. A build without Matrix shows the Discord
account's name, avatar and connection in the bottom-left panel.

Yapfast starts from Discord as it last saw it, kept in the data directory by
`yapfast_discord::cache`, and Discord's answers replace it as they arrive:

- `discord-account.json` holds the account's name and avatar, its servers
  with their channels, roles and emoji, its DMs, friends, friend requests and
  status. It also holds the names Yapfast learned (users, server nicknames,
  server avatars and members' roles), which messages and member lists show
  people by, and the member lists shown lately. Every message Discord sends
  from any server teaches Yapfast its author, so it keeps the 20,000 people it
  learned most lately and forgets a quarter of them, those learned longest
  ago, once there are more. It's written at most every 20
  seconds while connected. Calls and who is in voice aren't saved. On the
  next start Yapfast shows it all at once, identifies with the saved status, and
  READY replaces it. A saved member list shows at once, even before Yapfast
  connects, and Discord's sync replaces it. The list on screen is kept with
  them when the connection drops, so it shows again at once after
  reconnecting.
- `discord-messages/` holds, for each channel shown lately, its newest 100
  messages, written within 20 seconds of a change, when the channel leaves
  the 40 kept in memory, and when Yapfast closes. The 300 channels used last are
  kept. Opening a channel shows its saved messages, and the latest page fits
  in as it does for a channel kept in memory.
- `discord-pictures/` holds avatars, server icons, emoji and stickers, and the
  media proxy's previews of attachments and links, up to 8 MiB each. Discord
  gives each version of a picture its own address, so a kept one is never
  stale; the proxy's signature (`ex`, `is` and `hm`) isn't part of the key,
  so a picture signed again is found. Past 256 MiB the least recently shown
  go. Attachments' own files aren't kept.

The files are readable only by the user and aren't encrypted. Each names its
login by a SHA-256 hash of the token, so another linked account starts empty,
and unlinking deletes them all. While Discord is unreachable, an open channel
shows what was saved, waits, and loads once Yapfast is back, rather than showing
an error.

- Discord servers appear in the server rail with a Discord badge, and Discord
  DMs appear in the DM list with the same badge. A build without Matrix leaves
  the badge off, since everything in it is Discord's.
- A build without Matrix leaves out the composer's line about encryption and
  keys, since everything in it is Discord's; what stops sending still shows
  there. The account's dot in the bottom-left shows its Discord status: idle,
  do not disturb, or the grey ring when invisible or disconnected.
- Discord servers keep the order the account's server list has in Discord,
  from `guild_folders` in its settings, and follow it when it changes
  elsewhere. A server the settings don't list yet, such as one just joined,
  goes first.
- Folders show in the rail as in Discord. Closed, a folder is a tile of its
  colour holding its first four servers' icons, with the pill, unread dot
  and mention count of all its servers; its tooltip is its name, or its
  servers' names when it has none. Clicking opens it: a folder icon in its
  colour, its servers below it on a strip of that colour. Yapfast remembers the
  open folders on this device, in `layout.json`, as Discord's client keeps
  them per device. Settings entries with an id are folders, read with their
  name and colour (`GuildFolder` fields 2 to 4, each in a wrapper); one
  without is a server on its own. Folders can't be made or changed in Yapfast
  yet.
- A server out of reach in a Discord outage (`GUILD_DELETE` with
  `unavailable`, or READY listing it so) stays in the rail, faded, as last
  known, until its `GUILD_CREATE` brings it back. Only a `GUILD_DELETE`
  without `unavailable`, as when the account leaves, removes it.
- Server icons and avatars come from Discord's CDN, which needs no login.
  Yapfast asks for each one once it shows: in the rail, beside people in a
  voice channel of the open server, in the call, or beside a message. A
  member's server avatar wins over their own. Yapfast decodes them at 128px
  before the UI sees them. Once more than 400 have arrived, those off
  screen go, and Yapfast asks for them again, from disk, when they show.
  Custom emoji stay, since the picker asks for hundreds at once.
  Initials show until a picture arrives or when it fails. A download that
  may work later, when the CDN is out of reach, fails or answers 429, is
  tried up to six times; every download then waits together, 10 seconds
  and doubling up to five minutes, so a CDN in trouble hears from Yapfast
  rarely. One Discord refuses, such as a 404, isn't tried again. A server
  without an icon keeps its initials, and a person without
  an avatar gets Discord's default avatar for their account.
- Who reacted comes as Discord's client asks for it: `GET
  /channels/{c}/messages/{m}/reactions/{emoji}?limit=3&type=0` for a
  reaction's tooltip, and `limit=100` with `after`, the last user id, for each
  page of the Reactions window. Users come by id; those not known in the
  server are asked for over the gateway first, so the names are server
  nicknames, as Discord's client shows them. A moderator removes someone's
  reaction with `DELETE …/reactions/{emoji}/0/{user}?location=Message&burst=false`.
  Super reactions aren't listed apart yet.
- Messages support text, replies, edits, deletions, reactions, mentions and
  attachments. Up to ten files go with a message, uploaded to Discord's
  storage first and then named in the message. Each file is limited as
  Discord limits it: 20 MB without Nitro, 50 MB with
  Nitro Basic or the old Nitro Classic, and 500 MB with Nitro, by the
  account's `premium_type`. A server boosted to level 2 allows 50 MB and one
  at level 3 100 MB to everyone, whichever is higher. Discord sends the
  server's `premium_tier` in READY and GUILD_UPDATE, and the account's
  `premium_type` in READY and USER_UPDATE. The web client's own constant
  without Nitro is still 10 MiB, but `getMaxFileSize` raises every limit to
  at least 20 MiB (`Math.max(0x1400000, …)`) before it checks a file or names
  the limit, so 20 MB is what it enforces.
- Link previews and bots' cards show from the embeds Discord adds to a
  message: Discord's servers read the linked page's Open Graph tags, often a
  moment after sending, and the preview arrives in a message update. A card
  shows the site, author, title, description, fields and footer behind a bar
  of its colour, with its picture large for articles and videos and small
  beside the text otherwise. A video the media proxy has (`video.proxy_url`)
  plays in place, as an attached one does, under the card's thumbnail as its
  poster, whatever the card's type: vxtwitter's posts come as `rich` cards
  with one. Others, such as YouTube's, open in the browser. Other pictures open in the image viewer. A linked image or GIF
  shows with the message's images, without its address when that's all the
  message says. Pictures come through Discord's
  media proxy (`images-ext-*.discordapp.net`), so Yapfast never contacts the
  linked site, and embeds of a message marked to hide them don't show.
- Discord's own links to a server, channel, message or person open that
  place in Yapfast, DM links included; an invite opens Join a Server first,
  and a gift link (`discord.gift/…`, `/gifts/…`) shows the gift and redeems
  it on the click, never from opening the link (`link.rs`, `gift.rs`). Only
  Discord's own hosts and snowflake ids match, so a look-alike stays an
  ordinary web link. Discord pages Yapfast has no screen for, such as events
  and app authorization, still open in the browser,
  as does a link Yapfast can't show: to a server the account isn't in, or any
  while Discord is offline (`AppState::opens_discord_route`).
- Animated GIFs, APNGs and animated WebPs play, in messages, as stickers and in
  the image viewer. The media proxy keeps GIFs as GIFs; other pictures come as
  WebP, asked for with `animated=true` as Discord's client does, so an animated
  one, such as vxtwitter's AVIF GIFs, stays animated rather than turning into
  its first frame. An animation keeps its first frame in memory, and its file
  in `discord-pictures/`, or in memory when it isn't kept there, as one over 8
  MB isn't. Its frames decode one at a time on the UI thread as they show,
  reading the file, and a second after it last showed, as when its row scrolls
  away, it lets its decoder go. Decoded whole, a few seconds of a GIF take tens
  of megabytes. Frames shorter than 20 ms play at 100 ms, as browsers do. Tenor
  and Giphy links arrive as MP4 "gifv" embeds, which play the same way, silent
  and looping, from the proxy's copy of the MP4 (`video.proxy_url`): up to a
  minute of it, decoded by the FFmpeg, libvpx and dav1d inside libwebrtc
  (`native/video.c`). One that can't be decoded shows its still picture. It's
  named and saved as its video, not its picture.
- Attached videos show their poster, which the media proxy makes from the
  first frame (`proxy_url?format=webp`), with a play button; they play in
  place, with sound, under a bar to pause, move through them, mute and show
  them full screen. Full screen, as the image viewer shows a picture, the
  video is enlarged to fit the window with its bar under it; Space plays or
  pauses it, the arrow keys move five seconds, and Escape brings it back to
  its cell. The worker draws its pictures at the size they show: playing,
  the next ones come at a new size; paused, the one showing is redrawn,
  which moving back to it would hold up while playing. MP4, MOV and WebM
  play, with H264, VP8, VP9 or AV1 pictures in 4:2:0, 4:2:2 or 4:4:4 at
  8 or 10 bits, or 4:2:0 at 12, and AAC, Opus, MP3, Vorbis or FLAC sound;
  HEVC, which iPhones record, doesn't, as libwebrtc's FFmpeg lacks it.
  The file downloads into `discord-videos/` beside the account file, and
  plays as it arrives: the worker's reads wait for the download. It goes
  when the video stops, as when the channel closes. Sound plays on the
  call's speakers through `audio_io.rs`, on Linux and Windows. Over the
  mute button a slider sets how loud, as does the wheel; its travel is
  squared, so its middle sounds about half as loud. Turning it to nothing
  mutes, and unmuting brings back the level it had. The next video plays as
  loud, until Yapfast closes.
- Markdown shows as Discord's client shows it. Bold, italics, strikethrough,
  inline code, lists and links are Slint's Markdown. Headings (`#` to `###`),
  subtext (`-# `), quotes (`> ` lines, or `>>> ` and the rest) and code fenced
  by three backticks are blocks that `blocks.rs` in `yapfast-core` splits out
  by Discord's rules, read from its web client, before anything else changes
  the text. Each is then drawn apart, spaced as Discord's client spaces them,
  with its mentions, emoji and spoilers. Code shows as typed, in the system's
  monospace font, without highlighting. Discord shows headings and lists only
  in messages from 22 March 2023; Yapfast shows them in older ones too. Rules
  (`---`) and lines under a line (`===`), which Discord doesn't have, show as
  typed. Embeds' descriptions and field values, pinned messages and search
  results show blocks too, a found message's up to 8 lines each. A reply's
  quote of the message above shows it on one line.
- Spoilers, `||like this||`, show under an opaque cover that a click lifts.
  The cover is drawn by Yapfast's patched Slint text renderer over a link to
  `yapfast:spoiler/{item}/{n}`; see [building](../building.md). A spoiler in a reply
  or a notification reads "(spoiler)". Attachments named `SPOILER_...` or
  flagged as spoilers, and pictures linked inside a spoiler, show blurred with
  a "Spoiler" label until clicked. Revealing lasts while Yapfast runs. The eye
  on a file waiting in the composer marks it as a spoiler, which blurs its
  thumbnail; Yapfast uploads it as `SPOILER_` and its name, as Discord's client
  does. Matrix has no spoiler for a file, so its rooms don't offer the eye.
- Custom emoji show as pictures: in messages, as reactions and in the emoji
  picker, which lists them by server after the frequently used ones, the open
  server's first. In messages Yapfast draws them as text: each emoji whose
  picture arrived gets a code point in the Supplementary Private Use Area B,
  and each batch of them a colour bitmap (CBDT) font made on the fly, which
  text falls back to after Satoshi, so they wrap and select like letters.
  The picker, suggestions and reaction buttons show the picture itself, so
  the hundreds a picker lists add no fonts; a quiet square holds each place
  until its picture arrives. A message of only
  emoji, up to 30, shows them large, as Discord does. Without Nitro, the
  picker offers only the open server's emoji, none animated, and sending
  turns `:name:` into Discord's `<:name:id>` only for those. Animated emoji
  show their first frame.
- Stickers show as pictures at 160px, PNG and APNG stickers from the CDN and
  GIF ones from the media proxy. Lottie stickers, which are vector
  animations, show their name.
- Messages from people this account blocked show collapsed as "Blocked
  message", which "Show it" opens, and they don't notify or show typing.
  Yapfast learns the blocked from READY's relationships (type 2) and keeps them
  current from RELATIONSHIP_ADD and RELATIONSHIP_REMOVE.
- A reply's quoted line jumps to the original and highlights it for a
  moment, as described under [jumping to a message](#jumping-to-a-message).
- Desktop notifications follow the person's Discord settings: each channel's,
  category's or server's level (every message, mentions only or nothing),
  falling back to the server's default, with mutes leaving only mentions. DMs
  notify unless muted; message requests and spam don't. Apps' answers to
  slash commands and context menu commands notify as people's messages do;
  joins, pins and call notices don't. Nothing notifies while the account's
  status is Do Not Disturb. They show while Yapfast
  isn't in front, titled with who wrote where, and clicking one opens the
  conversation. On Linux they go to the freedesktop notification service.
  Each also plays Yapfast's message sound, or its mention sound, unless it's in
  the conversation in front; see [voice](../voice.md#sounds).
- Mentions in messages are links on a light fill, in the role's colour or the
  person's name colour, as Discord shows them. A person's opens their profile
  card beside the mention, and a role's lists the members Yapfast has seen with it (from member lists, messages
  and member requests), each opening their card in turn.
- Typing `@` suggests who to mention: the channel's listed members, those who
  wrote lately, and server members a search by name finds (the gateway's
  member request with a query, as Discord's client does), then the roles
  anyone may mention, and every role with @everyone and @here for those
  allowed to mention everyone. Sending turns each `@Name` into Discord's
  `<@id>` or `<@&id>`.
- Who else is typing shows above the composer, for ten seconds after Discord
  last said so or until their message arrives. Writing tells Discord Alice is
  typing, at most every eight seconds, unless a setting turns it off.
- Clicking the account's avatar or name, bottom left, opens a menu of Online,
  Idle, Do Not Disturb and Invisible, as in Discord. Yapfast saves the choice in
  the settings below and sends it with opcode 3. Do Not Disturb holds back
  Yapfast's message notifications. Closing Yapfast takes the person offline, like
  closing the Discord app.
- The same menu ends with Edit Profile, which opens the [main profile's
  editor](people.md#editing-ones-main-profile), as Edit profile in My account does.
- The custom status shows under the account's name. "Change custom status" in
  that menu, or "Set a custom status" in My account, sets it: an emoji from the
  picker, words, and when it clears (never, 30 minutes, an hour, four hours,
  or at midnight). Discord keeps it in the account's settings, a protobuf
  (`PreloadedUserSettings`, field 11), which arrive in READY as
  `user_settings_proto` and change in USER_SETTINGS_PROTO_UPDATE. Yapfast reads
  only the status and custom status from it, and changes either with `PATCH
  /users/@me/settings-proto/1`, whose body sets nothing else. Each session
  shows the status to others itself, so Yapfast sends it with opcode 3 once
  connected, whenever the settings change, and clears it when it ends. A
  custom emoji in a status needs Nitro.

## GIFs

The GIF button left of the emoji button, or Ctrl+G, opens a picker with two
tabs, GIFs and Emoji, as Discord's does; Ctrl+E opens it on Emoji. Discord
finds GIFs through Klipy, and Yapfast asks Discord as its desktop client does:

- The home is `GET /gifs/trending?media_format=tinywebp&locale=en-US`:
  trending terms, each with a picture, and one trending GIF. It shows a
  Favorites tile, a "Trending GIFs" tile and the terms, 110px tall, two to a
  row. A term searches for itself; Trending GIFs lists `GET
  /gifs/trending-gifs`.
- Typing searches with `GET /gifs/search?q=` a quarter second after the last
  key, as Discord's does. Results come in Discord's masonry: columns at
  least 200px wide with 12px gutters, each GIF going to the shortest. Within
  1180px of the end, `GET /gifs/suggest?q=&limit=5` offers other terms, as
  buttons under the results ("Your perfect GIF is in another castle").
- A click sends the GIF's page URL as a message, keeping a reply that was
  being written, and a search's pick is reported with `POST /gifs/select`
  `{id, q}`. Discord unfurls the URL into a gifv embed, whose picture Yapfast
  plays. Shift and a click puts the URL in the message instead.
- Only the GIFs in sight, and a screen either side, load and play; they come
  from Klipy's or Tenor's media hosts, at the size they show, and go when the
  picker closes.
- Favourites are kept as Discord's are, in the second settings protobuf
  (`FrecencyUserSettings`, field 2): a map from each GIF's page to its
  picture, size, format and order. `GET /users/@me/settings-proto/2` reads
  them, the first time the picker opens or a star is pointed at, and `PATCH`
  writes all of them back; USER_SETTINGS_PROTO_UPDATE with type 2 brings
  changes made elsewhere. A star at a tile's top right, or at the top left of
  a GIF in a message, adds or takes one out; a new one gets the highest
  order, and the Favorites view lists the newest first, searchable by name.
  A message's picture has the star when it moves: a gifv embed, or an
  attachment or image embed whose picture Discord flags `IS_ANIMATED` or
  serves as `image/gif`, or one named `.gif`. As in Discord's client, the
  star keys it by the gifv embed's page, an image embed's own link, as when
  a favourite saved under a direct link to its file is sent, or the
  attachment's address. An address on Discord's CDN or media proxy under
  `/attachments/` or `/ephemeral-attachments/` loses its `ex`, `is` and
  `hm`, the signature that expires; any other stays as it is.
- Favourites show at once, without waiting on Discord or the GIF providers.
  Yapfast keeps the list on disk, shows it when the picker opens and replaces
  it when Discord answers; a star used meanwhile shows at once and is
  applied to Discord's list once it comes, so a change made on another
  device stays. Each favourite shown gets a thumbnail in `discord-gifs/`,
  50 MB at most, the least lately shown going first:
  - Tenor's own small MP4 when the GIF is Tenor's: its links name the
    format with a code, `AAAP1` for that MP4.
  - The picture itself, when it's a video or no larger than 64 KB, which
    keeps a small GIF's transparency.
  - Otherwise a VP9 WebM of it at the tiles' width, made once, off the UI
    thread, with the VP9 encoder already in Yapfast's libwebrtc. It's about
    a fifth of an animated WebP at the same quality, and a fifth of its
    decoding work. A transparent GIF goes over the panel's tone, as VP9
    keeps no transparency.

  The picture downloads to disk as it comes, so a full-size GIF of tens of
  megabytes takes no memory, and the thumbnail plays from its file.

## Jumping to a message

A reply's quote, a pin, a search result and the bar over the "New" divider
jump to a message. A loaded one scrolls into view. Otherwise Yapfast asks for
`GET /channels/{id}/messages?around={id}&limit=50`, as Discord's client does,
and the messages around it show in place of the timeline: a window of older
messages, with "You're viewing older messages" and "Jump to present" in a bar
over its end. Discord answers around a snowflake even when no message has it,
so a deleted message's place shows, and Yapfast says it couldn't find the
message. A jump into another channel opens that channel at the message.

Scrolling down the window loads the 50 `after` its newest message, and up,
the 50 `before` its oldest, a page at a time. Discord doesn't say how it
splits a page around a snowflake, so Yapfast learns where the channel starts
from a page before, and that the window reached the newest message from a
short page after, or once it reaches the newest messages Yapfast kept.

While the window shows, the channel's newest messages wait beside it, at
most 200 as a closed channel keeps, and follow new messages, edits,
deletions and reactions; edits, deletions and reactions show in the window
too, but new messages don't go in it. The channel stays unread, and isn't
marked read, until the newest show. Jump to present, sending a message, or a
page that reaches them shows them again without asking Discord, and the
window goes. Leaving the channel lets the window go too, and only the
newest go to disk. Pages loading for a window that a newer jump or the
present replaced are dropped when they come.

The window holds what was asked for, not every page between it and the
present. Growing upwards past 200 messages, it lets go of its newest,
below the screen, which load again; growing downwards it keeps its oldest,
which are above the rows on screen and would move them if they went, until
it joins the newest messages.

## Forwarding

Forward, in a message's menu or with F on it, opens "Forward To", as
Discord's client has it: a search of the conversations the message can go
to, a checkbox on each, the message as a quote, and "Add an optional
message...". Up to five can be chosen, after which the rest grey out and
the heading says "Maximum 5 places at once." Send, or Enter in the
message field, closes the dialog and forwards to each in parallel; with
one chosen, that conversation opens.

Each forward is `POST /channels/{target}/messages` with no content and a
`message_reference` of type 1 (FORWARD) naming the original's message,
channel and server, with `X-Context-Properties` of `{"location":
"forwarding"}`, as Discord's client sends it. Discord copies the original
into the new message's `message_snapshots`, which Yapfast shows under
"Forwarded". Discord takes no text with a forward (error 160011), so the
message added goes after it as one of its own, with the same location, to
each conversation the forward reached. A slowmode channel this account
doesn't bypass gets the forward alone, as the dialog warns, since the
second message would wait out the slowmode. `@silent` at the start of the
message added notifies nobody of either.

Messages forward as Discord's client allows: those written, by a person or
an app's command, with no poll, call or activity, and none of the flags it
doesn't forward, such as an ephemeral answer; forwards and voice messages
forward again. In a server, the account needs Read Message History in the
channel, and the server mustn't list `FORWARDING_DISABLED` among its
features. Targets are the DMs, group DMs, channels and threads this account
may send in, but not forums, whose posts start with a title, nor Matrix
rooms. Once each conversation answers, Yapfast says "Message forwarded.",
or opens the dialog again with those it failed in chosen, the message
added kept, and Discord's reason, so Send retries them. If the forward went
but the message added didn't, a notice says where.

The dialog lists places in the order Discord's client does. Before a
search, it lists those chosen, the latest first, as they were when the
search last changed; then the last eight conversations opened this
session; then those this account uses most. It shows fifteen at most, and
not the conversation the message is in, unless it's chosen. "Uses most" is
Discord's frecency: `guild_and_channel_frecency`, field 12 of the
`FrecencyUserSettings` that GET `/users/@me/settings-proto/2` returns,
fetched when a Discord message is first forwarded or the quick switcher
first opens ([chat UI](../chat-ui.md#implemented-behavior)), and kept up
to date by USER_SETTINGS_PROTO_UPDATE. It maps each channel's id to how
often it was used and the times of the last ten uses. A use counts 100 on
its day, 70 a day later, 50 at two or three days, 30 up to a week, then 10; a
place's frecency is its uses in all times that sum, over the number of
recent uses. Each conversation opened in Yapfast counts as a use, added
again to each count that comes, as Discord's client does until it saves
its own. Yapfast doesn't save them.

A search lists every place whose name matches, ranked as Discord's search
ranks them. A channel or group DM scores 10 for its whole name, 7 for its
start, 5 for a part, 3 when each word is in its name or server's name,
and 1 for its letters in order; a dash in a channel's name matches a
space. A person's DM scores 10 for the start of their name, 1 for its
letters in order. Each score is raised by up to double for a place used
most, and a person's by 0.2 more for a friend and 0.1 for the DM. `@`
searches people only, `#` channels only. Choosing a place clears the
search. Discord's search also finds people with no DM yet and opens one
to forward to; Yapfast lists only DMs that exist.

Discord's client also greys out a channel where the original's files,
embeds or stickers couldn't be posted, and an age-restricted one. Yapfast
lists those, and Discord's refusal shows when the forward fails there.
Mentions and custom emoji in the message added go as typed.

## Notification settings

Discord keeps the account's notification settings per server, and one set
for DMs: READY lists them in `user_guild_settings`, and
USER_GUILD_SETTINGS_UPDATE replaces a server's
(`yapfast_discord::notifications`). Each holds the server's mute, with
`mute_config`'s `end_time` when it ends, its level (every message, mentions
only, nothing, or the server's own default, `default_message_notifications`),
whether @everyone and @here, and role mentions, call on the account, whether
muted channels hide, and `channel_overrides`: each channel's and
category's own mute and level, where 3 means the category's or server's.
Yapfast publishes each server's settings with the server, shared between
snapshots until Discord changes them, and marks each channel muted when it
or its category is. A muted channel shows faint, as a muted DM does, and a
server set to hide muted channels leaves them out of the channel list, but
for the open one and those that mention the account, whose count the rail
shows. A mute with an end lifts by itself then: the session wakes at the
soonest end and publishes again.

`PATCH /users/@me/guilds/{guild}/settings` changes them, with `@me` for DMs.
The body names only what changes, and Discord keeps the rest; a channel's or
category's goes in `channel_overrides`, keyed by its id rather than listed:
`{"channel_overrides": {"<id>": {"muted": true, "mute_config":
{"selected_time_window": 900, "end_time": "..."}}}}`. A mute for good is
`selected_time_window` -1 and no end, as Discord's client sends it, and a
level is `message_notifications`. `Action::ChangeNotifications` asks for a
change to a server, by its server id, or to a channel or category, by its
room id: a mute for one of the DM menu's times, a level, or one of the
server's switches. A server's menu and its header's dropdown offer the
server's own.

A server text channel and a category offer their own mute and level, as
Discord's do, built alike in `src/ui/menus.rs` (`Notify`):

- The bell in the channel's header opens a menu of "Mute channel", whose
  submenu mutes for 15 minutes, 1, 3, 8 or 24 hours, or until unmuted, or
  "Unmute channel" with when the mute ends, then the levels: "Use category
  default", or "Use server default" outside a category, under which the
  level it follows shows, then All messages, Only @mentions and Nothing,
  the chosen one checked. The bell is slashed while the channel is muted,
  itself or through its category.
- Right-clicking the channel offers the same: "Mute channel" with its times,
  or "Unmute channel", and "Notification settings", whose second line is the
  level that applies and whose submenu lists the levels.
- Right-clicking a category's heading, or the Menu key on it, offers "Mute
  category" or "Unmute category" and "Notification settings", for the
  category's own override; a muted category's channels show muted. The
  heading names its first channel, and Yapfast finds the channel's category
  (`Application::category_of`); an empty category's heading names the
  category itself.
