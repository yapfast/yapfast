//! Installed themes on the window: a `theme.json` package's colours, shape,
//! type, spacing, motion, declared animations and text effects, applied over
//! a built-in theme through the `Look` global and the theme-only colours of
//! `Theme` (`ui/theme.slint`). See `docs/themes.md`.
//!
//! Everything here is undone by [`reset`], which puts every token back to its
//! stock value, so a theme that fails, a protected dialog, Ctrl+Shift+T and
//! `--default-theme` all come back to exactly the stock window. Nothing a
//! theme names runs: its fonts are parsed before they're registered, and its
//! animations are values this module computes and sets.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use slint::{Brush, Color, ComponentHandle, Image, ModelRc, SharedString, VecModel};
use yapfast_core::themes::check::{self, Files, Strictness};
use yapfast_core::themes::install::{Fetched, Installed, Themes};
use yapfast_core::themes::motion;
use yapfast_core::themes::package::{
    Close, Hover, Mode, Paint, Press, Property, Resolved, Role, Slot, Surface, TextSlot, Transition,
};
use yapfast_core::themes::{Package, Performance};
use yapfast_ui::{
    ClientStyle, Look, Shape, SlotMotion, ThemeListing, ThemeReview, ThemesPage, YapfastWindow,
};

use super::theme;

/// The prefix of an installed theme's id in `layout.json`'s `theme`. No
/// built-in or custom id has it.
pub const INSTALLED: &str = "installed:";

/// How long Try shows a theme before going back.
const TRIAL: u64 = 30;

// Started with `--default-theme`, or Ctrl+Shift+T was pressed: the default
// theme shows for this run, whatever is chosen.
thread_local! {
    static DEFAULT_THEME: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}
/// `--theme-dev <folder>`: the folder to show and reload on save.
static DEV: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// Takes `--default-theme` and `--theme-dev <folder>` out of the arguments.
pub fn take_flags(arguments: &mut Vec<String>) -> Result<(), String> {
    if let Some(at) = arguments
        .iter()
        .position(|argument| argument == "--default-theme")
    {
        arguments.remove(at);
        DEFAULT_THEME.set(true);
    }
    if let Some(at) = arguments
        .iter()
        .position(|argument| argument == "--theme-dev")
    {
        arguments.remove(at);
        if at >= arguments.len() {
            return Err("--theme-dev needs the folder of a theme".into());
        }
        let folder = PathBuf::from(arguments.remove(at));
        let _ = DEV.set(folder);
    }
    Ok(())
}

/// `yapfast theme check <folder> [--marketplace]`: the checks an install and
/// the marketplace run, printed. Returns the exit status.
pub fn run_check(arguments: &[String]) -> i32 {
    register_bases();
    let marketplace = arguments.iter().any(|argument| argument == "--marketplace");
    let Some(folder) = arguments
        .iter()
        .find(|argument| !argument.starts_with("--"))
    else {
        eprintln!("usage: yapfast theme check <folder> [--marketplace]");
        return 2;
    };
    let files = match Files::read(Path::new(folder)) {
        Ok(files) => files,
        Err(error) => {
            println!("error: {error}\nfailed");
            return 1;
        }
    };
    let report = check::check(
        &files,
        if marketplace {
            Strictness::Marketplace
        } else {
            Strictness::Install
        },
    );
    print!("{}", report.text());
    i32::from(!report.passed())
}

/// Tells the checks where the built-in themes' colours are, which a theme's
/// own colours are checked over.
pub fn register_bases() {
    check::set_base_colours(|base, token| {
        let built_in = theme::find(base)?;
        let index = theme::token_index(token)?;
        built_in
            .theme
            .colour(index)
            .map(|colour| colour.as_argb_encoded())
    });
}

/// What is drawn now, beyond a built-in theme.
struct Applied {
    /// `installed:<id>`, or `dev` for `--theme-dev`.
    id: String,
    name: String,
    resolved: Resolved,
    /// The theme's folder, which its pictures are read from.
    folder: PathBuf,
    /// Font families registered for each role.
    fonts: Vec<(Role, String)>,
    /// Families registered for ransom names, in the theme's order.
    ransom: Vec<String>,
}

/// A theme on trial: what to go back to, and when.
struct Trial {
    back_to: Option<String>,
    until: Instant,
    /// The installed theme it is, or the reviewed one to install on Keep.
    installed: Option<String>,
    timer: slint::Timer,
}

/// The text a slot's effect started on, and when.
struct Playing {
    text: SharedString,
    started: Instant,
    seed: u64,
}

#[derive(Default)]
struct State {
    root: Option<PathBuf>,
    installed: Vec<Installed>,
    applied: Option<Applied>,
    performance: Performance,
    reduced: bool,
    protected: bool,
    clock: Option<(Instant, slint::Timer)>,
    effects: HashMap<usize, Playing>,
    fx_timer: Option<slint::Timer>,
    trial: Option<Trial>,
    review: Option<Fetched>,
    /// Families registered this run, by the font's digest: a font is held
    /// for the rest of the run once registered, so it's registered once.
    families: HashMap<u64, String>,
    dev_stamp: Option<SystemTime>,
    dev_timer: Option<slint::Timer>,
    /// Why the chosen theme couldn't be shown, for a notice.
    failed: Option<String>,
    /// Installed themes whose `theme.json` this Yapfast refused, and why,
    /// so choosing one says so instead of nothing.
    refused: Vec<(String, String)>,
    /// A theme made ready off the UI thread, by its id, or why it can't be.
    ready: Option<(String, Result<Ready, String>)>,
    /// The latest [`prepare`], and what to do once it's ready.
    preparing: u64,
    when_ready: Option<Then>,
}

/// What to do once [`prepare`] has made a theme ready.
type Then = Box<dyn FnOnce(&YapfastWindow)>;

/// What showing a theme takes that's slow, made by [`prepare`] on another
/// thread: its files checked, its fonts read, its sounds decoded and its
/// pictures baked. The UI thread only registers and sets them.
pub struct Ready {
    package: Package,
    folder: PathBuf,
    mode: Mode,
    resolved: Resolved,
    /// Font files, checked, by their path in the folder.
    fonts: HashMap<PathBuf, Arc<Vec<u8>>>,
    sounds: HashMap<u64, Arc<[i16]>>,
    art: super::theme_art::Baked,
}

thread_local! {
    static STATE: std::cell::RefCell<State> = std::cell::RefCell::new(State::default());
}

/// Reads the installed themes. Called once at start, before the window's
/// first frame, and again after an install.
pub fn init(root: &Path) {
    register_bases();
    STATE.with_borrow_mut(|state| {
        state.root = Some(root.to_owned());
        (state.installed, state.refused) = Themes::open(root).installed_and_refused();
    });
}

/// Whether this run shows the default theme whatever is chosen.
pub fn default_only() -> bool {
    DEFAULT_THEME.get()
}

/// The name Appearance shows for a theme id, built-in, custom or installed.
pub fn name_of(id: &str, custom: &[theme::Custom]) -> Option<String> {
    if let Some(installed) = id.strip_prefix(INSTALLED) {
        return STATE.with_borrow(|state| {
            state
                .installed
                .iter()
                .find(|theme| theme.id == installed)
                .map(|theme| theme.package.name.clone())
        });
    }
    theme::find(id)
        .map(|built_in| built_in.name.to_owned())
        .or_else(|| {
            custom
                .iter()
                .find(|theme| theme.id == id)
                .map(|theme| theme.name.clone())
        })
}

/// The colours of a resolved theme: its base's, with its own over them.
fn colours_of(resolved: &Resolved) -> theme::Theme {
    let mut colours = theme::find(&resolved.base).map_or(theme::YAP, |built_in| built_in.theme);
    colours.dark = resolved.dark;
    for (token, paint) in &resolved.colours {
        if let Some(slot) = theme::token_index(token).and_then(|index| colours.colour_mut(index)) {
            *slot = Color::from_argb_encoded(paint.solid());
        }
    }
    colours
}

/// The variant the system asks for: dark unless its windows are light.
fn wanted_mode(window: &YapfastWindow) -> Mode {
    use slint::winit_030::WinitWindowAccessor as _;
    let light = window
        .window()
        .with_winit_window(|winit| winit.theme())
        .flatten()
        .is_some_and(|theme| theme == slint::winit_030::winit::window::Theme::Light);
    if light { Mode::Light } else { Mode::Dark }
}

/// Shows an installed theme, or `--theme-dev`'s folder, by its id. Returns
/// the colours to apply, or why it can't be shown; the caller falls back.
pub fn show(window: &YapfastWindow, id: &str) -> Result<theme::Theme, String> {
    if let Some(ready) = take_ready(window, id) {
        let ready = ready?;
        let (package, folder) = (ready.package.clone(), ready.folder.clone());
        return apply_package(window, id, &package, &folder, Some(ready));
    }
    if id == "dev" {
        let folder = DEV.get().ok_or("no --theme-dev folder")?.clone();
        return show_folder(window, &folder);
    }
    let installed_id = id.strip_prefix(INSTALLED).ok_or("not an installed theme")?;
    let (package, folder) = STATE
        .with_borrow(|state| {
            state
                .installed
                .iter()
                .find(|theme| theme.id == installed_id)
                .map(|theme| (theme.package.clone(), theme.folder.clone()))
        })
        .ok_or_else(|| refused_reason(installed_id))?;
    // An installed theme was checked when installed; its files are checked
    // again here, since the store is on disk where anything could change it.
    let files = Files::read(&folder)?;
    let report = check::check(&files, Strictness::Install);
    if let Some(reason) = report.reason() {
        return Err(reason.to_owned());
    }
    apply_package(window, id, &package, &folder, None)
}

/// Why an installed theme can't be found: this Yapfast refused its
/// `theme.json`, or it isn't installed.
fn refused_reason(id: &str) -> String {
    STATE.with_borrow(|state| {
        state
            .refused
            .iter()
            .find(|(refused, _)| refused == id)
            .map_or_else(
                || "that theme isn't installed any more".to_owned(),
                |(_, reason)| format!("its theme.json was refused: {reason}"),
            )
    })
}

