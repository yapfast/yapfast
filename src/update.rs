//! Whether a newer Yapfast has been released, and how this one was installed.
//!
//! Settings' About page can look the releases up on GitHub, say what it found
//! and, when a person presses Update and restart, install it ([`install`]).
//! The decisions are pure functions of the release list, this build and where
//! its program sits, so tests cover them without a network. [`check`] is the
//! one small request when a person turns the check on or presses Check now.
//!
//! The check is off until it's turned on: Yapfast makes no request of its own
//! to GitHub otherwise, and it installs only what a person asked for. See
//! docs/packaging.md.

use std::cmp::Ordering;

use crate::build_info::Channel;

pub mod install;

/// Where Yapfast's releases are listed. Thirty reach the newest stable release
/// behind a run of nightlies.
const RELEASES: &str = "https://api.github.com/repos/yapfast/yapfast/releases?per_page=30";

/// Where a release's page is, for the browser to open: only this project's.
const PAGES: &str = "https://github.com/yapfast/yapfast/releases/";

/// A SemVer version, as `scripts/set-version` allows them: a core of three
/// numbers and a dotted prerelease part. Build metadata is dropped, as it
/// doesn't take part in precedence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Version {
    core: [u64; 3],
    pre: Vec<Identifier>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Identifier {
    Number(u64),
    Text(String),
}

impl Ord for Identifier {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Identifier::Number(a), Identifier::Number(b)) => a.cmp(b),
            // A number sorts before text, as SemVer says.
            (Identifier::Number(_), Identifier::Text(_)) => Ordering::Less,
            (Identifier::Text(_), Identifier::Number(_)) => Ordering::Greater,
            (Identifier::Text(a), Identifier::Text(b)) => a.cmp(b),
        }
    }
}

impl PartialOrd for Identifier {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Version {
    /// Reads `0.4.0`, `v0.4.0` and `0.5.0-nightly.20261107.12`; anything else
    /// is not a version of Yapfast's.
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        let text = text.strip_prefix('v').unwrap_or(text);
        let text = text.split('+').next()?;
        let (core, pre) = match text.split_once('-') {
            Some((core, pre)) => (core, Some(pre)),
            None => (text, None),
        };
        let mut numbers = core.split('.');
        let mut next = || numbers.next()?.parse::<u64>().ok();
        let core = [next()?, next()?, next()?];
        if numbers.next().is_some() {
            return None;
        }
        let pre = match pre {
            None => Vec::new(),
            Some(pre) => pre
                .split('.')
                .map(|part| match part {
                    "" => None,
                    part => Some(
                        part.parse()
                            .map_or_else(|_| Identifier::Text(part.to_owned()), Identifier::Number),
                    ),
                })
                .collect::<Option<Vec<_>>>()?,
        };
        Some(Self { core, pre })
    }

    pub fn is_nightly(&self) -> bool {
        matches!(self.pre.first(), Some(Identifier::Text(text)) if text == "nightly")
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        self.core.cmp(&other.core).then_with(|| {
            // A release sorts after its own prereleases.
            match (self.pre.is_empty(), other.pre.is_empty()) {
                (true, true) => Ordering::Equal,
                (true, false) => Ordering::Greater,
                (false, true) => Ordering::Less,
                (false, false) => self.pre.cmp(&other.pre),
            }
        })
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// A file on a release.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub size: u64,
}

/// A published release of Yapfast.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    pub version: Version,
    /// The tag, such as `v0.4.0`, which Skip remembers.
    pub tag: String,
    /// The release's page, where its notes and files are.
    pub page: String,
    pub assets: Vec<Asset>,
    /// What's new, as plain lines ([`notes_of`]). Only the newest stable
    /// release and the newest nightly keep theirs, so the thirty listed don't
    /// stay in memory for hours.
    pub notes: String,
}

