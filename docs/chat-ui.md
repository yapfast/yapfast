# Chat UI

The chat window for Matrix and Discord, laid out after Discord's client.
Matrix messages are sent through the SDK's Markdown feature, and Slint's
Markdown draws formatted text. `pulldown-cmark` in `yapfast-core` reads
Markdown for links, spoilers, reply snippets and notification text
(`links.rs`, `spoiler.rs`, `chat.rs`). `image` decodes pictures within bounds,
and `rfd` opens native file dialogs.

## Try it

From the repository folder:

```sh
nix develop path:./nix --command cargo run --locked --profile fast -- --preview
```

The preview has two sample servers, DMs and a group DM. The upper-left button
opens DMs; buttons beneath it select servers. The sidebar shows only that server's
channels, or only DMs when the DM button is selected. Filter the current list and
send messages locally. The gear in the account panel opens settings, where "Back
to sign in" opens the real account form. Samples and entered messages disappear on leaving
the preview. They never connect to Matrix.

Launch without `--preview` for password sign-in and automatic restoration.
`--data-dir PATH` selects an isolated session directory. Passwords and recovery
keys are masked and cleared after submission. The sign-in form scrolls when
errors make it taller than the minimum window. SSO and OAuth are still pending.

## Implemented behavior

- Show existing DMs behind the upper-left button and joined Matrix Spaces as
  servers below it. Scope the channel list to the selected server, with a name
  filter that resets when switching servers or DMs.
- Flatten channels in joined subspaces, deduplicate shared links, and tolerate
  cyclic Space relationships. A shared channel can appear in multiple servers.
  Removed child links update the channel list during sync.
