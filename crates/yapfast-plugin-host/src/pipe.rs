//! The pipe to the parent: one reader thread, one writer.
//!
//! stdin carries frames from the parent and stdout frames to it, as
//! `stream_worker` does in the app. The environment is cleared at spawn, so a
//! guest can't write into the pipe through `EXTISM_ENABLE_WASI_OUTPUT` or any
//! other `EXTISM_*` switch, and this side never mixes logs into stdout: they go
//! to stderr, and a plugin's own log lines travel as frames.
//!
//! Three threads, none of them polling. One reads stdin into a channel of
//! [`Signal`]s, which is the run loop's only wakeup. One owns stdout and is the
//! only thing that ever writes there, so a frame can never interleave with
//! another on a partial header. The third relays the reports the runtime emits
//! while a guest is running — a host call, a log line — because the runtime is
//! given a plain `Sender<FromHelper>` and a standard channel can't be waited on
//! together with another. All three sit blocked while nothing happens, which is
//! what keeps the idle path free.

use std::io::{self, Read, Write};
use std::sync::mpsc::{self, Sender};
use std::thread::{self, JoinHandle};

use yapfast_plugin_api::frame::{Frame, FrameKind, read_frame, write_frame};
use yapfast_plugin_api::message::FromHelper;

use crate::logging::LogBuffer;

/// What wakes the run loop.
#[derive(Debug)]
pub enum Signal {
    /// A frame from the parent.
    Frame(Frame),
    /// The parent closed its end between frames: a clean shutdown.
    Closed,
    /// The stream broke inside a frame, so the helper can't resynchronize.
    Broken(io::Error),
    /// The scheduler has nothing left to run and asks the loop to finish.
    Finished,
}

/// What the writer thread is given.
enum Out {
    Frame(Frame),
    /// Write what is already queued, then stop: the helper is exiting.
    Stop,
}

/// A handle that writes frames to the parent's stdin.
#[derive(Clone)]
pub struct Outbox {
    frames: Sender<Out>,
}

impl Outbox {
    /// Sends the answer to the request with this frame id.
    pub fn reply(&self, id: u64, report: &FromHelper) -> io::Result<()> {
        self.answer(FrameKind::Reply, id, report)
    }

    /// Sends a report the parent reads on its own rather than as an answer: a
    /// status, a fault, a plugin's log line.
    pub fn report(&self, report: &FromHelper) -> io::Result<()> {
        self.send(encode(kind_of(report), 0, report)?)
    }

    /// Sends a report in a frame kind the caller chooses, which the handshake
    /// needs: both sides open with [`FrameKind::Hello`].
    pub fn answer(&self, kind: FrameKind, id: u64, report: &FromHelper) -> io::Result<()> {
        self.send(encode(kind, id, report)?)
    }

    /// Sends a frame already built.
    pub fn send(&self, frame: Frame) -> io::Result<()> {
        self.frames
            .send(Out::Frame(frame))
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "the pipe's writer has stopped"))
    }
}

/// Asks the run loop to finish.
#[derive(Clone)]
pub struct Waker {
    signals: Sender<Signal>,
}

impl Waker {
    /// Wakes the loop so it can exit, because nothing is left to run.
    pub fn finish(&self) {
        let _ = self.signals.send(Signal::Finished);
    }
}

/// The helper's end of the pipe to Yapfast.
pub struct Pipe {
    signals: mpsc::Receiver<Signal>,
    waker: Sender<Signal>,
    outbox: Outbox,
    reports: Sender<FromHelper>,
    logs: LogBuffer,
    writer: Option<JoinHandle<()>>,
    relay: Option<JoinHandle<()>>,
}

impl Pipe {
    /// Starts the threads on this process's own standard streams.
    pub fn stdio() -> io::Result<Self> {
        Pipe::new(io::stdin(), io::stdout())
    }

