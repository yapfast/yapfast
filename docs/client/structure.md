# How the client is built

## How it's built

The client is two crates. `crates/yapfast-iced-kit` knows nothing of the
core or the app: the theme's struct, styles, fonts, icons and the
widgets (see [the kit](kit.md)). `crates/yapfast-iced` is the app: what
knows the core, and every screen.

The app is one `App` (`app/mod.rs`) and one `Message` type. `App` holds
the shell (the window, the tray, the desktop's parts: `shell::Shell`),
what lives only as long as the window (`app::Window`), a struct for each
of the window's main areas, a `Slot` for each screen the core opens, and
what every update works through: the core (`Client`), what
`layout.json` keeps, the themes and the theme shown, and the requests
made of the root. Those are plain fields of `App`, which `ctx::cx!`
borrows apart from the rest, so a screen's slot and the `Cx` its update
takes are held at once. A message goes through `App::update`:
`handle_message` hands it to its area or screen by one arm each, then
`App::settle` does what the update asked (closes, preferences, the
focus) and syncs if anything changed (see Screens). The view is
built again from the top after each message (`app/view.rs`): the window's
layers stacked, each area and screen drawn from its own state and a
`ctx::Ctx`, which `App::ctx` makes once per view.

iced builds the whole view again after every message and lays it out, so
what costs is kept out of the view:

- The backend's events come on its receiver, batched, as messages
  (`app::batches`); nothing polls. Fonts the shaper wants, the control file and
  the window's size as it opens and is resized come as subscriptions.
- After each batch of the backend's events, `sync.rs` takes in only what
  changed, by what the core gives to tell: the core counts changes to its
  rooms and servers (`list_changes`), to the people lists (friends,
  requests, those blocked, pinned DMs: `people_changes`) and to the
  profile editor (`profile_editor_changes`), and shares messages and the
  profiles shown by `Arc`, so what's made of them is kept while those
  stay the same.
- Lists are virtualized by hand: `widgets::Rows` holds only the rows near
  its viewport, each keyed by its id, so its widget state follows it as
  rows come and go. A message's text is laid out once, when it comes, and
  kept with its row. The timeline's and the member list's scroll reports
  go on only when they matter (`widgets::Settle`): a step well inside the
  rows built beyond the edges, an end reached or left, a new size, and the
  last a moment after the list stops; a scrolling list reports on every
  frame it moves, and each report builds the view again. The member list keeps the
  look of each member who didn't change as the roster changes.
- Pictures are the core's own buffers in tiny-skia's format, premultiplied
  BGRA (`pictures.rs`, `chat::set_pixel_buffers`), handed to iced without a
  copy. Copies at other sizes are made once and go when the core lets go of
  the picture (`Converted`). A copy for a box of another shape (`paint::fitted`)
  either covers the box, cut about its middle (avatars, banners), or fits
  inside it keeping its shape, see-through around it, as a tall custom
  emoji or role icon does in its square.
- Moving pictures (`animated::Animated`) advance inside their widget on the
  window's redraws, all on one clock, so a frame costs no message and no
  rebuild of the view. Their players (`animated::Players`) are one for each
  picture and size however many places show it, start on the first redraw
  that finds the picture in view, and go after each view that no longer
  shows it; the window's are `Pictures::players`, and the emoji picker's
  tab holds its own, so they go with the tab before the heap is given
  back. A piece of text plays the custom emoji over its spans through the
  kit's `piece::Moving`, which `Animated` implements. They play as
  Messages' Animate setting says (`pictures::plays`): always, never, or
  while the window has the focus; held still, each shows its first frame.
- Popups open with a motion: over 100 ms,
  eased out, the card grows from 80% of its size about its centre and the
  dim under it fades in. The root keeps which popups are open and when
  each was first drawn (`app::layers::Opening`, compared as a mask of the
  layers, so a message that opens or closes nothing costs nothing more);
  `App::popup` wraps a layer that opens so in a `widgets::motion::Motion`,
  which asks for the window's redraws until it's open and, as it draws,
  says how far open it is to the `grow` round the card (in `dialog`,
  `panel`, `sheet::backdrop` and the placed cards and popovers) and to the
  dim (`controls::dimmed`). No message is sent and no view built for a
  frame of it. Which layers open so is `Layer::moves`; menus, the picker
  and the viewer open at once.
- Settings and a server tag's card close the same way backwards: their
  screen says it's drawn going (`Closes::LEAVES`), so its slot keeps it
  once its close has run (`Slot::leaving`), and the view draws it in
  `motion::leave`, which takes neither the pointer nor the keys, hands
  no focus into it and shows no overlay, so a click lands on what's under
  it. As it's gone it says `Message::PopupGone` and the slot lets go of
  it. Opened again as it goes, the new one grows on from where the old
  one was (`motion::reopened`).
- What moves on its own (`widgets/live.rs`): meters are `Gauge`s that
  read the level as they're drawn and clocks (the call's length, an
  activity's time, an invite's countdown) `LiveText`s that read their
  text so, each drawn again on a timer of its own while it shows; neither
  sends a message, so no view is built for them.
