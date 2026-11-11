# Go Live

Sharing a screen or window with its sound on Discord, and watching others'
streams. The rest of Discord is in [the Discord docs](README.md).

Go Live streams come through Discord's gateway: op 18 starts sharing, op 20
watches someone, op 22 pauses and op 19 ends either. Discord answers with
STREAM_CREATE, naming the RTC server, then STREAM_SERVER_UPDATE with an
endpoint and token. The stream gets a voice connection of its own to that
server, reusing the call's voice session id; a stream waits for that id too,
if this account's voice state arrives after both answers. `yapfast_discord::stream` names
streams by key (`guild:{server}:{channel}:{user}`) and
`Discord::{go_live, watch, end_stream, pause_stream}` run the signalling. Voice
states carry `self_stream`, which shows as a LIVE badge beside the person.
Sharing starts from the call's controls or from the voice panel's Share
button, which works wherever the window is; the panel says when the stream
is live and stops it. A click on someone's tile watches their stream, which
then shows large in it; someone who films as well has a second tile for the
stream. A stream being watched starts large, however watching began, and
stays watched in its tile after a click beside it. While the call is off
screen, the large tile's stream or camera, or else the stream being watched,
or this account's own, shows in a corner (`StreamPip`), as Discord's
picture in picture does. STREAM_CREATE and STREAM_UPDATE name a stream's
viewers in `viewer_ids`; Yapfast keeps them for the stream it shares and the one
it watches, and shows them over the stream (`ViewerStack`) and as a count in
the voice panel; a click on it returns to the call. This account's
own stream has a tile of its own, which opens large as others' do, without
watching it through Discord: it shows the worker's preview.

