//! Messages' rows: what a message going out, a notice, an app's answer,
//! pictures, stickers, a card of components and reactions show, and the
//! bar to the "New" divider. Grouping itself is `anomi_vm`'s
//! (`chat::timeline::group`'s tests).

use std::sync::Arc;

use yapfast_core::chat::{
    ButtonStyle, Card, CardButton, CardRow, ChatMessage, Delivery, Notice, NoticeKind, Picture,
    Pixels, Placement, UsedCommand,
};
use yapfast_lang::tr;

use anomi_vm::chat::components::Row as CardLine;
use anomi_vm::chat::message::{Header, Notice as Icon};
use anomi_vm::kind::Expected;
use anomi_vm::text::Level;

use super::{attachment, row};
use crate::project::chat::reactions;
use crate::project::chat::timeline::frame::unread_label;

#[test]
fn a_message_still_sending_is_only_faint() {
    let sending = row(&ChatMessage {
        item: "d:1".into(),
        body: "Hi".into(),
        delivery: Delivery::Sending,
        ..Default::default()
    });
    assert!(sending.delivery.as_ref().is_some_and(|line| line.quiet));
    // Files uploading keep the line and its Cancel.
    let uploading = row(&ChatMessage {
        item: "n:2".into(),
        delivery: Delivery::Sending,
        attachments: vec![yapfast_core::chat::Attachment {
            size: Some(40_000_000),
            video: true,
            ..attachment("clip.mp4", Placement::Message)
        }],
        ..Default::default()
    });
    let line = uploading.delivery.expect("its line");
    assert!(!line.quiet && line.cancel);
    assert_eq!(line.text, tr!("Sending..."));
    let waiting = row(&ChatMessage {
        item: "d:2".into(),
        body: "Hi".into(),
        delivery: Delivery::Waiting,
        ..Default::default()
    });
    let line = waiting.delivery.expect("its line");
    assert!(!line.quiet && !line.text.is_empty());
}

/// Its group is `anomi_vm`'s `a_notice_a_reply_and_an_apps_answer_start_groups`.
#[test]
fn a_notice_is_a_group_of_its_own_with_an_icon() {
    let said = ChatMessage {
        item: "d:4".into(),
        author: "Alice".into(),
        sender: "discord:1".into(),
        body: "Look".into(),
        timestamp: 100,
        ..Default::default()
    };
    let pinned = ChatMessage {
        item: "d:5".into(),
        body: "Alice pinned a message.".into(),
        timestamp: 160,
        notice: Some(Notice {
            kind: NoticeKind::Pinned(Some("d:4".into())),
            text: "pinned a message.".into(),
        }),
        ..said.clone()
    };
    assert_eq!(row(&pinned).notice, Some(Icon::Pin));
    assert_eq!(row(&said).notice, None);
}

#[test]
fn an_apps_answer_starts_a_group_under_who_used_which_command() {
    let thought = ChatMessage {
        item: "d:4".into(),
        author: "Status Bot".into(),
        sender: "discord:9".into(),
        body: "Pong!".into(),
        timestamp: 100,
        ..Default::default()
    };
    let answer = ChatMessage {
        item: "d:5".into(),
        timestamp: 110,
        used: Some(UsedCommand {
            user: "Alice".into(),
            sender: "discord:1".into(),
            avatar: None,
            color: Some(0x3498db),
            name: "/echo".into(),
            slash: Some("echo".into()),
            target: None,
        }),
        ..thought.clone()
    };
    let Header::Command(command) = row(&answer).header else {
        panic!("who used which command shows over it");
    };
    assert_eq!(
        (command.user.as_str(), command.name.as_str()),
        ("Alice", "/echo")
    );
    assert!(command.color.is_some());
    assert_eq!(row(&thought).header, Header::None);
}

#[test]
fn pictures_on_their_way_hold_their_place_as_skeletons() {
    let picture = |name: &str, preview_failed| yapfast_core::chat::Attachment {
        size: Some(1000),
        image: true,
        dimensions: Some((1600, 900)),
        preview_failed,
        ..attachment(name, Placement::Message)
    };
    let wide = row(&ChatMessage {
        attachments: vec![picture("wide.png", false)],
        ..Default::default()
    });
    let gallery = &wide.attachments.gallery;
    assert_eq!(gallery.cells[0].picture, Expected::Waiting);
    // As the 550 by 400 preview will be: 1600 by 900 scaled to fit.
    assert_eq!((gallery.width, gallery.height), (550, 309));
    assert!(
        wide.attachments.files.is_empty(),
        "not a file card meanwhile"
    );
    // One whose preview failed is a file card instead.
    let broken = row(&ChatMessage {
        attachments: vec![picture("broken.png", true)],
        ..Default::default()
    });
    assert!(broken.attachments.gallery.cells.is_empty());
    assert_eq!(broken.attachments.files.len(), 1);
}

