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

/// What a part shows in one set of its states: each value its theme sets,
/// laid over what the widget would draw without it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PartLook {
    pub ground: Option<Background>,
    pub ink: Option<Color>,
    pub border: Option<f32>,
    pub border_colour: Option<Color>,
    pub radius: Option<f32>,
    /// A shadow, or none (`Some(None)`).
    pub shadow: Option<Option<ShapeShadow>>,
    pub opacity: Option<f32>,
    /// Top, right, bottom, left.
    pub padding: Option<[f32; 4]>,
    pub weight: Option<u16>,
    pub size_step: Option<f32>,
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

    /// The part's ground, or `stock`, at its opacity.
    pub fn ground(&self, stock: Option<Background>) -> Option<Background> {
        self.ground
            .or(stock)
            .map(|ground| fade_background(ground, self.alpha()))
    }

    pub fn radius(&self, stock: f32) -> f32 {
        self.radius.unwrap_or(stock)
    }

    /// `stock`'s border with the part's width, colour and corners.
    pub fn border(&self, stock: Border) -> Border {
        Border {
            color: self.fade(self.border_colour.unwrap_or(stock.color)),
            width: self.border.unwrap_or(stock.width),
            radius: self.radius.map_or(stock.radius, Into::into),
        }
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

    /// Its words' face at the part's weight.
    pub fn font(&self, stock: Font) -> Font {
        match self.weight {
            Some(weight) => Font {
                weight: crate::theme::type_weight(i32::from(weight)),
                ..stock
            },
            None => stock,
        }
    }

    /// A container's `stock` style with the part's ground, ink, border,
    /// corners, shadow and opacity laid over it.
    pub fn style(&self, stock: container::Style) -> container::Style {
        container::Style {
            background: self.ground(stock.background),
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
            background: self.ground(stock.background),
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
                .ground(Some(stock.background))
                .unwrap_or(stock.background),
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
            radius: Some(8.0),
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
