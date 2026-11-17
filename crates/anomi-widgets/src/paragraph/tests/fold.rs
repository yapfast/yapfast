//! Folding and clamping: a long body folds to twelve lines once longer than
//! fifteen, with its toggle under it; one shown whole is whole; a block
//! clamped to a few lines is as tall as those, and draws nothing of the
//! lines it cuts off.

use iced::advanced::widget::operation;
use iced::widget::button;

use anomi_vm::fixtures::text as samples;
use anomi_vm::text::{Builder, Style};

use super::super::{Fold, LINE, TOGGLE_GAP};
use super::*;

/// A line of the message size.
const LINE_HEIGHT: f32 = 15.0 * LINE;

fn folded(text: &Text, open: bool) -> (Tree, f32) {
    let mut view = linked(text).fold(
        Fold {
            open,
            ..Fold::MESSAGE
        },
        button(iced::widget::text("Show more")).on_press(Asked::Toggle),
    );
    tree_of(&mut view, SIZE.width)
}

#[test]
fn a_twelve_line_clamp_measures_twelve_lines() {
    let (tree, height) = folded(&samples::long(20), false);
    let state = tree.state.downcast_ref::<State>();
    assert!(state.long);
    assert!(
        (state.full - 20.0 * LINE_HEIGHT).abs() < 0.5,
        "{}",
        state.full
    );
    assert!(
        (state.shown - 12.0 * LINE_HEIGHT).abs() < 0.01,
        "{}",
        state.shown
    );
    let toggle = tree.children[0].size.height;
    assert!(toggle > 0.0);
    assert!((height - (state.shown + TOGGLE_GAP + toggle)).abs() < 0.01);
}

#[test]
fn a_body_a_few_lines_past_twelve_shows_whole() {
    let (tree, height) = folded(&samples::long(14), false);
    let state = tree.state.downcast_ref::<State>();
    assert!(!state.long);
    assert_eq!(height, state.full);
}

#[test]
fn shown_whole_it_keeps_its_toggle_under_it() {
    let (tree, height) = folded(&samples::long(20), true);
    let state = tree.state.downcast_ref::<State>();
    assert!(state.long);
    assert_eq!(state.shown, state.full);
    assert!(height > state.full + TOGGLE_GAP);
}

#[test]
fn the_toggle_is_clicked_under_the_folded_body() {
    let text = samples::long(20);
    let view = linked(&text).fold(
        Fold::MESSAGE,
        button(iced::widget::text("Show more")).on_press(Asked::Toggle),
    );
    let mut ui = simulate(view);
    ui.click("Show more").expect("the toggle shows");
    assert_eq!(ui.into_messages().collect::<Vec<_>>(), [Asked::Toggle]);
}

#[test]
fn each_block_shows_at_most_its_lines() {
    let text = samples::long(6);
    let mut view = linked(&text).max_lines(2);
    let (_, height) = tree_of(&mut view, SIZE.width);
    assert!((height - 2.0 * LINE_HEIGHT).abs() < 0.01, "{height}");
    let mut whole = linked(&text);
    let (_, full) = tree_of(&mut whole, SIZE.width);
    assert!((full - 6.0 * LINE_HEIGHT).abs() < 0.5, "{full}");
}

#[test]
fn a_line_that_wraps_is_two_lines_of_a_narrow_paragraph() {
    let text = samples::history(53);
    let mut wide = linked(&text);
    let (_, one) = tree_of(&mut wide, 2000.0);
    let mut narrow = linked(&text);
    let (_, two) = tree_of(&mut narrow, 300.0);
    assert!((one - LINE_HEIGHT).abs() < 0.5, "{one}");
    assert!(two >= 2.0 * LINE_HEIGHT - 0.5, "{two}");
}

/// Two plain lines, then a third: inline code, a mention, a line through
/// and under, and a server's emoji; or plain text.
fn third_line(decorated: bool) -> Text {
    let mut builder = Builder::new();
    builder.text("Line one\nLine two\n", Style::default());
    if decorated {
        let code = Style {
            code: true,
            ..Style::default()
        };
        let lined = Style {
            strike: true,
            underline: true,
            ..Style::default()
        };
        builder
            .text("ok", code)
            .text(" ", Style::default())
            .link("@Alice", Style::default(), Link::User("alice".into()), None)
            .text(" gone ", lined)
            .picture(":talk:", samples::badge(samples::TALK), Some(samples::TALK));
    } else {
        builder.text("Line three, plain", Style::default());
    }
    builder.finish()
}

#[test]
fn a_clamped_block_draws_nothing_of_the_lines_it_cuts_off() {
    let draw = |text: &Text| {
        let mut ui = simulate(linked(text).max_lines(2));
        ui.operate(&mut operation::text::select_all());
        ui.snapshot(&Theme::built_in(Some("bonfire")))
            .expect("draws")
    };
    let (plain, decorated) = (third_line(false), third_line(true));
    let folder = std::env::temp_dir().join(format!("anomi-clamp-{}", std::process::id()));
    let shot = folder.join("clamped");
    // The plain third line's drawing is kept as a hash, the other's compared.
    let kept = draw(&plain).matches_hash(&shot);
    let same = draw(&decorated).matches_hash(&shot);
    let _ = std::fs::remove_dir_all(&folder);
    assert!(kept.expect("writes"));
    assert!(
        same.expect("reads"),
        "a capsule, a pill, a line, a selection or an emoji of the third line drew"
    );
    // Nor does it keep where they'd draw, which a whole one does.
    let whole = laid_out(&decorated, SIZE.width);
    assert!(!whole.shapes.borrow().blocks[0].runs.is_empty());
    let mut view = linked(&decorated).max_lines(2);
    let (tree, _) = tree_of(&mut view, SIZE.width);
    let state = tree.state.downcast_ref::<State>();
    assert!(state.shapes.borrow().blocks[0].runs.is_empty());
}