/// Forgets a [`prepare`] still under way, and anything it made ready: the
/// person chose another theme meanwhile.
pub fn cancel_prepare() {
    STATE.with_borrow_mut(|state| {
        state.preparing += 1;
        state.when_ready = None;
        state.ready = None;
    });
}

/// Whether [`prepare`] has made `id` ready, or found why it can't be.
pub fn is_ready(id: &str) -> bool {
    STATE.with_borrow(|state| state.ready.as_ref().is_some_and(|(ready, _)| ready == id))
}

/// Takes what [`prepare`] made for `id`, if it's still right for the window:
/// the variant the system asks for may have changed since.
fn take_ready(window: &YapfastWindow, id: &str) -> Option<Result<Ready, String>> {
    let (ready_id, ready) = STATE.with_borrow_mut(|state| state.ready.take())?;
    if ready_id != id {
        return None;
    }
    match ready {
        Ok(ready) if ready.package.mode_for(wanted_mode(window)) != ready.mode => None,
        ready => Some(ready),
    }
}

/// Makes an installed theme, or `--theme-dev`'s folder, ready to show on
/// another thread, so the window never waits on its files: the theme on
/// the window stays until `then` runs on the UI thread, which shows it with
/// [`show`] at no more cost than setting its values. A later call
/// supersedes this one.
pub fn prepare(window: &YapfastWindow, id: &str, then: impl FnOnce(&YapfastWindow) + 'static) {
    let folder = if id == "dev" {
        DEV.get()
            .cloned()
            .ok_or_else(|| "no --theme-dev folder".to_owned())
    } else {
        let installed_id = id.strip_prefix(INSTALLED).unwrap_or(id);
        STATE
            .with_borrow(|state| {
                state
                    .installed
                    .iter()
                    .find(|theme| theme.id == installed_id)
                    .map(|theme| theme.folder.clone())
            })
            .ok_or_else(|| refused_reason(installed_id))
    };
    let wanted = wanted_mode(window);
    let size = window.window().size();
    let generation = STATE.with_borrow_mut(|state| {
        state.preparing += 1;
        state.when_ready = Some(Box::new(then));
        state.preparing
    });
    let weak = window.as_weak();
    let id = id.to_owned();
    let work = move || {
        let started = Instant::now();
        let ready = folder.and_then(|folder| make_ready(folder, wanted, (size.width, size.height)));
        tracing::info!(
            id,
            ms = started.elapsed().as_millis() as u64,
            "a theme was made ready off the UI thread"
        );
        let _ = slint::invoke_from_event_loop(move || {
            let Some(window) = weak.upgrade() else { return };
            let then = STATE.with_borrow_mut(|state| {
                if state.preparing != generation {
                    return None;
                }
                state.ready = Some((id, ready));
                state.when_ready.take()
            });
            if let Some(then) = then {
                then(&window);
            }
        });
    };
    if std::thread::Builder::new()
        .name("theme".into())
        .spawn(work)
        .is_err()
    {
        tracing::warn!("couldn't start a thread to make a theme ready");
    }
}

/// [`prepare`]'s work: everything [`show`] would do before setting values.
fn make_ready(folder: PathBuf, wanted: Mode, window: (u32, u32)) -> Result<Ready, String> {
    // An installed theme was checked when installed; its files are checked
    // again here, since the store is on disk where anything could change it.
    let files = Files::read(&folder)?;
    let report = check::check(&files, Strictness::Install);
    for warning in &report.warnings {
        tracing::info!(warning, "theme");
    }
    if let Some(reason) = report.reason() {
        return Err(reason.to_owned());
    }
    let package = report.package.ok_or("the theme has no package")?;
    let mode = package.mode_for(wanted);
    let resolved = package.resolve(mode).map_err(|error| error.0)?;
    let mut fonts = HashMap::new();
    let named = resolved
        .types
        .values()
        .filter_map(|kind| kind.font.as_ref())
        .chain(resolved.layout.names.iter().flat_map(|names| &names.fonts));
    for font in named {
        if let Some(bytes) = files.files.get(font) {
            fonts.insert(font.clone(), Arc::new(bytes.clone()));
        }
    }
    let mut sounds = HashMap::new();
    for file in resolved.sounds.values().flat_map(|choice| &choice.files) {
        if let Some(bytes) = files.files.get(file) {
            let key = super::theme_media::digest(bytes);
            if !sounds.contains_key(&key)
                && let Ok(samples) = yapfast_core::themes::media::decode_sound(file, bytes)
            {
                sounds.insert(key, Arc::from(samples));
            }
        }
    }
    let art = super::theme_art::bake(&resolved, &folder, window);
    Ok(Ready {
        package,
        folder,
        mode,
        resolved,
        fonts,
        sounds,
        art,
    })
}

/// Shows a theme's folder as it is now, for `--theme-dev`.
fn show_folder(window: &YapfastWindow, folder: &Path) -> Result<theme::Theme, String> {
    let files = Files::read(folder)?;
    let report = check::check(&files, Strictness::Install);
    for warning in &report.warnings {
        tracing::info!(warning, "theme-dev");
    }
    for note in &report.notes {
        tracing::info!(note, "theme-dev");
    }
    if let Some(reason) = report.reason() {
        return Err(reason.to_owned());
    }
    let package = report.package.expect("a passed check has its package");
    apply_package(window, "dev", &package, folder, None)
}

fn apply_package(
    window: &YapfastWindow,
    id: &str,
    package: &Package,
    folder: &Path,
    ready: Option<Ready>,
) -> Result<theme::Theme, String> {
    let started = Instant::now();
    let checked = ready.is_some();
    let (resolved, prepared) = match ready {
        Some(ready) => {
            super::theme_media::seed_sounds(ready.sounds);
            super::theme_art::take_baked(ready.art);
            (ready.resolved, ready.fonts)
        }
        None => {
            let mode = package.mode_for(wanted_mode(window));
            (
                package.resolve(mode).map_err(|error| error.0)?,
                HashMap::new(),
            )
        }
    };
    let font = |path: &Path| match prepared.get(path) {
        Some(bytes) => register_font_bytes(bytes, &folder.join(path)),
        None => register_font(&folder.join(path)),
    };
    let mut fonts = Vec::new();
    for (role, kind) in &resolved.types {
        if let Some(path) = &kind.font {
            fonts.push((*role, font(path)?));
        }
    }
    super::theme_media::load(
        &resolved,
        folder,
        |token| token_colour(&resolved, token).map(Color::from_argb_encoded),
        checked,
    )?;
    super::theme_cursor::load(&resolved, folder);
    let mut ransom = Vec::new();
    for path in resolved.layout.names.iter().flat_map(|names| &names.fonts) {
        ransom.push(font(path)?);
    }
    let colours = colours_of(&resolved);
    super::theme::set_mention_chip(resolved.paint("mention-chip").map(Paint::solid));
    STATE.with_borrow_mut(|state| {
        state.applied = Some(Applied {
            id: id.to_owned(),
            name: package.name.clone(),
            resolved,
            folder: folder.to_owned(),
            fonts,
            ransom,
        });
        state.failed = None;
    });
    paint(window);
    if checked {
        // The thread that made it ready left its decoding's memory free.
        super::trim();
    }
    tracing::info!(
        id,
        prepared = checked,
        ms = started.elapsed().as_millis() as u64,
        "a theme's values were set on the UI thread"
    );
    Ok(colours)
}

/// A colour token's value in a resolved theme: its own, its base's, or what
/// it follows.
pub(super) fn token_value(resolved: &Resolved, token: &str) -> Option<u32> {
    token_colour(resolved, token)
}

fn token_colour(resolved: &Resolved, token: &str) -> Option<u32> {
    resolved
        .paint(token)
        .map(Paint::solid)
        .or_else(|| check::base_colour(&resolved.base, token))
        .or_else(|| {
            yapfast_core::themes::package::follows(token)
                .and_then(|follow| token_colour(resolved, follow))
        })
}

/// Registers a theme's font under a family name of its own, after parsing it
/// whole, and returns the name.
fn register_font(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
    check::check_font(&bytes).map_err(|error| format!("{}: {error}", path.display()))?;
    register_font_bytes(&Arc::new(bytes), path)
}

/// Registers a font whose bytes were already read and checked, as
/// [`register_font`] does.
fn register_font_bytes(bytes: &Arc<Vec<u8>>, path: &Path) -> Result<String, String> {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    let digest = hasher.finish();
    if let Some(family) = STATE.with_borrow(|state| state.families.get(&digest).cloned()) {
        return Ok(family);
    }
    let family = format!("Yapfast Theme {digest:016x}");
    let mut collection = slint::fontique_012::shared_collection();
    let registered = collection.register_fonts(
        slint::fontique_012::fontique::Blob::new(bytes.clone()),
        Some(slint::fontique_012::fontique::FontInfoOverride {
            family_name: Some(&family),
            ..Default::default()
        }),
    );
    if registered.is_empty() {
        return Err(format!(
            "{} isn't a font Yapfast can draw with",
            path.display()
        ));
    }
    STATE.with_borrow_mut(|state| state.families.insert(digest, family.clone()));
    Ok(family)
}

/// Forgets the applied theme and puts every token back.
pub fn clear(window: &YapfastWindow) {
    STATE.with_borrow_mut(|state| {
        state.applied = None;
        state.clock = None;
        state.effects.clear();
        state.fx_timer = None;
    });
    super::theme::set_mention_chip(None);
    super::theme_art::forget();
    super::theme_lists::forget();
    reset(window);
    super::theme_media::forget();
    super::theme_cursor::forget();
}

