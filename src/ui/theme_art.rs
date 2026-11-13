//! A theme's art on the window: pictures and textures under surfaces,
//! overlays, glass, lit text, particles, the event effects, foil and the
//! channel change, through the Slint global `Art` (`ui/theme_art.slint`).
//!
//! Pictures are decoded once per theme, at display size, with their blend,
//! blur, dim and opacity baked into their pixels, and kept as shared
//! `slint::Image`s; nothing is decoded again when the window is painted anew
//! (a protected dialog closing, Performance changing). One clock moves what
//! moves, at the rate Performance allows, and stops when nothing does: a
//! theme without art sets nothing and runs nothing here.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use image::{Rgba, RgbaImage};
use slint::{Color, ComponentHandle, Image, ModelRc, Rgba8Pixel, SharedPixelBuffer, VecModel};
use yapfast_core::themes::Performance;
use yapfast_core::themes::check::art::{
    CARD_ART, DISPLAY, GLASS_SHRINK, TEXTURE_SIDE, bake_key, blend_channel, shown, sized_cap,
};
use yapfast_core::themes::package::art::{
    Art, ArtSurface, Depth, Emitter, EventSlot, Fit, FoilSurface, Layer, Overlay, OverlayKind,
    Preset, Source, TextLook, TextRole, Texture, TransitionKind, Trigger,
};
use yapfast_core::themes::package::{Property, Resolved};
use yapfast_ui::{
    ArtImage, ArtOverlay, ArtParticle, ArtText, CardArt, Reserve, SlotMotion, YapfastWindow,
};

/// The rail's servers run a phase apart in this many groups, which repeat
/// down the rail.
const RAIL_PHASES: usize = 8;
/// The most particles in flight at once, whatever the theme asks.
const MOST_PARTICLES: usize = 160;

/// A particle in flight, in window pixels.
#[derive(Clone)]
struct Live {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    size: f32,
    colour: u32,
    age: f32,
    life: f32,
    rotation: f32,
    spin: f32,
    preset: Preset,
    sprite: i32,
    seed: f32,
    under: bool,
}

/// A channel change playing: when it began, how long it takes.
struct Shot {
    started: Instant,
    duration: f32,
}

/// Pixels as Slint takes them, which may be made on another thread.
type Pixels = SharedPixelBuffer<Rgba8Pixel>;

/// A theme's pictures, decoded and baked off the UI thread by [`bake`], by
/// the key [`apply`] looks them up under; it only wraps them.
#[derive(Default)]
pub struct Baked(HashMap<String, Pixels>);

