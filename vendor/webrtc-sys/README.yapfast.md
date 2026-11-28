# Yapfast patches

This is webrtc-sys 0.3.55 from crates.io under its original Apache-2.0 license.
The native media patches below are retained across the upstream update.

The Windows `PeerContext` forward-declaration fix (LiveKit issue 1154) is now
included upstream. The local changes are the following.

## Capture leveling

src/capture.rs and the `CaptureLeveling` class in
src/peer_connection_factory.cpp hand every call's captured audio, after WebRTC's
processing, to a processor the application sets with
`capture::set_capture_processor`. Yapfast levels the voice there. build.rs lists
the new bridge.

## Remote track volume

LiveKit's Rust API can't set how loud a remote track plays. src/audio_track.cpp
keeps each audio track wrapper in a registry by track id, and the new
`set_audio_track_volume` bridge function passes a volume to the matching
tracks' sources on the signaling thread, as WebRTC's `SetVolume` expects.

## Camera capture from pooled buffers

LiveKit's `NativeVideoSource::capture_frame` takes a new I420 buffer for each
frame, and the encoder holds it for a while, so a camera filming at 30 frames
a second allocates 30 buffers a second. The new `capture_i420` bridge method
on `VideoTrackSource` copies the application's picture, scaled if WebRTC's
adapter asks for less, into a buffer from a `VideoFrameBufferPool` of four,
which WebRTC reuses once the encoder lets go. libwebrtc's Rust
`NativeVideoSource` keeps its native handle private, so
`last_video_track_source` returns the source most recently made on the
calling thread; Yapfast asks for it right after making one. `release_buffers`
empties the pool while the camera is off. Both are in
src/video_track.cpp and include/livekit/video_track.h.

## Pointer in Windows screen shares

Windows Graphics Capture leaves the pointer out of some monitor captures, and
DXGI and GDI never draw it. On Windows, src/desktop_capturer.cpp asks a screen
capturer for frames without the pointer and wraps it in libwebrtc's
`DesktopAndCursorComposer`, which draws the pointer once, where Windows says
it is. Window captures are as upstream.

The matching native archive is `webrtc-a545de4`, shared by the Nix and Windows environments.
