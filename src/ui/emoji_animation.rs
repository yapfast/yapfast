//! Animated emoji's frames, decoded ahead of the draw: what is wanted next,
//! the frames kept once they arrive, and the byte budget they are kept within.
//!
//! Message text draws an emoji from a glyph its layout made, so its frames
//! cannot be decoded while drawing without stalling the frame. The rows a
//! refresh builds name the animated emoji on screen instead, and those go to
//! the front of the queue while a worker elsewhere decodes them at the size
//! jumbo text draws. This module is only the bookkeeping around that, with no
//! thread, no timer and no window, so it is tested on its own.

use super::*;

/// Ten frames a second, for a frame whose animation gave it no delay. A GIF's
/// delays are read with its pixels, so this only covers a picture that lost
/// them.
const DEFAULT_DELAY: u32 = 100;

/// One animated emoji's decoded frames, ready for the registry.
///
/// The images share the pixel buffers the decoder filled, so Slint and the
/// renderer draw rather than copy them, and the bytes counted here are what
/// they cost whoever holds them.
pub(super) struct Frames {
    images: Vec<slint::Image>,
    delays: Vec<u32>,
    bytes: usize,
}

impl Frames {
    /// `frames` are RGBA, decoded at the size the rows draw them; `delays`
    /// are milliseconds.
    ///
    /// A frame the delays do not reach takes the last delay given, so every
    /// image has one and the clock walks the frames one by one.
    pub(super) fn new(
        frames: Vec<slint::SharedPixelBuffer<slint::Rgba8Pixel>>,
        delays: &[u32],
    ) -> Self {
        let bytes: usize = frames.iter().map(|frame| frame.as_bytes().len()).sum();
        let images: Vec<slint::Image> = frames.into_iter().map(slint::Image::from_rgba8).collect();
        let delays: Vec<u32> = (0..images.len())
            .map(|index| {
                delays
                    .get(index)
                    .copied()
                    .or(delays.last().copied())
                    .unwrap_or(DEFAULT_DELAY)
            })
            .collect();
        Self {
            images,
            delays,
            bytes,
        }
    }

    /// What the frames cost, as the pixels they are made of.
    pub(super) fn bytes(&self) -> usize {
        self.bytes
    }

    /// The frames themselves, in the order they play.
    pub(super) fn images(&self) -> &[slint::Image] {
        &self.images
    }

    /// How long each frame shows, in milliseconds and in the same order.
    pub(super) fn delays(&self) -> &[u32] {
        &self.delays
    }
}

/// The emoji whose frames are wanted, visible ones first.
pub(super) struct Job {
    /// The name the picker and the registry know the emoji by.
    pub key: String,
    /// The glyph message text draws it as, if the emoji is also in the font.
    pub glyph: Option<char>,
    /// The picture to decode, which is what makes two jobs one job.
    pub address: String,
}

/// Wanted frames, deduplicated by picture address, visible first.
///
/// Nothing here starts a decode; the worker takes jobs as it can, and a job
/// still waiting when the refresh no longer wants it is taken and dropped.
#[derive(Default)]
pub(super) struct Queue {
    waiting: Vec<Job>,
}

impl Queue {
    /// Wants an emoji's frames: at the back behind what is already waiting, or
    /// at the front when it was waiting already.
    ///
    /// The front is for a refresh naming the rows on screen again, which it
    /// does every time, so what is on screen is decoded before what is left.
    /// One emoji posted in forty messages is one job, by its picture's
    /// address, and one name is one job, by its key: the frames decoded once
    /// serve every row that draws them.
    ///
    /// A key whose frames failed is never passed here. The caller asks
    /// [`Cache::is_still`] first, because a picture that could not be read
    /// would fail the same way on every refresh.
    pub(super) fn want(&mut self, job: Job) {
        let waiting = self
            .waiting
            .iter()
            .position(|waiting| waiting.address == job.address || waiting.key == job.key);
        if let Some(index) = waiting {
            // The newest address for an emoji wins: its picture is decoded for
            // the name the rows just asked for. Moving it in front is what a
            // refresh naming its rows again amounts to.
            self.waiting.remove(index);
            self.waiting.insert(0, job);
        } else {
            self.waiting.push(job);
        }
    }

    /// Takes the emoji to decode next, which is the one wanted most recently.
    pub(super) fn take(&mut self) -> Option<Job> {
        if self.waiting.is_empty() {
            None
        } else {
            Some(self.waiting.remove(0))
        }
    }

