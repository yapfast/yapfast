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
Windows deletes once both processes close it.
On Linux, libwebrtc's capturer asks the screen sharing portal what to share.
The portal answers over D-Bus through GLib, so the worker runs GLib's main
context while it waits for the dialog and for frames. Yapfast tells Discord it's
live only once frames flow, so a cancelled dialog shares nothing. Windows has
no such dialog, so the share menu lists the screens and windows libwebrtc's
capturers find (`stream_worker::shareable`), and the worker captures the one
picked by its id; see the
[Windows guide](../windows-beta.md#go-live-on-windows).
While live, the voice dock's share button switches what's shared without
ending the stream: the worker starts a second capturer (on Linux, through the
dialog again) and keeps sending the first one's frames. When the second one
gives a frame, it replaces the first. If the dialog is cancelled, the first
one goes on. The encoder takes the new size on the same connection.
`YAPFAST_STREAM_TEST_PATTERN=1` shares moving colour bars instead of a screen.

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
work sends H264. H264 goes as whole NAL units or FU-A fragments, H265 as whole
units or FU fragments (RFC 7798), and AV1 in AOM's [RTP payload
format](https://aomediacodec.github.io/av1-rtp-spec/v1.0.0.html), as
libwebrtc's packetizer sends it: OBU elements without their size fields,
temporal delimiters, tile lists and padding dropped, an element too large
for a packet carried on in the next, OBUs of different layers in packets of
their own, and a keyframe's first packet marked as starting a coded video
sequence (N). A frame with a malformed OBU isn't sent. DAVE leaves an AV1
frame's last OBU without its size, its trailer after it, and receivers keep
it so, since DAVE's decryptor reads the frame as it was sealed. Past a
frame's first 8 packets, 4 go out
each millisecond: a 1080p keyframe of 200 sent at once can overflow buffers on
the way. Yapfast keeps the last 2048 video packets, and when a viewer reports
some lost with a generic NACK (RTCP 205), resends up to 128 of them as
Discord's client does: on the RTX SSRC with RTX's payload type and numbering,
the original sequence number before the payload (RFC 4588), and the repaired
stream id (extension 12) in place of the stream id. An RTCP sender report goes
out every second.

Before sharing, the share button's menu picks a resolution and a frame rate, as
Discord's client asks for them: 720p, 1080p or the screen's own size (up to
4K), at 15, 30 or 60 fps. Its "Higher quality" checkbox, which Discord's client
doesn't have, has x264 work harder on each frame. Yapfast remembers the choice
beside the chosen audio devices (`devices.json`), and starts at 1080p30 without
it. Larger screens, and those of another shape, are scaled down to fit. Yapfast
sends them at the bitrates Discord's client uses: at most 3.5 Mbit/s up to 720p
and 9 Mbit/s above, the cap op 12 announces, with the frame rate as its
`max_framerate`. Like Discord's client it also follows the server's bandwidth
estimate (REMB): when that drops below the cap, the encoder aims for the
estimate instead, though never below 500 kbit/s, and it changes only when the
two differ by more than a tenth. Discord's client allows accounts without Nitro
720p30, but trials 1080p30 at 3.5 or 5 Mbit/s for them. The worker converts
captured frames to I420 with libyuv, scales them with libyuv's filtering
scaler, and encodes those; the preview is the same picture scaled down to 720p.

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
applications). On Linux, PulseAudio or PipeWire's
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
libwebrtc's lacks, and writes RGBA at up to 1080p into the shared frames.
dav1d takes 0.07 of a core for 1080p30 AV1 at 3.6 Mbit/s, against 0.1 for
H265 at the same bitrate. That FFmpeg is built with
its x86-64 assembly, without which H265 took over three times the processor
time. Watching is possible only in the call you're in, so clicking someone
sharing in another channel joins it first, once asked, and watches once the
call is connected; Discord drops a watch sent before. The stream's sound, Opus
on the streamer's audio SSRC, is DAVE-decrypted as audio and played by the
worker through `audio_io.rs` on the call's speakers (`stream_audio::Player`),
about 100 ms queued; being deafened silences it, which Yapfast tells the worker
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
on the running encoder). "Higher quality" changes nothing on the GPU: NVENC's
slowest preset looked no better than p4.

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
