//! A server's Stickers settings: task X11's tests, driving
//! `anomi_settings::server::stickers::{view, dialog}` over its fixtures in
//! the simulator (`super::support`).

use iced::Widget as _;
use iced::{Event, mouse};
use iced_test::simulator::Simulator;

use anomi_settings::server::stickers::{dialog, view};
use anomi_vm::fixtures::settings::stickers as sample;
use anomi_vm::settings::stickers::{Intent, Stickers, UPLOAD};

use super::support::{SIZE, asked, front, settings};

fn simulate(page: &Stickers) -> Simulator<'_, Intent, anomi_theme::Theme, anomi_theme::Renderer> {
    Simulator::with_size(settings(), SIZE, view(page, front()).boxed())
}

#[test]
fn the_cards_show_their_tiles_and_upload_asks_to_open() {
    let page = sample::stickers();
    let mut ui = simulate(&page);
    assert!(ui.find("Wave").is_ok());
    assert!(ui.find("2 of 5 slots available").is_ok());
    ui.click("Upload Sticker").expect("the tile is there");
    assert_eq!(asked(ui), [Intent::Upload]);
}

#[test]
fn a_reader_has_no_upload_tile() {
    let mut page = sample::stickers();
    page.may_upload = false;
    assert!(simulate(&page).find("Upload Sticker").is_err());
}

/// Moves the pointer to `at` and clicks there when `click`.
fn press_at(
    ui: &mut Simulator<'_, Intent, anomi_theme::Theme, anomi_theme::Renderer>,
    at: iced::Point,
    click: bool,
) {
    ui.point_at(at);
    ui.simulate([Event::Mouse(mouse::Event::CursorMoved { position: at })]);
    if click {
        ui.simulate([
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        ]);
    }
}

#[test]
fn a_hovered_tile_offers_edit_and_delete() {
    let mut page = sample::stickers();
    page.hovered = Some(sample::BOUNCE.into());
    let mut ui = simulate(&page);
    // The buttons draw no text: aim by the name under them. It spans the
    // tile less its 10 px padding and starts 96 px below its top; the 28 px
    // buttons sit 4 px in from the top right corner, 2 px apart.
    let name = ui
        .find("Bounce")
        .expect("the tile is there")
        .visible_bounds()
        .expect("it is in view");
    let (right, y) = (name.x + name.width + 10.0, name.y - 96.0 + 18.0);
    for x in [right - 48.0, right - 18.0] {
        press_at(&mut ui, iced::Point::new(x, y), true);
    }
    assert_eq!(
        asked(ui),
        [
            Intent::Hover(sample::BOUNCE.into(), true),
            Intent::Edit(sample::BOUNCE.into()),
            Intent::Delete(sample::BOUNCE.into())
        ]
    );
}

#[test]
fn crossing_from_a_tile_to_upload_moves_the_hover() {
    let page = sample::stickers();
    let mut ui = simulate(&page);
    let at = |ui: &mut Simulator<'_, Intent, _, _>, words: &str| {
        let bounds = ui
            .find(words)
            .expect("it is there")
            .visible_bounds()
            .expect("it is in view");
        bounds.center()
    };
    let (lottie, upload) = (at(&mut ui, "Lottie"), at(&mut ui, "Upload Sticker"));
    press_at(&mut ui, lottie, false);
    press_at(&mut ui, upload, false);
    assert_eq!(
        asked(ui),
        [
            Intent::Hover(sample::LOTTIE.into(), true),
            Intent::Hover(sample::LOTTIE.into(), false),
            Intent::Hover(UPLOAD.into(), true),
        ]
    );
}

#[test]
fn the_form_saves_or_cancels() {
    let form = sample::form();
    let mut ui = Simulator::with_size(settings(), SIZE, dialog(&form, front()));
    ui.click("Browse").expect("the button is there");
    ui.click("Upload").expect("the button is there");
    ui.click("Cancel").expect("the button is there");
    assert_eq!(asked(ui), [Intent::Browse, Intent::Save, Intent::Cancel]);
}

#[test]
fn an_edit_has_no_file_and_an_unfinished_form_cannot_save() {
    let mut form = sample::form();
    form.editing = true;
    form.may_save = false;
    form.error = "Too big".into();
    let mut ui = Simulator::with_size(settings(), SIZE, dialog(&form, front()));
    assert!(ui.find("Browse").is_err());
    assert!(ui.find("Too big").is_ok());
    let _ = ui.click("Update");
    assert!(asked(ui).is_empty());
}