/// Every token a theme sets, back to stock.
pub fn reset(window: &YapfastWindow) {
    super::theme_layout::reset(window);
    super::theme_lists::reset(window);
    super::theme_chrome::reset(window);
    super::theme_trim::reset(window);
    let look = window.global::<Look>();
    let stock = Shape {
        radius: -1.,
        border: -1.,
        shadow_blur: -1.,
        ..Shape::default()
    };
    for surface in Surface::ALL {
        set_shape(&look, surface, stock.clone());
    }
    look.set_radius_small(-1.);
    look.set_radius_medium(-1.);
    look.set_radius_large(-1.);
    look.set_row_inset_x(0.);
    look.set_row_inset_y(0.);
    look.set_card_inset_top(0.);
    look.set_shadows_on(false);
    for role in Role::ALL {
        set_font(&look, role, "");
    }
    look.set_size_headings(-1.);
    look.set_size_names(-1.);
    look.set_size_code(-1.);
    look.set_size_labels(-1.);
    look.set_weight_body(-1);
    look.set_size_header(-1.);
    look.set_size_title(-1.);
    look.set_weight_headings(-1);
    look.set_weight_names(-1);
    look.set_weight_labels(-1);
    look.set_spacing_body(0.);
    look.set_spacing_headings(0.);
    look.set_spacing_names(0.);
    look.set_spacing_labels(0.);
    look.set_headings_upper(false);
    look.set_avatar_size(-1.);
    look.set_chat_padding(-1.);
    look.set_member_row_height(-1.);
    look.set_hover(0);
    look.set_press(0);
    look.set_open(0);
    look.set_close_instant(false);
    look.set_curve(3);
    look.set_popup(100);
    let still = SlotMotion {
        opacity: 1.,
        scale: 1.,
        ..SlotMotion::default()
    };
    for slot in Slot::ALL {
        set_slot(&look, slot, still.clone());
    }
    look.set_window_tint(Color::from_argb_encoded(0));
    look.set_fx_channel(false);
    look.set_fx_server(false);
    look.set_fx_profile(false);
    look.set_fx_settings(false);
    look.set_fx_welcome(false);
    look.set_fx_name(false);
    let style = window.global::<ClientStyle>();
    style.set_text_size_theme(-1.);
    style.set_sidebar_width_theme(-1.);
    style.set_members_width_theme(-1.);
    style.set_channel_row_height_theme(-1.);
    style.set_message_gap_theme(-1.);
    style.set_message_group_gap_theme(-1.);
    let tokens = window.global::<yapfast_ui::Theme>();
    tokens.set_quick(120);
    tokens.set_hover_on(false);
    super::theme_art::reset(window);
    for token in yapfast_core::themes::package::EXTRA_COLOURS {
        set_extra(&tokens, token, None);
    }
    for token in yapfast_core::themes::package::BRUSHES {
        set_brush(&tokens, token, None);
    }
    super::theme_media::hide(window);
    super::theme_cursor::hide();
}

/// Sets every token of the applied theme, as Performance and reduced motion
/// allow, and starts what moves. Does nothing while a protected dialog is
/// open: that shows stock, and the theme comes back when it closes.
fn paint(window: &YapfastWindow) {
    reset(window);
    let protected = STATE.with_borrow(|state| state.protected);
    if protected {
        return;
    }
    if STATE.with_borrow(|state| state.applied.is_some()) {
        super::theme_media::show(window);
        super::theme_cursor::show();
    }
    STATE.with_borrow(|state| {
        let Some(applied) = &state.applied else {
            return;
        };
        let resolved = &applied.resolved;
        let names_face = applied
            .fonts
            .iter()
            .find(|(role, _)| *role == Role::Names)
            .map(|(_, family)| family.as_str());
        super::theme_layout::apply(
            window,
            &resolved.layout,
            &applied.ransom,
            names_face,
            state.performance,
            state.reduced,
        );
        super::theme_chrome::apply(window, &resolved.chrome);
        let family = |role: Role| {
            applied
                .fonts
                .iter()
                .find(|(shown, _)| *shown == role)
                .map(|(_, family)| family.as_str())
                .or((role == Role::Code).then_some("monospace"))
        };
        super::theme_chrome::apply_more(
            window,
            &resolved.chrome,
            resolved.chrome.font.and_then(family),
            state.performance == Performance::Light || state.reduced,
        );
        if let Some(initials) = resolved.content.avatars.initials {
            super::theme_content::set_initial_face(window, family(initials.font).unwrap_or(""));
        }
        super::theme_cursor::set_motion(state.performance, state.reduced);
        super::theme_trim::apply(window, resolved, &applied.fonts);
        let look = window.global::<Look>();
        let tokens = window.global::<yapfast_ui::Theme>();
        for (token, paint) in &resolved.colours {
            match paint {
                Paint::Colour(colour)
                    if yapfast_core::themes::package::EXTRA_COLOURS.contains(token) =>
                {
                    set_extra(&tokens, token, Some(Color::from_argb_encoded(*colour)));
                }
                Paint::Linear { .. } => set_brush(&tokens, token, Some(brush(paint))),
                Paint::Colour(_) => {}
            }
        }
        for (surface, shape) in &resolved.shapes {
            let (blur, x, y, colour) = shape.shadow.map_or((-1., 0., 0., 0), |shadow| {
                (shadow.blur, shadow.x, shadow.y, shadow.colour)
            });
            set_shape(
                &look,
                *surface,
                Shape {
                    radius: shape.radius.unwrap_or(-1.),
                    border: shape.border.unwrap_or(-1.),
                    border_color: Color::from_argb_encoded(shape.border_colour.unwrap_or(0)),
                    shadow_blur: blur,
                    shadow_x: x,
                    shadow_y: y,
                    shadow_color: Color::from_argb_encoded(colour),
                },
            );
        }
        if let Some((x, y)) = resolved
            .shapes
            .get(&Surface::Row)
            .and_then(|shape| shape.inset)
        {
            look.set_row_inset_x(x);
            look.set_row_inset_y(y);
        }
        if let Some(top) = resolved
            .shapes
            .get(&Surface::Card)
            .and_then(|shape| shape.inset_top)
        {
            look.set_card_inset_top(top);
        }
        let [small, medium, large] = resolved.radii;
        look.set_radius_small(small.unwrap_or(-1.));
        look.set_radius_medium(medium.unwrap_or(-1.));
        look.set_radius_large(large.unwrap_or(-1.));
        if let Some(shadows) = resolved.shadows {
            look.set_shadows_on(true);
            let shadow = shadows.unwrap_or(yapfast_core::themes::package::Shadow {
                blur: 0.,
                x: 0.,
                y: 0.,
                colour: 0,
            });
            look.set_shadows_blur(shadow.blur);
            look.set_shadows_x(shadow.x);
            look.set_shadows_y(shadow.y);
            look.set_shadows_color(Color::from_argb_encoded(shadow.colour));
        }
        for (role, family) in &applied.fonts {
            set_font(&look, *role, family);
        }
        let style = window.global::<ClientStyle>();
        for (role, kind) in &resolved.types {
            let size = kind.size.unwrap_or(-1.);
            let weight = kind.weight.unwrap_or(-1);
            let spacing = kind.letter_spacing.unwrap_or(0.);
            match role {
                Role::Body => {
                    style.set_text_size_theme(size);
                    look.set_spacing_body(spacing);
                    look.set_weight_body(weight);
                }
                Role::Headings => {
                    look.set_size_headings(size);
                    look.set_size_header(kind.header_size.unwrap_or(-1.));
                    look.set_size_title(kind.title_size.unwrap_or(-1.));
                    look.set_weight_headings(weight);
                    look.set_spacing_headings(spacing);
                    look.set_headings_upper(kind.uppercase.unwrap_or(false));
                }
                Role::Names => {
                    look.set_size_names(size);
                    look.set_weight_names(weight);
                    look.set_spacing_names(spacing);
                }
                Role::Code => look.set_size_code(size),
                Role::Small | Role::Time => {}
                Role::Labels => {
                    look.set_size_labels(size);
                    look.set_weight_labels(weight);
                    look.set_spacing_labels(spacing);
                }
            }
        }
        let spacing = &resolved.spacing;
        let or_stock = |value: Option<f32>| value.unwrap_or(-1.);
        style.set_message_gap_theme(or_stock(spacing.message_gap));
        style.set_message_group_gap_theme(or_stock(spacing.message_group_gap));
        style.set_channel_row_height_theme(or_stock(spacing.channel_row_height));
        style.set_sidebar_width_theme(or_stock(spacing.sidebar_width));
        style.set_members_width_theme(or_stock(spacing.members_width));
        look.set_avatar_size(or_stock(spacing.avatar_size));
        look.set_chat_padding(or_stock(spacing.chat_padding));
        look.set_member_row_height(or_stock(spacing.member_row_height));
        // Light keeps the stock motion; reduced motion keeps the stock
        // durations but no theme's flourish.
        let still = state.performance == Performance::Light || state.reduced;
        let motion = &resolved.motion;
        if !still {
            look.set_hover(motion.hover.unwrap_or(Hover::Fade).index());
            look.set_press(motion.press.unwrap_or(Press::None).index());
            look.set_open(motion.open.unwrap_or(Transition::Grow).index());
            look.set_close_instant(motion.close == Some(Close::None));
            if let Some(curve) = motion.curve {
                look.set_curve(curve.index());
            }
            if let Some(quick) = motion.quick {
                tokens.set_quick(quick as i64);
            }
            if let Some(popup) = motion.popup {
                look.set_popup(popup as i64);
            }
        }
        tokens.set_hover_on(resolved.paint("hover").is_some());
        super::theme_art::apply(
            window,
            resolved,
            &applied.folder,
            state.performance,
            state.reduced,
            (motion.hover_in, motion.hover_out),
        );
        super::theme_lists::apply(
            window,
            resolved,
            &applied.folder,
            &applied.fonts,
            state.performance,
            state.reduced,
        );
        let plays = motion::effects_play(state.performance, state.reduced);
        look.set_fx_name(
            plays
                && resolved
                    .art
                    .events
                    .contains_key(&yapfast_core::themes::package::art::EventSlot::NameHover),
        );
        for effect in &resolved.effects {
            let on = plays;
            match effect.slot {
                TextSlot::ChannelName => look.set_fx_channel(on),
                TextSlot::ServerName => look.set_fx_server(on),
                TextSlot::ProfileName => look.set_fx_profile(on),
                TextSlot::SettingsTitle => look.set_fx_settings(on),
                TextSlot::Welcome => look.set_fx_welcome(on),
            }
        }
    });
    start_clock(window);
}

