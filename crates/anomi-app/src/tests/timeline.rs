//! The timeline on the preview: its messages show, the rows in view reach
//! the core, the newest is read only while the window is in front and the
//! list follows it, and the pointer lights a message with its bar, which
//! goes as it leaves the timeline unless a menu of the message holds it.

use yapfast_core::application::Action;

use anomi_vm::chat::message::Intent as MessageIntent;
use anomi_vm::chat::timeline::{Intent, Shown, Span};
use anomi_vm::shell::{self, Layer};

use super::{shows, start};
use crate::app::App;
use crate::message::Message;

fn ask(intent: impl Into<anomi_vm::chat::Intent>) -> Message {
    Message::Intent(anomi_vm::Intent::from(intent.into()))
}

/// What the list tells with every row in view, following its end or not.
fn report(app: &App, following: bool) -> Message {
    let rows = &app.vm.chat.timeline.rows;
    ask(Intent::Shown(Shown {
        span: Span::new(rows, 0, rows.len() - 1),
        keys: rows.iter().map(|row| row.key).collect(),
        at_end: following,
        following,
    }))
}

#[test]
fn the_newest_is_read_in_front_while_followed() {
    let mut window = start("window-timeline", true);
    let app = &mut window.app;
    drop(app.dispatch(Action::Open("general".into())));
    assert!(shows(app, "Anyone around tonight?"));
    assert!(!app.core.state.reading, "nothing told yet");
    drop(app.update(report(app, true)));
    assert!(app.core.state.reading);
    assert!(app.vm.chat.timeline.shown.is_some());
    // Behind another window, nothing is read; in front again, it is.
    drop(app.update(Message::Focused(false)));
    assert!(!app.core.state.reading);
    drop(app.update(Message::Focused(true)));
    assert!(app.core.state.reading);
    // Scrolled up from the newest, it isn't.
    drop(app.update(report(app, false)));
    assert!(!app.core.state.reading);
}

#[test]
fn the_pointer_lights_a_message_with_its_bar() {
    let mut window = start("window-timeline-lit", true);
    let app = &mut window.app;
    drop(app.dispatch(Action::Open("general".into())));
    let key = app.vm.chat.timeline.rows[1].key;
    drop(app.update(ask(MessageIntent::Point(key))));
    let rows = &app.vm.chat.timeline.rows;
    assert!(rows[1].message.lit && !rows[0].message.lit);
    let bar = rows[1].message.bar.as_ref().expect("its bar");
    assert!(bar.may.react && bar.may.forward);
    let other = rows[0].key;
    drop(app.update(ask(MessageIntent::Point(other))));
    let rows = &app.vm.chat.timeline.rows;
    assert!(rows[0].message.lit && rows[0].message.bar.is_some());
    assert!(!rows[1].message.lit && rows[1].message.bar.is_none());
    // A new message keeps the light where it is.
    drop(app.dispatch(Action::SetDraft("hello".into())));
    drop(app.dispatch(Action::Send));
    let rows = &app.vm.chat.timeline.rows;
    assert!(
        rows.last()
            .is_some_and(|row| row.message.body.read() == "hello")
    );
    assert!(rows[0].message.lit);
}

#[test]
fn the_bar_goes_as_the_pointer_leaves_unless_a_menu_holds_it() {
    let mut window = start("window-timeline-left", true);
    let app = &mut window.app;
    drop(app.dispatch(Action::Open("general".into())));
    let key = app.vm.chat.timeline.rows[1].key;
    let lit = |app: &App| app.vm.chat.timeline.rows[1].message.lit;
    drop(app.update(ask(MessageIntent::Point(key))));
    drop(app.update(ask(Intent::Left)));
    assert!(!lit(app), "off the timeline");
    drop(app.update(ask(MessageIntent::Point(key))));
    app.vm.shell.open(Layer::MessageMenu);
    drop(app.update(ask(Intent::Left)));
    assert!(lit(app), "held by its menu");
    app.vm.shell.close(Layer::MessageMenu);
    let gone = anomi_vm::Intent::from(shell::Intent::Gone(Layer::MessageMenu));
    drop(app.update(Message::Intent(gone)));
    assert!(!lit(app), "gone with the menu");
}

#[test]
fn the_lit_bar_follows_shift() {
    let mut window = start("window-timeline-shift", true);
    let app = &mut window.app;
    drop(app.dispatch(Action::Open("general".into())));
    let key = app.vm.chat.timeline.rows[1].key;
    drop(app.update(ask(MessageIntent::Point(key))));
    let shifted = |app: &App| {
        let bar = app.vm.chat.timeline.rows[1].message.bar.as_ref();
        bar.expect("its bar").shift
    };
    assert!(!shifted(app));
    drop(app.update(Message::Shift(true)));
    assert!(shifted(app), "Shift's actions");
    drop(app.update(Message::Shift(false)));
    assert!(!shifted(app));
}
