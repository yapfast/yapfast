# Anomi's iced_tiny_skia

This is `tiny_skia/` of iced's master at the commit the workspace pins
(`e5c350f271a8046f875c6662d452fde9e462d486`, `0.15.0-dev`), under iced's MIT
licence (`LICENSE`, from iced's repository). The iced interface
(`crates/anomi-*`) draws with it on the CPU; the root `Cargo.toml` patches it
in through `[patch."https://github.com/iced-rs/iced"]`, so iced, `iced_test`
and `anomi-shots` all draw through it. Cargo ignores the patch unless the
name and version stay the git package's.

Beside iced's sources:

- `Cargo.toml` has iced's workspace fields and lints written out, and takes
  `iced_graphics` and `iced_debug` from the same commit, the other
  dependencies at the versions iced's workspace asks for.
- `clippy.toml` is iced's, so the crate lints as iced lints it.
- `Cargo.lock` is the root lock pruned to this crate, for its own tests.
  The crate is outside the workspace (`exclude` in the root `Cargo.toml`), so
  the Slint app's checks don't take iced in. `scripts/check-iced --review`
  checks its formatting and runs its tests:

      CARGO_TARGET_DIR=$PWD/target cargo test --locked \
        --manifest-path vendor/iced_tiny_skia/Cargo.toml --features image,svg,geometry

The patches are Yapfast's to its own copy of iced_tiny_skia 0.14.1
(`vendor/iced_tiny_skia/README.yapfast.md` in Yapfast), ported to this one.
Each keeps Yapfast's number, and the code says `Anomi port (patch N)` where
it changes iced's, so the two lists can be held side by side. Patches 16,
9, 23, 4, 5, 1, 3, 19, 21, 11, 17, 18 and 13 leave the pixels as they were:
the shots (`anomi-shots`, at 1x, 2x and in English) come out byte for byte
as before each. The picture blit (2, 6, 7, 10) moves some; it says which,
how many and why that's right. `anomi-shots --time FRAMES` times the
renderer drawing a shot whole.

## Ported

### 16. One glyph scaler for all text

`text.rs` `Pipeline`, `draw`. The pipeline keeps one
`cosmic_text::SwashCache` and lends it to every text drawn. iced made a new
one in `draw` for each text, so every glyph not yet in the glyph cache set
its font's hinting up again (running the font's programs), at several times
the cost of rasterizing the glyph. Only its scaler context is used
(`get_image_uncached`); its image caches stay empty.

To carry over: a `swash` field in `Pipeline`, made in `new`; `draw` takes
`swash: &mut cosmic_text::SwashCache` after the glyph cache instead of
making one, and the four `draw_*` methods pass `&mut self.swash`.

### 9. Neighbouring damaged regions merged

`window/compositor.rs` `merged`, called on `damage::group`'s regions before
`Renderer::draw`. Two regions merge when their union adds no more than
iced's own threshold (20,000 px) to what they cover together.
`damage::group` merges each region only into the one before it, in order of
distance from the window's corner, so the rows of two scrolled lists, taken
in turn, stay apart and overlap, and each region is drawn whole, its clear
and every layer in it. Tests: two scrolled lists come out one region each,
regions far apart stay apart, and a region grown by a merge takes in one it
had passed.

To carry over: if `iced_graphics::damage::group` still merges only
neighbours in its order, wrap it in `merged` where `present` groups the
damage.

### 23. The compositor told only what changed

`window/compositor.rs` `present`, `physical`. The buffer is presented with
`present_with_damage`, its damage the regions that differ from the frame
the compositor shows, the one presented last, in the buffer's pixels
(rounded out, within the buffer). iced presented the whole window on every
frame, so a compositor that copies a shared-memory buffer on each commit
(Weston at each repaint, Hyprland on its main thread, X11 with `XShmPutImage`)
copied the whole window for a blinking caret. The draw's own damage is
against the buffer's older frame (two back with two buffers), so when the
buffer isn't the one presented last, the compositor's damage is worked out
against the newest frame in `frames`. A new buffer after a resize, or a
changed background, presents the whole window as before. Test: the regions
in the buffer's pixels.

To carry over: in `present`, after `damage` and before the new frame goes
into `frames`, work out `shown` (`damage` itself when `buffer.age() == 1`,
else `damage::diff` against `frames.front()` when its background is the
same), group and merge it, convert it with `physical`, and present with it.

