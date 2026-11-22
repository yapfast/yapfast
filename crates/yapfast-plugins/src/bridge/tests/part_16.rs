use super::*;

use yapfast_plugin_api::call::{HostCall, HostReturn, UploadForm};
use yapfast_plugin_api::hook::{AttachmentPending, HookKind, HookOutcome, HookSubject};

fn upload(
    shared: &Shared,
    plugin: &PluginId,
    method: &str,
    payload: serde_json::Value,
) -> HostReturn {
    crate::bridge::session_answer::upload_call(
        shared,
        plugin,
        &HostCall {
            method: method.to_owned(),
            payload,
        },
    )
}

/// A file the upload chain holds is read in pieces while the chain runs, kept
/// by the plugin that answers `hold`, replaced piece by piece, and let go with
/// a link for the composer: what FileUpload does with a host of its own.
#[test]
fn a_held_upload_is_read_kept_replaced_and_finished() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let plugin = installed(directory.path(), UPLOAD_MANIFEST, b"not a real module");
    let (wake, notices) = notice_channel();
    let shared = shared(directory.path(), wake);
    let (_session_directory, mut session, written, _inbound) = session_stub();
    let picked = directory.path().join("cat.png");
    std::fs::write(&picked, b"the cat's bytes").expect("the person's file");

    let pending = AttachmentPending {
        room: "room-1".to_owned(),
        name: "cat.png".to_owned(),
        bytes: 15,
        content_type: Some("image/png".to_owned()),
        upload: 9,
        hold: false,
    };
    shared.uploads().hold(
        9,
        crate::uploads::Held::new("room-1", "cat.png", &picked, 15, 1024, vec![plugin.clone()]),
    );
    let held = hold(
        &mut session,
        &shared,
        9,
        HookKind::AttachmentPreUpload,
        HookSubject::Attachment(pending.clone()),
        "room-1",
    )
    .expect("a chain holds the file");
    let _activate = next_frame(&written);
    let hook = next_frame(&written);

    // Inside the chain the plugin may read the bytes.
    let answer = upload(
        &shared,
        &plugin,
        call::method::UPLOADS_READ,
        serde_json::json!({ "upload": 9, "offset": 4, "length": 3 }),
    );
    let HostReturn::Ok(piece) = answer else {
        panic!("a piece: {answer:?}");
    };
    assert_eq!(
        call::base64::decode(piece["data_base64"].as_str().unwrap()).unwrap(),
        b"cat"
    );
    assert_eq!(piece["bytes"], 15);

    // It answers `hold`: the file is its own, and the composer is told.
    let kept = AttachmentPending {
        hold: true,
        ..pending
    };
    session.on_frame(
        &shared,
        hook_done(
            hook.id,
            HookOutcome::Changed(HookSubject::Attachment(kept)),
            Some(plugin.clone()),
        ),
    );
    assert_eq!(
        notices
            .recv_timeout(Duration::from_secs(2))
            .expect("a notice"),
        Notice::UploadHeld {
            id: held,
            plugin: plugin.clone(),
            name: "cat.png".to_owned(),
        }
    );

    let wrote = upload(
        &shared,
        &plugin,
        call::method::UPLOADS_WRITE,
        serde_json::json!({ "upload": 9, "offset": 0, "data_base64": call::base64::encode(b"webp") }),
    );
    assert_eq!(wrote, HostReturn::Ok(serde_json::json!({ "written": 4 })));
    let (path, size, name) = shared.uploads().body(&plugin, 9, false).expect("a body");
    let form = crate::network::FileBody::new(
        path,
        size,
        &name,
        Some(&UploadForm {
            fields: vec![("reqtype".to_owned(), "fileupload".to_owned())],
            ..UploadForm::default()
        }),
    )
    .expect("a form");
    let before = String::from_utf8(form.before.clone()).unwrap();
    assert!(
        before.contains("name=\"reqtype\"\r\n\r\nfileupload\r\n"),
        "{before}"
    );
    assert!(
        before.contains("name=\"file\"; filename=\"cat.png\""),
        "{before}"
    );
    assert_eq!(
        form.length(),
        form.before.len() as u64 + 15 + form.after.len() as u64
    );
    assert!(
        form.content_type
            .unwrap()
            .starts_with("multipart/form-data; boundary=")
    );

    let finished = upload(
        &shared,
        &plugin,
        call::method::UPLOADS_FINISH,
        serde_json::json!({ "upload": 9, "send": "nothing", "insert": "https://files.example/cat" }),
    );
    assert_eq!(finished, HostReturn::Ok(serde_json::Value::Bool(true)));
    assert_eq!(
        notices
            .recv_timeout(Duration::from_secs(2))
            .expect("a notice"),
        Notice::UploadFinished {
            id: 9,
            file: None,
            name: "cat.png".to_owned(),
            insert: Some("https://files.example/cat".to_owned()),
        }
    );
    // Finished is finished.
    let again = upload(
        &shared,
        &plugin,
        call::method::UPLOADS_READ,
        serde_json::json!({ "upload": 9 }),
    );
    assert!(matches!(again, HostReturn::Error { .. }), "{again:?}");
}

/// Reading a held file is `uploads.intercept`'s, like the chain it comes from.
#[test]
fn a_plugin_without_uploads_intercept_reads_no_upload() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let plugin = installed(
        directory.path(),
        r#"{"api":1,"id":"com.example.quiet","name":"Quiet","version":"1.0.0",
            "wasm":"plugin.wasm","permissions":{"capabilities":["storage"]}}"#,
        b"not a real module",
    );
    let (wake, _notices) = notice_channel();
    let shared = shared(directory.path(), wake);
    let answer = upload(
        &shared,
        &plugin,
        call::method::UPLOADS_READ,
        serde_json::json!({ "upload": 1 }),
    );
    let HostReturn::Denied { reason } = answer else {
        panic!("should need uploads.intercept: {answer:?}");
    };
    assert!(reason.contains("uploads.intercept"), "{reason}");
}

/// An answer too large for one frame is kept for its plugin to read in
/// pieces, and for nobody else.
#[test]
fn a_large_answer_is_read_in_pieces_by_its_plugin() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let plugin = installed(directory.path(), UPLOAD_MANIFEST, b"not a real module");
    let (wake, _notices) = notice_channel();
    let shared = shared(directory.path(), wake);
    let path = directory.path().join("big.body");
    std::fs::write(&path, b"0123456789").expect("a body");
    let id = shared.keep_body(&plugin, path.clone(), 10);
    let read = |offset, length| call::BodyRead {
        body: id,
        offset,
        length,
    };
    assert_eq!(
        shared.read_body(&plugin, &read(3, Some(4))),
        Ok((b"3456".to_vec(), 10, false))
    );
    assert_eq!(
        shared.read_body(&plugin, &read(8, None)),
        Ok((b"89".to_vec(), 10, true))
    );
    let other = PluginId::parse("com.example.other").unwrap();
    assert!(shared.read_body(&other, &read(0, None)).is_err());
    assert!(!shared.release_body(&other, id));
    assert!(shared.release_body(&plugin, id));
    assert!(!path.exists(), "its file goes");
}
