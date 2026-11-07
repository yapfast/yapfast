//! Pictures: thumbnails of staged files, the picture cache, fitting and
//! animating pictures, blurred spoilers, galleries, and the QR code for linking.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;

use super::*;

/// The readable part of a Matrix ID: `@alice:example.org` becomes `alice`.
/// A QR code carrying `address`, black on white with the quiet zone around it,
/// one pixel to a module; Slint scales it up without smoothing.
#[cfg(feature = "discord")]
pub(super) fn qr_code(address: &str) -> Option<slint::Image> {
    /// The blank margin scanners need, in modules.
    const QUIET: usize = 4;
    let code = qrcode::QrCode::new(address.as_bytes()).ok()?;
    let width = code.width();
    let modules = code.to_colors();
    let side = width + 2 * QUIET;
    let mut buffer = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(side as u32, side as u32);
    for (index, pixel) in buffer.make_mut_slice().iter_mut().enumerate() {
        let (x, y) = (index % side, index / side);
        let dark = (QUIET..QUIET + width).contains(&x)
            && (QUIET..QUIET + width).contains(&y)
            && modules[(y - QUIET) * width + (x - QUIET)] == qrcode::Color::Dark;
        let value = if dark { 0 } else { 255 };
        *pixel = slint::Rgba8Pixel::new(value, value, value, 255);
    }
    Some(slint::Image::from_rgba8(buffer))
}

/// The picture at `url` once it has arrived, converted for Slint only once.
/// Reads a picture waiting in the composer off the UI thread, then shows its preview.
/// A staged file's picture, and the blur it shows as a spoiler.
pub(super) struct Thumbnail {
    pub(super) picture: slint::Image,
    pub(super) covered: slint::Image,
}

pub(super) fn load_thumbnail(
    runtime: &BackgroundRuntime,
    weak: std::rc::Weak<RefCell<Adapter>>,
    path: PathBuf,
) {
    let reading = {
        let path = path.clone();
        runtime.spawn_blocking(move || {
            std::fs::read(&path)
                .ok()
                .and_then(|data| yapfast_core::chat::decode_picture(&data, 240, 240))
        })
    };
    let task = slint::spawn_local(async move {
        let picture = reading.await.ok().flatten();
        let Some(adapter) = weak.upgrade() else {
            return;
        };
        let window = adapter.borrow().window.upgrade();
        let mut adapter = adapter.borrow_mut();
        // Unless it was taken out of the composer meanwhile.
        let Some(slot) = adapter.thumbnails.get_mut(&path) else {
            return;
        };
        *slot = picture.map(|picture| Thumbnail {
            picture: self::picture(&picture),
            covered: blurred(&picture),
        });
        if let Some(window) = window {
            adapter.refresh(&window);
        }
    });
    if let Err(error) = task {
        tracing::error!(%error, "could not preview a file to send");
    }
}

pub(super) fn cached_picture(
    cache: &mut HashMap<String, slint::Image>,
    state: &yapfast_core::application::AppState,
    url: Option<&str>,
) -> slint::Image {
    let Some(url) = url else {
        return slint::Image::default();
    };
    if let Some(image) = cache.get(url) {
        return image.clone();
    }
    let Some(arrived) = state.pictures.get(url) else {
        return slint::Image::default();
    };
    let image = picture(arrived);
    cache.insert(url.to_owned(), image.clone());
    image
}

/// The UI asks for avatars only as virtualized components use them. Weak
/// sources do not pin decoded core pictures; conversions share the bounded
/// preview cache rather than living in every off-screen row.
pub(super) fn refresh_avatar_sources(state: &AppState) {
    AVATAR_SOURCES.with_borrow_mut(|sources| {
        *sources = state
            .pictures
            .iter()
            .map(|(url, picture)| (url.clone(), Arc::downgrade(picture)))
            .collect();
    });
}

pub(super) fn avatar_picture(url: &str) -> slint::Image {
    let source =
        AVATAR_SOURCES.with_borrow(|sources| sources.get(url).and_then(std::sync::Weak::upgrade));
    source.map_or_else(slint::Image::default, |source| {
        PREVIEWS.with_borrow_mut(|cache| cache.image(&source, false))
    })
}

/// The address of the animated version of a collectible's still picture:
/// Discord serves a decoration's or nameplate's `/static` PNG and, beside it,
/// an `/animated` APNG. Any other address is its own.
pub(super) fn animated_url(url: &str) -> String {
    match url.strip_suffix("/static") {
        Some(base) if url.contains("/collectibles-shop/") => format!("{base}/animated"),
        _ => url.to_owned(),
    }
}