pub(super) fn brush(paint: &Paint) -> Brush {
    match paint {
        Paint::Colour(colour) => Brush::SolidColor(Color::from_argb_encoded(*colour)),
        Paint::Linear { angle, stops } => {
            Brush::LinearGradient(i_slint_core::graphics::LinearGradientBrush::new(
                *angle,
                stops
                    .iter()
                    .map(|(colour, at)| i_slint_core::graphics::GradientStop {
                        color: Color::from_argb_encoded(*colour),
                        position: *at,
                    }),
            ))
        }
    }
}

fn set_shape(look: &Look<'_>, surface: Surface, shape: Shape) {
    match surface {
        Surface::Button => look.set_button(shape),
        Surface::Field => look.set_field(shape),
        Surface::Composer => look.set_composer(shape),
        Surface::Card => look.set_card(shape),
        Surface::Menu => look.set_menu(shape),
        Surface::Dialog => look.set_dialog(shape),
        Surface::Tooltip => look.set_tooltip(shape),
        Surface::Chip => look.set_chip(shape),
        Surface::Message => look.set_message(shape),
        Surface::RailIcon => look.set_rail_icon(shape),
        Surface::Avatar => look.set_avatar(shape),
        Surface::Tile => look.set_tile(shape),
        Surface::Code => look.set_code(shape),
        Surface::Embed => look.set_embed(shape),
        Surface::Toggle => look.set_toggle(shape),
        Surface::Actions => look.set_actions(shape),
        Surface::Row => look.set_row(shape),
        // Trim's: see `theme_trim::apply`.
        Surface::Label | Surface::Header | Surface::CardSection => {}
        Surface::CardButton => look.set_card_button(shape),
    }
}

fn set_font(look: &Look<'_>, role: Role, family: &str) {
    let family: SharedString = family.into();
    match role {
        Role::Body => look.set_font_body(family),
        Role::Headings => look.set_font_headings(family),
        Role::Names => look.set_font_names(family),
        Role::Code => look.set_font_code(family),
        Role::Labels => look.set_font_labels(family),
        // Trim's: see `theme_trim::apply`.
        Role::Small | Role::Time => {}
    }
}

fn set_slot(look: &Look<'_>, slot: Slot, motion: SlotMotion) {
    match slot {
        Slot::Home => look.set_home(motion),
        Slot::Unread => look.set_unread(motion),
        Slot::Mention => look.set_mention(motion),
        Slot::Typing => look.set_typing(motion),
        Slot::ChannelName => look.set_channel_name(motion),
        Slot::ServerName => look.set_server_name(motion),
        Slot::ProfileName => look.set_profile_name(motion),
        Slot::OwnAvatar => look.set_own_avatar(motion),
        // The rail's servers each take their own phase: see `tick`.
        Slot::Window | Slot::Accent | Slot::RailIcon => {}
    }
}

/// Sets a colour only a theme sets, or lets it follow its stock colour.
fn set_extra(tokens: &yapfast_ui::Theme<'_>, token: &str, colour: Option<Color>) {
    let on = colour.is_some();
    let colour = colour.unwrap_or_default();
    match token {
        "accent-text" => {
            tokens.set_accent_text_set(colour);
            tokens.set_accent_text_on(on);
        }
        "on-tooltip" => {
            tokens.set_on_tooltip_set(colour);
            tokens.set_on_tooltip_on(on);
        }
        "selection" => {
            tokens.set_selection_set(colour);
            tokens.set_selection_on(on);
        }
        "focus-ring" => {
            tokens.set_focus_ring_set(colour);
            tokens.set_focus_ring_on(on);
        }
        "scrollbar" => {
            tokens.set_scrollbar_set(colour);
            tokens.set_scrollbar_on(on);
        }
        "scrollbar-track" => {
            tokens.set_scrollbar_track_set(colour);
            tokens.set_scrollbar_track_on(on);
        }
        "scrim" => {
            tokens.set_scrim_set(colour);
            tokens.set_scrim_on(on);
        }
        "menu" => {
            tokens.set_menu_set(colour);
            tokens.set_menu_on(on);
        }
        "menu-line" => {
            tokens.set_menu_line_set(colour);
            tokens.set_menu_line_on(on);
        }
        "composer" => {
            tokens.set_composer_set(colour);
            tokens.set_composer_on(on);
        }
        "placeholder" => {
            tokens.set_placeholder_set(colour);
            tokens.set_placeholder_on(on);
        }
        // In `Theme` already, with these stock values.
        "composer-focus" => {
            tokens.set_composer_focus_set(colour);
            tokens.set_composer_focus_on(on);
        }
        "row-selected" => {
            tokens.set_row_selected_set(colour);
            tokens.set_row_selected_on(on);
        }
        "pill" => {
            tokens.set_pill_set(colour);
            tokens.set_pill_on(on);
        }
        "toggle-off" => {
            tokens.set_toggle_off_set(colour);
            tokens.set_toggle_off_on(on);
        }
        "knob-off" => {
            tokens.set_knob_off_set(colour);
            tokens.set_knob_off_on(on);
        }
        "code" => {
            tokens.set_code_set(colour);
            tokens.set_code_on(on);
        }
        "embed" => {
            tokens.set_embed_set(colour);
            tokens.set_embed_on(on);
        }
        "members" => {
            tokens.set_members_set(colour);
            tokens.set_members_on(on);
        }
        "app-badge" => {
            tokens.set_app_badge_set(colour);
            tokens.set_app_badge_on(on);
        }
        "account" => {
            tokens.set_account_set(colour);
            tokens.set_account_on(on);
        }
        "actions" => {
            tokens.set_actions_set(colour);
            tokens.set_actions_on(on);
        }
        "actions-hover" => {
            tokens.set_actions_hover_set(colour);
            tokens.set_actions_hover_on(on);
        }
        "row-selected-text" => {
            tokens.set_row_selected_text_set(colour);
            tokens.set_row_selected_text_on(on);
        }
        "card-button" => {
            tokens.set_card_button_set(colour);
            tokens.set_card_button_on(on);
        }
        "card-button-hover" => {
            tokens.set_card_button_hover_set(colour);
            tokens.set_card_button_hover_on(on);
        }
        "card-button-icon" => {
            tokens.set_card_button_icon_set(colour);
            tokens.set_card_button_icon_on(on);
        }
        "mentioned" => tokens.set_mentioned(if on {
            colour
        } else {
            Color::from_argb_encoded(0x1ff8_a500)
        }),
        "mentioned-hover" => {
            tokens.set_mentioned_hover(if on {
                colour
            } else {
                Color::from_argb_encoded(0x29f8_a500)
            });
        }
        _ => {}
    }
}

fn set_brush(tokens: &yapfast_ui::Theme<'_>, token: &str, paint: Option<Brush>) {
    let on = paint.is_some();
    let paint = paint.unwrap_or_default();
    match token {
        "rail" => {
            tokens.set_rail_paint(paint);
            tokens.set_rail_painted(on);
        }
        "sidebar" => {
            tokens.set_sidebar_paint(paint);
            tokens.set_sidebar_painted(on);
        }
        "chat" => {
            tokens.set_chat_paint(paint);
            tokens.set_chat_painted(on);
        }
        "panel" => {
            tokens.set_panel_paint(paint);
            tokens.set_panel_painted(on);
        }
        "raised" => {
            tokens.set_raised_paint(paint);
            tokens.set_raised_painted(on);
        }
        "accent" => {
            tokens.set_accent_paint(paint);
            tokens.set_accent_painted(on);
        }
        _ => {}
    }
}

/// Starts the clock that moves the applied theme's declared animations, at
/// the rate the fastest asks for, or stops it when none may run.
fn start_clock(window: &YapfastWindow) {
    let rate = STATE.with_borrow(|state| {
        let applied = state.applied.as_ref()?;
        if state.protected {
            return None;
        }
        applied
            .resolved
            .animations
            .iter()
            .filter(|animation| motion::runs(animation, state.performance, state.reduced))
            .map(|animation| motion::frame_rate(animation, state.performance))
            .max()
    });
    let Some(rate) = rate.filter(|rate| *rate > 0) else {
        STATE.with_borrow_mut(|state| state.clock = None);
        return;
    };
    let started = Instant::now();
    let timer = slint::Timer::default();
    let weak = window.as_weak();
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(u64::from(1000 / rate.max(1))),
        move || {
            if let Some(window) = weak.upgrade() {
                tick(&window, started);
            }
        },
    );
    STATE.with_borrow_mut(|state| state.clock = Some((started, timer)));
    tick_now(window, started);
}

fn tick_now(window: &YapfastWindow, started: Instant) {
    tick(window, started);
}

