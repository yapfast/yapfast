//! A call's tiles: task V03's tests, driving `anomi_voice::tiles::view`
//! over its fixtures in the simulator (`super::support`). Each tile is laid
//! out at the size of the stage's in `voice/stage.png`, 420 by 236; `render`
//! draws the goldens' tiles where the goldens have them.

mod render;

use iced::{Event, Point, Rectangle, Size, mouse};
use iced_test::selector::Candidate;
use iced_test::{Simulator, simulator};

use anomi_theme::{Renderer, Theme};
use anomi_vm::fixtures::voice::tiles as samples;
use anomi_vm::menu::Spot;
use anomi_vm::voice::tiles::{Intent, Key, Tile};
use anomi_voice::tiles;

use super::support::{asked, front, settings};

const SIZE: Size = Size::new(420.0, 236.0);
const ALICE: &str = "1400000000000000001";
const BOB: &str = "1400000000000000002";

/// Its stop button at the top right, and the bottom right's two.
const STOP: Point = Point::new(SIZE.width - 26.0, 26.0);
const FULL: Point = Point::new(SIZE.width - 26.0, SIZE.height - 26.0);
const HIDE: Point = Point::new(SIZE.width - 66.0, SIZE.height - 26.0);

fn simulate(tile: &Tile) -> Simulator<'_, Intent, Theme, Renderer> {
    Simulator::with_size(settings(), SIZE, tiles::view(tile, front()))
}

/// The pointer moved onto the tile at `at`.
fn point(ui: &mut Simulator<'_, Intent, Theme, Renderer>, at: Point) {
    ui.point_at(at);
    ui.simulate([Event::Mouse(mouse::Event::CursorMoved { position: at })]);
}

fn click_at(tile: &Tile, at: Point) -> Vec<Intent> {
    let mut ui = simulate(tile);
    point(&mut ui, at);
    ui.simulate(simulator::click());
    asked(ui)
}

fn double_click(tile: &Tile) -> Vec<Intent> {
    let mut ui = simulate(tile);
    point(&mut ui, Point::new(200.0, 100.0));
    ui.simulate(simulator::click());
    ui.simulate(simulator::click());
    asked(ui)
}

fn right_click_at(tile: &Tile, at: Point) -> Vec<Intent> {
    let mut ui = simulate(tile);
    point(&mut ui, at);
    ui.simulate([
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)),
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Right)),
    ]);
    asked(ui)
}

/// Bob's stream watched in his tile, shown large.
fn watched() -> Tile {
    samples::watching().swap_remove(1)
}

#[test]
fn a_click_on_someone_shows_them_large() {
    let alice = samples::alice();
    let mut ui = simulate(&alice);
    ui.click("Alice").expect("her chip");
    assert_eq!(asked(ui), [Intent::Focus(Key::Person(ALICE.into()))]);
}

#[test]
fn a_click_on_a_stream_offered_watches_it() {
    let bob = samples::bob();
    let mut ui = simulate(&bob);
    assert!(ui.find("LIVE").is_ok(), "his chip says he is live");
    ui.click("Watch stream").expect("the pill at the top right");
    assert_eq!(asked(ui), [Intent::Watch(BOB.into())]);
}

#[test]
fn a_stream_in_its_own_tile_offers_itself_in_the_middle() {
    for preview in [false, true] {
        let offered = samples::offered(preview);
        let mut ui = simulate(&offered);
        let pill = ui.find("Watch stream").expect("the pill").bounds();
        assert!((pill.center_x() - SIZE.width / 2.0).abs() < 1.0, "{pill:?}");
        assert!(pill.y > SIZE.height / 2.0 && pill.y < SIZE.height / 2.0 + 30.0);
        assert_eq!(
            click_at(&offered, Point::new(60.0, 60.0)),
            [Intent::Watch(BOB.into())]
        );
    }
}

#[test]
fn the_large_tile_asks_nothing_of_a_click_but_goes_full_screen_on_two() {
    let large = watched();
    assert_eq!(click_at(&large, Point::new(200.0, 100.0)), []);
    assert_eq!(double_click(&large), [Intent::FullScreen(true)]);
    let full = Tile {
        full: true,
        ..watched()
    };
    assert_eq!(double_click(&full), [Intent::FullScreen(false)]);
}

#[test]
fn a_right_click_opens_their_menu_at_the_pointer() {
    let at = Point::new(120.0, 90.0);
    assert_eq!(
        right_click_at(&samples::alice(), at),
        [Intent::Menu(ALICE.into(), Spot::pointer(at))]
    );
    let stream = samples::with_camera().swap_remove(0);
    assert_eq!(
        right_click_at(&stream, at),
        [Intent::Menu(BOB.into(), Spot::pointer(at))]
    );
    assert_eq!(right_click_at(&samples::sharing(), at), []);
}

#[test]
fn the_large_tile_under_the_pointer_has_its_buttons() {
    assert_eq!(click_at(&watched(), STOP), [Intent::StopWatching]);
    assert_eq!(click_at(&watched(), HIDE), [Intent::HideOthers(true)]);
    assert_eq!(click_at(&watched(), FULL), [Intent::FullScreen(true)]);
    let hidden = Tile {
        others_hidden: true,
        ..watched()
    };
    assert_eq!(click_at(&hidden, HIDE), [Intent::HideOthers(false)]);
    let full = Tile {
        full: true,
        ..watched()
    };
    // Full screen leaves the large tile alone: nothing to hide.
    assert_eq!(click_at(&full, HIDE), []);
    assert_eq!(click_at(&full, FULL), [Intent::FullScreen(false)]);
}

