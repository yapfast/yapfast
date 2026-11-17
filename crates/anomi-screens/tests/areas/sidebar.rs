//! The sidebar's own part, its search field: F01's tests, driving the whole
//! window over the shell's fixtures in the simulator (`super::support`).

use anomi_vm::fixtures::shell;
use anomi_vm::{Intent, sidebar};

use super::support::{asked, simulate};

#[test]
fn typing_in_the_search_field_filters() {
    let vm = shell::dm_home();
    let mut ui = simulate(&vm);
    ui.click("Find a conversation").expect("the search field");
    ui.typewrite("bo");
    let intents = asked(ui);
    assert_eq!(
        intents.last(),
        Some(&Intent::Sidebar(sidebar::Intent::Filter("bo".into()))),
        "{intents:?}"
    );
}
