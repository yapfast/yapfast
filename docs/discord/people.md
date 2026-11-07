# DMs, friends and profiles

## DMs

DMs and group DMs are listed under the DM button after Matrix DMs, the latest
conversation first as in Discord, with the other person's avatar or the
group's icon and a Discord badge. They open, send and keep their messages
like server channels, under the room id `discord:@me:<channel>`, which takes
`@me` for the server as Discord's links do. Every unread message in a DM
counts as a mention, and a muted DM shows neither, as in Discord. A DM
without a read state counts as read, since Discord keeps one for each DM
with something to read. A one-to-one DM shows its person's status dot, with
their custom status or what they're doing under the name. As in Discord's
DM list, a presence comes from friends' presences, and for anyone in a DM or
group DM also from the servers the account shares with them:
READY_SUPPLEMENTAL's `merged_presences`, a server's PRESENCE_UPDATE and
member list rows. Yapfast keeps servers' presences only for people in a DM,
not for every member (`Tracker::in_private`). Someone with none shows the
grey offline ring. These follow PRESENCE_UPDATE live. The header of an open
one-to-one DM shows the same dot on their avatar, and their custom status or
what they're doing in a bubble beside the name. A group DM
shows how many are in it. A group DM lists its members by name, with their
presence. Yapfast downloads the pictures of the first
150 DMs listed; the rest keep their initials.

Friend requests, message requests and spam have their own screen. A
"Requests" entry below Friends at the top of the DM list opens it in place of the
conversation, with a badge counting the people waiting for an answer: an
incoming friend request (spam excluded), a message request, or both.
Message requests come from people outside the account's DMs. The screen
lists each person once, under "Requests": their friend request, with Accept
and Ignore, and under it the latest message of their message request, which
opens the conversation; an unread one is marked New. Discord's own client
keeps the two apart, friend requests under Friends and message requests on a
screen of their own; Yapfast puts them together, as one person's asking.
Friend requests this account sent follow under "Sent". Those Discord marks
as spam, friend requests and message requests alike, fold away under
"Spam", so a person Discord misjudged can still be found. The entry stays
highlighted while a request from the screen is open. Opening one shows its messages with Accept and Ignore in place of the
composer: replying waits for accepting, as in Discord. Accepting makes it an
ordinary DM; ignoring removes it, and the sender isn't told. Requests count
no mentions, spam isn't marked unread, and Yapfast never opens a request on its
own when it picks a conversation.

A request's message comes as Discord's client loads it:
`GET /users/@me/message-requests/supplemental-data` with up to 25
`channel_ids`, repeated, answers `[{channel_id, message_preview}]` with a
message object each. Yapfast asks once for each request it hasn't a message
for, after connecting and when a request arrives; MESSAGE_CREATE and
MESSAGE_UPDATE in a request replace it with a newer one, and deleting the
one shown asks again.

The pen in the DM list's header starts a DM: it lists the account's friends
(from READY's relationships), then anyone else Yapfast knows by name, and
opening one posts to `/users/@me/channels`, which finds the DM with them or
starts one. Yapfast opens it once its CHANNEL_CREATE lists it.

## Friends and requests

"Friends", at the top of the DM list, opens the friends list in place of the
conversation. It lists the account's friends from READY's relationships by
name, each with their status dot and custom status or activity, under two tabs
in the header: Online, for those online, idle or on do not disturb, and All. A
search field filters it by name, and "Add Friend" in the header opens the
add-friend dialog. Discord sends presence for friends in READY_SUPPLEMENTAL's
`merged_presences` and in PRESENCE_UPDATE, so the list keeps up with them.
Clicking a friend, or the message button on their row, opens the DM with them,
or starts one as the pen does and opens it once Discord lists it. A friend in a
voice channel of a server this account is in shows where, as Discord's Active
Now does, both here and in their one-to-one DM's row: a green speaker, the
channel and the server in place of "Online" or an empty line, or, beside a
custom status or activity, the speaker alone, with the channel and server in
its tooltip. Clicking it goes to the channel without joining it. It comes from
the voice states the channel list already keeps, so it shows only servers both
share and channels this account sees. DM calls stay out, as in Discord.

The person beside the pen, in the same header, adds a friend by username: `POST
/users/@me/relationships` with `{"username": ..., "discriminator": null}` and
`{"location":"Add Friend"}` as the context. Discord's codes 80004 (nobody by
that name), 80000 (they take no requests from strangers) and 80007 (already
friends) get their own words. Discord refuses requests to bots and apps, so
their menus, profile cards and DM headers offer none.

Friend requests show on the requests screen, from READY's
relationships (type 3 sent to this account, 4 sent by it) and kept current by
RELATIONSHIP_ADD and RELATIONSHIP_REMOVE. One sent to this account is
accepted with the check or ignored with the cross; one it sent is cancelled
with the cross. A profile card offers "Send friend request" to someone who
isn't a friend, or "Accept friend request" when they asked first, and the
header of a DM with them offers the same. Sending is
`PUT /users/@me/relationships/{id}` with an empty body, and accepting is the
same with `{"confirm_stranger_request": true}`: Discord calls a request a
stranger's, `stranger_request` in the relationship, when it comes from
someone with no friend and no small server in common, and answers 80013 to an
acceptance that doesn't say so. Ignoring or cancelling is `DELETE` on the
same path, each with the X-Context-Properties
header Discord's client sends: `{"location":"User Profile"}` when sending
from a profile, `{"location":"Friends"}` otherwise, as base64. Discord
doesn't let a client send many requests quickly, and Yapfast only sends one when
the person clicks. Requests Discord took for spam are listed too, marked
"Maybe spam".

