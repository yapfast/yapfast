//! From `a_decorator_is_asked_about_the_rows_on_screen_and_answers_with_a_tree` onward: part 9 of 11.

use super::*;

#[test]
fn a_decorator_is_asked_about_the_rows_on_screen_and_answers_with_a_tree() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let plugin = installed(directory.path(), DECORATOR_MANIFEST, b"not a real module");
    let (wake, notices) = notice_channel();
    let shared = shared(directory.path(), wake);
    let (_session_directory, mut session, written, _inbound) = session_stub();

    let row = yapfast_plugin_api::message::DecorateRequest {
        room: "room-1".to_owned(),
        message: "message-1".to_owned(),
        author: "person-1".to_owned(),
        body: "hi".to_owned(),
        own: false,
    };
    runtime().block_on(session.on_command(
        &shared,
        Request::Decorate {
            plugin: plugin.clone(),
            request: row.clone(),
        },
    ));

    // The instance comes up first, then the decoration is asked for.
    let activate = next_frame(&written);
    assert!(matches!(
        message(&activate),
        ToHelper::Activate { plugin: started, .. } if started == plugin
    ));
    let ask = next_frame(&written);
    let ToHelper::Decorate {
        plugin: asked,
        request,
    } = message(&ask)
    else {
        panic!("the second frame is the decoration");
    };
    assert_eq!(asked, plugin);
    assert_eq!(request, row, "the row travels as the plugin reads it");

    let tree = serde_json::json!({ "rows": [
            { "cells": [{ "kind": "text", "text": "Decorated" }] }
        ] });
    assert!(
        session
            .on_frame(
                &shared,
                Frame::new(
                    FrameKind::Report,
                    ask.id,
                    serde_json::to_vec(&FromHelper::Decorated {
                        plugin: plugin.clone(),
                        message: "message-1".to_owned(),
                        tree: Some(tree.clone()),
                        hide: false,
                        hide_media: false,
                    })
                    .expect("it serializes"),
                ),
            )
            .is_none()
    );
    let notice = notices
        .recv_timeout(Duration::from_secs(2))
        .expect("a notice");
    let Notice::Slot(slots::Output::Decorate {
        plugin: drawn_by,
        message: drawn_for,
        tree: some,
        hide,
    }) = notice
    else {
        panic!("a decoration is a slot notice: {notice:?}");
    };
    assert_eq!(drawn_by, plugin.to_string());
    assert_eq!(drawn_for, "message-1");
    assert!(some.is_some(), "the tree arrives as the plugin drew it");
    assert!(hide.none(), "a plugin that hid nothing hid nothing");
}

#[test]
fn a_decoration_that_doesnt_check_out_draws_nothing() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let plugin = installed(directory.path(), DECORATOR_MANIFEST, b"not a real module");
    let (wake, notices) = notice_channel();
    let shared = shared(directory.path(), wake);
    let (_session_directory, mut session, written, _inbound) = session_stub();

    runtime().block_on(session.on_command(
        &shared,
        Request::Decorate {
            plugin: plugin.clone(),
            request: yapfast_plugin_api::message::DecorateRequest {
                room: "room-1".to_owned(),
                message: "message-1".to_owned(),
                author: "person-1".to_owned(),
                body: "hi".to_owned(),
                own: false,
            },
        },
    ));
    let _activate = next_frame(&written);
    let ask = next_frame(&written);

    // A button with no id is what the widget set refuses: it could not
    // report a click, and the row would look interactive and do nothing.
    let tree = serde_json::json!({ "rows": [
            { "cells": [{ "kind": "button", "text": "No id" }] }
        ] });
    assert!(
        session
            .on_frame(
                &shared,
                Frame::new(
                    FrameKind::Report,
                    ask.id,
                    serde_json::to_vec(&FromHelper::Decorated {
                        plugin: plugin.clone(),
                        message: "message-1".to_owned(),
                        tree: Some(tree),
                        hide: false,
                        hide_media: false,
                    })
                    .expect("it serializes"),
                ),
            )
            .is_none()
    );
    assert!(
        notices.recv_timeout(Duration::from_millis(100)).is_err(),
        "a tree Yapfast refuses draws nothing at all"
    );
}

