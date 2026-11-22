# Yapfast read buffer fix

This is tungstenite 0.30.0 from crates.io under its original MIT or
Apache-2.0 licence (LICENSE-MIT, LICENSE-APACHE). Yapfast's Discord gateway
reads through it.

`FrameCodec` reserves room for a whole frame in its read buffer and splits
the frame off the front. After a frame larger than the buffer, such as
Discord's READY, what followed the frame stayed in the frame's allocation,
so the buffer kept it for the rest of the connection: 7 MiB on a large fake
account.

Changes, marked `Yapfast:` in the source:

- After a frame larger than the buffer's usual size, what follows it moves
  to a new buffer of that size, so the large allocation goes with the
  frame's payload. `a_large_frame_leaves_its_allocation_to_the_payload`
  tests it.
