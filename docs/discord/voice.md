# Discord voice

Scope for the beta:

- Server voice channels: join, leave, a participant list, speaking indicators,
  mute, deafen and per-person volume.
- DM and group DM calls, both incoming with ringing and outgoing.
- One connection at a time across Yapfast and Discord. Joining Discord voice asks
  to leave a Yapfast call, and the other way round.
- The same devices, push-to-talk, voice activation, echo cancellation and
  voice leveling as Yapfast calls. Noise suppression is DeepFilterNet, or
  RNNoise, rather than WebRTC's, as described under Microphone below.

Clicking a voice channel joins through the same voice dock and one-call rule as
Matrix voice. Calls start with the microphone on unless Settings turns that
off, mute and deafen show in Discord, and a voice gate sends audio only during
speech. Audio goes through `audio_io.rs`, PulseAudio on Linux and WASAPI on
Windows, and libwebrtc's processing module; see the [Windows
guide](../windows-beta.md#discord-voice-on-windows) for what's been checked there.
davey's change for Go Live groups is described in
`vendor/davey/README.yapfast.md`.

Discord may move a call to another voice server, saying so with a new
`VOICE_SERVER_UPDATE` while the call is connected. Yapfast connects the call
there and it goes on. The old server may drop the connection before that
word comes, so a dropped connection waits five seconds for it before the
call ends. A stream moves the same way, with a new `STREAM_CREATE` and
`STREAM_SERVER_UPDATE`: sharing sends to the new server, after waiting up to
five seconds for it when the old connection ends, and watching connects again.

The account may be in voice on another client too, such as Discord's app on
a phone. Yapfast tells that client's voice state from its own by the
`session_id` it carries: READY's is this gateway session's, and a voice
state with another is the other client's. The voice channel lists the
account there as Discord does: dimmed until hovered, without its icons, with
the tooltip "Connected on another client". Joining here takes the call over,
and the join waits for a voice state of this session. When the other client
joins voice, Yapfast's call ends without sending op 4 with no channel, which
would end the other client's call too; Yapfast sends that only for a join under
way or a voice state of its own session.

Camera video works in server voice channels and DM calls; see
[Camera video](#camera-video). Go Live is in [go-live.md](go-live.md).

Since March 2026, Discord voice requires its end-to-end encryption, DAVE, and
[rejects clients without
it](https://support.discord.com/hc/en-us/articles/38749827197591-A-V-E2EE-Enforcement-for-Non-Stage-Voice-Calls).
[davey](https://github.com/Snazzah/davey) (MIT) implements DAVE in Rust. Calls
run on Yapfast's own voice connection, the one Go Live uses, rather than
[songbird](https://github.com/serenity-rs/songbird); [How calls carry
video](#how-calls-carry-video) says why.

Yapfast's current audio path runs inside LiveKit's libwebrtc. Discord voice
needs its own Opus encoding and decoding plus the same audio processing.
libwebrtc's device module can't be reused outside a LiveKit room: it hands
captured audio straight to WebRTC and only plays remote WebRTC streams. Its
audio processing module can, through
`livekit::webrtc::native::apm::AudioProcessingModule`. So Discord voice gets a
small path of its own: `audio_io.rs` for the microphone and speakers at 48 kHz,
the processing module for echo cancellation and gain (noise suppression is
DeepFilterNet; see [Microphone](#microphone)), then Yapfast's voice connection
for Opus and DAVE. Feed the speaker mix to the processing module's reverse
stream so echo cancellation has its reference.

Opus comes from the libopus inside Yapfast's libwebrtc archive, called directly
(`yapfast_discord::opus`), not from a crate such as `opus2` that builds a
libopus of its own: two copies of the same symbols risk a clash when
linking, particularly with MSVC on Windows.

## Connection details

As in Discord's voice panel, the dock's status has a signal icon graded by
the call's connection, and hovering it or the status shows the last ping
("42 ms"). The same icon sits in the call's conversation header. A click on
either opens the details, as Discord's popout from its panel has them: a
Connection tab with a graph of the ping over the last five minutes, the
average and last ping, and the outbound packet loss rate; and while the call
is end-to-end encrypted with someone else in it, a Privacy tab with the voice
privacy code in six groups of five digits, to compare with the others, and
a button that copies it. Discord's client shows its graph only in developer
mode; Yapfast shows it to everyone. A notice at the bottom says "End-to-end
encrypted" while DAVE encrypts the call's frames, and "Secure connection"
otherwise, as Discord's does; the dock's detail line says the same.

The ping is the voice gateway's heartbeat round trip, from op 3's nonce,
the time it was sent, until op 6 echoes it, as Discord's web client
measures it. Heartbeats go every 5 s rather than the 13.75 s the server
asks, as that client caps them, and the call keeps the last 60 pings, 120
bytes. The average is over all of them; Discord's client averages its
oldest 20. The loss rate comes from the RTCP receiver reports Discord's
server sends about this account's audio and camera: the packets reported
lost between two reports, out of those expected, summed over both, as
Discord's client sums its outbound streams. It's 0 while nothing is sent,
and unknown until two reports came. The grades are Discord's: bad past
500 ms or 10% lost, average past 250 ms or 5%, fine below; the icon is
grey until the first ping.

The call's thread writes these into `yapfast_discord::call_stats::CallStats`.
Yapfast copies the pings out only when the tooltip opens and each second while
the popout is open. The state the UI refreshes from changes only when the
grade or the encryption does. `talks_to_the_fake_server` checks the ping and
the loss are measured.

While this account's screen is live, a Stream tab says how it goes out:
the encoder and where it runs ("NVENC on the GPU", "x264 on the CPU"), the
codec Discord chose, the size and frame rate sent, the bitrate sent over
the last second beside the one aimed for, which follows Discord's bandwidth
estimate, and the keyframes viewers asked for and the packets resent to
them. Discord's client shows its stream's numbers only in developer mode.
The stream worker reports them once a second while live
(`Report::Sending`), counting what `StreamSender` sends; the call keeps
the last report beside its own numbers, without refreshing the UI's state,
and the popout reads it as it reads the pings. A report more than 3 s old
means the stream stopped, and the tab goes. `talks_to_the_fake_server`
checks the bytes counted and a resend.

Matrix calls don't have these yet. LiveKit's SDK measures its signal
connection's round trip but doesn't expose it, and `Room::get_stats`
serialises all of WebRTC's statistics to JSON and parses them back, too much
to run every few seconds for a graph. A Matrix call's detail line says
"End-to-end encrypted" while LiveKit encrypts its frames.

## How calls carry video

Calls run on the connection Go Live uses (`voice_connection.rs`, shared by
`stream_connection.rs` and `call_connection.rs`), not on songbird with video
added. Video in songbird would have meant patching most of its driver, which
speaks version 4 of the voice gateway and has DAVE for Opus only, and keeping
those patches through its releases, while Yapfast already had a version 8
connection carrying video with DAVE. It also leaves one libopus, libwebrtc's.
`yapfast_core` drives a call through `Call`, `Microphone`, `Track` and
`Listener`, and the fake voice server exchanges audio both ways to test it.

A call's connection sends Opus stereo at 128 kbit/s in its music mode, 20 ms
a packet, with five silent frames when the microphone pauses, as Discord's
clients do. It marks speaking with op 5 when it starts. The RTP timestamp
goes on 20 ms every tick, sending or not, as a microphone's clock does.
WebRTC's jitter buffer (NetEq), which Discord's clients play others through,
compares packets' arrival times with their timestamps, so with a timestamp
that stopped during pauses, every packet after a pause looked as late as the
pause was long, and the start of speech was held back by up to half the pause
(`decision_logic.cc`, `packet_arrival_history.cc` in NetEq's source). Received
audio waits in a jitter buffer per speaker: playing
starts once 60 ms has arrived, a missing packet plays as Opus's loss
concealment (or from the next packet's forward error correction), and a
speaker who stays silent for 100 ms drops out of the mix until they speak
again. When the voice gateway drops, the connection resumes with op 7, and
connects again from scratch if resuming fails.

## Camera video

Turning the camera on in a call does what Discord's clients do: the main
gateway's voice state says `self_video`, and the call's connection announces
the camera with op 12, its video and RTX SSRCs (those op 2 assigned) and one
stream at rid "100", at most 1280×720 and 30 frames a second. Discord's
clients offer a second layer at quality 50; Yapfast sends one, as its Go Live
does. Turning it off sends op 12 again with a video SSRC of 0 and no
streams. Yapfast offers H264 to encode, as for Go Live, and both H264 and VP8 to
decode. The camera's pictures go to x264 on the camera's own thread, at Go
Live's lighter effort and 1.5 Mbit/s, and each frame to the call's thread, which encrypts it with DAVE
and sends it as Go Live's packetizer does: sender reports, a resend from RTX
for each NACK, and a keyframe when someone asks with a PLI. A frame the
call's thread can't take in time is dropped, and the next one is a keyframe.

The others' cameras arrive the same way round. Op 12 says who films on which
SSRC, and Discord forwards a camera only to those who ask for it with op 15
("media sink wants"): `{"any": 0, "<video SSRC>": quality, "pixelCounts":
{...}}`, quality from 0 to 100. The sender films at the quality asked of it, on a
ladder Discord's clients work out from 1280×720: 100 is 1280×720 at up to
2.5 Mbit/s, 90 896×504, 80 672×378, 70 640×360, 60 512×288, 50 416×234 at
264 kbit/s, down to 20. Yapfast asks for each camera whose tile is in
sight as Discord's desktop app does, with the tile's pixels: while more
than one camera films, the tile in focus at 100; in a call with one other person, the first step larger
than the tile, as far as all the cameras fit in 1280×720 together; in a
larger call, the largest step at which they fit (90 for two cameras, 80
for three). Any other camera, 0. Frames come together as Go Live's
do (reordering, NACK, a PLI after a loss) and are decrypted with the
sender's DAVE key. Whole frames go to one thread, `yapfast-discord-video`,
which keeps an FFmpeg H264 or libvpx VP8 decoder on one thread for each
camera in sight, and writes each picture straight into the tile's feed at the
tile's size. A tile that leaves the screen loses its decoder within a second,
and its buffers at once; a new decoder starts from a keyframe. The others'
voice states say `self_video`, which marks them with a camera in the channel
list and gives their tile a feed.

`talks_to_the_fake_server` checks cameras both ways in a server's voice channel
and in a DM call, against the fake's. `testing/measure_discord_cameras.py`
measures a call with cameras; see [measurements](../measurements/README.md).

## DM calls

A DM or group DM has a call, as in Discord. The phone in its header starts
one, or joins the one going on, through the same voice dock and one-call
rule as a voice channel. Joining sends op 4 without a server, naming the DM
as the channel; VOICE_STATE_UPDATE and VOICE_SERVER_UPDATE come back without
a server too, the latter with the channel's id, and the voice connection
identifies with the DM's id where a server's call gives the server's. Yapfast
keeps those voice states under the server `@me`, as DM room ids do.

Discord sends CALL_CREATE, CALL_UPDATE and CALL_DELETE for the DMs' calls,
including one for each call going on when Yapfast connects (the
AUTO_CALL_CONNECT capability Yapfast already asks for). Each names whom the call
rings. Starting a call, rather than joining one with someone in it, rings the
others with `POST /channels/{id}/call/ring` and no recipients, once Discord
has reported the call: it rings only in a call it knows. Calling and ringing
are allowed separately. In a DM, Yapfast rings a friend at once and anyone else
only when `GET /channels/{id}/call` says they're `ringable`, as discord.py-self
does; otherwise, or when Discord refuses, the call says so until someone
comes, and the other person sees the call only in the DM. Yapfast plays a soft
ringback while nobody has come. People
being rung show in the call faded, "Calling...", until they answer or Discord
stops ringing them.

A call ringing this account shows a card at the top of the window with the
DM's picture, Decline and Accept, and Yapfast rings: a rising arpeggio
synthesized in `ring.rs` and played through `audio_io.rs` on the call's speakers
until the call is answered, declined or stops ringing. While Yapfast isn't in
front a desktop notification says who is calling. On Do Not Disturb the card
shows without the ring or the notification. Accepting opens the DM and
joins, leaving any other call without asking, since answering already chose.
Declining posts `POST /channels/{id}/call/stop-ringing` with this account,
which leaves the call to the others. The DM's own call shows the same choice
above its messages when it's open.

While this account is in a DM's call, the call's stage shows above the DM's
messages, taking less than half the conversation, more while watching a
stream. Without it, a DM with a call going on shows who is in it and a button
to join, and its row in the DM list a green phone. Discord's call messages
(type 3) read "started a call.", and once Discord adds when it ended, "started
a call that lasted 5 minutes.". Go Live works in DM calls too: op 18 with the
type `call` and no server, and the stream key `call:{channel}:{user}`. The
fake server covers all of this. Message requests
have no call until accepted. A DM call through real Discord, and Go Live in
one, haven't been checked by hand yet.

## Soundboard

The soundboard button, before the camera in the dock and among the call's
controls, opens Discord's sounds: the account's favourites first, then the
call's server's, then with Nitro the account's other servers', then
Discord's own, each with its emoji. A click plays one for everyone in the
call. Hovering over one offers hearing it first, here only, and a star that
adds it to the favourites or takes it out; a favourite's star stays lit. In
a server's voice channel that takes Speak and Use Soundboard, as Discord's
client checks, and other servers' sounds take Use External Sounds too; a DM
call always has the soundboard. While deafened or muted by the server, it
says so and plays nothing, as Discord's does. A sound the server can't keep
without its boosts (`available: false`) doesn't show.

Sounds don't go through the voice connection. Playing one posts
`POST /channels/{id}/send-soundboard-sound` with its id and emoji, and for a
server's sound `source_guild_id`, as Discord's client does. Discord sends
VOICE_CHANNEL_EFFECT_SEND to everyone in the call, and each client
downloads the sound from the CDN, `/soundboard-sounds/{id}`, MP3 or Ogg,
and plays it at the sound's volume. Yapfast plays its own at once, as
Discord's client does, and leaves out the effect that comes back for it.
The sounds mix into the call's speakers in `Hearing`, at the sound's
volume times the soundboard volume times the speakers', so echo
cancellation takes them out of the microphone; eight play at once at
most, and deafening stops them. libwebrtc's FFmpeg demuxes and decodes
the file (`Video::open_sound`), resampled to 48 kHz stereo and cut at
6 s, Discord's limit being 5.2 s. A call keeps the files it downloaded,
up to 2 MiB, to play them again, and lets them go when it ends.

As a call connects, Yapfast asks for Discord's own sounds with
`GET /soundboard-default-sounds`, once a session, and for the server's with
the gateway's op 31, REQUEST_SOUNDBOARD_SOUNDS, which SOUNDBOARD_SOUNDS
answers; Discord's client asks for them then too. With Nitro it asks for
every server's, as Discord's client does; without, only the call's server's,
since no other's can be played. GUILD_SOUNDBOARD_SOUND_CREATE, _UPDATE and
_DELETE, and GUILD_SOUNDBOARD_SOUNDS_UPDATE, keep the list current. The list
lives in the call and goes with it.

The favourites are Discord's, kept for every client of the account in the
second settings, `FrecencyUserSettings`, as `favorite_soundboard_sounds`
(field 8): the ids, and their order. A call reads them as it connects with
`GET /users/@me/settings-proto/2`, and USER_SETTINGS_PROTO_UPDATE brings
another client's changes; a star writes the whole list back with `PATCH`,
250 at most, Discord's limit.

Someone's menu in a voice channel has Mute Soundboard beside Mute (see
[servers.md](servers.md#someones-menu-in-a-voice-channel)): the sounds they
play from then on stay silent here. Discord's client keeps it locally, and
so does Yapfast, in its layout.

### Soundboard volume and entrance sounds

Settings, Voice & video, has Discord's soundboard volume, a slider from 0%
to 100%, and the account's entrance sounds, both kept by Discord for every
client in the first settings, `PreloadedUserSettings`. The volume is
`voice_and_video.soundboard_settings.volume` (fields 5, 9, 1), a float
Discord stores on its slider's curve, (slider / 100)^2.8 × 100, the same
curve as the volume sliders; sounds play at that over 100. Letting go of
the slider writes it.

An entrance sound, a join sound in Discord's code, plays to everyone in a
voice channel as the account joins it, with Nitro. Each server's settings
in `guilds` (field 3, a map by server id) may hold one, `join_sound` (field
6): the sound's id and its own server's, 0 for Discord's own; the entry
under server 0 is the one for every server. Settings chooses one for "All
servers" or a server in the same picker as the call's, listing Discord's
sounds and every server's, which it asks for with op 31 as it opens, and
Remove takes it away. A `PATCH /users/@me/settings-proto/1` replaces each
top-level field it sends whole, so Yapfast reads the settings with `GET`
first and sends the whole of `voice_and_video` or `guilds` with the one
value changed.

As a call connects, with Nitro and outside a stage channel, Yapfast looks
up the server's entrance sound, else the one for every server, as Discord's
client does; a DM's call takes the one for every server. Once the sounds
have come, within 10 s, and if the account may play that one there and
isn't deafened or muted by the server, it plays it here and posts `POST
/channels/{id}/custom-call-sounds` with one of Discord's emoji animations,
`{"animation_type": 0, "animation_id": 10..15}`. Discord reads the sound
from the settings and sends the others VOICE_CHANNEL_EFFECT_SEND.

A server's sounds in its settings aren't there yet. `plays_the_soundboard`
and `keeps_the_soundboards_settings` check the service against the fake,
whose sounds are `testing/fixtures`' tones.

## Microphone

Discord's own client suppresses noise with Krisp, a neural network, and
WebRTC's suppressor sounded different from it. So Yapfast's Discord microphone
runs, in 10 ms blocks: WebRTC's echo cancellation and high-pass filter; if
asked, taking the room's echo out; a noise suppressor, which also tells the
gate which blocks are speech; WebRTC's gain control on what's left; then
Yapfast's leveling and the microphone's volume, from
[Settings](../voice.md#sounds).

The suppressor is DeepFilterNet 3, through libDF, its Rust implementation on
tract (crate `deep_filter`; the code and the model's weights are MIT or
Apache-2.0). crates.io has only libDF 0.2.5, from before DeepFilterNet 3, so
Yapfast takes the repository's last commit, `d375b2d`, and its `default-model`
feature builds the 8 MB model into Yapfast. It runs both of its stages on every
block but in a pause or while push to talk sends nothing, as libDF's command
line does on every block. The library's defaults skip deep filtering above
20 dB SNR, which dulled clean speech and took up to 5 dB off speech over a
fan. A pause starts after 500 ms of blocks its SNR estimate puts below -10 dB,
past the gate's 400 ms hold, and lasts until a block isn't: only its encoder
runs, a third of its work, and the block's gains are zero, which leaves the
strength's mix of the microphone. Skipping the decoders from the first such
block dulled what the gate sent. Push to talk sends pauses too while the key
is down, so it runs in full then, and with the key up only its encoder runs,
whatever the microphone hears.

RNNoise, through `nnnoiseless` (BSD-3-Clause), lets keyboard clicks through,
and they hold the gate open. Settings → Voice & video → "Noise suppression in
Discord calls" still chooses it, to compare the two, as does Off, for a quiet
room: RNNoise then only tells the gate which blocks are speech. Yapfast keeps
the choice in `layout.json` as `noise_suppression`. A call going on switches
at its next block, after DeepFilterNet's 0.2 s load if switching to it.
Muting closes the microphone, but the call's microphone thread,
`yapfast-discord-in`, waits with the suppressor until the call ends, 39 MiB
for DeepFilterNet, so unmuting or switching microphones doesn't load it
again. What it kept from before made no difference: after 30 s of another
room, the PESQ of what the gate sent moved by at most 0.02.

"Noise suppression strength" limits how far noise is turned down: Full takes
out all it can, Medium at most 30 dB and Light 20 dB (`noise_strength`, Medium
unless changed). Full cuts people's laughs: DeepFilterNet takes high-pitched
giggles (a pitch of 440-760 Hz) for noise, with its SNR estimate at -15 dB as
for typing, and puts out silence for them. The lighter limits let a few more
chair creaks open the gate. libDF's threshold for silencing a block, a pitch
detector and RNNoise's speech probability as a second opinion didn't save the
giggles: the network's own gains silence them, and the last two found at most
half of each while firing on typing over a hum. libDF mixes the microphone back
into its output spectrum to keep to the limit; RNNoise's output gets the same
mix of the input it lines up with, a block earlier. Below 20 dB, typing that
DeepFilterNet left got past the gate's level after gain control, so there's no
lighter step.

The voice gate opens on three blocks in a row that the suppressor takes for
speech and that are louder than the input sensitivity, -60 dBFS unless changed,
or that are 18 dB louder still. While open, each such block holds it another
400 ms. What DeepFilterNet leaves of a mouse click comes out over two blocks,
since its frames overlap, louder than the sensitivity, and its estimate takes
it for speech, so fewer than three blocks let clicks open the gate. Under
speech, DeepFilterNet turns clicks down far less than in pauses; the declicker,
below, takes out more. Levels are RMS over the block, after gain control and
at the microphone's volume, so at 0% the gate stays shut and a boosted quiet
microphone opens it sooner.
"Input sensitivity in Discord calls" sets it from -80 to 0 dBFS
(`input_sensitivity`); behind its handle, the microphone's level goes from
green through orange at -18 dBFS to red at -6, with whether the gate is open.
While the setting shows, Yapfast runs the chosen microphone through a call's
processing on a thread of its own, `yapfast-gate-meter`, and sends nothing; in
a call that's a second copy of the processing. DeepFilterNet takes a block for
speech when its own estimate of the block's SNR is at least 5 dB, RNNoise when
its speech probability is at least 0.6. A 0 dB threshold let typing open the
gate in pauses, and 10 dB missed quiet speech. RNNoise's probability beside
DeepFilterNet's output did no better, so Yapfast doesn't run both. The gate
keeps the last 100 ms while shut and sends them when it opens, so a soft start
such as an "h" isn't cut off; speech goes out that much later until the gate
closes. It stays shut for the first 200 ms after the microphone opens, while
the suppressor learns the noise.

The declicker (`crates/yapfast-core/src/declick.rs`, on unless
`noise::set_declick` turns it off) takes out what DeepFilterNet leaves of mouse
clicks while someone speaks. A click is a burst above 4 kHz whose band above
12 kHz jumps at least 25 dB within half a millisecond and comes within 15 dB of
the whole signal; an "s" or a "t" comes on slower and is weaker up there. It's
found in DeepFilterNet's input, and when it comes out 30 ms later the band
above 4 kHz is held for up to 30 ms to the level it had just before. A
linear-phase filter splits the bands, which puts the output 1 ms later; one
that shifts phase left much of a click between 4 and 10 kHz. Rebuilding the
click's samples from the speech around them (least-squares AR interpolation)
did worse: at 48 kHz an order of 32 spans less than a pitch period.

"Take out your room's echo in Discord calls" (`reduce_room_echo`, off unless
turned on) is for a voice that sounds far away in a bare room: the reverb the
room puts on it, which echo cancellation doesn't touch, since that only takes
out what the speakers play. It's online weighted prediction error (WPE;
Yoshioka and Nakatani 2012, in Caroselli et al.'s 2017 recursive least squares
form), in `crates/yapfast-core/src/dereverb.rs`: in each band up to 5 kHz, the
reverb in a 20 ms frame is predicted from the 200 ms of frames starting 20 ms
before it, and subtracted. The first 20 ms of the room, which the voice needs
to sound whole, stay. It puts the voice 10 ms later. DeepFilterNet takes some
of a room out on its own, more than RNNoise, but its short view of the past
limits it on late reverb. Its post-filter cut tails a little more without
bringing speech closer to the dry speech, and cost quality on noise, so Yapfast
leaves it off. No open model takes a room out at 48 kHz in real time on a
processor; LocalVQE does at 16 kHz.

`scripts/compare-room-echo` runs the noise comparison's speech through
synthetic rooms (a direct sound, then a decaying diffuse tail) and the whole
microphone.

Echo cancellation is WebRTC's AEC3 with its defaults, fed the speakers' mix
as it's written to PulseAudio. It takes no echo out with the speakers 600 ms
or more behind, past what its delay search covers. A room reverberating for
0.65 s or more leaves enough of the echo for the gate to send some of it,
which longer filters in AEC3's configuration could help.

`scripts/compare-noise-suppression [LIMIT]` compares the two, and Off through
the whole microphone, at a strength's limit in dB, on 90 s of speech, two
Spoken Wikipedia readings at -26 dBFS with 1.5-4 s pauses, mixed with four
keyboard recordings (95% of blocks below -32 dBFS), a recorded fan high-passed
as WebRTC does (-42 dBFS), pink hiss (-45 dBFS), and pairs of them; for the
gate, the pairs again 14 dB quieter. PESQ is wideband, against the clean
speech.

DeepFilterNet is 21 ms later than RNNoise. Both lag by half their 20 ms window,
10 ms, DeepFilterNet also looks two blocks ahead, and the declicker adds 1 ms.
With the gate's 100 ms, speech goes out 131 ms late. The speech it doesn't send
is mostly the first second after the microphone opens, while its normalization
and WebRTC's gain control settle, and 10-50 ms at the quiet ends of words.

To hear the processing, run a recording through it: raw mono 16-bit 48 kHz in
`YAPFAST_AUDIO_IN`, with `YAPFAST_AUDIO_OUT` and optionally `YAPFAST_AUDIO_SENT`, then
`cargo test -p yapfast-core processes_a_recording -- --ignored --nocapture`.
`YAPFAST_SUPPRESSOR=rnnoise` or `off` chooses the suppressor,
`YAPFAST_NOISE_LIMIT` limits it in dB, `YAPFAST_DEREVERB=on` takes the room's echo
out and `YAPFAST_LEVELING=off` turns leveling off. `YAPFAST_AUDIO_FAR`, in the same
format and as long, is what the speakers played, for echo cancellation;
`YAPFAST_AUDIO_ECHOED` then gets the microphone with only echo cancelled.

## Push to talk

Settings → Voice & video → Input mode chooses voice activity or push to talk,
as Discord's Input Mode does (`input_mode` in `layout.json`). In push to talk
the microphone sends only while a key is held, and for a release delay after
it comes up: 20 ms unless changed, from 0 to 2 s, Discord's default and its
slider's range (`push_to_talk_release_delay`). As in Discord's client, the
voice gate and its sensitivity don't apply then, so the sensitivity setting
hides, and its meter doesn't run; the rest of the processing still does,
though with the key up DeepFilterNet runs only its encoder (see
[Microphone](#microphone)). With the key up nothing is sent and the track
pauses, as when the gate shuts, so Discord, the call's speaking ring and the
account panel's show the account not speaking. Muting still wins.

Discord has no default key, and neither does Yapfast. "Record keybind" takes
the next key pressed, and Escape stops without changing it; until there's a
key, settings say the microphone sends nothing. Yapfast keeps the key by its
scan code (`push_to_talk_key`), so it's the same key on any keyboard layout,
with its name and, for the portal below, its keysym. One key, without
modifiers; mouse buttons can't be recorded.

`crates/yapfast-core/src/push_to_talk.rs` keeps the mode, whether the key is
down and the delay in atomics. The microphone reads them each 10 ms block,
allocating nothing, and counts the delay in blocks. Matrix calls take the mode
too, though their microphone is LiveKit's device module rather than this
processing: while the key is up their track is disabled, which sends silence
without showing others a mute, so LiveKit's active speakers, which ring the
account, leave it out. `matrix_voice`'s
`persistent_encrypted_voice_and_native_audio` checks it between two processes
on the local stack.

`src/push_to_talk.rs` hears the key in other apps where the desktop lets it:

| Desktop | How | Checked |
| --- | --- | --- |
| Wayland | The xdg-desktop-portal GlobalShortcuts portal: Yapfast binds one shortcut, offering the key as its preferred trigger, and holds it from Activated to Deactivated. The desktop may ask to approve it or give it another key; settings show the shortcut it gave. KDE Plasma, Hyprland and GNOME from 48 have the portal | Against a stand-in portal on a private bus (`the_portal_binds_the_key_and_holds_it`); not on a real desktop |
| X11 | XInput 2's raw key events on the root window, which come whatever window has the focus and leave the key to it | Under Xvfb, with xdotool typing (`x11_hears_the_key_whatever_has_the_focus`) |
| Windows | A low-level keyboard hook (`WH_KEYBOARD_LL`) on a thread of its own, which sees the key's release, unlike `RegisterHotKey`, and passes every key on | Under Wine 10.0, with `SendInput` typing (`the_hook_hears_the_key_go_down_and_up`); not on real Windows, where Windows may keep keys typed into a program running as administrator, such as some games, from the hook, as Discord warns of its own |

Where none of these works, as on a wlroots desktop such as Sway, which has no
GlobalShortcuts, the window's own key events stand in, so the key works while
Yapfast is in front, and settings say so, as Discord's browser client does.
Losing the focus lets the key go. The watcher runs while push to talk is on and
a key is set, in calls or not, and with the window closed to the tray: a thread
blocked on the X server or in the hook's message loop, or the portal's task on
the network runtime. The ignored tests above, and
`push_to_talk_sends_only_while_the_key_is_held` against the fake Discord, which
checks that beeps with the key up send nothing and with it held do, run by
hand; see their comments for what each needs.
