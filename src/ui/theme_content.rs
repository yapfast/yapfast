//! A theme's `content` on the window, when the person lets the theme style
//! people's profiles and media (Settings → Themes; off unless turned on, for
//! each theme). See `yapfast_core::themes::content` and `docs/themes.md`.
//!
//! Pictures are recoloured once, as [`super::pictures`] hands them to Slint,
//! and the converted ones are cached as before; nothing is redone per frame.
//! With `media` at `normal` a picture is shared with the decoder as stock,
//! so a theme without `content`, or with it turned off, costs nothing.
//! Names, name styles, embeds and profile cards are built with it in Rust;
//! avatars, nameplates, embed links, activity tiles and the LIVE pill read
//! the `ThemeContent` global (`ui/theme_content.slint`).

use std::cell::{Cell, RefCell};
use std::sync::{Arc, Mutex, RwLock, mpsc};

use slint::ComponentHandle as _;
use yapfast_core::themes::content::{
    AvatarShape, Content, Dither, Follow, Media, NameColours, Nameplates, Shown, dither, recolour,
};
use yapfast_ui::{ThemeContent, YapfastWindow};

use super::name_style;

/// The colours of the theme showing that content is drawn with, ARGB.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Colours {
    pub dark: bool,
    pub chat: u32,
    pub accent: u32,
}

thread_local! {
    static SHOWN: Cell<Option<(Content, Colours)>> = const { Cell::new(None) };
    /// What pictures are recoloured with. An animation's decoding thread is
    /// handed it as it starts.
    static MEDIA: Cell<Media> = const { Cell::new(Media::Normal) };
    /// Performance is on Light: a `dither` theme's pictures take its palette
    /// as a gradient, without the pattern.
    static LIGHT: Cell<bool> = const { Cell::new(false) };
    /// What runs on the window's thread once pictures were dithered off it.
    static ON_DITHERED: RefCell<Option<Box<dyn Fn()>>> = RefCell::default();
}

/// The pattern pictures are dithered with now, for the threads that decode
/// them: a picture decoded meanwhile is dithered there, as it's made.
static PATTERN: RwLock<Option<Dither>> = RwLock::new(None);

