//! The video playing in the conversation view: its file downloading to disk,
//! the worker playing it as it does ([`crate::video_player`]), and what the
//! UI shows of it. One plays at a time; dropping it stops it and deletes
//! the file.
//!
//! The file downloads in parts, with HTTP Range requests, as a browser
//! plays a video: from its start, and from wherever the worker waits for
//! bytes far from those arriving. An MP4 whose index comes after its
//! pictures, as OBS and many recorders write them, opens once its last bytes
//! arrive instead of the whole file; moving ahead plays from there. The gaps
//! fill in after.

use std::{
    future::Future,
    io::{Seek, SeekFrom, Write},
    path::Path,
    sync::Arc,
};

use anyhow::{Context, Result, bail};
use futures_util::{Stream, StreamExt};
use tokio::{
    sync::mpsc::{self, UnboundedReceiver, UnboundedSender},
    time::{Duration, Instant},
};
use yapfast_discord::api::FilePart;

use crate::{
    chat::{Video, VideoCommand},
    stream_worker::{Control, Report, WATCH_HEIGHT, WATCH_WIDTH, Worker},
    video_player::Ranges,
};

/// A byte the worker waits for this far ahead of the part arriving comes
/// with it, rather than from a part of its own.
const NEAR: u64 = 4 << 20;
/// A part asked for where the worker waited runs at least this far before
/// another wait moves the download, so a file whose sound lies far from its
/// pictures still downloads.
const LEAST_RUN: u64 = 1 << 20;
/// The worker hears of bytes that arrived this often, or once this many
/// wait to be told.
const TELL_EVERY: Duration = Duration::from_millis(50);
const TELL_BYTES: u64 = 1 << 20;

pub struct Playback {
    pub video: Video,
    worker: Worker,
    reports: Option<UnboundedReceiver<Report>>,
    /// What the download tells the worker: bytes arrived, then the end.
    arrivals: Option<UnboundedReceiver<Control>>,
    /// Where the worker waits, for the download.
    needs: UnboundedSender<u64>,
    download: tokio::task::JoinHandle<()>,
    _file: tempfile::TempPath,
}

/// What woke a playing video.
pub enum Wake {
    Report(Option<Report>),
    Arrival(Option<Control>),
}

