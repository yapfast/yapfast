//! What a plugin's address means, and the index a repository may hold.
//!
//! A person installs a plugin by pasting the address they would give someone
//! else: the browser address of the plugin's repository, or of the folder inside
//! a repository that holds several plugins ([`Spec`]). This module turns that
//! text into the three things a fetch needs — repository, ref, folder — and
//! reads a repository's optional `yapfast-plugins.json`, which lists the folders
//! it holds ([`Index`]). Nothing here touches the disk or the network: the fetch
//! itself is the helper's, so what it needs to know is decided where it can be
//! tested without one.
//!
//! Normalizing is also what makes a plugin's identity. The lock file records
//! [`Spec::source`] and, for a repository that holds several plugins, the
//! folder, so two spellings of one repository have to land on one string: hence
//! `.git`, a trailing slash and a browser's `/tree/<ref>/` all come off, and
//! [`Spec::url`] writes back the address that names the same thing.
//!
//! Refused, each with a sentence saying what to paste instead: SSH, which needs
//! a key Yapfast doesn't hold; `http`, `git://` and `ext::`, which either can't
//! be trusted or run a program the URL names; and a folder that isn't one inside
//! the repository, since the install copies exactly that folder.

use serde_json::Value;

use super::API;

/// The one scheme a network address may use.
const HTTPS: &str = "https://";
/// The scheme that reads a repository from this disk, for development and for
/// the helper's tests.
const FILE: &str = "file://";

/// Where one plugin lives: the repository, which ref, and the folder in it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Spec {
    /// The repository's https URL, without `.git`, without a trailing slash and
    /// without the fragment: this is what the lock file records.
    pub source: String,
    /// The branch or tag named after `@` or in a tree URL; `None` for the
    /// default branch.
    pub reference: Option<String>,
    /// The folder inside the repository that holds `yapfast-plugin.json`, empty
    /// when it is at the root.
    pub path: String,
}

impl Spec {
    /// The address that names this plugin again, as the page shows it.
    pub fn url(&self) -> String {
        let mut url = String::with_capacity(self.source.len() + self.path.len() + 16);
        url.push_str(&self.source);
        if let Some(reference) = &self.reference {
            url.push('@');
            url.push_str(reference);
        }
        if !self.path.is_empty() {
            url.push_str("#path=");
            url.push_str(&self.path);
        }
        url
    }
}

/// Why an address gave no plugin, each said plainly.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// Nothing was typed, or only spaces.
    Empty,
    /// No `://`, so there is no address in it.
    NotUrl(String),
    /// A scheme other than `https` or `file`, as it is written: `http://`,
    /// `git://`, `ext::`.
    Scheme(String),
    /// An `ssh://` address, or git's `user@host:owner/repo` form.
    Ssh,
    /// A host, or a local directory, with no repository in it.
    NoRepository,
    /// A fragment that names no folder, holding what was written after `#`.
    Fragment(String),
    /// A folder that isn't one inside the repository, holding it as written.
    Path(String),
    /// A tree address and a `#path=` naming two different folders.
    PathConflict { tree: String, named: String },
    /// An `@` and a tree address naming two different refs.
    RefConflict { tree: String, named: String },
    /// An `@` with nothing after it.
    EmptyRef,
    /// An `@` that isn't at the end of the address, so it can't be a ref and
    /// can't be part of a repository's address either.
    StrayRef,
    /// An index asked for by an address that names a folder, though an index is
    /// at the repository's root.
    IndexSource,
    /// An index that isn't JSON, or not the JSON this reads, and why.
    IndexNotJson(String),
    /// An index for another API version; `None` when it names none.
    IndexApi(Option<u64>),
    /// An index that lists no plugin.
    IndexEmpty,
    /// An index that lists one folder twice.
    IndexDuplicate(String),
}

