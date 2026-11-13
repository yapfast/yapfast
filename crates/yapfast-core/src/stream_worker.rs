//! Go Live runs in a worker process, `yapfast --stream-worker`, so screen
//! capture, video codecs and stream connections take memory only while
//! sharing or watching: the process exits when that stops and the memory
//! goes with it. Each worker shares this account's screen or shows one
//! other person's stream.
//!
//! Yapfast starts the worker with one [`Job`] as a JSON line on its standard
//! input, and reads [`Report`]s back from its standard output. Once Discord
//! has started the stream, a [`Control::Send`] line hands the worker the
//! stream's server, and it sends the video there; a [`Control::Switch`] line
//! shares another screen, window or camera on the same stream, and a
//! [`Control::Preview`] line says whether the UI shows the preview, which
//! the worker makes only while one does. A [`Control::Shown`] line says
//! whether the UI shows a watched stream's pictures, which a hidden or
//! minimised window doesn't, and none is converted or written while it
//! doesn't. A [`Control::Size`] line says how large the tile showing a
//! watched stream is, which the UI sends when that changes and on start, so
//! the stream is scaled to what shows it: a small tile costs a small frame,
//! and the frames file grows to hold a larger one. A `stop` line, or closing
//! its input, ends the job. Frames go to the UI through [`crate::frames`].

#[cfg(target_os = "linux")]
use std::os::fd::{AsFd, AsRawFd, BorrowedFd};
use std::{
    ffi::c_void,
    io::{BufRead, BufReader, Write},
    path::PathBuf,
    process::{Child, ChildStdin, Command, ExitStatus, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering},
        mpsc as channel,
    },
    time::{Duration, Instant},
};

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use yapfast_discord::{
    congestion,
    rtp::Codec,
    stream::StreamServer,
    stream_connection::{self, Link, ReceivedFrame, StreamReceiver, StreamSender},
};

use livekit::webrtc::{
    native::yuv_helper,
    video_frame::{I420Buffer, VideoBuffer},
};

use crate::capture;
#[cfg(target_os = "linux")]
use crate::capture::linear::{self, Order};
#[cfg(nvcuda)]
use crate::cuda;
#[cfg(any(wgc, dup))]
use crate::d3d11;
#[cfg(target_os = "linux")]
use crate::dmabuf;
#[cfg(dup)]
use crate::dup;
#[cfg(any(wgc, dup))]
use crate::frames::Drawn;
#[cfg(wgc)]
use crate::wgc;
use crate::{
    av_timing,
    decode::{Frame, Live},
    encode::{self, Effort, Encoder},
    frames::{Frames, Writer},
    picture::{self, I420, Tone},
    pressure, stream_audio,
    voice::{Presentation, Resolution, StreamQuality, StreamStats},
};

mod card;
mod codec;
mod consts;
mod encoding;
mod geometry;
mod job;
mod linux;
mod pattern;
mod processor;
mod report;
mod share;
mod sink;
mod watch;
mod worker;

pub(crate) use card::*;
pub(crate) use codec::*;
mod source;
pub(crate) use source::*;

pub(crate) use consts::*;

pub(crate) use encoding::*;
pub(crate) use geometry::*;
#[allow(unused_imports)]
pub(crate) use job::*;
#[allow(unused_imports)]
pub(crate) use linux::*;
pub(crate) use pattern::*;
pub(crate) use processor::*;
pub(crate) use report::*;
pub(crate) use share::*;
pub(crate) use sink::*;
pub use source::Shareable;
pub use source::Source;
pub use source::pick;
pub use source::portal_picks;
pub use source::shareable;
pub(crate) use watch::*;
#[allow(unused_imports)]
pub(crate) use worker::*;

pub use codec::StreamCodec;
pub use codec::set_hardware_encoding;
pub use codec::set_stream_codec;
pub use consts::ARGUMENT;
pub use consts::PREVIEW_HEIGHT;
pub use consts::PREVIEW_WIDTH;
pub use consts::SHARE_HEIGHT;
pub use consts::SHARE_WIDTH;
pub use consts::WATCH_HEIGHT;
pub use consts::WATCH_WIDTH;
pub use geometry::fit;
pub use job::Control;
pub use job::Job;
pub use job::run;
pub use report::Report;
pub use worker::Worker;

#[cfg(test)]
mod tests;
