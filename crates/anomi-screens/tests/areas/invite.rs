//! The invite dialog: task N12's tests, driving
//! `anomi_screens::places::invite::view` over its fixtures in the simulator
//! (`super::support`): the people and the link on its first page, the
//! link's settings on its second.

use iced::advanced::widget::operation::focusable;
use iced::advanced::widget::{Id, Operation};
use iced::keyboard::{self, key::Named};
use iced::{Point, Size};
use iced_test::Simulator;

use anomi_screens::places::invite::{self, view};
use anomi_theme::{Renderer, Theme};
use anomi_vm::fixtures::places::invite as samples;
use anomi_vm::places::invite::{Intent, Invite, Page};

use super::support::{asked, front, settings};

type Ui<'a> = Simulator<'a, Intent, Theme, Renderer>;

fn shown(invite: &Invite) -> Ui<'_> {
    Simulator::with_size(settings(), Size::new(600.0, 760.0), view(invite, front()))
}

/// Focus the button with this id and press it with Enter.
fn press(ui: &mut Ui<'_>, id: &'static str) {
    let mut focus = focusable::focus::<()>(Id::new(id));
    ui.operate(&mut focus as &mut dyn Operation);
    ui.tap_key(keyboard::Key::Named(Named::Enter));
}

/// Focuses the `want`th focusable, as the walk of Tab does by count.
struct Nth {
    want: usize,
    seen: usize,
}

impl Operation for Nth {
    fn focusable(
        &mut self,
        _id: Option<&Id>,
        _bounds: iced::Rectangle,
        state: &mut dyn focusable::Focusable,
    ) {
        if self.seen == self.want {
            state.focus();
        } else {
            state.unfocus();
        }
        self.seen += 1;
    }

    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }
}

#[test]
fn the_link_copies() {
    let invite = samples::link();
    let mut ui = shown(&invite);
    press(&mut ui, "dialog-copy");
    assert_eq!(
        asked(ui),
        [Intent::Copy("https://discord.gg/qsGhsSbQ".into())]
    );
}

#[test]
fn copy_waits_for_a_link() {
    let invite = samples::making();
    let mut ui = shown(&invite);
    press(&mut ui, "dialog-copy");
    assert!(asked(ui).is_empty());
}

#[test]
fn the_first_page_says_where_and_lists_whom_to_invite() {
    let invite = samples::link();
    let mut ui = shown(&invite);
    assert!(ui.find("Invite friends to Server").is_ok());
    assert!(ui.find("Recipients will land in").is_ok());
    assert!(ui.find("general").is_ok());
    assert!(ui.find("Erin").is_ok() && ui.find("person0").is_ok());
    assert!(ui.find("Or, send a server invite link to a friend").is_ok());
    // A text channel has no members' group and no private notice.
    assert!(ui.find("Server members").is_err());
    assert!(
        ui.find("This channel is private. Only select members and roles can view it.")
            .is_err()
    );
    assert!(
        ui.find("EXPIRE AFTER").is_err(),
        "the settings are the second page's"
    );
}

#[test]
fn typing_searches() {
    let invite = samples::link();
    let mut ui = shown(&invite);
    ui.click(Id::new(invite::SEARCH)).expect("the field");
    let _ = ui.typewrite("Er");
    assert_eq!(
        asked(ui),
        [Intent::Search("E".into()), Intent::Search("Er".into())]
    );
}

/// Clicks the Invite button of the row whose username is `username`: at
/// the row's height, by the right edge the link's Copy button shares.
fn press_invite(ui: &mut Ui<'_>, username: &str) {
    let copy = ui.find("Copy").expect("the link's button").bounds();
    let row = ui.find(username).expect("their row").bounds();
    ui.point_at(Point::new(copy.x + copy.width - 30.0, row.y));
    ui.simulate(iced_test::simulator::click());
}

#[test]
fn invite_sends_to_that_person_once() {
    let invite = samples::voice();
    let mut ui = shown(&invite);
    // Bob was sent it and Dave's was refused: theirs don't send again.
    assert!(ui.find("Sent").is_ok());
    assert!(
        ui.find("Couldn't send the invite: Cannot send messages to this user")
            .is_ok()
    );
    assert!(
        ui.find("dave").is_err(),
        "the reason takes the username's place"
    );
    press_invite(&mut ui, "mallory");
    press_invite(&mut ui, "bob");
    press_invite(&mut ui, "carol");
    press_invite(&mut ui, "erin");
    assert_eq!(
        asked(ui),
        [Intent::Send("mallory".into()), Intent::Send("erin".into())]
    );
}

