//! Named parts' looks, as the widgets lay them over their own: a theme's
//! `parts` layer, resolved once by the app when it loads the theme into one
//! [`PartLook`] per part and set of states ([`PartTable`]), and looked up by
//! the part's name through [`Theme::part`](crate::theme::Theme::part). The
//! names and states are the core's (`yapfast_core::themes::parts`); see
//! docs/themes.md.

use iced::{
    Background, Border, Color, Font, Gradient, Padding, Shadow, Vector,
    widget::{button, container},
};

use crate::theme::ShapeShadow;

/// A set of a part's states, one bit each, as the core numbers them.
pub type States = u8;

pub const HOVER: States = 1;
pub const PRESSED: States = 1 << 1;
pub const SELECTED: States = 1 << 2;
pub const UNREAD: States = 1 << 3;
pub const MUTED: States = 1 << 4;
pub const FOCUSED: States = 1 << 5;

/// The states set where `on` holds: `states(&[(HOVER, pointed), (SELECTED,
/// selected)])`.
pub fn states(on: &[(States, bool)]) -> States {
    on.iter()
        .filter(|(_, on)| *on)
        .fold(0, |set, (state, _)| set | state)
}

/// A part's words' case.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Case {
    Upper,
    Lower,
    None,
}

/// How a part's picture fills it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Fit {
    #[default]
    Cover,
    Contain,
    Tile,
}

/// A part's picture, decoded once when its theme loads.
#[derive(Clone, Debug)]
pub struct Picture {
    pub handle: iced::advanced::image::Handle,
    pub width: u32,
    pub height: u32,
}

/// What a part shows in one set of its states: each value its theme sets,
/// laid over what the widget would draw without it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PartLook {
    pub ground: Option<Background>,
    pub ink: Option<Color>,
    /// Top, right, bottom, left.
    pub border: Option<[f32; 4]>,
    pub border_colour: Option<Color>,
    /// Top left, top right, bottom right, bottom left.
    pub radius: Option<[f32; 4]>,
    /// A shadow, or none (`Some(None)`).
    pub shadow: Option<Option<ShapeShadow>>,
    pub opacity: Option<f32>,
    /// Top, right, bottom, left.
    pub padding: Option<[f32; 4]>,
    pub weight: Option<u16>,
    pub size_step: Option<f32>,
    /// Out of the window, room and all.
    pub hidden: bool,
    pub case: Option<Case>,
    pub italic: Option<bool>,
    /// Its picture: which of the theme's part pictures, and how it fills.
    pub picture: Option<(u16, Fit)>,
    pub icon_ink: Option<Color>,
    /// What styled it: the part's own values, and the states laid over them.
    pub from_base: bool,
    pub from_states: States,
}

impl PartLook {
    fn alpha(&self) -> f32 {
        self.opacity.unwrap_or(1.0)
    }

    /// `colour` at the part's opacity.
    pub fn fade(&self, colour: Color) -> Color {
        Color {
            a: colour.a * self.alpha(),
            ..colour
        }
    }

    /// The part's ink, or `stock`, at its opacity.
    pub fn ink(&self, stock: Color) -> Color {
        self.fade(self.ink.unwrap_or(stock))
    }

    /// Its ground where it has a picture, which [`show`] draws under the
    /// part's own (none) so the picture shows; else [`Self::ground`].
    pub fn under_picture(&self, stock: Option<Background>) -> Option<Background> {
        if self.picture.is_some() {
            None
        } else {
            self.ground(stock)
        }
    }

    /// The part's ground, or `stock`, at its opacity.
    pub fn ground(&self, stock: Option<Background>) -> Option<Background> {
        self.ground
            .or(stock)
            .map(|ground| fade_background(ground, self.alpha()))
    }

    /// One corner for a widget that takes one: its top left.
    pub fn radius(&self, stock: f32) -> f32 {
        self.radius.map_or(stock, |[top_left, ..]| top_left)
    }

    /// Its corners, or `stock`'s.
    pub fn corners(&self, stock: iced::border::Radius) -> iced::border::Radius {
        self.radius
            .map_or(stock, |[top_left, top_right, bottom_right, bottom_left]| {
                iced::border::Radius {
                    top_left,
                    top_right,
                    bottom_right,
                    bottom_left,
                }
            })
    }

