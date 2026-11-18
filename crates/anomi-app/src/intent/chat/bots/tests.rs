//! Bots' controls, forms and the cards on a Discord channel of the core's:
//! a press or a choice shows at once on its message's row alone, which no
//! second press can send again; a form opens over the window and goes with
//! Escape through the core; an emoji's or a sticker's card opens under it
//! and goes with its message.

use std::sync::Arc;

use iced::Rectangle;
use serde_json::json;
use yapfast_core::chat::{
    Attachment, ButtonStyle, Card, CardButton, CardOption, CardRow, ChatMessage, Event, Placement,
};

use anomi_vm::chat::Intent;
use anomi_vm::chat::bot_forms::{self, Kind};
use anomi_vm::chat::components::{self, Row};
use anomi_vm::chat::message::Intent as MessageIntent;
use anomi_vm::chat::poll;
use anomi_vm::chat::timeline::group;
use anomi_vm::menu::Spot;
use anomi_vm::shell::{self, Layer};
use anomi_vm::text::Link;

use crate::app::App;
use crate::project::chat::poll::tests::{core_with, poll_message};
use crate::tests::Folder;

/// The app on a Discord channel with Alice's "hi" and Status Bot's deploy
/// message: Approve, and a menu of two environments.
fn app(name: &str, more: Vec<Arc<ChatMessage>>) -> (Folder, App) {
    let folder = Folder::new(name);
    let mut app = App::for_tests(folder.path());
    let mut messages = vec![said("d:4", "hi"), deployed()];
    messages.extend(more);
    app.core = core_with(messages);
    app.motion.front = true;
    app.refresh();
    (folder, app)
}

fn said(item: &str, body: &str) -> Arc<ChatMessage> {
    Arc::new(ChatMessage {
        id: item.into(),
        item: item.into(),
        author: "Alice".into(),
        sender: "discord:1".into(),
        body: body.into(),
        ..ChatMessage::default()
    })
}

fn deployed() -> Arc<ChatMessage> {
    let option = |label: &str| CardOption {
        label: label.into(),
        value: label.to_lowercase(),
        description: None,
        default: false,
        emoji: None,
    };
    Arc::new(ChatMessage {
        id: "d:5".into(),
        item: "d:5".into(),
        author: "Status Bot".into(),
        sender: "discord:3".into(),
        bot: true,
        application_id: Some("3".into()),
        body: "Deploy finished.".into(),
        cards: vec![Card {
            rows: vec![
                CardRow::Buttons(vec![CardButton {
                    label: "Approve".into(),
                    style: ButtonStyle::Success,
                    url: None,
                    custom_id: Some("ok".into()),
                    sku_id: None,
                    disabled: false,
                }]),
                CardRow::Select {
                    text: "Pick an environment".into(),
                    disabled: false,
                    kind: 3,
                    custom_id: "environment".into(),
                    options: vec![option("Staging"), option("Production")],
                    defaults: Vec::new(),
                    channel_types: Vec::new(),
                    min_values: 1,
                    max_values: 1,
                },
            ],
            ..Card::default()
        }],
        ..ChatMessage::default()
    })
}

fn ask(app: &mut App, intent: impl Into<Intent>) {
    drop(super::super::handle(app, intent.into()));
}

fn row<'a>(app: &'a App, item: &str) -> &'a anomi_vm::chat::message::Message {
    let key = group::key(item);
    let rows = &app.vm.chat.timeline.rows;
    &rows.iter().find(|row| row.key == key).unwrap().message
}

fn close(app: &mut App, layer: Layer) {
    drop(crate::intent::handle(
        app,
        shell::Intent::Close(layer).into(),
    ));
}

#[test]
fn a_pressed_button_holds_its_messages_controls_back_on_its_row_alone() {
    let (_folder, mut app) = app("bots-button", Vec::new());
    let key = group::key("d:5");
    let components = &row(&app, "d:5").components;
    assert_eq!(components.key, key);
    assert!(!components.busy);
    assert!(matches!(&components.cards[0].rows[0], Row::Buttons(buttons) if buttons[0].enabled));
    let hi = row(&app, "d:4").body.clone();
    ask(&mut app, components::Intent::Use(key, "ok".into()));
    assert!(row(&app, "d:5").components.busy, "it shows at once");
    assert!(
        row(&app, "d:4").body.same(&hi),
        "the other row is as it was"
    );
    // A second press sends nothing more.
    let pending = app.core.state.interactions.newest_busy().map(str::to_owned);
    ask(&mut app, components::Intent::Use(key, "ok".into()));
    assert_eq!(
        app.core.state.interactions.newest_busy().map(str::to_owned),
        pending
    );
}

#[test]
fn a_chosen_option_shows_in_its_box_until_its_bot_answers() {
    let (_folder, mut app) = app("bots-select", Vec::new());
    let key = group::key("d:5");
    let spot = Spot::below(Rectangle::new([100.0, 200.0].into(), [300.0, 40.0].into()));
    ask(
        &mut app,
        components::Intent::OpenSelect(key, "environment".into(), spot),
    );
    assert!(app.vm.shell.is_open(Layer::BotSelect));
    let select = &app.vm.chat.bot_forms.select;
    assert_eq!(select.anchor, 300.0);
    let labels: Vec<&str> = select
        .choices
        .iter()
        .map(|choice| choice.label.as_str())
        .collect();
    assert_eq!(labels, ["Staging", "Production"]);
    ask(&mut app, bot_forms::Intent::Toggle(1));
    assert!(
        !app.vm.shell.is_open(Layer::BotSelect),
        "one option sends at once"
    );
    let components = &row(&app, "d:5").components;
    assert!(components.busy);
    assert!(matches!(
        &components.cards[0].rows[1],
        Row::Select(select) if select.text == "Production" && select.picked
    ));
    // The bot answers with its message as it now is: the box shows that.
    let answered = Arc::new((*deployed()).clone());
    let messages = vec![said("d:4", "hi"), answered];
    app.core.state.messages = Arc::new(messages);
    app.refresh();
    assert!(matches!(
        &row(&app, "d:5").components.cards[0].rows[1],
        Row::Select(select) if select.text == "Pick an environment"
    ));
}

