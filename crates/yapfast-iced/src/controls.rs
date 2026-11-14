//! The controls every screen shares, styled from the theme: the button of
//! dialogs and forms (the Slint client's `ActionButton`), the single-line
//! field (its `TextField`), the style of a button that lights only under
//! the pointer (icon buttons, menu and list rows), a menu's row and panel
//! (its `MenuItem` and `ContextMenu`), the layer that takes a click outside
//! an overlay, and the tooltip. A screen builds with these instead of
//! styling its own.

use iced::{
    Alignment, Border, Color, Element, Length, Shadow, Vector,
    widget::{Id, Space, button, container, mouse_area, opaque, row, text, text_input},
};

use crate::{
    app::{App, Message},
    fonts::{BOLD, MEDIUM},
    pictures::Icon,
    style::{fill, raised_shadow, rounded},
    theme::Theme,
};

/// How dark the window goes under a dialog, as the Slint client dims it.
pub const DIM: Color = Color {
    r: 0.0,
    g: 0.0,
    b: 0.0,
    a: 0.6,
};

/// A menu row's height and its icon's size, as the Slint client's
/// `MenuItem`.
pub const ITEM: f32 = 32.0;
pub const ITEM_ICON: u32 = 16;

/// What an [`action`] button is, as the Slint client's `ActionButton`
/// properties name it.
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

/// A button as the Slint client's `ActionButton`: 36 px tall, 15 px bold,
/// its face and ink by its [`Kind`], lit under the pointer; without a
/// message it's disabled. [`Action::wide`] stretches it across its column
/// and [`Action::ring`] rings it in the accent, as a focused one.
pub fn action<'a>(
    theme: &Theme,
    label: &'a str,
    kind: Kind,
    message: Option<Message>,
) -> Action<'a> {
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
        label,
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
pub struct Action<'a> {
    label: &'a str,
    message: Option<Message>,
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

impl Action<'_> {
    /// As wide as its column, as a button the Slint client lays out in a
    /// column stretches, `height` tall.
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
}

impl<'a> From<Action<'a>> for Element<'a, Message> {
    fn from(action: Action<'a>) -> Self {
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
                            .spacing(6)
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

/// A single-line field as the Slint client's `TextField`: the sunken
/// ground, ringed in the accent while it has the keys. It takes no input
/// until given `on_input`, as a disabled field, and greys its text while
/// not `enabled`.
pub fn field<'a>(
    theme: &Theme,
    id: &'static str,
    placeholder: &'a str,
    value: &'a str,
    secure: bool,
    enabled: bool,
) -> text_input::TextInput<'a, Message> {
    let (ground, ink, hint, faint, accent, selection) = (
        theme.sunken,
        theme.text,
        theme.muted,
        theme.faint,
        theme.accent,
        theme.selection,
    );
    text_input(placeholder, value)
        .id(Id::new(id))
        .secure(secure)
        .size(16)
        .padding([9, 12])
        .style(move |_, status| text_input::Style {
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
        })
}

/// The window's tooltip, as the Slint client's: 32 px tall, 14 px bold on
/// the theme's tooltip ground.
pub fn tip<'a>(theme: &Theme, label: impl text::IntoFragment<'a>) -> Element<'a, Message> {
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
pub fn outside<'a>(message: Message, dim: bool) -> Element<'a, Message> {
    let layer = container(Space::new())
        .width(Length::Fill)
        .height(Length::Fill);
    let layer = if dim {
        layer.style(|_| fill(DIM))
    } else {
        layer
    };
    opaque(mouse_area(layer).on_press(message))
}

/// A menu's panel, as the Slint client's `ContextMenu`: the menu's ground
/// in its outline, round corners, the raised shadow.
pub fn menu_panel(theme: &Theme) -> impl Fn(&iced::Theme) -> container::Style + use<> {
    let (ground, edge) = (theme.menu, theme.menu_line);
    move |_| container::Style {
        background: Some(ground.into()),
        border: Border {
            color: edge,
            width: 1.0,
            radius: 8.0.into(),
        },
        shadow: raised_shadow(),
        ..Default::default()
    }
}

