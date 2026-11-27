# Tests

`cargo test -p yapfast-iced-kit` runs the kit's own tests, with a message
of their own (`testing::Message`) and without the core.
`cargo test -p yapfast-iced` runs the client's.

## The window's tests

`crates/yapfast-iced/src/tests/` drives the window through `iced_test`'s
headless simulator on the core's preview: a test builds the view, clicks and
types into it, and hands the messages that come out to `App::update`.

Each file there holds one area's tests and says at its top what they cover:
`navigation.rs` the rail and the sidebar, `timeline.rs` a channel's
messages, `writing.rs` the composer, `shell.rs` the window's frame, the tray
and notifications, and so on. A new test goes in its area's file; a new
area gets a file and a `mod` line in `mod.rs`. `mod.rs` holds what they
share and no tests: `preview` and `start` make the window, `interact` runs a
simulator over it, and `shows`, `present`, `last` and `right_click` find and
click what it draws.

What to know when writing one:

- Tests find widgets by their text or their id, as iced has no accessible
  labels yet. The client's own text widgets report their text as it shows:
  an elided one's start and ellipsis, a folded message without its cut-off
  parts.
- `shows` counts a text only inside the window and the lists it's in, and
  not under a dimmed layer (`controls::DIMMED`). `present` counts any in the
  tree. A tooltip, which iced's operations don't reach, is told by what
  hovering draws (`hover_draws`).
- The tasks `update` returns (focus, the clipboard, timers) don't run, and
  the core's meters open no microphone (its `test-fixtures`).
- Each test starts as the window opens. The simulator keeps widget state
  only while it lives, so a field is clicked in the same simulator it's
  typed into.
- Server tests run on the Discord server `tests::server::discord_server`
  builds, with the permissions the test names.
- Emoji draw from the Twemoji font the window carries (`tests::twemoji`),
  which the app's `build.rs` packs from `assets/twemoji` with
  `emoji_font_tables.rs`, so the emoji tests always run.
- A test that reads the process's call volumes back holds them
  (`layout::tests::hold_volumes`), as every window plays its own as it
  opens.

## Tests beside the code

What needs no window is tested in its own module, at its end: `markdown.rs`
Discord's Markdown, `call/stage/layout.rs` the tiles' layout and the pixels
a tile's frames are made at, `call/dock.rs` the call's time,
`image_viewer.rs` the fit, `timeline/rows.rs` the rows, `themes.rs` the
theme a layout names.

`scripts/check` lints the build, runs the tests, and builds and smoke-tests
it; see [building](../building.md#checks).
