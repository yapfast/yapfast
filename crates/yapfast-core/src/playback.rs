//! The video playing in the conversation view: its file downloading to disk,
//! the worker playing it as it does ([`crate::video_player`]), and what the
//! UI shows of it. One plays at a time; dropping it stops it and deletes
//! the file.

use std::{future::Future, path::Path, sync::Arc};

use anyhow::{Context, Result};
use tokio::sync::{mpsc::Receiver, oneshot};

use crate::{
    chat::{Video, VideoCommand},
    stream_worker::{Control, Report, WATCH_HEIGHT, WATCH_WIDTH, Worker},
};

pub struct Playback {
    pub video: Video,
    worker: Worker,
    reports: Option<Receiver<Report>>,
    /// The download's size once it's done, none if it failed.
    downloaded: Option<oneshot::Receiver<Option<u64>>>,
    download: tokio::task::JoinHandle<()>,
    _file: tempfile::TempPath,
}

/// What woke a playing video.
pub enum Wake {
    Report(Option<Report>),
    Downloaded(Option<u64>),
}

impl Playback {
    /// Starts downloading a video with `download`, into a file in
    /// `directory`, and playing it as it arrives. `size` is the file's, when
    /// known before it's downloaded.
    #[allow(clippy::too_many_arguments)]
    pub fn start<F>(
        video: Video,
        directory: &Path,
        size: Option<u64>,
        frame: (u32, u32),
        speakers: Option<String>,
        download: impl FnOnce(std::fs::File) -> F,
    ) -> Result<Self>
    where
        F: Future<Output = Result<u64>> + Send + 'static,
    {
        std::fs::create_dir_all(directory).context("could not keep the video")?;
        let file = tempfile::Builder::new()
            .prefix("video-")
            .tempfile_in(directory)
            .context("could not keep the video")?;
        let (handle, path) = file.into_parts();
        let fetching = download(handle);
        let (done, downloaded) = oneshot::channel();
        let download = tokio::spawn(async move {
            let size = fetching
                .await
                .inspect_err(|error| {
                    tracing::warn!(error = format!("{error:#}"), "could not download a video")
                })
                .ok();
            let _ = done.send(size);
        });
        let (worker, frames, reports) =
            Worker::play(path.to_path_buf(), size, frame, speakers, gain(&video)).inspect_err(
                |_| {
                    download.abort();
                },
            )?;
        Ok(Self {
            video: Video {
                frames: Some(Arc::new(frames)),
                ..video
            },
            worker,
            reports: Some(reports),
            downloaded: Some(downloaded),
            download,
            _file: path,
        })
    }

    /// Waits for the worker or the download. Cancel safe.
    pub async fn wait(&mut self) -> Wake {
        tokio::select! {
            report = next(&mut self.reports) => Wake::Report(report),
            Ok(size) = async { self.downloaded.as_mut().expect("guarded").await }, if self.downloaded.is_some() => {
                self.downloaded = None;
                Wake::Downloaded(size)
            }
        }
    }

    /// Takes what woke it. False once it stopped.
    pub fn handle(&mut self, wake: Wake) -> bool {
        match wake {
            Wake::Downloaded(size) => {
                let _ = self.worker.tell(&Control::Downloaded { size });
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

/// The worker's next report, or never once they ended.
async fn next(reports: &mut Option<Receiver<Report>>) -> Option<Report> {
    let Some(receiver) = reports else {
        return std::future::pending().await;
    };
    receiver.recv().await
}

#[cfg(test)]
mod tests {
    use super::*;

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
