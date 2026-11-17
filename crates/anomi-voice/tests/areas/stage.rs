//! The participant grid: task V04's tests, driving `anomi_voice::stage::view`
//! over its fixtures in the simulator (`super::support`), in the goldens'
//! room for it: 1024 by 694.5 under the call's line and over the pill.
//! Where each tile lies there is `stage::Geometry`'s, which its own tests
//! hold to the goldens; these click on them.

use std::time::{Duration, Instant};

use iced::widget::container;
use iced::{Event, Point, Size, mouse, window};
use iced_test::{Simulator, simulator};

use anomi_theme::{Motion, Renderer, Theme};
use anomi_vm::fixtures::voice::{controls as pill, stage as samples};
use anomi_vm::menu::Spot;
use anomi_vm::voice::stage::{self, Placement, Stage};
use anomi_vm::voice::tiles::{self, Key};
use anomi_vm::voice::{Intent, Voice, controls};
use anomi_voice::controls::{BUTTON, DIVIDER, LEAVE, PAD, SPACING};
use anomi_voice::stage::view;

use super::support::{asked, front, settings};

const ROOM: Size = Size::new(1024.0, 694.5);
const BOB: &str = "1400000000000000002";
const CAROL: &str = "1400000000000000003";
const DAVE: &str = "1400000000000000004";

type Ui<'a> = Simulator<'a, Intent, Theme, Renderer>;

fn simulate(stage: &Stage, motion: Motion) -> Ui<'_> {
    Simulator::with_size(settings(), ROOM, view(stage, motion))
}

fn point(ui: &mut Ui<'_>, at: Point) {
    ui.point_at(at);
    ui.simulate([Event::Mouse(mouse::Event::CursorMoved { position: at })]);
}

fn click_at(stage: &Stage, at: Point) -> Vec<Intent> {
    let mut ui = simulate(stage, front());
    point(&mut ui, at);
    ui.simulate(simulator::click());
    asked(ui)
}

fn frame(ui: &mut Ui<'_>, now: Instant) {
    ui.simulate([Event::Window(window::Event::RedrawRequested(now))]);
}

fn focus(id: &str) -> Intent {
    tiles::Intent::Focus(Key::Person(id.into())).into()
}

#[test]
fn a_click_on_someone_shows_them_large() {
    let lounge = samples::lounge();
    let mut ui = simulate(&lounge, front());
    ui.click("Carol").expect("her chip");
    assert_eq!(asked(ui), [focus(CAROL)]);
}

#[test]
fn a_click_on_a_stream_offered_watches_it() {
    // Bob's tile, the second of the first row.
    let asked = click_at(&samples::lounge(), Point::new(728.0, 223.0));
    assert_eq!(asked, [tiles::Intent::Watch(BOB.into()).into()]);
}

#[test]
fn a_right_click_opens_their_menu_where_it_was() {
    let lounge = samples::lounge();
    let mut ui = simulate(&lounge, front());
    let at = Point::new(300.0, 400.0);
    point(&mut ui, at);
    ui.simulate([
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)),
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Right)),
    ]);
    let menu = tiles::Intent::Menu(CAROL.into(), Spot::pointer(at));
    assert_eq!(asked(ui), [menu.into()]);
}

#[test]
fn a_click_beside_the_large_tile_shows_them_all() {
    let watching = samples::watching();
    assert_eq!(
        click_at(&watching, Point::new(20.0, 300.0)),
        [stage::Intent::ShowAll.into()]
    );
    // The large one asks nothing more of a click; a small one shows large.
    assert_eq!(click_at(&watching, Point::new(500.0, 300.0)), []);
    assert_eq!(
        click_at(&watching, Point::new(500.0, 600.0)),
        [focus(CAROL)]
    );
    // With none large, beside the tiles is nothing.
    assert_eq!(click_at(&samples::lounge(), Point::new(20.0, 20.0)), []);
}

#[test]
fn full_screen_shows_the_large_tile_alone() {
    let full = samples::full();
    let mut ui = simulate(&full, front());
    assert!(ui.find("Carol").is_err(), "the others aren't built");
    assert!(ui.find("Bob").is_ok());
    point(&mut ui, Point::new(10.0, 10.0));
    ui.simulate(simulator::click());
    ui.simulate(simulator::click());
    assert_eq!(asked(ui), [tiles::Intent::FullScreen(false).into()]);
}