#[derive(Default)]
struct State {
    art: Option<Art>,
    folder: PathBuf,
    /// Decoded pictures, by what made them: one picture however many
    /// surfaces lay it, kept while the theme shows, so painting it again,
    /// or showing its other variant, decodes nothing that's still in use.
    cache: HashMap<String, Image>,
    /// What [`bake`] made for the next [`apply`].
    baked: HashMap<String, Pixels>,
    /// The keys the last [`apply`] asked for; the rest of the cache goes.
    used: HashSet<String>,
    performance: Performance,
    reduced: bool,
    clock: Option<(Instant, slint::Timer)>,
    last: Option<Instant>,
    particles: Vec<Live>,
    model: Option<Rc<VecModel<ArtParticle>>>,
    /// Ambient particles owed, per set, carried between frames.
    owed: Vec<f32>,
    shot: Option<Shot>,
    seed: u64,
    /// Whether anything other than particles moves on the clock.
    moving: bool,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

/// Whether something of this weight runs at this setting.
fn runs(heavy: bool, performance: Performance, reduced: bool) -> bool {
    !reduced
        && match performance {
            Performance::Full => true,
            Performance::Balanced => !heavy,
            Performance::Light => false,
        }
}

/// Clears every art value back to none. Called by `theme_look::reset`.
pub fn reset(window: &YapfastWindow) {
    let art = window.global::<yapfast_ui::Art>();
    let none = || ModelRc::new(VecModel::<ArtImage>::default());
    art.set_window(none());
    art.set_rail(none());
    art.set_sidebar(none());
    art.set_chat(none());
    art.set_members(none());
    art.set_account(none());
    art.set_card(none());
    art.set_dialog(none());
    art.set_composer(none());
    art.set_menu(none());
    art.set_actions(none());
    art.set_profile_pane(none());
    art.set_reserve_sidebar(Reserve::default());
    art.set_reserve_members(Reserve::default());
    art.set_reserve_chat(Reserve::default());
    art.set_reserve_composer(Reserve::default());
    art.set_settings(none());
    art.set_embed(none());
    art.set_code(none());
    art.set_row_selected(none());
    art.set_card_over(none());
    art.set_call(none());
    art.set_toast(none());
    art.set_tooltip(none());
    art.set_picker(none());
    art.set_popout(none());
    art.set_aside(none());
    art.set_lightbox(none());
    art.set_cards(ModelRc::new(VecModel::<CardArt>::default()));
    art.set_overlays(ModelRc::new(VecModel::<ArtOverlay>::default()));
    art.set_chat_overlays(false);
    art.set_t(0.);
    for set in [
        yapfast_ui::Art::set_glass_rail,
        yapfast_ui::Art::set_glass_sidebar,
        yapfast_ui::Art::set_glass_chat,
        yapfast_ui::Art::set_glass_members,
        yapfast_ui::Art::set_glass_account,
        yapfast_ui::Art::set_glass_card,
        yapfast_ui::Art::set_glass_dialog,
        yapfast_ui::Art::set_glass_composer,
        yapfast_ui::Art::set_glass_menu,
        yapfast_ui::Art::set_glass_actions,
        yapfast_ui::Art::set_glass_profile_pane,
        yapfast_ui::Art::set_glass_settings,
        yapfast_ui::Art::set_glass_embed,
        yapfast_ui::Art::set_glass_code,
        yapfast_ui::Art::set_glass_row_selected,
        yapfast_ui::Art::set_glass_call,
        yapfast_ui::Art::set_glass_toast,
        yapfast_ui::Art::set_glass_tooltip,
        yapfast_ui::Art::set_glass_picker,
        yapfast_ui::Art::set_glass_popout,
        yapfast_ui::Art::set_glass_aside,
        yapfast_ui::Art::set_glass_lightbox,
    ] {
        set(&art, false);
    }
    art.set_rim_width(0.);
    art.set_rim_color(Color::default());
    art.set_particles_under(false);
    art.set_track(false);
    art.set_blurred(Image::default());
    art.set_glass_opacity(1.);
    art.set_highlight(Color::default());
    art.set_inset_size(0.);
    art.set_gap(0.);
    art.set_pane_radius(0.);
    let unlit = ArtText::default();
    art.set_names(unlit.clone());
    art.set_headings(unlit.clone());
    art.set_labels(unlit);
    art.set_particles(ModelRc::new(VecModel::<ArtParticle>::default()));
    art.set_sprites(ModelRc::new(VecModel::<Image>::default()));
    art.set_ripple(false);
    art.set_gloss(false);
    art.set_highlighter(false);
    art.set_row_highlighter(false);
    art.set_stamp(false);
    art.set_hover_in(-1);
    art.set_hover_out(-1);
    art.set_rail_icons(ModelRc::new(VecModel::<SlotMotion>::default()));
    art.set_foil_card(false);
    art.set_foil_tile(false);
    art.set_shot(Image::default());
    art.set_shot_kind(0);
    art.set_shot_left(0.);
    STATE.with_borrow_mut(|state| {
        state.clock = None;
        state.particles.clear();
        state.model = None;
        state.shot = None;
        state.moving = false;
    });
}

/// Forgets the theme's art and its decoded pictures.
pub fn forget() {
    STATE.with_borrow_mut(|state| {
        state.art = None;
        state.cache.clear();
        state.baked.clear();
    });
}

/// Pictures [`bake`] made, for the next [`apply`] to use.
pub fn take_baked(baked: Baked) {
    STATE.with_borrow_mut(|state| state.baked = baked.0);
}

/// Sets a theme's art, as Performance and reduced motion allow. Called by
/// `theme_look::paint` after `reset`; `hover` is the theme's hover timing.
pub fn apply(
    window: &YapfastWindow,
    resolved: &Resolved,
    folder: &Path,
    performance: Performance,
    reduced: bool,
    hover: (Option<f32>, Option<f32>),
) {
    let art = &resolved.art;
    STATE.with_borrow_mut(|state| {
        // Another theme's pictures are no use; this one's other variant
        // keeps whatever it shares with the one before.
        if state.folder != folder {
            state.cache.clear();
        }
        state.used.clear();
        state.folder = folder.to_owned();
        state.art = Some(art.clone());
        state.performance = performance;
        state.reduced = reduced;
        state.owed = vec![0.0; art.particles.len()];
    });
    let mut decoder = Decoder::new(folder);
    cards(window, resolved, &mut decoder);
    bubbles(window, resolved, &mut decoder);
    if art.is_empty() {
        settle();
        return;
    }
    let global = window.global::<yapfast_ui::Art>();
    let ground = |surface: ArtSurface| {
        surface
            .fixed_ground()
            .or_else(|| shown(resolved, surface.ground()))
            .unwrap_or(0xff00_0000)
    };
    let still = performance == Performance::Light || reduced;
    let mut moving = false;
    for (surface, layers) in &art.surfaces {
        let images: Vec<ArtImage> = layers
            .iter()
            .filter_map(|layer| {
                let image = picture(&mut decoder, layer, ground(*surface))?;
                let moves = layer.moves() && runs(layer.heavy, performance, reduced);
                moving |= moves;
                Some(art_image(layer, image, moves, 0.0))
            })
            .collect();
        let model = ModelRc::new(VecModel::from(images));
        match surface {
            ArtSurface::Window => global.set_window(model),
            ArtSurface::Rail => global.set_rail(model),
            ArtSurface::Sidebar => global.set_sidebar(model),
            ArtSurface::Chat => global.set_chat(model),
            ArtSurface::Members => global.set_members(model),
            ArtSurface::Account => global.set_account(model),
            ArtSurface::Card => global.set_card(model),
            ArtSurface::Dialog => global.set_dialog(model),
            ArtSurface::Composer => global.set_composer(model),
            ArtSurface::Menu => global.set_menu(model),
            ArtSurface::Actions => global.set_actions(model),
            ArtSurface::ProfilePane => global.set_profile_pane(model),
            ArtSurface::Settings => global.set_settings(model),
            ArtSurface::Embed => global.set_embed(model),
            ArtSurface::Code => global.set_code(model),
            ArtSurface::RowSelected => global.set_row_selected(model),
            ArtSurface::CardOver => global.set_card_over(model),
            ArtSurface::Call => global.set_call(model),
            ArtSurface::Toast => global.set_toast(model),
            ArtSurface::Tooltip => global.set_tooltip(model),
            ArtSurface::Picker => global.set_picker(model),
            ArtSurface::Popout => global.set_popout(model),
            ArtSurface::Aside => global.set_aside(model),
            ArtSurface::Lightbox => global.set_lightbox(model),
        }
    }
    // Content kept clear of a pane's art, at every Performance setting:
    // it's where the content is, not an effect.
    for surface in yapfast_core::themes::package::art::RESERVING {
        let Some(room) = art.reserve(surface) else {
            continue;
        };
        let room = Reserve {
            top: room.top,
            bottom: room.bottom,
            left: room.left,
            right: room.right,
        };
        match surface {
            ArtSurface::Sidebar => global.set_reserve_sidebar(room),
            ArtSurface::Members => global.set_reserve_members(room),
            ArtSurface::Chat => global.set_reserve_chat(room),
            _ => global.set_reserve_composer(room),
        }
    }
    // Overlays: Light leaves out the heavy ones, Balanced holds them still.
    let mut overlays = Vec::new();
    for overlay in &art.overlays {
        if overlay.heavy && (performance == Performance::Light) {
            continue;
        }
        let moves = overlay.moves() && runs(overlay.heavy, performance, reduced);
        moving |= moves;
        let colour = Color::from_argb_encoded(overlay.colour);
        let (kind, picture) = match overlay.kind {
            OverlayKind::Image => {
                let Some(layer) = &overlay.layer else {
                    continue;
                };
                let Some(image) = picture(&mut decoder, layer, ground(ArtSurface::Window)) else {
                    continue;
                };
                (0, art_image(layer, image, moves, 0.0))
            }
            OverlayKind::Scanlines | OverlayKind::Lcd | OverlayKind::Grain => {
                let Some(image) = cached(&pattern_key(overlay), || Some(pattern(overlay))) else {
                    continue;
                };
                let tile = ArtImage {
                    image,
                    fit: 2,
                    halign: 1,
                    valign: 1,
                    columns: 1,
                    frames: 1,
                    period: 1000.,
                    jitter: if overlay.kind == OverlayKind::Grain {
                        overlay.period
                    } else {
                        0.
                    },
                    moves,
                    ..ArtImage::default()
                };
                (1, tile)
            }
            OverlayKind::Vignette => (3, ArtImage::default()),
            OverlayKind::Band => (4, ArtImage::default()),
            OverlayKind::Flare => (5, ArtImage::default()),
        };
        overlays.push(ArtOverlay {
            kind,
            picture,
            colour,
            opacity: overlay.opacity,
            size: overlay.size,
            ax: overlay.at.0,
            ay: overlay.at.1,
            period: overlay.period,
            moves,
            chat: overlay.chat,
        });
    }
    global.set_chat_overlays(overlays.iter().any(|overlay| overlay.chat));
    global.set_overlays(ModelRc::new(VecModel::from(overlays)));
    if let Some(glass) = &art.glass {
        for surface in &glass.surfaces {
            let set = match surface {
                ArtSurface::Rail => yapfast_ui::Art::set_glass_rail,
                ArtSurface::Sidebar => yapfast_ui::Art::set_glass_sidebar,
                ArtSurface::Chat => yapfast_ui::Art::set_glass_chat,
                ArtSurface::Members => yapfast_ui::Art::set_glass_members,
                ArtSurface::Account => yapfast_ui::Art::set_glass_account,
                ArtSurface::Card => yapfast_ui::Art::set_glass_card,
                ArtSurface::Dialog => yapfast_ui::Art::set_glass_dialog,
                ArtSurface::Composer => yapfast_ui::Art::set_glass_composer,
                ArtSurface::Menu => yapfast_ui::Art::set_glass_menu,
                ArtSurface::Actions => yapfast_ui::Art::set_glass_actions,
                ArtSurface::ProfilePane => yapfast_ui::Art::set_glass_profile_pane,
                ArtSurface::Settings => yapfast_ui::Art::set_glass_settings,
                ArtSurface::Embed => yapfast_ui::Art::set_glass_embed,
                ArtSurface::Code => yapfast_ui::Art::set_glass_code,
                ArtSurface::RowSelected => yapfast_ui::Art::set_glass_row_selected,
                ArtSurface::Call => yapfast_ui::Art::set_glass_call,
                ArtSurface::Toast => yapfast_ui::Art::set_glass_toast,
                ArtSurface::Tooltip => yapfast_ui::Art::set_glass_tooltip,
                ArtSurface::Picker => yapfast_ui::Art::set_glass_picker,
                ArtSurface::Popout => yapfast_ui::Art::set_glass_popout,
                ArtSurface::Aside => yapfast_ui::Art::set_glass_aside,
                ArtSurface::Lightbox => yapfast_ui::Art::set_glass_lightbox,
                ArtSurface::Window | ArtSurface::CardOver => continue,
            };
            set(&global, true);
        }
        let size = window.window().size();
        let blurred = blurred(
            &mut decoder,
            art,
            glass.blur,
            (size.width, size.height),
            ground(ArtSurface::Window),
        );
        global.set_blurred(blurred);
        global.set_glass_opacity(glass.opacity);
        global.set_highlight(Color::from_argb_encoded(glass.highlight.unwrap_or(0)));
        if let Some(inset) = glass.inset {
            global.set_inset_color(Color::from_argb_encoded(inset.colour));
            global.set_inset_size(inset.blur.max(1.));
        }
        if let Some((colour, width)) = glass.rim {
            global.set_rim_color(Color::from_argb_encoded(colour));
            global.set_rim_width(width);
        }
        global.set_gap(glass.gap);
        global.set_pane_radius(if glass.gap > 0.0 { glass.radius } else { 0.0 });
    }
    for (role, look) in &art.text {
        let lit = text(look);
        match role {
            TextRole::Names => global.set_names(lit),
            TextRole::Headings => global.set_headings(lit),
            TextRole::Labels => global.set_labels(lit),
        }
    }
    if !still {
        for (slot, event) in &art.events {
            let colour = Color::from_argb_encoded(event.colour);
            let time = event.duration as i64;
            match slot {
                EventSlot::Click => {
                    global.set_ripple(true);
                    global.set_ripple_color(colour);
                    global.set_ripple_time(time);
                }
                EventSlot::Hover => {
                    global.set_gloss(true);
                    global.set_gloss_color(colour);
                    global.set_gloss_time(time);
                }
                EventSlot::Select => {
                    global.set_highlighter(true);
                    global.set_highlighter_color(colour);
                    global.set_highlighter_time(time);
                }
                EventSlot::Reaction => {
                    global.set_stamp(true);
                    global.set_stamp_time(time);
                }
                EventSlot::Row => {
                    global.set_row_highlighter(true);
                    global.set_row_highlighter_color(colour);
                    global.set_row_highlighter_time(time);
                }
                // `theme_look` plays it, as the text effects.
                EventSlot::NameHover => {}
            }
        }
        if let Some(time) = hover.0 {
            global.set_hover_in(time as i64);
        }
        if let Some(time) = hover.1 {
            global.set_hover_out(time as i64);
        }
    } else if !art.events.is_empty() {
        // Light and reduced motion keep the highlighter's colour as a plain
        // selection; the motion goes.
        if let Some(event) = art.events.get(&EventSlot::Select) {
            global.set_highlighter(true);
            global.set_highlighter_color(Color::from_argb_encoded(event.colour));
            global.set_highlighter_time(0);
        }
    }
    if let Some(foil) = &art.foil {
        global.set_foil_card(foil.surfaces.contains(&FoilSurface::Card));
        global.set_foil_tile(foil.surfaces.contains(&FoilSurface::Tile));
        let [a, b, c] = foil.colours.map(Color::from_argb_encoded);
        global.set_foil_a(a);
        global.set_foil_b(b);
        global.set_foil_c(c);
        global.set_foil_opacity(foil.opacity);
        global.set_foil_period(foil.period);
        moving |= runs(false, performance, reduced);
    }
    let sprites: Vec<Image> = art
        .particles
        .iter()
        .filter_map(|particles| particles.sprite.as_ref())
        .map(|sprite| {
            cached(&sprite_key(sprite), || {
                decoder.get(sprite, SPRITE).map(|picture| pixels(&picture))
            })
            .unwrap_or_default()
        })
        .collect();
    global.set_sprites(ModelRc::new(VecModel::from(sprites)));
    global.set_track(!art.particles.is_empty());
    global.set_particles_under(
        art.particles
            .iter()
            .any(|particles| particles.z == Depth::Under),
    );
    settle();
    STATE.with_borrow_mut(|state| state.moving = moving);
    start_clock(window);
    event(window, Trigger::ThemeApplied);
}

/// After an apply: drops what was baked for it and not used, and every
/// picture the theme no longer shows.
fn settle() {
    STATE.with_borrow_mut(|state| {
        state.baked.clear();
        let used = std::mem::take(&mut state.used);
        state.cache.retain(|key, _| used.contains(key));
    });
}

/// The largest a particle's sprite is decoded.
const SPRITE: (u32, u32) = (256, 256);

/// Bakes a theme's pictures, as [`apply`] would ask for them in a window of
/// `window` pixels, where nothing waits on it: decoding a theme's art is
/// most of what showing it costs.
pub fn bake(resolved: &Resolved, folder: &Path, window: (u32, u32)) -> Baked {
    let art = &resolved.art;
    let mut decoder = Decoder::new(folder);
    let mut made: HashMap<String, Pixels> = HashMap::new();
    let ground = |surface: ArtSurface| {
        surface
            .fixed_ground()
            .or_else(|| shown(resolved, surface.ground()))
            .unwrap_or(0xff00_0000)
    };
    let mut add = |key: String, make: &mut dyn FnMut() -> Option<Pixels>| {
        if !made.contains_key(&key)
            && let Some(pixels) = make()
        {
            made.insert(key, pixels);
        }
    };
    for (surface, layers) in &art.surfaces {
        for layer in layers {
            add(layer_key(layer, ground(*surface)), &mut || {
                baked(&mut decoder, layer, ground(*surface)).map(|picture| pixels(&picture))
            });
        }
    }
    for overlay in &art.overlays {
        match overlay.kind {
            OverlayKind::Image => {
                if let Some(layer) = &overlay.layer {
                    let under = ground(ArtSurface::Window);
                    add(layer_key(layer, under), &mut || {
                        baked(&mut decoder, layer, under).map(|picture| pixels(&picture))
                    });
                }
            }
            OverlayKind::Scanlines | OverlayKind::Lcd | OverlayKind::Grain => {
                add(pattern_key(overlay), &mut || Some(pattern(overlay)));
            }
            _ => {}
        }
    }
    if let Some(glass) = &art.glass {
        let under = ground(ArtSurface::Window);
        add(glass_key(art, glass.blur, window, under), &mut || {
            Some(blurred_pixels(&mut decoder, art, glass.blur, window, under))
        });
    }
    for sprite in art
        .particles
        .iter()
        .filter_map(|particles| particles.sprite.as_ref())
    {
        add(sprite_key(sprite), &mut || {
            decoder.get(sprite, SPRITE).map(|picture| pixels(&picture))
        });
    }
    let layout = &resolved.layout;
    let card_ground = layout
        .cards
        .ground
        .or_else(|| shown(resolved, "raised"))
        .unwrap_or(0xff00_0000);
    for card in &layout.cards.art {
        let cap = card.size.map_or(CARD_ART, sized_cap);
        let choices = card.choices.iter().map(|path| Layer {
            source: Source::Image(path.clone()),
            ..card.layer.clone()
        });
        for layer in std::iter::once(card.layer.clone()).chain(choices) {
            add(card_key(&layer, card_ground, cap), &mut || {
                baked_at(&mut decoder, &layer, card_ground, cap).map(|picture| pixels(&picture))
            });
        }
    }
    let (own_ground, other_ground) = bubble_grounds(resolved);
    let bubbles = &layout.bubbles;
    for (layers, under) in [
        (&bubbles.own_art, own_ground),
        (&bubbles.other_art, other_ground),
    ] {
        for layer in layers {
            add(layer_key(layer, under), &mut || {
                baked(&mut decoder, layer, under).map(|picture| pixels(&picture))
            });
        }
    }
    Baked(made)
}

/// The pictures on each message card: decoded once at no more than twice
/// the size they show, shared by every card, and how far they reach past
/// a card's edges, which each row's slice of the card leaves room for. A
/// place with several pictures has each decoded once; a card picks one by
/// its group.
fn cards(window: &YapfastWindow, resolved: &Resolved, decoder: &mut Decoder) {
    let layout = &resolved.layout;
    if layout.cards.art.is_empty() {
        return;
    }
    let ground = layout
        .cards
        .ground
        .or_else(|| shown(resolved, "raised"))
        .unwrap_or(0xff00_0000);
    let arrange = window.global::<yapfast_ui::Arrange>();
    let (mut over_top, mut over_bottom) = (
        arrange.get_cards_over_top(),
        arrange.get_cards_over_bottom(),
    );
    let mut placed = Vec::new();
    for card in &layout.cards.art {
        let cap = card.size.map_or(CARD_ART, sized_cap);
        let Some(image) = card_picture(decoder, &card.layer, ground, cap) else {
            continue;
        };
        let natural = image.size();
        let (width, height) = card.size.unwrap_or((
            (natural.width as f32).min(480.),
            (natural.height as f32).min(240.),
        ));
        // A picture as wide as the card turns about its middle: its reach
        // is as a long strip's.
        let turn = width.min(960.) / 2. * card.rotation.to_radians().sin().abs();
        if card.bottom {
            over_bottom = over_bottom.max(height / 2. + card.offset.1 + turn + 1.);
        } else {
            over_top = over_top.max(height / 2. - card.offset.1 + turn + 1.);
        }
        let picture = art_image(&card.layer, image, false, 0.0);
        let mut choices = Vec::new();
        if !card.choices.is_empty() {
            choices.push(picture.clone());
            for path in &card.choices {
                let layer = Layer {
                    source: Source::Image(path.clone()),
                    ..card.layer.clone()
                };
                if let Some(image) = card_picture(decoder, &layer, ground, cap) {
                    choices.push(art_image(&layer, image, false, 0.0));
                }
            }
        }
        placed.push(CardArt {
            picture,
            choices: ModelRc::new(VecModel::from(choices)),
            full: card.full,
            across: card.across,
            bottom: card.bottom,
            width,
            height,
            dx: card.offset.0,
            dy: card.offset.1,
            rotation: card.rotation,
        });
    }
    arrange.set_cards_over_top(over_top);
    arrange.set_cards_over_bottom(over_bottom);
    window
        .global::<yapfast_ui::Art>()
        .set_cards(ModelRc::new(VecModel::from(placed)));
}

/// The pictures on each of your own bubbles and on everyone else's, each
/// decoded once with its blend against that bubble's ground, and shared by
/// every bubble.
fn bubbles(window: &YapfastWindow, resolved: &Resolved, decoder: &mut Decoder) {
    let bubbles = &resolved.layout.bubbles;
    if bubbles.own_art.is_empty() && bubbles.other_art.is_empty() {
        return;
    }
    let (own_ground, other_ground) = bubble_grounds(resolved);
    let mut model = |layers: &[Layer], ground: u32| {
        let images: Vec<ArtImage> = layers
            .iter()
            .filter_map(|layer| {
                let image = picture(decoder, layer, ground)?;
                Some(art_image(layer, image, false, 0.0))
            })
            .collect();
        ModelRc::new(VecModel::from(images))
    };
    let arrange = window.global::<yapfast_ui::Arrange>();
    arrange.set_bubble_own_art(model(&bubbles.own_art, own_ground));
    arrange.set_bubble_other_art(model(&bubbles.other_art, other_ground));
}

/// The grounds your own bubbles' pictures and everyone else's are blended
/// with.
fn bubble_grounds(resolved: &Resolved) -> (u32, u32) {
    let bubbles = &resolved.layout.bubbles;
    let ground = |paint: Option<&yapfast_core::themes::package::Paint>, token: &str| {
        paint
            .map(yapfast_core::themes::package::Paint::solid)
            .or_else(|| shown(resolved, token))
            .unwrap_or(0xff00_0000)
    };
    (
        ground(bubbles.own.as_ref(), "accent"),
        ground(bubbles.other.as_ref(), "raised"),
    )
}

/// A card's still picture, decoded no larger than `cap`: kept, baked by
/// [`bake`], or made now.
fn card_picture(
    decoder: &mut Decoder,
    layer: &Layer,
    ground: u32,
    cap: (u32, u32),
) -> Option<Image> {
    cached(&card_key(layer, ground, cap), || {
        baked_at(decoder, layer, ground, cap).map(|picture| pixels(&picture))
    })
}

fn art_image(layer: &Layer, image: Image, moves: bool, jitter: f32) -> ArtImage {
    let align = |at: f32| {
        if at < 0.34 {
            0
        } else if at > 0.66 {
            2
        } else {
            1
        }
    };
    let frames = layer.frames.map_or((1, 1, 1000.0), |frames| {
        (
            frames.count as i32,
            frames.columns.max(1) as i32,
            frames.period,
        )
    });
    let (w, h) = layer.size.unwrap_or((0.0, 0.0));
    ArtImage {
        image,
        w,
        h,
        px: layer.position.0,
        py: layer.position.1,
        fit: match layer.fit {
            Fit::Cover => 0,
            Fit::Contain => 1,
            Fit::Tile => 2,
            Fit::Stretch => 3,
        },
        halign: align(layer.position.0),
        valign: align(layer.position.1),
        drift_x: layer.drift.0,
        drift_y: layer.drift.1,
        frames: frames.0,
        columns: frames.1,
        period: frames.2,
        jitter,
        moves,
        fade: 1.0 - layer.opacity,
    }
}

fn text(look: &TextLook) -> ArtText {
    let colour = Color::from_argb_encoded;
    let (glow_colour, glow) = look.glow.unwrap_or((0, 0.0));
    let third = ((glow_colour >> 24) / 3) << 24 | (glow_colour & 0xff_ffff);
    let (outline, outline_colour) = look.outline.unwrap_or((0.0, 0));
    ArtText {
        on: true,
        shadow_color: colour(look.shadow.map_or(0, |shadow| shadow.colour)),
        shadow_x: look.shadow.map_or(0., |shadow| shadow.x),
        shadow_y: look.shadow.map_or(0., |shadow| shadow.y),
        glow_color: colour(third),
        glow,
        outline_color: colour(outline_colour),
        outline,
        emboss: look.emboss,
    }
}

/// The picture under `key`: kept from before, baked by [`bake`], or made
/// now by `make`. Every key asked for stays in the cache.
fn cached(key: &str, make: impl FnOnce() -> Option<Pixels>) -> Option<Image> {
    let found = STATE.with_borrow_mut(|state| {
        state.used.insert(key.to_owned());
        if let Some(image) = state.cache.get(key) {
            return Some(image.clone());
        }
        let image = Image::from_rgba8(state.baked.remove(key)?);
        state.cache.insert(key.to_owned(), image.clone());
        Some(image)
    });
    if found.is_some() {
        return found;
    }
    let image = Image::from_rgba8(make()?);
    STATE.with_borrow_mut(|state| state.cache.insert(key.to_owned(), image.clone()));
    Some(image)
}

/// A picture decoded once, or none when it couldn't be.
type Decoded = Option<Arc<RgbaImage>>;

/// Decodes a theme's pictures, each once at each size asked for, however
/// many layers bake it.
struct Decoder<'a> {
    folder: &'a Path,
    decoded: HashMap<(PathBuf, (u32, u32)), Decoded>,
}

