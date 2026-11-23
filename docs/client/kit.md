# The kit

Screens build from shared pieces rather than styling their own, and take
every colour from the theme and every repeated size from a named constant.
When a second screen needs what a first one built, it moves: to the kit
if it draws with the theme and the icons alone, or to a module of the
app both use if it draws the core's data. The list below is both, each
by its module: `controls`, `form`, `style`, `dialog`, `sheet`, `tile`,
`viewer`, `popover`, `reorder`, `elide`, `widgets` and `Look` are the
kit's, and `pictures`, `people`, `looks`, `markdown`, `media`, `found`,
`info_cards`, `picker::kit`, `menus`, `thread` and `Ctx` the app's. The
kit is a crate of its own, `crates/yapfast-iced-kit`, which knows nothing
of the core or of the app: the theme (`theme`, which the app fills in
`themes.rs`), `style`, `fonts`, `icons` and `canvas`, `controls`, `form`,
`sheet`, `dialog`, `tile`, `viewer`, `field`, `piece`, `popover`,
`reorder`, `elide`, `timer`, `frames` and `widgets`. The client uses each
module under its own name (`crate::widgets::Rows`), and what the kit
can't know it's handed: the messages to send, the palette, a zoom wheel's
steps. A piece
that draws with the theme and the icons alone takes a `look::Look`, the two
borrowed (`App::look`), not the window's whole state. One that draws the
core's pictures (avatars, faces, shots, emoji, someone's status) takes a
`ctx::Ctx`: the look, the pictures made to draw, the core and its state,
and whether moving pictures play, borrowed and passed by value
(`App::ctx`). The kit's pieces name no message of the app's: each is
generic over the message it sends (`controls::action`,
`Look::icon_button`, `form::Switch`, `sheet::Sheet`, `dialog::dialog`),
and the app uses them with its one `Message`, so nothing is mapped:

- `controls::action`: the button of dialogs and forms, by kind: primary,
  plain, ghost, danger.
- `controls::field`: the single-line field, by any id;
  `controls::field_style` its look on a ground of the caller's (a colour
  code's field gone red, a name that shows its ground only when lit).
- `controls::edged`: an edge drawn over what it holds, so a card whose
  parts round their own grounds to its corners keeps its outline.
- `controls::outside` and `controls::dimmed`: the layer under an overlay
  that takes a click outside it, clear or dimmed (dialogs by 60%,
  settings cards by 70%, dialogs over them by 67%).
- `controls::lit_style`: a button with no face, or a fixed one, lit under
  the pointer: icon buttons, list rows, pills; `controls::outlined_lit_style`
  a card that's a button, in a 1 px edge that may light too;
  `controls::ink_style` a button that is only its text, brightening under
  the pointer.
- `Look::menu_item` with `controls::Item`: a row of a menu or a list of
  choices, lit under the pointer or, in a context menu, where
  the keys put the light (`menu_item_sized` at a height of its own); `controls::menu_panel` and `separator` are the
  menu's ground and the line between its groups.