#[test]
fn hidden_others_leave_the_large_tile_alone_until_a_click_beside_it() {
    let hidden = samples::others_hidden();
    let mut ui = simulate(&hidden, front());
    assert!(ui.find("Carol").is_err());
    // Where the row beneath was, under the large tile now as wide as the
    // room allows: 976 by 549, 72.75 px down.
    point(&mut ui, Point::new(500.0, 660.0));
    ui.simulate(simulator::click());
    assert_eq!(asked(ui), [stage::Intent::ShowAll.into()]);
}

#[test]
fn the_grid_tells_the_app_where_its_tiles_are() {
    let lounge = samples::lounge();
    let mut ui = simulate(&lounge, front());
    let now = Instant::now();
    frame(&mut ui, now);
    frame(&mut ui, now + Duration::from_millis(16));
    let placed = Placement {
        columns: 2,
        width: 420.0,
        height: 236.25,
        gap: 12.0,
        top: 105.0,
        scrolled: 0.0,
        shown: ROOM.height,
        focused: None,
        focus_width: 0.0,
        focus_height: 0.0,
    };
    assert_eq!(asked(ui), [stage::Intent::Placed(placed).into()], "once");
}

#[test]
fn many_scroll_under_the_wheel() {
    // Ten: three columns of 317 by 178.5, four rows, 103.5 px to scroll.
    let ten = samples::ten();
    let mut ui = simulate(&ten, front());
    point(&mut ui, Point::new(512.0, 300.0));
    ui.scroll(mouse::ScrollDelta::Lines { x: 0.0, y: -1.0 });
    frame(&mut ui, Instant::now());
    // Dave's tile, the second row's first, is now where Alice's was.
    point(&mut ui, Point::new(100.0, 184.5));
    ui.simulate(simulator::click());
    let asked = asked(ui);
    let Some(Intent::Stage(stage::Intent::Placed(placed))) = asked.first() else {
        panic!("placed first: {asked:?}");
    };
    assert_eq!((placed.columns, placed.scrolled), (3, 60.0));
    assert_eq!(asked[1..], [focus(DAVE)]);
}

#[test]
fn a_tile_shown_large_glides_there() {
    let (lounge, watching) = (samples::lounge(), samples::watching());
    let chip = |ui: &mut Ui<'_>| ui.find("Bob").expect("his chip").bounds().x;
    let mut ui = simulate(&lounge, front());
    let before = chip(&mut ui);
    let mut ui = ui.rebuild(view(&watching, front()));
    let start = Instant::now();
    frame(&mut ui, start);
    assert_eq!(chip(&mut ui), before, "sets off from where it was");
    frame(&mut ui, start + Duration::from_millis(90));
    let half = chip(&mut ui);
    frame(&mut ui, start + Duration::from_millis(200));
    let after = chip(&mut ui);
    assert!(after < half && half < before, "{before} > {half} > {after}");
    // The large tile's chip, 10 px in and 8 more from its left at 44.
    assert_eq!(after, 62.0);

    // A still window shows it there at once.
    let still = Motion::default();
    let ui = simulate(&lounge, still);
    let mut ui = ui.rebuild(view(&watching, still));
    frame(&mut ui, Instant::now());
    assert_eq!(chip(&mut ui), 62.0);
}

#[test]
fn a_room_that_grows_with_the_large_tile_still_glides() {
    // A DM's call grows while a tile is large (N01's `shared`): the room
    // changes with the tiles, and they glide all the same.
    let (lounge, watching) = (samples::lounge(), samples::watching());
    let taller = ROOM.height + 120.0;
    let chip = |ui: &mut Ui<'_>| ui.find("Bob").expect("his chip").bounds().x;
    let mut ui: Ui<'_> = Simulator::with_size(
        settings(),
        Size::new(ROOM.width, taller),
        container(view(&lounge, front())).height(ROOM.height),
    );
    let before = chip(&mut ui);
    let mut ui = ui.rebuild(container(view(&watching, front())).height(taller));
    let start = Instant::now();
    frame(&mut ui, start);
    assert_eq!(chip(&mut ui), before, "sets off from where it was");
    frame(&mut ui, start + Duration::from_millis(90));
    let half = chip(&mut ui);
    frame(&mut ui, start + Duration::from_millis(400));
    let after = chip(&mut ui);
    assert!(after < half && half < before, "{before} > {half} > {after}");
}