/// The pattern pictures are dithered with now, from any thread.
pub fn decoding_pattern() -> Option<Dither> {
    *PATTERN
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// The pattern pictures are dithered with, on the window's thread: the
/// theme's `dither` while its content shows, unless Performance is Light.
pub fn pattern() -> Option<Dither> {
    match media() {
        Media::Dither(dither) if !LIGHT.get() => Some(dither),
        _ => None,
    }
}

fn publish_pattern() {
    *PATTERN
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = pattern();
}

/// Performance is on Light, or not. True when that changed whether
/// pictures are dithered, so converted ones are let go.
pub fn set_light(light: bool) -> bool {
    let before = pattern();
    LIGHT.set(light);
    publish_pattern();
    before != pattern()
}

/// Runs `redraw` on the window's thread whenever pictures were dithered off
/// it, so the ones converted meanwhile are converted again. Called once.
pub fn on_dithered(redraw: impl Fn() + 'static) {
    ON_DITHERED.set(Some(Box::new(redraw)));
}

/// A picture's dithered copy, made once off the window's thread and kept
/// with the picture, for as long as the pattern doesn't change.
#[derive(Default)]
pub struct DitherSlot(Mutex<Dithered>);

#[derive(Default)]
enum Dithered {
    #[default]
    None,
    Pending(Dither),
    Done(Dither, slint::SharedPixelBuffer<slint::Rgba8Pixel>),
}

impl DitherSlot {
    /// A slot holding `source` dithered with `pattern` already, when the
    /// thread making it can afford it.
    pub fn made(pattern: Dither, source: &slint::SharedPixelBuffer<slint::Rgba8Pixel>) -> Self {
        Self(Mutex::new(Dithered::Done(
            pattern,
            dithered(pattern, source),
        )))
    }

    /// The copy dithered with `pattern`, once made.
    pub fn get(&self, pattern: Dither) -> Option<slint::SharedPixelBuffer<slint::Rgba8Pixel>> {
        match &*self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
        {
            Dithered::Done(made, buffer) if *made == pattern => Some(buffer.clone()),
            _ => None,
        }
    }
}

/// A copy of `source` dithered with `pattern`.
fn dithered(
    pattern: Dither,
    source: &slint::SharedPixelBuffer<slint::Rgba8Pixel>,
) -> slint::SharedPixelBuffer<slint::Rgba8Pixel> {
    let mut buffer = source.clone();
    let width = buffer.width();
    dither(&pattern, buffer.make_mut_bytes(), width);
    buffer
}

struct Job {
    source: slint::SharedPixelBuffer<slint::Rgba8Pixel>,
    slot: Arc<DitherSlot>,
    pattern: Dither,
}

/// Has `source` dithered with `pattern` into `slot` on the dithering
/// thread, unless it's made or on its way; meanwhile the picture shows its
/// palette as a gradient. Once the queue is empty the window converts the
/// pictures again (see [`on_dithered`]).
pub fn queue(
    source: &slint::SharedPixelBuffer<slint::Rgba8Pixel>,
    slot: &Arc<DitherSlot>,
    pattern: Dither,
) {
    {
        let mut state = slot
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match &*state {
            Dithered::Pending(made) | Dithered::Done(made, _) if *made == pattern => return,
            _ => *state = Dithered::Pending(pattern),
        }
    }
    static SEND: Mutex<Option<mpsc::Sender<Job>>> = Mutex::new(None);
    let mut send = SEND
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let job = Job {
        source: source.clone(),
        slot: slot.clone(),
        pattern,
    };
    let job = match send.as_ref() {
        Some(sender) => match sender.send(job) {
            Ok(()) => return,
            // The thread ended: start another.
            Err(mpsc::SendError(job)) => job,
        },
        None => job,
    };
    let (sender, jobs) = mpsc::channel();
    let started = std::thread::Builder::new()
        .name("yapfast-dither".into())
        .spawn(move || dither_jobs(&jobs));
    match started {
        Ok(_) => {
            let _ = sender.send(job);
            *send = Some(sender);
        }
        Err(error) => tracing::warn!(%error, "could not start dithering pictures"),
    }
}

/// The dithering thread: each job in turn, then, once none wait, one call
/// to the window to show them.
fn dither_jobs(jobs: &mpsc::Receiver<Job>) {
    while let Ok(job) = jobs.recv() {
        run(job);
        while let Ok(job) = jobs.try_recv() {
            run(job);
        }
        let _ = slint::invoke_from_event_loop(|| {
            ON_DITHERED.with_borrow(|redraw| {
                if let Some(redraw) = redraw {
                    redraw();
                }
            });
        });
    }
}

fn run(job: Job) {
    // Unless the pattern changed while it waited.
    let wanted = matches!(
        &*job.slot.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner),
        Dithered::Pending(pattern) if *pattern == job.pattern
    );
    if !wanted {
        return;
    }
    let buffer = dithered(job.pattern, &job.source);
    let mut state = job
        .slot
        .0
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if matches!(&*state, Dithered::Pending(pattern) if *pattern == job.pattern) {
        *state = Dithered::Done(job.pattern, buffer);
    }
}

/// What pictures are recoloured with now.
pub fn media() -> Media {
    MEDIA.get()
}

