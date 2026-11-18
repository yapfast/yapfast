//! Words and colours rows are made of: a server's initials, a thread's age,
//! an avatar's colour.

use std::sync::Arc;

use yapfast_core::application::AppState;
use yapfast_core::chat::{Picture, Pixels};

use crate::project::account::profile::dominant;
use crate::project::chat::threads::ago;
use crate::project::rows::initials;

/// The rest is `rows`' `initials_take_three_words_at_most`.
#[test]
fn a_server_without_a_picture_shows_its_initials_in_capitals() {
    assert_eq!(initials("the secret empire"), "TSE");
    assert_eq!(initials("   "), "?");
}

/// The rest is `forum`'s `ages_read_as_discords_list_of_threads_says_them`.
#[test]
fn a_threads_activity_reads_as_discords_list_of_threads_says_it() {
    let now = 100 * 86_400_000;
    let said = |ago_ms: u64| ago(now - ago_ms, now);
    assert_eq!(said(3 * 3_600_000), "3h ago");
    assert_eq!(said(30 * 86_400_000), "30d ago");
    assert_eq!(said(31 * 86_400_000), ">30d ago");
    assert_eq!(ago(0, 400 * 86_400_000), ">1y ago");
    assert_eq!(ago(now + 1_000, now), "0s ago", "a clock behind");
}

/// The colour itself is the core's (`dominant_colour`'s
/// `a_vivid_colour_on_grey_wins_in_either_order`); the Slint app kept it by
/// address, the core's picture store keeps the picture it's read from.
#[test]
fn an_avatars_colour_is_worked_out_once_it_has_arrived() {
    let mut state = AppState::default();
    assert_eq!(dominant(&state, "a.png"), None, "not here yet");
    let red = Picture {
        width: 4,
        height: 4,
        rgba: Pixels::new(4, 4, [200, 20, 20, 255].repeat(16)),
        ..Default::default()
    };
    state.pictures.insert("a.png".into(), Arc::new(red));
    let color = dominant(&state, "a.png").expect("worked out once it came");
    assert!(
        (color >> 16) & 0xff > 150 && (color >> 8) & 0xff < 90,
        "{color:06x}"
    );
    assert_eq!(dominant(&state, "b.png"), None);
}
