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
marked `Yapfast port` in the source (with the patch's number where one
change spans several places):

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
11. `engine.rs` `draw_quad`: a shadow is drawn only inside the clip;
    stock laid the whole shadow again over what was drawn for every
    damaged region. (Patch 34 also keeps it from under the quad's body.)
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

17. `engine.rs` `draw_quad`: a shadow's pixels are worked out once for its
    whole reach and, when it's small (a hover bar's, at most 32 Ki
    pixels), kept: the last eight, by size, place within a pixel, corners,
    blur, offset and colour. Each frame copies the bands it
    needs from them. Stock, and with patch 11, every pixel was worked out
    again each frame, so the hover bar moving with a scroll, or a menu
    over a scrolling list, paid for its shadow in full every frame.

18. `engine.rs` `draw_quad`, `Sliced`, `blit.rs` `over`: a shadow is kept
    as its nine slices: the corners whole, each edge one row or column
    across, and its middle one pixel, which the straight stretches of its
    edges and its middle repeat exactly, so a 1400 px card's keeps a few
    thousand pixels and any shadow is kept, not only small ones. Each band
    is copied from it directly, source over; patch 17 filled the bands
    through tiny-skia's pattern pipeline, and a large shadow was worked
    out pixel by pixel again each time it was drawn. Tests in
    `engine.rs` compare it with the shadow worked out whole (outside the
    body, as patch 34 draws it).
19. `blit.rs` `fill`, `blit`, `engine.rs`, `raster.rs` `Opaque`, `lib.rs`
    `Renderer::draw`: a region's clear and a plain one-colour quad are
    written into the frame directly, in place or source over, not through
    tiny-skia's fill pipeline. A picture remembers whether it has any
    pixel that isn't opaque, and an opaque one is copied without looking
    at each pixel's alpha. A test in `blit.rs` covers the fill.
20. `lib.rs` `Renderer::draw`, `covered`, `within_corners`: a region that
    one opaque quad covers whole, clear of its rounded corners (each a
    square of its radius), starts with that quad: what layers and quads
    come before it is hidden, so they and the clear are skipped. A test
    covers the corners.
21. `engine.rs` `draw_quad`: a quad of no colour (an outline drawn over a
    card) fills nothing and only its border is drawn. Stock, its whole
    area went through the anti-aliased path and the clip mask. A test
    draws one clipped.
22. `timing.rs` (new), `window/compositor.rs` `present`: a hook the app
    sets once (`timing::set_hook`), only while its frame trace is on, is
    told of each frame presented: waiting for a buffer the compositor let
    go of, the damage diff, the draw, the present, and the damaged regions
    and their area, and the area patch 23 told the compositor of. Without
    a hook no clock is read.
23. `window/compositor.rs` `present`, `physical`: the compositor is told
    only what differs from the frame presented last (`present_with_damage`
    with those regions in the buffer's pixels), as Slint's software
    renderer tells it its dirty region. Stock presented the whole window as
    damaged on every frame, so a compositor that copies a shared-memory
    buffer into its texture on each commit (Hyprland does so on its main
    thread, Weston at each repaint) copied the whole window for a scrolled
    member list. The draw's own damage is against the buffer's older frame
    (two back, with two buffers), so the compositor's is worked out against
    the last one presented, unless the buffer is that one.

24. `raster.rs` `Resized`, `blit.rs` `resized`, `engine.rs` `trim`: a
    picture drawn unrotated at a size other than its own (every picture at
    a zoom other than 100% or a desktop scale of 125%) is put on whole
    pixels, its corner on the nearest and its size rounded (so a list
    scrolled by fractions of a pixel doesn't change it), scaled once to
    them (bilinear, in integers, as tiny-skia's filter samples) and copied
    in like one at its own size from then on. The copy is kept while the
    picture is in the frame's layers at that size, drawn or not, and goes
    with the first frame it isn't. Stock, and with patches 2
    and 10, such a picture went through tiny-skia's pattern pipeline in
    f32 on every frame: at 110% a scrolled frame of a picture-heavy channel
    took 17 ms to draw rather than 1. Tests in `blit.rs`.
25. `engine.rs` `whole_pixels`: at a scale other than 1, a quad that patches
    8 and 12 would fill directly but for not lying on whole pixels has each
    edge put on its nearest pixel's (so quads that meet still meet), and
    its border a whole number of pixels, one at
    least. Stock, each went through the anti-aliased path and the clip
    mask. A test in `engine.rs`.

26. `enabled.rs` (new), `window/compositor.rs` `with_backend`, `lib.rs`
    `Headless::new`: `set_enabled(false)` keeps this renderer from
    starting, as the app does when it draws with the GPU in a build that
    has both, so a GPU that fails to start is an error rather than a
    window drawn by this renderer from pictures made for the other. iced's
    own choice between the two reads only `ICED_BACKEND`.

28. `lib.rs` `start_layer`: a layer started inside another is cut to its
    parent's bounds, as a clip inside a clip is. Stock, a picture cut to
    its box (`ContentFit::Cover`) or a themed button's own layer inside a
    scrolled page drew over the page's header once scrolled under it.
    This also cuts a layer that means to draw outside its parent, as
    iced's `Float` widget does with its shadow; the app uses no `Float`.
    A test in `lib.rs`.
30. `engine.rs` `draw_image`, `narrowed_mask`: a picture or SVG is cut to
    its own clip (the box iced passes with it) as well as the layer's.
    The direct copy cuts by the rectangle; a turned picture, one filled
    through tiny-skia, and an SVG go by the mask, set to the narrower clip
    while it draws and put back after. Stock ignored it, so a
    `ContentFit::Cover` picture spilled out of its box over its neighbours
    (the creator's suggested pictures, a Zine card's leaning print). Tests
    in `engine.rs` (the SVG one runs with the `svg` feature).
33. `engine.rs` `draw_quad`: a quad with no area draws nothing. Square
    cornered, its path couldn't be built and the draw panicked (a theme's
    0 px corners on a card's empty banner).
34. `engine.rs` `draw_quad`: a shadow lies outside its quad, as CSS's
    box-shadow does: nothing is drawn under the quad's body (its bounds
    inside its largest corner), opaque or not. Stock drew the whole shadow
    under it, so a see-through quad (a theme's menu, its ground and art
    drawn under a quad with no fill) showed its own shadow through itself.
    The shadow tests in `engine.rs` leave the body out of the shadow they
    compare with.
35. `engine.rs` `smoothstep`: a hard edge (a shadow with no blur, as
    themes' offset shadows are) steps at its edge, taking in half a pixel.
    Stock, 0 / 0 gave NaN inside the shape and the whole shadow came out
    clear.
36. `raster.rs` `draw`: a turned or flipped picture (drawn through
    tiny-skia's `draw_pixmap`) is placed at its bounds' own position, then
    scaled. Stock, the position was divided by the scale and cut to whole
    pixels of the picture, so a small picture stretched far (a turned
    card's ground) landed up to a scaled pixel away.
37. `engine.rs` `Sliced::draw`: a kept shadow's reach counts every kept
    pixel and the whole stretch its straight one stands for. Stock had no
    slices; patch 18 undercounted by the last kept pixel's place when the
    stretch ran to the reach's edge, so a hard shadow on square corners,
    whose reach is all stretch, drew one pixel of itself.

Unmarked lines are iced's. Updating iced means taking the new
iced_tiny_skia and carrying each marked change over.
