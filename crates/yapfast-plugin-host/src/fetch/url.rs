//! What a plugin's URL means: a repository, a ref, and a directory inside it.
//!
//! A person pastes the address they would give someone else: the browser
//! address of the plugin's repository, or of the folder inside it that holds the
//! manifest. Every form here is rewritten into the three things a fetch needs,
//! and the forms that would hand git a program to run — SSH needing a key,
//! `ext::` running a command — are refused with a sentence saying what to paste
//! instead, because the person holding the link is the one who can fix it.
//!
//! `file://` is accepted for local development and for the tests: it reaches no
//! server, and what it reads is the copy on this disk.

use std::path::{Component, Path};

use anyhow::{Context, bail};

/// The one scheme a network source may use.
const HTTPS: &str = "https://";
/// The scheme that reads a repository from this disk.
const FILE: &str = "file://";

/// A plugin's source, split into what a fetch needs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Source {
    /// The repository URL git is asked for, with no ref, path or fragment.
    pub repo: String,
    /// The branch, tag or commit to read; `None` means the remote's `HEAD`.
    pub reference: Option<String>,
    /// The directory inside the repository that holds `yapfast-plugin.json`.
    pub path: Option<String>,
    /// What the person typed, kept verbatim in the lock file.
    pub original: String,
}

impl Source {
    /// Reads a source from the text a person pasted.
    pub fn parse(text: &str) -> anyhow::Result<Source> {
        let original = text.trim();
        if original.is_empty() {
            bail!("a plugin's source is a git URL, as in https://github.com/owner/repo");
        }
        if original.starts_with("ext::") {
            bail!(
                "`ext::` tells git to run the program named in the URL, and Yapfast never runs a \
                 program a URL names; paste the https:// address of the repository instead"
            );
        }
        if looks_like_scp(original) {
            bail!(
                "`{original}` is an SSH address, which needs a key Yapfast doesn't hold; paste the \
                 repository's https:// address instead, as in https://github.com/owner/repo"
            );
        }
        match scheme(original).as_deref() {
            Some("https") => https_source(original),
            Some("file") => file_source(original),
            Some("http") => bail!(
                "plain http can be read and changed on the way to you; paste the https:// address \
                 of the same repository instead"
            ),
            Some("ssh" | "git+ssh" | "ssh+git") => bail!(
                "SSH needs a key Yapfast doesn't hold; paste the repository's https:// address \
                 instead, as in https://github.com/owner/repo"
            ),
            Some("git") => bail!(
                "the git:// protocol is unencrypted; paste the repository's https:// address instead"
            ),
            Some(other) => bail!(
                "`{other}://` isn't a scheme Yapfast fetches plugins over; paste the https:// \
                 address of the repository instead"
            ),
            None => bail!(
                "`{original}` isn't a URL; paste the address of the plugin's repository, as in \
                 https://github.com/owner/repo"
            ),
        }
    }

    /// The ref as the lock file records it: what was asked for, or `HEAD`.
    pub fn reference_name(&self) -> &str {
        self.reference.as_deref().unwrap_or("HEAD")
    }
}

/// Reads `https://host/owner/repo`, with an optional ref, path or forge
/// address.
fn https_source(text: &str) -> anyhow::Result<Source> {
    let (body, path, asked) = split_fragment(text)?;
    let rest = &body[HTTPS.len()..];
    let (host, inside) = match rest.split_once('/') {
        Some(pair) => pair,
        None => {
            bail!("`{text}` names a host, not a repository; paste the repository's own address")
        }
    };
    if host.is_empty() || host.contains('@') {
        bail!("`{host}` isn't a host to fetch from; a plugin's address has no user name in it");
    }
    if inside.trim_matches('/').is_empty() {
        bail!("`{text}` names a host, not a repository; paste the repository's own address");
    }

    let mut segments: Vec<&str> = inside.split('/').filter(|part| !part.is_empty()).collect();
    // The browser addresses of the two forges name a folder inside a
    // repository: `/tree/<ref>/<path>` on GitHub, `/-/tree/<ref>/<path>` on
    // GitLab. They're what a person has on screen when they copy the address,
    // so they're what a person pastes.
    let mut posed = None;
    if let Some((repo_end, ref_index)) = forge_marker(&segments) {
        let reference = segments
            .get(ref_index)
            .copied()
            .map(clean_reference)
            .transpose()?
            .context("`/tree/` needs the branch or tag after it")?;
        let inner = segments.get(ref_index + 1..).unwrap_or_default().join("/");
        segments.truncate(repo_end);
        posed = Some((reference, inner));
    } else if let Some((head, reference)) = inside.rsplit_once('@') {
        // `owner/repo@ref`: the ref is not part of the path, so whatever follows
        // the last `@` is the branch, tag or commit.
        posed = Some((clean_reference(reference)?, String::new()));
        segments = head
            .split('/')
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>();
    }

    let (reference, sub_path) = match (asked, posed) {
        (Some(asked), Some((posed, _))) => bail!(
            "`{text}` names the ref twice, `{asked}` and `{posed}`; say it once so it's clear \
             which one is meant"
        ),
        (Some(asked), None) => (Some(asked), None),
        (None, Some((posed, inner))) => {
            let sub_path = if inner.is_empty() {
                None
            } else {
                Some(clean_path(&inner)?)
            };
            (Some(posed), sub_path)
        }
        (None, None) => (None, None),
    };

    if segments.iter().any(|segment| segment.contains('@')) {
        bail!(
            "`{text}` names the ref twice, once after `@` and once in the `/tree/` address; say \
             it once so it's clear which one is meant"
        );
    }
    if segments.len() < 2 {
        bail!("`{text}` names a host, not a repository; paste the repository's own address");
    }
    // A browser address ends in `.git` as often as not; git takes both.
    if let Some(last) = segments.last_mut() {
        *last = last.strip_suffix(".git").unwrap_or(last);
        if last.is_empty() {
            bail!("`{text}` names no repository");
        }
    }
    let repo = format!("{HTTPS}{host}/{}", segments.join("/"));
    Ok(Source {
        repo,
        reference,
        // A `#path=` and a `/tree/` address are two ways of saying one thing.
        path: match (path, sub_path) {
            (Some(asked), Some(posed)) => bail!(
                "`{text}` names the directory inside the repository twice, `{asked}` and `{posed}`"
            ),
            (Some(path), None) | (None, Some(path)) => Some(path),
            (None, None) => None,
        },
        original: text.to_owned(),
    })
}

