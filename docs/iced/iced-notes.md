# iced at the pinned commit: notes for writers

The interface builds on iced's master at commit
`e5c350f271a8046f875c6662d452fde9e462d486` (2026-11-16, "Fix text selection
while scrolling", version `0.15.0-dev`, MSRV 1.93), pinned once in the root
`Cargo.toml`:

```toml
iced = { git = "https://github.com/iced-rs/iced", rev = "e5c350f2…", default-features = false,
         features = ["tiny-skia", "x11", "wayland", "wayland-dlopen", "image-without-codecs",
                     "svg", "canvas", "qr_code", "advanced", "crisp", "tokio"] }
iced_runtime = { git = "…", rev = "e5c350f2…" }  # anomi-shots: UserInterface
iced_test = { git = "…", rev = "e5c350f2…" }     # dev-dependency only
```

Read iced's source at this commit, not docs.rs, which documents 0.14: a
checkout is in `~/.cargo/git/checkouts/iced-*/e5c350f/` after any build.
Every pattern below is compiled and partly run by
[`crates/anomi-screens/tests/patterns.rs`](../../crates/anomi-screens/tests/patterns.rs);
when a snippet here and that file differ, the file is right. The other
snippets (the types, a widget of our own, the simulator) were compiled as
written in a crate of their own against this commit.

## Why master and not 0.14

Only master has what a chat client needs: `selectable` text and rich text
with selection across widgets (#3497), undo and redo in `text_editor`
(#3396), pictures in the clipboard (#3238), smooth wheel scrolling (#3479),
`popover`, `sticky`, `.ellipsis()`, fonts by family at run time. And
master's widget contract (`impl Widget`, `Meta`, layout written into the
tree) is what the next release will have; 0.14's would be a second port.

## Features left out, on purpose

`wgpu` (3-4x the memory, GPU driver threads waking at idle; keep it for
experiments), `linux-theme-detection` (zbus threads that wake), `thread-pool`
(a thread a core; we run our own executor), `wayland-decorations` (Anomi
draws its frame), `fira-sans` (we ship DM Sans; `iced_test` turns it on in
tests, which is fine). `image-without-codecs`: the core decodes pictures;
the PNG codec comes in through `anomi-widgets` for the logo. `canvas` is in
for paths (names drawn from glyph outlines, the ping graph), `qr_code` for
the sign-in's code.

## The types every view uses

```rust
use anomi_theme::prelude::*; // View, Element, Theme, Motion, iced::Widget as _

pub fn view(vm: &Area) -> impl View<Intent> + '_ { … }   // borrows the view-model
let either: Element<'_, Intent> = widget.boxed();          // when branches differ
let mapped = area_view.map(Intent::from);                  // into the parent's intent
```

- `View<M>` is `iced::Widget<M, anomi_theme::Theme, iced::Renderer>`. There is
  no `Element::new`, `From` or `.into()`; `.boxed()` makes an `Element`.
- The return type is what tells a widget its theme: iced's own `Theme`
  styles every widget too, so a widget built into a `let` with nothing
  around it naming ours fails with "type annotations needed". Build it
  where an `impl View<M>` is returned, or annotate:
  `let list: Element<'_, M> = column![..].boxed();`.
- `column![…]`, `row![…]`, `stack![…]` box their children; `column(iter)`
  and `Column::with_children(iter)` take one widget type, so box the items.
- Built-in widgets take `.style(fn)` only for catalogs `anomi-theme`
  implements. A missing one fails with "the trait `Catalog` is not
  implemented for `anomi_theme::Theme`": add `catalog/<widget>.rs`.

## Patterns (in tests/patterns.rs)

| # | Pattern | Use it for |
| --- | --- | --- |
| 1 | `match` arms `.boxed()` | a view whose widget type varies |
| 2 | `container(..).style(\|theme: &Theme\| …palette()…)` | any surface |
| 3 | `button(..).on_press(i).style(catalog::button::plain)` | rows and icons that act |
| 4 | `rich_text([span(..).font(..).background(..).border(..).padding(..), span(url).link(url)]).on_link_click(..).selectable(true)` | mention pills, links, selectable text |
| 5 | `text_input(ph, value).id(Id::new("x")).on_input(..).on_submit(..)` + `operation::focus(Id::new("x"))` | fields; focus is a Task |
| 6 | `text_editor(&content).on_action(..).key_binding(bindings)`, falling back to `Binding::from_key_press` | the composer |
| 7 | `scrollable(..).id(..).anchor_bottom().on_scroll(\|s\| ..s.viewport..)` + `operation::scrollable::scroll_to/snap_to_end` | timelines and lists |
| 8 | `keyed_column((key, row))` with a `u64` key | lists whose rows come and go |
| 9 | `tooltip(item, tip).position(..)`, `mouse_area(..).on_right_press(..)` | tips, context menus |
| 10 | `stack![base, container(..).align_right(Fill).align_bottom(Fill)]` | badges and overlays |
| 11 | `Task::perform`, `Task::run(stream, f)`, `Task::batch`, `.chain`, `.abortable()` + `handle.abort_on_drop()` | effects, backend streams |
| 12 | `event::listen_with(fn)`, `time::every(d)` only while in front | focus, clocks |
| 13 | `motion.animation(v, motion::SLIDE, motion::ARRIVE).go(t, now)`, `.interpolate(a, b, now)` | animations in state |
| 14 | `image::Handle::from_rgba(w, h, bytes::Bytes::from_owner(arc))` | pictures without a copy |

Notes on them:

- `rich_text` needs its link type fixed: `.on_link_click(Message::Link)`, or
  `.on_link_click(iced::never)` when no span links.
- `rich_text` redraws on hover only between "on a link" and "off", not
  between two links (#3533).
- `text_editor` has no "grow to N lines": set `.height(Fixed(..))` from
  `content.line_count()`. A key `key_binding` maps to `None` isn't captured
  and reaches `keyboard::listen` (#3467): always fall back to
  `Binding::from_key_press`.
- `Id::new("…")` takes a `&'static str`; for ids made at run time use
  `Id::from(String)` and keep them stable across views.
- iced doesn't re-export `bytes::Bytes`; add `bytes = "1"` where needed.
- `time::every` exists only with the `tokio` (or `smol`) executor feature,
  which the workspace enables.

## A widget of our own

[`anomi-widgets/src/rail_item.rs`](../../crates/anomi-widgets/src/rail_item.rs)
is the reference: hover and two animations in tree state, frames only while
one plays, and a picture drawn through `picture::Player`.
[`picture/widget.rs`](../../crates/anomi-widgets/src/picture/widget.rs) is
the smallest. Before writing one, look in [widgets.md](widgets.md): most of
what a screen needs exists. The contract at this commit:

```rust
impl<Message> widget::Meta for MyWidget<Message> {}             // one empty line, required;
                                                                // `impl widget::Meta for MyWidget {}`
                                                                // when the widget isn't generic
impl<Message: Clone> widget::Widget<Message, Theme, Renderer> for MyWidget<Message> {
    fn tag(&self) -> tree::Tag { tree::Tag::of::<State>() }      // state kept across views
    fn state(&self) -> tree::State { tree::State::new(State::default()) }
    fn size(&self) -> Size<Length> { Size::new(Length::Fixed(w), Length::Fixed(h)) }
    fn layout(&mut self, tree: &mut Tree, _r: &Renderer, limits: &layout::Limits) {
        tree.size = limits.resolve(w, h, Size::new(w, h));        // no layout::Node any more
    }
    fn update(&mut self, tree: &mut Tree, event: &Event, layout: Layout, cursor: mouse::Cursor,
              _r: &Renderer, shell: &mut Shell<'_, Message>, _viewport: &Rectangle) {
        // CursorMoved → hover changed → shell.request_redraw(); nothing redraws unasked.
        // ButtonPressed over it → shell.publish(message.clone()); shell.capture_event();
        // RedrawRequested(now) → advance animations; request_redraw() only while one plays.
    }
    fn draw(&self, tree: &Tree, renderer: &mut Renderer, theme: &Theme, _s: &renderer::Style,
            layout: Layout, _c: mouse::Cursor, viewport: &Rectangle) {
        // renderer.fill_quad(Quad { bounds, border, ..Quad::default() }, colour);
        // renderer.fill_text(Text { .. }, position, colour, clip);
        // renderer.draw_image(raster::Image::new(handle), bounds, *viewport);
        // renderer.draw_svg(vector::Svg::new(handle), bounds, *viewport);
        // renderer.with_layer(clip, |renderer| …);
        // A layer's quads draw before its pictures, then its text, and a
        // parent layer before the layers opened in it: what must lie over
        // children's text (a drag's ground, a scrollbar) goes in a layer
        // opened after theirs, never in the same one or the parent.
    }
}
```

- Implement `iced::advanced::widget::Widget`, not `iced::Widget`: the latter
  is the extension trait with `boxed`/`map`, given to every widget.
- Bring the renderer traits into scope to call their methods:
  `advanced::renderer::Renderer as _`, `advanced::text::Renderer as _`,
  `advanced::image::Renderer as _`, `advanced::svg::Renderer as _`.
- A widget with children: `diff` calls `tree.diff_children(..)`, `layout`
  lays each child into `tree.children[i]` and sets its
  `tree.children[i].translation`, and `operate`, `update`, `draw`,
  `mouse_interaction`, `overlay` forward to them through
  `layout.iter(&tree.children)`. Read `widget/src/container.rs` and
  `column.rs` at the pin, and forward `operate`, or focus, scrolling and
  text selection won't reach the children.
- `Layout` has no lifetime; `limits.bounds()` replaced `limits.max()`;
  `Widget::update` has no clipboard (clipboard is `iced::clipboard::*` tasks
  or `shell.write_clipboard(String::from(..))`; it takes a `String`, not a
  `&str`).

## Pictures and tiny-skia

- `image::Handle::from_rgba` makes a new id each call: make a picture's
  handle once and clone it (the app does: `anomi-app/src/pictures/`).
- tiny-skia converts each straight RGBA picture it draws to premultiplied
  BGRA and keeps that copy while each frame draws it, converting it again
  after one that didn't, so such a picture costs its pixels twice. A handle made
  by `iced_tiny_skia::premultiplied::from_rgba` (the vendored renderer's
  patch 1) has its bytes converted in place and is drawn from them, with no
  copy. `picture` makes its copies and its moving pictures' frames so
  (`look::handle`), so the store's eight MiB (`picture::bytes()`) are all
  they cost; what the renderer holds for the rest (the core's pictures
  drawn as they are, a spoiler's blur, fixtures) is
  `iced_tiny_skia::converted_bytes()`. Such a handle's bytes aren't RGBA:
  nothing but the renderer reads them, no copy is made from one, and only
  this renderer knows the mark (wgpu would swap their red and blue). What
  pictures hold once drawn, without a window: `cargo run --profile fast -p
  anomi-widgets --example picture_memory -- [COUNT] [SIDE] [DRAWN]`.
- **An image's `border_radius` rounds its clip**, as master's contract and
  its wgpu renderer have it; iced's tiny-skia ignored it (`TODO: Border
  radius`), the vendored one cuts it (`vendor/iced_tiny_skia/PATCHES.md`,
  patches 2, 7 and 10). Draw pictures with `anomi_widgets::picture`
  (`picture(handle).radius(r)` or `.round()`), or a `picture::Player` in a
  widget of your own: its copies are made at the pixels drawn, which the
  renderer copies in on whole pixels and cuts the corners of. A turned
  picture stays square.
- The `image` widget shows one frame; animation is ours (F05): a still the
  app registered frames for (`picture::register`) plays in `picture` and
  `Player` while its `Motion` plays, from one clock; video frames come from
  the core (`yapfast_core::picture::set_frame_buffers`, the app's).
- Copies are made at `picture::scale()`, which the app sets
  (`picture::set_scale`, I01) and `anomi-shots` sets to its `--scale`.
- **An svg turned by a quarter or a half turn draws nothing** on tiny-skia
  (`tiny_skia/src/vector.rs` sizes the raster by the transform's `sx`, 0 at
  90 degrees and -1 at 180). Swap to the glyph pointing the other way
  (`Icon::ChevronDown` for a turned `ChevronRight`, as a menu row's arrow
  does once turned), or stroke the chevron as a path and turn that
  (`anomi_widgets::button::chevron`).
- A transparent tint on an svg paints black: tiny-skia drops the tint's
  alpha. Draw nothing instead (`field(..).trailing(icon, Some(TRANSPARENT))`
  keeps the room and skips the picture).

## Headless rendering and tests

```rust
let mut ui = iced_test::Simulator::with_size(settings, Size::new(1400.0, 860.0), view);
ui.click("Friends")?;                              // by text, or iced_test::selector::id("x")
ui.point_at(Point::new(36.0, 133.0)); ui.simulate(iced_test::simulator::click());
ui.typewrite("bo"); ui.tap_key(key::Named::Enter);
let intents: Vec<Intent> = ui.into_messages().collect();
```

- `settings` comes from `support::settings()` in `tests/areas/main.rs`:
  it calls `fonts::install()` (Twemoji ahead of system emoji, name faces at
  every weight) and sets `typography::UI`. Never hand iced `fonts::ALL`
  instead: emoji then draw in the system's font. `ui.rebuild(next_view)`
  keeps the widgets' state and shows the next view-model.
- A test that reads pixels: `ui.snapshot(&theme)?.matches_image(tmp)` writes
  the PNG (2x), `image::open` reads it back; see `rendered` in
  `anomi-widgets/src/button/tests.rs`.
- `Simulator::snapshot(&theme)` renders at 2x and `matches_image(path)`
  writes the PNG only when missing, then compares. `anomi-shots` renders at
  any scale through `iced_runtime::UserInterface` and the headless renderer
  (`<Renderer as Headless>::new`, called through the trait).
- Selection and copy are operations: `operation::text::select(from, to,
  Target::Character)`, then `operation::text::copy()` (pattern test
  `rich_text_selects_and_copies`).

## The program

`anomi-app/src/run.rs`: `fonts::install()`, then
`iced::daemon(boot, App::update, App::view)` with `.title`, `.theme`,
`.subscription`, `.font(typography::UI)` and
`.executor::<executor::Small>()` (Tokio with two workers; iced's own
executors take a thread a core). A daemon runs with no
window, for the tray; `window::open(settings)` returns the id and a task.
`exit_on_close_request: false` makes the close button a message.

## Measured at the pin (version scout, this machine)

- Idle, window behind another, tiny-skia: 0 CPU, 0 wakeups. wgpu: 80 driver
  and 10 main-thread wakeups in 20 s, 3-4x the memory.
- A built message row costs about 51 KB of anonymous memory; a rebuild of
  1 000 rows 0.8 ms, of 5 000 5.2 ms. Build only what shows.
- A full 1920x1080 frame on tiny-skia: 3.9 ms at 1x, 14.8 ms at 2x.

## Open upstream bugs that touch us

#3496 a minimized Wayland window stops redrawing after ~20 min (GNOME,
NVIDIA) · #3488 `keyed::Column` can reuse incompatible state (panic) ·
#3381 large RGBA handles with new ids each frame can flicker on wgpu ·
#3368 tiny-skia is slow on dense text · #3306 tiny-skia may be replaced by
vello_cpu without damage tracking · #3515 selection offset in checkbox
labels · #3540 a second window resets the system theme · #3320/#3108
multi-window starvation on Windows · #3189 Japanese IME popup on Windows ·
#3467 unmapped keys in `text_editor`.

## Porting examples written for 0.13 or 0.14

| Old | At the pin |
| --- | --- |
| `fn view(&self) -> Element<'_, M>` and `.into()` | `-> impl View<M> + '_`, `.boxed()` where types differ |
| `impl Widget for X` | `impl advanced::widget::Widget …` and `impl widget::Meta for X {}` |
| `fn layout(..) -> layout::Node` | write `tree.size`; children in their own trees |
| `Layout<'a>`, `limits.max()` | `Layout`, `limits.bounds()` |
| `update(.., clipboard, ..)` | no clipboard argument |
| `on_scroll(\|viewport\| ..)` | `on_scroll(\|scroll: Scroll\| scroll.viewport ..)` |
| `iced::Renderer::new(..)` in tests | `<iced::Renderer as Headless>::new(..)` |
| tooltip styling | on the tooltip's content (#3544) |

## Moving the pin

A task of its own: change both `rev`s, `cargo update -p iced`, run
`scripts/check-iced --review`, re-measure idle (`scripts/measure-process`
on the skeleton, focused and not), and read `git log OLD..NEW` in the
checkout for `Widget`, `layout` and `operation` changes: master breaks the
custom-widget contract every few weeks, and every widget of our own follows.
The renderer is vendored at the pin (`vendor/iced_tiny_skia`): take the new
commit's `tiny_skia` and carry each patch over as its `PATCHES.md` says, set
its two iced `rev`s, then compare the shots with the old pin's, which must
come out the same.
