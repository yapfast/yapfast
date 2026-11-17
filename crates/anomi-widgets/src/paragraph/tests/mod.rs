//! The paragraph in the simulator over the fake Discord's messages
//! (`anomi_vm::fixtures::text`): what a click and a right press on a link,
//! a mention, a spoiler and an emoji send; how blocks lie; folding and
//! clamping; selecting and copying; and what a thousand cost.

mod fold;
mod layout;
mod lines;
mod looks;
mod memory;
mod pointer;

use std::borrow::Cow;

use iced::advanced::widget::Tree;
use iced::{Event, Point, Settings, Size, mouse};
use iced_test::Simulator;

use anomi_theme::prelude::*;
use anomi_theme::{Renderer, fonts, typography};
use anomi_vm::text::{Link, Text};

use super::state::{Look, State};
use super::{Paragraph, paragraph};

#[derive(Clone, Debug, PartialEq)]
enum Asked {
    Open(Link),
    Right(Link),
    Menu,
    Toggle,
}

/// The window the tests draw in.
const SIZE: Size = Size::new(600.0, 400.0);

fn settings() -> Settings {
    fonts::install();
    Settings {
        fonts: fonts::ALL.iter().map(|font| Cow::Borrowed(*font)).collect(),
        font: typography::UI,
        ..Settings::default()
    }
}

/// A paragraph that sends what its links open.
fn linked(text: &Text) -> Paragraph<'_, Asked> {
    paragraph(text)
        .on_link(Asked::Open)
        .on_right_press(Asked::Right)
        .motion(Motion {
            front: true,
            ..Motion::default()
        })
}

fn simulate<'a>(view: impl View<Asked> + 'a) -> Simulator<'a, Asked, Theme, Renderer> {
    Simulator::with_size(settings(), SIZE, view.boxed())
}

/// `text` laid out as a paragraph `width` wide at the message size.
fn laid_out(text: &Text, width: f32) -> State {
    fonts::install();
    let mut state = State::default();
    state.lay_out(
        text,
        Look {
            size: if text.jumbo() { 15.0 * 2.2 } else { 15.0 },
            framed: false,
            max_lines: None,
            hint_factor: None,
        },
        width,
    );
    state.arrange(width);
    state.shown = state.full;
    state
}

/// The middle of the first run of the span that opens `link`, in the
/// paragraph's coordinates.
fn middle_of(state: &State, link: &Link) -> Point {
    let shapes = state.shapes.borrow();
    for ((_, spans), shaped) in state.text.blocks().zip(&shapes.blocks) {
        for run in &shaped.runs {
            let opens = spans
                .get(run.span as usize)
                .and_then(|span| span.link)
                .and_then(|index| state.text.link(index));
            if opens == Some(link) {
                let line = run.line_box();
                return Point::new(
                    shaped.origin.x + line.center_x(),
                    shaped.origin.y + line.center_y(),
                );
            }
        }
    }
    panic!("no span opens {link:?}");
}

/// Moves the pointer to `point`, as the window would tell it.
fn move_to(ui: &mut Simulator<'_, Asked, Theme, Renderer>, point: Point) {
    ui.point_at(point);
    ui.simulate([Event::Mouse(mouse::Event::CursorMoved { position: point })]);
}

/// A tree for the paragraph alone, laid out `width` wide: its state, and
/// the height it takes.
fn tree_of(view: &mut Paragraph<'_, Asked>, width: f32) -> (Tree, f32) {
    use iced::advanced::layout::Limits;
    use iced::advanced::widget::Widget as _;
    fonts::install();
    let renderer = Renderer::new(iced::advanced::renderer::Settings {
        font: typography::UI,
        text_size: iced::Pixels(15.0),
        ..iced::advanced::renderer::Settings::default()
    });
    let mut tree = Tree::new(&*view);
    view.diff(&mut tree);
    view.layout(
        &mut tree,
        &renderer,
        &Limits::new(Size::ZERO, Size::new(width, f32::INFINITY)),
    );
    let height = tree.size.height;
    (tree, height)
}