#[test]
fn a_voice_channel_lists_its_members_first_under_a_heading_that_folds() {
    let invite = samples::voice();
    let mut ui = shown(&invite);
    assert!(
        ui.find("This channel is private. Only select members and roles can view it.")
            .is_ok()
    );
    assert!(ui.find("Friends").is_ok());
    ui.click("Server members").expect("the heading");
    assert_eq!(asked(ui), [Intent::Fold]);
    let folded = Invite {
        folded: true,
        ..samples::voice()
    };
    let mut ui = shown(&folded);
    assert!(ui.find("Mallory").is_err() && ui.find("Erin").is_ok());
}

#[test]
fn a_search_that_finds_no_one_says_so() {
    let invite = samples::nobody();
    let mut ui = shown(&invite);
    assert!(ui.find("No one found.").is_ok());
}

#[test]
fn the_choices_are_options_to_click() {
    let invite = samples::settings();
    let mut ui = shown(&invite);
    ui.click("1 day").expect("an option");
    ui.click("25").expect("an option");
    ui.click("Never").expect("an option");
    assert_eq!(asked(ui), [Intent::Age(4), Intent::Uses(4), Intent::Age(6)]);
}

#[test]
fn the_options_take_the_keys() {
    let invite = samples::settings();
    let mut ui = shown(&invite);
    // What Tab reaches: the X, then the options of the ages.
    let mut second = Nth { want: 1, seen: 0 };
    ui.operate(&mut second as &mut dyn Operation);
    ui.tap_key(keyboard::Key::Named(Named::Space));
    assert_eq!(asked(ui), [Intent::Age(0)]);
}

#[test]
fn a_new_link_is_made_from_the_choices() {
    let invite = Invite {
        age: 2,
        uses: 3,
        ..samples::settings()
    };
    let mut ui = shown(&invite);
    press(&mut ui, invite::REMAKE);
    assert_eq!(asked(ui), [Intent::Remake { age: 2, uses: 3 }]);
}

#[test]
fn a_new_link_waits_while_one_is_being_made() {
    let invite = Invite {
        page: Page::Link,
        ..samples::making()
    };
    let mut ui = shown(&invite);
    press(&mut ui, invite::REMAKE);
    assert!(asked(ui).is_empty());
}

#[test]
fn back_turns_to_the_people_and_the_x_closes() {
    let invite = samples::settings();
    let mut ui = shown(&invite);
    press(&mut ui, "dialog-cancel");
    assert_eq!(asked(ui), [Intent::Show(Page::People)]);
    // The link to the settings starts the line under the link's well when
    // nothing comes before it.
    let invite = Invite {
        note: String::new(),
        ..samples::link()
    };
    let mut ui = shown(&invite);
    let left = ui
        .find("Or, send a server invite link to a friend")
        .expect("the foot")
        .bounds()
        .x;
    let copy = ui.find("Copy").expect("the link's button").bounds();
    // A link hears the pointer move onto it before the press.
    let at = Point::new(left + 20.0, copy.y + copy.height + 20.0);
    ui.point_at(at);
    ui.simulate([iced::Event::Mouse(iced::mouse::Event::CursorMoved {
        position: at,
    })]);
    ui.simulate(iced_test::simulator::click());
    assert_eq!(asked(ui), [Intent::Show(Page::Link)]);
}

#[test]
fn the_parts_show_as_the_state_has_them() {
    let invite = Invite {
        page: Page::Link,
        ..samples::failed()
    };
    // The headings and the error show; a link being made says so instead.
    let mut ui = shown(&invite);
    assert!(ui.find("EXPIRE AFTER").is_ok());
    assert!(ui.find("MAX NUMBER OF USES").is_ok());
    assert!(ui.find("Discord refused to make the link.").is_ok());
    let making = samples::making();
    let mut ui = shown(&making);
    assert!(ui.find("Making a link...").is_ok());
}
