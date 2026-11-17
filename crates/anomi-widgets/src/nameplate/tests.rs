//! The nameplate: where it fades and how strong it is in each place, where
//! it finds the row's text, when it shows and when its picture plays, what
//! it draws and where it stops, and a row found and pressed by its name.

use std::time::{Duration, Instant};

use iced::widget::{button, column, container, row, scrollable, space, text};
use iced::{Color, Point, Rectangle, Size, Widget as _};
use iced_test::Simulator;

use anomi_theme::{Element, Motion, Palette, Renderer, Theme, fonts};

use crate::name::tests::support::{laid, renderer, shot};
use crate::name::{Style, styled_name};

use super::widget::State;
use super::{Nameplate, Place, extent, nameplate, sample};

fn front() -> Motion {
    Motion {
        front: true,
        ..Motion::default()
    }
}

/// A member row as the member list lays it: 8 px in, a 32 px avatar, 12 px,
/// then the name over its second line.
fn member<'a>(place: Place, motion: Motion) -> Nameplate<'a, ()> {
    let text_column = extent(column![text("Bob").size(16), text("Factorio").size(14)]);
    let content = row![space::horizontal().width(32), text_column]
        .spacing(12)
        .padding([0, 8]);
    nameplate(
        container(content).width(iced::Fill).height(42),
        place,
        motion,
    )
    .picture(Some(sample::strip("1500000000000000006")))
    .palette(sample::palette("sky"))
    .radius(8.0)
}

fn extent_of(plate: &mut Nameplate<'_, ()>) -> Option<(f32, f32)> {
    let tree = laid(plate, Size::new(248.0, 42.0));
    tree.state.downcast_ref::<State>().extent
}

#[test]
fn each_place_has_discords_strength_and_fade() {
    assert_eq!(Place::Member.strength(false, false), 0.6);
    assert_eq!(Place::Member.strength(true, false), 0.9);
    assert_eq!(Place::Dm.strength(false, true), 0.9);
    assert_eq!(Place::Account.strength(false, false), 0.4);
    assert_eq!(Place::Account.strength(true, true), 0.6);
    // A member row reads the text's width, and fades from 10 px after it.
    assert_eq!(Place::Member.fade(248.0, (52.0, 60.0)), (70.0, 120.0));
    // A DM row from where the text ends, 5 px before.
    assert_eq!(Place::Dm.fade(248.0, (52.0, 60.0)), (107.0, 157.0));
    // Never past the plate.
    assert_eq!(Place::Member.fade(100.0, (0.0, 95.0)), (100.0, 100.0));
}

#[test]
fn the_tint_ends_stronger_when_chosen_or_pointed_at() {
    let colour = Color::from_rgb(0.0, 0.5, 0.7);
    let end = |place: Place, hovered, selected| {
        place
            .tint(colour, hovered, selected)
            .stops
            .iter()
            .flatten()
            .last()
            .map(|stop| stop.color.a)
    };
    assert_eq!(end(Place::Member, false, false), Some(0.2));
    assert_eq!(end(Place::Member, true, false), Some(0.3));
    assert_eq!(end(Place::Member, false, true), Some(0.5));
    assert_eq!(end(Place::Dm, true, false), Some(0.2), "a DM row's stays");
    assert_eq!(end(Place::Account, false, false), Some(0.4));
}

#[test]
fn the_plate_finds_the_rows_text() {
    let (x, width) = extent_of(&mut member(Place::Member, front())).expect("the text");
    assert_eq!(x, 52.0, "after the padding, the avatar and the spacing");
    assert!(
        width > 40.0 && width < 100.0,
        "as wide as \"Factorio\": {width}"
    );
    // A container around the mark takes its tag: the mark inside is found.
    let wrapped = container(extent(text("Bob"))).padding(4);
    let mut plate: Nameplate<'_, ()> = nameplate(
        row![space::horizontal().width(20), wrapped],
        Place::Dm,
        front(),
    );
    let (x, _) = extent_of(&mut plate).expect("the text");
    assert_eq!(x, 24.0);
    // Without a mark, none.
    let mut bare: Nameplate<'_, ()> = nameplate(text("Bob"), Place::Member, front());
    assert_eq!(extent_of(&mut bare), None);
}

#[test]
fn a_dm_rows_plate_shows_only_while_the_row_is_active() {
    let plate = member(Place::Dm, front());
    assert!(!plate.shows(false));
    assert!(plate.shows(true), "pointed at");
    assert!(member(Place::Dm, front()).selected(true).shows(false));
    assert!(member(Place::Dm, front()).focused(true).shows(false));
    assert!(member(Place::Member, front()).shows(false));
    // Nothing to draw: no plate.
    let bare: Nameplate<'_, ()> = nameplate(text("Bob"), Place::Member, front());
    assert!(!bare.shows(true));
}

