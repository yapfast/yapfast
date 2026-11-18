//! A message: task C02's tests, driving `anomi_chat::message::view` over its
//! fixtures in the simulator (`super::support`).

use iced::{Event, Point, Rectangle, Size, mouse};
use iced_test::Simulator;
use iced_test::selector::Candidate;

use anomi_chat::message;
use anomi_theme::{Renderer, Theme};
use anomi_vm::chat::{
    self,
    message::{Intent, Message, Who},
};
use anomi_vm::fixtures::chat::message as fixtures;
use anomi_vm::text::Link;

use super::support::{asked, front, settings};

const SIZE: Size = Size::new(760.0, 420.0);

fn simulate(message: &Message) -> Simulator<'_, chat::Intent, Theme, Renderer> {
    use iced::Widget as _;
    Simulator::with_size(settings(), SIZE, message::view(message, front()).boxed())
}

fn ask(intent: Intent) -> chat::Intent {
    chat::Intent::Message(intent)
}

/// What clicking `text` on `message` asks for.
fn click(message: &Message, text: &str) -> Vec<chat::Intent> {
    let mut ui = simulate(message);
    ui.click(text).unwrap_or_else(|_| panic!("no {text:?}"));
    asked(ui)
}

/// What a press at a point found from a text of `message` asks for: the
/// buttons that draw their own text and the paragraph's links aren't
/// found by their words.
fn press(message: &Message, at: impl Fn(&mut Ui<'_>) -> Point) -> Vec<chat::Intent> {
    let mut ui = simulate(message);
    let position = at(&mut ui);
    point(&mut ui, position);
    let _moved = ui.drain().count();
    ui.simulate(iced_test::simulator::click());
    asked(ui)
}

/// The pointer moved to `position`.
fn point(ui: &mut Ui<'_>, position: Point) {
    ui.point_at(position);
    ui.simulate([Event::Mouse(mouse::Event::CursorMoved { position })]);
}

type Ui<'a> = Simulator<'a, chat::Intent, Theme, Renderer>;

/// Where `text` is.
fn bounds(ui: &mut Ui<'_>, text: &str) -> Rectangle {
    ui.find(text)
        .unwrap_or_else(|_| panic!("no {text:?}"))
        .bounds()
}

/// Where the paragraph block starting with `start` is.
fn block(ui: &mut Ui<'_>, start: &str) -> Rectangle {
    ui.find(|candidate: Candidate<'_>| match candidate {
        Candidate::Text {
            content, bounds, ..
        } if content.starts_with(start) => Some(bounds),
        _ => None,
    })
    .unwrap_or_else(|_| panic!("no block starting {start:?}"))
}

#[test]
fn the_pointer_lights_a_message() {
    let message = fixtures::bobs(40, true);
    let mut ui = simulate(&message);
    point(&mut ui, Point::new(300.0, 30.0));
    assert_eq!(asked(ui), [ask(Intent::Point(message.key))]);
}

#[test]
fn a_lit_message_asks_nothing_as_the_pointer_comes() {
    let message = fixtures::lit(fixtures::bobs(40, true));
    let mut ui = simulate(&message);
    point(&mut ui, Point::new(300.0, 30.0));
    assert!(asked(ui).is_empty());
}

#[test]
fn a_frequent_reaction_reacts() {
    let message = fixtures::lit(fixtures::bobs(46, true));
    assert_eq!(
        click(&message, "👍"),
        [ask(Intent::React(message.key, "👍".into()))]
    );
}

#[test]
fn show_more_unfolds_and_show_less_folds() {
    // The paragraph lays its toggle out under the folded body.
    let toggle = |label: &'static str| move |ui: &mut Ui<'_>| bounds(ui, label).center();
    let folded = fixtures::long(false);
    assert_eq!(
        press(&folded, toggle("Show more")),
        [ask(Intent::Fold(folded.key, true))]
    );
    let open = fixtures::long(true);
    assert_eq!(
        press(&open, toggle("Show less")),
        [ask(Intent::Fold(open.key, false))]
    );
}

