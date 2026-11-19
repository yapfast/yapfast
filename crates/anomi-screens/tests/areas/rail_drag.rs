//! Dragging servers and folders on the rail, in the whole window, with the
//! app played as it answers the rail's intents: the drag is kept in the
//! view-model, which the window shows again after each. Over
//! `samples::folders()`, the rows from y 65: the unread DM, the line, Server
//! (122), the open folder f1 (170) with F2 (222) and F3 (270), the closed
//! folder f2 (322) and Epsilon (378).

use std::time::{Duration, Instant};

use iced::{Event, Point, keyboard, mouse, window};
use iced_test::Simulator;

use anomi_theme::{Renderer, Theme};
use anomi_vm::fixtures::rail as samples;
use anomi_vm::rail::{self, Drag, Group, Place, arrange};
use anomi_vm::{Intent, Vm};
use anomi_widgets::list::DWELL;

use super::support::{simulate, window};

type Ui = Simulator<'static, Intent, Theme, Renderer>;

/// What a drag came to: the rail's intents, and the order a drop made.
#[derive(Default)]
struct Outcome {
    intents: Vec<rail::Intent>,
    order: Option<String>,
    /// What it was onto after each point: at once, then once the dwell
    /// passed.
    onto: Vec<(Option<String>, Option<String>)>,
    /// The drag the view-model still held once it ended.
    left: Option<Drag>,
}

/// The view-model, kept for the rest of the test as the simulator borrows it.
fn kept(vm: Vm) -> &'static Vm {
    Box::leak(Box::new(vm))
}

/// Answers the intents waiting as the app does, and shows the window
/// again; the list keeps its drag, as the window's tree does.
fn answer(mut ui: Ui, vm: &mut Vm, outcome: &mut Outcome) -> Ui {
    let asked: Vec<Intent> = ui.drain().collect();
    for intent in asked {
        let Intent::Rail(intent) = intent else {
            continue;
        };
        match &intent {
            rail::Intent::Lift(id) => {
                vm.rail.drag = Some(Drag {
                    id: id.clone(),
                    ..Drag::default()
                });
            }
            rail::Intent::Aim(place) => {
                if let Some(drag) = &mut vm.rail.drag {
                    drag.aim(place.clone());
                }
            }
            rail::Intent::Drop => {
                if let Some(drag) = vm.rail.drag.take() {
                    outcome.order = arrange::dropped(&vm.rail, &drag).map(|groups| text(&groups));
                }
            }
            rail::Intent::PutBack => vm.rail.drag = None,
            _ => {}
        }
        outcome.intents.push(intent);
    }
    ui.rebuild(window(kept(vm.clone())))
}

