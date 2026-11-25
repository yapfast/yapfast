//! What the expression picker's tabs share: the tabs over them, the search
//! field, the rail of sections down the left that the grid jumps by, the
//! grid itself (tiles placed at their own boxes, of which only those near
//! the viewport are built), a section's heading, a favourite's star, the
//! foot that names what's pointed at, and the masonry GIFs are placed in.

use crate::widgets::scroll_view;
use iced::{
    Alignment, Element, Length, Padding, Rectangle, Size,
    widget::{Id, button, column, container, mouse_area, row, text},
};

use super::{Event, Tab};
use crate::{
    app::Message,
    controls::lit_style,
    field::{self, Draft, Field},
    fonts::{BOLD, SEMIBOLD},
    pictures::Icon,
    style::scrollbar,
    sync::MARGIN,
    theme::Theme,
    widgets::Rows,
};

/// The grid's scrollable and the search field, for the app's tasks.
pub const GRID: &str = "picker-grid";
pub const FIELD: &str = "picker-field";
/// The rail's width, and its buttons'.
pub const RAIL: f32 = 48.0;
const RAIL_BUTTON: f32 = 36.0;

/// How far the grid is scrolled and how tall it shows.
#[derive(Clone, Copy, Debug)]
pub struct Scroll {
    pub top: f32,
    pub height: f32,
}

impl Default for Scroll {
    fn default() -> Self {
        Self {
            top: 0.0,
            height: 300.0,
        }
    }
}

/// A tile of the grid, in its box from the grid's corner.
pub struct Tile<T> {
    pub at: Rectangle,
    pub item: T,
}

/// A section the rail jumps to: where its tiles start and end down the
/// grid, what its button shows, and its name, which the foot says while
/// the pointer is on the button.
pub struct Section<S> {
    pub top: f32,
    pub end: f32,
    pub mark: S,
    pub name: String,
}

/// The tiles a tab laid out, its sections, and the grid's height.
pub struct Grid<T, S = ()> {
    pub tiles: Vec<Tile<T>>,
    pub sections: Vec<Section<S>>,
    pub height: f32,
}

impl<T, S> Default for Grid<T, S> {
    fn default() -> Self {
        Self {
            tiles: Vec::new(),
            sections: Vec::new(),
            height: 0.0,
        }
    }
}

impl<T, S> Grid<T, S> {
    pub fn place(&mut self, at: Rectangle, item: T) {
        self.height = self.height.max(at.y + at.height);
        self.tiles.push(Tile { at, item });
    }

    /// The tiles within a screen's margin of what shows, by their place.
    pub fn in_sight(&self, scroll: Scroll) -> impl Iterator<Item = (usize, &Tile<T>)> {
        let (from, to) = (scroll.top - MARGIN, scroll.top + scroll.height + MARGIN);
        self.tiles
            .iter()
            .enumerate()
            .filter(move |(_, tile)| tile.at.y + tile.at.height >= from && tile.at.y <= to)
    }

    /// The section the grid's top is in.
    pub fn current(&self, scroll: Scroll) -> Option<usize> {
        self.sections
            .iter()
            .position(|section| scroll.top >= section.top - 1.0 && scroll.top < section.end - 1.0)
    }

    /// The tiles in sight as the grid's list, each built by `tile`; `build`
    /// tells this layout's tiles from another's, whose state they don't take.
    pub fn rows<'a>(
        &'a self,
        scroll: Scroll,
        build: u64,
        tile: impl Fn(usize, &'a T, Size) -> Element<'a, Message>,
    ) -> Rows<'a, Message> {
        let mut rows = Rows::new(self.height);
        for (index, placed) in self.in_sight(scroll) {
            let key = build.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ index as u64;
            rows.place(key, placed.at, tile(index, &placed.item, placed.at.size()));
        }
        rows
    }
}

/// The tabs over the composer's picker.
pub fn tabs<'a>(theme: &Theme, chosen: Tab) -> Element<'a, Message> {
    let tab = |label: &'static str, tab: Tab| {
        let on = tab == chosen;
        let (rest, lit) = (theme.secondary, theme.text);
        let (active, hover) = (theme.active, theme.hover);
        button(text(label).size(14.0).font(SEMIBOLD))
            .padding([0, 12])
            .height(30)
            .on_press(Message::Picker(Event::Tab(tab)))
            .style(move |_, status| {
                let lights = crate::controls::lights(status);
                button::Style {
                    background: if on {
                        Some(active.into())
                    } else {
                        lights.then(|| hover.into())
                    },
                    text_color: if on || lights { lit } else { rest },
                    border: crate::style::round(6),
                    ..Default::default()
                }
            })
    };
    container(
        row![
            tab("GIFs", Tab::Gifs),
            tab("Emoji", Tab::Emoji),
            tab("Stickers", Tab::Stickers)
        ]
        .spacing(8),
    )
    .padding(Padding::ZERO.top(16).left(16).bottom(12))
    .into()
}

