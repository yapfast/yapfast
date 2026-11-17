//! A server's channel list: task N04's tests, driving
//! `anomi_screens::sidebar::channels::view` over its fixtures in the
//! simulator (`super::support`), the list filling the window from its top
//! left: the "Channels" heading 0-40 px down, #general 40-72, its thread
//! Plans 74-106, ideas 108-140, the Lounge 142-174 and its people 30 px
//! each from 176, then Create Room.

use iced::advanced::mouse;
use iced::advanced::widget::Operation;
use iced::advanced::widget::operation::Focusable;
use iced::keyboard::key::Named;
use iced::{Event, Point, keyboard};
use iced_test::Simulator;

use anomi_screens::sidebar::channels::view;
use anomi_theme::{Renderer, Theme};
use anomi_vm::fixtures::sidebar::channels::{
    self, CATEGORY, CREATE_ROOM, GENERAL, IDEAS, LOUNGE, bob, carol,
};
use anomi_vm::menu::Spot;
use anomi_vm::sidebar::channels::{Arrange, Channels, Intent, Move};

use super::support::{SIZE, asked, front, settings};

/// The middle of each row, down the list.
const IDEAS_Y: f32 = 124.0;
const LOUNGE_Y: f32 = 158.0;
const CAROL_Y: f32 = 221.0;
const CREATE_ROOM_Y: f32 = 288.0;

fn simulate(channels: &Channels) -> Simulator<'_, Intent, Theme, Renderer> {
    Simulator::with_size(settings(), SIZE, view(channels, front()))
}

/// Focuses the `.0`th of what may take the focus, as Tab's walk would.
struct Nth(usize, usize);

impl Operation for Nth {
    fn focusable(
        &mut self,
        _id: Option<&iced::advanced::widget::Id>,
        _bounds: iced::Rectangle,
        state: &mut dyn Focusable,
    ) {
        if self.1 == self.0 {
            state.focus();
        } else {
            state.unfocus();
        }
        self.1 += 1;
    }

    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }
}

fn moved(position: Point) -> Event {
    Event::Mouse(mouse::Event::CursorMoved { position })
}

fn press(button: mouse::Button) -> Event {
    Event::Mouse(mouse::Event::ButtonPressed(button))
}

fn release(button: mouse::Button) -> Event {
    Event::Mouse(mouse::Event::ButtonReleased(button))
}

#[test]
fn clicking_a_channel_asks_to_open_it() {
    let list = channels::channels();
    let mut ui = simulate(&list);
    ui.click("ideas").expect("the forum's row");
    assert_eq!(asked(ui), [Intent::Open(IDEAS.into())]);
}

#[test]
fn clicking_a_heading_folds_its_category() {
    let list = channels::channels();
    let mut ui = simulate(&list);
    ui.click("Channels").expect("the category's heading");
    assert_eq!(asked(ui), [Intent::Fold(CATEGORY.into())]);
}

#[test]
fn a_right_click_opens_the_channels_menu_at_the_pointer() {
    let list = channels::channels();
    let mut ui = simulate(&list);
    let at = Point::new(120.0, IDEAS_Y);
    ui.point_at(at);
    let _ = ui.simulate([press(mouse::Button::Right), release(mouse::Button::Right)]);
    assert_eq!(asked(ui), [Intent::Menu(IDEAS.into(), Spot::pointer(at))]);
}

#[test]
fn a_right_click_on_a_heading_opens_the_categorys_menu() {
    let list = channels::channels();
    let mut ui = simulate(&list);
    let at = Point::new(40.0, 28.0);
    ui.point_at(at);
    let _ = ui.simulate([press(mouse::Button::Right)]);
    assert_eq!(
        asked(ui),
        [Intent::CategoryMenu(GENERAL.into(), Spot::pointer(at))]
    );
}

#[test]
fn clicking_someone_in_a_voice_channel_opens_their_profile_beside_them() {
    let list = channels::channels();
    let mut ui = simulate(&list);
    ui.click("Carol").expect("Carol under the Lounge");
    let asked = asked(ui);
    let [Intent::Profile { channel, user, at }] = asked.as_slice() else {
        panic!("{asked:?}");
    };
    assert_eq!((channel.as_str(), user), (LOUNGE, &carol().user));
    assert_eq!(at.anchor.x, 40.0, "their row starts 40 px in");
    assert_eq!(at.anchor.height, 30.0);
    assert_eq!(*at, Spot::beside(at.anchor));
}

#[test]
fn a_live_tag_watches_the_stream() {
    let list = channels::channels();
    let mut ui = simulate(&list);
    ui.click("LIVE").expect("Bob's LIVE tag");
    assert_eq!(
        asked(ui),
        [Intent::Watch {
            channel: LOUNGE.into(),
            person: bob().id,
        }]
    );
}

#[test]
fn a_voice_channels_chat_opens_from_its_button_under_the_pointer() {
    let list = channels::channels();
    let mut ui = simulate(&list);
    let lounge = Point::new(300.0, LOUNGE_Y);
    ui.point_at(lounge);
    let _ = ui.simulate([moved(lounge)]);
    // The button sits at the row's right: 8 px in from the list's edge
    // and 8 from the row's.
    let button = Point::new(SIZE.width - 16.0 - 12.0, LOUNGE_Y);
    ui.point_at(button);
    let _ = ui.simulate([moved(button)]);
    let _ = ui.simulate(iced_test::simulator::click());
    assert_eq!(asked(ui), [Intent::OpenChat(LOUNGE.into())]);
}

