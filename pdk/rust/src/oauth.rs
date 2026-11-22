//! A plugin's own sign-in to a service, such as Spotify with an app of its
//! author's: `oauth.login`, for each provider the manifest's `oauth` names.
//!
//! [`login`] and [`token`] are answered later, as a [`Pending`]; the answer is
//! `{ provider, access_token, token_type, expires_at_ms, scopes }`, and the
//! plugin sends the token to the service's API itself with
//! [`http::fetch`](crate::http), the API's host in its manifest's `network`.

use serde_json::json;

use crate::call::{self, HostReturn, Pending, method};
use crate::{Error, Result};

fn ask(method: &str, provider: &str) -> Result<Pending> {
    match call::send(method, json!({ "provider": provider }))? {
        HostReturn::Pending { tag } => Ok(Pending::from_tag(tag)),
        other => call::value(method, other).and_then(|value| {
            Err(Error::msg(format!(
                "`{method}` answered at once, with {value}, where an answer later was expected"
            )))
        }),
    }
}

/// Signs the person in, in their browser: call it from a click, a command or
/// a menu row, since it needs one.
pub fn login(provider: &str) -> Result<Pending> {
    ask(method::OAUTH_LOGIN, provider)
}

/// The access token, renewed first when it is about to run out.
pub fn token(provider: &str) -> Result<Pending> {
    ask(method::OAUTH_TOKEN, provider)
}

/// Forgets the sign-in; true when there was one.
pub fn logout(provider: &str) -> Result<bool> {
    let value = call::ok(method::OAUTH_LOGOUT, json!({ "provider": provider }))?;
    Ok(value.as_bool().unwrap_or(false))
}
