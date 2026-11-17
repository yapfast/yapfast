//! Drawing a paragraph's blocks: a quote's bar and fenced code's box, then
//! under the text a mention's pill and inline code's capsule, the text and
//! what's selected of it, then over it a spoiler's cover, a server's emoji,
//! lines through and under spans, and the link under the pointer's
//! underline. Ports `TextBlockView` (`ui/messages.slint:2041`) and the
//! `draw_*` patches of `sharedparley/draw.rs`.

use iced::advanced::renderer::{self, Renderer as _};
use iced::advanced::text::Renderer as _;
use iced::border::{Border, Radius};
use iced::{Color, Rectangle, Size, Vector};

use anomi_theme::{Palette, Renderer, Theme, rgb};
use anomi_vm::text::{Block, Level, Look, Span, Text};

use super::capsule;
use super::decorate::{self, Run};
use super::shape::Inks;
use super::state::{BAR, Shaped, State, Target};
use crate::picture::Shape;

/// A hidden spoiler's cover, the same in every theme
/// (`draw_spoiler_covers`, `sharedparley/draw.rs:456`).
const COVER: Color = rgb(0x3a3c45);
/// The corners of a pill and a cover (`RADIUS`, `sharedparley/draw.rs:450`, `:496`).
const PILL_RADIUS: f32 = 4.0;
/// A pill is a little less tall than its line (`INSET_RATIO`, `:499`).
const PILL_INSET: f32 = 0.08;
/// How opaque a mention's pill is (`draw_mention_backgrounds`, `:510`).
const PILL_ALPHA: f32 = 0.16;
/// Fenced code's box (`TextBlockView`, `ui/messages.slint:2066`).
const CODE_RADIUS: f32 = 4.0;
/// The corners of a quote's bar where the quote starts and ends
/// (`TextBlockView`, `ui/messages.slint:2055-2058`).
const QUOTE_RADIUS: f32 = 2.0;
/// How far past its block's box a glyph may reach and still draw, above
/// and below: an emoji, Twemoji's square of 1.36 em in a line of 1.3 em
/// (`LINE`) around DM Sans's baseline, passes its line by about 0.03 em
/// each way, a pixel at most even at the jumbo size of 33; this leaves
/// room to spare. A block cut short keeps none of it below the cut.
const OVERHANG: f32 = 4.0;
/// Where a line under a span starts below its baseline, where a line
/// through it starts above, and how thick both are, in ems: DM Sans's own
/// (`post.underlinePosition`, `OS/2.yStrikeoutPosition`, and the 0.05 em
/// both are thick), which Slint read from the run's font
/// (`underline_offset`, `strikethrough_offset` and their sizes,
/// `sharedparley/draw.rs:551-560`, `:870-881`).
const UNDERLINE_DROP: f32 = 0.1;
const STRIKE_RISE: f32 = 0.316;
const LINE_THICKNESS: f32 = 0.05;
/// No line thinner than a device pixel, as Slint's (`decoration_rect`, `:962`).
const MIN_THICKNESS: f32 = 1.0;
/// Rounding between a block's line heights summed and its shape's own: a
/// cut or a sliver of selection within it is none.
const SLACK: f32 = 0.5;

/// The colours a paragraph's blocks draw in.
pub struct Colors {
    pub text: Color,
    pub heading: Color,
}

/// The colour a block's text takes: a quote's text and subtext are dimmer,
/// headings in the heading colour, code in the text colour
/// (`TextBlockView`, `ui/messages.slint:2078`).
fn color_of(block: &Block, palette: &Palette, colors: &Colors) -> Color {
    match block.level {
        Level::Code => palette.text,
        Level::Subtext => palette.secondary,
        Level::Heading(_) => colors.heading,
        Level::Text if block.quoted => palette.secondary,
        Level::Text => colors.text,
    }
}

/// What draws the blocks: the paragraph's place, the theme's colours,
/// what's under the pointer, and how far down it shows.
pub struct Painter<'a> {
    pub state: &'a State,
    pub text: &'a Text,
    pub theme: &'a Theme,
    pub colors: Colors,
    pub origin: Vector,
    pub viewport: &'a Rectangle,
    /// Device pixels a logical one, which lines under and through spans
    /// snap to: `crate::picture::scale()`, as the app sets it.
    pub scale: f32,
}

