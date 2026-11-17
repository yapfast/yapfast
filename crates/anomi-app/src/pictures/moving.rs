//! Which of the core's pictures move: their handles' ids, each with the
//! picture it draws, held weakly so nothing here keeps a picture the core
//! let go of. The widgets ask by id ([`find`]) as a picture shows, and get
//! its frames, decoded from the core's file a frame at a time. The Slint
//! side: `AVATAR_SOURCES` and `avatar_animation` in `src/ui/pictures.rs`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, Weak};

use anomi_widgets::picture::Moving;
use iced::advanced::image::Id;
use iced::widget::image;
use yapfast_core::chat::Picture;

/// Moving pictures by their handle's id, and how many there may be before
/// those the core let go of are swept out.
struct Known {
    pictures: HashMap<Id, Weak<Picture>>,
    sweep_at: usize,
}

thread_local! {
    static KNOWN: RefCell<Known> = RefCell::new(Known {
        pictures: HashMap::new(),
        sweep_at: 64,
    });
}

/// Notes that `handle` draws the moving `picture`.
pub(super) fn remember(handle: &image::Handle, picture: &Arc<Picture>) {
    KNOWN.with_borrow_mut(|known| {
        let id = handle.id();
        if known.pictures.len() >= known.sweep_at && !known.pictures.contains_key(&id) {
            known
                .pictures
                .retain(|_, picture| picture.strong_count() > 0);
            known.sweep_at = (known.pictures.len() * 2).max(64);
        }
        known.pictures.insert(id, Arc::downgrade(picture));
    });
}

/// The frames behind the still with the handle id `id`, while the core
/// holds its picture; see `anomi_widgets::picture::set_animations`.
pub(super) fn find(id: Id) -> Option<Arc<dyn Moving>> {
    let picture = KNOWN.with_borrow(|known| known.pictures.get(&id).and_then(Weak::upgrade))?;
    picture.animation.as_ref()?;
    Some(Arc::new(Animated(picture)))
}

/// A picture of the core's that moves, as the widgets read it.
struct Animated(Arc<Picture>);

impl Moving for Animated {
    fn size(&self) -> (u32, u32) {
        (self.0.width, self.0.height)
    }

    fn delays(&self) -> &[u32] {
        self.0
            .animation
            .as_ref()
            .map_or(&[][..], |animation| animation.delays.as_slice())
    }

    fn plays(&self) -> u32 {
        self.0
            .animation
            .as_ref()
            .map_or(0, |animation| animation.plays)
    }

    fn frames(&self) -> Box<dyn Iterator<Item = Vec<u8>>> {
        self.0.frames()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_still_or_a_picture_let_go_of_moves_nothing() {
        let still = Arc::new(Picture {
            width: 1,
            height: 1,
            ..Default::default()
        });
        let handle = image::Handle::from_rgba(1, 1, vec![0u8; 4]);
        remember(&handle, &still);
        assert!(find(handle.id()).is_none(), "a still");
        drop(still);
        assert!(find(handle.id()).is_none(), "gone");
    }
}