/// Advances `plate`'s picture at `now` with the pointer on it or not.
fn advance(
    plate: &Nameplate<'_, ()>,
    state: &mut State,
    hovered: bool,
    now: Instant,
) -> Option<Instant> {
    state.hovered = hovered;
    let bounds = Rectangle::new(Point::ORIGIN, Size::new(248.0, 42.0));
    plate.advance(state, &renderer(), bounds, &bounds, now)
}

#[test]
fn the_picture_plays_only_under_the_pointer_while_the_window_plays() {
    let start = Instant::now();
    let plate = member(Place::Member, front());
    let mut state = State::default();
    assert_eq!(
        advance(&plate, &mut state, false, start),
        None,
        "at rest, the still"
    );
    assert!(
        advance(&plate, &mut state, true, start).is_some(),
        "pointed at, it plays"
    );
    let later = start + Duration::from_millis(150);
    assert!(advance(&plate, &mut state, true, later).is_some());
    assert_eq!(state.player.frame(), 1, "the second frame");
    // The pointer leaves: back to the still, and no more frames.
    assert_eq!(advance(&plate, &mut state, false, later), None);
    assert_eq!(state.player.frame(), 0);
    // Behind another window, or with animations off, it holds still.
    let behind = member(Place::Member, Motion::default());
    assert_eq!(advance(&behind, &mut State::default(), true, start), None);
    let never = member(
        Place::Member,
        Motion {
            never: true,
            ..front()
        },
    );
    assert_eq!(advance(&never, &mut State::default(), true, start), None);
    // Held for the preview: as pointed at, but still.
    let held = member(Place::Account, front()).held();
    assert_eq!(advance(&held, &mut State::default(), false, start), None);
    assert_eq!(advance(&held, &mut State::default(), true, start), None);
}

/// The mean brightness of the plate's right end, where its picture is.
fn right_end(pointer: Option<Point>) -> f32 {
    let shot = shot(
        member(Place::Member, front()),
        Size::new(248.0, 42.0),
        &Theme::built_in(None),
        pointer,
    );
    let (width, height) = shot.dimensions();
    let mut sum = 0.0;
    let mut count = 0.0;
    for y in height / 4..height * 3 / 4 {
        for x in width - 60..width - 20 {
            let pixel = shot.get_pixel(x, y).0;
            sum += f32::from(pixel[0]) + f32::from(pixel[1]) + f32::from(pixel[2]);
            count += 1.0;
        }
    }
    sum / count
}

#[test]
fn the_plate_draws_its_picture_stronger_under_the_pointer() {
    let ground = {
        let theme = Theme::built_in(None);
        let empty: Nameplate<'_, ()> = nameplate(
            container(text("")).width(248).height(42),
            Place::Member,
            front(),
        );
        let shot = shot(empty, Size::new(248.0, 42.0), &theme, None);
        let pixel = shot.get_pixel(shot.width() - 40, 40).0;
        f32::from(pixel[0]) + f32::from(pixel[1]) + f32::from(pixel[2])
    };
    let rest = right_end(None);
    let pointed = right_end(Some(Point::new(200.0, 20.0)));
    assert!(
        rest > ground + 60.0,
        "the strip shows at its end: {rest} over {ground}"
    );
    assert!(
        pointed > rest + 20.0,
        "stronger pointed at: {pointed} over {rest}"
    );
}

/// How a row stands, each over its own ground as the lists lay it.
#[derive(Clone, Copy, Debug)]
enum Stand {
    Rest,
    Held,
    Chosen,
}

impl Stand {
    fn ground(self, palette: &Palette) -> Color {
        match self {
            Self::Rest => palette.sidebar,
            Self::Held => palette.hover,
            Self::Chosen => palette.active,
        }
    }
}

/// A member plate at `opacity`, standing so, over its ground: no text in
/// the row, but a mark `mark` px wide, so the plate fades from 10 px after
/// it; with no mark, from its right end.
fn plate_over_ground(mark: Option<f32>, opacity: f32, stand: Stand) -> image::RgbaImage {
    let row: Element<'_, ()> = match mark {
        Some(width) => extent(space::horizontal().width(width)).boxed(),
        None => space::horizontal().boxed(),
    };
    let row = container(row).width(iced::Fill).height(42);
    let plate = nameplate(row, Place::Member, front())
        .picture(Some(sample::strip("1500000000000000006")))
        .palette(sample::palette("sky"))
        .radius(8.0)
        .opacity(opacity);
    let plate = match stand {
        Stand::Rest => plate,
        Stand::Held => plate.held(),
        Stand::Chosen => plate.selected(true),
    };
    let view = container(plate)
        .style(move |theme: &Theme| container::background(stand.ground(theme.palette())));
    shot(view, Size::new(248.0, 42.0), &Theme::built_in(None), None)
}