Those this account blocked are listed at the end of Settings, Discord, as
Discord's client lists them under Ignore & Block: "Blocked accounts" with
the count, then each by avatar, name and, under a display name, username,
five at first and five more with each "See N more". It comes from READY's
relationships of type 2, kept current as above, and shows only while
someone is blocked. Unblock on a row is the menu's `DELETE`.

## A DM's menu

Right-clicking a DM offers, besides Profile and copying, these:

- Pin keeps a DM or group DM at the top of the list. Discord's desktop client
  doesn't pin DMs, so Yapfast keeps the pins in `layout.json`, on this device.
- Start call joins the DM's call, as the phone in its header does.
- Add note writes this account's note on the other person:
  `GET /users/@me/notes/{id}` fills the dialog, a 404 meaning there's none,
  and `PUT` with `{"note": ...}` saves it, an empty one removing it. A note
  read is kept in memory, so opening a profile again asks nothing:
  USER_NOTE_UPDATE changes it, and a new session forgets them all.
- Add friend nickname, for a friend, is `PATCH
  /users/@me/relationships/{id}` with `{"nickname": ...}`, or null to remove
  it. RELATIONSHIP_UPDATE carries it back, and Yapfast shows a friend by their
  nickname in the DM list and the friends list, as Discord does.
- Close DM is `DELETE /channels/{id}`; Discord answers with CHANNEL_DELETE,
  and the DM returns when either side writes.
- Remove friend is `DELETE /users/@me/relationships/{id}`, and Block is `PUT`
  on the same path with `{"type": 2}`, each after the person confirms, with
  `{"location":"ContextMenu"}` as the context. Unblock is the same `DELETE`.
- Ignore is `PUT /users/@me/relationships/{id}/ignore`, and `DELETE` stops it.
  Discord marks the relationship `user_ignored`, which Yapfast tracks to offer
  the opposite.
- Mute is `PATCH /users/@me/guilds/@me/settings` with the channel's override:
  `muted` and a `mute_config` of `selected_time_window`, in seconds or -1 for
  good, and its `end_time`. Unmute sets `muted` false. USER_GUILD_SETTINGS_UPDATE
  brings the change back, and a muted DM shows faint and counts nothing unread.
