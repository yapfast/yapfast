//! Which streams' previews are in sight: the tiles of others' streams this
//! account doesn't watch, which show their preview behind "Watch stream",
//! where the grid shows them uncovered with the window in front. The core
//! asks for those again as they fall due (`crate::intent::voice::previews`);
//! none is in sight behind another window, in the tray, or while the call
//! is off screen, where the corner shows only a stream watched.

use yapfast_core::voice;

use super::focus::Stage;
use super::lives::Sight;

/// Notes in `seen` whose previews are in sight, in the tiles' order; it
/// stays as it was, with no copy, while they're the same.
pub fn note(voice: &voice::State, stage: Stage<'_>, sight: &Sight, seen: &mut Vec<String>) {
    let watched = voice
        .watching
        .as_ref()
        .map(|watching| watching.user.as_str());
    let shown = (stage.participants.iter().enumerate())
        .filter(|(_, person)| person.streaming && !person.local)
        .filter(|(_, person)| Some(person.id.as_str()) != watched)
        .filter(|&(index, _)| {
            let slot = stage.stream_slot(index);
            slot.and_then(|slot| sight.in_grid(slot)).is_some()
        })
        .map(|(_, person)| person.id.as_str());
    if !shown.clone().eq(seen.iter().map(String::as_str)) {
        *seen = shown.map(str::to_owned).collect();
    }
}