/// Reads `file:///a/repository`, for local development and for the tests.
fn file_source(text: &str) -> anyhow::Result<Source> {
    let (body, path, reference) = split_fragment(text)?;
    let rest = &body[FILE.len()..];
    if rest.trim_matches('/').is_empty() {
        bail!("`file://` needs a directory after it, as in file:///home/you/plugin");
    }
    // A local address can't carry `@ref`: a directory name may hold an `@`, so
    // the ref has to come from `#ref=`.
    Ok(Source {
        repo: body.trim_end_matches('/').to_owned(),
        reference,
        path,
        original: text.to_owned(),
    })
}

/// The URL without its `#path=`/`#ref=` fragment.
fn split_fragment(text: &str) -> anyhow::Result<(&str, Option<String>, Option<String>)> {
    let Some((body, fragment)) = text.split_once('#') else {
        return Ok((text, None, None));
    };
    let mut path = None;
    let mut reference = None;
    for pair in fragment.split('&') {
        let Some((key, value)) = pair.split_once('=') else {
            bail!("`#{pair}` isn't a plugin source suffix; write `#path=plugins/foo`");
        };
        match key {
            "path" => path = Some(clean_path(value)?),
            "ref" => reference = Some(clean_reference(value)?),
            other => bail!("`#{other}=` isn't a plugin source suffix; use `#path=` or `#ref=`"),
        }
    }
    Ok((body, path, reference))
}