#[test]
fn a_form_opens_on_its_first_field_and_goes_with_escape_through_the_core() {
    let (_folder, mut app) = app("bots-form", Vec::new());
    drop(app.core.apply(Event::InteractionModal(json!({
        "id": "42", "channel_id": "7", "custom_id": "notes",
        "application": {"id": "3"}, "title": "Write notes",
        "components": [
            {"type": 10, "content": "Notes go to **the team**."},
            {"type": 18, "label": "Notes", "component": {"type": 4, "custom_id": "notes", "style": 2}}
        ]
    }))));
    app.refresh();
    assert!(app.vm.shell.is_open(Layer::BotModal));
    assert_eq!(app.shown.focus, Some(anomi_chat::bot_forms::FIRST));
    let form = &app.vm.chat.bot_forms.form;
    assert_eq!(form.heading, "Write notes");
    let kinds: Vec<Kind> = form.fields.iter().map(|field| field.kind).collect();
    assert_eq!(kinds, [Kind::Text, Kind::Input]);
    ask(&mut app, bot_forms::Intent::Edit(1, "Ship it".into()));
    assert_eq!(app.vm.chat.bot_forms.form.fields[1].value, "Ship it");
    close(&mut app, Layer::BotModal);
    assert!(app.core.state.interaction_modal.is_none());
    assert!(!app.vm.shell.is_open(Layer::BotModal));
}

#[test]
fn an_emojis_card_opens_and_a_stickers_goes_with_its_message() {
    let sticker = Arc::new(ChatMessage {
        attachments: vec![Attachment {
            name: "wave".into(),
            size: None,
            image: true,
            dimensions: Some((160, 160)),
            preview: None,
            preview_failed: false,
            preview_percent: 100,
            placement: Placement::Sticker,
            spoiler: false,
            video: false,
            gif: None,
            url: Some("https://media.discordapp.net/stickers/9.png?size=160".into()),
            description: None,
        }],
        ..(*said("d:6", "")).clone()
    });
    let (_folder, mut app) = app("bots-cards", vec![sticker]);
    let key = group::key("d:4");
    ask(&mut app, MessageIntent::Link(key, Link::Emoji("😭".into())));
    assert!(app.vm.shell.is_open(Layer::MessageCard));
    let emoji = app.vm.chat.cards.emoji.as_ref().expect("its card");
    assert_eq!((emoji.name.as_str(), emoji.glyph.as_str()), (":sob:", "😭"));
    close(&mut app, Layer::MessageCard);
    assert!(!app.vm.shell.is_open(Layer::MessageCard));
    // One in the topic in full opens the same card.
    let topic = anomi_vm::header::Intent::Link(Link::Emoji("👍".into()));
    drop(crate::intent::handle(
        &mut app,
        anomi_vm::Intent::Header(topic),
    ));
    assert!(app.vm.shell.is_open(Layer::MessageCard));
    assert_eq!(
        app.vm
            .chat
            .cards
            .emoji
            .as_ref()
            .map(|card| card.glyph.as_str()),
        Some("👍")
    );
    close(&mut app, Layer::MessageCard);
    let spot = Spot::below(Rectangle::new([10.0, 10.0].into(), [160.0, 160.0].into()));
    let sticker = anomi_vm::chat::attachments::Intent::Sticker {
        item: group::key("d:6"),
        index: 0,
        spot,
    };
    ask(&mut app, Intent::Attachments(sticker));
    assert!(app.vm.shell.is_open(Layer::MessageCard));
    assert_eq!(
        app.vm
            .chat
            .cards
            .sticker
            .as_ref()
            .map(|card| card.name.as_str()),
        Some("wave")
    );
    // Its message deleted: the card goes rather than hold the keys unseen.
    app.core.state.messages = Arc::new(vec![said("d:4", "hi"), deployed()]);
    app.refresh();
    assert!(!app.vm.shell.is_open(Layer::MessageCard));
}

#[test]
fn a_vote_the_backend_cant_send_leaves_the_poll_as_it_was() {
    let answer = |id: u64, text: &str| yapfast_core::chat::PollAnswer {
        id,
        text: text.into(),
        ..Default::default()
    };
    let poll = Arc::new(ChatMessage {
        id: "d:7".into(),
        item: "d:7".into(),
        ..(*poll_message(vec![answer(1, "Yes"), answer(2, "No")])).clone()
    });
    let (_folder, mut app) = app("bots-poll", vec![poll]);
    let key = group::key("d:7");
    ask(&mut app, poll::Intent::Pick(key, 2));
    let card = &row(&app, "d:7").poll;
    assert!(card.answers[1].picked && card.action_enabled);
    ask(&mut app, poll::Intent::Act(key, poll::Action::Vote));
    // No backend runs here: the vote can't go, and says so; the poll shows
    // as it was, its pick kept to vote again.
    assert!(app.core.state.notice.is_some());
    let card = &row(&app, "d:7").poll;
    assert!(!card.results);
    assert!(card.answers[1].picked && card.action_enabled);
    assert!(app.shown.timeline.kept.polls["d:7"].voting.is_none());
}
