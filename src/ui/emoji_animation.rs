//! Animated emoji's frames, decoded ahead of the draw: what is wanted next,
//! the frames kept once they arrive, and the byte budget they are kept within.
//!
//! Message text draws an emoji from a glyph its layout made, so its frames
//! cannot be decoded while drawing without stalling the frame. The rows a
//! refresh builds name the animated emoji on screen instead, and those go to
//! the front of the queue while a worker elsewhere decodes them at the size
//! jumbo text draws. [`Queue`] and [`Cache`] are only the bookkeeping around
//! that, with no thread, no timer and no window, so they are tested on their
//! own; [`Animations`] has the decoder's thread and the timer that moves the
//! clock the frames are drawn by.

use std::time::Instant;

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
    /// The emoji a refresh showed, which eviction leaves alone, each with
    /// whether it is in view: in a message on screen, rather than in a bar or
    /// a menu that may not be open.
    pinned: HashMap<String, bool>,
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
            pinned: HashMap::new(),
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

    /// Lets go of every pin, as a refresh does before it pins what it shows:
    /// each refresh names the whole visible set, so the pinned set is
    /// replaced rather than added to.
    pub(super) fn unpin(&mut self) {
        self.pinned.clear();
    }

    /// Pins an emoji a refresh just showed, so eviction never takes it, with
    /// whether it is in view. The key is the one the refresh made, not a
    /// copy of it.
    ///
    /// A pinned set larger than the budget stays: an emoji dropping back to
    /// its still while it is on screen is worse than the memory, and it is
    /// what a budget alone would do.
    pub(super) fn pin(&mut self, key: String, in_view: bool) {
        // In a message and in the hover bar at once, it is in view.
        *self.pinned.entry(key).or_default() |= in_view;
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
            .filter(|(key, _)| !self.pinned.contains_key(*key))
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

/// How often the timer looks for frames a decoder has on the way, in
/// milliseconds.
const POLL: u32 = 30;

/// The shortest the timer waits, in milliseconds: a frame of a 60 Hz screen.
const SHORTEST: u32 = 16;

/// An emoji a refresh names: its key, the glyph message text draws it as, and
/// its picture's address, which is copied only for one whose frames are
/// wanted.
pub(super) type Shown<'a> = (String, Option<char>, &'a str);

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
    /// Wakes for the next frame of what shows, and for frames on their way
    /// from a decoder; stopped otherwise, so a window with no animated emoji
    /// on screen has no wakeups.
    timer: slint::Timer,
    /// How long the timer waits, in milliseconds; 0 while it is stopped.
    interval: u64,
    /// Until when the registry's clock has been moved; none while the timer
    /// is stopped, when no time passes for the animations. The clock moves by
    /// what passed since, whenever the timer fires or is set again, so the
    /// part of a frame already waited is never lost.
    caught_up: Option<Instant>,
    /// The registry knows whom to wake when a draw shows an emoji that plays.
    wakes: bool,
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
            caught_up: None,
            wakes: false,
        }
    }

    /// Names the animated emoji a refresh shows: those kept are pinned so
    /// eviction leaves them, and those missing are wanted for decoding.
    ///
    /// `in_view` are in the messages on screen, and play from now on. `kept`
    /// are in a bar, a menu or a card that may not be showing: their frames
    /// are as ready, and they play while something draws them, which the
    /// registry sees for itself.
    pub(super) fn show<'a>(
        &mut self,
        in_view: impl IntoIterator<Item = Shown<'a>>,
        kept: impl IntoIterator<Item = Shown<'a>>,
        pictures: &HashMap<String, Arc<Picture>>,
    ) {
        let in_view = in_view.into_iter().map(|shown| (shown, true));
        let kept = kept.into_iter().map(|shown| (shown, false));
        self.cache.unpin();
        for ((key, glyph, address), in_view) in in_view.chain(kept) {
            // The font may have been made after the frames were kept, or made
            // again since, so the glyph that draws this emoji is named here.
            if let Some(glyph) = glyph {
                i_slint_core::yapfast_emoji::set_glyph(&key, glyph);
            }
            // One whose picture has not arrived shows its still, and the
            // next refresh wants it again once it has.
            if self.cache.get(&key).is_none()
                && !self.cache.is_still(&key)
                && !self.decoding.contains(&key)
                && pictures.contains_key(address)
            {
                self.queue.want(Job {
                    key: key.clone(),
                    glyph,
                    address: address.to_owned(),
                });
            }
            self.cache.pin(key, in_view);
        }
        self.cache.evict();
        self.prune_waiting();
        self.name_in_view();
    }

    /// Wants frames for emoji shown somewhere that pins nothing, such as the
    /// picker's visible cells: they scroll away, so they are decoded in turn
    /// but never hold another emoji's place in the cache. They play while
    /// their cells draw them, as what is kept does.
    pub(super) fn also<'a>(
        &mut self,
        shown: impl IntoIterator<Item = Shown<'a>>,
        pictures: &HashMap<String, Arc<Picture>>,
    ) {
        self.picker.clear();
        for (key, glyph, address) in shown {
            if self.cache.get(&key).is_none()
                && !self.cache.is_still(&key)
                && !self.decoding.contains(&key)
                && pictures.contains_key(address)
            {
                self.queue.want(Job {
                    key: key.clone(),
                    glyph,
                    address: address.to_owned(),
                });
            }
            self.picker.insert(key);
        }
        self.prune_waiting();
    }

    /// Queued work and late results belong only to messages or to the
    /// picker's current rows, not every row it has ever passed.
    fn prune_waiting(&mut self) {
        let (pinned, picker) = (&self.cache.pinned, &self.picker);
        self.queue
            .waiting
            .retain(|job| pinned.contains_key(&job.key) || picker.contains(&job.key));
    }

    /// Tells the registry which of its emoji are in the messages on screen,
    /// by answering for each it keeps: no set is built and no key copied,
    /// on a path every refresh and every scroll of the timeline takes.
    fn name_in_view(&self) {
        let pinned = &self.cache.pinned;
        i_slint_core::yapfast_emoji::set_in_view(|key| pinned.get(key) == Some(&true));
    }

    /// Whether the picker's ready rows animate `key`.
    pub(super) fn animates_in_picker(&self, key: &str) -> bool {
        self.picker.contains(key)
    }

    /// The picker closed. Its unpinned frames must leave the renderer too,
    /// and a decoder already running must not register them again later.
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
        let mut arrived = false;
        while let Ok(decoded) = self.decoded.try_recv() {
            self.decoding.remove(&decoded.key);
            let in_view = self.cache.pinned.get(&decoded.key).copied();
            if in_view.is_none() && !self.picker.contains(&decoded.key) {
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
            arrived = true;
        }
        if arrived {
            // The registry takes what it is given as in view until told.
            self.name_in_view();
        }
    }

    /// Moves the clock by the time that passed, takes what arrived from the
    /// decoders, starts what is wanted next, and draws what shows a frame
    /// again. Called each tick and each refresh, so a decode that finishes is
    /// seen at once rather than at the next change.
    pub(super) fn run(
        &mut self,
        window: &YapfastWindow,
        pictures: &HashMap<String, Arc<Picture>>,
        now: Instant,
    ) {
        self.catch_up(now);
        self.start(pictures);
        self.drain();
        // Only a frame that changed, or one that came or went, draws again:
        // the timer also wakes for decoders, and each draw walks the window.
        let revision = i_slint_core::yapfast_emoji::revision() as i32;
        let global = window.global::<EmojiAnimations>();
        if global.get_revision() != revision {
            global.set_revision(revision);
            window.window().request_redraw();
        }
    }

    /// Moves the registry's clock by the whole milliseconds since it was last
    /// moved, and leaves the rest of a millisecond for the next time.
    fn catch_up(&mut self, now: Instant) {
        let Some(then) = self.caught_up else {
            return;
        };
        let passed = now.saturating_duration_since(then).as_millis();
        let passed = Duration::from_millis(u64::try_from(passed).unwrap_or(u64::MAX));
        if !passed.is_zero() {
            i_slint_core::yapfast_emoji::advance(passed);
            self.caught_up = Some(then + passed);
        }
    }

    /// Sets whether the clock may move, and has the timer wake for what is
    /// next: the frame after the one that shows, or frames on their way from
    /// a decoder. With neither it stops, as it does for paused animations
    /// once their decodes have been collected.
    ///
    /// [`Animations::run`] comes first, so the clock is at `now` and the
    /// wait is what is left of the frame, not the whole of it.
    pub(super) fn sync(
        &mut self,
        window: &YapfastWindow,
        playing: bool,
        adapter: std::rc::Weak<RefCell<Adapter>>,
        now: Instant,
    ) {
        i_slint_core::yapfast_emoji::set_playing(playing);
        self.side = frame_side(window.window().scale_factor());
        if !self.wakes {
            self.wakes = true;
            wake_when_drawn(adapter.clone());
        }
        // Poll only while a worker has frames on the way. Once they arrive,
        // slow animations wake at their own pace instead of polling.
        let decoding = !self.decoding.is_empty() || self.queue.len() > 0;
        let poll = decoding.then_some(POLL);
        let frame = if playing {
            i_slint_core::yapfast_emoji::next_frame_delay()
        } else {
            None
        };
        let Some(wait) = poll.into_iter().chain(frame).min() else {
            self.stop();
            return;
        };
        let interval = u64::from(wait.max(SHORTEST));
        if self.timer.running() {
            // The closure stays, so a wait that changes at every frame, as
            // what is left of one does, makes nothing.
            if self.interval != interval {
                self.interval = interval;
                self.timer.set_interval(Duration::from_millis(interval));
            }
            return;
        }
        self.interval = interval;
        self.caught_up = Some(now);
        let weak = window.as_weak();
        self.timer.start(
            slint::TimerMode::Repeated,
            Duration::from_millis(interval),
            move || {
                let (Some(adapter), Some(window)) = (adapter.upgrade(), weak.upgrade()) else {
                    return;
                };
                // Moves the clock, and sets this timer for what is next: a
                // decode that finished changes both how long to wait and
                // whether a paused window needs the timer at all.
                adapter.borrow_mut().run_emoji_animations(&window);
            },
        );
    }

    /// Stops the timer, and time for the animations with it.
    pub(super) fn stop(&mut self) {
        self.timer.stop();
        self.interval = 0;
        self.caught_up = None;
    }
}