/// Reads GitHub's list of releases (`GET /repos/{owner}/{repo}/releases`),
/// leaving out drafts and tags that aren't versions.
pub fn parse_releases(json: &[u8]) -> Result<Vec<Release>, serde_json::Error> {
    #[derive(serde::Deserialize)]
    struct Listed {
        tag_name: String,
        #[serde(default)]
        draft: bool,
        #[serde(default)]
        html_url: String,
        #[serde(default)]
        body: Option<String>,
        #[serde(default)]
        assets: Vec<ListedAsset>,
    }
    #[derive(serde::Deserialize)]
    struct ListedAsset {
        name: String,
        browser_download_url: String,
        #[serde(default)]
        size: u64,
    }
    let listed: Vec<Listed> = serde_json::from_slice(json)?;
    let mut releases: Vec<Release> = listed
        .into_iter()
        .filter(|release| !release.draft)
        .filter_map(|release| {
            // The page opens in a browser, so it is this project's or a guess
            // from the tag, whatever the listing says.
            let page = if release.html_url.starts_with(PAGES) {
                release.html_url
            } else {
                format!("{PAGES}tag/{}", release.tag_name)
            };
            Some(Release {
                version: Version::parse(&release.tag_name)?,
                tag: release.tag_name,
                page,
                assets: release
                    .assets
                    .into_iter()
                    .map(|asset| Asset {
                        name: asset.name,
                        url: asset.browser_download_url,
                        size: asset.size,
                    })
                    .collect(),
                notes: release.body.as_deref().map(notes_of).unwrap_or_default(),
            })
        })
        .collect();
    // Keep the notes of the two releases an update can offer: the newest of
    // each kind.
    let newest = |releases: &[Release], nightly: bool| {
        releases
            .iter()
            .enumerate()
            .filter(|(_, release)| release.version.is_nightly() == nightly)
            .max_by(|(_, a), (_, b)| a.version.cmp(&b.version))
            .map(|(at, _)| at)
    };
    let keep = [newest(&releases, false), newest(&releases, true)];
    for (at, release) in releases.iter_mut().enumerate() {
        if !keep.contains(&Some(at)) {
            release.notes = String::new();
        }
    }
    Ok(releases)
}

/// The longest notes kept, in characters; the release's page has the rest.
const NOTES_LIMIT: usize = 4000;

/// A release's description as plain lines for About: without the install
/// instructions, the commit list's folded part, links and the footer, and
/// with a changelog line's `feat(scope): ` before and ` by @name in <sha>`
/// after it taken off. Headings of the release's own sections stay as lines.
pub fn notes_of(body: &str) -> String {
    let mut lines: Vec<String> = Vec::new();
    let mut install = false;
    let mut folded = false;
    // The last line pushed was a heading, which its list follows closely.
    let mut heading = false;
    for line in body.lines().map(str::trim) {
        if line.starts_with("All changes:") {
            break;
        }
        if line.starts_with("<details") {
            folded = true;
            continue;
        }
        if line.starts_with("</details") {
            folded = false;
            continue;
        }
        if folded {
            continue;
        }
        if line.starts_with('#') {
            let level = line.chars().take_while(|c| *c == '#').count();
            let title = line.trim_start_matches('#').trim();
            if level <= 2 {
                // The page names its own sections: "What's new" is its title.
                install = title.eq_ignore_ascii_case("install");
                continue;
            }
            if !install && !title.is_empty() {
                if lines.last().is_some_and(|last| !last.is_empty()) {
                    lines.push(String::new());
                }
                lines.push(title.to_owned());
                heading = true;
            }
            continue;
        }
        if install || line.starts_with("A nightly build of ") {
            continue;
        }
        let line = line.replace("**", "").replace('`', "");
        let line = match line.strip_prefix("- ").or_else(|| line.strip_prefix("* ")) {
            Some(item) => format!("\u{2022} {}", entry(item)),
            None => line,
        };
        if line.is_empty() && (heading || lines.last().is_none_or(String::is_empty)) {
            continue;
        }
        heading = false;
        lines.push(line);
    }
    while lines.last().is_some_and(String::is_empty) {
        lines.pop();
    }
    while lines.first().is_some_and(String::is_empty) {
        lines.remove(0);
    }
    let mut notes = lines.join("\n");
    if notes.chars().count() > NOTES_LIMIT {
        let end = notes
            .char_indices()
            .nth(NOTES_LIMIT)
            .map_or(notes.len(), |(at, _)| at);
        // Cut at a line's end, not in the middle of a sentence.
        notes.truncate(notes[..end].rfind('\n').unwrap_or(end));
    }
    notes
}

