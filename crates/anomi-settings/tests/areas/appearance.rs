//! The Appearance settings page: task S06's tests, driving
//! `anomi_settings::appearance::view` over its fixtures in the simulator
//! (`super::support`).

use iced::Widget as _;
use iced_test::simulator::Simulator;

use anomi_settings::appearance::page;
use anomi_vm::fixtures::settings::appearance as sample;
use anomi_vm::settings::appearance::{Appearance, Intent, Look};

use super::support::{SIZE, asked, front, settings};

fn simulate(
    appearance: &Appearance,
) -> Simulator<'_, Intent, anomi_theme::Theme, anomi_theme::Renderer> {
    Simulator::with_size(settings(), SIZE, page(appearance, front()).boxed())
}

#[test]
fn a_tile_chooses_its_theme_and_the_row_below_does_too() {
    let page = sample::appearance();
    let mut ui = simulate(&page);
    ui.click("Midnight").expect("the tile is there");
    ui.click("Tokyo Night").expect("the tile is there");
    assert_eq!(
        asked(ui),
        [
            Intent::ChooseTheme("midnight".into()),
            Intent::ChooseTheme("tokyo-night".into())
        ]
    );
}

#[test]
fn the_buttons_by_the_headings_ask_for_their_actions() {
    let page = sample::appearance();
    let mut ui = simulate(&page);
    ui.click("Import").expect("import is there");
    ui.click("New theme").expect("new theme is there");
    ui.click("Add font files").expect("add is there");
    assert_eq!(
        asked(ui),
        [Intent::Import, Intent::NewTheme, Intent::AddFonts]
    );
}

#[test]
fn a_font_adding_is_not_offered_again_and_reset_shows_with_files() {
    let page = Appearance {
        font_importing: true,
        ..sample::appearance()
    };
    let mut ui = simulate(&page);
    assert!(ui.find("Adding…").is_ok());
    assert!(ui.find("Add font files").is_err());
    assert!(ui.find("Back to the bundled font").is_err());
    let page = sample::with_fonts().settings.appearance;
    let mut ui = simulate(&page);
    ui.click("Back to the bundled font")
        .expect("reset is offered");
    // The pencil on the person's own theme: an icon its tooltip names, which is no text to find.
    ui.point_at(iced::Point::new(850.5, 302.0));
    ui.simulate(iced_test::simulator::click());
    assert_eq!(
        asked(ui),
        [Intent::ResetFont, Intent::EditTheme("custom-1".into())]
    );
}

#[test]
fn the_title_bar_choice_shows_only_where_it_is_offered() {
    let page = Appearance {
        title_bar_choice: false,
        ..sample::appearance()
    };
    assert!(simulate(&page).find("Use the system title bar").is_err());
    let page = sample::appearance();
    let mut ui = simulate(&page);
    ui.click("Use the system title bar")
        .expect("the switch is there");
    assert_eq!(asked(ui), [Intent::SystemTitleBar(false)]);
}

#[test]
fn a_look_switch_asks_for_its_flip() {
    let page = sample::appearance();
    let mut ui = simulate(&page);
    ui.click("Show nameplates").expect("the switch is there");
    ui.click("Play profile effects")
        .expect("the switch is there");
    assert_eq!(
        asked(ui),
        [
            Intent::Look(Look::Nameplates, false),
            Intent::Look(Look::ProfileEffects, false)
        ]
    );
}
