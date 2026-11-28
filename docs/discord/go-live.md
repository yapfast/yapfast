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
stream. A call watches any number of streams at once (`voice::State::watching`),
each with its own connection, worker and sound, by the rule of Discord's
client (`STREAM_WATCH`'s `allowMultiple`, `Application::watch`): watching
a stream while none or one other is watched takes that one's place, and
shows the new one large; Shift and a click on a tile, or the eye with a
plus beside a tile's Watch stream (Start watching multiple streams, offered
while another stream is watched), watches it beside the others, and once
two or more are watched every next one goes beside them. Start watching
multiple streams leaves what's large as it is; a click on a tile shows its
stream large. Each watched stream stays in its own tile of the grid, the
size of the others, with its own Stop watching; stopping the one shown
large shows the grid again. Discord's client has no control that watches
every stream at once, so Yapfast has none. A stream watched in place of
another starts large, however watching began, and stays watched in its tile
after a click beside it. While the call is off screen, the large tile's
stream or camera, or else the stream watched last, or this account's own,
shows in a corner, as Discord's picture in picture does
(`VideoSpeakerStore.getSpeaker`).

The window looks for a watched stream's pictures only while its tile is in
sight on the stage, or it's the corner's: one scrolled away, hidden under
Hide Members, beside another tile full screen, or off screen gets no
pictures made. After five seconds of that its worker stops decoding as
well, and when the stream shows again it asks for a keyframe (`REST_AFTER`
in `stream_worker`) and decodes from it. Discord's client receives a
watched stream's video while its window shows, wherever its tile is, and
stops all video after the window has been hidden for 30 seconds
(`WindowVisibilityVideoManager`); Yapfast saves the decoding sooner, at
the cost of a keyframe when a stream comes back into sight.

