//! The controls every screen shares, styled from the theme: the button of
//! dialogs and forms, the single-line field, the style of a button that
//! lights only under the pointer (icon buttons, menu and list rows), a
//! menu's row and panel, the layer that takes a click outside an overlay,
//! and the tooltip. A screen builds with these instead of styling its own.

use iced::{
    Alignment, Border, Color, Element, Length, Shadow, Vector,
    widget::{Id, Space, button, column, container, mouse_area, opaque, row, text, text_input},
};

use crate::{
    fonts::{BOLD, MEDIUM},
    icons::Icon,
    style::{fill, rounded},
    theme::Theme,
};

/// How dark the window goes under a dialog.
pub const DIM: Color = Color {
    r: 0.0,
    g: 0.0,
    b: 0.0,
    a: 0.6,
};

/// A menu row's height and its icon's size.
pub const ITEM: f32 = 32.0;
pub const ITEM_ICON: u32 = 16;
/// The chevron at the end of a row that opens a submenu.
pub const SUBMENU_ARROW: u32 = 24;

/// What an [`action`] button is.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// The accent's, for the screen's main action.
    Primary,
    /// The neutral face.
    Plain,
    /// No face until the pointer is over it, in the secondary text colour.
    Ghost,
    /// Red, for an action that destroys.
    Danger,
    /// Green, as an app's success button.
    Success,
}

/// A dialog's or form's button: 36 px tall, 15 px bold, its face and ink
/// by its [`Kind`], lit under the pointer; without a message it's
/// disabled. [`Action::wide`] stretches it across its column and
/// [`Action::ring`] rings it in the accent, as a focused one.
pub fn action<'a, M: Clone + 'a>(
    theme: &Theme,
    label: impl Into<std::borrow::Cow<'a, str>>,
    kind: Kind,
    message: Option<M>,
) -> Action<'a, M> {
    let enabled = message.is_some();
    let on_accent = theme.on_accent;
    let (disabled, rest, lit) = match kind {
        Kind::Primary => (
            Color {
                a: 0.3,
                ..theme.accent
            },
            theme.accent,
            theme.accent_hover,
        ),
        Kind::Plain => (
            theme.raised,
            theme.button_default,
            theme.button_default_hover,
        ),
        Kind::Ghost => (Color::TRANSPARENT, Color::TRANSPARENT, theme.wash(0.08)),
        Kind::Danger => (theme.raised, theme.danger, theme.danger_hover),
        Kind::Success => (
            Color {
                a: 0.3,
                ..theme.live
            },
            theme.live,
            crate::theme::mix(Color::WHITE, theme.live, 0.1),
        ),
    };
    let ink = match (kind, enabled) {
        (Kind::Primary | Kind::Success, false) => Color {
            a: 0.6,
            ..on_accent
        },
        (_, false) => theme.faint,
        (Kind::Primary | Kind::Success, true) => on_accent,
        (Kind::Danger, true) => theme.on_danger,
        (Kind::Ghost | Kind::Plain, true) => theme.text,
    };
    Action {
        label: label.into(),
        message,
        width: Length::Shrink,
        height: 36.0,
        faces: Faces {
            ground: if enabled { [rest, lit] } else { [disabled; 2] },
            ink: if kind == Kind::Ghost && enabled {
                [theme.secondary, ink]
            } else {
                [ink; 2]
            },
            ring: None,
        },
        accent: theme.accent,
        small: false,
        icon: None,
    }
}

/// An [`action`] button being built.
pub struct Action<'a, M> {
    label: std::borrow::Cow<'a, str>,
    message: Option<M>,
    width: Length,
    height: f32,
    faces: Faces,
    accent: Color,
    small: bool,
    /// An icon before its label, drawn in its ink.
    icon: Option<iced::advanced::image::Handle>,
}

/// A button's ground and ink at rest and lit, and its ring.
#[derive(Clone, Copy)]
struct Faces {
    ground: [Color; 2],
    ink: [Color; 2],
    ring: Option<Color>,
}

impl<M: Clone> Action<'_, M> {
    /// As wide as its column, `height` tall.
    pub fn wide(mut self, height: f32) -> Self {
        self.width = Length::Fill;
        self.height = height;
        self
    }

    /// Ringed in the accent while `on`, as a focused button.
    pub fn ring(mut self, on: bool) -> Self {
        self.faces.ring = on.then_some(self.accent);
        self
    }

    /// An icon before the label (16 px small, else 18), drawn in its ink.
    pub fn icon(mut self, icon: iced::advanced::image::Handle) -> Self {
        self.icon = Some(icon);
        self
    }

    /// The small one: 28 px tall, 14 px text, less room each side.
    pub fn small(mut self) -> Self {
        self.height = 28.0;
        self.small = true;
        self
    }

    /// `height` tall, in place of its kind's.
    pub fn height(mut self, height: f32) -> Self {
        self.height = height;
        self
    }
}

