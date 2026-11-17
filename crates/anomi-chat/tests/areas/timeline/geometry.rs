//! Where the timeline puts what stands around its messages, measured from
//! the end of a short history, as the Slint timeline lays it out: the
//! newest row 20 px over the composer, a row 12 px under the one above when
//! a date or the "New" divider opens it, the history's edge 42 px with its
//! line 4 px down, a divider's line 26 px with 6 under it
//! (`ui/yapfast.slint:3855`-`:3888`, `ui/messages.slint:2409`). The
//! messages are C02's stubs here, of no height, so only the timeline's own
//! spaces count.

use iced_test::Simulator;

use anomi_theme::{Renderer, Theme};
use anomi_vm::chat;
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

#[test]
fn a_dms_start_and_its_day_stand_over_the_newest_message() {
    let timeline = fixtures::person_chat();
    let mut ui = shown(&timeline);
    // The one row: 12 px, the edge's 42, the day's 26, 20 px over the end.
    let top = SIZE.height - 20.0 - 26.0 - 42.0 - 12.0;
    let start = middle(
        &mut ui,
        "This is the beginning of your messages with Person 5.",
    );
    assert!(near(start, top + 12.0 + 4.0 + 15.0), "the start at {start}");
    let day = middle(&mut ui, "1 October 2026");
    assert!(near(day, top + 12.0 + 42.0 + 10.0), "the day at {day}");
}

#[test]
fn the_new_divider_opens_its_row_12_px_down() {
    let timeline = fixtures::bob();
    let mut ui = shown(&timeline);
    // The last row is 1 px over the end; the divided one above it 12 px,
    // its divider's 26 and 1 px under its message.
    let last = SIZE.height - 20.0 - 1.0;
    let divided = last - 1.0 - 26.0 - 12.0;
    let new = middle(&mut ui, "New");
    assert!(near(new, divided + 12.0 + 10.0), "\"New\" at {new}");
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