/// How far from `ground` a shot lies across the row's middle about `x`, as
/// a mean over 4 px, each channel.
fn lift(shot: &image::RgbaImage, ground: Color, x: u32) -> [f32; 3] {
    let ground = ground.into_rgba8();
    let mut lift = [0.0; 3];
    for at in x * 2 - 4..x * 2 + 4 {
        let pixel = shot.get_pixel(at, 42).0;
        for (sum, (a, b)) in lift.iter_mut().zip(pixel.iter().zip(&ground)) {
            *sum += (f32::from(*a) - f32::from(*b)) / 8.0;
        }
    }
    lift
}

#[test]
fn an_offline_members_plate_fades_as_a_whole() {
    // Slint fades the plate as one layer: half as far from the ground at
    // every point, under the text as past it, pointed at and chosen too,
    // where the tint ends stronger.
    let theme = Theme::built_in(None);
    let dot = |a: [f32; 3], b: [f32; 3]| a.iter().zip(&b).map(|(a, b)| a * b).sum::<f32>();
    for stand in [Stand::Rest, Stand::Held, Stand::Chosen] {
        let ground = stand.ground(theme.palette());
        let full = plate_over_ground(Some(150.0), 1.0, stand);
        let half = plate_over_ground(Some(150.0), 0.5, stand);
        // Under the text, where the fade from 160 px lets .3 of the picture
        // through, and past the fade.
        for x in [140, 230] {
            let (whole, faded) = (lift(&full, ground, x), lift(&half, ground, x));
            assert!(
                dot(whole, whole) > 100.0,
                "the plate shows at {x}, {stand:?}: {whole:?}"
            );
            let ratio = dot(faded, whole) / dot(whole, whole);
            assert!(
                (ratio - 0.5).abs() <= 0.07,
                "half as far from the ground at {x}, {stand:?}: {ratio} ({faded:?} of {whole:?})"
            );
        }
    }
}

#[test]
fn a_plate_with_no_mark_stays_shaded_to_its_end() {
    // With no mark the fade starts at the plate's right end, as one whose
    // text reaches it: shaded all along, not faded out across it.
    let ground = Theme::built_in(None).palette().sidebar;
    let bare = lift(&plate_over_ground(None, 1.0, Stand::Rest), ground, 230);
    let late = lift(
        &plate_over_ground(Some(230.0), 1.0, Stand::Rest),
        ground,
        230,
    );
    for (bare, late) in bare.iter().zip(&late) {
        assert!(
            (bare - late).abs() <= 3.0,
            "as dark as a fade from 240 px: {bare:?} against {late:?}"
        );
    }
}

#[test]
fn a_plate_half_out_of_its_list_draws_nothing_past_it() {
    // The row's lower half lies under the list's bottom edge, where the
    // window's ground goes on.
    let plate = member(Place::Member, front()).selected(true);
    let list = scrollable(plate).height(21);
    let view = column![
        space::vertical().height(21),
        list,
        space::vertical().height(21)
    ];
    let shot = shot(view, Size::new(248.0, 63.0), &Theme::built_in(None), None);
    let ground = shot.get_pixel(0, 0).0;
    let off = |x: u32, y: u32| {
        let pixel = shot.get_pixel(x, y).0;
        pixel[..3]
            .iter()
            .zip(&ground[..3])
            .map(|(a, b)| u32::from(a.abs_diff(*b)))
            .sum::<u32>()
    };
    // Twice the pixels: the list is 42 to 84 down the shot.
    let shows = (0..shot.width()).filter(|x| off(*x, 63) > 12).count();
    assert!(shows > 100, "the plate shows in the list: {shows}");
    for y in 84..shot.height() {
        for x in 0..shot.width() {
            assert!(
                off(x, y) <= 3,
                "past the list at {x},{y}: {:?}",
                shot.get_pixel(x, y).0
            );
        }
    }
}

#[test]
fn a_row_is_found_and_pressed_by_the_name_in_it() {
    fonts::install();
    let name = styled_name("Bob", Style::new(14, 3, &[0x00ff99]));
    let row = button(extent(column![name])).on_press(());
    let plate = nameplate(row, Place::Member, front());
    let mut ui: Simulator<'_, (), Theme, Renderer> =
        Simulator::with_size(iced::Settings::default(), Size::new(248.0, 42.0), plate);
    ui.click("Bob").expect("the row, by its name");
    assert_eq!(ui.into_messages().count(), 1, "pressed");
}
