//! The stream's menu: its submenus open under the pointer and the keys,
//! which pass over the groups' titles and skip what can't be chosen.

use super::*;
use crate::voice::share::{Resolution, Sound};

/// The menu as `anomi_voice::share::built` makes it, in the code's English.
fn menu(changeable: bool) -> Live {
    let quality = vec![
        Row::heading("FRAME RATE"),
        Row::new("30 fps", Intent::Fps(30)).radio(true),
        Row::new("60 fps", Intent::Fps(60)).radio(false),
        Row::heading("RESOLUTION").separated(),
        Row::new("720p", Intent::Resolution(Resolution::P720)).radio(false),
    ];
    let sound = vec![
        Row::new("All applications", Intent::Sound(Sound::All)).radio(true),
        Row::new("No sound", Intent::Sound(Sound::None)).radio(false),
    ];
    let hover = |part| Intent::Hover(Some(part));
    Live {
        quality: Row::new("Stream Quality", hover(Part::Quality)).submenu(quality),
        sound: Row::new("Sound", hover(Part::Sound)).submenu(sound),
        change: Row::new("Change Windows", Intent::Change).disabled(!changeable),
        stop: Row::new("Stop Streaming", Intent::Stop).danger(),
        ..Live::new(changeable)
    }
}

fn press(live: &mut Live, key: Key) -> Option<Intent> {
    live.apply(&Intent::Key(key))
}

#[test]
fn the_pointer_opens_a_rows_submenu_and_another_row_closes_it() {
    let mut live = menu(true);
    live.apply(&Intent::Hover(Some(Part::Quality)));
    assert_eq!(live.open, Some(Part::Quality));
    assert!(live.lit(Part::Quality));
    live.apply(&Intent::Hover(Some(Part::Change)));
    assert_eq!((live.open, live.current), (None, Some(Part::Change)));
}

#[test]
fn the_keys_go_into_a_submenu_past_its_titles_and_choose() {
    let mut live = menu(true);
    assert_eq!(press(&mut live, Key::Down), None);
    assert_eq!(live.current, Some(Part::Quality));
    press(&mut live, Key::Right);
    // Its first row is "FRAME RATE", a title: the keys start under it.
    assert_eq!((live.open, live.sub), (Some(Part::Quality), Some(1)));
    press(&mut live, Key::Down);
    press(&mut live, Key::Down);
    assert_eq!(live.sub, Some(4), "past RESOLUTION");
    press(&mut live, Key::Down);
    assert_eq!(live.sub, Some(1), "round past FRAME RATE");
    press(&mut live, Key::Up);
    assert_eq!(live.sub, Some(4));
    assert_eq!(
        press(&mut live, Key::Enter),
        Some(Intent::Resolution(Resolution::P720))
    );
    press(&mut live, Key::Escape);
    assert_eq!((live.open, live.sub), (None, None));
    assert_eq!(live.current, Some(Part::Quality));
}

#[test]
fn enter_on_the_last_rows_chooses_them() {
    let mut live = menu(true);
    live.current = Some(Part::Sound);
    press(&mut live, Key::Down);
    assert_eq!(press(&mut live, Key::Enter), Some(Intent::Change));
    press(&mut live, Key::Down);
    assert_eq!(press(&mut live, Key::Enter), Some(Intent::Stop));
}

#[test]
fn what_is_shared_is_not_changed_before_the_stream_is_live() {
    let mut live = menu(false);
    live.current = Some(Part::Sound);
    press(&mut live, Key::Down);
    assert_eq!(live.current, Some(Part::Stop), "past Change Windows");
    live.current = Some(Part::Change);
    assert_eq!(press(&mut live, Key::Enter), None);
}

#[test]
fn made_again_the_keys_stay_where_the_rows_still_are() {
    let mut live = menu(true);
    live.open = Some(Part::Sound);
    live.sub = Some(5);
    live.current = Some(Part::Change);
    live.change.disabled = true;
    live.settle();
    assert_eq!((live.sub, live.current), (None, None));
}
