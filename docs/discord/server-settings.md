# Server settings

Server settings, in a server's menu after Hide muted channels and in its
header's dropdown after Invite, open Yapfast's version of Discord's Server
Settings (`application/server_settings/`): a screen over the window, as
Yapfast's own settings are, with the server's name heading a sidebar of the
pages this account may change and the open page beside it. Discord's
client offers its settings to anyone with Manage Server, Manage Roles,
Administrator, Ban Members, Manage Nicknames, either expressions
permission, Manage Webhooks, View Audit Log or View Server Insights
(`canAccessGuildSettings`); Yapfast offers them once at least one page it
has is open to this account. Each page shows only with its own
permission:

- Server Profile, with Manage Server: the server's name (2 to 100
  characters), icon and description (up to 300).
- Engagement, with Manage Server: the channel system messages go to and
  which of them Discord sends, members' default notifications, and the
  inactive channel and its timeout.
- Boost Perks, with Manage Server: the server's banner and invite
  background, each locked until its boost level, and the boost progress
  bar.
- [Emoji](#emoji) and [Stickers](#stickers), under Expression, with
  Manage Expressions or Create Expressions: the server's custom emoji and
  stickers.
- [Members](#members), under People, with any of Manage Server, Kick, Ban
  or Time Out Members, Manage Roles and Manage Nicknames: the server's
  members, found by name or id.
- Roles, with Manage Roles, under People: the server's roles, their order,
  and each role's name, colour, icon, permissions and members.
- [Invites](#invites), with Manage Server, under People: the server's
  invites, and pausing invites and DMs.
- [Safety Setup](#safety-setup), with Manage Server, under Moderation: the
  verification level, whether moderators need two-factor authentication,
  and the sensitive content filter.
- [Audit Log](#audit-log), with View Audit Log, under Moderation: who did
  what in the server.
- [Bans](#bans), with Ban Members or View Audit Log, under Moderation: the
  server's bans, and revoking one, which takes Ban Members.
- [Delete Server](#delete-server-and-transfer-ownership), last, in red
  after a line, for the owner: deleting the server.

A heading with no page under it doesn't show, so someone who may only
manage roles sees People alone.

Discord's other pages, from Server Tag to Templates, are left out on
purpose; the plan is kept outside the repository.

Only the open page's settings are held, from when it opens to when it
closes: opening a page asks Discord for it, and leaving it drops it. Each
page but Members, Invites, Safety Setup, Audit Log and Bans, which act at
once as Discord's do, edits a draft against what Discord sent. Once the draft differs,
Discord's bar shows over the bottom of the page, "Careful — you have
unsaved changes!", with Reset and Save Changes; until one is chosen,
choosing another page, Escape and the close button shake the bar red
instead, as Discord's does. Save Changes sends only what changed, and
Discord's answer replaces the page, so the bar goes. A refusal keeps the
draft, with Discord's words: a field it refused reads "Name: Must be
between 2 and 100 in length.", from the `errors` of its 50035. A new
snapshot closes the settings once this account may change none of their
pages, or isn't in the server, and a change elsewhere, which GUILD_UPDATE
brings, shows unless the draft changed the same field.

Server Profile reads `GET /guilds/{id}/profile` and saves with `PATCH
/guilds/{id}/profile`, as Discord's page does, with `name`,
`description` and `icon`, a data URI or `null` to remove it; Discord
answers with the profile and sends GUILD_UPDATE. Change Server Icon reads
the picture off the UI thread as the [profile
editor](people.md#editing-ones-server-profile) reads an avatar: the middle square,
at most 1024 pixels, with a 128-pixel preview. An animated icon takes boost
level 1 (`ANIMATED_ICON`).

Engagement and Boost Perks read `GET /guilds/{id}` and save with `PATCH
/guilds/{id}`, as Discord's pages do. Engagement sends
`system_channel_id`, `system_channel_flags`,
`default_message_notifications`, `afk_channel_id` and `afk_timeout`. Its
four switches each turn a bit of `system_channel_flags` off, as Discord's
page has them: welcome messages (1), the sticker reply to them (8), boosts
(2) and setup tips (4); bits Yapfast doesn't show stay as Discord has
them. The switches wait for a system channel and the timeout for an
inactive channel, offering Discord's 1, 5, 15 and 30 minutes and 1 hour.
The channel lists are the server's text channels, without forums and
announcement channels, and its voice channels, each in the sidebar's
order. Boost Perks sends `banner`, `splash` and
`premium_progress_bar_enabled`: the banner opens at level 2 (`BANNER`), an
animated one at level 3 (`ANIMATED_BANNER`), and the invite background at
level 1 (`INVITE_SPLASH`), and below its level each reads "Unlocks at
Level N", as Discord's do. Both pictures are read as the icon is, cut to
16:9, at most 1920 by 1080, with a 480 by 270 preview. GUILD_UPDATE, which
follows each save, reaches the open page, so a level reached elsewhere
unlocks its pictures; it keeps the snapshot's banner and owner current too.

The fake's "Server" is the one to test these pages on; its comments say
what each route takes and refuses. `scripts/ui-session do 'pick ...'`
chooses a server icon, banner, invite background, role icon, emoji or
sticker as the file chooser would; `scripts/ui-session --help` lists them.

## Roles

Roles lists the server's roles highest first, @everyone apart above them
as Default Permissions, with each one's members, a search box and Create
Role. Discord's client lets this account change only roles below its
highest, unless it owns the server; the rest show a lock, whose tooltip
gives Discord's reason, "Role is locked because it is your highest ranked
role…" or "…a higher rank than your highest role.", and open to be viewed.
A role an integration manages, a bot's or the boosters', can't be copied,
deleted or given by hand. Dragging a row by its handle, or its menu's
Move Up and Move Down, reorders the roles below the locked ones.

A role opens on three tabs, as Discord's editor has them:

- Display: the name, up to 100 characters; a colour from Discord's 20
  swatches, any other written as `#rrggbb` in place of Discord's colour
  picker, or none (Discord's grey, saved as 0), keeping a gradient's
  other colours as they are; the icon, from boost level 2 (`ROLE_ICONS`),
  an image up to 256 KB and not animated, read at most 128 pixels square,
  or a Unicode emoji typed or pasted in place of Discord's emoji picker,
  each taking the other's place; "Display role members separately" and
  "Allow anyone to @mention this role".
- Permissions: Discord's 49 permissions in its eight groups, with its
  titles and descriptions (51 on a Community server, with Stage's), a
  search box, and Clear permissions. They're switches, as Discord's are,
  in a list view that makes only the rows in sight. A
  switch is locked when this account holds that permission through none
  of its roles, or when turning it off on one of its own roles would take
  it from the account; the lock's tooltip gives Discord's reason. Clear
  permissions is locked the same way.
- Manage Members: the role's members, with a search box, Remove for each,
  after "Remove member", and Add Members, which searches the server's
  members and adds up to 30 at once.

The draft is the order and the open role. Save Changes sends the order
first, then the role whole, as Discord's client does; opening another role
or leaving Roles with changes shakes the bar. Create Role, Duplicate Role,
Delete (after "Delete Role"), Add Members and Remove go at once. A copy is
named "{name} copy", as Discord names it, takes the role as its draft
stands, and goes just below it. Created and copied roles open.
GUILD_ROLE_CREATE, GUILD_ROLE_UPDATE and GUILD_ROLE_DELETE reach the open
page: a role changed elsewhere shows unless the draft changed the same
field, and one deleted elsewhere closes if it was open.

Roles reads `GET /guilds/{id}` for the roles, boost level and features,
and `GET /guilds/{id}/roles/member-counts`, and holds them only while it's
open; members come by id, with names from those Yapfast already knows or
asks the gateway for. It saves with:

- `PATCH /guilds/{id}/roles`, `[{id, position}]`, only the roles that
  move; @everyone stays at 0.
- `PATCH /guilds/{id}/roles/{role}`: `name`, `permissions` as a decimal
  string, `color`, `colors` (`primary_color`, `secondary_color`,
  `tertiary_color`), `hoist`, `mentionable`, `unicode_emoji`, and `icon`,
  a data URI or `null`, only when it changed.
- `POST /guilds/{id}/roles`, `{name: "new role", color: 0, colors,
  permissions: "0"}`; a copy then saves its fields with the PATCH above,
  and moves.
- `DELETE /guilds/{id}/roles/{role}`.
- `GET /guilds/{id}/roles/{role}/member-ids`, up to 100, then the members
  by id over the gateway.
- `PATCH /guilds/{id}/roles/{role}/members`, `{member_ids}`, to add.
- `PATCH /guilds/{id}/members/{user}`, `{roles}`, with the role left out,
  to remove, as Discord's client does; Yapfast first asks the gateway for
  the member's roles, and refuses, "Yapfast doesn't know that member's
  roles yet. Try again.", if it still doesn't know them.

## Emoji

Emoji lists the server's custom emoji from `GET /guilds/{id}/emojis`, in
two tables, Emoji and Animated Emoji, newest first, each headed with the
slots it has left, "49 slots available", as Discord's page has them: the
picture, the name and who uploaded it. Each kind has 50 slots, or 200
with `MORE_EMOJI`, or 50 more than `premium_features.
additional_emoji_slots` gives, whichever is most; without
`premium_features`, boost levels 1 to 3 give 100, 150 and 250. Manage
Expressions changes any emoji, Create Expressions only those its account
uploaded, as Discord's does. A name changes in place, sent with `PATCH
{name}` on Enter or once the field loses the keys, and the X deletes the
emoji at once with `DELETE`, without asking; both show before Discord
answers, and the list Discord then gives is taken once no other answer is
still to come. A name keeps only letters, digits and `_`, at most 32, and
at least 2, padded with `_`, as Discord's client makes it.

Upload Emoji chooses a file, as Discord's does without its Emoji Studio,
which Yapfast leaves out with its crop, zoom, rotation and flip: a still
picture is cut to its middle square and made at most 128 pixels, a GIF
goes as it is, and over 256 KB is refused with "That emoji was too big!
Emoji must be under 256 KB." A dialog shows it at 128, 48 and 22 pixels,
as chat would, with its name, the file's name made valid, where spaces
turn to `_` as they're typed; Upload sends `POST {name, image}`. With no
slot of its kind left it says "This server does not have any open emoji
slots!", as it does for Discord's 30008 and 30018. GUILD_EMOJIS_UPDATE
changes the snapshot's emoji, and the open page then asks Discord again,
as Discord's does, unless its own request will bring the list anyway.

## Stickers

Stickers lists the server's stickers from `GET /guilds/{id}/stickers` on
Discord's four cards: Free Slots, with 5, and Levels 1 to 3, with 10, 15
and 30 more, or 90 at level 3 with `MORE_STICKERS`, which makes 120. The
stickers fill the cards in the list's order. An open card says "3 of 5
slots available", one the server's level hasn't reached the boosts it
takes, "2 Boosts", with a lock. Upload Sticker shows on the card where
the next goes, with Create Expressions, while a slot is left. A tile has
the sticker's picture, name and uploader; its pencil and X show on hover
for those this account may change, by the same rule as Emoji's. The X
deletes the sticker at once with `DELETE`; a Lottie sticker has no
picture.

Upload Sticker and the pencil open Discord's form: the file, chosen with
Browse, PNG, APNG, JPEG or GIF, which an edit can't change; Related Emoji,
an emoji typed or pasted, without Discord's emoji picker; the Sticker
Name, 2 to 30 characters; and the Description, empty or 2 to 100. A GIF,
an APNG, or a PNG of 512 KB or less goes as it is; another picture is made
a PNG within 320 pixels; over 512 KB is "That sticker was too big!
Stickers must be under 512 KB", and an unreadable file "Invalid sticker
file". Upload sends `POST` as multipart form data with `name`, `tags`,
`description` and `file`, and Update `PATCH {name, tags, description}`;
Discord's refusal shows in the form, in its words. GUILD_STICKERS_UPDATE
keeps the open page current, keeping who uploaded each sticker, which it
doesn't say.

## Members

Members is Discord's Server Members page for a server that isn't a
Community one; Discord's client sends a Community server's to a page in
its channel list, and Yapfast keeps the settings page. Its heading reads
"Recent Members", or "Search Results" once a search or an order is chosen.
Each row has the member's avatar and name, in their highest coloured
role's colour, with the username under it when it differs; Member Since
and Joined Discord, as Discord words them, "3 days ago" (Discord's months
are 30 days and its years 360); Join Method, with Manage Server: the
invite's code, "Server Discovery", "Bot Invite", "Manual Verification
(code)" and so on, or "Unknown"; their highest role and "+2" for the rest;
and Discord's signals, each with its tooltip: timed out, with the time
left, unusual DMs in the last two days, suspected spam, and a name AutoMod
quarantined. A row opens the member's profile, and a right click or More
Options their menu, which acts in the settings' server; once a nickname,
timeout, kick or ban from it is done, the search runs again.

`POST /guilds/{id}/members-search` finds them as Discord's client does:
`{"or_query": {}, "and_query": {}, "limit": 250}` for the newest, and the
search box, 300 ms after typing stops, splits its text at commas into ids,
17 to 19 digits, for `and_query.user_id` and names for
`and_query.usernames`; one character searches nothing. Sort chooses
Discord's four orders, by when members joined the server or Discord,
newest or oldest first. Discord sends chunks of 250, or 500 with 100 to a
page; the page sorts what came and shows 12, 25, 50 or 100 a page, with
"Showing … members of N", and a page past those fetched asks for the next
chunk, `after` the last member's join time and id. While Discord indexes
the server it answers 202; Yapfast says "Before searching, we need to
index this server. Give us a bit.", waits the `retry_after` it gives and
asks again, three times at most. With no match the page says "No members
match these search results."; a refused search shows nothing new, as in
Discord's client.

The page holds the members fetched, and only the page shown has rows and
asks for avatars.

Left out, as with the rest of the plan's: the date, role, signal and join
method filters, Prune, selecting members to ban several at once, the mod
view, the count of members who joined since the page opened, and Show
Members In Channel List.

## Invites

Invites lists the server's invites from `GET /guilds/{id}/invites`, by
their inviter's username, as Discord's page does: the inviter's avatar
and username with the channel under it, the code, the uses, "3" or "3/10",
when it expires, counting down each second as "DD:HH:MM:SS", or "∞" for
never, and the first role it gives with "+2" for more. A row's X revokes
the invite at once with `DELETE /invites/{code}`, without asking, as
Discord's does. Create Invite Link opens Yapfast's invite dialog for the
server, and the list asks Discord again once it makes the invite.

Pause Invites, or Enable Invites while they're paused, opens Security
Actions: a duration of 1 to 24 hours, 2 at first, and the Pause Invites
and Pause DMs switches as the server has them; Save waits for a change,
then sends `PUT /guilds/{id}/incident-actions` with
`invites_disabled_until` and `dms_disabled_until`, the same time that many
hours on, or `null`, and `lockdown_duration_hours`. Invites are paused
while the server's `incidents_data` says so, or it has `INVITES_DISABLED`;
their rows dim. GUILD_UPDATE keeps it current. The questions Discord's
client asks after resuming go only to its analytics, and are left out.

## Safety Setup

Safety Setup is Discord's page of that name for a server that isn't a
Community one, without what Yapfast leaves out: member prune, Activity
Alerts, the mod queue and Member Verification. Each choice goes to Discord
as it's made, without the bar, and shows while Discord takes it; a refusal
puts back what Discord has, with its words.

- **Verification Level**: None, Low, Medium, High and Highest, each with
  Discord's colour and what it asks of members, from a verified email to a
  verified phone. A Community server can't have None, which says "This
  option is not available for Community Servers." Saved with `PATCH
  /guilds/{id}`, `{verification_level}`.
- **Require 2FA for moderator actions**: only the owner, with two-factor
  authentication on, may move it, as Discord's client decides. For anyone
  else its lock's tooltip says why: "This setting can only be changed by a
  server owner with 2FA enabled on their account.", or for an owner
  without 2FA, "To change this setting, first enable 2FA on your
  account.", with Discord's note under it; a Discoverable server keeps it
  on. Saved with `POST /guilds/{id}/mfa`, `{level}`, 1 or 0, which Discord
  holds back until the owner [confirms](#two-factor-authentication).
  Whether this account has 2FA on comes from READY's `mfa_enabled`, kept
  current by USER_UPDATE.
- **Sensitive content filters**: everyone's images, those of members
  without roles, or no one's; a Community server filters everyone's. Saved
  with `PATCH /guilds/{id}`, `{explicit_content_filter}`.

On a Community server Discord's client spreads these over its own Safety
Setup pages; Yapfast keeps them on one, with Discord's Community wording.

## Audit Log

Audit Log is Discord's page of that name: who did what in the server,
newest first, each row with the moderator's avatar, Discord's sentence for
it, such as "**alice** kicked **mallory**", and when, as Discord's client
words it: "Today at 16:20", "Yesterday at …", "Last Monday at …", then the
day. `GET /guilds/{id}/audit-logs?limit=50` fetches the newest 50, and
older ones follow with `before`, the last row's id, when the list is
scrolled to its end or Load More is pressed, while the last page was full.
Filter by User and Filter by Action ask again from the newest, with
`user_id` and `action_type`; the actions are Discord's 68, in its order.
Discord's client waits on a filter while a page is coming, and so does
Yapfast. With no entries the page says "No Logs Yet"; a refused first page
says "This is Awkward", and a refused older one only stops loading, as in
Discord's client.

Each page Discord sends is put into words once, as it arrives
(`application/server_settings/audit_log.rs`), and only the words are kept.
They are Discord's, from its web client's templates:

- **Rows.** Entries by the same moderator, of the same action, to the same
  thing and with the same options, within 30 minutes of the first, show as
  one row, of at most 51, with when the first and last were, "Today at
  16:20—Today at 16:41"; invites, messages, members moved or disconnected,
  channel and role moves, bots added, prunes and command permissions never
  group. An entry whose target is gone and has no name is dropped, but for
  the actions that have none.
- **Sentences** for every action Discord's client words, from "made changes
  to" a server to AutoMod's and Server Guide's, with each channel's kind
  for a new channel or thread, and Discord's four forms for members moved
  or disconnected from voice. An action Discord's client has no sentence
  for reads "{user} made changes".
- **Names.** A channel is `#name` where it holds messages; a role, emoji or
  channel by the server's names, a user, webhook, integration, thread,
  event, AutoMod rule or app command by the names Discord sends with the
  page, and something deleted by the name its deletion recorded, also on
  earlier rows; failing those, the name its own changes give, then its id.
- **Lines.** A row's arrow opens its lines, one row at a time, numbered
  "01—" and on in the green, yellow or red of making, changing or
  removing. The reason comes first, "With reason **spam**", then each
  change in Discord's words for the kind of thing changed: the server, a
  channel and its permission overrides, a member, a role, an invite, a
  webhook, an emoji, a sticker, an integration, a stage, an event and its
  recurrences, a thread, an AutoMod rule, a sound, a voice channel's status,
  onboarding and its questions, the Server Guide, member applications and
  the server profile. A role's permissions granted or denied, and a channel
  override's granted, denied or reset, list each permission by Discord's
  name under their line ("Manage Channel" and "View Channel" for a
  channel's); a member's roles added or removed list each role; a timeout
  gives its end as the times above. A deletion shows only its reason, but
  for a kick, prune or ban, and keys Discord's client hides, such as a
  channel's overwrites or a sticker's asset, stay hidden.

Yapfast differs where Discord's client needs what it doesn't hold: Filter
by User lists the moderators the fetched entries name, where Discord's
lists the loaded members with moderation permissions; a row has no action
icon beside the avatar; a channel's type reads as its kind, where
Discord's client shows its number; and an app's, an emoji's or a forum
tag's picture is left out of its line, as are the answers under a changed
onboarding question and the Server Guide's changed items. Load More shows
while there may be more, where Discord's waits for three pages loaded by
scrolling.

A row holds its sentence as one string, with its bold and times marked
in it, its oldest entry's id as a number, and its moderator as a place in
the user filter's list, whose avatar its rows share; only the open row's
lines are made for the screen. Opening either filter's list the first
time keeps its memory until Yapfast closes, as every list of Slint's
ComboBox does.

## Bans

Bans is Discord's Server Ban List: the server's bans in Discord's order, 100
to a page with Previous and Next, each with its avatar, name, and username
under a display name. `GET /guilds/{id}/bans?limit=1000` fetches them, and
while the last batch was full, a page past those fetched asks for the next
batch after its last user's id, as Discord's client does. Search, or Enter
in its box, asks `GET /guilds/{id}/bans/search` with the first name typed
as `query` and every user id, 17 to 19 digits, as `user_ids`, split at
commas, and shows those fetched that match; emptying the box shows them
all again.

A ban opens a dialog with the username, its reason or "No reason
provided", and, with Ban Members, Revoke Ban and Done. Revoke Ban sends
`DELETE /guilds/{id}/bans/{user}`, which Discord may hold back for
[two-factor authentication](#two-factor-authentication); the ban goes from
the list once Discord lifts it, and a refusal shows in the dialog. While
Bans is open, GUILD_BAN_ADD adds a ban made elsewhere, without a reason as
in Discord's client, and GUILD_BAN_REMOVE takes one off. The page holds the
bans fetched, and only the page shown has rows and asks for avatars.

## Delete Server and Transfer Ownership

Both are the owner's, as in Discord's client: an owner who may give the
server up, with two-factor authentication on if the server's moderators
need it (`isOwnerWithRequiredMfaLevel`). Whether the server needs it comes
from its `mfa_level`, kept current by GUILD_UPDATE.

Delete Server, the sidebar's last row, opens "Delete 'Server'": "Are you
sure you want to delete **Server**? This action cannot be undone." Without
two-factor authentication on, it asks for the server's name too, compared
in any case and untrimmed, as Discord's is, and a wrong one says "You
didn't enter the server name correctly". Delete Server sends `POST
/guilds/{id}/delete` with no body, which Discord holds back for
[two-factor authentication](#two-factor-authentication) when it's on;
GUILD_DELETE then takes the server away.

Transfer Ownership is in a member's menu on [Members](#members), in its
own red group before Copy User ID, as Discord's client has it there and
only there; it isn't offered for this account or a bot. Its dialog says
"This will transfer ownership of **Server** to **bob**. This cannot be
undone!" with both owners' avatars, and Transfer Ownership waits for the
box "I acknowledge that by transferring ownership of this server to
**bob**, it officially belongs to them." A Verified or Partnered server's
box stays off, under Discord's notice that the Community team handles
those. With two-factor authentication on, or without an email address,
it sends `PATCH /guilds/{id}` with `{"owner_id", "code": null}`, which
two-factor authentication confirms; otherwise `PUT /guilds/{id}/pincode`
has Discord email a code, the dialog asks for it, with "Didn't receive a
code or it expired? Resend it.", and the `PATCH` sends it as `code`.
Discord's 50164 reads "Server ownership transfer failed. It looks like the
new owner is ineligible for Server Monetization." Whether the account has
an email address comes from READY's `user.email`, kept current by
USER_UPDATE.

Yapfast differs from Discord's client in waiting: Discord's closes each
dialog as it sends, and drops any refusal but 50164 unseen, where
Yapfast's dialogs wait for Discord's answer and show a refusal in them.
Once Discord deletes or gives the server away, the settings close. Left
out: the server's icon beside its name, a nickname's "AKA" in the
transfer's text, and the Server Monetization notice.

To test either against the fake, `POST /_control/guild` makes "Server"
Alice's; its comment says how.

## Channel settings

Edit channel, in a channel's or category's menu, opens its settings over
the window, as Discord's client does (`application/channel_settings/`,
`ui/channel_settings.slint`), with the server settings' frame and
unsaved changes bar. The menu offers it with Manage Channels or Manage
Permissions (Discord's Manage Roles) there, and each page shows only with
its own: Overview with Manage Channels, Permissions with Manage
Permissions. Delete Channel (or Delete Category), the sidebar's last row,
for whoever the menu offers it to, closes the settings and asks as the
menu's Delete channel does. Leaving a page or the settings with unsaved
changes shakes the bar, and the settings close once this account may
change neither, or the channel is gone.

Overview is the name, and for a text channel its topic, slowmode, one of
Discord's from 0 to 6 hours, and age restriction; a voice channel or a
category has only its name. A text channel's are read with `GET
/channels/{id}` as Overview opens, and saved with `PATCH /channels/{id}`.
Names are 1 to 100 characters, a text channel's lower case with dashes
for spaces, and topics up to 1024, checked before asking.

Permissions is Discord's Channel Permissions page:

- **Sync notice.** A channel in a category whose permissions this account
  may also change says whether its overwrites match the category's,
  "Permissions synced with category: Talk", and if not offers Sync Now,
  which asks "Sync permissions?" first. An overwrite that allows and
  denies nothing doesn't count.
- **Private Channel.** The switch denies @everyone View Channel, and for a
  voice channel Connect as well, or no longer either way. Without every
  permission, turning it on gives this account an overwrite of its own
  that keeps them, as Discord's client does.
- **Advanced permissions.** On the left are the overwrites: @everyone
  first, then roles highest first, then members by name, with "+" to add
  a role or member. On the right are the shown overwrite's permissions,
  with Remove for any but @everyone. Each permission is Discord's three
  buttons: deny, neither (left to the roles), and allow. They come in
  Discord's groups for the channel's kind (`generateChannelPermissionSpec`),
  with its words for a channel or a category, as the list view's rows.
- **Locks.** As Discord's client decides, this account may change only a
  permission it holds across the server, and never Manage Permissions,
  unless it has every permission or the channel's own overwrites give it
  Manage Permissions. A locked permission shows a lock whose tooltip says
  "You cannot modify this permission because none of your roles have
  it."
- **Denying yourself.** A change after which this account couldn't do
  that in the channel is refused with Discord's "Pretty sure you don't
  want to do this".

Saving sends each changed overwrite with `PUT
/channels/{id}/permissions/{overwrite}`, `{id, type, allow, deny}`, type
0 for a role and 1 for a member, and the bits as decimal strings, one
after another as Discord's client does; the first refusal stops the
rest. Discord checks each against what the ones before it left, and a
denied View Channel takes Manage Permissions with it, so those that only
give go first, then this account's own, and @everyone's last: Private
Channel's deny before the account's own allow would lock it out of the
channel halfway. A role or member added from the picker is put at once with nothing
set and shown. Remove, after "Delete Permission Settings", sends `DELETE
/channels/{id}/permissions/{overwrite}`. Sync Now sends `PATCH
/channels/{id}` with the category's `permission_overwrites`, and an empty
one for @everyone if the category has none. Discord sends CHANNEL_UPDATE
after each. The page takes what Discord took at once, and a snapshot from
before that CHANNEL_UPDATE doesn't undo it. Overwrites changed elsewhere
show unless the draft changed the same one. The page holds the
overwrites and the names of the members they're for, which Yapfast asks
the gateway for, only while it's open.

## Two-factor authentication

Discord holds some requests back until this account confirms with
two-factor authentication: moderation in a server that requires 2FA for
it, and an owner's own actions. It answers 401 with code 60003 and a
ticket, `{"mfa": {"ticket": ..., "methods": [{"type": "totp"}, ...]}}`.
Discord's client then shows "Multi-Factor Authentication", turns the
confirmation into a token with `POST /mfa/finish` (`{ticket, mfa_type,
data}`, answered with `{token}`), and sends the request again with the
token in `X-Discord-MFA-Authorization`. It keeps no token: each request
held back asks again.

Yapfast does the same for member moderation (nicknames, roles, timeouts,
kicks, bans, and voice), channel changes and server settings' saves:
`Discord::stepping_up` in `yapfast-discord` runs the request, and on
60003 sends `Live::MfaRequired` and waits for the prompt's answer, then
runs it again inside the token's scope, so whatever asked keeps waiting
for its usual answer. The prompt (`application/mfa.rs`, `ui/mfa.slint`)
starts at Discord's first method and lists the rest under "Verify with
something else", as Discord's does: a code from an authenticator app or a
text, six digits; the password; or a backup code, eight to eleven
characters, sent without its dashes, after Discord's warning, "Slow down!
Are you sure?", whose button waits ten seconds each time it shows. A text goes out as its method opens,
with `POST /auth/mfa/sms/send`, and "Resend Code" sends another. A
passkey needs a browser, so Yapfast asks to confirm in Discord's app
instead. A confirmation Discord refuses (60008) leaves the prompt open to
try again; Cancel fails the request, "Two-factor authentication was
cancelled." Requests held back at once each get a prompt, one after
another.

`POST /_control/mfa` has the fake hold these requests back; its comment
gives the codes it takes.
