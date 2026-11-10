//! NEW tags: a small mark beside a setting that came with an update, until the
//! person has looked at the page it is on.
//!
//! [`NEW`] lists the tagged settings: each one's key, the Settings section it
//! is in and the release it first shipped in (`since`). A setting is new to a
//! person when it is newer than the version their layout counts as having met
//! (`first_version`) and they haven't left its section since. Leaving the
//! section (or closing Settings) saves the keys as seen in `layout.json`, so
//! the mark never comes back.
//!
//! A fresh install has met every setting that exists, so nothing is new to it
//! until an update brings one. A layout from before the tags existed has met
//! the release before them ([`BEFORE_TAGS`]), so everything tagged since shows.
//! Updating over several releases without opening Settings shows what each one
//! added.
//!
//! To tag a setting that ships later: add its key, section and release to
//! [`NEW`], and give its row `new-key: "<key>"` (a `SettingsSwitch` has it).
//! A test fails when the list and the `.slint` files disagree. Take an entry
//! off once its release is old; its key is then dropped from the layout.

use super::*;
use crate::update::Version;

struct Tag {
    key: &'static str,
    /// The `settings-section` number the setting is on; 9 is About.
    section: i32,
    /// The release that first has the setting.
    since: &'static str,
}

const NEW: [Tag; 2] = [
    Tag {
        key: "check-updates",
        section: 9,
        since: "0.4.0",
    },
    Tag {
        key: "update-nightlies",
        section: 9,
        since: "0.4.0",
    },
];

/// The release before NEW tags: what a layout that predates them has met.
const BEFORE_TAGS: &str = "0.3.0";

/// What `first_version` is for a layout that has none yet.
pub(super) fn first_version(fresh: bool) -> String {
    if !fresh {
        return BEFORE_TAGS.to_owned();
    }
    // A fresh install has met every tagged setting, also those of a release
    // that isn't out yet, which a development build runs ahead of.
    let running = crate::build_info::current();
    let mut first = running
        .version()
        .split('-')
        .next()
        .unwrap_or(BEFORE_TAGS)
        .to_owned();
    for tag in &NEW {
        if Version::parse(tag.since) > Version::parse(&first) {
            first = tag.since.to_owned();
        }
    }
    first
}

fn is_new(layout: &Layout, tag: &Tag) -> bool {
    let met = layout
        .first_version
        .as_deref()
        .and_then(Version::parse)
        .or_else(|| Version::parse(BEFORE_TAGS));
    Version::parse(tag.since) > met && !layout.seen_new.iter().any(|seen| seen == tag.key)
}

/// Saves a fresh install's layout, which holds the version it has met, as
/// nothing was written yet to say so.
pub(super) fn start(adapter: &Rc<RefCell<Adapter>>) {
    let adapter = adapter.borrow();
    if !adapter.root.join(LAYOUT).exists() {
        adapter.layout.write(&adapter.root);
    }
}

/// Answers the window's questions about what is new, and saves what a person
/// has seen when Settings leaves a section.
pub(super) fn bind(window: &YapfastWindow, adapter: &Rc<RefCell<Adapter>>) {
    let tags = window.global::<NewTags>();
    let state = adapter.clone();
    tags.on_is_new(move |key, _| {
        state.try_borrow().is_ok_and(|adapter| {
            NEW.iter()
                .any(|tag| tag.key == key.as_str() && is_new(&adapter.layout, tag))
        })
    });
    let state = adapter.clone();
    tags.on_section_new(move |section, _| {
        state.try_borrow().is_ok_and(|adapter| {
            NEW.iter()
                .any(|tag| tag.section == section && is_new(&adapter.layout, tag))
        })
    });
    let (state, weak) = (adapter.clone(), window.as_weak());
    tags.on_left(move |section| {
        let mut adapter = state.borrow_mut();
        let before = adapter.layout.seen_new.clone();
        let seen = &mut adapter.layout.seen_new;
        // Only listed keys stay, so the file doesn't collect old ones.
        seen.retain(|key| NEW.iter().any(|tag| tag.key == key));
        for tag in &NEW {
            if tag.section == section && !seen.iter().any(|seen| seen == tag.key) {
                seen.push(tag.key.to_owned());
            }
        }
        if *seen != before {
            adapter.layout.write(&adapter.root);
            if let Some(window) = weak.upgrade() {
                let tags = window.global::<NewTags>();
                tags.set_revision(tags.get_revision() + 1);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tag(since: &'static str) -> Tag {
        Tag {
            key: "check-updates",
            section: 9,
            since,
        }
    }

    #[test]
    fn a_fresh_install_has_met_every_tagged_setting() {
        let folder = tempfile::tempdir().unwrap();
        let layout = Layout::read(folder.path());
        assert!(NEW.iter().all(|tag| !is_new(&layout, tag)));
    }

    #[test]
    fn a_layout_from_before_the_tags_sees_every_tagged_setting_as_new() {
        let folder = tempfile::tempdir().unwrap();
        std::fs::write(folder.path().join(LAYOUT), b"{}").unwrap();
        let layout = Layout::read(folder.path());
        assert!(NEW.iter().all(|tag| is_new(&layout, tag)));
    }

    #[test]
    fn a_setting_is_new_only_when_it_came_after_the_version_met() {
        let mut layout = Layout {
            first_version: Some("0.4.0".into()),
            ..Layout::default()
        };
        assert!(!is_new(&layout, &tag("0.3.0")));
        assert!(!is_new(&layout, &tag("0.4.0")));
        assert!(is_new(&layout, &tag("0.4.1")));
        assert!(is_new(&layout, &tag("0.5.0")));
        layout.seen_new = vec!["check-updates".into()];
        assert!(!is_new(&layout, &tag("0.5.0")));
    }

    #[test]
    fn what_was_seen_stays_seen_after_a_restart() {
        let folder = tempfile::tempdir().unwrap();
        std::fs::write(folder.path().join(LAYOUT), b"{}").unwrap();
        let mut layout = Layout::read(folder.path());
        layout.seen_new = vec!["check-updates".into(), "gone-long-ago".into()];
        layout.write(folder.path());
        let layout = Layout::read(folder.path());
        assert!(!is_new(&layout, &NEW[0]));
        assert!(is_new(&layout, &NEW[1]));
    }

    /// Every key a row names is listed, and every listed key has a row, so a
    /// new setting can't be tagged on one side only.
    #[test]
    fn the_list_and_the_rows_name_the_same_settings() {
        fn collect(folder: &std::path::Path, found: &mut Vec<String>) {
            for entry in std::fs::read_dir(folder).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    collect(&path, found);
                } else if path
                    .extension()
                    .is_some_and(|extension| extension == "slint")
                {
                    let text = std::fs::read_to_string(&path).unwrap();
                    for part in text.split("new-key: \"").skip(1) {
                        found.push(part.split('"').next().unwrap().to_owned());
                    }
                }
            }
        }
        let mut found = Vec::new();
        collect(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("ui"),
            &mut found,
        );
        found.sort();
        let mut listed: Vec<String> = NEW.iter().map(|tag| tag.key.to_owned()).collect();
        listed.sort();
        assert_eq!(found, listed);
    }
}