/// Whether `kind` is a Conventional Commits prefix such as `fix` or `feat(ui)!`.
fn is_commit_kind(kind: &str) -> bool {
    const TYPES: [&str; 11] = [
        "feat", "fix", "perf", "refactor", "docs", "test", "build", "ci", "chore", "style",
        "revert",
    ];
    let kind = kind.strip_suffix('!').unwrap_or(kind);
    let word = match kind.split_once('(') {
        Some((word, scope)) if scope.ends_with(')') && !scope.contains(' ') => word,
        Some(_) => return false,
        None => kind,
    };
    TYPES.contains(&word)
}

/// A changelog entry as the release's page writes it, such as
/// `fix(ui): keep a link underlined by @name in 0a1b2c3`, without the type,
/// the scope and the credit.
fn entry(item: &str) -> String {
    let mut text = item;
    if let Some((kind, rest)) = text.split_once(": ")
        && is_commit_kind(kind)
    {
        text = rest;
    }
    if let Some((before, credit)) = text.rsplit_once(" by ") {
        let mut words = credit.split(' ');
        let who = words.next().unwrap_or_default();
        let commit_ok = match (words.next(), words.next(), words.next()) {
            (None, ..) => true,
            (Some("in"), Some(sha), None) => {
                (7..=40).contains(&sha.len()) && sha.chars().all(|c| c.is_ascii_hexdigit())
            }
            _ => false,
        };
        if commit_ok && !who.is_empty() {
            text = before;
        }
    }
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// What a look at the releases found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// This build is the newest, or newer than any release.
    UpToDate,
    /// A newer release, which the person hasn't skipped.
    Available(Release),
    /// A newer release, which the person skipped.
    Skipped(Release),
    /// A build from source says nothing about releases.
    FromSource,
}

/// Compares `current` with `releases`. Nightlies count only where
/// `nightlies` is set. A build from source isn't compared: its version is the
/// release it grew from, so every later release would look like news.
pub fn decide(
    releases: &[Release],
    current: &Version,
    channel: Channel,
    nightlies: bool,
    skipped: Option<&str>,
) -> Outcome {
    if channel == Channel::Development {
        return Outcome::FromSource;
    }
    let newest = releases
        .iter()
        .filter(|release| nightlies || !release.version.is_nightly())
        .max_by(|a, b| a.version.cmp(&b.version))
        .filter(|release| release.version > *current);
    match newest {
        None => Outcome::UpToDate,
        Some(release) if skipped == Some(release.tag.as_str()) => Outcome::Skipped(release.clone()),
        Some(release) => Outcome::Available(release.clone()),
    }
}

/// How this copy of Yapfast was put on the computer, which decides who
/// updates it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// A Linux AppImage, which replaces itself with a newer file.
    AppImage,
    /// Windows' installer, whose newer installer replaces the program.
    Installer,
    /// A package manager owns the files, and updates them: its name.
    Manager(&'static str),
    /// A folder the person unpacked, or a build of their own.
    Other,
}

impl Source {
    /// The package manager that updates this copy, if one does.
    pub fn manager(self) -> Option<&'static str> {
        match self {
            Source::Manager(name) => Some(name),
            _ => None,
        }
    }
}

