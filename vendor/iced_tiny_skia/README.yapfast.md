# Yapfast's iced_tiny_skia

This is iced_tiny_skia 0.14.1 from crates.io under its original MIT licence
(as the crate's manifest declares; the published crate ships no licence
file). The iced client (`crates/yapfast-iced`, docs/iced-client.md) draws
with it on the CPU; the root `Cargo.toml` patches it in through
`[patch.crates-io]`, and `crates/yapfast-iced` pins iced to 0.14.0 so the
two stay matched.

Stock, it copies and converts every picture on screen, draws pictures,
glyphs and most quads through tiny-skia's f32 pipeline and a window-sized
clip mask, and draws a scaled video frame at a core's whole cost. Changes,
marked `Yapfast port` in the source and numbered there:

1. `raster.rs`: an `Rgba` handle's pixels are taken to be premultiplied BGRA
   already, as the client decodes pictures (`chat::set_pixel_buffers`), and
   drawn from the handle's bytes. Stock, the raster cache made a converted
   copy of each and kept it only while it was drawn in the last frame's
   damage.
2. `blit.rs`: pictures drawn at their own size, and glyphs, are copied onto
   the frame source-over, clipped to a rectangle, with the corners cut to
   the image's `border_radius` (stock ignores it). Only at scale factor 1
   and whole-pixel positions; anything else takes the stock path.
3. `engine.rs` `draw_quad`: a plain one-colour rectangle is filled inside
   the clip, with no mask and no anti-aliased path.
4. `engine.rs` `adjust_clip_mask`: clip masks are only ever rectangles here;
   only the rows of the old and new one are touched, and setting the same
   one again does nothing.
5. `engine.rs` `draw_text`, `text.rs`: text is clipped by its rectangle in
   the blit, with no mask.
6. `raster.rs`: the one-to-one copy also takes partly transparent pictures
   (offline avatars, nameplates).
7. `blit.rs`, `engine.rs`: each corner its own radius, for a card's banner,
   round at the top only.
8. `blit.rs` `gradient`, from `draw_quad`: a linear-gradient quad on whole
   pixels without border or shadow is filled directly inside the clip, as
   tiny-skia's `SpreadMode::Pad` gradient colours it.
9. `window/compositor.rs` `merged`: two damaged regions merge when their
   union adds no more than iced's own threshold (20,000 px), so a scrolled
   list is one region a frame.
10. `raster.rs`: a picture drawn scaled with a `border_radius` is filled as
    its rounded rectangle (`engine::rounded_rectangle`, now `pub(crate)`)
    with the picture as a pattern; stock filled the whole rectangle.
11. `engine.rs` `draw_quad`: a shadow is drawn only inside the clip, and
    only outside the quad's body when the fill is opaque; stock laid the
    whole shadow again over what was drawn for every damaged region.
12. `engine.rs` `draw_quad`, `blit.rs` `gradient`: a one-colour quad with
    round corners on whole pixels, or an opaque border round an opaque
    fill, is filled as patch 8's gradient of one colour; an opaque row clear
    of the corners is one copy.
13. `video.rs` (new), `raster.rs`, `lib.rs`: a hook the app sets once
    (`video::set_hook`) is handed each unrotated picture before the blit,
    with the clip and the target, and may draw it itself. The client's hook
    (`crates/yapfast-iced/src/video.rs`) copies or scales its video frames
    with libyuv, which the libwebrtc archive already links in; anything it
    declines draws as before.

14. `window/compositor.rs`: each past frame's layers are kept with the
    background colour they were drawn on, and a buffer whose background
    differs is drawn whole. Stock kept one colour for all of them, so after
    the window's background changed (a new theme), the next buffer, two
    frames old, kept the old background wherever nothing else changed.

15. `layer.rs` `Layer::damage`, `engine.rs` `shadow_reach`: a quad's
    damage takes in its shadow's reach. Stock counted the quad's bounds
    alone, so when a raised panel shrank or moved (the mention suggestions
    narrowing from "@" to "@B"), its old shadow stayed outside the new one.

16. `text.rs` `Pipeline`, `draw`: one `SwashCache`, the scaler context,
    serves every glyph drawn, keeping its fonts and their hinting
    instances. Stock made one for each text drawn, so every glyph missing
    from the glyph cache set its font's hinting up again, running the
    font's programs, at several times the cost of rasterizing it: a busy
    channel's new names and messages spent most of their glyphs' time
    there.

Unmarked lines are iced's. Updating iced means taking the new
iced_tiny_skia and carrying each marked change over.
