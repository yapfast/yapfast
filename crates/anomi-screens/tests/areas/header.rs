//! The conversation's header: task N07's tests, driving
//! `anomi_screens::header::view` over its fixtures in the simulator
//! (`super::support`).

use iced::{Point, Size};
use iced_test::{Simulator, simulator};

use anomi_screens::header;
use anomi_theme::{Renderer, Theme};
use anomi_vm::fixtures::header as fixtures;
use anomi_vm::header::{Header, Intent};

use super::support::{asked, front, settings};

const WIDTH: f32 = 1000.0;
/// The middle of the bar.
const MIDDLE: f32 = 24.0;
/// A button's side, and the gap between two (`:3145`).
const STEP: f32 = 42.0;

fn simulate(header: &Header) -> Simulator<'_, Intent, Theme, Renderer> {
    use iced::Widget as _;
    let view = header::view(header, front()).boxed();
    Simulator::with_size(settings(), Size::new(WIDTH, 50.0), view)
}

/// The centre of the nth button from the right, counting from 0.
fn button(from_right: f32) -> Point {
    Point::new(WIDTH - 32.0 - from_right * STEP, MIDDLE)
}

fn click(ui: &mut Simulator<'_, Intent, Theme, Renderer>, at: Point) {
    ui.point_at(at);
    ui.simulate(simulator::click());
}

/// What a click at `at` on `header` asked for.
fn after_click(header: &Header, at: Point) -> Vec<Intent> {
    let mut ui = simulate(header);
    click(&mut ui, at);
    asked(ui)
}

#[test]
fn a_dm_has_its_buttons_in_discords_order() {
    let header = fixtures::dm();
    // From the right: the profile, the friends picker, the pins, the video
    // call, the voice call.
    assert!(matches!(
        after_click(&header, button(0.0))[..],
        [Intent::ProfilePanel]
    ));
    assert!(matches!(
        after_click(&header, button(1.0))[..],
        [Intent::AddPeople(_)]
    ));
    assert!(matches!(
        after_click(&header, button(2.0))[..],
        [Intent::Pins(_)]
    ));
    assert!(matches!(
        after_click(&header, button(3.0))[..],
        [Intent::VideoCall]
    ));
    assert!(matches!(
        after_click(&header, button(4.0))[..],
        [Intent::Call]
    ));
}

#[test]
fn add_friend_asks_to_befriend() {
    // Left of the calls, a button wider than the icons.
    let at = Point::new(button(4.0).x - 100.0, MIDDLE);
    assert_eq!(after_click(&fixtures::dm(), at), [Intent::Befriend]);
    assert_eq!(
        after_click(&fixtures::dm_activity(), at),
        [Intent::Befriend]
    );
}

#[test]
fn a_call_going_on_is_joined_and_has_no_video_button() {
    // Bob's call: the voice button follows Add Friend, after the pins,
    // with no video button between.
    let header = fixtures::dm_activity();
    assert!(matches!(
        after_click(&header, button(3.0))[..],
        [Intent::Call]
    ));
}

#[test]
fn a_call_that_cannot_start_asks_for_nothing() {
    let Header::Open(mut place) = fixtures::dm() else {
        panic!("a place");
    };
    place.tools.call = Some(anomi_vm::header::Call {
        going: false,
        can: false,
    });
    let header = Header::Open(place);
    assert!(after_click(&header, button(4.0)).is_empty());
    assert!(after_click(&header, button(3.0)).is_empty());
}

#[test]
fn the_person_in_a_one_to_one_dm_opens_their_card() {
    let intents = after_click(&fixtures::dm(), Point::new(60.0, MIDDLE));
    assert!(matches!(intents[..], [Intent::Profile(_)]), "{intents:?}");
}

#[test]
fn a_group_has_no_person_button_but_a_member_toggle() {
    let header = fixtures::group();
    assert!(after_click(&header, Point::new(60.0, MIDDLE)).is_empty());
    // The toggle is the last button; its picture is at its left.
    let intents = after_click(&header, Point::new(WIDTH - 16.0 - 20.0, MIDDLE));
    assert_eq!(intents, [Intent::Members]);
}

#[test]
fn a_channel_opens_its_topic_threads_and_notifications() {
    let header = fixtures::channel();
    // From the right: the members, the pins, the bell, the threads.
    assert_eq!(after_click(&header, button(0.0)), [Intent::Members]);
    assert!(matches!(
        after_click(&header, button(1.0))[..],
        [Intent::Pins(_)]
    ));
    assert!(matches!(
        after_click(&header, button(2.0))[..],
        [Intent::Notifications(_)]
    ));
    assert!(matches!(
        after_click(&header, button(3.0))[..],
        [Intent::Threads(_)]
    ));
    // The topic's line, after the name and its dot.
    let intents = after_click(&header, Point::new(300.0, MIDDLE));
    assert!(matches!(intents[..], [Intent::Topic(_)]), "{intents:?}");
}