#[test]
fn a_small_tile_has_no_large_tile_buttons() {
    let small = samples::waiting();
    assert_eq!(
        click_at(&small, FULL),
        [Intent::Focus(Key::Person(BOB.into()))]
    );
    assert_eq!(click_at(&small, STOP), [Intent::StopWatching]);
}

#[test]
fn a_press_on_the_viewers_asks_nothing() {
    let small = samples::waiting();
    // Under the pointer they make room for the stop button.
    let mut ui = simulate(&small);
    point(&mut ui, Point::new(100.0, 150.0));
    let more = ui.find("+2").expect("the viewers").bounds();
    let at = Point::new(more.center_x(), more.center_y());
    let right = [
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)),
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Right)),
    ];
    let presses: [Vec<Event>; 3] = [
        simulator::click().collect(),
        right.to_vec(),
        simulator::click().chain(simulator::click()).collect(),
    ];
    for press in presses {
        let mut ui = simulate(&small);
        point(&mut ui, at);
        ui.simulate(press.clone());
        assert_eq!(asked(ui), [], "{press:?}");
    }
}

#[test]
fn the_viewers_make_room_for_the_stop_button_under_the_pointer() {
    let small = samples::waiting();
    let mut ui = simulate(&small);
    assert!(ui.find("5").is_ok(), "five watch");
    let rest = ui.find("+2").expect("two beyond the faces").bounds();
    point(&mut ui, Point::new(100.0, 150.0));
    let hovered = ui.find("+2").expect("still there").bounds();
    assert!(
        (rest.x - hovered.x - 40.0).abs() < 0.5,
        "{rest:?} {hovered:?}"
    );
    assert!((rest.y - hovered.y).abs() < 0.5);
    assert!(rest.x + rest.width < SIZE.width - 10.0);

    let large = watched();
    let mut ui = simulate(&large);
    assert!(ui.find("1").is_ok());
    assert!(ui.find("+0").is_err(), "nobody beyond the faces");
}

#[test]
fn what_a_tile_waits_for_is_said() {
    let waiting = samples::waiting();
    let mut ui = simulate(&waiting);
    assert!(ui.find("Connecting to Bob's stream...").is_ok());

    let starting = samples::sharing_starting();
    let mut ui = simulate(&starting);
    assert!(ui.find("Starting your stream...").is_ok());
    assert!(ui.find("Your screen").is_ok());
    assert!(ui.find("LIVE").is_err());

    let sharing = samples::sharing();
    let mut ui = simulate(&sharing);
    assert!(ui.find("LIVE").is_ok());
    assert!(
        ui.find("Live").is_err(),
        "the status shows only before a picture"
    );

    let camera = samples::starting();
    let mut ui = simulate(&camera);
    let pill = ui.find("Starting camera...").expect("the pill").bounds();
    assert!(pill.x < 25.0 && pill.y < 25.0, "at the top left: {pill:?}");
}

#[test]
fn a_note_follows_the_name() {
    let carol = samples::joining().swap_remove(1);
    let mut ui = simulate(&carol);
    let name = ui.find("Carol").expect("her name").bounds();
    let note = ui.find("Connection lost").expect("the note").bounds();
    assert!(note.x > name.x + name.width);
    assert!((note.center_y() - name.center_y()).abs() < 1.0);
}

/// The outermost container `height` tall that holds `inside`'s middle.
fn holding(
    ui: &mut Simulator<'_, Intent, Theme, Renderer>,
    inside: Rectangle,
    height: f32,
) -> Rectangle {
    let middle = inside.center();
    ui.find(move |candidate: Candidate<'_>| match candidate {
        Candidate::Container { bounds, .. }
            if bounds.contains(middle) && (bounds.height - height).abs() < 0.5 =>
        {
            Some(bounds)
        }
        _ => None,
    })
    .expect("a container")
}

#[test]
fn a_long_name_leaves_the_live_tag_its_room() {
    const NAME: &str = "Bartholomew Fitzgerald Ashcomb";
    const SMALL: Size = Size::new(192.0, 108.0);
    let full = simulate(&samples::bob())
        .find("LIVE")
        .expect("his tag")
        .bounds();
    let mut long = samples::bob();
    long.chip.name = NAME.into();
    let mut ui = Simulator::with_size(settings(), SMALL, tiles::view(&long, front()));
    let live = ui.find("LIVE").expect("his tag").bounds();
    assert!((live.width - full.width).abs() < 0.5, "{live:?} {full:?}");
    let tag = holding(&mut ui, live, 16.0);
    assert!((tag.width - (live.width + 10.0)).abs() < 0.5, "{tag:?}");
    let chip = holding(&mut ui, live, 26.0);
    assert!(chip.x >= 10.0 - 0.5, "{chip:?}");
    assert!(chip.width <= SMALL.width - 20.0 + 0.5, "{chip:?}");
    assert!(
        tag.x >= chip.x && tag.x + tag.width <= chip.x + chip.width,
        "{tag:?}"
    );
    let name = ui.find(NAME).expect("his name, cut short").bounds();
    assert!(
        name.width > 0.0 && name.x + name.width <= tag.x,
        "{name:?} {tag:?}"
    );
}

#[test]
fn the_chip_sits_at_the_bottom_left() {
    let dave = samples::dave();
    let mut ui = simulate(&dave);
    let name = ui.find("Dave").expect("his name").bounds();
    // The chip is 26 tall, 10 from the foot; the mark and 6 px before the
    // name, 8 inside the chip.
    assert!(
        (name.center_y() - (SIZE.height - 23.0)).abs() < 1.5,
        "{name:?}"
    );
    assert!((name.x - (10.0 + 8.0 + 14.0 + 6.0)).abs() < 1.0, "{name:?}");
}
