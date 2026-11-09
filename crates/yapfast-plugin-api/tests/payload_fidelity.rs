//! The call and reply payloads cross the parent-helper boundary as bytes.
//!
//! Decoding a frame and writing it out again must not reorder keys, drop
//! whitespace, or rewrite a number: the guest export sees the bytes the parent
//! sent, and the parent sees the bytes the guest answered.

use serde_json::json;
use yapfast_plugin_api::{Frame, Kind, Payload, ProtocolError, Request, Response};

/// JSON no `serde_json::Value` would reproduce: keys out of order, spaces and
/// newlines, an exponent past f64, an integer past u64 and a `-0.0`.
const ODD: &str = "{\n    \"z\": 1E+400,\n    \"a\": 123456789012345678901234567890,\n    \"m\": {\"b\": -0.0, \"a\": true},\n    \"n\": 1.0\n  }";

fn call_frame(inner: &str, id: u64) -> Frame {
    let payload =
        format!(r#"{{"type":"call","plugin":"echo","method":"shout","payload":{inner}}}"#);
    Frame::new(Kind::Call, id, payload.into_bytes()).expect("frame")
}

fn reply_frame(inner: &str, id: u64) -> Frame {
    let payload = format!(r#"{{"type":"reply","payload":{inner}}}"#);
    Frame::new(Kind::Reply, id, payload.into_bytes()).expect("frame")
}

fn call_payload(request: &Request) -> &Payload {
    match request {
        Request::Call { payload, .. } => payload,
        other => panic!("{other:?} is not a call"),
    }
}

fn reply_payload(response: &Response) -> &Payload {
    match response {
        Response::Reply { payload } => payload,
        other => panic!("{other:?} is not a reply"),
    }
}

#[test]
fn a_decoded_call_payload_keeps_its_exact_bytes() {
    let frame = call_frame(ODD, 7);
    let request = Request::from_frame(&frame).expect("a call");

    assert_eq!(call_payload(&request).as_str(), ODD);
    assert_eq!(
        request.to_frame(7).expect("framing").payload(),
        format!(r#"{{"type":"call","plugin":"echo","method":"shout","payload":{ODD}}}"#).as_bytes()
    );
}

#[test]
fn a_decoded_reply_payload_keeps_its_exact_bytes() {
    let frame = reply_frame(ODD, 8);
    let response = Response::from_frame(&frame).expect("a reply");

    assert_eq!(reply_payload(&response).as_str(), ODD);
    assert_eq!(
        response.to_frame(8).expect("framing").payload(),
        format!(r#"{{"type":"reply","payload":{ODD}}}"#).as_bytes()
    );
}

#[test]
fn key_order_survives_decoding() {
    let first = Request::from_frame(&call_frame(r#"{"a":1,"b":2}"#, 1)).expect("a call");
    let second = Request::from_frame(&call_frame(r#"{"b":2,"a":1}"#, 1)).expect("a call");

    assert_ne!(
        first, second,
        "two key orders decoded as the same request, so one was rewritten"
    );
    assert_eq!(call_payload(&first).as_str(), r#"{"a":1,"b":2}"#);
    assert_eq!(call_payload(&second).as_str(), r#"{"b":2,"a":1}"#);
}

#[test]
fn number_forms_survive_decoding() {
    for inner in [
        "1E+400",
        "123456789012345678901234567890",
        "1.0",
        "-0.0",
        "0.30000000000000004",
        "1e-320",
        "[1, 2.50, 3e0]",
    ] {
        let request = Request::from_frame(&call_frame(inner, 2)).expect("a call");
        assert_eq!(call_payload(&request).as_str(), inner, "{inner}");
    }
}

#[test]
fn an_omitted_payload_is_raw_null() {
    let frame = Frame::new(
        Kind::Call,
        3,
        br#"{"type":"call","plugin":"echo","method":"noop"}"#.to_vec(),
    )
    .unwrap();
    let request = Request::from_frame(&frame).expect("a call");
    assert_eq!(call_payload(&request).as_str(), "null");
    assert_eq!(
        request.to_frame(3).expect("framing").payload(),
        br#"{"type":"call","plugin":"echo","method":"noop","payload":null}"#
    );
}

#[test]
fn a_payload_from_a_value_is_that_values_json() {
    // Programmatic construction has no source bytes: the payload is the Value's
    // own serialisation, and the docs say so.
    let payload = Payload::from(json!({ "a": 1, "b": [true, null] }));
    assert_eq!(payload.as_str(), r#"{"a":1,"b":[true,null]}"#);
    assert_eq!(
        payload.to_value().expect("JSON"),
        json!({ "a": 1, "b": [true, null] })
    );
    assert_eq!(Payload::default().as_str(), "null");
}

#[test]
fn a_payload_that_is_not_json_is_refused() {
    assert!(matches!(
        Payload::new("not json"),
        Err(ProtocolError::Json(_))
    ));
    assert!(matches!(Payload::new(""), Err(ProtocolError::Json(_))));
    assert!(matches!(
        Payload::from_bytes(b"{\"a\":"),
        Err(ProtocolError::Json(_))
    ));
    assert_eq!(
        Payload::from_bytes(br#"{"a":1}"#).expect("JSON").as_str(),
        r#"{"a":1}"#
    );
}

#[test]
fn a_payload_keeps_its_bytes_through_every_envelope() {
    let request = Request::Call {
        plugin: "echo".into(),
        method: "shout".into(),
        payload: Payload::new(ODD).expect("JSON"),
    };
    let frame = request.to_frame(4).expect("framing");
    assert_eq!(Request::from_frame(&frame).expect("a call"), request);

    let response = Response::Reply {
        payload: Payload::new(ODD).expect("JSON"),
    };
    let frame = response.to_frame(5).expect("framing");
    assert_eq!(Response::from_frame(&frame).expect("a reply"), response);
}