    /// Forgets what was wanted, as everything wanted changed at once: another
    /// channel, or a frame size that leaves every decoded frame too small.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(super) fn clear(&mut self) {
        self.waiting.clear();
    }

    /// How many emoji are waiting to be decoded.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(super) fn len(&self) -> usize {
        self.waiting.len()
    }
}

/// Frames kept, by emoji key, within a byte budget.
pub(super) struct Cache {
    entries: HashMap<String, Kept>,
    /// Counts every showing, so the smallest count is the emoji shown longest
    /// ago: the whole of a least-recently-shown cache.
    shown: u64,
    /// The emoji a refresh showed, which eviction leaves alone.
    pinned: HashSet<String>,
    /// The emoji whose frames could not be decoded, so they are not asked for
    /// again on every refresh.
    stills: HashSet<String>,
    bytes: usize,
}

/// One emoji's frames, and when they last showed.
struct Kept {
    frames: Rc<Frames>,
    glyph: Option<char>,
    shown: u64,
}

impl Cache {
    pub(super) const BUDGET: usize = 6 * 1024 * 1024;

    pub(super) fn new() -> Self {
        Self {
            entries: HashMap::new(),
            shown: 0,
            pinned: HashSet::new(),
            stills: HashSet::new(),
            bytes: 0,
        }
    }

    /// Keeps frames that just arrived, shown from now on. Frames for a key
    /// already here replace them, which is what a new frame size amounts to.
    pub(super) fn insert(&mut self, key: String, glyph: Option<char>, frames: Rc<Frames>) {
        self.shown += 1;
        let shown = self.shown;
        self.bytes += frames.bytes();
        self.stills.remove(&key);
        let replaced = self.entries.insert(
            key,
            Kept {
                frames,
                glyph,
                shown,
            },
        );
        if let Some(replaced) = replaced {
            self.bytes -= replaced.frames.bytes();
        }
    }

    /// The frames kept for `key`, if any, from now the last thing shown.
    pub(super) fn get(&mut self, key: &str) -> Option<Rc<Frames>> {
        self.shown += 1;
        let shown = self.shown;
        let kept = self.entries.get_mut(key)?;
        kept.shown = shown;
        Some(kept.frames.clone())
    }

    /// Pins what a refresh just showed, so eviction never takes it.
    ///
    /// Every refresh pins the whole visible set, so this replaces the pinned
    /// set rather than adding to it. A pinned set larger than the budget
    /// stays: an emoji dropping back to its still while it is on screen is
    /// worse than the memory, and it is what a budget alone would do.
    pub(super) fn pin<'a>(&mut self, keys: impl IntoIterator<Item = &'a str>) {
        self.pinned.clear();
        self.pinned.extend(keys.into_iter().map(str::to_owned));
    }

    /// Drops the frames shown longest ago until the budget is met again,
    /// never one pinned.
    pub(super) fn evict(&mut self) {
        self.evict_to(Self::BUDGET);
    }

    /// The budget is a constant, so the tests lower it rather than fill
    /// megabytes of pictures to make eviction happen.
    fn evict_to(&mut self, budget: usize) {
        while self.bytes > budget {
            let Some(key) = self.longest_unshown() else {
                // Only pinned emoji are left, and they are never taken.
                return;
            };
            if let Some(taken) = self.entries.remove(&key) {
                // The renderer owns the same pixels through its registry.
                // Dropping our cache entry alone does not release any frames.
                i_slint_core::yapfast_emoji::remove(&key);
                self.bytes -= taken.frames.bytes();
            }
        }
    }

    /// The key shown longest ago, of those not pinned.
    fn longest_unshown(&self) -> Option<String> {
        self.entries
            .iter()
            .filter(|(key, _)| !self.pinned.contains(*key))
            .min_by_key(|(_, kept)| kept.shown)
            .map(|(key, _)| key.clone())
    }

    /// What the kept frames cost, for a report and for the tests.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(super) fn bytes(&self) -> usize {
        self.bytes
    }

    /// Whether frames are kept for `key`, without counting as a showing.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(super) fn contains(&self, key: &str) -> bool {
        self.entries.contains_key(key)
    }

    /// The glyph an emoji's frames draw as, for a pass that registers what is
    /// kept once more, as a new frame size makes it do.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(super) fn glyph(&self, key: &str) -> Option<char> {
        self.entries.get(key).and_then(|kept| kept.glyph)
    }

    /// Remembers that an emoji's frames could not be decoded, so every refresh
    /// until they do is spared asking again.
    pub(super) fn still(&mut self, key: &str) {
        self.stills.insert(key.to_owned());
    }

    /// Whether an emoji's frames are known not to decode. A still costs no
    /// bytes and is not evicted: the pictures it names are few, and forgetting
    /// one would only retry it.
    pub(super) fn is_still(&self, key: &str) -> bool {
        self.stills.contains(key)
    }
}

