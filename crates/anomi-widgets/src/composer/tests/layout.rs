//! What the field keeps and asks of the window: its height grows with its
//! lines up to ten and then the caret's line stays in view, the pills add
//! their height, the caret blinks only while the window is in front and in
//! use, the marks hold for the view-model's text alone, and it draws in
//! every state.

use std::time::{Duration, Instant};

use iced::advanced::text::editor::{Editor as _, Selection};
use iced::keyboard::Modifiers;
use iced::keyboard::key::Named;
use iced::widget::space;

use super::*;

/// One line of DM Sans at 16 px, as the editor lays it out.
const LINE: f32 = 16.0 * TEXT_LINE;

fn height(text: &str) -> f32 {
    let mut view = field(text, 1);
    Rig::new(&mut view).size().height
}

#[test]
fn it_grows_with_its_lines_up_to_ten_then_holds() {
    assert_eq!(height(""), MIN_HEIGHT, "one line sits in the least height");
    let three = height("a\nb\nc");
    assert!((three - (3.0 * LINE + 2.0 * PAD)).abs() < 0.5, "{three}");
    let tallest = LINES * TEXT_SIZE * LINE_ROOM + 2.0 * PAD;
    let lines = |count: usize| {
        (0..count)
            .map(|line| format!("line {line}"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    // Ten lines of 1.5 × the size hold eleven and a half of the text's own.
    let eleven = height(&lines(11));
    assert!((eleven - (11.0 * LINE + 2.0 * PAD)).abs() < 0.5, "{eleven}");
    for count in [12, 30] {
        let tall = height(&lines(count));
        assert!((tall - tallest).abs() < 0.5, "{count} lines: {tall}");
    }
    // The channel's composer: Discord's padding and least height.
    let mut channel = field("", 1)
        .pad(anomi_theme::metrics::COMPOSER_PADDING)
        .min_height(anomi_theme::metrics::COMPOSER_HEIGHT);
    assert_eq!(Rig::new(&mut channel).size().height, 58.0);
}

#[test]
fn the_caret_line_stays_in_view_as_the_text_outgrows_it() {
    let mut view = field("", 1);
    let mut rig = Rig::new(&mut view);
    rig.focus();
    for line in 0..25 {
        rig.type_text(&mut view, &format!("line {line}"));
        rig.send(&mut view, named(Named::Enter, Modifiers::SHIFT));
        rig.lay_out(&mut view);
    }
    rig.type_text(&mut view, "last");
    rig.lay_out(&mut view);
    let state = rig.state();
    assert!(state.overflows);
    let editor = state.editor.borrow();
    let Some(Selection::Caret(at)) = editor.selection() else {
        panic!("a caret");
    };
    assert!(
        at.y >= 0.0 && at.y + LINE <= state.visible + 0.5,
        "{at:?} in {}",
        state.visible
    );
}

#[test]
fn the_pills_add_their_height_over_the_text() {
    let mut view = field("", 1)
        .command(Some(Command {
            options: 1,
            ..Command::default()
        }))
        .pills(space().height(26));
    let rig = Rig::new(&mut view);
    assert_eq!(rig.size().height, MIN_HEIGHT + 26.0 + PILLS_GAP);
    assert_eq!(rig.state().pills, 26.0 + PILLS_GAP);
}

#[test]
fn the_caret_blinks_only_in_front_and_in_use() {
    let start = Instant::now();
    let mut view = field("hi", 1);
    let mut rig = Rig::new(&mut view);
    rig.focus();
    // The first frame tells of the focus, and draws the caret at once.
    assert_eq!(rig.frame(&mut view, start).redraw, RedrawRequest::NextFrame);
    let reply = rig.frame(&mut view, start);
    assert!(
        matches!(reply.redraw, RedrawRequest::At(at) if at > start && at <= start + Duration::from_millis(500)),
        "{:?}",
        reply.redraw
    );
    for motion in [
        Motion::default(),
        Motion {
            idle: true,
            ..front()
        },
    ] {
        let mut still = field("hi", 1);
        still.motion = motion;
        let reply = rig.frame(&mut still, start + Duration::from_millis(600));
        assert_eq!(reply.redraw, RedrawRequest::Wait, "{motion:?}");
    }
    // Without the keys, nothing at all.
    let mut idle = field("hi", 1);
    let mut rig = Rig::new(&mut idle);
    assert_eq!(rig.frame(&mut idle, start).redraw, RedrawRequest::Wait);
}

#[test]
fn marks_hold_for_the_view_models_text_alone() {
    let marks = [(3..7, None)];
    let theme = Theme::default();
    let mut view = field("hi @Bob", 1).marks(&marks);
    let mut rig = Rig::new(&mut view);
    rig.draw(&view, &theme);
    assert!(rig.state().marker.borrow().any());
    // A keystroke the app hasn't answered: the field shows another text.
    rig.focus();
    rig.type_text(&mut view, "!");
    rig.draw(&view, &theme);
    assert!(!rig.state().marker.borrow().any(), "drawn plain");
    // A mark past the text, or inside a character, is left out.
    let wrong = [(0..40, None), (1..2, None)];
    let mut view = field("hé", 1).marks(&wrong);
    let mut rig = Rig::new(&mut view);
    rig.draw(&view, &theme);
    assert!(!rig.state().marker.borrow().any());
}

#[test]
fn it_draws_empty_written_selected_and_off_in_every_theme() {
    let marks = [(0..4, Some(iced::Color::from_rgb(0.9, 0.3, 0.3)))];
    for built_in in &anomi_theme::builtin::BUILT_IN {
        let theme = Theme::new(built_in.id, built_in.name, built_in.palette);
        let mut empty = field("", 1);
        Rig::new(&mut empty).draw(&empty, &theme);
        let mut written = field("@Bob and a long line\nand another", 1).marks(&marks);
        let mut rig = Rig::new(&mut written);
        rig.focus();
        rig.send(&mut written, ctrl("a", Code::KeyA));
        rig.draw(&written, &theme);
        let mut off = composer::<Asked>("off", front()).placeholder("Locked");
        Rig::new(&mut off).draw(&off, &theme);
    }
}
