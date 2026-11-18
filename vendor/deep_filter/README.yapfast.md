# Yapfast libDF on tract 0.23

This is libDF, DeepFilterNet's Rust implementation, from
https://github.com/Rikorose/DeepFilterNet at `d375b2d`, the repository's last
commit, under its MIT or Apache-2.0 licence (LICENSE-MIT, LICENSE-APACHE, from
the repository's root). crates.io has only its 0.2.5, from before
DeepFilterNet 3.

`src/` is the repository's `libDF/src`. `models/` holds only
`DeepFilterNet3_onnx.tar.gz`, the model `default-model` builds in, from the
repository's `models/`; the other models' features won't build.
`Cargo.toml.orig` is the repository's manifest.

libDF asks for tract `^0.21.4`. It runs on tract 0.23, which takes a third
less time per 10 ms block than 0.21.4, loads faster and allocates less;
the commit that brought it in has the numbers. Changes, marked `Yapfast:` in
the source where they aren't plain renames:

- `Cargo.toml`: tract 0.23, ndarray 0.17 (tract's), no `readme`, which
  pointed outside the crate.
- `src/tract.rs`: tract 0.23's names for the same calls (`to_plain_array_view`,
  `into_plain_array`, `symbols`, a borrowed plan for `SimpleState::new`, a
  scalar read through `try_as_plain_ram`); `with_output_names` put back over
  `select_outputs_by_name`; `tvalue_to_array_view_mut` for tract's `TValue`,
  now a plain `Arc<Tensor>`; the model read from `../models`.
- `src/lib.rs`: allows ndarray 0.17's deprecation of `into_shape` and newer
  Rust's elided-lifetime lint, which a path dependency's build shows.

Update by copying a newer commit's `libDF` over these and porting the changes
again.