impl<'a, M: Clone + 'a> From<Action<'a, M>> for Element<'a, M> {
    fn from(action: Action<'a, M>) -> Self {
        let faces = action.faces;
        button(
            container({
                let label = text(action.label)
                    .size(if action.small { 14 } else { 15 })
                    .font(BOLD);
                let side = if action.small { 16.0 } else { 18.0 };
                match action.icon {
                    Some(icon) => Element::from(
                        row![crate::widgets::Pic::square(icon, side), label]
                            .spacing(8)
                            .align_y(Alignment::Center),
                    ),
                    None => label.into(),
                }
            })
            .height(Length::Fill)
            .align_y(Alignment::Center)
            .center_x(action.width),
        )
        .padding([0, if action.small { 10 } else { 16 }])
        .width(action.width)
        .height(action.height)
        .on_press_maybe(action.message)
        .style(move |_, status| {
            let lit = usize::from(lights(status));
            button::Style {
                background: Some(faces.ground[lit].into()),
                text_color: faces.ink[lit],
                border: Border {
                    color: faces.ring.unwrap_or(Color::TRANSPARENT),
                    width: if faces.ring.is_some() { 2.0 } else { 0.0 },
                    radius: 6.0.into(),
                },
                ..Default::default()
            }
        })
        .into()
    }
}

/// The style of a button with no face of its own, or `rest`, that shows
/// `lit` under the pointer: icon buttons, menu and list rows.
pub fn lit_style(
    rest: Option<Color>,
    lit: Color,
    radius: f32,
) -> impl Fn(&iced::Theme, button::Status) -> button::Style + Copy {
    move |_, status| button::Style {
        background: if lights(status) {
            Some(lit.into())
        } else {
            rest.map(Into::into)
        },
        border: Border::default().rounded(radius),
        ..Default::default()
    }
}

/// The style of a card that's a button: `rest` in a 1 px `edge`, `lit`
/// in `edge_lit` under the pointer.
pub fn outlined_lit_style(
    rest: Color,
    lit: Color,
    edge: Color,
    edge_lit: Color,
    radius: f32,
) -> impl Fn(&iced::Theme, button::Status) -> button::Style + Copy {
    move |_, status| {
        let lit_now = lights(status);
        button::Style {
            background: Some(if lit_now { lit } else { rest }.into()),
            border: crate::style::edge(if lit_now { edge_lit } else { edge }, radius),
            ..Default::default()
        }
    }
}

/// The style of a button that is only its text, in `rest`, brightening to
/// `lit` under the pointer: a link-like name, as a thread's channel in the
/// header.
pub fn ink_style(
    rest: Color,
    lit: Color,
) -> impl Fn(&iced::Theme, button::Status) -> button::Style + Copy {
    move |_, status| button::Style {
        text_color: if lights(status) { lit } else { rest },
        ..Default::default()
    }
}

/// Whether a button shows lit: under the pointer, or pressed.
pub fn lights(status: button::Status) -> bool {
    matches!(status, button::Status::Hovered | button::Status::Pressed)
}

/// A single-line field: the sunken ground, ringed in the accent while it
/// has the keys ([`field_style`]). It takes no input until given
/// `on_input`, as a disabled field, and greys its text while not `enabled`.
pub fn field<'a, M: Clone + 'a>(
    theme: &Theme,
    id: impl Into<Id>,
    placeholder: &str,
    value: &str,
    secure: bool,
    enabled: bool,
) -> text_input::TextInput<'a, M> {
    text_input(placeholder, value)
        .id(id)
        .secure(secure)
        .size(16)
        .padding([9, 12])
        .style(field_style(theme, theme.sunken, enabled))
}

/// A single-line field's look on `ground`: ringed in the accent while it
/// has the keys, its text greyed while not `enabled`.
pub fn field_style(
    theme: &Theme,
    ground: Color,
    enabled: bool,
) -> impl Fn(&iced::Theme, text_input::Status) -> text_input::Style + use<> {
    let (ink, hint, faint, accent, selection) = (
        theme.text,
        theme.muted,
        theme.faint,
        theme.accent,
        theme.selection,
    );
    move |_, status| text_input::Style {
        background: ground.into(),
        border: Border {
            color: Color { a: 0.8, ..accent },
            width: if matches!(status, text_input::Status::Focused { .. }) {
                1.0
            } else {
                0.0
            },
            radius: 6.0.into(),
        },
        icon: ink,
        placeholder: if enabled { hint } else { faint },
        value: if enabled { ink } else { faint },
        selection,
    }
}

/// The window's tooltip: 32 px tall, 14 px bold on the theme's tooltip
/// ground.
pub fn tip<'a, M: Clone + 'a>(theme: &Theme, label: impl text::IntoFragment<'a>) -> Element<'a, M> {
    container(text(label).size(14).font(BOLD).color(theme.text))
        .padding([0, 10])
        .height(32)
        .align_y(Alignment::Center)
        .style(tip_style(theme))
        .into()
}

/// A tooltip's ground, round corners and shadow, for one with more in it
/// than a label (a reaction's).
pub fn tip_style(theme: &Theme) -> impl Fn(&iced::Theme) -> container::Style + use<> {
    let ground = theme.tooltip;
    move |_| container::Style {
        shadow: Shadow {
            color: Color {
                a: 0.5,
                ..Color::BLACK
            },
            offset: Vector::new(0.0, 3.0),
            blur_radius: 12.0,
        },
        ..rounded(ground, 6)
    }
}

/// A full-window layer under an overlay that takes a click outside it, so
/// nothing under it gets one: clear under a menu, the picker or the card,
/// dimmed by [`DIM`] under a dialog.
pub fn outside<'a, M: Clone + 'a>(message: M, dim: bool) -> Element<'a, M> {
    if dim {
        return dimmed(message, DIM);
    }
    let layer = container(Space::new())
        .width(Length::Fill)
        .height(Length::Fill);
    opaque(mouse_area(layer).on_press(message))
}