/// Shows `content`, or takes it away with `None`. True when what Rust builds
/// with it changed, so rows, names and profile cards are built again; the
/// caller also lets go of converted pictures when [`media_changed`] says.
pub fn show(window: &YapfastWindow, content: Option<(Content, Colours)>) -> Change {
    let content = content.filter(|(content, _)| !content.is_stock());
    let before = SHOWN.get();
    let global = window.global::<ThemeContent>();
    let stock = Content::default();
    let shown = content.map_or(stock, |(content, _)| content);
    let avatars = shown.avatars;
    global.set_avatar_shape(match avatars.shape {
        AvatarShape::Keep => 0,
        AvatarShape::Circle => 1,
        AvatarShape::Square => 2,
        AvatarShape::Rounded => 3,
    });
    global.set_avatar_radius(avatars.radius);
    global.set_hide_decorations(avatars.decorations == Shown::Hide || avatars.ring.is_some());
    let (ring, width) = avatars.ring.unwrap_or((0, 0.0));
    global.set_ring(slint::Color::from_argb_encoded(ring));
    global.set_ring_width(width);
    global.set_nameplates(match shown.names.nameplates {
        Nameplates::Keep => 0,
        Nameplates::Hide => 1,
        Nameplates::Tint => 2,
    });
    global.set_embeds(shown.embeds == Follow::Theme);
    global.set_activity(shown.activity == Follow::Theme);
    let media_before = media();
    MEDIA.set(shown.media);
    publish_pattern();
    SHOWN.set(content);
    Change {
        rows: before != content,
        media: media_before != shown.media,
    }
}

/// What changed when content was shown or taken away.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Change {
    /// Names, styles, embeds or profiles: rows are built again.
    pub rows: bool,
    /// Pictures are recoloured otherwise: converted ones are let go.
    pub media: bool,
}

/// A copy of `rgba` recoloured as the theme says, or `None` while pictures
/// show as they are, when the caller shares the original.
pub fn recoloured(
    rgba: &[u8],
    width: u32,
    height: u32,
) -> Option<slint::SharedPixelBuffer<slint::Rgba8Pixel>> {
    let media = media();
    if media == Media::Normal {
        return None;
    }
    let mut buffer = slint::SharedPixelBuffer::clone_from_slice(rgba, width, height);
    recolour(media, buffer.make_mut_bytes());
    Some(buffer)
}

/// Recolours a buffer just made, such as an animation's frame, in place, as
/// `media` (from [`media`] on the UI thread) says; a `dither` theme's
/// pattern is drawn where pictures are dithered now.
pub fn recolour_buffer(media: Media, buffer: &mut slint::SharedPixelBuffer<slint::Rgba8Pixel>) {
    let (width, height) = (buffer.width(), buffer.height());
    match (media, decoding_pattern()) {
        (Media::Normal, _) => {}
        (Media::Dither(_), Some(pattern)) if Dither::patterned(width, height) => {
            dither(&pattern, buffer.make_mut_bytes(), width);
        }
        _ => recolour(media, buffer.make_mut_bytes()),
    }
}

/// A name's role colour as it shows, RGB; `None` draws it in the theme's
/// text colour.
pub fn name(rgb: Option<u32>) -> Option<u32> {
    let Some((content, colours)) = SHOWN.get() else {
        return rgb;
    };
    match content.names.colours {
        NameColours::Keep => rgb,
        NameColours::Theme => content.names.colour.map(|argb| argb & 0xff_ffff),
        NameColours::Clamp => rgb.map(|rgb| clamp(rgb, &content, colours)),
        // Without someone to pick for, the palette's first.
        NameColours::Palette => content.names.palette().first().map(|argb| argb & 0xff_ffff),
    }
}

/// [`name`] for someone by their id, which a `palette` picks their colour
/// from.
pub fn name_of(id: &str, rgb: Option<u32>) -> Option<u32> {
    palette_of(id).or_else(|| name(rgb))
}

/// Someone's colour from the theme's names `palette`, RGB, while it shows.
pub fn palette_of(id: &str) -> Option<u32> {
    let (content, _) = SHOWN.get()?;
    if content.names.colours != NameColours::Palette {
        return None;
    }
    content
        .names
        .palette_colour(id)
        .map(|argb| argb & 0xff_ffff)
}

/// Whether the theme draws names in its own colours, so mentions leave their
/// roles' colours for the theme's chips.
pub fn styles_names() -> bool {
    SHOWN
        .get()
        .is_some_and(|(content, _)| content.names.colours != NameColours::Keep)
}