- Ctrl+K opens the quick switcher, "Where would you like to go?", as in
  Discord, wherever the focus is, but not over the settings or a dialog;
  Ctrl+K again, Escape or a click outside closes it, and the composer has
  the focus again. A keyboard layout without Latin letters uses the key in
  K's place. Before a search it lists the last eight conversations opened
  this session, of either network, then the Discord places this account
  uses most, fifteen at most and not the one open. A search lists up to 50
  conversations and servers, ranked as Forward To ranks them
  ([Forwarding](discord/messages.md#forwarding)), and those as good by name;
  `@` finds people, `#` text channels, `!` voice channels and `*` servers.
  Channels this account may not read stay out, and so do message requests.
  The arrow keys or Tab choose, and Enter or a click goes there: a server
  opens on its channel shown last. Unread places show bold, and mentions as
  a count.
- Restore each server's last selected channel and the last selected DM. Yapfast
  keeps each server's channel in `layout.json`, so after a restart a server
  opens its last channel; a server opened for the first time shows its top one.
- List the joined members of the open text channel or group DM on the right,
  grouped under ADMINS, MODERATORS and MEMBERS by Matrix power level (100 and
  50), each by name. The first view fetches the full list from the homeserver,
  since sync loads members lazily, and membership changes refresh it. Voice
  channels and one-to-one DMs have no member list; a one-to-one DM shows the
  other person's profile there instead, which the person icon in its header
  hides or shows. A Discord text channel
  lists whoever can read it, grouped as in Discord: under hoisted roles, then
  ONLINE and OFFLINE, with counts, avatars, status dots, role colours,
  custom statuses, and a red clock beside someone timed out. Yapfast subscribes to the list's
  first 100 rows while the channel is open, and Discord keeps them current.
- Read live text messages. Earlier history loads once the timeline has
  stayed within a screen of its top for 50 ms, a page at a time (30 events
  for Matrix, 50 messages for Discord), until the conversation's first
  message, which "This is the start of #general." marks. A room opening
  shows its list at the top for a moment, before it moves to the newest
  message or the "New" divider, and that moment loads nothing. While a page is on its way a pill above
  the oldest message says "Loading earlier messages..."; if it fails, the pill
  offers Retry, and pages load again only when asked. The messages shown
  stay where they are on screen as older ones go in above them, even when
  the top of the history is on screen; see
  [building](building.md#dependencies).
- Jump to a message from a reply's quote, a pin or a search result. One
  that's loaded scrolls a third of the way down and is highlighted for a
  moment. Otherwise the messages around it load in place of the timeline,
  as a window of older messages: Discord's 50 around it, Matrix's 30 around
  it through the SDK's event-focused timeline (`/context`). A bar over its
  end says "You're viewing older messages" with "Jump to present", and
  scrolling near its end loads newer pages, with "Loading newer messages..."
  under the last one, as scrolling up loads earlier ones. New messages don't
  go in the window, and the conversation stays unread. Jump to present,
  sending a message, or a newer page that reaches the newest shows the
  newest again, as Yapfast kept them, and leaving the conversation lets the
  window go. A jump to a conversation that isn't open opens it at the
  message.
- Recover encryption keys or recheck verification performed in another client.
- Send only from a cross-signed device into a joined encrypted room. Existing
  plaintext rooms are read-only.
- Write Markdown. Messages with formatting render bold, italic, strikethrough,
  inline code, lists and links through Slint's Markdown, and headings, subtext,
  quotes and fenced code as blocks that `crates/yapfast-core/src/blocks.rs`
  splits out by Discord's rules (`TextBlocks` in `ui/messages.slint`); see
  [Discord](discord/README.md) for the details. Message text, authors and times are
  selectable across messages through Yapfast's patched Slint
  ([building](building.md#dependencies)); Ctrl+C or the menu's Copy selection
  copies it, and Copy text copies a whole message.
- Start a DM from the pen in the DM list's header: type a name to search the
  homeserver's user directory, or a full Matrix id. Yapfast opens the DM you
  already have with them, or starts an encrypted one and opens it once sync
  lists it.
- Use the keyboard for messages: Up in an empty composer edits your last
  message; Alt+Up moves into the messages, where arrows step, Enter opens the
  menu, + reacts, R replies, E edits, Delete deletes, C copies, F forwards and
  Escape returns to the composer.
- Clicking a mention, a link to a Matrix user, opens their profile card.
- Mention someone by typing `@` and picking from the room's members. The
  message links to them (`[Name](https://matrix.to/#/@user:server)`) and lists
  them in `m.mentions`, so their client notifies them. Who else is typing
  shows just above the composer, gently pulsing, after three dots under the
  avatars that rise in turn, and Yapfast tells the room while you type unless a
  setting turns it off.
- Hover or click a message for its toolbar: your three most used reactions, Add
  reaction, Edit on your own message or Reply on someone else's, and More.
  More, or a right click on the message, opens a menu with your four most used
  reactions, Add reaction, View Reactions, Edit, Reply, Forward, Create Thread
  or View Thread, Copy text, Copy selection, Pin or Unpin, and Delete, with
  Copy Image, Save Image, Copy Link and Open Link on a picture or link
  (`ui/messages.slint`). View Reactions appears once a message has reactions,
  Copy selection while text is selected, and Edit on your own messages only.
  Delete appears on your own messages, and on anyone's for a Discord moderator
  with Manage Messages. Until you have reacted, the most used reactions are 👍,
  ❤️, 😂 and 🎉. Yapfast counts the reactions you add, not the ones you remove,
  in `layout.json` on this computer. The icons have tooltips and accessible
  names. Add reaction opens a picker with a search field, frequently used emoji
  and every category of the `emojis` crate up to Unicode 15.0, which Noto Color
  Emoji and Windows 11 draw; Enter picks the first match. Clicking an existing
  reaction adds or removes yours.
- A GIF picker next to the emoji button, on Discord: a search over Klipy,
  trending terms and favourites, as in Discord's client. A click sends a GIF,
  Shift and a click puts it in the message, and a star keeps it among the
  favourites; GIFs and moving pictures in messages have the star too. Its left
  edge drags it wider, as Discord's does: the right edge and the height stay,
  the grid fits another column for every 212px, and the edge stops 8px past
  the server list. The width is kept in `layout.json` (`gif_picker_width`) and
  comes back at the next start. See [Discord](discord/messages.md#gifs).
- GIFs, animated emoji, stickers and the picker's tiles play only while
  Yapfast's window has the focus, as in Discord: on another screen they hold
  still on the frame they were showing, and go on from there when the
  window has the focus again.
- Who reacted, as in Discord's client: half a second on a reaction shows a
  tooltip, ":tada: reacted by Ann, Bo, Cy, and 2 others", naming the first
  three by their name there, once they come (`Command::Reactors`, three at a
  time). The tooltip stays while the pointer is on it, and clicking it opens
  the Reactions window on that reaction. The window, from View Reactions,
  lists the message's reactions down the left, the most first, and who
  reacted with the chosen one, 100 at a time, the next page as the list nears
  its end. Hovering your own row offers to remove your reaction; a Discord
  moderator, with Manage Messages, may remove anyone's. Yapfast keeps the last
  32 tooltips' names while their counts stay, and the window's people only
  while it's open. Matrix rooms answer from the timeline, which holds every
  reaction's senders.
- Forward a sent message from a dialog with a search field: choose up to five
  conversations on the message's own network, add a message if you like, and
  send; with one chosen, it opens. On Matrix, Yapfast forwards only into
  encrypted conversations and sends the message's current content as a new
  message, without its reply quote. An encrypted attachment keeps its uploaded
  file, and its key travels in the new encrypted event. On Discord it forwards
  as Discord's client does ([Forwarding](discord/messages.md#forwarding)). The message
  added goes after the forward as a message of its own. If a forward fails
  somewhere, the dialog opens again with those conversations chosen and why.
  Delete asks for confirmation, then
  redacts the message for everyone. Replies quote the original; edits are marked
  "(edited)". Reply and edit modes show above the composer, with Cancel.
  Cancelling an edit restores the draft it replaced.
- Attach files with the system file dialog, paste them with Ctrl+V: files
  copied in a file manager, or a picture copied from a browser or a screenshot
  tool, or drop them on the window from a file manager. A drag over the
  window dims it under Discord's card, "Upload to #general" (`@Sam` in a DM,
  the name in quotes for a thread, the name alone for a group DM), with "You
  can add comments before uploading." Discord's second line, "Hold shift to
  upload directly.", is left out: Yapfast can't see Shift during a drag on
  Wayland or X11. A drop goes whole or not at all, as in Discord's client,
  which then says why until Got It, a click or the next drag: "Too many
  uploads!" past ten files with those waiting, "Your files are too powerful"
  with "Max file size is 20.00 MB please." for a file past the limit, and
  "Invalid file type" for a folder. winit takes drops on X11 and Windows; on
  Wayland, where it has none, Yapfast keeps a data device for each seat on
  winit's own connection, which a small thread watches, and takes
  `text/uri-list`. A drop is taken only where the composer can take files,
  as a paste is. They wait above the text as tiles, with a preview for pictures and an
  X to take each out, until the message is sent; text can be typed beside
  them, and the message may be the files alone. A message carries up to ten
  files. Each must fit the conversation's limit, and a file that doesn't is
  refused with its size and the limit. On Wayland the clipboard is read
  through the data-control protocol, so a picture keeps its copied bytes and a
  GIF stays animated; elsewhere arboard reads it, and pixels are saved as PNG.
  Pasted pictures are saved as `image.png` in a temporary folder removed when
  Yapfast closes. Text on the clipboard pastes as text. A right click in the
  composer opens Discord's menu for the field, without its spelling
  suggestions: Cut and Copy while text is selected, Paste, which pastes as
  Ctrl+V does, and Select All, which Discord's lacks. A thread's composer
  has the menu too, and pastes text only, as it holds no files. In Matrix the SDK encrypts
  and uploads each file through its send queue, up to 50 MB, then sends the
  text after them. Images show a decoded preview of up to
  360×270, decoded with dimension and memory limits, with a save button over the
  picture on hover. Other files show a card with a save button. Saving writes the
  decrypted file where you choose. A Discord message with several images shows
  them as square tiles, two to a row.
- Click an image to see it large over the window. It opens at the image's full
  proportions with the preview, then sharpens when Yapfast has downloaded it at up
  to 1920×1200. The left and right arrow keys or the side buttons step through
  the message's other images, a counter shows which one it is, and Escape, the
  close button or a click outside the image closes it. It can save the image
  too.
- Send through the SDK's persistent send queue. A message appears at once with
  "Sending...". After a connection problem it shows "Not sent yet" and goes out
  by itself when sync recovers. Sync uses the SDK's offline mode, which keeps
  probing the homeserver. A refused message shows "Not sent" with Retry. Queued
  messages can be cancelled, and they survive closing Yapfast.
- Save each room's draft in the encrypted session store, so drafts survive
  restarts. Sending clears the saved draft.
- Show unread channels in bold with a short pill at the sidebar edge, and the
  same pill beside servers and the DM button that contain them. Mentions show a
  red count. A conversation is read once its newest message is in view with
  the window in front, as Discord's client reads one: a Discord channel
  then, a Matrix room as its timeline loads. A conversation opened with
  unread messages opens at a marigold
  "New" divider above the first one, sitting at the top of the view rather
  than at the newest message, and the author's name shows again below it. The
  divider stays put while the conversation is open, as newer messages are
  read as they come, and it's gone the next time once the conversation was
  read. While the divider is out of view, until the newest message has been
  in view, a marigold bar at the top says how
  many messages are new and since when ("80 new messages since 14:44"), with
  an arrow toward it, and clicking it scrolls back to the divider. A Discord
  channel places it by message order; when every loaded message is unread,
  it opens on a window of the messages around the last read one, where the
  bar counts those loaded with a plus ("50+ new messages"). A Matrix room
  places it after the event its read receipt is on, once that event is in
  the loaded timeline.
- Highlight messages that call on the user with a faint marigold tint and a
  marigold edge on the left, as Discord does: a mention of them, of a Discord
  role they have, or @everyone or @here where those reach them, always, and a
  reply to their message while it's new. New is from the "New" divider down,
  and what comes while the conversation is open; a conversation opened with
  nothing unread counts from its newest message, and the thread beside a
  channel from its own last read message. A reply that pings only lists
  them as mentioned, so it counts as a mention only when its text names
  them. Matrix's push rules decide which messages mention the user; a reply
  stays a mention when its text has their user ID or a link to it, or
  @room. Their own messages are never highlighted. Yapfast counts unread messages on this device; for a room that isn't open,
  sync often delivers only the newest event, so Yapfast shows that something is
  unread rather than how much.
- Show sync status in the account panel. Reconnect appears there when offline;
  sign out (which retains encrypted stores) is in settings.

## Layout and look

Ctrl with + or -, or with the scroll wheel, zooms the whole window, text and
layout together, from 50% to 200%: 5% a step up to 150%, then 10%. Ctrl+0
returns to 100%. The level shows
for a moment at the top and is kept in `layout.json`. Yapfast multiplies the
display's scale factor by it, so the window lays out in fewer logical pixels,
and zooming in stops before those would fall below the window's 800×560
minimum, which would make it grow.

The window starts at the size it had when Yapfast last ran, maximized if it
was, and on Windows in the same place (`window` in `layout.json`,
`src/ui/placement.rs`). Yapfast writes it a second after the window stops
moving or resizing, and as it quits, so a Windows shutdown that ends Yapfast
without closing it keeps it too. Maximized, it keeps the size and place
un-maximizing returns to. On Windows a place whose title bar would show on no
monitor, as after one was unplugged, is dropped and Windows chooses one, and a
size larger than the monitor's work area shrinks to fit it. Linux keeps only
the size: Wayland doesn't let a window choose its place, and an X11 place could
be on a monitor that's gone. A window made again from the tray comes back
from the same record.

On Linux desktops that give their windows title bars, Yapfast draws its own
in place of the desktop's, as Discord does (`src/ui/title_bar.rs`,
`ui/title_bar.slint`): a 32px bar on the rail's ground across the top, with
the server's name, "Direct messages" or "Yapfast" in the middle and
minimize, maximize or restore, and close at the right. Pressing on it and
moving moves the window, a double click maximizes it, and close does what
the desktop's close button did. Without the desktop's frame, the window's
outer 5px resize it, its 8px corners both ways, except while maximized. The
desktop doesn't say whether it would draw a title bar: KWin and Hyprland
both answer xdg-decoration with server-side, and only KWin draws one. So
`XDG_CURRENT_DESKTOP` decides: KDE, GNOME, Cinnamon, XFCE, MATE, LXQt, LXDE,
Budgie, Pantheon, Deepin, COSMIC, Unity and Enlightenment get Yapfast's bar,
and a tiling compositor such as Hyprland, sway or niri, or any desktop not
listed, keeps whatever the desktop draws, which may be nothing. "Use the
system title bar" in Appearance overrides it either way
(`system_title_bar` in `layout.json`) and applies at once. Each window is
made with the desktop's frame or without it, through Slint's winit
attributes hook, and a maximized one is made again when the setting changes,
reopening on Appearance: where the desktop draws no frame, as on GNOME and
Weston, winit draws one, and adding it to a maximized window or taking it
off makes the window larger or smaller than maximized, for which Weston ends
the connection. To try the bar, `scripts/ui-session start --desktop` runs
Weston's desktop shell, whose windows move and resize, where its kiosk shell
makes the window full screen, which hides the bar. A window
without the desktop's frame gets no shadow from KWin or Mutter, and the
desktop's title bar menu isn't offered: winit can't show it on Linux. In
full screen, as a call's stage is, the bar goes. Windows keeps its own frame.

The window follows Discord's arrangement:

- A 72px server rail. Each server is a circle that becomes a rounded square,
  with a white pill at the rail edge for hover, selection or unread activity.
  The server of a call this account is in, joining included, shows a white
  speaker on green at its icon's top right, so the call stays in sight from
  anywhere; a closed folder shows it for its servers. The rail is built again
  for a voice event only when the call starts, ends or moves.
  Hovering shows the server's name. A right click on a server, or the Menu
  key or Shift+F10 on a focused one, opens its menu: Mark as read when any of
  its channels is unread, then Copy link and Copy ID. A Discord server's
  menu also has, while Discord is connected, Invite to server where the
  account may invite; Mute server, whose submenu
  mutes for the DM menu's times, or Unmute server with when the mute ends;
  Notification settings, whose submenu picks All messages, Only @mentions
  or Nothing, with the level that applies under it, and ticks Suppress
  @everyone and @here and Suppress all role @mentions; Hide muted
  channels, a box to tick; Edit server profile, which opens the member
  menu's nickname dialog for one's own nickname there, where the account
  may change it; where the account may manage channels, Create channel
  and Create category; and, in red, Leave server, which asks first and
  isn't offered to the server's owner.
- A sidebar, 240px by default and resizable by dragging its edge, with the
  server name, a filter, and uppercase TEXT CHANNELS and VOICE CHANNELS groups.
  A Discord server's name, with a chevron beside it, opens a dropdown below
  it, as in Discord, and the chevron turns over while it's open; Enter, Space,
  the Menu key or Shift+F10 open it too. A person with a plus, on the
  header's right, invites to the server. The dropdown lists the choices of the
  server's menu that Discord's dropdown has, in its order, from the same list
  (`Application::header_choices`), so the two can't disagree. A Discord
  channel list heads each category's channels with its name; a category
  without channels shows its heading alone, unless its permissions hide it.
  A right click on a heading, or the Menu key or Shift+F10 on it, opens the
  category's menu: Mark as read when its channels have messages to read,
  its mute and notification settings (below), Edit category and Delete
  category where the account may manage it, and Copy ID. The heading
  names its first channel, or an empty category's own id, and Yapfast finds
  the category from that.
  DMs show the person's avatar. Participants of the call you are in appear under
  its voice channel, with a green ring while they speak and a muted icon; for
  Discord, who is in every other voice channel shows too, with a LIVE tag for a
  screen share and a camera icon. You, in a channel on another client such as
  your phone, show dimmed until hovered, without icons, and the tooltip says
  "Connected on another client". Every row takes the hover tone, and clicking
  someone opens their profile card beside the row, as Discord's popout does;
  someone listed only in a voice channel gets one from the name and avatar the
  channel gives, which their Discord profile fills in. Clicking someone's LIVE
  tag opens their channel's call and watches them. Outside that call it asks
  first, "Join <channel> to watch <name>'s stream?", or in another call the
  switch prompt asks, and the stream opens once the call connects and shows
  them sharing. Cancelling, leaving, a failed join or opening another room
  forgets it. A right click on a conversation, or
  the Menu key or Shift+F10 on a focused one, opens its menu: Mark as read when
  it's unread, Join voice for a voice channel you aren't in, Profile for a
  one-to-one DM, Pin or Unpin and Start call for a DM, then Copy link and Copy
  ID; a Discord channel's adds Invite to channel before Copy link, and
  where the account may manage it Edit channel, Duplicate channel, Create
  text (or voice) channel, which goes in its category, and Delete channel
  after it. A pinned
  DM stays at the top of the list, with a pin, on this device. A
  Discord DM's menu also has Add note, Add or Change friend nickname, Close DM,
  Remove friend, Ignore and Block, each undone by its opposite, and Mute, whose
  submenu mutes for 15 minutes, 1, 3, 8 or 24 hours, or until unmuted. A
  Discord server's text channel offers "Mute channel", with the same times,
  or "Unmute channel", and "Notification settings", whose submenu picks which
  messages notify; a right click on a category's heading offers the same for
  the category. A muted
  DM or Discord channel shows faint, and a Discord server set to hide muted
  channels leaves them out, but for the open one and those that mention you.
  Removing a friend and blocking ask first. Matrix links go through
  matrix.to. Arrow keys, Home and End move through the menu, Right opens a
  submenu and Left leaves it, Enter chooses, and Escape closes it.
- Inviting opens a dialog with a link Discord made at once, lasting 7 days
  and working any number of times, a button that copies it, and a line on
  how long it lasts. Choices for how long a link lasts (30 minutes to 7
  days, or for good) and how many may use it (any number, or 1 to 100), as
  Discord's dialog offers them, make another with Generate a new link.
- Creating a channel asks for its name and, as Discord's dialog does,
  whether it's for text or voice; a category asks only for a name. Editing
  a text channel loads its name, topic, slowmode, stepped through Discord's
  choices from off to 6 hours, and age restriction, and saves them with
  Save changes; a voice channel or a category edits only its name.
  Duplicating asks for the copy's name. Deleting asks first, in red. Text
  channel names become lower case with dashes, as Discord makes them, and
  a text channel made opens. The dialogs are in `ui/server.slint`.
- Menus (`ContextMenu` in `ui/controls.slint`) are as wide as their rows'
  text, from 188 to 320px, and each row may open a submenu of its own,
  beside the menu, or on its left near the window's right edge. A row can
  carry a second line under its text, such as the level a notification
  submenu has chosen, a check in place of its icon for the current choice,
  or a box to tick. Ticking leaves the menu open. A row can show greyed, as a
  role this account may not give does, and can't be chosen. A menu or
  submenu taller than the window scrolls, and the keys keep their row in
  sight. `src/ui/menus.rs` builds a menu's rows with each row's submenu, and
  ticks a box when it's chosen.
- A dock across the rail and sidebar: the voice panel, whose signal icon
  and status show a Discord call's ping on hover and its connection details
  on a click (see [Connection details](discord/voice.md#connection-details)), then the account panel
  with avatar, status, mute and deafen during a call, and settings (account,
  theme, audio devices, message text size, clock and link previews, notifications,
  sounds, reconnect, sign out). A caret beside mute and deafen opens a
  device menu; the input menu shows a local level meter. While you speak in
  a call, unmuted, the panel's avatar has the same green ring as your row in
  the call.
- A conversation with a 48px header: the conversation's icon and name, and
  for a Discord server channel its topic on one line after a rule, which a
  click shows in full. Its buttons sit on the right, as in Discord: a DM's
  call, pins, adding friends, member list or profile, and search; a server
  channel's bell, pins, member list and search. The bell opens the channel's
  mute and notification level, and shows slashed while it's muted. A server
  channel's search looks through the whole server, takes Discord's filters
  such as `from:`, `in:` and `has:`, heads its results with their channels
  and stays open in them. Messages from one person within seven
  minutes are grouped under one avatar and name. Hovering a grouped message
  shows its time in the gutter. Replies draw a spine to the quoted message, and
  a date divider starts each day. The top of the history shows what's
  before it: earlier messages loading, a way to retry them, or the start of
  the conversation. The composer is a rounded field with attach, the
  message ("Message #general"), and a send button; reply and edit modes attach
  a bar above it. With no conversation selected, the area shows "Pick a
  conversation" and hides the composer. The local preview labels itself in a
  strip under the header.
- A 240px member sidebar right of the conversation, under its header, on the
  sidebar's tone. The member count in a group DM's header shows or hides it,
  as the people icon does in a server channel's, bright while the list
  shows, and Yapfast remembers a closed list. While it shows, dragging the left sidebar still
  leaves the conversation 420px.
- A right click on someone, in the member list, on a message author's avatar
  or name, or in the friends list, opens their menu: Profile, Message to open
  or start the DM with them, and on Discord Add friend, Accept friend request
  or Cancel friend request, then Copy ID. In a Discord server's channel it's
  their menu as a member, as Discord's: Mention, Call, a note and nickname,
  their roles in a submenu of checkboxes, and Time out, Kick and Ban last, in
  red, as this account's permissions and its rank against theirs allow, then
  Copy ID; see [Discord](discord/servers.md#a-server-members-menu). In a Discord
  server's call, or under a voice channel, their volume sits above that
  menu, which adds a Mute just for you, Move to and, in red, Server mute,
  Server deafen and Disconnect; see
  [Discord](discord/servers.md#someones-menu-in-a-voice-channel). Time out and Ban ask how long
  or how much in their dialog, and each asks a reason. In a one-to-one DM, the
  other person's menu is the DM's own, less Mark as read and Copy link, and
  Copy ID copies theirs. Clicking an author's avatar or name, which underlines while
  pointed at, shows their card beside it.
- A voice stage with participant tiles, a green border while someone speaks,
  and round mute, deafen and leave controls.

Atkinson Hyperlegible Next (SIL Open Font License, embedded in the binary; see
`ui/fonts/README.md`) sets all text in four weights. Satoshi, the second choice,
isn't distributed; `ui/fonts/README.md` says how to switch back. The palette is layered graphite with one
marigold accent for selection and primary actions, red only for mentions and
destructive actions, and green only for live voice. Avatars get one of eight
tints from the person's name.

Network operations run on the existing two-worker Tokio runtime. The UI
receives SDK events, and the message list virtualizes its rows. Rows keep their
place by message id: a change rebuilds only the rows of the messages it
changed, and older messages go in above the rows shown, which stay where they
are on screen. There is no UI polling loop. On Matrix, only joined Spaces and
their joined channels appear. DMs stay in the DM list even if linked to a
Space. Matrix standalone rooms outside Spaces, channel discovery, room
creation, threads, video and audio playback, and deleting other people's
messages remain later work. Closing the last window exits Yapfast.

## Validation

The Matrix chat integration tests in `crates/yapfast-core/src/matrix_chat.rs`
create disposable accounts and cover restoration, encryption and its gates,
Spaces, Markdown, replies, reactions, edits, deletion, attachments, drafts,
offline sending across a restart, and messages missed while a room was
closed. Run them after `scripts/test-stack up`:

```sh
nix develop path:./nix --command cargo test --locked --profile fast -p
yapfast-core matrix_chat::tests -- --ignored --test-threads=1
```

To check the window by hand, see [Seeing the app](building.md#seeing-the-app).
The native file dialogs need an XDG portal, which `scripts/ui-session` doesn't
run, so Attach and Save are checked only through the integration test.

On Linux Yapfast finds an installed emoji font with CBDT or sbix bitmaps and
no COLR table, or failing that a COLR one, maps it and registers it with Slint
as "Yapfast Color Emoji" (`src/emoji.rs`); fontconfig alone can prefer the
COLRv1 build of Noto Color Emoji when both are installed. That font also leads
the sans-serif and UI fallback lists, ahead of display fonts that carry a few
monochrome emoji, so emoji in message text draw in color too; symbols the UI
typeface lacks that have an emoji form, such as ♥, then show as emoji. Linux
uses the prebuilt Skia with Skia's own FreeType, which draws COLRv1, the only
build Fedora installs (issue #32), as `skia_draws_colrv1_glyphs_in_colour`
checks. Windows uses "Segoe UI Emoji".

On Windows, fontique falls back by script, with a sample of each script, and
its sans-serif and UI lists are only Arial and Segoe UI. Emoji, most symbols
and the styled letters of Unicode's mathematical alphabets that Discord names
use, such as 𝓯𝓪𝓷𝓬𝔂, are all of the Common script, so they would show as boxes.
Yapfast puts Segoe UI Emoji, Segoe UI, Segoe UI Symbol, Cambria Math and Segoe
UI Historic at the front of both lists (`emoji::WINDOWS_FALLBACK`). Wine draws
no emoji from a bitmap font standing in for Segoe UI Emoji, so color emoji in
message text need a check on real Windows.