/// The frames and delays of the animation at `url`, once it has arrived and
/// the frames decode as they show; empty for a still picture or one on its
/// way.
pub(super) fn avatar_animation(url: &str) -> (ModelRc<slint::Image>, ModelRc<i32>) {
    let source =
        AVATAR_SOURCES.with_borrow(|sources| sources.get(url).and_then(std::sync::Weak::upgrade));
    source.map_or_else(Default::default, |source| animation(&source))
}

thread_local! {
    static AVATAR_SOURCES: RefCell<HashMap<String, std::sync::Weak<Picture>>> = RefCell::default();
}

/// A full image's size once shrunk to the viewer's largest.
pub(super) fn fit(width: u32, height: u32) -> (u32, u32) {
    use yapfast_core::chat::{VIEW_HEIGHT, VIEW_WIDTH};
    let scale = f64::min(
        1.0,
        f64::min(
            f64::from(VIEW_WIDTH) / f64::from(width.max(1)),
            f64::from(VIEW_HEIGHT) / f64::from(height.max(1)),
        ),
    );
    let side = |side: u32| ((f64::from(side) * scale).round() as u32).max(1);
    (side(width), side(height))
}

/// A picture's pixels in Slint's own buffer, so showing it shares them.
struct SlintPixels(slint::SharedPixelBuffer<slint::Rgba8Pixel>);

impl yapfast_core::chat::PixelBuffer for SlintPixels {
    fn bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

/// Has pictures decoded into Slint's buffers from now on, so showing one
/// shares its pixels rather than copying them; otherwise each picture on
/// screen would be in memory twice.
pub(super) fn decode_into_slint_buffers() {
    yapfast_core::chat::set_pixel_buffers(|width, height, rgba| {
        Box::new(SlintPixels(slint::SharedPixelBuffer::clone_from_slice(
            rgba, width, height,
        )))
    });
}

/// A moving picture's frame in Slint's own buffer, which it shows as it is.
struct SlintFrame(slint::SharedPixelBuffer<slint::Rgba8Pixel>);

impl yapfast_core::picture::FrameBuffer for SlintFrame {
    fn size(&self) -> (u32, u32) {
        (self.0.width(), self.0.height())
    }

    fn pixels_mut(&mut self) -> &mut [u8] {
        // The feed writes only buffers Slint no longer shows, so this never
        // copies.
        self.0.make_mut_bytes()
    }
}

/// Has moving pictures made in Slint's buffers from now on.
pub(super) fn make_frames_in_slint_buffers() {
    yapfast_core::picture::set_frame_buffers(|width, height| {
        Box::new(SlintFrame(slint::SharedPixelBuffer::new(width, height)))
    });
}

/// A moving picture's frame, shown without a copy.
pub(crate) fn frame(buffer: &dyn yapfast_core::picture::FrameBuffer) -> slint::Image {
    match (buffer as &dyn std::any::Any).downcast_ref::<SlintFrame>() {
        Some(frame) => slint::Image::from_rgba8(frame.0.clone()),
        None => slint::Image::default(),
    }
}

pub(crate) fn picture(picture: &Picture) -> slint::Image {
    let buffer = match picture.rgba.buffer::<SlintPixels>() {
        Some(pixels) => pixels.0.clone(),
        None => {
            slint::SharedPixelBuffer::clone_from_slice(&picture.rgba, picture.width, picture.height)
        }
    };
    slint::Image::from_rgba8(buffer)
}

/// Gallery metadata is cheap to keep for the whole timeline. Copy pixels
/// into Slint only when a visible row asks for its image.
struct ImageCells(Vec<(ImageCell, Option<Arc<Picture>>)>);

pub(super) fn image_cells(cells: Vec<(ImageCell, Option<Arc<Picture>>)>) -> ModelRc<ImageCell> {
    ModelRc::new(ImageCells(cells))
}

impl Model for ImageCells {
    type Data = ImageCell;

    fn row_count(&self) -> usize {
        self.0.len()
    }

    fn row_data(&self, row: usize) -> Option<ImageCell> {
        let (cell, source) = self.0.get(row)?;
        let mut cell = cell.clone();
        if let Some(source) = source {
            cell.image = PREVIEWS.with_borrow_mut(|cache| cache.image(source, cell.spoiler));
        }
        Some(cell)
    }