/// `rgb` with its hue kept and its saturation and lightness in the theme's
/// ranges, then made to read 4.5:1 on the chat.
fn clamp(rgb: u32, content: &Content, colours: Colours) -> u32 {
    let ((low_s, high_s), (low_l, high_l)) = content.names.clamp_ranges(colours.dark);
    let (hue, saturation, lightness) = name_style::to_hsl(name_style::unpack(rgb));
    let clamped = name_style::pack(name_style::from_hsl(
        hue,
        saturation.clamp(f64::from(low_s), f64::from(high_s)),
        lightness.clamp(f64::from(low_l), f64::from(high_l)),
    ));
    name_style::readable(clamped, colours.chat & 0xff_ffff, 4.5)
}

/// Whether people's own name styles give way to the theme's names.
pub fn plain_names() -> bool {
    SHOWN
        .get()
        .is_some_and(|(content, _)| content.names.styles == Follow::Theme)
}

/// An embed's or a card's colour bar as it shows, RGB.
pub fn embed(rgb: Option<u32>) -> Option<u32> {
    match SHOWN.get() {
        Some((content, colours)) if content.embeds == Follow::Theme => {
            Some(colours.accent & 0xff_ffff)
        }
        _ => rgb,
    }
}

/// Whether profile cards leave their own theme colours for the theme's card.
pub fn plain_profiles() -> bool {
    SHOWN
        .get()
        .is_some_and(|(content, _)| content.profiles.themes == Follow::Theme)
}

/// Whether profile effects are left out.
pub fn no_profile_effects() -> bool {
    SHOWN
        .get()
        .is_some_and(|(content, _)| content.profiles.effects == Shown::Hide)
}

#[cfg(test)]
pub(super) fn reset_for_tests() {
    SHOWN.set(None);
    MEDIA.set(Media::Normal);
    LIGHT.set(false);
    publish_pattern();
}

#[cfg(test)]
mod tests {
    use super::*;
    use yapfast_core::themes::content::Names;

    fn colours() -> Colours {
        Colours {
            dark: true,
            chat: 0xff1e_1f24,
            accent: 0xff5c_ff85,
        }
    }

    #[test]
    fn names_follow_the_theme_or_keep_their_hue_readably() {
        i_slint_backend_testing::init_no_event_loop();
        let window = YapfastWindow::new().unwrap();
        reset_for_tests();
        assert_eq!(name(Some(0x123456)), Some(0x123456));
        let theme = Content {
            names: Names {
                colours: NameColours::Theme,
                ..Names::default()
            },
            ..Content::default()
        };
        assert!(show(&window, Some((theme, colours()))).rows);
        assert_eq!(name(Some(0x123456)), None);
        let clamp = Content {
            names: Names {
                colours: NameColours::Clamp,
                ..Names::default()
            },
            embeds: Follow::Theme,
            ..Content::default()
        };
        show(&window, Some((clamp, colours())));
        // A dark navy keeps its blue and comes up to read on the chat.
        let shown = name(Some(0x101040)).unwrap();
        let (hue, _, _) = name_style::to_hsl(name_style::unpack(shown));
        assert!((hue - 240.0).abs() < 2.0, "{hue}");
        assert!(name_style::to_hsl(name_style::unpack(shown)).2 >= 0.59);
        assert_eq!(embed(Some(0xff0000)), Some(0x5cff85));
        let change = show(&window, None);
        assert!(change.rows && !change.media);
        assert_eq!(name(Some(0x101040)), Some(0x101040));
        assert_eq!(embed(Some(0xff0000)), Some(0xff0000));
    }

