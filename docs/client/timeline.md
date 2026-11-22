# Header and timeline

The header (`header.rs`) shows a DM's avatar and
presence or a channel's icon, a thread's channel (which leads back to it),
the name, a channel's topic on one line after a dot, what a DM's person is
up to; then the room's buttons in this order (in a Discord DM with
someone who isn't a friend, Add Friend, or Accept Friend Request in the
accent when they asked; calls, threads, notifications, pins, add people,
the member list, the profile) and the search box.

The header's popovers (`header_popovers.rs`) open under
their button, its right edge level with theirs (each header button's
place is noted as it's pressed, `App::header_spot`), and close on Escape, a
click outside or what they're for; a click on a channel's topic shows it
in full under its line, its left edge level with the line's, with its
own line breaks, mentions and links, until Escape, a click outside or
another room; the core says which is open, and the
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

The search box (`search.rs`, `header_search`) is Discord's 244 by 32 px, in the
dark theme's measured inks; it names where it looks, widens
while it holds something and clears with its X; Enter searches the DM, or
the channel's server. What it found shows beside the conversation in the
member list's place (`search::view`), 420 px wide: how many, the newest
first as found cards, a server's under their channels' names, Show more
for the next page, a click jumping to the message in its room. Close,
Escape or clearing the box ends the search and lets go of the cards.

Over the timeline, and under the header of a voice channel's call
(`banners::view`), stacked from the top: the
preview's "Local preview: sample data" line, the note that the GPU
couldn't start, Matrix's verification while
the device isn't trusted, and the core's notice (`AppState::notice`) with
Dismiss.

The timeline (`timeline/`: its rows' looks, the view, scrolling and clocks each a file; one message's content in `message/`: its embeds, attachments, reaction chips and hover bar each a file) groups
messages as Discord does: the same person within seven minutes, unless
one replies, answers a command or is a notice. Each message's time is
worked out as its row is made, and again at midnight (a timer set for it
while a timeline shows), so today's become yesterday's. A new day's date and the
"New" divider above the first unread message sit on a line across; a
message that mentions this account (or replies to its message, while new)
is tinted marigold with an edge. The first row holds the history's edge:
"Earlier messages" (loading, or failed with a retry), the conversation's
start, or Discord's welcome at a server channel's start. Until a
conversation's first messages come, rows the shape of messages hold its
place, still (`timeline::view::messages_skeleton`), as they do in the
thread beside the channel. A short history
is held down over the composer. While the "New" divider is
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
components, and reactions. The author line has the name (in its role
colour, else the brightest white), the server tag, a green star for an
author who joined the server in the last week (`new_member`; not a bot, not
one who left and came back, and not in a server itself under a week old, as
Discord's client marks them), and, in a forum post, an "OP" tag for the
post's author unless the APP tag is there; a reply's line quotes the
author's tag a little smaller. A notice shows its icon in place of the avatar
and its words in Discord's wording. An app's answer still to come says
"{app} is thinking…", and "The application did not respond" once it's 15
minutes old.

The author's avatar and name open their card, and a right click on either
opens their menu, not the message's; the name is underlined under the
pointer, drawn by the widget with no message sent. On the line over an
app's answer, the person who used the command opens their card, and a
slash command, where one may write, is written in the composer
(`Action::SlashQuery`) with the keys there.

- Pictures (`media.rs`): one shows at its own size, several as square
  tiles two to a row, scaled down to the room there is. A picture's corners are 4 px round. A moving picture the star
  could favourite wears a "GIF" badge in its top left while it isn't
  playing (Appearance's animations off, or the window not in front) and isn't
  pointed at, covered or loading, as Discord's does, the star taking its place
  under the pointer. A spoiler's
  picture is blurred under a cover until clicked. Copies at a tile's size
  are made off the window's thread (`pictures::Fitter`) and dropped once
  no view shows them. On the message under the pointer each picture and
  video has Save in its top right corner, and a GIF under the pointer a
  star in its top left (iced's `hover`, so pointing at it costs no
  message) that adds it to the favourite GIFs or takes it out, asking for
  them as the pointer comes onto it. A link preview's pictures have
  neither; a video's poster there that doesn't play here (YouTube's) shows
  a play mark and opens the video's page. A thumbnail beside a card's
  text is pressed as a gallery's picture is (`attachments::pressed`). A
  picture clicked shows large (`image_viewer.rs`):
  the core's full picture once it
  comes, the preview until then at the size it opened at, fitted beside
  its bars; its name and place among the message's pictures, Save and
  Close above, round buttons and the arrow keys stepping through them, a
  note while the full one loads or why it can't; Escape, Close or a click
  outside closes it, and the core lets go of it. A right click on a
  picture or video, there or in the timeline, offers Copy Image, Save
  Image or Video, Copy Link and Open Link, after the message's own rows
  (alone when the message has none).
  Saving, from there or a file's card, asks where with the system's
  dialog, offering only the file's own name.
- A video (`playback.rs`) shows its poster with a play mark; clicked, it
  plays in its cell, its frames coming through the stream worker's shared
  mapping as a watched stream's do (`video::Sources::clip`). While the
  pointer is on it, or it's paused, a bar along its foot plays or pauses
  it, shows its time, moves through it and mutes it; a volume slider rises
  over the mute button under the pointer, the wheel over either turning it
  5 % a step (a touchpad by the part of a step it moved, a turn sideways
  not at all), and the last button shows it full screen in the viewer
  (`viewer::Viewer`), its name, Save and Exit full screen above it and the
  same bar below, its frames made at the viewer's size. Full screen it
  takes the keys (`playback::clip_key`): Space plays or pauses, Left and
  Right move five seconds, Escape brings it back; a right click on it
  opens the video's rows of the menu alone.
- Stickers show at 160 px, playing if they move (Lottie ones as the core
  renders them).
- Custom emoji are pictures in the text's line, larger when the message is
  only emoji; a copy gives back their `:name:`. An animated one plays
  while pictures play (`Ctx::playing_emoji`): in a message's text or reply
  quote in the timeline or the thread beside it, which hands it to the piece as a
  moving picture over its span (`Piece::moving`), in its reaction chip,
  and in the hover bar's and the menu's quick reactions when this
  account may use it there. Text in no timeline row (a bio, a topic, a
  found card, or the composer's reply bar) shows its still.
- A poll (`polls.rs`) lists its answers to pick, one or several, then
  Vote; once voted or ended it shows each answer's share with a bar, and
  Show results looks without voting. Its time left counts down. With the
  shares showing, a click on an answer opens the Reactions window on who
  voted for it (`reactions.rs`, "Votes", the answers down its left), and
  one on the votes so far opens it on the first answer. In a found card
  (`polls::found_view`) the poll is the same card that nothing is done
  in: its shares show once this account voted or it closed, and its foot
  says "View original message to interact", or the votes and "Poll
  closed"; a click on it goes to the card.
- A voice message (`playback.rs`) plays and pauses from its button; its
  waveform fills as it plays, and a press or a drag on it moves there
  (pausing while held); its time counts down. While the pointer is on it,
  or always with animations off, its speed and mute show; on the mute,
  the volume opens over it, an upright slider 120 px tall that the wheel
  turns 5 % a step, staying while the pointer is on either (a
  `widgets::Corner` made `free` to be wider than the button). Pressing
  the slider takes the keys from the fields, once a press; while it's open, keys no
  field took turn it (`playback::voice_key`: Up and Down 5 %, Page Up and
  Page Down 20 %) or close it (Escape). The core plays it.
- An app's components (`components.rs`): text beside a thumbnail or a button,
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
  reactions for Pin, Create Thread (where one may start and the message
  has none; not in the thread beside) and (one's own) Reply, and More for
  a Delete that doesn't ask, as Discord's client does, unless its menu or
  picker is open. The bar reaches over the row above; the pointer on it
  stays on its message. It stays while what it opened is open, its menu,
  its picker or the question whether to delete it, wherever the pointer
  goes (`hover_bar::kept`).
- A message's reaction chips (`message/reaction_chips.rs`) are 30 px
  tall and wrap onto as
  many lines as its width asks; after the last one, Discord's add
  button (34 by 30 px, a smiling face with a plus, its edge lit under the
  pointer) opens the reaction picker where a new reaction may be added. A click on one adds this account's
  reaction or takes it back. What the channel allows comes from the core
  (`reacts_in`): where it allows none, as while timed out, a chip takes
  no click; where it allows only reactions already there, as without Add
  Reactions, the hover bar, the menu and the `+` key offer no new one.
- A message's menu, on a right click or More: the
  four reactions used most, Add reaction, View Reactions, Edit, Reply,
  Forward, Copy text, Copy selection, End Poll Now (asked about in the poll's card), Pin or
  Unpin, Mark Unread, Copy Message Link, Delete and Copy Message ID.
  A right click on a link in its text adds Copy Link and Open Link, with
  the web address the link goes to, which a masked link's words don't say;
  a mention or one of Yapfast's own links adds none.
  View Reactions opens who reacted (`reactions.rs`): the reactions down
  the left, the most first, and the people with the one chosen, a page at
  a time as the list nears its end; one's own reaction, or as a moderator
  anyone's, comes off from its row.
  Forward opens Discord's dialog (`forward.rs`): up to five conversations
  the core finds, an optional message, the slowmode warning, Send.
- The keyboard steps through messages (`timeline/keys.rs`): Alt+Up from the
  composer lands on the newest, lit with its hover bar and a 3 px
  marigold edge down its left (`Lanes::keyed`); the arrows, Page
  Up and Down, Home and End step; Enter (or the Menu key, or Shift+F10)
  opens its menu, + reacts, R replies, E edits, Delete deletes, C copies,
  F forwards, Alt+Enter marks it unread, and Escape goes back to the
  composer. A press anywhere but on the list takes the keys from the
  messages too (`follow_press`, before each update, against the list's
  place as it last said it; while stepping, the window's press spot sends
  a message with each press, so one that sends nothing else is heard).
- A message from someone this account blocked shows as "Blocked user"
  and "Blocked message." with Show it, as Discord's does, until asked; with
  blocked messages hidden in Settings, the core leaves them out.
- A masked link (words that aren't where they go) asks first: Go back,
  or Visit site. The text, and its embeds', is laid out once per message (`markdown.rs`):
the core's blocks (headings, subtext, quotes, fenced code) apart, and in
each the inline styles as runs, with mentions of people, roles and
everyone on pills of their colour (a channel's and a notice's coloured text
in its colour alone),
links, and spoilers covered until clicked. A web address opens the browser; a
mention or a notice's name opens the person's card, and a role's mention
the role's card beside it (`info_cards.rs`):
the role in its colour, then who Yapfast has seen with it, each opening
their card over it, or "Loading..." until Discord answers; Escape or a
click outside closes their card first, then the role's. An emoji in a
message's text is a link too (a shortcode sent unexpanded, such as
`:sob:`, shows as its emoji), and a click on it, or on a message's
sticker, opens its card 4 px under it, or above it without room (worked
out by the core's `emoji::card`): the emoji or sticker at 48 px,
its name and what it is, and for one of a server's this account can reach
the server it's from, whose card asks Discord how many are online there
and what Discover says of it; a click on the server goes there. An emoji
of a server out of reach says "Invite-Only Server" and no more. A Discord link this
account can follow stays in the app, as the core's `discord_route` reads
it: an invite opens its Join dialog, an app's authorization, a gift and
an event their dialogs (see [dialogs](dialogs.md)), a server, channel or
message goes
there, and a person's opens their card.