    /// Its border's sides when they differ, which [`show`] draws: a quad's
    /// border is one width all round.
    pub fn sides(&self) -> Option<[f32; 4]> {
        self.border
            .filter(|sides| sides.iter().any(|side| *side != sides[0]))
    }

    /// `stock`'s border with the part's width, colour and corners; sides
    /// that differ are drawn apart ([`show`]), so the quad's is none.
    pub fn border(&self, stock: Border) -> Border {
        Border {
            color: self.fade(self.border_colour.unwrap_or(stock.color)),
            width: match self.border {
                Some(_) if self.sides().is_some() => 0.0,
                Some([width, ..]) => width,
                None => stock.width,
            },
            radius: self.corners(stock.radius),
        }
    }

    /// Its icons' ink: its own, else its ink, else `stock`, faded.
    pub fn icon_ink(&self, stock: Color) -> Color {
        self.fade(self.icon_ink.or(self.ink).unwrap_or(stock))
    }

    pub fn shadow(&self, stock: Shadow) -> Shadow {
        let shadow = match self.shadow {
            None => stock,
            Some(None) => Shadow::default(),
            Some(Some(shadow)) => Shadow {
                color: shadow.colour,
                offset: Vector::new(shadow.x, shadow.y),
                blur_radius: shadow.blur,
            },
        };
        Shadow {
            color: self.fade(shadow.color),
            ..shadow
        }
    }

    pub fn padding(&self, stock: Padding) -> Padding {
        self.padding
            .map_or(stock, |[top, right, bottom, left]| Padding {
                top,
                right,
                bottom,
                left,
            })
    }

    /// Its words' size: `stock` and the part's step.
    pub fn size(&self, stock: f32) -> f32 {
        (stock + self.size_step.unwrap_or(0.0)).max(1.0)
    }

    /// Its words' face at the part's weight and slant.
    pub fn font(&self, stock: Font) -> Font {
        let mut font = stock;
        if let Some(weight) = self.weight {
            font.weight = crate::theme::type_weight(i32::from(weight));
        }
        match self.italic {
            Some(true) => font.style = iced::font::Style::Italic,
            Some(false) => font.style = iced::font::Style::Normal,
            None => {}
        }
        font
    }

    /// `stock` words in the part's case.
    pub fn words<'w>(
        &self,
        stock: impl Into<std::borrow::Cow<'w, str>>,
    ) -> std::borrow::Cow<'w, str> {
        let stock = stock.into();
        match self.case {
            Some(Case::Upper) => std::borrow::Cow::Owned(stock.to_uppercase()),
            Some(Case::Lower) => std::borrow::Cow::Owned(stock.to_lowercase()),
            Some(Case::None) | None => stock,
        }
    }

    /// A container's `stock` style with the part's ground, ink, border,
    /// corners, shadow and opacity laid over it.
    pub fn style(&self, stock: container::Style) -> container::Style {
        container::Style {
            background: self.under_picture(stock.background),
            text_color: match (self.ink, stock.text_color) {
                (Some(ink), _) => Some(self.fade(ink)),
                (None, Some(stock)) => Some(self.fade(stock)),
                (None, None) => None,
            },
            border: self.border(stock.border),
            shadow: self.shadow(stock.shadow),
            ..stock
        }
    }
}

impl PartLook {
    /// A button's `stock` style with the part's look laid over it.
    pub fn button(&self, stock: button::Style) -> button::Style {
        button::Style {
            background: self.under_picture(stock.background),
            text_color: self.ink(stock.text_color),
            border: self.border(stock.border),
            shadow: self.shadow(stock.shadow),
            ..stock
        }
    }
}

/// [`PartLook::button`] where `look` is there.
pub fn button_style(look: Option<&PartLook>, stock: button::Style) -> button::Style {
    match look {
        Some(look) => look.button(stock),
        None => stock,
    }
}

/// A button's part's looks at rest, under the pointer and pressed, looked
/// up once as the button is made, for its style to pick by its status.
#[derive(Clone, Copy, Debug, Default)]
pub struct ByStatus {
    rest: Option<PartLook>,
    hover: Option<PartLook>,
    pressed: Option<PartLook>,
}

