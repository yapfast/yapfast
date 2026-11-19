# Voice: wave 3

The dock, the stage and its tiles, call controls, devices, sharing and the
soundboard. Screens in `anomi-voice/src/<area>/`, view-models in
`anomi-vm/src/voice/<area>.rs`. Integration is I18. See
[the index](../tasks.md).

### V01 · The voice dock (Sonnet · M)
- **Owns:** `anomi-voice/src/dock/`, `anomi-vm/src/voice/dock.rs`.
- **After:** F11, F14.
- **Ports:** `VoiceDock`, `DockButton`, `DockNotice` (`ui/voice.slint:393`, `:326`, `:362`); `src/ui/refresh.rs:4592` (`show_voice`, the dock); `src/ui/call_timer.rs`.
- **Shots:** `voice/joined_sidebar_dock.png`, `dmcall/connected_bob_dm.png`.
- **Done when:** "Voice connected" with its timer (ticking only in front), quality bars, camera, share and activity buttons, hang-up, notices. I18.

### V02 · Connection details (Sonnet · M)
- **Owns:** `anomi-voice/src/connection/`, `anomi-vm/src/voice/connection.rs`.
- **After:** F06.
- **Ports:** `CallLinkCard`, `LinkRow`, `LinkTab`, `LinkTouch`, `SignalIcon` (`ui/voice.slint:206`, `:193`, `:171`, `:148`, `:136`); `src/ui/call_link.rs`.
- **Shots:** `voice/connection_popout.png`.
- **Done when:** the ping graph, averages, the secure notice. I18.

### V03 · Tiles (Opus · L)
- **Owns:** `anomi-voice/src/tiles/`, `anomi-vm/src/voice/tiles.rs`, `anomi-voice/tests/areas/tiles/`.
- **After:** F05, F14.
- **Ports:** `StageTile`, `TileChip`, `LivePill`, `ViewerStack`, `StreamPicture`, `StreamPreview` (`ui/voice.slint:751`, `:819`, `:41`, `:60`, `:840`, `:849`); `src/ui/tiles.rs`.
- **Shots:** `voice/stage.png`, `voice/speaking_ring.png`, `voice/with_camera.png`.
- **Done when:** avatar tiles, the speaking ring, mute and deafen marks, camera and stream frames as handles without a copy per frame, the stream preview with "Watch". I18.

