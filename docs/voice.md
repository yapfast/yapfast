# Persistent voice channels

Yapfast can join one encrypted voice channel at a time, wait alone, and keep
the connection while browsing text channels, other servers and DMs. Each join
starts muted and turns the microphone on as the call connects, unless Settings
› Voice & video turns that off. The dock under the rail and sidebar exposes
mute, deafen, audio devices and Leave. A caret beside mute and beside deafen
opens a menu that chooses the input or output device and links to the full
settings. Deafen also mutes the microphone; undeafening keeps it muted, while
unmuting when deafened undeafens too. The voice view shows the participants and
their mute, deafen and speaking state; a deafened person shows crossed-out
microphone and headphones in the channel list, and only the red crossed-out
headphones on their tile in the call. MatrixRTC doesn't share deafening, so in
Yapfast calls only your own shows. Clicking a voice channel joins it. Clicking
another one during a call opens a dialog that asks before leaving the current
one. Coming back to a server that had a voice channel selected shows it without
joining.

The native LiveKit audio engine captures and plays audio through system
devices. Settings › Voice & video selects the microphone and speakers too.
Yapfast remembers the choice in `devices.json` in its data folder; it never
leaves the machine. A remembered device that is unplugged gives way to the
system default until it returns. Refresh devices in Settings after plugging in
hardware. No microphone track is created until Unmute. Leaving releases the
audio handle, and restarting Yapfast requires joining again. Preview calls are
simulated, but the preview lists and chooses real devices without remembering
them.

## Cameras

Settings › Voice & video chooses the camera, beside the microphone and
speakers, and remembers it in `devices.json`. "Test video" films with it in a
preview there until stopped, or until settings close. An unplugged camera
gives way to the first one listed until it returns, as audio devices do.
`--audio-devices` lists cameras too.

Yapfast films through libwebrtc's own capture module, which its libwebrtc
archive already holds (`crates/yapfast-core/native/camera.cc`), through V4L2 on
Linux and DirectShow on Windows. The module reads the device on a thread of its
own. Yapfast registers for the device's raw frames, so the module hands over
the driver's buffer without converting it, and libyuv converts each one (YUY2,
MJPEG, NV12, RGB and the rest) into the same I420 buffer, frame after frame. It
asks for 1280×720 at 30 frames a second, as Discord's client films without
Nitro and as LiveKit's default camera does, and scales down a camera that can't
film that small.

A picture on screen, such as the preview, is a `picture::Feed`: whoever makes
the frames converts each straight to RGBA at the size it shows, in one of at
most four Slint pixel buffers the feed reuses, and the window shows that
buffer without a copy. Off screen, a feed converts nothing and lets go of its
buffers.

In a call that carries video, the camera button in the call's controls turns
the camera on and off. Each participant whose camera is on has a feed of
their own, and their tile in the grid shows it in place of their avatar; the
people list beside the channel marks them with a camera. Your own camera shows
mirrored, as a mirror would. Up to four people take the columns that make
their tiles largest within both the grid's width and height, so a call
above a DM's messages shows both people side by side without scrolling;
more take tiles at least 240px wide, and scroll. A click on a tile shows it
large, as Discord's focus does, with the others in rows of small tiles
beneath; a click beside it shows the grid again. The large tile goes full
screen from its button or a double click, over the whole window, until
Escape. Rust keeps which tile is large (`tiles::Focus`), by whose it is, so
it stays as people come and go. The grid tells Rust its columns, tile size,
scroll and large tile whenever they change (`call-tiles`), so each feed is
made at its tile's size in physical pixels, and only while the tile is in
view. A tile scrolled away, or a grid hidden behind settings, makes no
pictures and keeps no buffers. While the call is off screen, one camera
shows in the corner (`StreamPip`), made at the corner's size: the large
tile's, or else, with no stream to show there, the first of someone else's.
The others make none. Skia draws each frame from Slint's buffer in place
(`docs/building.md`). Preview, Matrix and Discord calls carry video.

A camera takes a moment to open, and longer to send its first picture, so it
opens on a blocking thread (`Camera::start`) while the call goes on. Until
the first picture, the call says the camera is starting: its buttons pulse
with "Starting camera...", and your own tile says so too. Clicking the button
then stops it. Only then does Discord's voice state say `self_video`, and a
Matrix call publish or unmute the track; a camera that fails to open turns
off again with the reason. Choosing another camera while filming opens it the
same way, after the old one closes. Listing the cameras looks at the devices
once, rather than once for each camera listed.

In a Matrix call, Yapfast publishes the camera to LiveKit as a simulcast VP8
track, asking for 1280×720, so each receiver can take the layer its tile needs.
Each picture goes to WebRTC through `capture_i420`, a Yapfast patch to
`webrtc-sys` that copies it into one of four pooled buffers rather than a new
one each frame (`vendor/webrtc-sys/README.yapfast.md`). Turning the camera off
mutes the track and closes the camera, as Element does. Unpublishing would
leave the track's sender and encoder in LiveKit's connection each time. The
track goes when the call ends.

