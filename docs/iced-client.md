# iced client

`crates/yapfast-iced` is a second desktop front end, written in
[iced](https://iced.rs) 0.14, which is to replace the Slint client once it
does everything Slint's does. Until then Slint's is the default and the
release, and the iced client is built on request. It drives `yapfast-core`
as the Slint client does, through `Action`s, `AppState`, effects and the
backend's events, so drafts, sending, the member list and voice behave the
same. What it shows so far: sign-in (Discord's link by code or token,
Matrix's password sign-in), the window in any theme, the server rail with
its folders and marks, the sidebar (a server's channels under categories
that fold, or the DMs), the account's card, the conversation's header, the
timeline with Discord's grouping, dividers, history edge, jump bars and
Markdown, galleries, stickers, custom emoji, polls, voice messages, apps'
buttons and cards, the hover bar, reactions and their tooltips, selection across messages, the composer (below), editing in
place, the expression picker (GIFs, emoji, stickers), a message's and the composer's
menus, Delete's confirmation, the member list with each member's looks
and its Activity panel, people's menus and their questions, the profile
card, the full profile and the panel beside a DM, the friends list with
Active Now, requests and Add Friend, calls (below) in voice channels and
DMs, Settings (below), Discord's two-factor prompt, and round the window
the tray, notifications, links handed over, its own title bar, the zoom
and updates (below).

## Building and running

The root package's `iced` feature builds the iced client into the
`yapfast` executable in place of Slint's, drawn on the CPU with tiny-skia:

```sh
nix develop path:./nix --command cargo build --profile fast --no-default-features --features iced
target/fast/yapfast --preview
```

With `--features iced,matrix` it carries Matrix too, as the Slint build
does with `matrix`, and signs in with Matrix instead of linking Discord.
`iced-gpu` draws with wgpu instead. The renderer is chosen when it's built:
a build has tiny-skia or wgpu, so `--renderer` only logs a warning when it
asks for the other. `--data-dir`, `--preview`, `--stream-worker`,
`--background`, `--release-discord-links` and a `LINK` work as in the
default build.

`desktop` and `iced` can't be on together, and the build says so: each
brings its own window toolkit and its own copy of the windowing stack, and
`main.rs` starts one front end. A build of both would carry two of
everything for no use.

Under the private test display, `scripts/ui-session start --bin
target/fast/yapfast` runs a built iced binary on the preview, and with
`--discord` against the fake Discord. It reads the
window-control file (`YAPFAST_UI_TEST_CONTROL`) as the Slint client does
(`control.rs`): `stats` logs the allocator's figures, `trim` gives back
free heap, `hide` and `show` hide and show the window, `close` and `quit`
end it, `destroy` drops the window without giving its memory back,
`open` and `create` open it as the tray's Open Yapfast does, `theme ID`
chooses a theme and keeps it, and `stage`, `stage-aside` and `pick` go to
the core as the Slint client sends them.

## The window and the desktop

The client runs as an iced daemon (`lib.rs`, `shell.rs`): it lives without
a window, so closing to the tray drops the window, its renderer and its
widgets, and the tray's Open Yapfast makes a new one at the size it had.
What iced doesn't do, the root crate's shared desktop modules do, the same
copy the Slint client uses: `src/iced_desktop.rs` fills in the iced
crate's `Desktop` (`desktop.rs`) with the tray, notifications, the single
instance, starting at sign-in, Discord's links, the clipboard's pictures,
updates, the window's icon and, on Wayland, files dragged over it. They
tell the window what they hear through a `Tell`, from threads of their
own, which a stream hands to the event loop as messages; nothing polls.
The root package's `platform` feature brings those modules to an iced
build.

- The tray (`shell.rs`): its icon, with the unread dot for a mention or an
  unread DM, and Disconnect in its menu during a call; Open Yapfast, and a
  click on it, brings the window back with the click's activation token.
  Closing the window goes to the tray when Close to tray is on and a tray
  shows the icon, and quits otherwise; with the tray gone, a closed window
  opens again. The window's memory is given back half a second after it
  closes. `--background` starts in the tray with a saved session, as
  starting at sign-in does, unless there's no tray to start in.
- New messages, as the Slint client tells of them: a sound unless it's
  the conversation in front; while Yapfast isn't in front the taskbar
  flashes and a notification shows, as the settings allow. A click on the
  notification opens its conversation, over Settings, and brings the
  window back.
- A later start of Yapfast on the same data folder hands over to the
  running one and ends: the window comes forward with that start's token,
  and a Discord link it brought (a `LINK` on its command line, or a
  `discord://` link the desktop opened) opens once Discord is online and
  nothing is open over the window, as a link clicked in a message does.
  Each start installs the desktop entry and keeps the link handler and
  autostart entry pointing at this executable, outside the preview.
- The title bar (`window_bar.rs`): where the desktop doesn't frame windows
  (Windows, and desktops such as GNOME and KDE that `frame.rs` names),
  Yapfast draws its own, as Slint's does; elsewhere it takes the system's.
  Appearance's "Use the system title bar" switches, and the window is made
  again to take the change. The bar holds Back and Forward through the
  conversations visited, the place's name, the Inbox (Unreads and Mentions
  of the rooms the core knows, a click going there), Help, and minimize,
  maximize and close; its ground moves the window after a 3 px drag and a
  double click maximizes it, and its edges resize it. It goes while the
  window or a video is full screen. The content under it is laid out in
  the room below (`widgets::Below`), so overlays placed by the layout's
  sums or at the pointer needn't know it's there, and `App::window` is
  that room's size. Alt+Left and Alt+Right, and the mouse's back and
  forward buttons, go back and forward too, whichever bar there is.
- Zoom: Ctrl with + or - steps through the shared `zoom.rs` levels, no
  further in than the window's room allows, and Ctrl+0 goes back to 100%;
  the level is kept in `layout.json` and shows a moment at the top. iced
  scales the whole window by it.
- Updates (`settings/updates.rs`): with the check on, a look at the
  releases at start and every six hours, and Check now on About; a newer
  release shows in About with its notes, Update and restart (which
  downloads and checks it, its progress shown and Cancel offered, then
  ends the window so `main` finishes it), View release and Skip this
  version, and as a banner under Settings' sections. A look nobody asked
  for tells of a newer release once in a notification that opens About.

## Theme

`theme.rs` holds the theme every widget paints with: one struct of the
colours of the Slint global `Theme`, taken from the core's palettes
(`yapfast_core::theme`, see [themes](themes.md)) and turned into iced's
colour type once, plus the colours `ui/theme.slint` works out from them
(the menu's ground and outline, the composer's ground and placeholder, a
destructive row's text, the selection, a mention's colour) and iced's own
theme for the few styles iced draws itself. Views take it by reference;
nothing else in the client names a colour, but for Discord's own (the
crown, blurple, role and name colours, the nameplates' two grounds).

It's the theme `layout.json` names, with the person's custom themes from
the same file, as the Slint client keeps them; with none it's Bonfire, the
Slint client's default with its Discord look, which is the only look this
client has. Settings' Appearance chooses another (below), as does the
window-control command `theme ID`: either writes `theme` into
`layout.json`, leaving the rest as it was, and swaps the struct. A switch
costs no picture: the client's icons
and dots are drawn per colour and the ones of the old theme go, but
avatars, emoji and attachments are the same buffers; the names styled for
the other ground (dark or light) are made again as they show.

## Rail and sidebar

The rail (`rail.rs`) is the Slint client's `ServerRail` with its Discord
look: the DM home (the logo; its badge counts DM mentions and friend
requests), the DMs with unread messages, a line, then the servers and
folders, and "Join a server" at the foot, which opens Join a Server. Each tile has its pill at the
rail's edge (open, pointed at, unread), its mentions in a red badge, a
speaker while this account is in a call there, and its name in a tooltip.
A folder opens and closes on a click, its servers on a strip of its
colour; closed it shows its first four servers' icons.

The sidebar (`sidebar.rs`) is the Slint client's `RoomList`: on the DM
home the search field (which keeps the DMs whose names hold its text),
Friends and Requests, "Direct Messages" and each DM with its person's
presence, the voice channel they're in and what they're up to; in a
server its name, which opens the server's menu, then its pinned channels,
then each category's heading and channels. A category folds on a click,
keeping the open channel, unread ones and busy voice channels, as
Discord's does. A channel's row shows its kind's icon, its unread mark at
the sidebar's edge, its mentions, and muted or locked faint; a busy voice
channel lists who is in it and how long the call has gone on, which a
timer brings up to date each minute only while one shows. A right click on
a row opens its menu (`places.rs`), built from the core's
`menu_choices` in the Slint client's wording, at the pointer. A server's
menu mutes it (for a time, in a submenu, or until it's turned back on
with a click on the row itself) or unmutes it, saying until when; its
Notification settings' submenu picks the level and ticks Suppress
@everyone and @here and Suppress all role @mentions; Hide muted channels
and Show all channels are boxes ticked in place. A channel's or category's
mutes it the same way and picks its level (or its category's or server's
default, named under it); the header's bell opens the open channel's
mute and levels. Pin and Unpin, Add to and Remove from the channel list,
Unfollow category, Join and Leave thread, Invite, Create channel and
category, Duplicate and Delete, and Leave server go to the core, those
that ask first or need more opening their dialog (see Dialogs). A menu
taller than the window scrolls, kept inside it. A
DM's menu mutes it for a time and, for its person, offers what their own
menu does, with Invite to server and Apps in submenus.

A server's top is the Slint client's: with a banner, the picture with a
shade over its top and the name and chevron in white on it, the banner
asked for only while the server is open; Invite people beside the name for
those who may invite; under it, while the server shows its Boost progress
bar, the Boost Goal pill (its boosts of 33, filled in pink); and on a
community server or one with events, the "N Events" row, whose menu lists
each event with "Happening now" or when it starts and where, or "No
upcoming events" greyed. An event's row does nothing yet. The server's
verified badge isn't drawn (its icon has two colours).

The account's card (`account.rs`) spans the foot of both: the avatar with
its presence, the name and status (the username under the pointer), and
the mic, headphones and Settings, which mute and deafen in a call; out of
one they set whether the next call starts unmuted or deafened. The carets
beside the mic and headphones open the input and output menus. The call's
dock sits above the card while this account is in a call (below).

In a voice channel someone is in, each person's row shows them speaking
(a green ring), muted or deafened, and LIVE while they stream, which
watches the stream on a click, joining the call first. A click on the
row opens their card at the pointer, and a right click their voice menu.

Open folders, folded categories and Developer Mode (which shows Copy ID)
are kept in the data folder's `layout.json`, which the Slint client keeps
them in too (`layout.rs`); each change writes its one key back.

Browse Channels (`browse_channels.rs`, the Slint client's `BrowseScreen`),
from the top of a Community server's list, takes the conversation's place:
a search of the channels' names, then each category's heading with its
Follow category switch and its channels, each with its icon, name, topic,
View, Open or Join, and a box ticked while the channel list follows it; a
click on the row ticks it or clears it, unless its category follows it.

## Settings

Settings (`settings/`) is the Slint client's `SettingsScreen` with its
Discord look: a card over the window dimmed by 70%, 72 px in from its
edges and at most 1400 px wide, centred; on its left, on the chat's ground, the account with Edit Profile,
a search that keeps the sections whose names hold it, the sections under
Account, Experience and App Settings (Discord and Game Activity only
outside the preview), and which build this is, which a click copies; on
the right the section's title and the close button over its page, a
column at most 696 px wide that starts at its top for each section. The
account card's gear opens it on My Account, a device menu's Voice Settings
on Voice & Video. Escape closes an open dropdown, then stops recording a
key, then Settings; so does a click outside or the close button.

It holds its state only while open (`Settings`, boxed in the app):
closing it lets go of the theme being edited, the camera being tried, the
gate's meter and the recording. Each change is kept at once in
`layout.json` under the Slint client's key for it (`layout::Prefs` and
`layout::Voice` read them) and handed to the core as the Slint client
hands it; the Slint client reads what this one writes, and the other way
round.

- My account (`pages.rs`): the account's card, where its login is kept,
  Reconnect while offline, Edit profile, the custom status, and Sign out
  (Back to sign in in the preview).
- Voice & video (`voice.rs`): the microphone, speakers and camera, each a
  dropdown of the devices the core lists (listed again as the page shows),
  the microphone's and speakers' volumes, which play while dragged and are
  kept when let go, and the camera's preview, filmed only while Test video
  is on (`camera:settings` among the window's video sources, at 400 by
  225); then joining unmuted, evening out the voice, and in Discord calls
  noise suppression and its strength, the soundboard's volume (Discord
  keeps it), the entrance sound for every server or one (Choose, with
  Nitro, lists the sounds Discord has), the room's echo and, on Windows,
  Windows' own effects; the input mode, and either the gate's sensitivity
  over the microphone's level as the gate hears it (read twenty times a
  second only while it shows) or push to talk's key and release delay.
  Record keybind takes the next key pressed, wherever the keys are: its
  scan code as winit gives it (iced's key codes are winit's by name), its
  name and its keysym, as the root crate's `Keybind` keeps it; Escape
  stops without changing it. The root crate's watcher then watches the
  new key, and says under it where it works.
