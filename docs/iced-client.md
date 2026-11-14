# iced client

`crates/yapfast-iced` is a second desktop front end, written in
[iced](https://iced.rs) 0.14, which is to replace the Slint client once it
does everything Slint's does. Until then Slint's is the default and the
release, and the iced client is built on request. It drives `yapfast-core`
as the Slint client does, through `Action`s, `AppState`, effects and the
backend's events, so drafts, sending, the member list and voice behave the
same. What it shows so far: the server rail, a server's channels or the
DMs, the timeline with Markdown runs, pictures, reactions and their
tooltips, selection across messages, the composer with replies, edits and
mention suggestions, the emoji picker, a message's and the composer's
menus, Delete's confirmation, the member list with each member's looks,
the profile card, and a voice channel's call with cameras and a watched
stream.

## Building and running

The root package's `iced` feature builds the iced client into the
`yapfast` executable in place of Slint's, drawn on the CPU with tiny-skia:

```sh
nix develop path:./nix --command cargo build --profile fast --no-default-features --features iced
target/fast/yapfast --preview
```

`iced-gpu` draws with wgpu instead. The renderer is chosen when it's built:
a build has tiny-skia or wgpu, so `--renderer` only logs a warning when it
asks for the other. `--data-dir`, `--preview` and `--stream-worker` work as
in the default build; there is no tray, so `--background` shows the window,
and links given on the command line aren't opened yet.

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
end it, and `stage`, `stage-aside` and `pick` go to the core as the Slint
client sends them. The tray's and themes' commands are logged and
ignored.

## How it's built

iced builds the whole view again after every message and lays it out, so
what costs is kept out of the view:

- The backend's events come on its receiver, batched, as messages
  (`app.rs`); nothing polls. Fonts the shaper wants and the control file
  come as subscriptions.
- Lists are virtualized by hand: `widgets::Rows` holds only the rows near
  its viewport, each keyed by its id, so its widget state follows it as
  rows come and go.
- Pictures are the core's own buffers in tiny-skia's format, premultiplied
  BGRA (`pictures.rs`, `chat::set_pixel_buffers`), handed to iced without a
  copy. Copies at other sizes are made once and go when the core lets go of
  the picture (`Converted`).
- Moving pictures (`widgets::Animated`) advance inside their widget on the
  window's redraws, all on one clock, so a frame costs no message and no
  rebuild of the view; decoders of pictures no longer shown go after each
  view.
- Fonts: DM Sans, the packed Twemoji (the root `build.rs` makes it for this
  build too), the display-name fonts on first use, and for other scripts a
  system font loaded the first time a script is shaped (`fonts.rs`).
- Video (`video.rs`, `call.rs`): cameras' frames are made in the client's
  own memory (`picture::set_frame_buffers`) at their tile's size, and a
  watched stream's frames come from the stream worker's shared mapping.
  The renderer's hook (patch 13 below) copies or scales them into the
  window with libyuv. The core wakes the client for each camera frame made
  (`picture::set_frame_wake`); a watched stream is made in another process,
  which has no wake, so while one shows it's looked at every 8 ms.

Each part of the window has its module: `rail`, `sidebar`, `header`,
`timeline`, `composer`, `members`, `profile`, `picker`, `menus`, `call`.
`sync.rs` takes in what changed in the core's state after each message.
`widgets.rs`, `field.rs` (the composer's text field), `select.rs`
(selectable text) and `video.rs` are the client's own widgets. Overlays
(the card, suggestions, the picker, menus, the modal) are layers of one
stack, each over a backdrop that takes a click outside; a menu or the modal
takes the keys by moving focus to an id nothing has (`app::NOBODY`).

## The vendored toolkit

`vendor/iced_tiny_skia` and `vendor/iced_wgpu` are iced 0.14's renderers
with Yapfast's changes, patched in through `[patch.crates-io]`; the crate
pins iced to 0.14.0 to match. Their `README.yapfast.md` files list the
changes, and each is marked `Yapfast port` in the source. The tiny-skia
ones draw pictures from the app's own pixels, copy pictures and glyphs onto
the frame directly, clip by rectangles instead of masks, merge damage, and
draw video frames through the app's hook; the wgpu ones fix two shader
mistakes and upload large pictures in the frame.

## Tests

`cargo test -p yapfast-iced` runs the crate's tests, among them `tests.rs`,
which drives the window through `iced_test`'s headless simulator on the
core's preview: it builds the view, clicks and types into it, and hands the
messages that come out to `App::update`. They find widgets by their text or
their id, as iced has no accessible labels yet; the client's own text
widgets report their text for this. The tasks `update` returns (focus, the
clipboard, timers) don't run in them, and the simulator keeps widget state
only while it lives, so a field is clicked in the same simulator it's typed
into. `scripts/check` lints both renderers' builds, runs the tests, and
builds and smoke-tests the CPU build; see [building](building.md#checks).

## Not there yet

The theme (the client draws in the default dark colours), sign-in (it
starts on a saved session or the preview), settings, threads, the full
profile, uploads, voice's controls beyond Leave, the tray, notifications,
links, accessibility and Windows' own checks.