Each remote camera arrives as WebRTC's decoded I420 frame, and its feed
scales and converts it straight into the tile's buffer, with no copy in
between. The tile's size goes to LiveKit (`set_constraints`), so the server
sends the smallest layer that fills it, and a camera whose tile is out of
sight, or muted, is paused at the server: nothing arrives to decode, and its
buffers go. `testing/measure_cameras.py` measures a five-person call as the
others' cameras and then Alice's turn on.

In a Discord call, Yapfast encodes the camera in H264 itself and sends it over
the call's own voice connection, and decodes each remote camera on one
thread, into its tile's feed; [Camera video](discord/voice.md#camera-video)
has the details.

libwebrtc's module was chosen over V4L2 directly, PipeWire's camera portal
and nokhwa: it adds no dependency, converts once into a reused buffer where
nokhwa makes a new one each frame, and films on Windows too.

The portal is in the same module: `VideoCaptureOptions` with PipeWire allowed
asks it over D-Bus, as screen sharing asks its own portal. Yapfast will turn it
on when it ships in a sandbox; until then V4L2 needs no dialog.

On Windows the same module films through DirectShow (`VideoCaptureDS`), from
the Windows libwebrtc archive, and libyuv flips DirectShow's bottom-up RGB in
the same conversion. The module's device list joins COM's multithreaded
apartment on the thread that opens a camera, and leaves it on the thread that
closes it. A camera there is opened, kept and closed by a thread of its own,
`yapfast-camera`, so no other thread's apartment is joined or left by mistake;
it's one thread more than on Linux, parked while the camera films. A camera's
id is its device path (`\\?\usb#vid_…&pid_…#…`), which stays the same across
restarts; a camera without a serial number may get a new one in another USB
port, and the first camera stands in until it's chosen again. A camera
without a device path, such as a virtual one, goes by its name. Cameras that
only Media Foundation lists, such as some laptops' built-in MIPI cameras,
aren't listed. [Windows beta](windows-beta.md#cameras-on-windows) has what
was checked under Wine.

`YAPFAST_CAMERA_TEST_PATTERN=1` adds a "Test pattern" camera that films moving
bars, for tests and machines without a camera. `cargo test -p yapfast-core
films_a_real_camera -- --ignored` films the first real camera for a second,
or the first whose name holds `YAPFAST_TEST_CAMERA`; a V4L2 loopback device fed
by `ffmpeg -re -f lavfi -i testsrc2=size=1280x720:rate=30 -pix_fmt yuyv422 -f
v4l2 /dev/videoN` works as one.

## Sounds

Yapfast plays its own short sounds when this account joins or leaves a call, and
when others connect or go while it's there. It also plays them on mute, unmute,
deafen and undeafen, and for messages that notify, with a brighter one for
mentions. People arriving together make one sound. Reconnecting makes none, and
someone a DM call is ringing makes one only once they answer. Muting sounds as
it's asked for, rather than once the call confirms it. Turning the microphone
on as a call connects, for the unmute-on-join preference, makes no sound, since
joining already did. While deafened, only deafening's own sounds play. A message
in the conversation in front of you makes no sound, and nor does any on Do Not
Disturb. Someone else starting or stopping watching this account's stream, or
the one it watches, plays the call's join or leave an octave higher, quicker
and quieter, as Discord's client sounds a stream's viewers; a call sound at the
same moment wins, and a stream starting or ending makes no viewer sound.

`sounds.rs` makes each sound as it plays. The six sets are New Kalimba, the
default, Felt piano, Soft marimba, Mellow Rhodes, Glow and Flute, all in F
major pentatonic so any two sound well together. Felt piano, Soft marimba,
Mellow Rhodes and Glow are synthesized; New Kalimba and Flute play the same
notes on one recorded note each, made with ElevenLabs' sound generator and kept
as 24 kHz samples in `crates/yapfast-core/sounds/`, played faster or slower for
each note. They play on the call's speakers through `audio_io.rs`, on Linux and
Windows, like ringing. Settings › Sounds chooses the set and the volume, from
0% to 200%. A slider's percentage becomes a gain by its square, which spreads
the quiet settings along more of it. At 100%, the default, the loudest sounds
peak 12 dB under full scale, and at 200% they reach it. Each sound can also
have its own volume up to 100%, as a share of that one. `layout.json` remembers
them. Desktop notifications ask the desktop to stay quiet, since Yapfast sounds
itself; someone who had turned the desktop's sound off starts with messages and
mentions at 0%.

