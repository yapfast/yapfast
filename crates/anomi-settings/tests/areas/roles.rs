//! A server's role list: task X08's tests, driving
//! `anomi_settings::server::roles::list::view` over its fixtures in the
//! simulator (`super::support`).

use iced::{Event, Point, mouse};
use iced_test::Simulator;

use anomi_settings::server::roles::list::{menu, view};
use anomi_theme::{Renderer, Theme};
use anomi_vm::fixtures::settings::roles::{
    BOTS, CREW, EVERYONE, LEADS, MODS, STAFF, none, roles, searched,
};
use anomi_vm::settings::roles::{Intent, Roles};

use super::support::{SIZE, asked, front, settings};

type Ui<'a> = Simulator<'a, Intent, Theme, Renderer>;

/// Where a row's parts are across the simulator's window: the handle, and
/// Edit and More at the row's right end.
const HANDLE: f32 = 14.0;
const EDIT: f32 = 1334.0;
const MORE: f32 = 1376.0;

fn simulate(vm: &Roles) -> Ui<'_> {
    Simulator::with_size(settings(), SIZE, view(vm, front()))
}

fn click(vm: &Roles, text: &str) -> Vec<Intent> {
    let mut ui = simulate(vm);
    ui.click(text).unwrap_or_else(|_| panic!("{text}"));
    asked(ui)
}

fn has(vm: &Roles, text: &str) -> bool {
    simulate(vm).find(text).is_ok()
}

/// The middle of the row that names `name`, down the window.
fn row_y(ui: &mut Ui<'_>, name: &str) -> f32 {
    ui.find(name)
        .unwrap_or_else(|_| panic!("{name}"))
        .bounds()
        .center_y()
}

fn press() -> Event {
    Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
}

fn release() -> Event {
    Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
}

fn moved(position: Point) -> Event {
    Event::Mouse(mouse::Event::CursorMoved { position })
}

/// A click at `at`.
fn press_at(ui: &mut Ui<'_>, at: Point) {
    ui.point_at(at);
    let _ = ui.simulate([moved(at), press(), release()]);
}

/// Drags the role named `from` by its handle onto the row named `onto`.
fn drag(ui: &mut Ui<'_>, from: &str, onto: &str) {
    let start = Point::new(HANDLE, row_y(ui, from));
    let end = Point::new(HANDLE, row_y(ui, onto));
    ui.point_at(start);
    let _ = ui.simulate([moved(start), press()]);
    let nudge = Point::new(HANDLE, start.y + (end.y - start.y).signum() * 6.0);
    ui.point_at(nudge);
    let _ = ui.simulate([moved(nudge)]);
    ui.point_at(end);
    let _ = ui.simulate([moved(end), release()]);
}

/// What the drag asked, held rows passing over others left out.
fn outcome(ui: Ui<'_>) -> Vec<Intent> {
    let mut asked = asked(ui);
    asked.dedup();
    asked
}

#[test]
fn a_row_opens_its_role() {
    assert_eq!(click(&roles(), "Crew"), vec![Intent::Open(CREW.into())]);
    assert_eq!(click(&roles(), "Leads"), vec![Intent::Open(LEADS.into())]);
}

#[test]
fn default_permissions_opens_everyone() {
    assert_eq!(
        click(&roles(), "Default Permissions"),
        vec![Intent::Open(EVERYONE.into())]
    );
    assert!(has(&roles(), "@everyone • applies to all server members"));
}

#[test]
fn edit_and_view_open_the_role() {
    let vm = roles();
    let mut ui = simulate(&vm);
    let crew = row_y(&mut ui, "Crew");
    let staff = row_y(&mut ui, "Staff");
    press_at(&mut ui, Point::new(EDIT, crew));
    press_at(&mut ui, Point::new(EDIT, staff));
    assert_eq!(
        asked(ui),
        vec![Intent::Open(CREW.into()), Intent::Open(STAFF.into())],
        "Edit for Crew, View for Staff, which is locked"
    );
}

