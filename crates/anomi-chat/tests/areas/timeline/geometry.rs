//! Where the timeline puts what stands around its messages, as the Slint
//! timeline lays it out: the newest row 20 px over the composer, a row
//! 12 px under the one above when a date or the "New" divider opens it, the
//! history's edge 42 px with its line 4 px down, a divider's line 26 px
//! with 6 under it (`ui/yapfast.slint:3855`-`:3888`,
//! `ui/messages.slint:2409`). The rows are C02's messages, so each is
//! measured from the keyboard's ground: it takes in the space over its
//! message when nothing stands over it (`body`, `:3802`), and starts just
//! under the edge or the divider otherwise.

use iced::Rectangle;
use iced::advanced::widget::{Id, Operation};
use iced_test::Simulator;

use anomi_chat::timeline::ACTIVE;
use anomi_theme::{Renderer, Theme};
use anomi_vm::chat::{
    self,
    timeline::{Keys, Timeline},
};
use anomi_vm::fixtures::chat::timeline as fixtures;

use super::{SIZE, shown};

/// The middle of `text`'s line, down the timeline.
fn middle(ui: &mut Simulator<'_, chat::Intent, Theme, Renderer>, text: &str) -> f32 {
    let bounds = ui.find(text).expect(text).bounds();
    bounds.y + bounds.height / 2.0
}

fn near(found: f32, wanted: f32) -> bool {
    (found - wanted).abs() <= 0.5
}

/// The keyboard's ground and the message on it.
#[derive(Default)]
struct Ground {
    ground: Option<Rectangle>,
    message: Option<Rectangle>,
}

impl Operation for Ground {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        if self.message.is_none() {
            operate(self);
        }
    }

    fn container(&mut self, id: Option<&Id>, bounds: Rectangle, _viewport: &Rectangle) {
        if self.ground.is_none() {
            if id == Some(&ACTIVE) {
                self.ground = Some(bounds);
            }
        } else if self.message.is_none() {
            self.message = Some(bounds);
        }
    }
}

/// Bob's DM with the keyboard on row `index`: "snacks" (1) starts a group
/// under the first, with no date or "New" over it; "great" (2) has "New".
fn on_bob(index: usize, compact: bool) -> Timeline {
    let bob = fixtures::bob();
    Timeline {
        keys: Keys {
            active: Some(bob.rows[index].key),
            browsing: true,
            ..Keys::default()
        },
        compact,
        ..bob
    }
}

/// The keyboard's ground and the message on it in `ui`.
fn ground(ui: &mut Simulator<'_, chat::Intent, Theme, Renderer>) -> (Rectangle, Rectangle) {
    let mut found = Ground::default();
    ui.operate(&mut found);
    let (Some(ground), Some(message)) = (found.ground, found.message) else {
        panic!("the keyboard's message");
    };
    (ground, message)
}

#[test]
fn the_keyboards_ground_takes_in_the_space_over_a_group() {
    for (compact, gap) in [(false, 16.0), (true, 8.0)] {
        let timeline = on_bob(1, compact);
        let mut ui = shown(&timeline);
        let (ground, message) = ground(&mut ui);
        assert_eq!(message.y - ground.y, gap, "compact: {compact}");
        // Down to the row's foot, with its 1 px.
        assert_eq!(
            ground.y + ground.height,
            message.y + message.height + 1.0,
            "compact: {compact}"
        );
    }
}

#[test]
fn under_the_new_divider_the_ground_starts_with_the_message() {
    let timeline = on_bob(2, false);
    let mut ui = shown(&timeline);
    let (ground, message) = ground(&mut ui);
    assert_eq!(ground.y, message.y);
    // The divider's line, 10 px down to its middle, and the 6 px under it.
    let new = middle(&mut ui, "New");
    assert!(
        near(ground.y, new + 10.0 + 6.0),
        "{ground:?}, \"New\" at {new}"
    );
}

#[test]
fn a_dms_start_and_its_day_stand_over_the_newest_message() {
    let person = fixtures::person_chat();
    let timeline = Timeline {
        keys: Keys {
            active: Some(person.rows[0].key),
            ..Keys::default()
        },
        ..person
    };
    let mut ui = shown(&timeline);
    let (ground, _) = ground(&mut ui);
    // The one row ends 20 px over the end, its message under the day's
    // 26 px, the day under the edge's 42.
    assert_eq!(ground.y + ground.height, SIZE.height - 20.0, "{ground:?}");
    let day = middle(&mut ui, "1 October 2026");
    assert!(
        near(day, ground.y - 26.0 + 10.0),
        "the day at {day}, {ground:?}"
    );
    let start = middle(
        &mut ui,
        "This is the beginning of your messages with Person 5.",
    );
    assert!(
        near(start, ground.y - 26.0 - 42.0 + 4.0 + 15.0),
        "the start at {start}, {ground:?}"
    );
}

#[test]
fn the_new_divider_opens_its_row_12_px_down() {
    // "snacks" (1) ends with its 1 px where "great" (2) starts: 12 px, then
    // the divider's line, "New" 10 px down it.
    let timeline = on_bob(1, false);
    let mut ui = shown(&timeline);
    let (above, _) = ground(&mut ui);
    let new = middle(&mut ui, "New");
    assert!(
        near(new, above.y + above.height + 12.0 + 10.0),
        "\"New\" at {new}, {above:?}"
    );
}

#[test]
fn a_conversations_start_sits_24_px_over_the_composer() {
    let timeline = fixtures::empty_channel();
    let mut ui = shown(&timeline);
    let says = ui
        .find("This is the start of #Lounge. No messages here yet.")
        .expect("the line")
        .bounds();
    assert!(near(says.y + says.height, SIZE.height - 24.0), "{says:?}");
    assert_eq!(says.x, 24.0);
    let title = ui.find("Welcome to #Lounge").expect("the title").bounds();
    assert!(near(title.y + title.height, says.y - 8.0), "{title:?}");
}