- Fonts (`fonts.rs`): DM Sans and its italic, the packed Twemoji (the
  app's `build.rs` makes it), the system's monospace face for
  code, and the display-name fonts on first use. DM Sans then Twemoji are
  the fallbacks every text tries. A script DM Sans lacks loads the system's
  font for its language (fontconfig's `:lang=`, on Windows a table of the
  fonts it ships) the first time a script is shaped. A character none of
  those has, whatever its script (𝔟, ⓒ, ⠃, ᐊ, z̷), is handed over by the
  shaper (`vendor/cosmic-text` patch 1); the system's font for that
  character (fontconfig's `:charset=`, on Windows the first of a list of
  symbol and script fonts whose character map has it) is loaded once and
  named for that character from then on, and a face already chosen for a
  character in the same block of
  128 code points is used when it has it. Each character is looked up
  once, found or not. The lookups and loads run on a blocking thread
  (`wakes`), the font system's version moves so iced shapes text again,
  and `Message::FontsLoaded` lays the window out again. Fallback faces are
  mapped from their files and kept upright and italic at any weight: the
  shaper takes a family's face at the weight nearest the text's (patch 2),
  so semibold names take DM Sans and Twemoji as regular text does.
  The person's own font files (`custom_font.rs`, `woff.rs`; Settings'
  Appearance chooses them, see [Fonts](../themes.md#fonts)) are put in the
  same font system under the name DM Sans, and DM Sans's faces leave it
  while they're in: every text asks for that family by name, so each is
  drawn in the chosen font with no change to a view, and resetting puts
  DM Sans back. `fonts::shape_again` then has iced shape its text again.
  Elided text (`elide.rs`) clips across only, so marks above and below the
  line show.
- Video (`video.rs`, `call/`): cameras' frames are made in the client's
  own memory (`picture::set_frame_buffers`) at their tile's size, and a
  watched stream's frames come from the stream worker's shared mapping.
  The renderer's hook (patch 13 in `vendor/iced_tiny_skia`'s
  `README.yapfast.md`, see [the toolkit](toolkit.md)) copies or scales
  them into the window with libyuv. The core wakes the client for each
  frame made
  (`picture::set_frame_wake`): a camera's as it's decoded, and a stream
  worker's as the worker says so on its reports' pipe, a line per frame it
  writes only for a client that asked to be woken. Nothing looks for
  frames on a timer. A new frame only redraws the window: `Video` reads
  its source's newest frame as it's drawn (`video::Shared::frame`), and
  only a source's first frame (which brings its tile's face) or a
  message's video (whose bar shows its time) builds the view again.

Each part of the window has its module:

- signing in: `signin`, `mfa`, `accounts` (Manage Accounts);
- the sides: `rail`, `sidebar` (with `arrange`), `account`, `members`,
  `browse_channels`, `place_menus`;
