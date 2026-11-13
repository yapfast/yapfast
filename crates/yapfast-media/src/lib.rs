//! The vocabulary a stream shares with whatever carries it: codecs and their
//! payload types, the buffers an encoded frame travels in, what a receiver
//! report says about a stream, where a streamer's clocks sit, and the captures,
//! encoders and decoders that put a picture on the wire.
//!
//! Nothing here knows Discord, Matrix or any other protocol: a session names
//! these, and this crate does not.

pub mod audio_io;
pub mod av_timing;
pub mod bitstream;
pub mod camera;
pub mod capture;
pub mod clock;
pub mod codec;
pub mod declick;
pub mod decode;
pub mod device;
// A picture the compositor drew, as the descriptor it is: Linux's alone.
#[cfg(target_os = "linux")]
pub mod dmabuf;
#[cfg(all(feature = "discord", d3d11enc))]
pub mod d3d11;
#[cfg(dup)]
pub mod dup;
#[cfg(feature = "discord")]
pub mod encode;
pub mod frames;
pub mod geometry;
pub mod gpu;
pub mod lease;
pub mod logging;
pub mod noise;
pub mod opus;
pub mod picture;
pub mod pool;
pub mod pressure;
pub mod rtcp;
pub mod rtp;
pub mod sound;
#[cfg(feature = "discord")]
pub mod stream_audio;
pub mod surplus;
pub mod volume;
#[cfg(all(feature = "discord", wgc))]
pub mod wgc;

/// `mutex`'s guard, whether or not a thread panicked holding it: a stream's
/// picture is worth more than the panic that poisoned the lock.
pub(crate) fn lock<T>(mutex: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