WebRTC's PulseAudio devices report no GUID, and LiveKit selects devices only by
GUID, which would always choose the first device. Yapfast names such devices
`name:` and their reported name, and selects them by their current index
instead.

A call's own speakers and microphone, and a watched stream's sound, outlast
their device (`audio_io::LastingOutput` and `LastingInput`). Bluetooth
headphones that drop out end the stream on Windows, and on PulseAudio the
server moves it to another device and leaves it there when they return.
So a stream that fails opens again, on its device or on the default while
that's gone, and every 2 s it checks which endpoint it would open now and
moves there if that's another one. On PulseAudio that endpoint is the
sink's or source's name with its index, which a device gets anew each time
it comes back, so even a short gap brings the stream back. On Windows it's
the endpoint id, the default's while the chosen device is gone, so the
default also follows Windows when it changes. While nothing opens, the
stream keeps time with silence and tries again every half second. Settings
show a chosen device that isn't connected last in its list, as "(not
connected)", by the name it was last listed with; the call plays on the
default meanwhile. Each picker's selection is bound both ways, since a
ComboBox sets its own index as it's used, which undoes a one-way binding
and left it showing another device once the list changed. Yapfast calls'
own audio goes through WebRTC's device module, which these don't cover.

Right-clicking someone in a call, in the sidebar or on their tile, sets how
loud they sound from 0% to 200%, and their stream's sound too while they're
live. A drag snaps to 100% near it. The sliders follow Discord's, read from its
web client: below 100% the gain is `(slider / 100)^2.8`, so 50% is about −17 dB
and 40% a gain of 0.077; above it each 100% adds 6 dB (`volume::from_slider`).
A linear slider had a music bot at 4% to sound as Discord's 40% did. A
stream's sound plays at Discord's default gain of 0.18, about 54% on the
slider, and voices at 1.0. Yapfast remembers the gains by Discord or Matrix
user id in `layout.json`, so they hold in every call and after a restart. Discord
voice scales each person as it mixes the call, and a watched stream's worker
scales its sound. Yapfast calls play remote audio inside WebRTC, which LiveKit's
Rust API can't adjust per track, so Yapfast's patch to `webrtc-sys` sets each
track's volume through WebRTC's `SetVolume` by track id.

Settings › Voice & video has a volume under each device, as Discord's
does: "Microphone volume" sets how loud every call sends you, and "Speaker
volume" how loud every call plays, both from 0% to 200% on the same curve
as a person's volume, with 100% marked. Dragging plays each step at once,
and letting go saves the value in `layout.json` (`microphone_volume`,
`speaker_volume`). Everyone in a call, and their stream's sound, plays at
their own volume times the speakers' (`volume::heard`), so Yapfast calls
set it on each track as above and Discord voice mixes with it. The
microphone's volume applies with leveling, as one gain ramped across each
10 ms block, and like leveling it never pushes a peak past 90% of full
scale, so a boosted microphone doesn't clip on a shout; leveling hears the
voice before the volume, so changing it doesn't move your usual level.
Yapfast's own sounds keep their volume in Settings › Sounds.

Settings' "Even out your voice", on unless turned off, levels the microphone
after WebRTC's processing. Yapfast follows your usual speaking level over the last
half minute or so, and undoes up to 8 dB of drift from it as you lean towards
the microphone or away from it. Drifts of more than 12 dB, such as whispers and
shouts, pass through as they are, and a shout lets go of any correction at once.
Pauses hold the gain, and raising never pushes a peak past 90% of full scale.
Yapfast calls level inside LiveKit's WebRTC, through a capture hook in Yapfast's patch
to `webrtc-sys`. Discord voice levels in its own microphone thread; its voice
activation gate still opens on the level before leveling, so held gain can't
lift noise past it.