    /// Starts the threads on the given streams, which is how a test drives the
    /// framing without a parent process.
    pub fn new(
        input: impl Read + Send + 'static,
        output: impl Write + Send + 'static,
    ) -> io::Result<Self> {
        let (signals, incoming) = mpsc::channel();
        let waker = signals.clone();
        // The reader is never joined: it stays blocked on stdin, and waiting
        // for input that is not coming would hold the exit.
        thread::Builder::new()
            .name("plugin-pipe-reader".into())
            .spawn(move || read_loop(input, signals))?;

        let (frames, outgoing) = mpsc::channel();
        let writer = thread::Builder::new()
            .name("plugin-pipe-writer".into())
            .spawn(move || write_loop(output, outgoing))?;

        let logs = LogBuffer::new();
        let (reports, relayed) = mpsc::channel();
        let relay = {
            let frames = frames.clone();
            let logs = logs.clone();
            thread::Builder::new()
                .name("plugin-pipe-reports".into())
                .spawn(move || relay_loop(relayed, frames, logs))?
        };

        Ok(Pipe {
            signals: incoming,
            waker,
            outbox: Outbox { frames },
            reports,
            logs,
            writer: Some(writer),
            relay: Some(relay),
        })
    }

    /// Blocks until the parent says something, or the loop is asked to finish.
    pub fn recv(&self) -> Signal {
        // Every sender is gone, so the parent closed the pipe.
        self.signals.recv().unwrap_or(Signal::Closed)
    }

    /// The handle the scheduler writes its statuses and answers through.
    pub fn outbox(&self) -> Outbox {
        self.outbox.clone()
    }

    /// The channel the runtime reports through: the host calls a guest makes
    /// while it runs, and the guest's own log lines.
    pub fn reports(&self) -> Sender<FromHelper> {
        self.reports.clone()
    }

    /// The handle the scheduler ends the run loop with.
    pub fn waker(&self) -> Waker {
        Waker {
            signals: self.waker.clone(),
        }
    }

    /// Every plugin's recent log lines, as they passed the relay.
    pub fn logs(&self) -> LogBuffer {
        self.logs.clone()
    }

    /// Writes `last` after everything already queued, waits for it to reach the
    /// writer, and closes the pipe.
    ///
    /// Everything else that writes must be dropped first — the scheduler and
    /// the runtime it owns — because the relay stops only when no sender is
    /// left, and this waits for it.
    pub fn finish(mut self, last: Option<FromHelper>) {
        // The last frame goes straight to the writer rather than through the
        // relay. The relay's channel has other holders for the life of the
        // process — the log subscriber among them — so waiting for it to close
        // would wait forever, and this is the one frame whose delivery has to
        // be ordered against `Out::Stop`.
        if let Some(report) = last {
            match encode(kind_of(&report), 0, &report) {
                Ok(frame) => {
                    let _ = self.outbox.frames.send(Out::Frame(frame));
                }
                Err(error) => tracing::error!(%error, "the last report could not be written"),
            }
        }
        // Everything the relay still holds is written before the pipe closes:
        // a fault reported a moment before the exit is the one the parent most
        // needs to see. It ends when the last report sender is gone, which is
        // why the caller lets go of the log subscriber's sender first.
        drop(self.reports);
        if let Some(relay) = self.relay.take() {
            let _ = relay.join();
        }
        if let Some(writer) = self.writer.take() {
            // A frame sent after this is lost, which is why the scheduler is
            // stopped before the pipe is closed.
            let _ = self.outbox.frames.send(Out::Stop);
            let _ = writer.join();
        }
    }
}

/// Reads frames until the parent closes the pipe or the stream breaks.
fn read_loop(mut input: impl Read, signals: Sender<Signal>) {
    loop {
        let (signal, last) = match read_frame(&mut input) {
            Ok(Some(frame)) => (Signal::Frame(frame), false),
            Ok(None) => (Signal::Closed, true),
            Err(error) => (Signal::Broken(error), true),
        };
        if signals.send(signal).is_err() || last {
            return;
        }
    }
}

/// Writes frames and flushes each one, on the only thread that touches stdout.
fn write_loop(mut output: impl Write, frames: mpsc::Receiver<Out>) {
    for out in frames {
        match out {
            Out::Frame(frame) => {
                if let Err(error) = write_frame(&mut output, &frame) {
                    tracing::error!(%error, "the pipe to Yapfast broke; the frame is lost");
                    return;
                }
            }
            Out::Stop => return,
        }
    }
}