impl Playback {
    /// Starts downloading a video into a file in `directory`, and playing it
    /// as it arrives. `fetch(from, to)` asks for its bytes from `from`, up
    /// to `to` when given. `size` is the file's, when known before.
    #[allow(
        clippy::too_many_arguments,
        reason = "the video, where it goes, and the callbacks that fetch and show it"
    )]
    pub fn start<F, P, S, B>(
        video: Video,
        directory: &Path,
        size: Option<u64>,
        frame: (u32, u32),
        speakers: Option<String>,
        fetch: F,
    ) -> Result<Self>
    where
        F: FnMut(u64, Option<u64>) -> P + Send + 'static,
        P: Future<Output = Result<FilePart<S>>> + Send,
        S: Stream<Item = Result<B>> + Unpin + Send,
        B: std::ops::Deref<Target = [u8]> + Send,
    {
        std::fs::create_dir_all(directory).context("could not keep the video")?;
        let file = tempfile::Builder::new()
            .prefix("video-")
            .tempfile_in(directory)
            .context("could not keep the video")?;
        let (handle, path) = file.into_parts();
        let (worker, frames, reports) =
            Worker::play(path.to_path_buf(), size, frame, speakers, gain(&video))?;
        let (tell, arrivals) = mpsc::unbounded_channel();
        let (needs, needed) = mpsc::unbounded_channel();
        let download = tokio::spawn(async move {
            let mut download = Download {
                file: handle,
                ranges: Ranges::default(),
                size,
                tell,
                told: None,
                told_at: Instant::now(),
            };
            let downloaded = download
                .run(fetch, needed)
                .await
                .inspect_err(|error| {
                    tracing::warn!(error = format!("{error:#}"), "could not download a video");
                })
                .ok();
            let _ = download.tell.send(Control::Downloaded { size: downloaded });
        });
        Ok(Self {
            video: Video {
                frames: Some(Arc::new(frames)),
                ..video
            },
            worker,
            reports: Some(reports),
            arrivals: Some(arrivals),
            needs,
            download,
            _file: path,
        })
    }

    /// Waits for the worker or the download. Cancel safe.
    pub async fn wait(&mut self) -> Wake {
        tokio::select! {
            report = next(&mut self.reports) => Wake::Report(report),
            arrival = next(&mut self.arrivals) => Wake::Arrival(arrival),
        }
    }

    /// Takes what woke it. False once it stopped.
    pub fn handle(&mut self, wake: Wake) -> bool {
        match wake {
            Wake::Arrival(Some(control)) => {
                let _ = self.worker.tell(&control);
            }
            Wake::Arrival(None) => self.arrivals = None,
            Wake::Report(Some(Report::Needs { at })) => {
                let _ = self.needs.send(at);
            }
            Wake::Report(Some(Report::Ready)) => {
                if let Some(frames) = &self.video.frames {
                    frames.unlink();
                }
            }
            Wake::Report(Some(Report::Opened {
                duration_ms,
                width,
                height,
                sound,
            })) => {
                self.video.loading = false;
                self.video.duration_ms = duration_ms;
                self.video.size = Some((width, height));
                self.video.sound = sound;
            }
            Wake::Report(Some(Report::Finished)) => {
                self.video.playing = false;
                self.video.ended = true;
            }
            Wake::Report(Some(Report::Ended { error })) => {
                self.stopped(error);
                return false;
            }
            Wake::Report(None) => {
                self.stopped(None);
                return false;
            }
            Wake::Report(Some(
                Report::Sharing { .. }
                | Report::Sending(_)
                | Report::Streaming
                | Report::Preview { .. }
                | Report::Screen { .. },
            )) => {}
        }
        true
    }

    /// The worker's gone; what it showed stays, with why.
    fn stopped(&mut self, error: Option<String>) {
        self.reports = None;
        self.video.playing = false;
        self.video.loading = false;
        self.video.frames = None;
        self.video.error = error;
    }

    /// Applies one of the person's controls.
    pub fn command(&mut self, command: VideoCommand) {
        let control = match command {
            VideoCommand::Play => {
                self.video.playing = true;
                self.video.ended = false;
                Control::Play
            }
            VideoCommand::Pause => {
                self.video.playing = false;
                Control::Pause
            }
            VideoCommand::Seek(time_ms) => {
                self.video.ended = false;
                Control::Seek(time_ms)
            }
            // Pictures larger than the frames file holds wouldn't show.
            VideoCommand::Size { width, height } => Control::Size {
                width: width.min(WATCH_WIDTH),
                height: height.min(WATCH_HEIGHT),
            },
            VideoCommand::Mute(muted) => {
                self.video.muted = muted;
                Control::Volume(gain(&self.video))
            }
            VideoCommand::Volume(volume) => {
                turn(&mut self.video, volume);
                Control::Volume(gain(&self.video))
            }
            // Dropping the playback stops it.
            VideoCommand::Stop => return,
        };
        if let Err(error) = self.worker.tell(&control) {
            tracing::debug!(error = format!("{error:#}"), "the video's worker is gone");
        }
    }
}

impl Drop for Playback {
    fn drop(&mut self) {
        self.download.abort();
    }
}

/// Sets how loud a video plays; 0 mutes it, keeping its level for when
/// it's unmuted.
fn turn(video: &mut Video, volume: f32) {
    let volume = volume.clamp(0.0, 1.0);
    video.muted = volume == 0.0;
    if volume > 0.0 {
        video.volume = volume;
    }
}

/// What the video's sound is multiplied by. Loudness is heard roughly as
/// the square of the slider's travel, so its middle sounds half as loud.
fn gain(video: &Video) -> f32 {
    if video.muted {
        0.0
    } else {
        video.volume * video.volume
    }
}

/// The next message, or never once they ended.
async fn next<T>(messages: &mut Option<UnboundedReceiver<T>>) -> Option<T> {
    let Some(receiver) = messages else {
        return std::future::pending().await;
    };
    receiver.recv().await
}

/// The video's file as it downloads, in parts, into the file the worker
/// reads.
struct Download {
    file: std::fs::File,
    /// What arrived.
    ranges: Ranges,
    /// The file's size, once known.
    size: Option<u64>,
    tell: UnboundedSender<Control>,
    /// Bytes that arrived the worker hasn't heard of, and when it last did.
    told: Option<(u64, u64)>,
    told_at: Instant,
}

