# What you build with

What waves 1 to 3 built that later code builds from: the types a
view-model holds, the widgets a view calls, the pieces the screens share,
and the fixtures a shot starts from. Each entry names its entry point and
what trips people; the module's own doc (`//!`) has the rest and the Slint
lines it ports. Read this before you open `anomi-widgets/src`.

## Every view

- `use anomi_theme::prelude::*;` and return `impl View<Intent> + '_`
  ([iced-notes](iced-notes.md#the-types-every-view-uses)). Colours from
  `theme.palette()` in a style closure; sizes from `anomi_theme::metrics`;
  type from `typography` (`UI`, `MEDIUM`, `SEMIBOLD`, `BOLD`,
  `typography::size::{BODY, SMALL, CAPTION, HEADING, TITLE, ...}`); timing
  from `motion` (`QUICK`, `SLIDE`, `POPUP`...) through the `Motion` handed in.
- Named looks several places share: `catalog::button::{plain, bare,
  primary, secondary, danger, ghost, success}`,
  `catalog::container::{transparent, rail, sidebar, chat, panel, raised,
  tooltip}` and `container::middle(..)` (centred on its cross axis),
  `catalog::text::{primary, secondary, muted, faint, danger, link}`.
- Emoji in any text draw as Twemoji pictures, and a name face by
  `anomi_theme::fonts::name(face)`, once `fonts::install()` ran: the app,
  `anomi-shots` and every `tests/areas/main.rs` `support::settings()` do it.
  Don't pass `fonts::ALL` to iced yourself.
- Builders that end in `.view()` before they are a widget: `portrait`,
  `server_tag`, `status_text`, `voice_spot`, `icon_button`,
  `header_toggle`, `device_toggle`, `field`. Everything else below is a
  widget already.

## What a view-model holds

All plain data, `Clone + Debug + PartialEq`; pictures are `image::Handle`s
the app made once (clone them, never make one in a view).

| Type | Where | Notes |
| --- | --- | --- |
| `Avatar { initial, tint, picture }`, `Presence` | `anomi_vm::kind` | a person's decoration isn't in it: hold `decoration: Option<image::Handle>` beside it |
| `Expected` | `kind` | a picture on its way: `None`, `Waiting` (room kept), `Ready(handle)` |
| `Tag`, `Status` + `Doing`, `Spot`, `count_badge(n)` | `kind` | server tag, custom status or activity, voice channel, "99+" |
| `NameStyle`, `NameEffect`, `Nameplate { picture, palette }` | `kind` | `NameStyle::new(font, effect, &colors)` is `None` without colours; in a server use `.font_only()` |
| `Effect`, `EffectLayer`, `FrameLayer`, `FrameKind`, `FrameAnchor` | `kind` | a profile's effect and frame |
| `Live`, `Level`, `Elapsed` | `kind` | values that move on their own, read as the widget draws: a picture's newest frame, a level from 0 to 1, a time from its start (`Elapsed::Since(instant)`; `Stopped(length)` in fixtures); equal only to themselves or their start, so moving builds no view |
| `Icon` | `anomi_vm::icon` | every SVG of `ui/icons`: `Icon::BellOff` (CamelCase of the file name) |
| `Text` | `anomi_vm::text` | a message body as spans; `text::markdown(..)` parses, `Builder` makes a notice's |
| `Row`, `Menu`, `Spot` | `anomi_vm::menu` | `Row::new(text, intent).icon(Icon::Reply).danger().separated()`; `Spot::below(bounds)`, `Spot::pointer(at)` |
| `Layer`, `Opened` | `anomi_vm::shell` | what opens over the window; see [Layers](#layers-menus-and-popovers) |

## Widgets (`anomi_widgets::...`)

**People.**
- `avatar::avatar(&avatar, size, presence, Ground)`: the circle with its
  dot, whose ring is cut from the `Ground` it lies on (`Sidebar`, `Active`,
  `Hover`, `Panel`, `Raised`, `Chat`). `avatar::portrait(&avatar, size)`
  for the rest: `.shape(..)`, `.ground(..)`, `.presence(..)`, `.dot(..)`,
  `.opacity(..)`, `.decoration(Some(&handle))`, `.speaking(bool)`,
  `.motion(m)`, `.view()`. `avatar::circle`, `avatar::gap(size)` for a row
  without one.
- `name::styled_name(&text, row.style)`: `.size()`, `.weight()`,
  `.ink(Ink)`, `.width()`, `.opacity()`. Takes `Option<kind::NameStyle>`
  as it is (`name::Style` is that type).
- `nameplate::nameplate(row_content, Place::{Member, Dm, Account}, motion)`
  `.plate(&row.plate).selected(..)`; wrap the name column in
  `nameplate::extent(..)` so the plate knows where the text ends.
- `tag::server_tag(&tag, motion).on_open(..).view()`;
  `presence::status_dot(presence, width, ring)`,
  `presence::status_text(&status, motion).view()`,
  `presence::voice_spot(&spot, on_press, motion).view()`;
  `badge::badge(&count)` (empty draws nothing), `badge::discord_badge`.
- `effect::effect(&effect, motion).radius(card_radius).play(Play::Hover)`
  `.on_wanted(msg)` over a card; `frame::frame(&layers, Side::Back|Front)`
  `.large(bool)` behind and over it (order in `frame`'s doc).

**Pictures and covers.**
- `picture::picture(handle)`: `.side(px)` or `.width()/.height()`,
  `.round()`, `.radius(r)`, `.fit(Fit::Cover|Contain|Top)`, `.opacity()`,
  `.motion(m)` (moving pictures play only with it). Corners are real (a
  copy with them cleared), on any ground. A widget of your own that draws
  pictures keeps a `picture::Player` per picture (as `rail_item` does).
  `picture::blurred(&handle)` for a spoiler under `cover::spoiler(..)`.
- `picture::live(&live)`: a moving picture's newest frame, read from its
  `kind::Live` as it draws; a frame within two pixels of its box is drawn
  one to one, centred (`Shape::exact`).
- `skeleton::skeleton(motion)` (`.width/.height/.radius`),
  `skeleton::messages_skeleton(motion)`; `cover::banner(notice, on_dismiss,
  motion)`; `new_tag::new_tag()`.

**Controls.**
- `button::action_button(text, Kind, motion)`, `call_button`,
  `ping_toggle(on, motion)`: `.on_press(msg)`, `.enabled(bool)`.
  `button::icon_button(Icon, tip, motion)` (`.size`, `.glyph`, `.radius`,
  `.tint`, `.fill`, `.danger`, `.alert`, `.tip_side`, `.view()`; the tip is
  words of the view's own or a `String` made in it, `tr!("Save {}", name)`),
  `header_toggle(icon, text, tip, open, motion)`,
  `device_toggle(icon, tip, menu_tip, motion)`. All have hover, press,
  disabled, Tab focus with Space/Enter, and their tooltip.
- A button of a look of its own (a profile's theme, a call's bar):
  `button::Face::new(motion, |theme, hovered, enabled, open|
  Colours::new(fill, ink))` with `.leading(Icon)`, `.trailing(..)`,
  `.text(..)`, `.glyph`, `.width`, `.height`, `.padding`, `.radius`,
  `.on_press`, `.id`, then `.tipped(label, Side)`. It eases between looks
  over `QUICK` and takes Tab as the rest; `icon_button(..).face()` is one.
  `button::chevron(renderer, at, side, angle, colour)` draws a turned
  chevron for a widget of your own.
- `field::field(placeholder, value)`: `.on_input`, `.on_submit`,
  `.on_focus`, `.id`, `.draft(&Draft)` (a refused value survives),
  `.error`, `.password`, `.trailing(Icon, tint)`, and its look where a
  screen's differs (`.height`, `.radius`, `.pad`, `.text_size`, `.fill`,
  `.rim`, `.ring`, `.ink`, `.placeholder_style`), `.view()`;
  `field::search(.., motion)` is the sidebar's, and `field::steady(input,
  motion)` holds the caret of an input of other looks in an idle window,
  as the well does; `field::focus(id)` focuses one. A
  field the app focuses as it opens has a `pub const` id in its screen's
  module (`ownership::NAME`, `editor::BIO`, `inline_edit::FIELD`).
- `slider::volume(Volume, on_change, on_commit)` (`Volume::tip()` shows
  the value over the handle while dragged, as Discord's menus do),
  `slider::sensitivity(Sensitivity, on_change, on_commit)`,
  `slider::upright(value, on_change)`; `slider::level_bar(level)` draws a
  level the app hands it and asks for no frames.
- `tip::tip(content, text, Side, motion)`, `tip_icon(Icon, tint, text,
  motion)`, `tip_card` for a badge's card; a tip card shows only (it takes
  no pointer) unless `.holding(on_leave)`: then it takes the pointer, stays
  while the pointer crosses to it and 200 ms after it left both, and sends
  `on_leave` as it goes (the card of who reacted). A click hides a tip until
  the pointer leaves, unless `.stays()` (the rail's).
- `layer::layers(base, motion)`: under a menu, picker or card (a layer
  that keeps the pointer, no dim), the base's overlays, such as a message's
  hover bar, stay drawn under the cards and take nothing.
- `icon::icon(Icon, size)`: an `Svg`, muted until hovered; `.style(..)`.

**Lists and text.**
- `list::list(count, |i| key, |i| row(i), motion)`: only rows near the view
  are built. `.id(..)`, `.from_end()`, `.follow(bool)`, `.on_viewport(..)`
  (visible keys, for loading pictures), `.on_leave(..)` (the pointer
  left it: a lit row goes dark), `.reorder(may_drag, may_drop,
  on_drag)` with `.drag_ground(..)`, `.drag_copy(..)`, `.drop_line(false)`,
  `.drag_handle(px)`, or `.live(onto)` for rows that make room: the owner
  moves the dragged row as `Drag::Aimed` says (above, onto or below a row,
  "onto" after `list::DWELL`, or past the last row in the room under it)
  and the rows slide to their places (the
  rail's servers); `.estimate(px)` for rows of one height,
  `.report_pixels(true)`, `.reveal(..)`. `list::scroll_view(content,
  motion)` for one piece that scrolls, `List::with_rows` for a few. Tasks:
  `list::operation::{jump_to, to_end, to_start, scroll_by}`. Use it, not
  `scrollable`, for anything that can grow long.
- `selection::selection(list)` around a timeline, each row in
  `selection::row(index, key, content)`, a name in `selection::whole(..)`,
  a picture in `selection::picture(..)`.
- `paragraph::paragraph(&text)`: `.size`, `.color`, `.max_lines(n)`,
  `.fold(Fold, toggle)`, `.on_link(..)`, `.on_right_press(..)`, `.motion`.
- `composer::composer(&draft, motion).key(room)`: `.on_edit`, `.on_send`,
  `.on_suggestion`, `.on_paste`, `.marks(..)`, `.placeholder(..)`;
  `composer::place_caret(id, at)` once the app put text in.
- `clock::clock(elapsed, format, motion.front)`: a time that runs on its
  own, as text (the call's length in the voice dock): `.size`, `.font`,
  `.line_height`, `.style`. It shapes its text at each frame and asks the
  next frame at the next whole second, only while the window is in front;
  a new second builds no view.
- `shake::shake(content, nudge, motion)`: Discord's bar of unsaved
  changes, 60 px on the raised ground under a line, that sways and turns
  red each time `nudge` goes up (leaving was refused); `.height`,
  `.radius`, `.shadow`. The settings kit's `unsaved_bar` and the profile
  editor's bar are made with it; text in it inherits its ink.

## Layers, menus and popovers

A screen never builds a layer. The shell (N01) draws every open
`anomi_vm::shell::Layer` with its area's view (`shell/layers/content.rs`)
and its look (`look` in `shell/layers/mod.rs`: dialog, sheet, popover,
header popover, screen): backdrop, Escape, focus kept inside, the way in
and out. Your dialog or popover is a plain view of your view-model.

- Opening one is the app's: your view sends your area's intent, carrying a
  `Spot` when it opens beside something. Wrap the button in
  `menu::opener(button).on_press(|bounds| Intent::Pins(Spot::below(bounds)))`
  (or `.on_right_press(|at| ..Spot::pointer(at))`): it knows where it is in
  the window, scrolled or not.
- A shot or test opens it on the fixture: `fixtures::shell::open(vm,
  Layer::Pins)` or `open_at(vm, layer, spot)`.
- The window's context menu is `vm.menu`, rows of `menu::Row` carrying the
  window's `Intent`; the shell places it. A menu a screen lays out itself
  (a pick list, a submenu in a card) uses `menu::item(&row, motion)` per row;
  `menu::context_menu(&rows, spot, motion)` for one of its own. In such a
  card, `menu::beside(item, &row.submenu, open, motion)` shows a row's
  submenu beside the card (flipping left without room on the right), and
  `menu::keyed(card, on_key)` hands the card the arrows, Enter and, while
  a submenu shows, Escape as `menu::Key`s for its view-model to move by
  (`voice::devices`). `Row::radio(chosen)` ends a row with a round mark.
- Keys: `focus::{COMPOSER, SEARCH, TIMELINE}` are the shared ids; move the
  focus with the tasks `focus::{into, select, to, next, previous}`.
  Ctrl+K, Alt+arrows, Escape and typing anywhere are the shell's scope.

## Pieces the screens share

- **Menus.** `menu::item(&row, motion)` per row of a menu laid out in a
  card; `menu::width(&rows, narrowest)` (`menu::NARROWEST` by default) for
  a card that holds one; `menu::place::{MARGIN, HEADER_CLEARANCE}` and
  `place`, `room`, `cleared` for what a screen places itself;
  `menu::text::width(words, size, font)` measures a line.
- **Settings** (`anomi_settings::kit`, the crate's own): `switch`,
  `choices`, `picker` (36 px, Fluent's chevron), `area`, `link`, `card`,
  `heading` and `title`, `sections` and `nav_row`, `unsaved_bar`,
  `recorder`, `words`, `pressable` (a part made of widgets that presses as
  one, fading the text it inherits) and `pressable_on` (with a ground that
  lights under the pointer), `reach(px, list)` (a table's rows reaching
  past the page, as Slint's `ListView` with `x: -8px`), and `Hot` and
  `fill` for a part of a page's own that lights as the kit's do.
- **Settings' dialogs** are layers over the server's or a channel's
  settings (`Layer::Ban`, `Ownership`, `RoleConfirm`...): a card of the
  page's, drawn from its view-model while it holds one, darker behind and
  in the middle below the bar. Their close is the shell's
  (`shell::Intent::Close(layer)`), which the app answers with the page's
  Cancel.
- **Chat.** `search::card` (a found message, the pins' too) and
  `composer::suggestions::view_at(.., gutter)` over a field of another
  width.
- **People.** `activity::card::view` (Active Now, the full profile and the
  editor's preview), `editor::colour::view` (a theme colour's and a name
  style's picker), `profile::card::card(.., Place)` (the popout, the
  member list's panel, the full profile's side).

## Fixtures, shots and tests

- A fixture starts from the whole window: `fixtures::shell::dm_home()` or
  `server_home()`, then fills its own field; `fixtures::shell::{channel_with,
  dm_with}` put a conversation in.
- Pictures: `fixtures::pictures::pictured(avatar(initial, tint), name)`
  gives the fake's people theirs by name (Alice, Bob, Carol, Erin, `Person
  N`, `Member N`, `Writer N`), `pictures::server("Server")`, and by path
  `avatar(user, hash)`, `icon(server, hash)`, `picture(path)` (banners are
  wide), `badge(id, side)` (custom emoji, stickers), `sunset(w, h)`. Moving:
  `anomi_widgets::picture::sample::bouncing` (in `shots()` only, as
  `anomi-vm` can't reach the widgets). Plates, effects and frames:
  `nameplate::sample::{strip, palette}`, `effect::sample::petals()`,
  `frame::sample::brass()`. Statuses and tags: `fixtures::kind`. Message
  bodies: `fixtures::text`.
- Shots: your module's `pub fn shots() -> Vec<Shot>`, each
  `Shot { area, name, size: GOLDEN, theme: "bonfire", vm: your_fixture }`,
  named after the golden screenshot. `anomi-shots --only area/name
  [--scale 2] [--language en]` renders them.
- Tests: `tests/areas/<module>.rs` builds
  `Simulator::with_size(support::settings(), SIZE, view)` over its area's
  view (`support::front()` for a window in front) and compares
  `support::asked(ui)`; anomi-screens' `support::simulate(&vm)` draws the
  whole window. Click by the code's English text: tests choose no language.
