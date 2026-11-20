# The microphone: a design, not yet built

`microphone` is the one capability in the contract with nothing behind it. A
manifest may name it, and reading the manifest leaves it out, so the review
never promises something that doesn't happen
([permissions](permissions.md#the-capabilities)). This page is what would be
built behind it, so that a voice changer or a soundboard can be written as a
plugin without the plugin ever holding the microphone on its own terms.

What is built today, and what the two would use:

- `audio.play` plays a plugin's own sound on the speakers, carried in the call
  or one of its package's assets ([the ABI](abi.md#a-plugins-own-sound-its-voice-and-its-assets)).
- `voice.state` and the `voice_state` and `speaking` events tell a plugin about
  the call it would speak into ([the ABI](abi.md#the-call-and-the-save-dialog)).

## Why it isn't in this pass

The microphone's processing is `discord_audio::Processing`: a 10 ms block of
48 kHz mono goes through WebRTC's echo canceller, the chosen noise suppressor,
WebRTC's gain control, Yapfast's leveller and the gate, on a thread that has to
finish each block inside the 10 ms it lasts and that already runs the suppressor
lightly when it falls behind (`Pace`). Everything a plugin does today crosses a
pipe of JSON frames to a helper process and is answered in tens of
milliseconds, with deadlines of 16 ms for a decoration and 25 ms for a hook. A
processing hook needs a different lane — shared memory, a real-time thread in
the helper, a budget counted in microseconds — and a kill switch that works from
the audio thread without waiting on anything. That is a feature of its own, and
doing it half would put a plugin's stall on the microphone of everyone in the
call.

## The shape

```text
mic ─► echo ─► suppressor ─► gain ─► [plugin stage] ─► leveller ─► gate ─► encoder
                                         │      ▲
                              shared ring │      │ shared ring
                                         ▼      │
                                   helper: real-time lane, one instance
```

**Where it sits.** After the suppressor and the gain control, before the
leveller and the gate, so a plugin is given clean speech at a steady level, and
what it gives back is still levelled and gated like the person's own voice: a
plugin can't hold the gate open, and push-to-talk and mute stay Yapfast's.
Muted, deafened or gated audio never reaches the stage at all.

**How the audio crosses.** Two single-producer single-consumer rings in a shared
memory segment the parent creates and hands the helper at activation, one each
way, each four 20 ms frames (the call's own frame) of 48 kHz mono `f32`. No
JSON, no frame on the pipe, no allocation per block. The parent's audio thread
writes a frame and reads the frame from four frames ago, so the stage adds a
fixed 20 ms of latency, not a variable one.

**How the guest runs.** One export, `yf_audio(frame_ptr, samples) -> i32`, in
place on a buffer in the guest's linear memory, called on a real-time thread of
the helper's own, outside the two lanes. One plugin at a time holds the stage:
two voice changers in a chain is two budgets in a row, and the stage is the
person's choice of one. A module that wants the stage declares `audio` in its
activation and the `microphone` capability, and is compiled with its fuel
metered so a frame can be cut off in the guest rather than waited on.

**The budget.** 2 ms of guest time per 20 ms frame, measured by fuel and by the
clock, whichever runs out first. A frame not back in time is replaced by the
frame as it went in, so a late plugin passes the voice through rather than
dropping it. Three late frames in a second, a trap, or a frame back that isn't
finite (`NaN`, infinity) or is over full scale for more than a few samples
**disengages the stage at once**: the audio thread stops writing to the ring on
the next block — a flag it reads, never a lock it waits on — the plugin takes a
strike, and the person is told in the indicator below. Turning the plugin off,
`--no-plugins`, leaving the call or the helper exiting all disengage it the same
way.

**What a plugin can't do with it.** It hears only what the person would send,
and only while they would send it; it can't record (there is no file it can
write but `files.save`, behind a click, and nothing streams to it but the
frames); it can't send audio while the person is muted; it never sees what
others in the call say. A soundboard that mixes its own asset into the voice
does it inside `yf_audio`, with its asset's samples, under the same budget.

## Consent, and the indicator

- **At install**, `microphone` is shown in the danger tone, the way
  `messages.send.background` is: *Hears and changes your microphone while you
  talk in a call*.
- **Before the first use**, the stage asks once, in the same kind of dialog as a
  network host: *Voice Changer wants to hear and change your microphone in this
  call*, Allow for this call, Always allow, Don't allow. The answer is kept in
  `grants.json` beside the hosts, and revoked on the plugin's page.
- **While it is engaged**, the call panel shows a chip that can't be hidden by
  `ui.modify` — *Voice Changer is changing your voice* — with the plugin's name
  and icon and a button that disengages it for the rest of the call. The chip is
  part of the call state that `docs/plugins/interface.md` already lists as not
  touchable. When the stage disengages itself, the chip says why.

## What it would cost while unused

Nothing: no segment, no thread and no fuel metering exist unless an enabled
plugin declares `audio` and the person is in a call with it allowed. The audio
thread's own cost is one relaxed load of the flag per block.
