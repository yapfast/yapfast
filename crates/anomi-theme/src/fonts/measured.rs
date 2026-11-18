//! What text measured in the interface's fonts, kept by key until the fonts
//! change: a width a label, a button or a choice takes.

use std::borrow::Borrow;
use std::collections::HashMap;
use std::hash::Hash;

use super::{Version, version};

/// Values measured in the fonts of one [`version`], by key. A lookup after
/// the fonts changed finds nothing and drops what was kept, so a width is
/// measured again in the new font, and the old ones go.
#[derive(Debug)]
pub struct Measured<K, V> {
    fonts: Version,
    values: HashMap<K, V>,
}

impl<K, V> Default for Measured<K, V> {
    fn default() -> Self {
        Self {
            fonts: Version::default(),
            values: HashMap::new(),
        }
    }
}

impl<K: Eq + Hash, V: Copy> Measured<K, V> {
    /// The value kept for `key` in the fonts there are now.
    pub fn get<Q>(&mut self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: Eq + Hash + ?Sized,
    {
        let fonts = version();
        if fonts != self.fonts {
            self.fonts = fonts;
            self.values = HashMap::new();
            return None;
        }
        self.values.get(key).copied()
    }

    /// Keeps `value`, measured just now, for `key`; when `most` are kept
    /// already, drops them first, so a cache of passing words stays small.
    pub fn keep(&mut self, key: K, value: V, most: usize) -> V {
        if self.values.len() >= most {
            self.values.clear();
        }
        self.values.insert(key, value);
        value
    }
}