### 4. The clip mask kept as its rectangle

`clip.rs` (new), `engine.rs` `Engine::clip`, `draw_image`, `lib.rs`
`Renderer::draw`. The window-sized clip mask is set to a rectangle once for
each layer in each damaged region, again for each group of primitives and
each picture. iced cleared the whole mask each time and filled the
rectangle as a path. `Clip` knows which rectangle the mask holds: setting
another clears only the old one's rows and fills the new one's, and setting
the same one does nothing. Its pixels are those tiny-skia's fill without
anti-aliasing took, each edge rounded to the nearest pixel. A mask is empty
between draws (a new one is; `Renderer::draw` clears its rectangle at the
end), which lets a draw take the surface's mask or a screenshot's. Yapfast
kept the last rectangle in a thread-local keyed by the mask's address, which
a new mask at a freed one's address would fool. Test: a run of rectangles
(whole, half and fractional pixels, past each edge, the same one twice)
masks exactly the pixels stock's fill did.

To carry over: replace each `adjust_clip_mask(mask, bounds)` with
`clip.set(mask, bounds)` (`self.engine.clip` in `lib.rs`, `self.clip` in
`engine.rs`), add the `clear` at the end of `Renderer::draw`, and drop
`adjust_clip_mask`. If iced starts writing the mask elsewhere, that code must
go through `Clip` too.

### 5. Glyphs clipped by a rectangle, not the mask

