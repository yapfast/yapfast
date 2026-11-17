//! Editing a message in place: task C13's tests, driving
//! `anomi_chat::inline_edit::view` over its fixtures in the simulator
//! (`super::support`), and the message and the timeline around it.
//!
//! The editor sits at the bottom of its window, as the last message's does,
//! so the suggestions have room to float up over it. Its pieces are found
//! from the hint under the box: the box ends 5 px over the hint and is 58
//! tall with one line of text; the emoji button sits 12 px in from the box's
//! right edge and 14 over its bottom.

use iced::keyboard::key::Named;
use iced::widget::container;
use iced::{Fill, Point, Rectangle, Size};
use iced_test::{Simulator, simulator};

use anomi_chat::{inline_edit, message, timeline};
use anomi_theme::{Renderer, Theme};
use anomi_vm::chat::composer::Suggest;
use anomi_vm::chat::inline_edit::{InlineEdit, Intent};
use anomi_vm::chat::timeline::Timeline;
use anomi_vm::chat::{Intent as Chat, suggestions};
use anomi_vm::fixtures::chat::inline_edit as sample;
use anomi_vm::fixtures::chat::message::bobs;

use super::support::{asked, front, settings};

const WINDOW: Size = Size::new(760.0, 320.0);
/// The box: 58 tall, 5 px over the hint.
const HEIGHT: f32 = 58.0;
const OVER_HINT: f32 = 5.0;

type Ui<'a> = Simulator<'a, Chat, Theme, Renderer>;

fn ui(edit: &InlineEdit) -> Ui<'_> {
    use iced::Widget as _;
    let editor = container(inline_edit::view(edit, front()).map(Chat::from))
        .align_bottom(Fill)
        .height(Fill)
        .width(Fill);
    Simulator::with_size(settings(), WINDOW, editor.boxed())
}

fn said(intent: Intent) -> Chat {
    Chat::InlineEdit(intent)
}

/// Where the box is, from the hint under it.
fn boxed(ui: &mut Ui<'_>) -> Rectangle {
    let hint = ui
        .find("escape to")
        .unwrap_or_else(|_| panic!("no hint"))
        .bounds();
    let bottom = hint.y - OVER_HINT;
    Rectangle::new(
        Point::new(hint.x, bottom - HEIGHT),
        Size::new(WINDOW.width - hint.x, HEIGHT),
    )
}

/// A click into the field, past the end of its text.
fn focus(ui: &mut Ui<'_>) {
    let field = boxed(ui);
    ui.point_at(Point::new(field.x + 500.0, field.center_y()));
    ui.simulate(simulator::click());
}

/// What `edit` asks for as `keys` are typed into its field.
fn keyed(edit: &InlineEdit, key: Named) -> Vec<Chat> {
    let mut ui = ui(edit);
    focus(&mut ui);
    let _clicked = ui.drain().count();
    ui.tap_key(key);
    asked(ui)
}

fn text(edit: &InlineEdit) -> &str {
    edit.field
        .as_deref()
        .map_or("", |field| field.text.as_str())
}

#[test]
fn typing_reports_the_text_and_caret_under_the_message_key() {
    let edit = sample::empty();
    let mut ui = ui(&edit);
    focus(&mut ui);
    ui.typewrite("a");
    assert!(asked(ui).contains(&said(Intent::Edited {
        key: sample::edited(),
        text: "a".into(),
        caret: 1,
    })));
}

#[test]
fn the_caret_moving_alone_is_reported() {
    let edit = sample::open();
    let end = text(&edit).len();
    assert!(
        keyed(&edit, Named::ArrowLeft).contains(&said(Intent::Caret {
            key: sample::edited(),
            caret: end - 1,
        }))
    );
}

#[test]
fn enter_saves() {
    assert!(keyed(&sample::open(), Named::Enter).contains(&said(Intent::Save)));
}

#[test]
fn escape_cancels() {
    assert!(keyed(&sample::open(), Named::Escape).contains(&said(Intent::Cancel)));
}

