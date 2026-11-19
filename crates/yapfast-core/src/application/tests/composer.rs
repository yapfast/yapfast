//! The composer: drafts, replies, edits and files.

use super::*;

#[test]
fn failed_sends_keep_the_draft_and_newer_drafts_survive_confirmation() {
    let mut app = connected();
    assert!(matches!(
        app.dispatch(Action::SetDraft("First".into())).as_slice(),
        [
            Effect::Backend(Command::Typing { room: typing, typing: true }),
            Effect::Backend(Command::SaveDraft { room, body }),
        ] if typing == "dm" && room == "dm" && body == "First"
    ));
    assert!(app.dispatch(Action::SetDraft("First".into())).is_empty());
    assert!(matches!(
        send_command(app.dispatch(Action::Send)),
        Command::Send { reply: None, .. }
    ));
    assert!(app.dispatch(Action::Send).is_empty(), "one send at a time");
    app.apply(Event::SendResult {
        room: "dm".into(),
        body: "First".into(),
        result: Err("offline"),
    });
    assert_eq!(app.state.draft(), "First");
    assert_eq!(app.state.notice.as_deref(), Some("offline"));
    send_command(app.dispatch(Action::Send));
    app.dispatch(Action::SetDraft("Next message".into()));
    app.apply(Event::SendResult {
        room: "dm".into(),
        body: "First".into(),
        result: Ok(()),
    });
    assert_eq!(app.state.draft(), "Next message");
    assert!(app.state.sending.is_none());
    app.dispatch(Action::Navigate(Navigation::Server("friends".into())));
    app.dispatch(Action::SetDraft("Other room draft".into()));
    app.apply(Event::SendResult {
        room: "dm".into(),
        body: "Next message".into(),
        result: Ok(()),
    });
    assert_eq!(app.state.draft(), "Other room draft");
}

#[test]
fn up_in_an_empty_composer_edits_the_newest_own_message() {
    let mut app = connected();
    app.apply(Event::Timeline {
        room: "dm".into(),
        messages: timeline(vec![
            message("e:$old", true),
            message("e:$mine", true),
            message("e:$bob", false),
        ]),
    });
    app.dispatch(Action::EditLast);
    let editing = app.state.edit().expect("editing");
    assert_eq!(editing.item, "e:$mine");
    assert_eq!(editing.text, "Body of e:$mine");
    assert_eq!(app.state.draft(), "", "the composer is left alone");
}

#[test]
fn a_reply_mentions_its_author_unless_switched_off() {
    let mut app = connected();
    app.apply(Event::Timeline {
        room: "dm".into(),
        messages: timeline(vec![message("e:$bob", false)]),
    });
    app.dispatch(Action::ReplyPing(true));
    app.dispatch(Action::Reply("e:$bob".into()));
    assert!(matches!(
        app.state.compose(),
        Some(Compose::Reply { ping: true, .. })
    ));
    app.dispatch(Action::ToggleReplyPing { aside: false });
    app.dispatch(Action::SetDraft("@silent hi".into()));
    let Command::Send { body, options, .. } = send_command(app.dispatch(Action::Send)) else {
        unreachable!()
    };
    assert_eq!(body, "@silent hi", "Matrix has no silent messages");
    assert_eq!(
        options,
        crate::chat::SendOptions {
            quiet_reply: true,
            silent: false
        }
    );
}

#[test]
fn replies_and_saved_drafts_share_one_composer() {
    let mut app = connected();
    app.apply(Event::Timeline {
        room: "dm".into(),
        messages: timeline(vec![message("e:$bob", false), message("e:$mine", true)]),
    });
    app.apply(Event::Draft {
        room: "dm".into(),
        body: "Saved earlier".into(),
    });
    assert_eq!(app.state.draft(), "Saved earlier");
    app.dispatch(Action::SetDraft("Typed now".into()));
    app.apply(Event::Draft {
        room: "dm".into(),
        body: "Stale".into(),
    });
    assert_eq!(app.state.draft(), "Typed now");

    assert!(app.dispatch(Action::Edit("e:$bob".into())).is_empty());
    assert!(app.state.edit().is_none(), "only own messages are editable");
    app.dispatch(Action::Reply("e:$bob".into()));
    assert!(matches!(app.state.compose(), Some(Compose::Reply { author, .. }) if author == "Bob"));
    assert!(matches!(
        send_command(app.dispatch(Action::Send)),
        Command::Send { reply: Some(item), body, .. } if item == "e:$bob" && body == "Typed now"
    ));
    app.apply(Event::SendResult {
        room: "dm".into(),
        body: "Typed now".into(),
        result: Ok(()),
    });
    assert!(app.state.compose().is_none() && app.state.draft().is_empty());
}

