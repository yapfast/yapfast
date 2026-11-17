//! What's under the pointer: a click on a link, a mention, a covered
//! spoiler or an emoji sends what it opens; a right press sends it before
//! the menu around opens; plain text sends nothing and starts a selection.

use iced::widget::mouse_area;
use iced::{Point, mouse};
use iced_test::simulator::click;

use anomi_vm::fixtures::text as samples;
use anomi_vm::text::Link;

use super::*;

/// Clicks what opens `link` in `text`, laid out across the window.
fn click_link(text: &Text, link: &Link) -> Vec<Asked> {
    let state = laid_out(text, SIZE.width);
    let at = middle_of(&state, link);
    let mut ui = simulate(linked(text));
    move_to(&mut ui, at);
    ui.simulate(click());
    ui.into_messages().collect()
}

#[test]
fn a_click_on_a_link_opens_it() {
    let link = Link::Leave("https://example.org".into());
    assert_eq!(click_link(&samples::welcome(), &link), [Asked::Open(link)]);
}

#[test]
fn a_click_on_a_mention_opens_its_person() {
    let link = Link::User("bob".into());
    assert_eq!(click_link(&samples::mentions(), &link), [Asked::Open(link)]);
}

#[test]
fn a_click_on_a_covered_spoiler_reveals_it() {
    assert_eq!(
        click_link(&samples::ending(), &Link::Spoiler(1)),
        [Asked::Open(Link::Spoiler(1))]
    );
}

#[test]
fn a_click_on_an_emoji_opens_its_card() {
    let thumbs = Link::Emoji("👍".into());
    assert_eq!(
        click_link(&samples::history(55), &thumbs),
        [Asked::Open(thumbs)]
    );
    let talk = Link::CustomEmoji(samples::TALK.into());
    assert_eq!(click_link(&samples::ship_it(), &talk), [Asked::Open(talk)]);
}

#[test]
fn plain_text_opens_nothing() {
    let text = samples::welcome();
    let mut ui = simulate(linked(&text));
    move_to(&mut ui, Point::new(10.0, 8.0));
    ui.simulate(click());
    assert_eq!(ui.into_messages().collect::<Vec<_>>(), []);
}

#[test]
fn a_right_press_names_the_link_and_still_opens_the_menu() {
    let text = samples::article();
    let link = Link::Url("https://example.com/news/bridges".into());
    let at = middle_of(&laid_out(&text, SIZE.width), &link);
    let mut ui = simulate(mouse_area(linked(&text)).on_right_press(Asked::Menu));
    move_to(&mut ui, at);
    ui.simulate([
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)),
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Right)),
    ]);
    assert_eq!(
        ui.into_messages().collect::<Vec<_>>(),
        [Asked::Right(link), Asked::Menu]
    );
}

#[test]
fn a_press_on_a_link_and_a_release_off_it_opens_nothing() {
    let text = samples::welcome();
    let link = Link::Leave("https://example.org".into());
    let at = middle_of(&laid_out(&text, SIZE.width), &link);
    let mut ui = simulate(linked(&text));
    move_to(&mut ui, at);
    ui.simulate([Event::Mouse(mouse::Event::ButtonPressed(
        mouse::Button::Left,
    ))]);
    move_to(&mut ui, Point::new(5.0, 300.0));
    ui.simulate([Event::Mouse(mouse::Event::ButtonReleased(
        mouse::Button::Left,
    ))]);
    assert_eq!(ui.into_messages().collect::<Vec<_>>(), []);
}

#[test]
fn the_hand_shows_over_links_and_the_beam_over_text() {
    use iced::advanced::widget::Widget as _;
    let text = samples::welcome();
    let mut view = linked(&text);
    let (tree, height) = tree_of(&mut view, SIZE.width);
    let state = tree.state.downcast_ref::<State>();
    let link = middle_of(state, &Link::Leave("https://example.org".into()));
    assert!(height > 0.0);
    assert!(state.link_at(link).is_some());
    assert!(state.glyph_at(Point::new(10.0, 8.0)));
    assert!(!state.glyph_at(Point::new(590.0, 8.0)));
    let interaction = view.mouse_interaction(
        &tree,
        iced::advanced::Layout::new(tree.size),
        mouse::Cursor::Available(Point::new(590.0, 8.0)),
        &iced::Rectangle::with_size(SIZE),
        &Renderer::new(Default::default()),
    );
    assert_eq!(interaction, mouse::Interaction::None);
}