#[test]
fn the_call_in_the_conversation_has_the_pill_under_its_grid() {
    let voice = Voice {
        stage: samples::lounge(),
        controls: pill::joined(),
        ..Voice::default()
    };
    let size = Size::new(ROOM.width, ROOM.height + 80.0);
    let mut ui: Ui<'_> = Simulator::with_size(
        settings(),
        size,
        anomi_voice::stage::with_controls(&voice, front()),
    );
    frame(&mut ui, Instant::now());
    // The pill's first button, mute: five buttons, two dividers and the
    // hang-up in the middle, 8 px under the grid.
    let pill = 5.0 * BUTTON + 2.0 * DIVIDER + LEAVE + 7.0 * SPACING + 2.0 * PAD;
    let mute = Point::new(
        (size.width - pill) / 2.0 + PAD + BUTTON / 2.0,
        ROOM.height + 8.0 + PAD + BUTTON / 2.0,
    );
    point(&mut ui, mute);
    ui.simulate(simulator::click());
    let asked = asked(ui);
    let Some(Intent::Stage(stage::Intent::Placed(placed))) = asked.first() else {
        panic!("placed first: {asked:?}");
    };
    assert_eq!(
        placed.shown, ROOM.height,
        "the grid has the room over the pill"
    );
    assert_eq!(asked[1..], [controls::Intent::ToggleMute.into()]);
}

// TEMPORARY: V04's look check, removed before the hand-off.
#[test]
#[ignore]
fn zz_render_for_review() {
    use anomi_vm::fixtures::voice::stage as s;
    use iced::Background;
    use iced::Widget as _;
    let out = std::path::Path::new(
        "/tmp/claude-1000/-home-milaha-Documents-yapfast/f62344d3-f734-41cb-a91f-d689dab9dff1/scratchpad/v04-snap",
    );
    let _ = std::fs::remove_dir_all(out);
    anomi_widgets::picture::set_scale(2.0);
    let cases: Vec<(&str, anomi_vm::Vm, Size, &str)> = vec![
        ("stage", s::window(s::lounge()), Size::new(1024.0, 774.5), "bonfire"),
        ("speaking", s::speaking_window(), Size::new(1024.0, 774.5), "bonfire"),
        ("watching", s::window(s::watching()), Size::new(1024.0, 774.5), "bonfire"),
        ("camera", s::camera_window(), Size::new(1024.0, 774.5), "bonfire"),
        ("light", s::quiet_window(), Size::new(1024.0, 774.5), "catppuccin-latte"),
        ("small", s::window(s::lounge()), Size::new(624.0, 555.0), "bonfire"),
        ("dm", s::dm_window(s::dm_call()), Size::new(704.0, 439.0), "bonfire"),
        ("sharing", s::window(s::sharing()), Size::new(1024.0, 774.5), "bonfire"),
        ("ten", s::window(s::ten()), Size::new(1024.0, 774.5), "bonfire"),
        ("hidden", s::window(s::others_hidden()), Size::new(1024.0, 774.5), "bonfire"),
        ("full", s::window(s::full()), Size::new(1400.0, 860.0), "bonfire"),
    ];
    for (name, vm, size, theme) in cases {
        let voice = vm.voice.clone();
        let full = name == "full";
        let content: anomi_theme::Element<'_, Intent> = if full {
            view(&voice.stage, Motion::default()).boxed()
        } else {
            anomi_voice::stage::with_controls(&voice, Motion::default()).boxed()
        };
        let ground = container(content)
            .width(iced::Fill)
            .height(iced::Fill)
            .style(move |theme: &Theme| container::Style {
                background: Some(Background::Color(if full {
                    iced::Color::BLACK
                } else {
                    theme.palette().stage
                })),
                ..container::Style::default()
            });
        let mut ui: Ui<'_> = Simulator::with_size(settings(), size, ground);
        frame(&mut ui, Instant::now());
        let shot = ui.snapshot(&Theme::built_in(Some(theme))).expect("a snapshot");
        shot.matches_image(out.join(name)).expect("saved");
    }
}