/// Turns the runtime's reports into frames, keeping each plugin's log lines.
fn relay_loop(reports: mpsc::Receiver<FromHelper>, frames: Sender<Out>, logs: LogBuffer) {
    for report in reports {
        if let FromHelper::Log { plugin, record } = &report {
            logs.record(plugin, record.clone());
        }
        let frame = match encode(kind_of(&report), 0, &report) {
            Ok(frame) => frame,
            Err(error) => {
                tracing::error!(%error, "a report could not be written; it is lost");
                continue;
            }
        };
        if frames.send(Out::Frame(frame)).is_err() {
            return;
        }
    }
}

/// The frame kind a report travels in.
///
/// A guest's host call is a request the parent answers with a `Return`, and a
/// log line has a kind of its own so the parent needn't decode it to file it.
fn kind_of(report: &FromHelper) -> FrameKind {
    match report {
        FromHelper::Call { .. } => FrameKind::Request,
        FromHelper::Log { .. } => FrameKind::Log,
        FromHelper::Bye { .. } => FrameKind::Bye,
        FromHelper::Ready { .. } => FrameKind::Hello,
        _ => FrameKind::Report,
    }
}

/// One report as a frame, with its JSON payload.
fn encode(kind: FrameKind, id: u64, report: &FromHelper) -> io::Result<Frame> {
    serde_json::to_vec(report)
        .map(|payload| Frame::new(kind, id, payload))
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

#[cfg(test)]
mod tests {
    use super::*;
    use yapfast_plugin_api::PluginId;
    use yapfast_plugin_api::message::{Fault, FaultKind, PluginStatus};

    /// Collects what the writer thread produced.
    #[derive(Clone, Default)]
    struct Captured(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

    impl Write for Captured {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.0
                .lock()
                .expect("the capture is not poisoned")
                .extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl Captured {
        fn frames(&self) -> Vec<Frame> {
            let bytes = self.0.lock().expect("the capture is not poisoned").clone();
            let mut cursor = &bytes[..];
            let mut frames = Vec::new();
            while let Ok(Some(frame)) = read_frame(&mut cursor) {
                frames.push(frame);
            }
            frames
        }
    }

    #[test]
    fn a_report_carries_its_own_kind_and_no_id() {
        let captured = Captured::default();
        let pipe = Pipe::new(io::empty(), captured.clone()).expect("the threads start");
        let plugin = PluginId::parse("com.example.word-filter").expect("a valid id");
        pipe.outbox()
            .report(&FromHelper::Status {
                plugin,
                status: PluginStatus::Enabled,
            })
            .expect("the writer takes the frame");
        pipe.finish(None);
        let frames = captured.frames();
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].kind, FrameKind::Report);
        assert_eq!(frames[0].id, 0);
    }

    #[test]
    fn a_reply_keeps_the_request_id() {
        let captured = Captured::default();
        let pipe = Pipe::new(io::empty(), captured.clone()).expect("the threads start");
        let plugin = PluginId::parse("com.example.word-filter").expect("a valid id");
        pipe.outbox()
            .reply(
                41,
                &FromHelper::Fault {
                    plugin,
                    fault: Fault {
                        kind: FaultKind::Deadline,
                        reason: "ran past its 25 ms".into(),
                    },
                },
            )
            .expect("the writer takes the frame");
        pipe.finish(None);
        let frames = captured.frames();
        assert_eq!(frames[0].kind, FrameKind::Reply);
        assert_eq!(frames[0].id, 41);
    }

    #[test]
    fn the_last_report_is_written_before_the_pipe_closes() {
        let captured = Captured::default();
        let pipe = Pipe::new(io::empty(), captured.clone()).expect("the threads start");
        pipe.finish(Some(FromHelper::Bye {
            reason: "Yapfast asked".into(),
        }));
        let frames = captured.frames();
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].kind, FrameKind::Bye);
    }

    #[test]
    fn a_frame_written_by_the_relay_keeps_its_payload() {
        let captured = Captured::default();
        let pipe = Pipe::new(io::empty(), captured.clone()).expect("the threads start");
        let plugin = PluginId::parse("com.example.word-filter").expect("a valid id");
        pipe.reports()
            .send(FromHelper::EventDone { plugin })
            .expect("the relay takes the report");
        pipe.finish(None);
        let frames = captured.frames();
        assert_eq!(frames.len(), 1);
        let report: FromHelper =
            serde_json::from_slice(&frames[0].payload).expect("the payload is a report");
        assert!(matches!(report, FromHelper::EventDone { .. }));
    }
}
