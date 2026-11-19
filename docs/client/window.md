# The window and the desktop

## Building and running

A plain build makes the `yapfast` executable: the window with Discord and
both its renderers, tiny-skia on the CPU and wgpu on the GPU.

```sh
nix develop path:./nix --command cargo build --profile fast
target/fast/yapfast --preview
```

With `--features matrix` it carries Matrix too, and signs in with Matrix
instead of linking Discord. `--data-dir`, `--preview`, `--stream-worker`,
`--background`, `--release-discord-links` and a `LINK` work in both.

The renderer is chosen once as Yapfast starts (`graphics::plan`):
Settings → Advanced → Render with the GPU, off by default, unless
`--renderer gpu|software` says otherwise for that start.
The window system allows one renderer per process, so a changed setting
applies from the next start, which Advanced's Restart Yapfast makes.
`renderer::choose` lets only the chosen renderer start (each vendored one
has a switch, as iced's own choice between them reads only
`ICED_BACKEND`), before any picture is made, as each takes pictures in its
own format: tiny-skia premultiplied BGRA, wgpu straight RGBA. A GPU that
can't start ends the window (`Exit::UseSoftware`); `main` starts Yapfast
again on the CPU with the reason, and that start says so in a banner
(over the conversation, the call, and the sign-in screen's card) and
turns the setting off. wgpu opens the Vulkan loader, `libvulkan.so.1`, as
the window opens; the dev shell and the Nix package put Nixpkgs' on the
library path. The tests draw with tiny-skia, as wgpu is off until chosen.

Each frame the CPU renderer presents tells the compositor only what
differs from the frame before (a scrolled list's part of the window, say),
so a compositor that copies the window's buffer into a texture on each
commit copies no more than that.

`YAPFAST_LOG=debug` also turns on the frame trace (`frames.rs`, log
target `yapfast::frames`): for each frame the CPU renderer presents, how
long it waited for a buffer, told what changed, drew and presented, the
area drawn and the area the compositor was told changed; and for each
frame a list's motion takes a step on, when that frame was asked for, how
late the step was taken, where the list was and how far it moved. Times
are microseconds since the window began, so a frame's lines line up. Off,
it reads no clock.

Under the private test display, `scripts/ui-session start --bin
target/fast/yapfast` runs a built binary on the preview, and with
`--discord` against the fake Discord. It reads the
window-control file (`YAPFAST_UI_TEST_CONTROL`, `control_file.rs`):
`stats` logs the allocator's figures, `trim` gives back
free heap, `hide` and `show` hide and show the window, `close` and `quit`
end it, `destroy` drops the window without giving its memory back,
`open` and `create` open it as the tray's Open Yapfast does, `theme ID`
chooses a theme and keeps it, and `stage`, `stage-aside` and `pick` go to
the core.

## The window and the desktop

The client runs as an iced daemon (`lib.rs`, `shell.rs`): it lives without
a window, so closing to the tray drops the window, its renderer, its
widgets and every picture made to draw (`pictures::Pictures`), and the
tray's Open Yapfast makes a new one at the size it had.
The window's size while it isn't maximized, and whether it was, are kept
in `layout.json` (`placement`, in the desktop's own units, so the zoom
doesn't change them) as either changes, and the next start opens with
them (`App::keep_placement`, `App::place_as_kept`); with none kept it
opens 1120 by 760. The test display's size (`YAPFAST_UI_TEST_SIZE`)
stands over what was kept, and nothing is kept of it.
Where the window was is kept with them on Windows alone
(`placement.position`, the frame's top left corner in the desktop's
pixels): Wayland doesn't say where a window is, and on X11 a monitor gone
since could leave the next one off screen, while Windows tells where the
monitors are. The desktop reads the place as the window opens, moves or
changes size (`Desktop::window_place`, `src/window_place.rs`), and says
none while it's maximized, minimized or full screen, so the place it goes
back to stands. A window with a place kept is made hidden, put there
(`Desktop::put_window`), maximized if it was, and only then shown, so it
never shows where Windows first made it. A place whose title bar would
show on no monitor, as after one was unplugged, is left for where Windows
made the window, and a window larger than its monitor's work area is
shrunk to it.
What iced doesn't do, the root crate's desktop modules do:
`src/desktop.rs` fills in the window's `Desktop` (`desktop.rs`) with the
tray, notifications, the single
instance, starting at sign-in, Discord's links, the clipboard's pictures,
updates, the window's icon and, on Wayland, files dragged over it. They
tell the window what they hear through a `Tell`, from threads of their
own, which a stream hands to the event loop as messages; nothing polls.

- The tray (`shell.rs`): its icon, with the unread dot for a mention or an
  unread DM, and Disconnect in its menu during a call; Open Yapfast, and a
  click on it, brings the window back with the click's activation token.
  Closing to it closes Settings as their own button does, and the call's
  popup with the microphone its level reads; the call's cameras make no
  frames and its clocks don't tick until the window is back.
  Closing the window goes to the tray when Close to tray is on and a tray
  shows the icon, and quits otherwise; with the tray gone, a closed window
  opens again. The window's memory is given back half a second after it
  closes. A window closed while earlier messages were read opens on them
  again: the row at the view's top is kept (`Timeline::left_at`), and the
  new window's list goes back to it as it first says where it is, before
  its end could count as read. `--background` starts in the tray with a saved session, as
  starting at sign-in does, unless there's no tray to start in or the
  start brings a link to open.
- The window's focus: the core hears each time it's gained or lost, and
  that there's none while Yapfast is in the tray. The open conversation
  counts as read only with its newest message in view and the window in
  front (`App::reads`), said with each of the timeline's reports and as
  the focus changes, so what comes while Yapfast is behind stays unread.
- New messages: a sound unless it's
  the conversation in front; while Yapfast isn't in front the taskbar
  flashes and a notification shows, as the settings allow. A click on the
  notification opens its conversation, closing Settings, and brings the
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
  Yapfast draws its own; elsewhere it takes the system's.
  Appearance's "Use the system title bar" switches, and the window is made
  again to take the change. The bar holds Back and Forward through the
  conversations visited (16 px arrows in 24 px boxes), the place's name
  (on the Friends page "Friends", with the people icon), the Inbox
  (Unreads and Mentions
  of the rooms the core knows, a click going there) and Help (18 px
  glyphs in 32 px boxes), and minimize,
  maximize and close, drawn as 10 px strokes the way Discord's frame draws
  them; its ground moves the window after a 3 px drag and a
  double click maximizes it, and its edges resize it. It goes while the
  window or a video is full screen. The content under it is laid out in
  the room below (`widgets::Below`), so overlays placed by the layout's
  sums or at the pointer needn't know it's there, and `App::window` is
  that room's size. Alt+Left and Alt+Right, and the mouse's back and
  forward buttons, go back and forward too, whichever bar there is.
- Zoom: Ctrl with + or - steps through the core's `zoom` levels, no
  further in than the window's room allows, and Ctrl+0 goes back to 100%;
  Ctrl and the wheel step too (a notch, or 60 px of a touchpad), taken
  round the whole window (`widgets::CtrlWheel`) so nothing under the
  pointer scrolls;
  the level is kept in `layout.json` and shows a moment at the top, also
  after a step that couldn't change it. iced
  scales the whole window by it. The window opens at its size in the
  desktop's pixels whatever the level, and a level kept on a larger screen
  starts no further in than the first window's room allows, the kept
  level staying for the larger screen. The level
  multiplies the display's own scale: on Windows the scale of the monitor
  the window is on, read as it opens, moves and changes size
  (`Desktop::monitor_scale`), since a window held to the primary display's
  density (Windows' HIGHDPIAWARE setting) names that density on every
  display and is told no change; elsewhere the window's own.
- Scrolling (`widgets::scroll_view`, every list that scrolls): a wheel's
  notch moves 60
  px over 180 ms, fast at first and slowing on Flutter's power curve; a
  notch while one moves adds what was left of it; a wheel spun past 12
  notches a second scrolls each notch farther, in proportion, at most six
  times, its recent notches weighing less by e every 200 ms. A touchpad
  moves the list with the fingers, less their first event and 8 px, and
  once they lift faster than 50 px a second it goes on at their speed
  (the last three moves' speeds blended 0.6, 0.35 and 0.05, oldest first)
  and slows to a stop on Android's fling spline. iced hands the wheel on
  without winit's phases: a notch comes as lines and a touchpad as
  pixels, the fingers lifting as a move of nothing, which is how the
  phases are told. What's inside a list has the wheel first (a field, a
  smaller list) and the list what's left, except while the fingers move
  it or while the pointer stays where it last scrolled it, for 800 ms. A
  press on it stops it; put somewhere (a jump, the end, another room) it
  drops its motion, and moved by (rows measured above what's read) it
  keeps on. Frames are asked for only while it moves. A list held to its
  foot (the timeline, the thread beside) counts the other way round. No
  rubber band at the ends.
- A list's scroll bar (`widgets/smooth/bar.rs`) is a thumb drawn over
  what the list holds, 2 px in from its right edge (its foot, for a list
  that scrolls across), and takes no room from it: iced's own bar is left
  without width. It's 4 px thick and faint; with the pointer over the
  list's last 12 px, or while it's dragged, it thickens to 8 px and its
  ink gets plainer, over 150 ms. A press on the thumb holds it where it's
  pressed, a press beside it brings its middle there, and it follows the
  pointer until the button lifts. It's never shorter than 24 px, and a
  list that holds less than a pixel more than it shows has none.
- Autoscroll (`widgets::Autoscroll`, round the whole window): a middle
  click (outside a text field, where it pastes) on a list that has more than
  fits scrolls it towards the pointer, still within 10 px of the click and
  faster the farther (8 px a second for each px past, more by 0.05 for
  each px squared, at most 12000), until the next press, a key or the
  wheel; held and moved away, until it's let go. The pointer's events go
  to nothing under it meanwhile, the press that ends it included, and a
  circle with arrows marks the click. It scrolls the list found at the
  press on each frame it asks for, and asks for frames only while the
  pointer is far enough from the click to scroll; the timeline and the thread
  beside, anchored to their foot, scroll the other way round.
- Updates (`updates.rs`, which runs with Settings closed; About shows
  it, `settings/updates.rs`): with the check on, a look at the
  releases at start and every six hours, and Check now on About; a newer
  release shows in About with its notes, Update and restart (which
  downloads and checks it, its progress shown and Cancel offered, then
  ends the window so `main` finishes it), View release and Skip this
  version, and as a banner under Settings' sections. A look nobody asked
  for tells of a newer release once in a notification that opens About.

## Theme

`theme.rs` holds the theme every widget paints with: one struct of the
theme's colours, which `themes.rs` fills from the
core's palettes (`yapfast_core::theme`, see [themes](../themes.md)), turned
into iced's colour type once (`themes::theme`), plus the colours
`Theme::worked_out` works out from them
(the menu's ground and outline, the composer's ground and placeholder, a
destructive row's text, the selection, a mention's colour) and iced's own
theme for the few styles iced draws itself. Views take it by reference;
nothing else in the client names a colour, but for Discord's own (the
crown, blurple, role and name colours, the nameplates' two grounds).

It's the theme `layout.json` names, with the person's custom themes from
the same file; with none it's Bonfire. Settings' Appearance chooses
another (see [settings](settings.md)), as does the
window-control command `theme ID`: either writes `theme` into
`layout.json`, leaving the rest as it was, and swaps the struct. A switch
costs no picture: the client's icons
and dots are drawn per colour and the ones of the old theme go, but
avatars, emoji and attachments are the same buffers; the names styled for
the other ground (dark or light) are made again as they show. Text laid out
with the theme's inks in it (mentions, a notice's names, the topic) is laid
out again in the new ones: the timeline's and the thread's rows at once,
keeping their heights, and found cards, pins and a forum's guidelines as
each is next taken in.
