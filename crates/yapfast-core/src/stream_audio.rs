//! Go Live's sound. Sharing: what every other application plays, mixed
//! into 20 ms Opus frames for the stream. Yapfast's own sound is left out, so
//! viewers never hear the call echoed back, as Discord's client and Vesktop
//! do; on Linux so are other Discord clients', whose sound is a call too, which
//! a stream would echo back. Windows can leave out only what Yapfast's own
//! processes play, so another Discord client's call reaches viewers there.
//! [`crate::audio_io::others`] hears the applications.
//! Watching: the streamer's Opus, decoded and played ([`Player`]).

use std::{
    collections::{HashMap, VecDeque},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
        mpsc,
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use yapfast_discord::{
    opus, rtp,
    stream_connection::{
        ReceivedSound, SenderReport, Sound, SoundReceiver, SoundSender, StreamClock,
    },
    voice::{QUIET, Surplus},
};

use crate::audio_io::{Heard, LastingOutput, RATE};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mixes_applications_once_each_has_enough() {
        let mut inputs = HashMap::new();
        let mut out = vec![0.0; FRAME];
        let mut music = Input::default();
        music.push(std::iter::repeat_n(0.25, PRIMED));
        inputs.insert(1, music);
        let mut late = Input::default();
        late.push(std::iter::repeat_n(0.5, FRAME));
        inputs.insert(2, late);
        assert!(mix(&mut inputs, &mut out));
        assert_eq!(out[0], 0.25, "the late one waits to fill up");
        inputs
            .get_mut(&2)
            .unwrap()
            .push(std::iter::repeat_n(0.5, FRAME));
        assert!(mix(&mut inputs, &mut out));
        assert_eq!(out[0], 0.75);
        // The first ran dry: it waits again, and the second goes loud.
        inputs
            .get_mut(&2)
            .unwrap()
            .push(std::iter::repeat_n(0.9, FRAME * 3));
        assert!(mix(&mut inputs, &mut out));
        assert_eq!(out[0], 0.5);
        inputs
            .get_mut(&1)
            .unwrap()
            .push(std::iter::repeat_n(0.9, PRIMED));
        assert!(mix(&mut inputs, &mut out));
        // 1.8 summed: bent towards full scale rather than flat-topped at it.
        assert!(out[0] < 1.0, "{} is inside full scale", out[0]);
        assert!(out[0] > 0.98, "{} of full scale, not pulled down", out[0]);
        inputs.clear();
        assert!(!mix(&mut inputs, &mut out), "silence sends nothing");
    }

    #[test]
    fn loud_applications_stay_inside_full_scale_and_quiet_ones_keep_their_loudness() {
        let mut inputs = HashMap::new();
        let mut out = vec![0.0; FRAME];
        for (source, loudness) in [(1, 0.25), (2, 0.5)] {
            let mut input = Input::default();
            input.push(std::iter::repeat_n(loudness, PRIMED));
            inputs.insert(source, input);
        }
        assert!(mix(&mut inputs, &mut out));
        assert_eq!(out[0], 0.75, "under the knee, mixed exactly as it summed");

        for (source, loudness) in [(1, 0.9), (2, 0.9)] {
            let mut input = Input::default();
            input.push(std::iter::repeat_n(loudness, PRIMED));
            inputs.insert(source, input);
        }
        assert!(mix(&mut inputs, &mut out));
        assert!(
            out.iter().all(|sample| sample.abs() <= 1.0),
            "two applications at 1.8 summed stay inside full scale"
        );
        assert!(out[0] < 1.0, "{} is inside full scale", out[0]);
        assert!(out[0] > 0.98, "{} of full scale, not pulled down", out[0]);

        inputs.clear();
        let mut quiet = Input::default();
        quiet.push(std::iter::repeat_n(0.25, PRIMED));
        inputs.insert(1, quiet);
        assert!(mix(&mut inputs, &mut out));
        assert_eq!(out[0], 0.25, "one quiet application alone is untouched");

        // The one thing the bend costs a single application: full scale it
        // played, and 0.95 of full scale it comes out, so the loudest sound
        // gives up under half a decibel.
        inputs.clear();
        let mut loudest = Input::default();
        loudest.push(std::iter::repeat_n(1.0, PRIMED));
        inputs.insert(1, loudest);
        assert!(mix(&mut inputs, &mut out));
        assert!(out[0] < 1.0, "{} is inside full scale", out[0]);
        assert!(
            out[0] > 0.94,
            "{} of full scale at most a half decibel down",
            out[0]
        );
    }

    #[test]
    fn a_flooding_application_starves_no_other_and_grows_nothing() {
        let mut inputs = HashMap::new();
        let mut flood = Input::default();
        flood.push(std::iter::repeat_n(0.5, BACKLOG));
        inputs.insert(1, flood);
        let mut steady = Input::default();
        steady.push(std::iter::repeat_n(0.25, PRIMED));
        inputs.insert(2, steady);
        let mut out = vec![0.0; FRAME];
        for turn in 0..50 {
            // Each callback from the flooder carries a whole backlog, ten
            // turns of sound; the other delivers a frame a turn, as a system
            // handing over every 20 ms would.
            inputs
                .get_mut(&1)
                .unwrap()
                .push(std::iter::repeat_n(0.5, BACKLOG));
            inputs
                .get_mut(&2)
                .unwrap()
                .push(std::iter::repeat_n(0.25, FRAME));
            assert!(mix(&mut inputs, &mut out));
            assert_eq!(out[0], 0.75, "turn {turn}: the steady one still plays");
            assert_eq!(out[FRAME - 1], 0.75, "turn {turn}: for its whole frame");
            assert!(
                inputs[&1].samples.len() <= BACKLOG,
                "turn {turn}: the flooder's backlog stays bounded"
            );
        }
    }

    #[test]
    fn the_frame_kept_for_mixing_holds_the_new_mix_not_the_old() {
        let mut inputs = HashMap::new();
        let mut loud = Input::default();
        loud.push(std::iter::repeat_n(0.5, PRIMED));
        inputs.insert(1, loud);
        let mut frame = vec![0.0; FRAME];
        assert!(mix(&mut inputs, &mut frame));
        assert_eq!(frame[0], 0.5);
        assert_eq!(frame[FRAME - 1], 0.5);
        // The same buffer, another turn, with an application playing silence.
        inputs.clear();
        let mut quiet = Input::default();
        quiet.push(std::iter::repeat_n(0.0, PRIMED));
        inputs.insert(2, quiet);
        assert!(mix(&mut inputs, &mut frame), "silence still plays");
        assert!(
            frame.iter().all(|&sample| sample == 0.0),
            "the old mix is gone from the buffer it was mixed in"
        );
    }

    #[test]
    fn counts_the_sound_a_backlog_holds_and_drops() {
        let mut music = Input::default();
        assert_eq!(
            music.push(std::iter::repeat_n(0.25, PRIMED)),
            0,
            "room to spare"
        );
        assert_eq!(music.waiting(), (PRIMED / CHANNELS) as u32);
        // Past the bound, the oldest goes, and it's counted.
        let overflow = BACKLOG + 2 * CHANNELS;
        assert_eq!(
            music.push(std::iter::repeat_n(0.25, overflow)),
            (overflow - (BACKLOG - PRIMED)) as u64,
            "what the bound dropped"
        );
        assert_eq!(music.waiting(), (BACKLOG / CHANNELS) as u32, "bounded");
        let counted = Counted::default();
        counted
            .dropped
            .store(RATE as u64 / 1000 * 30 * CHANNELS as u64, Ordering::Relaxed);
        // Held comes from the mixer in samples of one channel, as every
        // application's backlog is measured, and the report counts every
        // channel's.
        counted.hold(RATE / 1000 * 40);
        assert_eq!(
            counted.read(),
            SoundStats {
                held_ms: 40,
                dropped_ms: 30
            }
        );
    }

    #[test]
    fn a_full_backlog_waits_two_hundred_milliseconds() {
        let mut input = Input::default();
        input.push(counted(BACKLOG));
        let stats = Counted::default();
        stats.hold(input.waiting());
        assert_eq!(
            stats.read().held_ms,
            200,
            "the delay a viewer would hear at the bound"
        );
    }

    /// Plays a quiet tone with paplay and hears it through the monitor:
    /// `YAPFAST_TONE=/path/to/tone.wav cargo test -- --ignored captures_`.
    #[cfg(target_os = "linux")]
    #[test]
    #[ignore]
    fn captures_what_another_application_plays() {
        let tone = std::env::var("YAPFAST_TONE").expect("YAPFAST_TONE names a WAV file");
        let inputs = Inputs::default();
        let stop = Arc::new(AtomicBool::new(false));
        let capture = listen(inputs.clone(), Arc::default(), stop.clone()).unwrap();
        let mut player = std::process::Command::new("paplay")
            .args(["--volume=100", &tone])
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let loudest = loop {
            let loudest = inputs
                .lock()
                .unwrap()
                .values()
                .flat_map(|input| input.samples.iter().copied())
                .fold(0.0f32, |loudest, sample| loudest.max(sample.abs()));
            if loudest > 0.0 || Instant::now() > deadline {
                break loudest;
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        let _ = player.wait();
        stop.store(true, Ordering::Relaxed);
        capture.join().unwrap();
        assert!(loudest > 0.0, "heard nothing");
    }

    /// Plays a quiet tone from this process, as Yapfast plays a call, and hears
    /// none of it.
    #[cfg(target_os = "linux")]
    #[test]
    #[ignore]
    fn leaves_out_yapfasts_own_sound() {
        let inputs = Inputs::default();
        let stop = Arc::new(AtomicBool::new(false));
        let capture = listen(inputs.clone(), Arc::default(), stop.clone()).unwrap();
        let mut player =
            crate::audio_io::Output::<f32>::open(None, "Discord voice", RATE, 2, None).unwrap();
        // A second of a 440 Hz tone at about -56 dB.
        let tone: Vec<f32> = (0..RATE)
            .flat_map(|at| {
                let level =
                    0.0016 * (at as f32 * 440.0 * std::f32::consts::TAU / RATE as f32).sin();
                [level, level]
            })
            .collect();
        player.write(&tone).unwrap();
        player.drain().unwrap();
        let ours = inputs
            .lock()
            .unwrap()
            .values()
            .filter(|input| input.process == Some(std::process::id()))
            .count();
        let others = inputs.lock().unwrap().len();
        stop.store(true, Ordering::Relaxed);
        capture.join().unwrap();
        assert_eq!(ours, 0, "Yapfast's own playback was captured");
        eprintln!("monitored {others} other applications' streams");
    }

    #[test]
    fn leaves_out_other_discord_clients() {
        for client in [
            "Yapfast",
            "Discord",
            "DiscordCanary",
            ".Discord-wrapped",
            "vesktop",
            "Equibop",
            "WebCord",
            "legcord",
        ] {
            assert!(is_discord_client(client), "{client}");
        }
        for other in [
            "Firefox",
            "mpv",
            "electron",
            "Spotify",
            "steam",
            "talkative",
        ] {
            assert!(!is_discord_client(other), "{other}");
        }
    }

    /// Plays 400 ms of Opus silence on the default speakers, then ends with
    /// the sound.
    #[test]
    fn plays_a_stream_at_its_volume() {
        let volume = Volume::new(0.5);
        let pcm = [1000i16, -1000, 30_000];
        let mut scaled = pcm;
        scale(&mut scaled, volume.get());
        assert_eq!(scaled, [500, -500, 15_000]);
        volume.set(2.0);
        let mut scaled = pcm;
        scale(&mut scaled, volume.get());
        assert_eq!(
            scaled,
            [2000, -2000, i16::MAX],
            "louder clips at full scale"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    #[ignore]
    fn plays_a_streams_sound_until_it_ends() {
        let (sounds, heard) = std::sync::mpsc::sync_channel(32);
        let now = Instant::now();
        for sequence in 0..20u16 {
            let timestamp = u32::from(sequence) * SAMPLES as u32;
            let opus = vec![0xf8, 0xff, 0xfe];
            sounds
                .send(sound(sequence, timestamp, 7, opus, now))
                .unwrap();
        }
        drop(sounds);
        let started = Instant::now();
        let mut player = Player::start(
            SoundReceiver::new(heard, Arc::new(StreamClock::default())),
            None,
            Arc::new(AtomicBool::new(true)),
            Arc::new(Volume::new(1.0)),
        )
        .unwrap();
        player.thread.take().unwrap().join().unwrap();
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    /// A sender report whose clock read `rtp` at `seconds` of NTP time.
    fn reported(seconds: u32, rtp: u32) -> SenderReport {
        SenderReport {
            ntp: seconds << 16,
            rtp,
            heard: Instant::now(),
        }
    }

    #[test]
    fn sender_reports_say_how_far_the_sound_lags_the_picture() {
        // The streamer's sound read 4_800_000 at 100 s and its picture
        // 9_000_000 at that same instant. A second later the sound is at
        // 101 s; half a second after that the newest picture is at 101.5 s,
        // so the sound plays half a second behind the picture.
        let sound = (reported(100, 4_800_000), 4_800_000 + 48_000);
        let picture = (reported(100, 9_000_000), 9_000_000 + 90_000 + 45_000);
        let skew = behind(Some(sound), Some(picture)).unwrap();
        assert!((skew - 0.5).abs() < 1e-9, "{skew}");
        // The plan's 80 ms as a sanity bound: a picture 80 ms newer than the
        // sound on the same wall clock reads as 0.08 s behind.
        let picture = (reported(100, 9_000_000), 9_000_000 + 90_000 + 7_200);
        let skew = behind(Some(sound), Some(picture)).unwrap();
        assert!((skew - 0.08).abs() < 1e-9, "{skew}");
    }

    #[test]
    fn sender_reports_hold_across_both_wraps() {
        // The sound's RTP timestamp turns over between the report and the
        // playhead: from 24_000 samples before the wrap to 24_000 samples
        // past it is a second, not a jump of four billion ticks.
        let sound = (
            reported(100, u32::MAX - 23_999),
            (u32::MAX - 23_999).wrapping_add(48_000),
        );
        let picture = (reported(100, 0), 90_000 + 7_200);
        let skew = behind(Some(sound), Some(picture)).unwrap();
        assert!((skew - 0.08).abs() < 1e-9, "{skew}");
        // The NTP time kept is the middle 32 bits, whose seconds turn over
        // every 18 hours: a sound at 65_535 s and a picture whose report sits
        // just past that turn are two seconds apart, not 18 hours.
        let sound = (reported(65_535, 4_800_000), 4_800_000);
        let picture = (reported(0, 0), 90_000);
        let skew = behind(Some(sound), Some(picture)).unwrap();
        assert!((skew - 2.0).abs() < 1e-9, "{skew}");
    }

    #[test]
    fn missing_sender_reports_leave_the_skew_unknown() {
        let sound = (reported(100, 4_800_000), 4_800_000);
        let picture = (reported(100, 9_000_000), 9_000_000);
        assert_eq!(behind(None, Some(picture)), None, "no report for the sound");
        assert_eq!(behind(Some(sound), None), None, "none for the picture");
        assert_eq!(behind(None, None), None);
    }

    /// The player's turn that plays a frame that came, from the start of what
    /// it decoded.
    const PLAYING: Turn = Turn::Play {
        at: 0,
        samples: SAMPLES,
        of: Played::Frame,
    };
    /// The player's turn that makes up for a packet that didn't come.
    const CONCEALED: Turn = Turn::Play {
        at: 0,
        samples: SAMPLES,
        of: Played::Concealed,
    };
    /// The player's turn that plays silence.
    const SILENT: Turn = Turn::Play {
        at: 0,
        samples: SAMPLES,
        of: Played::Silence,
    };

    /// An encoder for a player's sound, as a stream's is sent: music mode.
    fn music() -> opus::Encoder {
        opus::Encoder::new(CHANNELS as u16, opus::Application::Audio, BITRATE).unwrap()
    }

    /// One 20 ms frame of a tone, `at` frames in, encoded.
    fn tone(encoder: &mut opus::Encoder, at: usize) -> Vec<u8> {
        let pcm: Vec<f32> = (0..FRAME)
            .map(|sample| {
                let when = (at * FRAME + sample) as f32;
                (when * 330.0 * std::f32::consts::TAU / RATE as f32).sin() * 0.5
            })
            .collect();
        let mut opus = vec![0; 4000];
        let length = encoder.encode_float(&pcm, &mut opus).unwrap();
        opus.truncate(length);
        opus
    }

    /// Opus frames of a tone, 20 ms each, in order.
    fn tones(encoder: &mut opus::Encoder, count: usize) -> Vec<Vec<u8>> {
        (0..count).map(|at| tone(encoder, at)).collect()
    }

    /// One 60 ms frame of a tone, `at` frames in, encoded: what a music bot
    /// passing a source's Opus through sends.
    fn long(encoder: &mut opus::Encoder, at: usize) -> Vec<u8> {
        let pcm: Vec<f32> = (0..FRAME * 3)
            .map(|sample| {
                let when = (at * FRAME * 3 + sample) as f32;
                (when * 330.0 * std::f32::consts::TAU / RATE as f32).sin() * 0.25
            })
            .collect();
        let mut opus = vec![0; 4000];
        let length = encoder.encode_float(&pcm, &mut opus).unwrap();
        opus.truncate(length);
        opus
    }

    /// A frame as the connection hands it to a player.
    fn sound(
        sequence: u16,
        timestamp: u32,
        ssrc: u32,
        opus: Vec<u8>,
        arrived: Instant,
    ) -> ReceivedSound {
        let packet = rtp::Packet {
            payload_type: rtp::OPUS,
            marker: false,
            sequence,
            timestamp,
            ssrc,
            payload: &[],
            // A test's packet says nothing about which way the camera was.
            orientation: None,
        };
        ReceivedSound::new(&packet, opus, arrived)
    }

    /// Pushes `count` frames of a tone, from `sequence` on, in order.
    fn push_tones(playout: &mut Playout, opus: &[Vec<u8>], first: u16, ssrc: u32, now: Instant) {
        for (at, frame) in opus.iter().enumerate() {
            let sequence = first.wrapping_add(at as u16);
            let timestamp = u32::from(sequence) * SAMPLES as u32;
            playout.push(sound(sequence, timestamp, ssrc, frame.clone(), now), now);
        }
    }

    /// Speakers a test plays into: every turn kept, and taken as long to play
    /// as a device would, so the player runs on a clock of its own.
    #[derive(Default)]
    struct Recorded {
        /// Every turn written, in order.
        chunks: Arc<Mutex<Vec<Vec<i16>>>>,
        /// When the first one went out, which is when sound is heard.
        first: Arc<Mutex<Option<Instant>>>,
    }

    impl Recorded {
        fn lock(chunks: &Mutex<Vec<Vec<i16>>>) -> std::sync::MutexGuard<'_, Vec<Vec<i16>>> {
            chunks
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
        }

        fn written(&self) -> usize {
            Self::lock(&self.chunks).len()
        }
    }

    impl Speakers for Recorded {
        fn write(&mut self, samples: &[i16]) {
            self.first
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .get_or_insert_with(Instant::now);
            Self::lock(&self.chunks).push(samples.to_vec());
            // The device paces the player through its buffer, so a turn here
            // takes as long as its sound would.
            std::thread::sleep(TURN);
        }
    }

    #[test]
    fn sound_that_comes_while_the_player_waits_is_played_and_not_thrown_away() {
        let sent = START + 5;
        let (sending, arriving) = mpsc::sync_channel(sent);
        let mut encoder = music();
        // None is waiting when the player starts, and they come one at a time
        // after it begins to wait: every one arrives during a blocking wait,
        // which is where a packet used to be taken off the channel and
        // dropped. Three start it; five more play after them.
        let sender = std::thread::spawn(move || {
            for at in 0..sent {
                std::thread::sleep(Duration::from_millis(15));
                let packet = tone(&mut encoder, at);
                let timestamp = (at * SAMPLES) as u32;
                let sound = sound(at as u16, timestamp, 9, packet, Instant::now());
                if sending.send(sound).is_err() {
                    break;
                }
            }
        });
        let mut playout = Playout::new().unwrap();
        let mut output = Recorded::default();
        let hearing = AtomicBool::new(true);
        let volume = Volume::new(1.0);
        let stop = AtomicBool::new(false);
        let receiver = SoundReceiver::new(arriving, Arc::new(StreamClock::default()));
        let clock = receiver.clock();
        let started = Instant::now();
        let played = std::sync::atomic::AtomicU64::new(u64::MAX);
        let queued = std::sync::atomic::AtomicU64::new(u64::MAX);
        turns(
            &receiver,
            &mut playout,
            &mut output,
            &hearing,
            &volume,
            &stop,
            &clock,
            &played,
            &queued,
        );
        sender.join().unwrap();
        assert_eq!(
            playout.counts.came, sent as u64,
            "every packet that came was queued"
        );
        assert_eq!(
            playout.counts.played, sent as u64,
            "and every one of them reached the playhead"
        );
        assert_eq!(playout.counts.late, 0, "none came after its slot played");
        assert_eq!(playout.counts.trimmed, 0, "and none waited too long");
        assert_eq!(playout.counts.lost, 0, "nothing was skipped by the sender");
        assert!(
            output.written() >= sent,
            "{} turns reached the speakers",
            output.written()
        );
        let first = *output
            .first
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let first = first.expect("sound played");
        assert!(
            first.saturating_duration_since(started) < Duration::from_millis(400),
            "first sound {:?} in",
            first.saturating_duration_since(started)
        );
    }

    #[test]
    fn plays_frames_in_order_and_drops_what_comes_twice() {
        let mut encoder = music();
        let opus = tones(&mut encoder, 4);
        let mut playout = Playout::new().unwrap();
        let now = Instant::now();
        push_tones(&mut playout, &opus, 0, 7, now);
        assert_eq!(playout.counts.came, 4, "all four came in order");
        assert_eq!(playout.next(now), PLAYING, "three waiting start it");
        // The packet it just played, again: its slot has gone.
        assert_eq!(
            playout.push(sound(0, 0, 7, opus[0].clone(), now), now),
            Landing::Late
        );
        // And one still waiting, again: it can only play once.
        let timestamp = 2 * SAMPLES as u32;
        assert_eq!(
            playout.push(sound(2, timestamp, 7, opus[2].clone(), now), now),
            Landing::Duplicate
        );
        for at in 1..4 {
            assert_eq!(playout.next(now), PLAYING, "frame {at}");
        }
        assert!(playout.waiting.is_empty());
    }

    #[test]
    fn holds_a_gap_for_a_late_packet_then_conceals_it() {
        let mut encoder = music();
        let opus = tones(&mut encoder, 5);
        let mut playout = Playout::new().unwrap();
        let now = Instant::now();
        push_tones(&mut playout, &opus[..2], 0, 7, now);
        // Two are short of the three that start it, so they play because the
        // first waited out PRIME.
        let primed = now + PRIME;
        assert_eq!(playout.next(primed), PLAYING);
        assert_eq!(playout.next(primed), PLAYING);
        // Packet 2 never came, and 3 and 4 did.
        push_tones(&mut playout, &opus[3..], 3, 7, primed);
        // Its slot waits 60 ms for it, then makes it up.
        assert_eq!(playout.next(primed), Turn::Wait(LATE));
        assert_eq!(playout.next(primed + LATE / 2), Turn::Wait(LATE / 2));
        assert_eq!(playout.next(primed + LATE), CONCEALED);
        // Packet 3, which carried the concealment, still plays as itself.
        assert_eq!(playout.next(primed + LATE), PLAYING);
        assert_eq!(playout.waiting.len(), 1, "packet 4 waits its turn");
    }

    #[test]
    fn an_out_of_order_packet_fills_the_gap_it_belongs_in() {
        let mut encoder = music();
        let opus = tones(&mut encoder, 5);
        let mut playout = Playout::new().unwrap();
        let now = Instant::now();
        push_tones(&mut playout, &opus[..3], 0, 7, now);
        assert_eq!(playout.next(now), PLAYING);
        // 4 comes before 3 does.
        let landing = playout.push(sound(4, 4 * SAMPLES as u32, 7, opus[4].clone(), now), now);
        assert_eq!(landing, Landing::InOrder);
        let landing = playout.push(sound(3, 3 * SAMPLES as u32, 7, opus[3].clone(), now), now);
        assert_eq!(landing, Landing::Reordered, "it goes in front of 4");
        for at in 1..5 {
            assert_eq!(playout.next(now), PLAYING, "frame {at}");
        }
        assert!(playout.waiting.is_empty());
    }

    #[test]
    fn a_packet_that_comes_before_its_slot_is_made_up_plays() {
        let mut encoder = music();
        let opus = tones(&mut encoder, 5);
        let mut playout = Playout::new().unwrap();
        let now = Instant::now();
        push_tones(&mut playout, &opus[..2], 0, 7, now);
        // Two are short of the three that start it, so they play because the
        // first waited out PRIME.
        let primed = now + PRIME;
        assert_eq!(playout.next(primed), PLAYING);
        assert_eq!(playout.next(primed), PLAYING);
        // 2 hasn't come, and 3 and 4 have.
        push_tones(&mut playout, &opus[3..], 3, 7, primed);
        assert_eq!(
            playout.next(primed),
            Turn::Wait(LATE),
            "its slot waits for it"
        );
        // It comes within that wait, so it plays, and 3 and 4 after it.
        let later = primed + LATE / 2;
        let landing = playout.push(
            sound(2, 2 * SAMPLES as u32, 7, opus[2].clone(), later),
            later,
        );
        assert_eq!(landing, Landing::Reordered);
        for at in 2..5 {
            assert_eq!(playout.next(primed + LATE), PLAYING, "frame {at}");
        }
        assert!(playout.waiting.is_empty());
    }

    #[test]
    fn the_packet_after_a_gap_still_plays_after_carrying_its_concealment() {
        // Voice mode is what turns Opus's forward error correction on, which
        // a stream's own encoder needn't send.
        let mut encoder =
            opus::Encoder::new(CHANNELS as u16, opus::Application::Voice, BITRATE).unwrap();
        let opus = tones(&mut encoder, 5);
        let mut playout = Playout::new().unwrap();
        let now = Instant::now();
        push_tones(&mut playout, &opus[..2], 0, 7, now);
        // Two are short of the three that start it, so they play because the
        // first waited out PRIME.
        let primed = now + PRIME;
        assert_eq!(playout.next(primed), PLAYING);
        assert_eq!(playout.next(primed), PLAYING);
        // Packet 2 is lost, and 3 and 4 came.
        push_tones(&mut playout, &opus[3..], 3, 7, primed);
        assert_eq!(playout.next(primed), Turn::Wait(LATE));
        assert_eq!(playout.next(primed + LATE), CONCEALED);
        assert_eq!(
            playout.waiting.front().map(|frame| frame.sequence),
            Some(3),
            "the packet that carried it isn't counted as played"
        );
        assert_eq!(playout.next(primed + LATE), PLAYING, "packet 3, as itself");
        assert_eq!(playout.waiting.front().map(|frame| frame.sequence), Some(4));
    }

    #[test]
    fn an_undecodable_packet_still_gets_its_own_concealed_turn() {
        let mut encoder = music();
        let opus = tones(&mut encoder, 5);
        let mut playout = Playout::new().unwrap();
        let now = Instant::now();
        push_tones(&mut playout, &opus[..2], 0, 7, now);
        // Two are short of the three that start it, so they play because the
        // first waited out PRIME.
        let primed = now + PRIME;
        assert_eq!(playout.next(primed), PLAYING);
        assert_eq!(playout.next(primed), PLAYING);
        // Packet 2 is lost, and packet 3 is one libopus can't read: the try
        // at its forward error correction fails, so 2's slot is concealed
        // and 3 is left to play, and fail, on its own turn.
        let garbage = vec![0xff];
        playout.push(sound(3, 3 * SAMPLES as u32, 7, garbage, primed), primed);
        assert_eq!(playout.next(primed), Turn::Wait(LATE));
        assert_eq!(playout.next(primed + LATE), CONCEALED);
        assert_eq!(
            playout.waiting.front().map(|frame| frame.sequence),
            Some(3),
            "a failed decode is not swallowed"
        );
        assert!(
            matches!(playout.next(primed + LATE), Turn::Play { .. }),
            "it plays, and fails, on its own turn"
        );
        assert!(playout.waiting.is_empty());
        assert_eq!(playout.next, Some(4));
        // Sound after it plays as usual.
        let landing = playout.push(
            sound(4, 4 * SAMPLES as u32, 7, opus[4].clone(), primed),
            primed,
        );
        assert_eq!(landing, Landing::InOrder);
        assert_eq!(playout.next(primed + LATE), PLAYING);
    }

    #[test]
    fn wraps_sequences_and_timestamps_without_starting_again() {
        let mut encoder = music();
        let opus = tones(&mut encoder, 4);
        let mut playout = Playout::new().unwrap();
        let now = Instant::now();
        // The sequences wrap the u16 as the timestamps wrap the u32.
        let sequences = [65_534u16, 65_535, 0, 1];
        let mut timestamp = u32::MAX - 2 * SAMPLES as u32;
        for (at, sequence) in sequences.iter().enumerate() {
            let frame = sound(*sequence, timestamp, 7, opus[at].clone(), now);
            assert_eq!(playout.push(frame, now), Landing::InOrder, "{sequence}");
            timestamp = timestamp.wrapping_add(SAMPLES as u32);
        }
        for at in 0..4 {
            assert_eq!(playout.next(now), PLAYING, "frame {at}");
        }
        assert!(playout.waiting.is_empty());
        assert_eq!(playout.counts.restarts, 0, "a wrap is not a restart");
    }

    #[test]
    fn starts_again_on_another_ssrc_or_a_timestamp_jump() {
        let mut encoder = music();
        let opus = tones(&mut encoder, 8);
        let mut playout = Playout::new().unwrap();
        let now = Instant::now();
        push_tones(&mut playout, &opus[..3], 0, 7, now);
        for _ in 0..3 {
            assert_eq!(playout.next(now), PLAYING);
        }
        // Another SSRC is another stream: what waited means nothing to it.
        let landing = playout.push(sound(0, 0, 8, opus[3].clone(), now), now);
        assert_eq!(landing, Landing::Restart);
        assert_eq!(playout.next, None, "it waits to start again");
        assert_eq!(playout.waiting.len(), 1);
        // A sender that began again may keep its SSRC: a timestamp a second
        // away is what says so.
        push_tones(&mut playout, &opus[4..6], 1, 8, now);
        for _ in 0..3 {
            assert_eq!(playout.next(now), PLAYING);
        }
        let jump = 3 * SAMPLES as u32 + 48_000 + SAMPLES as u32;
        let landing = playout.push(sound(3, jump, 8, opus[6].clone(), now), now);
        assert_eq!(landing, Landing::Restart);
        assert_eq!(playout.next, None, "it waits to start again");
    }

    #[test]
    fn goes_quiet_after_a_long_gap_and_starts_again_when_sound_comes() {
        let mut encoder = music();
        let opus = tones(&mut encoder, 9);
        let mut playout = Playout::new().unwrap();
        let now = Instant::now();
        push_tones(&mut playout, &opus[..3], 0, 7, now);
        for _ in 0..3 {
            assert_eq!(playout.next(now), PLAYING);
        }
        // Nothing more comes: its slot is made up for a while, then the
        // sender has stopped, and silence plays rather than sound that isn't
        // there.
        for missing in 1..GONE {
            assert_eq!(playout.next(now), CONCEALED, "{missing} packets missing");
        }
        assert_eq!(playout.next(now), SILENT);
        assert_eq!(playout.next, None, "it waits to start again");
        assert_eq!(playout.next(now), Turn::Wait(PRIME), "no sound to wait for");
        // The sender comes back where the silence left the stream: no jump.
        push_tones(&mut playout, &opus[3..6], 8, 7, now);
        assert_eq!(playout.next(now), PLAYING, "three frames start it again");
        assert_eq!(playout.counts.restarts, 0, "silence isn't a restart");
    }

    #[test]
    fn drops_a_frame_that_waited_too_long_and_starts_again() {
        let mut encoder = music();
        let opus = tones(&mut encoder, 4);
        let mut playout = Playout::new().unwrap();
        let now = Instant::now();
        push_tones(&mut playout, &opus[..3], 0, 7, now);
        for _ in 0..3 {
            assert_eq!(playout.next(now), PLAYING);
        }
        // A frame that waited in the queue through a stall of 300 ms.
        let stalled = now.checked_sub(STALE + Duration::from_millis(20)).unwrap();
        let landing = playout.push(
            sound(3, 3 * SAMPLES as u32, 7, opus[3].clone(), stalled),
            now,
        );
        assert_eq!(landing, Landing::Late);
        assert_eq!(playout.next, None, "a stalled player starts again");
        assert!(playout.waiting.is_empty(), "nothing old waits to replay");
    }

    #[test]
    fn primes_on_three_frames_or_a_hundred_ms_of_one() {
        let mut encoder = music();
        let opus = tones(&mut encoder, 3);
        let mut playout = Playout::new().unwrap();
        let now = Instant::now();
        assert_eq!(
            playout.push(sound(0, 0, 7, opus[0].clone(), now), now),
            Landing::InOrder
        );
        assert_eq!(
            playout.next(now),
            Turn::Wait(PRIME),
            "one frame isn't enough"
        );
        push_tones(&mut playout, &opus[1..3], 1, 7, now);
        assert_eq!(playout.next(now), PLAYING, "three are");
        // One that came alone starts once it has waited 100 ms.
        playout.reset();
        assert_eq!(
            playout.push(sound(5, 0, 7, opus[0].clone(), now), now),
            Landing::InOrder
        );
        assert_eq!(playout.next(now), Turn::Wait(PRIME));
        assert_eq!(playout.next(now + PRIME / 2), Turn::Wait(PRIME / 2));
        assert_eq!(playout.next(now + PRIME), PLAYING);
    }

    #[test]
    fn keeps_at_most_most_frames_and_plays_the_newest() {
        let mut encoder = music();
        let opus = tones(&mut encoder, MOST + 5);
        let mut playout = Playout::new().unwrap();
        let now = Instant::now();
        push_tones(&mut playout, &opus, 0, 7, now);
        assert_eq!(playout.waiting.len(), MOST, "the oldest went");
        assert_eq!(playout.waiting.front().map(|frame| frame.sequence), Some(5));
        assert_eq!(playout.next(now), PLAYING);
        assert_eq!(
            playout.waiting.front().map(|frame| frame.sequence),
            Some(6),
            "and what plays is the newest ten"
        );
    }

    #[test]
    fn plays_a_sixty_ms_packet_over_three_turns() {
        let mut encoder = music();
        // A music bot passing a source's Opus through can send 60 ms packets.
        let pcm: Vec<f32> = (0..FRAME * 3)
            .map(|sample| (sample as f32 * 0.01).sin() * 0.25)
            .collect();
        let opus: Vec<Vec<u8>> = (0..START)
            .map(|_| {
                let mut packet = vec![0; 4000];
                let length = encoder.encode_float(&pcm, &mut packet).unwrap();
                packet.truncate(length);
                packet
            })
            .collect();
        let mut playout = Playout::new().unwrap();
        let now = Instant::now();
        for (at, frame) in opus.into_iter().enumerate() {
            let timestamp = (at * SAMPLES * 3) as u32;
            playout.push(sound(at as u16, timestamp, 7, frame, now), now);
        }
        for turn in 0..START * 3 {
            assert_eq!(
                playout.next(now),
                Turn::Play {
                    at: (turn % 3) * FRAME,
                    samples: SAMPLES,
                    of: Played::Frame,
                },
                "turn {turn}: each 20 ms plays from where the one before left off"
            );
            assert_eq!(
                playout.waiting.len(),
                START - 1 - turn / 3,
                "a packet plays over three turns; turn {turn}"
            );
        }
    }

    #[test]
    fn a_sixty_ms_packet_plays_each_of_its_parts_once_at_any_volume() {
        let mut encoder = music();
        // Three 20 ms stretches that can't be mistaken for each other, so a
        // turn that played the start of the packet again says so in the PCM
        // itself and not only in the count of turns.
        let pcm: Vec<f32> = (0..FRAME * 3)
            .map(|sample| match sample / FRAME {
                0 => 0.5,
                1 => -0.25,
                _ => 0.125,
            })
            .collect();
        let packet = {
            let mut opus = vec![0; 4000];
            let length = encoder.encode_float(&pcm, &mut opus).unwrap();
            opus.truncate(length);
            opus
        };
        let mut playout = Playout::new().unwrap();
        let now = Instant::now();
        for at in 0..START {
            let timestamp = (at * SAMPLES * 3) as u32;
            playout.push(sound(at as u16, timestamp, 7, packet.clone(), now), now);
        }
        // What the packet holds, decoded in the same order the player decodes
        // it, so that every sample emitted can be compared with the one it
        // should have played: a whole packet is three turns of three different
        // 20 ms stretches, each scaled once.
        let mut reference = opus::Decoder::new(CHANNELS as u16).unwrap();
        let mut expected = vec![0; FRAME * 3 * START];
        for at in 0..START {
            let decoded = reference
                .decode(&packet, &mut expected[at * FRAME * 3..], false)
                .unwrap();
            assert_eq!(decoded, SAMPLES * 3, "a packet of three turns");
        }
        let volume = 0.5;
        let mut played = Vec::new();
        for turn in 0..START * 3 {
            let Turn::Play { at, samples, of } = playout.next(now) else {
                panic!("turn {turn}: a primed playout plays");
            };
            assert_eq!(
                (at, samples, of),
                ((turn % 3) * FRAME, SAMPLES, Played::Frame)
            );
            let mut chunk = playout.pcm[at..at + samples * CHANNELS].to_vec();
            scale(&mut chunk, volume);
            played.push(chunk);
        }
        for (turn, chunk) in played.iter().enumerate() {
            let mut once = expected[turn * FRAME..][..FRAME].to_vec();
            scale(&mut once, volume);
            assert_eq!(chunk, &once, "turn {turn}: exactly this 20 ms, scaled once");
        }
        // The three turns of one packet are three different stretches: not
        // its first 20 ms three times.
        assert_ne!(played[0], played[1], "the second turn is not the first");
        assert_ne!(played[1], played[2], "and the third is not the second");
    }

    #[test]
    fn a_sixty_ms_packet_stream_is_held_by_time_not_frame_count() {
        let mut encoder = music();
        let opus: Vec<Vec<u8>> = (0..MOST).map(|at| long(&mut encoder, at)).collect();
        let mut playout = Playout::new().unwrap();
        let now = Instant::now();
        // Ten 60 ms frames are 600 ms of sound. A queue that only counted
        // frames would hold all of them, three times the delay a 20 ms
        // stream is held to, and play it all a second late.
        for (at, frame) in opus.into_iter().enumerate() {
            let timestamp = (at * SAMPLES * 3) as u32;
            playout.push(sound(at as u16, timestamp, 7, frame, now), now);
            assert!(
                playout.held() <= MOST_SAMPLES,
                "{} samples held after {} frames",
                playout.held(),
                at + 1
            );
            assert!(playout.waiting.len() <= MOST, "room for them all");
        }
        // Three of the ten are the little under 200 ms that's left, all at
        // the newest end: the oldest went to make room.
        assert_eq!(playout.waiting.len(), 3);
        assert_eq!(playout.waiting.front().map(|frame| frame.sequence), Some(7));
        assert!(
            playout.counts.trimmed > 0,
            "the time bound did the dropping"
        );
    }

    #[test]
    fn a_bound_drops_the_sound_that_came_early_not_the_playhead() {
        let mut encoder = music();
        let opus = tones(&mut encoder, MOST + 6);
        let mut playout = Playout::new().unwrap();
        let now = Instant::now();
        push_tones(&mut playout, &opus[..MOST], 0, 7, now);
        assert_eq!(playout.next(now), PLAYING, "ten frames start it");
        assert_eq!(playout.next, Some(1), "and the playhead is at the second");
        // Six more come before its turn: the ones furthest ahead are the
        // sound that came too early to play in time, so they're the ones to
        // go, and the playhead's own packet is never one of them.
        push_tones(&mut playout, &opus[MOST..], MOST as u16, 7, now);
        assert_eq!(playout.next, Some(1), "the playhead didn't skip");
        assert_eq!(playout.waiting.front().map(|frame| frame.sequence), Some(1));
        assert_eq!(
            playout.waiting.back().map(|frame| frame.sequence),
            Some(MOST as u16),
            "the newest went, and what's left is still in order"
        );
        assert!(playout.held() <= MOST_SAMPLES);
        assert!(playout.waiting.len() <= MOST);
        assert_eq!(playout.counts.trimmed, 5, "the five dropped are counted");
        // What's left plays through without a gap or a wait: nothing was
        // taken out of the middle of it.
        for sequence in 1..=MOST {
            assert_eq!(playout.next(now), PLAYING, "frame {sequence}");
        }
        assert!(playout.waiting.is_empty());
    }

    #[test]
    fn a_fast_sender_has_its_delay_trimmed_rather_than_raised() {
        let mut encoder = music();
        let opus = tones(&mut encoder, MOST * 3 * 3);
        let mut playout = Playout::new().unwrap();
        let now = Instant::now();
        // Three frames arrive a turn where one plays, as a sender whose clock
        // runs at three times the playout's: the queue, and the delay with it,
        // would grow two turns a turn without a bound.
        for (at, frame) in opus.into_iter().enumerate() {
            let sequence = at as u16;
            let timestamp = u32::from(sequence) * SAMPLES as u32;
            playout.push(sound(sequence, timestamp, 7, frame, now), now);
            assert!(
                playout.held() <= MOST_SAMPLES,
                "{} samples held after {} frames",
                playout.held(),
                at + 1
            );
            assert!(
                playout.waiting.len() <= MOST,
                "{} frames after {}",
                playout.waiting.len(),
                at + 1
            );
            if at % 3 == 2 {
                let turn = playout.next(now);
                assert!(matches!(turn, Turn::Play { .. }), "{at}: {turn:?}");
            }
        }
        assert!(playout.counts.trimmed > 0, "the bound trimmed the surplus");
        assert_eq!(
            playout.counts.lost, 0,
            "every number came, so none of this is loss"
        );
        // What's left plays out and empties, so the delay it was held to
        // doesn't stay raised once the sender stops outrunning the player.
        for _ in 0..MOST + usize::from(GONE) {
            playout.next(now);
        }
        assert!(playout.waiting.is_empty(), "the queue drained");
        assert_eq!(playout.held(), 0, "and holds nothing");
    }

    #[test]
    fn a_fast_sender_is_trimmed_and_skipped_but_not_lost() {
        let mut encoder = music();
        let turns = 200;
        let opus = tones(&mut encoder, turns * 20 / 19 + 2);
        let mut playout = Playout::new().unwrap();
        let start = Instant::now();
        // A sender 5 % fast: a frame every 19 ms of the 20 ms it should be.
        // The queue fills a little each turn until its bound trims it, and
        // the turns the queue has to spare take a quiet one unheard.
        let mut sent = 0;
        for turn in 0..turns {
            let now = start + Duration::from_millis(20) * turn as u32;
            while sent * 19 <= turn * 20 {
                let sequence = sent as u16;
                let timestamp = u32::from(sequence) * SAMPLES as u32;
                playout.push(sound(sequence, timestamp, 7, opus[sent].clone(), now), now);
                sent += 1;
            }
            playout.next(now);
        }
        assert!(playout.counts.trimmed > 0, "the bound kept the delay down");
        assert!(playout.counts.skipped > 0, "and a spare turn went unheard");
        assert_eq!(
            playout.counts.lost, 0,
            "every number came, so none of this is loss"
        );
        assert_eq!(playout.counts.starved, 0, "and the queue never ran dry");
        assert_eq!(playout.counts.restarts, 0, "the sender kept its numbering");
    }

    #[test]
    fn a_skipped_frame_shortens_playback_instead_of_playing_silence() {
        let mut encoder = music();
        let turns = 200;
        let opus = tones(&mut encoder, turns * 20 / 19 + 2);
        let mut playout = Playout::new().unwrap();
        let start = Instant::now();
        // A sender 5 % fast, its packets and the player's turns on clocks the
        // test keeps: a frame every 19 ms of the 20 ms a turn plays, so the
        // queue fills until its bound trims it and the drift control takes a
        // frame on the turns the queue has to spare.
        let mut sent = 0;
        let mut played = 0u64;
        let mut skips = 0u64;
        for turn in 0..turns {
            let now = start + Duration::from_millis(20) * turn as u32;
            while sent * 19 <= turn * 20 {
                let sequence = sent as u16;
                let timestamp = u32::from(sequence) * SAMPLES as u32;
                playout.push(sound(sequence, timestamp, 7, opus[sent].clone(), now), now);
                sent += 1;
            }
            let skipped = playout.counts.skipped;
            if let Turn::Play { at, samples, of } = playout.next(now) {
                assert_eq!(samples, SAMPLES, "a 20 ms packet is a turn; turn {turn}");
                if of == Played::Frame {
                    played += 1;
                    assert!(
                        playout.pcm[at..at + samples * CHANNELS]
                            .iter()
                            .any(|&sample| sample != 0),
                        "turn {turn}: a frame played is the frame that came"
                    );
                }
                if playout.counts.skipped > skipped {
                    skips += 1;
                    // The frame the drift control took is gone from the
                    // output: this turn goes to the sound after it, not to
                    // silence standing in for what was dropped.
                    assert_ne!(
                        of,
                        Played::Silence,
                        "turn {turn}: a skipped frame must not cost a turn of silence"
                    );
                }
            }
            assert!(
                playout.held() <= MOST_SAMPLES,
                "turn {turn}: {} samples held, not a queue growing with the mismatch",
                playout.held()
            );
        }
        assert!(skips > 0, "the drift control took a frame");
        assert_eq!(
            skips, playout.counts.skipped,
            "each skip cost one turn at most"
        );
        // What input took and output spent, in turns: every frame taken is one
        // turn of sound, and a skipped one no turn at all, so the output is
        // shorter than the input by exactly what was skipped. That shorter
        // output is the drift caught up with.
        assert_eq!(
            played + skips,
            playout.counts.played,
            "{} turns played and {} skipped, of {} frames taken",
            played,
            skips,
            playout.counts.played
        );
    }

    #[test]
    fn a_slow_sender_starves_the_playhead_rather_than_losing_packets() {
        let mut encoder = music();
        let turns = 200;
        let opus = tones(&mut encoder, turns);
        let mut playout = Playout::new().unwrap();
        let start = Instant::now();
        // A sender 5 % slow: a frame every 21 ms of the 20 ms it should be,
        // so a turn every so often has nothing to play.
        let mut sent = 0;
        for turn in 0..turns {
            let now = start + Duration::from_millis(20) * turn as u32;
            while sent * 21 <= turn * 20 {
                let sequence = sent as u16;
                let timestamp = u32::from(sequence) * SAMPLES as u32;
                playout.push(sound(sequence, timestamp, 7, opus[sent].clone(), now), now);
                sent += 1;
            }
            playout.next(now);
            assert!(playout.waiting.len() <= MOST, "the queue doesn't grow");
            assert!(
                playout.held() <= MOST_SAMPLES,
                "{} samples held at turn {turn}",
                playout.held()
            );
        }
        assert!(playout.counts.starved > 0, "the playhead ran dry");
        assert!(
            playout.counts.late > 0,
            "and the sound it outran came after its slot"
        );
        assert_eq!(
            playout.counts.lost, 0,
            "the sender skipped no number: it's slow, not lossy"
        );
        assert_eq!(
            playout.counts.trimmed, 0,
            "nothing arrives too fast to play"
        );
        assert_eq!(playout.counts.restarts, 0, "a slow sender isn't a new one");
        assert!(
            playout.counts.played > (turns * 3 / 4) as u64,
            "and sound plays through it"
        );
    }

    #[test]
    fn a_lossy_stream_reports_loss_and_no_trimming() {
        let mut encoder = music();
        let opus = tones(&mut encoder, 8);
        let mut playout = Playout::new().unwrap();
        let now = Instant::now();
        // Three start it, and four more play on their own turns, so the
        // playhead is at 5 with the queue empty behind it.
        push_tones(&mut playout, &opus[..3], 0, 7, now);
        assert_eq!(playout.next(now), PLAYING, "three start it");
        push_tones(&mut playout, &opus[3..5], 3, 7, now);
        for frame in 1..5 {
            assert_eq!(playout.next(now), PLAYING, "frame {frame}");
        }
        // 5 and 6 never come and 7 does: a gap in the sender's numbering
        // with sound after it, which is what loss looks like.
        push_tones(&mut playout, &opus[7..], 7, 7, now);
        assert_eq!(playout.next(now), Turn::Wait(LATE), "its slot waits for it");
        assert_eq!(playout.next(now + LATE), CONCEALED, "5 is made up");
        assert_eq!(playout.next(now + LATE), CONCEALED, "and 6");
        assert_eq!(playout.next(now + LATE), PLAYING, "then 7 plays");
        assert_eq!(playout.counts.lost, 2, "both gaps are loss");
        assert_eq!(playout.counts.trimmed, 0, "nothing came too fast to hold");
        assert_eq!(playout.counts.starved, 0, "the playhead never ran dry");
        assert_eq!(playout.counts.skipped, 0, "and nothing went unheard");
        assert_eq!(playout.counts.restarts, 0, "the sender kept sending");
    }

    #[test]
    fn a_bound_emptied_slot_is_trimming_not_loss() {
        let mut encoder = music();
        let opus = tones(&mut encoder, MOST * 3);
        let mut playout = Playout::new().unwrap();
        let now = Instant::now();
        push_tones(&mut playout, &opus[..START], 0, 7, now);
        assert_eq!(playout.next(now), PLAYING, "three start it");
        // A burst from a sender that outran the playout: everything past the
        // queue's bounds goes, and those numbers can never come again.
        push_tones(&mut playout, &opus[START..], START as u16, 7, now);
        assert!(playout.counts.trimmed > 0, "the bounds dropped the far end");
        // What's left plays, and then the emptied slots are made up at
        // once: none of them is counted as a number the sender skipped, and
        // the run of them is what says the playout should wait for sound it
        // can play rather than chase what its own bound threw away.
        for frame in 1..=MOST as u16 {
            assert_eq!(playout.next(now), PLAYING, "frame {frame}");
        }
        for slot in MOST + 1..MOST + usize::from(GONE) {
            assert_eq!(playout.next(now), CONCEALED, "trimmed slot {slot}");
        }
        assert_eq!(playout.next(now), SILENT, "then it waits for sound");
        assert_eq!(playout.next, None);
        assert_eq!(
            playout.counts.trimmed,
            (MOST * 2 - 1) as u64,
            "each dropped frame is counted once"
        );
        assert_eq!(playout.counts.lost, 0, "not one of them is the link's");
        assert_eq!(playout.counts.starved, 0, "nor a playhead outrun");
    }

    #[test]
    fn a_frame_longer_than_the_room_is_clamped_to_it() {
        let mut playout = Playout::new().unwrap();
        let room = playout.pcm.len();
        // More to play than `pcm` holds, which a decoder that overran its
        // buffer would leave: the slices stop at the room there is.
        let start = room - CHANNELS * 100;
        playout.left = start..room + CHANNELS * 100;
        assert_eq!(
            playout.next(Instant::now()),
            Turn::Play {
                at: start,
                samples: 100,
                of: Played::Frame
            }
        );
    }

    /// Samples numbered in the order they were played, so what a backlog
    /// keeps says which ones did.
    fn counted(count: usize) -> impl Iterator<Item = f32> {
        (0..count).map(|sample| sample as f32)
    }

    #[test]
    fn keeps_a_bounded_backlog() {
        let mut input = Input::default();
        input.push(std::iter::repeat_n(0.1, BACKLOG * 3));
        assert_eq!(input.samples.len(), BACKLOG);
    }

    #[test]
    fn a_sample_that_is_not_a_number_becomes_silence() {
        let mut input = Input::default();
        input.push([0.25, f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 0.5]);
        assert_eq!(
            input.samples.len(),
            5,
            "a replaced sample keeps its place in the frame"
        );
        assert!(
            input.samples.iter().all(|sample| sample.is_finite()),
            "nothing Opus can't encode gets past the boundary"
        );
        assert_eq!(input.samples[0], 0.25);
        assert_eq!(input.samples[1], 0.0);
        assert_eq!(input.samples[2], 0.0);
        assert_eq!(input.samples[4], 0.5);
    }

    #[test]
    fn keeps_the_newest_samples_of_a_huge_callback() {
        let mut input = Input::default();
        input.push(counted(BACKLOG * 3));
        assert_eq!(input.samples.len(), BACKLOG);
        assert_eq!(
            input.samples[0],
            (BACKLOG * 2) as f32,
            "the oldest go first"
        );
        assert_eq!(input.samples[BACKLOG - 1], (BACKLOG * 3 - 1) as f32);
        for (older, newer) in input.samples.iter().zip(input.samples.iter().skip(1)) {
            assert_eq!(newer - older, 1.0, "kept samples stay in the order played");
        }
    }

    #[test]
    fn a_huge_callback_needs_no_more_room_than_the_backlog() {
        let mut input = Input::default();
        input.push(counted(BACKLOG * 3));
        let room = input.samples.capacity();
        assert!(room >= BACKLOG, "room for the backlog it keeps");
        for scale in [10, 100] {
            input.push(std::iter::repeat_n(0.0, BACKLOG * scale));
            assert_eq!(input.samples.len(), BACKLOG);
            assert_eq!(
                input.samples.capacity(),
                room,
                "a bigger callback reuses the room instead of growing it"
            );
        }
    }

    #[test]
    fn pushes_that_cross_the_bound_keep_the_newest() {
        let mut input = Input::default();
        input.push(counted(BACKLOG - 2));
        assert_eq!(input.samples.len(), BACKLOG - 2);
        input.push(std::iter::repeat_n(-1.0, 2));
        assert_eq!(
            input.samples.len(),
            BACKLOG,
            "exactly full, nothing dropped yet"
        );
        assert_eq!(input.samples[0], 0.0);
        input.push(std::iter::repeat_n(-2.0, 3));
        assert_eq!(input.samples.len(), BACKLOG);
        assert_eq!(input.samples[0], 3.0, "the three oldest went");
        assert_eq!(input.samples[BACKLOG - 1], -2.0, "the newest is still last");
    }

    #[test]
    fn an_empty_push_changes_nothing() {
        let mut input = Input::default();
        input.push(std::iter::empty());
        assert!(input.samples.is_empty());
        input.push(counted(PRIMED));
        input.push(std::iter::empty());
        assert_eq!(
            input.samples.len(),
            PRIMED,
            "waiting sound is not disturbed"
        );
        assert_eq!(input.samples[0], 0.0);
        assert_eq!(input.samples[PRIMED - 1], (PRIMED - 1) as f32);
    }

    #[test]
    fn an_exact_bound_push_keeps_all_of_it() {
        let mut input = Input::default();
        input.push(counted(BACKLOG));
        assert_eq!(input.samples.len(), BACKLOG);
        assert_eq!(input.samples[0], 0.0);
        assert_eq!(input.samples[BACKLOG - 1], (BACKLOG - 1) as f32);
    }

    #[test]
    fn a_start_that_fails_stops_the_threads_it_already_began() {
        // A listener standing in for the operating system's capture: it says
        // when it has begun, and when it has let go of what it held.
        let listening = Arc::new(AtomicBool::new(false));
        let running = Arc::new(AtomicBool::new(true));
        let began = Arc::clone(&listening);
        let going = Arc::clone(&running);
        let heard = move |_inputs: Inputs, _counted: Arc<Counted>, stop: Arc<AtomicBool>| {
            Ok(std::thread::spawn(move || {
                began.store(true, Ordering::Relaxed);
                while !stop.load(Ordering::Relaxed) {
                    std::thread::yield_now();
                }
                going.store(false, Ordering::Relaxed);
            }))
        };
        let codec = Arc::new(AtomicBool::new(false));
        let opened = Arc::clone(&codec);
        let started = capture(
            Inputs::default(),
            Arc::new(Counted::default()),
            move || {
                opened.store(true, Ordering::Relaxed);
                opus::Encoder::new(2, opus::Application::Audio, BITRATE)
            },
            heard,
            // The step after the listener: a mixer whose thread won't start.
            |_encoder, _stop, _counted, _inputs| {
                Err(std::io::Error::other("no thread for the mixer"))
            },
        );
        assert!(started.is_err(), "the mixer's thread wouldn't start");
        assert!(
            !running.load(Ordering::Relaxed),
            "the listener stopped before the error went back"
        );
        assert!(
            listening.load(Ordering::Relaxed),
            "it had begun before the step that failed"
        );
        assert!(codec.load(Ordering::Relaxed), "the codec came first");

        // A codec that won't open leaves nothing to undo: it runs before the
        // capture begins hearing the machine, so no thread was ever started.
        let anything = Arc::new(AtomicBool::new(false));
        let begun = Arc::clone(&anything);
        let none = move |_inputs: Inputs, _counted: Arc<Counted>, _stop: Arc<AtomicBool>| {
            begun.store(true, Ordering::Relaxed);
            Ok(std::thread::spawn(|| {}))
        };
        let refused = capture(
            Inputs::default(),
            Arc::new(Counted::default()),
            || Err(anyhow::anyhow!("no codec")),
            none,
            |_encoder, _stop, _counted, _inputs| Ok(std::thread::spawn(|| {})),
        );
        assert!(refused.is_err(), "the codec wouldn't open");
        assert!(
            !anything.load(Ordering::Relaxed),
            "the listener never began, so there was nothing to stop"
        );
    }
}

mod capture;
mod player;

pub(crate) use capture::*;
pub(crate) use player::*;

pub use capture::Capture;
pub use capture::SoundStats;
pub use player::Player;
pub use player::Volume;