#[test]
fn a_popover_opens_from_the_button_it_is_under() {
    let intents = after_click(&fixtures::channel(), button(1.0));
    let [Intent::Pins(spot)] = &intents[..] else {
        panic!("{intents:?}");
    };
    // Under the button, its right edge on the button's.
    assert_eq!(spot.anchor.size(), Size::new(32.0, 32.0));
    assert_eq!(spot.anchor.center().x, button(1.0).x);
    assert_eq!(spot.align, anomi_vm::menu::Align::End);
}

#[test]
fn a_thread_leads_back_to_its_channel_and_has_a_menu() {
    let header = fixtures::thread();
    // The channel's name after its icon.
    let intents = after_click(&header, Point::new(16.0 + 22.0 + 10.0 + 20.0, MIDDLE));
    assert_eq!(intents, [Intent::Parent]);
    // From the right: its menu after the member list, which follows the pins.
    assert!(matches!(
        after_click(&header, button(0.0))[..],
        [Intent::More(_)]
    ));
    assert_eq!(after_click(&header, button(1.0)), [Intent::Members]);
}

/// Every intent a click asked for along the bar's middle between `from` and
/// `to`, for a button whose place the test does not know to the pixel.
fn along(header: &Header, from: f32, to: f32) -> Vec<Intent> {
    let mut found = Vec::new();
    let mut x = from;
    while x < to {
        found.extend(after_click(header, Point::new(x, MIDDLE)));
        x += 8.0;
    }
    found
}

#[test]
fn a_voice_channel_switches_between_its_stage_and_its_chat() {
    // Its stage shows only the chat button and the count, no pins.
    let found = along(&fixtures::voice(), 300.0, WIDTH);
    assert!(found.contains(&Intent::VoiceChat(true)), "{found:?}");
    assert!(!found.iter().any(|intent| matches!(intent, Intent::Pins(_))));
}

#[test]
fn friends_has_its_tabs() {
    let header = fixtures::friends();
    let mut ui = simulate(&header);
    ui.click("All").expect("the All tab");
    assert_eq!(asked(ui), [Intent::FriendsAll(true)]);
    let mut ui = simulate(&header);
    ui.click("Online").expect("the Online tab");
    assert_eq!(asked(ui), [Intent::FriendsAll(false)]);
}

#[test]
fn nothing_open_shows_nothing_to_press() {
    assert!(after_click(&Header::None, Point::new(60.0, MIDDLE)).is_empty());
}

#[test]
fn the_skeleton_shapes_show_their_title() {
    let direct = Header::Direct {
        name: "Person 5".into(),
        avatar: anomi_vm::kind::Avatar::default(),
    };
    let mut ui = simulate(&direct);
    assert!(ui.find("Person 5").is_ok());
    let channel = Header::Channel {
        name: "general".into(),
        topic: "plans".into(),
    };
    let mut ui = simulate(&channel);
    assert!(ui.find("general").is_ok());
}

#[test]
fn the_topic_popover_closes_from_its_button() {
    use iced::Widget as _;
    let header = fixtures::channel_long_topic();
    let view = header::topic(&header, front()).boxed();
    let mut ui = Simulator::with_size(settings(), Size::new(440.0, 300.0), view);
    // Its close button at the right of the heading.
    click(&mut ui, Point::new(440.0 - 10.0 - 14.0, 24.0));
    assert_eq!(asked(ui), [Intent::Close]);
}

#[test]
fn a_header_without_a_topic_has_no_topic_popover() {
    use iced::Widget as _;
    let header = fixtures::thread();
    let view = header::topic(&header, front()).boxed();
    let mut ui = Simulator::with_size(settings(), Size::new(440.0, 300.0), view);
    click(&mut ui, Point::new(416.0, 24.0));
    assert!(asked(ui).is_empty());
}

#[test]
fn friends_has_an_add_friend_button() {
    // Right of the tabs, a 28 px button.
    let found = along(&fixtures::friends(), 150.0, 500.0);
    assert!(found.contains(&Intent::AddFriend), "{found:?}");
}

