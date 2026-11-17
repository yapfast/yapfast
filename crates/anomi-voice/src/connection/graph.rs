//! The ping graph (`ui/voice.slint:236`): an 80 px sunken box with a line
//! at the top and one at the half, the pings as steps in the link colour,
//! and what the two lines stand for.

use iced::advanced::mouse;
use iced::widget::canvas::{self, Frame, LineJoin, Path, Stroke};
use iced::widget::{canvas as canvas_widget, container, stack, text};
use iced::{Fill, Point, Rectangle};

use anomi_theme::prelude::*;
use anomi_theme::{Renderer, Theme, catalog, metrics};
use anomi_vm::voice::connection::{Connection, GRAPH};
use yapfast_lang::tr;

const HEIGHT: f32 = 80.0;
const STROKE: f32 = 2.0;
const LABEL: f32 = 11.0;

pub(super) fn graph<M: 'static>(connection: &Connection) -> impl View<M> + '_ {
    let top = connection.top();
    let label = |ms: u32, y: f32| {
        container(
            text(tr!("{} ms", ms))
                .size(LABEL)
                .style(catalog::text::muted),
        )
        .width(Fill)
        .height(Fill)
        .padding(iced::Padding {
            top: y,
            left: 6.0,
            ..iced::Padding::ZERO
        })
    };
    let lines = canvas_widget(Lines {
        steps: connection.steps(),
    })
    .width(Fill)
    .height(Fill);
    container(stack![
        lines,
        label(top, 4.0),
        label(top / 2, HEIGHT / 2.0 + 4.0)
    ])
    .width(Fill)
    .height(HEIGHT)
    .clip(true)
    .style(|theme: &Theme| container::Style {
        background: Some(theme.palette().sunken.into()),
        border: iced::Border::default().rounded(metrics::RADIUS_ROW),
        ..container::Style::default()
    })
}

struct Lines {
    steps: Vec<Point>,
}

impl<M> canvas::Program<M, Theme, Renderer> for Lines {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let palette = theme.palette();
        let mut frame = Frame::new(renderer, bounds.size());
        for y in [0.0, bounds.height / 2.0] {
            let line = Path::rectangle(Point::new(0.0, y), iced::Size::new(bounds.width, 1.0));
            frame.fill(&line, palette.line);
        }
        if let Some((first, rest)) = self.steps.split_first() {
            let (sx, sy) = (bounds.width / GRAPH.0, bounds.height / GRAPH.1);
            let scaled = |point: &Point| Point::new(point.x * sx, point.y * sy);
            let path = Path::new(|path| {
                path.move_to(scaled(first));
                for point in rest {
                    path.line_to(scaled(point));
                }
            });
            frame.stroke(
                &path,
                Stroke::default()
                    .with_color(palette.link)
                    .with_width(STROKE)
                    .with_line_join(LineJoin::Miter),
            );
        }
        // Slint clips the lines and the path to the rounded box
        // (`ui/voice.slint:235`): the corners outside the arcs are the box's
        // card's colour again.
        let r = metrics::RADIUS_ROW;
        let corner = |x: f32, y: f32, sx: f32, sy: f32| {
            Path::new(|path| {
                path.move_to(Point::new(x, y));
                path.line_to(Point::new(x + sx * r, y));
                path.arc_to(Point::new(x, y), Point::new(x, y + sy * r), r);
                path.line_to(Point::new(x, y + sy * r));
                path.close();
            })
        };
        let (w, h) = (bounds.width, bounds.height);
        for (x, y, sx, sy) in [
            (0.0, 0.0, 1.0, 1.0),
            (w, 0.0, -1.0, 1.0),
            (0.0, h, 1.0, -1.0),
            (w, h, -1.0, -1.0),
        ] {
            frame.fill(&corner(x, y, sx, sy), palette.raised);
        }
        vec![frame.into_geometry()]
    }
}
