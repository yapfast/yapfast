//! The activity cards: task P06's tests, driving
//! `anomi_people::activity::view` and `card::view` over its fixtures in the
//! simulator (`super::support`). The panel is laid out as the member list's
//! top: 8 px of padding, then a 32 px header (centre y 24), and cards 68 px
//! tall 76 apart from y 40 (the first's centre at 74).

use iced::{Event, Point, Size, mouse};
use iced_test::Simulator;

use anomi_people::activity::{card, view};
use anomi_theme::{Element, Theme};
use anomi_vm::fixtures::people::activity as samples;
use anomi_vm::people::activity::card::{Card, Intent as CardIntent, Look};
use anomi_vm::people::activity::{Activity, Intent};

use super::support::{asked, front, settings};

const WIDTH: f32 = 265.0;
const PANEL: Size = Size::new(WIDTH, 420.0);

fn panel(activity: &Activity) -> Simulator<'_, Intent, Theme, anomi_theme::Renderer> {
    Simulator::with_size(settings(), PANEL, view(activity, front()))
}

fn click_at<'a, M: 'a>(
    mut ui: Simulator<'a, M, Theme, anomi_theme::Renderer>,
    at: Point,
) -> Vec<M> {
    ui.point_at(at);
    ui.simulate(iced_test::simulator::click());
    asked(ui)
}

#[test]
fn the_header_counts_the_cards() {
    let activity = samples::panel();
    let mut ui = panel(&activity);
    ui.find("Activity — 5").expect("the header's count");
}

#[test]
fn the_header_opens_the_rest_of_the_cards() {
    let activity = samples::panel();
    let asked = click_at(panel(&activity), Point::new(60.0, 24.0));
    assert_eq!(asked, [Intent::Expand(true)]);
    let opened = samples::opened();
    let asked = click_at(panel(&opened), Point::new(60.0, 24.0));
    assert_eq!(asked, [Intent::Expand(false)]);
}

#[test]
fn the_eye_hides_the_cards_and_nothing_else() {
    let activity = samples::panel();
    let asked = click_at(panel(&activity), Point::new(WIDTH - 48.0, 24.0));
    assert_eq!(asked, [Intent::Hide(true)]);
}

#[test]
fn hidden_cards_come_back_from_the_header_or_the_eye() {
    let hidden = samples::hidden();
    assert_eq!(
        click_at(panel(&hidden), Point::new(60.0, 24.0)),
        [Intent::Hide(false)]
    );
    assert_eq!(
        click_at(panel(&hidden), Point::new(WIDTH - 48.0, 24.0)),
        [Intent::Hide(false)]
    );
}

#[test]
fn three_cards_show_until_it_is_opened() {
    let activity = samples::panel();
    let mut ui = panel(&activity);
    assert!(ui.find("Celeste").is_ok());
    assert!(ui.find("Most played: 13h").is_err());
    let opened = samples::opened();
    let mut ui = panel(&opened);
    assert!(ui.find("Most played: 13h").is_ok());
    let hidden = samples::hidden();
    let mut ui = panel(&hidden);
    assert!(ui.find("Factorio").is_err());
}

#[test]
fn a_card_opens_its_persons_profile() {
    let activity = samples::panel();
    let asked = click_at(panel(&activity), Point::new(100.0, 74.0));
    assert!(
        matches!(&asked[..], [Intent::Profile(id, at)] if id == "1400000000000000002" && at.y < 74.0),
        "{asked:?}"
    );
}

#[test]
fn a_right_click_opens_the_member_menu() {
    let activity = samples::panel();
    let mut ui = panel(&activity);
    ui.point_at(Point::new(100.0, 150.0));
    ui.simulate([
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)),
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Right)),
    ]);
    let asked = asked(ui);
    assert!(
        matches!(&asked[..], [Intent::Menu(id, _)] if id == "1400000000000000003"),
        "{asked:?}"
    );
}

#[test]
fn without_cards_the_panel_is_not_there() {
    let none = samples::none();
    let mut ui = panel(&none);
    assert!(ui.find("Activity — 0").is_err());
    assert!(asked(ui).is_empty());
}

fn card_ui<'a>(
    card: &'a Card,
    look: &'a Look,
    menu: bool,
) -> Simulator<'a, CardIntent, Theme, anomi_theme::Renderer> {
    let view: Element<'a, CardIntent> = card::view(card, samples::NOW, look, menu, front());
    Simulator::with_size(settings(), Size::new(300.0, 200.0), view)
}

#[test]
fn a_game_counts_how_long_it_has_run() {
    let game = samples::game();
    let look = Look::default();
    let mut ui = card_ui(&game, &look, false);
    ui.find("PLAYING").expect("its heading");
    ui.find("42:07 elapsed").expect("its time");
}

#[test]
fn a_song_shows_how_far_it_has_got() {
    let song = samples::song();
    let look = Look::default();
    let mut ui = card_ui(&song, &look, false);
    ui.find("06:51").expect("played");
    ui.find("08:00").expect("length");
}

#[test]
fn a_song_title_opens_its_page() {
    let song = samples::song();
    let look = Look::default();
    let mut ui = card_ui(&song, &look, false);
    ui.click("Weightless").expect("its title");
    assert_eq!(
        asked(ui),
        [CardIntent::Link(
            "https://open.spotify.com/track/weightless".into()
        )]
    );
}

#[test]
fn a_stream_offers_to_be_watched() {
    let stream = samples::stream();
    let look = Look::default();
    // The button draws its own text, so it is found by place: under the
    // heading (16), the words (about 55) and the gaps (8 each).
    let asked = click_at(card_ui(&stream, &look, false), Point::new(30.0, 100.0));
    assert_eq!(asked, [CardIntent::Link("https://twitch.tv/anyone".into())]);
}

#[test]
fn a_card_with_no_time_says_none() {
    let hanging = samples::hanging();
    let look = Look::default();
    let mut ui = card_ui(&hanging, &look, false);
    assert!(ui.find("Chilling").is_ok());
    assert!(ui.find("elapsed").is_err());
}

#[test]
fn the_more_button_is_discords_and_only_when_asked_for() {
    let game = samples::game();
    let look = Look::default();
    let ui = card_ui(&game, &look, false);
    assert!(asked(ui).is_empty());
    // At the heading's end: the card's width, less the button.
    let mut at = Point::new(290.0, 8.0);
    let with = card_ui(&game, &look, true);
    let asked = click_at(with, at);
    assert!(matches!(&asked[..], [CardIntent::More(_)]), "{asked:?}");
    at.x = 100.0;
    let none = click_at(card_ui(&game, &look, false), at);
    assert!(none.is_empty());
}

#[test]
fn the_clock_runs_only_for_cards_with_times() {
    let cards = [samples::hanging(), samples::game()];
    assert!(card::wants_clock(&cards, samples::NOW));
    assert!(!card::wants_clock(&cards[..1], samples::NOW));
}