#[test]
fn more_opens_a_roles_menu_unless_it_cant_be_copied() {
    let vm = roles();
    let mut ui = simulate(&vm);
    let crew = row_y(&mut ui, "Crew");
    let leads = row_y(&mut ui, "Leads");
    press_at(&mut ui, Point::new(MORE, leads));
    press_at(&mut ui, Point::new(MORE, crew));
    let asked = asked(ui);
    // A still More lets the click through to its row, as Slint's did.
    assert!(
        matches!(asked.as_slice(), [Intent::Open(leads), Intent::More(id, _)] if leads == LEADS && id == CREW),
        "{asked:?}"
    );
}

#[test]
fn the_menu_moves_only_among_the_roles_that_may_move() {
    let vm = roles();
    let crew = menu::rows(&vm, CREW);
    let live: Vec<bool> = crew.iter().map(|row| !row.disabled).collect();
    assert_eq!(live, [false, true, true, true], "Crew is the first movable");
    assert_eq!(
        crew[1].intent,
        Some(
            Intent::Move {
                id: CREW.into(),
                to: 3
            }
            .into()
        )
    );
    assert!(crew[3].danger);
    let mods = menu::rows(&vm, MODS);
    assert!(!mods[0].disabled && mods[1].disabled, "Mods is the last");
    assert!(menu::rows(&vm, LEADS).iter().all(|row| row.disabled));
    assert!(menu::rows(&vm, "0").is_empty());
    let mut saving = roles();
    saving.saving = true;
    assert!(
        menu::rows(&saving, BOTS)[..2]
            .iter()
            .all(|row| row.disabled)
    );
}

#[test]
fn dragging_a_handle_moves_the_role() {
    let vm = roles();
    let mut ui = simulate(&vm);
    drag(&mut ui, "Mods", "Crew");
    assert_eq!(
        outcome(ui),
        vec![
            Intent::Dragging(Some(MODS.into())),
            Intent::Move {
                id: MODS.into(),
                to: 2
            },
        ]
    );
}

#[test]
fn nothing_moves_above_the_locked_roles() {
    let vm = roles();
    let mut ui = simulate(&vm);
    drag(&mut ui, "Crew", "Leads");
    assert_eq!(
        outcome(ui),
        vec![Intent::Dragging(Some(CREW.into())), Intent::Dragging(None)],
        "Leads' place refuses it: it goes back"
    );
    let mut ui = simulate(&vm);
    drag(&mut ui, "Staff", "Bots");
    let asked = asked(ui);
    assert!(
        !asked
            .iter()
            .any(|intent| matches!(intent, Intent::Move { .. } | Intent::Dragging(_))),
        "a locked role has no handle: {asked:?}"
    );
}

#[test]
fn a_search_or_a_save_stops_dragging() {
    let vm = searched();
    let mut ui = simulate(&vm);
    drag(&mut ui, "Mods", "Bots");
    assert!(asked(ui).is_empty());
    let mut saving = roles();
    saving.saving = true;
    let mut ui = simulate(&saving);
    drag(&mut ui, "Mods", "Crew");
    assert!(asked(ui).is_empty());
}

#[test]
fn typing_searches() {
    let vm = roles();
    let mut ui = simulate(&vm);
    ui.click("Search Roles").unwrap();
    ui.typewrite("mo");
    let asked = asked(ui);
    assert!(
        asked
            .iter()
            .all(|intent| matches!(intent, Intent::Search(_))),
        "{asked:?}"
    );
    assert_eq!(asked.last(), Some(&Intent::Search("mo".into())));
}

#[test]
fn create_role_waits_for_a_save() {
    assert_eq!(click(&roles(), "Create Role"), vec![Intent::Create]);
    let mut saving = roles();
    saving.saving = true;
    let mut ui = simulate(&saving);
    let _ = ui.click("Create Role");
    assert!(asked(ui).is_empty());
}

#[test]
fn the_heading_counts_the_roles_and_an_empty_search_says_so() {
    assert!(has(&roles(), "ROLES – 5"));
    assert!(has(&roles(), "MEMBERS"));
    assert!(!has(&roles(), "No roles"));
    let none = none();
    assert!(has(&none, "No roles"));
    assert!(has(&none, "ROLES – 0"));
}