STREAM_CREATE and STREAM_UPDATE name a stream's
viewers in `viewer_ids`; Yapfast keeps them for the stream it shares and those
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
Windows deletes once both processes close it.
The worker's standard error goes to Yapfast's log file. If its output ends
without an `ended` report, as when it crashes, the popup says "Screen sharing
stopped unexpectedly." and Yapfast logs a warning, "the stream worker ended
without saying why", with how it exited (`exit="signal=11 (SIGSEGV)"` on Unix,
the exit code on Windows), the job (`share`, `watch`, `play` or `glance`), the
encoder's `engine` and `codec` from the last stats a share reported, and the
picture `size` (or "up to" the job's bounds before one was reported).
`a_worker_that_dies_says_how` runs a worker script that kills itself, through
`YAPFAST_STREAM_WORKER`, which names the program to start instead of Yapfast.
In a Wayland session, libwebrtc's capturer asks the screen sharing portal what
to share. The portal answers over D-Bus through GLib, so the worker runs GLib's main
context while it waits for the dialog and for frames. Yapfast tells Discord it's
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
one goes on. The encoder takes the new size on the same connection.
`YAPFAST_STREAM_TEST_PATTERN=1` shares moving colour bars instead of a screen.

A camera can be shared too, such as OBS's virtual camera (v4l2loopback on
Linux, its DirectShow filter on Windows). The share menu lists the cameras
[the call's camera](voice.md#camera-video) lists, under CAMERAS, after the
screens and windows; on Wayland, where the portal picks a screen or window, a
"Screen or window" choice comes first. The menu stays available while live
so audio can be changed without opening the portal again. The worker films the camera through the same
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
and whenever RTCP asks for one, at most one a half second: every viewer who
lost the same packet asks at once, and a keyframe is many times a frame, so a
run of them at a rate that just dropped would be lost in its turn; [Choosing the H264
encoder](#choosing-the-h264-encoder) says how it's set up and why. Yapfast also
has VP8 (`native/vp8.c`, still built and tested); every Go Live sender known to
work sends H264. Each encoded frame owns the buffer it was encoded into
(`yapfast_discord::pool`): the encoder takes one of three and the frame gives
it back when it's dropped, so its bytes reach DAVE without another copy. The
encoder says how big the unit it produced is before it is copied, and the
buffer grows to hold it, so a picture is never encoded a second time and its
unit is copied once, straight out of the encoder's own memory: x264's output or
FFmpeg's packet. Only a frame that came out of FFmpeg as several packets is
gathered first, into a buffer one frame big, and libvpx's frame always is. The
buffers start at 512 KiB, or at a frame's own cap where that is smaller, and
double as a frame needs, up to 2 bytes a pixel and 4 KiB more; three of those
are 1.5 MiB at rest, where one buffer of the cap took 4 MiB at 1080p before. A
frame beyond the three allocates its buffer rather than waiting for one. H264
goes as whole NAL units or FU-A fragments, H265 as whole
units or FU fragments (RFC 7798), and AV1 in AOM's [RTP payload
format](https://aomediacodec.github.io/av1-rtp-spec/v1.0.0.html), as
libwebrtc's packetizer sends it: OBU elements without their size fields,
temporal delimiters, tile lists and padding dropped, an element too large
for a packet carried on in the next, OBUs of different layers in packets of
their own, and a keyframe's first packet marked as starting a coded video
sequence (N). A frame with a malformed OBU isn't sent. DAVE leaves an AV1
frame's last OBU without its size, its trailer after it, and receivers keep
it so, since DAVE's decryptor reads the frame as it was sealed. Past a
frame's first 8 packets, the rest are paced at 2.5 times the stream's
target bitrate, as libwebrtc's pacer sends, or faster when that wouldn't
send the frame within two frames' time, so a keyframe doesn't hold back the
frames after it until they're dropped: a 1080p keyframe of 200 packets sent
at once can overflow buffers on the way, and fills the sender's uplink. A
frame's packets wait in a queue with when each is due, and go out from a
branch of their own, so the stream's sound, NACKs and receiver reports are
handled while a frame goes out, and a report is timed when it comes; the
next frame is taken once the queue is empty.
Yapfast keeps the last 2048 video packets, none older than two seconds, and
when a viewer reports some lost with a generic NACK (RTCP 205), resends up to
128 of them as Discord's client does: on the RTX SSRC with RTX's payload type
and numbering, the original sequence number before the payload (RFC 4588), and
the repaired stream id (extension 12) in place of the stream id. A packet two
viewers lost is named in two blocks of the same report, and resent once. The
resends wait their turn (`rtp::Resends`) and go out eight at once, then a batch
a turn of the loop at 2.5 times the target: a NACK names up to 128 packets
because a burst just left a queue full, and answering it as one burst fills it
again. A request repeated while its answer is still on its way, half the
lowest round trip and no more than 100 ms (short of the 300 ms a viewer holds a
gap for), is the same lost packet, not another, and isn't answered twice. The
camera in a call still resends at once. An RTCP sender report goes out every
second.

Before sharing, the share button's menu picks a resolution and a frame rate, as
Discord's client asks for them: 720p, 1080p, 1440p or the screen's own size
(Source, up to 4K), at 15, 30 or 60 fps. It adds 480p, which Discord's client
keeps for its mobile performance preset. A resolution taller than the screen
picked is disabled, as scaling never enlarges, and a remembered one that is
shares at Source for that stream. Its "Higher quality" checkbox, which
Discord's client doesn't have, has the encoder work harder on each frame: x264
with "Encode streams with the GPU" off, and with it on the GPU's encoder, or
x264 where the GPU has none. Yapfast remembers the choice, the checkbox apart
for each, beside the chosen audio devices (`devices.json`), and starts at
1080p30 with both off.
The menu lists every screen, window and camera, and is never taller than
the window: where they don't all fit, as with many windows open or the window
short or zoomed, they scroll above the settings and Go Live, which stay in
view. Each setting is one row naming what's chosen, whose choices open beside
it: Audio, then Resolution and Frame rate in the order of the settings menu of
Discord's Go Live dialog (`id:"resolution"`, `id:"frame-rate"`).
Larger screens, and those of another shape, are scaled down to fit. Yapfast
caps them at the bitrates Discord's client uses: at most 3.5 Mbit/s up to 720p
and 9 Mbit/s above, the cap op 12 announces, with the frame rate as its
`max_framerate`. Discord's client allows accounts without Nitro 720p30, but
trials 1080p30 at 3.5 or 5 Mbit/s for them. The worker converts captured
frames to I420 with libyuv, scales them with libyuv's filtering scaler, and
encodes those; the preview is the same picture scaled down to the size the
window draws it, at most 720p and 30 fps, or up to the stream's size and frame
rate with "Enhanced live preview quality" in Settings, Advanced. The window
takes preview frames only while the preview is on screen, and once one has
waited untaken for a quarter of a second the worker makes no more until it's
taken, so a share hidden behind a game costs no preview. The window also
pauses it at once when its tile leaves the view, and while the window isn't
focused, as Discord's client does by default
(`pauseSelfStreamPreviewWhenUnfocused`): the tile keeps its last picture under
Discord's "Your stream is still running!" and "We’ve paused this preview to
save your resources." Discord's thumbnails go on being made.

Under the cap, the bitrate follows what the sender's uplink carries
(`yapfast_discord::congestion`). Sending more than it carries fills its
router's queue, and everything else the sender does online waits in it: a
game's ping climbs to hundreds of milliseconds and stays there while the
stream lasts. Discord's client runs libwebrtc's delay-based control on
transport-wide feedback (RTCP 205/15, about the sequence numbers of
extension 5), which Discord's stream server sends a sender on its UDP
connection too, about 9 a second. Yapfast doesn't read that feedback yet; it
measures the round trip from the receiver reports the server sends about the
video, about two a second, which echo the last sender report (`lsr`) with
the delay since (`dlsr`), as RFC 3550 describes, and reads the share of
packets lost from them. A queue at the router lengthens the round trip as
much as the delay it adds.

A stream starts at 2.5 Mbit/s, or its cap when that's lower. Each report
whose round trip is within 25 ms of the lowest of the last minute, with at
most 2% of packets lost, raises the target by 8%, up to the cap: about 17 s
from the start to 1080p's 9 Mbit/s, with a report a second. It doesn't rise
past 1.5 times what went out over the last half second, video and resends,
counted in slices of 50 ms, nor before the stream has sent for that long: a
calm picture sends less than the target, and a target raised over it would
be tried on the uplink only once the picture gets busy, all at once. A
round trip more than 60 ms over the lowest in two reports in a row, not one
alone as after a keyframe, or more than 10% of packets lost in one, lowers
it to 85% of what Discord's server received, or of the target when that's
lower, and it doesn't rise again for 3 s. What the server received comes
from its receiver reports at least 0.4 s apart: how far the highest
sequence number it got went, less the packets it counted lost meanwhile, at
the size of the packets sent lately. The server passes on viewers' reports
about the same video too, counted from further back than its own, so each
reporting SSRC (up to four, forgotten after 5 s of silence) keeps its own
count, round trips and queue reports: the rate received is the fastest of
their counts, a report that hasn't moved past the one it would be measured
from says nothing, and a viewer's longer round trip is compared with that
viewer's own lowest, not the server's. While a queue builds that's what the
uplink carries, however much more went out, so when a calm picture turns
busy at once and sends far past it, one back-off takes the target under
it. Without a count, as before the second report, it backs off from what
went out over the last half second. From
80% of the rate it backed off from, each rise adds a hundredth of that rate,
so it comes back to where the queue built slowly and stays there longer
before the next back-off; past 115% of it the uplink carries more than it
did, and the target rises by 8% again. It stays under the server's bandwidth
estimate (REMB) when that's lower than the cap, while the estimate is less
than 5 s old (one that stops coming no longer holds it down), and never goes
below 500 kbit/s. Without a measured round trip, as when reports don't echo a
sender report, the target doesn't rise from where it started. The encoder
changes its bitrate only when it differs from the target by more than a
tenth, so a rise reaches it every second step and a fall at once.

Discord shows a still of each stream, in the call and to people outside it.
Like Discord's client, Yapfast makes one once the stream connects and the
picture isn't black any more (a screen is black until what's shared draws),
then every 5 minutes: the picture scaled down to fit 512 by 288, a JPEG at
quality 92. The worker reports it as a data URI, and Yapfast uploads it with
`POST /streams/{stream key}/preview`. A failed upload is tried again a minute
later, up to three times; a newer still replaces an upload not done yet, and
stopping the share cancels it.

The share menu's Audio choices apply before Go Live and immediately
while sharing, independently of the video source: No audio, All applications,
or on Windows an application selected by its window title. The latter captures
the window's owning process and its child processes using WASAPI's include
process-tree mode. This shares an application's audio, not an individual browser
tab or one window within the same process. A closed or unsupported window gives
an audio error and keeps the video running without falling back to all applications.
Choosing No audio stops and releases capture; choosing another source opens a
fresh capture on the same stream connection.

Yapfast remembers the choice with the stream's quality (`devices.json`, its
`audio`), as Discord's client keeps its Share Audio switch with the resolution
and frame rate (`soundshareEnabled` in its `ApplicationStreamingSettingsStore`).
A window's id ends with its window, so an application is remembered by its
program's file name, such as `game.exe` (`audio_io::window_programs`, kept in
`voice::State::stream_app`). When the share menu lists the windows, before
going live, a window of that program becomes the choice. While the application
isn't running, as it may not be after a restart, All applications is shared
and shown as the choice, and the application stays remembered until another
choice is made. A window chosen in this run that has closed, with no other
window of its program open, shows as Unavailable application. A live stream's
audio changes only when it's chosen.