#[test]
fn a_code_block_is_asked_about_by_its_hash_and_comes_back_coloured() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let plugin = installed(directory.path(), COLOURER_MANIFEST, b"not a real module");
    let (wake, notices) = notice_channel();
    let shared = shared(directory.path(), wake);
    let (_session_directory, mut session, written, _inbound) = session_stub();

    let text = "let x = 1;";
    let block = crate::plugins::code_block::hash(Some("rust"), text);
    let request = yapfast_plugin_api::code::CodeBlockRequest {
        block: block.clone(),
        language: Some("rust".to_owned()),
        text: text.to_owned(),
    };
    runtime().block_on(session.on_command(
        &shared,
        Request::CodeBlock {
            plugin: plugin.clone(),
            request: request.clone(),
        },
    ));

    let activate = next_frame(&written);
    assert!(matches!(
        message(&activate),
        ToHelper::Activate { plugin: started, .. } if started == plugin
    ));
    let ask = next_frame(&written);
    let ToHelper::CodeBlock {
        plugin: asked,
        request: sent,
    } = message(&ask)
    else {
        panic!("the second frame is the code block");
    };
    assert_eq!(asked, plugin);
    assert_eq!(
        sent, request,
        "the block travels with its hash and language"
    );

    let spans = vec![
        yapfast_plugin_api::code::Span::new("let", yapfast_plugin_api::code::Tone::Keyword),
        yapfast_plugin_api::code::Span::new(" x = 1;", yapfast_plugin_api::code::Tone::Plain),
    ];
    assert!(
        session
            .on_frame(
                &shared,
                Frame::new(
                    FrameKind::Report,
                    ask.id,
                    serde_json::to_vec(&FromHelper::CodeBlock {
                        plugin: plugin.clone(),
                        block: block.clone(),
                        spans: Some(spans.clone()),
                    })
                    .expect("it serializes"),
                ),
            )
            .is_none()
    );
    let notice = notices
        .recv_timeout(Duration::from_secs(2))
        .expect("a notice");
    let Notice::CodeBlock {
        plugin: coloured_by,
        block: coloured,
        rank,
        spans: answered,
    } = notice
    else {
        panic!("colours for a block are their own notice: {notice:?}");
    };
    assert_eq!(coloured_by, plugin);
    assert_eq!(coloured, block);
    assert_eq!(rank, 0);
    assert_eq!(answered, Some(spans));
}

#[test]
fn a_plugin_that_may_filter_hides_a_row_and_its_media() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let plugin = installed(directory.path(), FILTER_MANIFEST, b"not a real module");
    let (wake, notices) = notice_channel();
    let shared = shared(directory.path(), wake);
    let (_session_directory, mut session, written, _inbound) = session_stub();
    let ask = decorate_ask(&mut session, &shared, &written, &plugin);
    // The plugin answers a hide and nothing to draw.
    answer_decorated(&mut session, &shared, ask, &plugin, None, true, true);
    let notice = notices
        .recv_timeout(Duration::from_secs(2))
        .expect("a notice");
    let Notice::Slot(slots::Output::Decorate { hide, tree, .. }) = notice else {
        panic!("a hide is a decoration: {notice:?}");
    };
    assert!(hide.row && hide.media, "both the row and its media go");
    assert_eq!(tree, None, "a hide draws nothing under the row");
}

#[test]
fn a_plugin_without_messages_filter_hides_nothing() {
    // The same answer from a plugin that may only draw: the hide is
    // dropped, and with no tree there is nothing to announce at all.
    let directory = tempfile::tempdir().expect("a temporary directory");
    let plugin = installed(directory.path(), DECORATOR_MANIFEST, b"not a real module");
    let (wake, notices) = notice_channel();
    let shared = shared(directory.path(), wake);
    let (_session_directory, mut session, written, _inbound) = session_stub();
    let ask = decorate_ask(&mut session, &shared, &written, &plugin);
    answer_decorated(&mut session, &shared, ask, &plugin, None, true, true);
    assert!(
        notices.recv_timeout(Duration::from_millis(50)).is_err(),
        "a hide without `messages.filter` hides nothing"
    );
}