`blit.rs` (new), `text.rs` `draw`, `engine.rs` `draw_text`, `lib.rs`. Each
glyph is laid onto the frame by `blit::glyph`, clipped to a rectangle; iced
drew it with `draw_pixmap` (a pattern shader through tiny-skia's f32
pipeline) and, for text not wholly inside its clip, first set the clip mask
to the clip. The blit works out each partly covered pixel with that
pipeline's own arithmetic (f32, its load, opacity and store rounding), so
the frame is the same; an empty pixel is skipped and an opaque one copied.
Text wholly inside its clip was drawn with no mask at all, so a glyph
reaching past the text's bounds (a descender, an emoji's edge) showed past
the clip; `reach::text_clip` keeps that, bounded by the damaged region being
drawn, which is cleared first (and since by the text's reach: "Regions drawn
alone as the frame drawn whole" below), where iced laid those pixels again over
themselves on each frame the text was redrawn. Yapfast's patch clips such
text by its clip, which cuts glyphs where a clip is tight to its text (the
tail of "у" in a field, an emoji's top row); that is not taken. Yapfast
also scales a glyph's opacity in integers; here it is tiny-skia's float.
Tests: a glyph blit equals `draw_pixmap`, masked and not, at opacities 1,
0.5, 0.731 and 0, clipped at whole and fractional pixels and past the
frame's edges; `text_clip` (`reach.rs`) keeps text inside its clip to
the damaged region.

To carry over: `text.rs` takes `clip_bounds: Rectangle` where it took
`clip_mask: Option<&tiny_skia::Mask>` and calls `blit::glyph` where it
called `draw_pixmap`; `Engine::draw_text` takes the damaged region instead
of the mask, and passes `reach::text_clip(bounds, reach, clip_bounds,
damage_bounds)` where it chose between no mask and the mask (the editor
always takes its clip, as iced's does).

### 1. Pictures drawn from their own bytes, when they say so

`premultiplied.rs` (new), `raster.rs` `native`, `Pipeline::load`,
`dimensions`, `draw`, `Cache::allocate`. `premultiplied::from_rgba(width,
height, rgba)` turns a picture's straight RGBA bytes in place into
premultiplied BGRA, tiny-skia's own format, with the conversion the raster
cache makes (`premultiply`, which the cache now calls too), and makes a
handle whose `Bytes` own them in a buffer that registers its address and
length while it lives. The raster pipeline draws a handle whose bytes are a
registered buffer straight from them (`PixmapRef::from_bytes`): no converted
copy in the cache, and nothing converted again after a frame whose damage
missed it. Any other `Rgba` handle is straight RGBA and goes through the
cache as in iced. Yapfast's patch takes every `Rgba` handle to be
premultiplied BGRA already; here the core's pictures, the fixtures and a
spoiler's blur are straight RGBA, so the mark is per buffer. The address
can be trusted: the buffer unregisters in its `drop`, before its memory is
freed, so no other allocation can begin at a registered address while it's
registered; the handle's `Bytes` keep the buffer alive while a clone of the
handle (a layer's) can be drawn, and it lends its bytes only to be read.
`converted_bytes()` says what the caches hold, for measuring.
`anomi-widgets` hands its copies and its moving pictures' frames over this
way (`picture/look.rs` `handle`). Tests: a marked picture draws byte for
byte as its stock copy (1x1 to 33x17, at its own size and scaled, at
opacities 1, 0.5 and 0.731, unscaled, zoomed by 1.25 and 2 and turned,
clipped and not, with both filters) and makes no copy; one not marked is
converted as iced did and its bytes are left alone; the mark goes with the
buffer, and a part or a copy of its bytes, or bytes of the wrong size,
aren't marked.

To carry over: add `premultiplied.rs`, `pub mod premultiplied` and `pub
use raster::converted_bytes` in `lib.rs`; in `raster.rs` have `load`,
`dimensions` and `draw` take `native(handle)` before the cache, and convert
in the cache with `premultiplied::premultiply`. If iced's `Handle::Rgba`
stops holding `Bytes`, the buffer needs another owner the handle keeps.

### 2, 6, 7, 10 and 19's opaque pictures: corners in the picture blit

`blit.rs` `whole`, `Rounded`, `picture`; `raster.rs` `Pipeline::blit`,
`draw`, `picture`, `Entry::opaque`; `premultiplied.rs` `opaque`;
`engine.rs` `draw_image`. An unturned picture drawn at its own size on
whole pixels (`whole`: its corner rounded to the nearest pixel, its size
rounded) is copied onto the frame, clipped to the layer's rectangle and cut
to its own clip's rounded rectangle, each corner its own radius (patches 2,
6, 7): opaque pixels copied, partly covered ones laid source over with
tiny-skia's f32 arithmetic (the glyph blit's), so a square picture comes out
byte for byte as `draw_pixmap` drew it through the mask; a pixel a corner
passes is scaled by its coverage (centre distance plus half a pixel, as
`anomi_widgets::picture` cleared its copies' corners). An opaque picture
(found while it's converted, kept with its marked buffer or cache entry,
patch 19) has its rows clear of the corners copied whole. A scaled one with
corners fills its clip's rounded rectangle with the picture as its pattern,
anti-aliased (patch 10); a turned one keeps square corners, as stock. iced
ignored the radius (`TODO: Border radius`), so `picture` cleared corners in
its copies; it no longer does (`look.rs`), and a round and a square use of
one size share a copy. Not taken from Yapfast: corners on the picture's
bounds (master's contract and its wgpu renderer cut those of the picture's
clip, `core::Image::border_radius`), a radius scaled twice by the scale
factor, integer opacity (it moves every see-through pixel), and patch 24
below. Also here, an Anomi fix: a scaled picture is drawn at its own corner;
stock truncated `bounds.x / width_scale`, up to a source pixel up and left
(a stream frame three pixels at 2x), which a clip at the picture then cut.
Tests in `blit.rs`: a square picture equals `draw_pixmap` (opaque or not,
three opacities, clipped, past the edges); a round one covers its circle's
area within a pixel, symmetric; each corner its own radius; a clip smaller
than the picture is cut at its own corners.

Pixels (1737 shots, 1703 differ): (a) 30,903 clusters of at most 2 levels,
the edge pixels of rounded corners, the coverage applied to premultiplied
pixels rather than straight alpha (274 shots have only these): neutral. (b)
9,343 clusters of more: pictures at half a pixel put on the nearest one,
where stock truncated, so their own clip no longer cuts their first row or
column (a voice tile's avatar flat on its left, a game icon's corner
missing its top row; at 1x only, 2x has whole pixels), and scaled pictures
at their exact corner (the stream frame, the viewer's pictures resampled a
fraction of a pixel over, 1 level): closer to the picture's place. None
regressed. `profile/full_board` 49.9 to 43.3 ms a frame at 1x, 190.9 to
164.2 at 2x; `members/group` 39.5 to 31.0 at 2x; text-only shots unchanged.

To carry over: `blit.rs`'s picture half, `Pipeline::blit` before the
stock draw in `draw_image`, the corner of `Pipeline::draw`'s transform, and
`opaque` beside `registered`.

### 3, 19 and 21: plain quads filled directly

`engine.rs` `draw_quad`, `whole_pixels`; `blit.rs` `fill`. A quad of one
colour with square corners and no border of any colour, on whole pixels
(each edge whole, or a pixel or more outside the clip, which masks it), is
filled inside the clip directly: an opaque colour written in, a
see-through one laid over with tiny-skia's 8-bit arithmetic (`s + (d (255
- a) + 255) >> 8`), as its anti-aliased fill through the mask leaves it.
A quad of no colour fills nothing (patch 21); its border is still drawn.
Anything else (round corners, a border, a gradient, an edge within a
pixel) is drawn as stock. Test in `engine.rs`: plain quads, whole and
fractional, clipped or not, opaque, half and 6% see-through and clear, with
no border, a clear one and a red one, come out as stock's fill and stroke.
No pixel moved: the shots are byte for byte those of the picture blit.
To carry over: `whole_pixels`, `blit::fill`, and the two early ways out
of `draw_quad` before its path.

### 11, 17 and 18: shadows worked out once and laid where they show

`shadow.rs` (new), `engine.rs` `draw_quad`. A quad whose shadow alone
reaches into the clip draws its shadow (stock skipped the quad, so a
damaged region over the shadow alone lost it). The shadow's pixels are
stock's, each by its formula on stock's grid (corner and size truncated),
kept as nine slices (one pixel of each straight stretch, which the pixels
along it equal exactly; a pixel short of it on each side kept whole), the
last eight by shape, and laid with `draw_pixmap`'s arithmetic only inside
the clip and outside the quad's opaque body (inset by its largest corner,
which its fill covers whole). Stock worked out every pixel of the reach
into a pixmap of its own each time the quad was drawn, then laid it through
the pattern pipeline and the mask. Tests in `shadow.rs`: shadows of four
blurs, offsets and corners (unequal ones too), at whole and fractional
places, past the frame's left, at 1x and 2x, clipped or not, drawn twice
(the second from what's kept), equal stock's byte for byte; a 1400 by 700
card's keeps under 120 by 120 pixels and lays none under its body. No pixel
moved. `profile/full_board` (a modal's shadow) 43.3 to 10.4 ms a frame at
1x, 164.2 to 35.6 at 2x; `menu/message_context` 12.1 to 5.9 and 46.3 to
21.3; `card/server` 10.2 to 4.8 and 39.6 to 19.8; a list (`members/states`)
5.8 to 4.7 and 22.6 to 18.0, these times with the screenshot's buffer and
its conversion to RGBA in them.

To carry over: `shadow.rs`, and in `draw_quad` the shadow drawn through
`Shadows::draw` before the early return for a quad outside the clip, the
plain fill, and the fill skipped when clear.

### 13. Video frames drawn from the core's memory, scaled by the app

`premultiplied.rs` `frame`, `video.rs` (new), `raster.rs`
`Pipeline::video`, `blit.rs` `visible`, `Under`, `engine.rs`
`draw_image`. A video frame is a new picture at every frame: stock, the
raster cache converted each into a premultiplied copy (3.7 MB for 720p)
and drew it scaled through the pattern pipeline. `premultiplied::frame`
makes a handle over the `Arc<Vec<u8>>` the app's frame buffer keeps,
already premultiplied BGRA, registered while the handle lives as patch 1's
buffers are; the registry counts owners, since several handles may share
one frame, and knows frames as such. Its bytes can't change while it's
registered: the writer writes the next frame with `Arc::make_mut`, which
copies them first while a handle holds them (the feed's four buffers keep
that from happening while frames are drawn in turn). At its own size a
frame takes patch 2's blit, a row copied at a time, corners cut. At
another size, opaque, unturned, at full opacity and filtered, it's offered
to the hook the app sets once (`video::set_hook`), which scales it into
the window within the visible part (`blit::visible`, as the blit clips);
the pixels under the corners are kept first and laid back under what it
drew with the blit's coverage and arithmetic (`Under`), so a scaled frame
is cut exactly as the blit cuts a picture of the hook's pixels. Anything
the hook declines, or every frame without a hook, draws as stock.
Yapfast's hook is offered every picture, recognises frames by memory it
registers itself and takes them as RGBA, swapping red and blue in the
copy; here the app turns a frame into BGRA where it's decoded
(`yapfast_core::picture::FrameBuffer::written`, on the decoding thread,
0.17 ms for 720p) and the hook only scales (`anomi-app/src/pictures/scale.rs`,
libyuv's `ARGBScaleClip`, whose filter leaves alpha at 253, set back to
opaque there). Tests in `premultiplied.rs`: a frame is drawn from its
writer's bytes, marked while any of its handles lives, its writer's next
write a copy while one does and in place after. In `video.rs`: a marked
frame draws as its stock picture (at its own size, on half pixels, larger
and smaller, square, rounded and with unequal corners, clipped, at 1x and
2x); a frame scaled by a nearest-neighbour hook equals the blit of the
scaled picture with the same corners and clip; only opaque frames drawn
unturned at another size, at full opacity and filtered, are offered, and
a declined one draws as stock. `tests/frames.rs`: thirty 720p frames
through the whole renderer convert nothing and each shows. No pixel moved
for the patch. The shots of a playing video's cell moved for `anomi-chat`,
which now gives its frame the cell's corners (8 px, 4 in an embed), as
rounding a frame no longer costs a copy of each: 15 of 1737 (four cells
and the cell under the upright video full screen, in each set): about a
hundred pixels at the cell's four corners at 1x, and some 2,450 across
the frame by 1 level, where the fixture's still frame, rounded, is copied
at its exact pixels (an area average) rather than scaled by tiny-skia.

Drawing 300 frames of 1280 by 720 through the renderer, each a new handle
(`tests/frames.rs`, `--release`; the scaled one with the hook in
`anomi-app`, `--profile fast`): one to one 0.76 ms a frame and 1,105 MB
converted before, 0.10 ms and none after; scaled to 960 by 540, 4.88 ms
and 1,105 MB before, 4.22 ms and none marked without a hook, 0.36 ms and
none with libyuv.

`premultiplied::frame_of` makes the same handle over memory any owner
keeps and lends: the app's frames of a message's video and of streams lie
in a slot of a mapping the stream worker writes, and the handle's owner
keeps the mapping alive while the renderer may draw it (the worker gets
the slot back only once the window drew a newer frame).

To carry over: `premultiplied::frame`, `frame_of` and the registry's `Entry`,
`video.rs`, `Pipeline::video` after the blit in `draw_image`, and
`blit::visible` and `Under`.

## Anomi's own

Not Yapfast's: found measuring a scrolled channel and a member list whose
presences change (`anomi-shots`' `examples/frames`, below). None moves a
pixel of a frame drawn whole (the shots come out byte for byte without
`anomi_widgets::picture`'s change below), and every frame drawn from its
damage equals the frame drawn whole (`--verify`) but where it did already:
a rounded quad cut by a region's edge, through the mask, by up to 3
levels, and at a fractional scale up to 9.

### Damage paired from the end, and paths compared

`window/compositor.rs` `layers`, `layer.rs` `Layer::damage`. iced pairs two
frames' layers by place, so a layer more or fewer early in the frame (a
row's picture scrolled in, the bar over the message the pointer is on)
paired every later layer with another: the member list, the composer, and
the whole window wherever a layer over it (a nameplate's, a list's
scrollbar) met one that isn't. Layers that end both frames with the same
bounds are now paired from the end, the rest by place as before; any
pairing damages what differs, so the frame is the same. And a canvas's
paths drawn live or grouped were never equal to the frame before's (`_ =>
false`), so every frame redrew them, the sidebar's thread lines and what
lies under them among them; they are compared by value now (`Primitive`
is `PartialEq`). A scrolled frame redrew 721,000 pixels on average, now
593,000 (every sixth was the whole window); a presence change 9,140, now
1,361.

To carry over: `layers` where `present` diffs layers, and the two arms of
the primitives' comparison.

### Quads clear of their corners within the clip

`engine.rs` `draw_quad`, `clip_pixels`, `clear_of_corners`,
`clear_of_border`, `cut`. Patch 3's direct fill takes a rounded quad too
where the clip's pixels lie clear of every corner's square (a damaged
region one pixel wide along a panel's rounded ground, which stock filled
whole through the mask: 0.24 ms a scrolled frame); a border that reaches
no pixel of the clip isn't stroked; and a quad square within the clip but
on half a pixel (a row's ground 349.5 px tall) has its path cut to a
pixel round the clip before tiny-skia rasterizes it. Test: rounded quads
(one, four and uneven corners, whole and fractional, with no border, a
clear one and two coloured ones) in clips along their edges, through
them one pixel thin, over a corner and whole, come out as stock's fill
and stroke through the mask.

### Regions drawn alone as the frame drawn whole

`engine.rs` `unmasked` and `touches`, `reach.rs` (new), `layer.rs`
`Layer::damage`, `lib.rs`. Four ways a frame drawn from its damage kept
pixels the frame drawn whole doesn't have, found scrolling #general at
output scales 1 and 2 and at zoom 1.25.

- A shape is drawn without the mask when every pixel it touches is one the
  clip lets through (`unmasked`: pixels whose centres are inside, as the
  mask is set and a region cleared). Stock asked `Rectangle::is_within` of
  the clip's fractional edges: the header's 3 px dot on y = 23.5, within a
  region starting there, laid the half pixel above the region over the
  frame before's again on each frame such a region came, 19 levels lighter
  after one. Quads, paths and SVGs alike.
- A shape is drawn in a clip when it touches a pixel the clip lets through
  (`touches`). Stock skipped it unless the rectangles overlapped: at zoom
  1.25 a region ending on y = 777.5 left the composer's layer the pixel row
  777, which the region cleared, but the composer's box, its top on 777.5,
  covers half of that row and wasn't drawn there: its edge stayed the
  ground's colour across the timeline's width.
- Text damages its bounds and `REACH` (8 px) round them, and its glyphs
  are cut there (`text_reach`, `text_clip`). Stock damaged its bounds
  alone, the box of its lines, but laid a glyph wherever it reached: an
  emoji's top row, in a timeline scrolled by wheel notches at scale 2,
  stayed behind as a strip 30 px long; so would the marks over Й and Ё.
  Cached text (menus, names) is damaged where its alignment puts it:
  stock took the box right of a centred line's anchor, half of it.
- Whether text lies within its clip asks of its layer's clip, not of that
  clip cut by the region drawn (`draw_text` takes the layer's own bounds):
  a region crossing a line within its clip cut its glyphs at the layer's
  edge where the frame drawn whole shows them past it.

A scrolled frame redraws 599,000 pixels where it redrew 593,000, a presence
change the same 1,361, and the frames' times stay within their noise. With
`--verify`, `scroll` at scales 1, 1.25, 1.5 and 2, by touchpad steps and by
wheel notches, `presence` and `churn` differ from the frame drawn whole only
as above (a rounded quad through the mask by up to 3 levels, at a
fractional scale up to 9). In the window, each frame presented was held
against the same frame drawn whole (a check in `present`, not kept): about
2,600 frames of #general scrolled by touchpad steps, wheel steps and eased
runs to both ends of its history, with the hover bar and a message's menu
showing, and a busy server's churn, at scale 1 (zoom 1 and 1.25) and output
scale 2 (zoom 1 and 0.9); none was further off than 3 levels. Tests:
`tests/damage.rs`, a case for each, drawn over the frame before as the
window's buffer keeps it and held against the frame drawn whole within
those 3 levels; each failed before its fix.

To carry over: `unmasked` wherever a draw chose the mask by `is_within`,
`touches` wherever it skipped a shape by `Rectangle::intersects`,
`text_reach` where `Layer::damage` took `Text::visible_bounds`, and
`draw_text`'s clips from `text_clip`.

### Rounding without `rintf`

`blit.rs` `ties_even`. The blits' f32 source over rounded each channel
with `round_ties_even`, a call to libm's `rintf` on a processor without
SSE 4.1, which the builds target: a sixth of a scrolled frame's drawing.
Adding and taking away 2^23 rounds the same (test: every 4096th of a level
from 0 to 255).

### Measuring

`stats.rs` (new), `window/offscreen.rs` (new). `stats::enable` has each
primitive's drawing timed by kind (a quad filled directly, through its
path, masked, with a gradient, a shadow or a border; a picture copied,
scaled or a video frame; text, SVG, paths) and the pictures drawn at
another size than their own listed; off, it costs a load a primitive.
`Offscreen` draws a window's frames as `present` does, into two buffers
taken in turn. `anomi-shots`' `examples/frames` drives the whole window
from its sample view-model as iced's loop does, paced at 60 frames a
second: `scroll` (#general's 400 messages, a 480x320 picture in every
fourth, scrolled 25 px a frame up and down by turns), `presence` (300
dressed members, one in view changing presence a frame) and `churn` (one
going offline or back a frame, moving the rows between); `--verify` holds
each frame against the frame drawn whole.

Per frame, fast profile, best of three (scroll / presence / churn):

| | scroll | presence | churn |
| --- | --- | --- | --- |
| before | 3.10 ms (draw 2.31) | 1.15 ms (draw 0.46) | 1.27 ms (draw 0.50) |
| damage paired, paths compared | 2.02 (1.25) | 0.80 (0.15) | 0.88 (0.13) |
| corners, `ties_even` | 1.80 (1.04) | 0.87 (0.16) | 0.89 (0.12) |
| pictures at their size (`anomi-widgets`) | 1.80 (1.04) | 0.75 (0.07) | 0.85 (0.08) |
| paths cut to the clip | 1.78 (1.01) | 0.76 (0.07) | 0.86 (0.08) |

What's left of a scrolled frame: events 0.14 ms, the view built again
0.22 (a message lights the row under the pointer, the list tells what's
in view), the frame's update 0.17, `draw` building the layers 0.19, the
damage 0.05, drawing 1.01, most of it the timeline's 593,000 pixels drawn
again (text 0.39). A presence change builds and lays out the whole window
again (0.35 ms, the member list's rows 0.06 of it).

Tried and dropped: moving a scrolled list's pixels in the buffer instead
of drawing them again (a layer whose paragraphs all moved by whole pixels,
the strip it uncovers and what changed drawn): 1.28 ms a scrolled frame,
181,000 pixels, but a row's content lies on half pixels (a picture's row
349.5 px tall), and f32 layout sums put the same picture or line on
101.49999 one frame and 51.5 the frame before, which round a row apart:
5 frames of 200 differed from the frame drawn whole by a row of a
picture's edge, and moved on wrong until something redrew it. Taking only
rows on whole pixels left little to move.

## Examined, not ported here

- 15 (a quad's damage takes in its shadow's reach): master's
  `Layer::damage` already grows a shadowed quad's damage by its offset and
  blur.
- 12 (one-colour quads with round corners and opaque borders filled by
  coverage), 8 (linear gradients filled directly), 25 (quads put on whole
  pixels at a scale other than 1): each moves pixels, corners by a
  coverage other than tiny-skia's 4x4 sampling and fractional edges to whole
  ones. Measured after the changes above (`examples/frames`), what they'd
  take is small: a scrolled frame spends 0.07 ms on rounded quads through
  the mask (the hover bar's buttons, the composer's edge) and none on
  gradients; a presence change 0.02 and 0.00, the nameplates' gradients
  being drawn only where a row changed. 20 (a region an opaque quad covers
  whole starts with it) would save the clear, 0.04 ms a scrolled frame.
  Port them with a measurement that shows more.

- 14 (each frame's layers with its background): iced's master already
  keeps `Frame { background, layers }`.
- 26 (`set_enabled(false)` keeps this renderer from starting): master's
  `backend::Settings` already lets the app choose the backend.
- 22 (a timing hook for the frame trace): the app has no frame trace on
  this renderer yet; port it with one.
- 24 (a picture drawn at a size other than its own kept scaled to the
  pixels drawn): `picture` makes its copies at the pixels drawn, at any
  scale, so they take the blit. What still reached the renderer scaled
  (`stats`) were pictures without corners only shrunk, which `picture`
  made at a mip level: a decoration (96 px, its mip 48, drawn at 38.4: 0.09
  ms each frame it was drawn), a tag's badge (16 at 14), a custom emoji.
  Those copies, up to 256 pixels a side, are made at the pixels drawn now
  too, smaller than the mip they replace (`picture::look::SMALL`); what
  stays scaled is a larger picture only shrunk (the viewer's, zoomed, which
  a mip level spares a copy at each frame) and one drawn larger than it is,
  for which a kept copy would add that size in memory. Pixels (1737 shots,
  676 differ, all at such pictures: decorations, tags' badges, custom
  emoji and stickers, the emoji picker's grid): 3 by at most 2 levels, 97
  by 8, 341 by 32, 235 by more, the copy's area average in place of
  tiny-skia's bilinear from the mip, and the picture put on whole pixels
  as rounded copies are, its edge up to half a pixel over (a ring's edge,
  a badge's corner): crisper, the same size within a pixel; neutral. The
  picture store holds less (the members benchmark 0.11 to 0.10 MiB, the
  scrolled one 0.28 to 0.26).
