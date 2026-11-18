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
it changes iced's, so the two lists can be held side by side. Every patch
here leaves the pixels as they were: the shots (`anomi-shots`, at 1x, 2x
and in English) come out byte for byte as with iced's own crate.

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
the clip; `blit::text_clip` keeps that, bounded by the damaged region being
drawn, which is cleared first, where iced laid those pixels again over
themselves on each frame the text was redrawn. Yapfast's patch clips such
text by its clip, which cuts glyphs where a clip is tight to its text (the
tail of "у" in a field, an emoji's top row); that is not taken. Yapfast
also scales a glyph's opacity in integers; here it is tiny-skia's float.
Tests: a glyph blit equals `draw_pixmap`, masked and not, at opacities 1,
0.5, 0.731 and 0, clipped at whole and fractional pixels and past the
frame's edges; `text_clip` keeps text inside its clip to the damaged
region.

To carry over: `text.rs` takes `clip_bounds: Rectangle` where it took
`clip_mask: Option<&tiny_skia::Mask>` and calls `blit::glyph` where it
called `draw_pixmap`; `Engine::draw_text` takes the damaged region instead
of the mask, and passes `blit::text_clip(physical_bounds, clip_bounds,
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

## Examined, not ported here

- 14 (each frame's layers with its background): iced's master already
  keeps `Frame { background, layers }`.
- 26 (`set_enabled(false)` keeps this renderer from starting): master's
  `backend::Settings` already lets the app choose the backend.
- 22 (a timing hook for the frame trace): the app has no frame trace on
  this renderer yet; port it with one.
- 15 (a quad's damage takes in its shadow's reach): with the shadows (R5,
  R6). Without it a shrinking or moving shadow leaves pixels outside the
  quad.
- 2 and 6 (a picture at its own size blitted onto the frame, partly
  transparent ones too), 7 (each corner its own radius in that blit), 10 (a
  scaled picture with round corners filled as its rounded rectangle):
  corners in the blit, the next stage of pictures, when `picture/look.rs`
  stops clearing corners. The blit puts a picture on whole pixels and
  scales its opacity in integers, which moves pixels, so the shots are
  taken again with it. Yapfast's patch 5 uses patch 2's picture `blit`; the
  glyph blit here stands alone, and the picture one joins it in `blit.rs`.
- 19's opaque pictures (copied without looking at each pixel's alpha):
  with patch 2's blit, which it speeds.
- 24 (a picture drawn at a size other than its own kept scaled to the
  pixels drawn): with the blit; it adds a copy for each such picture, which
  `picture` mostly needs none of, as it makes its copies at the pixels
  drawn.
- 13 (a hook the app draws its video frames with): with I09's video. The
  core's frames (`anomi-app/src/pictures/frames.rs`) are straight RGBA and
  still converted by the cache.
