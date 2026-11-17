//! A new forum post: task C27's tests, driving
//! `anomi_chat::forum::new_post::view` over its fixtures in the simulator
//! (`super::support`).

use iced::Widget as _;
use iced::{Point, keyboard::key::Named};
use iced_test::Simulator;

use anomi_chat::forum::new_post::view;
use anomi_theme::{Renderer, Theme};
use anomi_vm::chat::new_post::{Intent as Ask, NewPost, Part};
use anomi_vm::chat::{self, Intent};
use anomi_vm::fixtures::chat::new_post as sample;

use super::support::{SIZE, asked, front, settings};

fn simulate(post: &NewPost) -> Simulator<'_, chat::Intent, Theme, Renderer> {
    Simulator::with_size(settings(), SIZE, view(post, front()).boxed())
}

fn ask(intent: Ask) -> Intent {
    Intent::NewPost(intent)
}

/// Where the blank form's controls sit, from its fixed heights: Add tags
/// under the tags' heading, the message's box, Publish at the end.
const ADD_TAGS: (f32, f32) = (90.0, 179.0);
const MESSAGE: (f32, f32) = (700.0, 326.0);
const PUBLISH: (f32, f32) = (1340.0, 381.0);

fn click_at(post: &NewPost, x: f32, y: f32) -> Vec<Intent> {
    let mut ui = simulate(post);
    ui.point_at(Point::new(x, y));
    ui.simulate(iced_test::simulator::click());
    asked(ui)
}

#[test]
fn the_form_shows_what_discords_does() {
    let post = sample::blank();
    let mut ui = simulate(&post);
    for shown in [
        "New Post",
        "POST TITLE",
        "TAGS",
        "Post Guidelines",
        "One idea per post. Search before posting.",
    ] {
        assert!(ui.find(shown).is_ok(), "{shown} is not shown");
    }
}

#[test]
fn a_forum_that_requires_a_tag_says_so() {
    let post = sample::required();
    let mut ui = simulate(&post);
    assert!(ui.find("TAGS (REQUIRED)").is_ok());
    // The guidelines are folded to their heading.
    assert!(ui.find("Post Guidelines").is_ok());
    assert!(
        ui.find("One idea per post. Search before posting.")
            .is_err()
    );
}

#[test]
fn a_forum_without_tags_or_guidelines_has_neither() {
    let post = sample::plain();
    let mut ui = simulate(&post);
    assert!(ui.find("TAGS").is_err());
    assert!(ui.find("Post Guidelines").is_err());
    assert!(ui.find("POST TITLE").is_ok());
}

#[test]
fn typing_in_the_title_sends_the_text_and_enter_moves_to_the_message() {
    let post = sample::blank();
    let mut ui = simulate(&post);
    ui.click("Title").unwrap();
    ui.typewrite("b");
    assert_eq!(
        asked(ui),
        [
            ask(Ask::Focus(Part::Title, true)),
            ask(Ask::Title("b".into()))
        ]
    );

    let mut ui = simulate(&post);
    ui.click("Title").unwrap();
    ui.tap_key(Named::Enter);
    assert!(asked(ui).contains(&ask(Ask::ToBody)));
}

#[test]
fn typing_in_the_message_sends_the_text_and_enter_posts() {
    let post = sample::blank();
    let mut ui = simulate(&post);
    ui.point_at(Point::new(MESSAGE.0, MESSAGE.1));
    ui.simulate(iced_test::simulator::click());
    ui.typewrite("h");
    assert!(asked(ui).contains(&ask(Ask::Body("h".into()))));

    let post = sample::written();
    let mut ui = simulate(&post);
    ui.click("Pasting a GIF from the browser closes the app.")
        .unwrap();
    ui.tap_key(Named::Enter);
    assert!(asked(ui).contains(&ask(Ask::Create)));
}

#[test]
fn the_fields_report_the_keys_they_take() {
    let post = sample::blank();
    assert!(click_at(&post, MESSAGE.0, MESSAGE.1).contains(&ask(Ask::Focus(Part::Body, true))));
}

#[test]
fn add_tags_opens_the_menu() {
    let post = sample::blank();
    assert_eq!(
        click_at(&post, ADD_TAGS.0, ADD_TAGS.1),
        [ask(Ask::Tags(true))]
    );
}