- Leave group, in a group DM, asks first with Discord's dialog, whose box
  leaves without notifying the other members. Either way it's `DELETE
  /channels/{id}` with `silent` true or false, which Discord's client always
  sends; Discord posts no "left the group" message with true. CHANNEL_DELETE
  takes the group out of the list.

- [Invite to server](servers.md#invite-to-server) sends them an invite to one of this
  account's servers, chosen in its submenu.

Someone who isn't a friend is offered Add friend, Accept friend request or
Cancel friend request in place of Remove friend, unless they're blocked.
Right-clicking the other person's avatar or name in the DM's messages opens
the same menu. The person's choices show only in a one-to-one DM. A message request offers
neither pinning, closing, leaving nor muting, as it waits for an answer.

## A DM's header

A DM or group DM's header has, as Discord's does, its call, its pinned
messages, adding friends, the profile or member list, and a search. Video
calls aren't there yet. A server's text channel has, after its icon and name,
its topic, then on the right Discord's buttons in Discord's order: the bell
of its [notification settings](messages.md#notification-settings), its pinned
messages, the member list's toggle, bright while the list shows, and the
search. The Threads button comes before the bell; see [a channel's list of
threads](servers.md#a-channels-list-of-threads).

- The topic is the channel's `topic`, from READY and GUILD_CREATE, which
  CHANNEL_UPDATE changes. Yapfast keeps it with the channel, shared between the
  server's snapshots (`Option<Arc<str>>`), and when a channel opens, renders
  its Discord markup as a message's: mentions by name, custom emoji,
  Markdown and links. It sends the result (`Event::Topic`) again only when
  the topic changes, or when it mentions someone and the names Yapfast knows
  change. The header shows it on one line, after a rule, cut at the last
  word that fits; clicking it shows it in full under the header, with its
  links. A channel without one shows none.

- Pinned messages open under the pin, the latest pinned first, from `GET
  /channels/{id}/messages/pins`, whose `items` carry each `message`. Yapfast asks
  each time they open, and for the first 50.
- Notices, such as that someone pinned a message, joined, started a thread
  or a call, or that a poll closed, show as Discord's client shows them: a
  row of their own with an icon in place of the avatar, the author's name
  in bold in their role colour, which opens their card, what happened, and
  when. A pin's says "pinned **a message** to this channel. See all
  **pinned messages**.", linking to the message its `message_reference`
  names and to the pinned messages.
- Search, as in Discord, looks through the DM in a DM, with `GET
  /channels/{id}/messages/search`, and through the whole server in a server
  channel, with `GET /guilds/{id}/messages/search`; the box says "Search"
  and the DM's or the server's name. Results come 25 a page, newest first,
  each of `messages` a list whose first is the one found; they take the
  member list's or the profile's place, and Show more loads the next page.
  Discord indexes a DM or a server the first time it's searched, answering
  202 with code 110000 and a `retry_after`; Yapfast asks again up to five
  times.
- The box takes Discord's filters with the words (`yapfast_discord::search`):
  `from:` and `mentions:` someone, `in:` a channel of the server, `has:`
  link, embed, poll, file, video, image, sound, sticker or forward,
  `before:`, `after:` and `during:` a day such as 2026-10-13, and
  `pinned:true`. A value with spaces goes in quotes. Yapfast sends them as
  Discord's client does, each repeated for several: `author_id`, `mentions`,
  `channel_id`, `has` (`snapshot` for forwards), `min_id` and `max_id`, the
  snowflakes of the day's local midnights, and `pinned`. `in:` names a text
  channel by its name; `from:` and `mentions:` find the member through the
  gateway's member search in a server, as the box's suggestions do, and
  among the people Yapfast knows in a DM, someone called exactly that first. A
  name nobody has, or a filter Discord doesn't take, says so in place of
  results.
- A server's results are headed by the channel they're in, and the search
  stays open in the server's channels. Clicking a result in another
  channel opens it on the messages around the result
  (`Action::JumpInRoom`). In an unread channel the "New" divider still goes
  in, but neither takes the view from the result nor loads the messages
  around the last read one in place of its window. Another server, a DM or
  a voice channel closes the search.
- In a server, pins and results carry no member profiles, so Yapfast asks the
  gateway for those of authors it doesn't know yet and waits up to five
  seconds for them; then they show with their server nickname, avatar and
  role colour.
- Clicking a pin or a result jumps to it in the timeline, as a reply's
  original does; see [jumping to a message](messages.md#jumping-to-a-message).
  Pins and results show their text and name their files, without
  downloading them.
- Adding friends to a one-to-one DM makes a new group of them and the other
  person with `POST /users/@me/channels` and their ids in `recipients`, as
  Discord's client does, and opens it; the DM stays as it was. In a group,
  each is `PUT /channels/{id}/recipients/{user}`, which CHANNEL_RECIPIENT_ADD
  confirms. A group holds ten people, this account included, so the picker
  stops there, and it offers only friends who aren't there already.

## Profiles

Clicking someone in a channel's member list or under a voice channel, or
following a Discord `/users/{id}` link,
shows their profile in a small card beside it, whose picture or name
enlarges it into a centred one. The card leaves the dates and the roles'
heading to the full profile, and ends in a box, "Message @name", as
Discord's does: Enter opens or starts the DM with them and sends what it
holds there, without touching the DM's own composer. The box shows only for
someone Yapfast can message, never for this account;
right-clicking a DM and choosing "Profile" opens the full one. A one-to-one
DM shows its other person's profile in a panel on the right, with the bio in
full and the mutual servers and friends. Mutual servers show their icon and
the person's nickname there; clicking one opens a card beside it with the
server's banner, member count (from READY's `member_count`), channel count
and "Go to server". Mutual friends show their avatar and status dot;
clicking one opens their profile card beside it, or their full profile from
a full profile; the person icon in the DM's header
hides or shows it, and Yapfast remembers the choice in `layout.json`. Yapfast fetches the
profile once each time the open DM changes to someone else. Yapfast fetches it
from `GET /users/{id}/profile` with the open server's id, so the server's
nickname, avatar, banner, bio and pronouns win over the person's own, and the
profile lists their roles in role order, when they joined Discord and the server,
and the servers and friends the account shares with them. Profiles are kept for
five minutes, at most 32 of them. Presence and activity come from the member
list, since the profile doesn't carry them. When Discord refuses, the card
keeps what the member list knew and says so. A linked profile with nobody in
a list to show first opens with the id as its name while that fetch runs.

The profile shows each of the person's activities as a card, as Discord's does:
what they're playing, streaming, listening to, watching or competing in, or
their Hang status, headed by what it is ("LISTENING TO SPOTIFY"), with its
large picture and the small one on its corner, its details and state, and their
party's size as "(2 of 4)". A game with a start shows how long it has run,
"42:10 elapsed"; one with only an end, how long is left; a song with both, a
bar with how far it's got and how long it is. A Spotify song is titled by the
song, then "by" the artist and "on" the album, and its title opens the song's
page, `https://open.spotify.com/track/{sync_id}` (or `/episode/` when the
activity's metadata says so); a stream has "Watch stream". The line under the
name goes when it only names one of these. The activities come with the
presence, from READY_SUPPLEMENTAL's `merged_presences`, PRESENCE_UPDATE and the
member list, and the DM panel follows them live. Their pictures resolve as
Discord's client does: an application's asset from `/app-assets/`, `mp:` keys
through the media proxy, and `spotify:`, `youtube:` and `twitch:` keys from
those services' image hosts, which downloads allow for this alone; keys with
anything but plain characters show no picture. A game Discord detected has no
pictures, so its card shows the game's icon, which Yapfast asks for once per
game with `GET /applications/public?application_ids={id}` and downloads from
`/app-icons/`. Application pictures stay in the picture cache. The timers tick
once a second, and only while a profile with one shows. Matrix members and
one-to-one Matrix DMs get the same card with their name, Matrix ID, avatar and
room role.

