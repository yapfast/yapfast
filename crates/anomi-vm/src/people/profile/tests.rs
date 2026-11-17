use super::*;
use crate::icon::Icon;
use crate::kind::Doing;

#[test]
fn services_take_their_marks_and_others_a_link() {
    assert_eq!(connection_icon("steam"), Icon::Steam);
    assert_eq!(connection_icon("github"), Icon::Github);
    assert_eq!(connection_icon("battlenet"), Icon::Battledotnet);
    assert_eq!(connection_icon("twitter"), Icon::XLogo);
    assert_eq!(connection_icon("playstation-stg"), Icon::Playstation);
    assert_eq!(connection_icon("domain"), Icon::Globe);
    assert_eq!(connection_icon("myspace"), Icon::Link);
}

#[test]
fn a_thought_needs_words_or_an_emoji() {
    let mut card = Card::default();
    assert!(card.thought().is_none());
    card.thought = Some(Status::default());
    assert!(card.thought().is_none());
    card.thought = Some(Status {
        emoji: Expected::Waiting,
        doing: Doing::Nothing,
        ..Status::default()
    });
    assert!(card.thought().is_some());
}

#[test]
fn the_note_shows_as_typed_until_saved() {
    let mut card = Card {
        note: Some("old".into()),
        ..Card::default()
    };
    assert_eq!(card.note_shown(), "old");
    card.touch.note = Some("new".into());
    assert_eq!(card.note_shown(), "new");
}

#[test]
fn only_a_counting_card_on_screen_wants_the_clock() {
    let game = Activity {
        start: 10,
        ..Activity::default()
    };
    let card = Card {
        activities: vec![game],
        now: 20,
        ..Card::default()
    };
    assert!(card.wants_clock(Place::Popout));
    // The full profile's side has no activity cards.
    assert!(!card.wants_clock(Place::Side));
    let profile = Profile {
        card: card.clone(),
        panel: None,
    };
    assert!(profile.wants_clock(true));
    assert!(!profile.wants_clock(false));
    let docked = Profile {
        card: Card::default(),
        panel: Some(card),
    };
    assert!(docked.wants_clock(false));
}