/// One frame of the declared animations.
fn tick(window: &YapfastWindow, started: Instant) {
    // Balanced holds still while another window has the focus.
    let paused = STATE
        .with_borrow(|state| state.performance == Performance::Balanced && !super::FOCUSED.get());
    if paused {
        return;
    }
    let elapsed = started.elapsed().as_secs_f32() * 1000.0;
    let look = window.global::<Look>();
    STATE.with_borrow(|state| {
        let Some(applied) = &state.applied else {
            return;
        };
        let mut slots: HashMap<Slot, SlotMotion> = HashMap::new();
        let mut rail = super::theme_art::rail_still();
        let mut rail_moved = false;
        let mut tint = None;
        let mut accent = None;
        for animation in &applied.resolved.animations {
            if !motion::runs(animation, state.performance, state.reduced) {
                continue;
            }
            if animation.slot == Slot::RailIcon {
                rail_moved = true;
                // Each place in the rail runs `phase` behind the one above.
                for (place, moved) in rail.iter_mut().enumerate() {
                    let at = elapsed - animation.phase * place as f32;
                    if let motion::Value::Number(number) = motion::value_at(animation, at) {
                        super::theme_art::move_by(moved, animation.property, number);
                    }
                }
                continue;
            }
            let value = motion::value_at(animation, elapsed);
            match (animation.slot, animation.property, value) {
                // Tints lay over one another, the later on top.
                (Slot::Window, Property::Tint, motion::Value::Colour(colour)) => {
                    tint = Some(tint.map_or(colour, |under| super::theme_art::over(under, colour)))
                }
                (Slot::Window, Property::Opacity, motion::Value::Number(opacity)) => {
                    // A window's opacity dims it under a tint of the chat's colour.
                    let ground = window
                        .global::<yapfast_ui::Theme>()
                        .get_chat()
                        .as_argb_encoded();
                    let alpha = ((1.0 - opacity).clamp(0.0, 1.0) * 255.0) as u32;
                    let colour = (alpha << 24) | (ground & 0xff_ffff);
                    tint = Some(tint.map_or(colour, |under| super::theme_art::over(under, colour)));
                }
                (Slot::Accent, Property::Colour, motion::Value::Colour(colour)) => {
                    accent = Some(colour)
                }
                (slot, property, motion::Value::Number(number)) => {
                    let moved = slots.entry(slot).or_insert(SlotMotion {
                        opacity: 1.,
                        scale: 1.,
                        ..SlotMotion::default()
                    });
                    match property {
                        Property::Opacity => moved.opacity *= number,
                        Property::X => moved.x += number,
                        Property::Y => moved.y += number,
                        Property::Scale => moved.scale *= number,
                        Property::Rotation => moved.rotation += number,
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        for (slot, moved) in slots {
            set_slot(&look, slot, moved);
        }
        if rail_moved {
            super::theme_art::set_rail(window, rail);
        }
        if let Some(tint) = tint {
            look.set_window_tint(Color::from_argb_encoded(tint));
        }
        if let Some(accent) = accent {
            window
                .global::<yapfast_ui::Theme>()
                .set_accent(Color::from_argb_encoded(accent));
        }
    });
}

/// A text slot's text while its effect plays: called by `Look.fx` with the
/// slot's index in `TextSlot::ALL`.
pub fn effect_text(window: &YapfastWindow, slot: i32, text: SharedString) -> SharedString {
    let Ok(slot) = usize::try_from(slot) else {
        return text;
    };
    let found = STATE.with(|state| {
        let Ok(mut state) = state.try_borrow_mut() else {
            return None;
        };
        let (effect, duration) = effect_for(&state.applied.as_ref()?.resolved, slot)?;
        let playing = state.effects.entry(slot).or_insert_with(|| Playing {
            text: SharedString::new(),
            started: Instant::now(),
            seed: 0,
        });
        if playing.text != text {
            playing.text = text.clone();
            playing.started = Instant::now();
            playing.seed = playing.seed.wrapping_add(0x9e37_79b9_7f4a_7c15);
        }
        let elapsed = playing.started.elapsed().as_secs_f32() * 1000.0;
        Some(motion::effect_text(
            effect,
            &text,
            elapsed,
            duration,
            playing.seed,
        ))
    });
    let Some((shown, playing)) = found else {
        return text;
    };
    if playing {
        keep_effects_moving(window);
    }
    shown.into()
}

/// The text slot index a name under the pointer plays its effect in, and
/// the top bar's right label; after `TextSlot::ALL`'s.
const NAME_SLOT: usize = 5;
const BAR_SLOT: usize = 6;

/// The effect a text slot plays, and its milliseconds: `effects`' slots,
/// then a name under the pointer (`art.events.name_hover`) and what's
/// playing in the top bar.
fn effect_for(
    resolved: &yapfast_core::themes::package::Resolved,
    slot: usize,
) -> Option<(yapfast_core::themes::package::Effect, f32)> {
    use yapfast_core::themes::chrome::BarEffect;
    use yapfast_core::themes::package::{Effect, art::EventSlot};
    match slot {
        NAME_SLOT => resolved
            .art
            .events
            .get(&EventSlot::NameHover)
            .map(|event| (Effect::Decrypt, event.duration)),
        BAR_SLOT => resolved
            .chrome
            .topbar
            .as_ref()
            .filter(|bar| bar.effect == BarEffect::Decrypt)
            .map(|_| (Effect::Decrypt, 700.0)),
        _ => resolved
            .effects
            .iter()
            .find(|effect| effect.slot.index() == slot)
            .map(|effect| (effect.effect, effect.duration)),
    }
}

/// A name came under the pointer: its effect plays from the start.
pub fn name_hovered(window: &YapfastWindow, name: SharedString) {
    restart(window, NAME_SLOT, name);
}

fn restart(window: &YapfastWindow, slot: usize, text: SharedString) {
    let restarted = STATE.with(|state| {
        let Ok(mut state) = state.try_borrow_mut() else {
            return false;
        };
        let playing = state.effects.entry(slot).or_insert_with(|| Playing {
            text: SharedString::new(),
            started: Instant::now(),
            seed: 0,
        });
        playing.text = text;
        playing.started = Instant::now();
        playing.seed = playing.seed.wrapping_add(0x9e37_79b9_7f4a_7c15);
        true
    });
    if restarted {
        keep_effects_moving(window);
    }
}

/// Advances `Look.fx-tick` while an effect plays, and stops once none does.
fn keep_effects_moving(window: &YapfastWindow) {
    let running = STATE.with(|state| {
        state
            .try_borrow()
            .map(|state| state.fx_timer.as_ref().is_some_and(slint::Timer::running))
            .unwrap_or(true)
    });
    if running {
        return;
    }
    let weak = window.as_weak();
    // Started after this binding is evaluated: a timer made inside a pure
    // callback mustn't change properties while they're read.
    slint::Timer::single_shot(Duration::ZERO, move || {
        let Some(window) = weak.upgrade() else { return };
        let timer = slint::Timer::default();
        let inner = window.as_weak();
        let mut quiet = 0;
        timer.start(
            slint::TimerMode::Repeated,
            Duration::from_millis(33),
            move || {
                let Some(window) = inner.upgrade() else {
                    return;
                };
                let look = window.global::<Look>();
                look.set_fx_tick(look.get_fx_tick().wrapping_add(1));
                // Stops once every effect has had its time; the next text change
                // starts it again.
                let longest = STATE.with_borrow(|state| {
                    state.applied.as_ref().map_or(0.0, |applied| {
                        (0..=BAR_SLOT)
                            .filter_map(|slot| effect_for(&applied.resolved, slot))
                            .map(|(_, duration)| duration)
                            .fold(0.0, f32::max)
                    })
                });
                let all_done = STATE.with_borrow(|state| {
                    state.effects.values().all(|playing| {
                        playing.started.elapsed().as_secs_f32() * 1000.0 > longest + 50.0
                    })
                });
                if all_done {
                    quiet += 1;
                    if quiet > 2 {
                        STATE.with_borrow_mut(|state| {
                            if let Some(timer) = &state.fx_timer {
                                timer.stop();
                            }
                        });
                    }
                } else {
                    quiet = 0;
                }
            },
        );
        STATE.with_borrow_mut(|state| state.fx_timer = Some(timer));
    });
}

/// A protected dialog opened or closed: while one is open, the window shows
/// the stock theme, shape and motion.
pub fn protect(window: &YapfastWindow, open: bool) -> Option<theme::Theme> {
    let changed = STATE.with_borrow_mut(|state| {
        let changed = state.protected != open && state.applied.is_some();
        state.protected = open;
        changed
    });
    if !changed {
        return None;
    }
    paint(window);
    if open {
        Some(theme::find("bonfire").map_or(theme::YAP, |built_in| built_in.theme))
    } else {
        STATE.with_borrow(|state| {
            state
                .applied
                .as_ref()
                .map(|applied| colours_of(&applied.resolved))
        })
    }
}

/// The Performance setting and reduced motion changed.
pub fn set_motion(window: &YapfastWindow, performance: Performance, reduced: bool) {
    STATE.with_borrow_mut(|state| {
        state.performance = performance;
        state.reduced = reduced;
    });
    if STATE.with_borrow(|state| state.applied.is_some()) {
        paint(window);
    }
}

/// Why the chosen theme couldn't show at start, once.
pub fn take_failure() -> Option<String> {
    STATE.with_borrow_mut(|state| state.failed.take())
}

pub fn note_failure(reason: String) {
    STATE.with_borrow_mut(|state| state.failed = Some(reason));
}

/// The theme to show at start: `--theme-dev`'s folder, or the chosen
/// installed theme, unless this run shows the default.
pub fn startup(window: &YapfastWindow, chosen: Option<&str>) -> Option<theme::Theme> {
    if default_only() {
        return None;
    }
    let id = if DEV.get().is_some() {
        "dev"
    } else {
        chosen.filter(|id| id.starts_with(INSTALLED))?
    };
    match show(window, id) {
        Ok(colours) => {
            if id == "dev" {
                watch_dev(window);
            }
            Some(colours)
        }
        Err(reason) => {
            let name = STATE.with_borrow(|state| {
                state
                    .installed
                    .iter()
                    .find(|theme| id.strip_prefix(INSTALLED) == Some(theme.id.as_str()))
                    .map_or_else(|| id.to_owned(), |theme| theme.package.name.clone())
            });
            tracing::warn!(id, reason, "a theme couldn't be shown; showing the default");
            note_failure(format!(
                "{name} couldn't be shown, so Yapfast is using its default theme: {reason}"
            ));
            clear(window);
            None
        }
    }
}

/// Reloads `--theme-dev`'s folder when a file in it changes: checked twice a
/// second, by the newest change time of the folder's files.
fn watch_dev(window: &YapfastWindow) {
    let Some(folder) = DEV.get().cloned() else {
        return;
    };
    let timer = slint::Timer::default();
    let weak = window.as_weak();
    STATE.with_borrow_mut(|state| state.dev_stamp = newest(&folder));
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(500),
        move || {
            let stamp = newest(&folder);
            let moved = STATE.with_borrow_mut(|state| {
                let moved = stamp != state.dev_stamp;
                state.dev_stamp = stamp;
                moved
            });
            if !moved {
                return;
            }
            let Some(window) = weak.upgrade() else { return };
            window.invoke_theme_reload();
        },
    );
    STATE.with_borrow_mut(|state| state.dev_timer = Some(timer));
}

/// The newest change in a folder, two levels deep.
fn newest(folder: &Path) -> Option<SystemTime> {
    fn walk(folder: &Path, depth: usize, newest: &mut Option<SystemTime>) {
        let Ok(entries) = std::fs::read_dir(folder) else {
            return;
        };
        for entry in entries.flatten() {
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            if let Ok(modified) = metadata.modified() {
                *newest = (*newest).max(Some(modified));
            }
            if metadata.is_dir() && depth > 0 {
                walk(&entry.path(), depth - 1, newest);
            }
        }
    }
    let mut found = None;
    walk(folder, 2, &mut found);
    found
}

/// The applied theme's id, if a package theme is shown.
pub fn applied_id() -> Option<String> {
    STATE.with_borrow(|state| state.applied.as_ref().map(|applied| applied.id.clone()))
}

pub fn applied_name() -> Option<String> {
    STATE.with_borrow(|state| state.applied.as_ref().map(|applied| applied.name.clone()))
}

// Settings → Themes.

/// A theme the marketplace lists.
#[derive(Clone, Debug, Default)]
pub struct Listed {
    pub id: String,
    pub name: String,
    pub author: String,
    pub version: String,
    pub description: String,
    pub modes: String,
    pub cost: String,
    pub reviewed: bool,
}

/// The marketplace's themes: the plugin catalog's entries of kind theme.
#[cfg(feature = "plugins")]
pub fn marketplace(window: &YapfastWindow) -> Vec<Listed> {
    super::plugin_settings::theme_listings(window)
        .into_iter()
        .map(|listing| Listed {
            id: listing.id,
            name: listing.name,
            author: listing.author,
            version: listing.version,
            description: listing.description,
            modes: listing.modes,
            cost: listing.cost,
            reviewed: listing.reviewed,
        })
        .collect()
}

/// A build without plugins has no marketplace.
#[cfg(not(feature = "plugins"))]
pub fn marketplace(_: &YapfastWindow) -> Vec<Listed> {
    Vec::new()
}

/// Fills the Themes tab: installed themes, and the marketplace's.
pub fn show_page(window: &YapfastWindow, listed: &[Listed]) {
    let page = window.global::<ThemesPage>();
    let query = page.get_query().to_lowercase();
    let applied = applied_id();
    let installed: Vec<ThemeListing> = STATE.with_borrow(|state| {
        state
            .installed
            .iter()
            .map(|theme| {
                let id = format!("{INSTALLED}{}", theme.id);
                listing_of(
                    &theme.package,
                    &theme.id,
                    Some(&theme.folder),
                    applied.as_deref() == Some(id.as_str()),
                    theme
                        .entry
                        .previous
                        .as_ref()
                        .map(|previous| previous.rev.chars().take(7).collect()),
                )
            })
            .collect()
    });
    let listed: Vec<ThemeListing> = listed
        .iter()
        .filter(|listing| {
            query.trim().is_empty()
                || [&listing.name, &listing.description, &listing.author]
                    .iter()
                    .any(|text| text.to_lowercase().contains(query.trim()))
        })
        .filter(|listing| {
            !STATE.with_borrow(|state| state.installed.iter().any(|theme| theme.id == listing.id))
        })
        .map(|listing| ThemeListing {
            id: listing.id.as_str().into(),
            name: listing.name.as_str().into(),
            author: listing.author.as_str().into(),
            version: listing.version.as_str().into(),
            description: listing.description.as_str().into(),
            modes: listing.modes.as_str().into(),
            cost: listing.cost.as_str().into(),
            reviewed: listing.reviewed,
            chat: Color::from_argb_encoded(0xff1e_1f24),
            sidebar: Color::from_argb_encoded(0xff17_181c),
            rail: Color::from_argb_encoded(0xff11_1215),
            accent: Color::from_argb_encoded(0xffff_bf47),
            text: Color::from_argb_encoded(0xffee_f0f4),
            muted: Color::from_argb_encoded(0xff8d_919c),
            ..ThemeListing::default()
        })
        .collect();
    page.set_installed_rows(pairs(&installed));
    page.set_listed_rows(pairs(&listed));
    page.set_installed(ModelRc::new(VecModel::from(installed)));
    page.set_listed(ModelRc::new(VecModel::from(listed)));
}

/// Two to a row, for the gallery.
fn pairs(themes: &[ThemeListing]) -> ModelRc<ModelRc<ThemeListing>> {
    let rows: Vec<ModelRc<ThemeListing>> = themes
        .chunks(2)
        .map(|pair| ModelRc::new(VecModel::from(pair.to_vec())))
        .collect();
    ModelRc::new(VecModel::from(rows))
}

fn modes_text(package: &Package) -> &'static str {
    match (package.dark.is_some(), package.light.is_some()) {
        (true, true) => "Dark and light",
        (false, true) => "Light",
        _ => "Dark",
    }
}

fn listing_of(
    package: &Package,
    id: &str,
    folder: Option<&Path>,
    applied: bool,
    previous: Option<String>,
) -> ThemeListing {
    let resolved = package.resolve(package.mode_for(Mode::Dark)).ok();
    let colours = resolved.as_ref().map_or(theme::YAP, colours_of);
    let cost = folder
        .and_then(|folder| Files::read(folder).ok())
        .map(|files| check::check(&files, Strictness::Install).cost.text())
        .unwrap_or_default();
    let preview = folder
        .zip(package.preview.first())
        .and_then(|(folder, preview)| preview_picture(&folder.join(preview)))
        .unwrap_or_default();
    ThemeListing {
        id: id.into(),
        name: package.name.as_str().into(),
        author: package.authors.join(", ").into(),
        version: package.version.as_str().into(),
        description: package.description.as_str().into(),
        modes: modes_text(package).into(),
        cost: cost.into(),
        preview,
        chat: colours.chat,
        sidebar: colours.sidebar,
        rail: colours.rail,
        accent: colours.accent,
        text: colours.text,
        muted: colours.muted,
        installed: folder.is_some(),
        applied,
        reviewed: false,
        previous: previous.unwrap_or_default().into(),
        notes: package.notes.join("\n").into(),
        based_on: package.based_on.clone().unwrap_or_default().into(),
    }
}

/// A preview picture, decoded at most 960 pixels wide: the gallery draws it
/// at about 400.
fn picture(path: &Path) -> Option<Image> {
    let bytes = std::fs::read(path).ok()?;
    if path.extension().and_then(|extension| extension.to_str()) == Some("svg") {
        return Image::load_from_svg_data(&bytes).ok();
    }
    check::check_image(&bytes).ok()?;
    let decoded = image::load_from_memory(&bytes).ok()?;
    let decoded = if decoded.width() > 960 {
        decoded.resize(
            960,
            960 * decoded.height() / decoded.width(),
            image::imageops::FilterType::Triangle,
        )
    } else {
        decoded
    };
    let rgba = decoded.into_rgba8();
    let (width, height) = rgba.dimensions();
    Some(Image::from_rgba8(
        slint::SharedPixelBuffer::clone_from_slice(rgba.as_raw(), width, height),
    ))
}

/// An installed theme's preview, decoded once while the page is open: the
/// page is drawn again each time a theme is applied.
fn preview_picture(path: &Path) -> Option<Image> {
    PREVIEWS.with_borrow_mut(|previews| {
        previews
            .entry(path.to_owned())
            .or_insert_with(|| picture(path))
            .clone()
    })
}

thread_local! {
    static PREVIEWS: std::cell::RefCell<HashMap<PathBuf, Option<Image>>> =
        std::cell::RefCell::new(HashMap::new());
}

/// Drops the gallery's pictures, once the page closes.
pub fn close_page(window: &YapfastWindow) {
    PREVIEWS.with_borrow_mut(HashMap::clear);
    let page = window.global::<ThemesPage>();
    page.set_installed(ModelRc::default());
    page.set_listed(ModelRc::default());
    page.set_installed_rows(ModelRc::default());
    page.set_listed_rows(ModelRc::default());
}

/// Reads the installed themes again, after an install, a roll back or a
/// removal.
fn reload_installed() {
    STATE.with_borrow_mut(|state| {
        if let Some(root) = &state.root {
            (state.installed, state.refused) = Themes::open(root).installed_and_refused();
        }
    });
}

fn themes() -> Option<Themes> {
    STATE.with_borrow(|state| state.root.as_deref().map(Themes::open))
}

/// Removes an installed theme.
pub fn remove(id: &str) -> Result<(), String> {
    let themes = themes().ok_or("no data folder")?;
    themes.uninstall(id).map_err(|error| format!("{error:#}"))?;
    reload_installed();
    Ok(())
}

pub fn roll_back(id: &str) -> Result<(), String> {
    let themes = themes().ok_or("no data folder")?;
    themes.roll_back(id).map_err(|error| format!("{error:#}"))?;
    reload_installed();
    Ok(())
}

/// Fetches a theme from its repository with the plugin helper, on a thread
/// of its own, and shows its review when it's back.
#[cfg(feature = "plugins")]
pub fn find(window: &YapfastWindow, url: String) {
    let page = window.global::<ThemesPage>();
    page.set_finding(true);
    page.set_error(SharedString::new());
    let weak = window.as_weak();
    std::thread::spawn(move || {
        let result = yapfast_core::plugins::installing::fetch(&url);
        let _ = slint::invoke_from_event_loop(move || {
            let Some(window) = weak.upgrade() else { return };
            let page = window.global::<ThemesPage>();
            page.set_finding(false);
            match result {
                Ok(yapfast_core::plugins::installing::Outcome::Theme(fetched)) => {
                    show_review(&window, *fetched);
                }
                Ok(_) => page.set_error(
                    "That repository holds a plugin, not a theme; add it from Add from URL.".into(),
                ),
                Err(error) => page.set_error(error.into()),
            }
        });
    });
}

/// A build without plugins has no helper to fetch with.
#[cfg(not(feature = "plugins"))]
pub fn find(window: &YapfastWindow, _: String) {
    window
        .global::<ThemesPage>()
        .set_error("This build has no plugin helper to fetch themes with.".into());
}

#[cfg_attr(not(feature = "plugins"), allow(dead_code))]
fn show_review(window: &YapfastWindow, fetched: Fetched) {
    let package = fetched.package().clone();
    let previews: Vec<Image> = package
        .preview
        .iter()
        .filter_map(|path| {
            let bytes = fetched.files.files.get(path)?;
            let decoded = image::load_from_memory(bytes).ok()?.into_rgba8();
            let (width, height) = decoded.dimensions();
            Some(Image::from_rgba8(
                slint::SharedPixelBuffer::clone_from_slice(decoded.as_raw(), width, height),
            ))
        })
        .collect();
    let slots: Vec<_> = package
        .modes()
        .into_iter()
        .filter_map(|mode| package.resolve(mode).ok())
        .flat_map(|resolved| resolved.sounds.into_keys())
        .collect();
    let content = package
        .modes()
        .into_iter()
        .filter_map(|mode| package.resolve(mode).ok())
        .find_map(|resolved| resolved.content.phrase());
    let review = ThemeReview {
        shown: true,
        content: content.unwrap_or_default().into(),
        name: package.name.as_str().into(),
        version: package.version.as_str().into(),
        author: package.authors.join(", ").into(),
        description: package.description.as_str().into(),
        source: fetched.source.as_str().into(),
        commit: fetched.rev.chars().take(7).collect::<String>().into(),
        modes: modes_text(&package).into(),
        cost: fetched.report.cost.text().into(),
        notes: package.notes.join("\n").into(),
        based_on: package.based_on.clone().unwrap_or_default().into(),
        previews: ModelRc::new(VecModel::from(previews)),
        notification_sounds: slots.iter().any(|slot| !slot.interface()),
        interface_sounds: slots.iter().any(|slot| slot.interface()),
    };
    let page = window.global::<ThemesPage>();
    // Notification sounds start on, interface sounds off, and the theme
    // leaves people's content alone until it's let.
    page.set_review_notify_on(true);
    page.set_review_ui_on(false);
    page.set_review_content_on(false);
    page.set_review(review);
    STATE.with_borrow_mut(|state| state.review = Some(fetched));
    // The review is a protected dialog: it shows in the stock theme.
    theme::protect_for(window, theme::Protected::ThemeReview, true);
}

/// The shown theme's `content` and the colours it's drawn with; none while
/// no installed theme shows.
pub(super) fn applied_content() -> Option<(
    yapfast_core::themes::content::Content,
    super::theme_content::Colours,
)> {
    STATE.with_borrow(|state| {
        let resolved = &state.applied.as_ref()?.resolved;
        let colour = |token, stock| token_colour(resolved, token).unwrap_or(stock);
        Some((
            resolved.content,
            super::theme_content::Colours {
                dark: resolved.dark,
                chat: colour("chat", 0xff00_0000),
                accent: colour("accent", 0xff58_65f2),
            },
        ))
    })
}

/// The id of the theme under review, if one is.
pub fn review_id() -> Option<String> {
    STATE.with_borrow(|state| {
        state
            .review
            .as_ref()
            .map(|fetched| fetched.package().id.clone())
    })
}

/// Installs the reviewed theme. Returns its layout id.
pub fn install_review(window: &YapfastWindow) -> Result<String, String> {
    let fetched = STATE
        .with_borrow_mut(|state| state.review.take())
        .ok_or("nothing to install")?;
    close_review(window);
    let themes = themes().ok_or("no data folder")?;
    themes
        .install(&fetched)
        .map_err(|error| format!("{error:#}"))?;
    reload_installed();
    Ok(format!("{INSTALLED}{}", fetched.package().id))
}

pub fn close_review(window: &YapfastWindow) {
    window
        .global::<ThemesPage>()
        .set_review(ThemeReview::default());
    STATE.with_borrow_mut(|state| {
        if !state
            .trial
            .as_ref()
            .is_some_and(|trial| trial.installed.is_none())
        {
            state.review = None;
        }
    });
    theme::protect_for(window, theme::Protected::ThemeReview, false);
}

/// Shows the reviewed theme for 30 seconds without installing it, from its
/// fetched files in a temporary folder.
pub fn try_review(window: &YapfastWindow, back_to: Option<String>) -> Result<theme::Theme, String> {
    let (package, files) = STATE
        .with_borrow(|state| {
            state
                .review
                .as_ref()
                .map(|fetched| (fetched.package().clone(), fetched.files.clone()))
        })
        .ok_or("nothing to try")?;
    window
        .global::<ThemesPage>()
        .set_review(ThemeReview::default());
    theme::protect_for(window, theme::Protected::ThemeReview, false);
    // Its fonts are registered from bytes on disk, as an installed theme's
    // are, in a folder that goes when the trial ends.
    let folder = std::env::temp_dir().join(format!(".yapfast-theme-trial-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    for (path, bytes) in &files.files {
        let target = folder.join(path);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        std::fs::write(&target, bytes).map_err(|error| error.to_string())?;
    }
    let colours = apply_package(window, "trial", &package, &folder, None);
    let _ = std::fs::remove_dir_all(&folder);
    let colours = colours?;
    begin_trial(window, back_to, None, &package.name);
    Ok(colours)
}

/// Shows an installed theme for 30 seconds.
pub fn try_installed(
    window: &YapfastWindow,
    id: &str,
    back_to: Option<String>,
) -> Result<theme::Theme, String> {
    let layout_id = format!("{INSTALLED}{id}");
    let colours = show(window, &layout_id)?;
    let name = applied_name().unwrap_or_default();
    begin_trial(window, back_to, Some(layout_id), &name);
    Ok(colours)
}

fn begin_trial(
    window: &YapfastWindow,
    back_to: Option<String>,
    installed: Option<String>,
    name: &str,
) {
    let page = window.global::<ThemesPage>();
    page.set_trying(name.into());
    page.set_seconds(TRIAL as i32);
    let until = Instant::now() + Duration::from_secs(TRIAL);
    let timer = slint::Timer::default();
    let weak = window.as_weak();
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(250),
        move || {
            let Some(window) = weak.upgrade() else { return };
            let left = until
                .saturating_duration_since(Instant::now())
                .as_secs_f32()
                .ceil() as i32;
            window.global::<ThemesPage>().set_seconds(left);
            if left <= 0 {
                window.global::<ThemesPage>().invoke_revert();
            }
        },
    );
    STATE.with_borrow_mut(|state| {
        state.trial = Some(Trial {
            back_to,
            until,
            installed,
            timer,
        })
    });
}

/// Ends a trial: returns what to go back to, or with `keep`, the id to choose
/// (installing a reviewed theme first).
pub fn end_trial(window: &YapfastWindow, keep: bool) -> Option<Result<String, Option<String>>> {
    let trial = STATE.with_borrow_mut(|state| state.trial.take())?;
    trial.timer.stop();
    let _ = trial.until;
    window
        .global::<ThemesPage>()
        .set_trying(SharedString::new());
    if keep {
        let id = match trial.installed {
            Some(id) => Ok(id),
            None => install_review(window).map_err(|error| {
                tracing::warn!(error, "the theme tried couldn't be installed");
                trial.back_to.clone()
            }),
        };
        return Some(id);
    }
    STATE.with_borrow_mut(|state| state.review = None);
    Some(Err(trial.back_to))
}

/// Ctrl+Shift+T: the default theme for the rest of this run.
pub fn default_for_session(window: &YapfastWindow) {
    DEFAULT_THEME.set(true);
    STATE.with_borrow_mut(|state| {
        state.trial = None;
        state.dev_timer = None;
    });
    clear(window);
}

#[cfg(test)]
mod tests {
    use super::*;

    const TIDE: &str = r##"{ "theme_api": 1, "id": "dev.example.tide", "name": "Tide", "version": "1.0.0",
        "shape": { "button": { "radius": 999 }, "radii": { "large": 2 } },
        "type": { "headings": { "size": 18, "uppercase": true } },
        "spacing": { "avatar_size": 44 },
        "motion": { "hover": "glow", "open": "pop" },
        "animations": [ { "slot": "unread", "property": "scale", "values": [1, 1.5], "period": 1000 } ],
        "effects": { "channel.name": "decrypt" },
        "variants": { "dark": { "base": "yap",
            "colours": { "chat": "#16262f", "accent-text": "#5fe0c6", "rail": "linear(#0d1b24, #1a1030)" } } } }"##;

    fn window() -> YapfastWindow {
        i_slint_backend_testing::init_no_event_loop();
        YapfastWindow::new().unwrap()
    }

    /// Installs Tide into a fresh data folder, through the store as an
    /// install from GitHub would.
    fn installed() -> tempfile::TempDir {
        let data = tempfile::tempdir().unwrap();
        register_bases();
        let fetched = Fetched::new(
            "https://github.com/ada/tide".into(),
            String::new(),
            "main".into(),
            "0123456789abcdef".into(),
            Files {
                manifest: TIDE.into(),
                ..Files::default()
            },
        )
        .unwrap();
        Themes::open(data.path()).install(&fetched).unwrap();
        init(data.path());
        data
    }

    fn stock_shape() -> Shape {
        Shape {
            radius: -1.,
            border: -1.,
            shadow_blur: -1.,
            ..Shape::default()
        }
    }

    #[test]
    fn the_colours_themes_name_are_the_windows_own() {
        let names: Vec<String> = theme::Theme::NAMES
            .iter()
            .map(|name| name.replace('_', "-"))
            .collect();
        assert_eq!(names, yapfast_core::themes::package::COLOURS);
    }

    #[test]
    fn an_installed_theme_sets_its_tokens_and_clearing_puts_back_stock() {
        let _data = installed();
        let window = window();
        let colours = show(&window, "installed:dev.example.tide").unwrap();
        assert_eq!(colours.chat, Color::from_argb_encoded(0xff16_262f));
        assert_eq!(colours.text, theme::YAP.text, "the base's colours stand in");
        let look = window.global::<Look>();
        assert_eq!(look.get_button().radius, 999.);
        assert_eq!(look.get_radius_large(), 2.);
        assert_eq!(look.get_avatar_size(), 44.);
        assert!(look.get_headings_upper());
        assert!(look.get_fx_channel());
        let tokens = window.global::<yapfast_ui::Theme>();
        assert!(tokens.get_accent_text_on());
        assert!(tokens.get_rail_painted());

        clear(&window);
        assert_eq!(look.get_button(), stock_shape());
        assert_eq!(look.get_radius_large(), -1.);
        assert_eq!(look.get_avatar_size(), -1.);
        assert!(!look.get_headings_upper());
        assert!(!look.get_fx_channel());
        assert!(!tokens.get_accent_text_on());
        assert!(!tokens.get_rail_painted());
        assert_eq!(window.global::<ClientStyle>().get_text_size_theme(), -1.);
    }

    #[test]
    fn light_keeps_the_stock_motion_and_no_animation() {
        let _data = installed();
        let window = window();
        set_motion(&window, Performance::Light, false);
        show(&window, "installed:dev.example.tide").unwrap();
        let look = window.global::<Look>();
        assert_eq!(look.get_hover(), 0);
        assert_eq!(look.get_open(), 0);
        assert!(!look.get_fx_channel());
        assert!(STATE.with_borrow(|state| state.clock.is_none()));
        assert_eq!(look.get_button().radius, 999., "shapes stay");
        set_motion(&window, Performance::Balanced, true);
        assert_eq!(look.get_hover(), 0, "reduced motion keeps it still too");
        set_motion(&window, Performance::Balanced, false);
        assert_eq!(look.get_hover(), Hover::Glow.index());
        clear(&window);
    }

    #[test]
    fn a_theme_that_cant_be_shown_falls_back_with_a_notice() {
        let data = installed();
        let window = window();
        // Its theme.json changed in the store since it was installed.
        let folder = STATE.with_borrow(|state| state.installed[0].folder.clone());
        std::fs::write(
            folder.join("theme.json"),
            TIDE.replace("#16262f", "#ffffff"),
        )
        .unwrap();
        init(data.path());
        assert!(startup(&window, Some("installed:dev.example.tide")).is_none());
        assert!(applied_id().is_none());
        let reason = take_failure().unwrap();
        assert!(reason.contains("default theme"), "{reason}");
        // A theme that's gone is the same.
        assert!(startup(&window, Some("installed:dev.example.gone")).is_none());
        assert!(take_failure().is_some());
    }

    #[test]
    fn a_protected_dialog_shows_stock_and_the_theme_comes_back() {
        let _data = installed();
        let window = window();
        show(&window, "installed:dev.example.tide").unwrap();
        let stock = protect(&window, true).unwrap();
        assert_eq!(stock, theme::find("bonfire").unwrap().theme);
        assert_eq!(window.global::<Look>().get_button(), stock_shape());
        let back = protect(&window, false).unwrap();
        assert_eq!(back.chat, Color::from_argb_encoded(0xff16_262f));
        assert_eq!(window.global::<Look>().get_button().radius, 999.);
        clear(&window);
    }

    /// A mono 16-bit WAV of a short blip.
    fn blip() -> Vec<u8> {
        let data: Vec<u8> = (0..2400)
            .flat_map(|index| {
                let value = (std::f32::consts::TAU * 880.0 * index as f32 / 48_000.0).sin();
                ((value * 12_000.0) as i16).to_le_bytes()
            })
            .collect();
        let mut file = b"RIFF".to_vec();
        file.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
        file.extend_from_slice(b"WAVEfmt \x10\0\0\0\x01\0\x01\0");
        file.extend_from_slice(&48_000u32.to_le_bytes());
        file.extend_from_slice(&96_000u32.to_le_bytes());
        file.extend_from_slice(b"\x02\0\x10\0data");
        file.extend_from_slice(&(data.len() as u32).to_le_bytes());
        file.extend_from_slice(&data);
        file
    }

    #[test]
    fn a_themes_icons_and_sounds_show_and_stock_comes_back() {
        const POPS: &str = r##"{ "theme_api": 1, "id": "dev.example.pops", "name": "Pops", "version": "1.0.0",
            "sounds": { "hover": "sounds/blip.wav", "message": { "file": "sounds/blip.wav", "volume": 0.5 } },
            "icons": { "hash": "icons/hash.svg", "logo": { "file": "icons/logo.svg", "colour": "keep" } },
            "variants": { "dark": { "base": "yap" } } }"##;
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><circle cx="12" cy="12" r="9" fill="#ff71ce"/></svg>"##;
        let data = tempfile::tempdir().unwrap();
        register_bases();
        let files = [
            ("sounds/blip.wav", blip()),
            ("icons/hash.svg", svg.to_vec()),
            ("icons/logo.svg", svg.to_vec()),
        ];
        let fetched = Fetched::new(
            "https://github.com/ada/pops".into(),
            String::new(),
            "main".into(),
            "0123456789abcdef".into(),
            Files {
                manifest: POPS.into(),
                files: files
                    .into_iter()
                    .map(|(path, bytes)| (std::path::PathBuf::from(path), bytes))
                    .collect(),
                ..Files::default()
            },
        )
        .unwrap();
        Themes::open(data.path()).install(&fetched).unwrap();
        init(data.path());
        let window = window();
        let pack = window.global::<yapfast_ui::IconPack>();
        let cue = window.global::<yapfast_ui::Cue>();
        let stock_hash = pack.get_hash();
        assert!(!pack.get_active() && !cue.get_hover());

        show(&window, "installed:dev.example.pops").unwrap();
        assert!(!cue.get_hover(), "interface sounds start off");
        let both = super::super::theme_media::Prefs {
            interface: true,
            ..Default::default()
        };
        super::super::theme_media::set_prefs(&window, both);
        assert!(pack.get_hash() != stock_hash);
        assert!(pack.get_active(), "a kept-colour icon asks for its paint");
        let logo = pack.get_logo();
        let stock = Brush::SolidColor(Color::from_argb_encoded(0xff00_ff00));
        assert!(super::super::theme_media::paint(&logo, stock.clone()).is_transparent());
        assert_eq!(
            super::super::theme_media::paint(&pack.get_hash(), stock.clone()),
            stock
        );
        assert!(cue.get_hover() && !cue.get_press());
        assert!(
            super::super::theme_media::replacing(yapfast_core::sounds::Sound::Message).is_some()
        );
        assert!(super::super::theme_media::replacing(yapfast_core::sounds::Sound::Join).is_none());

        protect(&window, true);
        assert!(pack.get_hash() == stock_hash && !cue.get_hover());
        assert!(
            super::super::theme_media::replacing(yapfast_core::sounds::Sound::Message).is_none()
        );
        protect(&window, false);
        assert!(pack.get_hash() != stock_hash && cue.get_hover());

        super::super::theme_media::set_prefs(
            &window,
            super::super::theme_media::Prefs { on: false, ..both },
        );
        assert!(cue.get_hover(), "the two groups are separate");
        assert!(
            super::super::theme_media::replacing(yapfast_core::sounds::Sound::Message).is_none()
        );
        super::super::theme_media::set_prefs(&window, Default::default());
        assert!(!cue.get_hover());

        clear(&window);
        assert!(pack.get_hash() == stock_hash && !pack.get_active() && !cue.get_hover());
    }

    #[test]
    fn a_trial_goes_back_unless_kept() {
        let _data = installed();
        let window = window();
        try_installed(&window, "dev.example.tide", Some("sakura".into())).unwrap();
        assert_eq!(window.global::<ThemesPage>().get_trying(), "Tide");
        assert_eq!(window.global::<ThemesPage>().get_seconds(), 30);
        let back = end_trial(&window, false).unwrap();
        assert_eq!(back, Err(Some("sakura".into())));
        assert_eq!(window.global::<ThemesPage>().get_trying(), "");

        try_installed(&window, "dev.example.tide", None).unwrap();
        let kept = end_trial(&window, true).unwrap();
        assert_eq!(kept, Ok("installed:dev.example.tide".into()));
        assert!(end_trial(&window, false).is_none(), "one trial at a time");
        clear(&window);
    }

    #[test]
    fn the_default_theme_flag_skips_the_chosen_one() {
        let _data = installed();
        let window = window();
        default_for_session(&window);
        assert!(startup(&window, Some("installed:dev.example.tide")).is_none());
        assert!(take_failure().is_none(), "a reset isn't a failure");
        DEFAULT_THEME.set(false);
    }

    #[test]
    fn effects_scramble_then_settle() {
        let _data = installed();
        let window = window();
        show(&window, "installed:dev.example.tide").unwrap();
        let first = effect_text(&window, 0, "general".into());
        assert_eq!(first.chars().count(), 7);
        assert_ne!(first, "general");
        clear(&window);
        assert_eq!(effect_text(&window, 0, "general".into()), "general");
    }
}