/// The search field, sunken, at the panel's top.
pub fn search<'a>(draft: &'a Draft, placeholder: &str, theme: &'a Theme) -> Element<'a, Message> {
    container(search_field(draft, placeholder, theme))
        .padding(Padding::from(10).bottom(6))
        .width(Length::Fill)
        .into()
}

/// The search field alone, for a header of its own.
pub fn search_field<'a>(
    draft: &'a Draft,
    placeholder: &str,
    theme: &'a Theme,
) -> Element<'a, Message> {
    Field::new(
        draft,
        Id::new(FIELD),
        placeholder.into(),
        |event| Message::Picker(Event::Field(event)),
        field::Colors {
            ground: theme.sunken,
            text: theme.text,
            placeholder: theme.muted,
            selection: theme.token_colour("selection", theme.selection),
        },
    )
    // The field's ring while it has the keys.
    .ring(crate::style::faded(theme.accent, 0.8))
    .single()
    .size(15.0)
    .padding(Padding::from([5, 10]))
    .into()
}

/// The rail: a button per section, its mark in it, the one the grid's top
/// is in lit; a press jumps there, and the pointer on one tells the picker,
/// whose foot names it.
pub fn rail<'a>(
    marks: impl IntoIterator<Item = Element<'a, Message>>,
    current: Option<usize>,
    theme: &'a Theme,
) -> Element<'a, Message> {
    let (wash, active, panel) = (theme.wash(0.12), theme.active, theme.panel);
    let mut rail = column![]
        .spacing(4)
        .padding(Padding::ZERO.top(4))
        .align_x(Alignment::Center)
        .width(RAIL);
    for (index, mark) in marks.into_iter().enumerate() {
        let section = button(container(mark).center(RAIL_BUTTON))
            .padding(0)
            .width(RAIL_BUTTON)
            .height(RAIL_BUTTON)
            .on_press(Message::Picker(Event::Jump(index)))
            .style(lit_style(
                (current == Some(index)).then_some(active),
                wash,
                6.0,
            ));
        rail = rail.push(
            mouse_area(section)
                .on_enter(Message::Picker(Event::RailHover(index)))
                .on_exit(Message::Picker(Event::RailLeft(index))),
        );
    }
    // It scrolls when its sections outgrow it, with no bar.
    container(scroll_view(rail).barless())
        .width(RAIL)
        .height(Length::Fill)
        .style(move |_| crate::style::fill(panel))
        .into()
}

/// A section's heading over its tiles: its mark, if it has one, and its
/// name.
pub fn heading<'a>(
    mark: Option<Element<'a, Message>>,
    name: &str,
    theme: &Theme,
) -> Element<'a, Message> {
    let mut line = row![].spacing(6).align_y(Alignment::Center);
    if let Some(mark) = mark {
        line = line.push(mark);
    }
    line = line.push(
        text(name.to_owned())
            .size(13)
            .font(BOLD)
            .color(theme.secondary)
            .wrapping(text::Wrapping::None),
    );
    container(line)
        .padding(Padding::ZERO.left(10).bottom(2).right(8))
        .height(Length::Fill)
        .width(Length::Fill)
        .clip(true)
        .align_y(Alignment::End)
        .into()
}

/// A small thing at a corner of the tile it's stacked over, `inset` in.
pub fn pinned<'a>(
    child: impl Into<Element<'a, Message>>,
    x: Alignment,
    y: Alignment,
    inset: f32,
) -> Element<'a, Message> {
    container(child)
        .padding(inset)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(x)
        .align_y(y)
        .into()
}

/// The grid's scrollable, reporting where it's scrolled to.
pub fn grid<'a>(rows: Rows<'a, Message>, theme: &Theme) -> Element<'a, Message> {
    scroll_view(rows)
        .id(Id::new(GRID))
        .on_scroll(|viewport| Message::Picker(Event::Scrolled(viewport)))
        .width(Length::Fill)
        .height(Length::Fill)
        .style(scrollbar(theme))
        .into()
}

/// What the grid says in place of tiles.
pub fn note<'a>(words: String, theme: &Theme) -> Element<'a, Message> {
    container(
        text(words)
            .size(15)
            .color(theme.muted)
            .align_x(Alignment::Center),
    )
    .padding(Padding::from([40, 24]))
    .width(Length::Fill)
    .height(Length::Fill)
    .align_x(Alignment::Center)
    .into()
}

