//! The dialogs' clicks and keys on the real core: the switcher searched and
//! left by Enter in the window, the invite's choices, a new DM started once,
//! leaving a group, and the masked link's question holding Ctrl+K.

use iced::keyboard::{Key, key::Named};
use yapfast_core::application::{Action, InviteDialog, LeaveDialog, NewDm as Dialog};
use yapfast_core::chat::Contact;

use anomi_vm::Intent;
use anomi_vm::places::{invite, join, leave, leaving_link};
use anomi_vm::shell::{self, Layer};

use crate::tests::{id, interact, shows, start};

fn handle(app: &mut crate::app::App, intent: impl Into<Intent>) {
    drop(crate::intent::handle(app, intent.into()));
}

fn place(intent: impl Into<anomi_vm::places::Intent>) -> Intent {
    intent.into().into()
}

#[test]
fn the_switcher_finds_a_place_and_enter_goes_there() {
    let mut window = start("switcher-enter", true);
    let app = &mut window.app;
    drop(crate::keys::switcher(app));
    assert!(shows(app, "Where would you like to go?"));
    let room = (app.core.state.rooms.iter())
        .find(|room| {
            !room.direct && !room.locked && Some(&room.id) != app.core.state.selected.as_ref()
        })
        .expect("a channel not open");
    let (name, wanted) = (room.name.clone(), room.id.clone());
    interact(app, |ui| {
        ui.click(id(anomi_screens::switcher::FIELD))
            .expect("the field");
        let _ = ui.typewrite(&name);
    });
    assert_eq!(app.vm.switcher.query, name);
    assert_eq!(app.core.state.switcher.as_deref(), Some(name.as_str()));
    let at = app.vm.switcher.find(&wanted).expect("found");
    handle(app, anomi_vm::switcher::Intent::Choose(at));
    assert_eq!(app.vm.switcher.chosen, at);
    interact(app, |ui| {
        ui.click(id(anomi_screens::switcher::FIELD))
            .expect("the field");
        let _ = ui.tap_key(Key::Named(Named::Enter));
    });
    assert_eq!(app.core.state.selected.as_deref(), Some(wanted.as_str()));
    assert!(app.core.state.switcher.is_none());
    assert!(!app.vm.shell.is_open(Layer::Switcher));
}

#[test]
fn escape_closes_the_switcher_through_the_core() {
    let mut window = start("switcher-escape", true);
    let app = &mut window.app;
    drop(crate::keys::switcher(app));
    handle(app, shell::Intent::Close(Layer::Switcher));
    assert!(app.core.state.switcher.is_none());
    assert!(!app.vm.shell.is_open(Layer::Switcher));
}

#[test]
fn the_invites_choices_are_the_windows_until_a_link_is_asked_for() {
    let mut window = start("invite-choices", true);
    let app = &mut window.app;
    app.core.state.invite = Some(InviteDialog {
        server: "Yapfast".into(),
        room: "discord:1:2".into(),
        channel: "general".into(),
        voice: false,
        max_age: 604_800,
        max_uses: 0,
        link: Some("https://discord.gg/abc".into()),
        making: false,
        error: None,
    });
    app.refresh();
    handle(
        app,
        place(invite::Intent::Copy("https://discord.gg/abc".into())),
    );
    assert!(app.vm.places.invite.copied);
    handle(app, place(invite::Intent::Age(1)));
    handle(app, place(invite::Intent::Uses(2)));
    let invite = &app.vm.places.invite;
    assert_eq!((invite.age, invite.uses), (1, 2));
    let remake = invite.remake();
    handle(app, place(remake));
    let asked = app.core.state.invite.as_ref().expect("still open");
    assert_eq!((asked.max_age, asked.max_uses), (3600, 5));
    app.core.state.invite.as_mut().expect("open").link = Some("https://discord.gg/new".into());
    app.refresh();
    assert!(!app.vm.places.invite.copied, "a new link isn't copied yet");
    handle(app, place(invite::Intent::Close));
    assert!(!app.vm.shell.is_open(Layer::Invite));
}

#[test]
fn join_keeps_its_field_and_says_what_to_paste() {
    let mut window = start("join-field", true);
    let app = &mut window.app;
    drop(app.dispatch(Action::OpenJoinServer));
    handle(app, place(join::Intent::Link("hello there".into())));
    assert_eq!(app.vm.places.join.link, "hello there");
    app.refresh();
    assert_eq!(
        app.vm.places.join.link, "hello there",
        "the window's text stays"
    );
    handle(app, place(join::Intent::Find(String::new())));
    assert_eq!(
        app.vm.places.join.error,
        "Paste an invite link, such as https://discord.gg/yapfast."
    );
    handle(app, shell::Intent::Close(Layer::Join));
    assert!(app.core.state.join.is_none());
}

#[test]
fn a_new_dm_starts_the_person_clicked_once() {
    let mut window = start("new-dm-start", true);
    let app = &mut window.app;
    let bob = Contact {
        id: "bob".into(),
        name: "Bob".into(),
        avatar: None,
        detail: "Discord friend".into(),
        discord: true,
    };
    app.core.state.new_dm = Some(Dialog {
        discord: vec![bob],
        ..Dialog::default()
    });
    app.refresh();
    handle(app, anomi_vm::new_dm::Intent::Query("bo".into()));
    assert_eq!(app.vm.new_dm.query, "bo");
    handle(app, anomi_vm::new_dm::Intent::Start("nobody".into()));
    assert!(!app.vm.new_dm.starting, "no one by that id");
    app.core.state.new_dm.as_mut().expect("open").starting = true;
    app.refresh();
    assert_eq!(app.vm.new_dm.note, "Starting the conversation...");
    // Sent again, it would reach no backend here, which ends the start.
    handle(app, anomi_vm::new_dm::Intent::Start("bob".into()));
    assert!(app.vm.new_dm.starting, "one at a time");
    handle(app, anomi_vm::new_dm::Intent::Close);
    assert!(app.core.state.new_dm.is_none());
}

#[test]
fn leaving_a_group_ticks_and_cancels() {
    let mut window = start("leave-group", true);
    let app = &mut window.app;
    app.core.state.leave = Some(LeaveDialog {
        place: "discord:7".into(),
        name: "Weekend plans".into(),
        group: true,
        silent: false,
        leaving: false,
        error: None,
    });
    app.refresh();
    handle(app, place(leave::Intent::Tick));
    assert!(app.vm.places.leave.ticked);
    handle(app, place(leave::Intent::Alternative));
    assert!(
        app.core.state.leave.is_some(),
        "leaving has no other answer"
    );
    handle(app, place(leave::Intent::Close));
    assert!(app.core.state.leave.is_none());
    assert!(!app.vm.shell.is_open(Layer::Leave));
}

#[test]
fn the_masked_links_question_holds_ctrl_k_until_it_closes() {
    let mut window = start("leaving-link", true);
    let app = &mut window.app;
    app.vm.places.leaving_link.link = "https://example.org/x".into();
    app.vm.shell.open(Layer::LeavingLink);
    app.refresh();
    assert!(!app.keys.switcher);
    handle(app, place(leaving_link::Intent::Close));
    app.refresh();
    assert!(!app.vm.shell.is_open(Layer::LeavingLink));
    assert!(app.keys.switcher);
    handle(app, shell::Intent::Gone(Layer::LeavingLink));
    assert!(app.vm.places.leaving_link.link.is_empty());
}
