//! The window's colours, as every widget reads them, in iced's colour type,
//! with the colours worked out from them. The app fills one from a theme of
//! its own ([`Theme::new`], then [`Theme::worked_out`]). See docs/themes.md.

use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
    sync::{Arc, RwLock},
    time::Duration,
};

use iced::{Background, Border, Color, Font, Shadow, Vector, font::Weight};

/// The non-colour parts of an installed theme. The app sets them on the
/// [`Theme`] it makes from a package; a theme without them draws stock.
#[derive(Clone, Debug, Default)]
pub struct Metrics {
    /// Package colour tokens which cannot live in [`Theme`] without
    /// changing its public shape. This includes the theme's extra colours
    /// and the six surface brushes when they are gradients.
    pub paints: HashMap<String, Background>,
    /// Surfaces whose package has art or glass. The kit does not own those
    /// images, but it uses this flag to leave the app-side decoration's
    /// ground visible instead of covering it with Wired's fallback solid.
    pub decorations: HashSet<String>,
    pub shapes: HashMap<String, Shape>,
    pub outlines: HashMap<String, Outline>,
    pub radii: [Option<f32>; 3],
    pub shadows: Option<Option<ShapeShadow>>,
    pub outline_width: Option<f32>,
    pub spacing: Spacing,
    pub motion: Motion,
    pub types: HashMap<String, TypeStyle>,
    /// Settings → Appearance → Readable text. Reading roles retain their
    /// stock face and weight while keeping the theme's size, as in Slint.
    pub readable_text: bool,
    /// Optional theme art and glass for `tooltip` surfaces.
    pub tooltip: Option<Arc<TooltipSurface>>,
    /// Whether the active package has armed Hover / Press interface sound cues.
    pub hover_cue: bool,
    pub press_cue: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TooltipFit {
    #[default]
    Cover,
    Contain,
    Tile,
    Stretch,
}

#[derive(Clone, Debug)]
pub struct TooltipLayer {
    pub handle: iced::advanced::image::Handle,
    pub frames: Vec<iced::advanced::image::Handle>,
    pub period_ms: f32,
    pub width: u32,
    pub height: u32,
    pub opacity: f32,
    pub scale: f32,
    pub position: (f32, f32),
    pub fit: TooltipFit,
    pub drift: (f32, f32),
    pub size: Option<(f32, f32)>,
    pub moves: bool,
}

#[derive(Clone, Debug)]
pub struct TooltipGlass {
    pub blurred: crate::widgets::BlurredWindow,
    pub ground: Color,
    pub opacity: f32,
    pub radius: f32,
    pub rim: Option<(Color, f32)>,
    pub highlight: Option<Color>,
    pub inset: Option<(Color, f32)>,
    pub gap: f32,
}

#[derive(Clone, Debug, Default)]
pub struct TooltipSurface {
    pub layers: Vec<TooltipLayer>,
    pub glass: Option<TooltipGlass>,
    pub reserve: Option<[f32; 4]>,
}

/// Interface sound cue events emitted by kit interactive controls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CueEvent {
    Hover,
    Press,
}