impl<'a> Decoder<'a> {
    fn new(folder: &'a Path) -> Self {
        Self {
            folder,
            decoded: HashMap::new(),
        }
    }

    fn get(&mut self, path: &Path, cap: (u32, u32)) -> Option<Arc<RgbaImage>> {
        let folder = self.folder;
        self.decoded
            .entry((path.to_owned(), cap))
            .or_insert_with(|| load(&folder.join(path), cap).map(Arc::new))
            .clone()
    }
}

/// Reads and decodes a picture in the theme's folder, no larger than `cap`.
fn load(path: &Path, cap: (u32, u32)) -> Option<RgbaImage> {
    let bytes = std::fs::read(path).ok()?;
    let format = image::guess_format(&bytes).ok()?;
    let mut reader = image::ImageReader::with_format(std::io::Cursor::new(&bytes), format);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(96 << 20);
    reader.limits(limits);
    let picture = reader.decode().ok()?;
    let picture = if picture.width() > cap.0 || picture.height() > cap.1 {
        picture.resize(cap.0, cap.1, image::imageops::FilterType::Triangle)
    } else {
        picture
    };
    Some(picture.to_rgba8())
}

fn pixels(picture: &RgbaImage) -> Pixels {
    let (width, height) = picture.dimensions();
    SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(picture.as_raw(), width, height)
}