/// A room with a message of Bob's and one of this account's. An open edit is
/// its own field, as in Discord's inline editor, so the composer keeps its
/// draft and its reply.
fn editing_app() -> Application {
    let mut app = connected();
    app.apply(Event::Timeline {
        room: "dm".into(),
        messages: timeline(vec![message("e:$bob", false), message("e:$mine", true)]),
    });
    app
}

fn edit_command(effects: Vec<Effect>) -> (String, String) {
    match effects.as_slice() {
        [Effect::Backend(Command::Edit { item, body, .. })] => (item.clone(), body.clone()),
        other => panic!("expected one edit command, got {} effects", other.len()),
    }
}

#[test]
fn an_edit_is_its_own_field_and_leaves_the_composer_alone() {
    let mut app = editing_app();
    app.dispatch(Action::SetDraft("Unsent".into()));
    app.dispatch(Action::Edit("e:$mine".into()));
    let editing = app.state.edit().expect("editing");
    assert_eq!(
        (editing.item.as_str(), editing.text.as_str()),
        ("e:$mine", "Body of e:$mine")
    );
    assert_eq!(app.state.draft(), "Unsent", "the composer keeps its draft");
    // Typing in either field goes to that field alone.
    assert!(
        app.dispatch(Action::SetEditText("Corrected".into()))
            .is_empty(),
        "an edit's text is not saved as a draft"
    );
    assert!(
        !app.dispatch(Action::SetDraft("Unsent too".into()))
            .is_empty()
    );
    assert_eq!(app.state.edit().unwrap().text, "Corrected");
    assert_eq!(app.state.draft(), "Unsent too");
    // Saving sends the edit's text and keeps it open until Discord has it.
    assert_eq!(
        edit_command(app.dispatch(Action::SaveEdit)),
        ("e:$mine".into(), "Corrected".into())
    );
    app.apply(Event::EditResult {
        room: "dm".into(),
        item: "e:$mine".into(),
        result: Err("refused".into()),
    });
    assert_eq!(app.state.edit().expect("still open").text, "Corrected");
    assert_eq!(app.state.draft(), "Unsent too");
    app.dispatch(Action::SaveEdit);
    app.apply(Event::EditResult {
        room: "dm".into(),
        item: "e:$mine".into(),
        result: Ok(()),
    });
    assert!(app.state.edit().is_none());
    assert_eq!(app.state.draft(), "Unsent too", "the draft is still there");
}

#[test]
fn cancelling_an_edit_drops_its_text_and_keeps_the_draft() {
    let mut app = editing_app();
    app.dispatch(Action::SetDraft("Unsent".into()));
    app.dispatch(Action::Edit("e:$mine".into()));
    app.dispatch(Action::SetEditText("Never saved".into()));
    assert!(app.dispatch(Action::CancelEdit).is_empty());
    assert!(app.state.edit().is_none());
    assert_eq!(app.state.draft(), "Unsent");
    // Editing again starts from the message, not from what was dropped.
    app.dispatch(Action::Edit("e:$mine".into()));
    assert_eq!(app.state.edit().unwrap().text, "Body of e:$mine");
}

#[test]
fn a_reply_and_an_edit_are_open_together() {
    let mut app = editing_app();
    app.dispatch(Action::Reply("e:$bob".into()));
    app.dispatch(Action::Edit("e:$mine".into()));
    assert!(matches!(app.state.compose(), Some(Compose::Reply { .. })));
    assert!(app.state.edit().is_some());
    app.dispatch(Action::CancelCompose);
    assert!(app.state.compose().is_none(), "cancelling the reply");
    assert!(app.state.edit().is_some(), "leaves the edit open");
    app.dispatch(Action::Reply("e:$bob".into()));
    app.dispatch(Action::SetDraft("Hi".into()));
    assert!(matches!(
        send_command(app.dispatch(Action::Send)),
        Command::Send { reply: Some(item), body, .. } if item == "e:$bob" && body == "Hi"
    ));
    assert!(app.state.edit().is_some(), "sending doesn't touch the edit");
}