/// The id of every dimmed layer, by which the tests tell what's under one.
pub const DIMMED: &str = "dimmed";

/// The window dimmed by `dim` under a dialog or a settings card, which a
/// click on sends `message`; nothing under it takes one.
pub fn dimmed<'a, M: Clone + 'a>(message: M, dim: Color) -> Element<'a, M> {
    // The dim fades in as the popup over it opens (`widgets::motion`).
    let layer = container(Space::new())
        .id(Id::new(DIMMED))
        .width(Length::Fill)
        .height(Length::Fill);
    opaque(mouse_area(crate::widgets::motion::dim(layer, dim)).on_press(message))
}

/// `content` with an edge round it `radius` round, drawn over it last: a
/// card whose parts paint their own grounds out to its edge keeps its
/// outline (iced's `clip` cuts to a square, not to the corners).
pub fn edged<'a, M: Clone + 'a>(
    content: impl Into<Element<'a, M>>,
    edge: Color,
    radius: f32,
) -> Element<'a, M> {
    iced::widget::stack![content.into(), Outline { edge, radius }].into()
}

/// An edge alone, over what it's stacked on: it takes no event and shows
/// to an operation as nothing, as it hides nothing.
struct Outline {
    edge: Color,
    radius: f32,
}

impl<M> iced::advanced::Widget<M, iced::Theme, iced::Renderer> for Outline {
    fn size(&self) -> iced::Size<Length> {
        iced::Size::new(Length::Fill, Length::Fill)
    }

    fn layout(
        &mut self,
        _tree: &mut iced::advanced::widget::Tree,
        _renderer: &iced::Renderer,
        limits: &iced::advanced::layout::Limits,
    ) -> iced::advanced::layout::Node {
        iced::advanced::layout::Node::new(limits.max())
    }

    fn draw(
        &self,
        _tree: &iced::advanced::widget::Tree,
        renderer: &mut iced::Renderer,
        _theme: &iced::Theme,
        _style: &iced::advanced::renderer::Style,
        layout: iced::advanced::Layout<'_>,
        _cursor: iced::advanced::mouse::Cursor,
        _viewport: &iced::Rectangle,
    ) {
        use iced::advanced::Renderer as _;
        renderer.fill_quad(
            iced::advanced::renderer::Quad {
                bounds: layout.bounds(),
                border: crate::style::edge(self.edge, self.radius),
                ..Default::default()
            },
            Color::TRANSPARENT,
        );
    }
}

impl<'a, M: Clone + 'a> From<Outline> for Element<'a, M> {
    fn from(outline: Outline) -> Self {
        Element::new(outline)
    }
}

/// A menu's panel: the menu's ground in its outline, round corners, the
/// raised shadow.
pub fn menu_panel(theme: &Theme) -> impl Fn(&iced::Theme) -> container::Style + use<> {
    let ground = theme.menu;
    let edge = if theme.bonfire {
        crate::style::hex(0x35353a)
    } else {
        theme.menu_line
    };
    move |_| crate::style::panel(ground, edge, 8.0)
}

/// A box of one colour: a line between parts of the window, a bar, a dot.
pub fn bar<'a, M: Clone + 'a>(
    color: Color,
    width: impl Into<Length>,
    height: impl Into<Length>,
) -> container::Container<'a, M> {
    container(Space::new())
        .width(width)
        .height(height)
        .style(move |_| fill(color))
}

/// The line between a menu's groups of rows.
pub fn separator<'a, M: Clone + 'a>(theme: &Theme) -> Element<'a, M> {
    container(bar(theme.line, Length::Fill, 1))
        .padding([4, 0])
        .into()
}

/// The line between a context menu's groups of rows as Discord draws it:
/// 17 px, its line 8 px down.
pub fn menu_separator<'a, M: Clone + 'a>(theme: &Theme) -> Element<'a, M> {
    let line = if theme.bonfire {
        crate::style::hex(0x35353a)
    } else {
        theme.line
    };
    container(bar(line, Length::Fill, 1))
        .padding(iced::Padding::ZERO.top(8).bottom(8))
        .into()
}

/// A row of a menu or of a list of choices, as [`App::menu_item`] draws it.
#[derive(Clone)]
pub struct Item<'a> {
    pub label: std::borrow::Cow<'a, str>,
    pub icon: Option<Icon>,
    /// A destructive choice, drawn in the danger colours.
    pub danger: bool,
    /// Room kept for an icon when it has none, so rows line up; a place's
    /// menu keeps none.
    pub slot: bool,
    /// Shown greyed, not to be chosen.
    pub disabled: bool,
    /// A second line under it, in the muted ink: what a device row has
    /// chosen, what a choice does.
    pub subtitle: Option<std::borrow::Cow<'a, str>>,
    pub check: Check,
}

/// What marks a row as chosen: nothing, a tick in the accent in its icon's
/// place with its label bold (the device in use), or a checkbox at its end.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Check {
    #[default]
    None,
    Current,
    Box(bool),
    /// It opens a submenu: a chevron at its end.
    Submenu,
}

