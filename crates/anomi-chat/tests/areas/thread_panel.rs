//! A thread beside its channel: task C24's tests, driving
//! `anomi_chat::threads::panel::view` over its fixtures in the simulator
//! (`super::support`).

use iced::advanced::widget::Id as WidgetId;
use iced::keyboard::{self, key::Named};
use iced::{Point, Size};
use iced_test::Simulator;

use anomi_chat::threads::panel::{CLOSE_ID, EXPAND_ID, view};
use anomi_vm::chat::Intent as Chat;
use anomi_vm::chat::composer::Intent as Composer;
use anomi_vm::chat::thread_panel::{Intent, ThreadPanel};
use anomi_vm::chat::timeline::Intent as Timeline;
use anomi_vm::fixtures::chat::thread_panel as sample;

use super::support::{asked, front, settings};

const SIZE: Size = Size::new(420.0, 760.0);

type Sim<'a> = Simulator<'a, Chat, anomi_theme::Theme, anomi_theme::Renderer>;

fn simulate(vm: &ThreadPanel) -> Sim<'_> {
    Simulator::with_size(settings(), SIZE, view(vm, front()))
}

/// What the panel asked, its own intents as they are and what it wrapped
/// as the wrapped one.
fn panel(intents: Vec<Chat>) -> Vec<Intent> {
    intents
        .into_iter()
        .map(|intent| match intent {
            Chat::ThreadPanel(intent) => intent,
            other => panic!("not the panel's: {other:?}"),
        })
        .collect()
}

/// The composer's intents among what the panel wrapped.
fn composer(intents: Vec<Chat>) -> Vec<Composer> {
    panel(intents)
        .into_iter()
        .filter_map(|intent| match intent {
            Intent::Within(inner) => match *inner {
                Chat::Composer(intent) => Some(intent),
                _ => None,
            },
            other => panic!("not wrapped: {other:?}"),
        })
        .collect()
}

fn click(ui: &mut Sim<'_>, id: &'static str) {
    ui.click(WidgetId::new(id)).expect("the button");
}

#[test]
fn the_header_names_the_thread() {
    let vm = sample::post();
    let mut ui = simulate(&vm);
    assert!(ui.find("Dark theme for the sidebar").is_ok());
}

#[test]
fn the_cross_asks_to_close() {
    let vm = sample::post();
    let mut ui = simulate(&vm);
    click(&mut ui, CLOSE_ID);
    assert_eq!(panel(asked(ui)), [Intent::Close]);
}

#[test]
fn expand_asks_to_show_it_in_place_of_the_channel() {
    let vm = sample::post();
    let mut ui = simulate(&vm);
    click(&mut ui, EXPAND_ID);
    assert_eq!(panel(asked(ui)), [Intent::Expand]);
}

#[test]
fn more_asks_for_its_menu_from_where_it_sits() {
    let vm = sample::post();
    let mut ui = simulate(&vm);
    // It sits third from the right of the header, 28 px each, 8 apart and
    // 10 from the edge.
    ui.point_at(Point::new(
        SIZE.width - 10.0 - 28.0 - 8.0 - 28.0 - 8.0 - 14.0,
        24.0,
    ));
    ui.simulate(iced_test::simulator::click());
    let asked = panel(asked(ui));
    assert!(matches!(asked.as_slice(), [Intent::More(_)]), "{asked:?}");
}

#[test]
fn the_menu_leaves_the_thread_and_copies_its_link() {
    let rows = sample::post().menu_rows();
    let intents: Vec<_> = rows.iter().map(|row| row.intent.clone()).collect();
    let own = |intent: Intent| Some(anomi_vm::Intent::from(Chat::from(intent)));
    assert_eq!(intents, [own(Intent::Leave), own(Intent::CopyLink)]);
    assert_eq!(rows[0].text, "Leave thread");
    assert_eq!(rows[1].text, "Copy link");
}

#[test]
fn typing_asks_for_the_threads_draft_not_the_channels() {
    let vm = sample::post();
    let mut ui = simulate(&vm);
    ui.click(anomi_widgets::focus::ASIDE_COMPOSER)
        .expect("the field");
    let _ = ui.typewrite("hi");
    let asked = composer(asked(ui));
    assert!(
        matches!(asked.last(), Some(Composer::Edited { text, .. }) if text == "hi"),
        "{asked:?}"
    );
}

#[test]
fn enter_sends() {
    let vm = sample::post();
    let mut ui = simulate(&vm);
    ui.click(anomi_widgets::focus::ASIDE_COMPOSER)
        .expect("the field");
    ui.tap_key(keyboard::Key::Named(Named::Enter));
    assert!(composer(asked(ui)).contains(&Composer::Send));
}

#[test]
fn a_locked_thread_takes_no_text() {
    let vm = sample::locked();
    let mut ui = simulate(&vm);
    let _ = ui.click(anomi_widgets::focus::ASIDE_COMPOSER);
    let _ = ui.typewrite("hi");
    assert!(composer(asked(ui)).is_empty());
}

#[test]
fn the_plus_attaches_files() {
    let vm = sample::post();
    let mut ui = simulate(&vm);
    click(&mut ui, "thread-panel-attach");
    assert_eq!(composer(asked(ui)), [Composer::Attach]);
}

#[test]
fn the_reply_bar_cancels_and_toggles_the_ping() {
    let vm = sample::replying();
    let mut ui = simulate(&vm);
    click(&mut ui, "thread-panel-reply-ping");
    click(&mut ui, "thread-panel-cancel-reply");
    assert_eq!(
        composer(asked(ui)),
        [Composer::ToggleReplyPing, Composer::CancelReply]
    );
}

#[test]
fn earlier_messages_are_asked_for_in_the_thread() {
    let vm = sample::earlier();
    let mut ui = simulate(&vm);
    ui.click("Earlier messages").expect("the button");
    let asked: Vec<_> = panel(asked(ui))
        .into_iter()
        .filter_map(|intent| match intent {
            Intent::Within(inner) => match *inner {
                Chat::Timeline(intent) => Some(intent),
                _ => None,
            },
            _ => None,
        })
        .filter(|intent| !matches!(intent, Timeline::Shown(_)))
        .collect();
    assert_eq!(asked, [Timeline::Earlier]);
}

#[test]
fn an_empty_thread_says_where_it_starts() {
    let vm = sample::empty();
    let mut ui = simulate(&vm);
    assert!(
        ui.find("This is the start of the Dark theme for the sidebar thread.")
            .is_ok()
    );
}

#[test]
fn a_thread_still_loading_shows_no_start() {
    let vm = sample::loading();
    let mut ui = simulate(&vm);
    assert!(ui.find("Dark theme for the sidebar").is_ok());
    assert!(ui.find("Earlier messages").is_err());
}