/// One theme text role after its optional font file has been loaded
/// into iced's font system. Keeping this beside the other companion metrics
/// preserves the public [`Theme`] shape.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TypeStyle {
    pub font: Option<Font>,
    pub size: Option<f32>,
    pub weight: Option<i32>,
    pub letter_spacing: Option<f32>,
    pub uppercase: Option<bool>,
    pub header_size: Option<f32>,
    pub title_size: Option<f32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutlineStroke {
    Solid,
    Dashed,
    Dotted,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Outline {
    pub stroke: OutlineStroke,
    pub dash: f32,
    pub gap: f32,
    pub width: Option<f32>,
    pub colour: Option<Color>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Shape {
    pub radius: Option<f32>,
    pub border: Option<f32>,
    pub border_colour: Option<Color>,
    pub shadow: Option<ShapeShadow>,
    pub inset: Option<(f32, f32)>,
    pub inset_top: Option<f32>,
    pub width: Option<f32>,
    pub hover_width: Option<f32>,
    pub ground: Option<Background>,
    pub text: Option<Color>,
    pub bar: Option<Background>,
    pub bar_text: Option<Color>,
    pub stripe: Option<Stripe>,
    pub case: Option<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stripe {
    pub colour: Color,
    pub channels: bool,
    pub members: bool,
    pub messages: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShapeShadow {
    pub blur: f32,
    pub x: f32,
    pub y: f32,
    pub colour: Color,
}

impl ShapeShadow {
    fn iced(self) -> Shadow {
        Shadow {
            color: self.colour,
            offset: Vector::new(self.x, self.y),
            blur_radius: self.blur,
        }
    }
}

/// Gaps, row heights and pane widths. An absent value keeps Wired's stock
/// metric at its call site.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Spacing {
    pub message_gap: Option<f32>,
    pub message_group_gap: Option<f32>,
    pub channel_row_height: Option<f32>,
    /// A theme's channel list's category heading height.
    pub channel_heading_height: Option<f32>,
    pub member_row_height: Option<f32>,
    pub avatar_size: Option<f32>,
    pub chat_padding: Option<f32>,
    pub sidebar_width: Option<f32>,
    pub members_width: Option<f32>,
    /// Messages laid out as cards (`layout.messages: "cards"`): the space
    /// between two cards, which replaces the gaps between groups and rows
    /// (a group's rows run on inside its card), as Slint's `cards-gap`.
    pub cards_gap: Option<f32>,
}

/// Popup motion after the person's Performance and Reduce motion choices
/// have been applied by the app.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Motion {
    pub quick: Duration,
    pub popup: Duration,
    pub hover_in: Duration,
    pub hover_out: Duration,
    pub curve: Curve,
    pub hover: Hover,
    pub press: Press,
    pub open: Transition,
    pub close: Close,
    pub unfocused: Unfocused,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Unfocused {
    Full,
    #[default]
    Slow,
    Still,
}

impl Default for Motion {
    fn default() -> Self {
        Self {
            quick: Duration::from_millis(120),
            popup: Duration::from_millis(100),
            hover_in: Duration::from_millis(120),
            hover_out: Duration::from_millis(120),
            curve: Curve::EaseOut,
            hover: Hover::Fade,
            press: Press::None,
            open: Transition::Grow,
            close: Close::Reverse,
            unfocused: Unfocused::Slow,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Hover {
    #[default]
    Fade,
    None,
    Lift,
    Glow,
    Scale,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Press {
    #[default]
    None,
    Scale,
    Sink,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Curve {
    Linear,
    Ease,
    EaseIn,
    #[default]
    EaseOut,
    EaseInOut,
    Spring,
    Bounce,
    Elastic,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Transition {
    #[default]
    Grow,
    None,
    Fade,
    Zoom,
    Pop,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Close {
    #[default]
    Reverse,
    None,
}

impl Motion {
    pub fn hover_duration(self, entering: bool) -> Duration {
        if self.hover == Hover::None {
            Duration::ZERO
        } else if entering {
            self.hover_in
        } else {
            self.hover_out
        }
    }

    pub fn duration(self, closing: bool) -> Duration {
        if self.open == Transition::None || (closing && self.close == Close::None) {
            Duration::ZERO
        } else {
            self.popup
        }
    }

    pub fn ease(self, t: f32) -> f32 {
        let curve = if self.open == Transition::Pop {
            Curve::Spring
        } else {
            self.curve
        };
        ease(curve, t)
    }

    pub fn scale(self, shown: f32) -> f32 {
        match self.open {
            Transition::None | Transition::Fade => 1.0,
            Transition::Zoom => 0.95 + 0.05 * shown,
            Transition::Grow | Transition::Pop => 0.8 + 0.2 * shown,
        }
    }

    pub fn opacity(self, shown: f32) -> f32 {
        if self.open == Transition::None {
            f32::from(shown > 0.0)
        } else {
            shown
        }
    }

    pub fn lift(self, hovered: f32, pressed: bool) -> f32 {
        let up = if self.hover == Hover::Lift && !pressed {
            -2.0 * hovered
        } else {
            0.0
        };
        let down = if self.press == Press::Sink && pressed {
            1.0
        } else {
            0.0
        };
        up + down
    }

    pub fn grow(self, hovered: f32, pressed: bool) -> f32 {
        if self.press == Press::Scale && pressed {
            0.96
        } else if self.hover == Hover::Scale {
            1.0 + 0.04 * hovered
        } else {
            1.0
        }
    }

    pub fn halo(self, hovered: f32, glow: Color) -> Shadow {
        match self.hover {
            Hover::Glow if hovered > 0.0 => Shadow {
                color: Color {
                    a: 0.55 * hovered,
                    ..glow
                },
                blur_radius: 14.0 * hovered,
                ..Shadow::default()
            },
            Hover::Lift if hovered > 0.0 => Shadow {
                color: Color {
                    a: 0.33 * hovered,
                    ..Color::BLACK
                },
                offset: Vector::new(0.0, 3.0 * hovered),
                blur_radius: 8.0 * hovered,
            },
            _ => Shadow::default(),
        }
    }
}

static CUE_HANDLER: RwLock<Option<fn(CueEvent)>> = RwLock::new(None);

/// Registers the app-level callback invoked when kit buttons trigger a hover
/// or press sound cue.
pub fn set_cue_handler(handler: fn(CueEvent)) {
    *CUE_HANDLER
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(handler);
}

/// Emits an interface cue if a handler has been registered by the app.
pub fn emit_cue(event: CueEvent) {
    if let Some(handler) = *CUE_HANDLER
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
    {
        handler(event);
    }
}

/// Calls `$then!` with the names of [`Theme`]'s own colours, so the app
/// that fills them names each once too.
#[macro_export]
macro_rules! for_theme_colours {
    ($then:ident) => {
        $then! {
        rail,
        sidebar,
        panel,
        chat,
        stage,
        raised,
        field,
        sunken,
        hover,
        active,
        row_hover,
        line,
        edge,
        outline,
        outline_chat,
        outline_field,
        text,
        secondary,
        muted,
        faint,
        accent,
        accent_hover,
        on_accent,
        accent_soft,
        danger,
        danger_hover,
        on_danger,
        live,
        loud,
        link,
        button_default,
        button_default_hover,
        overlay,
        shimmer,
        reply_spine,
        tooltip,
        avatar,
        }
    };
}

/// Declares [`Theme`] with its colours.
macro_rules! declare_theme {
    ($($name:ident,)*) => {
        /// A theme's colours, as the widgets paint with them.
        #[derive(Clone, Debug, PartialEq)]
        pub struct Theme {
            /// The theme's id, as `layout.json` names it.
            pub id: String,
            pub dark: bool,
            /// Bonfire or a theme made from it, which draws a few things in
            /// Discord's measured dark-theme inks instead of its palette's.
            pub bonfire: bool,
            $(pub $name: Color,)*
            /// What's worked out from those: a context menu's ground and
            /// outline, the composer's ground and placeholder, a destructive
            /// row's text.
            pub menu: Color,
            pub menu_line: Color,
            pub composer: Color,
            pub placeholder: Color,
            pub danger_text: Color,
            /// The channel list's inks: a read channel's name, glyph and a
            /// category's name, and the open or unread channel's name. Bonfire
            /// draws Discord's, measured; other themes the muted and text
            /// colours.
            pub channel_read: Color,
            pub channel_white: Color,
            /// The Friends and Requests rows above the DM list: inactive,
            /// and open (Discord's on Bonfire; the muted and text colours on
            /// other themes).
            pub nav_read: Color,
            pub nav_open: Color,
            /// Text selected in the timeline and fields: the accent at 35%.
            pub selection: Color,
            /// A mention's colour without one of its own.
            pub mention: Color,
            /// A custom theme's own colour for the rail's pills and the
            /// header row's ground (`pill` and `header`, as a package
            /// names them), where it sets one: see [`Theme::pill`] and
            /// [`Theme::header_ground`].
            pub own_pill: Option<Color>,
            pub own_header: Option<Color>,
            /// What iced's own styles read: the theme's ground, text and
            /// accent.
            iced: iced::Theme,
            /// A package's fonts, shapes and spacing, if one dresses it.
            metrics: Shared,
        }

        impl Theme {
            /// A theme named `id`, every colour clear, for the app to fill
            /// and then [`work out`](Theme::worked_out).
            pub fn new(id: &str, dark: bool) -> Self {
                Self {
                    id: id.to_owned(),
                    dark,
                    bonfire: false,
                    $($name: Color::TRANSPARENT,)*
                    menu: Color::TRANSPARENT,
                    menu_line: Color::TRANSPARENT,
                    composer: Color::TRANSPARENT,
                    placeholder: Color::TRANSPARENT,
                    danger_text: Color::TRANSPARENT,
                    channel_read: Color::TRANSPARENT,
                    channel_white: Color::TRANSPARENT,
                    nav_read: Color::TRANSPARENT,
                    nav_open: Color::TRANSPARENT,
                    selection: Color::TRANSPARENT,
                    mention: Color::TRANSPARENT,
                    own_pill: None,
                    own_header: None,
                    iced: iced::Theme::Dark,
                    metrics: Shared(None),
                }
            }
        }
    };
}

for_theme_colours!(declare_theme);

/// A theme's metrics, which compare by pointer: two themes are equal only
/// if they share the same package metrics, and comparing stays cheap.
#[derive(Clone, Debug)]
struct Shared(Option<Arc<Metrics>>);

impl PartialEq for Shared {
    fn eq(&self, other: &Self) -> bool {
        match (&self.0, &other.0) {
            (None, None) => true,
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            _ => false,
        }
    }
}

impl Theme {
    /// This theme with a package's fonts, shapes and spacing. Built-in and
    /// colour-only themes have none and keep every Wired metric.
    pub fn with_metrics(mut self, metrics: Arc<Metrics>) -> Self {
        self.set_metrics(Some(metrics));
        self
    }

    /// Sets or clears this theme's package metrics.
    pub fn set_metrics(&mut self, metrics: Option<Arc<Metrics>>) {
        self.metrics = Shared(metrics);
    }

    /// The package metrics this theme draws with, if any.
    pub fn package_metrics(&self) -> Option<&Arc<Metrics>> {
        self.metrics.0.as_ref()
    }

    /// Whether an installed theme package dresses the window.
    pub fn packaged(&self) -> bool {
        self.metrics.0.is_some()
    }

    fn metrics(&self) -> Option<&Metrics> {
        self.metrics.0.as_deref()
    }

    /// A layout outline keyed by its theme place name.
    pub fn outline(&self, place: &str) -> Option<Outline> {
        self.metrics()
            .and_then(|metrics| metrics.outlines.get(place).copied())
    }

    /// Whether the resolved theme package decorates this named surface with
    /// art or glass. Built-in and colour-only themes return false.
    pub fn decorated(&self, surface: &str) -> bool {
        self.metrics()
            .is_some_and(|metrics| metrics.decorations.contains(surface))
    }

    /// A stock radius through the package's small, medium or large class.
    pub fn radius(&self, stock: f32) -> f32 {
        let Some(metrics) = self.metrics() else {
            return stock;
        };
        let [small, medium, large] = metrics.radii;
        let class = if stock <= 0.0 {
            None
        } else if stock <= 4.0 {
            small
        } else if stock <= 8.0 {
            medium
        } else {
            large
        };
        class.unwrap_or(stock)
    }

    pub fn surface_radius(&self, surface: &str, stock: f32) -> f32 {
        self.metrics()
            .and_then(|metrics| metrics.shapes.get(surface).and_then(|shape| shape.radius))
            .unwrap_or_else(|| self.radius(stock))
    }

    /// A message band uses `row`'s radius, then `message`'s, matching the
    /// theme theme's surface fallback.
    pub fn row_radius(&self, stock: f32) -> f32 {
        self.metrics()
            .and_then(|metrics| {
                metrics
                    .shapes
                    .get("row")
                    .and_then(|shape| shape.radius)
                    .or_else(|| metrics.shapes.get("message").and_then(|shape| shape.radius))
            })
            .unwrap_or_else(|| self.radius(stock))
    }

    /// An avatar's radius is a percentage of its side in theme packages.
    pub fn avatar_radius(&self, size: f32, stock: f32) -> f32 {
        let stock_percent = if size > 0.0 {
            stock * 100.0 / size
        } else {
            0.0
        };
        let percent = self
            .metrics()
            .and_then(|metrics| metrics.shapes.get("avatar").and_then(|shape| shape.radius))
            .unwrap_or(stock_percent);
        size * percent / 100.0
    }

    /// A named surface's border, falling back to the exact border Wired
    /// supplied. Theme border colours only count with an explicit width,
    /// matching the Slint theme resolver.
    pub fn surface_border(
        &self,
        surface: &str,
        stock_radius: f32,
        stock_width: f32,
        stock_colour: Color,
    ) -> Border {
        let metrics = self.metrics();
        let shape = metrics
            .as_ref()
            .and_then(|metrics| metrics.shapes.get(surface));
        let width = shape
            .and_then(|shape| shape.border)
            .or_else(|| {
                metrics.as_ref().and_then(|metrics| {
                    (stock_width > 0.0 && stock_width <= 1.0)
                        .then_some(metrics.outline_width)
                        .flatten()
                })
            })
            .unwrap_or(stock_width);
        let color = match shape.and_then(|shape| shape.border_colour) {
            Some(colour) if shape.and_then(|shape| shape.border).is_some() => colour,
            _ => stock_colour,
        };
        Border {
            color,
            width,
            radius: self.surface_radius(surface, stock_radius).into(),
        }
    }

    pub fn surface_shadow(&self, surface: &str, stock: Shadow) -> Shadow {
        let Some(metrics) = self.metrics() else {
            return stock;
        };
        if let Some(shadow) = metrics.shapes.get(surface).and_then(|shape| shape.shadow) {
            return shadow.iced();
        }
        // The theme's shadows replace the stock ones, as Slint's
        // `Look.shadow-*` do; what had none (a row, a heading) gets none.
        let stock_has_none = stock.color.a <= 0.0;
        match metrics.shadows {
            None => stock,
            Some(_) if stock_has_none => stock,
            Some(None) => Shadow::default(),
            Some(Some(shadow)) => shadow.iced(),
        }
    }

    pub fn surface_inset(&self, surface: &str) -> Option<(f32, f32)> {
        self.metrics()
            .and_then(|metrics| metrics.shapes.get(surface).and_then(|shape| shape.inset))
    }

    /// Whether an installed package explicitly supplies this surface. This
    /// matters for trim such as profile-card sections: stock draws only the
    /// content, while a theme that names the surface adds the frame around
    /// it even when that frame happens to be transparent.
    pub fn has_surface(&self, surface: &str) -> bool {
        self.metrics()
            .is_some_and(|metrics| metrics.shapes.contains_key(surface))
    }

    pub fn surface_inset_top(&self, surface: &str) -> Option<f32> {
        self.metrics().and_then(|metrics| {
            metrics
                .shapes
                .get(surface)
                .and_then(|shape| shape.inset_top)
        })
    }

    pub fn surface_ground(&self, surface: &str) -> Option<Background> {
        self.metrics()
            .and_then(|metrics| metrics.shapes.get(surface).and_then(|shape| shape.ground))
    }

    /// A paint explicitly supplied by an installed package. Ordinary
    /// colour themes return `None`, so every call site keeps Wired's exact
    /// fallback. Surface wrappers use this for theme gradients and extra
    /// grounds such as `members`, `account`, `composer`, `menu` and `code`.
    pub fn package_paint(&self, token: &str) -> Option<Background> {
        self.metrics()
            .and_then(|metrics| metrics.paints.get(token).copied())
    }

    /// A package's explicit solid colour. Extra tokens are required to be
    /// solid by the manifest checker.
    pub fn package_colour(&self, token: &str) -> Option<Color> {
        match self.package_paint(token) {
            Some(Background::Color(colour)) => Some(colour),
            _ => None,
        }
    }

    pub fn token_colour(&self, token: &str, stock: Color) -> Color {
        self.package_colour(token).unwrap_or(stock)
    }

    /// The pills beside the rail's tiles: a package's `pill`, else a
    /// custom theme's own, else the text colour.
    pub fn pill(&self) -> Color {
        self.token_colour("pill", self.own_pill.unwrap_or(self.text))
    }

    /// The header row's own ground: a package's `header` surface, else a
    /// custom theme's own. None lets the chat's ground show under it.
    pub fn header_ground(&self) -> Option<Background> {
        self.surface_ground("header")
            .or(self.own_header.map(Background::Color))
    }

    pub fn accent_text(&self) -> Color {
        self.token_colour("accent-text", self.accent)
    }

    pub fn is_readable_text(&self) -> bool {
        self.metrics().is_some_and(|metrics| metrics.readable_text)
    }

    pub fn tooltip_surface(&self) -> Option<Arc<TooltipSurface>> {
        self.metrics().and_then(|metrics| metrics.tooltip.clone())
    }

    pub fn has_hover_cue(&self) -> bool {
        self.metrics().is_some_and(|metrics| metrics.hover_cue)
    }

    pub fn has_press_cue(&self) -> bool {
        self.metrics().is_some_and(|metrics| metrics.press_cue)
    }

    /// The effective background for a theme's surface token. An installed
    /// package's explicit paint wins; otherwise the same Wired colour that
    /// surface used before this port remains the fallback.
    pub fn surface_paint(&self, token: &str) -> Background {
        if let Some(paint) = self.package_paint(token) {
            return paint;
        }
        match token {
            "rail" => self.rail,
            "sidebar" | "embed" | "popout" | "label" => self.sidebar,
            "chat" | "members" | "header" | "aside" | "window" => self.chat,
            "panel" | "account" | "code" | "settings" => self.panel,
            "raised" | "card" | "dialog" | "toast" | "picker" | "card_over" => self.raised,
            "accent" => self.accent,
            "composer" => self.composer,
            "menu" => self.menu,
            "actions" => self.raised,
            "row-selected" => self.active,
            "tooltip" => self.tooltip,
            "stage" | "call" => self.stage,
            "sunken" | "lightbox" => self.sunken,
            _ => Color::TRANSPARENT,
        }
        .into()
    }

    pub fn surface_text(&self, surface: &str) -> Option<Color> {
        self.metrics()
            .and_then(|metrics| metrics.shapes.get(surface).and_then(|shape| shape.text))
    }

    pub fn surface_bar(&self, surface: &str) -> Option<(Background, Option<Color>)> {
        self.metrics().and_then(|metrics| {
            let shape = metrics.shapes.get(surface)?;
            shape.bar.map(|bar| (bar, shape.bar_text))
        })
    }

    pub fn surface_stripe(&self, surface: &str) -> Option<Stripe> {
        self.metrics()
            .and_then(|metrics| metrics.shapes.get(surface).and_then(|shape| shape.stripe))
    }

    pub fn surface_case(&self, surface: &str) -> Option<u8> {
        self.metrics()
            .and_then(|metrics| metrics.shapes.get(surface).and_then(|shape| shape.case))
    }

    pub fn surface_words<'a>(&self, surface: &str, words: &'a str) -> Cow<'a, str> {
        match self.surface_case(surface) {
            Some(1) => Cow::Owned(words.to_lowercase()),
            Some(2) => Cow::Owned(words.to_uppercase()),
            _ => Cow::Borrowed(words),
        }
    }

    pub fn surface_widths(&self, surface: &str) -> (Option<f32>, Option<f32>) {
        self.metrics()
            .and_then(|metrics| {
                metrics
                    .shapes
                    .get(surface)
                    .map(|shape| (shape.width, shape.hover_width))
            })
            .unwrap_or_default()
    }

    pub fn spacing(&self) -> Spacing {
        self.metrics()
            .map_or_else(Spacing::default, |metrics| metrics.spacing)
    }

    pub fn motion(&self) -> Motion {
        self.metrics()
            .map_or_else(Motion::default, |metrics| metrics.motion)
    }

    /// A text role's face and weight. Readable text restores Wired's face
    /// and weight for reading roles, while decorative headings, small text
    /// and times continue to follow the package.
    pub fn font(&self, role: &str, stock: Font) -> Font {
        let Some(metrics) = self.metrics() else {
            return stock;
        };
        if metrics.readable_text && matches!(role, "body" | "names" | "labels" | "code") {
            return stock;
        }
        let Some(kind) = metrics.types.get(role) else {
            // A role the theme doesn't dress (times, small print) is in the
            // window's own face, which is the theme's body, as Slint's
            // `default-font-family`; at its own weight and slant.
            if metrics.readable_text {
                return stock;
            }
            return match metrics.types.get("body").and_then(|body| body.font) {
                Some(face) => Font {
                    weight: stock.weight,
                    style: stock.style,
                    ..face
                },
                None => stock,
            };
        };
        let face = kind.font.unwrap_or(stock);
        let weight = kind.weight.map_or(stock.weight, type_weight);
        // Reading text keeps its emphasis, as Slint's: a bold or italic
        // run in a message stays bold or italic in the theme's face (its
        // bold file is a face of the same family).
        let reading = matches!(role, "body" | "code");
        Font {
            weight: if reading
                && stock.weight as u8 > weight as u8
                && stock.weight as u8 >= Weight::Semibold as u8
            {
                stock.weight
            } else {
                weight
            },
            style: if reading && stock.style != iced::font::Style::Normal {
                stock.style
            } else {
                face.style
            },
            ..face
        }
    }

    /// The open conversation's and server's names use headings normally,
    /// but are reading text when Readable text is on.
    pub fn header_font(&self, stock: Font) -> Font {
        if self.metrics().is_some_and(|metrics| metrics.readable_text) {
            stock
        } else {
            self.font("headings", stock)
        }
    }

    pub fn type_size(&self, role: &str, stock: f32) -> f32 {
        self.metrics()
            .and_then(|metrics| metrics.types.get(role).and_then(|kind| kind.size))
            .unwrap_or(stock)
    }

    pub fn header_size(&self, stock: f32) -> f32 {
        self.metrics()
            .and_then(|metrics| {
                let kind = metrics.types.get("headings")?;
                kind.header_size.or(kind.size)
            })
            .unwrap_or(stock)
    }

    pub fn title_size(&self, stock: f32) -> f32 {
        self.metrics()
            .and_then(|metrics| {
                let kind = metrics.types.get("headings")?;
                kind.title_size.or(kind.size)
            })
            .unwrap_or(stock)
    }

    pub fn heading<'a>(&self, words: &'a str) -> Cow<'a, str> {
        if self.metrics().is_some_and(|metrics| {
            metrics
                .types
                .get("headings")
                .and_then(|kind| kind.uppercase)
                .unwrap_or(false)
        }) {
            Cow::Owned(words.to_uppercase())
        } else {
            Cow::Borrowed(words)
        }
    }

    /// Retained for renderers that gain letter-spacing support. iced 0.14's
    /// ordinary text widget cannot consume it yet.
    pub fn letter_spacing(&self, role: &str) -> f32 {
        let Some(metrics) = self.metrics() else {
            return 0.0;
        };
        if metrics.readable_text && matches!(role, "body" | "names" | "labels" | "code") {
            return 0.0;
        }
        metrics
            .types
            .get(role)
            .and_then(|kind| kind.letter_spacing)
            .unwrap_or(0.0)
    }

    /// The colours worked out from the others, once those are set.
    pub fn worked_out(self) -> Self {
        let dark = self.dark;
        let iced = iced::Theme::custom(
            format!("Yapfast {}", self.id),
            iced::theme::Palette {
                background: self.chat,
                text: self.text,
                primary: self.accent,
                success: self.live,
                warning: self.loud,
                danger: self.danger,
            },
        );
        Self {
            menu: if dark {
                mix(self.raised, Color::WHITE, 0.98)
            } else {
                self.raised
            },
            menu_line: if dark {
                mix(self.line, Color::BLACK, 0.95)
            } else {
                self.line
            },
            composer: if dark {
                mix(self.field, Color::WHITE, 0.99)
            } else {
                self.field
            },
            placeholder: if dark {
                mix(self.muted, self.field, 0.66)
            } else {
                self.muted
            },
            // Themes can't set the danger colours, so no theme could fix a
            // red too faint on its ground: it's darkened on a light ground
            // and lightened on a dark one until it reads at 4.5:1.
            danger_text: readable_on(
                if dark {
                    crate::style::hex(0xfa777c)
                } else {
                    self.danger
                },
                self.chat,
                4.5,
            ),
            channel_read: if self.bonfire {
                crate::style::hex(0x81828a)
            } else {
                self.muted
            },
            channel_white: if self.bonfire {
                crate::style::hex(0xfbfbfb)
            } else {
                self.text
            },
            nav_read: if self.bonfire {
                crate::style::hex(0x96979e)
            } else {
                self.muted
            },
            nav_open: if self.bonfire {
                crate::style::hex(0xefeff1)
            } else {
                self.text
            },
            selection: Color {
                a: 0.35,
                ..self.accent
            },
            iced,
            ..self
        }
    }

    /// iced's own theme in these colours, for the styles iced draws itself.
    pub fn iced(&self) -> iced::Theme {
        self.iced.clone()
    }

    /// The overlay colour at `alpha`: what lifts a surface under the
    /// pointer, white on a dark theme, dark on a light one.
    pub fn wash(&self, alpha: f32) -> Color {
        Color {
            a: alpha,
            ..self.overlay
        }
    }
}

fn type_weight(weight: i32) -> Weight {
    match (weight.clamp(100, 900) + 50) / 100 {
        0 | 1 => Weight::Thin,
        2 => Weight::ExtraLight,
        3 => Weight::Light,
        4 => Weight::Normal,
        5 => Weight::Medium,
        6 => Weight::Semibold,
        7 => Weight::Bold,
        8 => Weight::ExtraBold,
        _ => Weight::Black,
    }
}

/// The Slint build's named easing curves. Keeping this in the kit lets the popup
/// widget consume a resolved motion without depending on yapfast-core.
fn ease(curve: Curve, t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    let cubic = |x1: f32, y1: f32, x2: f32, y2: f32, x: f32| {
        let bezier = |a: f32, b: f32, t: f32| {
            let u = 1.0 - t;
            3.0 * u * u * t * a + 3.0 * u * t * t * b + t * t * t
        };
        let (mut low, mut high) = (0.0f32, 1.0f32);
        for _ in 0..24 {
            let middle = (low + high) / 2.0;
            if bezier(x1, x2, middle) < x {
                low = middle;
            } else {
                high = middle;
            }
        }
        bezier(y1, y2, (low + high) / 2.0)
    };
    match curve {
        Curve::Linear => t,
        Curve::Ease => cubic(0.25, 0.1, 0.25, 1.0, t),
        Curve::EaseIn => cubic(0.42, 0.0, 1.0, 1.0, t),
        Curve::EaseOut => cubic(0.0, 0.0, 0.58, 1.0, t),
        Curve::EaseInOut => cubic(0.42, 0.0, 0.58, 1.0, t),
        Curve::Spring => {
            let (c1, c3) = (1.701_58, 2.701_58);
            1.0 + c3 * (t - 1.0).powi(3) + c1 * (t - 1.0).powi(2)
        }
        Curve::Bounce => {
            let (n, d) = (7.5625, 2.75);
            if t < 1.0 / d {
                n * t * t
            } else if t < 2.0 / d {
                let t = t - 1.5 / d;
                n * t * t + 0.75
            } else if t < 2.5 / d {
                let t = t - 2.25 / d;
                n * t * t + 0.9375
            } else {
                let t = t - 2.625 / d;
                n * t * t + 0.984_375
            }
        }
        Curve::Elastic if t == 0.0 || t == 1.0 => t,
        Curve::Elastic => {
            let c4 = std::f32::consts::TAU / 3.0;
            2f32.powf(-10.0 * t) * ((t * 10.0 - 0.75) * c4).sin() + 1.0
        }
    }
}

/// An opaque colour as `0xrrggbb`, for the client's own pictures (icons,
/// dots), which are drawn once per colour.
pub fn rgb(color: Color) -> u32 {
    let [r, g, b, _] = color.into_rgba8();
    (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b)
}

/// WCAG's relative luminance of `color`, ignoring its alpha.
fn luminance(color: Color) -> f32 {
    let linear = |value: f32| {
        if value <= 0.039_28 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * linear(color.r) + 0.7152 * linear(color.g) + 0.0722 * linear(color.b)
}

/// WCAG's contrast ratio between two colours.
pub fn contrast(a: Color, b: Color) -> f32 {
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

/// `color` as text on `ground` reading at `ratio` at least: as it is when
/// it already does, else with the least change of lightness that does,
/// darker on a light ground and lighter on a dark one. Its hue and
/// saturation stay.
pub fn readable_on(color: Color, ground: Color, ratio: f32) -> Color {
    if contrast(color, ground) >= ratio {
        return color;
    }
    let (hue, saturation, lightness) = to_hsl(color);
    let lighter = contrast(Color::WHITE, ground) > contrast(Color::BLACK, ground);
    let shade = |lightness: f32| {
        let (r, g, b) = from_hsl(hue, saturation, lightness);
        Color {
            r,
            g,
            b,
            a: color.a,
        }
    };
    // The far end reads best; search between it and the colour for the
    // nearest lightness that's enough.
    let (mut near, mut far) = (lightness, if lighter { 1.0 } else { 0.0 });
    if contrast(shade(far), ground) < ratio {
        return shade(far);
    }
    for _ in 0..24 {
        let middle = (near + far) / 2.0;
        if contrast(shade(middle), ground) >= ratio {
            far = middle;
        } else {
            near = middle;
        }
    }
    shade(far)
}

fn to_hsl(color: Color) -> (f32, f32, f32) {
    let (r, g, b) = (color.r, color.g, color.b);
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let lightness = (max + min) / 2.0;
    let delta = max - min;
    if delta == 0.0 {
        return (0.0, 0.0, lightness);
    }
    let saturation = delta / (1.0 - (2.0 * lightness - 1.0).abs());
    let hue = if max == r {
        ((g - b) / delta).rem_euclid(6.0)
    } else if max == g {
        (b - r) / delta + 2.0
    } else {
        (r - g) / delta + 4.0
    };
    (hue * 60.0, saturation, lightness)
}

fn from_hsl(hue: f32, saturation: f32, lightness: f32) -> (f32, f32, f32) {
    let chroma = (1.0 - (2.0 * lightness - 1.0).abs()) * saturation;
    let sector = hue.rem_euclid(360.0) / 60.0;
    let x = chroma * (1.0 - (sector % 2.0 - 1.0).abs());
    let (r, g, b) = match sector as u32 {
        0 => (chroma, x, 0.0),
        1 => (x, chroma, 0.0),
        2 => (0.0, chroma, x),
        3 => (0.0, x, chroma),
        4 => (x, 0.0, chroma),
        _ => (chroma, 0.0, x),
    };
    let m = lightness - chroma / 2.0;
    let channel = |value: f32| (value + m).clamp(0.0, 1.0);
    (channel(r), channel(g), channel(b))
}

/// Slint's `mix`: `factor` of `color` and the rest of `other`, weighted by
/// their alpha as Sass mixes them.
pub fn mix(color: Color, other: Color, factor: f32) -> Color {
    let factor = factor.clamp(0.0, 1.0);
    let normal = 2.0 * factor - 1.0;
    let distance = color.a - other.a;
    let weighted = normal * distance;
    let combined = if weighted == -1.0 {
        normal
    } else {
        (normal + distance) / (1.0 + weighted)
    };
    let channels = (combined + 1.0) / 2.0;
    let lerp = |a: f32, b: f32, f: f32| (a * f + b * (1.0 - f)).clamp(0.0, 1.0);
    Color {
        r: lerp(color.r, other.r, channels),
        g: lerp(color.g, other.g, channels),
        b: lerp(color.b, other.b, channels),
        a: lerp(color.a, other.a, factor),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn danger_text_reads_at_four_and_a_half_on_any_ground_keeping_its_hue() {
        // The stock look keeps its own red.
        let stock = Theme::new("test:stock-danger", true).worked_out();
        assert_eq!(stock.danger_text, crate::style::hex(0xfa777c));

        // Sakura's red on a pink light ground reads at 3.4:1 as it is.
        let mut light = Theme::new("test:light-danger", false);
        light.chat = crate::style::hex(0xf5d5e0);
        light.danger = crate::style::hex(0xd2404f);
        assert!(contrast(light.danger, light.chat) < 4.5);
        let light = light.worked_out();
        assert!(contrast(light.danger_text, light.chat) >= 4.5);
        assert!(
            contrast(light.danger_text, light.chat) < 4.7,
            "no darker than needed"
        );
        let (hue, _, lightness) = to_hsl(light.danger_text);
        let (wanted, _, before) = to_hsl(light.danger);
        assert!(
            (hue - wanted).abs() < 2.0,
            "the same red: {hue} for {wanted}"
        );
        assert!(lightness < before, "darkened on a light ground");

        // On a dark ground too faint a red is lightened.
        let mut dark = Theme::new("test:dark-danger", true);
        dark.chat = crate::style::hex(0x5a2a2e);
        let dark = dark.worked_out();
        assert!(contrast(dark.danger_text, dark.chat) >= 4.5);
        assert!(to_hsl(dark.danger_text).2 > to_hsl(crate::style::hex(0xfa777c)).2);
    }

    #[test]
    fn metrics_belong_to_the_theme_that_carries_them() {
        let stock = Theme::new("test:reused-metrics", true);
        let mut metrics = Metrics::default();
        metrics.shapes.insert(
            "button".into(),
            Shape {
                radius: Some(19.0),
                ..Shape::default()
            },
        );
        let metrics = Arc::new(metrics);
        let dressed = stock.clone().with_metrics(metrics.clone());
        assert_eq!(dressed.surface_radius("button", 6.0), 19.0);
        // A theme of the same id without them draws Wired's.
        assert_eq!(stock.surface_radius("button", 6.0), 6.0);
        assert!(dressed.packaged() && !stock.packaged());

        assert_ne!(dressed, stock);
        assert_eq!(dressed, stock.clone().with_metrics(metrics));
        assert_ne!(
            dressed,
            stock.with_metrics(Arc::new(Metrics::default())),
            "other metrics, even equal ones, are another theme"
        );
    }
}