/// The foot that names what the pointer is on, or what a click does.
pub fn foot<'a>(
    content: impl Into<Element<'a, Message>>,
    height: f32,
    theme: &Theme,
) -> Element<'a, Message> {
    let panel = theme.panel;
    container(content)
        .padding([0, 14])
        .height(height)
        .width(Length::Fill)
        .align_y(Alignment::Center)
        .clip(true)
        .style(move |_| crate::style::fill(panel))
        .into()
}

/// The foot's words: a name, bold, or a hint, muted.
pub fn foot_words<'a>(words: String, named: bool, theme: &Theme) -> Element<'a, Message> {
    text(words)
        .size(15)
        .font(if named { BOLD } else { crate::fonts::REGULAR })
        .color(if named { theme.text } else { theme.muted })
        .wrapping(text::Wrapping::None)
        .into()
}

/// The panel the picker is drawn on: raised, edged and shadowed.
/// The picker frame with a clear ground when theme art or glass is drawn
/// beneath it by the shared surface renderer.
pub fn frame<'a>(
    content: impl Into<Element<'a, Message>>,
    size: Size,
    theme: &'a Theme,
    transparent: bool,
) -> Element<'a, Message> {
    let raised = if transparent {
        iced::Color::TRANSPARENT
    } else {
        theme.raised
    };
    let edge = theme.edge;
    container(content)
        .width(size.width)
        .height(size.height)
        .clip(true)
        .style(move |_| crate::style::surface_panel(theme, "picker", raised, edge, 10.0))
        .into()
}

/// A favourite's star, `side` across with its `glyph` in it, on the
/// panel's ground: gold while it's a favourite.
pub fn star<'a>(
    look: crate::look::Look<'_>,
    on: bool,
    side: f32,
    glyph: u32,
    message: Message,
) -> Element<'a, Message> {
    let theme = look.theme;
    let (icon, tint) = if on {
        (Icon::StarFilled, crate::style::hex(crate::style::CROWN))
    } else {
        (Icon::Star, theme.text)
    };
    let panel = theme.panel;
    button(container(look.glyph(icon, glyph, tint)).center(side))
        .padding(0)
        .width(side)
        .height(side)
        .on_press(message)
        .style(move |_, _| button::Style {
            background: Some(panel.into()),
            border: crate::style::round(side / 6.0),
            ..Default::default()
        })
        .into()
}

/// Places tiles in the shortest of its columns, leftmost on ties, as
/// Discord's GIF picker does: columns about 200 px wide, 12 px apart and
/// from the edges, at most eight.
pub struct Masonry {
    columns: Vec<f32>,
    pub width: f32,
}

impl Masonry {
    pub const GUTTER: f32 = 12.0;
    const DESIRED: f32 = 200.0;
    const MOST: f32 = 8.0;

    /// The columns of a grid `width` wide, less its gutters at the sides.
    pub fn new(width: f32) -> Self {
        let count = (width / Self::DESIRED).floor().clamp(1.0, Self::MOST);
        Self {
            columns: vec![Self::GUTTER; count as usize],
            width: (width - Self::GUTTER * (count - 1.0)) / count,
        }
    }

    /// Where a tile `height` tall goes: its left and top.
    pub fn place(&mut self, height: f32) -> (f32, f32) {
        let (column, top) = self
            .columns
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| a.total_cmp(b))
            .map(|(column, top)| (column, *top))
            .unwrap_or((0, Self::GUTTER));
        self.columns[column] = top + height + Self::GUTTER;
        (
            Self::GUTTER + column as f32 * (self.width + Self::GUTTER),
            top,
        )
    }

    pub fn bottom(&self) -> f32 {
        self.columns.iter().copied().fold(0.0, f32::max)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masonry_fills_the_shortest_column_first() {
        let mut masonry = Masonry::new(474.0);
        assert_eq!(masonry.columns.len(), 2);
        assert_eq!(masonry.width, 231.0);
        assert_eq!(masonry.place(100.0), (12.0, 12.0));
        assert_eq!(masonry.place(300.0), (255.0, 12.0));
        // The left column is shorter, so it takes the next two.
        assert_eq!(masonry.place(50.0), (12.0, 124.0));
        assert_eq!(masonry.place(50.0), (12.0, 186.0));
        assert_eq!(masonry.bottom(), 324.0);
        assert_eq!(Masonry::new(2400.0).columns.len(), 8);
    }
}
