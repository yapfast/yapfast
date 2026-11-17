//! A server's audit log: task X07's tests, driving
//! `anomi_settings::server::audit::view` over its fixtures in the simulator
//! (`super::support`).

use iced::Widget as _;
use iced_test::simulator::Simulator;

use anomi_settings::server::audit::view;
use anomi_vm::fixtures::settings::audit as sample;
use anomi_vm::settings::audit::{Audit, Intent};

use super::support::{SIZE, asked, front, settings};

fn simulate(page: &Audit) -> Simulator<'_, Intent, anomi_theme::Theme, anomi_theme::Renderer> {
    Simulator::with_size(settings(), SIZE, view(page, front()).boxed())
}

#[test]
fn the_filters_label_their_pickers() {
    let page = sample::audit();
    let mut ui = simulate(&page);
    assert!(ui.find("Filter by User").is_ok());
    assert!(ui.find("Filter by Action").is_ok());
    assert!(ui.find("All Users").is_ok());
    assert!(ui.find("All Actions").is_ok());
}

#[test]
fn older_entries_load_from_the_button_only_while_idle() {
    let mut page = sample::opened();
    let mut ui = simulate(&page);
    ui.click("Load More").expect("the button is there");
    assert!(asked(ui).contains(&Intent::Older));
    page.fetching = true;
    let mut ui = simulate(&page);
    assert!(ui.find("Load More").is_err());
    assert!(ui.find("Loading...").is_ok());
    drop(ui);
    page.fetching = false;
    page.older = false;
    let mut ui = simulate(&page);
    assert!(ui.find("Load More").is_err());
}

#[test]
fn the_list_reports_its_end_only_while_older_entries_may_load() {
    let mut page = sample::opened();
    let mut ui = simulate(&page);
    ui.click("Today at 00:49").expect("the first row");
    let seen = asked(ui);
    assert!(
        seen.iter()
            .all(|intent| matches!(intent, Intent::Scrolled { .. } | Intent::Toggle(0))),
        "{seen:?}"
    );
    page.older = false;
    let mut ui = simulate(&page);
    ui.click("Today at 00:49").expect("the first row");
    assert_eq!(asked(ui), [Intent::Toggle(0)]);
}

#[test]
fn a_row_with_lines_opens_and_one_without_doesnt() {
    let page = sample::audit();
    let mut ui = simulate(&page);
    ui.click("Last Sunday at 01:09").expect("a row with lines");
    ui.click("Yesterday at 00:09").expect("a row with none");
    assert_eq!(asked(ui), [Intent::Toggle(4)]);
}

#[test]
fn an_open_row_shows_its_lines_and_a_closed_one_doesnt() {
    let mut page = sample::opened();
    let mut ui = simulate(&page);
    assert!(ui.find("Manage Messages").is_ok());
    assert!(
        ui.find("Changed the colour from #2ecc71 to #1abc9c")
            .is_ok()
    );
    assert!(ui.find("Changed the name from Old to Server").is_err());
    drop(ui);
    page.open = None;
    let mut ui = simulate(&page);
    assert!(ui.find("Manage Messages").is_err());
}

#[test]
fn a_waiting_empty_and_failed_log_say_so() {
    for (vm, words) in [
        (sample::loading(), "Loading..."),
        (sample::empty(), "No Logs Yet"),
        (sample::failed(), "This is Awkward"),
    ] {
        let page = vm.settings.audit;
        let mut ui = simulate(&page);
        assert!(ui.find(words).is_ok(), "{words}");
        assert!(ui.find("Load More").is_err());
    }
}
