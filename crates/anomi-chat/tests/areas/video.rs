//! A video in a message, and full screen: task C09's tests, driving
//! `anomi_chat::video::view` over its fixtures in the simulator
//! (`super::support`).

use std::time::Instant;

use iced::keyboard::{Key, key::Named};
use iced::widget::container;
use iced::{Event, Point, Size, mouse, window};
use iced_test::Simulator;
use iced_test::simulator::click;

use anomi_chat::video::{full_screen, view};
use anomi_theme::prelude::*;
use anomi_vm::chat::video::{FullScreen, Intent, Video};
use anomi_vm::fixtures::chat::video as fixture;
use anomi_vm::menu::Spot;

use super::support::{self, SIZE, asked};

/// Bob's clip's cell, alone in a window of its size.
const CELL: Size = Size::new(fixture::CELL.0, fixture::CELL.1);
const MIDDLE: Point = Point::new(200.0, 100.0);
/// The controls along the cell's bottom, 36 high: play at the left, mute
/// and full screen at the right, 6 from the edge; the bar between.
const STRIP: f32 = 225.0 - 18.0;
const PLAY: Point = Point::new(18.0, STRIP);
const BETWEEN: Point = Point::new(35.0, STRIP);
const EXPAND: Point = Point::new(380.0, STRIP);
const MUTE: Point = Point::new(346.0, STRIP);
/// On the volume panel over mute, half way up its rail.
const HALF_WAY: Point = Point::new(346.0, 135.0);

type Ui<'a> = Simulator<'a, Intent, Theme, anomi_theme::Renderer>;

fn cell(video: &Video) -> Ui<'_> {
    let sized = container(view(video, support::front()))
        .width(CELL.width)
        .height(CELL.height);
    Simulator::with_size(support::settings(), CELL, sized)
}

fn clicked(video: &Video, at: Point) -> Vec<Intent> {
    let mut ui = cell(video);
    ui.point_at(at);
    ui.simulate(click());
    asked(ui)
}

fn moved(ui: &mut Ui<'_>, to: Point) {
    ui.point_at(to);
    ui.simulate([Event::Mouse(mouse::Event::CursorMoved { position: to })]);
}

fn volume(intents: &[Intent]) -> f32 {
    match intents {
        [Intent::Volume(volume)] => *volume,
        other => panic!("not one volume: {other:?}"),
    }
}

#[test]
fn a_click_on_a_cell_not_playing_plays_it_at_the_cells_size() {
    let play = || Intent::Play {
        item: fixture::ITEM,
        index: fixture::INDEX,
        width: CELL.width,
        height: CELL.height,
    };
    assert_eq!(clicked(&fixture::idle(), MIDDLE), [play()]);
    // One that failed tries again.
    assert_eq!(clicked(&fixture::failed(), MIDDLE), [play()]);
}

#[test]
fn a_click_on_the_video_playing_plays_or_pauses_it() {
    assert_eq!(clicked(&fixture::playing(), MIDDLE), [Intent::Toggle]);
    assert_eq!(clicked(&fixture::paused(), MIDDLE), [Intent::Toggle]);
    assert_eq!(clicked(&fixture::ended(), MIDDLE), [Intent::Toggle]);
}

#[test]
fn the_controls_play_go_full_screen_and_mute() {
    let paused = fixture::paused();
    assert_eq!(clicked(&paused, PLAY), [Intent::Toggle]);
    assert_eq!(clicked(&paused, EXPAND), [Intent::Expand(true)]);
    assert_eq!(clicked(&paused, MUTE), [Intent::Mute(true)]);
    assert_eq!(clicked(&fixture::muted(), MUTE), [Intent::Mute(false)]);
}

#[test]
fn a_click_between_the_controls_leaves_the_video_be() {
    assert!(clicked(&fixture::paused(), BETWEEN).is_empty());
}

#[test]
fn a_file_without_sound_has_no_mute() {
    let asked = clicked(&fixture::silent(), MUTE);
    assert!(asked.iter().all(|intent| matches!(intent, Intent::Seek(_))));
}

#[test]
fn the_bar_moves_through_the_video() {
    let asked = clicked(&fixture::paused(), Point::new(215.0, STRIP));
    match asked.as_slice() {
        [Intent::Seek(at)] => assert!((0.35..0.65).contains(at), "{at}"),
        other => panic!("not a seek: {other:?}"),
    }
}

#[test]
fn the_wheel_over_mute_turns_the_volume() {
    let paused = fixture::paused();
    let mut ui = cell(&paused);
    moved(&mut ui, MUTE);
    ui.simulate([Event::Mouse(mouse::Event::WheelScrolled {
        delta: mouse::ScrollDelta::Lines { x: 0.0, y: 1.0 },
    })]);
    assert!((volume(&asked(ui)) - 0.85).abs() < 1e-4);
}

#[test]
fn a_press_on_the_panel_over_mute_sets_the_volume_there() {
    let paused = fixture::paused();
    let mut ui = cell(&paused);
    moved(&mut ui, MUTE);
    moved(&mut ui, HALF_WAY);
    ui.simulate(click());
    assert!((volume(&asked(ui)) - 0.5).abs() < 1e-4);
}

#[test]
fn the_panel_shows_only_with_the_pointer_on_mute() {
    // Without the pointer on mute first, the same press is the video's.
    assert_eq!(clicked(&fixture::paused(), HALF_WAY), [Intent::Toggle]);
}

