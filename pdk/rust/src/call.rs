//! Host calls: the one request-and-answer import and the fire-and-forget one.
//!
//! Both take one JSON string and, for the answering one, return one. The
//! method name lives inside the JSON, so a method added later needs no new
//! import and a plugin built before it gets `unsupported` instead of failing
//! to link.

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::Error;
use crate::json;
use crate::wire::HostCall;

pub use crate::wire::{CallResult, HostReturn};

#[::extism_pdk::host_fn("extism:host/user")]
extern "ExtismHost" {
    /// One call the helper answers, now or in a later call.
    pub fn yf_call(input: String) -> String;
    /// One call with no answer.
    pub fn yf_post(input: String);
}

/// A call whose answer is coming in a later call.
///
/// The helper answers a call it can't complete with the id it will answer
/// under, written as a decimal string in [`HostReturn::Pending`]. Hold the
/// `Pending` and ask [`Context::answer`](crate::plugin::Context::answer) for it: the
/// answer arrives in a later event batch as a [`CallResult`], which carries
/// both that id and the answer itself.
///
/// A request can carry a name of its own as well — `HttpRequest` has a `tag`
/// the answer echoes — but that is the plugin's label for its own bookkeeping,
/// not how the two halves of a call are paired.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pending {
    tag: String,
}

impl Pending {
    pub(crate) fn from_tag(tag: String) -> Self {
        Pending { tag }
    }

    /// The tag the helper gave the deferred call.
    pub fn tag(&self) -> &str {
        &self.tag
    }

    /// The call id the answer will carry, when the helper's tag is one.
    ///
    /// This is what [`Context::answer`](crate::plugin::Context::answer) matches on.
    pub fn call(&self) -> Option<u64> {
        self.tag.parse().ok()
    }
}

impl From<HostReturn> for Option<Pending> {
    fn from(answer: HostReturn) -> Self {
        match answer {
            HostReturn::Pending { tag } => Some(Pending { tag }),
            _ => None,
        }
    }
}

/// Makes one host call and returns its answer, whatever it is.
///
/// Every method has a typed wrapper in this crate; this is the one to reach
/// for when a method is newer than the PDK, and the answer can be read as
/// [`Value`].
pub fn send(method: &str, payload: impl Serialize) -> Result<HostReturn, Error> {
    let request = HostCall {
        method: method.to_owned(),
        payload: json::to_value(payload)?,
    };
    let request = json::to_string(&request)?;
    // The import is unsafe because it crosses into the host: the helper is
    // trusted to give back a well-formed UTF-8 payload, which is then checked.
    #[allow(unsafe_code, reason = "an Extism host import")]
    let answer = unsafe { yf_call(request)? };
    if answer.is_empty() {
        return Err(Error::msg(format!("`{method}` was answered with nothing")));
    }
    json::from_str(&answer)
        .map_err(|error| Error::msg(format!("`{method}`'s answer was not JSON: {error}")))
}

/// Makes one host call and returns its `ok` payload.
///
/// Every other answer is an error a plugin can show: a method this build
/// doesn't have, a refusal, or a failure the host explained.
pub fn ok(method: &str, payload: impl Serialize) -> Result<Value, Error> {
    value(method, send(method, payload)?)
}

/// The `ok` payload of an answer, or the error the answer amounts to.
pub fn value(method: &str, answer: HostReturn) -> Result<Value, Error> {
    match answer {
        HostReturn::Ok(value) => Ok(value),
        HostReturn::Unsupported => Err(Error::msg(format!(
            "this Yapfast doesn't have `{method}`; it may need to be enabled in the plugin's manifest"
        ))),
        HostReturn::Denied { reason } => Err(Error::msg(format!("`{method}` was refused: {reason}"))),
        HostReturn::Pending { tag } => Err(Error::msg(format!(
            "`{method}` answered later, under `{tag}`, and this caller can't wait"
        ))),
        HostReturn::Error { message } => Err(Error::msg(format!("`{method}` failed: {message}"))),
    }
}

/// Makes one host call with no answer, for a method whose effect is the point.
pub fn post(method: &str, payload: impl Serialize) -> Result<(), Error> {
    let request = HostCall {
        method: method.to_owned(),
        payload: json::to_value(payload)?,
    };
    let request = json::to_string(&request)?;
    // Safety: as in `send`, and nothing comes back to interpret.
    #[allow(unsafe_code, reason = "an Extism host import")]
    unsafe {
        yf_post(request)?;
    }
    Ok(())
}

/// Decodes an answer's payload as `T`.
pub(crate) fn parse<T: DeserializeOwned>(method: &str, value: Value) -> Result<T, Error> {
    json::from_value(value)
        .map_err(|error| Error::msg(format!("`{method}`'s answer made no sense: {error}")))
}

/// Sends `method` and decodes its answer's payload as `T`.
pub(crate) fn call<T: DeserializeOwned>(
    method: &str,
    payload: impl Serialize,
) -> Result<T, Error> {
    parse(method, ok(method, payload)?)
}

/// The method names this crate calls, for reading this module as a list.
pub use crate::wire::method;
