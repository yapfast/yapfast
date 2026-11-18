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
- Pictures, R4 (1, 2, 6, 7, 10, 13, 24): an `Rgba` handle drawn from its own
  premultiplied BGRA bytes, pictures blitted at their own size with round
  corners, scaled copies kept: they change what `anomi-widgets` hands the
  renderer. Yapfast's patch 5 uses patch 2's picture `blit`; the glyph blit
  here stands alone, and the picture one joins it in `blit.rs`.
- Quads and shadows, R5 and R6 (3, 8, 11, 12, 17-21, 25): they change pixels
  (integer fills and gradients, whole-pixel edges at scales other than 1).