By default a shared screen carries the sound every other application plays, as Discord's
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
back to the people in it. A sample a driver hands over that isn't a number
becomes silence, and a sum over 0.9 of full scale bends towards full scale
rather than clipping. The worker mixes them every 20 ms, waiting until an
application has 40 ms queued and keeping the newest 200 ms of each, and
encodes Opus at 64 kbit/s, what Discord's client gives a stream's sound. Frames go out
as plain Opus RTP on the stream's audio SSRC, DAVE-encrypted, with sender
reports, and op 5 marks them as shared sound (speaking flag 2). Nothing is
sent while no application plays. On Windows, WASAPI's process loopback hears
everything but Yapfast's own process tree as one stream, from Windows 10's 2004
update on; it can't leave other Discord clients out. Other systems share
video only for now, and so does the test pattern. Two ignored tests check the capture against the
machine's sound server with a quiet tone: another process's is heard, and
Yapfast's own is not.

On Windows, the opt-in `captures_and_encodes_selected_application_audio_on_windows`
test plays two quiet tones in separate processes and checks all-app capture,
isolation of each selected application, reopening capture, and an Opus round
trip. Run it with working speakers in a desktop session:

```sh
cargo test -p yapfast-core --features discord captures_and_encodes_selected_application_audio_on_windows -- --ignored --nocapture --test-threads=1
```