/// Full screen in the golden window: the clip is 1301 by 732 under the
/// 64 px bar, its controls 8 under it.
const SCREEN: Point = Point::new(700.0, 430.0);
const OUTSIDE: Point = Point::new(20.0, 430.0);
const BAR_EMPTY: Point = Point::new(400.0, 32.0);
const CLOSE: Point = Point::new(1372.0, 32.0);
const SAVE: Point = Point::new(1332.0, 32.0);
const SHRINK: Point = Point::new(1330.0, 824.0);

fn large(full: &FullScreen) -> Ui<'_> {
    Simulator::with_size(
        support::settings(),
        SIZE,
        full_screen(full, support::front()),
    )
}

fn clicked_large(full: &FullScreen, at: Point) -> Vec<Intent> {
    let mut ui = large(full);
    ui.point_at(at);
    ui.simulate(click());
    asked(ui)
}

fn key(full: &FullScreen, key: Key) -> Vec<Intent> {
    let mut ui = large(full);
    ui.tap_key(key);
    asked(ui)
}

#[test]
fn full_screen_a_click_on_it_plays_or_pauses_and_elsewhere_brings_it_back() {
    let full = fixture::full_screen_video();
    assert_eq!(clicked_large(&full, SCREEN), [Intent::Toggle]);
    assert_eq!(clicked_large(&full, OUTSIDE), [Intent::Expand(false)]);
    assert_eq!(clicked_large(&full, BAR_EMPTY), [Intent::Expand(false)]);
    assert_eq!(clicked_large(&full, CLOSE), [Intent::Expand(false)]);
    assert_eq!(clicked_large(&full, SHRINK), [Intent::Expand(false)]);
}

#[test]
fn full_screen_saves_the_file_but_not_a_links_preview() {
    let full = fixture::full_screen_video();
    let save = Intent::Save {
        item: fixture::ITEM,
        index: fixture::INDEX,
    };
    assert_eq!(clicked_large(&full, SAVE), [save]);
    let preview = fixture::preview_video();
    assert_eq!(clicked_large(&preview, SAVE), [Intent::Expand(false)]);
}

#[test]
fn full_screen_space_plays_and_the_arrows_move_five_seconds() {
    let full = fixture::full_screen_video();
    assert_eq!(key(&full, Key::Named(Named::Space)), [Intent::Toggle]);
    let on = key(&full, Key::Named(Named::ArrowRight));
    let back = key(&full, Key::Named(Named::ArrowLeft));
    let at = full.player.progress;
    match (on.as_slice(), back.as_slice()) {
        ([Intent::Seek(on)], [Intent::Seek(back)]) => {
            assert!((on - (at + 5.0 / 90.0)).abs() < 1e-4);
            assert!((back - (at - 5.0 / 90.0)).abs() < 1e-4);
        }
        other => panic!("not two seeks: {other:?}"),
    }
    // Its length unknown, the arrows do nothing.
    let loading = fixture::full_screen_of(fixture::loading().player.expect("a player"));
    assert!(key(&loading, Key::Named(Named::ArrowRight)).is_empty());
}

#[test]
fn full_screen_a_right_click_opens_its_menu_where_the_pointer_is() {
    let full = fixture::full_screen_video();
    let mut ui = large(&full);
    ui.point_at(SCREEN);
    ui.simulate([
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)),
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Right)),
    ]);
    assert_eq!(asked(ui), [Intent::Menu(Spot::pointer(SCREEN))]);
}

#[test]
fn full_screen_tells_the_player_the_size_it_shows_at() {
    let full = fixture::full_screen_video();
    let mut ui = large(&full);
    ui.simulate([Event::Window(
        window::Event::RedrawRequested(Instant::now()),
    )]);
    let asked = asked(ui);
    match asked.as_slice() {
        [Intent::Resize { width, height }] => {
            assert!((height - (SIZE.height - 128.0)).abs() < 0.5, "{height}");
            assert!((width / height - 16.0 / 9.0).abs() < 0.01, "{width}");
        }
        other => panic!("not a resize: {other:?}"),
    }
}

/// A second of a 720p video at 30 frames a second, each frame a new handle
/// on the pixels the core wrote, as the app hands them to the same cell:
/// it draws each as it is, so nothing of them is copied into the pictures'
/// store.
#[test]
fn frames_are_new_handles_drawn_without_a_copy() {
    use std::time::Duration;

    use iced::advanced::graphics::core::Bytes;
    use iced::widget::image;

    let pixels = Bytes::from(vec![90u8; 1280 * 720 * 4]);
    let frames: Vec<Video> = (0..30u8)
        .map(|frame| {
            let mut video = fixture::playing();
            if let Some(player) = video.player.as_mut() {
                player.frame = Some(image::Handle::from_rgba(1280, 720, pixels.clone()));
                player.progress = f32::from(frame) / 30.0;
            }
            video
        })
        .collect();
    let sized = |video| {
        container(view(video, support::front()))
            .width(CELL.width)
            .height(CELL.height)
    };
    let mut ui = Simulator::with_size(support::settings(), CELL, sized(&frames[0]));
    let start = Instant::now();
    for (at, video) in frames.iter().enumerate() {
        ui = ui.rebuild(sized(video));
        let now = start + Duration::from_millis(33 * at as u64);
        ui.simulate([Event::Window(window::Event::RedrawRequested(now))]);
        ui.draw(&Theme::default());
    }
    assert_eq!(anomi_widgets::picture::bytes(), 0);
}
