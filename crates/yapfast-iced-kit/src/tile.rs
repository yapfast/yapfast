//! A video tile, the one piece the call's stage, its large tile full
//! screen, the picture in picture and a DM's call are built of, as the
//! Slint client's `StageTile`, `TileChip`, `ViewerStack` and `StreamPip`:
//! what fills it (a frame, someone's picture, a note), the speaking ring
//! over it, a name chip at its foot, things at its top corners, and
//! controls that show while the pointer is on it.

use iced::{
    Alignment, Border, Color, Element, Length, Point, Size,
    widget::{Space, button, container, mouse_area, pin, row, stack, text, tooltip},
};

use crate::look::Look;
use crate::{
    controls::{self, lights},
    fonts::BOLD,
    icons::Icon,
    style::rounded,
    widgets::{Above, Hover},
};

/// The dark glass of a tile's chip and buttons over video (`#000000a6`).
pub const GLASS: Color = Color {
    r: 0.0,
    g: 0.0,
    b: 0.0,
    a: 0xa6 as f32 / 255.0,
};
/// The foot of the shade under a corner picture's title.
pub const SHADE: Color = Color {
    r: 0.0,
    g: 0.0,
    b: 0.0,
    a: 0xb0 as f32 / 255.0,
};
/// What a tile says over the black of a stream not yet showing, in every
/// theme: the stage is black in all of them.
pub const NOTE: Color = Color::from_rgb8(0xb5, 0xb8, 0xc1);
/// A tile's corners, and how far its parts sit in from its edges.
pub const RADIUS: f32 = 12.0;
const INSET: f32 = 10.0;
/// The speaking ring's width.
const RING: f32 = 3.0;
/// A chip's height, and a corner button's side.
const CHIP: f32 = 26.0;
const BUTTON: f32 = 32.0;

/// A tile being built.
pub struct Tile<'a, M> {
    size: Size,
    radius: f32,
    ground: Option<Color>,
    content: Element<'a, M>,
    ring: Option<Color>,
    chip: Option<Element<'a, M>>,
    top_left: Option<Element<'a, M>>,
    top_right: Vec<Element<'a, M>>,
    bottom_right: Option<Element<'a, M>>,
    hovered: Vec<Element<'a, M>>,
    hover_kept: bool,
    on_point: Option<(M, M)>,
    on_press: Option<M>,
    on_double: Option<M>,
    on_right: Option<Box<dyn Fn(Point) -> M + 'a>>,
}

impl<'a, M: Clone + 'a> Tile<'a, M> {
    /// A tile of `size` that `content` fills, on `ground` (none where the
    /// content paints all of it, as a frame does).
    pub fn new(size: Size, content: impl Into<Element<'a, M>>, ground: Option<Color>) -> Self {
        Self {
            size,
            radius: RADIUS,
            ground,
            content: content.into(),
            ring: None,
            chip: None,
            top_left: None,
            top_right: Vec::new(),
            bottom_right: None,
            hovered: Vec::new(),
            hover_kept: false,
            on_point: None,
            on_press: None,
            on_double: None,
            on_right: None,
        }
    }

    /// Square corners, as the large tile full screen has.
    pub fn square(mut self) -> Self {
        self.radius = 0.0;
        self
    }

    /// The ring of someone speaking, in `color`.
    pub fn ring(mut self, color: Option<Color>) -> Self {
        self.ring = color;
        self
    }

    /// The chip at its bottom left: their name and marks.
    pub fn chip(mut self, chip: Element<'a, M>) -> Self {
        self.chip = Some(chip);
        self
    }

    pub fn top_left(mut self, badge: Element<'a, M>) -> Self {
        self.top_left = Some(badge);
        self
    }

    /// Something at its top right, after those before it.
    pub fn top_right(mut self, corner: Element<'a, M>) -> Self {
        self.top_right.push(corner);
        self
    }

    /// Something at its bottom right, as the large tile's full screen.
    pub fn bottom_right(mut self, corner: Element<'a, M>) -> Self {
        self.bottom_right = Some(corner);
        self
    }

    /// What it says as the pointer comes onto it and leaves, where what
    /// shows over it moves with the pointer (a stage tile's stop shifts its
    /// viewers aside).
    pub fn on_point(mut self, enter: M, exit: M) -> Self {
        self.on_point = Some((enter, exit));
        self
    }

    /// What shows over it while the pointer is on it, filling it, or
    /// always while `kept`.
    pub fn hovered(mut self, over: Element<'a, M>, kept: bool) -> Self {
        self.hovered.push(over);
        self.hover_kept |= kept;
        self
    }

    pub fn on_press(mut self, message: M) -> Self {
        self.on_press = Some(message);
        self
    }

    pub fn on_double(mut self, message: M) -> Self {
        self.on_double = Some(message);
        self
    }

    /// A right click on it, where in the window.
    pub fn on_right(mut self, message: impl Fn(Point) -> M + 'a) -> Self {
        self.on_right = Some(Box::new(message));
        self
    }
}