This checks the local Windows capture and codec path, not delivery through Discord.

Each video packet carries the one-byte extensions a Discord client sends: a
transport-wide sequence number (id 5), playout delay (6), screen content
type (7) and the stream id `100` (11). The stream id is what ties a packet to
the stream op 12 declared. These ids aren't the ones in Discord's WebRTC
SDP, where 5 is playout delay: with only playout delay as id 5, Discord's
server drops every packet, and viewers time out with error 2012.

DAVE follows songbird's handshake (`dave.rs`) and encrypts each frame once the
group is ready. A stream's MLS group isn't the voice channel's: its id is the
RTC server id minus one, which Discord-video-stream found too. If the server's
proposals or commits name another group before Yapfast joins, it restarts the
DAVE session under that id and sends a fresh key package. A welcome for the
old package triggers the usual invalid-welcome recovery. Outgoing media
switches to the new key when the gateway executes the transition. Yapfast
accepts the people op 11 names into the group, and
also those named in video attributes (op 12) or in the stream key.

With `YAPFAST_LOG=debug`, Yapfast logs every 5 s while sharing what it sent and the
RTCP that came back (`stream sent`): receiver reports with the highest
sequence number Discord's server got and its echo of our sender reports
(`lsr`), its bitrate estimate (REMB), and keyframe requests, with the last
round trip measured (`round_trip_ms`), the target (`target_kbps`) and what
went out over the last half second (`sending_kbps`) and what the server
received (`received_kbps`). Each
change of the target is logged as "changing the stream's bitrate", with
`kbps`, the `reason`, the report's `round_trip_ms` and `lost` (out of 256),
and the lowest round trip (`lowest_ms`); the encoder following it logs "the
encoder follows the stream's target". The voice connection logs the voice server's RTCP too, to compare
with a sender Discord accepts. Messages the stream server sends are logged
too, except those carrying keys.