impl Drop for Cache {
    fn drop(&mut self) {
        for key in self.entries.keys() {
            i_slint_core::yapfast_emoji::remove(key);
        }
    }
}

/// One emoji's frames on their way back from the decoder.
struct Decoded {
    key: String,
    glyph: Option<char>,
    frames: Vec<Vec<u8>>,
    delays: Vec<u32>,
}

/// How many decodes run at once: one, so the rows and the picker keep their
/// cores and the emoji a refresh shows next do not queue behind a long one.
const DECODING_AT_ONCE: usize = 1;

/// The animated emoji on screen: what they need decoded, what is kept, and the
/// clock they all draw from.
pub(super) struct Animations {
    cache: Cache,
    queue: Queue,
    /// The picker's current ready rows, replaced rather than accumulated as
    /// it scrolls. Message emoji are tracked separately by the cache's pins.
    picker: HashSet<String>,
    /// The emoji whose frames a thread is decoding now.
    decoding: std::collections::HashSet<String>,
    done: std::sync::mpsc::Sender<Decoded>,
    decoded: std::sync::mpsc::Receiver<Decoded>,
    /// The side frames decode at: 48px as jumbo text draws them, for the
    /// screen's scale, and never past 96.
    side: u32,
    /// Moves the clock while anything plays; stopped otherwise, so a window
    /// with no animated emoji on screen has no wakeups.
    timer: slint::Timer,
    /// How often the clock moves, for the fastest animation on screen.
    interval: u64,
}

impl Animations {
    pub(super) fn new() -> Self {
        let (done, decoded) = std::sync::mpsc::channel();
        Self {
            cache: Cache::new(),
            queue: Queue::default(),
            picker: HashSet::new(),
            decoding: std::collections::HashSet::new(),
            done,
            decoded,
            side: 48,
            timer: slint::Timer::default(),
            interval: 0,
        }
    }

    /// Names the animated emoji a refresh shows: those kept are pinned so
    /// eviction leaves them, and those missing are wanted for decoding.
    pub(super) fn show(
        &mut self,
        shown: impl IntoIterator<Item = (String, Option<char>, String)>,
        pictures: &HashMap<String, Arc<Picture>>,
    ) {
        let mut keys: Vec<String> = Vec::new();
        let mut wanted: Vec<(String, Option<char>, String)> = Vec::new();
        for (key, glyph, address) in shown {
            // The font may have been made after the frames were kept, or made
            // again since, so run of text that draws this emoji is named here.
            if let Some(glyph) = glyph {
                i_slint_core::yapfast_emoji::set_glyph(&key, glyph);
            }
            if self.cache.get(&key).is_some()
                || self.cache.is_still(&key)
                || self.decoding.contains(&key)
            {
                keys.push(key);
                continue;
            }
            if !pictures.contains_key(&address) {
                // Its picture has not arrived; the still shows, and the next
                // refresh wants it again once it has.
                keys.push(key);
                continue;
            }
            keys.push(key.clone());
            wanted.push((key, glyph, address));
        }
        self.cache.pin(keys.iter().map(String::as_str));
        self.cache.evict();
        for (key, glyph, address) in wanted {
            self.queue.want(Job {
                key,
                glyph,
                address,
            });
        }
        self.prune_waiting();
    }

    /// Wants frames for emoji shown somewhere that pins nothing, such as the
    /// picker's visible cells: they scroll away, so they are decoded in turn
    /// but never hold another emoji's place in the cache.
    pub(super) fn also(
        &mut self,
        shown: impl IntoIterator<Item = (String, Option<char>, String)>,
        pictures: &HashMap<String, Arc<Picture>>,
    ) {
        self.picker.clear();
        for (key, glyph, address) in shown {
            self.picker.insert(key.clone());
            if self.cache.get(&key).is_some()
                || self.cache.is_still(&key)
                || self.decoding.contains(&key)
                || !pictures.contains_key(&address)
            {
                continue;
            }
            self.queue.want(Job {
                key,
                glyph,
                address,
            });
        }
        self.prune_waiting();
    }

    /// Queued work and late results belong only to messages or to the
    /// picker's current rows, not every row it has ever passed.
    fn prune_waiting(&mut self) {
        let (messages, picker) = (&self.cache.pinned, &self.picker);
        self.queue
            .waiting
            .retain(|job| messages.contains(&job.key) || picker.contains(&job.key));
    }

