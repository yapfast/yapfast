//! The request and response envelopes a frame carries.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use yapfast_plugin_api::{
    Frame, FrameError, Kind, MAX_PAYLOAD_LEN, ProtocolError, Request, Response,
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
        payload: json!({ "text": "hi" }),
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
            payload: Value::Null
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
            payload: json!({ "text": "hi" }),
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
            payload: json!({ "text": "hi" })
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
            payload: json!({ "text": "Ærø ☃", "n": [1, 2] }),
        },
        Request::Call {
            plugin: "echo".into(),
            method: "noop".into(),
            payload: Value::Null,
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
            payload: json!({ "text": "Ærø ☃", "ok": true }),
        },
        Response::Reply {
            payload: Value::Null,
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
            payload: Value::Null,
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
    let frame = request_frame(
        Kind::Load,
        json!({ "type": "unload", "plugin": "echo" }),
    );
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
    let ready = Frame::new(Kind::Ready, 1, br#"{"type":"ready"}"#.to_vec()).unwrap();
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
        payload: Value::String("x".repeat(MAX_PAYLOAD_LEN)),
    };
    assert!(matches!(
        huge.to_frame(1),
        Err(ProtocolError::Frame(FrameError::TooLarge { .. }))
    ));
}