#[test]
fn only_one_message_is_edited_at_a_time_in_a_room() {
    let mut app = editing_app();
    app.apply(Event::Timeline {
        room: "dm".into(),
        messages: timeline(vec![message("e:$one", true), message("e:$two", true)]),
    });
    app.dispatch(Action::Edit("e:$one".into()));
    app.dispatch(Action::SetEditText("half".into()));
    app.dispatch(Action::Edit("e:$two".into()));
    let editing = app.state.edit().unwrap();
    assert_eq!(
        (editing.item.as_str(), editing.text.as_str()),
        ("e:$two", "Body of e:$two")
    );
}

#[test]
fn saving_an_edit_that_changed_nothing_just_closes_it() {
    let mut app = editing_app();
    app.dispatch(Action::Edit("e:$mine".into()));
    assert!(app.dispatch(Action::SaveEdit).is_empty(), "nothing is sent");
    assert!(app.state.edit().is_none());
}

#[test]
fn saving_an_empty_edit_asks_whether_to_delete_the_message() {
    let mut app = editing_app();
    app.dispatch(Action::Edit("e:$mine".into()));
    app.dispatch(Action::SetEditText("  \n".into()));
    assert!(app.dispatch(Action::SaveEdit).is_empty(), "nothing is sent");
    assert_eq!(app.state.confirm_delete.as_deref(), Some("e:$mine"));
    assert!(
        app.state.edit().is_some(),
        "the edit stays open behind the question"
    );
    // Keeping the message goes back to editing it.
    app.dispatch(Action::CancelDelete);
    assert!(app.state.confirm_delete.is_none());
    assert_eq!(app.state.edit().unwrap().item, "e:$mine");
    // Deleting it ends the edit with the message.
    app.dispatch(Action::SaveEdit);
    let effects = app.dispatch(Action::ConfirmDelete);
    assert!(matches!(
        effects.as_slice(),
        [Effect::Backend(Command::Delete { item, .. })] if item == "e:$mine"
    ));
    assert!(app.state.edit().is_none());
}

#[test]
fn an_empty_edit_of_a_message_with_files_is_saved_not_deleted() {
    let mut app = editing_app();
    let mut with_file = message("e:$file", true);
    with_file.attachments.push(chat::Attachment {
        name: "a.png".into(),
        description: None,
        size: None,
        image: true,
        dimensions: None,
        preview: None,
        preview_failed: false,
        placement: chat::Placement::Message,
        gif: None,
        spoiler: false,
        video: false,
        url: None,
    });
    app.apply(Event::Timeline {
        room: "dm".into(),
        messages: timeline(vec![with_file]),
    });
    app.dispatch(Action::Edit("e:$file".into()));
    app.dispatch(Action::SetEditText(String::new()));
    assert_eq!(
        edit_command(app.dispatch(Action::SaveEdit)),
        ("e:$file".into(), String::new())
    );
    assert!(app.state.confirm_delete.is_none());
}

#[test]
fn an_edit_ends_with_its_message_or_its_room() {
    let mut app = editing_app();
    app.dispatch(Action::Edit("e:$mine".into()));
    app.dispatch(Action::DeleteNow("e:$mine".into()));
    assert!(
        app.state.edit().is_none(),
        "a deleted message can't be edited"
    );
}

