//! How Yapfast presents itself to Discord. The gateway and HTTP API both carry the
//! same description, matching a current desktop browser on the web client.

use std::{
    sync::{
        Mutex, OnceLock, PoisonError,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};

/// A local fake Discord for tests and measurements, such as
/// `http://127.0.0.1:18400`; see testing/fake_discord.ts. While it is set,
/// Yapfast talks only to it, with [`TEST_TOKEN`] instead of the saved login.
pub fn test_server() -> Option<&'static str> {
    static SERVER: OnceLock<Option<String>> = OnceLock::new();
    SERVER
        .get_or_init(|| {
            let server = std::env::var("YAPFAST_DISCORD_TEST_SERVER").ok()?;
            let server = server.trim().trim_end_matches('/');
            (!server.is_empty()).then(|| server.to_owned())
        })
        .as_deref()
}

/// The token Yapfast uses with the test server.
pub const TEST_TOKEN: &str = "yapfast-test-token";

/// The Firefox release Yapfast claims to be: one behind the newest, as most
/// people's browsers are. Firefox 155 came on 1 September 2026, and one has
/// come every two weeks since; revisit this if Mozilla changes the cadence
/// again.
fn firefox_version() -> i64 {
    const FIRST: i64 = 155;
    let first = chrono::NaiveDate::from_ymd_opt(2026, 9, 1).expect("a real date");
    let days = (chrono::Utc::now().date_naive() - first).num_days();
    FIRST + days.max(0) / 14 - 1
}

/// The `User-Agent` of a current Firefox on Linux.
pub fn user_agent() -> &'static str {
    static AGENT: OnceLock<String> = OnceLock::new();
    AGENT.get_or_init(|| {
        let version = firefox_version();
        format!(
            "Mozilla/5.0 (X11; Linux x86_64; rv:{version}.0) Gecko/20100101 Firefox/{version}.0"
        )
    })
}

/// The web client's build number when Yapfast last checked, for when its page
/// can't be read.
const CLIENT_BUILD_NUMBER: u64 = 637268;
/// The build number read from the web client's page, this run or the last
/// time it was read, or 0.
static BUILD_NUMBER: AtomicU64 = AtomicU64::new(0);
/// [`BUILD_NUMBER`] was read from the page this run.
static LEARNED: AtomicBool = AtomicBool::new(false);

/// The build number Yapfast sends: the web client's current one once it's
/// known.
fn build_number() -> u64 {
    match BUILD_NUMBER.load(Ordering::Relaxed) {
        0 => CLIENT_BUILD_NUMBER,
        number => number,
    }
}

/// Whether the current build number was read this run.
pub(crate) fn knows_build_number() -> bool {
    LEARNED.load(Ordering::Relaxed)
}

/// Whether a build number read from the page is known: this run, or kept
/// from the last time it was read.
pub(crate) fn has_build_number() -> bool {
    BUILD_NUMBER.load(Ordering::Relaxed) != 0
}

/// Takes the build number kept from the last time the page was read,
/// unless one was read this run.
pub(crate) fn recall_build_number(number: u64) {
    let _ = BUILD_NUMBER.compare_exchange(0, number, Ordering::Relaxed, Ordering::Relaxed);
}

/// Takes the build number from the start of the web client's page, which
/// sets it in `GLOBAL_ENV`. Returns whether the page had one.
pub(crate) fn learn_build_number(page: &[u8]) -> bool {
    const KEY: &[u8] = br#""BUILD_NUMBER":""#;
    let Some(at) = page.windows(KEY.len()).position(|window| window == KEY) else {
        return false;
    };
    let digits = &page[at + KEY.len()..];
    let end = digits
        .iter()
        .position(|byte| !byte.is_ascii_digit())
        .unwrap_or(digits.len());
    let Some(number) = std::str::from_utf8(&digits[..end])
        .ok()
        .and_then(|digits| digits.parse::<u64>().ok())
        .filter(|&number| number > 0)
    else {
        return false;
    };
    BUILD_NUMBER.store(number, Ordering::Relaxed);
    LEARNED.store(true, Ordering::Relaxed);
    true
}

