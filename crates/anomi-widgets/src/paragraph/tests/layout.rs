//! How blocks lie and what they keep: a heading's size, code's box, a
//! quote's bar, emoji alone large, pills and covers where their spans are;
//! copying as typed; shaping again in another theme; and a server's emoji
//! asking for frames only while things may move.

use std::sync::Arc;
use std::time::Instant;

use iced::Rectangle;
use iced::advanced::widget::operation;

use anomi_theme::builtin;
use anomi_vm::fixtures::text as samples;
use anomi_vm::text::Look as SpanLook;

use super::super::shape::Inks;
use super::super::state::BAR;
use super::*;
use crate::picture::{self, Frames};

#[test]
fn a_heading_text_and_code_lie_as_discord_lays_them() {
    let state = laid_out(&samples::status(), SIZE.width);
    let shapes = state.shapes.borrow();
    let [heading, text, code] = shapes.blocks.as_slice() else {
        panic!("three blocks");
    };
    assert_eq!(heading.size, 15.0 * 1.25);
    assert_eq!(heading.top, 8.0);
    assert_eq!(text.top, heading.bottom + 8.0);
    assert_eq!(code.top, text.bottom + 6.0);
    let boxed = code.boxed.expect("code has its box");
    let padding = 15.0 * 0.875 / 2.0;
    assert_eq!((boxed.x, boxed.width), (0.0, SIZE.width));
    assert_eq!(code.origin, Point::new(padding, code.top + padding));
    assert_eq!(code.bottom, boxed.y + boxed.height);
    assert_eq!(state.full, code.bottom);
}

#[test]
fn a_quotes_text_lies_beside_its_bar() {
    let state = laid_out(&samples::notes(), SIZE.width);
    let shapes = state.shapes.borrow();
    let quote = &shapes.blocks[0];
    assert_eq!(quote.origin.x, BAR + 12.0);
    assert_eq!(quote.top, 4.0);
}

#[test]
fn emoji_alone_draw_large_and_in_text_at_twemojis_size() {
    let picture_side = |text: &Text| {
        let state = laid_out(text, SIZE.width);
        let shapes = state.shapes.borrow();
        let ((_, spans), shaped) = state
            .text
            .blocks()
            .zip(&shapes.blocks)
            .next()
            .expect("a block");
        let run = shaped
            .runs
            .iter()
            .find(|run| matches!(spans[run.span as usize].look, SpanLook::Picture(_)))
            .expect("a picture");
        run.picture().width
    };
    let small = picture_side(&samples::ship_it());
    let large = picture_side(&samples::jumbo());
    // Discord's 22 pixels in 16 pixel text, and 48 in its large text.
    assert!((small - 15.0 * 72.0 / 53.0).abs() < 0.01, "{small}");
    assert!((large / small - 2.2).abs() < 0.01, "{large}");
}

#[test]
fn mentions_and_spoilers_keep_their_runs_and_plain_text_keeps_none() {
    let runs = |text: &Text| -> Vec<SpanLook> {
        let state = laid_out(text, SIZE.width);
        let shapes = state.shapes.borrow();
        let ((_, spans), shaped) = state
            .text
            .blocks()
            .zip(&shapes.blocks)
            .next()
            .expect("a block");
        shaped
            .runs
            .iter()
            .map(|run| spans[run.span as usize].look)
            .collect()
    };
    assert_eq!(runs(&samples::history(52)), []);
    let mentions = runs(&samples::mentions());
    assert_eq!(
        mentions
            .iter()
            .filter(|look| **look == SpanLook::Mention)
            .count(),
        2
    );
    let ending = runs(&samples::ending());
    assert!(
        ending
            .iter()
            .filter(|look| **look == SpanLook::Spoiler)
            .count()
            >= 2
    );
}

/// Everything `text` holds, selected and copied.
fn copied(text: &Text) -> String {
    let mut ui = simulate(linked(text));
    ui.operate(&mut operation::text::select_all());
    copy_from(&mut ui)
}