Someone's server tag shows as a small chip, as in Discord: the badge of the
server they represent, then up to four letters. It comes from the
`primary_guild` of their user object, which has the `tag`, the
`identity_guild_id`, a `badge` hash that Yapfast fetches from
`/clan-badges/{guild}/{badge}.png`, and `identity_enabled`, which is true while
they wear it. It's false once they stop, and null until they take up a tag
their server changed; as in Discord's client, those and one without a tag show
nothing. The chip sits after the name on each message that starts a group,
after the name in the member list and in a one-to-one DM's row, and beside the
handle on a profile card. A tag without a badge shows its letters alone.
Yapfast learns tags from every user object it sees, with the names and avatars,
so a tag changes on screen as soon as a message, a member list row or a profile
carries the new one; a user object without `primary_guild`, as a partial one
is, changes nothing, and `null` takes it away. They are kept with the names
across restarts.

The card shows the person's badges, as Discord's does, in a row of pictures
under the name and handle: HypeSquad, Active Developer, a Nitro subscriber's
and the like, in the order the profile lists them. They come from the
`badges` of `GET /users/{id}/profile`: each has an `id`, a `description`, an
`icon` hash that Yapfast fetches from `/badge-icons/{icon}.png`, and
sometimes a `link`. Pointing at one shows its description; clicking one that
has a link opens it. A badge missing its description or icon is left out, and
a profile with none shows no row. Badges show on profile cards only, not
beside names in messages or the member list.

The badges that level up (playing time, game variety, streaming, account age
and Quests) say little in the profile: their `description` is only the kind,
"Spielzeit". Discord's tooltip for them shows more: a large picture of the
level, the kind in grey, the level's name in capitals and how far along the
person is ("1.816 Stunden gespielt"). That comes from `GET
/users/{id}/badges`, which works for anyone. Discord answers it more slowly
than the profile, so the card doesn't wait for it: `Discord::profile` returns
at once, and `Discord::badge_levels` asks afterwards, only when a badge has a
`simple_icon_url` or a link, and gives the card its levels as
`Event::BadgeLevels`. It also puts them in the kept and saved copy of the
profile, so the next opening has them from the start, and asks once for each
kept profile, as Discord's client keeps its answer. Each of its `badges` has a `name` (the kind), an `info_label` (the
progress), a `current_tier` naming one of its `tiers`, whose `name` is the
level and `complex_icon_static_url` the large picture, an SVG. A badge
without tiers, such as Quests, shows its own name as the title and its
`simple_icon_url`. Yapfast matches an entry to a profile badge by the
level's `simple_icon_url`, which both lists carry, else by the icon hash in
`simple_icon_raster_url`; one the person doesn't own (`owned: false`) is
skipped. A badge with no entry, such as Nitro's, keeps the plain tooltip, and
so does every badge when the request fails. The SVGs are drawn by
`decode_svg`, with resvg, and only for addresses under Discord's
`/assets/content/`: an attachment or avatar never reaches it. A badge's icon is
decoded at 40px at most. The
card is `BadgeTipCard` in `ui/people.slint`, set by `BadgeTip` in
`ui/theme.slint` and drawn by the window above everything else, with
Discord's own values: 200px wide, 8px corners, the picture 64px high, the
kind and progress in 14px medium and the level in 16px black.

## Avatar decorations and nameplates

What a person equips from Discord's shop shows as it does in Discord's client.
Both come on user objects, wherever Discord sends one: READY, messages and
their mentions, member lists, relationships, DM recipients and `GET
/users/{id}/profile`. `Names` learns them with the person's avatar hash and
keeps them with it, saved with the account and forgotten with the person; a
partial user object that leaves them out changes nothing, and `null` or no
nameplate among `collectibles` takes them off.

- **Avatar decoration**, `avatar_decoration_data: {asset, sku_id, expires_at}`:
  a picture 1.2 times the avatar's size, centred on it, as Discord's
  `--decoration-to-avatar-ratio` has it. It shows around the avatar on
  messages, in the member list and group DM member lists, on the profile card,
  in the DM list for a one-to-one DM, and for this account on its card at the
  bottom of the sidebar and in My account. Not on the small avatar in a reply's
  quote or in a mention, where Discord shows none either. Where the status dot
  sits on an avatar, the dot is drawn over the decoration; Discord cuts a hole
  for it, which Slint can't, so the dot's ring takes the row's colour.
