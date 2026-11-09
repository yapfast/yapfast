//! The request and response envelopes a frame carries.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use yapfast_plugin_api::{
    Frame, FrameError, Kind, MAX_PAYLOAD_LEN, Payload, ProtocolError, Request, Response,
};

/// An absolute module path, as `Load` requires.
fn module(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(name)
}

fn request_frame(kind: Kind, value: Value) -> Frame {
    Frame::new(kind, 1, serde_json::to_vec(&value).expect("JSON")).expect("frame")
}

#[test]
fn request_kinds_map_to_frame_kinds() {
    let load = Request::Load {
        plugin: "echo".into(),
        module: module("echo.wasm"),
    };
    let call = Request::Call {
        plugin: "echo".into(),
        method: "shout".into(),
        payload: json!({ "text": "hi" }).into(),
    };
    let unload = Request::Unload {
        plugin: "echo".into(),
    };
    assert_eq!(load.kind(), Kind::Load);
    assert_eq!(call.kind(), Kind::Call);
    assert_eq!(unload.kind(), Kind::Unload);
    assert_eq!(Request::Shutdown.kind(), Kind::Shutdown);
}

#[test]
fn response_kinds_map_to_frame_kinds() {
    assert_eq!(Response::Ready.kind(), Kind::Ready);
    assert_eq!(Response::Loaded.kind(), Kind::Loaded);
    assert_eq!(
        Response::Reply {
            payload: Payload::null()
        }
        .kind(),
        Kind::Reply
    );
    assert_eq!(Response::Unloaded.kind(), Kind::Unloaded);
    assert_eq!(
        Response::Error {
            message: "no".into()
        }
        .kind(),
        Kind::Error
    );
}

#[test]
fn request_json_shapes_are_stable() {
    let module = module("echo.wasm");
    let load = Request::Load {
        plugin: "echo".into(),
        module: module.clone(),
    };
    assert_eq!(
        load.to_value().expect("JSON"),
        json!({ "type": "load", "plugin": "echo", "module": module })
    );
    assert_eq!(
        Request::Call {
            plugin: "echo".into(),
            method: "shout".into(),
            payload: json!({ "text": "hi" }).into(),
        }
        .to_value()
        .expect("JSON"),
        json!({
            "type": "call",
            "plugin": "echo",
            "method": "shout",
            "payload": { "text": "hi" }
        })
    );
    assert_eq!(
        Request::Unload {
            plugin: "echo".into()
        }
        .to_value()
        .expect("JSON"),
        json!({ "type": "unload", "plugin": "echo" })
    );
    assert_eq!(
        Request::Shutdown.to_value().expect("JSON"),
        json!({ "type": "shutdown" })
    );
}

#[test]
fn response_json_shapes_are_stable() {
    assert_eq!(
        Response::Ready.to_value().expect("JSON"),
        json!({ "type": "ready" })
    );
    assert_eq!(
        Response::Loaded.to_value().expect("JSON"),
        json!({ "type": "loaded" })
    );
    assert_eq!(
        Response::Reply {
            payload: json!({ "text": "hi" }).into()
        }
        .to_value()
        .expect("JSON"),
        json!({ "type": "reply", "payload": { "text": "hi" } })
    );
    assert_eq!(
        Response::Unloaded.to_value().expect("JSON"),
        json!({ "type": "unloaded" })
    );
    assert_eq!(
        Response::Error {
            message: "the module trapped".into()
        }
        .to_value()
        .expect("JSON"),
        json!({ "type": "error", "message": "the module trapped" })
    );
}

#[test]
fn requests_round_trip_through_frames() {
    let requests = [
        Request::Load {
            plugin: "echo".into(),
            module: module("echo.wasm"),
        },
        Request::Call {
            plugin: "echo".into(),
            method: "shout".into(),
            payload: json!({ "text": "Ærø ☃", "n": [1, 2] }).into(),
        },
        Request::Call {
            plugin: "echo".into(),
            method: "noop".into(),
            payload: Payload::null(),
        },
        Request::Unload {
            plugin: "echo".into(),
        },
        Request::Shutdown,
    ];
    for (index, request) in requests.into_iter().enumerate() {
        let id = u64::try_from(index).unwrap() + 100;
        let frame = request.to_frame(id).expect("framing");
        assert_eq!(frame.kind(), request.kind());
        assert_eq!(frame.id(), id);
        assert_eq!(Request::from_frame(&frame).expect("parsing"), request);
    }
}