/// Which light a menu row shows: under the pointer, or only while the menu
/// says (a context menu, whose light the arrow keys move too).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Light {
    Pointer,
    Lit(bool),
}

impl crate::look::Look<'_> {
    /// An empty screen's words, under its icon on a disc, and maybe a
    /// button.
    pub fn empty_screen<'a, M: Clone + 'a>(
        self,
        icon: Icon,
        heading: &'a str,
        words: &'a str,
        offer: Option<(&'a str, M)>,
    ) -> Element<'a, M> {
        let theme = self.theme;
        let raised = theme.raised;
        let mut page = column![
            container(self.glyph(icon, 36, theme.secondary))
                .center(80)
                .style(move |_| rounded(raised, 40)),
            text(heading).size(22).font(BOLD).color(theme.text),
            text(words)
                .size(14)
                .color(theme.secondary)
                .align_x(Alignment::Center),
        ]
        .spacing(10)
        .align_x(Alignment::Center);
        if let Some((label, message)) = offer {
            page = page.push(action(theme, label, Kind::Primary, Some(message)));
        }
        container(page).padding(32).center(Length::Fill).into()
    }

    /// A menu row: 32 px, its icon in a 16 px slot (kept empty without
    /// one, so rows line up), 14 px medium text; the overlay's wash when
    /// lit, or the danger colour with white ink for a destructive row.
    pub fn menu_item<'a, M: Clone + 'a>(
        self,
        item: Item<'a>,
        light: Light,
        press: M,
    ) -> Element<'a, M> {
        self.menu_item_sized(item, light, press, ITEM)
    }

    /// A context menu's row as Discord draws them: 32 px, 34 with an arrow,
    /// 36 with an icon or a tick, 51 with a second line, the plain ones
    /// with 2 px of room under them, so rows with no icon stand 34 apart.
    pub fn context_item<'a, M: Clone + 'a>(
        self,
        item: Item<'a>,
        light: Light,
        press: M,
    ) -> Element<'a, M> {
        let iconic = item.icon.is_some() || matches!(item.check, Check::Current | Check::Box(_));
        let height = if item.subtitle.is_some() {
            51.0
        } else if iconic {
            36.0
        } else if item.check == Check::Submenu {
            34.0
        } else {
            ITEM
        };
        let row = self.menu_item_sized(item, light, press, height);
        if iconic {
            row
        } else {
            container(row).padding(iced::Padding::ZERO.bottom(2)).into()
        }
    }

    /// A menu row `height` tall (44 px with a subtitle, or `height` if that
    /// is taller), as the account's popover's 40 px rows.
    pub fn menu_item_sized<'a, M: Clone + 'a>(
        self,
        item: Item<'a>,
        light: Light,
        press: M,
        height: f32,
    ) -> Element<'a, M> {
        let theme = self.theme;
        let lit = light == Light::Lit(true);
        // Discord tints a destructive row under the pointer reddish and
        // leaves its text red.
        let ink = |_: bool| {
            if item.danger {
                theme.danger_text
            } else {
                theme.text
            }
        };
        let icon_ink = match (item.danger, lit) {
            (false, false) => theme.secondary,
            _ => ink(lit),
        };
        let slot = ITEM_ICON as f32;
        let current = item.check == Check::Current;
        let tall = item.subtitle.is_some();
        let mut face = row![].spacing(8).align_y(Alignment::Center);
        match item.icon {
            _ if current => face = face.push(self.glyph(Icon::Check, ITEM_ICON, theme.accent)),
            Some(icon) => face = face.push(self.glyph(icon, ITEM_ICON, icon_ink)),
            None if item.slot => face = face.push(Space::new().width(slot).height(slot)),
            None => {}
        }
        let label = text(item.label)
            .size(14)
            .font(if current { BOLD } else { MEDIUM })
            .wrapping(text::Wrapping::None);
        let words: Element<'a, M> = match item.subtitle {
            Some(subtitle) => column![
                label,
                text(subtitle)
                    .size(12)
                    .color(theme.muted)
                    .wrapping(text::Wrapping::None)
            ]
            .spacing(3)
            .into(),
            None => label.into(),
        };
        let mut face = face.push(container(words).width(Length::Fill).clip(true));
        if let Check::Box(ticked) = item.check {
            face = face.push(self.checkbox(ticked, lit));
        }
        if item.check == Check::Submenu {
            face = face.push(self.glyph(Icon::ChevronRight, SUBMENU_ARROW, theme.text));
        }
        let height = if tall { height.max(44.0) } else { height };
        let face = container(face)
            .height(Length::Fill)
            .align_y(Alignment::Center);
        let light_fill = match (item.danger, theme.bonfire) {
            (true, true) => crate::style::hex(0x34282d),
            (true, false) => crate::style::faded(theme.danger, 0.1),
            (false, _) => theme.wash(0.06),
        };
        let inks = [ink(false), ink(true)];
        // A disabled row: the whole of it at .45.
        let inks = if item.disabled {
            inks.map(|ink| crate::style::faded(ink, 0.45))
        } else {
            inks
        };
        button(face)
            .padding([0, 8])
            .width(Length::Fill)
            .height(height)
            .on_press_maybe((!item.disabled).then_some(press))
            .style(move |_, status| {
                let on = match light {
                    Light::Pointer => lights(status),
                    Light::Lit(lit) => lit,
                } && !matches!(status, button::Status::Disabled);
                button::Style {
                    background: on.then(|| light_fill.into()),
                    text_color: inks[usize::from(on)],
                    border: Border::default().rounded(4),
                    ..Default::default()
                }
            })
            .into()
    }

    /// A menu row's checkbox: 18 px, the accent's with a tick in it while
    /// ticked, else an outline in the muted ink, or the text's while its
    /// row is lit.
    pub fn checkbox<'a, M: Clone + 'a>(self, ticked: bool, lit: bool) -> Element<'a, M> {
        let theme = self.theme;
        let (accent, edge) = (theme.accent, if lit { theme.text } else { theme.muted });
        let tick: Element<'a, M> = if ticked {
            self.glyph(Icon::Check, 12, theme.on_accent).into()
        } else {
            Space::new().into()
        };
        container(tick)
            .center(18)
            .style(move |_| container::Style {
                background: ticked.then(|| accent.into()),
                border: Border {
                    color: if ticked { Color::TRANSPARENT } else { edge },
                    width: if ticked { 0.0 } else { 1.5 },
                    radius: 4.0.into(),
                },
                ..Default::default()
            })
            .into()
    }
}

