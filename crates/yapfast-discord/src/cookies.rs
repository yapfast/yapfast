//! Cookies Discord's servers set, sent back on later requests as any HTTP
//! client that keeps cookies would. Discord sets device cookies on its first
//! answer, and Cloudflare, in front of it, sets its own for rate limiting and
//! bot management. Dropping them makes every request look like a new device.
//!
//! One jar serves every Discord request in the process, as a browser's does.
//! Persistent cookies are saved in Yapfast's data directory and load on the next
//! start; session cookies end with the process. With a test server the jar
//! stays in memory.

use std::{
    io::Write,
    path::PathBuf,
    sync::{Arc, LazyLock, Mutex},
};

use cookie_store::{CookieStore, RawCookie};
use reqwest::header::HeaderValue;
use url::Url;

use crate::client;

/// The cookies, and the file they're saved in.
#[derive(Default)]
pub struct Jar {
    store: Mutex<CookieStore>,
    file: Mutex<Option<PathBuf>>,
}

static JAR: LazyLock<Arc<Jar>> = LazyLock::new(Arc::default);

/// The process's jar, for an HTTP client.
pub(crate) fn jar() -> Arc<Jar> {
    JAR.clone()
}

/// Loads cookies saved in `file`, and saves them there from now on.
pub fn keep_in(file: PathBuf) {
    if client::test_server().is_some() {
        return;
    }
    match std::fs::File::open(&file) {
        Ok(saved) => match cookie_store::serde::json::load(std::io::BufReader::new(saved)) {
            Ok(store) => *JAR.store.lock().unwrap() = store,
            Err(error) => tracing::warn!(%error, "could not read the saved Discord cookies"),
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => tracing::warn!(%error, "could not open the saved Discord cookies"),
    }
    *JAR.file.lock().unwrap() = Some(file);
}

impl Jar {
    fn save(&self, store: &CookieStore) {
        let Some(file) = self.file.lock().unwrap().clone() else {
            return;
        };
        if let Err(error) = write(&file, store) {
            tracing::warn!(%error, "could not save the Discord cookies");
        }
    }
}

/// Writes the jar beside `file`, readable only by this user, then swaps it in.
fn write(file: &std::path::Path, store: &CookieStore) -> anyhow::Result<()> {
    let partial = file.with_extension("partial");
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut writer = std::io::BufWriter::new(crate::cache::open_in_folder(&options, &partial)?);
    cookie_store::serde::json::save(store, &mut writer)
        .map_err(|error| anyhow::anyhow!("{error}"))?;
    writer.flush()?;
    drop(writer);
    std::fs::rename(&partial, file)?;
    Ok(())
}

impl reqwest::cookie::CookieStore for Jar {
    fn set_cookies(&self, headers: &mut dyn Iterator<Item = &HeaderValue>, url: &Url) {
        let cookies: Vec<RawCookie<'static>> = headers
            .filter_map(|header| header.to_str().ok())
            .filter_map(|header| RawCookie::parse(header.to_owned()).ok())
            .collect();
        if cookies.is_empty() {
            return;
        }
        let mut store = self.store.lock().unwrap();
        store.store_response_cookies(cookies.into_iter(), url);
        self.save(&store);
    }

    fn cookies(&self, url: &Url) -> Option<HeaderValue> {
        let store = self.store.lock().unwrap();
        let cookies = store
            .get_request_values(url)
            .map(|(name, value)| format!("{name}={value}"))
            .collect::<Vec<_>>()
            .join("; ");
        if cookies.is_empty() {
            return None;
        }
        HeaderValue::from_str(&cookies).ok()
    }
}

#[cfg(test)]
mod tests {
    use reqwest::cookie::CookieStore as _;

    use super::*;

    #[test]
    fn sends_back_what_the_server_set_where_it_applies() {
        let jar = Jar::default();
        let api = Url::parse("https://discord.com/api/v9/users/@me").unwrap();
        jar.set_cookies(
            &mut [
                HeaderValue::from_static(
                    "__dcfduid=device; Max-Age=31536000; Path=/; Secure; HttpOnly",
                ),
                HeaderValue::from_static("__cf_bm=bot; Path=/; Domain=discord.com; Secure"),
            ]
            .iter(),
            &api,
        );
        let sent = jar.cookies(&api).unwrap();
        let sent = sent.to_str().unwrap();
        assert!(sent.contains("__dcfduid=device"), "{sent}");
        assert!(sent.contains("__cf_bm=bot"), "{sent}");
        // Not to other sites, nor over plain HTTP.
        assert!(
            jar.cookies(&Url::parse("https://example.com/").unwrap())
                .is_none()
        );
        assert!(
            jar.cookies(&Url::parse("http://discord.com/api/v9").unwrap())
                .is_none()
        );
    }

    #[test]
    fn saves_persistent_cookies_and_loads_them_again() {
        let file =
            std::env::temp_dir().join(format!("yapfast-cookies-{}.json", std::process::id()));
        let jar = Jar::default();
        *jar.file.lock().unwrap() = Some(file.clone());
        let api = Url::parse("https://discord.com/api/v9/users/@me").unwrap();
        jar.set_cookies(
            &mut [
                HeaderValue::from_static("__dcfduid=device; Max-Age=31536000; Path=/; Secure"),
                HeaderValue::from_static("session=gone; Path=/; Secure"),
            ]
            .iter(),
            &api,
        );
        let saved = std::fs::read_to_string(&file).unwrap();
        let store =
            cookie_store::serde::json::load(std::io::BufReader::new(saved.as_bytes())).unwrap();
        std::fs::remove_file(&file).unwrap();
        let names: Vec<_> = store
            .get_request_values(&api)
            .map(|(name, _)| name)
            .collect();
        assert_eq!(names, ["__dcfduid"]);
    }
}
