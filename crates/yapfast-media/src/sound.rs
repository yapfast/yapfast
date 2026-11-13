//! The sound a stream carries: one Opus frame at a time out of a pool of
//! buffers, and the frames that come back in, each with the place its numbering
//! and the streamer's clocks give it. What a mixer writes into and a player
//! reads out of, which neither a connection nor a protocol owns.

use std::{
    sync::{Arc, Mutex, mpsc as channel},
    time::Duration,
};
// The queue a frame of sound leaves on and the connection takes it from: bounded,
// and a frame that arrives at a full queue is dropped rather than waited for, so
// the mixer never stalls behind a network that can't keep up.
use tokio::sync::mpsc;

use crate::{clock::StreamClock, codec::Codec, lock, rtp};

/// Frames of sound a connection keeps to hand back: a fifth of a second at 50
/// frames each, which is a path's own jitter and no more.
const SOUND_BUFFERS: usize = 8;

/// One Opus frame of the stream's sound: 20 ms of stereo at 48 kHz, with
/// `timestamp` in samples.
pub struct Sound {
    pub opus: Vec<u8>,
    pub timestamp: u32,
}

/// The buffers the stream's sound is queued in: the mixer writes a frame into
/// one and the connection gives it back once the frame is on the wire, so a
/// stream sending 50 frames a second allocates for none of them. A queue that
/// backs up takes a buffer of its own rather than wait for one.
#[derive(Default)]
pub struct SoundBuffers(Mutex<Vec<Vec<u8>>>);

impl SoundBuffers {
    /// A buffer to write a frame into: one that came back, or a new one when
    /// every frame still waits to go out.
    pub fn take(&self) -> Vec<u8> {
        lock(&self.0).pop().unwrap_or_default()
    }
    /// Takes back the buffer of a frame that went out.
    /// Takes back the buffer of a frame that went out, which the connection
    /// gives here once the frame is on the wire.
    pub fn give(&self, mut buffer: Vec<u8>) {
        // As many as the queue holds: more would be memory kept for a
        // connection that can't keep up with the mixer.
        let mut idle = lock(&self.0);
        if idle.len() < SOUND_BUFFERS {
            buffer.clear();
            idle.push(buffer);
        }
    }
}

/// Queues the stream's sound, from a thread of the sender's own.
#[derive(Clone)]
pub struct SoundSender {
    pub sounds: mpsc::Sender<Sound>,
    pub buffers: Arc<SoundBuffers>,
}

impl SoundSender {
    /// A buffer to write a frame of sound into, which comes back here once the
    /// frame is sent.
    pub fn buffer(&self) -> Vec<u8> {
        self.buffers.take()
    }
    /// Queues a frame of sound, dropping it if the network can't keep up.
    pub fn send(&self, sound: Sound) {
        let _ = self.sounds.try_send(sound);
    }
}

/// One whole frame received, decrypted and ready to decode.
pub struct ReceivedFrame {
    pub codec: Codec,
    pub data: Vec<u8>,
    pub keyframe: bool,
    /// The sender's RTP timestamp for the picture, on its 90 kHz clock: the
    /// one thing that ties a decoded picture to the sound that came with it,
    /// which the frames file and the reports carry on from here.
    pub timestamp: u32,
}

/// One Opus frame of a watched stream's sound as it came: its payload, its
/// place in the sender's numbering, and when it reached Yapfast.
#[derive(Debug)]
pub struct ReceivedSound {
    pub opus: Vec<u8>,
    pub sequence: u16,
    pub timestamp: u32,
    pub ssrc: u32,
    /// When it arrived, on the monotonic clock that runs across threads in
    /// this process, which is what the player compares against.
    pub arrived: std::time::Instant,
}

impl ReceivedSound {
    /// `packet`'s numbering, and `opus`, its decrypted payload, which came
    /// at `arrived`.
    pub fn new(packet: &rtp::Packet, opus: Vec<u8>, arrived: std::time::Instant) -> Self {
        Self {
            opus,
            sequence: packet.sequence,
            timestamp: packet.timestamp,
            ssrc: packet.ssrc,
            arrived,
        }
    }
}

/// A watched stream's sound on its way to a player, with the clock the
/// streamer's sender reports put its sound and picture on, so the player can
/// say how far behind the picture it plays. It ends with the connection.
pub struct SoundReceiver {
    pub sounds: channel::Receiver<ReceivedSound>,
    pub clock: Arc<StreamClock>,
}

impl SoundReceiver {
    /// `sounds` as they come, with `clock` the receive side writes into.
    pub fn new(sounds: channel::Receiver<ReceivedSound>, clock: Arc<StreamClock>) -> Self {
        Self { sounds, clock }
    }

    /// The sound waiting now, without waiting for more.
    pub fn try_iter(&self) -> channel::TryIter<'_, ReceivedSound> {
        self.sounds.try_iter()
    }

    /// The next sound, waiting at most `wait`: `Err` when none came in time
    /// or the connection ended.
    pub fn recv_timeout(&self, wait: Duration) -> Result<ReceivedSound, channel::RecvTimeoutError> {
        self.sounds.recv_timeout(wait)
    }

    /// What the streamer last said about where its clocks sit.
    pub fn clock(&self) -> Arc<StreamClock> {
        Arc::clone(&self.clock)
    }
}