impl Error {
    /// What the page says, in plain words, as `page::FetchError::text` does.
    pub fn text(&self) -> String {
        match self {
            Error::Empty => "A plugin's address is the https:// address of its repository.".into(),
            Error::NotUrl(written) => format!(
                "`{written}` isn't a web address. Paste the https:// address of the plugin's \
                 repository."
            ),
            Error::Scheme(scheme) => {
                format!("`{scheme}` can't be installed. Use the repository's https:// address.")
            }
            Error::Ssh => {
                "SSH addresses can't be installed. Use the repository's https:// address.".into()
            }
            Error::NoRepository => {
                "That address names no repository. Paste the repository's own address.".into()
            }
            Error::Fragment(written) if written.starts_with("path=") => {
                "`#path=` names no folder. Write `#path=plugins/<name>`.".into()
            }
            Error::Fragment(written) => format!(
                "`#{written}` isn't part of a plugin's address. A folder is named with \
                 `#path=plugins/<name>`."
            ),
            Error::Path(written) if written.is_empty() => {
                "A folder inside a repository can't be empty.".into()
            }
            Error::Path(written) => format!(
                "`{written}` isn't a folder inside the repository. It can't be absolute and \
                 can't climb out with `..`."
            ),
            Error::PathConflict { tree, named } => format!(
                "That address names two folders, `{tree}` in its tree address and `{named}` \
                 after `#path=`."
            ),
            Error::RefConflict { tree, named } => format!(
                "That address names two refs, `{named}` after `@` and `{tree}` in its tree \
                 address."
            ),
            Error::EmptyRef => "`@` needs a branch, tag or commit after it.".into(),
            Error::StrayRef => "A ref is named after `@` at the end of the address, as in \
                 https://github.com/owner/repo@v1.4."
                .into(),
            Error::IndexSource => {
                "That address names a folder, but a repository's index is at its root.".into()
            }
            Error::IndexNotJson(why) => {
                format!("That repository's yapfast-plugins.json can't be read: {why}.")
            }
            Error::IndexApi(Some(api)) => format!(
                "That repository's yapfast-plugins.json is for plugin API {api}; this Yapfast \
                 reads API {API}."
            ),
            Error::IndexApi(None) => {
                "That repository's yapfast-plugins.json doesn't name its plugin API.".into()
            }
            Error::IndexEmpty => "That repository's yapfast-plugins.json lists no plugins.".into(),
            Error::IndexDuplicate(path) => {
                format!("That repository's yapfast-plugins.json lists `{path}` twice.")
            }
        }
    }
}

/// Reads what a person typed.
pub fn parse(url: &str) -> Result<Spec, Error> {
    let text = url.trim();
    if text.is_empty() {
        return Err(Error::Empty);
    }
    // `ext::` hands git a program to run, and has no `://` to read a scheme
    // from, so it's refused by name.
    if text.starts_with("ext::") {
        return Err(Error::Scheme("ext::".to_owned()));
    }
    let Some((scheme, after)) = text.split_once("://") else {
        // `git@host:owner/repo`, which git reads as SSH and Yapfast can't.
        return match (text.find('@'), text.find(':')) {
            (Some(at), Some(colon)) if at > 0 && at < colon => Err(Error::Ssh),
            _ => Err(Error::NotUrl(text.to_owned())),
        };
    };
    match scheme.to_ascii_lowercase().as_str() {
        "https" => https(after),
        "file" => local(after),
        "ssh" | "ssh+git" | "git+ssh" => Err(Error::Ssh),
        other => Err(Error::Scheme(format!("{other}://"))),
    }
}

/// A repository's `yapfast-plugins.json`: the folders that hold a plugin, in the
/// order the file lists them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Index {
    /// One [`Spec`] per folder, already naming its repository and ref, so the
    /// page can offer one and the helper can install it without joining
    /// strings again.
    pub plugins: Vec<Spec>,
}

