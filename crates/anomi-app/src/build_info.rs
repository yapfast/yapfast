//! Which build of Anomi this is, as Settings shows it under the sections and
//! on About: the Slint app's `src/build_info.rs`, whose facts `build.rs`
//! hands this crate too, worded as `src/ui/yapfast.rs` shows them (the
//! window names this fork). A build is a development build unless its
//! packaging says it's a release; a nightly's version names it.

use anomi_vm::settings::frame::{Build, Tone};
use yapfast_lang::PRODUCT;

/// The program's version: the workspace's, as `--version` gives it.
pub const VERSION: &str = match option_env!("ANOMI_BUILD_VERSION") {
    Some(version) => version,
    None => env!("CARGO_PKG_VERSION"),
};
const COMMIT: Option<&str> = option_env!("ANOMI_BUILD_COMMIT");
const DATE: Option<&str> = option_env!("ANOMI_BUILD_DATE");
const RELEASE: bool = option_env!("ANOMI_BUILD_RELEASE").is_some();
/// How many characters of a commit's hash show.
const SHORT_COMMIT: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Channel {
    Stable,
    Nightly,
    Prerelease,
    Development,
}

impl Channel {
    fn of(version: &str, release: bool) -> Self {
        if version.contains("-nightly.") {
            Self::Nightly
        } else if !release {
            Self::Development
        } else if version.contains('-') {
            Self::Prerelease
        } else {
            Self::Stable
        }
    }

    /// Its name, in English: the summary keeps it so.
    fn label(self) -> &'static str {
        match self {
            Self::Stable => "Stable",
            Self::Nightly => "Nightly",
            Self::Prerelease => "Pre-release",
            Self::Development => "Development build",
        }
    }

    fn tone(self) -> Tone {
        match self {
            Self::Stable => Tone::Release,
            Self::Nightly | Self::Prerelease => Tone::Nightly,
            Self::Development => Tone::Development,
        }
    }
}

/// This build, in the interface's language now.
pub fn current() -> Build {
    build(VERSION, RELEASE, COMMIT, DATE)
}

fn build(version: &str, release: bool, commit: Option<&str>, date: Option<&str>) -> Build {
    let commit = commit
        .map(str::trim)
        .filter(|commit| !commit.is_empty())
        .map(|commit| commit.get(..SHORT_COMMIT).unwrap_or(commit));
    let date = date.map(str::trim).filter(|date| !date.is_empty());
    let channel = Channel::of(version, release);
    let headline = match (channel, commit) {
        (Channel::Development, Some(commit)) => format!("{PRODUCT} {commit}"),
        (Channel::Development, None) => PRODUCT.to_owned(),
        _ => format!("{PRODUCT} v{version}"),
    };
    let shown_commit = commit.filter(|_| channel != Channel::Development);
    let details = [shown_commit, date].into_iter().flatten();
    let named = format!("{PRODUCT} v{version}");
    let summary = [Some(named.as_str()), Some(channel.label()), commit, date];
    Build {
        headline,
        channel: yapfast_lang::text(channel.label()).to_owned(),
        details: details.collect::<Vec<_>>().join(" · "),
        summary: summary
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" · "),
        tone: channel.tone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_development_build_names_its_commit_and_a_release_its_version() {
        let dev = build("0.4.1", false, Some("acd914cdef01"), Some("2026-11-16"));
        assert_eq!(dev.headline, format!("{PRODUCT} acd914cd"));
        assert_eq!(dev.details, "2026-11-16");
        assert_eq!(dev.tone, Tone::Development);
        assert_eq!(
            dev.summary,
            format!("{PRODUCT} v0.4.1 · Development build · acd914cd · 2026-11-16")
        );
        let nightly = build("0.5.0-nightly.20261107.12", true, Some("abc"), None);
        assert_eq!(
            nightly.headline,
            format!("{PRODUCT} v0.5.0-nightly.20261107.12")
        );
        assert_eq!(
            (nightly.details.as_str(), nightly.tone),
            ("abc", Tone::Nightly)
        );
        assert_eq!(build("0.4.1", true, None, None).tone, Tone::Release);
    }
}
