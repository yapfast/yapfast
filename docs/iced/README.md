# The iced interface

Anomi is getting a second interface, written with [iced](https://github.com/iced-rs/iced)
beside the Slint one, on the same core: `crates/yapfast-core` and
`crates/yapfast-discord` stay as upstream has them, so upstream's fixes keep
merging. The iced interface grows screen by screen until it does everything
the Slint one does; then the Slint one goes. This page is the architecture
for the agents who write it (writers) and those who check it (reviewers).
The iced API at the pinned commit is in [iced-notes.md](iced-notes.md); the
work, task by task, is in [tasks.md](tasks.md); the widgets, view-model types
and fixtures waves 1 to 3 built, and the pieces the screens share, with
their entry points, in [widgets.md](widgets.md).

Run it against the fake Discord, as the Slint app runs:

```sh
scripts/ui-session start --discord --anomi --size 1400x860
scripts/ui-session do 'click 150 281' 'theme catppuccin-latte' 'language en' 'shot dm'
```

## Crates and layers

```mermaid
flowchart BT
    theme[anomi-theme<br/>palettes, sizes, type, motion, styles]
    vm[anomi-vm<br/>view-models and intents]
    widgets[anomi-widgets<br/>our own widgets]
    screens[anomi-screens and the area crates<br/>views of the view-models]
    shots[anomi-shots<br/>headless PNGs]
    app[anomi-app<br/>state, update, projection, the binary]
    desktop[anomi-desktop<br/>tray, one instance, memory]
    core[yapfast-core + yapfast-discord]
    widgets --> theme
    widgets --> vm
    screens --> widgets
    shots --> screens
    app --> screens
    app --> core
    app --> desktop
```

| Crate | Holds | Depends on | Who edits it |
| --- | --- | --- | --- |
| `anomi-theme` | the twelve palettes (`builtin/`), `Theme` and every built-in widget's style catalog (`catalog/`), `metrics`, `typography`, `motion`, `fonts` (DM Sans, and `install()`, which puts every font in iced's font system with the fallback the interface wants), the `View` and `Element` types | iced, anomi-fonts | writers |
| `anomi-vm` | per area: the view-model a screen draws and the `Intent` it sends, the larger areas in folders (`sidebar/`, `places/`, `chat/`, `people/`, `voice/`, `settings/`); `kind` for shared vocabulary (avatar, presence, tag, status, name style and nameplate, profile effect and frame); `icon` (`Icon`); `text` for spans; `menu`, `shell` (layers); `fixtures` (feature) for samples, the fake's pictures and `Shot` | iced (for `image::Handle`, `svg::Handle`), pulldown-cmark, emojis | writers |
| `anomi-widgets` | one module per widget ([widgets.md](widgets.md)): compositions (`avatar`, `badge`) and widgets of our own (`list`, `picture`, `paragraph`, `composer`, `layer`, `menu`...) | theme, vm, yapfast-lang, image (PNG) | writers; the hard widgets are Opus tasks |
| `anomi-screens` | the window (`shell`), its chrome and its areas, one folder per area, one module per component; `shots` (feature) lists what `anomi-shots` renders | theme, widgets, vm, yapfast-lang, the area crates | writers |
| `anomi-chat`, `anomi-people`, `anomi-voice`, `anomi-settings` | the larger areas, split off so each compiles on its own: one module per task, drawing `anomi_vm::<area>`; the shell places them. They don't depend on each other | theme, widgets, vm, yapfast-lang | writers |
| `anomi-fonts` | Twemoji as a colour font (its build script makes it from `vendor/twemoji/72x72`, cached in `OUT_DIR`), the eleven display-name faces, which fonts draw Chinese, Japanese and Korean first, and the person's own font files (copies, WOFF); no iced | emojis, unicode-script, yapfast-lang, sha2, tempfile, memmap2, flate2 | F03 |
| `anomi-shots` | renders each screen from its fixture to PNG, no display, with the app's fonts and picture scale | screens, widgets, iced_runtime, png | reviewers run it |
| `anomi-desktop` | what both clients ask of the desktop beyond their toolkit: the tray icon, one instance a data folder and bringing its window forward, the window's icon, giving memory back; the Slint app (`src/`) uses it too | image, raw-window-handle, ksni (Linux), wayland-client (Linux), windows-sys (Windows), yapfast-lang | I14 and the I tasks after it |
| `anomi-app` | `App` (the program's state), `update`, the projection of the core's `AppState` (`project/`), intents to `Action`s (`intent/`), effects to tasks (`effects`), the backend, `layout.json`, the `anomi` binary | everything above and yapfast-core with `discord` | Opus integration tasks, never writers |

The rule that makes this work: **nothing a writer touches depends on
yapfast-core**. The core's `livekit` and `webrtc-sys` aren't optional and
its `discord` feature builds x264, FFmpeg and thorvg; any crate above it pays
for that in every `cargo check`. So screens draw view-models, plain data the
app fills from the core, as `src/ui/refresh.rs` fills the Slint window
today. A writer's check builds iced and nothing native.

Every crate takes iced from one entry in the root `Cargo.toml`
(`[workspace.dependencies] iced`), so the whole graph builds once with one
feature set; never give a crate iced features of its own.

## Modules, files and names

- **One folder per area, one module per component.** An area is a part of
  the window with its own Slint file: `rail`, `sidebar`, `conversation`,
  `composer`, `friends`, `voice`... The same area has the same name in
  `anomi-vm/src/<area>`, the screens crate's `src/<area>` and
  `anomi-app/src/{project,intent}/<area>`.
- **Name things after the Slint side**, in Rust's case: `RailItem` becomes
  `rail_item::RailItem`, the struct `RoomRow` becomes `sidebar::DmRow` or
  `ChannelRow` as it splits. Each module's doc names the Slint component and
  the glue (`file:line`) it ports, so a reviewer can hold them side by side.
- **400 lines a file at most**, tests included; `scripts/check-iced` fails
  a longer one. Split by component, not by kind of code.
- **No colour, size or duration literal in a screen**: they come from
  `anomi-theme` (`theme.palette()`, `metrics`, `typography`, `motion`). A
  value one widget needs and nothing else may stay in that widget's module
  as a named constant with the Slint line it came from.
- Text the person reads goes through `tr!` (see [Translation](#translation)).
- Every module starts with `use anomi_theme::prelude::*;`, which brings
  `View`, `Element`, `Theme`, `Motion` and iced's `Widget` trait (for
  `.boxed()` and `.map()`).

### Shared files

Writers work in parallel, each in the files its task owns
([tasks.md](tasks.md)), so nothing two areas need may live in one file that
both would edit:

- **Parent modules come first.** Task F01 created every module a task
  names, as a stub, with its `mod` line in the parent (`lib.rs`, `mod.rs`),
  its `Vm` field and its `Intent` variant. The larger areas keep their
  modules in folders whose `mod.rs` F01 keeps
  (`anomi-vm/src/{sidebar,places,chat,people,voice,settings}/mod.rs`): one
  field and one variant per module, flat, and a `parts!` line giving each
  module's `Intent` a `From` into its folder's and into the window's. A
  task fills its own files and adds no line elsewhere.
- **Map with `From`.** A view returns its module's own `Intent`, or its
  folder's (`chat::Intent`...) when it draws parts other tasks own: the
  timeline, the message, the composer, the frames of the settings. Whoever
  places a view maps it with `.map(Intent::from)` (or `chat::Intent::from`),
  never with a variant, so a view can move from one to the other without
  its caller changing. Every area's view is `view(vm, motion)`; the stub's
  signature is what its callers compile against, so change it with them.
- **Keep what the parents need.** A view-model type keeps `Clone, Debug,
  Default`; an `Intent` keeps `Clone, Debug, PartialEq` and
  `#[non_exhaustive]`, so the app matches it with a wildcard until its I
  task and a writer's new variant never breaks the app's build.
- **Lines F01 wrote into one task's file for another.** Where one task's
  file is the parent of another's (the message and its reactions,
  attachments, embeds, components, poll and editor; the composer and its
  suggestions and commands; the profile editor and its preview; the
  settings frames and their pages; the friends screen, the forum and the
  pickers' popup), F01 wrote the `mod` lines, and for the first three the
  fields and the calls to the parts' views, marked "wired by F01": the
  owner keeps them.
- **What only one area uses stays in that area.** A size, a colour
  derived from the palette, a button look: a named constant or a style
  function in the area's module, with the Slint line it came from.
  `anomi-theme` (`metrics`, `catalog`, `palette`) and `anomi_vm::kind` hold
  what several areas share; they change only through the tasks that own
  them (F02, F14), and a writer who needs something there says so in the
  review rather than editing it.
- **Fixtures, shots and tests go with the view-model.** A task owning
  `anomi-vm/src/<path>.rs` also owns `anomi-vm/src/fixtures/<path>.rs`,
  the `shots()` of its screen modules (each crate's `shots.rs` only
  collects them; F01 wrote that list) and its tests,
  `tests/areas/<name>.rs` in its screens crate: a module of the crate's one
  test binary (`tests/areas/main.rs`, F01's, with the helpers in
  `support`), so a change links one binary per crate rather than one per
  area. A shot is the whole window: an area's fixture starts from the
  shell's (`fixtures::shell::dm_home`) and fills its own field.
- **Manifests.** F01 wrote every crate's `Cargo.toml` with what the tasks
  are known to need: `pulldown-cmark` in `anomi-vm` (F08), iced's `canvas`
  for drawing paths (F15's glyph outlines, V02's ping graph) and its
  `qr_code` (F02, N09). A task that needs a crate nobody foresaw adds that
  one dependency line to the manifest of the crate it works in, runs
  `cargo update --workspace` so `Cargo.lock` lists it, and names it in its
  review: the one line a writer may add outside its files.
- **The integration tasks share `anomi-app`'s spine** (`app.rs`,
  `message.rs`, `effects/mod.rs`, `project/mod.rs`, `intent/mod.rs`,
  `project/rows.rs`): they run one at a time in one worktree, so each adds
  its arms there in turn.

## From a click to the core and back

```mermaid
sequenceDiagram
    participant S as Screen (view)
    participant U as App::update
    participant C as Application (core)
    participant B as Backend (core actor)
    S->>U: Message::Intent(Intent::Sidebar(Dms(Open(room))))
    U->>U: intent::sidebar::handle
    U->>C: dispatch(Action::Open(room))
    C-->>U: Vec<Effect>
    U->>B: effects::run: Effect::Backend(cmd) → backend.send(cmd)
    U->>U: show(Reach::after(action)): project the areas whose keys changed
    B-->>U: events, batched: Message::Backend { run, events }
    U->>C: apply(event) for each
    U->>U: effects::run, refresh()
```

- **Intent.** A screen asks with its area's `Intent` (in `anomi-vm`), wrapped
  in `anomi_vm::Intent`, mapped by the shell into `Message::Intent`. Intents
  are `Clone`, `Debug` and `PartialEq`, so tests compare them.
- **update.** `intent/<area>.rs` handles each: a change to the view-model's
  own state (a search field's text, a panel's width, which popup is open)
  stays in the app and re-projects; anything the core decides becomes an
  `Action` through `App::dispatch`, which then shows what the action can
  have changed (`Reach::after`, as Slint's `Adapter::action` narrowed it):
  a keystroke in the composer, a bot's form, the profile editor, the
  settings, the quick switcher or the new DM's field reaches that area
  alone, a picture asked for nothing, anything
  else every area, which compares its key. The core's `Action`, `Event` and
  `Effect` aren't `Clone`, so they never travel in a message; events ride
  in [`Once`](../../crates/anomi-app/src/message.rs), taken in `update`.
- **Effects.** `effects::run` turns each `Effect` into a direct call or an
  iced `Task`: `Start` starts the backend, `Backend(cmd)` sends at once and
  hands a refused command to `backend_failed`, `ListDevices` will be
  `Task::perform` over the core's `spawn_blocking` (I18), notifications and
  sounds direct calls (I15), linking Discord an abortable task (I05).
- **Events.** The backend's Tokio receiver becomes a stream, `ready_chunks(256)`,
  run as one abortable `Task` per backend start, numbered by `run`; when it
  ends, `BackendClosed(run)` reaches `stopped()`. Each event of a batch
  applies and then runs its effects before the next applies, as the Slint
  app's `Adapter::apply` does; the areas refresh once per batch, only
  those the batch can reach (`project::Reach`): who's typing reaches the
  composers alone, who speaks the call's views, a picture whatever shows
  pictures, 100 ms after the first of a run; an event whose effects ask
  more than a sound reaches everything. A quiet backend
  wakes nothing. Dropping `App` drops the events task, then the
  backend, then the runtime, in that field order.
- **Pictures.** The core leaves the pictures of list rows to the
  interface: a list reports the rows in view (`on_viewport`), the area's
  intent hands their ids to its projection, which wants their pictures
  from `App::pictures` (`pictures::Requests`), as it does again whenever
  it builds those rows; `App::ask_pictures`, at the end of each `update`,
  dispatches `Action::LoadPictures` at most every 50 ms, and nothing waits
  while nothing is wanted. The core counts every picture coming or going in
  one number (`picture_changes`); an area keeps which of its rows show which
  address (`pictures::Watch`, filled as rows are built, or by
  `pictures::noting` around a builder that looks its pictures up through
  `pictures::find`), and builds again only the rows, or for a small list
  the area, whose own pictures came or went: a timeline's images arriving
  don't rebuild the rail.
- **Back to the screens.** `App::show(reach)` (`App::refresh` for
  everything) calls the `project::<area>::show` of each area the reach
  reaches, with the core's `AppState` and the app's own `Layout` (the
  Slint app's `layout.json`, field for field: settings the core doesn't
  keep, such as how calls are joined; a change is written a second after
  the last of a run).
  Each keeps a key of its inputs (the core's change counters such as
  `list_changes` and `picture_changes`, the selection, the settings it
  reads, its own UI state) and returns at once when the key is the same;
  otherwise it rebuilds that area's view-model. A key must cover everything
  the rows read, or the area goes stale; a key too wide only costs time.
- **view.** `App::view` calls `anomi_screens::window(&vm, motion)` on every
  update, and iced lays the whole window out again after it: every batch
  of backend events, a game running or not, costs a view and a layout of
  everything shown (tiny-skia then redraws only what changed). So views
  borrow the view-model and build only what shows: lists that can be long
  (timeline, members, pickers) build only the rows near the viewport
  through the virtual list (task F07), and an area whose view-model didn't
  change can be wrapped in `iced::widget::lazy` over its `version`, as
  `Sidebar::version` is made for. Q03 measures a busy server behind a game.

## How a screen gets its data

1. The writer defines the area's view-model in `anomi-vm/src/<area>.rs`
   after the Slint structs it replaces (`MessageRow`, `RoomRow`...), keeping
   only what the screen draws, already formatted: text translated, counts as
   badges, times as text. Pictures are `iced::widget::image::Handle`s the
   app made from the core's pixels without a copy (`anomi-app/src/pictures/`),
   straight RGBA; `picture` draws them through copies the renderer draws
   from their own bytes ([iced-notes](iced-notes.md#pictures-and-tiny-skia)).
2. Writes fixtures for it in `anomi-vm/src/fixtures` after the fake
   Discord's data, used by tests and shots.
3. Writes the view in the screens crate, from the view-model alone.
4. An Opus integration task writes `project/<area>.rs` (porting the
   `show_<area>` of `src/ui/refresh.rs`) and `intent/<area>.rs` (porting the
   area's `on_*` handlers in `src/ui/callbacks.rs`).

## Styling

`anomi_theme::Theme` is the theme type of every widget: a palette (the 37
tokens of `ui/theme.slint`) with its id and name. Each built-in widget's
`Catalog` is implemented for it in `anomi-theme/src/catalog/<widget>.rs`
with the look the interface wants by default, plus named styles several
places share (`catalog::button::primary`, `catalog::container::sidebar`).
A widget takes another look with `.style(|theme: &Theme, status| ...)`,
reading `theme.palette()`; derived colours (`menu()`, `composer()`,
`placeholder()`, `mention()`) are methods on `Palette`. Every built-in
widget the interface uses has its catalog (F02); one that hasn't doesn't
compile with our `Theme`, and its catalog is asked for in the review.
Switching theme replaces `App::theme`; nothing is rebuilt, as styles are
read at draw time.

## Motion

The durations and curves of `ui/theme.slint` are `anomi_theme::motion`:
`QUICK` 120 ms, `SLIDE` 180, `LINGER` 400, `POPUP` 150, `DIALOG` 220, `LEAVE`
120, `ARRIVE` = `cubic-bezier(0.22, 1, 0.36, 1)`, `GLIDE` = ease-in-out.
`Motion { front, never, idle }` says whether anything may move; every
animation takes its duration from `motion.duration(token)` or
`motion.animation(...)`, which are zero while `still()`. An animation either
lives in the app's state, interpolated in `view` at the frame time from
`window::frames()` (subscribed only while it plays and the window is in
front), or in a widget's tree state, as `RailItem` keeps its pill: the widget
asks `shell.request_redraw()` on each `RedrawRequested` only while it plays,
and lands at once when `motion.still()`. Scrollables get
`.smooth_scroll(motion.playing())`. The settings' "never animate" sets
`never`; the window's focus sets `front`; nobody using the window for 15 s
(a key, a click or the wheel, as Discord's client counts it) sets `idle`
(`anomi-app/src/clocks/away.rs`), which the core tells Discord and which
holds what only decorates (`motion.watched()`: moving pictures, a
skeleton's sweep, the typing dots) and the composer's caret, steady.

## The background rule

While the window isn't in front nothing ticks or redraws: the owner plays
games with this client open. Concretely:

- Nothing ticks on a period: no `time::every`. Each kind of clock (the
  minute, the day, the second) is one wait set for the moment its words
  next change (`anomi-app/src/clocks.rs`), running only while the window is
  in front and something shows it, dropped behind another window or in the
  tray, and every clock is said again the moment the window comes back. A
  tick reaches only what shows the time (`Reach::MINUTE`, `Reach::DAY`).
  `window::frames()` is subscribed only while an animation in the app's
  state plays in front; test it with `subscription().units()`.
- Widgets request frames only while an animation plays, and none while
  `still`.
- A video's or a camera's frames reach the window through one task waiting
  on the core's frame wake (`anomi-app/src/media/frames.rs`): each frame is
  put in its `kind::Live` and asks for a redraw with no message, so the
  damage is its rectangle and no view is built; only a source's first frame
  and a video's bar moving a step (four times a second) build the view.
  The task follows sources only while they can be seen: in front, not
  paused or ended, full screen or with their row in view.
- Backend events still update the window, once each, as Slint did.
- Measured under `scripts/ui-session` on the fake Discord: at rest, behind
  another window and in the tray, 0 CPU, and the main thread and iced's
  executor wake only for a clock that shows (in front, once a minute for a
  busy voice channel's row). Three threads wake on their own: the test
  session's control-file reader (its private display only), the core's
  network (the gateway's heartbeats) and the `blocking` crate's pool, which
  zbus starts and whose idle thread wakes twice a second for good.

## Translation

The interface is Russian by default (`yapfast_lang::DEFAULT`); English and
Russian are `lang/en.po` and `lang/ru.po`, keyed by the English text; see
[translations](../translations.md). In a view, `tr!("Friends")` is a
`&'static str` that borrows into the widget; `trn!("{n} member", "{n}
members", count)` picks the plural; `yapfast_lang::text_in("Microphone",
"Mute")` takes a context. Reuse the Slint text where it exists (grep
`lang/ru.po`); a new text needs its entry in `ru.po`, and in `en.po` too if
it names the product. Text made in the projection is translated there and
made again when the language changes (`language::choose` forgets every
area's key). `scripts/check-translations` (run by `check-iced`) lists text
without a translation.

## Testing

- **View-model and projection logic**: plain unit tests in the module.
- **Screens**: `iced_test::Simulator` over a fixture, in the area's
  `tests/areas/<name>.rs`: click by text (`ui.click("Friends")`), type,
  collect `into_messages()` and compare intents; `ui.rebuild(view)` shows
  the next view-model. Tests choose no language, so text is the code's
  English. See `crates/anomi-screens/tests/areas/dms.rs`; the area crates
  drive their area's view alone, the screens crate the whole window.
- **Looks**: each area module has a `shots()` its crate's `shots.rs`
  collects; `anomi-shots` renders them at 1x (or `--scale 2`, `--language
  en`, `--only TEXT`) for review, and `--time FRAMES` times the renderer
  drawing each. Shots draw the window with no pointer:
  hover and pressed looks are checked in the app. Snapshot comparison
  (`Simulator::snapshot(...).matches_image(...)`) comes when a screen is
  accepted, so later changes show as diffs.
- **The app**: `cargo test --profile fast -p anomi-app` (reviewers).
  `anomi-app/src/tests` drives the whole window on the real core, one
  module per area: `start` gives an app on a data folder of its own (the
  preview, or the sign-in screen), `interact` clicks and types into
  `App::view` in the simulator and hands what came out to `App::update`,
  `shows` finds a text. Tasks returned by `update` (focus, timers, a
  linking's progress) don't run there.

## Checks and builds

| Who | Command | What it costs (measured, 32 threads) |
| --- | --- | --- |
| Writer | `scripts/check-iced` | fmt, 400-line limit, translations, `cargo check` and Clippy of the crates without the core: 2 s in a new worktree (it copies the main checkout's `target/debug` with reflinks), 0.8 s with nothing changed, 1.1-1.3 s after an edit; 9 s and 3.0-3.3 GiB at its peak in an empty target |
| Reviewer | `scripts/check-iced --review` | adds the task list's coverage and ownership, those crates' tests, the vendored renderer's formatting and tests (`vendor/iced_tiny_skia`), Clippy and tests of `anomi-app`, the `anomi` binary (`fast` profile) and the shots: 15-19 s warm; the first app build in a target that has the Slint app's native builds took 52 s, in an empty one several minutes and 5-7 GiB |

- Always name packages: `cargo check -p anomi-screens`. A bare `cargo check`
  at the root builds the Slint app (`default-members`), 6-8 GiB.
- `scripts/check` (the Slint app's) leaves the `anomi-*` crates out, so the
  two never resolve each other's dependencies. Both find the crates by
  their folder's name, as the workspace does (`crates/anomi-*`): a new
  area crate needs no list edited.
- A fresh worktree for `anomi-app` builds the core's native code again
  (`vendor/` crates are keyed by path). Keep one long-lived integration
  worktree, run one cold build at a time, and set `LK_CUSTOM_WEBRTC` to an
  unpacked libwebrtc for every build in that worktree, or each new target
  downloads 659 MB.

## The review loop

1. A writer takes a task from [tasks.md](tasks.md), works only in the
   modules it owns, and runs `scripts/check-iced` until it passes. It never
   builds or runs the app.
2. A reviewer (Opus) runs `scripts/check-iced --review`, opens the task's
   shots beside its reference screenshots (`~/.cache/anomi-iced/golden`, see
   its `index.md`), and checks the acceptance list: layout to the pixel
   grid of the reference at 1x, text and translations, states (hover,
   selected, empty, loading), keyboard, and the background rule.
3. Then, once the integration task of the area is in, on the real backend:
   `scripts/ui-session start --discord --anomi`, the same clicks as the
   reference shot, and `scripts/measure-process` for idle CPU and wakeups.
4. Anything off goes back to the writer as a list: what, where (shot and
   coordinates or `file:line`), and what the reference shows. Repeat.

## Gaps of the pinned iced, and who closed them

| Gap | Where it's handled |
| --- | --- |
| No virtual list | `list` (F07) |
| tiny-skia ignores an image's border radius | the vendored renderer (patches 2, 7, 10) cuts each corner of the picture's clip; `picture` (F05) makes its copies at the pixels drawn, which it copies in on whole pixels |
| tiny-skia keeps a converted copy of each picture it draws | the vendored renderer (R4, patch 1): `picture`'s copies and frames are made in its own format and drawn from their own bytes |
| No pictures inside text | `paragraph` (F08): emoji through Twemoji's font, a server's emoji in the room a glyph keeps |
| Selection only of built widgets | `selection` (F09), over the list's rows |
| No exit animations, no layer manager | `layer` (F06) |
| No layer drawn at an opacity | not closed: a faded widget fades its colours, not its pictures (the list's drag copy, a name) |
| No tray, no single instance | `anomi-desktop`, shared with the Slint app (I14) |
| Wayland file drops | a data device of the app's own on winit's connection, `anomi-desktop`'s `drop`, shared with the Slint app (I15) |
| No accessibility | not planned at this pin (pop-os's fork has AccessKit) |
