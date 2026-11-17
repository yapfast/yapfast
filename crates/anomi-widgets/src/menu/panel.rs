//! A menu's or a submenu's card: `MenuRows` (`ui/controls.slint:1287`) in
//! the `ContextMenu`'s panels (`:1446`, `:1497`). Rows 2 px apart inside
//! 6 px of padding, a group under a 1 px line in 9 px of room; the card in
//! the menu's colour with its outline, 8 px corners and a deep shadow, as
//! wide as its widest row within 255 and 320 px. Taller than the window
//! allows, it scrolls, with the wheel or as the keys move.

use iced::advanced::renderer::{Quad, Renderer as _};
use iced::border::Radius;
use iced::{Border, Color, Point, Rectangle, Shadow, Size, Vector};

use anomi_theme::{Renderer, Theme};
use anomi_vm::menu::Row;

use super::row::{self, Paint, Shape};
use super::text::faded;

/// Inside the card, and between rows (`:1300`).
pub(crate) const PADDING: f32 = 6.0;
const SPACING: f32 = 2.0;
/// The room a group's line takes, and the line (`:1305`, `:1306`).
const SEPARATOR: f32 = 9.0;
const LINE: f32 = 1.0;
/// A menu's and a submenu's width bounds (`narrowest`, `widest`; `:1355`).
pub const NARROWEST: f32 = 255.0;
pub(crate) const WIDEST: f32 = 320.0;
/// The card's corners, outline and shadow (`:1449`, `:1452`).
const RADIUS: f32 = 8.0;
const OUTLINE: f32 = 1.0;
const SHADOW: Color = Color::from_rgba(0.0, 0.0, 0.0, 0.5);
const SHADOW_BLUR: f32 = 24.0;
const SHADOW_DROP: f32 = 8.0;
/// A wheel's line, as iced's scrollable takes one.
const WHEEL_LINE: f32 = 60.0;

/// Where a card's rows are, in the card's own space: the content from the
/// card's top, before scrolling.
#[derive(Clone, Debug, Default)]
pub(crate) struct Panel {
    /// The card in the window.
    pub bounds: Rectangle,
    rows: Vec<Line>,
    /// Each group's line, from the content's top.
    lines: Vec<f32>,
    /// The content's height, padding included.
    content: f32,
    /// How far the content is scrolled up.
    pub scroll: f32,
}

/// A row's place in its card: its top, its height, and its subtitle's height
/// and icon as measured.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Line {
    top: f32,
    height: f32,
    shape: Shape,
}

impl Panel {
    /// Lays out `rows` as wide as they want within `narrowest` and
    /// [`WIDEST`], as high as they are until [`Panel::fit`]. The card is
    /// placed later.
    pub fn new<M>(rows: &[Row<M>], narrowest: f32) -> Self {
        let wanted = rows
            .iter()
            .map(|row| row::width(row, !row.submenu.is_empty()))
            .fold(0.0, f32::max);
        let width = (wanted + 2.0 * PADDING).min(WIDEST).max(narrowest);
        let inner = width - 2.0 * PADDING;
        let mut panel = Self {
            rows: Vec::with_capacity(rows.len()),
            ..Self::default()
        };
        let mut y = PADDING;
        for (index, row) in rows.iter().enumerate() {
            if index > 0 {
                y += SPACING;
                if row.separated {
                    panel.lines.push(y + (SEPARATOR - LINE) / 2.0);
                    y += SEPARATOR + SPACING;
                }
            }
            let below = row::below(row, !row.submenu.is_empty(), inner);
            let height = row::height(row, below);
            panel.rows.push(Line {
                top: y,
                height,
                shape: Shape {
                    below,
                    icon: row::icon(row),
                },
            });
            y += height;
        }
        panel.content = y + PADDING;
        panel.bounds = Rectangle::new(Point::ORIGIN, Size::new(width, panel.content));
        panel
    }

    /// Keeps the scrolling of the panel it replaces, as far as it still can
    /// once fitted.
    pub fn keep(mut self, old: &Self) -> Self {
        self.scroll = old.scroll;
        self
    }

    /// At most `tallest` high, scrolling the rest.
    pub fn fit(&mut self, tallest: f32) {
        self.bounds.height = self.content.min(tallest);
        self.scroll = self.scroll.min(self.most());
    }

