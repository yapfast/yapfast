//! The DM list: task N03's tests, driving the whole window over the shell's
//! fixtures in the simulator (`super::support`).

use anomi_vm::fixtures::shell;
use anomi_vm::{Intent, sidebar};

use super::support::{asked, simulate};

#[test]
fn clicking_a_dm_asks_to_open_it() {
    let vm = shell::dm_home();
    let mut ui = simulate(&vm);
    ui.click("Person 4").expect("Person 4's row");
    assert_eq!(
        asked(ui),
        [Intent::Sidebar(sidebar::Intent::Dms(
            sidebar::dms::Intent::Open("discord:p4".into())
        ))]
    );
}

#[test]
fn clicking_friends_asks_for_the_friends_screen() {
    let vm = shell::dm_home();
    let mut ui = simulate(&vm);
    // No language is chosen in tests, so the text is the code's own.
    ui.click("Friends").expect("the Friends row");
    assert_eq!(asked(ui), [Intent::from(sidebar::dms::Intent::Friends)]);
}
