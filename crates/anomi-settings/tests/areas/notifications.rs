//! The Notifications settings page: task S05's tests, driving
//! `anomi_settings::notifications::view` over its fixtures in the simulator
//! (`super::support`).

use iced::Widget as _;
use iced_test::simulator::Simulator;

use anomi_settings::notifications::view;
use anomi_vm::fixtures::settings::notifications as sample;
use anomi_vm::settings::notifications::{Intent, Notifications};

use super::support::{SIZE, asked, front, settings};

fn simulate(
    page: &Notifications,
) -> Simulator<'_, Intent, anomi_theme::Theme, anomi_theme::Renderer> {
    Simulator::with_size(settings(), SIZE, view(page, front()).boxed())
}

#[test]
fn a_press_on_a_switch_asks_for_its_opposite() {
    let page = sample::notifications();
    let mut ui = simulate(&page);
    for text in [
        "Show desktop notifications",
        "Show what messages say",
        "Flash the taskbar",
    ] {
        ui.click(text).expect("the switch is there");
    }
    assert_eq!(
        asked(ui),
        [
            Intent::Notifications(false),
            Intent::Text(false),
            Intent::FlashTaskbar(false)
        ]
    );
}

#[test]
fn text_waits_for_notifications_to_be_on() {
    let mut page = sample::notifications();
    page.notifications = false;
    let mut ui = simulate(&page);
    let _ = ui.click("Show what messages say");
    assert!(asked(ui).is_empty());
}

#[test]
fn the_description_names_the_networks_in_use() {
    let matrix = "For new messages while Yapfast isn't in front. Your Matrix notification \
                  rules decide which: usually DMs and mentions. Clicking one opens the \
                  conversation.";
    let mut page = sample::notifications();
    assert!(simulate(&page).find(matrix).is_err());
    page.matrix = true;
    page.discord = false;
    assert!(simulate(&page).find(matrix).is_ok());
}