Capture runs in a worker process, `yapfast --stream-worker`, so its buffers take
memory only while sharing; the process exits when sharing stops. Playing a
video file runs there too (`Job::Play`, `video_player`), which also keeps
FFmpeg's reading of files people sent out of the window's process. Its
frames carry their time, which the seek bar shows.
`stream_worker` starts it with one job as a JSON line and reads its reports
back. On Linux it starts `/proc/self/exe`, the running Yapfast, since Yapfast's path
names a deleted file once a rebuild or update replaces it. Frames reach the UI through `frames`, a triple buffer in a memory-mapped
file in the runtime directory. The file is unlinked once the worker opens it.
Windows won't delete a mapped file, so there it's a temporary file that
Windows deletes once both processes close it. The file holds pixels, and a
slot's numbers; a picture the worker drew on the card is not in it, and the
two ways one crosses are [below](#encoding-on-the-gpu).
The worker's standard error goes to Yapfast's log file. If its output ends
without an `ended` report, as when it crashes, the popup says "Screen sharing
stopped unexpectedly." and Yapfast logs a warning, "the stream worker ended
without saying why", with how it exited (`exit="signal=11 (SIGSEGV)"` on Unix,
the exit code on Windows), the job (`share`, `watch`, `play` or `glance`), the
encoder's `engine` and `codec` from the last stats a share reported, and the
picture `size` (or "up to" the job's bounds before one was reported).
`a_worker_that_dies_says_how` runs a worker script that kills itself, through
`YAPFAST_STREAM_WORKER`, which names the program to start instead of Yapfast.
In a Wayland session, the desktop's screen sharing portal is what captures: the
desktop build turns on the `screencast` feature, PipeWire's headers are what
build it, and Yapfast then asks the portal itself and reads the DMA-BUFs the
compositor draws; a build without either leaves libwebrtc's capturer to ask the
portal instead, and the worker runs GLib's main context while it waits for the
dialog and for frames
([below](#capturing-on-the-gpu-on-linux) says which of the two runs, and why
the pointer is in the frames only in the first).
Yapfast tells Discord it's
live only once frames flow, so a cancelled dialog shares nothing. Windows has
no such dialog, and libwebrtc uses the portal only on Wayland, so elsewhere,
as on Windows and in an X11 session such as XFCE's, the share menu lists the
screens and windows libwebrtc's capturers find (`stream_worker::shareable`),
and the worker captures the one picked by its id; see the
[Windows guide](../windows-beta.md#go-live-on-windows). On X11 the capturers
read the X server, and Yapfast's own windows are listed too.
`stream_worker::portal_picks` makes the choice as libwebrtc does:
`XDG_SESSION_TYPE` starting with `wayland` and `WAYLAND_DISPLAY` set. The
ignored `glances_at_the_listed_screens` and `shares_a_listed_screen_and_window`
list, picture, share and switch between a screen and a window, under Xvfb on
Linux; the latter's comment says what the window there needs.
While live, the voice dock's share button switches what's shared without
ending the stream: the worker starts a second capturer (on Wayland, through
the dialog again) and keeps sending the first one's frames. When the second one
gives a frame, it replaces the first. If the dialog is cancelled, the first
one goes on. The encoder takes the new size on the same connection. A share
that is on the card switches differently, since nothing of it is captured in
memory: another screen is opened the way the share opened — on Windows the old
capture first, because DXGI gives an output to one duplicator at a time, then
the encoder session and the tile's own, which were both built on the device the
old capture made; on Linux the picture's descriptor is dropped with the capture
that made it — and the stream goes on. A camera, and on Windows a window, can't
be captured on the card, so the share carries on from the processor rather than
ending; on Linux a window is the card's as much as a screen is.
`YAPFAST_STREAM_TEST_PATTERN=1` shares moving colour bars instead of a screen.

A camera can be shared too, such as OBS's virtual camera (v4l2loopback on
Linux, its DirectShow filter on Windows). The share menu lists the cameras
[the call's camera](voice.md#camera-video) lists, under CAMERAS, after the
screens and windows; on Wayland, where the portal picks a screen or window, a
"Screen or window" choice comes first, and switching with no camera listed
goes straight to the dialog. The worker films the camera through the same
capture module (`camera::Camera::open_at`), asking for the stream's
resolution and frame rate rather than the call camera's 720p at 30, and
copies each picture into one buffer it keeps for the next before the encoder
takes it. Switching between a camera and a screen works as between two
screens. `YAPFAST_CAMERA_TEST_PATTERN=1` lists a test pattern camera to share.

Once Discord has started the stream, Yapfast unpauses it with op 22, as
Discord-video-stream does, and the worker gets its server and sends the video
there (`yapfast_discord::stream_connection`). The connection is a call's
(`voice_connection.rs`), on voice gateway version 8 as Discord's clients use:
heartbeats acknowledge the last sequence number, and binary DAVE messages start
with one. It identifies with the stream's RTC server and `video: true`,
discovers its address, then uses `aead_aes256_gcm_rtpsize` transport
encryption. Yapfast offers H264, and AV1 then H265 before it where the GPU
encodes them ([Encoding on the GPU](#encoding-on-the-gpu)), and announces its
size with op 12, again whenever someone connects. H264 frames come from x264 (`vendor/x264-src`,
built from source) through `native/h264.c`, with a keyframe at least every 3 s
and whenever RTCP asks for one; [Choosing the H264
encoder](#choosing-the-h264-encoder) says how it's set up and why. Yapfast also
has VP8 (`native/vp8.c`, still built and tested); every Go Live sender known to
work sends H264. Each encoded frame owns the buffer it was encoded into: the
encoder takes one of three and the frame gives it back when its packets are
planned, so its bytes reach DAVE without a copy. The encoder says how big the
unit it produced is before it is copied, and the buffer grows to hold it, so a
picture is never encoded a second time and its unit is copied once, straight out
of the encoder's own memory: x264's output, FFmpeg's packet, or NVENC's locked
bitstream. Only a frame that came out of FFmpeg as several packets is gathered
first, into a buffer one frame big, and libvpx's frame always is. The buffers start at 512 KiB, or at a
frame's own cap where that is smaller, and double as a frame needs, up to 2
bytes a pixel and 4 KiB more; three of those are 1.5 MiB at rest, and 12 MiB
at 1080p only if every buffer grew to the cap. A frame beyond the three
allocates its buffer rather than waiting for one. Packetizing a frame asks for
nothing either: its packets are planned in one pass, as where each one's bytes
come from rather than a copy of them, into a plan the next frame reuses, and
each is written into one datagram buffer that the whole stream sends from —
header, extensions and payload, with room after them for the encryption's tag
and counter — and sealed where it lies. A packet's bytes are copied once, where
a vector of its own used to be built around them. H264 goes as whole NAL units
or FU-A fragments, H265 as whole units or FU fragments (RFC 7798), and AV1 in
AOM's [RTP payload format](https://aomediacodec.github.io/av1-rtp-spec/v1.0.0.html),
as libwebrtc's packetizer sends it: OBU elements without their size fields,
temporal delimiters, tile lists and padding dropped, an element too large
for a packet carried on in the next, OBUs of different layers in packets of
their own, and a keyframe's first packet marked as starting a coded video
sequence (N). A frame with a malformed OBU isn't sent. DAVE leaves an AV1
frame's last OBU without its size, its trailer after it, and receivers keep
it so, since DAVE's decryptor reads the frame as it was sealed. Past a
frame's first 8 packets, 4 go out
each millisecond: a 1080p keyframe of 200 sent at once can overflow buffers on
the way, and it goes out over as many turns of the send loop as it has batches:
between batches the loop is back to poll the stream's sound, the gateway's
heartbeats and session updates, and viewers' feedback, so a keyframe that takes
tens of milliseconds holds none of them up. A frame partway out keeps its plan,
its encrypted bytes and the count written; the next frame waits until it is
whole. Yapfast keeps the last 2048 video packets, none older than two seconds,
and when a viewer reports
some lost with a generic NACK (RTCP 205), resends up to 128 of them as
Discord's client does: on the RTX SSRC with RTX's payload type and numbering,
the original sequence number before the payload (RFC 4588), and the repaired
stream id (extension 12) in place of the stream id. An RTCP sender report goes
out every second. A viewer that lost a packet asks for a whole frame with a
picture loss indication (RTCP 206/1) or a full intra request (206/4), and
Yapfast's streamer makes one at most every 500 ms, as a viewer may ask: every
viewer who lost the same packet asks at once, and a keyframe is many times a
frame, so a run of them at a rate that just dropped is lost in its turn. A
keyframe it needs itself, after a frame was dropped or for a codec Discord
switched to, is made at once, since nothing sent can be repaired without it.

Before sharing, the share button's menu picks a resolution and a frame rate, as
Discord's client asks for them: 720p, 1080p or the screen's own size (up to
4K), at 15, 30 or 60 fps. Its "Higher quality" checkbox, which Discord's client
doesn't have, has the processor's encoder work harder on each frame — x264's or
libvpx's slower preset for a sharper picture at the same bitrate. The graphics
card's encoders run the one tuning `native/hwenc.c` names, so with "Encode
streams with the GPU" on the row says that and can't be ticked: a slower preset
there would change nothing. Yapfast remembers the choice
beside the chosen audio devices (`devices.json`), and starts at 1080p30 without
it. Larger screens are scaled down to fit, and one of another shape keeps its
shape: the box a share fits in is named landscape, and it turns with the
picture, so a 1080x1920 display shared at 1080p goes at 1080x1920 rather than
the 606x1080 a box left standing would leave it. Nothing is padded with black —
the black around a share is the viewer's client's, drawn around a picture of the
wrong shape — and nothing is cropped. Yapfast declares what the picture is
worth: 0.07 bits per pixel per frame, floored at 3.5 Mbit/s and capped at 9
Mbit/s, announced as op 12's `max_bitrate` (bps) with the frame rate and size
as its `max_framerate` and `max_resolution`. A 1080p60 share declares 8.7
Mbit/s, under the cap; a 3440x1440@60 share asks for 20.8 and a 4K30 one for
17.4, and both are declared 9. The frame rate is the one the interface asked
for, whatever the picture's size: a screen above 1080 lines offered 60 sends 60,
and declares what that is worth up to the cap.

it. The menu lists every screen, window and camera, and is never taller than
the window: where they don't all fit, as with many windows open or the window
short or zoomed, the choices scroll as one above Go Live, which stays in view.
Larger screens, and those of another shape, are scaled down to fit. Yapfast
caps them at the bitrates Discord's client uses: at most 3.5 Mbit/s up to 720p
and 9 Mbit/s above, the cap op 12 announces, with the frame rate as its
`max_framerate`. Discord's client allows accounts without Nitro 720p30, but
trials 1080p30 at 3.5 or 5 Mbit/s for them. The worker converts captured
frames to I420 with libyuv, scales them with libyuv's filtering scaler, and
encodes those; the preview is the same picture scaled down to 720p.

9 Mbit/s is the safe cap because it is already well above what Discord's own
web client asks for: its screen-share ladder is `desktopBitrate {min: 500_000,
max: 3_500_000, target: 600_000}`, its 1080p30 overrides are 3.5 or 5 Mbit/s
and its tier-1 720p60 one is 9. Settings, Advanced's "Raise the stream's
bitrate cap (experimental)" lifts the cap to 35 Mbit/s, what 3840x2160 at 60 is
worth and far past anything Discord Web declares, for the next share started: a
stream already running declared its ceiling when it began, and keeps it.

Those numbers are measured here rather than copied: four-second clips at the p4
low-latency setting a stream opens with, VMAF against the source. Busy
3840x2160@60 scores 92.9 / 94.2 / 95.2 / 96.5 through AV1 at 15 / 25 / 35 / 50
Mbit/s, 91.8 / 93.8 / 95.0 / 96.5 through HEVC and 91.8 / 94.1 / 94.9 / 96.2
through H.264 at the same rates; busy 3440x1440@60 scores 93.7 / 95.3 / 96.9
through AV1 at 9 / 20 / 35. 0.07 is where busy content reaches VMAF 95 at every
size, so it is the rate a stream asks for and the safe cap is where most
pictures meet it: a 1080p60 share declares it whole, and a larger one is cut to
the cap unless the switch is on. A screen share is far cheaper: a 4K60 desktop
scored 98 on 2.4 Mbit/s and a 3440x1440@60 one 98 on 5.9, so a share of a screen
leaves most of its cap unspent.

`max_bitrate` is a declaration and the voice server sends no such field back:
Discord's reference documents no maximum, default or tier. A probe through a
real stream had 3840x2160 declared at 60, 120 and even 200 Mbit/s, and
7680x4320@60, and Discord refused none of it. So the declaration is a ceiling
and not the rate: what a stream sends at is decided by a congestion controller
of its own (`crates/yapfast-discord/src/congestion.rs`), the way Discord's
client runs libwebrtc's. It starts below the ceiling, at 2.5 Mbit/s, and raises
the target a little each report while the round trip stays near the lowest it
has measured; when the round trip climbs well over that, or packets are lost,
it cuts the target below what the server received — what the uplink carried
while the queue built — then holds. Discord's client reads this from
transport-wide feedback; Yapfast reads the receiver reports the stream server
sends about the video, which a queue at the sender's router lengthens too.

A round trip 60 ms over the lowest, on two reports in a row, or more than about
10% of the packets lost, cuts the target at once to 85% of what the server got,
and it doesn't rise again for three seconds; above 2% lost it doesn't rise
either. A round trip within 25 ms of the lowest raises it 8% a report, and near
the rate it last backed off from 1%, so the climb back is slow where the queue
built before. It never rises past half again as fast as the encoder is really
sending, so a calm picture can't take the target to a rate the uplink has never
carried. The server's bandwidth estimate (REMB), where it sends one, caps the
target as either declared ceiling does — a path that says it carries 4 Mbit/s
sends 4, whichever cap was declared — and a frame of another size declares a new
ceiling. The encoder follows the target once it has moved a tenth of its rate.
None of this raises the declared ceiling: 0.07 bits per pixel per frame is what
a picture is worth, the safe cap is where a larger one is declared instead, and
the controller decides where under either the stream really sends. The worker converts
captured frames to I420 with libyuv, scales them with libyuv's filtering
scaler, and encodes those. The preview — the same picture written for the
share's tile and the corner — is made only while the UI shows one: the window
tells the worker, and the first frame after it says so has one. It is written at
the size, in pixels, of the tile showing it, which the window says when that
changes, on the same terms as a watched stream's tile: a small tile costs a small
frame, a large one grows the frames file once, and a picture smaller than the
tile is never scaled up. Until a tile says anything the box is 1280x720, and the
frame Discord is stilled from is made in that box rather than the tile's, since a
tile smaller than the 512x288 still would cost it its detail. A grid
scrolled past the share's tile is not showing one, and neither is a call with no
share tile at all. Each
goes into a buffer the capture keeps frame after frame, made again only
when the screen or the stream changes size, so a captured frame allocates
nothing once it has settled, and the preview scales into one buffer of its own.
Conversion and scaling happen before the sink is locked, so the capture's
status is never held while a frame is made. On Windows, with GPU encoding on and a screen whose card opens NVENC on the
capture's own D3D11 device, no frame is converted at all: Windows Graphics
Capture fills a BGRA texture and NVENC reads it there, the driver's video
processor scaling it to the stream's size on the way. The preview's tile and
Discord's still come from that same frame. The tile is the stream's own picture
— scaled on the card into a 1920-long-edge box, which is the largest a 1080p
share sends — drawn into the frames file's own slots: one picture for each slot,
and a slot is only drawn into once the window showing it has finished with the
picture that was there — a picture taken is one on the screen until the next
replaces it and the window's own rendering queue has drawn a frame without it,
which the window says slot by slot and generation by generation, because only
it knows. So a picture a person is looking at is never written over, and neither
is one the renderer hasn't finished reading: a window that falls behind costs
preview frames, never a torn picture, and the
stream itself is untouched, since it encodes the capture's texture and not the
tile. A window that stops saying it has finished with a slot for a whole second
— a render queue that has stopped, rather than one that is behind — costs the
share the card's picture path: the frames after that are written as pixels,
which still reach the window, and the share stays on them for the rest of the
stream. That is the way out of a ring that has stopped turning; it is one way on
purpose, since the thing that would have to come back is the window's answer,
which is what stopped. A picture crosses to the window as a shared texture it opens once, shown
where the card drew it every frame, with a fence saying when the drawing
finished. A shared texture's handle is a number that names the same object in
both processes, so it goes in the frames file's slot; on Linux a picture is a
DMA-BUF instead, and a file descriptor names a different object in every
process, so it can't: the window makes a `SOCK_SEQPACKET` socket pair before it
starts the worker (`frames::Frames::open_pictures`), keeps one end, writes the
descriptor of the other in the frames header, and starts the worker in
`dmabuf::inheriting`, which keeps that descriptor open across the spawn so the
worker inherits it at the number the header names. The worker adopts it on the
other side of the `exec` (`dmabuf::Sender::adopt`, checked against the socket's
type and the window's process id) and, from then on, sends one message for each
picture it publishes: its slot and generation, the fourcc, the DRM modifier and
the offset and stride of each plane, with one descriptor for each plane
attached as `SCM_RIGHTS`, which is the kernel taking a reference to the memory
for the window. One `sendmsg` a picture, one drain a take, and no allocation in
either.
The window takes a slot for a picture only when the picture's message is
waiting for that generation (`dmabuf::Reader::take`), and the descriptors it
took are closed when it lets go of the picture, which is as it retires the slot:
the memory a picture is in belongs to the worker until the window has finished
sampling it, and no descriptor outlives that. The import itself duplicates each
descriptor it is given, because Vulkan takes the copy and keeps it until the
imported memory is freed: a picture's own descriptors stay the channel's and are
closed with it. A window with no importer — one drawing without Vulkan, or a
build without Skia's Vulkan — refuses a stream's pictures once, lets go of the
picture and its slot at once, and the stream writes pixels from its next frame.
A message that can't be read —
another version, or one whose planes and descriptors disagree — is closed as it
is read, and one it displaces is closed then too, so a window that stopped
taking pictures holds one per slot and no more. The device a picture has to be
opened on is the window's own: Skia on Vulkan through a wgpu 30 device Yapfast
makes where the machine's driver opens one — a build carrying Skia's Vulkan does,
which the release is, see [building.md](../building.md#skia-with-vulkan), and
Windows draws with Direct3D 12 — and Skia on OpenGL otherwise, which the same
archive carries (`src/ui/vulkan.rs`). Where the two adapters can't share a
texture, or the window draws in software, the tile is read back into one
staging texture at 1280×720 at most instead, ten times a second while the UI
shows a preview. Everything else — a window or a camera, the processor setting,
or a card that won't open on the capture's device — takes the libyuv path above,
and one log line says which ran and why. Where Windows Graphics Capture is missing or won't open, DXGI's Desktop
Duplication fills the same kind of texture — upright, in BGRA, with the pointer
drawn in and no border — and everything above it, the scale to the stream's size
and the preview's readback, is identical. Which of them runs first depends on what the platform can do: Desktop
Duplication on Windows 10, where Graphics Capture cannot shed its outline, and
Graphics Capture from Windows 11 22H2 on, where it can. A capture that opens with
its border still on is dropped for Duplication when Duplication can open, since
the outline is something the person being shared with can see. Settings can name
either rung, for a machine where another application already holds an output's
duplication and the choice falls to the other. Discord's still is made from the
same scaled picture, so it still refreshes while the local preview is hidden.

### Capturing on the GPU on Linux

Linux captures on the GPU the same way, through a different door: instead of a
texture, the pictures are the DMA-BUFs the desktop's compositor drew, and the
window imports the descriptor and draws it where it lies. Two rungs capture them,
and `capture::linux_order` chooses the order from what the session answers rather
than from a setting — nothing here draws a border, so there is nothing to prefer
between them but what each one sees:

* the desktop's screen sharing portal (`capture::portal`, `capture::pipewire`),
  which is asked for the kind of thing the person picked — windows for a window,
  one monitor for a display — and hands back the compositor's own buffers on a
  PipeWire node. It is asked whether it answers at all first, with one call on
  the session bus, so a desktop without one is not left waiting for a timeout.
  PipeWire's client functions are bound from `libpipewire-0.3` when the capture
  opens, rather than by the link: LiveKit's libwebrtc carries a weak stub of
  each of them for builds without PipeWire, and a link that takes those calls
  them instead, which ends in a jump to 0. A machine with no PipeWire library
  says so in the capture's own words instead. The format it asks for carries a
  linear modifier, because an importer here maps rows: a compositor on a card
  with a tiling of its own — KWin on an NVIDIA screen hands over
  `0x0300000000e08014`, block-linear — copies into a linear buffer instead of
  handing over a picture nothing can read;
* the X server's Composite redirection exported through DRI3
  (`capture::x11`), which redirects the window or the screen to a pixmap and
  exports that as one DMA-BUF. It captures a window as that window and never the
  desktop behind it, and a monitor as the screen cut to the monitor's rectangle.

A Wayland session has no X11 rung at all: what an X11 capture sees under
Wayland is Xwayland's windows and not the desktop they sit on, and the screen it
would name is Xwayland's overlay window, whose storage is empty — a share of
nothing, and a refusal in the log that reads as if it were about the desktop. An
X session tries the portal and then the X server, and each rung that refuses is
named with what it said. A session with neither, a build without the `screencast`
feature or PipeWire's headers, a window whose importer is missing, or a machine
with the processor's encoder chosen all keep the share on the libyuv path, and
the log says which: `sharing from the processor`, with the reason. The Stream tab
says the same thing twice over, since a picture looks alike whichever way it
came: `StreamStats.capture` names the rung and the size ("X11's DRI3 capture ·
1920×1080"), and `fallback` says why a share left the card, in the words of the
refusal that sent it there.

The pointer is drawn by whoever draws the frames. The portal's rung asks for it
in them — `cursor_mode` 2, embedded, `Cursor::Embedded` in `capture::portal` — so
the compositor paints it into the buffers and a viewer sees it. The X server's
DRI3 export can never have it: the X server draws the pointer beside the
picture, not into the window's redirected storage. The processor's path asks
libwebrtc's capturer for the pointer (`set_include_cursor(true)`), and on Wayland
that capturer asks the portal for it as metadata (`cursor_mode` 4), which nothing
in it paints into a frame — so a Wayland share that fell to the processor has no
pointer, and the same share on Yapfast's own portal rung has one. That is a
reason of its own to build the `screencast` feature, which the desktop app does.

Each pass of the loop asks the capture for a picture and gives it the wait until
the stream's next slot, so a desktop that isn't changing sleeps in the capture's
own timeout instead of spinning beside it; an arrival of `Still` is a pass that
sends the picture the capture last handed over, and only while the stream wants
one — a viewer joining, a decoder that lost its place, a codec Discord chose. A
picture that has just arrived is published for the window when a preview is
wanted, into a slot `frames::Writer::claim` answers, with
`frames::Writer::publish_dmabuf`: the descriptor and its plane go over the
stream's socket, and the window imports them. `frames::Writer::dmabuf_showable`
is asked before any capture opens, and again before a publish that leaves a slot
free — but it decides the *preview*, not the stream. A window drawing on the
processor can't sample a descriptor, and before this it took the whole share down
to the processor with it; now the capture stays on the card, the encoder goes on
reading the descriptor (VA-API's import, or the CUDA rung NVIDIA's cards need) and
the tile's preview is the picture read back — the same `mmap` the still is made
of — said once in the log. So a software-rendered window costs one copy of the
screen per preview frame it shows, and nothing of the stream. A session whose
build has neither an importer nor an encoder that reads descriptors keeps the
processor's capturer, which is where its frames have always been pixels. A size
change (`Resized`, or a descriptor of another size) is reported to the UI at the
size the next picture is sent at, and the stream's encoder is opened at it on the
same connection.

What the encoder is given is the descriptor itself, through
`stream_worker::Encoding::encode_dmabuf`: a VA-API session imports it and no pixel
of the picture comes back to the processor ([Encoding on the
GPU](#encoding-on-the-gpu) says what a descriptor can hold there). A screen's
DMA-BUF is BGRA, which is not the NV12 a VA-API surface is made of, and it is
the screen's size where the stream sends a smaller one. Where this build has
FFmpeg's `scale_vaapi` (`cfg(vaapivpp)`) and the driver's video processor
answers, the card makes the surface the encoder reads: the picture is mapped as
a surface of the format it arrived in, scaled to the stream's size and converted
to NV12 on the card, and the encoder reads that — said once in the log and on the
Stream tab's encoder line ("…, scaling and converting DMA-BUFs on the card").
Without that path — an FFmpeg built without those filters, a driver with no video
processor, a picture whose format it doesn't take — a screen still costs one
copy: the picture is mapped read-only with `mmap` (`capture::linear`), converted
to I420 in a buffer kept for it, and encoded from there — said once in the log
and in the share's report, while the window goes on drawing the descriptor
itself. A picture a driver tiled can't be read back that way and isn't read
wrongly: it is the video processor's, which takes the modifier, and where there
is no processor the share goes on from the processor's capturer instead, which
is the one thing that always has pixels. Discord's still is made from the same
map, is the one picture this path copies, and is made only when it is due.

A share that stops lets go of its capture, which closes the portal's session and
undoes the X server's redirection; a capture that ends — the person stopped
sharing, the window was closed — ends the share with the capture's own sentence,
as the capture of a screen does on Windows.

A desktop that isn't changing is still a stream that can be asked for a picture.
The capture keeps the last picture it made in its own texture, and a viewer
joining, a decoder that lost its place and asked for a whole frame, a preview
coming back or Discord's still being due are all served from it at the next
pass of the loop — throttled by the same keyframe and still intervals, and by
the send queue's own backpressure, so a request is only answered by a picture
that can be queued. Nothing is captured or encoded again for them, and a
desktop that neither moves nor is wanted encodes nothing at all; a picture the
capture no longer holds, after a resize or a device the card took away, waits
for a real frame rather than refreshing the source before it.

Windows Graphics Capture takes a window as readily as a display: `create for
window` rather than `create for monitor`, with the window validated as a
top-level one first. A window that closes or excludes itself from capture ends
that share rather than refreshing protected content, and a minimised one has no
picture to give. Where the window can't be captured on the card, the processor's
capturer shares the same window — never the desktop it happens to be on — and the
stream goes on from there with a whole frame, since the pictures have changed
kind.

Discord shows a still of each stream, in the call and to people outside it.
Like Discord's client, Yapfast makes one once the stream connects and the
picture isn't black any more (a screen is black until what's shared draws),
then every 5 minutes: the picture scaled down to fit 512 by 288, a JPEG at
quality 92. The worker reports it as a data URI, and Yapfast uploads it with
`POST /streams/{stream key}/preview`. A failed upload is tried again a minute
later, up to three times; a newer still replaces an upload not done yet, and
stopping the share cancels it.

A shared screen carries the sound every other application plays, as Discord's
client and Vesktop share it; Yapfast's own sound stays out, so viewers never hear
the call echoed back (`stream_audio`, with `audio_io::others` hearing the
applications). On Windows that means Yapfast's own process tree: another
Discord client's call is shared, since only its own processes can be left out
there. On Linux, PulseAudio or PipeWire's
PulseAudio server monitors each application's playback stream on its own
(`pa_stream_set_monitor_stream`, as pavucontrol's level meters do), and
streams from Yapfast's processes are skipped by process id. So are other Discord
clients' (Discord, Vesktop, Equibop, WebCord, Legcord, ArmCord, by application
name or program): with one of them in the call too, its sound echoed the call
back to the people in it. The worker mixes them every 20 ms, waiting until an
application has 40 ms queued and keeping at most 200 ms of each, and encodes
Opus at 64 kbit/s, what Discord's client gives a stream's sound. Frames go out
as plain Opus RTP on the stream's audio SSRC, DAVE-encrypted, with sender
reports, and op 5 marks them as shared sound (speaking flag 2). Nothing is
sent while no application plays. On Windows, WASAPI's process loopback hears
everything but Yapfast's own process tree as one stream, from Windows 10's 2004
update on; it can't leave other Discord clients out. Other systems share
video only for now, and so does the test pattern. Two ignored tests check the capture against the
machine's sound server with a quiet tone: another process's is heard, and
Yapfast's own is not.

Each video packet carries the one-byte extensions a Discord client sends: a
transport-wide sequence number (id 5), playout delay (6), screen content
type (7) and the stream id `100` (11). The stream id is what ties a packet to
the stream op 12 declared. These ids aren't the ones in Discord's WebRTC
SDP, where 5 is playout delay: with only playout delay as id 5, Discord's
server drops every packet, and viewers time out with error 2012.

DAVE follows songbird's handshake (`dave.rs`) and encrypts each frame once the
group is ready. A stream's MLS group isn't the voice channel's: its id is the
RTC server id minus one, which Discord-video-stream found too. If the server's
proposals or commits name another group, Yapfast moves to it. The vendored
davey's `set_channel_id` keeps the key package already sent, so the streamer's
welcome still opens. Yapfast accepts the people op 11 names into the group, and
also those named in video attributes (op 12) or in the stream key.

With `YAPFAST_LOG=debug`, Yapfast logs every 5 s while sharing what it sent and the
RTCP that came back (`stream sent`): receiver reports with the highest
sequence number Discord's server got and its echo of our sender reports
(`lsr`), its bitrate estimate (REMB), and keyframe requests, and each change
of bitrate. The voice connection logs the voice server's RTCP too, to compare
with a sender Discord accepts. Messages the stream server sends are logged
too, except those carrying keys.

Watching works the same way in reverse. Clicking someone's LIVE badge, or their
tile in the call, sends op 20 and starts a worker with a watch job. The worker
connects as a viewer, offers AV1, H265, H264 and VP8 for decoding only (H265
only where it's built, on Linux and Windows), and asks for video with op 15. It
learns the streamer's SSRCs and codec from op 12 and op 14, then puts RTP
packets back in order (`rtp::Reorder`) and together into frames
(`rtp::Assembler`: VP8 descriptors, H264 single units, STAP-A and FU-A, and
H265 single units, aggregation and fragmentation units, as Annex B; AV1's
OBU elements as OBUs, the last without its size, as DAVE's decryptor needs
it and dav1d takes it). As
Discord's client does, it asks for a missing packet again at once with a
generic NACK, again every 80 ms up to three times, and takes the resend from
the streamer's RTX SSRC (RFC 4588); padding there, which probes the path, is
dropped. Packets after a gap wait for it at most 300 ms. A frame still missing
a packet is dropped, and so is everything after it until the next keyframe. So
are frames while none has come yet, and after one the decoder was too far
behind to take. The connection asks for a keyframe with a picture loss
indication, at most every 500 ms. It sends a receiver report every second
(packets lost, the highest received, and the streamer's last sender report
echoed). With `YAPFAST_LOG=debug` it logs what it received every 5 s (`stream
received`): packets out of order, resent, recovered, given up on, and keyframes
asked for. The worker decodes with libvpx or with the FFmpeg H264 decoder in
libwebrtc's archive (`native/decode.c`), AV1 with the dav1d in the same
archive (`native/av1.c`), a picture out for each frame in, or H265 with the
HEVC decoder of `vendor/ffmpeg-src`'s FFmpeg (`native/hevc.c`), which
libwebrtc's lacks, and writes RGBA at up to 1080p into the shared frames. A
frame the decoder can't use leaves the worker asking for a keyframe, at most
every 500 ms as above, and decoding nothing until one comes, the deltas after it
being built on a picture the decoder never had. A decoder replaced because the
stream changed codec starts from a keyframe too. A stream in a codec this build
has no decoder for keeps its sound playing, and gets pictures again if the
streamer turns to one this build has. A decoder asks for up to four threads
from a budget it gives back when it goes, and a picture a newer one replaces
before the screen draws it is decoded but not converted or shown. A window that is hidden or minimised stops there too — the pictures
stop being converted and written while the stream keeps decoding, because the
next frames are built on them, and its sound keeps playing; showing it again
presents the next complete picture rather than a stale one. A stream is written at the size its tile is shown at, scaled with the
same filter the share's preview uses and never upscaled, so a 480×270 tile
costs 0.5 MB a frame rather than the 8.3 MB a 1080p picture takes, and a
window grown to full screen grows the file once.
dav1d takes 0.07 of a core for 1080p30 AV1 at 3.6 Mbit/s, against 0.1 for
H265 at the same bitrate. That FFmpeg is built with
its x86-64 assembly, without which H265 took over three times the processor
time. Watching is possible only in the call you're in, so clicking someone
sharing in another channel joins it first, once asked, and watches once the
call is connected; Discord drops a watch sent before. The stream's sound, Opus
on the streamer's audio SSRC, is DAVE-decrypted as audio and handed to the
player with each frame's RTP sequence number, timestamp, SSRC and arrival time
(`stream_connection::ReceivedSound`), so it can put the sound back in order and
give up on what no longer matters. The streamer's sender reports are kept beside it
(`stream_connection::StreamClock`): each SSRC's RTP timestamp against the middle
32 bits of its NTP time, plus the newest picture timestamp that arrived. The
worker reports two skews from them (`av_timing::Timeline`) in its debug log and
in the call's Stream tab, both unknown while a mapping is missing or older than
five seconds: the *arrival* skew carries each
clock to now from its own report and so compares what arrived of each stream,
before any device buffer, and the *presentation* skew compares the sound the
device says it has played against the picture last handed to the window. A
sender report names no SSRC, so a stream that changed SSRC or restarted is
caught by its timestamps no longer carrying the held clock on, which drops both
mappings rather than measuring across the break. The decode loop is bounded by
the sound (`av_timing::decide`): a picture more than 80 ms behind the audible
sound is dropped rather than shown late, and one the sound hasn't reached is
shown when it arrives. Holding an early picture back would mean keeping it, and
a decoded picture is the decoder's own only until its next frame, so a deferral
would cost a copy of the picture and the memory for it to save a wait of at most
80 ms — the drop is the bound, and it neither blocks nor sleeps. The picture's
time is its submission to the frames file;
the renderer's own fence is the window's to observe, so a record carries
`estimated` rather than a worker claiming a presentation it did not see. The
player's five-second line still reads `skew_ms` and `map_age_ms` — both `None`,
said once, when a report never came — and neither the device
buffer nor the decoder's delay is in them, so they compare the sound the player
holds against the newest picture received. The player plays 20 ms a turn: frames wait
in sequence order, and the sound they hold is capped at 200 ms by their
timestamps as well as ten frames by the room they take, so a music bot's 60 ms
packets don't become 1.2 s of delay, and it starts once three wait
(60 ms), or after 100 ms with a single one. A packet missing for 60 ms gets its
slot made up by the next packet's forward error correction, or by Opus's
concealment, and the packet that carried it still plays on its own turn; five
packets missing in a row (100 ms) mean the sender has stopped, so silence plays
and the playout starts again when sound comes. Duplicates, packets whose slot
played, frames that waited over 300 ms in the queue, and anything past the
queue's bound are dropped — the sound that came too early goes, never the packet
about to play — so a sender running fast has a frame a second dropped unheard
while the queue is full, and another SSRC or a timestamp a second away starts
it again (`stream_audio::Playout`). It keeps about 100 ms queued on the output
device (`audio_io.rs`), which is a device buffer apart from that network one: a
device that disappears keeps the player ticking on silence, with a fresh buffer
when it returns. Being deafened drops what waited, so nothing old plays on
unmute, which Yapfast tells the worker with a `hear` control. Yapfast's workers
play as "Yapfast", which sharing
leaves out, so a stream you watch never reaches one you share. Watching a
stream from Discord's own client works: H264 with DAVE. Discord's client shows
Yapfast's stream at 1080p with its sound, without the call's.

The fake server (`testing/fake_discord.ts`) checks what a stream sends, without
DAVE, and streams its recordings to viewers; its header lists the controls
that add a viewer or change a stream's codec.

## Choosing the H264 encoder

OpenH264, in libwebrtc's archive, doesn't suit Go Live: on anything that moves
it drops frames rather than lower their quality, and with frame skipping off
its rate control runs far past the target.

x264 encodes every frame within the bitrate. `native/h264.c` uses its superfast
preset, or faster with "Higher quality", tuned for zero latency (a frame out
for each frame in, no B-frames or lookahead, a slice for each of up to four
threads), in Main profile, which Discord's client sends (profile-level-id
`4d0033`). Superfast is the default because it costs little processor time;
faster takes about twice as much for about NVENC's quality. Adaptive
quantization is off: it was the one setting that helped both screens and games
against x264's defaults. The bitrate peaks at the target over half a second's
buffer; a second's buffer let a second reach twice the cap, and a two-frame
buffer starved keyframes. Capped CRF ran past Discord's cap.

## Encoding on the GPU

With "Encode streams with the GPU" in Settings, Advanced, on unless turned off,
a stream's H264 goes to the graphics card's encoder where it has one:
`native/hwenc.c` opens FFmpeg's NVENC encoder, then on Windows its AMF one
(AMD) and on Linux its VA-API one (AMD and Intel), and x264 when none opens.
It's on by default because it takes almost no processor time and looks better
than x264 at low bitrates. Intel graphics encode through Quick Sync — FFmpeg's
`h264_qsv`, `hevc_qsv` and `av1_qsv`, through oneVPL — wherever one of those
opens, and with x264 where none does: `vendor/ffmpeg-src` is configured with
them on both targets, and a build without oneVPL never tries them, saying "not
in this FFmpeg" for each. Quick Sync and AMF read a capture's own texture where
their driver takes one: the session is opened on the capture's D3D11 device
(`encode::Encoder::on_device`) and given a frames context of `AV_PIX_FMT_QSV` or
`AV_PIX_FMT_D3D11` with no pool — `native/hwenc.c`'s `open_qsv_shared`, and the
same shape for AMF, whose FFmpeg encoder takes a `ID3D11Texture2D` out of
`frame->data[0]` — so the picture is handed over as the texture it already is
(`yapfast_hwenc_encode_texture`, and `Encoding::QsvTextures` in the worker), with
nothing converted or uploaded. A driver, a device or a size that refuses it says
so once and the session opened from memory takes the stream as it always did. It
has not been measured on Intel or AMD hardware here, so what is known of it is
that it compiles and refuses cleanly, not that its picture beats x264's.

On Linux, a picture that is already on the card goes to a VA-API encoder where
it lies, with no copy at all: `encode::Encoder::encode_dmabuf` hands a frames
channel's DMA-BUF descriptor to `native/hwenc_dmabuf.c`, which imports the
descriptor's buffer as a VA-API surface — `av_hwframe_map` on a DRM PRIME frame,
which wraps the buffer instead of copying it — and gives that surface to
FFmpeg's VA-API encoder. No pixel of such a picture comes back to the processor,
and the Stream tab's line says so once one has been encoded that way ("…,
encoding DMA-BUFs where they lie" — or "…, scaling and converting DMA-BUFs on
the card" for a picture the video processor had to make a surface of first), as
does the log at the open. The encoder is
opened on the GPU the stream's encoder opened on, whose render node and libva
driver are read back out of its report, and with the same settings as the
session fed from memory: `native/hwenc_dmabuf.c` keeps them in one place and a
test holds them to what the stream was measured with, so a picture can't depend
on which way its frame reached the encoder. A stream whose pictures all arrive
that way runs one session on the card, not two: the encoder opened for pictures
in memory is given up when it has never encoded one, and opened again for the
first picture that has to come through memory.

NVIDIA's cards have no VA-API to reach, so the same picture takes a different
door: `native/cudaconvert.c` imports the descriptor the compositor drew as an
EGLImage (with its modifier), registers it with CUDA, and runs two kernels —
`native/cuda_convert.cu`, the BGRA to NV12 scale and convert a video processor
would do, compiled to PTX by `build.rs` with clang alone — into an NV12 buffer
in the card's memory, which `native/cudaenc.c` encodes with NVENC opened on the
CUDA device (`NV_ENC_DEVICE_TYPE_CUDA`) and reading that buffer where it lies
(`NV_ENC_INPUT_RESOURCE_TYPE_CUDADEVICEPTR`). Nothing crosses to the processor
and nothing is uploaded, which `yapfast_cudaconvert_copied` answers. It needs
NVIDIA's driver and nothing else: `libcuda` and `libEGL` are dlopened, the PTX
is the driver's to run, and the CUDA context the picture lives in is entered per
call because the encoder runs on another thread. The PTX is built for `sm_52` —
Maxwell — since PTX runs on the arch it names and every newer one, so a single
blob covers every card whose driver still encodes; it is handed over as a C
string, which is what the driver reads it as. The EGL device is the one the
descriptor's render node names (`EGL_EXT_device_drm_render_node`, or the same
card by PCI address), and the CUDA device is found by that PCI address when the
loader in front of the driver — glvnd, which distributions link — answers nothing
for `EGL_CUDA_DEVICE_NV`. A driver that answers an error from
`cuGraphicsMapResources` and hands the frame over anyway is taken at its frame,
since a mapping that wasn't made is nothing to unmake. A tiled picture — which a
compositor on NVIDIA's driver shares, KWin handing over
`0x0300000000e08014` — is one the driver holds as a CUDA array, and it takes a
second door: the driver answers an error from the mapping for every resource an
EGLImage registers and then hands back an array nothing can read, but a GL
texture over the same picture maps, so the picture is wrapped as a texture
(`glEGLImageTargetTexture2DOES`, on a display made from a gbm device, which is
the one whose GL CUDA binds to), and the kernels read the array behind it where
it lies, through a texture object, while it is mapped: an integer read at whole
texel coordinates, each channel the byte it is, filtered by the kernels as rows
are. A driver without texture objects copies the array on the card into linear
rows first, with `cuMemcpy3D` — the copy a block-linear array takes, where
`cuMemcpy2D` answers `CUDA_ERROR_INVALID_VALUE`. The texture is made as ARGB8888
whatever the descriptor says, since a screencast's XRGB8888 becomes a
three-channel texture CUDA refuses to register and the bytes are the same either
way, and the array holds the picture red first, as GL does, which the kernels
are told. A layout the first door refused once goes to
GL's from the next picture on, rather than being refused again every frame. Both
doors leave every pixel on the card, and GL is opened only when a picture takes
it. The session hands its access unit back in two steps: the encode leaves it
locked in the driver's bitstream buffer and says its size, and the drain copies
it into a buffer from the stream's pool, which is what is sent. EGL gives every converter on one device the same display, and a share
opens one for each codec it could send before it keeps one, so the displays are
counted across the process: the first converter to take one initialises it and
the last to let go terminates it, and a converter dropped never ends the display
the kept one imports on. A format that isn't 32-bit BGRA, a picture the driver
holds in neither rows nor an array, and a device that isn't the picture's are
one sentence and the
processor's copy, which is the ladder every encoder here walks. The session's
settings are `native/hwenc.c`'s NVENC row, and a test holds the two tables to one
answer.

What a descriptor can hold here is an NV12 picture in one buffer: that is what
a picture the card decoded is, and what a VA-API surface is made of without
converting it. A screen capture's DMA-BUF is BGRA — PipeWire's frames the
compositor drew — of the screen's size where the stream sends a smaller one, and
the same descriptor is given to the card's video processor rather than read back:
`native/hwenc_dmabuf.c` carries an `AVFilterGraph` of a VA-API buffer source —
the picture mapped as a surface of the format it arrived in — `scale_vaapi` set
to the stream's width, height and `nv12`, and a buffer sink, and the encoder is
handed the surface that comes out. The graph, its source's frames and its
surface all live on the one VA-API display the encoder opened on, so the picture
never reaches system memory and the whole conversion is the driver's video
processor. The arguments the processor is given are `encode::vpp_filter`'s, made
once per stream rather than per picture; a 32-bit picture is converted with
BT.601's limited range, which is what libyuv made of the same picture and what
the stream declares, and an NV12 picture of another size is only scaled. Which
build has this is a `cfg`: `vaapivpp`, set by `build.rs` where the vendored
FFmpeg was configured with `CONFIG_AVFILTER` and `CONFIG_SCALE_VAAPI_FILTER`, so
a build without them compiles the file without the graph and answers every
picture as it did before. A driver whose video processor is missing or refuses a
picture says so once through the shim's error, and that stream's pictures go the
processor's way from then on.

Every encoder here that reads its frames from memory — NVENC, Quick Sync, AMF
and x264 — answers a DMA-BUF with a typed refusal (`encode::Refused`) rather than
taking it and uploading it, and so does a VA-API encoder given a picture that
neither the import nor the video processor takes: the caller logs one line and
hands the same picture to the path fed from the processor, and nothing is copied
on the way to that answer. The order is the import first, then the video
processor, then the processor's own copy: `Encoder::copied` counts the pictures
that last one moved, which is every picture of that path and none of the card's
own, and `Encoder::encoded_in_place` counts the ones the card made into the
encoder's frames, imported where they lay or converted by its video processor
first.
The setting lives in `layout.json` and reaches the worker with each
`Job::Share`; the camera stays on x264. When the GPU's H264 encoder fails
during a stream, x264 takes over from that frame with a keyframe, and the log
says which encoder runs now and what the GPU's refusal was. The worker
logs which encoder each stream got ("encoding the stream", `engine` and
`codec`), and for each codec none opened for, why each GPU encoder failed
("no GPU encoder opened"). The call's Stream tab shows the encoder and the
codec Discord chose ([Voice](voice.md)).

FFmpeg is built from `vendor/ffmpeg-src` into the one binary, with only its
hardware encoders and every symbol prefixed so it can't meet the FFmpeg
inside libwebrtc. NVIDIA's driver, AMD's AMF runtime and libva load only
when an encoder opens: the binary starts without any of them, and needs
NVIDIA's driver 520 or newer for NVENC, and for AMF an AMD driver with AMF
1.5.2 or newer, from about May 2026 on; an older one fails to open, and x264
encodes.

VA-API tries each render node (`/dev/dri/renderD128` on) with the libva
driver its kernel driver calls for: radeonsi for amdgpu, iHD then i965 for
Intel's. Naming it overrides `LIBVA_DRIVER_NAME`, which NixOS and others set
to `nvidia` for browsers, and which would otherwise load NVIDIA's decoding-only
driver for every GPU, AMD's and Intel's too. NVIDIA's nodes are skipped:
NVENC encodes there. NixOS keeps libva itself off the library path, so the
Nix package's wrapper and the dev shell put it there (`runtimeLibraries` in
`nix/native.nix`); it's no build input, or webrtc-sys would build LiveKit's
own VA-API encoder into libwebrtc. The AppImage takes the system's libva,
and the Arch package lists it as optional.

The settings match x264's where they can: low-latency tuning (NVENC's p4
preset, `ll`; AMF's `lowlatency` usage and balanced preset; Quick Sync's medium
preset, one frame in flight and no look-ahead), no B-frames, a frame out for
each frame in, Main profile, keyframes every 3 seconds and when asked, and
variable bitrate capped at the target over half a second, which
leaves a calm screen below the cap rather than padding it. Quick Sync's rate is
MFX's VBR with the cap at the target and the average a kbit/s under it, because
FFmpeg's Quick Sync reads a target level with the cap as its constant bitrate,
which fills a calm screen out to the cap. `native/hwenc.c` sorts them into what
the stream needs and what only improves it: a needed
setting this FFmpeg doesn't have, or the driver refuses, fails that encoder's
open, and the worker logs it by name ("no GPU encoder opened"); an optional
one the driver drops is logged too ("the GPU's encoder dropped optional
settings"), and the stream goes on without it, so it never claims a setting it
hasn't got. A backend that was passed over says so too, and why — a codec this
FFmpeg wasn't built with, a driver that wouldn't open, an interface the platform
doesn't have — so a card whose encoder wasn't used never looks like nothing at
all. NVENC is the one backend checked before it is opened: it has no device of
its own to ask, so a machine with no NVIDIA card is told that rather than left
with FFmpeg's driver error, while AMF and VA-API reach their own runtime and
render node and say what those answered. The colour the stream declares is the
colour its frames are: capture
converts to limited-range I420, and the declaration says that. A screen showing HDR hands its frames over as codes of its own scale,
so capture tonemaps them to SDR's brightness on the way — one pass, in the same
buffer — while a screen showing SDR is converted exactly as it was. The format it writes is the one the encoder reports it took, NV12
or planar I420, and an open that gets neither fails by name rather than
scrambling the picture. Its frames carry the caller's 90 kHz time rescaled into
the encoder's own time base, so every engine, and a fallback between them, runs
on one clock; a packet the encoder returns for a frame it has already finished
is dropped and counted rather than sent under a later frame's timestamp, and
the frame after it is a keyframe, since the viewer never got the picture that
frame builds on and would otherwise decode garbage until the interval's IDR.
The frame carries that timestamp on to the packetizer, and one function derives
it, so a frame can't go out under another's time. The format, colour claim,
time base and rate it settled on are reported at open.
NVENC's tuning lives in one table in `native/hwenc.c` — preset, tune, rate
control, zerolatency, frame delay and forced-IDR in a row — so choosing a
different one is adding a row rather than editing the open path, and what a
stream actually got is logged at open. The default is unchanged: a preset has to
earn its place with a measured comparison, not a guess.
NVENC, AMF and VA-API change their bitrate as they run, without the IDR frame
FFmpeg would insert, with a patch to FFmpeg for each (AMF's sets the target,
peak and buffer on the running encoder). Quick Sync needs no patch: FFmpeg
applies a new target, cap and buffer to its running session itself, and
`native/hwenc.c` hands it the same cap-and-target pair it opened with. "Higher
quality" changes nothing on
the GPU: NVENC's slowest preset looked no better than p4.

Where the GPU's encoder encodes AV1 or H265, the stream offers them too,
first, AV1 before H265, as Discord's client orders them: each looks better
than the one after it at the same bitrate. When the stream starts, the
worker opens each to see that it opens, and keeps the first: an encoder
FFmpeg has isn't enough, the GPU and its driver must encode the codec. There
is no AV1 encoder on the processor, so with "Encode streams with the GPU"
off, the stream offers H264 alone. Discord picks the codec Yapfast offers
first that every viewer's client decodes (op 4's `video_codec`), and when
someone who can't decode it starts watching, switches the stream with op
14; Yapfast then opens the other encoder and starts it with a keyframe,
dropping frames still in the old codec. When AV1 or H265 fails mid-stream,
or doesn't open again for a new size or after a switch, Yapfast withdraws
it with its own op 14 `codecs`, marking it not to encode, and goes on in the
next codec offered, H264 last. Another Yapfast watching keeps the stream in
AV1 or H265. Which of Discord's own clients decode H265 and AV1 isn't
checked yet.

AV1 encodes on NVIDIA's RTX 40 series and newer (`av1_nvenc`), on AMD's
RX 7000 series and newer and the Radeon 700M graphics, through AMF on
Windows (`av1_amf`) and through Mesa's radeonsi on Linux (`av1_vaapi`), and
on Intel's Arc and Core Ultra graphics through VA-API (iHD) on Linux, and
through Quick Sync (`av1_qsv`) on either target where this build carries it,
which no Intel machine here has encoded. GPUs
before those have no AV1 encoder, so the stream offers H265 and H264. On an
RX 7900 XTX with AMD's driver 32.0.31041.1004 on Windows, `native/hwenc.c`,
built on its own with FFmpeg's Windows build, encoded with `av1_amf`,
`h264_amf` and `hevc_amf`, forced keyframes and a bitrate change included,
and dav1d decoded the AV1; a whole Yapfast hasn't run there yet, nor AV1 on
Linux, NVIDIA's or Intel's. The packetizer was checked against libaom's AV1
of the test recording, which dav1d decodes to the same pictures after the
packets are put back together, and against DAVE, which decrypts it.

AMD's RX 7000 series codes AV1 in blocks of 64 by 16 pixels, and the
picture's size rounds up: 320x180 came out as 320x192, and AMD gives
1920x1080 as 1920x1082. FFmpeg hands the crop on beside the stream, which
RTP can't carry, so viewers may see the extra rows at the bottom; sizes
such as 1280x720 fit the blocks. See AMD's [alignment
documentation](https://github.com/GPUOpen-LibrariesAndSDKs/AMF/wiki/AV1-Encoder#av1-specific-api)
and [the issue about it](https://github.com/GPUOpen-LibrariesAndSDKs/AMF/issues/423).
The Stream tab gives the size Yapfast encodes, before the rounding.

Wine has no AMF runtime, so there AMF fails to open and x264 encodes.

## The Stream tab, and what a share gives back when the machine is short

The call's Stream tab shows how a share goes out and what makes it: the
encoder and codec, what captured it and what that captured, the size and frame
rate, the bitrate sent and aimed for, the keyframes viewers asked for, the
packets resent, the frames a full queue skipped, and the shared sound waiting
or dropped. A watched stream's tab adds how far its picture runs from its sound
on the streamer's timeline, as it arrived and as the window was handed it, and
the pictures the audible sound had already passed by — the bound that keeps a
viewer from falling further behind. Beside those it says how the pictures reach the window — the
worker's own texture, or a copy of every frame — and the adapter the window
draws on, the wait a frame had between the capture having it and the encoder
being given it, which only the card's own capture times (the processor's
capturer hands frames over on its own thread, so that row reads "unknown"),
what the encoder's output buffers hold now and at their most, and, when there
is one, why the share isn't on the path it asked for. A value nothing answered
reads "unknown" rather than zero. The buffer figures are the encoder's output
pool: the receive side keeps no byte count for its reorder queues or its
resend history, so a watched stream shows no buffer row.

*Copy diagnostics* copies the same report as JSON, for a bug report, and
nothing else: no token, no server, no window title, and no media. What the
encoded frame's bytes were is never part of it; how many there were is. It
deliberately carries no count of the receive side's memory: nothing measures the
reorder queues or the resend history, so the encoder's pool is the only one in
it.

A worker samples the machine once a second — the physical memory the system
could still give a process, and the budget the active adapter's driver says it
has left — from the same loop that paces the stream, so there is no thread to
own or stop. When either is under 10% the share gives back what it can spare:
the local preview is scaled into a 640×360 box instead of its 1920 and the
encoder's idle output buffers are let go. What the window has already opened is
its own to release, and a share's memory policy doesn't reach across the
process boundary to take it back. It takes the room back only after every signal has
been above 20% for ten seconds, so a page coming free doesn't flip it back and
forth. What a viewer sees is untouched: the network's resolution, bitrate and
codec, the decoder's reference pictures and the sound stay exactly as they
were, because a share that lowered its resolution would be a worse stream, not
a cheaper one. A machine that can't be asked — no DXGI budget, or a platform
with no sampler — is not a machine that is short, and keeps normal behaviour.
The thresholds are policy, not measurements: they are named constants in
`crates/yapfast-core/src/pressure.rs`.

## What isn't verified here

This tree is not a qualification of any machine. What the code does is
described above; what has been run on hardware is this:

- **Nothing on a graphics card.** d3d11va decode, the video processor pass,
  the shared texture the window samples, NVENC reading a captured texture,
  AMF and Quick Sync output, and the D3D11 fences are compiled and
  unit-tested where a test can be without a device, and have not been
  exercised on one from here. A stream the card can't take falls back to the
  processor, and the log says why; whether a given machine's driver takes it
  is that machine's to answer.
- **Intel Quick Sync** is in the vendored build (oneVPL 2.14.0,
  `--enable-libvpl`) and in the probe, and no Intel GPU has run it. It is fed
  from the processor, like AMF, and says so.
- **Linux's card paths are wired, and the CUDA one has run.**
  The portal/PipeWire capturer, the X11 DRI3 capturer, the DMA-BUF picture
  channel, the VA-API decoder, the VA-API encode-from-a-DMA-BUF and the VA-API
  video processor that scales and converts a screen's picture into the surface
  the encoder reads are called from the stream worker now, each with its own
  refusal that falls back to the processor's path and says why. This machine
  has no portal, no compositor, no DRI3 and no VA-API driver, so every one of
  them was exercised only through its refusal: what a working session does is
  what the code says, not what anyone saw. The video processor is the one of
  them whose arguments, formats and fallback order are unit-tested without a
  device; whether a driver converts BGRA to NV12, at what cost, and whether its
  picture matches the processor's copy is unmeasured here — and on NVIDIA's
  linux driver, which has no video processor, a screen's pictures take the
  processor's copy, which is what the log says. NVIDIA's CUDA rung is the
  exception: on an RTX 4090 with driver 620.70.06 it opened, imported a linear
  DMA-BUF, converted it with its kernels and encoded the result with NVENC,
  moving no byte through the processor — and a tiled picture, the shape this
  compositor shares, took the GL-texture door, was read where it lay through a
  texture object and encoded the same way, also with nothing through the
  processor. What has
  not been seen is a share's own session end to end: the portal's handshake, the
  picture arriving on a PipeWire node and the worker's ladder run on it are the
  code's to answer, since this machine's session is not the one the tests drive.
- **Vulkan on Linux** is the GPU setting's path, in every Linux build: the
  window draws on a Vulkan device where one opens and on OpenGL, which the same
  Skia archive carries, where none does. The device, the import and the pass
  that reads the card's NV12 have no session on hardware behind them from here;
  what has run is the shader's validation and the pass being made on a device.
- **Lip sync** is not measured. The 80 ms bound and the two skews are
  computed as described, but a picture's presentation is its submission to
  the frames file — the renderer's own fence is the window's to observe — so
  the presentation skew is an estimate and the Stream tab says so by making
  the record `estimated`.
- **Window capture's transitions** — resize, minimise and restore, close,
  capture exclusion, repeated switches — are implemented and every one a test
  can decide without a window is decided; they have not been driven by a
  window on Windows.
- **Not measured on this branch's paths:** NVENC against x264, the receive
  side's allocation rate, the per-frame mmap cost, and what a constrained
  machine actually saves. `docs/measurements/README.md` says which numbers
  exist and which do not.

The optimisations a review proposed and this work did not take — a scaler
separate from an encoder session, a pool for the receive path's packets, a
byte budget for the reorder queues and resend history, memory tests beyond
Rust's allocator, presentation demand from the tile rather than the window,
fence waits off the UI thread, and per-source audio queue capacity — are
still open, and none of them is a known defect.