/// Where the running program came from: `exe` is its path, `appimage` the
/// `APPIMAGE` environment variable and `flatpak` whether `FLATPAK_ID` is set.
/// Paths are matched in lower case with `/` for `\`, so one list serves both
/// systems.
pub fn classify(exe: &str, appimage: bool, flatpak: bool) -> Source {
    if appimage {
        return Source::AppImage;
    }
    if flatpak {
        return Source::Manager("Flatpak");
    }
    let path = exe.replace('\\', "/").to_lowercase();
    let inside = |folder: &str| path.contains(folder);
    const MANAGERS: [(&str, &str); 9] = [
        ("/scoop/", "Scoop"),
        ("/chocolatey/", "Chocolatey"),
        ("/winget/packages/", "WinGet"),
        ("/windowsapps/", "the Microsoft Store"),
        ("/nix/store/", "Nix"),
        ("/snap/", "Snap"),
        ("/homebrew/", "Homebrew"),
        ("/cellar/", "Homebrew"),
        ("/linuxbrew/", "Homebrew"),
    ];
    if let Some((_, name)) = MANAGERS.iter().find(|(folder, _)| inside(folder)) {
        return Source::Manager(name);
    }
    // Distribution packages install under /usr, which a person doesn't
    // unpack into.
    if path.starts_with("/usr/") && !path.starts_with("/usr/local/") {
        return Source::Manager("your package manager");
    }
    if inside("/appdata/local/programs/") || inside("/program files") {
        return Source::Installer;
    }
    Source::Other
}

/// [`classify`] for the running program.
pub fn source() -> Source {
    let exe = std::env::current_exe().unwrap_or_default();
    classify(
        &exe.to_string_lossy(),
        std::env::var_os("APPIMAGE").is_some(),
        std::env::var_os("FLATPAK_ID").is_some(),
    )
}

/// The client every request here uses: the program's name and version, no
/// cookies, HTTPS only, and the same process-wide TLS provider the other
/// connections install. A download that stops for a minute is given up.
pub(crate) fn client() -> Result<reqwest::Client, String> {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    reqwest::Client::builder()
        .user_agent(concat!("Yapfast/", env!("CARGO_PKG_VERSION")))
        .https_only(true)
        .connect_timeout(std::time::Duration::from_secs(20))
        .read_timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|error| error.to_string())
}