- **Nameplate**, `collectibles.nameplate: {sku_id, asset, label, palette}`: a
  strip behind a member's row, a one-to-one DM's row and this account's card.
  A DM's row shows it only while the row is selected, pointed at or focused, as
  in Discord's client, so a long DM list holds none at rest. Its gradient doesn't
  brighten under the pointer, and its picture starts fading in 5 pixels before
  the text's end. A member's row fills with a
  gradient in the palette's colour, `transparent` to 8% at 20% across, 8% to
  50%, then 20% at the right edge (30% hovered, 50% selected), with the plate's
  picture at the right end, the row's height tall, at 60% opacity (90%
  selected). Under the row's text the picture shows at 30% of that, fading to
  full 10 to 60 pixels after the text's end, where Discord masks the picture
  with `mask-image`; Slint has no masks, so Yapfast draws the row's colour over
  it in the same gradient. The account card's plate is a 10% to 40% gradient
  under the picture at 40% opacity, with no fade, filling the card and a pixel
  over its top and bottom. A row with a plate keeps 12 pixels free at its
  right, and an offline member's is faded with the rest of the row. The twelve
  palettes (`crimson`, `berry`, `sky`, `teal`, `forest`, `bubble_gum`,
  `violet`, `cobalt`, `clover`, `lemon`, `white`, `black`) have the colours
  of the client's own `darkBackground` and `lightBackground`, by the theme
  (`collectibles.rs`); a palette it
  doesn't know, such as `custom_palette`, shows the plate's picture without
  the gradient. Show nameplates in Appearance (on unless turned off, kept in
  `layout.json` as `nameplates`) hides every plate: none is fetched or drawn.

