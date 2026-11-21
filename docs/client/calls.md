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
  again. The tiles scroll when there are more than fit. Each tile is a
  `tile::Tile`: a camera, a stream or the avatar (a camera shows the
  avatar until its first frame, and keeps its last if frames stop), a green ring while the
  person speaks, their name and marks, LIVE and how many watch. A stream
  not watched offers Watch stream; a watched one shows large with Stop
  watching and the full-screen button under the pointer, and a double click
  goes full screen too, the stage over the whole window and the window
  full screen on the desktop until Escape, the button, a double click, or
  the tile or the call going (`call::fit_screen`, after every update). A
  window the desktop had full screen already stays so after. Closed to
  the tray meanwhile, the tile is back in the call and the next window
  opens as a window; brought forward while full screen, it stays so. A
  right click opens the person's voice menu.
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
- Go Live's menu shows screen and window previews two to a row, followed
  by the system dialog where supported and the available cameras. The
  preview worker exits when it finishes or the menu closes. The same
  scroll area holds audio, resolution, frame rate and encoding, with the
  action button always in view. Escape, a second click, another popup or
  leaving the call cancels a pending source listing. While live, audio
  changes apply immediately. Pick a source and select Switch to change
  the video.
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
  encoder, codec, size, bitrate sent and aimed for, keyframes asked for and
  packets resent; and how the call is secured. What they show is copied
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
- While the call isn't on screen, the stream watched, this account's
  screen or a camera stays in a corner of the window (`call/corner.rs`),
  its title and stop under the pointer; a click returns to the call, a
  drag moves it to the nearest corner.
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
