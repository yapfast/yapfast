use super::*;

/// Waits until `deadline`, or for ever when there is nothing due.
pub(super) async fn sleep_until(deadline: Option<Instant>) {
    match deadline {
        Some(deadline) => tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)).await,
        None => std::future::pending().await,
    }
}

/// The bytes a plugin brought base64 inside its call, or the sentence saying
/// why they aren't usable.
pub(super) fn brought(encoded: &str, cap: usize) -> Result<Vec<u8>, String> {
    let bytes = call::base64::decode(encoded)
        .ok_or_else(|| "the bytes this call carries aren't base64".to_owned())?;
    if bytes.len() > cap {
        return Err(format!(
            "this call carries {} bytes, and {} is the most it may",
            bytes.len(),
            cap
        ));
    }
    Ok(bytes)
}

pub(super) fn wrong_payload(method: &str, error: serde_json::Error) -> HostReturn {
    HostReturn::Error {
        message: format!("`{method}` was given a payload it can't read: {error}"),
    }
}

/// A read's answer, or the sentence saying the value couldn't be written as
/// JSON. That can't happen for the views the world answers with, and a call
/// that answered `ok: null` instead would look like an empty result.
pub(super) fn answered<T: serde::Serialize>(answer: T) -> HostReturn {
    match serde_json::to_value(answer) {
        Ok(value) => HostReturn::Ok(value),
        Err(error) => HostReturn::Error {
            message: format!("could not write the answer: {error}"),
        },
    }
}

/// A read that may have nothing to say: `null`, which is what a plugin reads as
/// "there is none" without an error to catch.
pub(super) fn answered_option<T: serde::Serialize>(answer: Option<T>) -> HostReturn {
    match answer {
        Some(answer) => answered(answer),
        None => HostReturn::Ok(serde_json::Value::Null),
    }
}

/// A read's payload, or the answer saying it couldn't be read.
pub(super) fn parsed<T: serde::de::DeserializeOwned>(
    call: &HostCall,
    method: &str,
) -> Result<T, HostReturn> {
    call.parse().map_err(|error| wrong_payload(method, error))
}

/// The answer to a read about a room the application isn't showing.
///
/// The application keeps one timeline and one member list, the open room's, so
/// a question about another room is answered with why rather than with an empty
/// page that would look like a room nobody has spoken in.
pub(super) fn unknown_room(room: &str, what: &str) -> HostReturn {
    HostReturn::Error {
        message: format!(
            "{what} of {room} isn't open: Yapfast keeps the conversation on screen, \
             and `rooms.selected` names it"
        ),
    }
}