impl ByStatus {
    /// `name`'s looks in `on` (such as selected) and each status.
    pub fn new(theme: &crate::theme::Theme, name: &str, on: States) -> Self {
        let rest = theme.part(name, on);
        if rest.is_none() {
            return Self::default();
        }
        Self {
            rest,
            hover: theme.part(name, on | HOVER),
            pressed: theme.part(name, on | HOVER | PRESSED),
        }
    }

    pub fn get(&self, status: button::Status) -> Option<&PartLook> {
        match status {
            button::Status::Hovered => self.hover.as_ref(),
            button::Status::Pressed => self.pressed.as_ref(),
            button::Status::Active | button::Status::Disabled => self.rest.as_ref(),
        }
    }

    /// The look at rest.
    pub fn rest(&self) -> Option<&PartLook> {
        self.rest.as_ref()
    }

    /// `stock` with the look for `status` laid over it.
    pub fn style(&self, status: button::Status, stock: button::Style) -> button::Style {
        button_style(self.get(status), stock)
    }
}

/// A text field's `stock` style with the part's ground, border and
/// opacity laid over it; its words keep theirs.
pub fn field_style(
    look: Option<&PartLook>,
    stock: iced::widget::text_input::Style,
) -> iced::widget::text_input::Style {
    match look {
        Some(look) => iced::widget::text_input::Style {
            background: look
                .under_picture(Some(stock.background))
                .unwrap_or(Background::Color(Color::TRANSPARENT)),
            border: look.border(stock.border),
            ..stock
        },
        None => stock,
    }
}

/// [`PartLook::style`] where `look` is there, `stock` where it isn't.
pub fn style(look: Option<&PartLook>, stock: container::Style) -> container::Style {
    match look {
        Some(look) => look.style(stock),
        None => stock,
    }
}

/// [`PartLook::ink`] where `look` is there.
pub fn ink(look: Option<&PartLook>, stock: Color) -> Color {
    look.map_or(stock, |look| look.ink(stock))
}

/// [`PartLook::radius`] where `look` is there.
pub fn radius(look: Option<&PartLook>, stock: f32) -> f32 {
    look.map_or(stock, |look| look.radius(stock))
}

/// [`PartLook::words`] where `look` is there.
pub fn words<'w>(
    look: Option<&PartLook>,
    stock: impl Into<std::borrow::Cow<'w, str>>,
) -> std::borrow::Cow<'w, str> {
    match look {
        Some(look) => look.words(stock),
        None => stock.into(),
    }
}

/// [`PartLook::icon_ink`] where `look` is there.
pub fn icon_ink(look: Option<&PartLook>, stock: Color) -> Color {
    look.map_or(stock, |look| look.icon_ink(stock))
}

/// [`PartLook::size`] where `look` is there.
pub fn size(look: Option<&PartLook>, stock: f32) -> f32 {
    look.map_or(stock, |look| look.size(stock))
}

/// [`PartLook::font`] where `look` is there.
pub fn font(look: Option<&PartLook>, stock: Font) -> Font {
    look.map_or(stock, |look| look.font(stock))
}

/// [`PartLook::padding`] where `look` is there.
pub fn padding(look: Option<&PartLook>, stock: impl Into<Padding>) -> Padding {
    let stock = stock.into();
    look.map_or(stock, |look| look.padding(stock))
}

fn fade_background(background: Background, alpha: f32) -> Background {
    if alpha >= 1.0 {
        return background;
    }
    match background {
        Background::Color(colour) => Background::Color(Color {
            a: colour.a * alpha,
            ..colour
        }),
        Background::Gradient(Gradient::Linear(mut linear)) => {
            for stop in linear.stops.iter_mut().flatten() {
                stop.color.a *= alpha;
            }
            Background::Gradient(Gradient::Linear(linear))
        }
    }
}

