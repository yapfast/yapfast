//! A message's gallery, files and stickers: task C04's tests, driving
//! `anomi_chat::attachments::view` over its fixtures in the simulator
//! (`super::support`).

use iced::{Event, Point, Size, mouse};
use iced_test::Simulator;

use anomi_chat::attachments::view;
use anomi_theme::{Renderer, Theme};
use anomi_vm::chat::attachments::{Attachments, Gallery, Intent};
use anomi_vm::fixtures::chat::attachments as sample;

use super::support;

const WINDOW: Size = Size::new(700.0, 700.0);
const ITEM: u64 = sample::ITEM;

type Ui<'a> = Simulator<'a, Intent, Theme, Renderer>;

fn simulate(attachments: &Attachments) -> Ui<'_> {
    simulate_in(attachments, WINDOW)
}

fn simulate_in(attachments: &Attachments, window: Size) -> Ui<'_> {
    Simulator::with_size(
        support::settings(),
        window,
        view(attachments, support::front()),
    )
}

fn at(ui: &mut Ui<'_>, x: f32, y: f32) {
    let position = Point::new(x, y);
    ui.point_at(position);
    ui.simulate([Event::Mouse(mouse::Event::CursorMoved { position })]);
}

fn click(ui: &mut Ui<'_>, x: f32, y: f32, button: mouse::Button) {
    at(ui, x, y);
    ui.simulate([
        Event::Mouse(mouse::Event::ButtonPressed(button)),
        Event::Mouse(mouse::Event::ButtonReleased(button)),
    ]);
}

fn asked(attachments: &Attachments, x: f32, y: f32, button: mouse::Button) -> Vec<Intent> {
    let mut ui = simulate(attachments);
    click(&mut ui, x, y, button);
    support::asked(ui)
}

#[test]
fn the_mosaic_puts_the_tall_one_left_and_two_right() {
    let gallery = sample::gallery().gallery;
    let places: Vec<_> = gallery
        .cells
        .iter()
        .map(|cell| (cell.x, cell.y, cell.width, cell.height))
        .collect();
    assert_eq!(
        places,
        [(0, 0, 273, 550), (277, 0, 273, 273), (277, 277, 273, 273)]
    );
    assert_eq!((gallery.width, gallery.height), (550, 550));
    let five = sample::five().gallery;
    assert_eq!(five.cells[0].width, 550);
    assert_eq!(five.height, 3 * 273 + 2 * 4);
    assert_eq!(Gallery::mosaic(Vec::new()), Gallery::default());
}

#[test]
fn a_click_on_a_picture_opens_it_and_a_right_click_asks_its_menu() {
    let single = sample::single();
    assert_eq!(
        asked(&single, 50.0, 50.0, mouse::Button::Left),
        [Intent::Open {
            item: ITEM,
            index: 0
        }]
    );
    let asks = asked(&single, 50.0, 50.0, mouse::Button::Right);
    assert!(matches!(
        asks.as_slice(),
        [Intent::Menu {
            item: ITEM,
            index: 0,
            video: false,
            ..
        }]
    ));
}

#[test]
fn a_narrow_timeline_shrinks_the_gallery_keeping_its_shape() {
    // 400 by 300 in 200 px is 200 by 150: a click under that misses it.
    let single = sample::single();
    let narrow = Size::new(200.0, 700.0);
    let mut ui = simulate_in(&single, narrow);
    click(&mut ui, 100.0, 4.0 + 140.0, mouse::Button::Left);
    assert_eq!(
        support::asked(ui),
        [Intent::Open {
            item: ITEM,
            index: 0
        }]
    );
    let mut ui = simulate_in(&single, narrow);
    click(&mut ui, 100.0, 4.0 + 200.0, mouse::Button::Left);
    assert!(support::asked(ui).is_empty());
    // The file card, 432 px where there is room, takes the 200 there are:
    // Save sits 8 px in from its end.
    let files = sample::files();
    let mut ui = simulate_in(&files, narrow);
    click(&mut ui, 200.0 - 24.0, 4.0 + 32.0, mouse::Button::Left);
    assert!(support::asked(ui).contains(&Intent::Save {
        item: ITEM,
        index: 0
    }));
}

#[test]
fn a_covered_spoiler_reveals_instead_of_opening() {
    let asks = asked(&sample::spoiler(), 50.0, 50.0, mouse::Button::Left);
    assert_eq!(
        asks,
        [Intent::Reveal {
            item: ITEM,
            index: 0
        }]
    );
    assert!(simulate(&sample::spoiler()).find("SPOILER").is_ok());
}

#[test]
fn save_shows_on_the_lit_message_and_asks_to_save() {
    // Top right of the first cell of the mosaic, 8 px in and 16 to its centre.
    let lit = sample::lit_gallery();
    let asks = asked(&lit, 273.0 - 24.0, 4.0 + 24.0, mouse::Button::Left);
    assert!(asks.contains(&Intent::Save {
        item: ITEM,
        index: 0
    }));
    // Without the pointer on the message or the picture, there is no Save.
    let asks = asked(
        &sample::single(),
        400.0 - 24.0,
        4.0 + 100.0,
        mouse::Button::Left,
    );
    assert!(!asks.iter().any(|ask| matches!(ask, Intent::Save { .. })));
}

#[test]
fn nothing_saves_while_a_message_is_going_out() {
    let asks = asked(
        &sample::undelivered(),
        273.0 - 24.0,
        4.0 + 24.0,
        mouse::Button::Left,
    );
    assert!(!asks.iter().any(|ask| matches!(ask, Intent::Save { .. })));
}

#[test]
fn a_gif_has_its_star_while_the_pointer_is_on_it() {
    let gif = sample::gif(false);
    let mut ui = simulate(&gif);
    click(&mut ui, 8.0 + 16.0, 4.0 + 8.0 + 16.0, mouse::Button::Left);
    assert!(support::asked(ui).contains(&Intent::Favorite {
        item: ITEM,
        index: 0
    }));
}

#[test]
fn a_file_card_names_the_file_and_saves_it() {
    let files = sample::files();
    let mut ui = simulate(&files);
    assert!(ui.find("trip-notes.pdf").is_ok());
    assert!(ui.find("2.4 MB").is_ok());
    // Save is at the card's end, 8 px in.
    click(&mut ui, 432.0 - 24.0, 4.0 + 32.0, mouse::Button::Left);
    assert!(support::asked(ui).contains(&Intent::Save {
        item: ITEM,
        index: 0
    }));
}

#[test]
fn a_sticker_opens_its_card() {
    let stickers = sample::stickers();
    let asks = asked(&stickers, 80.0, 4.0 + 80.0, mouse::Button::Left);
    assert!(matches!(
        asks.as_slice(),
        [Intent::Sticker {
            item: ITEM,
            index: 0,
            ..
        }]
    ));
}

#[test]
fn a_message_without_attachments_draws_and_asks_nothing() {
    let none = Attachments::default();
    assert!(none.is_empty());
    assert!(asked(&none, 10.0, 10.0, mouse::Button::Left).is_empty());
}