/// Asks GitHub for the releases: one request, with no account or identifier
/// beyond the program's name and version.
pub async fn check() -> Result<Vec<Release>, String> {
    let response = client()?
        .get(RELEASES)
        .header("Accept", "application/vnd.github+json")
        .timeout(std::time::Duration::from_secs(20))
        .send()
        .await
        .and_then(|response| response.error_for_status())
        .map_err(|error| error.without_url().to_string())?;
    let body = response.bytes().await.map_err(|error| error.to_string())?;
    parse_releases(&body).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(text: &str) -> Version {
        Version::parse(text).unwrap()
    }

    fn release(tag: &str) -> Release {
        Release {
            version: version(tag),
            tag: tag.into(),
            page: format!("https://github.com/yapfast/yapfast/releases/tag/{tag}"),
            assets: Vec::new(),
            notes: String::new(),
        }
    }

    #[test]
    fn notes_keep_what_changed_and_drop_the_rest() {
        let body = "A nightly build of main at abc, for testing.\n\n\
            ## Changes since v0.4.0-nightly.1\n\n### Features\n\n\
            - feat(ui): fold long messages with Show more by @6gah in f86b16852652dd9cad668958d3e66b4dceb50c0c\n\
            - feat: add Developer Mode by mistycalpi in ae6749c58741d437eb35003c42510dd6bc8163ad\n\n\
            ### Fixes\n\n- fix(voice)!: keep the call: all of it by @a in 1234567\n\
            - Add navigation, calls and extras by @Fayberr in 1eb921958c4e5b3b2e87d9f2d4330dbaedc9fb0b\n\n\
            <details><summary>Other changes (1)</summary>\n\n- docs: hidden by @a in 1234567\n\n</details>\n\n\
            All changes: https://github.com/yapfast/yapfast/compare/a...b\n\nOn Arch Linux, install it.";
        assert_eq!(
            notes_of(body),
            "Features\n\u{2022} Fold long messages with Show more\n\u{2022} Add Developer Mode\n\n\
             Fixes\n\u{2022} Keep the call: all of it\n\u{2022} Add navigation, calls and extras"
        );
    }

    #[test]
    fn a_stable_releases_notes_skip_the_install_instructions() {
        let body = "Go Live on your **graphics card**.\n\n## Install\n\nRun `setup.exe`.\n\n\
            ### Linux\n\nDownload it.\n\n## What's new\n\nGo Live\n- Streams go out on `NVENC`.\n\
            - the soundboard: hear it\n";
        assert_eq!(
            notes_of(body),
            "Go Live on your graphics card.\n\nGo Live\n\u{2022} Streams go out on NVENC.\n\u{2022} The soundboard: hear it"
        );
        assert_eq!(notes_of(""), "");
        let long = format!("- {}\n", "word ".repeat(50)).repeat(40);
        assert!(notes_of(&long).chars().count() <= NOTES_LIMIT);
        assert!(!notes_of(&long).ends_with(' '));
    }

    #[test]
    fn only_the_releases_an_update_can_offer_keep_their_notes() {
        let json = br#"[
            {"tag_name": "v0.3.0", "body": "old"},
            {"tag_name": "v0.2.0", "body": "older"},
            {"tag_name": "v0.4.0-nightly.1", "body": "first nightly"},
            {"tag_name": "v0.4.0-nightly.2", "body": "second nightly"}
        ]"#;
        let notes: Vec<_> = parse_releases(json)
            .unwrap()
            .into_iter()
            .map(|release| release.notes)
            .collect();
        assert_eq!(notes, ["old", "", "", "second nightly"]);
    }

    #[test]
    fn versions_read_as_semver_says() {
        assert_eq!(version("v0.4.0"), version("0.4.0"));
        assert!(version("0.5.0-nightly.20261107.12").is_nightly());
        assert!(!version("0.5.0-rc.1").is_nightly());
        assert_eq!(version("1.2.3+abc"), version("1.2.3"));
        for bad in ["", "v", "1.2", "1.2.3.4", "a.b.c", "1.2.3-", "1.2.3-a..b"] {
            assert_eq!(Version::parse(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn versions_sort_by_precedence() {
        let sorted = [
            "0.4.0",
            "0.5.0-nightly.20261107.2",
            "0.5.0-nightly.20261107.12",
            "0.5.0-nightly.20261108.1",
            "0.5.0-rc.1",
            "0.5.0",
            "0.10.0",
        ];
        for pair in sorted.windows(2) {
            assert!(version(pair[0]) < version(pair[1]), "{pair:?}");
        }
        // A number in the nightly's run counter sorts as a number, not text.
        assert!(version("0.5.0-nightly.1.9") < version("0.5.0-nightly.1.10"));
    }

    #[test]
    fn a_listing_leaves_out_drafts_and_odd_tags() {
        let json = br#"[
            {"tag_name": "v0.4.0", "draft": false, "prerelease": false,
             "html_url": "https://github.com/yapfast/yapfast/releases/tag/v0.4.0",
             "assets": [{"name": "SHA256SUMS.txt", "browser_download_url": "https://example.test/sums", "size": 12}]},
            {"tag_name": "v0.5.0", "draft": true, "html_url": ""},
            {"tag_name": "preview", "draft": false, "html_url": ""},
            {"tag_name": "v0.5.0-nightly.20261107.12", "draft": false, "prerelease": true, "html_url": ""}
        ]"#;
        let releases = parse_releases(json).unwrap();
        let tags: Vec<_> = releases.iter().map(|r| r.tag.as_str()).collect();
        assert_eq!(tags, ["v0.4.0", "v0.5.0-nightly.20261107.12"]);
        assert_eq!(releases[0].assets[0].size, 12);
        assert_eq!(
            releases[0].page,
            "https://github.com/yapfast/yapfast/releases/tag/v0.4.0"
        );
        // A page elsewhere isn't opened.
        assert_eq!(
            releases[1].page,
            "https://github.com/yapfast/yapfast/releases/tag/v0.5.0-nightly.20261107.12"
        );
        assert!(parse_releases(b"{\"message\": \"rate limited\"}").is_err());
    }

    #[test]
    fn a_stable_build_sees_stable_releases_only_unless_it_asks_for_nightlies() {
        let releases = [
            release("v0.4.0"),
            release("v0.4.1"),
            release("v0.5.0-nightly.20261107.12"),
        ];
        let current = version("0.4.0");
        assert_eq!(
            decide(&releases, &current, Channel::Stable, false, None),
            Outcome::Available(release("v0.4.1"))
        );
        assert_eq!(
            decide(&releases, &current, Channel::Stable, true, None),
            Outcome::Available(release("v0.5.0-nightly.20261107.12"))
        );
        assert_eq!(
            decide(&releases, &version("0.4.1"), Channel::Stable, false, None),
            Outcome::UpToDate
        );
        assert_eq!(
            decide(&[], &current, Channel::Stable, true, None),
            Outcome::UpToDate
        );
    }

    #[test]
    fn a_nightly_moves_on_to_the_release_that_follows_it() {
        let releases = [
            release("v0.4.0"),
            release("v0.5.0"),
            release("v0.5.0-nightly.20261107.12"),
        ];
        let current = version("0.5.0-nightly.20261107.12");
        assert_eq!(
            decide(&releases, &current, Channel::Nightly, false, None),
            Outcome::Available(release("v0.5.0"))
        );
        // Newer than the newest release: nothing to offer.
        assert_eq!(
            decide(
                &releases,
                &version("0.6.0-nightly.20261201.1"),
                Channel::Nightly,
                true,
                None
            ),
            Outcome::UpToDate
        );
    }

    #[test]
    fn a_skipped_release_stays_quiet_until_a_newer_one_comes() {
        let releases = [release("v0.4.0"), release("v0.4.1")];
        let current = version("0.4.0");
        assert_eq!(
            decide(&releases, &current, Channel::Stable, false, Some("v0.4.1")),
            Outcome::Skipped(release("v0.4.1"))
        );
        let releases = [release("v0.4.0"), release("v0.4.1"), release("v0.4.2")];
        assert_eq!(
            decide(&releases, &current, Channel::Stable, false, Some("v0.4.1")),
            Outcome::Available(release("v0.4.2"))
        );
    }

    #[test]
    fn a_build_from_source_is_never_offered_an_update() {
        let releases = [release("v0.9.0")];
        assert_eq!(
            decide(
                &releases,
                &version("0.3.0"),
                Channel::Development,
                true,
                None
            ),
            Outcome::FromSource
        );
    }

    #[test]
    fn the_install_decides_who_updates() {
        let manager = |exe| classify(exe, false, false).manager();
        assert_eq!(
            classify("/tmp/.mount_Yap/usr/bin/yapfast", true, false),
            Source::AppImage
        );
        assert_eq!(
            classify("/app/bin/yapfast", false, true).manager(),
            Some("Flatpak")
        );
        assert_eq!(
            manager(r"C:\Users\Ann\scoop\apps\yapfast\current\yapfast.exe"),
            Some("Scoop")
        );
        assert_eq!(
            manager(r"C:\ProgramData\chocolatey\lib\yapfast\yapfast.exe"),
            Some("Chocolatey")
        );
        assert_eq!(manager("/nix/store/abc-yapfast/bin/yapfast"), Some("Nix"));
        assert_eq!(
            manager("/home/linuxbrew/.linuxbrew/Cellar/yapfast/1/bin/yapfast"),
            Some("Homebrew")
        );
        assert_eq!(manager("/usr/bin/yapfast"), Some("your package manager"));
        assert_eq!(manager("/usr/local/bin/yapfast"), None);
        assert_eq!(
            classify(
                r"C:\Users\Ann\AppData\Local\Programs\Yapfast\yapfast.exe",
                false,
                false
            ),
            Source::Installer
        );
        assert_eq!(
            classify("/home/ann/src/yapfast/target/release/yapfast", false, false),
            Source::Other
        );
        assert_eq!(
            classify(r"X:\Dev\yapfast\target\release\yapfast.exe", false, false),
            Source::Other
        );
    }
}