#[test]
fn escape_without_the_keys_cancels_nothing() {
    let edit = sample::open();
    let mut ui = ui(&edit);
    ui.tap_key(Named::Escape);
    assert!(!asked(ui).contains(&said(Intent::Cancel)));
}

#[test]
fn escape_closes_the_suggestions_before_it_cancels() {
    let asked = keyed(&sample::suggesting(), Named::Escape);
    assert!(asked.contains(&said(Intent::Suggest(Suggest::Dismiss))));
    assert!(!asked.contains(&said(Intent::Cancel)));
}

#[test]
fn the_suggestions_take_the_arrows_tab_and_enter() {
    let edit = sample::suggesting();
    assert!(keyed(&edit, Named::ArrowDown).contains(&said(Intent::Suggest(Suggest::Move(1)))));
    assert!(keyed(&edit, Named::ArrowUp).contains(&said(Intent::Suggest(Suggest::Move(-1)))));
    assert!(keyed(&edit, Named::Tab).contains(&said(Intent::Suggest(Suggest::Choose))));
    let entered = keyed(&edit, Named::Enter);
    assert!(entered.contains(&said(Intent::Suggest(Suggest::Choose))));
    assert!(!entered.contains(&said(Intent::Save)));
}

#[test]
fn a_click_on_a_suggestion_chooses_it() {
    let edit = sample::suggesting();
    let mut ui = ui(&edit);
    assert!(ui.find("Status Bot").is_ok(), "the app is offered");
    // The list floats up from a slot of no height over the box, its bottom
    // 8 px over it (6 px of padding, a 24 px heading, rows of 36), and the
    // simulator finds its text where it would be unfloated: click by place.
    let slot = boxed(&mut ui).y;
    let frame = 6.0 + 24.0 + 2.0 * 36.0 + 6.0;
    ui.point_at(Point::new(
        300.0,
        slot - 8.0 - frame + 6.0 + 24.0 + 36.0 + 18.0,
    ));
    ui.simulate(simulator::click());
    let asked = self::asked(ui);
    assert!(
        asked.contains(&said(Intent::Offered(suggestions::Intent::Choose(1)))),
        "{asked:?}"
    );
}

#[test]
fn cancel_and_save_are_clicked() {
    let edit = sample::open();
    let mut ui = ui(&edit);
    ui.click("cancel").expect("cancel");
    assert_eq!(asked(ui), vec![said(Intent::Cancel)]);
    let mut ui = self::ui(&edit);
    ui.click("save").expect("save");
    assert_eq!(asked(ui), vec![said(Intent::Save)]);
}

#[test]
fn the_emoji_button_opens_the_picker_under_its_right_edge() {
    let edit = sample::open();
    let mut ui = ui(&edit);
    let field = boxed(&mut ui);
    // 12 px in from the box's right edge, 14 over its bottom, 32 square.
    ui.point_at(Point::new(
        field.x + field.width - 12.0 - 16.0,
        field.y + field.height - 14.0 - 16.0,
    ));
    ui.simulate(simulator::click());
    let asked = asked(ui);
    let Some(spot) = asked.iter().find_map(|intent| match intent {
        Chat::InlineEdit(Intent::Emoji(spot)) => Some(spot),
        _ => None,
    }) else {
        panic!("no picker: {asked:?}");
    };
    assert_eq!(spot.align, anomi_vm::menu::Align::End);
    assert_eq!(spot.side, anomi_vm::menu::Side::Below);
    assert!(spot.anchor.x + spot.anchor.width <= field.x + field.width - 12.0 + 0.5);
    assert!(spot.anchor.y + spot.anchor.height <= field.y + field.height - 14.0 + 0.5);
}

#[test]
fn a_closed_edit_draws_nothing() {
    let closed = InlineEdit::default();
    assert!(!closed.open());
    let mut ui = ui(&closed);
    assert!(ui.find("escape to").is_err());
}

