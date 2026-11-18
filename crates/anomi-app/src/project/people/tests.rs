//! The member list keeps the rows of those who didn't change, a hidden list
//! keeps nothing, Friends follow their tab and search, the requests sort as
//! Discord's screen does, and a dialog about a person says what it asks.

use std::sync::Arc;

use yapfast_core::application::{Action, DmScreen, Navigation, PersonAsk, PersonDialog};
use yapfast_core::chat::{ChatMember, Friend, FriendRequest, Presence, Role};

use anomi_vm::kind::Presence as Shown;
use anomi_vm::people::members::Row;
use anomi_vm::people::requests::Friend as Asked;
use anomi_vm::shell::Layer;

use crate::app::App;

fn app(name: &str) -> (App, std::path::PathBuf) {
    let folder = std::env::temp_dir().join(format!("anomi-people-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    let app = App::for_tests(&folder);
    (app, folder)
}

fn done(app: App, folder: std::path::PathBuf) {
    drop(app);
    let _ = std::fs::remove_dir_all(folder);
}

pub(crate) fn member(id: &str, name: &str, presence: Presence, group: &str) -> ChatMember {
    ChatMember {
        id: id.into(),
        bot: false,
        name: name.into(),
        username: None,
        role: Role::Member,
        avatar: None,
        decoration: None,
        nameplate: None,
        style: None,
        group: Some(group.into()),
        group_icon: None,
        role_icon: None,
        presence: Some(presence),
        color: None,
        activity: None,
        activities: Default::default(),
        roles: Arc::from([]),
        timed_out_until: None,
        tag: None,
        boosting_since: None,
    }
}

fn show_members(app: &mut App, wanted: bool) {
    super::members::show(
        &app.core,
        &app.layout,
        wanted,
        &mut app.vm.people.members,
        &mut app.shown.people.members,
        &mut app.pictures,
    );
}

fn member_row(app: &App, id: &str) -> Arc<anomi_vm::people::members::Member> {
    app.vm
        .people
        .members
        .rows
        .iter()
        .find_map(|row| match row {
            Row::Member(member) if member.id == id => Some(member.clone()),
            _ => None,
        })
        .expect("a row of theirs")
}

#[test]
fn a_presence_change_makes_one_row_and_moves_it() {
    let (mut app, folder) = app("members");
    let online = |id, name| member(id, name, Presence::Online, "Online — 2");
    let list = vec![
        online("1", "Ann"),
        online("2", "Bob"),
        member("3", "Cat", Presence::Offline, "Offline — 1"),
    ];
    app.core.state.members = Some(Ok(Arc::new(list)));
    show_members(&mut app, true);
    let rows = &app.vm.people.members.rows;
    assert_eq!(rows.len(), 5, "two headings and three people");
    let (ann, cat) = (member_row(&app, "1"), member_row(&app, "3"));
    // Bob goes offline: the list comes again, whole.
    let list = vec![
        member("1", "Ann", Presence::Online, "Online — 1"),
        member("2", "Bob", Presence::Offline, "Offline — 2"),
        member("3", "Cat", Presence::Offline, "Offline — 2"),
    ];
    app.core.state.members = Some(Ok(Arc::new(list)));
    show_members(&mut app, true);
    let headings: Vec<&str> = app
        .vm
        .people
        .members
        .rows
        .iter()
        .filter_map(|row| match row {
            Row::Heading(heading) => Some(heading.text.as_str()),
            Row::Member(_) => None,
        })
        .collect();
    assert_eq!(headings, ["Online — 1", "Offline — 2"]);
    // Ann and Cat keep the rows made before; Bob's is made again.
    assert!(Arc::ptr_eq(&ann, &member_row(&app, "1")));
    assert!(Arc::ptr_eq(&cat, &member_row(&app, "3")));
    assert_eq!(member_row(&app, "2").presence, Shown::Offline);
    // Hidden, it keeps nothing.
    show_members(&mut app, false);
    assert!(app.vm.people.members.rows.is_empty());
    done(app, folder);
}

#[test]
fn friends_follow_their_tab_and_search() {
    let (mut app, folder) = app("friends");
    let friend = |id: &str, name: &str, presence| Friend {
        id: id.into(),
        name: name.into(),
        nickname: None,
        avatar: None,
        presence: Some(presence),
        activity: Some("Out for lunch".into()),
        activities: Default::default(),
        since: None,
    };
    let state = &mut app.core.state;
    state.friends = vec![
        friend("1", "Ann", Presence::Online),
        friend("2", "Bob", Presence::Offline),
    ];
    state.navigation = Navigation::DirectMessages;
    state.dm_screen = Some(DmScreen::Friends);
    state.people_changed();
    app.refresh();
    let friends = &app.vm.people.friends;
    assert_eq!((friends.rows.len(), friends.total), (1, 2), "those around");
    assert_eq!(friends.rows[0].status.text, "Out for lunch");
    app.vm.people.friends.all = true;
    app.refresh();
    assert_eq!(app.vm.people.friends.rows.len(), 2);
    app.vm.people.friends.filter = "bo".into();
    app.refresh();
    assert_eq!(app.vm.people.friends.rows[0].name, "Bob");
    // Another screen keeps no rows.
    app.core.state.dm_screen = None;
    app.refresh();
    assert!(app.vm.people.friends.rows.is_empty());
    done(app, folder);
}

#[test]
fn requests_sort_into_waiting_sent_and_spam() {
    let (mut app, folder) = app("requests");
    let request = |id: &str, incoming, spam| FriendRequest {
        id: id.into(),
        name: format!("Person {id}"),
        avatar: None,
        incoming,
        spam,
    };
    app.core.state.friend_requests = vec![
        request("1", true, false),
        request("2", false, false),
        request("3", true, true),
    ];
    let rows = super::friends::rows_of(&app.core.state, false);
    assert_eq!(rows.incoming.len(), 1);
    assert_eq!(rows.incoming[0].friend, Asked::Asked);
    assert_eq!(rows.sent[0].friend, Asked::Sent);
    assert_eq!(rows.spam[0].id, "3");
    done(app, folder);
}

#[test]
fn a_dialog_about_a_person_says_what_it_asks() {
    let (mut app, folder) = app("person");
    app.core.state.person_dialog = Some(PersonDialog {
        user: "1".into(),
        name: "Ann".into(),
        ask: PersonAsk::Timeout,
        location: "ContextMenu",
        text: Some(String::new()),
        saving: false,
        error: None,
        server: Some("discord:server:1".into()),
        choice: 1,
    });
    app.refresh();
    assert!(app.vm.shell.is_open(Layer::Person));
    let person = &app.vm.people.person;
    assert_eq!(person.title, "Time out Ann");
    assert_eq!(person.options.len(), 6);
    assert!(person.danger && person.has_field() && person.may_answer());
    app.core.state.person_dialog = None;
    drop(app.dispatch(Action::ClosePersonDialog));
    assert!(!app.vm.shell.is_open(Layer::Person));
    done(app, folder);
}

#[test]
fn the_pointer_coming_and_going_makes_that_row_alone_again() {
    let (mut app, folder) = app("pointed");
    let online = |id, name| member(id, name, Presence::Online, "Online — 2");
    let list = vec![online("1", "Ann"), online("2", "Bob")];
    app.core.state.members = Some(Ok(Arc::new(list)));
    show_members(&mut app, true);
    let (ann, bob) = (member_row(&app, "1"), member_row(&app, "2"));
    super::members_pointed(&mut app.shown.people, Some("1".into()));
    show_members(&mut app, true);
    assert!(
        !Arc::ptr_eq(&ann, &member_row(&app, "1")),
        "Ann's collectibles may play"
    );
    assert!(Arc::ptr_eq(&bob, &member_row(&app, "2")));
    let pointed = member_row(&app, "1");
    show_members(&mut app, true);
    assert!(
        Arc::ptr_eq(&pointed, &member_row(&app, "1")),
        "once is enough"
    );
    super::members_pointed(&mut app.shown.people, None);
    show_members(&mut app, true);
    assert!(
        !Arc::ptr_eq(&pointed, &member_row(&app, "1")),
        "still again"
    );
    assert!(Arc::ptr_eq(&bob, &member_row(&app, "2")));
    done(app, folder);
}