#[test]
fn responses_round_trip_through_frames() {
    let responses = [
        Response::Ready,
        Response::Loaded,
        Response::Reply {
            payload: json!({ "text": "Ærø ☃", "ok": true }).into(),
        },
        Response::Reply {
            payload: Payload::null(),
        },
        Response::Unloaded,
        Response::Error {
            message: "Øresund timed out".into(),
        },
    ];
    for (index, response) in responses.into_iter().enumerate() {
        let id = u64::try_from(index).unwrap();
        let frame = response.to_frame(id).expect("framing");
        assert_eq!(Response::from_frame(&frame).expect("parsing"), response);
    }
}

#[test]
fn a_call_without_a_payload_reads_as_null() {
    let value = json!({ "type": "call", "plugin": "echo", "method": "noop" });
    assert_eq!(
        Request::from_value(value).expect("parsing"),
        Request::Call {
            plugin: "echo".into(),
            method: "noop".into(),
            payload: Payload::null(),
        }
    );
}

#[test]
fn a_load_must_name_an_absolute_module() {
    let relative = json!({
        "type": "load",
        "plugin": "echo",
        "module": "plugins/echo.wasm"
    });
    assert!(
        matches!(
            Request::from_value(relative),
            Err(ProtocolError::RelativeModulePath(path)) if path == Path::new("plugins/echo.wasm")
        ),
        "a relative module path was accepted"
    );

    let unbuilt = Request::Load {
        plugin: "echo".into(),
        module: PathBuf::from("plugins/echo.wasm"),
    };
    assert!(matches!(
        unbuilt.to_value(),
        Err(ProtocolError::RelativeModulePath(_))
    ));
    assert!(matches!(
        unbuilt.to_frame(1),
        Err(ProtocolError::RelativeModulePath(_))
    ));
}

#[test]
fn an_unknown_type_is_a_json_error() {
    assert!(matches!(
        Request::from_value(json!({ "type": "reload", "plugin": "echo" })),
        Err(ProtocolError::Json(_))
    ));
    assert!(matches!(
        Response::from_value(json!({ "type": "call" })),
        Err(ProtocolError::Json(_))
    ));
}

