//! The tooltip of a badge that levels up (`BadgeTipCard`,
//! `ui/people.slint:716`), set by Discord's own values: 200px wide in the
//! theme's tooltip colour, 8px corners and a 1px edge, 12px inside at the
//! sides and 16px above and below (24px under a card with no progress), the
//! level's picture 64px high, then the kind, the level in capitals and the
//! progress 4px apart; a caret under it pointing at the badge. The tooltip
//! (`anomi_widgets::tip::tip_card`) gives it its way in.

use std::borrow::Cow;

use iced::widget::{canvas, column, container, text};
use iced::{Border, Color, Length, Point, Rectangle, Shadow, Vector, alignment, mouse};

use anomi_theme::catalog::text as ink;
use anomi_theme::prelude::*;
use anomi_theme::{Renderer, Theme, metrics, rgba, typography};
use anomi_vm::people::badges::Level;

use anomi_widgets::picture::{Fit, picture};

/// The card's width, and the picture's height.
const WIDTH: f32 = 200.0;
const PICTURE: f32 = 64.0;
/// Room inside at the sides, above, and below with and without progress.
const SIDE: f32 = 12.0;
const TOP: f32 = 16.0;
const BOTTOM: f32 = 16.0;
const BOTTOM_PLAIN: f32 = 24.0;
/// Between the picture and the words, and between the words.
const GAP: f32 = 16.0;
const LINES: f32 = 4.0;
/// The caret: 16 wide and 9 high, its tip on the badge.
pub const CARET_WIDTH: f32 = 16.0;
pub const CARET_HEIGHT: f32 = 9.0;
/// The edge and the shadow (`ui/people.slint:726-729`: `#97979f1f`, and
/// `#0000003d` 12px down, blurred 24px).
const EDGE: Color = rgba(0x97979f1f);
const SHADOW: Color = rgba(0x0000003d);
const SHADOW_OFFSET: f32 = 12.0;
const SHADOW_BLUR: f32 = 24.0;
const SMALL: f32 = typography::size::SMALL;
const TITLE: f32 = typography::size::TITLE;

pub fn card<'a, M: 'a>(level: &'a Level) -> impl View<M> + 'a {
    let mut words: Vec<Element<'a, M>> = Vec::new();
    if !level.kind.is_empty() {
        words.push(line(&level.kind, SMALL, typography::MEDIUM, false).boxed());
    }
    words.push(line(&level.title, TITLE, black(), true).boxed());
    let progress = !level.progress.is_empty();
    if progress {
        words.push(line(&level.progress, SMALL, typography::MEDIUM, false).boxed());
    }
    let words = column(words).spacing(LINES).width(Length::Fill);
    let face: Element<'a, M> = match level.picture.ready() {
        Some(handle) => container(picture(handle.clone()).height(PICTURE).fit(Fit::Contain))
            .center_x(Length::Fill)
            .boxed(),
        None => iced::widget::space().height(PICTURE).boxed(),
    };
    let body = column![face, words].spacing(GAP).width(Length::Fill);
    let bottom = if progress { BOTTOM } else { BOTTOM_PLAIN };
    let boxed = container(body)
        .padding(iced::Padding {
            top: TOP,
            bottom,
            left: SIDE,
            right: SIDE,
        })
        .width(WIDTH)
        .style(|theme: &Theme| {
            let palette = theme.palette();
            container::Style {
                background: Some(palette.tooltip.into()),
                border: Border {
                    color: EDGE,
                    width: 1.0,
                    radius: metrics::RADIUS_CARD.into(),
                },
                shadow: Shadow {
                    color: SHADOW,
                    offset: Vector::new(0.0, SHADOW_OFFSET),
                    blur_radius: SHADOW_BLUR,
                },
                ..container::Style::default()
            }
        });
    // The caret hangs 1px into the card so no seam shows between them.
    column![
        boxed,
        container(canvas(Caret).width(CARET_WIDTH).height(CARET_HEIGHT)).center_x(WIDTH),
    ]
    .spacing(-1.0)
}

fn black() -> iced::Font {
    typography::weight(iced::font::Weight::Black)
}

/// One centred line: wrapping, in the card's ink.
fn line<'a, M: 'a>(
    content: &'a str,
    size: f32,
    font: iced::Font,
    capitals: bool,
) -> impl View<M> + 'a {
    let shown: Cow<'a, str> = if capitals {
        Cow::Owned(content.to_uppercase())
    } else {
        Cow::Borrowed(content)
    };
    text(shown)
        .size(size)
        .font(font)
        .width(Length::Fill)
        .align_x(alignment::Horizontal::Center)
        .style(if capitals {
            ink::primary
        } else {
            ink::secondary
        })
}

/// The triangle, in the tooltip colour.
struct Caret;

impl<M> canvas::Program<M, Theme, Renderer> for Caret {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let path = canvas::Path::new(|path| {
            path.move_to(Point::new(0.0, 0.0));
            path.line_to(Point::new(CARET_WIDTH, 0.0));
            path.line_to(Point::new(CARET_WIDTH / 2.0, CARET_HEIGHT));
            path.close();
        });
        frame.fill(&path, theme.palette().tooltip);
        vec![frame.into_geometry()]
    }
}