/// What a part's element needs to show as the theme says: taken out when
/// it's hidden, and its picture and uneven border sides drawn round it.
/// Made where the element is, and as cheap as a look-up when the part has
/// neither.
#[derive(Clone, Debug)]
pub struct Dress {
    pub name: &'static str,
    pub states: States,
    pub look: Option<PartLook>,
    /// Its look under the pointer, for a part whose hover is the pointer's
    /// (a button), as the inspector labels it.
    pub pointed: Option<PartLook>,
    picture: Option<(Picture, Fit)>,
}

impl Dress {
    /// The part `name` in `states`, as `theme` styles it.
    pub fn new(theme: &crate::theme::Theme, name: &'static str, states: States) -> Self {
        Self::looked(theme, name, states, theme.part(name, states), None)
    }

    /// [`Dress::new`] for a part whose hover comes from the pointer.
    pub fn pointed(theme: &crate::theme::Theme, name: &'static str, states: States) -> Self {
        let pointed = theme.part(name, states | HOVER);
        Self::looked(theme, name, states, theme.part(name, states), pointed)
    }

    /// With the looks already looked up.
    pub fn looked(
        theme: &crate::theme::Theme,
        name: &'static str,
        states: States,
        look: Option<PartLook>,
        pointed: Option<PartLook>,
    ) -> Self {
        let picture = look.and_then(|look| {
            let (index, fit) = look.picture?;
            Some((theme.part_picture(index)?, fit))
        });
        Self {
            name,
            states,
            look,
            pointed,
            picture,
        }
    }

    /// `content` as the theme shows the part: nothing at all while it's
    /// hidden; with its picture under it and its uneven sides over it.
    pub fn wrap<'a, M: 'a>(self, content: impl Into<iced::Element<'a, M>>) -> iced::Element<'a, M> {
        let Some(look) = self.look else {
            return content.into();
        };
        if look.hidden {
            return iced::widget::Space::new().width(0).height(0).into();
        }
        if self.picture.is_none() && look.sides().is_none() {
            return content.into();
        }
        crate::widgets::Wrap::new(
            content,
            Decor {
                look,
                picture: self.picture,
            },
        )
        .into()
    }
}

/// `content` as the theme shows the part `name` in `states`: see [`Dress`].
pub fn show<'a, M: 'a>(
    theme: &crate::theme::Theme,
    name: &'static str,
    states: States,
    content: impl Into<iced::Element<'a, M>>,
) -> iced::Element<'a, M> {
    Dress::new(theme, name, states).wrap(content)
}

/// [`show`] for a part whose hover comes from the pointer (a button).
pub fn show_pointed<'a, M: 'a>(
    theme: &crate::theme::Theme,
    name: &'static str,
    states: States,
    content: impl Into<iced::Element<'a, M>>,
) -> iced::Element<'a, M> {
    Dress::pointed(theme, name, states).wrap(content)
}

/// The most tiles a tiled picture draws in a part.
const MOST_TILES: usize = 256;

struct Decor {
    look: PartLook,
    picture: Option<(Picture, Fit)>,
}

impl<M> crate::widgets::Hooks<M> for Decor {
    type State = crate::widgets::Stateless;