impl Download {
    /// Downloads the whole file, going where the worker waits, and returns
    /// its size.
    async fn run<F, P, S, B>(
        &mut self,
        mut fetch: F,
        mut needs: UnboundedReceiver<u64>,
    ) -> Result<u64>
    where
        F: FnMut(u64, Option<u64>) -> P,
        P: Future<Output = Result<FilePart<S>>>,
        S: Stream<Item = Result<B>> + Unpin,
        B: std::ops::Deref<Target = [u8]>,
    {
        // Where the next part starts, and whether the worker waits there.
        let mut next = Some((0, false));
        // Whether the server sends parts; until it doesn't, it's asked to.
        let mut ranged = true;
        while let Some((from, waited)) = next {
            let (from, to) = self.ranges.gap(from);
            if self.size.is_some_and(|size| from >= size) {
                next = self.next_gap(0);
                continue;
            }
            let mut part = fetch(from, to).await?;
            if part.start != from {
                if part.start != 0 {
                    bail!("the server sent another part of the video");
                }
                // It sends the whole file: the download goes on from the
                // start, as it is, wherever the worker waits.
                ranged = false;
            }
            self.size = self.size.or(part.size);
            self.file
                .seek(SeekFrom::Start(part.start))
                .context("could not keep the video")?;
            // Only a file whose end is known downloads out of order: the
            // last part, open, finds its end.
            let jumps = ranged && self.size.is_some();
            let mut at = part.start;
            let mut wanted = None;
            let finished = loop {
                let tell_at = self.told_at + TELL_EVERY;
                tokio::select! {
                    biased;
                    Some(need) = needs.recv(), if jumps => wanted = Some(need),
                    () = tokio::time::sleep_until(tell_at), if self.told.is_some() => self.tell_worker(),
                    chunk = part.chunks.next() => match chunk {
                        Some(chunk) => {
                            let chunk = chunk?;
                            self.write(at, &chunk)?;
                            at += chunk.len() as u64;
                        }
                        None => break true,
                    },
                }
                // A wait far from what's arriving moves the download there,
                // once this part has run far enough, if it's for a wait.
                if let Some(need) = wanted
                    && (self.ranges.run(need) > 0
                        || (at <= need && need < at + NEAR && to.is_none_or(|to| need < to)))
                {
                    wanted = None;
                }
                if wanted.is_some() && (!waited || at.saturating_sub(from) >= LEAST_RUN) {
                    break false;
                }
            };
            if finished {
                if at == part.start {
                    bail!("the server sent none of the video");
                }
                // An open part, or the whole file, ends where the file does,
                // unless the server said otherwise: a part cut short is
                // asked for again.
                if to.is_none() || !ranged {
                    self.size = self.size.or(Some(at));
                }
            }
            self.tell_worker();
            next = match wanted {
                Some(need) => Some((need, true)),
                None => self.next_gap(at),
            };
        }
        self.size.context("the video's size is unknown")
    }

    /// The next bytes to fetch after `at`, then from the start; none once
    /// the file is whole.
    fn next_gap(&self, at: u64) -> Option<(u64, bool)> {
        let size = self.size?;
        [at, 0]
            .into_iter()
            .map(|from| self.ranges.gap(from).0)
            .find(|&from| from < size)
            .map(|from| (from, false))
    }

    /// Writes bytes that arrived at `at`, where the file is, telling the
    /// worker of them soon.
    fn write(&mut self, at: u64, chunk: &[u8]) -> Result<()> {
        let to = at + chunk.len() as u64;
        self.file
            .write_all(chunk)
            .context("could not keep the video")?;
        self.ranges.add(at, to);
        self.told = match self.told {
            Some((from, end)) if end == at => Some((from, to)),
            Some(_) => {
                self.tell_worker();
                Some((at, to))
            }
            None => Some((at, to)),
        };
        if self.told.is_some_and(|(from, to)| to - from >= TELL_BYTES) {
            self.tell_worker();
        }
        Ok(())
    }