- Submenus (`menus.rs`): a `MenuRow` with a `submenu` shows a chevron and
  opens it beside itself, its first row level with the row, under the
  pointer, on a click or with Right; on the left when the right has no
  room, and kept in the window. Left or Escape closes it, the pointer onto
  another row too. A row's `click` is what a click on the row itself does
  instead (a mute's "until I turn it back on"); a row's `key` carries what
  it's about (a role, a server, a command, a channel) for `sub_act`. A
  `Check::Box` row ticks in place and keeps the menu open, in the menu or
  a submenu; a `Check::Current` row's tick makes every row of its menu
  leave room for one. A menu or submenu is as wide as its widest row, at
  least its own width, measured once as it first shows. `Place::beside`
  lays the submenu out next to a row found by its path in the menu's
  layout, so the menu never moves.
- `controls::outside`: the full-window layer under an overlay that takes
  a click outside it, clear, or dimmed under a dialog.
- `widgets::Below`: its child laid out, drawn and pointed at in the room
  under the window's own title bar as if that were the whole window, and
  told to operations as a scrollable that can't scroll, so what they find
  is where it shows.
- `menus.rs`: a menu taller than the window scrolls, at most the window's
  height less a margin each side, and `Place` keeps one below the pointer,
  above it, or else pinned to the window's foot.
- `dialog::dialog`: the frame of every dialog over the dimmed window;
  `dialog::panel` a dialog of a width (and height) of its own on the
  chat's ground, as Roles' questions and Add members are, a click on the
  dimmed window sending its own message.
- `reorder::Reorder`: a list of rows that dragging reorders: a row taken
  by the strip at its left (its handle, for those that may move) follows
  the pointer, lifted on a ground of its own, the rows between pushing
  aside for it, and where it's let go, rounded to whole rows, is sent as
  from and to for its owner to move.
- `widgets::scroll_view`: the one scrollable, built as iced's
  `scrollable` (`id`, sizes, `on_scroll`, `anchor_bottom`; `across` for
  one that scrolls sideways, `style` for its bar's inks, `barless` for
  none), whose wheel and touchpad move it smoothly
  (`widgets::flick`, the motion, tested alone). A `Smooth` round iced's
  scrollable takes the wheel's events a `Claim` round what it holds
  marks as nobody's, and moves the scrollable on the window's frames
  through an operation (`scroll_by`), so the scrollable's own reports go
  on as they did; `swipe` is a touchpad's move for tests. On the CPU a
  moving list asks for at most 60 frames a second (the display's own rate
  on the GPU): each frame draws the whole list again.
- `widgets::Autoscroll`: round the whole window, a middle click's
  autoscroll of the list under it (see Autoscroll in
  [the window](window.md#the-window-and-the-desktop)).
- `widgets::CtrlWheel`: round the whole window, the wheel with Ctrl held
  zooms and scrolls nothing under it; what a turn comes to is the app's
  (`zoom::Wheel`, kept in the widget's state).
- `widgets::Drags`: a row that a press moved 4 px drags and a press let
  go sooner clicks; while a drag is on it says when the pointer comes onto
  it and leaves, a release on it clicks nothing, and it hides the cards of
  what's under it (the sidebar's channels and categories, and the people
  under a voice channel, dragged to another).
- `widgets::Splitter`: the edge between two panels, which a drag moves:
  a 1 px line taking the pointer 3 px either side, lit 2 px wide under
  the pointer and while held, with the resize pointer; it sends the
  distance dragged, the release and a double click (the thread beside's
  width).
- `Look::icon_button`: every icon button (`controls::IconButton`), built
  up from an icon, its label and its message: 28 px
  with an 18 px glyph in the secondary ink unless `size` says otherwise,
  lit by the overlay's wash under the pointer, its tip above it or
  `below`, no tip for an empty label, greyed and faint without a message.
  `fill` puts it on a ground of its own (a tenth of
  the overlay lighter under the pointer) and `radius` rounds it; `danger`
  makes it red on a red wash under the pointer, on a ground or none;
  `Look::face_button` shows a face of its own instead of an icon.
- `elide::elide`: one line of text cut to its room with "…" (iced's
  text only wraps or clips): as wide as its text
  up to the room its row leaves, so what follows it follows the text, or
  `centred` across the room. Names in the lists, channel rows and table
  cells use it.
- `controls::tip` and `tip_style`: the tooltip.
- `info_cards::frame`: a card over the window at what was clicked (the
  role's, a mutual server's), on its ground with the edge and the raised
  shadow.
- `controls::tab`: a tab with a bar under it while chosen (the full
  profile's, the voice details').
- `controls::bar`: a box of one colour, for lines between parts of the
  window, bars and dots; `controls::placed` puts a layer of a stack at a
  point.
- `controls::count_pill`: a count in a red pill, on a ring or not, with
  the metrics of the rail's or a row's (`Count`).
- `looks::Status` and `Ctx::status_line`: what someone is up to on one
  line, cut with "…" where its row ends, as `people::StatusInk` writes it:
  `under_name` (12 px with the activity's icon, under a name in the member
  list, DMs, the account's card) or `words` (an emoji and words at a size
  of their own: friends, the profile's activity).
- `Look::glyph`: one of the kit's `icons/` at a size, in a colour, drawn
  once
  (by resvg onto a `canvas::Canvas`, in the renderer's own format) and
  kept in `icons::Icons` until the window goes or the theme changes;
  `Pic::boxed`
  centres it (or any picture) in a square, for a button's face or a badge;
  `Look::icon_button` is the icon button.
- `Ctx::avatar` and `Ctx::face`: an avatar, with its decoration and
  presence dot (`paint::status_dot`, offline's and invisible's a faint
  dot with a hole; lists that leave it off for offline people leave it
  out themselves).
- `people::Person` and `Ctx::person`: a person's row, their face, then
  their name over its line; `Ctx::person_name` draws a name in its style
  (`NameInk`), `Ctx::voice_spot` the voice channel someone is in, and
  `Ctx::list_heading` a list's heading.
- `person_menu::open_person_menu` and `person_menu::choose_person`: a person's menu
  and what its choices do, wherever the person shows.
- `members::nameplate`: a nameplate behind a row, as tall as the row and with
  its corners (`PlateRow::radius` where they aren't 8 px).
- `paint::NamePicture`: a display name painted whole in its style
  (gradients, strokes, stacked copies) and handed to the renderer once,
  its strokes past its text box: the profile's name, the Display Name
  Style window's tiles and previews.
- `markdown.rs`: text laid out as Discord's client does (`parse`,
  `plain`, `notice`), and drawn (`Ctx::body_view`, `Ctx::run_span`) as
  selectable pieces (`piece::Piece`), which send the link and
  selection messages they're made with.
- `style.rs`: container grounds (`fill`, `rounded`, `corners`, and
  `outlined` in a 1 px `edge`), `round` (a border that only rounds its
  corners, for a button's, a field's or a quad's style), a raised panel
  (`panel`: a ground in its
  edge over the raised shadow, as menus, suggestions and the picker are),
  the shadows (cards, raised panels and the
  settings' cards, the Display Name Style window, the timeline's floating
  bars, the hover bar), the scrollbar, and the colours drawn the same in
  every theme (`SMOKE`: black at 70%, the ground of marks and notes over
  pictures).
- `pictures::Face::new(url, name, size)` with `key`, `opacity`,
  `decoration` and `dot`: an avatar as `Ctx::face` draws it.
- `widgets/`: `Pic`, `Framed` (a profile frame's layers
  under and over a child, past its sides), `ColourPad` (a colour's
  saturation, brightness and hue, kept as `Hsv`), the virtualized `Rows` (rows across
  the list, or with `Rows::place` boxes of their own), the anchored
  overlays `Place` (at a corner, the pointer, over a point, or with
  `Anchor::Span` under a button or above it) and `Above`, `PlateRow`, `Watch`, `RightPress`,
  `Grip` (a child whose pointer takes another shape over a part of it, and a left press there
  sends its message before the child hears of it) and `Holds` (a
  child that keeps the pointer's moves over it from what it floats over; `Rows`
  hands events to its last row first, as a stack does), and `Settle` (a
  scrollable whose scroll reports go on only when they matter).
- `widgets::gauge` and `widgets::live_text`: a box painted from what it
  reads as it's drawn (a meter) and a line of text read so (a clock),
  each drawn again every so often while it shows, with no message.
  `Rows::measured` lays rows out at their content's height and hands the
  heights that differ to the app with no message, apart from those below
  the view of someone reading earlier messages. `Corner` holds a bar at a
  row's top right over its edge, `Fold` cuts a long child with a link
  under it, and `Scrub` says where along it it's pressed and dragged.
  A widget around one child is a `Wrap` with its `Hooks`, which say only
  what it adds (its layout, an event before or after the child, its
  drawing round the child's, a shift of the child's space, an operation
  passed on); the child gets
  everything else, its overlay included, so its tooltips show through.
  The widgets name no message of the app's: each is generic over the
  message it carries (`Rows<'a, M>`, `Hooks<M>`), and the app uses them
  with its one `Message`.
  `widgets::moved_to` reads where a move took the pointer, from the
  cursor as the widget sees it. `gradient_quad` fills a quad with a
  linear gradient.
- `media.rs`: `Shot`, a picture in a box (its own size, cut to cover,
  fitted within, blurred under a cover), and `Gallery`, a message's
  pictures laid out.
- `popover::Popover`: a widget with something that opens from it, above
  or below, in iced's overlay layer, placed by the widget's own bounds
  and kept in the window: suggestions over a field, a dropdown under its
  button. A press outside both can close it, and then goes no further.
- `picker::kit`: what a picker is built of: the tabs, the search field,
  the rail of sections, `Grid` (tiles placed at their own boxes, which
  `Rows::place` builds only near the viewport, with the sections the rail
  jumps to), a section's heading, `Look::star` (a favourite's star),
  `pinned` (a small thing at a tile's corner), the foot and the frame,
  and `Masonry`, the GIFs' columns.
- `tile::Tile`: a video tile, the one piece of the stage, the corner
  picture and a DM's call: its face (a frame, an avatar or a note), a ring,
  chips at its corners, `glass_button`s over the video and `pill`s.
- `viewer::Viewer`: something shown large over the whole window on a
  ground (a title bar with its buttons and a `subtitle`, the content
  fitted between, a foot under it, layers `over` it), for the stage full
  screen, a message's video full screen and the picture viewer.
- `controls::round_button`, `call_button` and `dock_button`: the call's
  round controls, the DM call's answer buttons and the dock's buttons;
  `controls::live_pill` LIVE; `controls::Slider` a volume slider;
  `controls::heading` a popup's heading; `controls::choices` a row of
  choices, one picked; `controls::Check` and `Look::checkbox` a menu row's
  tick or checkbox, and `Item::subtitle` a second line under its label.
- `widgets::Hover`: a layer shown over its child only under the pointer,
  over a shade of its own if it's given one (`shade`), drawn a pixel past
  the child and cut back by the layer's edge, so the child's last row keeps
  it.
- `widgets::Selectable`: text that selects on its own, as a read-only
  field does (a profile's name and username): a press and drag select,
  a double press a word and a triple the line (a drag after either goes
  on by them), Shift with a press runs it from its anchor, and the text
  pointer shows over it. The selection is kept in its tree state, drawn
  on the theme's `selection`; a left press elsewhere clears it, and so
  does a field taking the keys: a press here takes them by the fields'
  own count (`field::take_keys`), so one of the two answers Ctrl+A and
  Ctrl+C. Ctrl+C sends what's selected in the message it's made with,
  for the app to copy, and takes the key; with nothing selected the key
  goes on. `on_press` is sent as a press starts a selection, for another
  one the app keeps (the timeline's) to go.
- `found::Found` and `found::found_view`: a message found (pinned, a
  search's result, a thread's start) as a card: avatar, name in its role
  colour, when, eight lines of text, its first pictures, its poll's card
  (not to be voted in) and files; a click or Jump under the pointer shows
  it, and a link in its text opens without showing it.
- `header_popovers::View::frame`: a popover's frame under a header
  button: icon, heading, detail, buttons at its end,
  Close. `search::search_field`: a search box.
- `thread::SideComposer`: a composer beside the chat (the thread's, a new
  thread's or post's first message): its field, reply bar, files, attach
  and send. `thread::leave_question` asks before a form with changes
  closes.
- `form.rs`, the form kit Settings is built of and the editors to come
  will be: `title` (a page's), `heading` (a group's), `note` (what a
  control does, under it), `rule`, `group` and `page` (the spacing of
  each), `buttons`, `beside` (a stretched thing and one at its end) and
  `halves` (two equal columns); `Switch` (a label, what it does, the
  `toggle` at the row's end, the whole row a click; without a message
  greyed, with a lock saying why), `Dropdown` (the chosen option in a box,
  its options in a list under it that its owner opens and closes),
  `readout` (a value set elsewhere in a sunken box), `capture` (a key's
  box, "Press a key..." while recording), `field` (a form's text field),
  `entry` (a single-line field 40 px tall), `area` (a text several
  lines high with what's left of it),
  `colour_row` (a swatch, its name and its hex), `colour_card` (a
  colour's `widgets::ColourPad` over its code) and `card`. Rows of choices
  are `controls::choices` (`choices_within` for a wider row), sliders
  `controls::Slider` with `step`, `shown` (the value in other words) and
  `mark`. The `Unsaved` bar is an editor's bar once something changed, which
  `Shake` shakes red after leaving was refused (`Shake::start` takes the
  core's nudge and `Shake::step` its timer's steps, which the owner
  forwards as one event carrying a `ShakeStep`); `error` is what went
  wrong on a form or a dialog, in the danger's ink; `Dropdown::enabled`
  greys a dropdown and `Dropdown::height` sets its box's. `options` is a
  list of radio choices (`Choice`: a title in its own colour, what it
  means, greyed when not offered), one chosen. `tri_state` is Discord's
  three-way choice of a permission (`Tri`: deny, pass to the roles,
  allow; the chosen one filled red, grey or green), and `tri_line` a
  permission's row of it: the title (dimmed, with a lock saying why, when
  it can't change), the choice at its end, what it means, and a line.
- `server_settings::moderation::pager`: Previous, "Page 2 of 3" (or "2 of
  3") and Next, for Bans' and Members' footers.
- `sheet.rs`: the settings frame of a server's and a channel's settings
  (`Sheet`: the pages' column of `Nav` rows, the page, the close button,
  the bar over the page's foot, a page that fills its height for a list
  of its own); `backdrop`, the card over the dimmed window that Settings
  shares, and `nav_row`, a section's row in either.
- `suggest::emoji_button`: an emoji on a lit square, for quick reactions in
  menus and the hover bar. `controls::action(..).small()` and `.icon()`
  make an app's button; `Kind::Success` is the green one.