- Messages: a message as the clock and text size show it; the text size
  (which the timeline's text takes), the clock and its seconds, the date's
  order (message times are worked out again for each), when GIFs and emoji
  play, link previews (hidden ones leave "A card, hidden while link
  previews are off." where a card was the whole message), long messages
  folding, boost gems in the member list, sharing that this account types
  (off, the core's typing commands go nowhere), mentioning those replied
  to and tracking taken out of links.
- Notifications, Sounds (the set, which plays a sound when chosen, the
  volume, and each sound's own with its play button) and Advanced, in the
  Slint client's words. Advanced holds Start when you sign in, Open Discord
  links in Yapfast (outside the preview), Close to tray, saying where
  there's no tray, and the renderer's switch, with which renderer this
  build has: it's chosen when the build is.
- Appearance (`appearance.rs`): each theme as a small window in its own
  colours, as many 150 px or wider across as fit; a click shows one and
  keeps it. New theme opens the editor on a copy of the theme shown (Start
  from takes another, and its name unless it was renamed); each custom
  theme's pencil edits it. A colour's field shows its change at once, or
  says "Not a colour"; Save keeps the theme among the person's own and
  chooses it, Cancel (or leaving Appearance or Settings) shows the theme
  from before, and a saved one's Delete asks once more. Then the system
  title bar's switch, where Yapfast can draw its own, and the profile
  switches, which the core is told of.
- Discord: linking, as the sign-in screen's panel, while none is linked;
  then Unlink in a build with Matrix, hidden channels, reads marked on
  Discord, idle while away, blocked people's messages hidden, and those
  blocked, five more at a time, each with Unblock.
- Game Activity: sharing the game played (Discord keeps it), and the games
  named by hand, added, edited and removed, which detection takes at once.
- About: the build with Copy version, the update switches, the releases
  (above), and the links.

## Server settings

A Discord server's settings (`server_settings/`) are the Slint client's
`ServerSettingsScreen`, opened from the server's menu (Server settings): on
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
from what Discord has, the form kit's `unsaved_bar` floats over its foot
with Reset and Save Changes; leaving the page or the settings then shakes
it red instead, as Discord's does. Escape closes an open dropdown, then a
dialog over the page, then the settings.

- Server Profile (`profile.rs`): the name, the icon with Change and
  Remove (the file dialog, then the core reads it), and the description
  with what's left of its 300 characters.
- Engagement: the system messages' channel and its four switches, which
  wait for a channel; the default notifications; the inactive channel and
  its timeout, which waits for one.
- Boost Perks: the server's level, the progress bar's switch, and the
  banner and invite background at 16 by 9, each with the level that
  unlocks it.
- Roles (`roles.rs`, the Slint client's `RolesSection`): @everyone's card,
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
- Members (`people.rs`, the Slint client's `MembersSection`): the search,
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
  the invites with their inviter, code, uses, a countdown ticking each
  second while there are rows, and roles; dimmed while paused, the X under
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
- Emoji (`expressions.rs`, the Slint client's `EmojiSection`): Discord's
  words with the slots, Upload Emoji while one is free (the file dialog,
  then the core reads it), and the two tables, newest first, under their
  headings with the slots left, or No Emoji. The list is rows of 52 px,
  only those in view built and their pictures asked for: the emoji, its
  name (a field this account may change, its ground showing under the
  pointer or while it has the keys, the name going to Discord on Enter, a
  click beside it, or once another row's is written), who uploaded it, and
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

A Discord channel's or category's settings (`channel_settings.rs`) are the
Slint client's `ChannelSettingsScreen`, opened by Edit channel (or Edit
category) in its menu; a category's heading in the sidebar opens its menu
on a right click. They sit on the same settings frame: the channel's name
in capitals (a text channel's with its `#`) over Overview and Permissions,
and Delete Channel (or Category) in red after a line where the channel's
menu offers it, which closes them and asks. What they hold and may change
is the core's (`ChannelSettingsScreen`); the screen keeps only the bar's
shake and whether Add's search has had the keys, takes the keys from the
composer as it opens and gives them back when it closes. The form kit's
`unsaved_bar` saves the page's draft, and leaving the page or the
settings with changes shakes it. Escape closes Add, then the question,
then the settings.

- Overview: the name, and for a text channel the topic, slowmode stepped
  through Discord's intervals with the arrows either side of it, and the
  Age-Restricted Channel switch.
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

## Header and timeline

The header (`header.rs`) is the Slint client's: a DM's avatar and
presence or a channel's icon, a thread's channel (which leads back to it),
the name, a channel's topic on one line after a dot, what a DM's person is
up to; then the room's buttons in Slint's order (calls, threads,
notifications, pins, add people, the member list, the profile) and the
search box.

The header's popovers (`popovers.rs`, the Slint client's) open under
their button, its right edge level with theirs, and close on Escape, a
click outside or what they're for; the core says which is open, and the
window keeps only the rows it built while one shows. Pinned Messages lists
the conversation's pins as found cards (`found::Found`), a click or Jump
showing one, or says why it can't. Threads lists a channel's threads as
Discord's list does: the joined, other active and older ones under their
headings, older ones loading as a scroll nears the end, Enter in its
search finding threads by name, Create starting one, and a thread clicked
opening in its place (with Shift beside the channel). Select Friends
picks friends to add to a DM, saying how many more the group can take,
Create Group DM or Add to Group adding them. A thread open in its place
has the header's More, its menu as the sidebar's.

The search box (`search.rs`, `HeaderSearch`) names where it looks, widens
while it holds something and clears with its X; Enter searches the DM, or
the channel's server. What it found shows beside the conversation in the
member list's place (`SearchPanel`), 420 px wide: how many, the newest
first as found cards, a server's under their channels' names, Show more
for the next page, a click jumping to the message in its room. Close,
Escape or clearing the box ends the search and lets go of the cards.

The timeline (`timeline.rs`, one message's content in `message.rs`) groups
messages as Discord does: the same person within seven minutes, unless
one replies, answers a command or is a notice. A new day's date and the
"New" divider above the first unread message sit on a line across; a
message that mentions this account (or replies to its message, while new)
is tinted marigold with an edge. The first row holds the history's edge:
"Earlier messages" (loading, or failed with a retry), the conversation's
start, or Discord's welcome at a server channel's start. A short history
is held down over the composer, as Slint's is. While the "New" divider is
out of view a bar jumps to it, saying how many messages are new; a window
of older messages has the bar back to the present, and its last row ends
with "Newer messages" (loading, or failed with a retry); a reply's line
jumps to the message it quotes and tints it a moment. Scrolling near the
top asks for earlier history, and near the end of a window of older
messages for newer, until it reaches the present; after a failure, each
waits for its retry.

A message shows the line above it (the reply it quotes, or who used the
command it answers), the author line, its text, `(edited)`, its pictures,
a file's card (its name and size), link previews and bots' cards (a bar of
their colour, site, author, title, description, fields, pictures,
thumbnail and footer), stickers, a poll, a voice message, an app's
components, and reactions; a notice shows its icon in place of the avatar
and its words in Discord's wording.

- Pictures (`media.rs`): one shows at its own size, several as square
  tiles two to a row, scaled down to the room there is. A spoiler's
  picture is blurred under a cover until clicked. Copies at a tile's size
  are made off the window's thread (`pictures::Fitter`) and dropped once
  no view shows them. A picture clicked shows large (`image_viewer.rs`,
  the Slint client's `ImageViewer`): the core's full picture once it
  comes, the preview until then at the size it opened at, fitted beside
  its bars; its name and place among the message's pictures, Save and
  Close above, round buttons and the arrow keys stepping through them, a
  note while the full one loads or why it can't; Escape, Close or a click
  outside closes it, and the core lets go of it. A right click on a
  picture or video, there or in the timeline, offers Copy Image, Save
  Image or Video, Copy Link and Open Link, after the message's own rows.
  Saving, from there or a file's card, asks where with the system's
  dialog, offering only the file's own name.
- A video (`playback.rs`) shows its poster with a play mark; clicked, it
  plays in its cell, its frames coming through the stream worker's shared
  mapping as a watched stream's do (`video::Sources::clip`). While the
  pointer is on it, or it's paused, a bar along its foot plays or pauses
  it, shows its time, moves through it and mutes it; a volume slider rises
  over the mute button under the pointer, and the last button shows it
  full screen in the viewer (`viewer::Viewer`), its name and Exit full
  screen above it and the same bar below, its frames made at the
  viewer's size.
- Stickers show at 160 px, playing if they move (Lottie ones as the core
  renders them).
- Custom emoji are pictures in the text's line, larger when the message is
  only emoji; a copy gives back their `:name:`.
- A poll (`polls.rs`) lists its answers to pick, one or several, then
  Vote; once voted or ended it shows each answer's share with a bar, and
  Show results looks without voting. Its time left counts down.
- A voice message (`playback.rs`) plays and pauses from its button; its
  waveform fills as it plays, and a press or a drag on it moves there
  (pausing while held); its time counts down. While the pointer is on it,
  its speed and mute show. The core plays it.
- An app's components (`cards.rs`): text beside a thumbnail or a button,
  galleries and files, gaps and dividers, rows of buttons and menus, in a
  frame with the colour's bar when they're in a container. A link button
  opens its page; one that asks its app does, and the message's other
  controls wait until the app answers; why one failed shows under them.
  A menu opens its dropdown under it (`bot_select.rs`, over it without
  room): its options with their emoji and descriptions, a search for
  members, roles and channels, a tick by those picked; a menu of one
  option sends the one clicked, one of several what changed when it
  closes, by a press outside or Escape.
- With `collapse_long` in `layout.json`, a long message folds to its first
  lines with Show more.
- Under the pointer a message shows its hover bar: the three reactions
  this device uses most, Add reaction, Edit (one's own) or Reply, Forward,
  and More, which opens the message's menu. Holding Shift swaps the
  reactions for Pin and (one's own) Reply, and More for a Delete that
  doesn't ask, as Discord's client does, unless its menu or picker is
  open.
- A message's menu, on a right click or More, is the Slint client's: the
  four reactions used most, Add reaction, View Reactions, Edit, Reply,
  Forward, Copy text, Copy selection, End Poll Now (asked about in the poll's card), Pin or
  Unpin, Mark Unread, Copy Message Link, Delete and Copy Message ID.
  View Reactions opens who reacted (`reactions.rs`): the reactions down
  the left, the most first, and the people with the one chosen, a page at
  a time as the list nears its end; one's own reaction, or as a moderator
  anyone's, comes off from its row.
  Forward opens Discord's dialog (`forward.rs`): up to five conversations
  the core finds, an optional message, the slowmode warning, Send.
- The keyboard steps through messages (`browse.rs`): Alt+Up from the
  composer lands on the newest, lit with its hover bar; the arrows, Page
  Up and Down, Home and End step; Enter (or the Menu key, or Shift+F10)
  opens its menu, + reacts, R replies, E edits, Delete deletes, C copies,
  F forwards, Alt+Enter marks it unread, and Escape goes back to the
  composer.
- A message from someone this account blocked shows as "Blocked user"
  and "Blocked message." with Show it, as Discord's does, until asked; with
  blocked messages hidden in Settings, the core leaves them out.
- A masked link (words that aren't where they go) asks first, as Slint's
  does: Go back, or Visit site. The text, and its embeds', is laid out once per message (`markdown.rs`):
the core's blocks (headings, subtext, quotes, fenced code) apart, and in
each the inline styles as runs, with mentions on pills of their colour,
links, and spoilers covered until clicked. A web address opens the browser; a
mention or a notice's name opens the person's card. A Discord link this
account can follow stays in the app, as the core's `discord_route` reads
it: an invite opens its Join dialog, a server, channel or message goes
there, and a person's opens their card; an app's authorization, a gift
and an event still open the browser.

## Writing

The composer (`composer.rs`) is the Slint client's: in a message
request's place, its Accept and Ignore; over the field, the reply's bar
with "@ On" or "@ Off" (whether the reply mentions its author) and its
cancel; in a channel this account may not write in, Discord's one bar
saying so; under it, why it can't send (timed out, Discord disconnected)
when it can't. The field writes the core's draft for the open room, which
also tells the room this account is typing; its outline lightens while
it has the keys. Enter sends, Shift+Enter breaks the line, Up in an
empty composer edits the last message of one's own, Ctrl+E opens the
emoji picker, Ctrl+G the GIFs.

Files go with the next message (`uploads.rs`) from the system's file
dialog (the + where nothing else can start, or the + menu's "Upload a
File", whose Create Thread shows greyed until threads come; its Create
Poll opens Create a Poll, below), from Ctrl+V (files copied in a file manager, or a picture, which
the root crate's `paste.rs` reads and saves as `image.<extension>` in a
folder removed as Yapfast closes; text otherwise), or dropped on the
window where winit takes drops (X11 and Windows; Wayland's data device,
which the Slint client keeps itself, is still to come). A drag shows
Discord's card, "Upload to #general", and a drop that can't go whole
says why until Got It. They wait over the text as the Slint client's
tiles: a picture's preview (made off the window's thread at 240 px and
let go with its file) or a file's icon, the name and size, the spoiler
button where the room has spoilers, and the X.

A message of one's own is edited in its row (`edit.rs`), tinted with
Discord's edit tint: the text becomes a field of its own there, with the
emoji button beside it and "escape to cancel • enter to save" under it,
both links. The room's draft and a reply being written stay as they
were. Starting an edit gives the field the keys and scrolls the row into
view; Enter saves (the core asks whether to delete a message emptied),
Escape cancels.

Over the field typed in last (`suggest.rs`), an `@` or `#` word suggests
the people, roles and channels the core finds, and a `:name` word of two
letters or more the emoji whose names hold it, a server's own first
(`yapfast_core::emoji`, which the Slint client uses too). Up and Down
light one, Tab, Enter or a click takes it, Escape closes them until the
word is left.

Apps' commands (`commands.rs`), as the Slint client's `app_commands.slint`:
`/` in a Discord channel opens the list of the commands that fit it over
the composer (or says they're coming, or that none match), each with its
app's icon and name; one taken, or its whole name and a space typed, shows
its options as pills over the text, which writes the focused one, and the
list offers that option's values. Tab and Shift+Tab move between the
pills, a click takes one (a file's opens the file dialog), Enter runs the
command (or asks for a file option's file), Escape closes the list and
then the command, and Backspace in the empty first option puts the
command's name back. What they show and do is the core's
(`application/slash.rs`): the draft carries the query and each option's
text, and `Action::Send` runs the command.

The expression picker (`picker/`), as the Slint client's: in a Discord
room the composer's GIF, sticker and emoji buttons open one panel of three
tabs over them (498 by 486); elsewhere, and for the edit or a reaction,
the emoji alone at the reaction popup's size (408 by 432). The emoji tab
lists the Discord account's Favorites, its Frequently Used (for a
reaction, the emoji it reacts with most), each server's custom emoji
(those this account may not use here greyed with a lock), then Unicode's
categories; the rail jumps between them, a search lists the matches, the
foot names the emoji pointed at, and each emoji's star keeps it among the
favourites as Discord's settings do. The GIFs tab is Discord's over Klipy:
a home of Favorites, Trending GIFs and the trending terms, a search a
quarter second after typing pauses, suggested terms under a search that
found few, the GIFs in a masonry of columns about 200 px wide; a click
sends one, Shift and a click puts it in the composer, its star keeps it,
Escape goes home and then closes. The stickers tab lists Favorites,
Frequently Used, every server's own (the open server's first) and
Discord's packs in 96 px tiles. The tabs share `picker::kit`; each holds
its layout while it shows and lets go of it, and of the pictures it made,
when another shows or the picker closes, and the GIFs' and stickers'
pictures are asked of the core only for the tiles in sight.

Who else is typing shows just above the composer (`typing.rs`): three
dots under the avatars that rise in turn, drawn in their own widget at
twenty frames a second while they show, and "Sam is typing..." where the
messages' text starts. Under a message of one's own that didn't go:
"Sending..." with Cancel while its files upload, "Not sent yet" while
the connection is down, "Not sent" with Retry; under an answer only this
account sees, "Only you can see this · Dismiss message".

Rows are built only near the view at heights worked out ahead; once built,
`Rows` measures them and the rows keep their real heights. While earlier
messages are read, the message at the view's top stays where it is as
rows change height, new messages come, or the oldest go; sending goes
back to the newest message.

## Forums

A Discord forum shows in place of a channel's messages and composer
(`forum.rs`, the Slint client's `ForumPanel`): a search of its posts'
titles and New Post, then its posts as cards (tags, title with NEW, who
wrote the first message and how it starts, the top reaction or the
forum's own, replies, how long ago, the first picture), the active ones
first and older ones a page at a time as the list nears its end. A click
opens a post beside the forum, Shift and a click in its place. New Post
shows the form in the list's place (`NewPostForm`): the title, tags from
a menu of up to five, the forum's post guidelines folding away, and the
first message with files in a `thread::SideComposer`; Post makes it and
opens it beside, and closing with something written asks first. The cards
are built again only when what they show changed, and go as the forum
closes.

## Apps' forms and commands

An app's form (`modal.rs`, the Slint client's `BotModalDialog`) shows over
the dimmed window: its title, then its parts in order (Markdown to read,
a line or a paragraph to write, a select menu with its dropdown, radio
buttons, checkboxes, a checkbox of its own, files to upload with Choose
files and each file's Remove, or a note for a part Yapfast can't fill),
why it failed, and Cancel and Submit ("Submitting…" while Discord
answers). Escape, Cancel or a click outside closes it. What it holds is
the core's; the window keeps only the paragraphs' editors and the text to
read laid out, and lets go of them as it closes.

A message's menu has Apps after Pin message where the room's apps have
message commands: a submenu of them by name with their app's under it,
the chosen one run on the message. The menu asks for the room's command
index as it opens, and is built again with its Apps once the index comes.

## Threads

A thread opens beside its channel (`thread.rs`, the Slint client's
`ThreadPanel`), 420 px wide and never more than the chat leaves: from a
message's thread chip (its name, how many messages, a chevron), View
Thread in its menu, or a listed thread opened with Shift. Its head is the
thread's icon and name, More (the thread's own menu: Leave thread, Copy
link and the rest, as the sidebar's), Open in full view and Close. Its
messages are the channel's timeline's rows numbered from
`timeline::ASIDE`, so a message's look, menu, hover bar (two quick
reactions here), spoilers and selection work the same on either side;
selecting stays on one side. It loads earlier messages as its top comes
into view, tells the core which of its messages show, and its composer
(`thread::SideComposer`) has its own draft, reply bar, files and Send.
Closing it, or the core letting go of it, drops its rows.

Create Thread, in a message's menu or the composer's + menu, opens the
New Thread form beside the channel (the Slint client's
`NewThreadPanel`): the message it starts from as a found card
(`found::Found`), the thread's name (the message's text by default), the
Private Thread switch where the server allows one, and the first
message's composer, Create Thread sending both. Leaving it with something
written asks first (`thread::leave_question`).

## People

A person's row is built from `people.rs`: their face (avatar, decoration,
presence), their name in its style (`NameInk`), and a line under it. The
member list, the DMs, and the lists still to come build on it.

- The member list (`members.rs`) is the Slint client's: role groups with
  their counts, the offline section, each member's decoration, nameplate,
  name style, server tag, role icon, boosting gem, timeout clock and
  activity line; the nameplate and the decoration play under the pointer
  or while their card is open. Over the list, a server's Activity panel
  shows the first three cards of Discord's activity feed (what each person
  plays, listens to or watches, and since when); its eye hides the cards,
  kept in `layout.json`, and its arrow opens the whole feed.
- A DM's row (`sidebar.rs`) shows the person's face, styled name and server
  tag (OFFICIAL on Discord's own), the voice channel they're in or what
  they're up to, a group's members, and its nameplate under the pointer or
  while open; a new DM offers Wave, which sends 👋.
- A right click on a member, a DM or a message's author opens the person's
  menu (`person.rs`), built from the core's `person_choices` in the Slint
  client's order and wording: Profile, Mention, Message, Call, Add Note,
  the friend and block choices, Mute, Timeout, Kick and Ban by name, Pin,
  and Copy ID. Roles, Invite to server, Mute and Apps open submenus built
  as the menu opens: the server's roles this account may give or take
  (ticked where held, each tick sent at once with the menu kept open), the
  servers to invite them to, how long to mute their DM, and the open
  room's user commands with their apps' names, which the menu's opening
  asks the core to fetch. A choice that asks first (remove friend, block, kick, ban,
  timeout and its length, a nickname, a note) opens the person dialog,
  with its options, field and "Don't ask again" as Slint's, and sends the
  answer to the core.
- Profiles (`profile/`): `look.rs` works out what a profile shows once
  each time the core's view of it changes (its theme's inks, the status's
  bubble, activities, the bio laid out, the mutual line, the tabs). The
  card (`body.rs`) is the Slint client's `ProfileCard`: the banner (moving
  ones play), the avatar on its ring with its decoration and presence, the
  status's bubble, Add Friend or Friends and More over the banner, the
  name drawn once in its style, the username, pronouns and server tag,
  badges twelve to a row, the activities with their times, the mutual line
  (which opens the full profile on its tab), the bio cut to three lines
  with View full bio, the roles, and a box that messages them. The full
  profile (`full.rs`) has the card down its left, with Friends, Message and
  More under the name, Member Since, Friends Since, the roles, connections
  and the note (saved on Enter or as it closes), and its tabs on the right:
  Board, Activity (with recent activity), Wishlist, Mutual Friends and
  Mutual Servers, rows the shape of what's coming while Discord hasn't
  said, and Discord's words while a tab is empty. A one-to-one DM has the
  card docked beside it, with what the DM list knows of them now, and
  View Full Profile at its foot; the header's profile button shows or
  hides it, kept in `layout.json`. Activities' times move each second
  while one shows.
- The profile editor (`profile/editor.rs`), the Slint client's
  `ProfileEditorDialog`, opens from one's own card and full profile (Edit
  Profile), from Settings over it, and from a server's Edit server
  profile. Its card is a profile view of what's written over what Discord
  has, as the Slint client's adapter makes it, drawn with the card's own
  parts and made again only when the core's editor changes; a picture
  just chosen shows from the core's preview of it. Each text is edited in
  place: the name and pronouns turn into fields when clicked, the bio into
  a field of its own (Shift+Enter breaks a line) with what's left of
  Discord's 190 characters; the avatar and banner offer Change and Remove
  (Reset in a server) once there's one to remove, else open the file
  dialog, and without Nitro the main banner opens its colour, with
  swatches and "Not a colour". The main profile has the server tag's
  picker under the name, Message, the shop and More (Copy User ID), the
  custom status's thought (which opens its dialog), Connections and the
  note; down its left what it wears (`Main Profile`), which Hide Styles
  folds away, with the avatar and banner tiles, the theme's two colours
  (Nitro) as handles that open their codes, and the shop's for the rest;
  and its Board, Activity and Wishlist tabs where the card is 880 px or
  wider. A server's has what it edits over its banner and Nitro's offer
  over what takes it. Once something changed the form kit's
  `unsaved_bar` offers Reset and Save Changes, which waits while a text is
  too long or a picture is read. Escape ends the editing, closes what's
  open, then the editor.
- The friends list (`friends.rs`), from Friends on the DM home: the
  header's Online and All tabs and Add Friend, a search, and each friend
  with their presence, voice channel and status, ruled apart, with a
  message button; a click opens their DM, a right click their menu. Only
  the rows in view are built. With the conversation at least 1000 px wide,
  Active Now (`active_now.rs`) stands beside it: friends together in a
  voice channel, in one game party, or each doing something Discord can
  name, a card each, made as Discord's `NowPlayingViewStore` makes them,
  only while the list shows; a card's head opens its person's menu, a
  voice channel's row the channel.
- Requests: friend requests and message requests, a person's on one row
  with Accept and Ignore (or Cancel for those sent) and their latest
  message, which opens the conversation; then those sent, and spam folded
  away. The Requests row counts a person's two requests once.
- Add Friend's dialog sends a request by username and says how it went.
- Menus opened by a button rather than a right click open where it was
  pressed: the window notes each press's point (`RightPress::spot`).
- Escape closes a dialog whose field has the keys (Add Friend, a person's
  question, Forward), as the field alone would only let go of them.

## Sign-in

Without a saved account the window opens on the sign-in screen
(`signin.rs`), as the Slint client's `ui/login.slint` lays it out: on the
left the network's sign-in, on the right "Just looking?" with the preview.
It starts on a saved account when the core can restore one (a Discord
login without Matrix, Matrix's `active.json` with it), so no notice shows
for one it can't. While that account is restored the window shows only
the core's notice ("Connecting to Discord..."), where the Slint client
shows the card: the card would show for a moment at each start, and its
text, at sizes the workspace doesn't use, would stay in the renderer's
glyph cache. The card shows if the restore fails.

- Discord, in a build without Matrix: "Show the code" asks the core to link
  (`Action::LinkDiscord`); the client runs `discord_link` on the runtime and
  hands each step to the core, and the screen shows the QR code (drawn once
  per code, whole pixels per module), who scanned it, the captcha's notice
  with "Open it again", or the error with "Try again". "Use a token" opens
  the field instead; Link checks it with Discord. A linked account starts
  the workspace.
- Matrix, with `matrix`: the homeserver, Matrix ID and password go to the
  core's `Login`; the password is let go once it's sent. "Retry saved
  session" shows when there's one to retry. Until this device is verified,
  a banner over the timeline takes a recovery key ("Recover") or checks
  again after verifying from another device.
- Discord's two-factor prompt (`mfa.rs`), the Slint client's `MfaDialog`:
  for a request Discord holds back, with an authenticator's code, a text,
  the password or a backup code, the warning before a backup code and its
  ten seconds (one timer, only while the warning shows), and "Verify with
  something else".

Pages the core asks to open (the captcha) open in the browser
(`browser.rs`), never from tests or the private test display.

## Calls

The call (`call/`) is the Slint client's: `ui/voice.slint` and the call
parts of `ui/yapfast.slint`, with the adapter's logic beside them.

- A voice channel this account isn't in shows its name, who is there and
  Join voice (Switch voice channel while in another call), or why it can't
  be joined. A click on the channel in the sidebar joins it without
  showing it, as Discord's client does; a second click, or the dock's
  channel line, shows the call.
- The stage (`call/stage.rs`, ported from the Slint client's tile layout)
  lays the tiles out in a grid, or one large with the rest in a row under
  it; a click on a tile makes it large, and a click beside it the grid
  again. The tiles scroll when there are more than fit. Each tile is a
  `tile::Tile`: a camera, a stream or the avatar, a green ring while the
  person speaks, their name and marks, LIVE and how many watch. A stream
  not watched offers Watch stream; a watched one shows large with Stop
  watching and the full-screen button under the pointer, and a double click
  goes full screen too, the stage over the whole window until Escape. A
  right click opens the person's voice menu.
- The controls under the stage, in the Slint client's order: mute and
  deafen (each with its device menu), the camera, Go Live, the soundboard,
  and leave.
- The device menus (`call/popups.rs`): the device to use (one chosen that
  went away shows as not connected), its volume, which plays while it's
  dragged and is kept in `layout.json` when let go, and on the input side
  the microphone's level, measured only while the menu shows, and the
  noise suppression kind.
- Go Live's menu lists the screens with a picture of each (from a stream
  worker started for them and gone once they're in), the windows, the
  system's own dialog where the portal picks, and the cameras, then the
  resolution, frame rate and encoding; it fits a short window by
  scrolling, keeping its button in view. While live, the same menu
  switches what's shared.
- The soundboard lists the server's sounds and the defaults in rows of
  three, each played on a click, previewed from its speaker under the
  pointer and starred as a favourite.
- Someone's voice menu (`menus.rs` with the person's choices) has their
  user and stream volumes above its rows and checkboxes for muting them
  for this account, muting their soundboard, and on Discord the server's
  mute and deafen; one's own has mute and deafen. Move to opens the voice
  channels they may be moved to (this account moves by joining). Volumes
  play while dragged and are kept when let go, as are those muted.
- The dock (`call/dock.rs`) at the sidebar's foot: the connection's state
  and the call's time, the channel line (underlined under the pointer)
  that shows the call, noise suppression's toggle, which turns back on the
  kind last used, leave, and while sharing its state with a stop; the
  soundboard, camera and Go Live buttons; an error with Dismiss.
- A DM's call (`call/dm.rs`) shows above its messages, taking more room
  while a tile is large: the stage and controls while in it, else who is
  in it with Join the call, or Accept and Decline while it rings. A call
  ringing in a DM that isn't open shows a card at the window's top, and
  switching calls asks first (Escape cancels, Enter confirms). The DM's
  header starts a call or a video call.
- While the call isn't on screen, the stream watched, this account's
  screen or a camera stays in a corner of the window (`call/corner.rs`),
  its title and stop under the pointer; a click returns to the call, a
  drag moves it to the nearest corner.
- Sounds play as the core says (joining, leaving, muting, someone
  streaming), and a DM's call rings while it's incoming.
- Push to talk (the root `src/push_to_talk.rs`) runs in this build too: the
  key kept in `layout.json` is watched through the desktop portal's global
  shortcuts or X11, beside the window.

Only what shows takes frames: a tile's video is mapped while its tile is
in view or in the corner, at the size it shows.

## Dialogs

Over the dimmed window, in the frame `dialogs::dialog` draws, each as the
Slint client's in `ui/dialogs.slint`; Escape or a click outside closes
each, and what each lists or makes is the core's.

- The account's popover (`status.rs`, the Slint client's
  `AccountPopover`), from the account card's picture or name: the account
  over its banner colour or theme, as Discord's own card shows it (the
  core's `account_profile`, asked for as it opens), the custom status,
  Edit Profile, the Discord status (its row shows the four to choose),
  Set or Change custom status, Copy User ID and User Settings.
- Set a custom status (`status.rs`): an emoji from the picker (opened over
  the dialog, beside its button; its badge takes it off), the words, and
  Clear After; it starts from the status set, and Save, Clear status and
  Cancel go to the core, which says while it saves and why it couldn't.
- The quick switcher (`switcher.rs`), from Ctrl+K wherever the keys are:
  the core's `switcher_places` for what's typed (PREVIOUS CHANNELS while
  nothing is), each with its picture or channel icon, where it is and its
  mentions; Up, Down and Tab move to the place Enter goes to.
- New message, from the DM list's pencil: the people the core finds for
  what's typed, each with Message, and what the search came to.
- Create a Poll, from the composer's + menu: the question with its count,
  the answers each with an emoji from the picker and a bin, Add another
  answer, the duration, Allow Multiple Answers, what's too long, and Post.

The dialogs a server's and a channel's menus lead to are
`server_dialogs.rs`'s, as the Slint client's `InviteDialog`, `JoinDialog`
and `ChannelDialog` (which asks before leaving too) in `ui/server.slint`.
An opening one's field takes the keys, else nothing under it does, and
Escape closes it.

- Invite: "Invite friends to" the server and the channel the link lands
  people in, the link with Copy (Copied once copied), how long it lasts
  and how often it works, why it couldn't be made, and Expire after and
  Max number of uses, starting from the link's, for Generate a new link.
- Join a Server, from the rail's button or a clicked invite link: the
  field (Join's alone; Enter finds the invite), the server once found
  with its icon and who is online, how joining goes (looking up, joining,
  already in it, a captcha or a form to finish in the browser), and Find
  Invite, Join Server or Open Server; Open the captcha again and Open in
  browser go through the core to `browser.rs`.
- Create channel (Text or Voice) and Create category, in the server or
  the category the menu was for, Duplicate channel with its name to change,
  and Delete channel or category in red, which Delete Channel in the
  channel's settings opens too; Enter does what the button does, and the
  core says while it saves and why it couldn't.
- Leave a server or group, in red: a group's offers to leave without
  notifying its other members; Enter leaves.

## How it's built

iced builds the whole view again after every message and lays it out, so
what costs is kept out of the view:

- The backend's events come on its receiver, batched, as messages
  (`app.rs`); nothing polls. Fonts the shaper wants, the control file and
  the window's size as it opens and is resized come as subscriptions.
- Lists are virtualized by hand: `widgets::Rows` holds only the rows near
  its viewport, each keyed by its id, so its widget state follows it as
  rows come and go. A message's text is laid out once, when it comes, and
  kept with its row.
- Pictures are the core's own buffers in tiny-skia's format, premultiplied
  BGRA (`pictures.rs`, `chat::set_pixel_buffers`), handed to iced without a
  copy. Copies at other sizes are made once and go when the core lets go of
  the picture (`Converted`).
- Moving pictures (`widgets::Animated`) advance inside their widget on the
  window's redraws, all on one clock, so a frame costs no message and no
  rebuild of the view; decoders of pictures no longer shown go after each
  view.
- Fonts: DM Sans and its italic, the packed Twemoji (the root `build.rs` makes it for this
  build too), the display-name fonts on first use, and for other scripts a
  system font loaded the first time a script is shaped (`fonts.rs`).
- Video (`video.rs`, `call/`): cameras' frames are made in the client's
  own memory (`picture::set_frame_buffers`) at their tile's size, and a
  watched stream's frames come from the stream worker's shared mapping.
  The renderer's hook (patch 13 below) copies or scales them into the
  window with libyuv. The core wakes the client for each frame made
  (`picture::set_frame_wake`): a camera's as it's decoded, and a stream
  worker's as the worker says so on its reports' pipe, a line per frame it
  writes only for a client that asked to be woken. Nothing looks for
  frames on a timer.

Each part of the window has its module: `signin`, `mfa`, `rail`,
`sidebar`, `account`, `header`, `timeline`, `message`, `cards`, `bot_select`, `browse`, `browse_channels`, `found`, `popovers`, `search`, `thread`, `image_viewer`, `composer`, `edit`, `suggest`, `commands`, `typing`, `uploads`, `forward`, `members`,
`people`, `person`, `profile` (with `editor`), `friends`, `active_now`, `reactions`, `picker` (with its tabs `emoji`, `gifs`, `stickers`), `menus`, `places`, `dialogs`, `call` (with `stage`, `dock`, `popups`, `dm`, `corner`), `settings` (with `pages`, `voice`, `appearance`, `updates`), `shell` and `window_bar` (with `desktop`, the desktop's parts), `server_settings` (with `profile`, `roles`, `people`, `moderation`, `expressions`, `ownership`), `channel_settings`, `server_dialogs`, `status`, `switcher`; `form` and `sheet` are the settings' kit; `theme` holds
the colours and `layout` what's kept in `layout.json`.
`sync.rs` takes in what changed in the core's state after each message.
`widgets.rs`, `field.rs` (the composer's text field), `select.rs`
(selectable text) and `video.rs` are the client's own widgets. Overlays
(the card, suggestions, the picker, menus, a dialog) are layers of one
stack, each over a layer that takes a click outside; a menu or a dialog
takes the keys by moving focus to an id nothing has (`app::NOBODY`).

## The kit

Screens build from shared pieces rather than styling their own, and take
every colour from the theme and every repeated size from a named constant.
When a second screen needs what a first one built, it moves here:

- `controls::action`: the button of dialogs and forms (the Slint client's
  `ActionButton`), by kind: primary, plain, ghost, danger.
- `controls::field`: the single-line field (`TextField`).
- `controls::lit_style`: a button with no face, or a fixed one, lit under
  the pointer: icon buttons, list rows; `controls::ink_style` a button that
  is only its text, brightening under the pointer.
- `App::menu_item` with `controls::Item`: a row of a menu or a list of
  choices (`MenuItem`), lit under the pointer or, in a context menu, where
  the keys put the light; `controls::menu_panel` and `separator` are the
  menu's ground and the line between its groups.
- Submenus (`menus.rs`): a `MenuRow` with a `submenu` shows a chevron and
  opens it beside itself, its first row level with the row, under the
  pointer, on a click or with Right; on the left when the right has no
  room, and kept in the window. Left or Escape closes it, the pointer onto
  another row too. A row's `click` is what a click on the row itself does
  instead (a mute's "until I turn it back on"); a row's `key` carries what
  it's about (a role, a server, a command, a channel) for `sub_act`. A
  `Check::Box` row ticks in place and keeps the menu open, in the menu or
  a submenu; a `Check::Current` row's tick makes every row of its menu
  leave room for one. A menu or submenu is as wide as its widest row, at
  least its own width, measured once as it first shows. `Place::beside`
  lays the submenu out next to a row found by its path in the menu's
  layout, so the menu never moves.
- `controls::outside`: the full-window layer under an overlay that takes
  a click outside it, clear, or dimmed under a dialog.
- `widgets::Below`: its child laid out, drawn and pointed at in the room
  under the window's own title bar as if that were the whole window, and
  told to operations as a scrollable that can't scroll, so what they find
  is where it shows.
- `menus.rs`: a menu taller than the window scrolls, at most the window's
  height less a margin each side, and `Place` keeps one below the pointer,
  above it, or else pinned to the window's foot.
- `dialogs::dialog`: the frame of every dialog over the dimmed window;
  `dialogs::panel` a dialog of a width (and height) of its own on the
  chat's ground, as Roles' questions and Add members are, a click on the
  dimmed window sending its own message.
- `reorder::Reorder`: a list of rows that dragging reorders: a row taken
  by the strip at its left (its handle, for those that may move) follows
  the pointer, lifted on a ground of its own, and where it's let go,
  rounded to whole rows, is sent as from and to for its owner to move.
- `App::filled_icon_button`: an icon button on a round ground of its own
  (the Slint client's `IconButton` with a `fill`), greyed without a
  message, its tip above; an empty label shows no tip.
  `App::danger_icon_button` is one with `danger`: red on a red wash under
  the pointer, on a `fill` or none.
- `elide::elide`: one line of text cut to its room with "…", as Slint's
  `overflow: elide` (iced's text only wraps or clips): as wide as its text
  up to the room its row leaves, so what follows it follows the text, or
  `centred` across the room. Names in the lists, channel rows and table
  cells use it.
- `controls::tip` and `tip_style`: the tooltip.
- `controls::bar`: a box of one colour, for lines between parts of the
  window, bars and dots; `controls::placed` puts a layer of a stack at a
  point.
- `controls::count_pill`: a count in a red pill, on a ring or not, with
  the metrics of the rail's or a row's (`Count`).
- `looks::Status` and `App::status_line`: what someone is up to, on one
  line under their name (the member list, DMs, the account's card).
- `App::glyph`: one of `ui/icons` at a size, in a colour; `Pic::boxed`
  centres it (or any picture) in a square, for a button's face or a badge;
  `App::icon_button` is the header's and composer's icon button.
- `App::avatar` and `App::face`: an avatar, with its decoration and
  presence dot.
- `people::Person` and `App::person`: a person's row, their face, then
  their name over its line; `App::person_name` draws a name in its style
  (`NameInk`), `App::voice_spot` the voice channel someone is in, and
  `App::list_heading` a list's heading.
- `App::open_person_menu` and `person::choose_person`: a person's menu
  and what its choices do, wherever the person shows.
- `members::nameplate`: a nameplate behind a row.
- `App::status_text`: an emoji and words on one line at a size of their
  own.
- `markdown.rs`: text laid out as Discord's client does (`parse`,
  `plain`, `notice`), and drawn (`App::body_view`, `App::run_span`) as
  selectable pieces (`select::Piece`), which follow their links.
- `style.rs`: container grounds (`fill`, `rounded`, `corners`), the two
  shadows, the scrollbar.
- `widgets.rs`: `Pic`, `Animated`, the virtualized `Rows` (rows across
  the list, or with `Rows::place` boxes of their own), the anchored
  overlays `Place` and `Above`, `PlateRow`, `Watch` and `RightPress`.
  `Rows::measured` lays rows out at their content's height and hands the
  heights that differ to the app with no message, apart from those below
  the view of someone reading earlier messages. `Corner` holds a bar at a
  row's top right over its edge, `Fold` cuts a long child with a link
  under it, and `Scrub` says where along it it's pressed and dragged.
- `media.rs`: `Shot`, a picture in a box (its own size, cut to cover,
  fitted within, blurred under a cover), and `Gallery`, a message's
  pictures laid out.
- `popover::Popover`: a widget with something that opens from it, above
  or below, in iced's overlay layer, placed by the widget's own bounds
  and kept in the window: suggestions over a field, a dropdown under its
  button. A press outside both can close it.
- `picker::kit`: what a picker is built of: the tabs, the search field,
  the rail of sections, `Grid` (tiles placed at their own boxes, which
  `Rows::place` builds only near the viewport, with the sections the rail
  jumps to), a section's heading, `App::star` (a favourite's star),
  `pinned` (a small thing at a tile's corner), the foot and the frame,
  and `Masonry`, the GIFs' columns.
- `tile::Tile`: a video tile, the one piece of the stage, the corner
  picture and a DM's call: its face (a frame, an avatar or a note), a ring,
  chips at its corners, `glass_button`s over the video and `pill`s.
- `viewer::Viewer`: something shown large over the whole window on a
  ground (a title bar with its buttons and a `subtitle`, the content
  fitted between, a foot under it, layers `over` it), for the stage full
  screen, a message's video full screen and the picture viewer.
- `controls::round_button`, `call_button` and `dock_button`: the call's
  round controls, the DM call's answer buttons and the dock's buttons;
  `controls::live_pill` LIVE; `controls::Slider` a volume slider;
  `controls::heading` a popup's heading; `controls::choices` a row of
  choices, one picked; `controls::Check` and `App::checkbox` a menu row's
  tick or checkbox, and `Item::subtitle` a second line under its label.
- `widgets::Hover`: a layer shown over its child only under the pointer.
- `found::Found` and `App::found_view`: a message found (pinned, a
  search's result, a thread's start) as a card: avatar, name in its role
  colour, when, eight lines of text, its first pictures, its poll and
  files; a click or Jump under the pointer shows it.
- `App::popover_frame`: a popover's frame under a header button (the
  Slint client's `Popover`): icon, heading, detail, buttons at its end,
  Close. `App::search_field`: a search box (`HeaderSearch`).
- `thread::SideComposer`: a composer beside the chat (the thread's, a new
  thread's or post's first message): its field, reply bar, files, attach
  and send. `thread::leave_question` asks before a form with changes
  closes.
- `form.rs`, the form kit Settings is built of and the editors to come
  will be: `title` (a page's), `heading` (a group's), `note` (what a
  control does, under it), `rule`, `group` and `page` (the spacing of
  each), `buttons`, `beside` (a stretched thing and one at its end) and
  `halves` (two equal columns); `Switch` (a label, what it does, the
  `toggle` at the row's end, the whole row a click; without a message
  greyed, with a lock saying why), `Dropdown` (the chosen option in a box,
  its options in a list under it that its owner opens and closes),
  `readout` (a value set elsewhere in a sunken box), `capture` (a key's
  box, "Press a key..." while recording), `field` (a form's text field),
  `entry` (the Slint client's 40 px `TextField`), `area` (a text several
  lines high with what's left of it),
  `colour_row` (a swatch, its name and its hex) and `card`. Rows of choices
  are `controls::choices` (`choices_within` for a wider row), sliders
  `controls::Slider` with `step`, `shown` (the value in other words) and
  `mark`. `unsaved_bar` is an editor's bar once something changed, which
  `Shake` shakes red after leaving was refused; `Dropdown::enabled`
  greys a dropdown and `Dropdown::height` sets its box's. `options` is a
  list of radio choices (`Choice`: a title in its own colour, what it
  means, greyed when not offered), one chosen. `tri_state` is Discord's
  three-way choice of a permission (`Tri`: deny, pass to the roles,
  allow; the chosen one filled red, grey or green), and `tri_line` a
  permission's row of it: the title (dimmed, with a lock saying why, when
  it can't change), the choice at its end, what it means, and a line.
- `server_settings::moderation::pager`: Previous, "Page 2 of 3" (or "2 of
  3") and Next, for Bans' and Members' footers.
- `sheet.rs`: the settings frame of a server's and a channel's settings
  (`Sheet`: the pages' column of `Nav` rows, the page, the close button,
  the bar over the page's foot, a page that fills its height for a list
  of its own); `backdrop`, the card over the dimmed window that Settings
  shares, and `nav_row`, a section's row in either.
- `App::emoji_button`: an emoji on a lit square, for quick reactions in
  menus and the hover bar. `controls::action(..).small()` and `.icon()`
  make an app's button; `Kind::Success` is the green one.

## The vendored toolkit

`vendor/iced_tiny_skia` and `vendor/iced_wgpu` are iced 0.14's renderers
with Yapfast's changes, patched in through `[patch.crates-io]`; the crate
pins iced to 0.14.0 to match. Their `README.yapfast.md` files list the
changes, and each is marked `Yapfast port` in the source. The tiny-skia
ones draw pictures from the app's own pixels, copy pictures and glyphs onto
the frame directly, clip by rectangles instead of masks, merge damage,
draw video frames through the app's hook, keep each saved frame's
background with it so a theme switch repaints regions an older frame
painted in the old colour, count a shadow in its quad's damage so a
shrinking overlay leaves no shadow behind, and keep one scaler context for
every glyph, so fonts' hinting is set up once rather than for each text
drawn; the wgpu ones fix two shader
mistakes and upload large pictures in the frame.

## Tests

`cargo test -p yapfast-iced` runs the crate's tests, among them `tests/`,
which drives the window through `iced_test`'s headless simulator on the
core's preview: it builds the view, clicks and types into it, and hands the
messages that come out to `App::update`. They find widgets by their text or
their id, as iced has no accessible labels yet; the client's own text
widgets report their text for this. The tasks `update` returns (focus, the
clipboard, timers) don't run in them, and the simulator keeps widget state
only while it lives, so a field is clicked in the same simulator it's typed
into. They cover the theme's switch and what it keeps, the sign-in screen
on a data folder without an account, linking's steps and the two-factor
prompt, the rail's unread DMs and folders, categories folding, unread
marks and the DMs' search, switching channels with their headers and
topics, the history's edge as earlier messages load, a window of older
messages ending with newer ones loading or failed, the unread bar's
jump, the view staying put on earlier messages as new ones come, and
sending from there going to the newest; and in `tests/writing.rs` the
composer's line breaks, the reply's mention toggle and Escape, editing in
place, `:name` suggestions, the emoji picker's search, Forward from a
message's menu, and the keyboard's steps through messages; and in
`tests/menus.rs` a server's notification choices and what each sends,
the bell's and a channel's alike, a submenu beside its row (on the left
without room) that the pointer and keys open and close, roles ticked in
place, the servers to invite someone to, Delete channel, Invite and
Leave server asking first, a server's boost goal and events row with the
events' menu (and "No upcoming events" without any), and the window's size
following a resize; and in `tests/people.rs` a profile's More menu being the person's own and asking
before a block, the profile beside a DM and the header's button, the
friends list's tabs and search, requests listing someone once, a blocked
person's message folded until asked, View Reactions, and the full
profile's tabs, labels and theme inks; and in `tests/calls.rs` the dock's
noise toggle, a device gone away, the device volumes played and kept,
the share menu in a short window, the input level only while its menu
shows, the dock's channel line, a voice channel's Join, and someone's
volume from their menu; and in `tests/settings.rs` a theme chosen in
Appearance shown at once and kept, a custom theme made, edited, cancelled
and deleted, preferences kept across a restart (and message times
following the clock), a device chosen in Voice & video, push to talk's key
recorded and kept, and closing Settings letting go of an edited theme;
in `tests/server.rs`, on a Discord server `discord_server` builds, server
settings opening from the server's menu and holding their changes until
saved, Roles locking what ranks above this account and the permissions it
lacks, a role dragged by its handle moving, Safety Setup's levels and
locked 2FA switch with Bans and a ban's dialog, Audit Log's rows opening
their numbered lines and a filter fetching anew, and Members (columns,
the search settling, a profile and Escape over it, an order) with Invites
(countdowns and Security Actions), Emoji's tables (only this account's own
a field, renamed on Enter, deleted) and Stickers' cards and form, and
Delete Server asking for the name;
in `tests/channel_settings.rs`, on the same server, channel settings
opening from the channel's menu, Overview holding its draft until Reset
(another page waiting for it), Permissions' @everyone with its locks,
and Delete Channel closing them to ask;
in `tests/editor.rs` the profile editor's card and bar (Escape ending the
editing, Reset putting it back), a bio too long that can't be saved, and
a banner colour that isn't one; in `tests/dialogs.rs` the custom status
starting from the one set and saving what's written, and the quick
switcher finding a place and going there with the keys; in
`tests/header.rs` pins under their button closing as one is jumped to or
on Escape, a server search's results beside the channel ending on Escape,
and Select Friends counting the room left; in
`forum.rs` a post's card; in
`tests/threads.rs` the thread beside writing its own draft and letting go
of its rows as it closes, and the viewer letting go of its picture; in
`tests/server_dialogs.rs`, on the same server, the invite dialog's link,
how long it lasts and its choices for a new one, Create channel and
category, Duplicate and Delete from the menus, leaving a server or group
in red (a group's without telling anyone), and Join a Server from the
rail and a clicked invite link, and Browse Channels opening from a
Community server's list and searching; in `tests/shell.rs` closing to the
tray and back, quitting without one, the tray's dot, a notification's
click and the release's, a later start's link, the own title bar's place
and inbox, clicks under it landing, Back and Forward, and the zoom. `call/stage.rs` tests the tiles' layout, the
stage's slots and the corner, `call/dock.rs` the call's time and
`viewer.rs` the fit. The emoji
ones need the Twemoji font the root crate's `build.rs` packs: they take
the newest any build left under `target/`, and say so and pass without
it. `markdown.rs` has its own for Discord's Markdown. `scripts/check` lints both renderers' builds, runs the tests, and
builds and smoke-tests the CPU build; see [building](building.md#checks).

## Not there yet

In Settings: the NEW tags on settings not yet seen, the entrance sound's
soundboard (it's a list), and idle while away, which isn't sent yet.
In the profile editor: Display Name Style's window (its tile does
nothing), the nameplate's and effect's art on their tiles, the theme
colours' picker (each is its code alone), a moving banner, and the avatar's
colour behind a banner without one, which the profile cards lack too.
Other accounts, profile effects and frames, a mutual server's card, an
activity's menu, accessibility and Windows' own checks. Zooming with
Ctrl and the wheel (iced's lists would scroll with it too), and the
inbox's avatars tinted by the room's id rather than its name, as Slint's
are. On the rail
and in the sidebar: dragging servers, channels and people, the voice
card beside a busy channel and a voice channel's chat, a server's
verified badge and its events' own cards, and the dialog that asks whether a
dragged channel takes its category's permissions (there's no dragging
yet). An app's authorization, a gift's and an event's dialogs: their
links open the browser. In the header:
a channel's topic in full on a click. Dragging the thread beside's edge to widen it. In the timeline: keys typed in the timeline going to the composer. In the composer: dragging the expression
picker's left edge to widen it, and Nitro's offer on a locked emoji or
sticker. In calls: push to talk's fallback while the
window has focus where no global shortcut is had, the ringing card's
pulse, and the video over the switch dialog's dim (the renderer draws
video after the layers above it).