/// Has the registry wake the adapter when a draw shows an emoji that plays
/// while the timer rests: one in text no refresh names, such as the
/// composer's, a reply's or a topic's, whose frames another place had decoded.
/// The draw is inside the paint, so the work waits for the event loop.
fn wake_when_drawn(adapter: std::rc::Weak<RefCell<Adapter>>) {
    i_slint_core::yapfast_emoji::on_wake(move || {
        let adapter = adapter.clone();
        slint::Timer::single_shot(Duration::ZERO, move || {
            let Some(adapter) = adapter.upgrade() else {
                return;
            };
            let Ok(mut adapter) = adapter.try_borrow_mut() else {
                return;
            };
            if let Some(window) = adapter.window.upgrade() {
                adapter.run_emoji_animations(&window);
            }
        });
    });
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
    animations.show(vec![(key.to_owned(), Some(glyph), "pic")], [], &pictures);
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
        animations.show([(message.into(), None, "message.png")], [], &pictures);
        animations.also([(picker.into(), None, "picker.gif")], &pictures);
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
    fn slow_visible_emoji_do_not_poll_at_the_cached_fast_emojis_rate() {
        i_slint_backend_testing::init_no_event_loop();
        let window = YapfastWindow::new().unwrap();
        let mut animations = Animations::new();
        // One instant throughout: the test moves the clock by hand.
        let now = Instant::now();
        let pictures = HashMap::new();
        animations.show(
            [
                ("slow:1".into(), None, "slow.gif"),
                ("fast:1".into(), None, "fast.gif"),
            ],
            [],
            &pictures,
        );
        for (key, delay) in [("slow:1", 1000), ("fast:1", 20)] {
            animations
                .done
                .send(Decoded {
                    key: key.into(),
                    glyph: None,
                    frames: vec![vec![0; 16], vec![255; 16]],
                    delays: vec![delay, delay + 500],
                })
                .unwrap();
        }
        animations.run(&window, &pictures, now);
        animations.sync(&window, true, Default::default(), now);
        assert_eq!(animations.interval, 20);
        animations.show([("slow:1".into(), None, "slow.gif")], [], &pictures);
        animations.run(&window, &pictures, now);
        animations.sync(&window, true, Default::default(), now);
        assert_eq!(animations.interval, 1000);
        assert!(animations.cache.contains("fast:1"));
        let revision = i_slint_core::yapfast_emoji::revision();
        i_slint_core::yapfast_emoji::advance(Duration::from_millis(20));
        assert_eq!(i_slint_core::yapfast_emoji::revision(), revision);
        assert_eq!(i_slint_core::yapfast_emoji::next_frame_delay(), Some(980));
        i_slint_core::yapfast_emoji::advance(Duration::from_millis(980));
        assert_eq!(i_slint_core::yapfast_emoji::next_frame_delay(), Some(1500));
    }

    #[test]
    fn cached_offscreen_emoji_stop_redrawing_and_resume_without_decoding() {
        i_slint_backend_testing::init_no_event_loop();
        let window = YapfastWindow::new().unwrap();
        let mut animations = Animations::new();
        // One instant throughout: the test moves the clock by hand.
        let now = Instant::now();
        let pictures = HashMap::new();
        let key = "offscreen-clock:1";
        animations.show([(key.into(), None, "animation.gif")], [], &pictures);
        animations
            .done
            .send(Decoded {
                key: key.into(),
                glyph: None,
                frames: vec![vec![0; 16], vec![255; 16]],
                delays: vec![50, 50],
            })
            .unwrap();
        animations.run(&window, &pictures, now);
        i_slint_core::yapfast_emoji::set_playing(true);
        assert_eq!(i_slint_core::yapfast_emoji::shortest_delay(), Some(50));

        animations.show([], [], &pictures);
        animations.run(&window, &pictures, now);
        let revision = i_slint_core::yapfast_emoji::revision();
        i_slint_core::yapfast_emoji::advance(Duration::from_millis(50));
        assert_eq!(i_slint_core::yapfast_emoji::revision(), revision);
        assert!(i_slint_core::yapfast_emoji::shortest_delay().is_none());
        assert!(
            animations.cache.contains(key),
            "scrolling back reuses the frames"
        );

        animations.show([(key.into(), None, "animation.gif")], [], &pictures);
        animations.run(&window, &pictures, now);
        assert_eq!(animations.queue.len(), 0);
        assert!(animations.decoding.is_empty());
        i_slint_core::yapfast_emoji::advance(Duration::from_millis(50));
        assert_ne!(i_slint_core::yapfast_emoji::revision(), revision);
        animations.sync(&window, true, Default::default(), now);
        assert!(animations.timer.running());
        animations.sync(&window, false, Default::default(), now);
        assert!(!animations.timer.running(), "paused animations do not poll");
        animations.sync(&window, true, Default::default(), now);
        assert!(
            animations.timer.running(),
            "focus resumes cached animations"
        );
    }

    /// Two frames of 2 by 2 pixels on their way back from a decoder.
    fn decoded(key: &str, glyph: Option<char>, delays: [u32; 2]) -> Decoded {
        Decoded {
            key: key.into(),
            glyph,
            frames: vec![vec![0; 16], vec![255; 16]],
            delays: delays.to_vec(),
        }
    }

    #[test]
    fn setting_the_timer_again_keeps_the_time_already_waited() {
        use i_slint_core::yapfast_emoji as registry;
        i_slint_backend_testing::init_no_event_loop();
        let window = YapfastWindow::new().unwrap();
        let mut animations = Animations::new();
        animations.side = 2;
        let pictures = HashMap::new();
        let slow = || ("rearm-slow:1".to_owned(), None, "slow.gif");
        let fast = || ("rearm-fast:2".to_owned(), None, "fast.gif");
        let start = Instant::now();
        let at = |millis| start + Duration::from_millis(millis);
        let step = |animations: &mut Animations, now| {
            animations.run(&window, &pictures, now);
            animations.sync(&window, true, Default::default(), now);
        };

        animations.show([slow()], [], &pictures);
        let send = animations.done.clone();
        send.send(decoded("rearm-slow:1", None, [1000, 1000]))
            .unwrap();
        step(&mut animations, start);
        assert_eq!(animations.interval, 1000);
        assert_eq!(registry::elapsed(), 0, "time starts with the timer");

        // 600 ms into the slow frame a faster emoji scrolls in.
        animations.show([slow(), fast()], [], &pictures);
        send.send(decoded("rearm-fast:2", None, [50, 50])).unwrap();
        step(&mut animations, at(600));
        assert_eq!(
            registry::elapsed(),
            600,
            "the wait so far reached the clock"
        );
        assert_eq!(animations.interval, 50);

        // It scrolls away: the slow frame has what is left of it to wait,
        // not the whole of it again.
        animations.show([slow()], [], &pictures);
        step(&mut animations, at(630));
        assert_eq!(registry::elapsed(), 630);
        assert_eq!(animations.interval, 370);

        let revision = registry::revision();
        step(&mut animations, at(1000));
        assert_ne!(
            registry::revision(),
            revision,
            "the slow frame changed on time"
        );
        assert_eq!(animations.interval, 1000);

        // The part of a millisecond a tick leaves over counts at the next.
        step(&mut animations, start + Duration::from_micros(1_000_900));
        assert_eq!(registry::elapsed(), 1000);
        step(&mut animations, start + Duration::from_micros(1_001_100));
        assert_eq!(registry::elapsed(), 1001);

        // Paused, the timer stops, and time with it.
        animations.run(&window, &pictures, at(1100));
        animations.sync(&window, false, Default::default(), at(1100));
        assert!(!animations.timer.running());
        step(&mut animations, at(5000));
        assert_eq!(
            registry::elapsed(),
            1100,
            "none passed while it was stopped"
        );
        assert_eq!(animations.interval, 900);
    }

    #[test]
    fn an_emoji_drawn_where_no_refresh_names_it_plays_while_it_is_drawn() {
        use i_slint_core::yapfast_emoji as registry;
        i_slint_backend_testing::init_no_event_loop();
        let window = YapfastWindow::new().unwrap();
        let mut animations = Animations::new();
        animations.side = 2;
        let pictures = HashMap::new();
        let (key, glyph) = ("drawn-elsewhere:1", '\u{10ffe4}');
        let now = Instant::now();
        let step = |animations: &mut Animations, now| {
            animations.run(&window, &pictures, now);
            animations.sync(&window, true, Default::default(), now);
            animations.timer.running()
        };

        // A message showed it, so its frames were decoded and are kept.
        animations.show([(key.into(), Some(glyph), "a.gif")], [], &pictures);
        animations
            .done
            .send(decoded(key, Some(glyph), [50, 50]))
            .unwrap();
        assert!(step(&mut animations, now));
        // The message scrolled away, and nothing a refresh names shows it.
        animations.show([], [], &pictures);
        assert!(!step(&mut animations, now), "kept, and not on screen");
        assert!(animations.cache.contains(key));

        // The composer draws its glyph: the paint takes its frame.
        registry::begin_paint();
        assert!(registry::frame_of_glyph(glyph).is_some());
        assert!(
            step(&mut animations, now),
            "drawn, so the clock moves for it"
        );
        let revision = registry::revision();
        let later = now + Duration::from_millis(50);
        assert!(step(&mut animations, later));
        assert_ne!(
            registry::revision(),
            revision,
            "and it shows its next frame"
        );

        // The draft was sent: the paint that frame asked for leaves it out.
        registry::begin_paint();
        assert!(!step(&mut animations, later), "nothing draws it any more");
    }

    #[test]
    fn what_is_kept_for_a_bar_or_a_menu_plays_only_while_drawn() {
        use i_slint_core::yapfast_emoji as registry;
        let mut animations = Animations::new();
        animations.side = 2;
        let pictures = HashMap::new();
        let (message, bar) = ("kept-message:1", "kept-bar:2");
        animations.show(
            [(message.into(), None, "message.gif")],
            [(bar.into(), None, "bar.gif")],
            &pictures,
        );
        for (key, delay) in [(message, 400), (bar, 20)] {
            animations
                .done
                .send(decoded(key, None, [delay, delay]))
                .unwrap();
        }
        animations.drain();
        registry::set_playing(true);
        assert!(
            animations.cache.contains(bar),
            "ready for when the bar opens"
        );
        assert_eq!(
            registry::next_frame_delay(),
            Some(400),
            "a closed bar's emoji wakes nothing"
        );
        // The bar opens over a message, and its cell takes the frame.
        registry::begin_paint();
        assert!(registry::frame_of_key(bar).is_some());
        assert_eq!(registry::next_frame_delay(), Some(20));
        // Both pinned: a refresh that names them again takes neither.
        animations.show(
            [(message.into(), None, "message.gif")],
            [(bar.into(), None, "bar.gif")],
            &pictures,
        );
        animations.cache.evict_to(0);
        assert!(animations.cache.contains(bar) && animations.cache.contains(message));
        // One in a message and in the bar at once is in view.
        animations.show(
            [(bar.into(), None, "bar.gif")],
            [(bar.into(), None, "bar.gif")],
            &pictures,
        );
        registry::advance(Duration::from_millis(20));
        registry::begin_paint();
        assert_eq!(registry::next_frame_delay(), Some(20));
    }

    #[test]
    fn only_what_shows_moves_the_clocks_revision() {
        use i_slint_core::yapfast_emoji as registry;
        let animation = |delay| {
            let frames = small(2);
            Rc::new(registry::Animation::new(
                frames.images().to_vec(),
                &[delay, delay],
            ))
        };
        registry::insert("shows-fast:1", None, animation(10));
        registry::insert("shows-slow:2", None, animation(400));
        registry::set_playing(true);
        assert_eq!(
            registry::next_frame_delay(),
            Some(10),
            "all show until some are named"
        );
        registry::set_in_view(|key| key == "shows-slow:2");
        assert_eq!(registry::next_frame_delay(), Some(400));
        let before = registry::revision();
        registry::advance(Duration::from_millis(390));
        assert_eq!(registry::revision(), before, "the fast one is kept, unseen");
        assert_eq!(registry::next_frame_delay(), Some(10));
        registry::advance(Duration::from_millis(10));
        assert_eq!(registry::revision(), before + 1);
        registry::set_in_view(|_| false);
        assert_eq!(registry::next_frame_delay(), None);
        assert_eq!(registry::shortest_delay(), None);
        // Removing what is not there draws nothing again.
        registry::remove("shows-gone:3");
        assert_eq!(registry::revision(), before + 1);
        registry::remove("shows-fast:1");
        registry::remove("shows-slow:2");
    }

    #[test]
    fn a_draw_wakes_the_application_once_and_a_paint_without_it_ends_it() {
        use i_slint_core::yapfast_emoji as registry;
        let glyph = '\u{10ffe5}';
        let frames = small(2);
        let animation = Rc::new(registry::Animation::new(
            frames.images().to_vec(),
            &[50, 50],
        ));
        registry::insert("wakes:1", Some(glyph), animation);
        let woken = Rc::new(Cell::new(0));
        registry::on_wake({
            let woken = woken.clone();
            move || woken.set(woken.get() + 1)
        });
        registry::set_in_view(|_| false);
        registry::set_playing(true);
        assert_eq!(registry::next_frame_delay(), None, "kept, and not shown");

        registry::begin_paint();
        assert!(registry::frame_of_glyph(glyph).is_some());
        assert_eq!(woken.get(), 1, "the draw wakes the application");
        assert_eq!(registry::next_frame_delay(), Some(50), "drawn, so it plays");
        assert!(registry::frame_of_glyph(glyph).is_some());
        assert_eq!(woken.get(), 1, "awake already");

        // A paint of something else draws no text again, and ends nothing.
        registry::begin_paint();
        assert_eq!(registry::next_frame_delay(), Some(50));

        let before = registry::revision();
        registry::advance(Duration::from_millis(50));
        assert_eq!(registry::revision(), before + 1, "its frame changed");
        // Until the window paints, what the last paint drew still shows: a
        // window that cannot paint yet loses no emoji.
        assert_eq!(registry::next_frame_delay(), Some(50));
        registry::begin_paint();
        assert!(registry::frame_of_glyph(glyph).is_some());
        assert_eq!(registry::next_frame_delay(), Some(50), "drawn again");

        // The text went away: the paint after the next frame leaves it out.
        registry::advance(Duration::from_millis(50));
        registry::begin_paint();
        assert_eq!(registry::next_frame_delay(), None);
        assert_eq!(woken.get(), 1);
        let before = registry::revision();
        registry::advance(Duration::from_millis(500));
        assert_eq!(registry::revision(), before, "nothing shows, nothing draws");

        // A paused window's draws wake nothing.
        registry::set_playing(false);
        assert!(registry::frame_of_key("wakes:1").is_some());
        assert_eq!(woken.get(), 1);
        registry::set_playing(true);
        assert_eq!(registry::next_frame_delay(), Some(50), "its cell drew it");
        registry::remove("wakes:1");
        assert!(registry::frame_of_glyph(glyph).is_none());
    }

    #[test]
    fn a_glyph_follows_its_emoji() {
        use i_slint_core::yapfast_emoji as registry;
        let (first, second) = ('\u{10ffe6}', '\u{10ffe7}');
        let animation = || {
            let frames = small(2);
            Rc::new(registry::Animation::new(
                frames.images().to_vec(),
                frames.delays(),
            ))
        };
        registry::insert("named:1", None, animation());
        assert!(registry::frame_of_glyph(first).is_none());
        registry::set_glyph("named:1", first);
        assert!(registry::frame_of_glyph(first).is_some());
        // Its font was made again, and its glyph with it.
        registry::set_glyph("named:1", second);
        assert!(registry::frame_of_glyph(first).is_none());
        assert!(registry::frame_of_glyph(second).is_some());
        // Frames at another size take the glyph they are given.
        registry::insert("named:1", Some(first), animation());
        assert!(registry::frame_of_glyph(second).is_none());
        assert!(registry::frame_of_glyph(first).is_some());
        registry::set_glyph("unknown:2", second);
        assert!(registry::frame_of_glyph(second).is_none());
        registry::remove("named:1");
        assert!(registry::frame_of_glyph(first).is_none());
    }

    #[test]
    fn scrolling_to_rows_without_animations_discards_obsolete_work() {
        let mut animations = Animations::new();
        let pictures = HashMap::new();
        let key = "scroll-left-animation:1";
        animations.also([(key.into(), None, "animation.gif")], &pictures);
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
        cache.pin("b".to_owned(), false);
        cache.evict_to(0);
        assert!(cache.contains("b"), "on screen, so it keeps its frames");
        assert!(!cache.contains("a"));
        assert_eq!(cache.bytes(), 16, "over the budget, and pinned");
        // A refresh pins what it showed, so the pinned set replaces the last.
        cache.unpin();
        cache.pin("a".to_owned(), true);
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