impl Painter<'_> {
    pub fn draw(&self, renderer: &mut Renderer) {
        let shapes = self.state.shapes.borrow();
        let shown = Rectangle::new(
            iced::Point::ORIGIN + self.origin,
            Size::new(self.state.width, self.state.shown),
        );
        // An emoji is taller than its line, so text may go past its box.
        let Some(visible) = shown.expand([OVERHANG, 0.0]).intersection(self.viewport) else {
            return;
        };
        let folded = self.state.long && self.state.shown < self.state.full;
        let draw_all = |renderer: &mut Renderer| {
            for (index, ((block, spans), shaped)) in
                self.text.blocks().zip(&shapes.blocks).enumerate()
            {
                if shaped.top > self.state.shown {
                    break;
                }
                let area = Rectangle::new(
                    iced::Point::new(0.0, shaped.top) + self.origin,
                    Size::new(self.state.width, shaped.bottom - shaped.top),
                );
                if area.expand([OVERHANG, 0.0]).intersects(&visible) {
                    self.block(renderer, index, block, spans, shaped, visible);
                }
            }
        };
        match shown.intersection(self.viewport) {
            Some(cut) if folded => renderer.with_layer(cut, draw_all),
            _ if folded => {}
            _ => draw_all(renderer),
        }
    }

    fn block(
        &self,
        renderer: &mut Renderer,
        index: usize,
        block: &Block,
        spans: &[Span],
        shaped: &Shaped,
        visible: Rectangle,
    ) {
        let palette = self.theme.palette();
        if block.quoted {
            let corner = |rounded: bool| if rounded { QUOTE_RADIUS } else { 0.0 };
            let radius = Radius {
                top_left: corner(block.quote_start),
                top_right: corner(block.quote_start),
                bottom_left: corner(block.quote_end),
                bottom_right: corner(block.quote_end),
            };
            let bar = Rectangle::new(
                iced::Point::new(0.0, shaped.top),
                Size::new(BAR, shaped.bottom - shaped.top),
            );
            fill(
                renderer,
                bar + self.origin,
                palette.reply_spine,
                radius,
                None,
            );
        }
        if let Some(boxed) = shaped.boxed {
            let border = Border {
                color: palette.line,
                width: 1.0,
                radius: CODE_RADIUS.into(),
            };
            fill(
                renderer,
                boxed + self.origin,
                palette.panel,
                border.radius,
                Some(border),
            );
        }
        let at = Vector::new(shaped.origin.x, shaped.origin.y) + self.origin;
        let color = color_of(block, palette, &self.colors);
        // Past its box by an emoji's overhang, but not into a line it cuts off.
        use iced::advanced::text::Paragraph as _;
        let cut = shaped.shown + SLACK < shaped.shape.min_bounds().height;
        let below = if cut { 0.0 } else { OVERHANG };
        let lines = Rectangle::new(
            iced::Point::new(at.x, at.y - OVERHANG),
            Size::new(self.state.width, OVERHANG + shaped.shown + below),
        );
        let clip = lines.intersection(&visible).unwrap_or(lines);
        self.under(renderer, spans, shaped, at, color);
        renderer.fill_paragraph(&shaped.shape, iced::Point::ORIGIN + at, color, clip);
        let selected = <Theme as iced::widget::text::Catalog>::selection(self.theme);
        // A selection may run on into the lines a clamp cuts off.
        for region in shaped.shape.selection() {
            let height = region.height.min(shaped.shown - region.y);
            if height <= SLACK {
                continue;
            }
            renderer.fill_quad(
                renderer::Quad {
                    bounds: Rectangle { height, ..*region } + at,
                    ..renderer::Quad::default()
                },
                selected,
            );
        }
        self.over(renderer, index, spans, shaped, at, color, clip);
    }

    /// Under the text: mentions' pills and inline code's capsules.
    fn under(
        &self,
        renderer: &mut Renderer,
        spans: &[Span],
        shaped: &Shaped,
        at: Vector,
        color: Color,
    ) {
        let palette = self.theme.palette();
        let inks = Inks::of(palette);
        let pills = decorate::joined(
            shaped.runs.iter().filter(|run| {
                spans
                    .get(run.span as usize)
                    .is_some_and(|span| span.look == Look::Mention)
            }),
            |run| spans.get(run.span as usize).and_then(|span| span.link),
        );
        for (link, line) in pills {
            let span = spans
                .iter()
                .find(|span| span.link == link && span.look == Look::Mention);
            let ink = span
                .and_then(|span| span.color)
                .map_or_else(|| palette.mention(), |ink| inks.ink(ink));
            let inset = line.height * PILL_INSET;
            let pill = Rectangle::new(
                iced::Point::new(line.x - decorate::SIDE_PADDING, line.y + inset),
                Size::new(
                    line.width + 2.0 * decorate::SIDE_PADDING,
                    (line.height - 2.0 * inset).max(1.0),
                ),
            );
            fill(
                renderer,
                pill + at,
                ink.scale_alpha(PILL_ALPHA),
                PILL_RADIUS.into(),
                None,
            );
        }
        for run in shaped.runs.iter().filter(|run| {
            spans
                .get(run.span as usize)
                .is_some_and(|span| span.style.code)
        }) {
            capsule::draw(renderer, run, at, color);
        }
    }

    /// Over the text: covers, pictures, lines and the hovered underline.
    #[allow(clippy::too_many_arguments)]
    fn over(
        &self,
        renderer: &mut Renderer,
        index: usize,
        spans: &[Span],
        shaped: &Shaped,
        at: Vector,
        color: Color,
        clip: Rectangle,
    ) {
        let palette = self.theme.palette();
        let inks = Inks::of(palette);
        let covers = decorate::joined(
            shaped.runs.iter().filter(|run| {
                spans
                    .get(run.span as usize)
                    .is_some_and(|span| span.look == Look::Spoiler)
            }),
            |run| spans.get(run.span as usize).and_then(|span| span.link),
        );
        if !covers.is_empty() {
            // A layer of their own: within one, quads draw under the text.
            renderer.with_layer(clip, |renderer| {
                for (_, line) in covers {
                    let cover = line.expand([0.0, decorate::SIDE_PADDING]);
                    fill(renderer, cover + at, COVER, PILL_RADIUS.into(), None);
                }
            });
        }
        let hovered = self.state.hovered.filter(|target| target.block == index);
        for run in &shaped.runs {
            let Some(span) = spans.get(run.span as usize) else {
                continue;
            };
            if let Look::Picture(picture) = span.look
                && let (Some(player), Some(picture)) = (
                    self.state.players.get(picture as usize),
                    self.text.picture(picture),
                )
            {
                let shape = Shape {
                    fit: crate::picture::Fit::Contain,
                    ..Shape::default()
                };
                player.draw(
                    renderer,
                    &picture.still,
                    run.picture() + at,
                    shape,
                    1.0,
                    &clip,
                );
                continue;
            }
            let own = span.color.map(|ink| inks.ink(ink));
            let ink = match span.look {
                Look::Mention | Look::Channel => own.unwrap_or_else(|| palette.mention()),
                Look::Text if span.link.is_some() => own.unwrap_or(palette.link),
                _ => own.unwrap_or(color),
            };
            let hover = hovered.is_some_and(|Target { link, .. }| span.link == Some(link))
                && span.underlines();
            let drawn = Drawn {
                at,
                scale: self.scale,
                color: ink,
            };
            if span.style.underline || hover {
                drawn.line(renderer, run, run.baseline + run.size * UNDERLINE_DROP);
            }
            if span.style.strike {
                drawn.line(renderer, run, run.baseline - run.size * STRIKE_RISE);
            }
        }
    }
}

