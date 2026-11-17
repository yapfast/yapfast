//! The composer's frame: task C14's tests, driving
//! `anomi_chat::composer::view` over its fixtures in the simulator
//! (`super::support`).
//!
//! The frame sits at the bottom of a window 800 wide and 300 tall, as it
//! does in the conversation column; its pieces are found by where they are:
//! the box ends 8 px above the bottom, is 58 tall with nothing above its
//! field, the plus is 32 px wide 10 px in, and the three buttons after the
//! field end 8 px before the box does.

mod keys;

use iced::Widget as _;
use iced::widget::container;
use iced::{Fill, Point, Size};
use iced_test::{Simulator, simulator};

use anomi_chat::composer::view;
use anomi_theme::{Renderer, Theme};
use anomi_vm::chat::composer::{Composer, Expression, Intent, Suggest};
use anomi_vm::chat::{self, Intent as Chat};
use anomi_vm::fixtures::chat::composer as sample;

use super::support::{asked, front, settings};

const WINDOW: Size = Size::new(800.0, 300.0);
/// The box's bottom edge: 8 px above the window's.
const BOTTOM: f32 = 292.0;

/// The frame at the window's bottom, as the conversation column holds it.
fn frame(composer: &Composer) -> iced::Element<'_, Chat, Theme, Renderer> {
    container(view(composer, front()).boxed())
        .align_bottom(Fill)
        .height(Fill)
        .width(Fill)
        .boxed()
}

fn ui(composer: &Composer) -> Simulator<'_, Chat, Theme, Renderer> {
    Simulator::with_size(settings(), WINDOW, frame(composer))
}

fn click(composer: &Composer, at: Point) -> Vec<Chat> {
    let mut ui = ui(composer);
    ui.point_at(at);
    ui.simulate(simulator::click());
    asked(ui)
}

fn said(intent: Intent) -> Vec<Chat> {
    vec![Chat::Composer(intent)]
}

/// The centre of a button on the field's last line, `from_right` px from the
/// box's right edge to its far side.
fn button(from_right: f32) -> Point {
    Point::new(WINDOW.width - 8.0 - from_right - 16.0, BOTTOM - 14.0 - 16.0)
}

/// Where the box's top is with `above` px over the field's row.
fn top(above: f32) -> f32 {
    BOTTOM - 58.0 - above
}

#[test]
fn the_plus_opens_the_file_picker_where_it_has_no_menu() {
    assert_eq!(
        click(&sample::dm(), Point::new(34.0, BOTTOM - 30.0)),
        said(Intent::Attach)
    );
}

#[test]
fn the_plus_opens_its_menu_above_itself_where_it_has_one() {
    let sent = click(&sample::channel(), Point::new(34.0, BOTTOM - 30.0));
    let [Chat::Composer(Intent::More(spot))] = sent.as_slice() else {
        panic!("the plus asked {sent:?}");
    };
    assert_eq!(spot.side, anomi_vm::menu::Side::Above);
    // `root.menu-y - self.height - 8px`.
    assert_eq!(spot.gap, 8.0);
    assert!(spot.anchor.contains(Point::new(34.0, BOTTOM - 30.0)));
}

#[test]
fn a_plus_with_nothing_to_do_does_nothing() {
    let mut off = sample::dm();
    off.more = Default::default();
    assert!(click(&off, Point::new(34.0, BOTTOM - 30.0)).is_empty());
}

#[test]
fn the_buttons_after_the_field_open_their_pickers() {
    let dm = sample::dm();
    assert_eq!(
        click(&dm, button(0.0)),
        said(Intent::Expressions(Expression::Emoji))
    );
    assert_eq!(
        click(&dm, button(38.0)),
        said(Intent::Expressions(Expression::Stickers))
    );
    assert_eq!(
        click(&dm, button(76.0)),
        said(Intent::Expressions(Expression::Gifs))
    );
}