    fn model_tracker(&self) -> &dyn slint::ModelTracker {
        &()
    }
}

/// Converted previews shared across row rebuilds, with an eight MiB limit.
/// Visible Slint components keep their own handles after cache eviction.
#[derive(Default)]
struct Previews {
    images: std::collections::VecDeque<CachedPreview>,
    bytes: usize,
}

struct CachedPreview {
    source: std::sync::Weak<Picture>,
    spoiler: bool,
    image: slint::Image,
    bytes: usize,
}

const PREVIEW_CACHE_BYTES: usize = 8 * 1024 * 1024;

impl Previews {
    fn image(&mut self, source: &Arc<Picture>, spoiler: bool) -> slint::Image {
        self.images
            .retain(|cached| cached.source.strong_count() > 0);
        self.bytes = self.images.iter().map(|cached| cached.bytes).sum();
        let weak = Arc::downgrade(source);
        if let Some(index) = self
            .images
            .iter()
            .position(|cached| cached.source.ptr_eq(&weak) && cached.spoiler == spoiler)
        {
            let cached = self.images.remove(index).unwrap();
            let image = cached.image.clone();
            self.images.push_back(cached);
            return image;
        }
        let image = if spoiler {
            blurred(source)
        } else {
            picture(source)
        };
        let size = image.size();
        let bytes = size.width as usize * size.height as usize * 4;
        if bytes <= PREVIEW_CACHE_BYTES {
            while self.bytes + bytes > PREVIEW_CACHE_BYTES {
                self.bytes -= self.images.pop_front().unwrap().bytes;
            }
            self.bytes += bytes;
            self.images.push_back(CachedPreview {
                source: weak,
                spoiler,
                image: image.clone(),
                bytes,
            });
        }
        image
    }
}

thread_local! {
    static PREVIEWS: RefCell<Previews> = RefCell::default();
}

/// An animation's frames for Slint and how long each shows, in milliseconds;
/// both empty for a still picture. The frames decode as they show.
pub(super) fn animation(picture: &Arc<Picture>) -> (ModelRc<slint::Image>, ModelRc<i32>) {
    let Some(animation) = &picture.animation else {
        return (ModelRc::default(), ModelRc::default());
    };
    let delays = ModelRc::new(VecModel::from(
        animation
            .delays
            .iter()
            .map(|delay| i32::try_from(*delay).unwrap_or(i32::MAX))
            .collect::<Vec<_>>(),
    ));
    let frames = FRAMES.with_borrow_mut(|cache| {
        cache.retain(|frames| frames.strong_count() > 0);
        let shared = cache
            .iter()
            .filter_map(std::rc::Weak::upgrade)
            .find(|frames| Arc::ptr_eq(&frames.picture, picture));
        let frames = shared.unwrap_or_else(|| {
            let frames = Rc::new(Frames {
                picture: picture.clone(),
                decoding: RefCell::default(),
            });
            cache.push(Rc::downgrade(&frames));
            frames
        });
        ModelRc::from(frames)
    });
    (frames, delays)
}

thread_local! {
    /// Animations' frames handed to Slint, so a row built again shares its
    /// picture's.
    static FRAMES: RefCell<Vec<std::rc::Weak<Frames>>> = RefCell::default();
    /// Runs while decoders are in use, including after the last row scrolls away.
    static FRAME_SWEEP: slint::Timer = slint::Timer::default();
}

/// How long an animation keeps its decoder after its last frame showed, as
/// when its row scrolled away.
const IDLE_FRAMES: Duration = Duration::from_secs(1);
/// How many places one animation plays from at once, each at its own frame,
/// as the same GIF posted twice does.
const CURSORS: usize = 4;
/// How many frames a run may be behind the frame asked for and still give
/// it, its decoder passing over the ones between; further back, the asker
/// is another place showing the animation at its own frame.
const CATCH_UP: usize = 4;

/// An animation's frames, decoded in order off the UI thread as Slint asks
/// for them. Each place it shows at its own frame has its own decoder, so
/// two of them don't send each other back to the start.
struct Frames {
    picture: Arc<Picture>,
    decoding: RefCell<Decoding>,
}

#[derive(Default)]
struct Decoding {
    cursors: Vec<Cursor>,
    used: Option<Duration>,
}

type Frame = (usize, slint::SharedPixelBuffer<slint::Rgba8Pixel>);

/// One run around an animation's frames. Its thread decodes them in order
/// and around again, one ahead of the one showing, passing over those
/// before the frame asked for, and stops when the run is dropped.
struct Cursor {
    coming: mpsc::Receiver<Frame>,
    wanted: Arc<AtomicUsize>,
    /// The frame to come next.
    next: usize,
    shown: Option<(usize, slint::Image)>,
    used: Duration,
}

impl Cursor {
    fn start(picture: &Arc<Picture>, row: usize, now: Duration) -> Self {
        let (send, coming) = mpsc::sync_channel(0);
        let wanted = Arc::new(AtomicUsize::new(row));
        let decoding = {
            let picture = picture.clone();
            let wanted = wanted.clone();
            std::thread::Builder::new()
                .name("yapfast-frames".into())
                .spawn(move || decode_frames(&picture, &wanted, &send))
        };
        if let Err(error) = decoding {
            tracing::warn!(%error, "could not start decoding an animation");
        }
        Self {
            coming,
            wanted,
            next: row,
            shown: None,
            used: now,
        }
    }