    /// The picker closed. Its unpinned frames must leave the renderer too,
    /// and a decoder already running must not register them again later.
    /// Whether the picker's ready rows animate `key`.
    pub(super) fn animates_in_picker(&self, key: &str) -> bool {
        self.picker.contains(key)
    }

    pub(super) fn hide_picker(&mut self) {
        self.picker.clear();
        self.prune_waiting();
        self.cache.evict_to(0);
    }

    /// Starts decoding what is wanted, within the budget of work in flight.
    pub(super) fn start(&mut self, pictures: &HashMap<String, Arc<Picture>>) {
        while self.decoding.len() < DECODING_AT_ONCE {
            let Some(job) = self.queue.take() else {
                break;
            };
            let Some(picture) = pictures.get(&job.address) else {
                continue;
            };
            let (picture, side, done) = (picture.clone(), self.side, self.done.clone());
            self.decoding.insert(job.key.clone());
            std::thread::spawn(move || {
                let decoded = decode(&picture, side);
                let _ = done.send(Decoded {
                    key: job.key,
                    glyph: job.glyph,
                    frames: decoded.0,
                    delays: decoded.1,
                });
            });
        }
    }

    /// Keeps the frames that arrived, and gives the registry each emoji's
    /// animation, so text and the picker draw them from now on.
    pub(super) fn drain(&mut self) {
        while let Ok(decoded) = self.decoded.try_recv() {
            self.decoding.remove(&decoded.key);
            if !self.cache.pinned.contains(&decoded.key) && !self.picker.contains(&decoded.key) {
                continue;
            }
            if decoded.frames.is_empty() {
                self.cache.still(&decoded.key);
                continue;
            }
            let mut buffers = Vec::with_capacity(decoded.frames.len());
            for frame in decoded.frames {
                let mut buffer =
                    slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(self.side, self.side);
                let bytes = buffer.make_mut_bytes();
                let count = bytes.len().min(frame.len());
                bytes[..count].copy_from_slice(&frame[..count]);
                buffers.push(buffer);
            }
            let frames = Rc::new(Frames::new(buffers, &decoded.delays));
            let animation = Rc::new(i_slint_core::yapfast_emoji::Animation::new(
                frames.images().to_vec(),
                frames.delays(),
            ));
            i_slint_core::yapfast_emoji::insert(&decoded.key, decoded.glyph, animation);
            self.cache.insert(decoded.key, decoded.glyph, frames);
            self.cache.evict();
        }
    }

    /// Takes what arrived from the decoders, starts what is wanted next, and
    /// draws what shows a frame again. Called each tick and each refresh, so a
    /// decode that finishes is seen at once rather than at the next change.
    pub(super) fn run(&mut self, window: &YapfastWindow, pictures: &HashMap<String, Arc<Picture>>) {
        self.start(pictures);
        self.drain();
        if self.decoding.is_empty()
            && self.queue.len() == 0
            && i_slint_core::yapfast_emoji::is_empty()
        {
            self.timer.stop();
            self.interval = 0;
        }
        // Only a frame that changed, or one that came or went, draws again:
        // the clock ticks faster than most frames change, and each draw walks
        // the whole window.
        let revision = i_slint_core::yapfast_emoji::revision() as i32;
        let global = window.global::<EmojiAnimations>();
        if global.get_revision() != revision {
            global.set_revision(revision);
            window.window().request_redraw();
        }
    }

    /// Sets whether the clock may move, and starts or stops the timer: it runs
    /// while anything animates, or while frames are on their way, and it is
    /// the clock itself that holds still when playback is off.
    pub(super) fn sync(
        &mut self,
        window: &YapfastWindow,
        playing: bool,
        adapter: std::rc::Weak<RefCell<Adapter>>,
    ) {
        i_slint_core::yapfast_emoji::set_playing(playing);
        self.side = frame_side(window.window().scale_factor());
        let waiting = !self.decoding.is_empty()
            || self.queue.len() > 0
            || !i_slint_core::yapfast_emoji::is_empty();
        if !waiting {
            self.timer.stop();
            self.interval = 0;
            return;
        }
        // A frame no faster than 16ms and no slower than 50 keeps the clock off
        // a busy loop while still following a slow animation's own pace; with
        // nothing registered yet, it only looks for what arrives.
        let delay = i_slint_core::yapfast_emoji::shortest_delay().unwrap_or(30);
        let interval = u64::from(delay.clamp(16, 50));
        if self.timer.running() && self.interval == interval {
            return;
        }
        self.interval = interval;
        let (weak, clock) = (
            window.as_weak(),
            Rc::new(Cell::new(std::time::Instant::now())),
        );
        self.timer.start(
            slint::TimerMode::Repeated,
            std::time::Duration::from_millis(interval),
            move || {
                let now = std::time::Instant::now();
                let delta = now.saturating_duration_since(clock.replace(now));
                i_slint_core::yapfast_emoji::advance(delta);
                let (Some(adapter), Some(window)) = (adapter.upgrade(), weak.upgrade()) else {
                    return;
                };
                let mut adapter = adapter.borrow_mut();
                let Adapter {
                    app, animations, ..
                } = &mut *adapter;
                animations.run(&window, &app.state.pictures);
            },
        );
    }
}