    fn draw(
        &self,
        _: &Self::State,
        renderer: &mut iced::Renderer,
        at: crate::widgets::At,
        child: &mut dyn FnMut(&mut iced::Renderer),
    ) {
        use iced::advanced::Renderer as _;
        let bounds = at.bounds;
        let corners = self.look.corners(0.0.into());
        if let Some((picture, fit)) = &self.picture {
            if let Some(ground) = self.look.ground(None) {
                renderer.fill_quad(
                    iced::advanced::renderer::Quad {
                        bounds,
                        border: Border {
                            radius: corners,
                            ..Border::default()
                        },
                        ..Default::default()
                    },
                    ground,
                );
            }
            let (width, height) = (picture.width.max(1) as f32, picture.height.max(1) as f32);
            let opacity = self.look.alpha();
            let radius = [
                corners.top_left,
                corners.top_right,
                corners.bottom_right,
                corners.bottom_left,
            ];
            let mut draw = |place: iced::Rectangle, radius: [f32; 4]| {
                use iced::advanced::image::Renderer as _;
                let [top_left, top_right, bottom_right, bottom_left] = radius;
                renderer.draw_image(
                    iced::advanced::image::Image {
                        handle: picture.handle.clone(),
                        filter_method: iced::advanced::image::FilterMethod::Linear,
                        rotation: iced::Radians(0.0),
                        border_radius: iced::border::Radius {
                            top_left,
                            top_right,
                            bottom_right,
                            bottom_left,
                        },
                        opacity,
                        snap: true,
                    },
                    place,
                    bounds,
                );
            };
            match fit {
                Fit::Cover | Fit::Contain => {
                    let scale = if *fit == Fit::Cover {
                        (bounds.width / width).max(bounds.height / height)
                    } else {
                        (bounds.width / width).min(bounds.height / height)
                    };
                    let size = iced::Size::new(width * scale, height * scale);
                    let place = iced::Rectangle::new(
                        iced::Point::new(
                            bounds.x + (bounds.width - size.width) / 2.0,
                            bounds.y + (bounds.height - size.height) / 2.0,
                        ),
                        size,
                    );
                    // A picture cut by the part's edge takes its corners
                    // only where it fits it.
                    let fits = (place.width - bounds.width).abs() < 0.5
                        && (place.height - bounds.height).abs() < 0.5;
                    draw(place, if fits { radius } else { [0.0; 4] });
                }
                Fit::Tile => {
                    let across = (bounds.width / width).ceil() as usize;
                    let down = (bounds.height / height).ceil() as usize;
                    if across * down <= MOST_TILES {
                        for row in 0..down {
                            for column in 0..across {
                                draw(
                                    iced::Rectangle::new(
                                        iced::Point::new(
                                            bounds.x + column as f32 * width,
                                            bounds.y + row as f32 * height,
                                        ),
                                        iced::Size::new(width, height),
                                    ),
                                    [0.0; 4],
                                );
                            }
                        }
                    }
                }
            }
        }
        child(renderer);
        if let Some([top, right, bottom, left]) = self.look.sides() {
            let colour = self.look.fade(
                self.look
                    .border_colour
                    .or(self.look.ink)
                    .unwrap_or(Color::WHITE),
            );
            for side in [
                iced::Rectangle::new(bounds.position(), iced::Size::new(bounds.width, top)),
                iced::Rectangle::new(
                    iced::Point::new(bounds.x + bounds.width - right, bounds.y),
                    iced::Size::new(right, bounds.height),
                ),
                iced::Rectangle::new(
                    iced::Point::new(bounds.x, bounds.y + bounds.height - bottom),
                    iced::Size::new(bounds.width, bottom),
                ),
                iced::Rectangle::new(bounds.position(), iced::Size::new(left, bounds.height)),
            ] {
                if side.width > 0.0 && side.height > 0.0 {
                    renderer.fill_quad(
                        iced::advanced::renderer::Quad {
                            bounds: side,
                            ..Default::default()
                        },
                        colour,
                    );
                }
            }
        }
    }
}

/// One part's looks in every set of the states it takes, worked out once
/// when the theme loads.
#[derive(Clone, Debug, Default)]
pub struct PartTable {
    /// The states the part takes.
    mask: States,
    /// By the states that are on among `mask`'s, packed low bit first.
    looks: Box<[PartLook]>,
}

impl PartTable {
    /// A table of the looks `look` gives each set of `mask`'s states.
    pub fn new(mask: States, look: impl Fn(States) -> PartLook) -> Self {
        let bits: Vec<States> = (0..8)
            .map(|bit| 1 << bit)
            .filter(|bit| mask & bit != 0)
            .collect();
        let looks = (0..1u32 << bits.len())
            .map(|packed| {
                let states = bits
                    .iter()
                    .enumerate()
                    .filter(|(index, _)| packed & (1 << index) != 0)
                    .fold(0, |set, (_, bit)| set | bit);
                look(states)
            })
            .collect();
        Self { mask, looks }
    }