#[test]
fn a_room_without_gifs_or_stickers_has_the_emoji_alone() {
    let mut plain = sample::dm();
    plain.gifs = false;
    plain.stickers = false;
    assert_eq!(
        click(&plain, button(0.0)),
        said(Intent::Expressions(Expression::Emoji))
    );
    // Where the stickers' button was is the field's now.
    assert!(
        !click(&plain, button(38.0))
            .contains(&Chat::Composer(Intent::Expressions(Expression::Stickers)))
    );
}

#[test]
fn the_buttons_are_off_while_the_field_is() {
    let mut off = sample::dm();
    off.enabled = false;
    assert!(click(&off, button(0.0)).is_empty());
}

#[test]
fn typing_reports_the_draft_with_its_room_and_enter_sends() {
    let composer = sample::dm();
    let mut ui = ui(&composer);
    ui.point_at(Point::new(300.0, BOTTOM - 30.0));
    ui.simulate(simulator::click());
    ui.typewrite("hi");
    ui.tap_key(iced::keyboard::Key::Named(
        iced::keyboard::key::Named::Enter,
    ));
    let sent = asked(ui);
    assert!(sent.contains(&Chat::Composer(Intent::Focus(true))));
    assert!(sent.iter().any(|intent| matches!(
        intent,
        Chat::Composer(Intent::Edited { room: 1, text, .. }) if text == "hi"
    )));
    assert_eq!(sent.last(), Some(&Chat::Composer(Intent::Send)));
}

#[test]
fn a_field_that_takes_nothing_reports_nothing() {
    let mut off = sample::dm();
    off.enabled = false;
    let mut ui = ui(&off);
    ui.point_at(Point::new(300.0, BOTTOM - 30.0));
    ui.simulate(simulator::click());
    ui.typewrite("hi");
    assert!(asked(ui).is_empty());
}

#[test]
fn keys_over_suggestions_are_the_suggestions() {
    let mut writing = sample::dm();
    writing.suggesting = true;
    let mut ui = ui(&writing);
    ui.point_at(Point::new(300.0, BOTTOM - 30.0));
    ui.simulate(simulator::click());
    ui.tap_key(iced::keyboard::Key::Named(
        iced::keyboard::key::Named::ArrowDown,
    ));
    ui.tap_key(iced::keyboard::Key::Named(iced::keyboard::key::Named::Tab));
    let sent = asked(ui);
    assert!(sent.contains(&Chat::Composer(Intent::Suggest(Suggest::Move(1)))));
    assert!(sent.contains(&Chat::Composer(Intent::Suggest(Suggest::Choose))));
}

#[test]
fn the_reply_bar_closes_and_toggles_its_ping() {
    let replying = sample::replying();
    // The bar is 36 tall on the box's top; its close is 28 wide, 4 from the
    // right edge, and the toggle sits before it.
    let middle = top(0.0) - 18.0;
    assert_eq!(
        click(
            &replying,
            Point::new(WINDOW.width - 8.0 - 4.0 - 14.0, middle)
        ),
        said(Intent::CancelReply)
    );
    assert_eq!(
        click(
            &replying,
            Point::new(WINDOW.width - 8.0 - 4.0 - 28.0 - 8.0 - 20.0, middle)
        ),
        said(Intent::ToggleReplyPing)
    );
}

#[test]
fn a_staged_file_comes_out_and_is_covered() {
    let composer = sample::staged_and_mentioning();
    // The row is 152 tall over the field's row; the first tile is 10 px in,
    // 120 wide, and its discs sit 10 px from its top and right.
    let disc = top(152.0) + 10.0 + 10.0 + 12.0;
    let right = 8.0 + 1.0 + 10.0 + 120.0 - 10.0 - 12.0;
    assert_eq!(
        click(&composer, Point::new(right, disc)),
        said(Intent::Unstage(0))
    );
    assert_eq!(
        click(&composer, Point::new(right - 28.0, disc)),
        said(Intent::ToggleSpoiler(0))
    );
}

#[test]
fn a_room_without_spoilers_has_no_cover_button() {
    let mut composer = sample::staged_and_mentioning();
    composer.can_spoil = false;
    let disc = top(152.0) + 10.0 + 10.0 + 12.0;
    let right = 8.0 + 1.0 + 10.0 + 120.0 - 10.0 - 12.0;
    assert!(click(&composer, Point::new(right - 28.0, disc)).is_empty());
}