/// An icon button: a square face lit under the pointer, its label in the
/// window's tooltip (none for an empty label), greyed and faint without a
/// message. Made by
/// [`Look::icon_button`](crate::look::Look::icon_button), 28 px across with
/// an 18 px glyph in the secondary ink until told otherwise.
#[must_use]
pub struct IconButton<'a, M> {
    look: crate::look::Look<'a>,
    face: Face,
    ink: Option<Color>,
    label: std::borrow::Cow<'a, str>,
    message: Option<M>,
    side: f32,
    glyph: u32,
    ground: Ground,
    radius: f32,
    below: bool,
}

enum Face {
    Icon(Icon),
    /// A face of its own, which shows once.
    Own(Option<crate::widgets::Pic>),
}

#[derive(Clone, Copy)]
enum Ground {
    /// None at rest, the overlay's wash under the pointer.
    Clear,
    /// Its own ground, a tenth of the overlay lighter under the pointer.
    Filled(Color),
    /// Destructive: red on a red wash under the pointer, on a ground at
    /// rest or none.
    Danger(Option<Color>),
}

impl<'a> crate::look::Look<'a> {
    /// An icon button showing `icon`; see [`IconButton`].
    pub fn icon_button<M: Clone>(
        self,
        icon: Icon,
        label: impl Into<std::borrow::Cow<'a, str>>,
        message: impl Into<Option<M>>,
    ) -> IconButton<'a, M> {
        IconButton {
            look: self,
            face: Face::Icon(icon),
            ink: None,
            label: label.into(),
            message: message.into(),
            side: 28.0,
            glyph: 18,
            ground: Ground::Clear,
            radius: 6.0,
            below: false,
        }
    }

    /// An icon button showing `face` of its own, `side` across.
    pub fn face_button<M: Clone>(
        self,
        face: crate::widgets::Pic,
        side: f32,
        label: impl Into<std::borrow::Cow<'a, str>>,
        message: impl Into<Option<M>>,
    ) -> IconButton<'a, M> {
        IconButton {
            face: Face::Own(Some(face)),
            side,
            ..self.icon_button(Icon::X, label, message)
        }
    }
}

impl<'a, M: Clone + 'a> IconButton<'a, M> {
    /// `side` across, its glyph `glyph` square.
    pub fn size(mut self, side: f32, glyph: u32) -> Self {
        self.side = side;
        self.glyph = glyph;
        self
    }

    /// The glyph in `ink` rather than the secondary ink.
    pub fn ink(mut self, ink: Color) -> Self {
        self.ink = Some(ink);
        self
    }

    /// On a ground of its own, as `IconButton` with a `fill`.
    pub fn fill(mut self, fill: Color) -> Self {
        self.ground = Ground::Filled(fill);
        self
    }

    /// Destructive, as `IconButton` with `danger`, on `fill` or no ground.
    pub fn danger(mut self, fill: Option<Color>) -> Self {
        self.ground = Ground::Danger(fill);
        self
    }

    pub fn radius(mut self, radius: f32) -> Self {
        self.radius = radius;
        self
    }

    /// Its tip below it, as the header's (`tip-side: 2`), not above.
    pub fn below(mut self) -> Self {
        self.below = true;
        self
    }
}