/// Where a `/tree/<ref>` address puts the repository's end and the ref.
fn forge_marker(segments: &[&str]) -> Option<(usize, usize)> {
    for (index, segment) in segments.iter().enumerate() {
        // The owner (and any group, on GitLab) comes before this, so a
        // repository called `tree` is only a marker below that.
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

/// The scheme of a URL in lowercase, if it has one.
fn scheme(text: &str) -> Option<String> {
    let at = text.find("://")?;
    Some(text[..at].to_ascii_lowercase())
}

/// Whether an address is the `user@host:path` form git reads as SSH.
fn looks_like_scp(text: &str) -> bool {
    if text.contains("://") {
        return false;
    }
    match (text.find('@'), text.find(':')) {
        (Some(at), Some(colon)) => at < colon && at > 0,
        _ => false,
    }
}

/// Checks a ref name and returns it, refusing what git would refuse and what
/// would be read as an option.
fn clean_reference(text: &str) -> anyhow::Result<String> {
    if text.is_empty() {
        bail!("the ref is empty; name a branch, tag or commit");
    }
    if text.starts_with('-')
        || text.contains("..")
        || text
            .chars()
            .any(|c| c.is_whitespace() || "~^:?*[\\".contains(c))
    {
        bail!("`{text}` isn't a branch, tag or commit that git accepts");
    }
    Ok(text.to_owned())
}

/// Checks a directory inside a repository and returns it with `/` separators,
/// refusing one that climbs out.
fn clean_path(text: &str) -> anyhow::Result<String> {
    let mut parts: Vec<String> = Vec::new();
    for component in Path::new(text).components() {
        match component {
            Component::Normal(part) => parts.push(part.to_string_lossy().into_owned()),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                bail!("`{text}` isn't a directory inside the repository: it leaves it")
            }
        }
    }
    if parts.is_empty() {
        bail!("`{text}` names no directory");
    }
    Ok(parts.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_repository_address() {
        let source = Source::parse("https://github.com/owner/repo").unwrap();
        assert_eq!(source.repo, "https://github.com/owner/repo");
        assert_eq!(source.reference, None);
        assert_eq!(source.path, None);
        assert_eq!(source.reference_name(), "HEAD");
    }

    #[test]
    fn reads_a_git_suffix_a_ref_and_a_path() {
        let source =
            Source::parse("https://github.com/owner/repo.git@v1.4#path=plugins/filter").unwrap();
        assert_eq!(source.repo, "https://github.com/owner/repo");
        assert_eq!(source.reference.as_deref(), Some("v1.4"));
        assert_eq!(source.path.as_deref(), Some("plugins/filter"));
    }

    #[test]
    fn reads_a_github_folder_address() {
        let source =
            Source::parse("https://github.com/owner/repo/tree/main/plugins/filter").unwrap();
        assert_eq!(source.repo, "https://github.com/owner/repo");
        assert_eq!(source.reference.as_deref(), Some("main"));
        assert_eq!(source.path.as_deref(), Some("plugins/filter"));
    }

    #[test]
    fn reads_a_gitlab_folder_address() {
        let source =
            Source::parse("https://gitlab.com/group/sub/repo/-/tree/v2/plugins/filter").unwrap();
        assert_eq!(source.repo, "https://gitlab.com/group/sub/repo");
        assert_eq!(source.reference.as_deref(), Some("v2"));
        assert_eq!(source.path.as_deref(), Some("plugins/filter"));
    }

    #[test]
    fn a_folder_address_without_a_path_reads_the_repository_root() {
        let source = Source::parse("https://github.com/owner/repo/tree/main").unwrap();
        assert_eq!(source.repo, "https://github.com/owner/repo");
        assert_eq!(source.reference.as_deref(), Some("main"));
        assert_eq!(source.path, None);
    }

    #[test]
    fn a_repository_named_tree_is_a_repository() {
        let source = Source::parse("https://github.com/owner/tree").unwrap();
        assert_eq!(source.repo, "https://github.com/owner/tree");
        assert_eq!(source.reference, None);
    }

    #[test]
    fn a_local_address_carries_its_ref_and_path_in_the_fragment() {
        let source = Source::parse("file:///home/you/plugin#ref=v1&path=plugins/filter").unwrap();
        assert_eq!(source.repo, "file:///home/you/plugin");
        assert_eq!(source.reference.as_deref(), Some("v1"));
        assert_eq!(source.path.as_deref(), Some("plugins/filter"));
    }

    #[test]
    fn a_trailing_slash_is_not_a_directory() {
        let source = Source::parse("file:///home/you/plugin/").unwrap();
        assert_eq!(source.repo, "file:///home/you/plugin");
        assert_eq!(source.path, None);
    }

    #[test]
    fn refuses_what_would_need_a_key_or_run_a_program() {
        for (text, expected) in [
            ("git@github.com:owner/repo.git", "SSH address"),
            ("ssh://git@github.com/owner/repo", "SSH needs a key"),
            ("http://github.com/owner/repo", "plain http"),
            ("git://github.com/owner/repo", "unencrypted"),
            ("ext::sh -c 'curl evil'", "never runs a program"),
            ("ftp://example.com/repo", "isn't a scheme"),
            ("owner/repo", "isn't a URL"),
        ] {
            let error = Source::parse(text).unwrap_err().to_string();
            assert!(
                error.contains(expected),
                "{text} should mention {expected}, said: {error}"
            );
        }
    }

    #[test]
    fn refuses_a_path_that_climbs_out() {
        for text in [
            "https://github.com/owner/repo#path=../up",
            "https://github.com/owner/repo#path=/etc",
            "file:///home/you/plugin#path=a/../../b",
        ] {
            assert!(Source::parse(text).is_err(), "{text} should be refused");
        }
    }

    #[test]
    fn refuses_saying_the_same_thing_twice() {
        for text in [
            "https://github.com/owner/repo@main/tree/dev",
            "https://github.com/owner/repo@main#ref=dev",
            "https://github.com/owner/repo/tree/main/x#path=y",
        ] {
            assert!(Source::parse(text).is_err(), "{text} should be refused");
        }
    }

    #[test]
    fn refuses_a_ref_git_would_read_as_an_option() {
        for text in [
            "https://github.com/owner/repo@-x",
            "https://github.com/owner/repo@a b",
            "https://github.com/owner/repo@a..b",
        ] {
            assert!(Source::parse(text).is_err(), "{text} should be refused");
        }
    }
}