/// What a layer's baked picture is kept under: its source and how it's
/// baked, with the ground's colour only when a blend mixes with it.
fn layer_key(layer: &Layer, ground: u32) -> String {
    format!("layer:{}", bake_key(layer, Some(&format!("{ground:08x}"))))
}

fn card_key(layer: &Layer, ground: u32, cap: (u32, u32)) -> String {
    format!(
        "card:{}:{}x{}",
        bake_key(layer, Some(&format!("{ground:08x}"))),
        cap.0,
        cap.1
    )
}

fn sprite_key(sprite: &Path) -> String {
    format!("sprite:{}", sprite.display())
}

fn pattern_key(overlay: &Overlay) -> String {
    format!(
        "overlay:{}:{}:{:08x}",
        overlay.kind.name(),
        overlay.size,
        overlay.colour
    )
}

/// A scanline, LCD or grain overlay's tile.
fn pattern(overlay: &Overlay) -> Pixels {
    pixels(&match overlay.kind {
        OverlayKind::Scanlines => scanlines(overlay.size as u32, overlay.colour),
        OverlayKind::Lcd => lcd(overlay.size as u32, overlay.colour),
        _ => texture_pixels(Texture::Grain, overlay.colour),
    })
}

/// A layer's pixels at display size: decoded or made, scaled, blurred, and
/// mixed with the surface's ground as its blend says, and dimmed. Its
/// opacity is drawn, so one picture serves every surface that lays it.
fn baked(decoder: &mut Decoder, layer: &Layer, ground: u32) -> Option<RgbaImage> {
    let cap = if layer.fit == Fit::Tile {
        (1024, 1024)
    } else if let Some(size) = layer.size {
        sized_cap(size)
    } else {
        DISPLAY
    };
    baked_at(decoder, layer, ground, cap)
}