/// The build number known now: the one read from the page, or 0.
pub(crate) fn known_build_number() -> u64 {
    BUILD_NUMBER.load(Ordering::Relaxed)
}

/// A random UUID, as the web client makes them, with `cleared` bits unset.
fn uuid(cleared: u128) -> String {
    let mut bits = rand::random::<u128>();
    // Version 4, variant 1.
    bits = (bits & !(0xf << 76)) | (0x4 << 76);
    bits = (bits & !(0x3 << 62)) | (0x2 << 62);
    bits &= !cleared;
    let hex = format!("{bits:032x}");
    format!(
        "{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..]
    )
}

/// Identifies this run of Yapfast, as a launch of the client.
fn launch_id() -> &'static str {
    static ID: OnceLock<String> = OnceLock::new();
    ID.get_or_init(|| uuid(0))
}

/// The launch signature: a random UUID with the bits that report client
/// modifications unset.
fn launch_signature() -> &'static str {
    const MODIFICATIONS: u128 = 0x0080_1010_0810_0800_2081_0040_0100_0800;
    static SIGNATURE: OnceLock<String> = OnceLock::new();
    SIGNATURE.get_or_init(|| uuid(MODIFICATIONS))
}

/// The analytics heartbeat session, which the web client starts again every
/// 30 minutes.
fn heartbeat_session_id() -> String {
    const AGE: Duration = Duration::from_secs(30 * 60);
    static SESSION: Mutex<Option<(Instant, String)>> = Mutex::new(None);
    let mut session = SESSION.lock().unwrap_or_else(PoisonError::into_inner);
    if session
        .as_ref()
        .is_none_or(|(started, _)| started.elapsed() >= AGE)
    {
        *session = Some((Instant::now(), uuid(0)));
    }
    session
        .as_ref()
        .map(|(_, id)| id.clone())
        .unwrap_or_default()
}

/// The `properties` object sent when identifying on the gateway.
pub fn properties() -> Value {
    json!({
        "os": "Linux",
        "browser": "Firefox",
        "device": "",
        "system_locale": "en-US",
        "has_client_mods": false,
        "browser_user_agent": user_agent(),
        "browser_version": format!("{}.0", firefox_version()),
        "os_version": "",
        "referrer": "",
        "referring_domain": "",
        "referrer_current": "",
        "referring_domain_current": "",
        "release_channel": "stable",
        "client_build_number": build_number(),
        "client_event_source": null,
        "client_launch_id": launch_id(),
        "launch_signature": launch_signature(),
        "client_heartbeat_session_id": heartbeat_session_id(),
    })
}

/// The same properties for HTTP requests' `X-Super-Properties` header.
pub fn super_properties() -> String {
    STANDARD.encode(properties().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_build_number_from_the_web_page() {
        // One kept from the last start is sent until the page is read.
        recall_build_number(600000);
        assert!(has_build_number() && !knows_build_number());
        assert_eq!(build_number(), 600000);
        assert!(!learn_build_number(
            b"<script>window.GLOBAL_ENV = {}</script>"
        ));
        assert!(!learn_build_number(br#"{"BUILD_NUMBER":""}"#));
        assert!(learn_build_number(
            br#"GLOBAL_ENV = {"NODE_ENV":"production","BUILD_NUMBER":"637268","PROJECT_ENV":"production"}"#
        ));
        assert_eq!(build_number(), 637268);
        assert!(knows_build_number());
        recall_build_number(600000);
        assert_eq!(build_number(), 637268, "the one read this run stays");
    }

    #[test]
    fn launch_signatures_report_no_modifications() {
        let signature = launch_signature().replace('-', "");
        let bits = u128::from_str_radix(&signature, 16).unwrap();
        assert_eq!(bits & 0x0080_1010_0810_0800_2081_0040_0100_0800, 0);
        assert_eq!(launch_id().len(), 36);
        assert_eq!(&launch_id()[14..15], "4");
    }

    #[test]
    fn claims_a_firefox_from_this_year() {
        assert!(firefox_version() >= 157);
        assert!(user_agent().contains(&format!("Firefox/{}.0", firefox_version())));
    }
}
