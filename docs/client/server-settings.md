# Server and channel settings

## Server settings

A Discord server's settings (`server_settings/`) open from the server's
menu (Server settings): on
the settings frame (`sheet.rs`), a card over the window dimmed by 70%,
48 px in from its edges, the pages this account may change down its left
on the sidebar's ground, headed by the server's name and grouped under
Expression, People and Moderation as Discord's are, with Delete Server
after a line for its owner; the open page on the right, at most 660 px
wide, with the round close button and ESC at its top right. What each page
holds, what this account may do there and what saving sends are the
core's (`ServerSettings`); the screen keeps only its own (an open
dropdown, the description's field, the bar's shake) and lets go of it,
and of the pages' pictures, when they close. Once a page's draft differs
from what Discord has, the form kit's `Unsaved` bar floats over its foot
with Reset and Save Changes; leaving the page or the settings then shakes
it red instead, as Discord's does. Escape closes an open dropdown, then a
dialog over the page, then the settings.

- Server Profile (`profile.rs`): the name, the icon with Change and
  Remove (the file dialog, then the core reads it), the banner colour
  (Discord's ten swatches, `BANNERS`, four across, under a card that
  previews the one picked with the icon and name), five trait fields three
  across, each by a steady id (`server-trait-N`) so the one typed in keeps
  the keys, and the description with what's left of its 300 characters.
- Engagement: the system messages' channel and its four switches, which
  wait for a channel; the default notifications; the inactive channel and
  its timeout, which waits for one.
- Boost Perks: the server's level, the progress bar's switch, and the
  banner and invite background at 16 by 9, each with the level that
  unlocks it.
- Roles (`roles/`: the list, the editor and its members' tab each a
  file): @everyone's card,
  the search with Create Role, and the roles in their order with how many
  hold each, Edit (View for a locked one) and More (Move Up, Move Down,
  Duplicate Role, Delete). A role at or above this account's highest is
  locked, with Discord's words why under its lock; the others move by
  dragging their handle or from More. Edit opens the editor: Back, the
  role's name, a notice for a locked or managed role, and the Display
  (name, colour swatches and a custom hex, the icon, hoist and mention
  switches), Permissions (searchable, a permission this account lacks or
  would lose locked with why) and Manage Members (search, Add Members,
  remove) tabs. Delete Role's and Remove member's question, and Add
  members (search, up to 30 ticked), open over the settings; Escape closes
  the More menu, then them. What may change is the core's `RoleRules`.
