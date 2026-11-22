# The vendored toolkit

`vendor/iced_tiny_skia` and `vendor/iced_wgpu` are iced 0.14's renderers
with Yapfast's changes, patched in through `[patch.crates-io]`; the crate
pins iced to 0.14.0 to match. Their `README.yapfast.md` files list the
changes, and each is marked `Yapfast port` in the source. The tiny-skia
ones draw pictures from the app's own pixels, copy pictures and glyphs onto
the frame directly, clip by rectangles instead of masks, merge damage,
draw video frames through the app's hook, keep each saved frame's
background with it so a theme switch repaints regions an older frame
painted in the old colour, count a shadow in its quad's damage so a
shrinking overlay leaves no shadow behind, keep each shadow worked out as
its nine slices and copy it in directly, write a region's clear and
plain fills into the frame directly, copy an opaque picture without
reading its alpha, start a region at the last opaque quad that covers it
whole, fill nothing for a quad of no colour (an outline over a card), draw
a picture at a size other than its own (all of them at a zoom other than
100%) from a copy scaled once to the whole pixels it covers and kept while
it's on screen, put quads on whole pixels at such a zoom, and
keep one scaler context for every glyph, so fonts' hinting is set up once
rather than for each text drawn; the wgpu ones fix two shader mistakes
and upload large pictures in the frame. Each has a switch that keeps it
from starting, for the GPU setting (`renderer::choose`).

`vendor/cosmic-text` is the text shaper iced uses, cosmic-text 0.15.0,
patched in the same way with its own `README.yapfast.md`: the shaper hands
the client each cluster its default, script and common fallbacks can't draw
and tries the families the client names for it next, and those fallbacks
match a family's face at the weight nearest the text's.
