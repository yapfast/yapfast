//! The rows' heights: each measured row's last height, by key, and the
//! average of them as the estimate for a row not measured yet. The Slint
//! side: `cached_item_height` in `vendor/i-slint-core/model/repeater.rs`,
//! which keeps only the average of the rows built; keeping each row's own
//! height makes a row measured once land where it was when it comes back.

use std::collections::HashMap;

/// A row's height before any row was measured, unless the list is given
/// one: a message row without its author line.
pub const ESTIMATE: f32 = 44.0;

#[derive(Debug, Default)]
pub struct Heights {
    measured: HashMap<u64, f32>,
    /// The sum of `measured`, for the average.
    sum: f64,
    /// What a row is taken to be before anything was measured.
    fallback: f32,
}

impl Heights {
    pub fn new(fallback: f32) -> Self {
        Self {
            fallback,
            ..Self::default()
        }
    }

    /// The height a row measured last, if it was.
    pub fn get(&self, key: u64) -> Option<f32> {
        self.measured.get(&key).copied()
    }

    /// The average measured height, or the fallback before any.
    pub fn estimate(&self) -> f32 {
        if self.measured.is_empty() {
            self.fallback
        } else {
            (self.sum / self.measured.len() as f64) as f32
        }
    }

    /// A row's height: measured, or estimated.
    pub fn height(&self, key: u64) -> f32 {
        self.get(key).unwrap_or_else(|| self.estimate())
    }

    pub fn set(&mut self, key: u64, height: f32) {
        let height = height.max(0.0);
        if let Some(old) = self.measured.insert(key, height) {
            self.sum -= f64::from(old);
        }
        self.sum += f64::from(height);
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.measured.len()
    }

    /// The cache holds far more rows than the list's `count`: a list whose
    /// rows were replaced (another room's messages) shouldn't keep the old
    /// ones' heights for ever.
    pub fn overfull(&self, count: usize) -> bool {
        self.measured.len() > 2 * count + 256
    }

    /// Forgets the rows `keep` refuses.
    pub fn prune(&mut self, keep: impl Fn(u64) -> bool) {
        self.measured.retain(|key, _| keep(*key));
        self.sum = self
            .measured
            .values()
            .map(|height| f64::from(*height))
            .sum();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimates_by_the_average_of_what_was_measured() {
        let mut heights = Heights::new(30.0);
        assert_eq!(heights.estimate(), 30.0);
        assert_eq!(heights.height(7), 30.0);
        heights.set(1, 20.0);
        heights.set(2, 60.0);
        assert_eq!(heights.estimate(), 40.0);
        assert_eq!(heights.height(1), 20.0);
        heights.set(2, 20.0);
        assert_eq!(heights.estimate(), 20.0);
    }

    #[test]
    fn prunes_rows_gone_once_far_more_than_the_list() {
        let mut heights = Heights::new(30.0);
        for key in 0..400 {
            heights.set(key, 10.0);
        }
        assert!(!heights.overfull(400), "not yet far more than the list");
        assert!(heights.overfull(10));
        heights.prune(|key| key < 10);
        assert_eq!(heights.len(), 10);
        assert_eq!(heights.estimate(), 10.0);
    }
}