/// A box of one colour: a line between parts of the window, a bar, a dot.
pub fn bar<'a>(
    color: Color,
    width: impl Into<Length>,
    height: impl Into<Length>,
) -> container::Container<'a, Message> {
    container(Space::new())
        .width(width)
        .height(height)
        .style(move |_| fill(color))
}

/// The line between a menu's groups of rows.
pub fn separator<'a>(theme: &Theme) -> Element<'a, Message> {
    container(bar(theme.line, Length::Fill, 1))
        .padding([4, 0])
        .into()
}

/// A row of a menu or of a list of choices, as [`App::menu_item`] draws it.
#[derive(Clone, Copy)]
pub struct Item<'a> {
    pub label: &'a str,
    pub icon: Option<Icon>,
    /// A destructive choice, drawn in the danger colours.
    pub danger: bool,
    /// Room kept for an icon when it has none, so rows line up; a place's
    /// menu keeps none.
    pub slot: bool,
    /// Shown greyed, not to be chosen.
    pub disabled: bool,
}

/// Which light a menu row shows: under the pointer, or only while the menu
/// says (a context menu, whose light the arrow keys move too).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Light {
    Pointer,
    Lit(bool),
}

impl App {
    /// An emoji as a button `side` across, its picture `glyph` across (its
    /// text where there's no picture), on `rest` and `lit` under the
    /// pointer: the menu's quick reactions, the hover bar's.
    pub fn emoji_button(
        &self,
        key: &str,
        glyph: u32,
        side: f32,
        (rest, lit): (Option<Color>, Color),
        press: Message,
    ) -> Element<'_, Message> {
        button(container(self.emoji_face(key, glyph)).center(side))
            .padding(0)
            .width(side)
            .height(side)
            .style(lit_style(rest, lit, 6.0))
            .on_press(press)
            .into()
    }

    /// A menu row as the Slint client's `MenuItem`: 32 px, its icon in a
    /// 16 px slot (kept empty without one, so rows line up), 14 px medium
    /// text; the overlay's wash when lit, or the danger colour with white
    /// ink for a destructive row.
    pub fn menu_item<'a>(
        &self,
        item: Item<'a>,
        light: Light,
        press: Message,
    ) -> Element<'a, Message> {
        let theme = &self.theme;
        let lit = light == Light::Lit(true);
        let ink = |lit: bool| match (item.danger, lit) {
            (true, true) => theme.on_danger,
            (true, false) => theme.danger_text,
            (false, _) => theme.text,
        };
        let icon_ink = match (item.danger, lit) {
            (false, false) => theme.secondary,
            _ => ink(lit),
        };
        let slot = ITEM_ICON as f32;
        let mut face = row![].spacing(8).align_y(Alignment::Center);
        match item.icon {
            Some(icon) => face = face.push(self.glyph(icon, ITEM_ICON, icon_ink)),
            None if item.slot => face = face.push(Space::new().width(slot).height(slot)),
            None => {}
        }
        let face = face.push(text(item.label).size(14).font(MEDIUM));
        let face = container(face)
            .height(Length::Fill)
            .align_y(Alignment::Center);
        let light_fill = if item.danger {
            theme.danger
        } else {
            theme.wash(0.09)
        };
        let inks = [ink(false), ink(true)];
        // Slint's disabled row: the whole of it at .45.
        let inks = if item.disabled {
            inks.map(|ink| crate::style::faded(ink, 0.45))
        } else {
            inks
        };
        button(face)
            .padding([0, 8])
            .width(Length::Fill)
            .height(ITEM)
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
}

/// `child` at `x`, `y` from its layer's corner, for the layers of a stack.
pub fn placed<'a>(child: impl Into<Element<'a, Message>>, x: f32, y: f32) -> Element<'a, Message> {
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

/// A count in a red pill, as the rail's mentions (on a ring of the rail's
/// colour) and a channel's show; at least as wide as it's tall.
pub fn count_pill<'a>(
    theme: &Theme,
    count: &str,
    look: Count,
    ring: Option<Color>,
) -> Element<'a, Message> {
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