    /// The look in `states`; states the part doesn't take are left out.
    pub fn get(&self, states: States) -> &PartLook {
        let mut packed = 0usize;
        let mut slot = 0;
        for bit in 0..8 {
            let state = 1 << bit;
            if self.mask & state != 0 {
                if states & state != 0 {
                    packed |= 1 << slot;
                }
                slot += 1;
            }
        }
        &self.looks[packed]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_look_cases_slants_and_tints_and_splits_uneven_sides() {
        let look = PartLook {
            case: Some(Case::Upper),
            italic: Some(true),
            ink: Some(Color::WHITE),
            border: Some([0.0, 0.0, 2.0, 0.0]),
            radius: Some([8.0, 8.0, 0.0, 0.0]),
            ..PartLook::default()
        };
        assert_eq!(look.words("general"), "GENERAL");
        assert_eq!(look.font(Font::DEFAULT).style, iced::font::Style::Italic);
        assert_eq!(
            look.icon_ink(Color::BLACK),
            Color::WHITE,
            "icons follow its ink"
        );
        assert_eq!(look.sides(), Some([0.0, 0.0, 2.0, 0.0]));
        let border = look.border(Border::default());
        assert_eq!(border.width, 0.0, "uneven sides are drawn apart");
        assert_eq!(border.radius.top_left, 8.0);
        assert_eq!(border.radius.bottom_left, 0.0);
        let even = PartLook {
            border: Some([2.0; 4]),
            ..PartLook::default()
        };
        assert_eq!(even.sides(), None);
        assert_eq!(even.border(Border::default()).width, 2.0);
        let none = PartLook {
            case: Some(Case::None),
            ..PartLook::default()
        };
        assert_eq!(none.words("General"), "General");
    }

    #[test]
    fn a_hidden_part_takes_no_room() {
        let theme = crate::theme::Theme::new("parts-hidden", true).worked_out();
        let mut metrics = crate::theme::Metrics::default();
        metrics.parts.insert(
            "menu.row",
            PartTable::new(SELECTED, |states| PartLook {
                hidden: states & SELECTED != 0,
                ..PartLook::default()
            }),
        );
        let theme = theme.with_metrics(std::sync::Arc::new(metrics));
        let content = || iced::widget::Space::new().width(40).height(20);
        let hidden: iced::Element<'_, ()> = show(&theme, "menu.row", SELECTED, content());
        let shown: iced::Element<'_, ()> = show(&theme, "menu.row", 0, content());
        assert_eq!(hidden.as_widget().size().width, iced::Length::Fixed(0.0));
        assert_eq!(shown.as_widget().size().width, iced::Length::Fixed(40.0));
    }

    #[test]
    fn a_table_finds_each_set_of_its_states_and_ignores_others() {
        let table = PartTable::new(HOVER | SELECTED | MUTED, |states| PartLook {
            weight: Some(u16::from(states)),
            ..PartLook::default()
        });
        for states in [0, HOVER, SELECTED, MUTED, HOVER | SELECTED | MUTED] {
            assert_eq!(table.get(states).weight, Some(u16::from(states)));
        }
        // Pressed isn't one of its states: the look without it.
        assert_eq!(table.get(HOVER | PRESSED).weight, Some(u16::from(HOVER)));
    }

    #[test]
    fn a_look_lays_only_what_it_sets_and_fades_by_its_opacity() {
        let stock = container::Style {
            background: Some(Color::BLACK.into()),
            text_color: Some(Color::WHITE),
            border: Border {
                color: Color::WHITE,
                width: 1.0,
                radius: 3.0.into(),
            },
            ..container::Style::default()
        };
        assert_eq!(PartLook::default().style(stock), stock);
        let look = PartLook {
            radius: Some([8.0; 4]),
            opacity: Some(0.5),
            shadow: Some(None),
            ..PartLook::default()
        };
        let style = look.style(stock);
        assert_eq!(style.border.width, 1.0);
        assert_eq!(style.border.radius, 8.0.into());
        assert_eq!(style.text_color.unwrap().a, 0.5);
        assert_eq!(
            style.background,
            Some(
                Color {
                    a: 0.5,
                    ..Color::BLACK
                }
                .into()
            )
        );
        assert_eq!(look.size(14.0), 14.0);
        let bigger = PartLook {
            size_step: Some(2.0),
            weight: Some(700),
            ..PartLook::default()
        };
        assert_eq!(bigger.size(14.0), 16.0);
        assert_eq!(bigger.font(Font::DEFAULT).weight, iced::font::Weight::Bold);
    }
}