- Members (`people.rs`): the search,
  sent once the typing has stopped for 300 ms (a timer task numbered by
  the typing, so only the last one's ending searches; Enter at once), the
  order, and the columns' headings sharing the row as Discord's grow, Join
  Method only with Manage Server. The members on the page are rows of
  56 px, only those in view built and their pictures asked for: the
  avatar, the name in its highest coloured role's colour, the username,
  Member Since and Joined Discord as Discord words them, the join method,
  the first role as a chip with "+N", and Discord's signals (unusual DMs,
  timed out with what's left, spam, quarantined) as icons with tips. A
  click opens the member's profile over the settings, More Options and a
  right click their menu; Escape closes either before the settings, and
  the composer under them doesn't take the keys back. The footer: the
  page size, how many in all, and the pager.
- Invites: Pause or Enable Invites (Security Actions: how long, invites,
  DMs, Save once changed), Create Invite Link (the invite dialog), and
  the invites with their inviter, code, uses, a countdown read each
  second as it's drawn, and roles; dimmed while paused, the X under
  the pointer revokes.
- Safety Setup (`moderation.rs`): the verification levels and content
  filters as the kit's radio `options`, in Discord's colours, and those a
  Community server can't choose in Discord's words why; the switch
  requiring 2FA for moderators, locked with why unless its owner has 2FA,
  with Discord's note. Each choice goes to Discord as it's made.
- Audit Log: filters by user and action, the rows in bordered cards (who
  did what in bold, when on the chosen clock, an arrow for a row with
  lines), an open row's numbered lines in its kind's colour; Load More,
  and older entries loading at the list's end; empty and failed states.
- Bans: the search, the bans a page at a time with the pager, No Bans, and
  a ban's dialog with its reason and Revoke Ban.
- Emoji (`expressions.rs`): Discord's
  words with the slots, Upload Emoji while one is free (the file dialog,
  then the core reads it), and the two tables, newest first, under their
  headings with the slots left, or No Emoji. The list is rows of 52 px,
  only those in view built and their pictures asked for: the emoji, its
  name (a field this account may change, its ground showing under the
  pointer or while it has the keys, the name going to Discord on Enter, a
  click beside it, once another row's is written, or as another page,
  Upload, Delete or closing the settings is chosen), who uploaded it, and
  Delete under the pointer. Upload Emoji's dialog shows the picture at
  chat's three sizes over the name, its field taking the keys.
- Stickers: Discord's four cards (Free Slots, then Levels 1 to 3, a lock
  and the boosts it takes on one the server hasn't reached) with their
  tiles wrapping 140 px apiece, Edit and Delete in a tile's corner under
  the pointer, and Upload Sticker where the next one goes. The sticker
  form: the preview, Browse for a new one's file, the related emoji, name
  and description, Upload or Update.
- Delete Server and Transfer Ownership (`ownership.rs`): Discord's
  question with the server's (and the new owner's) name in bold. Delete
  Server asks for the server's name without two-factor authentication;
  Transfer Ownership, from a member's menu on Members, shows both owners'
  avatars, a Verified server's warning and the acknowledgement, then the
  code Discord emailed with Resend. A dialog's field takes the keys when it
  shows, and Escape or a click beside it closes it unless Discord is
  answering.

## Channel settings

A Discord channel's or category's settings (`channel_settings.rs`) open
from Edit channel (or Edit
category) in its menu; a category's heading in the sidebar opens its menu
on a right click. They sit on the same settings frame: the channel's name
in capitals (a text channel's with its `#`) over Overview and Permissions,
and Delete Channel (or Category) in red after a line where the channel's
menu offers it, which closes them and asks. What they hold and may change
is the core's (`ChannelSettingsScreen`); the screen keeps only the bar's
shake, whether Add's search has had the keys, and the sidebar's pages and
Delete as they were while Discord was last online (the core names no page
while it's away, and the sidebar stays as it was), takes the keys from the
composer as it opens and gives them back when it closes. The form kit's
`Unsaved` bar saves the page's draft, and leaving the page or the
settings with changes shakes it. Escape closes Add, then the question,
then the settings.

- Overview: the name, for a text or announcement channel the topic, for
  those and a voice channel slowmode stepped through Discord's intervals
  with the arrows either side of it and the Age-Restricted Channel switch,
  and for a voice channel or stage Bitrate (8 kbps up to 96, or 128, 256
  or 384 with the server's boosts), a voice channel's Video Quality (Auto
  or 720p), User Limit and Region Override, each stepped the same way.
- Permissions (the page fills the frame and scrolls its own list): the
  notice of whether a channel's permissions follow its category, with
  Sync Now when they don't; Private Channel (or Category), which takes
  View Channel (and a voice channel's Connect) from @everyone, locked
  with why when this account lacks them; and under Advanced Permissions
  the roles and members with an overwrite, 180 px wide with Add, beside
  the chosen one's permissions in Discord's groups, each a `tri_line`
  (denied, left to the roles, or allowed; locked with Discord's words
  where this account lacks it), then Remove for any but @everyone.
- Over the page: Delete Permission Settings, Sync permissions? and
  Discord's refusal to deny a permission to the account's own roles, and
  Add, its search taking the keys, listing the roles without an overwrite
  that match and the members Discord finds, one chosen added at once.
