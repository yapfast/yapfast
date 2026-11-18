//! NEW tags: a small mark beside a setting that came with an update, until the
//! person has looked at the page it is on. Which settings are tagged and
//! when one is new is the core's (`yapfast_core::new_tags`), which the iced
//! client asks too; this answers the window's questions from the layout,
//! and keeps what was seen in `layout.json` as Settings leaves a section.
//!
//! To tag a setting that ships later: add it to the core's `NEW`, and give
//! its row `new-key: "<key>"` (a `SettingsSwitch` has it). A test fails
//! when the list and the `.slint` files disagree.

use yapfast_core::new_tags::{self, NEW, Page};

use super::*;

/// The `settings-section` number of a page that has tagged settings.
fn section(page: Page) -> i32 {
    match page {
        Page::About => 9,
    }
}

/// What `first_version` is for a layout that has none yet.
pub(super) fn first_version(fresh: bool) -> String {
    new_tags::first_version(fresh, crate::build_info::current().version())
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
            let layout = &adapter.layout;
            new_tags::key_new(layout.first_version.as_deref(), &layout.seen_new, &key)
        })
    });
    let state = adapter.clone();
    tags.on_section_new(move |number, _| {
        state.try_borrow().is_ok_and(|adapter| {
            let layout = &adapter.layout;
            NEW.iter().any(|tag| {
                section(tag.page) == number
                    && new_tags::is_new(layout.first_version.as_deref(), &layout.seen_new, tag)
            })
        })
    });
    let (state, weak) = (adapter.clone(), window.as_weak());
    tags.on_left(move |number| {
        let mut adapter = state.borrow_mut();
        let Some(page) = NEW
            .iter()
            .map(|tag| tag.page)
            .find(|page| section(*page) == number)
        else {
            return;
        };
        if new_tags::leave(&mut adapter.layout.seen_new, page) {
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

    #[test]
    fn a_fresh_install_has_met_every_tagged_setting() {
        let folder = tempfile::tempdir().unwrap();
        let layout = Layout::read(folder.path());
        assert!(NEW.iter().all(|tag| !new_tags::is_new(
            layout.first_version.as_deref(),
            &layout.seen_new,
            tag
        )));
    }

    #[test]
    fn what_was_seen_stays_seen_after_a_restart() {
        let folder = tempfile::tempdir().unwrap();
        std::fs::write(folder.path().join(LAYOUT), b"{}").unwrap();
        let mut layout = Layout::read(folder.path());
        layout.seen_new = vec!["check-updates".into(), "gone-long-ago".into()];
        layout.write(folder.path());
        let layout = Layout::read(folder.path());
        let new = |tag| new_tags::is_new(layout.first_version.as_deref(), &layout.seen_new, tag);
        assert!(!new(&NEW[0]));
        assert!(new(&NEW[1]));
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
