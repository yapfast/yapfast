//! A level that moves on its own, such as the microphone's, shared between
//! the app and the widget that draws it.

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

/// A level from 0 to 1 the app stores as it measures it and the widget
/// that shows it reads as it draws (`anomi_voice::devices::meter`): a new
/// level changes no view-model, builds no view and lays nothing out, only
/// a redraw the app asks for when the level moved enough to show. Two are
/// equal only when they're the same one.
#[derive(Clone, Default)]
pub struct Level(Arc<AtomicU32>);

impl Level {
    /// One at `level`.
    pub fn new(level: f32) -> Self {
        let made = Self::default();
        made.set(level);
        made
    }

    /// Stores `level`, clamped to 0 to 1; says whether it changed.
    pub fn set(&self, level: f32) -> bool {
        let bits = level.clamp(0.0, 1.0).to_bits();
        self.0.swap(bits, Ordering::AcqRel) != bits
    }

    /// The level now, from 0 to 1.
    pub fn get(&self) -> f32 {
        f32::from_bits(self.0.load(Ordering::Acquire))
    }
}

impl PartialEq for Level {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl fmt::Debug for Level {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("Level").field(&self.get()).finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_level_says_whether_it_moved_and_is_equal_only_to_itself() {
        let level = Level::new(0.5);
        assert!(!level.set(0.5));
        assert!(level.set(2.0));
        assert_eq!(level.get(), 1.0);
        assert_eq!(level, level.clone());
        assert_ne!(level, Level::new(1.0));
    }
}
