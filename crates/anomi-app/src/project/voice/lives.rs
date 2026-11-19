//! The call's moving pictures, each through one `Live` kept while its feed
//! lasts: the cameras, the stream watched and this account's own screen.
//! What decodes them is told the size they show at, or that they don't
//! (`place_videos`, `src/ui/callbacks.rs:4172`), and the window follows
//! only those in sight while it's in front (`crate::media::Source`): a
//! camera of a tile scrolled away, a stream covered by Settings or the
//! window behind take no frames for display; the call's sound goes on.

use std::collections::HashMap;
use std::sync::Arc;

use yapfast_core::frames::Frames;
use yapfast_core::picture::Feed;
use yapfast_core::voice;

use anomi_vm::kind::Live;
use anomi_vm::voice::stage::Placement;

use super::focus::{Focus, Pip, Stage};
use crate::media::{Clock, Source};

/// A worker's frames, the `Live` they show in, and how far in they are.
pub type Mapped = (Arc<Frames>, Live, Arc<Clock>);

/// The call's `Live`s, one per feed.
#[derive(Default)]
pub struct Lives {
    cameras: HashMap<String, (Feed, Live)>,
    pub watched: Option<Mapped>,
    pub own: Option<Mapped>,
}

/// Where the call shows: the grid and its tiles' places, the corner.
#[derive(Clone, Copy, Debug, Default)]
pub struct Sight {
    /// The window is in front and shows a window.
    pub front: bool,
    /// The grid shows, in the conversation or full screen, uncovered.
    pub grid: bool,
    pub placement: Option<Placement>,
    /// The corner shows, and what.
    pub pip: Option<Pip>,
    /// Pixels a logical one.
    pub scale: f32,
}

impl Sight {
    /// The size in pixels the grid's tile at `slot` shows at, while it's in
    /// sight.
    pub fn in_grid(&self, slot: usize) -> Option<(u32, u32)> {
        let placement = self.placement.filter(|_| self.front && self.grid)?;
        placement.shown_at(slot, self.scale)
    }
}

impl Lives {
    /// The `Live` of someone's camera, by participant id.
    pub fn camera(&self, id: &str) -> Option<&Live> {
        self.cameras.get(id).map(|(_, live)| live)
    }

    /// Keeps a `Live` for each feed of `voice`, and lets go of those gone:
    /// a new feed, or new frames, gets a new one.
    pub fn follow(&mut self, voice: &voice::State) {
        let active = voice.active();
        self.cameras.retain(|id, (feed, _)| {
            active
                && voice
                    .participants
                    .iter()
                    .any(|person| &person.id == id && person.video.as_ref() == Some(feed))
        });
        for person in voice.participants.iter().filter(|_| active) {
            if let Some(feed) = &person.video {
                self.cameras
                    .entry(person.id.clone())
                    .or_insert_with(|| (feed.clone(), Live::default()));
            }
        }
        let watched = voice
            .watching
            .as_ref()
            .and_then(|watching| watching.frames.as_ref());
        keep(&mut self.watched, watched.filter(|_| active));
        let own = voice.sharing.as_ref().map(|sharing| &sharing.frames);
        keep(&mut self.own, own.filter(|_| active));
    }

    /// Tells each camera the size it shows at, or that it doesn't, and says
    /// which pictures the window follows: those in sight, in front.
    pub fn place(&self, voice: &voice::State, stage: Stage<'_>, sight: Sight) -> Vec<Source> {
        let mut sources = Vec::new();
        let before = stage.before();
        let pixels = |side: f32| (side * sight.scale).round() as u32;
        let corner = (
            anomi_vm::voice::pip::Pip::WIDTH,
            anomi_vm::voice::pip::Pip::HEIGHT,
        );
        let in_grid = |slot: usize| sight.in_grid(slot);
        for (index, person) in stage.participants.iter().enumerate() {
            let Some(feed) = &person.video else { continue };
            let in_corner = sight.front && sight.pip == Some(Pip::Camera(index));
            let at = if in_corner {
                Some((pixels(corner.0), pixels(corner.1)))
            } else {
                in_grid(before + index)
            };
            let large = !in_corner
                && at.is_some()
                && sight.placement.and_then(|placement| placement.focused) == Some(before + index);
            feed.show_at(at);
            feed.show_large(large);
            if at.is_some()
                && let Some(live) = self.camera(&person.id)
            {
                sources.push(Source::Feed(feed.clone(), live.clone()));
            }
        }
        let watching = voice
            .watching
            .as_ref()
            .map(|watching| watching.user.as_str());
        if let (Some(watched), Some(user)) = (&self.watched, watching) {
            let slot = stage.slot(&Focus::Stream(user.to_owned()));
            let seen = sight.front
                && (sight.pip == Some(Pip::Watched) || slot.and_then(in_grid).is_some());
            mapped(watched, seen, &mut sources);
        }
        if let Some(own) = &self.own {
            let seen = sight.front
                && (sight.pip == Some(Pip::Shared) || in_grid(0).is_some() && stage.sharing);
            mapped(own, seen, &mut sources);
        }
        sources
    }

    /// Lets every picture go, as the window does into the tray: the feeds
    /// stop decoding for it.
    pub fn hide(&mut self, voice: &voice::State) {
        for person in voice.participants.iter() {
            if let Some(feed) = &person.video {
                feed.show_at(None);
                feed.show_large(false);
            }
        }
        *self = Self::default();
    }
}

/// A worker's frames followed while `seen`; told to make none otherwise
/// (the media's alarm tells it again as it follows them).
fn mapped(kept: &Mapped, seen: bool, sources: &mut Vec<Source>) {
    if seen {
        sources.push(Source::Mapped(
            kept.0.clone(),
            kept.1.clone(),
            kept.2.clone(),
        ));
    } else {
        kept.0.pause(true);
    }
}

/// Keeps `kept` on `frames`: a `Live` of its own for each worker's frames.
fn keep(kept: &mut Option<Mapped>, frames: Option<&Arc<Frames>>) {
    match frames {
        Some(frames)
            if kept
                .as_ref()
                .is_some_and(|kept| Arc::ptr_eq(&kept.0, frames)) => {}
        Some(frames) => *kept = Some((frames.clone(), Live::default(), Arc::default())),
        None => *kept = None,
    }
}