impl<'a, M: Clone + 'a> From<IconButton<'a, M>> for Element<'a, M> {
    fn from(icon_button: IconButton<'a, M>) -> Self {
        let IconButton {
            look,
            mut face,
            ink,
            label,
            message,
            side,
            glyph,
            ground,
            radius,
            below,
        } = icon_button;
        let theme = look.theme;
        let enabled = message.is_some();
        let rest_ink = ink.unwrap_or(if enabled || matches!(ground, Ground::Clear) {
            theme.secondary
        } else {
            theme.faint
        });
        // A face of its own shows once: a destructive button, whose lit
        // face is its glyph again in red, has none.
        let mut face_in = |ink: Color| match &mut face {
            Face::Icon(icon) => look.glyph(*icon, glyph, ink).boxed(side),
            Face::Own(own) => own
                .take()
                .expect("a face of its own shows once")
                .boxed(side),
        };
        let pressed = |face, style| {
            button(face)
                .padding(0)
                .width(side)
                .height(side)
                .on_press_maybe(message.clone())
                .style(style)
        };
        let shown: Element<'a, M> = match ground {
            Ground::Clear => {
                pressed(face_in(rest_ink), lit_style(None, theme.wash(0.1), radius)).into()
            }
            Ground::Filled(fill) => {
                let lit = crate::theme::mix(fill, theme.overlay, 0.9);
                pressed(
                    face_in(rest_ink),
                    lit_style(Some(fill), if enabled { lit } else { fill }, radius),
                )
                .into()
            }
            Ground::Danger(fill) => {
                let rest = pressed(
                    face_in(rest_ink),
                    lit_style(fill, fill.unwrap_or_default(), radius),
                );
                if !enabled {
                    return rest.into();
                }
                let red = Color {
                    a: 0.16,
                    ..theme.danger
                };
                let lit = pressed(face_in(theme.danger), lit_style(Some(red), red, radius));
                crate::widgets::Hover::new(rest, lit).into()
            }
        };
        if label.is_empty() {
            return shown;
        }
        let side = if below {
            iced::widget::tooltip::Position::Bottom
        } else {
            iced::widget::tooltip::Position::Top
        };
        iced::widget::tooltip(shown, tip(theme, label), side)
            .gap(6)
            .into()
    }
}

/// `child` at `x`, `y` from its layer's corner, for the layers of a stack.
pub fn placed<'a, M: Clone + 'a>(
    child: impl Into<Element<'a, M>>,
    x: f32,
    y: f32,
) -> Element<'a, M> {
    container(child)
        .padding(iced::Padding {
            top: y,
            left: x,
            ..iced::Padding::ZERO
        })
        .into()
}

/// A count pill's metrics: its height, its text's size, and the room each
/// side of the text.
#[derive(Clone, Copy)]
pub struct Count {
    pub height: f32,
    pub size: f32,
    pub pad: f32,
}

/// The mention count on a tile: 16 px, 12 px bold, on a 3 px ring.
pub const RAIL_COUNT: Count = Count {
    height: 16.0,
    size: 12.0,
    pad: 4.0,
};
/// A channel's count of mentions: 18 px, 11 px bold.
pub const ROW_COUNT: Count = Count {
    height: 18.0,
    size: 11.0,
    pad: 5.0,
};
/// An inbox row's count of mentions: 16 px, 11 px bold.
pub const INBOX_COUNT: Count = Count {
    height: 16.0,
    size: 11.0,
    pad: 5.0,
};

/// A count as a badge shows it: nothing for none, and at most "99+".
pub fn count_badge(count: u64) -> String {
    match count {
        0 => String::new(),
        count if count > 99 => "99+".into(),
        count => count.to_string(),
    }
}

/// A count in a red pill, as the rail's mentions (on a ring of the rail's
/// colour) and a channel's show; at least as wide as it's tall.
pub fn count_pill<'a, M: Clone + 'a>(
    theme: &Theme,
    count: &str,
    look: Count,
    ring: Option<Color>,
) -> Element<'a, M> {
    let danger = theme.danger;
    let number = text(count.to_owned())
        .size(look.size)
        .font(BOLD)
        .color(theme.on_danger)
        .wrapping(text::Wrapping::None);
    let pill = container(number)
        .padding([0.0, look.pad])
        .height(look.height)
        .align_y(Alignment::Center)
        .align_x(Alignment::Center);
    let pill = if count.chars().count() < 2 {
        pill.width(look.height)
    } else {
        pill
    };
    let pill = pill.style(move |_| rounded(danger, look.height / 2.0));
    match ring {
        Some(ring) => container(pill)
            .padding(3)
            .style(move |_| rounded(ring, look.height / 2.0 + 3.0))
            .into(),
        None => pill.into(),
    }
}

/// Discord's red LIVE tag, 16 px tall: beside someone in a voice channel,
/// on a tile's name, in the dock. With `press` it's a button that watches
/// the stream, lit under the pointer.
pub fn live_pill<'a, M: Clone + 'a>(theme: &Theme, press: Option<M>) -> Element<'a, M> {
    let (rest, lit) = (theme.danger, theme.danger_hover);
    let face = container(text("LIVE").size(10).font(BOLD).color(theme.on_danger))
        .padding([0, 5])
        .height(16)
        .align_y(Alignment::Center);
    match press {
        Some(press) => button(face)
            .padding(0)
            .on_press(press)
            .style(move |_, status| button::Style {
                background: Some(if lights(status) { lit } else { rest }.into()),
                border: Border::default().rounded(8),
                ..Default::default()
            })
            .into(),
        None => face.style(move |_| rounded(rest, 8)).into(),
    }
}