### V04 · The participant grid (Opus · L)
- **Owns:** `anomi-voice/src/stage/`, `anomi-vm/src/voice/stage.rs`.
- **After:** V03.
- **Ports:** `ParticipantGrid` (`ui/voice.slint:857`); `src/ui/refresh.rs:4541` (`show_stage`); `src/ui/tiles.rs` (visible tiles and their sizes back to the core); `src/ui/alone.rs`; `src/ui/callbacks.rs:3759`, `:3789`.
- **Shots:** `voice/stage.png`, `voice/watching_stream.png`, `dmcall/connected_discord_dm.png`, `small/voice_stage.png`, `light/voice_stage.png`.
- **Done when:** the grid's own layout at every count and size, a focused tile, a watched stream big with small tiles below, only visible tiles decoded; a ten-person call measured against Slint's memory. Its tiles are V03's `tiles::view` over `tiles::Tile`, and the floating pill V05's `controls::view` (F01's stubs until those pass). I18.

### V05 · Call controls (Sonnet · M)
- **Owns:** `anomi-voice/src/controls.rs`, `anomi-vm/src/voice/controls.rs`.
- **After:** F11, F06.
- **Ports:** `CallControls` (`ui/voice.slint:1121`); `src/ui/callbacks.rs:3073`, `:3591`, `:3749`, `:3087`.
- **Shots:** `voice/stage.png` (the pill), `dmcall/connected_bob_dm.png`.
- **Done when:** the floating pill with mute, deafen, camera, share, activities, soundboard and leave, each with its menu arrow. I18.

### V06 · Picture in picture (Sonnet · S)
- **Owns:** `anomi-voice/src/pip.rs`, `anomi-vm/src/voice/pip.rs`.
- **After:** V03.
- **Ports:** `StreamPip`, `StreamWaiting` (`ui/voice.slint:676`, `:658`).
- **Shots:** `voice/stream_pip_over_text_channel.png`.
- **Done when:** a watched stream floats over a text channel and can be dragged and closed. I18.

### V07 · Joining (Sonnet · S)
- **Owns:** `anomi-voice/src/join.rs`, `anomi-vm/src/voice/join.rs`.
- **After:** F11.
- **Ports:** `JoinVoice`, `CallInvite` (`ui/voice.slint:629`, `:598`); `src/ui/callbacks.rs:3061`, `:3577`, `:3584`.
- **Shots:** `voice/text_chat_of_voice_channel.png`, `light/dm_bob.png` (call ended, join).
- **Done when:** a voice channel's join screen and a DM's call bar. I18.

### V08 · Incoming calls (Sonnet · S)
- **Owns:** `anomi-voice/src/incoming.rs`, `anomi-vm/src/voice/incoming.rs`.
- **After:** F06, F14.
- **Ports:** `IncomingCall`, `SwitchDialog` (`ui/voice.slint:1213`, `:1269`); `src/ui/callbacks.rs:1095`.
- **Shots:** `dmcall/incoming.png`.
- **Done when:** the ring around the avatar pulses only in front, Accept and Decline; switching calls asks. I18.

### V09 · Device menus and the input level (Sonnet · M)
- **Owns:** `anomi-voice/src/devices/`, `anomi-vm/src/voice/devices.rs`.
- **After:** F06, F12.
- **Ports:** `QuickDeviceMenu` (`ui/controls.slint:1610`), `InputLevel` (`ui/voice.slint:1331`); `src/ui/callbacks.rs:3605`, `:3637`, `:3838`-`:3881`.
- **Shots:** `voice/mic_device_menu.png`, `voice/headphones_device_menu.png`.
- **Done when:** as Discord's: "Input Device"/"Output Device" over the device in use, its submenu of devices with radio marks beside the card; for the microphone with Discord linked, "Input Profile" (Voice Isolation, Studio, Custom: what Voice & video's switches make, a preset setting them); the volume with its value in a tip while dragged; Voice Settings; arrows, Right, Left, Enter and Escape. The input level shows in Voice & video, not in the menu. I18.

### V10 · Sharing the screen (Sonnet · M)
- **Owns:** `anomi-voice/src/share/`, `anomi-vm/src/voice/share.rs`.
- **After:** F06, F05.
- **Ports:** `ShareMenu`, `ScreenCard` (`ui/voice.slint:1414`, `:1359`); `src/ui/callbacks.rs:3108`-`:3302`.
- **Shots:** `voice/share_menu.png`.
- **Done when:** sound, resolution, frame rate and encoder choices, the source cards, Go Live. I18.

### V11 · A participant's menu (Sonnet · S)
- **Owns:** `anomi-voice/src/participant_menu.rs`, `anomi-vm/src/voice/participant_menu.rs`.
- **After:** F06, F12.
- **Ports:** `VolumeMenu` (`ui/voice.slint:1594`); `src/ui/menus.rs:594` (move rows).
- **Shots:** `voice/participant_menu.png`.
- **Done when:** volume slider, profile rows, mute checkboxes, invite, move, server mute, disconnect. I18.

### V12 · Soundboard (Sonnet · M)
- **Owns:** `anomi-voice/src/soundboard/`, `anomi-vm/src/voice/soundboard.rs`.
- **After:** F06, F03.
- **Ports:** `Soundboard`, `SoundButton` (`ui/voice.slint:1699`, `:1639`); `src/ui/refresh.rs:5013`, `:5052`; `src/ui/callbacks.rs:3663`-`:3728`.
- **Shots:** `voice/soundboard.png`.
- **Done when:** favourites, server and default sections, preview and play (the soundboard volume is Voice & Video's slider, S03). I18.