Both pictures are served by `https://cdn.discordapp.com/media/v1/collectibles-shop/{sku_id}/static`,
a PNG: 288 by 288 for a decoration, 448 by 84 or 672 by 126 for a plate. The
client asks for `/animated`, an APNG, or `/video`, a WebM, and Yapfast
asks for `/animated` (Discord's own client does on Linux and Safari). A
decoration plays on a message while the pointer is on the message that starts
its group, and on the profile card, in the member list, a DM's row and the
account card while the pointer is on them. Discord's client plays a chat
decoration while the pointer is on any message of the group, and a card's
whenever it's open. A nameplate plays while its row is pointed at, and the
account card's while the card is; a selected row's holds still, as playing
costs some 7% of a core for as long as it's selected. Everything holds still when Yapfast's window isn't focused
(`Motion.playing`), and a surface that stops being pointed at goes back to the
still picture. The animated picture is fetched the first time it's to play,
and decodes the frame showing: `Picture`'s frames come from the kept file one
at a time, as a GIF's do, and a decoder is let go a second after its last frame
shows, so nothing animating off screen keeps anything. The pictures are kept in
the picture cache with avatars. A decoration decodes at up to 160 pixels and a
plate at up to 448 by 84, where other pictures take 128, and the still ones stay
while the screen shows their person (`pictures_on_screen`).

What a member sets for one server alone, `avatar_decoration_data`,
`collectibles` and `display_name_styles` on a member object (member lists,
GUILD_MEMBER_UPDATE, messages' `member`), wins in that server, as in Discord's
client (`lw({userValue, guildValue})`); everywhere else their own show. A
member object that leaves a key out says nothing, and `null` takes the
server's off. `Names` keeps these beside the user's own, by server and user.

## Profile effects

A profile names the effect its person equips, `profile_effect: {id,
expires_at}`, in `user_profile` (or `guild_member_profile`, which wins in its
server). The effect itself is the shop's: Yapfast asks for it once the card
shows, `GET /collectibles-products/{sku}` with the account's own login, as
Discord's client does, and takes the first item with `effects`: its
`staticFrameSrc` (else `reducedMotionSrc`, else `thumbnailPreviewSrc`) and
`effects`, each layer with `src`, `randomizedSources` (one of which plays in
each layer in place of `src`, the same one in all), `loop`, `start`,
`duration` and `loopDelay` in milliseconds, `position` and `zIndex` (the
fields are camelCase, as the client reads them).

Discord's client plays an effect over the card for as long as its window has
the focus. Yapfast shows the still picture, and plays the layers only while
the pointer is on the card, starting them over each time, as a card's
decoration does: playing costs a fifth of a core on the software renderer. An
effect with no layer that loops stops at the end of its last layer and shows
the still picture again.

The layers are APNGs on the CDN, 450 by 880 pixels, scaled to the card; each
picture decodes at no more than `EFFECT_SIZE`, and a layer's is asked for when
it first plays. An effect's pictures are the size of the card, so they go as
soon as its card closes (`let_go_of_effect_pictures`) rather than waiting for
the trim. The backend keeps the last eight effects (`Event::ProfileEffect`);
each card's profile points to one by SKU.

## Display name styles

A name can carry a style, `display_name_styles: {font_id, effect_id, colors}`,
on a user object or a member object (which wins in its server, as the
decorations do): a font, an effect and up to five colours. Yapfast draws it
where Discord's client does: a message's author, a member list's rows, a
one-to-one DM's row and the profile card's name (`StyledName` in
`ui/controls.slint`). `src/ui/name_style.rs` makes the colours, as the client's
`lZ` does: lighter against a dark theme's background and darker against a light
one's, until they read, and against #333 for toon on either. Each style's two
palettes are made once, and the rows wearing it share them; a row without a
style holds an empty list.

- **Fonts**, by `font_id`: 11 is the usual font, then Sakura 3, Jellybean 4,
  Modern 6, 8Bit 8, Tempo 12, Monkey Bars 13, Mainframe 14, Headbang 15 and
  Journal 16, bundled in `ui/fonts` (see [its
  README](../../ui/fonts/README.md) for where each came from and its
  licence). Medieval 7 and Vampyre 10 aren't bundled, since their licence
  isn't confirmed, so those names show in the usual font; so does a name
  in a script the font lacks.
- **Effects**, by `effect_id`: solid 1, gradient 2 (the colours across the
  name from 10% to 90%), neon 3 (white letters edged in a lighter shade of
  the colour, with a halo), toon 4 (light bands edged in the dark shade),
  pop 5 (white letters edged in the darkest shade over a copy in the colour
  a little lower), glow 6 (a gradient, as Discord's client falls back to),
  prism 7 (the colours once round, back to the first) and gummy 8 (the
  letters in the colours in turn, spaces not counted; a name too long for
  its place is elided in the first colour). An effect this client doesn't know
  is drawn solid.
- **Colours** are made to read against the dark ground as the client does:
  lightened five percent of HSL lightness at a time, at most ten times,
  until they have a contrast of 3 with `#121214` (2.5 for gradient, glow and
  prism); the shades neon, toon and pop use are derived from the first with
  the client's HSL steps (`$0`).

What still differs: names are drawn at rest, Discord's `STATIC` display type
and its default; the `ANIMATED` one (neon flicker, toon and pop bounce, prism
scroll and gummy squish, in the client's `_dfb989` keyframes) isn't drawn. Neon's halo is a stroke of the colour at 28% opacity,
where Discord blurs a `text-shadow`; Slint has no text shadow or blur.
Toon's light bands are a guess at the gradient the client draws, whose
stops its stylesheet doesn't show.

## Editing one's main profile

Edit Profile, in the account's menu bottom left, or Edit profile in My
account, opens Yapfast's version of the "Main Profile" tab of Discord's
Profiles settings (`application/profile_editor.rs`, the dialog in
`ui/server.slint`, which it shares with the [server
profile's](#editing-ones-server-profile)). It reads the profile with `GET
/users/@me/profile` and edits the fields Discord's tab has:

- Display Name, up to 32 characters. Left empty, the username shows.
- Pronouns, up to 40.
- Avatar, with Change Avatar and, when there's one, Remove Avatar, which
  goes back to Discord's default avatar.
- Without Nitro, Banner Color: the colour the card's banner shows, as
  `#rrggbb` or one of nine swatches, "Default" for none. With Nitro,
  Profile Banner, with Change Banner and Remove Banner, in its place.
- Bio, Markdown up to 190 characters, with what's left counted under it.

Rather than Discord's form beside a preview, the editor is the large
profile card itself, as it will show. A pencil beside the name, the
pronouns after the username, and About Me turns each into a field in
place, as does clicking the text; Enter, Escape or a click elsewhere on
the card ends it. Hovering the avatar or banner darkens it under a
pencil, and clicking it opens the file chooser, or a menu of Change and
Remove when there's one to remove; without Nitro the banner opens the
colour under it. A pill on the banner's corner says which profile it is.
Once something changed, Discord's bar shows at the card's foot:
"Careful — you have unsaved changes!", with Reset, which puts back what
Discord has, and Save Changes. The card's top stays put as the bar or
the colour opens.

Discord's client checks the same Nitro as for server profiles: a banner
takes `canUsePremiumProfileCustomization`, so `premium_type` 2, and an
animated avatar `canUseAnimatedAvatar`, so 1 or 2. A GIF avatar without
it says "An animated avatar takes Nitro." where the client offers Nitro;
the rest, the bio included, is free. The client's colour picker suggests
colours from the avatar and has an eyedropper; Yapfast takes the hex.

Save Changes sends what changed, in up to two requests, as the client
does:

1. `PATCH /users/@me` with `global_name` and `avatar`. A cleared display
   name goes as `""`.
2. `PATCH /users/@me/profile` with `bio`, `pronouns`, `banner` and
   `accent_color`, an integer, or `null` for the default.

Discord answers each with USER_UPDATE, which brings the new name and
avatar to the account panel, messages and member lists. A picture goes
as for server profiles: the same crop and size, as a data URI, or `null`
to remove it. The client never sends the bio, pronouns or banner to
`/users/@me`, though Discord's docs list them there.

The client's tab also has Profile Widgets, an avatar decoration, a
nameplate, a profile effect, a frame, display name styles and, with
Nitro, a profile theme of two colours in place of the banner colour, then
the Server Tag and badges. Yapfast leaves them out: it
[shows](#avatar-decorations-and-nameplates) what someone equips, and others' server tags
and badges, but doesn't change it. Changing the username, in My Account with the password, is left
for later.

## Editing one's server profile

Edit server profile, in a server's menu, its header's dropdown and one's
own entry in its member list, opens Yapfast's version of Discord's "Per-server
Profiles" settings page (`application/profile_editor.rs`, the same card
as the [main profile's](#editing-ones-main-profile)). Discord's client
offers it to everyone; it reads the profile with `GET
/users/@me/profile?guild_id=...` and edits:

- Server Nickname, up to 32 characters, with no pencil without Change
  Nickname or Manage Nicknames.
- Pronouns, up to 40.
- Avatar, with Change Avatar and Reset Avatar.
- Profile Banner, with Change Banner and Reset Banner.
- Bio, Markdown up to 190 characters, with what's left counted under it.

The card shows the profile as the server will, the main profile's name,
pronouns, bio, avatar and banner standing in for each field left empty,
as Discord merges them: an empty text and a missing picture both go by
the main profile. Reset shows in a picture's menu only when the server
has a picture of its own, as it stands or newly chosen.

The nickname and pronouns are free. The avatar, banner and bio take
Discord's full Nitro: the client checks `canUsePremiumProfileCustomization`,
a feature of the Nitro SKU alone, so `premium_type` 2 and not Basic (3) or
Classic (1). Without it Discord still shows them, greyed, under an offer:
"Make unique profiles for each server", "Customize your profile for every
server, from avatar decorations to bios. Only with Nitro.", and "Unlock
with Nitro", or "Upgrade Nitro" with a lesser Nitro, which opens
`https://discord.com/nitro`. Yapfast shows the offer under the name, and
a lock in place of the pencil on each of them.

Change Avatar and Change Banner use the file chooser attachments use. Yapfast
reads the picture off the UI thread, as Discord's cropper sends it: the
middle square of an avatar, at most 1024 pixels, and the middle 17 by 6 of
a banner, at most 2400 by 848, from a PNG, JPG, GIF or WebP under 10 MB.
It keeps only that file, a PNG where the picture has transparency and
otherwise a JPEG, and a preview at the size the card gets everyone's
pictures in, 128 pixels square and 600 by 212; an
animated GIF goes as it is, so it stays animated. The full decoded picture
is dropped at once, and closing the editor drops the rest. Previews any
larger stay held by the renderer after the editor closes.

Save Changes sends what changed, as the client does, in up to two
requests, each only with fields to send:

1. `PATCH /guilds/{id}/members/@me` with `nick` and `avatar`. Discord
   answers with the member and sends GUILD_MEMBER_UPDATE, which Yapfast's
   names already take in, so the nickname and server avatar show in the
   member list and on messages.
2. `PATCH /guilds/{id}/profile/@me` with `banner`, `bio` and `pronouns`.
   No gateway event follows; the client reads its answer.

A picture goes as a data URI, `data:image/png;base64,...`, or `null` to go
back to the main profile's; a cleared text goes as `""`. Discord's docs
say the member endpoint also takes `bio`, `banner` and `pronouns`, but the
client never sends them there.

Discord's editor also has a profile theme, an avatar decoration, display
name styles, a nameplate, a profile effect and a frame, all Nitro and most
bought from its shop. Yapfast leaves them out; it only
[shows](#avatar-decorations-and-nameplates) what people equip.

A member's server profile shows wherever Yapfast shows them in that server:
their server avatar in the member list and on their messages, from their
member object, and their server banner, bio and pronouns on their profile
card, from `GET /users/{id}/profile?guild_id=...`, whose
`guild_member_profile` wins over their own where it isn't empty. Yapfast
keeps a fetched profile for five minutes, but fetches it again once
GUILD_MEMBER_UPDATE changes their nickname or server avatar, and after
this account saves its own.

`scripts/ui-session do 'pick avatar PATH'` chooses a picture in the editor,
as its file chooser would.

## Linux Rich Presence

Games and tools on Linux tell Discord what they're doing through its local
RPC socket, which the discord-rpc library and Discord's game SDK use. While
the account shares its game ([game detection](#game-detection)), Yapfast
serves that socket itself, so no bridge such as arRPC needs to run. Windows'
named pipes and the macOS socket aren't served yet.

The socket is the first free `discord-ipc-0` to `discord-ipc-9` in
`XDG_RUNTIME_DIR`, or else in `TMPDIR`, `TMP`, `TEMP` or `/tmp`. A socket
another program serves, such as Discord's own app, stays that program's, and
a game that only tries `discord-ipc-0` talks to it instead. A socket left by
an ended program of the same user is taken over. The socket has mode `0600`
and takes only that user's programs. `YAPFAST_DISCORD_RPC=0` turns it off,
and `YAPFAST_DISCORD_RPC_DIR` puts it in another directory, for Wine or a
sandboxed game that can't see the runtime directory; Yapfast doesn't change
sandbox permissions or set up Wine's bridge.

Yapfast answers the handshake with READY, PING with PONG, and CLOSE and
SET_ACTIVITY, where `activity: null` clears the activity; other commands get
an error. As Discord's desktop client does when a game connects, it asks
`GET /oauth2/applications/{id}/rpc` for the application's name, and the
first time the game names an image, `GET /oauth2/applications/{id}/assets`
for its images' ids. Images given by https address go to
`POST /applications/{id}/external-assets` and show through the media proxy
as `mp:` assets; each client's last 32 addresses are kept. Timestamps in
seconds become the gateway's milliseconds, and buttons become labels with
their addresses in the activity's `metadata`. Playing, Listening, Watching
and Competing activities show. Yapfast copies only the fields it knows, so a
game can't replace the custom status or claim another application.

The game that set its activity last shows. When it clears it or
disconnects, the one before shows again, and once none is left, the game
detection found. The activity lasts through reconnecting to the gateway and
is never saved. Presence updates keep to Discord's limit, as every other
presence change does.

## Game detection

Settings → Game activity → Share the game you play is the account's own
`show_current_game` setting, which Discord's apps change too. Yapfast reads
it from READY and settings updates and writes it with
`PATCH /users/@me/settings-proto/1`; it's on unless the account turned it
off, and a newly linked account shares nothing until READY says. While it's
off, nothing scans processes, downloads the game list or serves the RPC
socket.

Every 15 seconds (half a second against the test server, for tests) a
blocking task lists this user's processes with
`sysinfo`, built with only its `system` feature: each one's executable, start
time and user, and the command line only for Wine processes and the few
rules that need arguments. It never reads CPU, memory, environments or
threads, and drops the process list after each scan. A process seen before and unchanged
isn't matched again. Wine and Proton processes match by the Windows program
in their command line, and runtime helpers, crash reporters and anti-cheat
services don't count. The game started last shows and stays while it runs.
One that ended clears after two scans without it, up to 30 s. Only the
game's name, its application id and when it started go to Discord.

Rich Presence comes first, then the custom games, Discord's list and Steam.
A custom game is an executable and the name to show after Playing: a file
name matches the executable in any folder, a full path only there, and
Windows and Wine paths ignore case. Custom games are this device's, kept in
`layout.json`, and a change rescans at once.

Discord's list comes from
`https://cdn.discordapp.com/detectables/games-v1.json`, about 4 MB, as
Discord's desktop client fetches it. Yapfast keeps this platform's executables
and the Steam ids in `discord-games.json` beside the account cache, which
accounts share, and the CDN's ETag in `discord-games.etag`. It asks again a day
later with the ETag, so an unchanged list costs a 304; a failed download keeps
the old list and tries again an hour later. Memory holds only a hash of each
executable's file name. A process whose name has one has the file read again
for that name's rules, once per scan for all such processes, and the rules are
kept for the next scans.

Discord's list names many games only by their Windows executable. Steam's
`libraryfolders.vdf` and `appmanifest_*.acf` fill in: a process under a
game's install folder is that game, under Discord's identity when the list
has its Steam id and else under the manifest's name. Proton, the Steam Linux
Runtime and any other install with a `toolmanifest.vdf` don't count, nor do
the Steamworks redistributables. The manifests are read at most once a
minute, from Steam's usual folders, its Flatpak's and the libraries they
name, and the list again only when the games installed change; no game's
files are walked. A sandbox that hides the host's processes from Yapfast
hides their games too.

These take effect at startup:

- `YAPFAST_DISCORD_GAME_DETECTION=0` turns off scanning and the list's
  download. Rich Presence stays, under `YAPFAST_DISCORD_RPC`.
- `YAPFAST_DISCORD_GAMES_CACHE=/path/games.json` keeps the list there.
- `YAPFAST_DISCORD_GAMES_FILE=/path/rules.json` uses that list instead of
  Discord's. Steam still matches; point `YAPFAST_STEAM_DIR` at an empty
  folder too to match only the list.
- `YAPFAST_STEAM_DIR=/path/to/Steam` reads Steam's manifests from there.

A list of your own takes Discord's shape. Use `win32` for a Wine game and
`darwin` on macOS, leave out `id` for a game Discord doesn't know, and give
`arguments` to tell apart games one runtime starts:

```json
[
  {
    "id": "123456789012345678",
    "name": "My Game",
    "executables": [{ "os": "linux", "name": "my-game", "is_launcher": false }]
  }
]
```

`shows_status_and_games` in `crates/yapfast-discord/tests/fake_server.rs` gives
the socket, the list and its cache private paths, and plays a handshake,
activities, images, clearing, several clients, the share setting and a detected
process through to the fake gateway. Other runs against the fake leave Rich
Presence and detection off unless those paths are set, so they never take the
desktop's socket.

`yapfast-game-detection-probe` measures the detector alone, without Discord,
and prints counts, times and heap, never process names or paths:

```sh
nix develop path:./nix --command cargo build --profile measure -p yapfast-discord --bin yapfast-game-detection-probe
target/measure/yapfast-game-detection-probe /path/to/discord-games.json 30
```

## Role icons

A role with an icon (`icon` on the role, from GUILD_CREATE and GUILD_ROLE_*)
shows it 20px beside its group's name in the member list, and the highest role
with an icon shows 16px before a member's tag in the list. The picture is
`/role-icons/{role}/{hash}.png?size=64`. Only a server with the `ROLE_ICONS`
feature, or at boost level 2 or 3, shows them: Discord keeps a role's icon when
a server loses its boosts, and its client stops drawing it, so Yapfast does too.
GUILD_UPDATE brings the change. Unicode-emoji role icons and the icon beside a
message author aren't drawn.