/// A heading over a group of a menu's choices, as the share menu's
/// "SCREENS": 12 px bold in the muted ink.
pub fn heading<'a, M: Clone + 'a>(theme: &Theme, label: &'a str, top: f32) -> Element<'a, M> {
    container(text(label).size(12).font(BOLD).color(theme.muted))
        .padding(iced::Padding {
            top,
            right: 0.0,
            bottom: 4.0,
            left: 8.0,
        })
        .into()
}

/// A volume slider: its label and the value as a percentage over a 4 px
/// track, the accent filling it up to a round handle in the text's colour.
/// Past 100% (a `maximum` over it) it marks 100% and snaps to it within
/// four points, as Discord's does.
pub struct Slider<'a, M> {
    label: std::borrow::Cow<'a, str>,
    value: u32,
    maximum: u32,
    on_change: Box<dyn Fn(u32) -> M + 'a>,
    on_release: Option<M>,
    step: u32,
    shown: Option<String>,
    mark: bool,
}

/// A [`Slider`] for `value` out of `maximum`.
pub fn slider<'a, M: Clone + 'a>(
    label: impl Into<std::borrow::Cow<'a, str>>,
    value: u32,
    maximum: u32,
    on_change: impl Fn(u32) -> M + 'a,
) -> Slider<'a, M> {
    Slider {
        label: label.into(),
        value: value.min(maximum),
        maximum,
        on_change: Box::new(on_change),
        on_release: None,
        step: 5,
        shown: None,
        mark: maximum > 100,
    }
}

impl<'a, M: Clone + 'a> Slider<'a, M> {
    /// What's sent when the drag or the keys let go of it, when it's kept.
    pub fn on_release(mut self, message: M) -> Self {
        self.on_release = Some(message);
        self
    }

    /// How far a step goes; 5 unless set.
    pub fn step(mut self, step: u32) -> Self {
        self.step = step;
        self
    }

    /// The value as shown, in place of its percentage.
    pub fn shown(mut self, shown: String) -> Self {
        self.shown = Some(shown);
        self
    }

    /// Whether 100% is marked and snapped to; past 100% unless set.
    pub fn mark(mut self, mark: bool) -> Self {
        self.mark = mark;
        self
    }

    pub fn view(self, theme: &Theme) -> Element<'a, M> {
        let Slider {
            label,
            value,
            maximum,
            on_change,
            on_release,
            step,
            shown,
            mark,
        } = self;
        let head = row![
            text(label)
                .size(13)
                .font(MEDIUM)
                .color(theme.secondary)
                .width(Length::Fill),
            text(shown.unwrap_or_else(|| format!("{value}%")))
                .size(13)
                .font(MEDIUM)
                .color(theme.text),
        ];
        let (field, accent, knob) = (theme.field, theme.accent, theme.text);
        let track = iced::widget::slider(0..=maximum, value, move |value| {
            let value = if mark && value.abs_diff(100) <= 4 {
                100
            } else {
                value
            };
            on_change(value)
        })
        .step(step)
        .height(18)
        .style(move |_, _| iced::widget::slider::Style {
            rail: iced::widget::slider::Rail {
                backgrounds: (accent.into(), field.into()),
                width: 4.0,
                border: Border::default().rounded(2),
            },
            handle: iced::widget::slider::Handle {
                shape: iced::widget::slider::HandleShape::Circle { radius: 7.0 },
                background: knob.into(),
                border_width: 0.0,
                border_color: Color::TRANSPARENT,
            },
        });
        let track = match on_release {
            Some(message) => track.on_release(message),
            None => track,
        };
        let track: Element<'a, M> = if mark {
            // The handle travels the track less its own width; 100% sits
            // where the handle's middle does there.
            let secondary = theme.secondary;
            iced::widget::stack![
                iced::widget::responsive(move |size| {
                    let x = 7.0 + (size.width - 14.0) * 100.0 / maximum as f32 - 1.0;
                    container(bar(secondary, 2, 10))
                        .padding(iced::Padding {
                            left: x.max(0.0),
                            top: 4.0,
                            ..iced::Padding::ZERO
                        })
                        .into()
                })
                .height(18),
                track,
            ]
            .into()
        } else {
            track.into()
        };
        column![head, track].spacing(6).into()
    }
}

/// A round control of the call's: 52 px across on `fill`, its glyph 22 px
/// in `ink`, lifted under the pointer; its label in a tooltip over it.
pub fn round_button<'a, M: Clone + 'a>(
    face: crate::widgets::Pic,
    label: impl text::IntoFragment<'a>,
    fill: Color,
    theme: &Theme,
    message: Option<M>,
) -> Element<'a, M> {
    let lit = crate::theme::mix(fill, theme.overlay, 0.9);
    let enabled = message.is_some();
    let face = button(face.boxed(ROUND))
        .padding(0)
        .width(ROUND)
        .height(ROUND)
        .on_press_maybe(message)
        .style(move |_, status| button::Style {
            background: Some(if enabled && lights(status) { lit } else { fill }.into()),
            border: Border::default().rounded(ROUND / 2.0),
            ..Default::default()
        });
    iced::widget::tooltip(
        face,
        tip(theme, label),
        iced::widget::tooltip::Position::Top,
    )
    .gap(6)
    .into()
}

/// A call control's side and its glyph's.
pub const ROUND: f32 = 52.0;
pub const ROUND_GLYPH: u32 = 22;

