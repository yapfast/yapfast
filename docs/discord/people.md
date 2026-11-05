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
  and `PUT` with `{"note": ...}` saves it, an empty one removing it.
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
enlarges it into the [full profile](#the-full-profile). The card leaves the
dates and the roles' heading to the full profile, and ends in a box, "Message
@name", as Discord's does: Enter opens or starts the DM with them and sends what
it holds there, without touching the DM's own composer. The box shows only for
someone Yapfast can message, never for this account, whose own card has Edit
Profile there, and shows no mutuals, as its full profile doesn't either. The
line of what's mutual stacks up to three of the mutual friends' avatars, 16
pixels each, before its words, in the list's order: Discord's client sorts
them by how much the account talks to them, which Yapfast doesn't know. With
none in common it has no stack; Discord's client shows server icons there,
which Yapfast leaves out. The badges go in a pill beside the handle and pronouns,
which shorten with "…" to leave it room;
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
their Hang status, headed by what it is ("Listening to Spotify"), with its
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

A card takes the rest of its look from the profile, the server's before their
own. `theme_colors` colours it, darkened on a dark theme, with its text picked
to read on them; `avatar_decoration_data.asset` frames the picture, and an
avatar or banner whose hash starts `a_` moves, each as the [profile
switches](#profile-switches) allow; the user's
`primary_guild` is the server tag after the name, unless `identity_enabled` is
false. `display_name_styles` `{font_id, effect_id, colors}` draws the name, in
one of the fonts in `ui/fonts/names`; a font Yapfast doesn't have falls back to
the UI's typeface. The custom status is a bubble beside the picture, two lines
until pointed at.

With [Play profile effects](#profile-switches) on, a profile effect comes from
`profile_effect.sku_id` through `GET /collectibles-products/{sku}`, once for each: `items[].effects[]`, APNG layers on
a 450 by 880 canvas. It starts half a second after all its layers arrive, fades
while the card is pointed at, and holds a layer that loops with no wait when its
file says it plays a set number of times. Clicking one's own picture on a card
opens the editor.

### Profile switches

Settings → Appearance → Profiles has a switch for each cosmetic that downloads
or moves (`Cosmetics` in `application/cosmetics.rs`, `Looks` in
`ui/theme.slint`, saved in `layout.json`). What's off is left out before
anything is asked for: the core requests no picture for it, the card gets
nothing of it, and the profile is fetched without asking Discord about the
effect or frame (`DiscordCommand::Profile`'s `collectibles`). Discord's
profile is kept without them, so turning one on shows it when the profile is
next fetched.

| Switch | Off | Starts |
| --- | --- | --- |
| Play profile effects (`profile_effects`) | No `/collectibles-products/` request for the effect, and none of its layers | Off |
| Show avatar decorations and profile frames (`decorations`) | No decoration, no frame request and none of its layers | On |
| Animate profiles (`moving_profiles`) | A moving avatar or banner comes as its PNG still, a decoration without `passthrough`, so one frame, and styled names hold still | Off |
| Show name styles (`name_styles`) | The name in the UI's typeface and the theme's colour | On |
| Show server tags (`server_tags`) | No tag and no badge picture | On |

Effects and motion start off: they cost a decode of every frame and a redraw
for each while a card shows, for looks alone. The others start on: a
decoration, a frame, a tag's badge and a still banner are a picture or a few,
decoded once, and name styles use the fonts Yapfast carries, so they ask for
nothing.

The account's menu, bottom left, is a card of its own, fetched as it opens with
the same request as anyone's, with what the account shows it's doing, then Edit
Profile, its status, Badges and Copy User ID.

## The full profile

The full profile is a window over the rest, with Discord's two parts: the
person's card, which keeps their theme and effect, and tabs beside it. The
card has their banner, picture, name, handle, pronouns, tag and badges, then
Message, Add Friend when there's a friendship to offer, and the more button,
then the bio, Member Since, Friends Since, roles and a line that opens the note's
dialog; the note itself is only asked for there. On one's own, Edit Profile
takes Message's place. Message closes the window and opens the DM. A window too
narrow for both parts has the tabs under the card, and all of it scrolls.

Its sizes are the client's stylesheet's: 962 wide and up to 800 tall, 48 clear
of the window's edges, all of it under 929 wide, with the card 45.5% of the
width, 300 to 400. A themed person's window takes on their colours, as
`--profile-gradient-modal-background-color`: the two colours half and half, with
the lightness held to 10% or less, or 80% or more on a light theme, so a card in
dark colours blends into it and a bright one stands out. Without a theme the
window is a shade darker than the card. Their Message button is the first colour
darkened or brightened until white on it contrasts 7 to 10
(`--profile-gradient-button-color`). A friend has a button between Message and
the more button; it opens the same menu, where Discord's has a shorter one.

With [decorations and frames](#profile-switches) on, a profile frame comes from
the profile's `collectibles`, the item of type `profile_frame`, through `GET /collectibles-products/{sku}`: `inner_width`, the
overflows, and `layers` of `{id, type, order, anchor}`, each a still at
`/media/v1/collectibles-shop/{sku}/{id}/static`. The client has no moving
version of these. It scales them so `inner_width` is the card's width and draws
them past its edges: over the card from the front layers, behind it from the
back, and on the full profile without a foot's. It hides them there in a window
785 tall or less. The small card shows them too, without the room Discord leaves around it.

Friends Since comes from `since` on the relationship in READY and
RELATIONSHIP_ADD, for a friend. Dates here read as Discord writes them, "Feb 21,
2026".

The tabs, in the client's order and by its rules:

| Tab | Shows when | Label |
| --- | --- | --- |
| Board | It's one's own, or they have widgets | Board |
| Activity | Always | Activity |
| Wishlist | It's one's own, or it's public and they're a friend or share a server | Wishlist |
| Mutual friends | It isn't one's own | "3 Mutual Friends", "No Mutual Friends", or "Mutual Friends" until Discord says |
| Mutual servers | It isn't one's own | The same, of servers |

Activity lists the call they're in, from the voice states Yapfast keeps for each
DM and channel this account sees, with Join when `can_join_voice` allows; their
activities under "Current activity"; then under "Recent activity", from `GET
/content-inventory/users/{id}/outbox` when the tab first opens, what they listened
to (`extra.entries[0].media`: its `title`, first artist and `image_url`), watched
(`extra.media_title`) and played (`extra.game_name`), each with how long ago, from
`ended_at`, else `started_at`.

The first that shows opens first, so a board comes before activity. The tabs
don't move when one's chosen, and what's long scrolls under them. An empty
Activity or mutual tab says what the client's does. It picks from several
lines at random and Yapfast always says the first. It also opens on the Wishlist
for some accounts, under an experiment, which Yapfast doesn't.

Yapfast differs in two ways, both so it asks for less. The client fetches a
wishlist when the window opens, and hides the tab if that comes back empty;
Yapfast asks when the tab opens, and an empty one stays, with a line saying so.
It leaves out the client's age check on the wishlist (`nsfwAllowed`), and the
wishlist's Filter and Browse the Shop, and doesn't track new items. A private
list or a refusal says so in a line.

Other people's board and wishlist come only for the full profile, and only
when their tab opens: `GET /games` and `GET /applications/public` for the board's
names, read-only, once for each profile shown, and the wishlist as under [Board and
Wishlist](#board-and-wishlist). A wishlist that failed is asked for again when
its tab next opens. A profile that arrives after the tab opened asks then. A
small card and the DM's panel ask for neither, and show them not at all.

### Board and Wishlist

The full profile's Board and Wishlist tabs show what someone has on Discord,
read-only, one's own included. Long ones scroll under the tabs' headings. They
never show on a card, as reading them costs requests a card shouldn't make.

The Board comes with the profile: `widgets`, each `{id, data: {type, ...}}`. A
game list's `data` holds `games: [{game_id, comment, tags}]`, read as the client
reads them (its `Widget` record's `fromServer`), a game listed twice showing once
and an unknown type left out. Their headings are the client's:

| `type` | Heading | Shows |
| --- | --- | --- |
| `favorite_games` | Favorite Game | Its game and comment |
| `current_games` | Games in Rotation | Its games |
| `want_to_play_games` | Want to Play | Its games |
| `played_games` | Games I Like | Its games |
| `application` | The application's name, from `GET /applications/public` | A line that it only shows in Discord |
| `personal` | Personal | The same line |
| `clips_gallery` | Clips Gallery | The same line |

Games are named by `GET /games?game_ids=...`, at most 20 at once as the client
asks, when the tab first opens, and only if it has any. A game's picture
is `media.icon`, else `media.cover`, each `{type: "hash" | "url", value}`, where
a hash is at `/app-icons/{game}/{hash}.png`. One Discord doesn't know is an
"Unknown game", as in the client. Tags aren't shown.

The Wishlist is asked for when its tab first opens, and again if that failed:
`GET /wishlists/{id}?source=user_profile`, which the client uses for its own
account's and for anyone's, the first id in the profile's `wishlist_settings`
`{id: {updated_at, visibility}}`, where visibility 0 is private. Its only other
route Yapfast knows of, `POST /users/@me/wishlist/items`, adds an item, which
Yapfast never sends. With no id there's nothing on it, and a 403 or 404 says it's
private; a private list of one's own loads, and says that. An item in
`wishlist_items` is named by `sku.name`, else `sku_name`, and pictured by what it
is, `sku.product_line`, else the item's `sku_product_line`:

| Line | What | Picture |
| --- | --- | --- |
| 1 | Nitro | None: the client draws its own, so the card shows a letter |
| 7 | Avatar decoration, by `type` 0 | `/avatar-decoration-presets/{asset}.png` |
| 7 | Profile effect, by `type` 1 | `thumbnailPreviewSrc`, else `reducedMotionSrc`, `staticFrameSrc` |
| 7 | Nameplate, by `type` 2 | `/media/v1/collectibles-shop/{sku}/static` |
| 7 | Profile frame, by `type` 3 | `/media/v1/collectibles-shop/{sku}/{layer}/static`, the first layer not `responsive` |
| 7 | Bundle | `sku.preview_asset_paths.fg_static`, else the first thing in it with a picture |
| 14 | A game's item | `/app-assets/{sku.application_id}/store/{asset}.webp`, the asset being `tenant_metadata.social_layer.card_image_asset_id`, else `thumbnail_asset_id` |

A collectible's kind and `asset` are in `sku.tenant_metadata.collectibles.item`,
else the item's `collectibles_item` or `bundle_items`. The client's card shows
only lines 1, 7 and 14; Yapfast also shows others it can name, by the store
picture if they have one.

### Someone's badges

Clicking a badge on someone's small card or full profile opens their badge
window on that badge, unless they're a bot, whose badges keep their links; on
one's own, it opens the account's window. The DM's panel keeps its links too. It's
`GET /users/{id}/badges`, which lists what they own, with `GET
/users/@me/badges` for this account's, as the client does, and it asks each time
it opens. A profile's badge `id` maps to the window's number by the client's
table (`profile::directory_id`).

On the left are their avatar and name, shortened past 19 characters, their badges
and View Your Badges, which opens the account's window. On the right is the
chosen badge at the level the client picks: the one reached if owned, else the
next, else the first. There's its art, the badge's name over the level's, or
"Nitro" and the level for Nitro, or the badge's name alone without levels. Under
that are "Beta" for Account Age, Streaming, Game Time and Game Variety, "{name}'s
badge" and their `info_label`. "View yours" shows when this account owns the badge
too, and opens its window on it, on a line of its own after three things.

Then come the tiles: "Date unlocked" at the first level or "Level unlocked" past
it, from `tier_obtained_at`, which a badge without a date leaves out, and
rarity, the level's over the badge's. The requirement's words are this account's
version, or the client's "You've maxed out this badge" when it has the last
level. Then every level of theirs: dimmed and locked if not reached.

A badge that can no longer be earned is marked Legacy Badge, says so to anyone
without it, and gets one of the client's lines for a missing date, the same each
time where the client picks at random. Its buttons open Discord's pages in the
browser, and never give or buy anything:

| Badge | Button | Page |
| --- | --- | --- |
| Staff | Work With Us | `/careers` |
| Nitro | Nitro Home, or Get Nitro without it | `/store` |
| Orbs | Orbs Shop | `/shop?tab=orbs` |
| Quests | Quests Home | `/quest-home` |
| Gifting | Send Gift | `/shop`; the client opens its gift dialog in place |

Boosting has none, as the client's goes to settings with no page for it. The
client also offers Nitro's Upgrade to someone on a lesser Nitro, and has newer
layouts of this window under an experiment; Yapfast keeps to the one above.

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
the Server Tag and badges. Yapfast's editor leaves them out. Changing the
username, in My Account with the password, is left
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
bought from its shop. Yapfast leaves them out, as it shows none of them.

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

## This account's own activities

Discord's client doesn't take its own activities from a presence. It shows
what its session sends, and the activities of the first other active session in
READY's `sessions` and `SESSIONS_REPLACE`, without custom statuses, each
`type:application_id:name` once, sorted as anyone's are, with one game at most.
`Snapshot::own_activities` is that list. It fills the account's menu, its own
profile card and its full profile's Activity tab, under Discord's headings.

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