#[test]
fn a_frame_whose_kind_contradicts_its_json_is_refused() {
    let frame = request_frame(Kind::Load, json!({ "type": "unload", "plugin": "echo" }));
    assert!(matches!(
        Request::from_frame(&frame),
        Err(ProtocolError::KindMismatch {
            kind: Kind::Load,
            payload: Kind::Unload
        })
    ));

    let frame = Frame::new(Kind::Reply, 1, br#"{"type":"ready"}"#.to_vec()).unwrap();
    assert!(matches!(
        Response::from_frame(&frame),
        Err(ProtocolError::KindMismatch {
            kind: Kind::Reply,
            payload: Kind::Ready
        })
    ));
}

#[test]
fn a_frame_of_the_other_direction_is_refused() {
    let ready = Frame::new(Kind::Ready, 0, br#"{"type":"ready"}"#.to_vec()).unwrap();
    assert!(matches!(
        Request::from_frame(&ready),
        Err(ProtocolError::NotRequest(Kind::Ready))
    ));

    let call = request_frame(
        Kind::Call,
        json!({ "type": "call", "plugin": "echo", "method": "shout" }),
    );
    assert!(matches!(
        Response::from_frame(&call),
        Err(ProtocolError::NotResponse(Kind::Call))
    ));
}

#[test]
fn a_payload_of_the_wrong_json_shape_is_a_json_error() {
    let frame = Frame::new(Kind::Call, 1, b"[]".to_vec()).unwrap();
    assert!(matches!(
        Request::from_frame(&frame),
        Err(ProtocolError::Json(_))
    ));

    let frame = Frame::new(Kind::Reply, 1, b"not json".to_vec()).unwrap();
    assert!(matches!(
        Response::from_frame(&frame),
        Err(ProtocolError::Json(_))
    ));
}

#[test]
fn an_envelope_too_large_for_a_frame_is_refused() {
    let huge = Request::Call {
        plugin: "echo".into(),
        method: "shout".into(),
        payload: Payload::from(Value::String("x".repeat(MAX_PAYLOAD_LEN))),
    };
    assert!(matches!(
        huge.to_frame(1),
        Err(ProtocolError::Frame(FrameError::TooLarge { .. }))
    ));
}

#[test]
fn frame_id_zero_is_reserved_for_the_unsolicited_ready() {
    // A request may not use it: nothing would answer, since it answers nothing.
    for request in [
        Request::Shutdown,
        Request::Unload {
            plugin: "echo".into(),
        },
        Request::Load {
            plugin: "echo".into(),
            module: module("echo.wasm"),
        },
        Request::Call {
            plugin: "echo".into(),
            method: "shout".into(),
            payload: Payload::null(),
        },
    ] {
        assert!(
            matches!(
                request.to_frame(0),
                Err(ProtocolError::ReservedId(kind)) if kind == request.kind()
            ),
            "{request:?} was framed with id 0"
        );

        let valid = request.to_frame(1).expect("framing");
        let zeroed = Frame::new(valid.kind(), 0, valid.payload().to_vec()).expect("frame");
        assert!(
            matches!(
                Request::from_frame(&zeroed),
                Err(ProtocolError::ReservedId(kind)) if kind == request.kind()
            ),
            "{request:?} was read from id 0"
        );
    }

    let shutdown = Frame::new(Kind::Shutdown, 0, br#"{"type":"shutdown"}"#.to_vec()).unwrap();
    assert!(matches!(
        Request::from_frame(&shutdown),
        Err(ProtocolError::ReservedId(Kind::Shutdown))
    ));
}

#[test]
fn ready_is_the_only_response_frame_with_id_zero() {
    let ready = Response::Ready.to_frame(0).expect("framing");
    assert_eq!(ready.id(), 0);
    assert_eq!(
        Response::from_frame(&ready).expect("parsing"),
        Response::Ready
    );

    assert!(matches!(
        Response::Ready.to_frame(1),
        Err(ProtocolError::ReadyWithId(1))
    ));
    let ready = Frame::new(Kind::Ready, 9, br#"{"type":"ready"}"#.to_vec()).unwrap();
    assert!(matches!(
        Response::from_frame(&ready),
        Err(ProtocolError::ReadyWithId(9))
    ));

    for response in [
        Response::Loaded,
        Response::Unloaded,
        Response::Reply {
            payload: Payload::null(),
        },
        Response::Error {
            message: "no".into(),
        },
    ] {
        assert!(
            matches!(
                response.to_frame(0),
                Err(ProtocolError::ReservedId(kind)) if kind == response.kind()
            ),
            "{response:?} was framed with id 0"
        );

        let valid = response.to_frame(1).expect("framing");
        let zeroed = Frame::new(valid.kind(), 0, valid.payload().to_vec()).expect("frame");
        assert!(
            matches!(
                Response::from_frame(&zeroed),
                Err(ProtocolError::ReservedId(kind)) if kind == response.kind()
            ),
            "{response:?} was read from id 0"
        );
    }
}

#[test]
fn an_error_is_a_response_in_either_direction() {
    // The helper reports a call it could not answer; the parent reports a
    // request it could not serve. Both are this one response envelope, paired
    // with the id of the exchange that failed.
    let frame = Frame::new(
        Kind::Error,
        7,
        br#"{"type":"error","message":"the module trapped"}"#.to_vec(),
    )
    .expect("frame");

    let response = Response::from_frame(&frame).expect("a response");
    assert_eq!(
        response,
        Response::Error {
            message: "the module trapped".into()
        }
    );
    assert_eq!(
        response.to_frame(7).expect("framing").payload(),
        frame.payload()
    );

    // It is never a request, whichever side wrote it.
    assert!(matches!(
        Request::from_frame(&frame),
        Err(ProtocolError::NotRequest(Kind::Error))
    ));
    assert!(matches!(
        Response::from_frame(
            &Frame::new(
                Kind::Error,
                0,
                br#"{"type":"error","message":"no"}"#.to_vec()
            )
            .unwrap()
        ),
        Err(ProtocolError::ReservedId(Kind::Error))
    ));
}

#[test]
fn shutdown_requires_its_documented_envelope() {
    let request = Request::Shutdown.to_frame(1).expect("framing");
    assert_eq!(request.kind(), Kind::Shutdown);
    assert_eq!(request.payload(), br#"{"type":"shutdown"}"#);

    // No bare frame, and no `{}` standing in for the envelope.
    for payload in [&b""[..], &b"{}"[..], &b"null"[..]] {
        let frame = Frame::new(Kind::Shutdown, 1, payload.to_vec()).expect("frame");
        assert!(
            matches!(Request::from_frame(&frame), Err(ProtocolError::Json(_))),
            "{payload:?} was accepted as a shutdown"
        );
    }

    let frame = Frame::new(Kind::Shutdown, 1, br#"{"type":"shutdown"}"#.to_vec()).unwrap();
    assert_eq!(
        Request::from_frame(&frame).expect("parsing"),
        Request::Shutdown
    );
}
