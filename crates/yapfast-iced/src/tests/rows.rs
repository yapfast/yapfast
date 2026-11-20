//! What a click on a row sends names what it was on, so a batch handled
//! first that moves every row (the oldest messages let go, history
//! prepended, the member list churning) doesn't turn it onto another.

use std::sync::Arc;

use iced::{Point, Settings, mouse};
use iced_test::{Simulator, selector::Candidate};

use super::{change, long_history, numbered, preview};
use crate::app::{App, Message};

/// What `act` on the view as it is now sends, not yet handled.
fn sent(app: &App, act: impl FnOnce(&mut Simulator<'_, Message>)) -> Vec<Message> {
    let mut ui = Simulator::with_size(Settings::default(), app.shell.frame, app.view());
    act(&mut ui);
    ui.into_messages().collect()
}

/// The oldest `count` messages go and as many new ones come, from
/// someone else, as the core's let-go does under a crowd.
fn shift(app: &mut App, count: usize) {
    let len = app.client.state().messages.len();
    let new: Vec<_> = numbered(app, len, len + count)
        .into_iter()
        .map(|message| {
            Arc::new(yapfast_core::chat::ChatMessage {
                sender: "someone-else".into(),
                author: "Someone else".into(),
                ..(*message).clone()
            })
        })
        .collect();
    change(app, |state| {
        let mut messages = state.messages[count..].to_vec();
        messages.extend(new);
        state.messages = Arc::new(messages);
    });
}

#[test]
fn a_right_click_opens_the_menu_of_its_message_after_the_rows_moved() {
    let (mut app, _folder) = preview();
    long_history(&mut app, 80);
    let clicked = sent(&app, |ui| super::right_click(ui, "Message number 78"));
    assert!(!clicked.is_empty(), "the right click sent something");
    shift(&mut app, 5);
    for message in clicked {
        let _ = app.update(message);
    }
    let menu = app.win.menu.as_ref().expect("a menu");
    assert_eq!(
        menu.item.as_deref(),
        Some("long-78"),
        "the clicked message's"
    );
}

#[test]
fn a_click_on_an_author_opens_that_messages_author_after_the_rows_moved() {
    let (mut app, _folder) = preview();
    long_history(&mut app, 80);
    let sender = app.timeline.messages[78].sender.clone();
    let clicked = sent(&app, |ui| {
        // The author's name over the message clicked.
        let body = ui.find("Message number 78").expect("the message");
        let at = body.visible_bounds().expect("in view").position();
        ui.point_at(Point::new(at.x + 4.0, at.y - 12.0));
        ui.simulate(iced_test::simulator::click());
    });
    shift(&mut app, 5);
    for message in clicked {
        let _ = app.update(message);
    }
    let card = app.profiles.card.as_ref().expect("a card");
    assert_eq!(card.user, sender, "the clicked message's author's");
}

#[test]
fn a_click_on_a_member_opens_their_card_after_the_list_moved() {
    let (mut app, _folder) = preview();
    app.show_roster(false);
    let riley = app
        .client
        .state()
        .members
        .iter()
        .flatten()
        .flat_map(|members| members.iter())
        .find(|member| member.name == "Riley")
        .map(|member| member.id.clone())
        .expect("Riley in the preview");
    let right = app.window.width - 300.0;
    let clicked = sent(&app, |ui| {
        let name = ui
            .find(|candidate: Candidate<'_>| match candidate {
                Candidate::Text {
                    content: "Riley",
                    visible_bounds: Some(bounds),
                    ..
                } if bounds.x > right => Some(bounds),
                _ => None,
            })
            .expect("Riley in the member list");
        ui.point_at(name.center());
        ui.simulate([
            iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        ]);
    });
    // Someone comes online above Riley, and every line below moves.
    change(&mut app, |state| {
        for members in state.members.iter_mut().flatten() {
            let members = Arc::make_mut(members);
            let mut first = members[0].clone();
            first.id = "newcomer".into();
            first.name = "Aaron".into();
            members.insert(0, first);
        }
    });
    app.show_roster(false);
    for message in clicked {
        let _ = app.update(message);
    }
    let card = app.profiles.card.as_ref().expect("a card");
    assert_eq!(card.user, riley, "Riley's");
}

#[test]
fn a_selection_starts_on_its_message_after_the_rows_moved() {
    let (mut app, _folder) = preview();
    long_history(&mut app, 80);
    let pressed = sent(&app, |ui| {
        let body = ui.find("Message number 78").expect("the message");
        ui.point_at(body.visible_bounds().expect("in view").center());
        ui.simulate([iced::Event::Mouse(mouse::Event::ButtonPressed(
            mouse::Button::Left,
        ))]);
    });
    shift(&mut app, 5);
    for message in pressed {
        let _ = app.update(message);
    }
    let (from, _) = crate::timeline::lanes(&app).marks().expect("a selection");
    assert_eq!(
        from.row,
        crate::timeline::row_of(&app, "long-78").expect("still there")
    );
}

