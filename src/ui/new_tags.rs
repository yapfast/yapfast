//! NEW tags: a small mark beside a setting that came with an update, until the
//! person has looked at the page it is on.
//!
//! [`NEW`] lists the tagged settings and the Settings section each is in. A
//! tag shows until Settings leaves that section (or closes), which saves the
//! setting as seen in `layout.json`, so it never comes back. A fresh install
//! has seen everything, as nothing is new to it; a layout from before the tags
//! existed has seen nothing, so what came since shows.
//!
//! To tag a setting that ships later: add its key and section to [`NEW`], a
//! property to `NewTags` (ui/yapfast/new-tags.slint) set in [`show`], and
//! `fresh:` on its row. Take a tag off the list once the release it came in is
//! old; a key that isn't listed is dropped from the layout.

use super::*;

/// The key of each tagged setting, and the Settings section it is on. The
/// sections are `settings-section`'s numbers; 9 is About.
const NEW: [(&str, i32); 2] = [("check-updates", 9), ("update-nightlies", 9)];

/// Every key, as a fresh install has seen them.
pub(super) fn all() -> Vec<String> {
    NEW.iter().map(|(key, _)| (*key).to_owned()).collect()
}

fn unseen(layout: &Layout, key: &str) -> bool {
    layout
        .seen_new
        .as_ref()
        .is_some_and(|seen| !seen.iter().any(|seen| seen == key))
}

/// Puts which settings are new in the window.
pub(super) fn show(window: &YapfastWindow, layout: &Layout) {
    let tags = window.global::<NewTags>();
    tags.set_check(unseen(layout, "check-updates"));
    tags.set_nightlies(unseen(layout, "update-nightlies"));
    tags.set_about(
        NEW.iter()
            .any(|(key, section)| *section == 9 && unseen(layout, key)),
    );
}

/// Saves a fresh install's layout, which holds that it has seen every tag, as
/// nothing was written yet to say so.
pub(super) fn start(adapter: &Rc<RefCell<Adapter>>, window: &YapfastWindow) {
    {
        let adapter = adapter.borrow();
        show(window, &adapter.layout);
        if !adapter.root.join(LAYOUT).exists() {
            adapter.layout.write(&adapter.root);
        }
    }
    let (state, weak) = (adapter.clone(), window.as_weak());
    window.global::<NewTags>().on_left(move |section| {
        let mut adapter = state.borrow_mut();
        let seen = adapter.layout.seen_new.get_or_insert_with(Vec::new);
        let before = seen.len();
        // Only listed keys stay, so the file doesn't collect old ones.
        seen.retain(|key| NEW.iter().any(|(listed, _)| listed == key));
        for (key, in_section) in NEW {
            if in_section == section && !seen.iter().any(|seen| seen == key) {
                seen.push(key.to_owned());
            }
        }
        if seen.len() != before {
            adapter.layout.write(&adapter.root);
        }
        if let Some(window) = weak.upgrade() {
            show(&window, &adapter.layout);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_install_has_seen_every_tag() {
        let folder = tempfile::tempdir().unwrap();
        let layout = Layout::read(folder.path());
        assert!(!unseen(&layout, "check-updates"));
        assert!(!unseen(&layout, "update-nightlies"));
    }

    #[test]
    fn a_layout_from_before_the_tags_has_seen_none() {
        let folder = tempfile::tempdir().unwrap();
        std::fs::write(folder.path().join(LAYOUT), b"{}").unwrap();
        let layout = Layout::read(folder.path());
        assert!(unseen(&layout, "check-updates"));
        assert!(unseen(&layout, "update-nightlies"));
    }

    #[test]
    fn what_was_seen_stays_seen_after_a_restart() {
        let folder = tempfile::tempdir().unwrap();
        std::fs::write(folder.path().join(LAYOUT), b"{}").unwrap();
        let mut layout = Layout::read(folder.path());
        layout.seen_new = Some(vec!["check-updates".into(), "gone-long-ago".into()]);
        layout.write(folder.path());
        let layout = Layout::read(folder.path());
        assert!(!unseen(&layout, "check-updates"));
        assert!(unseen(&layout, "update-nightlies"));
    }
}