#[test]
fn a_link_in_the_topic_in_full_is_asked_for() {
    use iced::Widget as _;
    let header = fixtures::channel();
    // Along the body's first lines, where the mention of #general and the
    // address are.
    let mut found = Vec::new();
    for line in 0..5 {
        let y = 49.0 + 16.0 + 10.0 + line as f32 * 22.0;
        for step in 0..40 {
            let mut ui = Simulator::with_size(
                settings(),
                Size::new(440.0, 300.0),
                header::topic(&header, front()).boxed(),
            );
            let at = Point::new(20.0 + step as f32 * 10.0, y);
            // A link is under the pointer once it has moved there.
            ui.point_at(at);
            ui.simulate([iced::Event::Mouse(iced::mouse::Event::CursorMoved {
                position: at,
            })]);
            ui.simulate(simulator::click());
            found.extend(asked(ui));
        }
    }
    assert!(
        found
            .iter()
            .any(|intent| matches!(intent, Intent::Link(anomi_vm::text::Link::Channel(_)))),
        "{found:?}"
    );
    assert!(
        found
            .iter()
            .any(|intent| matches!(intent, Intent::Link(anomi_vm::text::Link::Url(_)))),
        "{found:?}"
    );
}

#[test]
fn nothing_open_shows_the_sidebars_title() {
    let header = fixtures::sidebar_title();
    let mut ui = simulate(&header);
    assert!(ui.find("Direct messages").is_ok());
}

#[test]
fn a_voice_channel_shows_how_many_are_in_it_where_there_is_no_list() {
    let header = fixtures::voice();
    let mut ui = simulate(&header);
    assert!(ui.find("4 members").is_ok());
}

#[test]
fn the_search_field_is_placed_after_the_profile_button() {
    use iced::Widget as _;
    use iced::widget::text;
    let header = fixtures::dm();
    let view = header::with_search::<Intent>(&header, text("search-here").boxed(), front());
    let mut ui = Simulator::with_size(settings(), Size::new(WIDTH, 50.0), view);
    let field = ui
        .find("search-here")
        .expect("the field is placed")
        .bounds();
    // The profile button is the last of the buttons, and the field sits
    // right of it, a gap away.
    let profile = field.x - 10.0 - 16.0;
    drop(ui);
    let view = header::with_search::<Intent>(&header, text("search-here").boxed(), front());
    let mut ui = Simulator::with_size(settings(), Size::new(WIDTH, 50.0), view);
    click(&mut ui, Point::new(profile, MIDDLE));
    assert!(
        matches!(asked(ui)[..], [Intent::ProfilePanel]),
        "the profile button is left of the field at {field:?}"
    );
    // Without the place's search, it is left out.
    let Header::Open(mut place) = fixtures::dm() else {
        panic!("a place");
    };
    place.tools.search = false;
    let header = Header::Open(place);
    let view = header::with_search::<Intent>(&header, text("search-here").boxed(), front());
    let mut ui = Simulator::with_size(settings(), Size::new(WIDTH, 50.0), view);
    assert!(ui.find("search-here").is_err());
}

/// Bob's activity bubble, in a conversation of `width`.
fn bubble_at(width: f32, compact: bool) -> bool {
    use iced::Widget as _;
    let Header::Open(mut place) = fixtures::dm_activity() else {
        panic!("a place");
    };
    place.tools.compact = compact;
    let header = Header::Open(place);
    let view = header::view(&header, front()).boxed();
    let mut ui = Simulator::with_size(settings(), Size::new(width, 50.0), view);
    ui.find("Playing Factorio").is_ok()
}

#[test]
fn the_activity_bubble_gives_way_where_less_than_96_px_is_left() {
    assert!(bubble_at(1000.0, false));
    // Under 96 px are left beside the person and the buttons, 4 px apart.
    assert!(bubble_at(620.0, true));
    assert!(!bubble_at(500.0, true));
}

#[test]
fn the_persons_picture_stays_round_in_a_narrow_header() {
    use iced::Widget as _;
    let Header::Open(mut place) = fixtures::dm_activity() else {
        panic!("a place");
    };
    place.tools.compact = true;
    let header = Header::Open(place);
    for width in [380.0, 440.0] {
        let view = header::view(&header, front()).boxed();
        let mut ui = Simulator::with_size(settings(), Size::new(width, 50.0), view);
        // The person's button is there, at its 42 px at the least, and
        // opens their card.
        click(&mut ui, Point::new(4.0 + 12.0 + 13.0, MIDDLE));
        assert!(
            matches!(asked(ui)[..], [Intent::Profile(_)]),
            "at {width} px"
        );
    }
}