    fn dithering() -> Content {
        let dither = match yapfast_core::themes::package::Package::parse(
            r##"{ "theme_api": 1, "id": "com.example.dither", "name": "Dither", "version": "1.0.0",
                "content": { "media": { "dither": { "palette": ["#020A04", "#2F8A4A", "#B8FFC8"] } } },
                "variants": { "dark": { "base": "yap" } } }"##,
        )
        .unwrap()
        .resolve(yapfast_core::themes::package::Mode::Dark)
        .unwrap()
        .content
        .media
        {
            Media::Dither(dither) => dither,
            other => panic!("{other:?}"),
        };
        Content {
            media: Media::Dither(dither),
            ..Content::default()
        }
    }

    #[test]
    fn theme_dither_is_made_once_off_the_window_and_light_keeps_a_gradient() {
        i_slint_backend_testing::init_no_event_loop();
        let window = YapfastWindow::new().unwrap();
        reset_for_tests();
        assert!(pattern().is_none() && decoding_pattern().is_none());
        assert!(show(&window, Some((dithering(), colours()))).media);
        // (The decoding threads' copy is the process's, which tests running
        // beside this one may clear, so only this thread's is held to it.)
        let on = pattern().expect("dithered while content shows");
        // A picture's slot: made where it's decoded, or by the thread.
        let source = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(
            &[64u8, 64, 64, 255].repeat(100 * 100),
            100,
            100,
        );
        let made = DitherSlot::made(on, &source);
        let copy = made.get(on).unwrap();
        let colours: std::collections::BTreeSet<[u8; 3]> = copy
            .as_bytes()
            .chunks_exact(4)
            .map(|pixel| [pixel[0], pixel[1], pixel[2]])
            .collect();
        assert_eq!(colours.len(), 2, "a pattern of two palette colours");
        let slot = Arc::new(DitherSlot::default());
        assert!(slot.get(on).is_none());
        run(Job {
            source: source.clone(),
            slot: slot.clone(),
            pattern: on,
        });
        // Nothing waited for it, so nothing was made.
        assert!(slot.get(on).is_none());
        *slot.0.lock().unwrap() = Dithered::Pending(on);
        run(Job {
            source: source.clone(),
            slot: slot.clone(),
            pattern: on,
        });
        assert_eq!(slot.get(on).unwrap().as_bytes(), copy.as_bytes());
        // Light: the palette as a gradient, no pattern, nothing to redo
        // twice.
        assert!(set_light(true));
        assert!(pattern().is_none() && decoding_pattern().is_none());
        assert!(!set_light(true));
        let mut frame = source.clone();
        recolour_buffer(media(), &mut frame);
        let flat: std::collections::BTreeSet<[u8; 4]> = frame
            .as_bytes()
            .chunks_exact(4)
            .map(|pixel| [pixel[0], pixel[1], pixel[2], pixel[3]])
            .collect();
        assert_eq!(flat.len(), 1);
        assert!(set_light(false));
        // Content turned off: no pattern.
        assert!(show(&window, None).media);
        assert!(pattern().is_none() && decoding_pattern().is_none());
        reset_for_tests();
    }

    #[test]
    fn pictures_are_shared_until_a_theme_recolours_them() {
        i_slint_backend_testing::init_no_event_loop();
        let window = YapfastWindow::new().unwrap();
        reset_for_tests();
        assert!(recoloured(&[1, 2, 3, 4], 1, 1).is_none());
        let mono = Content {
            media: Media::Mono,
            avatars: yapfast_core::themes::content::Avatars {
                shape: AvatarShape::Square,
                ..Default::default()
            },
            ..Content::default()
        };
        assert!(show(&window, Some((mono, colours()))).media);
        assert_eq!(window.global::<ThemeContent>().get_avatar_shape(), 2);
        let grey = recoloured(&[255, 0, 0, 200], 1, 1).unwrap();
        let pixel = grey.as_bytes();
        assert_eq!((pixel[0], pixel[1], pixel[3]), (pixel[2], pixel[2], 200));
        assert!(show(&window, None).media);
        assert_eq!(window.global::<ThemeContent>().get_avatar_shape(), 0);
        assert!(recoloured(&[1, 2, 3, 4], 1, 1).is_none());
    }
}