#[test]
fn files_wait_in_the_composer_until_sent() {
    let folder = tempfile::tempdir().unwrap();
    let file = |name: &str, size: u64| {
        let path = folder.path().join(name);
        std::fs::File::create(&path).unwrap().set_len(size).unwrap();
        path
    };
    let (picture, notes) = (file("cat.png", 3), file("notes.txt", 2048));
    let big = file("big.mov", 51 * 1024 * 1024);
    let mut app = connected();
    assert!(!app.state.can_send(), "nothing to send yet");
    app.dispatch(Action::Stage(vec![picture.clone(), big, notes.clone()]));
    let staged: Vec<_> = app
        .state
        .staged()
        .iter()
        .map(|file| (file.name.as_str(), file.image))
        .collect();
    assert_eq!(staged, [("cat.png", true), ("notes.txt", false)]);
    assert_eq!(
        app.state.notice.as_deref(),
        Some("big.mov is 51 MB; files here can be up to 50 MB.")
    );
    app.dispatch(Action::Unstage(1));
    app.dispatch(Action::Stage(vec![notes.clone()]));
    assert!(app.state.can_send(), "files alone make a message");
    let Command::Send { body, files, .. } = send_command(app.dispatch(Action::Send)) else {
        unreachable!()
    };
    assert_eq!(
        (body.as_str(), files),
        ("", vec![picture.clone().into(), notes.clone().into()])
    );
    app.apply(Event::SendResult {
        room: "dm".into(),
        body,
        result: Ok(()),
    });
    assert!(
        app.state.staged().is_empty(),
        "sent files leave the composer"
    );
    // Only a network with spoilers marks files as them.
    app.dispatch(Action::Stage(vec![picture.clone(), notes]));
    app.dispatch(Action::ToggleSpoiler(0));
    assert!(
        !app.state.staged()[0].spoiler,
        "Matrix has no spoiler files"
    );
    let room = app
        .state
        .rooms
        .iter_mut()
        .map(Arc::make_mut)
        .find(|room| room.id == "dm")
        .unwrap();
    room.capabilities = Network::Discord.capabilities();
    app.dispatch(Action::ToggleSpoiler(1));
    app.dispatch(Action::ToggleSpoiler(1));
    app.dispatch(Action::ToggleSpoiler(0));
    let Command::Send { files, .. } = send_command(app.dispatch(Action::Send)) else {
        unreachable!()
    };
    let spoilers: Vec<_> = files.iter().map(|file| file.spoiler).collect();
    assert_eq!(spoilers, [true, false]);
    app.apply(Event::SendResult {
        room: "dm".into(),
        body: String::new(),
        result: Ok(()),
    });
    app.dispatch(Action::Stage(vec![picture.clone(); chat::MAX_FILES + 1]));
    assert_eq!(app.state.staged().len(), chat::MAX_FILES);
    assert_eq!(
        app.state.notice.as_deref(),
        Some("A message carries up to 10 files.")
    );
    assert!(!app.state.can_attach());
    app.state.notice = None;
    app.dispatch(Action::Stage(vec![picture]));
    assert_eq!(
        app.state.notice.as_deref(),
        Some("A message carries up to 10 files."),
        "a paste into a full message says why it's refused"
    );
}

#[test]
fn a_drop_is_refused_whole_as_discords_client_refuses_it() {
    let folder = tempfile::tempdir().unwrap();
    let file = |name: &str, size: u64| {
        let path = folder.path().join(name);
        std::fs::File::create(&path).unwrap().set_len(size).unwrap();
        path
    };
    let (picture, big) = (file("cat.png", 3), file("big.mov", 51 * 1024 * 1024));
    let mut app = connected();
    assert_eq!(app.state.drop_refusal(std::slice::from_ref(&picture)), None);
    let refusal = app.state.drop_refusal(&[picture.clone(), big]).unwrap();
    assert_eq!(
        (refusal.title(), refusal.help()),
        (
            "Your files are too powerful",
            "Max file size is 50.00 MB please.".into()
        )
    );
    let refusal = app
        .state
        .drop_refusal(&[picture.clone(), folder.path().into()])
        .unwrap();
    assert_eq!(refusal, chat::Refusal::Invalid, "a folder isn't a file");
    assert_eq!(app.state.drop_refusal(&[]), Some(chat::Refusal::Invalid));
    // The files already waiting count.
    app.dispatch(Action::Stage(vec![picture.clone(); 4]));
    assert_eq!(app.state.drop_refusal(&vec![picture.clone(); 6]), None);
    let refusal = app.state.drop_refusal(&vec![picture; 7]).unwrap();
    assert_eq!(
        (refusal.title(), refusal.help()),
        (
            "Too many uploads!",
            "You can only upload 10 files at a time!".into()
        )
    );
}

#[test]
fn uploads_name_their_destination_as_discord_does() {
    let mut app = connected();
    fn room(app: &mut Application) -> &mut ChatRoom {
        let id = app.state.selected.clone().unwrap();
        app.state
            .rooms
            .iter_mut()
            .map(Arc::make_mut)
            .find(|room| room.id == id)
            .unwrap()
    }
    room(&mut app).name = "Sam".into();
    room(&mut app).direct = true;
    room(&mut app).members = 2;
    room(&mut app).person = Some("sam".into());
    assert_eq!(app.state.upload_destination(), "@Sam");
    room(&mut app).members = 3;
    assert_eq!(app.state.upload_destination(), "Sam", "a group DM");
    room(&mut app).direct = false;
    assert_eq!(app.state.upload_destination(), "#Sam");
    room(&mut app).parent = Some("general".into());
    assert_eq!(app.state.upload_destination(), "\"Sam\"", "a thread");
}