/// The side frames decode at: 48px, as jumbo text draws an emoji, for the
/// screen's scale, and never past the 96 that keeps the cache's budget useful.
fn frame_side(scale: f32) -> u32 {
    (48.0 * scale.max(1.0)).round().clamp(48.0, 96.0) as u32
}

/// An animation's frames at `side`, and how long each shows, decoded whole and
/// scaled at once: the picture's own frames are as large as its file.
fn decode(picture: &Picture, side: u32) -> (Vec<Vec<u8>>, Vec<u32>) {
    use image::imageops;
    let delays = picture
        .animation
        .as_ref()
        .map(|animation| animation.delays.clone())
        .unwrap_or_default();
    let frames = picture
        .frames()
        .filter_map(|frame| {
            let decoded = image::RgbaImage::from_raw(picture.width, picture.height, frame)?;
            let scaled = imageops::resize(
                &crate::emoji_text::premultiplied(&decoded),
                side,
                side,
                imageops::FilterType::Triangle,
            );
            Some(crate::emoji_text::unpremultiplied(&scaled).into_raw())
        })
        .collect();
    (frames, delays)
}

#[test]
fn frames_reach_the_registry() {
    use yapfast_core::chat;
    // A GIF of two frames, as a Discord emoji's picture is.
    let mut data = Vec::new();
    {
        let mut encoder = image::codecs::gif::GifEncoder::new(&mut data);
        for index in 0..2u8 {
            let shade = image::Rgba([index.wrapping_mul(200), 0, 0, 255]);
            let frame = image::Frame::from_parts(
                image::RgbaImage::from_pixel(8, 8, shade),
                0,
                0,
                image::Delay::from_numer_denom_ms(50, 1),
            );
            encoder.encode_frame(frame).unwrap();
        }
    }
    let picture = Arc::new(chat::decode_picture(&data, 128, 128).expect("a picture"));
    assert!(picture.animation.is_some(), "a GIF animates");

    let (key, glyph) = ("frames-reach:1", '\u{10ffe1}');
    let mut animations = Animations::new();
    animations.side = 16;
    let pictures = HashMap::from([("pic".to_owned(), picture)]);
    animations.show(
        vec![(key.to_owned(), Some(glyph), "pic".to_owned())],
        &pictures,
    );
    animations.start(&pictures);
    // The worker decodes on its own thread; the tick waits for it the same way.
    for _ in 0..200 {
        animations.drain();
        if i_slint_core::yapfast_emoji::frame_of_glyph(glyph).is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(
        i_slint_core::yapfast_emoji::frame_of_glyph(glyph).is_some(),
        "the frames reached the registry"
    );
    i_slint_core::yapfast_emoji::remove(key);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A decoded picture as the decoders hand them over, `side` pixels a side.
    fn frame(side: u32) -> slint::SharedPixelBuffer<slint::Rgba8Pixel> {
        slint::SharedPixelBuffer::new(side, side)
    }

    /// One emoji's frames: `count` pictures of 2 by 2 pixels, 16 bytes each,
    /// small enough that a test fills a budget of its own choosing.
    fn small(count: usize) -> Rc<Frames> {
        let delays = vec![40; count];
        Rc::new(Frames::new((0..count).map(|_| frame(2)).collect(), &delays))
    }

    fn job(key: &str, address: &str) -> Job {
        Job {
            key: key.to_owned(),
            glyph: Some('a'),
            address: address.to_owned(),
        }
    }

    #[test]
    fn one_picture_is_decoded_once_however_many_rows_draw_it() {
        let mut queue = Queue::default();
        queue.want(job("party:1", "https://cdn/1.gif"));
        queue.want(job("party:1", "https://cdn/1.gif"));
        assert_eq!(queue.len(), 1, "the same emoji posted twice");
        queue.want(job("wave:2", "https://cdn/1.gif"));
        assert_eq!(queue.len(), 1, "one picture, whatever it is called");
        assert_eq!(queue.take().map(|job| job.key).as_deref(), Some("wave:2"));
        assert_eq!(queue.len(), 0, "taken");
        assert!(queue.take().is_none(), "nothing left to take");
    }

    #[test]
    fn re_wanting_an_emoji_moves_it_ahead_of_the_rest() {
        let mut queue = Queue::default();
        queue.want(job("a", "1"));
        queue.want(job("b", "2"));
        queue.want(job("c", "3"));
        // A refresh names the rows on screen again, and they are decoded first.
        queue.want(job("c", "3"));
        let order: Vec<String> = std::iter::from_fn(|| queue.take())
            .map(|job| job.key)
            .collect();
        assert_eq!(order, ["c", "a", "b"]);
        queue.want(job("d", "4"));
        queue.clear();
        assert_eq!(queue.len(), 0);
    }

    #[test]
    fn eviction_takes_the_frames_shown_longest_ago() {
        let mut cache = Cache::new();
        for key in ["a", "b", "c"] {
            cache.insert(key.to_owned(), Some('a'), small(1));
        }
        assert_eq!(cache.bytes(), 48, "three frames of 2 by 2 pixels");
        // Showing a and c again leaves b the one not shown for longest.
        assert!(cache.get("a").is_some());
        assert!(cache.get("c").is_some());
        cache.evict_to(32);
        assert!(!cache.contains("b"), "shown longest ago");
        assert!(cache.contains("a") && cache.contains("c"));
        assert_eq!(cache.bytes(), 32);
        cache.evict_to(16);
        assert!(!cache.contains("a"), "shown before c");
        assert!(cache.contains("c"), "shown last, so kept");
        assert_eq!(cache.bytes(), 16);
    }

    #[test]
    fn eviction_releases_the_renderers_animation_too() {
        let mut cache = Cache::new();
        let key = "eviction-releases-registry:1";
        let frames = small(1);
        let animation = Rc::new(i_slint_core::yapfast_emoji::Animation::new(
            frames.images().to_vec(),
            frames.delays(),
        ));
        let retained = Rc::downgrade(&animation);
        i_slint_core::yapfast_emoji::insert(key, Some('\u{10ffe2}'), animation);
        cache.insert(key.into(), Some('\u{10ffe2}'), frames);
        cache.evict_to(0);
        let still_retained = retained.upgrade().is_some();
        // Clean up even on the broken implementation, before asserting.
        i_slint_core::yapfast_emoji::remove(key);
        assert!(
            !still_retained,
            "eviction left the renderer owning the animation"
        );
        assert!(i_slint_core::yapfast_emoji::frame_of_glyph('\u{10ffe2}').is_none());
    }

    #[test]
    fn scrolling_animation_frames_stays_within_the_real_pixel_budget() {
        let mut animations = Animations::new();
        animations.side = 96;
        let keys: Vec<String> = (0..64).map(|n| format!("scroll-memory:{n}")).collect();
        let bytes_per_animation = 16 * 96 * 96 * 4;
        for key in &keys {
            animations.picker = HashSet::from([key.clone()]);
            animations
                .done
                .send(Decoded {
                    key: key.clone(),
                    glyph: None,
                    frames: vec![vec![0; 96 * 96 * 4]; 16],
                    delays: vec![50; 16],
                })
                .unwrap();
            animations.drain();
        }
        let registered = keys
            .iter()
            .filter(|key| i_slint_core::yapfast_emoji::frame_of_key(key).is_some())
            .count();
        let retained_bytes = registered * bytes_per_animation;
        eprintln!(
            "animation pixels: cache={} bytes; renderer={} bytes; scrolled={} bytes",
            animations.cache.bytes(),
            retained_bytes,
            keys.len() * bytes_per_animation
        );
        animations.hide_picker();
        let after_close = keys
            .iter()
            .filter(|key| i_slint_core::yapfast_emoji::frame_of_key(key).is_some())
            .count()
            * bytes_per_animation;
        eprintln!("animation pixels after closing: {after_close} bytes");
        for key in &keys {
            i_slint_core::yapfast_emoji::remove(key);
        }
        assert!(
            retained_bytes <= Cache::BUDGET,
            "renderer retained {retained_bytes} bytes despite the {} byte cache budget",
            Cache::BUDGET
        );
        assert_eq!(
            after_close, 0,
            "a closed picker retains no unpinned animation pixels"
        );
    }

    /// Run alone under an external process-memory sampler. The phase pauses
    /// let it read the process without adding a platform-specific allocator or
    /// profiler to the app. Unlike the unit regression above, this processes
    /// 288 MiB of frame pixels to expose growth across a large scroll.
    #[test]
    #[ignore = "manual process-memory probe; run alone with --nocapture"]
    fn animation_scroll_process_memory_probe() {
        use std::io::Write;
        let phase = |name: &str| {
            println!("emoji-memory-phase:{name}");
            std::io::stdout().flush().unwrap();
            std::thread::sleep(Duration::from_millis(500));
        };
        let mut animations = Animations::new();
        animations.side = 96;
        phase("before");
        for index in 0..512 {
            let key = format!("process-memory-probe:{index}");
            animations.picker = HashSet::from([key.clone()]);
            animations
                .done
                .send(Decoded {
                    key,
                    glyph: None,
                    frames: vec![vec![0; 96 * 96 * 4]; 16],
                    delays: vec![50; 16],
                })
                .unwrap();
            animations.drain();
        }
        phase("after-scroll");
        animations.hide_picker();
        phase("after-close");
        trim();
        phase("after-trim");
        assert_eq!(animations.cache.bytes(), 0);
        assert!(i_slint_core::yapfast_emoji::is_empty());
    }

    #[test]
    fn closing_the_picker_discards_late_frames_and_preserves_message_animations() {
        let mut animations = Animations::new();
        animations.side = 2;
        let message = "close-keeps-message:1";
        let picker = "close-drops-picker:2";
        let pictures = HashMap::new();
        animations.show([(message.into(), None, "message.png".into())], &pictures);
        animations.also([(picker.into(), None, "picker.gif".into())], &pictures);
        for key in [message, picker] {
            animations
                .done
                .send(Decoded {
                    key: key.into(),
                    glyph: None,
                    frames: vec![vec![0; 16]],
                    delays: vec![50],
                })
                .unwrap();
            animations.queue.want(job(key, key));
        }
        animations.drain();
        assert!(i_slint_core::yapfast_emoji::frame_of_key(picker).is_some());
        animations.hide_picker();
        assert!(i_slint_core::yapfast_emoji::frame_of_key(picker).is_none());
        assert!(i_slint_core::yapfast_emoji::frame_of_key(message).is_some());
        assert_eq!(animations.queue.len(), 1);
        assert_eq!(animations.queue.take().unwrap().key, message);
        // A decoder already running at close cannot put the frames back.
        animations.decoding.insert(picker.into());
        animations
            .done
            .send(Decoded {
                key: picker.into(),
                glyph: None,
                frames: vec![vec![0; 16]],
                delays: vec![50],
            })
            .unwrap();
        animations.drain();
        assert!(animations.decoding.is_empty());
        assert!(i_slint_core::yapfast_emoji::frame_of_key(picker).is_none());
        assert_eq!(animations.cache.bytes(), 16);
        drop(animations);
        assert!(i_slint_core::yapfast_emoji::frame_of_key(message).is_none());
    }

    #[test]
    #[cfg(feature = "discord")]
    fn quiet_picker_updates_restart_animation_processing_after_close() {
        use yapfast_discord::{
            marked::MarkedServer,
            ready::{Emoji, Server},
            service::Snapshot,
        };
        i_slint_backend_testing::init_no_event_loop();
        let directory = tempfile::tempdir().unwrap();
        let window = YapfastWindow::new().unwrap();
        let adapter = Adapter::new(
            Arc::new(BackgroundRuntime::new().unwrap()),
            directory.path().into(),
            &window,
        )
        .unwrap();
        let mut adapter = adapter.borrow_mut();
        adapter.app.apply(Event::Discord(Arc::new(Snapshot {
            servers: vec![MarkedServer::from(Server {
                id: "10".into(),
                name: "Test".into(),
                emojis: vec![Emoji {
                    id: "123".into(),
                    name: "restart".into(),
                    animated: true,
                    available: true,
                }]
                .into(),
                ..Default::default()
            })],
            ..Default::default()
        })));
        let url = adapter.app.emoji_by_id(&["123"].into_iter().collect())[0]
            .picture
            .clone();
        let mut data = Vec::new();
        {
            let mut encoder = image::codecs::gif::GifEncoder::new(&mut data);
            for _ in 0..2 {
                encoder
                    .encode_frame(image::Frame::from_parts(
                        image::RgbaImage::from_pixel(2, 2, image::Rgba([255, 0, 0, 255])),
                        0,
                        0,
                        image::Delay::from_numer_denom_ms(50, 1),
                    ))
                    .unwrap();
            }
        }
        adapter.app.state.pictures.insert(
            url,
            Arc::new(yapfast_core::chat::decode_picture(&data, 2, 2).unwrap()),
        );
        adapter.models.emoji_rows.set_vec(vec![EmojiRow {
            cells: ModelRc::new(VecModel::from(vec![emoji::cell("restart:123")])),
            ..Default::default()
        }]);
        assert!(!adapter.animations.timer.running());
        // No full refresh: the same helper handles scroll and picture arrivals.
        super::super::callbacks::want_shown_animations(&mut adapter, &window);
        assert!(adapter.animations.timer.running());
        assert!(!adapter.animations.decoding.is_empty());
        adapter.animations.hide_picker();
        for _ in 0..200 {
            adapter.run_emoji_animations(&window);
            if adapter.animations.decoding.is_empty() {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(adapter.animations.decoding.is_empty());
        assert!(!adapter.animations.timer.running());
        super::super::callbacks::want_shown_animations(&mut adapter, &window);
        assert!(
            adapter.animations.timer.running(),
            "reopening restarts the timer without a refresh"
        );
        assert!(!adapter.animations.decoding.is_empty());
    }

    #[test]
    fn scrolling_to_rows_without_animations_discards_obsolete_work() {
        let mut animations = Animations::new();
        let pictures = HashMap::new();
        let key = "scroll-left-animation:1";
        animations.also([(key.into(), None, "animation.gif".into())], &pictures);
        animations.queue.want(job(key, "animation.gif"));
        animations.also(std::iter::empty(), &pictures);
        assert_eq!(animations.queue.len(), 0);
        animations
            .done
            .send(Decoded {
                key: key.into(),
                glyph: None,
                frames: vec![vec![0; 16]],
                delays: vec![50],
            })
            .unwrap();
        animations.drain();
        assert_eq!(animations.cache.bytes(), 0);
        assert!(i_slint_core::yapfast_emoji::frame_of_key(key).is_none());
    }

    #[test]
    fn a_pinned_emoji_stays_even_over_the_budget() {
        let mut cache = Cache::new();
        cache.insert("a".to_owned(), Some('a'), small(1));
        cache.insert("b".to_owned(), Some('b'), small(1));
        cache.pin(["b"]);
        cache.evict_to(0);
        assert!(cache.contains("b"), "on screen, so it keeps its frames");
        assert!(!cache.contains("a"));
        assert_eq!(cache.bytes(), 16, "over the budget, and pinned");
        // A refresh pins what it showed, so the pinned set replaces the last.
        cache.pin(["a"]);
        cache.evict_to(0);
        assert!(!cache.contains("b"));
        assert_eq!(cache.bytes(), 0);
    }

    #[test]
    fn bytes_are_the_pixels_kept_and_replacing_them_counts_once() {
        assert_eq!(Frames::new(vec![frame(4)], &[40]).bytes(), 64);
        let mut cache = Cache::new();
        assert_eq!(cache.bytes(), 0);
        cache.insert("a".to_owned(), Some('a'), small(3));
        assert_eq!(cache.bytes(), 48);
        cache.insert("a".to_owned(), Some('a'), small(1));
        assert_eq!(cache.bytes(), 16, "replaced rather than added");
        assert!(cache.get("a").is_some());
        assert_eq!(cache.bytes(), 16, "showing costs nothing");
        assert_eq!(cache.glyph("a"), Some('a'));
        assert_eq!(cache.glyph("gone"), None);
    }

    #[test]
    fn an_emoji_that_failed_stays_still_until_frames_arrive() {
        let mut cache = Cache::new();
        assert!(!cache.is_still("party:1"), "not known to fail yet");
        cache.still("party:1");
        assert!(cache.is_still("party:1"), "the caller skips wanting it");
        assert_eq!(cache.bytes(), 0, "a still is remembered, not kept");
        // Frames that arrive on a later try are what it draws from then on.
        cache.insert("party:1".to_owned(), Some('a'), small(1));
        assert!(!cache.is_still("party:1"));
    }

    #[test]
    fn every_image_has_a_delay_to_show_for() {
        let frames = Frames::new(vec![frame(2), frame(2), frame(2)], &[40, 60]);
        assert_eq!(frames.images().len(), 3);
        assert_eq!(frames.delays(), &[40, 60, 60], "the last delay repeats");
        let nothing = Frames::new(vec![frame(2)], &[]);
        assert_eq!(nothing.delays(), &[DEFAULT_DELAY]);
        let more = Frames::new(vec![frame(2)], &[40, 60]);
        assert_eq!(more.delays(), &[40], "a frame there is not, is not kept");
    }
}
