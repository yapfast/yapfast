# Calls

The call's views and what they do are in `call/`.

- A voice channel this account isn't in shows its name, who is there and
  Join voice (Switch voice channel while in another call), or why it can't
  be joined. A click on the channel in the sidebar joins it without
  showing it, as Discord's client does; a second click, or the dock's
  channel line, shows the call.
- The stage (`call/stage/`; `layout.rs` places the tiles with no window
  in it)
  lays the tiles out in a grid, or one large with the rest in a row under
  it; a click on a tile makes it large, and a click beside it the grid
  again. While a tile is large beside others, a pill on the strip's top
  edge (Hide Members) hides the strip under it, so its cameras stop
  decoding and the large tile fills the stage's height; the pill then sits
  just over the controls (Show Members) to bring it back. Each voice
  channel keeps its choice while Yapfast runs, through leaving and joining
  again, as Discord's client keeps it. The stage takes the height left
  between the line over it and the controls, laid out where it is drawn
  (`responsive`), so a banner over the call (the preview's line, a notice)
  never pushes the tiles under the controls; the size it last had is kept
  (`Call::stage_seen`) for placing the cameras' frames. Everyone fits while
  tiles stay 120 px wide, and the tiles scroll past that. On a tile under
  200 px tall the avatar sits above the name's chip, where its neighbours'
  do, and Watch stream goes on the chip's line opposite it, or as its eye
  icon alone where both don't fit. Each tile is a
  `tile::Tile`: a camera, a stream or the avatar (a camera shows the
  avatar until its first frame, and keeps its last if frames stop), a green ring while the
  person speaks, their name and marks, LIVE and how many watch. A stream
  not watched offers Watch stream, and while another is watched the eye
  with a plus beside it (Start watching multiple streams), which watches
  it beside the others; a plain click watches it in place of the one
  watched, unless two or more are, and Shift and a click beside them
  ([Go Live](../discord/go-live.md)). Each watched stream keeps its own
  tile, with its own Stop watching under the pointer, and its pictures are
  looked for only while the tile is in sight. A watched one shows large
  with Stop
  watching and the full-screen button under the pointer, and a double click
  goes full screen too, the stage over the whole window and the window
  full screen on the desktop until Escape, the button, a double click, or
  the tile or the call going (`call::fit_screen`, after every update). A
  window the desktop had full screen already stays so after. Closed to
  the tray meanwhile, the tile is back in the call and the next window
  opens as a window; brought forward while full screen, it stays so. A
  right click opens the person's voice menu.
- Participants whose speaking permission Discord denies show a red blocked
  microphone. The local microphone control shows the restriction in both
  the account panel and the call stage.
- The controls under the stage, in this order: mute and
  deafen (each with its device menu), the camera, Go Live, the soundboard,
  and leave.
- The device menus (`call/popups/`: the device menus, the share menu,
  the soundboard and someone's voice menu each a file): the device to use (one chosen that
  went away shows as not connected), its volume, which plays while it's
  dragged and is kept in `layout.json` when let go, and on the input side
  the microphone's level, measured only while the menu shows and read
  as it's drawn, and the
  noise suppression kind.
- Go Live's menu (`call/popups/share.rs`) lists what to share: screen and
  window previews two to a row, the system dialog where supported and the
  available cameras, which scroll past two rows of previews. The preview
  worker exits when it finishes or the menu closes. Under them, always in
  view, each setting is one row that names what's chosen (Audio,
  Resolution, Frame rate), then Higher quality's checkbox and the action
  button. The pointer on a setting's row, or a click, opens its choices
  beside the menu, level with the row, the one chosen ticked: a context
  menu's submenu, placed by the same `Place::beside`, on the right or the
  left without room. The menu neither moves nor changes size for them.
  One list shows at a time; a choice is taken and its list closes, as it
  does with the pointer on Higher quality, a source picked, or Escape,
  which closes the menu only after. The audio's list scrolls where many
  windows are open. The audio chosen is the call's at once and is kept
  for the next start; on Windows an application is kept by its program
  and found again by a window it has as the menu opens, and everything
  is shared while it isn't running
  ([Go Live](../discord/go-live.md)). While live only Audio has a row,
  and its changes apply immediately; pick a source and select Switch to
  change the video. Escape, a second click, another popup or leaving the
  call cancels a pending source listing.