impl<'a, M: Clone + 'a> From<Tile<'a, M>> for Element<'a, M> {
    fn from(tile: Tile<'a, M>) -> Self {
        let Size { width, height } = tile.size;
        let radius = tile.radius;
        let ground = tile.ground;
        let mut over = stack![].width(width).height(height);
        if let Some(ring) = tile.ring {
            over = over.push(container(Space::new()).width(width).height(height).style(
                move |_| container::Style {
                    border: Border {
                        color: ring,
                        width: RING,
                        radius: radius.into(),
                    },
                    ..Default::default()
                },
            ));
        }
        if let Some(chip) = tile.chip {
            over = over.push(
                pin(container(chip).max_width((width - 2.0 * INSET).max(0.0)))
                    .x(INSET)
                    .y(height - CHIP - INSET),
            );
        }
        if let Some(badge) = tile.top_left {
            over = over.push(pin(badge).x(INSET).y(INSET));
        }
        if !tile.top_right.is_empty() {
            over = over.push(
                container(row(tile.top_right).spacing(8).align_y(Alignment::Center))
                    .padding(INSET)
                    .align_right(width),
            );
        }
        if let Some(corner) = tile.bottom_right {
            over = over.push(
                container(corner)
                    .padding(INSET)
                    .width(width)
                    .height(height)
                    .align_x(Alignment::End)
                    .align_y(Alignment::End),
            );
        }
        let face = container(stack![tile.content, Above::new(over, 0.0)])
            .width(width)
            .height(height)
            .clip(true)
            .style(move |_| container::Style {
                background: ground.map(Into::into),
                border: Border::default().rounded(radius),
                ..Default::default()
            });
        let mut area = mouse_area(face);
        if let Some(press) = tile.on_press {
            area = area.on_press(press);
        }
        if let Some(double) = tile.on_double {
            area = area.on_double_click(double);
        }
        if let Some((enter, exit)) = tile.on_point {
            area = area.on_enter(enter).on_exit(exit);
        }
        let area: Element<'a, M> = match tile.on_right {
            Some(right) => crate::widgets::RightPress::new(area, right).into(),
            None => area.into(),
        };
        if tile.hovered.is_empty() {
            return area;
        }
        let over = stack(tile.hovered).width(width).height(height);
        Hover::new(area, over).kept(tile.hover_kept).into()
    }
}

/// A tile's name at its foot, as the Slint client's `TileChip`: marks
/// before it, then the name in white, LIVE while it's a stream, and a note
/// of what may be wrong in the accent.
pub fn chip<'a, M: Clone + 'a>(
    look: Look<'a>,
    marks: Vec<(Icon, Color)>,
    name: &'a str,
    live: bool,
    note: Option<&'a str>,
) -> Element<'a, M> {
    let theme = look.theme;
    let mut face = row![].spacing(6).align_y(Alignment::Center);
    for (icon, ink) in marks {
        face = face.push(look.glyph(icon, 14, ink));
    }
    face = face.push(
        text(name)
            .size(14)
            .font(BOLD)
            .color(Color::WHITE)
            .wrapping(text::Wrapping::None),
    );
    if live {
        face = face.push(controls::live_pill(theme, None));
    }
    if let Some(note) = note {
        face = face.push(
            text(note)
                .size(13)
                .color(theme.accent)
                .wrapping(text::Wrapping::None),
        );
    }
    container(face)
        .padding([0, 8])
        .height(CHIP)
        .align_y(Alignment::Center)
        .clip(true)
        .style(|_| rounded(GLASS, 6))
        .into()
}

/// A round button over video, as the tiles' and the picture in picture's
/// `IconButton`s: 32 px of glass, its icon in white, its label in a
/// tooltip.
pub fn glass_button<'a, M: Clone + 'a>(
    look: Look<'a>,
    icon: Icon,
    label: &'static str,
    message: M,
) -> Element<'a, M> {
    let theme = look.theme;
    let lit = crate::theme::mix(GLASS, theme.overlay, 0.9);
    let face = button(look.glyph(icon, 20, Color::WHITE).boxed(BUTTON))
        .padding(0)
        .width(BUTTON)
        .height(BUTTON)
        .on_press(message)
        .style(move |_, status| button::Style {
            background: Some(if lights(status) { lit } else { GLASS }.into()),
            border: Border::default().rounded(BUTTON / 2.0),
            ..Default::default()
        });
    tooltip(face, controls::tip(theme, label), tooltip::Position::Top)
        .gap(6)
        .into()
}

/// A pill on a tile, as "Watch stream" (in the danger colour) and
/// "Starting camera..." (on glass).
pub fn pill<'a, M: Clone + 'a>(label: &'a str, ground: Color, ink: Color) -> Element<'a, M> {
    container(text(label).size(13).font(BOLD).color(ink))
        .padding([0, 10])
        .height(26)
        .align_y(Alignment::Center)
        .style(move |_| rounded(ground, 13))
        .into()
}

/// A note in the middle of a black tile, under an icon, as a stream not yet
/// showing says what it waits for.
pub fn note<'a, M: Clone + 'a>(
    look: Look<'a>,
    icon: Option<Icon>,
    words: impl text::IntoFragment<'a>,
) -> Element<'a, M> {
    let mut column = iced::widget::column![]
        .spacing(10)
        .align_x(Alignment::Center);
    if let Some(icon) = icon {
        column = column.push(look.glyph(icon, 30, NOTE));
    }
    column = column.push(text(words).size(14).color(NOTE).align_x(Alignment::Center));
    container(column)
        .padding([0, 16])
        .center(Length::Fill)
        .into()
}