Yapfast calls suppress noise with WebRTC's own suppressor, inside LiveKit's
WebRTC. Discord calls use DeepFilterNet, which also takes out typing; see the
Microphone section of [Discord](discord/voice.md#microphone). Yapfast calls could share
it through the capture hook, which now only returns a gain for each block.

The input menu's level meter reads the chosen microphone locally through
`audio_io.rs`, raw and before any processing, only while the menu is open. It
works while muted, outside calls and in the preview, and it never touches the
call: muted, nothing is published or sent. It shows as "Input level" in the
system's recording streams on Linux. On Windows it reads the microphone
through WASAPI.

## Run the local demo

For testing with a friend on another machine, use two accounts on a
homeserver that offers MatrixRTC voice (the requirements are listed below),
in an encrypted space with a text channel and a voice channel.

To test on one machine, from the project folder, enter the development shell and start the services:

```sh
nix develop path:./nix
scripts/test-stack up
cargo run --locked --features matrix -- --voice-demo
```

The demo creates two disposable cross-signed accounts, a Matrix Space, an
encrypted text channel and an encrypted voice channel. It prints two commands
with separate data directories. Run each in a separate terminal. Select
**Yapfast voice test** in the server rail, then **Lounge → Join voice**. Both
accounts join with the microphone on, as Yapfast does unless Settings → Voice &
video turns that off, so mute one. Select devices from the carets beside mute
and deafen. Use separate machines or headphones for a conversation; two windows
on one computer share its audio.

List native devices without publishing a microphone:

```sh
cargo run --locked -- --audio-devices
```

Normal accounts require password sign-in, a verified device, an encrypted joined
room of type `org.matrix.msc3417.call`, an `m.rtc.slot` at `m.call#ROOM` with
`m.per_member` encryption, a homeserver supporting the pinned SDK's sticky events,
and a reachable MatrixRTC LiveKit service. Yapfast refuses missing or unencrypted
slots. Room creation is currently provided by the demo or another client.
There is no fallback to unencrypted media.

The local stack has an automatic authorization fallback at localhost:16080.
Other homeservers use SDK discovery. To configure a fallback in fish:

```fish
set -x YAPFAST_RTC_AUTHORIZATION_URL https://your-calling-service.example
cargo run --locked
```

The URL requires HTTPS except for loopback development addresses. A remote tester
needs a TLS deployment with reachable LiveKit media ports; the local Compose
stack binds to loopback. This slice does not establish a production SFU admission
policy.

## Validation and remaining checks

With the stack running, `scripts/test-voice` exercises encrypted native audio
using isolated Linux virtual devices and a generated signal. It does not record
the physical microphone or change the system's default devices. It covers a
30-second wait alone, a muted late join, native encrypted audio in both
directions, mute, deafen, undeafen staying muted and clean leave. It also
stalls authorization after membership publication, cancels with Leave, checks
membership removal and then joins again. The two clients run in separate
processes because LiveKit shares its audio engine within a process. The [pinned
dependency patch](../vendor/matrix-rtc-livekit/README.yapfast.md) reuses the
SDK's active-index handling, so native tracks first published after a key
rotation don't carry stale key indices. The call experiment
(`--call-experiment`) separately covers recovery, invalid keys and departure
key rotation. Release performance remains unverified.

Continue manual checks with the [Windows package](windows-beta.md) and two verified
accounts in the same encrypted channel:

1. Join alone and wait at least ten minutes. Join from the other machine; both
   users should appear.
2. Talk in both directions. Mute and deafen, then undeafen; the microphone
   should stay muted. Change both devices and confirm sound follows the
   selection.
3. Browse text channels and DMs during the call. Return to voice and leave;
   check that the other participant disappears.
4. Interrupt and restore the connection. Record whether voice resumes, or shows
   a useful failure and permits a fresh join. Close Yapfast during a call and check
   departure and the microphone indicator.
5. Record OS, GPU, device models, resource measurements and any audible glitches.

The controller shows reconnecting and allows up to 60 seconds before ending a
degraded connection. A joining request has a 35-second deadline. Leave cancels
a pending join immediately and waits up to ten seconds for membership cleanup;
normal leaving has the same cleanup deadline. Crash membership expiry uses a
five-minute sticky lifetime. Ringing DM/group calls and screen sharing are
later slices. Push-to-talk works as in
Discord calls; see [Discord's microphone](discord/voice.md#push-to-talk). A call goes
on with the window closed to the [tray](tray.md), whose menu can disconnect it.

## Reliability

Yapfast reports call problems instead of leaving them in the log:

- A participant whose device is unverified, unknown, or violates their verified
  identity is labelled in the voice view. Yapfast can't send them its audio key and
  refuses theirs, so the sidebar also shows a warning.
- A participant whose audio key hasn't arrived after ten seconds, or whose audio
  can't be decrypted, is labelled. Only a failure of Yapfast's own key mutes the
  microphone with an error.
- A participant who stops sending audio after having connected is shown as
  "Connection lost" until their membership ends.
- If Yapfast was killed during a call, the next start removes the sticky membership
  this device left, instead of others seeing it for up to five minutes.
- After a full LiveKit reconnect, Yapfast restores the microphone's key index, so
  others keep hearing it after a key rotation.
- Join failures name the cause users can act on: a homeserver without sticky
  events (MSC4354), a missing MatrixRTC transport, or a calling service that
  refused the connection.

`testing/test_voice_reliability.py` checks these with separate processes, the
local stack and private PulseAudio null devices; no physical microphone is
recorded. Bob runs in a pasta network namespace, so stopping pasta simulates a
network loss for Bob only. Run it from the development shell after
`scripts/test-stack up`:

```sh
python3 testing/test_voice_reliability.py --report .local/voice-reliability.json
```

Yapfast keeps the homeserver address the user entered, so a login response
can't move it to another address, such as an unchecked plain-HTTP one.
