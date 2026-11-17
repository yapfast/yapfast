//! Which pictures move, and how their frames are read. A view-model carries
//! a picture as its still, an `image::Handle`; the frames behind a still
//! are found by the handle's id, so no view-model changes for a picture to
//! play. The app answers for the core's pictures ([`set_animations`],
//! `anomi-app/src/pictures/`), and samples register their own
//! ([`register`]).
//!
//! The Slint side handed each `Picture` its frames as a model
//! (`PictureStore.frames`, `animation` in `src/ui/pictures.rs`).

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use iced::advanced::image::Id;
use iced::widget::image;

/// An animation behind a still: its frames, read from the first each time
/// a run over them starts, as the core keeps an animation as its file and
/// decodes a frame at a time (`yapfast_core::chat::Picture::frames`).
pub trait Moving: Send + Sync {
    /// Every frame's size, in pixels: the still's.
    fn size(&self) -> (u32, u32);
    /// How long each frame shows, in milliseconds.
    fn delays(&self) -> &[u32];
    /// How many times it plays before holding its last frame; 0 for ever.
    fn plays(&self) -> u32;
    /// The frames from the first, RGBA at [`Moving::size`], each decoded
    /// as it's taken. Called on a thread of its own.
    fn frames(&self) -> Box<dyn Iterator<Item = Vec<u8>>>;
}

/// Finds the animation behind a still, by the still's handle id.
pub type Find = fn(Id) -> Option<Arc<dyn Moving>>;

static FIND: OnceLock<Find> = OnceLock::new();

thread_local! {
    static SAMPLES: RefCell<HashMap<Id, Arc<dyn Moving>>> = RefCell::default();
}

/// Has `find` answer which stills move from now on: the app's, over the
/// pictures the core decoded. Set once; later calls are ignored.
pub fn set_animations(find: Find) {
    let _ = FIND.set(find);
}

/// Makes `still` play `moving`'s frames, for samples and tests. Kept until
/// [`forget`]: the app's pictures go through [`set_animations`] instead,
/// which holds nothing the core let go of.
pub fn register(still: &image::Handle, moving: Arc<dyn Moving>) {
    SAMPLES.with_borrow_mut(|samples| samples.insert(still.id(), moving));
}

/// Lets go of what [`register`] kept for `still`.
pub fn forget(still: &image::Handle) {
    SAMPLES.with_borrow_mut(|samples| samples.remove(&still.id()));
}

/// The animation behind `still`, if it moves.
pub(crate) fn find(still: &image::Handle) -> Option<Arc<dyn Moving>> {
    let id = still.id();
    SAMPLES
        .with_borrow(|samples| samples.get(&id).cloned())
        .or_else(|| FIND.get().and_then(|find| find(id)))
        .filter(|moving| moving.delays().len() > 1)
}

/// Frames held in memory: an animation drawn in code, as the samples'.
pub struct Frames {
    width: u32,
    height: u32,
    frames: Arc<[Arc<[u8]>]>,
    delays: Vec<u32>,
    plays: u32,
}

impl Frames {
    /// `frames` are RGBA, `width` by `height`; `delays` in milliseconds,
    /// one a frame.
    pub fn new(
        width: u32,
        height: u32,
        frames: Vec<Vec<u8>>,
        delays: Vec<u32>,
        plays: u32,
    ) -> Self {
        Self {
            width,
            height,
            frames: frames.into_iter().map(Arc::from).collect(),
            delays,
            plays,
        }
    }
}

impl Moving for Frames {
    fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    fn delays(&self) -> &[u32] {
        &self.delays
    }

    fn plays(&self) -> u32 {
        self.plays
    }

    fn frames(&self) -> Box<dyn Iterator<Item = Vec<u8>>> {
        let frames = self.frames.clone();
        Box::new((0..frames.len()).map(move |index| frames[index].to_vec()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_registered_still_moves_until_forgotten() {
        let still = image::Handle::from_rgba(1, 1, vec![0u8; 4]);
        assert!(find(&still).is_none());
        let frames = Frames::new(1, 1, vec![vec![0; 4], vec![255; 4]], vec![50, 50], 0);
        register(&still, Arc::new(frames));
        let moving = find(&still).expect("it moves");
        assert_eq!(moving.frames().nth(1), Some(vec![255; 4]));
        forget(&still);
        assert!(find(&still).is_none());
    }

    #[test]
    fn one_frame_is_a_still() {
        let still = image::Handle::from_rgba(1, 1, vec![0u8; 4]);
        register(
            &still,
            Arc::new(Frames::new(1, 1, vec![vec![0; 4]], vec![50], 0)),
        );
        assert!(find(&still).is_none());
        forget(&still);
    }
}
