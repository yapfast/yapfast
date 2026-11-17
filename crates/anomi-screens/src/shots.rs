//! The screens to render headless from sample view-models, for reviewers to
//! hold beside the reference screenshots of the Slint app
//! (`~/.cache/anomi-iced/golden`). `anomi-shots` draws each to a PNG.
//!
//! Each area module has a `shots()` of its own, named after the golden
//! screenshot it should match where there is one; [`all`] collects them,
//! with the area crates' own lists. F01 keeps this list.

pub use anomi_vm::fixtures::{GOLDEN, SMALL, Shot};

pub fn all() -> Vec<Shot> {
    let mut shots: Vec<Shot> = [
        crate::account::shots(),
        crate::browse::shots(),
        crate::chrome::shots(),
        crate::header::shots(),
        crate::inbox::shots(),
        crate::new_dm::shots(),
        crate::places::event::shots(),
        crate::places::gift::shots(),
        crate::places::invite::shots(),
        crate::places::join::shots(),
        crate::places::leave::shots(),
        crate::places::leaving_link::shots(),
        crate::places::oauth::shots(),
        crate::rail::shots(),
        crate::shell::shots(),
        crate::sidebar::channels::shots(),
        crate::sidebar::dms::shots(),
        crate::sidebar::server_header::shots(),
        crate::sign_in::shots(),
        crate::switcher::shots(),
        anomi_chat::shots::all(),
        anomi_people::shots::all(),
        anomi_settings::shots::all(),
        anomi_voice::shots::all(),
    ]
    .into_iter()
    .flatten()
    .collect();
    shots.sort_by_key(|shot| (shot.area, shot.name));
    shots
}

#[cfg(test)]
mod tests {
    /// Two shots of one name would write one file, the later over the
    /// earlier, and one reviewer would never see theirs.
    #[test]
    fn no_two_shots_share_a_name() {
        let shots = super::all();
        let named_twice: Vec<String> = shots
            .windows(2)
            .filter(|pair| (pair[0].area, pair[0].name) == (pair[1].area, pair[1].name))
            .map(|pair| format!("{}/{}", pair[0].area, pair[0].name))
            .collect();
        assert!(named_twice.is_empty(), "shots named twice: {named_twice:?}");
    }
}