    /// Takes the frames that came, up to `row`, showing the last; one not
    /// decoded yet leaves the one before showing. Frames passed over aren't
    /// made into images.
    fn advance(&mut self, row: usize, count: usize) {
        self.wanted.store(row, Ordering::Relaxed);
        let mut last = None;
        while not_past(self.next, row, count) {
            let Ok((index, frame)) = self.coming.try_recv() else {
                break;
            };
            self.next = (index + 1) % count;
            last = Some((index, frame));
        }
        if let Some((index, frame)) = last {
            self.shown = Some((index, slint::Image::from_rgba8(frame)));
        }
    }
}

/// Sends `picture`'s frames in order and around again, from the one wanted,
/// until nothing takes them.
fn decode_frames(picture: &Picture, wanted: &AtomicUsize, send: &mpsc::SyncSender<Frame>) {
    let count = picture
        .animation
        .as_ref()
        .map_or(0, |animation| animation.delays.len());
    loop {
        // A run that sends nothing would never send anything.
        let mut sent = false;
        for (index, rgba) in picture.frames().enumerate() {
            let wanted = wanted.load(Ordering::Relaxed);
            if index != wanted && not_past(index, wanted, count) {
                continue;
            }
            let frame =
                slint::SharedPixelBuffer::clone_from_slice(&rgba, picture.width, picture.height);
            // Not held twice while it waits to be taken.
            drop(rgba);
            if send.send((index, frame)).is_err() {
                return;
            }
            sent = true;
        }
        if !sent {
            return;
        }
    }
}

/// Whether frame `at` comes before `row` or is it, going around `count`
/// frames: less than half the way round from it.
fn not_past(at: usize, row: usize, count: usize) -> bool {
    (row + count - at) % count < count.div_ceil(2)
}

impl Model for Frames {
    type Data = slint::Image;

    fn row_count(&self) -> usize {
        self.picture
            .animation
            .as_ref()
            .map_or(0, |animation| animation.delays.len())
    }

    fn row_data(&self, row: usize) -> Option<slint::Image> {
        if row >= self.row_count() {
            return None;
        }
        let now = Duration::from(i_slint_core::animations::current_tick());
        let image = {
            let mut decoding = self.decoding.borrow_mut();
            decoding.used = Some(now);
            self.frame(&mut decoding, row, now)
        };
        FRAME_SWEEP.with(|timer| {
            if !timer.running() {
                timer.start(slint::TimerMode::Repeated, IDLE_FRAMES, || {
                    if !sweep(i_slint_core::animations::current_tick().into()) {
                        FRAME_SWEEP.with(slint::Timer::stop);
                    }
                });
            }
        });
        Some(image)
    }

    fn model_tracker(&self) -> &dyn slint::ModelTracker {
        &()
    }
}

impl Frames {
    /// Frame `row`, from the run showing it or the nearest one coming up to
    /// it; otherwise from a run of its own started there, in place of the
    /// one used longest ago if there are enough. Until a frame is decoded
    /// the one before shows, or the still.
    fn frame(&self, decoding: &mut Decoding, row: usize, now: Duration) -> slint::Image {
        let count = self.row_count();
        let cursors = &mut decoding.cursors;
        let at = cursors
            .iter()
            .position(|cursor| {
                cursor
                    .shown
                    .as_ref()
                    .is_some_and(|(shown, _)| *shown == row)
            })
            .or_else(|| {
                cursors
                    .iter()
                    .enumerate()
                    .filter(|(_, cursor)| not_past(cursor.next, row, count))
                    .map(|(at, cursor)| (at, (row + count - cursor.next) % count))
                    .filter(|(_, behind)| *behind <= CATCH_UP)
                    .min_by_key(|(_, behind)| *behind)
                    .map(|(at, _)| at)
            });
        let at = at.unwrap_or_else(|| {
            let fresh = Cursor::start(&self.picture, row, now);
            if cursors.len() < CURSORS {
                cursors.push(fresh);
                cursors.len() - 1
            } else {
                let at = cursors
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, cursor)| cursor.used)
                    .map_or(0, |(at, _)| at);
                cursors[at] = fresh;
                at
            }
        });
        let cursor = &mut cursors[at];
        cursor.used = now;
        cursor.advance(row, count);
        cursor
            .shown
            .as_ref()
            .map_or_else(|| picture(&self.picture), |(_, image)| image.clone())
    }
}