/// Where a run's lines go: its block's place, device pixels a logical
/// one, and the lines' colour.
struct Drawn {
    at: Vector,
    scale: f32,
    color: Color,
}

impl Drawn {
    /// A line under or through `run`, its top `y` below its block's top,
    /// on the device's pixels as Slint puts it (`decoration_rect`,
    /// `sharedparley/draw.rs:947-963`, and `snap_y`,
    /// `i-slint-renderer-skia/itemrenderer.rs:1275`): its top on a pixel's
    /// edge and whole pixels thick, at least one, so a quad smoothed at its
    /// edges never blurs it over two rows or drops it.
    fn line(&self, renderer: &mut Renderer, run: &Run, y: f32) {
        let scale = self.scale;
        let top = ((y + self.at.y) * scale).round() / scale;
        let thickness = (run.size * LINE_THICKNESS * scale)
            .round()
            .max(MIN_THICKNESS)
            / scale;
        let bounds = Rectangle::new(
            iced::Point::new(run.left + self.at.x, top),
            Size::new(run.right - run.left, thickness),
        );
        fill(renderer, bounds, self.color, Radius::default(), None);
    }
}

pub(super) fn fill(
    renderer: &mut Renderer,
    bounds: Rectangle,
    color: Color,
    radius: Radius,
    border: Option<Border>,
) {
    renderer.fill_quad(
        renderer::Quad {
            bounds,
            border: border.unwrap_or(Border {
                radius,
                ..Border::default()
            }),
            ..renderer::Quad::default()
        },
        color,
    );
}