Watching works the same way in reverse. Clicking someone's LIVE badge, or their
tile in the call, sends op 20 and starts a worker with a watch job, one for
each stream. The worker
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
echoed). A packet that comes after the frames went on without it, or a second
time, is counted received once (the 256 sequence numbers below the highest
are remembered), so reordering isn't reported as loss the streamer would back
off for. With `YAPFAST_LOG=debug` it logs what it received every 5 s (`stream
received`): packets out of order, resent, recovered, given up on, and keyframes
asked for. The worker decodes with libvpx or with the FFmpeg H264 decoder in
libwebrtc's archive (`native/decode.c`), AV1 with the dav1d in the same
archive (`native/av1.c`), a picture out for each frame in, or H265 with the
HEVC decoder of `vendor/ffmpeg-src`'s FFmpeg (`native/hevc.c`), which
libwebrtc's lacks, and writes RGBA at up to 1080p into the shared frames,
scaled first to the size the window draws the stream at, which the window
writes into the frames file's header (`Frames::want`): the picture is scaled
before it's made RGBA, which costs less than the window scaling RGBA each
time it draws. While the stream isn't on screen the worker still decodes each
frame, which the ones after it need, but doesn't make it a picture.
When a mobile sender signals its orientation in RTP, the viewer keeps the
last signaled orientation for that video source and applies it before display.
The upright dimensions determine how the picture fits in the shared frame.
dav1d takes 0.07 of a core for 1080p30 AV1 at 3.6 Mbit/s, against 0.1 for
H265 at the same bitrate. That FFmpeg is built with
its x86-64 assembly, without which H265 took over three times the processor
time. Watching is possible only in the call you're in, so clicking someone
sharing in another channel joins it first, once asked, and watches once the
call is connected; Discord drops a watch sent before. The stream's sound, Opus
on the streamer's audio SSRC, is DAVE-decrypted as audio and played by the
worker through `audio_io.rs` on the call's speakers (`stream_audio::Player`),
about 100 ms queued there. Before that a playout (`stream_audio/player.rs`)
puts the frames in sequence order, starting once 60 ms waits (or 100 ms after a
lone frame), and plays 20 ms a turn: a missing packet is waited for 60 ms, then
made up from the next packet's forward error correction or Opus's concealment,
and after 100 ms missing in a row silence plays until sound comes again. It
holds at most 200 ms of sound, so a stall or a sender running fast never turns
into lasting delay: what waited through a stall is dropped, and with frames to
spare for a second, one quiet frame goes unheard. A packet longer than 20 ms
plays over as many turns as it takes. Being deafened silences it, which Yapfast tells the worker
with a `hear` control. Yapfast's workers play as "Yapfast", which sharing
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
than x264 at low bitrates. Intel graphics encode with x264 on Windows so far.
The setting lives in `layout.json` and reaches the worker with each
`Job::Share`; the camera stays on x264. When the GPU's H264 encoder fails
during a stream, x264 takes over from that frame with a keyframe. The worker
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
preset, `ll`; AMF's `lowlatency` usage and balanced preset), no B-frames, a
frame out for each frame in, Main profile, keyframes every 3 seconds and when
asked, and variable bitrate capped at the target over half a second, which
leaves a calm screen below the cap rather than padding it. NVENC, AMF and
VA-API change their bitrate as they run, without the IDR frame FFmpeg would
insert, with a patch to FFmpeg for each (AMF's sets the target, peak and buffer
on the running encoder). "Higher quality" asks NVENC for p5, AMF for its
quality preset and VA-API for compression level 1, and opens the usual preset
where that fails. NVENC's slowest preset looked no better than p4; p5 hasn't
been compared.

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
on Intel's Arc and Core Ultra graphics through VA-API (iHD) on Linux. GPUs
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