/// Lets runs that stopped showing give up their decoders and frames.
fn sweep(now: Duration) -> bool {
    let mut active = false;
    FRAMES.with_borrow(|cache| {
        for frames in cache.iter().filter_map(std::rc::Weak::upgrade) {
            // The one decoding now is in use.
            let Ok(mut decoding) = frames.decoding.try_borrow_mut() else {
                active = true;
                continue;
            };
            decoding
                .cursors
                .retain(|cursor| now.saturating_sub(cursor.used) < IDLE_FRAMES);
            if decoding.cursors.is_empty() {
                *decoding = Decoding::default();
            }
            active |= decoding.used.is_some();
        }
    });
    active
}

/// A picture shrunk to a few pixels, which Slint smooths into a blur when it's
/// drawn at full size: a spoiler's picture before it's revealed.
pub(super) fn blurred(picture: &Picture) -> slint::Image {
    const SIDE: u32 = 10;
    let scale = (picture.width.max(picture.height) / SIDE).max(1);
    let (width, height) = (
        picture.width.div_ceil(scale).max(1),
        picture.height.div_ceil(scale).max(1),
    );
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let mut sum = [0u32; 4];
            let mut count = 0;
            for sy in y * scale..((y + 1) * scale).min(picture.height) {
                for sx in x * scale..((x + 1) * scale).min(picture.width) {
                    let at = ((sy * picture.width + sx) * 4) as usize;
                    for (channel, total) in sum.iter_mut().enumerate() {
                        *total += u32::from(picture.rgba[at + channel]);
                    }
                    count += 1;
                }
            }
            rgba.extend(sum.map(|total| (total / count.max(1)) as u8));
        }
    }
    let buffer =
        slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(&rgba, width, height);
    slint::Image::from_rgba8(buffer)
}

/// A picture whose preview is on its way, shown as a skeleton meanwhile.
pub(super) fn loading(attachment: &yapfast_core::chat::Attachment) -> bool {
    (attachment.image || attachment.video)
        && attachment.preview.is_none()
        && !attachment.preview_failed
}

/// The size a preview will show at, from the picture's own size when known,
/// so its skeleton takes the same room.
pub(super) fn coming_size(attachment: &yapfast_core::chat::Attachment) -> (i32, i32) {
    use yapfast_core::chat::{PREVIEW_HEIGHT, PREVIEW_WIDTH};
    let Some((width, height)) = attachment.dimensions else {
        return (PREVIEW_WIDTH as i32, 200);
    };
    let scale = f64::min(
        1.0,
        f64::min(
            f64::from(PREVIEW_WIDTH) / f64::from(width.max(1)),
            f64::from(PREVIEW_HEIGHT) / f64::from(height.max(1)),
        ),
    );
    let side = |side: u32| ((f64::from(side) * scale).round() as i32).max(1);
    (side(width), side(height))
}