#[test]
fn the_chat_button_is_not_there_until_pointed_at() {
    let list = channels::channels();
    let mut ui = simulate(&list);
    let button = Point::new(SIZE.width - 16.0 - 12.0, LOUNGE_Y);
    ui.point_at(button);
    let _ = ui.simulate(iced_test::simulator::click());
    assert_eq!(asked(ui), [Intent::Open(LOUNGE.into())], "the row itself");
}

#[test]
fn a_focused_row_presses_with_enter_and_opens_its_menu_from_the_keyboard() {
    let list = channels::channels();
    let mut ui = simulate(&list);
    // The heading takes the focus first, then #general.
    ui.operate(&mut Nth(0, 0));
    let _ = ui.tap_key(Named::Enter);
    ui.operate(&mut Nth(1, 0));
    let _ = ui.tap_key(Named::Space);
    let _ = ui.simulate([Event::Keyboard(keyboard::Event::KeyPressed {
        key: keyboard::Key::Named(Named::F10),
        modified_key: keyboard::Key::Named(Named::F10),
        physical_key: keyboard::key::Physical::Unidentified(
            keyboard::key::NativeCode::Unidentified,
        ),
        location: keyboard::Location::Standard,
        modifiers: keyboard::Modifiers::SHIFT,
        text: None,
        repeat: false,
    })]);
    // Under the row, 16 px in: #general is 40-72 px down, 8 px in.
    let under = Spot::pointer(Point::new(8.0 + 16.0, 72.0));
    assert_eq!(
        asked(ui),
        [
            Intent::Fold(CATEGORY.into()),
            Intent::Open(GENERAL.into()),
            Intent::Menu(GENERAL.into(), under),
        ]
    );
}

#[test]
fn a_channel_dragged_drops_where_the_app_said_it_may() {
    let list = channels::arranging();
    let mut ui = simulate(&list);
    let start = Point::new(200.0, LOUNGE_Y);
    ui.point_at(start);
    let _ = ui.simulate([
        press(mouse::Button::Left),
        moved(start + iced::Vector::new(0.0, 10.0)),
    ]);
    let over = Point::new(200.0, CREATE_ROOM_Y);
    ui.point_at(over);
    let _ = ui.simulate([moved(over), release(mouse::Button::Left)]);
    let asked = asked(ui);
    assert_eq!(asked.first(), Some(&Intent::Arrange(Arrange::Start(4))));
    assert!(
        asked.contains(&Intent::Arrange(Arrange::Over(Some(5)))),
        "{asked:?}"
    );
    assert_eq!(asked.last(), Some(&Intent::Arrange(Arrange::Drop(5))));
    assert!(
        !asked.contains(&Intent::Open(LOUNGE.into())),
        "a drag never clicks"
    );
}

#[test]
fn someone_dragged_onto_a_voice_channel_moves_there() {
    let list = channels::moving();
    let mut ui = simulate(&list);
    let start = Point::new(200.0, CAROL_Y);
    ui.point_at(start);
    let _ = ui.simulate([
        press(mouse::Button::Left),
        moved(start + iced::Vector::new(0.0, 10.0)),
    ]);
    let over = Point::new(200.0, CREATE_ROOM_Y);
    ui.point_at(over);
    let _ = ui.simulate([moved(over), release(mouse::Button::Left)]);
    let asked = asked(ui);
    assert_eq!(
        asked.first(),
        Some(&Intent::Move(Move::Start {
            channel: LOUNGE.into(),
            user: carol().user,
        }))
    );
    assert!(
        asked.contains(&Intent::Move(Move::Over(Some(CREATE_ROOM.into())))),
        "{asked:?}"
    );
    assert_eq!(
        asked.last(),
        Some(&Intent::Move(Move::Drop(CREATE_ROOM.into())))
    );
}

#[test]
fn a_drag_let_go_where_it_may_not_drop_stops() {
    let list = channels::arranging();
    let mut ui = simulate(&list);
    let start = Point::new(200.0, LOUNGE_Y);
    ui.point_at(start);
    let _ = ui.simulate([
        press(mouse::Button::Left),
        moved(start + iced::Vector::new(0.0, 10.0)),
    ]);
    let over = Point::new(200.0, IDEAS_Y);
    ui.point_at(over);
    let _ = ui.simulate([moved(over), release(mouse::Button::Left)]);
    assert_eq!(asked(ui).last(), Some(&Intent::Arrange(Arrange::Stop)));
}

#[test]
fn browse_channels_asks_for_its_screen() {
    let list = channels::browsing();
    let mut ui = simulate(&list);
    ui.click("Browse channels").expect("Browse Channels");
    assert_eq!(asked(ui), [Intent::Browse]);
}

#[test]
fn an_empty_list_says_why() {
    let list = channels::empty();
    let mut ui = simulate(&list);
    ui.find("No matching conversations.").expect("the note");
}

#[test]
fn a_busy_voice_channel_shows_who_is_in_it_under_the_pointer_until_clicked() {
    let list = channels::channels();
    let mut ui = simulate(&list);
    assert!(ui.find("3 people · 1 live").is_err(), "no card at rest");
    let lounge = Point::new(200.0, LOUNGE_Y);
    ui.point_at(lounge);
    let _ = ui.simulate([moved(lounge)]);
    ui.find("3 people · 1 live")
        .expect("the card beside the row");
    let _ = ui.simulate(iced_test::simulator::click());
    assert!(ui.find("3 people · 1 live").is_err(), "a click hides it");
}
