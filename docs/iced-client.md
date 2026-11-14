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
menus, Delete's confirmation, the member list with each member's looks,
the profile card, and a voice channel's call with cameras and a watched
stream, and Discord's two-factor prompt.

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
asks for the other. `--data-dir`, `--preview` and `--stream-worker` work as
in the default build; there is no tray, so `--background` shows the window,
and links given on the command line aren't opened yet; `--help` says so.

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
end it, `theme ID` chooses a theme and keeps it, and `stage`,
`stage-aside` and `pick` go to the core as the Slint client sends them.
The tray's commands are logged and ignored.

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
client has. There's no Appearance yet, so `theme ID` is how to choose one
while it runs: it writes `theme` into `layout.json`, leaving the rest as it
was, and swaps the struct. A switch costs no picture: the client's icons
and dots are drawn per colour and the ones of the old theme go, but
avatars, emoji and attachments are the same buffers; the names styled for
the other ground (dark or light) are made again as they show.

## Rail and sidebar

The rail (`rail.rs`) is the Slint client's `ServerRail` with its Discord
look: the DM home (the logo; its badge counts DM mentions and friend
requests), the DMs with unread messages, a line, then the servers and
folders, and "Join a server" at the foot. Each tile has its pill at the
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
`menu_choices` in the Slint client's wording; the choices this client
can't act on yet show greyed.

The account's card (`account.rs`) spans the foot of both: the avatar with
its presence, the name and status (the username under the pointer), and
the mic, headphones and Settings, which mute and deafen in a call.

Open folders, folded categories and Developer Mode (which shows Copy ID)
are kept in the data folder's `layout.json`, which the Slint client keeps
them in too (`layout.rs`); each change writes its one key back.

## Header and timeline

The header (`header.rs`) is the Slint client's: a DM's avatar and
presence or a channel's icon, a thread's channel (which leads back to it),
the name, a channel's topic on one line after a dot, what a DM's person is
up to; then the room's buttons in Slint's order (calls, threads,
notifications, pins, add people, the member list, the profile) and the
search box. Only the member list's toggle acts yet.

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
of older messages has the bar back to the present; a reply's line jumps to
the message it quotes and tints it a moment. Scrolling near the top asks
for earlier history.

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
  no view shows them.
- A video (`playback.rs`) shows its poster with a play mark; clicked, it
  plays in its cell, its frames coming through the stream worker's shared
  mapping as a watched stream's do (`video::Sources::clip`). While the
  pointer is on it, or it's paused, a bar along its foot plays or pauses
  it, shows its time, moves through it and mutes it.
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
  four reactions used most, Add reaction, Edit, Reply, Forward, Copy text,
  Copy selection, End Poll Now (asked about in the poll's card), Pin or
  Unpin, Mark Unread, Copy Message Link, Delete and Copy Message ID.
  Forward opens Discord's dialog (`forward.rs`): up to five conversations
  the core finds, an optional message, the slowmode warning, Send.
- The keyboard steps through messages (`browse.rs`): Alt+Up from the
  composer lands on the newest, lit with its hover bar; the arrows, Page
  Up and Down, Home and End step; Enter (or the Menu key, or Shift+F10)
  opens its menu, + reacts, R replies, E edits, Delete deletes, C copies,
  F forwards, Alt+Enter marks it unread, and Escape goes back to the
  composer.
- A masked link (words that aren't where they go) asks first, as Slint's
  does: Go back, or Visit site. The text, and its embeds', is laid out once per message (`markdown.rs`):
the core's blocks (headings, subtext, quotes, fenced code) apart, and in
each the inline styles as runs, with mentions on pills of their colour,
links, and spoilers covered until clicked. A web address opens the browser; a
mention or a notice's name opens the person's card.

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
File", whose Create Thread and Create Poll show greyed until their forms
come), from Ctrl+V (files copied in a file manager, or a picture, which
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

## How it's built

iced builds the whole view again after every message and lays it out, so
what costs is kept out of the view:

- The backend's events come on its receiver, batched, as messages
  (`app.rs`); nothing polls. Fonts the shaper wants and the control file
  come as subscriptions.
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
- Video (`video.rs`, `call.rs`): cameras' frames are made in the client's
  own memory (`picture::set_frame_buffers`) at their tile's size, and a
  watched stream's frames come from the stream worker's shared mapping.
  The renderer's hook (patch 13 below) copies or scales them into the
  window with libyuv. The core wakes the client for each camera frame made
  (`picture::set_frame_wake`); a watched stream is made in another process,
  which has no wake, so while one shows it's looked at every 8 ms.

Each part of the window has its module: `signin`, `mfa`, `rail`,
`sidebar`, `account`, `header`, `timeline`, `message`, `cards`, `bot_select`, `browse`, `composer`, `edit`, `suggest`, `commands`, `typing`, `uploads`, `forward`, `members`,
`profile`, `picker` (with its tabs `emoji`, `gifs`, `stickers`), `menus`, `places`, `dialogs`, `call`; `theme` holds
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
- `controls::outside`: the full-window layer under an overlay that takes
  a click outside it, clear, or dimmed under a dialog.
- `dialogs::dialog`: the frame of every dialog over the dimmed window.
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
topics, the history's edge as earlier messages load, the unread bar's
jump, the view staying put on earlier messages as new ones come, and
sending from there going to the newest; and in `tests/writing.rs` the
composer's line breaks, the reply's mention toggle and Escape, editing in
place, `:name` suggestions, the emoji picker's search, Forward from a
message's menu, and the keyboard's steps through messages. The emoji
ones need the Twemoji font the root crate's `build.rs` packs: they take
the newest any build left under `target/`, and say so and pass without
it. `markdown.rs` has its own for Discord's Markdown. `scripts/check` lints both renderers' builds, runs the tests, and
builds and smoke-tests the CPU build; see [building](building.md#checks).

## Not there yet

Settings (Appearance among them, so themes are chosen by `theme ID` and
custom ones are made in the Slint client), other accounts, threads, the
full profile, voice's controls beyond Leave, the tray,
notifications, links, accessibility and Windows' own checks. On the rail
and in the sidebar: dragging servers, channels and people, the voice
card beside a busy channel, the nameplates of DM rows, Wave, a server's
banner, boost bar and events row, and the dialogs and settings its menus
lead to (joining a server, invites, notification settings). In the header:
calls from a DM, threads, pins, notification settings, adding people, the
profile panel and the search. In the timeline: saving a file, a
picture's full view, a video's full screen and volume, apps' forms
(modals), blocked people's messages folded away, and the actions that
lead to what the client doesn't have yet: View Reactions, Create Thread
and View Thread, Apps' message commands, a picture's Copy Image, Save and
Open Link; keys typed in the timeline going to the composer. In the composer: dragging the expression
picker's left edge to widen it, and Nitro's offer on a locked emoji or
sticker.