#[test]
fn the_menu_lists_the_tags_and_a_row_toggles_its_tag() {
    let post = sample::tags_menu();
    let mut ui = simulate(&post);
    for shown in ["Select up to 5 tags", "Feature", "Bug", "Question"] {
        assert!(ui.find(shown).is_ok(), "{shown} is not shown");
    }
    ui.click("Bug").unwrap();
    assert_eq!(asked(ui), [ask(Ask::ToggleTag("t2".into()))]);
}

#[test]
fn a_chip_takes_its_tag_off() {
    let post = sample::written();
    let mut ui = simulate(&post);
    ui.click("Question").unwrap();
    assert_eq!(asked(ui), [ask(Ask::ToggleTag("t3".into()))]);
}

#[test]
fn with_five_tags_the_rest_cannot_be_taken() {
    let mut post = sample::all_tags();
    post.tags_open = true;
    let mut ui = simulate(&post);
    // The unticked row is off.
    ui.click("Wontfix").unwrap();
    assert_eq!(asked(ui), []);
}

#[test]
fn the_guidelines_chevron_folds_the_card() {
    // The chevron sits at the card's right end, 12 px in.
    let post = sample::blank();
    let asks = click_at(&post, SIZE.width - 40.0, 231.0);
    assert_eq!(asks, [ask(Ask::ToggleGuidelines)]);
}

#[test]
fn publish_posts_and_does_nothing_while_it_goes() {
    let post = sample::blank();
    assert_eq!(click_at(&post, PUBLISH.0, PUBLISH.1), [ask(Ask::Create)]);

    let post = NewPost {
        posting: true,
        ..sample::blank()
    };
    assert_eq!(click_at(&post, PUBLISH.0, PUBLISH.1), []);
    assert_eq!(click_at(&post, ADD_TAGS.0, ADD_TAGS.1), []);
}

#[test]
fn nothing_can_be_edited_while_it_posts() {
    let post = sample::posting();
    let mut ui = simulate(&post);
    ui.click("Crash when pasting a GIF").unwrap();
    ui.typewrite("x");
    ui.click("Question").unwrap();
    assert!(
        !asked(ui)
            .iter()
            .any(|intent| matches!(intent, Intent::NewPost(Ask::Title(_) | Ask::ToggleTag(_))))
    );
}

#[test]
fn the_cross_closes_the_form() {
    let post = sample::written();
    let asks = click_at(&post, SIZE.width - 24.0, 24.0);
    assert_eq!(asks, [ask(Ask::Close)]);
}

#[test]
fn a_refusal_is_said_in_the_form() {
    let post = sample::failed();
    let mut ui = simulate(&post);
    assert!(ui.find("Missing Access").is_ok());
    let post = sample::written();
    let mut ui = simulate(&post);
    assert!(ui.find("Missing Access").is_err());
}

#[test]
fn the_files_that_go_with_the_post_are_shown() {
    let post = sample::written();
    let mut ui = simulate(&post);
    assert!(ui.find("crash.png").is_ok());
    assert!(ui.find("7 KB").is_ok());
    let post = sample::blank();
    let mut ui = simulate(&post);
    assert!(ui.find("crash.png").is_err());
}

#[test]
fn leaving_asks_and_either_answer_is_sent() {
    let post = sample::leaving();
    let mut ui = simulate(&post);
    for shown in [
        "Are you sure you want to go?",
        "Any changes you made will be lost if you exit now.",
    ] {
        assert!(ui.find(shown).is_ok(), "{shown} is not shown");
    }
    // The buttons are under the question's text, at the card's right.
    let text = ui
        .find("Any changes you made will be lost if you exit now.")
        .unwrap()
        .visible_bounds()
        .unwrap();
    let (right, y) = (
        text.x + 440.0 - 16.0,
        text.y + text.height + 10.0 + 6.0 + 18.0,
    );
    assert_eq!(click_at(&post, right - 20.0, y), [ask(Ask::Leave)]);
    assert_eq!(click_at(&post, right - 100.0, y), [ask(Ask::Stay)]);

    let mut ui = simulate(&post);
    ui.tap_key(Named::Escape);
    assert_eq!(asked(ui), [ask(Ask::Stay)]);
}

#[test]
fn the_question_takes_the_clicks_of_the_form_under_it() {
    let post = sample::leaving();
    // Publish is under the dimmed backdrop: a press there is outside the
    // question.
    assert!(!click_at(&post, PUBLISH.0, PUBLISH.1).contains(&ask(Ask::Create)));
}
