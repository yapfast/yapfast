//! What each plugin answered about each message on screen, so a row that
//! scrolls back into view costs a lookup and not a call into a guest.
//!
//! An answer is filed under the plugin, the version of it that answered, the
//! message and a hash of what the message said when it was asked. A new
//! version of the plugin — an update, a folder plugin's rebuild — and an edit
//! of the message each make a key nothing was filed under, so the plugin is
//! asked again; a settings change forgets the plugin's answers, since a
//! translator's target language is a setting. Anything else is answered from
//! here, and handed to the interface the way a fresh answer is, because the
//! interface keeps fewer decorations than this does and may have let one go.

use std::collections::{HashMap, VecDeque};
use std::hash::{Hash, Hasher};
use std::time::{Duration, Instant};

use yapfast_plugin_api::PluginId;
use yapfast_plugin_api::message::DecorateRequest;

use crate::slots;

/// How many (plugin, message) answers are kept, the least recently asked
/// going first: four screenfuls each for a handful of plugins.
pub(crate) const DECORATE_CACHE: usize = 2048;

/// How long a question stays out before a pass asks it again: an answer that
/// never came — a deadline, a refused tree, a helper that went away — must not
/// leave the row plain for good.
pub(crate) const ASK_AGAIN: Duration = Duration::from_secs(2);

/// Where one answer is filed.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Key {
    plugin: PluginId,
    message: String,
}

/// One answer, or the question still out.
#[derive(Clone, Debug)]
struct Kept {
    /// The plugin's version and module, hashed: an update is a new key.
    version: u64,
    /// What the message said when it was asked, hashed: an edit is a new key.
    body: u64,
    /// `None` while the plugin hasn't answered yet; `Some(None)` for an answer
    /// that drew nothing, which is kept so it isn't asked again.
    answer: Option<Option<slots::Output>>,
    /// When it was asked.
    asked: Instant,
}

/// What a pass over the rows on screen should do about one (plugin, row).
#[derive(Debug, PartialEq)]
pub(crate) enum Look {
    /// Nothing is kept for it: ask the plugin.
    Ask,
    /// The plugin was asked and hasn't answered: nothing to do.
    Waiting,
    /// The answer is kept: hand it to the interface again.
    Answered(Option<slots::Output>),
}

#[derive(Debug, Default)]
pub(crate) struct DecorateCache {
    kept: HashMap<Key, Kept>,
    /// Oldest first; a key may appear more than once, and only its last place
    /// counts, which keeps a lookup from walking the queue.
    order: VecDeque<Key>,
    /// How many plugin calls the cache saved, for the measurements.
    pub(crate) hits: u64,
}

/// A plugin's version key: its manifest version and the module that runs.
pub(crate) fn version_key(version: &str, module: Option<&str>) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    version.hash(&mut hasher);
    module.hash(&mut hasher);
    hasher.finish()
}

/// What a row said, as far as a decoration depends on it.
pub(crate) fn body_key(row: &DecorateRequest) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    row.author.hash(&mut hasher);
    row.body.hash(&mut hasher);
    hasher.finish()
}

impl DecorateCache {
    /// What to do about `plugin` and `row`, and the question filed as asked
    /// when the answer is to ask.
    pub(crate) fn look(&mut self, plugin: &PluginId, version: u64, row: &DecorateRequest) -> Look {
        let key = Key {
            plugin: plugin.clone(),
            message: row.message.clone(),
        };
        let body = body_key(row);
        let look = match self.kept.get(&key) {
            Some(kept)
                if kept.version == version
                    && kept.body == body
                    && (kept.answer.is_some() || kept.asked.elapsed() < ASK_AGAIN) =>
            {
                match &kept.answer {
                    None => Look::Waiting,
                    Some(answer) => {
                        self.hits += 1;
                        Look::Answered(answer.clone())
                    }
                }
            }
            _ => {
                self.kept.insert(
                    key.clone(),
                    Kept {
                        version,
                        body,
                        answer: None,
                        asked: Instant::now(),
                    },
                );
                Look::Ask
            }
        };
        self.order.push_back(key);
        self.trim();
        look
    }

