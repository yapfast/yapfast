//! NEW tags: a small mark beside a setting that came with an update, until
//! the person has looked at the page it is on, worked out once for both
//! clients.
//!
//! [`NEW`] lists the tagged settings: each one's key, the Settings page it
//! is on and the release it first shipped in (`since`). A setting is new
//! to a person when it is newer than the version their layout counts as
//! having met (`first_version` in `layout.json`) and they haven't left its
//! page since (`seen_new`). Leaving the page (or closing Settings) keeps
//! its keys as seen ([`leave`]), so the mark never comes back.
//!
//! A fresh install has met every setting that exists, so nothing is new
//! to it until an update brings one. A layout from before the tags existed
//! has met the release before them ([`BEFORE_TAGS`]), so everything tagged
//! since shows. Updating over several releases without opening Settings
//! shows what each one added.
//!
//! To tag a setting that ships later: add its key, page and release to
//! [`NEW`], and mark its row in the window (`self.fresh("<key>")` on its
//! `form::Switch`). Take an entry off once its release is old; its key is
//! then dropped from the layout.

/// A tagged setting.
pub struct Tag {
    pub key: &'static str,
    /// The Settings page it's on.
    pub page: Page,
    /// The release that first has the setting.
    pub since: &'static str,
}

/// The Settings pages a tagged setting can be on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    About,
}

pub const NEW: [Tag; 2] = [
    Tag {
        key: "check-updates",
        page: Page::About,
        since: "0.4.0",
    },
    Tag {
        key: "update-nightlies",
        page: Page::About,
        since: "0.4.0",
    },
];

/// The release before NEW tags: what a layout that predates them has met.
pub const BEFORE_TAGS: &str = "0.3.0";

/// A release's three numbers, from `0.4.0`, `v0.4.0` or `0.5.0-nightly.1`.
fn release(text: &str) -> Option<[u64; 3]> {
    let text = text.trim();
    let text = text.strip_prefix('v').unwrap_or(text);
    let core = text.split(['-', '+']).next()?;
    let mut numbers = core.split('.').map(|number| number.parse::<u64>().ok());
    let release = [numbers.next()??, numbers.next()??, numbers.next()??];
    numbers.next().is_none().then_some(release)
}

/// What `first_version` is for a layout that has none yet: for one that
/// was there before (`fresh` false) the release before the tags; for a
/// fresh install the running release, or the latest a tag names, as a
/// development build runs ahead of a release that isn't out yet.
pub fn first_version(fresh: bool, running: &str) -> String {
    if !fresh {
        return BEFORE_TAGS.to_owned();
    }
    let mut first = running
        .split('-')
        .next()
        .filter(|core| release(core).is_some())
        .unwrap_or(BEFORE_TAGS)
        .to_owned();
    for tag in &NEW {
        if release(tag.since) > release(&first) {
            tag.since.clone_into(&mut first);
        }
    }
    first
}

/// Whether `tag` is new to a layout that has met `first_version` and seen
/// the keys in `seen`.
pub fn is_new(first_version: Option<&str>, seen: &[String], tag: &Tag) -> bool {
    let met = first_version
        .and_then(release)
        .or_else(|| release(BEFORE_TAGS));
    release(tag.since) > met && !seen.iter().any(|seen| seen == tag.key)
}

/// Whether the setting `key` is new.
pub fn key_new(first_version: Option<&str>, seen: &[String], key: &str) -> bool {
    NEW.iter()
        .any(|tag| tag.key == key && is_new(first_version, seen, tag))
}

/// Whether anything on `page` is new.
pub fn page_new(first_version: Option<&str>, seen: &[String], page: Page) -> bool {
    NEW.iter()
        .any(|tag| tag.page == page && is_new(first_version, seen, tag))
}

/// Settings left `page`: its keys are seen, and keys no longer listed go.
/// Whether `seen` changed, which is then kept.
pub fn leave(seen: &mut Vec<String>, page: Page) -> bool {
    let before = seen.clone();
    seen.retain(|key| NEW.iter().any(|tag| tag.key == key));
    for tag in &NEW {
        if tag.page == page && !seen.iter().any(|seen| seen == tag.key) {
            seen.push(tag.key.to_owned());
        }
    }
    *seen != before
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tag(since: &'static str) -> Tag {
        Tag {
            key: "check-updates",
            page: Page::About,
            since,
        }
    }

    #[test]
    fn a_fresh_install_has_met_every_tagged_setting() {
        let first = first_version(true, "0.3.5");
        assert!(NEW.iter().all(|tag| !is_new(Some(&first), &[], tag)));
        let first = first_version(true, "0.9.0-nightly.20261107.12");
        assert_eq!(first, "0.9.0");
    }

    #[test]
    fn a_layout_from_before_the_tags_sees_every_tagged_setting_as_new() {
        let first = first_version(false, "0.9.0");
        assert!(NEW.iter().all(|tag| is_new(Some(&first), &[], tag)));
        assert!(NEW.iter().all(|tag| is_new(None, &[], tag)));
    }

    #[test]
    fn a_setting_is_new_only_when_it_came_after_the_version_met() {
        let met = Some("0.4.0");
        assert!(!is_new(met, &[], &tag("0.3.0")));
        assert!(!is_new(met, &[], &tag("0.4.0")));
        assert!(is_new(met, &[], &tag("0.4.1")));
        assert!(is_new(met, &[], &tag("0.5.0")));
        assert!(!is_new(met, &["check-updates".into()], &tag("0.5.0")));
    }

    #[test]
    fn leaving_a_page_sees_its_keys_and_drops_old_ones() {
        let mut seen = vec!["gone-long-ago".to_owned()];
        assert!(leave(&mut seen, Page::About));
        assert_eq!(seen, ["check-updates", "update-nightlies"]);
        assert!(!leave(&mut seen, Page::About), "nothing more to keep");
        assert!(!page_new(None, &seen, Page::About));
    }
}