/// What's selected in `ui`, copied.
fn copy_from(ui: &mut Simulator<'_, Asked, Theme, Renderer>) -> String {
    let mut copy = operation::text::copy();
    ui.operate(&mut operation::black_box(&mut copy));
    match operation::Operation::finish(&copy) {
        operation::Outcome::Some(copied) => copied,
        _ => String::new(),
    }
}

#[test]
fn copying_reads_as_typed() {
    assert_eq!(copied(&samples::history(55)), "Message 55: 👍 sounds good");
    assert_eq!(copied(&samples::ship_it()), "Ship it :talk: today :party:");
    assert_eq!(
        copied(&samples::status()),
        "All systems normal\nEverything is running. Ask @Bob if something looks off.\nweb  ok\napi  ok"
    );
}

/// A message with the showcase's emoji.
fn with_emoji(markdown: &str) -> Text {
    anomi_vm::text::markdown(anomi_vm::text::Source {
        emoji: &samples::emoji(),
        cards: true,
        ..anomi_vm::text::Source::new(markdown)
    })
}

#[test]
fn copying_tells_a_typed_square_from_a_servers_emojis_place() {
    // A server's emoji keeps its place with Twemoji's ⬛, as a typed ⬛ draws.
    assert_eq!(copied(&with_emoji("⬛ ok")), "⬛ ok");
    assert_eq!(copied(&with_emoji("⬛🟩⬛")), "⬛🟩⬛");
    assert_eq!(copied(&with_emoji("⬛ :talk:")), "⬛ :talk:");
    assert_eq!(
        copied(&with_emoji(":party: ⬛ :talk: ⬛")),
        ":party: ⬛ :talk: ⬛"
    );
}

#[test]
fn copying_part_reads_the_emoji_it_holds() {
    let text = with_emoji(":talk: ⬛ :party: 1️⃣");
    let state = laid_out(&text, SIZE.width);
    let shapes = state.shapes.borrow();
    let first = shapes.blocks[0].runs[0];
    // From just after the first emoji to the end of its line.
    let y = first.top + first.height / 2.0;
    let mut ui = simulate(linked(&text));
    ui.operate(&mut operation::text::select(
        Point::new(first.right + 1.0, y),
        Point::new(SIZE.width - 1.0, y),
        iced::advanced::text::Target::Character,
    ));
    assert_eq!(copy_from(&mut ui), " ⬛ :party: 1️⃣");
}

#[test]
fn another_theme_shapes_its_links_again() {
    let state = laid_out(&samples::welcome(), SIZE.width);
    let latte = Inks::of(&builtin::chosen(Some("catppuccin-latte")).palette);
    state.reink(latte);
    assert_eq!(state.shapes.borrow().inks, latte);
}

#[test]
fn a_servers_emoji_asks_for_frames_only_while_things_may_move() {
    let text = samples::ship_it();
    let still = text.pictures()[0].still.clone();
    let frames = vec![vec![0u8; 48 * 48 * 4], vec![255u8; 48 * 48 * 4]];
    picture::register(
        &still,
        Arc::new(Frames::new(48, 48, frames, vec![100, 100], 0)),
    );
    let window = Rectangle::with_size(SIZE);
    let due = |motion: Motion| {
        let mut view = linked(&text).motion(motion);
        let (mut tree, height) = tree_of(&mut view, SIZE.width);
        let state = tree.state.downcast_mut::<State>();
        let bounds = Rectangle::new(Point::ORIGIN, Size::new(SIZE.width, height));
        view.play(state, bounds, &window, Instant::now())
    };
    assert!(
        due(Motion {
            front: true,
            ..Motion::default()
        })
        .is_some()
    );
    // Behind another window nothing asks for a frame.
    assert_eq!(due(Motion::default()), None);
    picture::forget(&still);
}