#[test]
fn a_message_that_did_not_go_retries_or_gives_up() {
    // Retry 4 px after what happened, Cancel 4 px after Retry.
    let after = |by: f32| {
        move |ui: &mut Ui<'_>| {
            let note = bounds(ui, "Not sent. The server refused this message.");
            Point::new(note.x + note.width + by, note.center_y())
        }
    };
    let failed = fixtures::failed();
    assert_eq!(
        press(&failed, after(14.0)),
        [ask(Intent::Retry(failed.key))]
    );
    assert_eq!(
        press(&failed, after(80.0)),
        [ask(Intent::CancelSend(failed.key))]
    );
}

#[test]
fn an_answer_only_this_account_sees_is_dismissed() {
    let answer = fixtures::command();
    assert_eq!(
        click(&answer, "Dismiss message"),
        [ask(Intent::Dismiss(answer.key))]
    );
    let preview = Message {
        preview: true,
        ..fixtures::command()
    };
    assert!(click(&preview, "Dismiss message").is_empty());
}

#[test]
fn the_command_goes_in_the_composer() {
    let answer = fixtures::command();
    assert_eq!(
        click(&answer, "/status"),
        [ask(Intent::UseCommand(answer.key))]
    );
}

#[test]
fn who_used_the_command_opens_their_card() {
    let answer = fixtures::command();
    let asked = click(&answer, "Alice");
    assert!(
        matches!(
            asked.as_slice(),
            [chat::Intent::Message(Intent::Card(key, Who::Commander, _))] if *key == answer.key
        ),
        "{asked:?}"
    );
}

#[test]
fn the_quote_jumps_to_the_message_it_quotes() {
    let reply = fixtures::reply();
    assert_eq!(
        click(&reply, "Welcome to Yapfast, with code and a link"),
        [ask(Intent::Quoted(reply.key))]
    );
}

#[test]
fn the_thread_opens() {
    let message = fixtures::with_thread();
    assert_eq!(click(&message, "Plans"), [ask(Intent::Thread(message.key))]);
}

#[test]
fn deleting_asks_first() {
    let mut message = fixtures::lit(fixtures::bobs(46, false));
    if let Some(bar) = &mut message.bar {
        bar.confirming = true;
    }
    // Delete 4 px after the question; Cancel last, its end 4 px in from
    // the bar's, which ends 16 px in from the row's.
    let question = |ui: &mut Ui<'_>| bounds(ui, "Delete for everyone?");
    let delete = move |ui: &mut Ui<'_>| {
        let question = question(ui);
        Point::new(question.x + question.width + 14.0, question.center_y())
    };
    let cancel = move |ui: &mut Ui<'_>| Point::new(SIZE.width - 32.0, question(ui).center_y());
    assert_eq!(
        press(&message, delete),
        [ask(Intent::ConfirmDelete(message.key))]
    );
    assert_eq!(
        press(&message, cancel),
        [ask(Intent::CancelDelete(message.key))]
    );
}

#[test]
fn a_blocked_message_shows_when_asked() {
    // "Show it" after the italic "Blocked message.", some 120 px in.
    let show = |ui: &mut Ui<'_>| {
        let body = block(ui, "Blocked message.");
        Point::new(body.x + 145.0, body.center_y())
    };
    let blocked = fixtures::blocked();
    assert_eq!(
        press(&blocked, show),
        [ask(Intent::Link(
            blocked.key,
            Link::Url("yapfast:blocked/blocked".into())
        ))]
    );
}

#[test]
fn a_deleted_message_has_no_bar() {
    let mut message = fixtures::lit(fixtures::bobs(46, false));
    message.deleted = true;
    let mut ui = simulate(&message);
    assert!(ui.find("👍").is_err());
}

#[test]
fn every_kind_draws() {
    for row in fixtures::kinds()
        .rows
        .into_iter()
        .chain(fixtures::kinds_compact().rows)
    {
        let mut ui = simulate(&row.message);
        ui.draw(&Theme::default());
    }
}