/// [`baked`], decoded no larger than `cap`.
fn baked_at(
    decoder: &mut Decoder,
    layer: &Layer,
    ground: u32,
    cap: (u32, u32),
) -> Option<RgbaImage> {
    let mut picture = match &layer.source {
        Source::Image(path) => RgbaImage::clone(&*decoder.get(path, cap)?),
        Source::Texture(kind) => texture_pixels(*kind, layer.colour),
    };
    if layer.fit == Fit::Tile && (layer.scale - 1.0).abs() > 0.01 {
        let width = ((picture.width() as f32 * layer.scale).round() as u32).clamp(1, 2048);
        let height = ((picture.height() as f32 * layer.scale).round() as u32).clamp(1, 2048);
        picture = image::imageops::resize(
            &picture,
            width,
            height,
            image::imageops::FilterType::Triangle,
        );
    }
    if layer.blur > 0.0 {
        picture = image::imageops::fast_blur(&picture, layer.blur / 2.0);
    }
    let plain = layer.blend == yapfast_core::themes::package::art::Blend::Normal;
    if plain && layer.dim <= 0.0 {
        return Some(picture);
    }
    let channel = |colour: u32, shift: u32| ((colour >> shift) & 0xff) as f32 / 255.0;
    let under = [channel(ground, 16), channel(ground, 8), channel(ground, 0)];
    for pixel in picture.pixels_mut() {
        let Rgba([red, green, blue, alpha]) = *pixel;
        let art = [red, green, blue].map(|value| f32::from(value) / 255.0);
        let mixed: [u8; 3] = std::array::from_fn(|index| {
            let value = if plain {
                art[index]
            } else {
                blend_channel(layer.blend, under[index], art[index])
            };
            ((value * (1.0 - layer.dim)).clamp(0.0, 1.0) * 255.0).round() as u8
        });
        *pixel = Rgba([mixed[0], mixed[1], mixed[2], alpha]);
    }
    Some(picture)
}

fn picture(decoder: &mut Decoder, layer: &Layer, ground: u32) -> Option<Image> {
    cached(&layer_key(layer, ground), || {
        baked(decoder, layer, ground).map(|picture| pixels(&picture))
    })
}

/// The size the glass panes' blurred copy of the window's art is kept at.
fn glass_size(window: (u32, u32)) -> (u32, u32) {
    (
        (window.0 / GLASS_SHRINK).clamp(64, DISPLAY.0 / GLASS_SHRINK),
        (window.1 / GLASS_SHRINK).clamp(48, DISPLAY.1 / GLASS_SHRINK),
    )
}

fn glass_key(art: &Art, blur: f32, window: (u32, u32), ground: u32) -> String {
    let (width, height) = glass_size(window);
    let layers: Vec<String> = art
        .surfaces
        .get(&ArtSurface::Window)
        .into_iter()
        .flatten()
        .map(|layer| {
            // A sized layer is laid where its position puts it.
            format!(
                "{}:{:?}:{}:{:?}",
                layer_key(layer, ground),
                layer.fit,
                layer.opacity,
                layer.size.map(|_| layer.position)
            )
        })
        .collect();
    format!(
        "glass:{width}x{height}:{blur}:{ground:08x}:{}",
        layers.join("|")
    )
}

/// The window's art, laid on its ground at a quarter of the window's size
/// and blurred: what glass panes show through.
fn blurred(decoder: &mut Decoder, art: &Art, blur: f32, window: (u32, u32), ground: u32) -> Image {
    cached(&glass_key(art, blur, window, ground), || {
        Some(blurred_pixels(decoder, art, blur, window, ground))
    })
    .unwrap_or_default()
}

