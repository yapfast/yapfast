//! The Advanced settings page: task S10's tests, driving
//! `anomi_settings::advanced::view` over its fixtures in the simulator
//! (`super::support`).

use iced::Widget as _;
use iced_test::simulator::Simulator;

use anomi_settings::advanced::view;
use anomi_vm::fixtures::settings::advanced as sample;
use anomi_vm::settings::advanced::{Advanced, Intent};

use super::support::{SIZE, asked, front, settings};

fn simulate(page: &Advanced) -> Simulator<'_, Intent, anomi_theme::Theme, anomi_theme::Renderer> {
    Simulator::with_size(settings(), SIZE, view(page, front()).boxed())
}

#[test]
fn a_switch_asks_for_its_opposite() {
    let page = sample::advanced();
    let mut ui = simulate(&page);
    for name in [
        "Start when you sign in",
        "Open Discord links in Yapfast",
        "Close to tray",
        "Allow multiple windows",
        "Render with the GPU",
        "Encode streams with the GPU",
        "Developer Mode",
    ] {
        ui.click(name).expect("the switch is there");
    }
    assert_eq!(
        asked(ui),
        [
            Intent::StartOnSignIn(true),
            Intent::DiscordLinks(true),
            Intent::CloseToTray(false),
            Intent::MultipleWindows(true),
            Intent::GpuRendering(true),
            Intent::GpuEncoding(false),
            Intent::DeveloperMode(true),
        ]
    );
}

#[test]
fn restart_shows_only_when_the_gpu_choice_differs_from_what_runs() {
    let page = sample::advanced();
    assert!(simulate(&page).find("Restart Yapfast").is_err());
    let page = Advanced {
        gpu_rendering: true,
        ..page
    };
    let mut ui = simulate(&page);
    ui.click("Restart Yapfast").expect("restart is offered");
    assert_eq!(asked(ui), [Intent::Restart]);
}

#[test]
fn switches_the_system_lacks_are_left_out() {
    let page = Advanced {
        autostart_available: false,
        discord_links_available: false,
        ..sample::advanced()
    };
    let mut ui = simulate(&page);
    assert!(ui.find("Start when you sign in").is_err());
    assert!(ui.find("Open Discord links in Yapfast").is_err());
}
