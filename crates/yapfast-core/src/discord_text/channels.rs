//! The open channel and those open lately, and who is typing in them.

use super::*;
use std::{collections::VecDeque, sync::Mutex};

/// Decoded previews shared by all recent channels. Active rows hold their
/// own handles; closing a channel leaves only this bounded cache owning pixels.
#[derive(Default)]
pub(super) struct PreviewCache {
    pictures: VecDeque<(String, Arc<Picture>)>,
    bytes: usize,
}

pub(super) const PREVIEW_CACHE_BYTES: usize = 8 * 1024 * 1024;
pub(super) type SharedPreviews = Arc<Mutex<PreviewCache>>;

impl PreviewCache {
    pub(super) fn get(&mut self, url: &str) -> Option<Arc<Picture>> {
        let index = self.pictures.iter().position(|(key, _)| key == url)?;
        let kept = self.pictures.remove(index).unwrap();
        let picture = kept.1.clone();
        self.pictures.push_back(kept);
        Some(picture)
    }

    pub(super) fn insert(&mut self, url: String, picture: Arc<Picture>) {
        if let Some(index) = self.pictures.iter().position(|(key, _)| key == &url) {
            self.bytes -= self.pictures.remove(index).unwrap().1.rgba.len();
        }
        let bytes = picture.rgba.len();
        if bytes > PREVIEW_CACHE_BYTES {
            return;
        }
        while self.bytes + bytes > PREVIEW_CACHE_BYTES {
            self.bytes -= self.pictures.pop_front().unwrap().1.rgba.len();
        }
        self.bytes += bytes;
        self.pictures.push_back((url, picture));
    }
}

/// The open channel and those open lately, kept so going back to one shows
/// it at once. Closed channels follow live events too, and the latest page
/// loads again when one opens, for what they missed.
#[derive(Default)]
pub struct Channels {
    pub(super) open: Option<OpenChannel>,
    /// A thread shown beside the open channel, kept as live as it.
    pub(super) aside: Option<OpenChannel>,
    /// Newest last.
    pub(super) recent: Vec<OpenChannel>,
    pub(super) previews: SharedPreviews,
    /// Who is typing in each channel, by user id, with their name and until when.
    pub(super) typing: HashMap<String, HashMap<String, (String, Instant)>>,
    /// Where this account last told Discord it's typing, and when.
    pub(super) typed: Option<(String, Instant)>,
    /// Counts member searches, so one typed over before it went is dropped.
    pub(super) searches: Arc<std::sync::atomic::AtomicU64>,
}

/// How long a member search waits for more typing before it goes to Discord.
pub(super) const SEARCH_PAUSE: Duration = Duration::from_millis(300);

impl Channels {
    /// Forgets who stopped typing in `room`, and says who still is.
    pub(super) fn typing_changed(&mut self, room: &str, events: &UnboundedSender<Event>) {
        let now = Instant::now();
        let mut names = Vec::new();
        if let Some(typing) = self.typing.get_mut(room) {
            typing.retain(|_, (_, until)| *until > now);
            names = typing.values().map(|(name, _)| name.clone()).collect();
            names.sort();
            if typing.is_empty() {
                self.typing.remove(room);
            }
        }
        let _ = events.send(Event::Typing {
            room: room.to_owned(),
            names,
        });
    }

    pub(super) fn close(&mut self) {
        let channel = self.open.take();
        self.keep(channel);
    }

    /// Stops showing the thread beside the open channel.
    pub(super) fn close_aside(&mut self) {
        let channel = self.aside.take();
        self.keep(channel);
    }

    /// Keeps a channel that closed among the recent ones.
    fn keep(&mut self, channel: Option<OpenChannel>) {
        if let Some(mut channel) = channel {
            channel.shelve();
            self.recent.push(channel);
            if self.recent.len() > KEPT_CHANNELS {
                let mut gone = self.recent.remove(0);
                save(gone.take_changes());
            }
        }
    }

