//! The rail's drag on the real core: kept in the rail's view-model while
//! it goes on, through the projection building the rail again, and gone
//! once it's dropped or put back.

use anomi_vm::rail::{Intent, Place};

use super::*;

fn rail(app: &mut App, intent: Intent) {
    drop(app.update(Message::Intent(anomi_vm::Intent::Rail(intent))));
}

#[test]
fn a_drag_is_kept_while_it_goes_on_and_ends_dropped_or_put_back() {
    let mut window = start("rail-drag", true);
    let app = &mut window.app;
    let first = app.vm.rail.servers[0].id.clone();
    rail(app, Intent::Lift(first.clone()));
    rail(app, Intent::Aim(Some(Place::After("elsewhere".into()))));
    rail(app, Intent::Aim(Some(Place::Onto("other".into()))));
    app.refresh();
    let drag = app.vm.rail.drag.clone().expect("kept through a rebuild");
    assert_eq!(drag.id, first);
    assert_eq!(drag.gap, Some(Place::After("elsewhere".into())));
    assert_eq!(drag.onto.as_deref(), Some("other"));
    rail(app, Intent::PutBack);
    assert_eq!(app.vm.rail.drag, None);
    rail(app, Intent::Lift(first));
    rail(app, Intent::Drop);
    assert_eq!(app.vm.rail.drag, None);
}
