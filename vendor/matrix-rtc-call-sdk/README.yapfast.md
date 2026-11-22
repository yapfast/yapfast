# Yapfast native call facade patches

Source: `element-hq/matrix-rust-rtc` at
`52af3d1f4dc6af5c92ce5f8c64418fd97a8c4f57`, crate `matrix-rtc-call-sdk` 0.5.0.
The upstream license files and source headers are retained. Workspace dependencies
are expanded to the same pinned sources; examples and upstream test targets are
omitted.

The previous `matrix-rtc-livekit::Call` facade is now `LiveKitCall` in this crate.
Yapfast retains these additions:

- `join_cancellable` keeps the room alive while joining and closes it on a
  cancelled or failed join, with a ten-second cleanup deadline. Callers signal
  cancellation through a watch channel and keep polling through cleanup.
- `publish_audio_track` publishes a native microphone and reapplies the active
  sender key index. `reassert_local_key_index` also serves camera publication and
  LiveKit republication.
- `participant_identities` maps each membership using the transport's token flow,
  so both legacy JWT-service identities and MSC4195 homeserver-token identities
  reach the correct audio and camera feeds.

Keep all MatrixRTC packages at the same revision, and keep the Matrix SDK pin
identical to that revision's workspace. Remove these copies when upstream exposes
the native publication and cancellation operations Yapfast needs.
