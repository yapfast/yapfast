# Chat UI

This guide describes shared chat behaviour and the current iced window.
For the client modules, widgets and headless UI tests, start with
[the client guide](client/README.md).

The chat window for Matrix and Discord, laid out after Discord's client.
Matrix messages are sent through the SDK's Markdown feature, and the window
lays formatted text out itself (`crates/yapfast-iced/src/markdown.rs`).
`pulldown-cmark` reads Markdown there, and in `yapfast-core` for links,
spoilers, reply snippets and notification text (`links.rs`, `spoiler.rs`,
`chat.rs`). `image` decodes pictures within bounds, and `rfd` opens native
file dialogs.

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
  uses most, fifteen at most and not the one open, headed "Previous
  Channels"; then, under "Mentions", the conversations that mention this
  account and aren't listed already, most mentions first
  (`AppState::switcher_mentions`). With "Search DMs with the quick
  switcher" on (Settings > Messages, `dm_search_switcher`, off by default),
  the DM list's box at its top is Discord's "Find or start a conversation",
  a button that opens it, rather than a filter. A search lists up to 50
  conversations and servers, ranked as Forward To ranks them
  ([Forwarding](discord/messages.md#forwarding)), and those as good by name;
  `@` finds people, `#` text channels, `!` voice channels and `*` servers.
  Channels this account may not read stay out, and so do message requests.
  The arrow keys or Tab choose, and Enter or a click goes there: a server
  opens on its channel shown last. Unread places show bold, and mentions as
  a count.
- Ctrl+F goes to the conversation's search box in the header, as in
  Discord, wherever the focus is, but not over the settings, a dialog or a
  menu. In a server's text channel it fills the box with `in:#channel `
  first, in place of what it held; Ctrl+Shift+F, or Ctrl+F in a DM, keeps
  what's typed. Escape in the box clears it and ends the search, keeping the
  focus there; in an empty box it lets go. A keyboard layout without Latin
  letters uses the key in F's place.
- Peek at a channel or DM without opening it, a Yapfast addition that
  Discord doesn't have. With "Peek at channels" on in Settings > Messages
  (`peek_channels` in `layout.json`, off by default), holding Alt alone over
  a row of the channel list or the DM list shows its newest six messages
  beside the row, as the pins show them, newest at the bottom. Letting go
  of Alt, or leaving the row, closes it, and its rows go. A channel Yapfast
  keeps in memory and that's up to date answers from there; any other asks
  Discord once for six (`GET /channels/{id}/messages?limit=6`), which aren't
  kept, and its pictures show only if they're kept already. Nothing is
  opened and no read goes to Discord: the peek reads no channel, so it stays
  unread with its mentions. Not over the open channel, a locked one, a
  voice channel without a chat, a forum or a Matrix room. Alt is read from the
  window's modifier keys wherever the focus is, and counts as let go when
  the window loses the focus.
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
  custom statuses, and a red clock beside someone timed out. An app has an APP
  tag after its name and the server's owner a crown, in that order, as in
  Discord. Above a server channel's list, an Activity panel shows Discord's
  feed of what people there are doing and did lately; see
  [the Activity panel](discord/people.md#the-activity-panel). Yapfast subscribes to the list's
  first 100 rows while the channel is open, and Discord keeps them current.
  The list shows a change at once, then holds for half a second and shows
  the changes from that time together, so a busy server's list draws at most
  twice a second.
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
  it through the SDK's event-focused timeline (`/context`). A floating pill
  over its end says "You're viewing older messages" with "Jump to present", and
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
  inline code, lists and links as runs of styled text, and headings, subtext,
  quotes and fenced code as blocks that `crates/yapfast-core/src/blocks.rs`
  splits out by Discord's rules (`crates/yapfast-iced/src/markdown.rs` lays
  both out); see [Discord](discord/README.md) for the details. Message text,
  authors and times are selectable across messages: each is a `Piece`
  (`crates/yapfast-iced-kit/src/piece.rs`) that draws its part of the
  selection, which `timeline/selection.rs` keeps; Ctrl+C or the menu's Copy
  selection copies it, with `[Picture]` on a line for each picture it runs
  through, and Copy text copies a whole message.
- Fold long messages, a Yapfast addition that Discord doesn't have. With
  "Collapse long messages" on in Settings > Messages (`collapse_long` in
  `layout.json`, off by default), a message whose text, code blocks and all,
  runs past fifteen lines shows its first twelve, cut off, with Show more
  under it; Show more shows it whole, and Show less folds it again. Its
  pictures, embeds and reactions show under it either way. Each message
  keeps its own choice while Yapfast runs, in the timeline and the thread
  beside it, and starts folded again after a restart. Folding measures the
  text as laid out, so it follows the window's width and the text size.
  Clicking either stops the view following the newest message, so the
  message's top stays where it is as it grows, and Show less on a message
  whose top is above the view brings its top back into view.
- Start a DM from the pen in the DM list's header: type a name to search the
  homeserver's user directory, or a full Matrix id. Yapfast opens the DM you
  already have with them, or starts an encrypted one and opens it once sync
  lists it.
- Use the keyboard for messages: Up in an empty composer edits your last
  message; Alt+Up moves into the messages, where arrows step (the message
  the keys are on has a marigold edge down its left), Enter opens the
  menu, + reacts, R replies, E edits, Delete deletes, C copies, F forwards,
  Alt+Enter marks a Discord message unread and Escape returns to the composer.
- Type where no field takes it, as after a menu closes, and the key goes to the
  end of the composer used last, the channel's or the thread's beside it, which
  takes the focus, as in Discord; Backspace and Delete too, and Tab with nothing
  focused. Enter doesn't send from there, unlike Discord. Not while the settings,
  a profile, a card, a dialog, a panel such as the pins, the image viewer, a
  ringing call or the call in full screen is open, nor on the friends list or a
  call, nor with Ctrl, Alt or Super held, nor for a push-to-talk key while
  unmuted in a call. Stepping through the messages, only letters and digits go.
- Clicking a mention, a link to a Matrix user, opens their profile card.
- Emoji in message text are drawn by Twemoji's picture of them, as the picker's
  are, in a colour bitmap font made of them when Yapfast is built
  (`crates/yapfast-iced/build.rs`); see [Discord](discord/messages.md) for
  how that font is made. A custom emoji is a picture in the text's line: its
  run keeps the picture's room, so the text wraps and selects round it, and
  the picture is drawn there. A picture is drawn 22px in the
  message text and 48px in the large text a message of emoji alone shows, and
  the composer shows a Unicode emoji as its picture too, since each glyph
  also answers to the emoji's own code point. A `:shortcode:` sent without
  taking its
  completion draws as its emoji. A link is underlined only while the pointer is
  on it, as in Discord's client. An emoji is a link, but one whose text is the
  picture alone is never underlined: the line would show across the bottom of
  it. Clicking one opens its card, 288px wide and opening
  below it: its name such as `:sob:`, a line saying what it is, and, for a
  server's emoji, the server it is from, with how many of its members are
  online and whether Discover lists it, whose icon and name open that server.
- Mention someone by typing `@` and picking from the room's members. The
  message links to them (`[Name](https://matrix.to/#/@user:server)`) and lists
  them in `m.mentions`, so their client notifies them. Who else is typing
  shows in its own strip over the composer, the names strong, after three
  dots that pulse in turn, and Yapfast tells the room while you type unless a
  setting turns it off.
- Hover or click a message for its toolbar: your three most used reactions, Add
  reaction, Edit on your own message or Reply on someone else's, Forward on a
  message that can be forwarded, and More. The toolbar stays while the menu,
  picker or delete question it opened is open. An author's name is underlined
  under the pointer; a click on it or the avatar opens their card.
  More, or a right click on the message, opens a menu with your four most used
  reactions, Add reaction, View Reactions, Edit, Reply, Forward, Create Thread
  or View Thread, Copy text, Copy selection, Pin or Unpin, Mark Unread and
  Copy Message Link on a Discord message, and Delete, with Copy Image, Save
  Image, Copy Link and Open Link on a picture or link, then Copy Message ID
  last on a Discord message (`MenuAct` in
  `crates/yapfast-iced/src/menus.rs`; see
  [Discord](discord/messages.md#mark-unread-and-message-links)). View Reactions appears once a message has reactions,
  Copy selection while text is selected, and Edit on your own messages only.
  Delete appears on your own messages, and on anyone's for a Discord moderator
  with Manage Messages. Until you have reacted, the most used reactions are 👍,
  ❤️, 😂 and 🎉. Yapfast counts the reactions you add, not the ones you remove,
  in `layout.json` on this computer. A custom emoji among them shows the
  picture the application keeps of it, fetched for the toolbar when it isn't
  there yet, as after a restart, rather than the picker's own, which goes when
  the picker closes. The icons have tooltips and accessible
  names. Add reaction opens a picker with a search field, favourite emoji,
  frequently used emoji and every category of the `emojis` crate up to
  Unicode 15.0, which Noto Color
  Emoji and Windows 11 draw; Enter picks the first match. Clicking an existing
  reaction adds or removes yours. A message's reactions wrap onto as many
  lines as its width needs, as in Discord's client.
- Holding Shift turns the toolbar into quick actions, as in Discord's
  client: the reactions give way to Pin or Unpin, Create Thread, and on your
  own message Reply, before Add reaction and Edit or Reply; Forward follows;
  and More gives way to Delete where you may delete, which deletes at once
  without asking. Shift and a click on Delete message in the message's menu
  deletes at once too, where a plain click asks. The thread beside a channel
  does the same, without Create Thread. Shift is read from the window's modifier keys wherever the focus
  is, and counts as let go when the window loses the focus, so it can't stick
  on after being let go elsewhere. It leaves a bar alone while its picker or
  menu is open. A pointer that stays put is told what's under it as the
  buttons change, so its tooltip and highlight match what a click does.
- A GIF picker next to the emoji button, on Discord: a search over Klipy,
  trending terms and favourites, as in Discord's client. A click sends a GIF,
  Shift and a click puts it in the message, and a star keeps it among the
  favourites, its tooltip saying "Add to Favorites" or "Remove from
  Favorites"; GIFs and moving pictures in messages have the star too. Ctrl+G
  closes it as it opened it. The search keeps the keys through clicks on the
  picker's tiles, stars, rail and Back, so typing goes on searching, in
  Trending GIFs too, where the search is out of sight under the title. The
  pointer on a section of the emoji or sticker rail names it at the foot. A
  favourite's small thumbnail is kept on disk for as long as the GIF is
  favourited, up to 200 MiB. A start fetches ahead the ones that cost little,
  videos and small files; a larger GIF's is made when its tile first shows. Each of the home's trending terms shows one GIF, and those are fetched
  ahead into a cache of their own, so the home paints at once. Its left
  edge drags it wider, as Discord's does: the right edge and the height stay,
  and the edge stops 8px past the server list. The GIFs and the Emoji share
  that window and its width: the GIFs' grid fits another column for every
  212px, and the emoji's another of Discord's 56px buttons a row. The buttons
  keep their size and their spacing as it widens, so what the width has over
  stays at the row's end. Each emoji is drawn from Twemoji's picture of it,
  which is the artwork Discord's client uses and which the program carries
  (`assets/twemoji`, CC-BY 4.0), taken once from the Twemoji font's PNG at
  its own 72px and shrunk by area to the pixels a cell draws
  (`paint::emoji`), in the renderer's own pixel format, so a frame copies it
  without scaling. It lists only the emoji Twemoji has a picture for. A
  screen of rows either side of those shown is drawn with them, and
  the custom emoji's pictures of three screens either side are asked for,
  nearest first, the ones this account may not use too. A cold cache or a
  scroll beyond that margin can still show placeholders until downloads finish. A
  picture asked for again moves ahead of those asked for since, so what shows
  now downloads and decodes before what a scroll passed, and one that arrives
  shows within a few frames without laying the rest of the window out again;
  asking changes nothing on screen, so a scroll lays nothing out either. A
  section's rail button and heading are a quiet square until its server's
  icon, or for a server without one its first emoji, comes, which every
  section asks for as the picker opens. A row further than those three
  screens lets go of its custom emoji's pictures and the application's copies
  and pending requests at the same time, even when none has arrived yet, so
  scrolling through every server's emoji holds a few screens of them however far it goes; a
  row scrolled back to reads them from the disk cache again. Downloads of
  pictures let go of before they came are called off, and one that comes all
  the same isn't kept. Startup queues the account's custom emoji pictures for
  the disk cache, even when avatars or some emoji were cached already. Later
  snapshots queue newly added emoji too; opening the picker shares that URL
  tracking rather than repeating startup's requests. Background downloads run
  behind visible pictures and decode nothing. Up to eight background workers
  drain a deduplicated URL queue; a visible request promotes an active background
  job instead of downloading the same picture twice. Cancelling a foreground
  request returns its URL to background warming instead of abandoning it. At most twelve encoded
  pictures read from the disk or downloaded are held waiting for the four
  decode turns, and eight downloads run at once; a picture the disk doesn't
  hold lets its turn go while it downloads, so one the disk holds never waits
  behind downloads waiting out a backoff. Cancelling a queued request
  releases its turn waiter, and a blocking decode keeps its permits until it
  actually finishes. A picture Discord says isn't there, or that doesn't
  decode, isn't asked for again that session; one that failed only while
  Discord or the network was down is tried again when next asked for. Once
  cached, emoji load from disk rather than waiting for the CDN. A row a
  screen past
  those shown lets go of its Twemoji pictures, and a picker that closes lets
  go of its grid and of every custom emoji's picture but those the messages
  on screen, the topic, the hover bar's reactions and the rest of the window
  (a status's emoji, a profile's) show; the disk cache
  keeps them, and the next look reads them from there. The grid builds only
  the tiles near the view (`picker::kit`), and makes only their pictures,
  each once at the size it shows. Every server's emoji show,
  as Discord's picker shows them: the sections down its left jump to each of
  them, a server's button drawing its icon as its heading draws it beside the
  server's name, Frequently Used's a clock and Favorites' a star, each heading
  in a row of its own, tighter than a row of emoji and at the grid's top as the
  rail's buttons are, and the ones
  this account may not use where it
  is looking — another server's without Nitro, or in a channel that denies
  Use External Emojis even with it, or an animated one without Nitro — are
  greyed with a lock in their corner, and pressing one opens Discord's Nitro
  page. A cell's star, in both pickers, adds the emoji to the Discord
  account's favourites or takes it out, as Discord's client's does: they are
  `favorite_emojis` in the second settings protobuf, a custom emoji by its id
  and a Unicode one by its Discord name without a skin tone, at most 250, which
  every client of the account shares, and Favorites lists them. The grid is
  built as a picker opens, not at a start. Cache warming runs off the thread that draws and does not decode
  pictures; a section reached before warming finishes can still show placeholders.
  A drag that changed the width keeps it in
  `layout.json` (`picker_width`), written once the drags have stopped for a
  second, and it comes back at the next start. See
  [Discord](discord/messages.md#gifs).
- A Stickers tab in the same picker, and a sticker button of its own, on
  Discord: a search field, then the collections down the left to jump between
  — Frequently Used with a clock's own icon, the open server's, whose icon
  leads its name, then Discord's default packs — and their stickers as 96px
  tiles in the picker's columns under the collection's name, as the emoji's
  own are. A click sends the sticker as a message of its own. Discord's packs
  and the open server's stickers are anyone's; another server's need Nitro,
  as Discord's client has them. Its pictures are
  kept on disk and fetched ahead, so the tab shows them at once. The left edge
  drags it wider as it does for the GIFs and the emoji, and the grid takes
  another 96px column for every 104px. Its tiles are those in sight and a
  screen either side, and a scroll that brings one in hands it the element of
  one that went out. See
  [Discord](discord/messages.md#stickers).
- GIFs, animated emoji, stickers and the picker's tiles play only while
  Yapfast's window has the focus, as in Discord: on another screen they hold
  still on the frame they were showing, and go on from there when the
  window has the focus again. Animated emoji stop too when nobody has
  touched the window for 15 seconds and while it is minimised, and go on
  from the frame they stopped on. Accessibility's "Play GIFs, emoji and stickers",
  at Always, plays them unfocused, but not while minimised, and at Never
  keeps them all still. An animated custom emoji plays in message text,
  in its reaction, in the quick reactions over a message and in its menu,
  in the emoji picker's grid and on its card; in a status, a bio, a
  topic, a search's results, the picker's rail and foot and the `:`
  suggestions it shows its first frame. Only those in view play, and an
  emoji shown many times at one size is decoded once.
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
  "(edited)". A reply shows a bar above the composer, with Cancel; Escape
  cancels it wherever the focus is, once no suggestions or anything over the
  conversation are open: first the search, then the channel's reply, then
  the thread's beside it, then the new thread's form.
- Edit a message in its own row, as Discord's client does: the text becomes a
  field there, tinted as the row is while edited, with "escape to cancel •
  enter to save" under it, and the cursor after the text. Enter saves, Shift+Enter
  breaks the line, and Escape or the hint's cancel closes it, changing nothing.
  The field takes `@` and `:name` suggestions and the emoji button's picker;
  while suggestions show, Enter and Escape choose and close them instead. It
  is its own text, apart from the composer's draft and a reply being written,
  which stay as they were; one message is edited at a time. Saving text that
  is empty asks whether to delete the message, as Delete does, unless the
  message has files, and saving text that changed nothing just closes the
  field. Saving turns the `@Name` of anyone the message mentioned back into
  their mention, even someone no longer suggested (a name two of them share
  stays text). The thread beside the channel edits its messages the same way.
  Starting an edit, from the message's Edit button, its menu, E, or Up in an empty
  composer, gives the field the keys and scrolls the row into view, and keeps
  it there as the field grows.
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
  `text/uri-list`. A drop is taken only where the composer can take files;
  files pasted into a message already carrying ten are refused with "A
  message carries up to 10 files." They wait above the text as tiles, with a
  preview for pictures and an
  X to take each out, until the message is sent; text can be typed beside
  them, and the message may be the files alone. A message carries up to ten
  files. Each must fit the conversation's limit, and a file that doesn't is
  refused with its size and the limit. A pasted picture keeps the bytes it
  was copied as, PNG, GIF, WebP or JPEG in that order of preference, so a GIF
  stays animated. Wayland's clipboard is read through the data-control
  protocol, and X11's by asking the CLIPBOARD selection's owner, taking large
  pictures in parts (INCR). Windows apps offer a picture under a name they
  register, "PNG", "GIF", "JFIF" or the MIME type, which Yapfast takes when
  the bytes are of that type; one copied only as a bitmap (`CF_DIB` or
  `CF_DIBV5`, whichever the app put) is decoded by Yapfast and saved as PNG,
  keeping alpha in a 32-bit bitmap's fourth byte as Chromium writes it, and
  opaque when that byte is zero throughout. On X11 and Windows, text on the
  clipboard wins over a picture beside it, since office suites put a picture
  of the cells or text they copy there too. Pasted pictures are saved as
  `image.<extension>` in a temporary folder removed when Yapfast closes. Text
  on the clipboard pastes as text. In any field, text selected with the
  pointer becomes the primary selection, which a middle click pastes at the
  pointer, as on Linux desktops, and a click with Shift held extends the
  selection. A right click in the
  composer opens Discord's menu for the field, without its spelling
  suggestions: Cut and Copy while text is selected, Paste, which pastes as
  Ctrl+V does, and Select All, which Discord's lacks. A thread's composer
  has the menu too, and pastes text only, as it holds no files. In Matrix the SDK encrypts
  and uploads each file through its send queue, up to 50 MB, then sends the
  text after them. Images show a decoded preview of up to
  360×270, decoded with dimension and memory limits, with a save button over the
  picture on hover. Other files show a card with a save button. Saving writes the
  decrypted file where you choose. A Discord message with several images shows
  them as square tiles, two to a row. Where the timeline is narrower than a
  message's pictures, or a link preview's, they shrink to its width, keeping
  their shape, as in Discord's client; they never grow past their size.
- Click an image to see it large over the window. It opens at the image's full
  proportions with the preview, then sharpens when Yapfast has downloaded it at up
  to 1920×1200. The left and right arrow keys or the side buttons step through
  the message's other images, a counter shows which one it is, and Escape, the
  close button or a click outside the image closes it. It can save the image
  too.
- Send through the SDK's persistent send queue. A message appears at once,
  faint until the server has it, with nothing under it; one with files says
  "Sending..." with Cancel while they upload. After a connection problem it
  shows "Not sent yet" and goes out by itself when sync recovers. Sync uses
  the SDK's offline mode, which keeps probing the homeserver. A refused
  message shows "Not sent" with Retry. Queued messages can be cancelled, and
  they survive closing Yapfast.
- Save each room's draft in the encrypted session store, so drafts survive
  restarts. Sending clears the saved draft.
- Show unread channels in bold with a short pill at the sidebar edge, and the
  same pill beside servers and the DM button that contain them. Mentions show a
  red count. A conversation is read once its newest message is in view with
  the window in front, as Discord's client reads one: a Discord channel
  then, a Matrix room as its timeline loads. A conversation opened with
  unread messages opens at a "New" divider above the first one, a line in
  the theme's red with a notched "NEW" tag at its right end (Discord's),
  sitting at the top of the view rather than at the newest message, and the
  author's name shows again below it. The divider stays put while the
  conversation is open, as newer messages are read as they come, and it's
  gone the next time once the conversation was read. Until the newest
  message has been in view, an accent banner across the top says how many
  messages are new and since when ("80 new messages since 14:44"); clicking
  the text scrolls back to the divider, and "Mark as read" at its right end
  reads the conversation and takes the banner away, leaving the divider. A
  Discord channel places it by message order; when every loaded message is
  unread, it opens on a window of the messages around the last read one,
  where the bar counts those loaded with a plus ("50+ new messages").
  Replying from that window shows the newest messages, which can leave the
  divider above everything loaded; the banner then stays, counting with a
  plus, until the conversation is left, and clicking it brings the divider
  back. A Matrix room places it after the event its read receipt is on, once that
  event is in the loaded timeline.
- Highlight messages that call on the user with a faint gold tint and a gold
  edge on the left, as Discord does: a mention of them, of a Discord role
  they have, or @everyone or @here where those reach them, always, and a
  reply to their message while it's new. The message a reply being written
  answers is tinted blurple, with a blurple edge, instead. New is from the
  "New" divider down, and what comes while the conversation is open; a
  conversation opened with nothing unread counts from its newest message,
  and the thread beside a channel from its own last read message. A reply
  that pings lists them as mentioned, so it's a mention and highlighted
  always, as Discord's client does; one with its ping off only while it's
  new. Matrix's push rules decide which messages mention the user; a reply
  stays a mention when its text has their user ID or a link to it, or
  @room. Their own messages are highlighted only when they name the user
  themselves on Discord, as Discord's client does, which never notifies of
  them. Yapfast counts unread
  messages on this device; for a room that isn't open, sync often delivers
  only the newest event, so Yapfast shows that something is
  unread rather than how much.
- Show sync status in the account panel. Reconnect appears there when offline;
  sign out (which retains encrypted stores) is in settings.

## Layout and look

Yapfast starts with the window as large as it was left, maximized if it
was.

Ctrl with + or -, or with the scroll wheel, zooms the whole window, text and
layout together, from 50% to 200%: 5% a step up to 150%, then 10%. Ctrl+0
returns to 100%. The level shows
for a moment at the top and is kept in `layout.json`. Yapfast multiplies the
display's scale factor by it, so the window lays out in fewer logical pixels,
and zooming in stops before those would fall below the window's 800×560
minimum, which would make it grow. On Windows that scale is the one of the
monitor the window is on, taken again when a move or resize settles: the
HIGHDPIAWARE compatibility setting holds the window's own scale to the
primary display's and sends no change on the others. Elsewhere it is the
window's, as a Wayland monitor names only a whole scale where the window
has its fractional one.

The window opens 1120×760. One made again from the [tray](tray.md) comes
back at the size the last one had, maximized if it was.

On Windows, and on Linux desktops that give their windows title bars,
Yapfast draws its own in place of the desktop's, as Discord does
(`crates/yapfast-iced/src/window_bar.rs`): a 32px bar on the rail's ground
across the top, with
the server's name, "Direct Messages" or "Yapfast" in the middle and
minimize, maximize or restore, and close at the right. Signed in, it also has
back and forward arrows at the left, which walk the places visited as
Alt+Left and Alt+Right and a mouse's back and forward buttons do, up to 50 of
them and past any that are gone, and Inbox and Help before the window's
buttons. A place is a conversation, a screen of the direct messages (Friends,
or the requests) or a server's Browse Channels, as in Discord's client; the
settings open over the window and are not one. A thread beside a channel isn't
one either. Help opens Discord's support site.
Inbox opens a popout with Discord's first two tabs: Unreads, the channels and
conversations with unread messages, muted ones only for a mention, and
Mentions, those that mention this account, each with its count; Discord's own
Mentions tab lists the messages themselves. A row opens its conversation, and
clicking outside or Escape closes it. Its rows exist only while it shows.
Pressing on it and
moving moves the window, a double click maximizes it, and close does what
the desktop's close button did. Without the desktop's frame, the window's
outer 5px resize it, its 8px corners both ways, except while maximized. The
desktop doesn't say whether it would draw a title bar: KWin and Hyprland
both answer xdg-decoration with server-side, and only KWin draws one. So
`XDG_CURRENT_DESKTOP` decides (`frame.rs`): KDE, GNOME, Cinnamon, XFCE, MATE,
LXQt, LXDE,
Budgie, Pantheon, Deepin, COSMIC, Unity and Enlightenment get Yapfast's bar,
and a tiling compositor such as Hyprland, sway or niri, or any desktop not
listed, keeps whatever the desktop draws, which may be nothing. There, and
under any desktop's title bar, Discord's bar stays without the window's
buttons, as Discord's client always shows it: where you are, back and
forward, Inbox and Help; it neither moves nor resizes the window. "Use the
system title bar" in Appearance overrides it either way
(`system_title_bar` in `layout.json`) and applies at once. Each window is
made with the desktop's frame or without it, so the window is made again
when the setting changes, with Settings still open on Appearance: where the
desktop draws no frame, as on GNOME and
Weston, winit draws one, and adding it to a maximized window or taking it
off makes the window larger or smaller than maximized, for which Weston ends
the connection. To try the bar, `scripts/ui-session start --desktop` runs
Weston's desktop shell, whose windows move and resize, where its kiosk shell
makes the window full screen, which hides the bar. A window
without the desktop's frame gets no shadow from KWin or Mutter, and the
desktop's title bar menu isn't offered: winit can't show it on Linux. In
full screen, as a call's stage is, the bar goes.

The window follows Discord's arrangement:

- A 72px server rail. Each server is a circle that becomes a rounded square,
  with a pill at the rail edge for hover, selection or unread activity, in
  the theme's text colour or its own `pill` ([themes](themes.md)).
  The server of a call this account is in, joining included, shows a white
  speaker on green at its icon's top right, so the call stays in sight from
  anywhere; a closed folder shows it for its servers. A Discord server
  with others in its voice channels shows one on grey, and hovering over
  it lists who is in which channel; see
  [Rail and sidebar](client/navigation.md) for what counts.
  Hovering shows the server's name. A right click on a server, or the Menu
  key or Shift+F10 on a focused one, opens its menu: Mark as read when any of
  its channels is unread, then Copy link and Copy ID. A Discord server's
  menu also has, while Discord is connected, Invite to server where the
  account may invite; Mute server, which mutes until it's turned back on
  when clicked itself, with the DM menu's times in its submenu, or Unmute
  server with when the mute ends;
  Notification settings, whose submenu picks All messages, Only @mentions
  or Nothing, with the level that applies under it, and ticks Suppress
  @everyone and @here and Suppress all role @mentions; Hide muted
  channels, a box to tick; Edit server profile, which opens the member
  menu's nickname dialog for one's own nickname there, where the account
  may change it; where the account may manage channels, Create channel
  and Create category; and, in red, Leave server, which asks first and
  isn't offered to the server's owner.
- A sidebar, 304px wide until its right edge is dragged (180px to 380px,
  leaving the conversation 420px; a double click on the edge gives it its
  own width back, and `layout.json` keeps the width as `sidebar_width`),
  with the
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
  channel gives, which their Discord profile fills in. A profile's name
  (unless it's drawn in a style) and its username line select, to copy,
  with a drag, a double click for a word or a triple for the line, and
  Ctrl+A and Ctrl+C; selecting there clears the timeline's selection, and
  a press elsewhere clears it. Its picture opens the full profile, as a
  styled name on the card does. Clicking someone's LIVE
  tag opens their channel's call and watches them. Outside that call it asks
  first, "Join <channel> to watch <name>'s stream?", or in another call the
  switch prompt asks, and the stream opens once the call connects and shows
  them sharing. Cancelling, leaving, a failed join or opening another room
  forgets it. A right click on a conversation, or
  the Menu key or Shift+F10 on a focused one, opens its menu: Mark as read when
  it's unread, Join voice for a voice channel you aren't in, Profile for a
  one-to-one DM, Pin or Unpin and Start call (Join call while people are in
  it) for a DM, then Copy link and Copy
  ID; a Discord channel's adds Invite to channel before Copy link, and
  where the account may manage it Edit channel, Duplicate channel, Create
  text (or voice) channel, which goes in its category, and Delete channel
  after it. A pinned
  DM stays at the top of the list, with a pin, on this device. A
  Discord DM's menu also has Add note, Add or Change friend nickname, Close DM,
  Remove friend, Ignore and Block, each undone by its opposite, and Mute, whose
  submenu mutes for 15 minutes, 1, 3, 8 or 24 hours, or until unmuted, and
  which clicked itself mutes until it's turned back on. A Discord server's
  text channel offers "Mute channel", with the same times and the same
  click, or "Unmute channel", and "Notification settings", whose submenu
  picks which messages notify; a right click on a category's heading offers
  the same for the category. A muted DM or Discord channel shows faint, and a
  Discord server set to hide muted
  channels leaves them out, but for the open one and those that mention you.
  Removing a friend and blocking ask first. Matrix links go through
  matrix.to. Arrow keys, Home and End move through the menu, Tab steps
  down and Shift+Tab up, Right opens a submenu and Left leaves it, Enter
  chooses, and Escape closes it.
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
  a text channel made opens. The dialogs are in
  `crates/yapfast-iced/src/server_dialogs.rs`, and editing in
  `channel_settings.rs` beside it.
- Menus (`crates/yapfast-iced/src/menus.rs`) are as wide as their rows'
  text needs, 224px at least, and each row may open a submenu of its own,
  beside the menu, or on its left near the window's right edge. A row can
  carry a second line under its text, such as the level a notification
  submenu has chosen, a check in place of its icon for the current choice,
  or a box to tick. Ticking leaves the menu open. A row can show greyed, as a
  role this account may not give does, and can't be chosen. A menu or
  submenu taller than the window scrolls, and the keys keep their row in
  sight. `place_menus.rs` and `person_menu.rs` build a menu's rows with each
  row's submenu, and `menus.rs` ticks a box when it's chosen.
- A dock across the rail and sidebar: the voice panel, whose signal icon
  and status show a Discord call's ping on hover and its connection details
  on a click (see [Connection details](discord/voice.md#connection-details)),
  and whose channel line under them opens the call, as Discord's does (see
  [The call's channel line](discord/voice.md#the-calls-channel-line)); then
  the account panel with avatar, status, mute and deafen during a call, and settings (account,
  theme, accessibility (text size, reduced motion and animations), audio devices, clock, link previews, folding long messages and peeking, notifications,
  sounds, reconnect, sign out). A caret beside mute and deafen opens a
  device menu (see [Persistent voice channels](voice.md)); the input menu
  shows a local level meter. While you speak in
  a call, unmuted, the panel's avatar has the same green ring as your row in
  the call.
- A conversation with a 50px header: the conversation's icon and name, and
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
  message ("Message #general"), and a send button; a reply attaches a bar
  above it. With no conversation selected, the area shows "Pick a
  conversation" and hides the composer. The local preview labels itself in a
  strip under the header.
- A 265px member sidebar right of the conversation, under its header, on the
  sidebar's tone. Its left edge drags it from 200px to 480px, leaving the
  conversation 420px, a double click on the edge gives it its own width
  back, and `layout.json` keeps the width as `members_width`. The member count in a group DM's header shows or hides it,
  as the people icon does in a server channel's, bright while the list
  shows, and Yapfast remembers a closed list.
- A right click on someone, in the member list, on a message author's avatar
  or name, or in the friends list, opens their menu: Profile, Message to open
  or start the DM with them, and on Discord Add friend, Accept friend request
  or Cancel friend request, then Copy ID. In a Discord server's channel it's
  their menu as a member, as Discord's: Mention, Call, a note ("Only visible
  to you" under Add note) and nickname,
  their roles in a submenu of checkboxes, and Time out, Kick and Ban last, in
  red, as this account's permissions and its rank against theirs allow, then
  Copy ID; see [Discord](discord/servers.md#a-server-members-menu). One's own
  has Edit server profile, which opens the profile editor on one's profile
  in that server. In a Discord
  server's call, or under a voice channel, their volume sits above that
  menu, which adds a Mute just for you, Move to and, in red, Server mute,
  Server deafen and Disconnect; see
  [Discord](discord/servers.md#someones-menu-in-a-voice-channel). With a
  voice channel's chat open, the member list's menu of someone in its call
  has the same boxes to tick and Disconnect, without the volume. Where Move
  to is offered, they can also be dragged onto another voice channel in the
  channel list; see
  [Discord](discord/servers.md#dragging-someone-to-another-voice-channel). Time out and Ban ask how long
  or how much in their dialog, and each asks a reason. In a one-to-one DM, the
  other person's menu is the DM's own, less Mark as read and Copy link, and
  Copy ID copies theirs. Clicking an author's avatar or name, which underlines while
  pointed at, shows their card beside it.
- Copy ID, in every menu that has it, shows only with Settings → Advanced →
  Developer Mode on, as in Discord, where the setting shows the same "Copy …
  ID" items. It's off unless turned on, and kept in `layout.json` on this
  device: Discord syncs it with its appearance settings, which Yapfast
  doesn't read.
- A voice stage with participant tiles, a green border while someone speaks,
  and round mute, deafen and leave controls.

DM Sans (SIL Open Font License, embedded in the binary; see
`docs/third-party/fonts.md`) sets all text in four weights and
an italic. The default theme is Bonfire; Yap uses layered graphite with a
marigold accent for selection and primary actions. See [themes](themes.md)
for the built-in palettes and custom themes. Avatars get one of eight
tints from the person's name.

Network operations run on the existing two-worker Tokio runtime. The UI
receives SDK events, and the message list virtualizes its rows. Rows keep their
place by message id: a change rebuilds only the rows of the messages it
changed, and older messages go in above the rows shown, which stay where they
are on screen. There is no UI polling loop. On Matrix, only joined Spaces and
their joined channels appear. DMs stay in the DM list even if linked to a
Space. Matrix standalone rooms outside Spaces, channel discovery, room
creation, threads, video and audio playback, and deleting other people's
messages remain later work.

## Validation

The Matrix chat integration tests in `crates/yapfast-core/src/matrix_chat.rs`
create disposable accounts and cover restoration, encryption and its gates,
Spaces, Markdown, replies, reactions, edits, deletion, attachments, drafts,
offline sending across a restart, and messages missed while a room was
closed. Run them after `scripts/test-stack up`:

```sh
nix develop path:./nix --command cargo test --locked --profile fast -p yapfast-core \
  --features matrix matrix_chat::tests -- --ignored --test-threads=1 \
  --skip fixture_conversation --skip opening_a_discord_channel_leaves_the_matrix_room_unread
```

The cross-network test also needs [fake Discord](../CONTRIBUTING.md#build-and-test).
With both services running and `YAPFAST_DISCORD_TEST_SERVER` set, replace
`matrix_chat::tests` and the two `--skip` options with the individual test
filter `opening_a_discord_channel_leaves_the_matrix_room_unread` before `--`.

To check the window by hand, see [Seeing the app](building.md#seeing-the-app).
The native file dialogs need an XDG portal, which `scripts/ui-session` doesn't
run, so Attach and Save are checked only through the integration test.

Every emoji is Twemoji's, on every platform, as Discord's client draws them:
the font `crates/yapfast-iced/build.rs` makes of Twemoji's pictures, each a
PNG in a CBDT table, is loaded as "Yapfast Twemoji" into a font system of the
client's own (`fonts::install` in `crates/yapfast-iced-kit/src/fonts.rs`),
which holds DM Sans, that font and the system's monospace face for code, and
none of the system's emoji fonts. DM Sans then Twemoji are the fallbacks
every text tries. So an emoji in any text, a name, a status, a reaction or
the composer's draft, draws Twemoji's picture, a flag, a keycap or a skin
tone joined into its one picture by the font's ligatures; symbols the UI
typeface lacks that have an emoji form, such as ♥, show as Twemoji's.

A character DM Sans and Twemoji lack, such as the styled letters of Unicode's
mathematical alphabets that Discord names use (𝓯𝓪𝓷𝓬𝔂), takes the system's
font for it, loaded the first time the shaper can't draw it (`char_fonts` in
`fonts.rs`): fontconfig's best match on Linux; on Windows, which has no
fontconfig, the first of Segoe UI, Segoe UI Symbol, Cambria Math, Segoe UI
Historic, Gadugi, Ebrima, Microsoft Himalaya, MS Gothic, SimSun-ExtB and
Segoe UI Emoji whose character map has it. A Han ideograph, kana or hangul
tries Microsoft YaHei, Microsoft JhengHei, Yu Gothic, Malgun Gothic and SimSun
before those: the font for the locale's language, such as Yu Gothic for
Japanese, lacks many of the others' ideographs, which showed as boxes beside
the ones it has (issue #192). No CJK font is bundled, and one is opened only
when a character falls through to it.