    /// Tells the worker of the bytes that arrived since it last heard.
    fn tell_worker(&mut self) {
        self.told_at = Instant::now();
        if let Some((from, to)) = self.told.take() {
            let _ = self.tell.send(Control::Arrived {
                from,
                to,
                size: self.size,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What a served file was asked for: where each part starts and ends.
    type Asked = Arc<std::sync::Mutex<Vec<(u64, Option<u64>)>>>;
    /// A part of a served file, its chunks in memory.
    type Served = std::future::Ready<
        Result<FilePart<futures_util::stream::Iter<std::vec::IntoIter<Result<Vec<u8>>>>>>,
    >;

    /// A file served in parts, as Discord's CDN serves them or, `ranged`
    /// false, as a server that sends the whole file whatever it's asked.
    /// Its parts come 64 KB at a time, and it records what it was asked.
    fn served(
        file: Arc<Vec<u8>>,
        ranged: bool,
        asked: Asked,
    ) -> impl FnMut(u64, Option<u64>) -> Served {
        move |from, to| {
            asked.lock().unwrap().push((from, to));
            let start = if ranged { from } else { 0 };
            let end = to.filter(|_| ranged).unwrap_or(file.len() as u64);
            let chunks: Vec<_> = file[start as usize..end as usize]
                .chunks(64 << 10)
                .map(|chunk| Ok(chunk.to_vec()))
                .collect();
            std::future::ready(Ok(FilePart {
                start,
                size: Some(file.len() as u64),
                chunks: futures_util::stream::iter(chunks),
            }))
        }
    }

    /// Downloads `file` from `served`, the worker first waiting at `needs`,
    /// and gives back what was asked for, what the worker heard and the
    /// file as it ended up on disk.
    async fn download(
        file: &Arc<Vec<u8>>,
        ranged: bool,
        needs: &[u64],
    ) -> (Vec<(u64, Option<u64>)>, Vec<Control>, Vec<u8>) {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("video");
        let (tell, mut told) = mpsc::unbounded_channel();
        let (need, needed) = mpsc::unbounded_channel();
        for &at in needs {
            need.send(at).unwrap();
        }
        let mut download = Download {
            file: std::fs::File::create(&path).unwrap(),
            ranges: Ranges::default(),
            size: None,
            tell,
            told: None,
            told_at: Instant::now(),
        };
        let asked = Arc::new(std::sync::Mutex::new(Vec::new()));
        let size = download
            .run(served(file.clone(), ranged, asked.clone()), needed)
            .await
            .unwrap();
        assert_eq!(size, file.len() as u64);
        drop(download);
        let mut controls = Vec::new();
        while let Ok(control) = told.try_recv() {
            controls.push(control);
        }
        let asked = asked.lock().unwrap().clone();
        (asked, controls, std::fs::read(&path).unwrap())
    }

    /// What the worker heard arrived, joined.
    fn heard(controls: &[Control]) -> Ranges {
        let mut ranges = Ranges::default();
        for control in controls {
            if let Control::Arrived { from, to, .. } = control {
                ranges.add(*from, *to);
            }
        }
        ranges
    }

    fn file(size: usize) -> Arc<Vec<u8>> {
        Arc::new((0..size).map(|at| (at % 251) as u8).collect())
    }

    #[tokio::test]
    async fn the_download_goes_where_the_worker_waits() {
        let file = file(8 << 20);
        let end = file.len() as u64;
        // An index at the end: its part comes next, then the rest.
        let index = end - 100_000;
        let (asked, controls, written) = download(&file, true, &[index]).await;
        assert_eq!(asked, [(0, None), (index, None), (0, Some(index))]);
        assert_eq!(written, *file);
        assert_eq!(heard(&controls).run(0), end, "the worker heard of it all");
        assert!(
            controls.iter().all(|control| matches!(
                control,
                Control::Arrived { size: Some(size), .. } if *size == end
            )),
            "with its size"
        );
        // The index arrived first.
        assert!(matches!(
            controls[0],
            Control::Arrived { from, to, .. } if (from, to) == (index, end)
        ));
    }

    #[tokio::test]
    async fn a_wait_close_ahead_waits_for_the_part_arriving() {
        let file = file(3 << 20);
        let (asked, _, written) = download(&file, true, &[NEAR / 2]).await;
        assert_eq!(asked, [(0, None)]);
        assert_eq!(written, *file);
    }

    #[tokio::test]
    async fn a_server_without_ranges_sends_the_whole_file() {
        let file = file(8 << 20);
        let end = file.len() as u64;
        // Asked for the end, it sends it all, from the start.
        let (asked, controls, written) = download(&file, false, &[end - 100, 10]).await;
        assert_eq!(asked, [(0, None), (end - 100, None)]);
        assert_eq!(written, *file);
        assert_eq!(heard(&controls).run(0), end);
    }

    #[test]
    fn the_volume_slider_turns_the_sound_and_zero_mutes_it() {
        let mut video = Video {
            room: "r".into(),
            item: "i".into(),
            index: 0,
            frames: None,
            duration_ms: None,
            size: None,
            loading: false,
            playing: true,
            ended: false,
            sound: true,
            muted: false,
            volume: 1.0,
            error: None,
        };
        assert_eq!(gain(&video), 1.0);
        turn(&mut video, 0.5);
        assert_eq!(gain(&video), 0.25, "half the travel sounds half as loud");
        turn(&mut video, 0.0);
        assert!(video.muted);
        assert_eq!(gain(&video), 0.0);
        video.muted = false;
        assert_eq!(gain(&video), 0.25, "unmuting brings back the level it had");
        turn(&mut video, 2.0);
        assert!(!video.muted);
        assert_eq!(gain(&video), 1.0, "it doesn't go past full");
    }
}