- The soundboard lists the server's sounds and the defaults in rows of
  three, each played on a click, previewed from its speaker under the
  pointer and starred as a favourite.
- Someone's voice menu (`menus.rs` with the person's choices) has their
  user and stream volumes above its rows, which keep the core's order
  (the person menu's sorts them), and checkboxes for muting them
  for this account, muting their soundboard, and on Discord the server's
  mute and deafen; one's own has mute and deafen. Move to opens the voice
  channels they may be moved to (this account moves by joining). Volumes
  play while dragged and are kept when let go, as are those muted.
- The dock (`call/dock.rs`) at the sidebar's foot: the connection's state
  and the call's time, the channel line (underlined under the pointer)
  that shows the call, noise suppression's toggle, which turns back on the
  kind last used, leave, and while sharing its state with a stop; the
  soundboard, camera and Go Live buttons; an error with Dismiss.
- The connection's details (`call/details.rs`): once the call measures
  its connection, its signal (the
  dock's, and the header's in the call's conversation) shows the last ping
  as its tooltip, and a click on it or the dock's status opens the details
  under it, or above without room: Connection, a graph of the last five
  minutes of pings with its top and middle marked, the average and last
  ping and the packets lost; Privacy, while the call is end-to-end
  encrypted with someone else in it, the voice privacy code in groups of
  five with Copy; Stream, while this account's screen is live, its
  encoder, codec, what's captured, size, bitrate sent and aimed for,
  keyframes asked for and packets sent and resent, and while it watches
  streams, the first one's decoder, codec, size, bitrate received, packets lost and
  resent and frames dropped, with "Copy diagnostics" for all of it as text;
  and how the call is secured. What they show is copied
  out of the call only while they or the tooltip show, again each second.
  Their words and graph are the core's `call_details`.
- A DM's call (`call/dm.rs`) shows above its messages, taking more room
  while a tile is large: the stage and controls while in it, else who is
  in it with Join the call, or Accept and Decline while it rings. A call
  ringing in a DM that isn't open shows a card at the window's top, its
  ring round the caller pulsing once a second (`call::dm::Pulse`, drawn
  anew 30 times a second only while it rings), and
  switching calls asks first (Escape cancels, Enter confirms). The DM's
  header starts a call or a video call, its buttons greyed while the call
  can't be joined (Discord offline, another call joining or leaving).
  Settings → Sounds can play files of the user's own for the ring and the
  ringback.
- While the call isn't on screen, the stream watched last, this account's
  screen or a camera stays in a corner of the window (`call/corner.rs`),
  its title and stop under the pointer; a click returns to the call, a
  drag moves it to the nearest corner. A drag at its grip, the corner of
  it away from the window's, where the pointer takes the shape that
  resizes, makes it larger or smaller, its 16:9 shape kept, as Discord's
  picture in picture: the width follows the pointer across only, 320 px
  at first, between 200 and 960 px (20 past while dragged) and within the
  window's room, and settles on the nearest 40 px when let go. The grip
  takes its press before the stop button under it (the picture sat top
  left). The width is kept in `layout.json` (`corner_width`), narrowed
  while the window has less room, and its frames are made at it.
- Sounds play as the core says (joining, leaving, muting, someone
  streaming), and a DM's call rings while it's incoming.
- Push to talk and the other keybinds (the root `src/keybinds.rs`,
  started by `keybinds.rs`, which runs with Settings closed): the
  keybinds kept in `layout.json` are watched through the
  desktop portal's global shortcuts, X11 or Windows' hooks, beside the
  window. In a call, unmuted, the last key of a push to talk keybind
  (told by its scan code) types nothing into the composer it would go
  to; every other key types as ever, held or not.

Only what shows takes frames: a tile's video is mapped while its tile is
in view or in the corner, at the size it shows in the window's pixels
(the display's scale, as the window says it as it opens and moves, times
the zoom), as a message's video and the camera's preview in Settings
are; `Video` lays a frame out in those pixels, so one made at its tile's
size is copied one to one.
