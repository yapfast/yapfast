# Yapfast native sender-key patch

Source: `element-hq/matrix-rust-rtc` at
`52af3d1f4dc6af5c92ce5f8c64418fd97a8c4f57`, crate `matrix-rtc-livekit` 0.5.0.
The upstream license files and source headers are retained. Workspace dependencies
are expanded to their pinned sources; upstream test targets are omitted.

`LiveKitTransportConnection::reassert_local_key_index` is public so Yapfast can
move newly published native microphone/camera cryptors onto the active sender key.
The native publication and cancellable join facade now lives in
`vendor/matrix-rtc-call-sdk`; see its README.yapfast.md.

Upstream now reapplies the sender key after reconnects itself. Its two-peer-
connection setting is retained: the new transport documents black remote video
when publishing and subscribing through one connection.

Keep the MatrixRTC and Matrix SDK revisions aligned. The shared LiveKit version
is 0.9.7; livekit-signaling is pinned to 0.1.7 because 0.1.9 removed the
`SignalOptions::use_legacy_data_streams` field still used by LiveKit 0.9.7.