#[test]
fn a_message_request_is_answered_in_place_of_the_field() {
    let request = sample::request();
    let middle = BOTTOM - 33.0;
    assert_eq!(
        click(
            &request,
            Point::new(WINDOW.width - 8.0 - 12.0 - 43.0, middle)
        ),
        said(Intent::Accept)
    );
    assert_eq!(
        click(
            &request,
            Point::new(WINDOW.width - 8.0 - 12.0 - 86.0 - 10.0 - 40.0, middle)
        ),
        said(Intent::Ignore)
    );
    // No plus, no field.
    assert!(click(&request, Point::new(34.0, BOTTOM - 30.0)).is_empty());
}

#[test]
fn a_locked_room_has_one_bar_and_no_buttons() {
    let locked = sample::locked();
    assert!(click(&locked, Point::new(34.0, BOTTOM - 30.0)).is_empty());
    assert!(click(&locked, button(0.0)).is_empty());
}

#[test]
fn the_frame_draws_in_every_state() {
    let slow = Composer {
        slow: Some(12),
        ..sample::channel()
    };
    for composer in [
        sample::dm(),
        sample::channel(),
        sample::replying(),
        sample::staged_and_mentioning(),
        sample::request(),
        sample::locked(),
        sample::typing(),
        slow,
    ] {
        let mut ui = ui(&composer);
        assert!(ui.snapshot(&Theme::default()).is_ok());
    }
}

#[test]
fn the_composer_intent_reaches_the_window() {
    let intent: anomi_vm::Intent = Intent::Send.into();
    assert_eq!(
        intent,
        anomi_vm::Intent::Chat(chat::Intent::Composer(Intent::Send))
    );
}

#[test]
fn the_plus_menu_lists_discords_rows_each_sending_its_own() {
    let vm = sample::plus_menu();
    let sent: Vec<_> = vm
        .menu
        .rows
        .iter()
        .filter_map(|row| row.intent.clone())
        .collect();
    assert_eq!(
        sent,
        [Intent::Attach, Intent::StartThread, Intent::StartPoll].map(anomi_vm::Intent::from)
    );
}

/// Where a click on the plus lands with the frame at the window's top: its
/// height decides, the box being 8 px off the bottom of it.
fn plus_at_top(composer: &Composer) -> Vec<Chat> {
    let frame = container(view(composer, front()).boxed())
        .align_top(Fill)
        .height(Fill)
        .width(Fill);
    let mut ui = Simulator::with_size(settings(), WINDOW, frame);
    // 58 tall box, 8 under it; the plus is 32 tall, 14 above the box's bottom.
    ui.point_at(Point::new(34.0, 58.0 - 14.0 - 16.0));
    ui.simulate(simulator::click());
    asked(ui)
}

#[test]
fn who_is_typing_takes_no_room_from_the_timeline() {
    let quiet = sample::dm();
    let typing = Composer {
        typing: "Bob is typing...".into(),
        ..sample::dm()
    };
    assert_eq!(plus_at_top(&quiet), said(Intent::Attach));
    assert_eq!(plus_at_top(&typing), plus_at_top(&quiet));
}

#[test]
fn the_plus_menu_has_the_rows_the_room_allows() {
    let all = sample::channel().more.rows();
    assert_eq!(all.len(), 3);
    assert!(!all[0].disabled);
    let no_polls = anomi_vm::chat::composer::More {
        polls: false,
        ..sample::channel().more
    };
    let texts: Vec<_> = no_polls.rows().into_iter().map(|row| row.text).collect();
    assert_eq!(texts.len(), 2);
    assert_eq!(texts[0], yapfast_lang::tr!("Upload a File"));
    assert_eq!(texts[1], yapfast_lang::tr!("Create Thread"));
    // Without `can-attach` the upload row is there but off.
    let no_files = anomi_vm::chat::composer::More {
        attach: false,
        ..sample::channel().more
    };
    assert!(no_files.rows()[0].disabled);
}