/// A message's previews laid out: one at its own size, several as square
/// tiles two to a row, together as wide as one preview.
pub(super) fn gallery(
    previews: &[Preview<'_>],
    covered: &dyn Fn(usize) -> bool,
) -> (ModelRc<ImageCell>, i32, i32) {
    let (cells, width, height) = gallery_cells(previews, covered);
    (image_cells(cells), width, height)
}

/// [`gallery`] shrunk to be at most `max_width` wide.
pub(super) fn gallery_within(
    previews: &[Preview<'_>],
    covered: &dyn Fn(usize) -> bool,
    max_width: i32,
) -> (ModelRc<ImageCell>, i32, i32) {
    let (mut cells, width, height) = gallery_cells(previews, covered);
    if width <= max_width {
        return (image_cells(cells), width, height);
    }
    let scale = |side: i32| (i64::from(side) * i64::from(max_width) / i64::from(width)) as i32;
    for (cell, _) in &mut cells {
        (cell.x, cell.y) = (scale(cell.x), scale(cell.y));
        (cell.width, cell.height) = (scale(cell.width).max(1), scale(cell.height).max(1));
    }
    (image_cells(cells), max_width, scale(height).max(1))
}

type Cells = Vec<(ImageCell, Option<Arc<Picture>>)>;

fn gallery_cells(previews: &[Preview<'_>], covered: &dyn Fn(usize) -> bool) -> (Cells, i32, i32) {
    const TILE: i32 = 273;
    const GAP: i32 = 4;
    let cell = |index: usize,
                attachment: &yapfast_core::chat::Attachment,
                preview: Option<&Arc<Picture>>| {
        // A video whose poster didn't come plays all the same.
        let Some(preview) = preview else {
            return ImageCell {
                index: index as i32,
                name: attachment.name.clone().into(),
                loading: loading(attachment),
                video: attachment.video,
                ..Default::default()
            };
        };
        let spoiler = attachment.spoiler && covered(index);
        if spoiler {
            return ImageCell {
                index: index as i32,
                name: attachment.name.clone().into(),
                spoiler,
                video: attachment.video,
                ..Default::default()
            };
        }
        let (frames, delays) = animation(preview);
        ImageCell {
            index: index as i32,
            name: attachment.name.clone().into(),
            frames,
            delays,
            video: attachment.video,
            gif: attachment
                .gif
                .as_ref()
                .map(|gif| gif.url.as_str().into())
                .unwrap_or_default(),
            ..Default::default()
        }
    };
    match previews {
        [] => (Vec::new(), 0, 0),
        [(index, attachment, preview)] => {
            let (width, height) = match preview {
                Some(preview) => (preview.width as i32, preview.height as i32),
                None => coming_size(attachment),
            };
            let image = ImageCell {
                width,
                height,
                ..cell(*index, attachment, *preview)
            };
            (vec![(image, preview.cloned())], width, height)
        }
        _ => {
            let cells = previews
                .iter()
                .enumerate()
                .map(|(place, (index, attachment, preview))| {
                    (
                        ImageCell {
                            x: (place % 2) as i32 * (TILE + GAP),
                            y: (place / 2) as i32 * (TILE + GAP),
                            width: TILE,
                            height: TILE,
                            ..cell(*index, attachment, *preview)
                        },
                        preview.cloned(),
                    )
                })
                .collect();
            let rows = previews.len().div_ceil(2) as i32;
            (cells, 2 * TILE + GAP, rows * TILE + (rows - 1) * GAP)
        }
    }
}

#[cfg(all(test, feature = "discord"))]
mod tests {
    use super::*;

    #[test]
    fn galleries_copy_only_requested_previews_and_bound_their_cache() {
        PREVIEWS.set(Previews::default());
        let attachments: Vec<_> = (0..40)
            .map(|index| yapfast_core::chat::Attachment {
                name: format!("{index}.png"),
                size: None,
                image: true,
                dimensions: Some((360, 240)),
                preview: Some(Arc::new(Picture {
                    width: 360,
                    height: 240,
                    rgba: yapfast_core::chat::Pixels::new(360, 240, vec![index; 360 * 240 * 4]),
                    ..Default::default()
                })),
                preview_failed: false,
                placement: Placement::Message,
                gif: None,
                spoiler: false,
                video: false,
                url: None,
            })
            .collect();
        let row = message_row(
            &ChatMessage {
                attachments,
                ..Default::default()
            },
            &HashSet::new(),
        );
        assert_eq!(row.images.row_count(), 40);
        PREVIEWS.with_borrow(|cache| assert_eq!(cache.bytes, 0));

        let first = row.images.row_data(0).unwrap();
        assert_eq!(
            (first.image.size().width, first.image.size().height),
            (360, 240)
        );
        assert_eq!(first.image, row.images.row_data(0).unwrap().image);
        PREVIEWS.with_borrow(|cache| assert_eq!(cache.bytes, 360 * 240 * 4));
        for index in 1..row.images.row_count() {
            row.images.row_data(index).unwrap();
        }
        PREVIEWS.with_borrow(|cache| {
            assert!(cache.bytes <= PREVIEW_CACHE_BYTES);
            assert!(cache.images.len() < row.images.row_count());
        });
        // Eviction cannot invalidate a handle held by a visible component.
        assert_eq!(
            (first.image.size().width, first.image.size().height),
            (360, 240)
        );
        assert_eq!(
            row.images.row_data(0).unwrap().image.size(),
            first.image.size()
        );
        PREVIEWS.set(Previews::default());
    }

    #[test]
    fn found_cards_show_their_first_pictures_small_and_name_the_rest() {
        let picture = Arc::new(Picture {
            width: 360,
            height: 240,
            rgba: yapfast_core::chat::Pixels::new(360, 240, vec![0; 360 * 240 * 4]),
            ..Default::default()
        });
        let attachment = |name: &str, image: bool, preview: Option<&Arc<Picture>>| {
            yapfast_core::chat::Attachment {
                name: name.into(),
                size: None,
                image,
                dimensions: Some((360, 240)),
                preview: preview.cloned(),
                preview_failed: false,
                placement: Placement::Message,
                gif: None,
                spoiler: false,
                video: false,
                url: None,
            }
        };
        let one = found_row(
            &ChatMessage {
                attachments: vec![attachment("sunset.png", true, Some(&picture))],
                ..Default::default()
            },
            false,
        );
        assert_eq!((one.gallery_width, one.gallery_height), (300, 200));
        let cell = one.images.row_data(0).unwrap();
        assert_eq!((cell.width, cell.height), (300, 200));
        assert_eq!(one.files, "");

        // Past the fourth picture, and a file, are named; one on its way
        // shows as a skeleton.
        let mut attachments = vec![attachment("notes.txt", false, None)];
        attachments
            .extend((0..3).map(|index| attachment(&format!("{index}.png"), true, Some(&picture))));
        attachments.push(attachment("coming.png", true, None));
        attachments.push(attachment("fifth.png", true, Some(&picture)));
        let many = found_row(
            &ChatMessage {
                attachments,
                ..Default::default()
            },
            false,
        );
        assert_eq!(many.images.row_count(), 4);
        assert!(many.images.row_data(3).unwrap().loading);
        assert!(many.gallery_width <= 300);
        assert_eq!(many.files, "notes.txt, fifth.png");
        PREVIEWS.set(Previews::default());
    }

    #[test]
    fn rows_fit_their_pictures_and_previews_to_a_narrow_panel() {
        PREVIEWS.set(Previews::default());
        let picture = Arc::new(Picture {
            width: 400,
            height: 300,
            rgba: yapfast_core::chat::Pixels::new(400, 300, vec![0; 400 * 300 * 4]),
            ..Default::default()
        });
        let attachment = |placement| yapfast_core::chat::Attachment {
            name: "bridge.png".into(),
            size: None,
            image: true,
            dimensions: Some((400, 300)),
            preview: Some(picture.clone()),
            preview_failed: false,
            placement,
            gif: None,
            spoiler: false,
            video: false,
            url: None,
        };
        let message = ChatMessage {
            attachments: vec![
                attachment(Placement::Message),
                attachment(Placement::Embed(0)),
            ],
            embeds: vec![yapfast_core::chat::Embed {
                title: "City opens its first bridge for bikes only".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        // Beside the avatar in the timeline, as they came.
        let wide = message_row(&message, &HashSet::new());
        assert_eq!((wide.gallery_width, wide.gallery_height), (400, 300));
        let card = wide.embeds.row_data(0).unwrap();
        assert_eq!((card.gallery_width, card.gallery_height), (400, 300));
        // In 232px, the picture fits it, and the card's within its padding.
        let narrow = message_row_within(&message, &HashSet::new(), Some(232));
        assert_eq!((narrow.gallery_width, narrow.gallery_height), (232, 174));
        let card = narrow.embeds.row_data(0).unwrap();
        assert_eq!((card.gallery_width, card.gallery_height), (200, 150));
        PREVIEWS.set(Previews::default());
    }

    #[test]
    fn avatar_sources_are_weak_and_pixels_are_converted_only_when_requested() {
        PREVIEWS.set(Previews::default());
        let mut state = AppState::default();
        let source = Arc::new(Picture {
            width: 1,
            height: 1,
            rgba: yapfast_core::chat::Pixels::new(1, 1, vec![255; 4]),
            ..Default::default()
        });
        let weak = Arc::downgrade(&source);
        state.pictures.insert("avatar".into(), source);
        refresh_avatar_sources(&state);
        PREVIEWS.with_borrow(|cache| assert_eq!(cache.bytes, 0));
        let shown = avatar_picture("avatar");
        assert_eq!(shown, avatar_picture("avatar"));
        PREVIEWS.with_borrow(|cache| assert_eq!(cache.bytes, 4));
        drop(state);
        assert!(
            weak.upgrade().is_none(),
            "neither registry nor converted cache pins core pixels"
        );
        assert_eq!(avatar_picture("avatar").size().width, 0);
        assert_eq!(
            shown.size().width,
            1,
            "a component's image handle stays valid"
        );
        AVATAR_SOURCES.with_borrow_mut(HashMap::clear);
        PREVIEWS.set(Previews::default());
    }

    #[test]
    fn showing_a_decoded_picture_shares_its_pixels() {
        decode_into_slint_buffers();
        let mut png = Vec::new();
        image::RgbaImage::from_pixel(3, 2, image::Rgba([10, 20, 30, 255]))
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let decoded = yapfast_core::chat::decode_picture(&png, 240, 240).unwrap();
        let shown = picture(&decoded).to_rgba8().unwrap();
        assert_eq!(shown.as_bytes(), &*decoded.rgba);
        assert_eq!(
            shown.as_bytes().as_ptr(),
            decoded.rgba.as_ptr(),
            "not a copy"
        );
    }

    #[test]
    fn idle_animation_releases_its_decoder_without_another_frame_request() {
        i_slint_backend_testing::init_no_event_loop();
        let file = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("testing/fixtures/gif-h264.mp4"),
        )
        .unwrap();
        let picture = Arc::new(yapfast_core::chat::decode_picture(&file, 32, 32).unwrap());
        let (frames, _) = animation(&picture);
        let shown = frames.row_data(0).unwrap();
        let model = FRAMES.with_borrow(|cache| cache.last().unwrap().upgrade().unwrap());
        assert_eq!(model.decoding.borrow().cursors.len(), 1);
        i_slint_backend_testing::mock_elapsed_time(IDLE_FRAMES * 2);
        slint::platform::update_timers_and_animations();
        assert!(model.decoding.borrow().cursors.is_empty());
        FRAME_SWEEP.with(|timer| assert!(!timer.running()));
        assert_eq!(shown.size(), frames.row_data(0).unwrap().size());
    }

    #[test]
    fn collectibles_play_from_their_animated_address_once_it_arrives() {
        i_slint_backend_testing::init_no_event_loop();
        let still = "https://cdn.discordapp.com/media/v1/collectibles-shop/5/static";
        let animated = "https://cdn.discordapp.com/media/v1/collectibles-shop/5/animated";
        assert_eq!(animated_url(still), animated);
        assert_eq!(animated_url(animated), animated);
        let avatar = "https://cdn.discordapp.com/avatars/5/abc.png/static";
        assert_eq!(animated_url(avatar), avatar, "only collectibles have one");
        // Until it arrives, no frames.
        AVATAR_SOURCES.with_borrow_mut(HashMap::clear);
        assert_eq!(avatar_animation(animated).0.row_count(), 0);
        let file = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("testing/fixtures/gif-h264.mp4"),
        )
        .unwrap();
        let picture = Arc::new(yapfast_core::chat::decode_picture(&file, 32, 32).unwrap());
        AVATAR_SOURCES.with_borrow_mut(|sources| {
            sources.insert(animated.to_owned(), Arc::downgrade(&picture));
        });
        let (frames, delays) = avatar_animation(animated);
        assert_eq!((frames.row_count(), delays.row_count()), (10, 10));
        AVATAR_SOURCES.with_borrow_mut(HashMap::clear);
    }

    #[test]
    fn animations_decode_the_frame_showing_and_let_it_go_when_idle() {
        i_slint_backend_testing::init_no_event_loop();
        let file = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("testing/fixtures/gif-h264.mp4"),
        )
        .unwrap();
        let picture = Arc::new(yapfast_core::chat::decode_picture(&file, 32, 32).unwrap());
        let decoded: Vec<_> = picture.frames().collect();
        let (frames, delays) = animation(&picture);
        assert_eq!((frames.row_count(), delays.row_count()), (10, 10));
        // Frames decode off the UI thread; until one comes, the one before shows.
        let show = |row: usize| {
            let started = std::time::Instant::now();
            loop {
                let image = frames.row_data(row).unwrap();
                if image.to_rgba8().unwrap().as_bytes() == decoded[row] {
                    break;
                }
                assert!(started.elapsed() < Duration::from_secs(5), "frame {row}");
                std::thread::sleep(Duration::from_millis(1));
            }
        };
        let model = FRAMES.with_borrow(|cache| cache.last().unwrap().upgrade().unwrap());
        let runs = || model.decoding.borrow().cursors.len();
        // Played in turn, around and around, by one decoder, which passes
        // over a frame or two that showed too briefly.
        for row in (0..10).chain(0..10).chain([2, 5, 6]) {
            show(row);
        }
        assert_eq!(runs(), 1);
        // Another place showing it at its own frame gets a decoder of its own.
        show(3);
        assert_eq!(runs(), 2);
        show(7);
        show(4);
        assert_eq!(runs(), 2);
        assert!(frames.row_data(10).is_none());
        // A row built again shares them.
        assert!(frames == animation(&picture).0);
        // Idle, it gives up its decoders, and decodes again when it shows.
        i_slint_backend_testing::mock_elapsed_time(IDLE_FRAMES * 2);
        slint::platform::update_timers_and_animations();
        assert_eq!(runs(), 0);
        show(4);
        assert_eq!(runs(), 1);
    }
}
