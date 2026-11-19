# People and sign-in

## People

A person's row is built from `people.rs`: their face (avatar, decoration,
presence), their name in its style (`NameInk`), and a line under it. The
member list, the DMs, and the lists still to come build on it.

- The member list (`members.rs`) shows role groups with
  their counts, the offline section, each member's decoration, nameplate,
  name style, server tag, role icon, boosting gem, timeout clock and
  activity line; the nameplate and the decoration play under the pointer
  or while their card is open. Over the list, a server's Activity panel
  shows the first three cards of Discord's activity feed (what each person
  plays, listens to or watches, and since when); its eye hides the cards,
  kept in `layout.json`, and its arrow opens the whole feed. Under it a
  note says the list is loading, why it failed, or that no one else has
  joined. Hidden (its toggle off, a thread or search in its place), the
  list keeps no lines, cards or pictures and builds none as the roster
  changes; shown again, it's built as the roster is then.
- A DM's row (`sidebar/dms.rs`) shows the person's face, styled name and server
  tag (OFFICIAL on Discord's own), the voice channel they're in (a click
  goes to it, in its server) or what they're up to, a group's members, and its
  nameplate under the pointer or while open, filling the row to its edges and
  6 px corners; a new DM offers Wave, which sends 👋.
- A right click on a member, a DM or a message's author opens the person's
  menu (`person_menu.rs`), built from the core's `person_choices`:
  Profile, Mention, Message, Call, Add Note,
  the friend and block choices, Mute, Timeout, Kick and Ban by name, Pin,
  and Copy ID, each row with its icon (`place_menus::icon`, one for a
  choice in every menu that offers it). Roles, Invite to server, Mute and Apps open submenus built
  as the menu opens: the server's roles this account may give or take
  (ticked where held, each tick sent at once with the menu kept open), the
  servers to invite them to, how long to mute their DM, and the open
  room's user commands with their apps' names, which the menu's opening
  asks the core to fetch. A choice that asks first (remove friend, block, kick, ban,
  timeout and its length, a nickname, a note) opens the person dialog,
  with its options, field and "Don't ask again", and sends the
  answer to the core.
- Profiles (`profile/`): `look.rs` works out what a profile shows once
  each time the core's view of it changes (its theme's inks, the status's
  bubble, activities, the bio laid out, the mutual line, the tabs). The
  card (`body.rs`) shows the banner (moving
  ones play; with no banner, accent or theme, the avatar's dominant colour,
  as the core's `dominant_colour` works it out), the avatar on its ring with its decoration and presence, the
  status's bubble, Add Friend or Friends and More over it, the
  name drawn once in its style (on the card, a click on it opens the full
  profile, as the avatar does), the username, pronouns and server tag,
  the plain name and the username line selectable, to copy
  (`widgets::Selectable`; Ctrl+C sends `Event::Copy`, which the clipboard
  takes, and a press on them clears the timeline's selection),
  badges twelve to a row (beside the username, up to six on the floating
  card and any number in the DM's panel; more on the floating card go
  under the name, 2 px below it; their tooltips after half a second's
  rest; a
  badge that levels up has Discord's card, its level's picture over the
  kind, the level and the progress), the activities with their times (on the
  floating card each with Discord's more menu: View All Activity opens the
  full profile on Activity, and one's own game's Manage Privacy opens Game
  Activity's settings), the mutual line
  (which opens the full profile on its tab; in the docked card with no
  mutual friend it leads with the first three mutual servers' icons, 16 px
  and overlapping, then the servers' count), the bio cut to three lines
  with View full bio, the roles, and a box that messages them. The full
  profile (`full.rs`) has the card down its left, with Friends, Message and
  More under the name, Member Since, Friends Since, the roles, connections
  and the note (saved on Enter, and as the profile closes, grows or goes
  to another person; Escape puts back what was kept), and its tabs on the
  right: Board, Activity (with recent activity), Wishlist, Mutual Friends and
  Mutual Servers (a server clicked shows its card over the profile:
  its banner and icon, name, members and
  channels, their nickname there, and Go to server), the tab settled as
  the profile opens or goes to another person, so a Board that comes with
  Discord's answer doesn't take over, rows the shape of
  what's coming while Discord hasn't said, and Discord's words while a tab is empty. A one-to-one DM has the
  card docked beside it, with what the DM list knows of them now, and
  View Full Profile at its foot, a 40 px button; the docked card of a
  private profile (not this account's own) has a "Private Profile" bar, 32
  px, over its banner in the profile's colour, mostly dark, and a note under
  the mutual line that the profile is private and a friend would see more;
  its Member Since has no Discord logo and its mutual line is the faded
  ink; the header's profile button shows or
  hides it, kept in `layout.json`. Activities' times and a song's bar
  move each second as they're drawn. A profile's effect (`cosmetic.rs`)
  lies over the card, the docked one and the full
  profile's left: its still at rest, and while the pointer is on the card
  and moving pictures play, its layers each from its start for its
  duration, again after its delay when it loops, until the last that
  doesn't loop ends. Its clock asks for a tick only when a layer shows or
  goes, and its layers are only asked for under the pointer. A profile's
  frame (`widgets::Framed`) lies
  round the card, 12 px past its sides, and the full profile's, 16 px past
  them with its top staple 8 px lower and none at the foot, in a window
  over 785 px tall; the docked card has none. Settings' cosmetics switches
  turn each off.
- A server tag on a message or a profile card (`tag_card.rs`) opens a
  card about its server beside it, 300 px
  wide: the server's icon, name and tag at once where this account is in
  it, then what `GET /guilds/{id}/profile` says (online and members,
  "Est." and the month, the description), Discord's words for a private
  or unknown server with a retry, and Go to Server, which goes there.
  Escape or a click outside closes it. See
  [server tags](../discord/people.md#profiles).
- The profile editor (`profile/editor/`: the card, its menus, the colours
  and the styles column each a file)
  opens from one's own card and full profile (Edit
  Profile), from Settings, which close under it, and from a server's Edit
  server profile and one's own member menu. Its card is a profile view of what's written over what Discord
  has, drawn with the card's own
  parts (a moving banner plays; with no banner, accent or theme, the
  avatar's colour lies behind it) and made again only when the core's
  editor changes; a picture
  just chosen shows from the core's preview of it. Each text is edited in
  place: the name and pronouns turn into fields when clicked, the bio into
  a plain text box of its own (Enter breaks a line) with what's left of
  Discord's 190 characters, and a click on the card that nothing on it
  takes ends the editing; the avatar and banner offer Change and Remove
  (Reset in a server) once there's one to remove, else open the file
  dialog, and without Nitro the main banner opens its colour, with
  swatches and "Not a colour". The main profile has the server tag's
  picker under the name, Message, the shop and More (Copy User ID), the
  custom status's thought (which opens its dialog), Connections and the
  note; down its left what it wears (`Main Profile`), which Hide Styles
  folds away, with the account's nameplate on its tile (still, the avatar
  and name over it), the avatar and banner tiles, the theme's two colours
  (Nitro) as handles that open their pickers (`widgets::ColourPad`:
  saturation across and brightness down a
  square of the hue, the hues under it, and the code, each following the
  others), the effect worn as its still
  over a little card, and the shop's for the rest; with Nitro the
  Display Name Style tile opens its window (`profile/name_style.rs`):
  the fonts' tiles (each one's name
  over it under the pointer), the effects' (each name drawn in the
  effect), and the colours: the effect's presets (gummy's as stripes),
  one's own colour on the eyedropper's pad, or for a gradient and a prism
  a bar with a handle for each colour, each opening the pad (the same
  `form::colour_card` as a theme colour's) over the row; beside them how
  it looks on the card, a message (its name plain, in the style's font)
  and a member row, those two in Discord's light mode or dark, and the
  note. Surprise Me picks at random; Apply, once something changed, hands
  the style to the core with the other edits, and a click on the dim
  closes it. Escape closes the pad, then the window. The main profile
  shows its Board, Activity and Wishlist tabs where the card is 880 px or
  wider, in the editor's words ("Your Widgets", "N Items", "Current
  activity", that adding more from Yapfast is coming soon, and the board's
  note while the profile loads or after it failed to). A server's has what it edits over its banner and Nitro's offer
  over what takes it. Once something changed the form kit's
  `Unsaved` bar offers Reset and Save Changes, which waits while a text is
  too long or a picture is read. Escape ends the editing, closes what's
  open, then the editor.
- The friends list (`friends/`, the Requests screen in `friends/requests.rs`), from Friends on the DM home: the
  header's Online, All and Pending tabs (Pending goes to Requests), Add Friend, which
  takes the list's place with Discord's Add Friend page (below), and at its end a button for a
  new group DM, which opens the same chooser as a new message; under it,
  once some account is blocked or ignored, a note that they are in the
  settings ("Go to Settings"; closed by its x for the session), a search box with a magnifier, a heading
  ("Online" or "All Friends", an em dash and the count), and each friend
  with their presence, voice channel (a click goes to it) and status,
  their avatar decoration (animated while pointed at), ruled apart from 6 px in to
  20 px short of the right, with a message button and a kebab that opens
  their menu; a click opens their DM, a right click their menu. Only
  the rows in view are built. With the conversation at least 1000 px wide,
  Active Now (`active_now.rs`) stands beside it: friends together in a
  voice channel, in one game party, or each doing something Discord can
  name, a card each, made as Discord's `NowPlayingViewStore` makes them,
  only while the list shows and has that room (narrower, none are made,
  timed or fetched for; the Add Friend page has none); a card's head
  opens its person's menu, a voice channel's row the channel. Its column
  is a shade darker than the conversation, each card 16 px round with a
  lighter head, the person's status dot on their avatar, no rules between
  rows, rows at least 83 px tall and a game's icon 24 px.
- Requests: friend requests and message requests, a person's on one row
  with Accept and Ignore (or Cancel for those sent) and their latest
  message, which opens the conversation, with its time today on the clock
  Settings choose, else its day; then those sent, and spam folded
  away. The Requests row counts a person's two requests once.
- Add Friend's page, on the friends list, and its dialog, anywhere else
  one is asked for, send a request by username and say how it went (Send
  Friend Request, in the same box as the field on the page, "Sent {name} a
  friend request." or Discord's words).
- Menus opened by a button rather than a right click open where it was
  pressed: the window notes each press's point (`RightPress::spot`).
- Escape closes a dialog whose field has the keys (Add Friend, a person's
  question, Forward), as the field alone would only let go of them.

## Sign-in

Without a saved account the window opens on the sign-in screen
(`signin.rs`): on the
left the network's sign-in, on the right "Just looking?" with the preview.
It starts on a saved account when the core can restore one (a Discord
login without Matrix, Matrix's `active.json` with it), so no notice shows
for one it can't. While that account is restored the window shows only
the core's notice ("Connecting to Discord..."), not the card: the card
would show for a moment at each start, and its
text, at sizes the workspace doesn't use, would stay in the renderer's
glyph cache. The card shows if the restore fails.

- Discord, in a build without Matrix: "Show the code" asks the core to link
  (`Action::LinkDiscord`); the client runs `discord_link` on the runtime and
  hands each step to the core, and the screen shows the QR code (drawn once
  per code, whole pixels per module), who scanned it, the captcha's notice
  with "Open it again", or the error with "Try again". "Use a token" opens
  the field instead; Link checks it with Discord. A refused token stays
  to fix; once the account is linked it's let go, its field closed. A
  linked account starts the workspace. The screen's secrets (token,
  password, recovery key) are overwritten as they go and never show in a
  log line of the screen or what's typed.
- Matrix, with `matrix`: the homeserver, Matrix ID and password go to the
  core's `Login`; the password is let go once it's sent. The Matrix ID
  takes the keys as the window opens and each time the screen comes back
  with no account (the core's `reset_version` moved: signed out, the
  preview left). "Retry saved
  session" shows when there's one to retry. Until this device is verified,
  a banner over the timeline takes a recovery key ("Recover") or checks
  again after verifying from another device.
- Discord's two-factor prompt (`mfa.rs`):
  for a request Discord holds back, with an authenticator's code, a text,
  the password or a backup code, the warning before a backup code and its
  ten seconds (one timer, only while the warning shows), and "Verify with
  something else".

Pages the core asks to open (the captcha) open in the browser
(`browser.rs`), never from tests or the private test display.