- the conversation: `conversation` (what takes the timeline's place),
  `header`, `banners`, `timeline` (with `view`, `keys`, `selection`), `message`, `media`,
  `polls`, `components`, `found`, `header_popovers`, `search`, `thread`, `forum`,
  `composer`, `edit`, `suggest`, `commands`, `typing`, `uploads`,
  `playback`, `links` (where a link goes);
- people: `people`, `person_menu`, `profile` (with `editor`), `friends`,
  `active_now`, `reactions`, `info_cards`, `tag_card`, `looks` (what
  people wear);
- what opens over the window: `picker` (with its tabs `emoji`, `gifs`,
  `stickers`), `menus`, `dialogs`, `bot_select`, `forward`,
  `image_viewer`, `server_dialogs`, `link_dialogs`, `status`,
  `switcher`, `bot_modal`;
- calls: `call` (with `stage`, `dock`, `details`, `popups`, `dm`,
  `corner`);
- settings: `settings` (with `pages`, `voice`, `appearance`, `keybinds`,
  `updates`), `server_settings` (with `profile`, `roles`, `people`,
  `moderation`, `expressions`, `ownership`), `channel_settings`;
- the window: `shell`, `window_bar` and `desktop` (the desktop's
  parts), `browser`, and `control_file` (the command file the test display
  drives);
- what runs with no screen open: `keybinds` (the keybinds' watcher),
  `away` (whether someone is using Yapfast), `updates` (the update
  check).

`themes` makes the kit's theme from the core's palette, and `layout`
holds what's kept in `layout.json`.
`app` holds the window's state and what each message does (its
messages in `app::message`, what it listens to in `app::subscription`, the
layers the view stacks in `app::view`); `sync.rs` takes in what changed
in the core's state after each message.
`widgets/`, `field.rs` (the composer's text field), `piece.rs`
(selectable text, whose selection across the timeline `timeline/selection.rs` keeps),
`animated.rs` and `video.rs` are the client's own widgets. Overlays
(the card, suggestions, the picker, menus, a dialog) are layers of one
stack, each over a layer that takes a click outside; a menu or a dialog
takes the keys by moving focus to an id nothing has (`app::NOBODY`).
What's open is one list, `app::layers::LAYERS`: the first open layer that takes
the keys has them (a dialog, settings, the menu), Escape that nothing took
closes the first open one that closes on it, and while one with a field
that keeps Escape is open, the window takes a captured Escape too.

## Screens

A screen (Settings, a server's and a channel's settings, the profile
editor, sign-in and the two-factor prompt, the dialogs, the switcher,
the custom status, the picture viewer, the profile and info cards, the
picker, the header's popovers and search, a server tag's card, and the
menus' view) keeps its own state in a type of its own and works on the
app's state only through what it's handed, so its code is its own and
not the app's. The window's main areas keep their state in a struct each
too, which the app holds: the rail (`rail::Rail`), the sidebar
(`sidebar::Sidebar`), the member list (`members::Roster`), the timeline
(`timeline::Timeline`), the composer (`composer::Composer`), the files
waiting (`uploads::Uploads`) and what plays in the timeline
(`playback::Playback`); the rail, the sidebar and the member list are
drawn by a view of their own state and a `Ctx` (`rail::view`,
`sidebar::view`, `members::view`), and the conversation's parts, its
header, timeline, rows, composer, thread, forum, friends and the call's
dock and popups by free functions of a `Ctx` in their own modules
(`header::view`, `composer::view`, `timeline::view::timeline`).

The main areas work on the root itself: what they do reaches across the
window (sending scrolls the timeline, a menu's row acts on any part), so
an area's update is a free function of the app in its own module,
`update(app: &mut App, event) -> Task<Message>` (`composer::update`,
`thread::update`, `call::update`, `rail::update`), with its helpers
beside it, and the views that read where the window lays things out (the
call's stage and corner, the window's bar) are free functions of an
`&App` there too. None is a method of `App`: rustc compiles a method with
its type, so they would all build as one unit, and an edit to any would
build them all again. `App`'s own methods are the root's: what each
message does and the settle pass (`app/`), taking in the core's state
(`sync.rs`) and the window, tray and desktop (`shell.rs`).

- Its state lives in a `slot::Slot` on the app while it's open, and goes
  only through `Slot::close`, which runs its `Closes::close` with why
  (`slot::Why`: the person, the core, or the window going), so nothing it
  started outlives it; one that closes as a popup is kept, closed, while
  it's drawn going (`Slot::leaving`). A screen the core opens (a server's or a channel's
  settings, the profile editor: the core holds what they edit) follows the core with `Slot::follow` as the
  window syncs: made as the core's state appears, closed with `Why::Core`
  as it goes. The root's glue for each screen, its message, Escape and
  follow, is in `app/screens.rs`. What lives only as long as the window
  (the menu, the picker, a tag's card, Settings, the pictures and icons
  made to draw) is `app::Window`, which closing the window tears down in
  one place, `App::forget_window`, naming each field; the title bar's
  switch, which makes the window again from inside Settings, keeps them
  open (`app::Keep`). One whose state is small and kept
  whether it shows or not (sign-in's fields, the two-factor prompt's
  wait) is a plain field of the app instead, its `sync` handed the
  core's state.
- Its update (`Settings::update`, `open`, `show`, `escape`) works through
  a `ctx::Cx`: the core to dispatch to, what `layout.json` keeps, the
  themes, the theme shown, the window's size, which build this is. Dispatching or
  `Cx::touch` marks the window to sync; the update never syncs itself.
  What it needs outside itself it asks for with a `ctx::Request`, the one
  list of what screens may ask: close a screen the window opened
  (`Close(Screen)`), close the menus or the call's popups, someone's menu
  where the pointer was pressed, an emoji picked placed where the picker
  was opened for, the pictures in view as a list scrolls, where the keys go (the
  composer's or nobody's, or a field by its id; the last asked wins), a title bar, a look at the releases,
  and `Changed` with a preference others show (`ctx::Pref`: the clock,
  the timeline's rows, the member list's, the keybinds, going idle,
  the theme, noise suppression).
- Its view is a free function of its state and a `ctx::Ctx`
  (`settings::view`), with its parts on a view type of its own
  (`settings::Page`) rather than on the app. A view is never a method
  of `Ctx`: rustc compiles a method with its type, so every view would
  build in one unit, and an edit to any of them would build them all
  again. `Ctx`'s own methods are the few every screen draws with: faces
  and pictures (`pictures.rs`), a person's row and what they're up to
  (`people.rs`), tags (`looks.rs`), a message's picture (`media.rs`),
  message text (`Ctx::body_view`, which reads the rows' looks and the
  selection through `timeline::Lanes`) and the timeline's lookups. `Ctx`
  is references to what views read, made once per view
  (`App::ctx`): the core's state, the theme and pictures, the areas'
  state, what the pointer is on and where it was pressed, the menu and
  picker open, and an `Open` record the root works out of what shows
  beside the conversation (the member list, the thread), so a view
  doesn't decide that from other screens' state. A menu's acts stay the root's: it opens one for whoever asks, and what
  its rows do reaches across the window. What a screen hands back to the root (Ctrl+K over Settings,
  an emoji's button opening the picker, a picture's menu) is decided in
  its glue in `app/screens.rs` before its update runs.
- After each message, `App::settle` (`app/settle.rs`) does what was
  asked, in passes while anything is asked or marked (at most four: a
  sync may close a screen the core closed, whose close asks more), each the requests (a close may ask more), each changed
  preference once (the root's part, such as working the rows out again
  or watching the keybinds again, then each open screen's `changed`),
  the focus, a sync if anything was marked (after the focus, so a field
  what the sync opens focuses takes the keys), the videos placed again if
  the camera Settings try changed, and keybinds resting while one is
  recorded. What the window shows is worked out from
  the screens' state rather than set by them: the theme is the one chosen
  unless Settings' editor previews another (`Settings::preview`), and the
  window's video sources take the camera being tried
  (`Settings::camera_test`). With nothing asked, the pass touches
  nothing.
- Where the keys go stays explicit: a screen asks with
  `Request::Focus`, the last ask wins, and nothing works it out from
  what's open, since a dozen updates refocus with nothing opened or
  closed and some layers leave the keys where they are on purpose. With
  `YAPFAST_LOG=debug`, an update that changed the top layer and asked
  nothing through `Request::Focus` is logged ("the top layer changed with
  no focus asked", with both layers), which finds one that leaves the
  keys in a field that's gone. Updates that focus with a task of their
  own show there too.

## Adding a screen

A screen opened over the window, as Settings, the picker or a tag's card
are, takes these, and the compiler asks for the rest:

1. A module with its state struct (which exists only while it's open),
   its `Event`, and `open`, `update` and `escape` taking a `&mut Cx`, a
   `close` through `slot::Closes`, and a free `view(state, ctx)`. Pages
   or parts in their own files are free functions there, or a view type
   of their own (`settings::Page`), so each compiles in its own unit.
2. One variant in `app::Message`, wrapping its `Event`, and one arm for
   it in `App::handle_message`; its glue (open, the message, Escape) in
   `app/screens.rs`, or `app/settle.rs` for Settings, whom the shell and
   the tray open. If the window opens it, one `Slot` field in
   `app::Window` and one variant in `ctx::Screen`; `App::forget_window`
   and `App::close_screen` then fail to build until they say what
   becomes of it. If the core opens it, a `Slot` on the app that follows
   the core's state in `sync` (`Slot::follow`).
3. If it opens over the window: one `Layer`, one line in `LAYERS` at its
   place in the order (topmost first), and one line in `app/view.rs`'s
   stack. `is_open` and `shut` (what Escape does to it) then ask for its
   arm; `takes_keys`, `field_keeps_escape` and `layer_key` (the keys it
   takes) are lists to add it to when it differs from their default.
4. Its view builds from the kit with `ctx.look`: colours from the theme,
   sizes from named constants, and corners, borders and shadows from
   `style`'s helpers (`style::rounded`, `style::round`, `style::edge`,
   `style::outlined`, `style::panel`), never a literal in the screen.
5. It reaches the core with `cx.dispatch`, keeps what `layout.json` holds
   with `cx.keep`, and asks anything of the rest of the window with
   `cx.request`. A need `Request` doesn't cover is a new variant, never a
   reference back to the app. It never syncs: dispatching or `cx.touch()`
   marks the window, and `App::settle` syncs once after the message.
6. A test in `tests/` that opens it, clicks or types through
   `App::update`, and reads what reached the core (`client.sent`) or what
   it shows.

Keybinds, the first part written to this, shows each of these on a
smaller scale, as a section of Settings rather than a screen of its own
(`settings/keybinds.rs`):

- Its state is Settings', so it goes with them: the keybind being
  recorded (`Settings::record`) and the action menu open. What a keybind
  is and how its keys are recorded is the core's
  (`yapfast_core::keybinds`), and the keybinds themselves are kept in
  `layout.json`.
- Its `Event` (Add, Remove, an action chosen) is wrapped as
  `settings::Event::Keybind`, so it adds no `Message` and no arm at the
  root. Settings' `update` hands it to `keybind_event`.
- `keybind_event` changes `cx.layout.keybinds`, keeps them with
  `cx.keep("keybinds", ..)`, and asks
  `cx.request(Request::Changed(Pref::Keybinds))`. It touches nothing
  outside Settings.
- What outlives Settings isn't Settings': the watcher that makes the
  keys work with Settings closed is the root's (`crate::keybinds`, its
  reports as `Message::Keybinds`). `App::settle` hears
  `Pref::Keybinds` and watches the keybinds again, and each open screen's
  `changed` hears it too.
- What the window shows is worked out, not set: keybinds rest while one
  records because `settle` asks `Settings::recording` after every
  message, so closing Settings by any path, the tray included, wakes
  them.
- Its view is `Page::keybinds_page`, built from `form` and `controls`
  with the theme's colours and named widths (`ACTION_WIDTH`,
  `KEYS_WIDTH`).
- `tests/settings.rs`'s `keybinds_are_added_chosen_recorded_and_removed`
  goes through it as a person would and reads what `layout.json` kept.