fn blurred_pixels(
    decoder: &mut Decoder,
    art: &Art,
    blur: f32,
    window: (u32, u32),
    ground: u32,
) -> Pixels {
    let (width, height) = glass_size(window);
    let [_, red, green, blue] = ground.to_be_bytes();
    let mut canvas = RgbaImage::from_pixel(width, height, Rgba([red, green, blue, 255]));
    for layer in art.surfaces.get(&ArtSurface::Window).into_iter().flatten() {
        let Some(mut picture) = baked(decoder, layer, ground) else {
            continue;
        };
        // Opacity is drawn, not baked: here it's laid into the copy.
        if layer.opacity < 1.0 {
            for pixel in picture.pixels_mut() {
                pixel.0[3] = (f32::from(pixel.0[3]) * layer.opacity).round() as u8;
            }
        }
        let filter = image::imageops::FilterType::Triangle;
        // A layer of its own size sits where it does, a quarter as large.
        if let Some((across, down)) = layer.size {
            let fitted = image::imageops::resize(
                &picture,
                ((across / GLASS_SHRINK as f32).round() as u32).max(1),
                ((down / GLASS_SHRINK as f32).round() as u32).max(1),
                filter,
            );
            let x = (width.saturating_sub(fitted.width()) as f32 * layer.position.0) as i64;
            let y = (height.saturating_sub(fitted.height()) as f32 * layer.position.1) as i64;
            image::imageops::overlay(&mut canvas, &fitted, x, y);
            continue;
        }
        match layer.fit {
            Fit::Tile => {
                let tile = image::imageops::resize(
                    &picture,
                    (picture.width() / GLASS_SHRINK).max(1),
                    (picture.height() / GLASS_SHRINK).max(1),
                    filter,
                );
                image::imageops::tile(&mut canvas, &tile);
            }
            Fit::Contain => {
                let fitted = image::DynamicImage::ImageRgba8(picture)
                    .resize(width, height, filter)
                    .to_rgba8();
                let x = i64::from((width - fitted.width()) / 2);
                let y = i64::from((height - fitted.height()) / 2);
                image::imageops::overlay(&mut canvas, &fitted, x, y);
            }
            Fit::Stretch => {
                let fitted = image::imageops::resize(&picture, width, height, filter);
                image::imageops::overlay(&mut canvas, &fitted, 0, 0);
            }
            Fit::Cover => {
                let fitted = image::DynamicImage::ImageRgba8(picture)
                    .resize_to_fill(width, height, filter)
                    .to_rgba8();
                image::imageops::overlay(&mut canvas, &fitted, 0, 0);
            }
        }
    }
    let blurred = image::imageops::fast_blur(&canvas, (blur / GLASS_SHRINK as f32).max(0.5));
    pixels(&blurred)
}

/// A small deterministic noise: 0 to 1 for a cell.
fn hash(x: u32, y: u32, seed: u32) -> f32 {
    let mut value =
        x.wrapping_mul(0x9e37_79b1) ^ y.wrapping_mul(0x85eb_ca77) ^ seed.wrapping_mul(0xc2b2_ae3d);
    value ^= value >> 15;
    value = value.wrapping_mul(0x2c1b_3c6d);
    value ^= value >> 12;
    value = value.wrapping_mul(0x297a_2d39);
    value ^= value >> 15;
    (value & 0xffff) as f32 / 65535.0
}

/// Value noise that tiles across `side`, at a cell size of `cell`.
fn smooth(x: u32, y: u32, cell: u32, side: u32, seed: u32) -> f32 {
    let cells = (side / cell).max(1);
    let (cx, cy) = (x / cell, y / cell);
    let (fx, fy) = (
        (x % cell) as f32 / cell as f32,
        (y % cell) as f32 / cell as f32,
    );
    let at = |dx: u32, dy: u32| hash((cx + dx) % cells, (cy + dy) % cells, seed);
    let ease = |t: f32| t * t * (3.0 - 2.0 * t);
    let (ex, ey) = (ease(fx), ease(fy));
    let top = at(0, 0) * (1.0 - ex) + at(1, 0) * ex;
    let bottom = at(0, 1) * (1.0 - ex) + at(1, 1) * ex;
    top * (1.0 - ey) + bottom * ey
}

/// A texture Yapfast makes: how much of its colour each pixel shows.
fn texture_pixels(kind: Texture, colour: u32) -> RgbaImage {
    let side = TEXTURE_SIDE;
    let [alpha, red, green, blue] = colour.to_be_bytes();
    RgbaImage::from_fn(side, side, |x, y| {
        let amount = match kind {
            Texture::Grain => hash(x, y, 1),
            Texture::Noise => {
                0.55 * smooth(x, y, 32, side, 2)
                    + 0.3 * smooth(x, y, 8, side, 3)
                    + 0.15 * hash(x, y, 4)
            }
            Texture::Paper => {
                // Fibres: noise stretched across, over a fine tooth.
                let fibre = smooth(x, y * 8 % side, 64, side, 5);
                0.35 * fibre + 0.35 * smooth(x, y, 4, side, 6) + 0.3 * hash(x, y, 7)
            }
            Texture::Kraft => {
                let speck = if hash(x, y, 8) > 0.985 { 1.0 } else { 0.0 };
                let fibre = smooth(x * 6 % side, y, 32, side, 9);
                (0.3 * fibre + 0.3 * smooth(x, y, 16, side, 10) + 0.2 * hash(x, y, 11) + speck)
                    .min(1.0)
            }
            Texture::Halftone => {
                // Dots on a grid, larger where the noise beneath is dark.
                let cell = 8.0;
                let (cx, cy) = (
                    (x as f32 % cell) - cell / 2.0,
                    (y as f32 % cell) - cell / 2.0,
                );
                let reach = 1.2 + 2.4 * smooth(x, y, 64, side, 12);
                if (cx * cx + cy * cy).sqrt() < reach {
                    1.0
                } else {
                    0.0
                }
            }
        };
        Rgba([red, green, blue, (amount * f32::from(alpha)).round() as u8])
    })
}

/// One line every `spacing` pixels.
fn scanlines(spacing: u32, colour: u32) -> RgbaImage {
    let [alpha, red, green, blue] = colour.to_be_bytes();
    let spacing = spacing.max(2);
    RgbaImage::from_fn(4, spacing, |_, y| {
        Rgba([red, green, blue, if y == spacing - 1 { alpha } else { 0 }])
    })
}

/// An LCD's pixel grid: red, green and blue stripes, faint, with a dark
/// line between pixels.
fn lcd(size: u32, colour: u32) -> RgbaImage {
    let size = size.max(2);
    let [alpha, red, green, blue] = colour.to_be_bytes();
    RgbaImage::from_fn(size * 3, size, |x, y| {
        if y == size - 1 || x % size == size - 1 {
            return Rgba([red, green, blue, alpha]);
        }
        let stripe = (x / size) % 3;
        let tint = [[255, 40, 40], [40, 255, 40], [40, 40, 255]][stripe as usize];
        Rgba([tint[0], tint[1], tint[2], alpha / 5])
    })
}

/// Starts the art clock when something moves, or stops it.
fn start_clock(window: &YapfastWindow) {
    let (rate, needed) = STATE.with_borrow(|state| {
        let rate = match state.performance {
            Performance::Full => 60,
            Performance::Balanced => 30,
            Performance::Light => 0,
        };
        let ambient = state.art.as_ref().is_some_and(|art| {
            art.particles.iter().any(|particles| {
                particles.on == Trigger::Ambient
                    && runs(particles.heavy, state.performance, state.reduced)
            })
        });
        let needed = state.moving || ambient || !state.particles.is_empty() || state.shot.is_some();
        (rate, needed && !state.reduced)
    });
    let running = STATE.with_borrow(|state| state.clock.is_some());
    if !needed || rate == 0 {
        STATE.with_borrow_mut(|state| state.clock = None);
        return;
    }
    if running {
        return;
    }
    let started = Instant::now();
    let timer = slint::Timer::default();
    let weak = window.as_weak();
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(1000 / rate),
        move || {
            if let Some(window) = weak.upgrade() {
                tick(&window, started);
            }
        },
    );
    STATE.with_borrow_mut(|state| {
        state.clock = Some((started, timer));
        state.last = None;
    });
}