    fn most(&self) -> f32 {
        (self.content - self.bounds.height).max(0.0)
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    fn line(&self, index: usize) -> Line {
        self.rows.get(index).copied().unwrap_or_default()
    }

    /// A row's top, from the card's top, as it's scrolled now.
    pub fn shown_top(&self, index: usize) -> f32 {
        self.line(index).top - self.scroll
    }

    /// A row's rectangle in the window.
    pub fn row(&self, index: usize) -> Rectangle {
        Rectangle::new(
            Point::new(
                self.bounds.x + PADDING,
                self.bounds.y + self.shown_top(index),
            ),
            Size::new(self.bounds.width - 2.0 * PADDING, self.line(index).height),
        )
    }

    /// The row under a point, if any.
    pub fn hit(&self, point: Point) -> Option<usize> {
        if !self.bounds.contains(point) {
            return None;
        }
        (0..self.len()).find(|&index| self.row(index).contains(point))
    }

    /// Scrolls so a row the keys moved to shows, 6 px clear (`reveal`, `:1399`).
    pub fn reveal(&mut self, index: usize) {
        let Some(&Line { top, height, .. }) = self.rows.get(index) else {
            return;
        };
        let shown = top - self.scroll;
        if shown - PADDING < 0.0 {
            self.scroll = top - PADDING;
        } else if shown + height + PADDING > self.bounds.height {
            self.scroll = top + height + PADDING - self.bounds.height;
        }
        self.scroll = self.scroll.clamp(0.0, self.most());
    }

    /// Scrolls by a wheel's turn; whether it moved.
    pub fn wheel(&mut self, delta: iced::mouse::ScrollDelta) -> bool {
        let by = match delta {
            iced::mouse::ScrollDelta::Lines { y, .. } => y * WHEEL_LINE,
            iced::mouse::ScrollDelta::Pixels { y, .. } => y,
        };
        let scroll = (self.scroll - by).clamp(0.0, self.most());
        let moved = scroll != self.scroll;
        self.scroll = scroll;
        moved
    }

    /// Draws the card and its rows; `paint` says how each row is drawn.
    pub fn draw<M>(
        &self,
        renderer: &mut Renderer,
        theme: &Theme,
        rows: &[Row<M>],
        square_top: bool,
        alpha: f32,
        paint: impl Fn(usize) -> Paint,
    ) {
        let palette = theme.palette();
        let corner = if square_top { 0.0 } else { RADIUS };
        renderer.fill_quad(
            Quad {
                bounds: self.bounds,
                border: Border {
                    color: faded(palette.menu_line(), alpha),
                    width: OUTLINE,
                    radius: Radius::new(RADIUS).top_left(corner).top_right(corner),
                },
                shadow: Shadow {
                    color: faded(SHADOW, alpha),
                    offset: Vector::new(0.0, SHADOW_DROP),
                    blur_radius: SHADOW_BLUR,
                },
                ..Quad::default()
            },
            faded(palette.menu(), alpha),
        );
        let inside = self.bounds.shrink(OUTLINE);
        renderer.with_layer(inside, |renderer| {
            for line in &self.lines {
                let at = Rectangle::new(
                    Point::new(self.bounds.x + PADDING, self.bounds.y + line - self.scroll),
                    Size::new(self.bounds.width - 2.0 * PADDING, LINE),
                );
                renderer.fill_quad(
                    Quad {
                        bounds: at,
                        ..Quad::default()
                    },
                    faded(palette.line, alpha),
                );
            }
            for (index, row) in rows.iter().enumerate().take(self.len()) {
                let bounds = self.row(index);
                if bounds.intersects(&inside) {
                    let shape = self.line(index).shape;
                    row::draw(renderer, theme, row, (bounds, shape), paint(index), inside);
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::icon::Icon;

    fn rows() -> Vec<Row<()>> {
        vec![
            Row::new("Mute channel", ()).icon(Icon::BellOff),
            Row::new("Use category default", ())
                .subtitle("Only @mentions")
                .checked(true)
                .separated(),
            Row::new("All messages", ()),
        ]
    }

    fn laid<M>(rows: &[Row<M>], tallest: f32) -> Panel {
        let mut panel = Panel::new(rows, NARROWEST);
        panel.fit(tallest);
        panel
    }

    #[test]
    fn rows_sit_in_padding_with_a_line_before_each_group() {
        let panel = laid(&rows(), f32::INFINITY);
        // 6 + 32, 2 + 9 + 2 for the line, 44, 2, 32, 6.
        let tops: Vec<f32> = panel.rows.iter().map(|line| line.top).collect();
        assert_eq!(tops, [6.0, 51.0, 97.0]);
        assert_eq!(panel.lines, [44.0]);
        assert_eq!(panel.bounds.height, 135.0);
        assert_eq!(panel.bounds.width, NARROWEST);
        // The icon is found once here, for every draw to come.
        let icons: Vec<_> = panel.rows.iter().map(|line| line.shape.icon).collect();
        assert_eq!(icons, [Some(crate::icon::Icon::BellOff), None, None]);
    }

    #[test]
    fn a_tall_menu_scrolls_to_the_row_the_keys_reach() {
        let many: Vec<Row<()>> = (0..30).map(|n| Row::new(format!("Row {n}"), ())).collect();
        let mut panel = laid(&many, 300.0);
        assert_eq!(panel.bounds.height, 300.0);
        panel.reveal(29);
        assert_eq!(panel.scroll, panel.most());
        let last = panel.row(29);
        assert!(last.y + last.height <= panel.bounds.y + panel.bounds.height);
        panel.reveal(0);
        assert_eq!(panel.scroll, 0.0);
        assert!(panel.wheel(iced::mouse::ScrollDelta::Lines { x: 0.0, y: -1.0 }));
        assert_eq!(panel.scroll, WHEEL_LINE);
    }

    #[test]
    fn a_point_on_a_line_or_the_padding_hits_no_row() {
        let panel = laid(&rows(), f32::INFINITY);
        assert_eq!(panel.hit(Point::new(20.0, 10.0)), Some(0));
        assert_eq!(panel.hit(Point::new(20.0, 44.0)), None);
        assert_eq!(panel.hit(Point::new(2.0, 10.0)), None);
        assert_eq!(panel.hit(Point::new(20.0, 60.0)), Some(1));
    }
}