/// A button with its label under it, as the one that answers a call: a 64
/// by 48 pill of `fill`, darker under the pointer, its icon in white.
pub fn call_button<'a, M: Clone + 'a>(
    face: crate::widgets::Pic,
    label: &'a str,
    fill: Color,
    theme: &Theme,
    message: Option<M>,
) -> Element<'a, M> {
    let lit = crate::theme::mix(fill, Color::BLACK, 0.85);
    let opacity = if message.is_some() { 1.0 } else { 0.5 };
    let pill = button(face.boxed(Length::Fill))
        .padding(0)
        .width(64)
        .height(48)
        .on_press_maybe(message)
        .style(move |_, status| button::Style {
            background: Some(
                crate::style::faded(if lights(status) { lit } else { fill }, opacity).into(),
            ),
            border: Border::default().rounded(24),
            ..Default::default()
        });
    column![
        pill,
        text(label)
            .size(13)
            .color(crate::style::faded(theme.secondary, opacity))
    ]
    .spacing(6)
    .align_x(Alignment::Center)
    .into()
}

/// One of the dock's wide buttons: 32 px tall, its icon in the middle, on
/// the raised ground, the live colour's wash while what it starts is on;
/// its label in a tooltip.
pub fn dock_button<'a, M: Clone + 'a>(
    face: crate::widgets::Pic,
    label: impl text::IntoFragment<'a>,
    on: bool,
    theme: &Theme,
    message: Option<M>,
) -> Element<'a, M> {
    let (rest, lit) = if on {
        (
            crate::style::faded(theme.live, 0.22),
            crate::style::faded(theme.live, 0.32),
        )
    } else {
        (theme.raised, theme.hover)
    };
    let enabled = message.is_some();
    let face = button(face.boxed(Length::Fill))
        .padding(0)
        .width(Length::Fill)
        .height(32)
        .on_press_maybe(message)
        .style(move |_, status| button::Style {
            background: Some(if enabled && lights(status) { lit } else { rest }.into()),
            border: Border::default().rounded(8),
            ..Default::default()
        });
    iced::widget::tooltip(
        face,
        tip(theme, label),
        iced::widget::tooltip::Position::Top,
    )
    .gap(6)
    .into()
}

/// A row of choices, one chosen: on the sunken ground, the chosen one
/// raised in the active colour and bold.
pub fn choices<'a, M: Clone + 'a>(
    theme: &Theme,
    options: &[&'a str],
    current: usize,
    on_choose: impl Fn(usize) -> M,
) -> Element<'a, M> {
    choices_within(theme, options, current, 360.0, on_choose)
}

/// [`choices`] at most `widest` across, for a row of many.
pub fn choices_within<'a, M: Clone + 'a>(
    theme: &Theme,
    options: &[&'a str],
    current: usize,
    widest: f32,
    on_choose: impl Fn(usize) -> M,
) -> Element<'a, M> {
    let mut row = row![].spacing(3).padding(3);
    for (index, option) in options.iter().enumerate() {
        let chosen = index == current;
        let (active, hover) = (theme.active, theme.hover);
        let (ink, lit_ink) = if chosen {
            (theme.text, theme.text)
        } else {
            (theme.secondary, theme.text)
        };
        row = row.push(
            button(
                container(
                    text(*option)
                        .size(14)
                        .font(if chosen { BOLD } else { MEDIUM })
                        .wrapping(text::Wrapping::None),
                )
                .center_x(Length::Fill)
                .height(Length::Fill)
                .align_y(Alignment::Center),
            )
            .padding([0, 4])
            .width(Length::FillPortion(option.len() as u16 + 6))
            .height(Length::Fill)
            .on_press(on_choose(index))
            .style(move |_, status| {
                let lit = lights(status);
                button::Style {
                    background: if chosen {
                        Some(active.into())
                    } else {
                        lit.then(|| hover.into())
                    },
                    text_color: if lit { lit_ink } else { ink },
                    border: Border::default().rounded(6),
                    ..Default::default()
                }
            }),
        );
    }
    let sunken = theme.sunken;
    container(row)
        .height(36)
        .max_width(widest)
        .style(move |_| rounded(sunken, 8))
        .into()
}

/// A tab with a bar under it while chosen (the full profile's, the voice
/// details'): its words `height` tall over a 2 px bar, `ink` while chosen
/// or under the pointer, `rest` otherwise, bold while chosen and in
/// `rest_font` otherwise.
pub struct Tab<'a> {
    pub label: &'a str,
    pub chosen: bool,
    pub ink: Color,
    pub rest: Color,
    pub bar: Color,
    pub height: f32,
    pub rest_font: iced::Font,
}

pub fn tab<'a, M: Clone + 'a>(tab: Tab<'a>, press: M) -> Element<'a, M> {
    let Tab {
        label,
        chosen,
        ink,
        rest,
        bar,
        height,
        rest_font,
    } = tab;
    let words = text(label)
        .size(14)
        .font(if chosen { BOLD } else { rest_font })
        .wrapping(text::Wrapping::None);
    button(
        column![
            container(words).height(height).align_y(Alignment::Center),
            container(Space::new())
                .width(Length::Fill)
                .height(2)
                .style(move |_| if chosen {
                    rounded(bar, 1)
                } else {
                    container::Style::default()
                }),
        ]
        .width(Length::Shrink),
    )
    .padding(0)
    .on_press(press)
    .style(move |_, status| button::Style {
        text_color: if chosen || lights(status) { ink } else { rest },
        ..Default::default()
    })
    .into()
}