    /// Applies a live event to the closed channels. Each keeps only its
    /// newest messages, as on closing; a busy one would otherwise grow for
    /// the whole session.
    pub(super) fn follow(&mut self, event: &Live) {
        for kept in &mut self.recent {
            if kept.live(event) {
                kept.trim();
            }
        }
    }

    /// Live events went by without them, so every channel loads its latest
    /// page again when it next opens.
    pub(super) fn missed(&mut self) {
        for channel in self
            .open
            .iter_mut()
            .chain(&mut self.aside)
            .chain(&mut self.recent)
        {
            channel.missed();
        }
    }

    /// Puts the newest messages of the channels that changed on disk.
    pub(super) fn save(&mut self) {
        for channel in self
            .open
            .iter_mut()
            .chain(&mut self.aside)
            .chain(&mut self.recent)
        {
            save(channel.take_changes());
        }
    }

    pub(super) fn reopen(&mut self, room: &str) -> Option<OpenChannel> {
        let index = self
            .recent
            .iter()
            .position(|channel| channel.room == room)?;
        Some(self.recent.remove(index))
    }

    pub(super) fn get_mut(&mut self, room: &str) -> Option<&mut OpenChannel> {
        self.open
            .iter_mut()
            .chain(&mut self.aside)
            .chain(&mut self.recent)
            .find(|channel| channel.room == room)
    }

    /// The open channel, or the thread beside it, if it's `room`.
    pub(super) fn shown_mut(&mut self, room: &str) -> Option<&mut OpenChannel> {
        self.open
            .iter_mut()
            .chain(&mut self.aside)
            .find(|channel| channel.room == room)
    }

    /// Whether `room` is on screen: the open channel or the thread beside it.
    pub(super) fn is_open(&self, room: &str) -> bool {
        self.open
            .iter()
            .chain(&self.aside)
            .any(|channel| channel.room == room)
    }
}

/// Writes a channel's messages to disk, off the async threads.
pub(super) fn save(saved: Option<(String, Vec<Message>)>) {
    if let Some((channel, messages)) = saved {
        tokio::task::spawn_blocking(move || {
            yapfast_discord::cache::save_messages(&channel, messages)
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn picture(bytes: usize) -> Arc<Picture> {
        Arc::new(Picture {
            rgba: crate::chat::Pixels::new(0, 0, vec![0; bytes]),
            ..Default::default()
        })
    }

    #[test]
    fn previews_evict_by_bytes_and_recent_use_without_invalidating_visible_handles() {
        let mut cache = PreviewCache::default();
        let first = picture(PREVIEW_CACHE_BYTES / 2);
        let weak = Arc::downgrade(&first);
        cache.insert("first".into(), first.clone());
        cache.insert("second".into(), picture(PREVIEW_CACHE_BYTES / 2));
        assert!(Arc::ptr_eq(&first, &cache.get("first").unwrap()));
        cache.insert("third".into(), picture(PREVIEW_CACHE_BYTES / 2));
        assert!(
            cache.get("second").is_none(),
            "least recently used leaves first"
        );
        cache.insert("fourth".into(), picture(PREVIEW_CACHE_BYTES / 2));
        assert!(cache.get("first").is_none());
        assert!(
            weak.upgrade().is_some(),
            "a visible row still owns its handle"
        );
        drop(first);
        assert!(
            weak.upgrade().is_none(),
            "eviction releases the cache's ownership"
        );
        assert_eq!(cache.bytes, PREVIEW_CACHE_BYTES);
        cache.insert("third".into(), picture(64));
        assert_eq!(cache.bytes, PREVIEW_CACHE_BYTES / 2 + 64);
        cache.insert("oversized".into(), picture(PREVIEW_CACHE_BYTES + 1));
        assert!(cache.get("oversized").is_none());
        assert_eq!(cache.bytes, PREVIEW_CACHE_BYTES / 2 + 64);
    }
}