#[test]
fn the_body_and_its_edited_mark_hide_while_the_message_is_edited() {
    use iced::Widget as _;
    let show = |message: &anomi_vm::chat::message::Message, words: &str| {
        let mut ui: Ui<'_> =
            Simulator::with_size(settings(), WINDOW, message::view(message, front()).boxed());
        ui.find(words).is_ok()
    };
    let shown = sample::message(InlineEdit::default());
    assert!(show(&shown, "See you at nine"));
    assert!(show(&shown, "(edited)"));
    let edited = sample::message(sample::open());
    assert!(!show(&edited, "See you at nine"));
    assert!(!show(&edited, "(edited)"));
    assert!(show(&edited, "escape to"));
    let mut compact = edited.clone();
    compact.compact = true;
    assert!(!show(&compact, "See you at nine"));
    assert!(show(&compact, "Alice"));
}

#[test]
fn the_edited_row_ends_21_px_from_its_edge() {
    use iced::Widget as _;
    let edited = sample::message(sample::open());
    let mut ui: Ui<'_> =
        Simulator::with_size(settings(), WINDOW, message::view(&edited, front()).boxed());
    let field = boxed(&mut ui);
    // The emoji button, 12 px in from the box's right edge: 48 px from the
    // row's edge, where other rows end, it would be missed.
    ui.point_at(Point::new(
        WINDOW.width - inline_edit::END - 12.0 - 16.0,
        field.y + field.height - 14.0 - 16.0,
    ));
    ui.simulate(simulator::click());
    let asked = asked(ui);
    assert!(
        asked
            .iter()
            .any(|intent| matches!(intent, Chat::InlineEdit(Intent::Emoji(_)))),
        "{asked:?}"
    );
}

/// The timeline in a window as tall as the conversation's.
fn timeline_ui(timeline: &Timeline) -> Ui<'_> {
    use iced::Widget as _;
    Simulator::with_size(
        settings(),
        Size::new(1000.0, 700.0),
        timeline::view(timeline, front()).boxed(),
    )
}

/// The edit's text once the app took `typed` after it, and a message more
/// came in after the one edited.
fn answered(typed: &str) -> Timeline {
    let mut timeline = sample::timeline(InlineEdit::editing(sample::edited(), typed));
    let late = bobs(53, true);
    timeline.rows.push(anomi_vm::chat::timeline::Row {
        key: late.key,
        first: true,
        message: late,
        ..Default::default()
    });
    timeline
}

#[test]
fn the_editor_survives_the_list_building_its_row_again() {
    let first = sample::timeline(sample::open());
    let typed = format!("{}!", text(&sample::open()));
    let again = answered(&typed);
    let mut ui = timeline_ui(&first);
    focus(&mut ui);
    let _clicked = ui.drain().count();
    ui.typewrite("!");
    let asked: Vec<Chat> = ui.drain().collect();
    assert!(
        asked.iter().any(|intent| matches!(
            intent,
            Chat::InlineEdit(Intent::Edited { key, text, .. })
                if *key == sample::edited() && *text == typed
        )),
        "{asked:?}"
    );
    // The app answers: the row comes again with the text it took, among
    // rows that changed; the field keeps the keys and its caret.
    let mut ui = ui.rebuild(timeline::view(&again, front()));
    ui.typewrite("?");
    let asked = self::asked(ui);
    assert!(
        asked.iter().any(|intent| matches!(
            intent,
            Chat::InlineEdit(Intent::Edited { text, caret, .. })
                if *text == format!("{typed}?") && *caret == typed.len() + 1
        )),
        "{asked:?}"
    );
}

#[test]
fn the_timeline_keeps_the_edited_row_in_view() {
    // The edited row first of a long history, far above where the list
    // starts, at its end.
    let mut far = sample::timeline(sample::open());
    let edited = far.rows.pop().expect("the edited row");
    far.rows.insert(0, edited);
    let mut ui = timeline_ui(&far);
    assert!(ui.find("escape to").is_ok());
}