#[test]
fn a_right_click_on_an_activity_card_opens_its_menu_at_the_pointer() {
    let (mut app, _folder) = preview();
    app.show_roster(false);
    app.roster.activity_cards = vec![crate::members::ActivityCard {
        id: "riley".into(),
        name: "Riley".into(),
        avatar: None,
        color: None,
        game: "Stardew Valley".into(),
        since: "for 5m".into(),
        thumb: None,
        icon: 0,
        live: true,
    }];
    let mut pointer = Point::ORIGIN;
    let pressed = sent(&app, |ui| {
        let game = ui.find("Stardew Valley").expect("the card");
        pointer = game.visible_bounds().expect("in view").center();
        ui.point_at(pointer);
        ui.simulate([
            iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)),
            iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Right)),
        ]);
    });
    let at = pressed
        .iter()
        .find_map(|message| match message {
            Message::PersonMenu(id, at) if id == "riley" => Some(*at),
            _ => None,
        })
        .expect("Riley's menu asked for");
    assert!(
        at.distance(pointer) < 2.0,
        "at the pointer {pointer:?}, not {at:?}"
    );
}

#[test]
fn the_hover_bar_of_a_grouped_message_keeps_it_where_it_reaches_over_the_row_above() {
    let (mut app, _folder) = preview();
    change(&mut app, |state| {
        let template = (*state.messages[0]).clone();
        let said = |item: &str, body: &str, timestamp| {
            Arc::new(yapfast_core::chat::ChatMessage {
                id: item.into(),
                item: item.into(),
                author: template.author.clone(),
                sender: template.sender.clone(),
                body: body.into(),
                timestamp,
                can_react: true,
                can_reply: true,
                ..Default::default()
            })
        };
        state.messages = Arc::new(vec![
            said("g0", "First of the group", 1_700_000_000),
            said("g1", "Second of the group", 1_700_000_060),
        ]);
    });
    assert!(
        !app.timeline.looks[1].first,
        "the second continues the group"
    );
    let text = {
        let mut ui = Simulator::with_size(Settings::default(), app.shell.frame, app.view());
        ui.find("Second of the group")
            .expect("the second")
            .visible_bounds()
            .expect("in view")
    };
    let moved = |app: &mut App, at: Point| {
        let messages = sent(app, |ui| {
            ui.point_at(at);
            ui.simulate([iced::Event::Mouse(mouse::Event::CursorMoved {
                position: at,
            })]);
        });
        let entered: Vec<Arc<str>> = messages
            .iter()
            .filter_map(|message| match message {
                Message::Timeline(crate::timeline::Event::PointRow(item, true)) => {
                    Some(item.clone())
                }
                _ => None,
            })
            .collect();
        for message in messages {
            let _ = app.update(message);
        }
        entered
    };
    let _ = moved(&mut app, text.center());
    assert_eq!(app.pointed.message.as_deref(), Some("g1"), "on the row");
    // Its bar shows now. Over More, at the bar's right end 16 px in from
    // the timeline's (the member list's left), 6 px above the row's top (its text less the 1 px
    // over a grouped message and its content's 2 px): over the row above.
    let row_top = text.y - 3.0;
    let x = crate::conversation::chat_right(&app) - 16.0 - 3.0 - 16.0;
    let entered = moved(&mut app, Point::new(x, row_top - 6.0));
    assert!(
        !entered.iter().any(|item| &**item == "g0"),
        "the row above, under the bar, isn't pointed"
    );
    assert_eq!(
        app.pointed.message.as_deref(),
        Some("g1"),
        "still the second's"
    );
}

#[test]
fn a_rows_menu_and_its_authors_card_open_by_its_message_under_any_head() {
    let (mut app, _folder) = preview();
    change(&mut app, |state| {
        let said = |item: &str, author: &str, timestamp| {
            Arc::new(yapfast_core::chat::ChatMessage {
                id: item.into(),
                item: item.into(),
                author: author.into(),
                sender: format!("{author}-id"),
                body: format!("Said by {author}"),
                timestamp,
                can_react: true,
                can_reply: true,
                ..Default::default()
            })
        };
        state.messages = Arc::new(vec![
            // Under the history's edge, then a plain first of a group, then
            // one under a new day's divider.
            said("a", "Ann", 1_700_000_000),
            said("b", "Ben", 1_700_000_060),
            said("c", "Cat", 1_700_200_000),
        ]);
    });
    assert!(
        app.timeline.looks[2].day.is_some(),
        "the third under a divider"
    );
    let names: Vec<f32> = {
        let mut ui = Simulator::with_size(Settings::default(), app.shell.frame, app.view());
        ["Ann", "Ben", "Cat"]
            .into_iter()
            .map(|name| {
                ui.find(name)
                    .expect("the author")
                    .visible_bounds()
                    .expect("in view")
                    .y
            })
            .collect()
    };
    let mut cards = Vec::new();
    for (row, name) in names.iter().enumerate() {
        let _ = crate::message::open_author(&mut app, row);
        cards.push(app.profiles.card.take().expect("a card").at.y - name);
    }
    assert!(
        cards.iter().all(|card| (card - cards[1]).abs() < 0.5),
        "each card as far from its author's name: {cards:?}"
    );
    let menus: Vec<f32> = (0..3)
        .map(|row| crate::message::hover_bar::bar_menu_at(app.ctx(), row).y - names[row])
        .collect();
    assert!(
        menus.iter().all(|menu| (menu - menus[1]).abs() < 0.5),
        "each menu as far from its author's name: {menus:?}"
    );
}