/// Reads a repository's index; paths are checked like a `#path=`.
pub fn parse_index(source: &str, text: &str) -> Result<Index, Error> {
    let repository = parse(source)?;
    if !repository.path.is_empty() {
        return Err(Error::IndexSource);
    }
    let value: Value =
        serde_json::from_str(text).map_err(|why| Error::IndexNotJson(why.to_string()))?;
    let Value::Object(listed) = &value else {
        return Err(Error::IndexNotJson("it isn't a JSON object".to_owned()));
    };
    // The API version first: a file written for another Yapfast may mean
    // something else by everything below it.
    match listed.get("api").and_then(Value::as_u64) {
        Some(api) if api == u64::from(API) => {}
        other => return Err(Error::IndexApi(other)),
    }
    let entries = listed
        .get("plugins")
        .and_then(Value::as_array)
        .filter(|entries| !entries.is_empty())
        .ok_or(Error::IndexEmpty)?;
    let mut plugins: Vec<Spec> = Vec::with_capacity(entries.len());
    for entry in entries {
        let Some(written) = entry.as_str() else {
            return Err(Error::IndexNotJson(
                "a plugin it lists isn't a folder name".to_owned(),
            ));
        };
        let path = folder(written)?;
        // A folder listed twice is one plugin offered twice; an index holds
        // tens of folders, so a scan costs less than a set.
        if plugins.iter().any(|spec| spec.path == path) {
            return Err(Error::IndexDuplicate(path));
        }
        plugins.push(Spec {
            source: repository.source.clone(),
            reference: repository.reference.clone(),
            path,
        });
    }
    Ok(Index { plugins })
}

/// Reads `https://host/owner/repo`, with an optional `@ref`, a forge's tree
/// address, or a `#path=`. `after` is what follows `https://`.
fn https(after: &str) -> Result<Spec, Error> {
    let (body, named) = split_fragment(after)?;
    // A ref may hold `/`, so the `@` that counts is the one after the last `/`:
    // the end of the address, before the fragment.
    let (body, at_ref) = match body.rsplit_once('@') {
        Some((head, reference)) if !reference.contains('/') => (head, Some(reference)),
        _ => (body, None),
    };
    let Some((host, inside)) = body.split_once('/') else {
        return Err(Error::NoRepository);
    };
    if host.is_empty() || host.contains('@') {
        return Err(Error::NoRepository);
    }
    let mut segments: Vec<&str> = inside.split('/').filter(|part| !part.is_empty()).collect();

    // A browser address names a folder inside a repository: `/tree/<ref>/<path>`
    // on GitHub, `/-/tree/<ref>/<path>` on GitLab. They are what a person has on
    // screen when they copy the address, so they are what a person pastes.
    let mut reference = None;
    let mut path = String::new();
    if let Some((end, marker)) = tree_marker(&segments) {
        let forged = *segments.get(marker).ok_or(Error::EmptyRef)?;
        path = match segments.get(marker + 1..) {
            Some(inner) if !inner.is_empty() => folder(&inner.join("/"))?,
            _ => String::new(),
        };
        segments.truncate(end);
        if let Some(asked) = at_ref
            && asked != forged
        {
            return Err(Error::RefConflict {
                tree: forged.to_owned(),
                named: asked.to_owned(),
            });
        }
        reference = Some(forged.to_owned());
    } else if let Some(asked) = at_ref {
        if asked.is_empty() {
            return Err(Error::EmptyRef);
        }
        reference = Some(asked.to_owned());
    }
    // An `@` that isn't a ref stays in the repository's own path, where it
    // would name nothing git can fetch; say what an `@` is for instead.
    if segments.iter().any(|segment| segment.contains('@')) {
        return Err(Error::StrayRef);
    }
    if segments.len() < 2 {
        return Err(Error::NoRepository);
    }
    // A browser address ends in `.git` as often as not; git takes both, and the
    // lock file should hold one.
    let last = segments.len() - 1;
    let name = segments[last].trim_end_matches(".git");
    if name.is_empty() {
        return Err(Error::NoRepository);
    }
    segments[last] = name;

    if let Some(asked) = named {
        if path.is_empty() {
            path = asked;
        } else if path != asked {
            // The two ways of naming the folder must agree, or it isn't clear
            // which plugin is meant.
            return Err(Error::PathConflict {
                tree: path,
                named: asked,
            });
        }
    }

    let mut source = String::with_capacity(HTTPS.len() + host.len() + inside.len());
    source.push_str(HTTPS);
    source.push_str(host);
    for segment in &segments {
        source.push('/');
        source.push_str(segment);
    }
    Ok(Spec {
        source,
        reference,
        path,
    })
}