fn text(groups: &[Group]) -> String {
    let number = |id: &String| id.trim_start_matches("discord:g").to_owned();
    groups
        .iter()
        .map(|group| match &group.folder {
            None => number(&group.servers[0]),
            Some(id) => {
                let servers: Vec<String> = group.servers.iter().map(number).collect();
                format!(
                    "{}[{}]",
                    if id.is_empty() { "+" } else { id },
                    servers.join(" ")
                )
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn moved_to(ui: &mut Ui, at: Point) {
    ui.point_at(at);
    let _ = ui.simulate([Event::Mouse(mouse::Event::CursorMoved { position: at })]);
}

/// A frame once the dwell has passed.
fn dwell(ui: &mut Ui) {
    let at = Instant::now() + DWELL + Duration::from_millis(20);
    let _ = ui.simulate([Event::Window(window::Event::RedrawRequested(at))]);
}

/// Picks the row at `y` up and takes it through `to`, waiting out the
/// dwell at each point, then `end`s the drag: a release at the last point,
/// or what `end` does.
fn drag(y: f32, to: &[Point], end: impl FnOnce(&mut Ui)) -> Outcome {
    drag_in(samples::folders_window(), y, to, end)
}

/// [`drag`] in the window `vm` shows.
fn drag_in(mut vm: Vm, y: f32, to: &[Point], end: impl FnOnce(&mut Ui)) -> Outcome {
    let mut outcome = Outcome::default();
    let mut ui = simulate(kept(vm.clone()));
    let start = Point::new(36.0, y);
    ui.point_at(start);
    let _ = ui.simulate([Event::Mouse(mouse::Event::ButtonPressed(
        mouse::Button::Left,
    ))]);
    moved_to(&mut ui, Point::new(36.0, y + 6.0));
    ui = answer(ui, &mut vm, &mut outcome);
    for point in to {
        moved_to(&mut ui, *point);
        ui = answer(ui, &mut vm, &mut outcome);
        let at_once = onto(&vm);
        dwell(&mut ui);
        ui = answer(ui, &mut vm, &mut outcome);
        outcome.onto.push((at_once, onto(&vm)));
    }
    end(&mut ui);
    let _ = answer(ui, &mut vm, &mut outcome);
    outcome.left = vm.rail.drag.clone();
    outcome
}

/// The server or folder the drag is onto: a folder forming around it, or a
/// closed folder taking it in.
fn onto(vm: &Vm) -> Option<String> {
    vm.rail.drag.as_ref().and_then(|drag| drag.onto.clone())
}

fn release(ui: &mut Ui) {
    let _ = ui.simulate([Event::Mouse(mouse::Event::ButtonReleased(
        mouse::Button::Left,
    ))]);
}

fn lifted(intents: &[rail::Intent]) -> Option<&str> {
    intents.iter().find_map(|intent| match intent {
        rail::Intent::Lift(id) => Some(id.as_str()),
        _ => None,
    })
}

#[test]
fn a_server_dragged_above_another_goes_there() {
    // Epsilon to the upper quarter of Server.
    let outcome = drag(395.0, &[Point::new(36.0, 126.0)], release);
    assert_eq!(lifted(&outcome.intents), Some("discord:g8"));
    assert!(
        outcome
            .intents
            .contains(&rail::Intent::Aim(Some(Place::Before("discord:g1".into()))))
    );
    assert_eq!(outcome.order.as_deref(), Some("8 1 f1[2 3] f2[4 5 6 7]"));
}

#[test]
fn a_server_held_over_the_middle_of_another_makes_a_folder() {
    let outcome = drag(395.0, &[Point::new(36.0, 146.0)], release);
    assert!(
        outcome
            .intents
            .contains(&rail::Intent::Aim(Some(Place::Onto("discord:g1".into()))))
    );
    assert_eq!(outcome.order.as_deref(), Some("+[1 8] f1[2 3] f2[4 5 6 7]"));
}

#[test]
fn a_server_held_over_a_closed_folder_goes_into_it() {
    let outcome = drag(140.0, &[Point::new(36.0, 350.0)], release);
    assert_eq!(lifted(&outcome.intents), Some("discord:g1"));
    assert_eq!(outcome.order.as_deref(), Some("f1[2 3] f2[4 5 6 7 1] 8"));
}

#[test]
fn a_server_dragged_out_of_a_folder_leaves_it_and_a_folder_of_one_dissolves() {
    // F2 to the lower quarter of Epsilon.
    let outcome = drag(240.0, &[Point::new(36.0, 422.0)], release);
    assert_eq!(lifted(&outcome.intents), Some("discord:g2"));
    assert!(
        outcome
            .intents
            .contains(&rail::Intent::Aim(Some(Place::After("discord:g8".into()))))
    );
    assert_eq!(outcome.order.as_deref(), Some("1 3 f2[4 5 6 7] 8 2"));
}

#[test]
fn escape_or_letting_go_beside_the_rail_puts_it_back() {
    let escape = drag(395.0, &[Point::new(36.0, 126.0)], |ui| {
        let _ = ui.tap_key(keyboard::key::Named::Escape);
    });
    assert_eq!(escape.intents.last(), Some(&rail::Intent::PutBack));
    assert_eq!(escape.order, None);
    let beside = drag(
        395.0,
        &[Point::new(36.0, 126.0), Point::new(300.0, 126.0)],
        release,
    );
    assert_eq!(beside.intents.last(), Some(&rail::Intent::PutBack));
    assert_eq!(beside.order, None);
}

/// Server taken under Epsilon, then held over Epsilon's middle until a
/// folder formed around it, the gap right under it as in the owner's
/// `ours-rail-drag-folder-forming.png`; then through `then`. The rows by
/// then: f1 from 122, F2 174, F3 222, f2 274, Epsilon 330, the gap 378 to
/// 426.
fn forming(then: &[Point]) -> Vec<Point> {
    let mut points = vec![Point::new(36.0, 420.0), Point::new(36.0, 354.0)];
    points.extend_from_slice(then);
    points
}

#[test]
fn a_folder_forming_goes_the_moment_the_server_leaves_it() {
    let epsilon = Some("discord:g8".to_owned());
    let leaves = [
        ("the gap under it", Point::new(36.0, 400.0)),
        ("the closed folder's lower quarter", Point::new(36.0, 322.0)),
        ("Epsilon's upper quarter", Point::new(36.0, 334.0)),
        ("under the last row", Point::new(36.0, 470.0)),
        ("beside the rail", Point::new(300.0, 354.0)),
        ("the unread DM", Point::new(36.0, 80.0)),
        ("the home tile", Point::new(36.0, 30.0)),
    ];
    for (name, point) in leaves {
        let outcome = drag(140.0, &forming(&[point]), |_| {});
        assert_eq!(outcome.onto[1], (None, epsilon.clone()), "it formed");
        assert_eq!(outcome.onto[2], (None, None), "gone at once: {name}");
        let left = outcome.left.expect("the drag goes on");
        assert_eq!(left.onto, None, "and stays gone: {name}");
    }
}

#[test]
fn let_go_in_the_gap_once_a_folder_formed_it_lands_in_the_gap() {
    let outcome = drag(140.0, &forming(&[Point::new(36.0, 400.0)]), release);
    assert_eq!(outcome.order.as_deref(), Some("f1[2 3] f2[4 5 6 7] 8 1"));
}

#[test]
fn from_a_forming_folder_into_a_closed_one_and_out_of_it_into_the_gap() {
    // f2's middle, 288 to 316: Epsilon lets go at once, f2 takes it after
    // the dwell, and lets go at once over the gap.
    let points = forming(&[Point::new(36.0, 302.0), Point::new(36.0, 400.0)]);
    let outcome = drag(140.0, &points, |_| {});
    assert_eq!(outcome.onto[2], (None, Some("f2".to_owned())));
    assert_eq!(outcome.onto[3], (None, None));
    let outcome = drag(140.0, &forming(&[Point::new(36.0, 302.0)]), release);
    assert_eq!(outcome.order.as_deref(), Some("f1[2 3] f2[4 5 6 7 1] 8"));
}

#[test]
fn escape_or_letting_go_beside_the_rail_leaves_no_folder_forming() {
    let escape = drag(140.0, &forming(&[]), |ui| {
        let _ = ui.tap_key(keyboard::key::Named::Escape);
    });
    assert_eq!(escape.onto[1].1.as_deref(), Some("discord:g8"));
    assert_eq!(escape.intents.last(), Some(&rail::Intent::PutBack));
    assert_eq!((escape.left, escape.order), (None, None));
    let beside = drag(140.0, &forming(&[]), |ui| {
        moved_to(ui, Point::new(300.0, 354.0));
        release(ui);
    });
    assert_eq!(beside.intents.last(), Some(&rail::Intent::PutBack));
    assert_eq!((beside.left, beside.order), (None, None));
}

#[test]
fn a_server_let_go_under_the_last_row_goes_last_out_of_an_open_folder() {
    // Server to the room under Epsilon: after it.
    let outcome = drag(140.0, &[Point::new(36.0, 520.0)], release);
    assert_eq!(outcome.order.as_deref(), Some("f1[2 3] f2[4 5 6 7] 8 1"));
    // Without f2 and Epsilon the open folder f1 ends the rail: F3 under it
    // leaves it, which leaves F2 alone.
    let mut vm = samples::folders_window();
    let gone =
        |server: &rail::Server| server.folder.as_deref() == Some("f2") || server.id == "discord:g8";
    vm.rail.servers.retain(|server| !gone(server));
    vm.rail.folders.retain(|folder| folder.id != "f2");
    let outcome = drag_in(vm, 288.0, &[Point::new(36.0, 400.0)], release);
    let past = rail::Intent::Aim(Some(Place::After("f1".into())));
    assert!(outcome.intents.contains(&past), "{:?}", outcome.intents);
    assert_eq!(outcome.order.as_deref(), Some("1 2 3"));
}