    /// Files a plugin's answer about a message it was asked about. An answer
    /// for a question nobody filed — the cache let it go, or a settings change
    /// forgot it — is not kept, and is still drawn.
    pub(crate) fn answered(
        &mut self,
        plugin: &PluginId,
        message: &str,
        answer: Option<slots::Output>,
    ) {
        let key = Key {
            plugin: plugin.clone(),
            message: message.to_owned(),
        };
        if let Some(kept) = self.kept.get_mut(&key) {
            kept.answer = Some(answer);
        }
    }

    /// Forgets every answer of one plugin: its settings changed, or it went.
    pub(crate) fn forget(&mut self, plugin: &PluginId) {
        self.kept.retain(|key, _| &key.plugin != plugin);
        self.order.retain(|key| &key.plugin != plugin);
    }

    /// How many answers are kept.
    pub(crate) fn len(&self) -> usize {
        self.kept.len()
    }

    fn trim(&mut self) {
        // The queue holds a key once per look, so it is compacted when it has
        // grown well past what it orders.
        if self.order.len() > DECORATE_CACHE * 4 {
            let mut seen = std::collections::HashSet::new();
            let mut compact: VecDeque<Key> = VecDeque::with_capacity(self.kept.len());
            for key in self.order.drain(..).rev() {
                if seen.insert(key.clone()) {
                    compact.push_front(key);
                }
            }
            self.order = compact;
        }
        while self.kept.len() > DECORATE_CACHE {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            // Only its last place in the queue counts.
            if !self.order.contains(&oldest) {
                self.kept.remove(&oldest);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(message: &str, body: &str) -> DecorateRequest {
        DecorateRequest {
            room: "room-1".into(),
            message: message.into(),
            author: "person-1".into(),
            body: body.into(),
            own: false,
        }
    }

    fn plugin() -> PluginId {
        PluginId::parse("dev.local.translate").expect("an id")
    }

    #[test]
    fn a_row_is_asked_once_per_plugin_version_and_text() {
        let mut cache = DecorateCache::default();
        let v1 = version_key("1.0.0", Some("aa"));
        assert_eq!(cache.look(&plugin(), v1, &row("m1", "hola")), Look::Ask);
        assert_eq!(
            cache.look(&plugin(), v1, &row("m1", "hola")),
            Look::Waiting,
            "asked and not answered yet"
        );
        cache.answered(&plugin(), "m1", None);
        assert_eq!(
            cache.look(&plugin(), v1, &row("m1", "hola")),
            Look::Answered(None),
            "an answer of nothing is kept too"
        );
        assert_eq!(cache.hits, 1);
        assert_eq!(
            cache.look(&plugin(), v1, &row("m1", "hola, editado")),
            Look::Ask,
            "an edit asks again"
        );
        cache.answered(&plugin(), "m1", None);
        let v2 = version_key("1.0.1", Some("bb"));
        assert_eq!(
            cache.look(&plugin(), v2, &row("m1", "hola, editado")),
            Look::Ask,
            "a new version asks again"
        );
    }

    #[test]
    fn a_settings_change_forgets_a_plugin_s_answers_and_no_one_else_s() {
        let mut cache = DecorateCache::default();
        let other = PluginId::parse("dev.local.other").expect("an id");
        let v = version_key("1.0.0", None);
        cache.look(&plugin(), v, &row("m1", "a"));
        cache.answered(&plugin(), "m1", None);
        cache.look(&other, v, &row("m1", "a"));
        cache.answered(&other, "m1", None);
        cache.forget(&plugin());
        assert_eq!(cache.look(&plugin(), v, &row("m1", "a")), Look::Ask);
        assert_eq!(cache.look(&other, v, &row("m1", "a")), Look::Answered(None));
    }

    #[test]
    fn the_cache_keeps_at_most_its_size() {
        let mut cache = DecorateCache::default();
        let v = version_key("1.0.0", None);
        for at in 0..DECORATE_CACHE + 100 {
            let message = format!("m{at}");
            cache.look(&plugin(), v, &row(&message, "x"));
            cache.answered(&plugin(), &message, None);
        }
        assert!(cache.len() <= DECORATE_CACHE, "{}", cache.len());
        assert_eq!(
            cache.look(&plugin(), v, &row("m0", "x")),
            Look::Ask,
            "the oldest went first"
        );
    }
}