/// Reads `file:///a/repository`, for local development and for the tests.
fn local(after: &str) -> Result<Spec, Error> {
    let (body, path) = split_fragment(after)?;
    let source = body.trim_end_matches('/');
    if !source.starts_with('/') || source.trim_matches('/').is_empty() {
        return Err(Error::NoRepository);
    }
    // No `@ref` here: a directory's own name may hold an `@`, and a local
    // checkout is being tried as it is on this disk.
    Ok(Spec {
        source: format!("{FILE}{source}"),
        reference: None,
        path: path.unwrap_or_default(),
    })
}

/// The address without its `#path=` fragment, and the folder it names.
fn split_fragment(after: &str) -> Result<(&str, Option<String>), Error> {
    let Some((body, fragment)) = after.split_once('#') else {
        return Ok((after, None));
    };
    let Some(written) = fragment.strip_prefix("path=") else {
        // Only a folder may follow `#`; a second suffix would be ignored, so
        // it's said instead.
        return Err(Error::Fragment(fragment.to_owned()));
    };
    if written.is_empty() || fragment.contains('&') || fragment.contains('#') {
        return Err(Error::Fragment(fragment.to_owned()));
    }
    Ok((body, Some(folder(written)?)))
}

/// Where a tree address puts the repository's end, and the ref after the
/// `tree`.
fn tree_marker(segments: &[&str]) -> Option<(usize, usize)> {
    for (index, segment) in segments.iter().enumerate() {
        // A repository may be called `tree`; the marker is below the owner (and
        // any group, on GitLab).
        if index < 2 {
            continue;
        }
        if *segment == "tree" {
            return Some((index, index + 1));
        }
        if *segment == "-" && segments.get(index + 1) == Some(&"tree") {
            return Some((index, index + 2));
        }
    }
    None
}

/// Reads a folder inside a repository: relative, made of `/`-separated parts,
/// and never climbing out, with `%XX` escapes decoded first so one that spells
/// `..` is refused.
fn folder(written: &str) -> Result<String, Error> {
    if written.is_empty() {
        return Err(Error::Path(String::new()));
    }
    let mut folder = String::with_capacity(written.len());
    let decoded = decode(written).ok_or_else(|| Error::Path(written.to_owned()))?;
    // A backslash isn't a separator here, and on Windows it would be one: a
    // folder is written with `/`.
    if decoded.contains('\\') {
        return Err(Error::Path(written.to_owned()));
    }
    for segment in decoded.split('/') {
        if segment.is_empty() || segment == ".." {
            return Err(Error::Path(written.to_owned()));
        }
        // `.` names the folder it is in, so dropping it keeps two spellings of
        // one folder from disagreeing.
        if segment == "." {
            continue;
        }
        if !folder.is_empty() {
            folder.push('/');
        }
        folder.push_str(segment);
    }
    if folder.is_empty() {
        return Err(Error::Path(written.to_owned()));
    }
    Ok(folder)
}

/// Decodes `%XX`, returning `None` for a `%` without two hex digits after it or
/// for bytes that aren't UTF-8 once decoded.
fn decode(written: &str) -> Option<String> {
    if !written.contains('%') {
        return Some(written.to_owned());
    }
    let bytes = written.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let high = hex(*bytes.get(index + 1)?)?;
            let low = hex(*bytes.get(index + 2)?)?;
            decoded.push(high << 4 | low);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).ok()
}

fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(source: &str, reference: Option<&str>, path: &str) -> Spec {
        Spec {
            source: source.to_owned(),
            reference: reference.map(str::to_owned),
            path: path.to_owned(),
        }
    }

    const REPO: &str = "https://github.com/owner/repo";

    /// Every address a person is likely to have on screen.
    const ACCEPTED: [(&str, [&str; 3]); 16] = [
        // The plain address of a repository, however it was copied.
        (REPO, [REPO, "", ""]),
        ("https://github.com/owner/repo.git", [REPO, "", ""]),
        ("https://github.com/owner/repo/", [REPO, "", ""]),
        ("  https://github.com/owner/repo  ", [REPO, "", ""]),
        ("HTTPS://github.com/owner/repo", [REPO, "", ""]),
        // A ref, with a tree address's help where it holds a `/`.
        ("https://github.com/owner/repo@v1.4", [REPO, "v1.4", ""]),
        ("https://github.com/owner/repo.git@v1.4", [REPO, "v1.4", ""]),
        // A folder inside a repository, named the forge's way.
        (
            "https://github.com/owner/repo/tree/main/plugins/filter",
            [REPO, "main", "plugins/filter"],
        ),
        (
            "https://github.com/owner/repo/tree/main",
            [REPO, "main", ""],
        ),
        (
            "https://gitlab.com/group/sub/repo/-/tree/v2/plugins/filter",
            ["https://gitlab.com/group/sub/repo", "v2", "plugins/filter"],
        ),
        // The same folder, named with `#path=`.
        (
            "https://github.com/owner/repo#path=plugins/filter",
            [REPO, "", "plugins/filter"],
        ),
        (
            "https://github.com/owner/repo@v1.4#path=plugins/filter",
            [REPO, "v1.4", "plugins/filter"],
        ),
        // Both ways of naming one folder agree.
        (
            "https://github.com/owner/repo/tree/main/plugins/filter#path=plugins/filter",
            [REPO, "main", "plugins/filter"],
        ),
        // A repository called `tree` is a repository.
        (
            "https://github.com/owner/tree",
            ["https://github.com/owner/tree", "", ""],
        ),
        // A folder on this disk, for development and for the helper's tests.
        (
            "file:///home/you/plugin",
            ["file:///home/you/plugin", "", ""],
        ),
        (
            "file:///home/you/plugin/#path=plugins/foo",
            ["file:///home/you/plugin", "", "plugins/foo"],
        ),
    ];

    #[test]
    fn reads_every_address_a_person_pastes() {
        for (text, [source, reference, path]) in ACCEPTED {
            let expected = spec(source, Some(reference).filter(|r| !r.is_empty()), path);
            assert_eq!(parse(text), Ok(expected), "{text} should read");
        }
    }

    #[test]
    fn an_address_names_its_plugin_again() {
        for (text, _) in ACCEPTED {
            let read = parse(text).unwrap();
            assert_eq!(parse(&read.url()), Ok(read), "{text} should be named again");
        }
        // A decoded escape and a `.` are written back in one form.
        for (written, expected) in [
            (
                "https://github.com/owner/repo#path=plugins%2Ffoo",
                "https://github.com/owner/repo#path=plugins/foo",
            ),
            (
                "https://github.com/owner/repo#path=./plugins/foo",
                "https://github.com/owner/repo#path=plugins/foo",
            ),
            (
                "https://github.com/owner/repo#path=plugins/%66oo",
                "https://github.com/owner/repo#path=plugins/foo",
            ),
        ] {
            assert_eq!(parse(written).unwrap().url(), expected);
        }
    }

    #[test]
    fn refuses_addresses_that_are_not_a_repository_a_person_can_install() {
        for (text, refused) in [
            ("", Error::Empty),
            ("   ", Error::Empty),
            (
                "github.com/owner/repo",
                Error::NotUrl("github.com/owner/repo".to_owned()),
            ),
            ("git@github.com:owner/repo.git", Error::Ssh),
            ("ssh://git@github.com/owner/repo", Error::Ssh),
            ("git+ssh://git@github.com/owner/repo", Error::Ssh),
            (
                "http://github.com/owner/repo",
                Error::Scheme("http://".to_owned()),
            ),
            (
                "git://github.com/owner/repo",
                Error::Scheme("git://".to_owned()),
            ),
            ("ftp://example.com/repo", Error::Scheme("ftp://".to_owned())),
            ("ext::sh -c 'curl evil'", Error::Scheme("ext::".to_owned())),
            ("https://github.com", Error::NoRepository),
            ("https://github.com/", Error::NoRepository),
            ("https://github.com/owner/", Error::NoRepository),
            ("file://", Error::NoRepository),
            ("file:///", Error::NoRepository),
            (
                "https://github.com/owner/repo#ref=main",
                Error::Fragment("ref=main".to_owned()),
            ),
            (
                "https://github.com/owner/repo#path=",
                Error::Fragment("path=".to_owned()),
            ),
            (
                "https://github.com/owner/repo#path=a&ref=b",
                Error::Fragment("path=a&ref=b".to_owned()),
            ),
            ("https://github.com/owner/repo@", Error::EmptyRef),
            ("https://github.com/owner/repo@feat/x", Error::StrayRef),
            (
                "https://github.com/owner/repo/tree/main/plugins/filter#path=plugins/other",
                Error::PathConflict {
                    tree: "plugins/filter".to_owned(),
                    named: "plugins/other".to_owned(),
                },
            ),
            (
                "https://github.com/owner/repo/tree/main@v2",
                Error::RefConflict {
                    tree: "main".to_owned(),
                    named: "v2".to_owned(),
                },
            ),
        ] {
            assert_eq!(parse(text), Err(refused), "{text} should be refused");
        }
    }

    #[test]
    fn refuses_a_folder_that_is_not_inside_the_repository() {
        for text in [
            "https://github.com/owner/repo#path=..",
            "https://github.com/owner/repo#path=../up",
            "https://github.com/owner/repo#path=a/../../b",
            "https://github.com/owner/repo#path=%2e%2e/up",
            "https://github.com/owner/repo#path=a/%2E%2E/b",
            "https://github.com/owner/repo#path=/etc",
            "https://github.com/owner/repo#path=%2Fetc",
            "https://github.com/owner/repo#path=a\\b",
            "https://github.com/owner/repo#path=%5Cetc",
            "https://github.com/owner/repo#path=a//b",
            "https://github.com/owner/repo#path=a/",
            "https://github.com/owner/repo#path=.",
            "https://github.com/owner/repo#path=%FF",
            "https://github.com/owner/repo#path=%zz",
            "https://github.com/owner/repo/tree/main/../etc",
            "https://github.com/owner/repo/tree/main/%2e%2e",
        ] {
            let refused = parse(text).unwrap_err();
            assert!(
                matches!(refused, Error::Path(_) | Error::Fragment(_)),
                "{text} should be refused, said {refused:?}"
            );
        }
    }

    #[test]
    fn reads_an_index_into_installable_folders() {
        let index = parse_index(
            REPO,
            r#"{"api": 1, "plugins": ["plugins/word-filter", "plugins/quiet-hours"]}"#,
        )
        .unwrap();
        assert_eq!(
            index.plugins,
            [
                spec(REPO, None, "plugins/word-filter"),
                spec(REPO, None, "plugins/quiet-hours"),
            ]
        );
        // The order is the file's, and each folder names its plugin again.
        assert_eq!(
            index.plugins[1].url(),
            "https://github.com/owner/repo#path=plugins/quiet-hours"
        );
        // An index read at a ref offers its folders at that ref.
        let pinned = parse_index(
            &format!("{REPO}@v2"),
            r#"{"api": 1, "plugins": ["plugins/a"]}"#,
        )
        .unwrap();
        assert_eq!(pinned.plugins, [spec(REPO, Some("v2"), "plugins/a")]);
        // Anything else in the object is the repository's own business.
        assert_eq!(
            parse_index(
                REPO,
                r#"{"api": 1, "name": "ada's", "plugins": ["plugins/a"]}"#
            )
            .unwrap(),
            Index {
                plugins: vec![spec(REPO, None, "plugins/a")]
            }
        );
    }

    #[test]
    fn refuses_an_index_that_cannot_be_used() {
        let refused = |text: &str| parse_index(REPO, text).unwrap_err();
        assert!(matches!(refused("not json"), Error::IndexNotJson(_)));
        assert!(matches!(refused("[]"), Error::IndexNotJson(_)));
        assert!(matches!(
            refused(r#"{"api": 1, "plugins": [1]}"#),
            Error::IndexNotJson(_)
        ));
        assert_eq!(
            refused(r#"{"api": 2, "plugins": ["a"]}"#),
            Error::IndexApi(Some(2))
        );
        assert_eq!(refused(r#"{"plugins": ["a"]}"#), Error::IndexApi(None));
        assert_eq!(refused(r#"{"api": 1}"#), Error::IndexEmpty);
        assert_eq!(refused(r#"{"api": 1, "plugins": []}"#), Error::IndexEmpty);
        assert_eq!(refused(r#"{"api": 1, "plugins": "a"}"#), Error::IndexEmpty);
        assert_eq!(
            refused(r#"{"api": 1, "plugins": ["../elsewhere"]}"#),
            Error::Path("../elsewhere".to_owned())
        );
        assert_eq!(
            refused(r#"{"api": 1, "plugins": ["plugins/a", "plugins/a"]}"#),
            Error::IndexDuplicate("plugins/a".to_owned())
        );
        // Two spellings of one folder are one folder.
        assert_eq!(
            refused(r#"{"api": 1, "plugins": ["plugins/a", "./plugins/a"]}"#),
            Error::IndexDuplicate("plugins/a".to_owned())
        );
        // An index is at the repository's root, not in a folder.
        let error = parse_index(
            "https://github.com/owner/repo#path=plugins/a",
            r#"{"api": 1, "plugins": ["plugins/a"]}"#,
        )
        .unwrap_err();
        assert_eq!(error, Error::IndexSource);
    }

    #[test]
    fn every_refusal_is_a_sentence() {
        let refusals = [
            Error::Empty,
            Error::NotUrl("owner/repo".to_owned()),
            Error::Scheme("http://".to_owned()),
            Error::Ssh,
            Error::NoRepository,
            Error::Fragment("ref=main".to_owned()),
            Error::Fragment("path=".to_owned()),
            Error::Path(String::new()),
            Error::Path("../up".to_owned()),
            Error::PathConflict {
                tree: "a".to_owned(),
                named: "b".to_owned(),
            },
            Error::RefConflict {
                tree: "a".to_owned(),
                named: "b".to_owned(),
            },
            Error::EmptyRef,
            Error::StrayRef,
            Error::IndexSource,
            Error::IndexNotJson("expected value at line 1 column 1".to_owned()),
            Error::IndexApi(Some(2)),
            Error::IndexApi(None),
            Error::IndexEmpty,
            Error::IndexDuplicate("plugins/a".to_owned()),
        ];
        for refusal in refusals {
            let text = refusal.text();
            assert!(
                text.ends_with('.') && text.contains(' '),
                "{refusal:?} isn't a sentence: {text}"
            );
        }
        // The one message the page already says, said the same way.
        assert_eq!(
            Error::Ssh.text(),
            "SSH addresses can't be installed. Use the repository's https:// address."
        );
    }
}
