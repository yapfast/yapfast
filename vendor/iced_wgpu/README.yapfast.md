# Yapfast's iced_wgpu

This is iced_wgpu 0.14.0 from crates.io under its original MIT licence (as
the crate's manifest declares; the published crate ships no licence file).
The iced client draws with it when its GPU setting is on
(docs/iced-client.md); the root `Cargo.toml` patches it in through
`[patch.crates-io]`.

Changes, marked `Yapfast port` in the source:

- `src/shader/image.wgsl`: an image's rounded edge at `0.5 - d`, as the quad
  shader and tiny-skia have it. Stock, `1.0 - d` put the edge half a pixel
  outside the box, and round avatars came out with flat sides.
- `src/shader/image.wgsl`: the offset passed to `rounded_box_sdf` negated,
  as the quad shader does. Stock, each corner took the opposite corner's
  radius, so a banner round at the top was round at the bottom.
- `src/image/cache.rs`: pictures up to 4K (`MAX_SYNC_SIZE`) are uploaded in
  the frame that draws them. A 1080p video frame comes as a new handle each
  frame; uploaded on the worker, it was never ready before the next one
  replaced it, and the stream never showed.
- `src/enabled.rs` (new), `src/window/compositor.rs` `with_backend`,
  `src/lib.rs` `Headless::new`: the renderer starts only once
  `set_enabled(true)` says it may, as the app does when its GPU setting is
  on. The client's build has both renderers, and iced's choice between
  them reads only `ICED_BACKEND`; off by default, a CPU start and the
  tests never start a GPU.
