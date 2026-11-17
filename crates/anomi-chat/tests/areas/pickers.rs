//! The expression picker above the composer, with its tabs: task C18's tests,
//! driving `anomi_chat::pickers::view` over its fixtures in the simulator
//! (`super::support`).

use iced::{Event, Point, mouse};
use iced_test::Simulator;

use anomi_chat::pickers::view;
use anomi_theme::{Renderer, Theme};
use anomi_vm::chat::pickers::{Intent, Pickers, Tab};
use anomi_vm::chat::{self};
use anomi_vm::fixtures::chat::pickers as sample;

use super::support::{self, SIZE};

fn simulate(pickers: &Pickers) -> Simulator<'_, chat::Intent, Theme, Renderer> {
    Simulator::with_size(support::settings(), SIZE, view(pickers, support::front()))
}

fn tab(ui_tab: Tab) -> chat::Intent {
    chat::Intent::from(Intent::Tab(ui_tab))
}

#[test]
fn the_tabs_choose_the_picker() {
    let pickers = sample::gifs();
    let mut ui = simulate(&pickers);
    ui.click("Emoji").unwrap();
    assert!(support::asked(ui).contains(&tab(Tab::Emoji)));

    let mut ui = simulate(&pickers);
    ui.click("Stickers").unwrap();
    assert!(support::asked(ui).contains(&tab(Tab::Stickers)));

    let emoji = sample::emoji();
    let mut ui = simulate(&emoji);
    ui.click("GIFs").unwrap();
    assert!(support::asked(ui).contains(&tab(Tab::Gifs)));
}

#[test]
fn the_gifs_tab_shows_the_gif_picker() {
    let pickers = sample::gifs();
    let mut ui = simulate(&pickers);
    assert!(ui.find("Search Klipy").is_ok());
    assert!(ui.find("Trending GIFs").is_ok());
}

#[test]
fn a_gif_picked_travels_as_the_chats_intent() {
    let pickers = sample::gifs();
    let mut ui = simulate(&pickers);
    ui.click("Favorites").unwrap();
    let asked = support::asked(ui);
    assert!(asked.iter().any(|intent| matches!(
        intent,
        chat::Intent::GifPicker(anomi_vm::chat::gif_picker::Intent::Show(_))
    )));
}

#[test]
fn the_left_edge_drags_the_card_wider() {
    let pickers = sample::gifs();
    let mut ui = simulate(&pickers);
    ui.point_at(Point::new(4.0, 200.0));
    ui.simulate([Event::Mouse(mouse::Event::ButtonPressed(
        mouse::Button::Left,
    ))]);
    // Up and down leaves the width as it is: nothing to say.
    let move_to = |ui: &mut Simulator<'_, chat::Intent, Theme, Renderer>, at: Point| {
        ui.point_at(at);
        ui.simulate([Event::Mouse(mouse::Event::CursorMoved { position: at })]);
    };
    move_to(&mut ui, Point::new(4.0, 260.0));
    assert!(ui.drain().next().is_none());
    move_to(&mut ui, Point::new(-96.0, 260.0));
    move_to(&mut ui, Point::new(-96.0, 300.0));
    assert_eq!(
        ui.drain().collect::<Vec<_>>(),
        vec![chat::Intent::from(Intent::Drag(598.0))]
    );

    let wider = Pickers {
        width: 598.0,
        room: 900.0,
        ..sample::gifs()
    };
    let mut ui = ui.rebuild(view(&wider, support::front()));
    ui.simulate([Event::Mouse(mouse::Event::ButtonReleased(
        mouse::Button::Left,
    ))]);
    assert_eq!(
        support::asked(ui),
        vec![chat::Intent::from(Intent::Chosen(598.0))]
    );
}