/// One frame: the clock, particles and a channel change.
fn tick(window: &YapfastWindow, started: Instant) {
    let hidden = window.window().is_minimized() || !window.window().is_visible();
    let paused = hidden
        || STATE.with_borrow(|state| {
            state.performance == Performance::Balanced && !super::FOCUSED.get()
        });
    let now = Instant::now();
    let step = STATE.with_borrow_mut(|state| {
        let step = state
            .last
            .map_or(0.0, |last| (now - last).as_secs_f32())
            .min(0.1);
        state.last = Some(now);
        step
    });
    if paused {
        return;
    }
    let global = window.global::<yapfast_ui::Art>();
    let moving = STATE.with_borrow(|state| state.moving);
    if moving {
        global.set_t(started.elapsed().as_secs_f32());
    }
    // The channel change.
    let left = STATE.with_borrow(|state| {
        state
            .shot
            .as_ref()
            .map(|shot| 1.0 - shot.started.elapsed().as_secs_f32() * 1000.0 / shot.duration)
    });
    if let Some(left) = left {
        if left <= 0.0 {
            global.set_shot_left(0.);
            global.set_shot(Image::default());
            STATE.with_borrow_mut(|state| state.shot = None);
        } else {
            global.set_shot_left(left);
        }
    }
    step_particles(window, step);
    let idle = STATE.with_borrow(|state| {
        !state.moving
            && state.particles.is_empty()
            && state.shot.is_none()
            && !state.art.as_ref().is_some_and(|art| {
                art.particles.iter().any(|particles| {
                    particles.on == Trigger::Ambient
                        && runs(particles.heavy, state.performance, state.reduced)
                })
            })
    });
    if idle {
        STATE.with_borrow_mut(|state| state.clock = None);
    }
}

fn random(seed: &mut u64) -> f32 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    (*seed >> 40) as f32 / (1u64 << 24) as f32
}

/// Where particles may start, in window pixels, as the window last said.
#[derive(Clone, Copy, Default)]
struct Origins {
    size: (f32, f32),
    /// The message box: x, y, width, height.
    composer: (f32, f32, f32, f32),
    send: (f32, f32),
    apply: (f32, f32),
    pointer: (f32, f32),
}

fn origins(window: &YapfastWindow) -> Origins {
    let art = window.global::<yapfast_ui::Art>();
    Origins {
        size: (
            art.get_window_width().max(400.),
            art.get_window_height().max(300.),
        ),
        composer: (
            art.get_composer_x(),
            art.get_composer_y(),
            art.get_composer_w(),
            art.get_composer_h(),
        ),
        send: (art.get_send_x(), art.get_send_y()),
        apply: (art.get_apply_x(), art.get_apply_y()),
        pointer: (art.get_pointer_x(), art.get_pointer_y()),
    }
}

/// Particles for an event, if the theme has a set for it and they may fly.
pub fn event(window: &YapfastWindow, trigger: Trigger) {
    let size = origins(window);
    let flew = STATE.with_borrow_mut(|state| {
        let Some(art) = state.art.clone() else {
            return false;
        };
        if state.reduced || state.performance == Performance::Light {
            return false;
        }
        let mut flew = false;
        let mut sprite_index = 0;
        for particles in &art.particles {
            let sprite = particles.sprite.as_ref().map(|_| {
                sprite_index += 1;
                sprite_index - 1
            });
            if particles.on != trigger {
                continue;
            }
            for _ in 0..particles.count {
                let live = spawn(state, particles, sprite, trigger, size);
                if state.particles.len() < MOST_PARTICLES {
                    state.particles.push(live);
                    flew = true;
                }
            }
        }
        flew
    });
    if flew {
        start_clock(window);
    }
}

fn spawn(
    state: &mut State,
    particles: &yapfast_core::themes::package::art::Particles,
    sprite: Option<i32>,
    trigger: Trigger,
    origins: Origins,
) -> Live {
    let (width, height) = origins.size;
    let seed = &mut state.seed;
    if *seed == 0 {
        *seed = 0x2545_f491_4f6c_dd1d;
    }
    let mut r = || random(seed);
    // Where it starts: the place the set names, once the window has said
    // where that is; until then, the stock places: the message box for a
    // message, the middle for a reaction, anywhere for the rest.
    let near = |(x, y): (f32, f32), r: &mut dyn FnMut() -> f32| {
        (x + (r() - 0.5) * 16.0, y + (r() - 0.5) * 16.0)
    };
    let (cx, cy, cw, ch) = origins.composer;
    let (x, y) = match particles.from {
        Emitter::Composer if cw > 0.0 => (cx + cw * (0.1 + 0.8 * r()), cy + ch * (0.2 + 0.6 * r())),
        Emitter::SendButton if origins.send != (0.0, 0.0) => near(origins.send, &mut r),
        // Discord's look has no send button: the message box's end stands in.
        Emitter::SendButton if cw > 0.0 => near((cx + cw - 24.0, cy + ch / 2.0), &mut r),
        Emitter::ApplyButton if origins.apply != (0.0, 0.0) => near(origins.apply, &mut r),
        Emitter::Pointer if origins.pointer != (0.0, 0.0) => near(origins.pointer, &mut r),
        _ => match trigger {
            Trigger::MessageSent => (width * (0.4 + 0.5 * r()), height - 70.0 - 20.0 * r()),
            Trigger::ReactionAdded => (width * (0.45 + 0.3 * r()), height * (0.45 + 0.2 * r())),
            Trigger::ThemeApplied | Trigger::Ambient => match particles.preset {
                Preset::Bubbles => (width * r(), height + 10.0),
                Preset::Scraps => (width * r(), -10.0),
                _ => (width * r(), height * r()),
            },
        },
    };
    let burst = trigger != Trigger::Ambient && trigger != Trigger::ThemeApplied;
    let angle = r() * std::f32::consts::TAU;
    let (vx, vy) = match particles.preset {
        Preset::Bubbles => ((r() - 0.5) * 30.0, -60.0 - 80.0 * r()),
        Preset::Sparkles if burst => (
            angle.cos() * (80.0 + 120.0 * r()),
            angle.sin() * (80.0 + 120.0 * r()),
        ),
        Preset::Sparkles => ((r() - 0.5) * 10.0, (r() - 0.5) * 10.0),
        Preset::Dust => ((r() - 0.5) * 16.0, (r() - 0.5) * 10.0 - 4.0),
        Preset::Scraps if burst => ((r() - 0.5) * 220.0, -160.0 - 120.0 * r()),
        Preset::Scraps => ((r() - 0.5) * 30.0, 50.0 + 60.0 * r()),
    };
    let colour = particles.colours
        [(r() * particles.colours.len() as f32) as usize % particles.colours.len()];
    Live {
        x,
        y,
        vx,
        vy,
        size: particles.size * (0.6 + 0.8 * r()),
        colour,
        age: 0.0,
        life: particles.life * (0.7 + 0.6 * r()) / 1000.0,
        rotation: r() * 360.0,
        spin: (r() - 0.5)
            * if particles.preset == Preset::Scraps {
                360.0
            } else {
                90.0
            },
        preset: particles.preset,
        sprite: sprite.unwrap_or(-1),
        seed: r() * 100.0,
        under: particles.z == Depth::Under,
    }
}