#[test]
fn a_messages_sticker_is_160px_square() {
    // A sticker's picture may come smaller than Discord's 160px, an old kept
    // one or a Lottie drawn small, but its cell is 160 square.
    crate::pictures::install();
    let picture = Arc::new(Picture {
        width: 96,
        height: 96,
        rgba: Pixels::new(96, 96, vec![0; 96 * 96 * 4]),
        ..Default::default()
    });
    let message = row(&ChatMessage {
        attachments: vec![yapfast_core::chat::Attachment {
            image: true,
            dimensions: Some((160, 160)),
            preview: Some(picture),
            ..attachment("Wave", Placement::Sticker)
        }],
        ..Default::default()
    });
    let sticker = &message.attachments.stickers[0];
    assert_eq!((sticker.width, sticker.height), (160, 160));
    assert!(sticker.picture.ready().is_some());
}

#[test]
fn rows_mark_mentions_and_the_new_bar_counts_what_others_sent() {
    let now = yapfast_core::clock::now_ms() / 1000;
    let message = |own: bool, mentions_me: bool| ChatMessage {
        own,
        mentions_me,
        time: "14:05".into(),
        // Today, so the bar leaves the date out.
        timestamp: now,
        ..Default::default()
    };
    assert!(message(false, true).highlighted(true));
    assert!(!message(false, false).highlighted(true));
    let unread = [
        message(false, true),
        message(true, false),
        message(false, false),
    ]
    .map(Arc::new);
    assert_eq!(
        unread_label(&unread, false, now),
        "2 new messages since 14:05",
        "Alice's own reply isn't new to her"
    );
    assert_eq!(
        unread_label(&unread[..1], false, now),
        "1 new message since 14:05"
    );
    assert_eq!(
        unread_label(&unread[..1], true, now),
        "1+ new messages since 14:05",
        "more came after a window of older messages"
    );
    let older = ChatMessage {
        timestamp: 1_791_329_400,
        ..message(false, false)
    };
    assert!(
        unread_label(&[Arc::new(older)], false, now).starts_with("1 new message since 14:05 on "),
        "with the date when it wasn't today"
    );
}

#[test]
fn a_reactions_tooltip_names_up_to_three_as_discords_client_does() {
    let tip = |list: &[&str], count| {
        let names: Vec<String> = list.iter().map(|&name| name.to_owned()).collect();
        reactions::who(":tada:", &names, count)
    };
    assert_eq!(tip(&["Ann"], 1), ":tada: reacted by Ann");
    assert_eq!(tip(&["Ann"], 2), ":tada: reacted by Ann and 1 other");
    assert_eq!(tip(&["Ann", "Bo"], 2), ":tada: reacted by Ann and Bo");
    assert_eq!(
        tip(&["Ann", "Bo"], 5),
        ":tada: reacted by Ann, Bo, and 3 others"
    );
    assert_eq!(
        tip(&["Ann", "Bo", "Cy"], 3),
        ":tada: reacted by Ann, Bo, and Cy"
    );
    assert_eq!(
        tip(&["Ann", "Bo", "Cy"], 4),
        ":tada: reacted by Ann, Bo, Cy, and 1 other"
    );
    // Discord's names for emoji: a word, not `:+1:`, and a custom one's own.
    assert_eq!(yapfast_core::emoji::shortcode("👍"), ":thumbsup:");
    assert_eq!(yapfast_core::emoji::shortcode("🎉"), ":tada:");
    assert_eq!(yapfast_core::emoji::shortcode(":talk:"), ":talk:");
}

#[test]
fn a_card_of_components_shows_headings_files_and_only_link_buttons_working() {
    let button = |style, url: Option<&str>| CardButton {
        label: "Go".into(),
        style,
        url: url.map(str::to_owned),
        custom_id: None,
        sku_id: None,
        disabled: false,
    };
    let message = row(&ChatMessage {
        item: "d:1".into(),
        attachments: vec![yapfast_core::chat::Attachment {
            size: Some(10),
            ..attachment("notes.txt", Placement::Component(0))
        }],
        cards: vec![Card {
            framed: true,
            color: Some(0x5865f2),
            rows: vec![
                CardRow::Text {
                    text: "# Hello\nThere\n-# small print".into(),
                    beside: None,
                },
                CardRow::Media(0),
                CardRow::Buttons(vec![
                    button(ButtonStyle::Link, Some("https://example.com")),
                    button(ButtonStyle::Primary, None),
                ]),
            ],
        }],
        ..Default::default()
    });
    // The file shows in its card, not under the message.
    assert!(message.attachments.files.is_empty());
    let card = &message.components.cards[0];
    assert!(card.framed && card.color.is_some());
    let CardLine::Text { text, .. } = &card.rows[0] else {
        panic!("the text first");
    };
    let levels: Vec<Level> = text.blocks().map(|(block, _)| block.level).collect();
    assert_eq!(levels, [Level::Heading(1), Level::Text, Level::Subtext]);
    assert_eq!(text.read(), "Hello\nThere\nsmall print");
    let CardLine::Media(media) = &card.rows[1] else {
        panic!("then the file");
    };
    assert_eq!(media.files[0].name, "notes.txt");
    let CardLine::Buttons(buttons) = &card.rows[2] else {
        panic!("then the buttons");
    };
    let enabled: Vec<bool> = buttons.iter().map(|button| button.enabled).collect();
    assert_eq!(enabled, [true, false], "only a link's button works");
}
