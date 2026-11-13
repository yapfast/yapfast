//! The vocabulary a stream shares with whatever carries it: codecs and their
//! payload types, the buffers an encoded frame travels in, what a receiver
//! report says about a stream, and where a streamer's clocks sit.
//!
//! Nothing here knows Discord, Matrix or any other protocol: a session names
//! these, and this crate does not.

pub mod clock;
pub mod codec;
pub mod pool;
pub mod rtcp;

/// `mutex`'s guard, whether or not a thread panicked holding it: a stream's
/// picture is worth more than the panic that poisoned the lock.
pub(crate) fn lock<T>(mutex: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