fn step_particles(window: &YapfastWindow, step: f32) {
    let origins = origins(window);
    let size = origins.size;
    let shown = STATE.with_borrow_mut(|state| {
        // Ambient sets start new ones at their rate, up to their count.
        if let Some(art) = state.art.clone() {
            let mut sprite_index = 0;
            for (index, particles) in art.particles.iter().enumerate() {
                let sprite = particles.sprite.as_ref().map(|_| {
                    sprite_index += 1;
                    sprite_index - 1
                });
                if particles.on != Trigger::Ambient
                    || !runs(particles.heavy, state.performance, state.reduced)
                {
                    continue;
                }
                let alive = state
                    .particles
                    .iter()
                    .filter(|live| live.preset == particles.preset)
                    .count();
                if let Some(owed) = state.owed.get_mut(index) {
                    *owed += particles.rate * step;
                }
                while state.owed.get(index).is_some_and(|owed| *owed >= 1.0) {
                    state.owed[index] -= 1.0;
                    if alive < particles.count as usize && state.particles.len() < MOST_PARTICLES {
                        let live = spawn(state, particles, sprite, Trigger::Ambient, origins);
                        state.particles.push(live);
                    }
                }
            }
        }
        for live in &mut state.particles {
            live.age += step;
            match live.preset {
                Preset::Bubbles => live.x += (live.age * 3.0 + live.seed).sin() * 20.0 * step,
                Preset::Sparkles => {
                    live.vx *= 1.0 - 2.5 * step;
                    live.vy *= 1.0 - 2.5 * step;
                }
                Preset::Scraps => {
                    live.vy = (live.vy + 260.0 * step).min(90.0);
                    live.x += (live.age * 2.0 + live.seed).sin() * 40.0 * step;
                }
                Preset::Dust => {}
            }
            live.x += live.vx * step;
            live.y += live.vy * step;
            live.rotation += live.spin * step;
        }
        state
            .particles
            .retain(|live| live.age < live.life && live.y > -60.0 && live.y < size.1 + 60.0);
        if state.particles.is_empty() && state.model.is_none() {
            return None;
        }
        let rows: Vec<ArtParticle> = state
            .particles
            .iter()
            .map(|live| {
                let progress = live.age / live.life;
                // In quickly, out slowly; sparkles twinkle.
                let mut opacity = (progress * 8.0).min(1.0) * (1.0 - progress).min(0.4) / 0.4;
                if live.preset == Preset::Sparkles {
                    opacity *= 0.6 + 0.4 * (live.age * 18.0 + live.seed).sin();
                }
                ArtParticle {
                    x: live.x,
                    y: live.y,
                    size: live.size,
                    color: Color::from_argb_encoded(live.colour),
                    opacity: opacity.clamp(0.0, 1.0),
                    rotation: live.rotation,
                    shape: match live.preset {
                        Preset::Bubbles => 0,
                        Preset::Sparkles => 1,
                        Preset::Dust => 2,
                        Preset::Scraps => 3,
                    },
                    sprite: live.sprite,
                    under: live.under,
                }
            })
            .collect();
        let empty = rows.is_empty();
        let model = state
            .model
            .get_or_insert_with(|| Rc::new(VecModel::default()))
            .clone();
        model.set_vec(rows);
        if empty {
            state.model = None;
        }
        Some(model)
    });
    if let Some(model) = shown {
        window
            .global::<yapfast_ui::Art>()
            .set_particles(ModelRc::from(model));
    }
}

/// Another channel is about to open: shows the conversation as it is going,
/// as the theme's transition says. Only Full plays it, never with reduced
/// motion.
pub fn channel_changing(window: &YapfastWindow) {
    let wanted = STATE.with_borrow(|state| {
        let art = state.art.as_ref()?;
        if state.performance != Performance::Full || state.reduced {
            return None;
        }
        match art.transition {
            Some(transition) => Some((
                if transition.kind == TransitionKind::Dissolve {
                    2
                } else {
                    1
                },
                transition.duration,
                1.0,
            )),
            // An LCD's ghost: a brief, faint afterimage.
            None if art.overlays.iter().any(|overlay| overlay.ghost) => Some((1, 260.0, 0.35)),
            None => None,
        }
    });
    let Some((kind, duration, strength)) = wanted else {
        return;
    };
    let Ok(snapshot) = window.window().take_snapshot() else {
        return;
    };
    let global = window.global::<yapfast_ui::Art>();
    global.set_shot(Image::from_rgba8(snapshot));
    global.set_shot_kind(kind);
    global.set_shot_strength(strength);
    global.set_shot_left(1.);
    STATE.with_borrow_mut(|state| {
        state.shot = Some(Shot {
            started: Instant::now(),
            duration,
        })
    });
    start_clock(window);
}

/// The rail's phases, all still.
pub fn rail_still() -> [SlotMotion; RAIL_PHASES] {
    std::array::from_fn(|_| SlotMotion {
        opacity: 1.,
        scale: 1.,
        ..SlotMotion::default()
    })
}

/// Moves a slot's motion by one animation's value.
pub fn move_by(moved: &mut SlotMotion, property: Property, number: f32) {
    match property {
        Property::Opacity => moved.opacity *= number,
        Property::X => moved.x += number,
        Property::Y => moved.y += number,
        Property::Scale => moved.scale *= number,
        Property::Rotation => moved.rotation += number,
        _ => {}
    }
}

pub fn set_rail(window: &YapfastWindow, rail: [SlotMotion; RAIL_PHASES]) {
    window
        .global::<yapfast_ui::Art>()
        .set_rail_icons(ModelRc::new(VecModel::from(rail.to_vec())));
}

/// `top` laid over `under`, both ARGB.
pub fn over(under: u32, top: u32) -> u32 {
    let alpha = |colour: u32| (colour >> 24) as f32 / 255.0;
    let (a_top, a_under) = (alpha(top), alpha(under));
    let a_out = a_top + a_under * (1.0 - a_top);
    if a_out <= 0.0 {
        return 0;
    }
    let channel = |shift: u32| {
        let value = |colour: u32| ((colour >> shift) & 0xff) as f32;
        let mixed = (value(top) * a_top + value(under) * a_under * (1.0 - a_top)) / a_out;
        (mixed.round() as u32).min(255) << shift
    };
    ((a_out * 255.0).round() as u32) << 24 | channel(16) | channel(8) | channel(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tints_lay_over_one_another() {
        assert_eq!(over(0, 0x80ff_0000), 0x80ff_0000);
        assert_eq!(over(0xff00_00ff, 0xffff_0000), 0xffff_0000);
        let mixed = over(0xff00_0000, 0x80ff_ffff);
        assert_eq!(mixed >> 24, 0xff);
        assert!((0x7f..=0x81).contains(&((mixed >> 16) & 0xff)));
    }

    #[test]
    fn textures_tile_and_take_their_colour() {
        for kind in Texture::ALL {
            let pixels = texture_pixels(*kind, 0x80ff_0000);
            assert_eq!(pixels.dimensions(), (TEXTURE_SIDE, TEXTURE_SIDE));
            assert!(
                pixels
                    .pixels()
                    .all(|pixel| pixel.0[0] == 255 && pixel.0[3] <= 0x80)
            );
        }
    }

    #[test]
    fn a_blend_mixes_with_the_ground() {
        let folder = Path::new("/nonexistent");
        let layer = Layer {
            source: Source::Texture(Texture::Grain),
            fit: Fit::Tile,
            position: (0.5, 0.5),
            opacity: 0.5,
            blend: yapfast_core::themes::package::art::Blend::Multiply,
            blur: 0.0,
            dim: 0.0,
            scale: 1.0,
            colour: 0xff80_8080,
            drift: (0.0, 0.0),
            frames: None,
            heavy: false,
            size: None,
            reserve: None,
        };
        let pixels = baked(&mut Decoder::new(folder), &layer, 0xff00_0000).unwrap();
        // Multiplied with black, every pixel is black; the opacity is drawn,
        // not baked, so the texture keeps its own alpha.
        assert!(pixels.pixels().all(|pixel| pixel.0[0] == 0));
        assert!(pixels.pixels().any(|pixel| pixel.0[3] > 128));
        assert_eq!(art_image(&layer, Image::default(), false, 0.0).fade, 0.5);
    }
}