/// What a press of `button` at `at` on `message` asks for.
fn pressed(message: &Message, at: Point, button: mouse::Button) -> Vec<chat::Intent> {
    let mut ui = simulate(message);
    point(&mut ui, at);
    let _moved = ui.drain().count();
    ui.simulate([
        Event::Mouse(mouse::Event::ButtonPressed(button)),
        Event::Mouse(mouse::Event::ButtonReleased(button)),
    ]);
    asked(ui)
}

/// The bar of a message at the window's top keeps to its top, its buttons
/// 32 px from its right end, 16 px in from the row's and 4 px round them.
fn bar_button(from_right: usize) -> Point {
    Point::new(SIZE.width - 20.0 - 16.0 - 32.0 * from_right as f32, 19.0)
}

/// The intent of a press, without where it opens.
fn of(asked: &[chat::Intent]) -> Option<&Intent> {
    match asked {
        [chat::Intent::Message(intent)] => Some(intent),
        _ => None,
    }
}

#[test]
fn the_bar_offers_more_forward_reply_and_the_picker() {
    let message = fixtures::lit(fixtures::bobs(46, false));
    let key = message.key;
    let press = |from_right| pressed(&message, bar_button(from_right), mouse::Button::Left);
    assert!(matches!(of(&press(0)), Some(Intent::Menu(k, _)) if *k == key));
    assert_eq!(press(1), [ask(Intent::Forward(key))]);
    assert_eq!(press(2), [ask(Intent::Reply(key))]);
    assert!(matches!(of(&press(3)), Some(Intent::Picker(k, _)) if *k == key));
}

#[test]
fn shift_offers_pin_a_thread_and_delete_at_once() {
    let mut message = fixtures::lit(fixtures::bobs(46, false));
    if let Some(bar) = &mut message.bar {
        bar.shift = true;
        bar.may.edit = true;
        bar.may.delete = true;
    }
    let key = message.key;
    let press = |from_right| pressed(&message, bar_button(from_right), mouse::Button::Left);
    assert_eq!(press(0), [ask(Intent::DeleteNow(key))]);
    assert_eq!(press(2), [ask(Intent::Edit(key))]);
    assert_eq!(press(4), [ask(Intent::Reply(key))]);
    assert_eq!(press(5), [ask(Intent::NewThread(key))]);
    assert_eq!(press(6), [ask(Intent::Pin(key))]);
}

#[test]
fn the_author_opens_their_card_and_their_menu() {
    let message = fixtures::bobs(40, true);
    let key = message.key;
    let picture = Point::new(36.0, 26.0);
    let card = pressed(&message, picture, mouse::Button::Left);
    assert!(matches!(of(&card), Some(Intent::Card(k, Who::Author, _)) if *k == key));
    let name = Point::new(80.0, 12.0);
    let card = pressed(&message, name, mouse::Button::Left);
    assert!(matches!(of(&card), Some(Intent::Card(k, Who::Author, _)) if *k == key));
    let menu = pressed(&message, picture, mouse::Button::Right);
    assert!(matches!(of(&menu), Some(Intent::AuthorMenu(k, _)) if *k == key));
}

#[test]
fn a_right_click_opens_the_message_menu() {
    let message = fixtures::bobs(40, true);
    // On its text, beside its name and time, and in the room its end keeps
    // for the hover bar.
    for at in [(400.0, 34.0), (300.0, 12.0), (SIZE.width - 20.0, 34.0)] {
        let asked = pressed(&message, Point::new(at.0, at.1), mouse::Button::Right);
        assert!(
            matches!(of(&asked), Some(Intent::Menu(k, _)) if *k == message.key),
            "{at:?}: {asked:?}"
        );
    }
    let mut gone = fixtures::bobs(40, true);
    gone.deleted = true;
    assert!(pressed(&gone, Point::new(400.0, 34.0), mouse::Button::Right).is_empty());
}

#[test]
fn the_tag_opens_its_servers_card() {
    let message = fixtures::bobs(40, true);
    let asked = click(&message, "BOB");
    assert!(matches!(of(&asked), Some(Intent::TagCard(k, _)) if *k == message.key));
}
