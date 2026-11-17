use iced::window::{self, RedrawRequest};
use iced::{Event, Size};
use iced_test::Simulator;

use super::sweep::{PERIOD, band, light, stops};
use super::*;

fn front() -> Motion {
    Motion {
        front: true,
        ..Motion::default()
    }
}

#[test]
fn the_band_sweeps_in_from_the_left_and_out_at_the_right() {
    let start = Instant::now();
    let (left, span) = band(200.0, start);
    assert_eq!(span, 120.0);
    // It is at most as far left as its own width, and at most the skeleton's width right.
    assert!(left >= -span && left < 200.0);
    let later = start + PERIOD / 2;
    assert!(band(200.0, later).0 != left || PERIOD.is_zero());
    let wide = band(400.0, start);
    assert_eq!(wide.1, 200.0, "half the width when that is more than 120");
}

#[test]
fn lit_in_the_middle_and_dark_at_the_edges() {
    assert_eq!(light(0.0), 0.0);
    assert_eq!(light(0.5), 1.0);
    assert_eq!(light(1.0), 0.0);
}

#[test]
fn a_band_out_of_sight_draws_nothing() {
    assert!(stops(200.0, -120.0, 120.0).is_none());
    assert!(stops(200.0, 200.0, 120.0).is_none());
}

#[test]
fn a_band_inside_peaks_in_the_middle_and_is_clear_at_its_edges() {
    let all = stops(300.0, 50.0, 150.0).unwrap();
    assert_eq!(
        all.as_slice(),
        [
            (50.0 / 300.0, 0.0),
            (125.0 / 300.0, 1.0),
            (200.0 / 300.0, 0.0)
        ]
    );
}

#[test]
fn a_band_half_out_on_the_left_is_lit_at_the_edge() {
    let all = stops(300.0, -75.0, 150.0).unwrap();
    let all = all.as_slice();
    assert_eq!(all[0].0, 0.0);
    assert_eq!(all[0].1, light(0.5));
    assert!(all.iter().all(|(offset, _)| (0.0..=1.0).contains(offset)));
}

#[test]
fn a_band_half_out_on_the_right_is_lit_at_the_edge() {
    let all = stops(300.0, 225.0, 150.0).unwrap();
    let all = all.as_slice();
    let last = all.last().unwrap();
    assert_eq!(last.0, 1.0);
    assert_eq!(last.1, light(0.5));
}

#[test]
fn it_sweeps_only_while_watched() {
    assert!(sweeping(front()));
    assert!(!sweeping(Motion::default()), "behind another window");
    assert!(
        !sweeping(Motion {
            idle: true,
            ..front()
        }),
        "nobody about"
    );
    assert!(
        !sweeping(Motion {
            never: true,
            ..front()
        }),
        "animations off"
    );
}

#[test]
fn it_asks_for_no_frames_out_of_sight() {
    let view = Rectangle::new(iced::Point::ORIGIN, Size::new(300.0, 300.0));
    let inside = Rectangle::new(iced::Point::new(0.0, 10.0), Size::new(100.0, 12.0));
    let below = Rectangle::new(iced::Point::new(0.0, 2000.0), Size::new(100.0, 12.0));
    assert!(animating(front(), inside, &view));
    assert!(!animating(front(), below, &view));
    assert!(!animating(Motion::default(), inside, &view));
}

/// What a skeleton at `bounds` asks of the window on a frame.
fn asked(motion: Motion, bounds: Rectangle) -> RedrawRequest {
    use iced::advanced::layout::Layout;
    use iced::advanced::renderer::Settings;
    use iced::advanced::shell::{Bus, Waker};
    use iced::advanced::widget::{Tree, Widget};
    use iced::advanced::{Shell, mouse};

    let mut widget: Skeleton = skeleton(motion).width(bounds.width).height(bounds.height);
    let mut tree = Tree::empty();
    tree.tag = Widget::<(), Theme, Renderer>::tag(&widget);
    tree.state = Widget::<(), Theme, Renderer>::state(&widget);
    let renderer = Renderer::new(Settings::default());
    let mut bus = Bus::<()>::new();
    let mut shell = Shell::new(&window::Headless, Waker::noop(), &mut bus);
    let viewport = Rectangle::new(iced::Point::ORIGIN, Size::new(300.0, 300.0));
    let layout = Layout::new(bounds.size()).move_to(bounds.position());
    let event = Event::Window(window::Event::RedrawRequested(Instant::now()));
    widget.update(
        &mut tree,
        &event,
        layout,
        mouse::Cursor::Unavailable,
        &renderer,
        &mut shell,
        &viewport,
    );
    shell.redraw_request()
}

#[test]
fn its_update_asks_for_frames_only_while_watched_and_in_view() {
    let inside = Rectangle::new(iced::Point::new(0.0, 10.0), Size::new(100.0, 12.0));
    let below = Rectangle::new(iced::Point::new(0.0, 2000.0), Size::new(100.0, 12.0));
    assert_eq!(asked(front(), inside), RedrawRequest::NextFrame);
    for motion in [
        Motion::default(),
        Motion {
            idle: true,
            ..front()
        },
        Motion {
            never: true,
            ..front()
        },
    ] {
        assert_eq!(asked(motion, inside), RedrawRequest::Wait);
    }
    assert_eq!(asked(front(), below), RedrawRequest::Wait);
}

#[test]
fn the_ground_steps_off_the_theme() {
    for theme in [
        Theme::built_in(None),
        Theme::built_in(Some("catppuccin-latte")),
    ] {
        let palette = theme.palette();
        let expected = if palette.dark {
            palette.raised
        } else {
            palette.hover
        };
        assert_eq!(ground(&theme), expected);
    }
}

#[test]
fn it_draws_still_and_sweeping() {
    for motion in [Motion::default(), front()] {
        let view = skeleton(motion).width(120.0).height(12.0).radius(6.0);
        let mut ui: Simulator<'_, (), Theme, Renderer> =
            Simulator::with_size(iced::Settings::default(), Size::new(300.0, 100.0), view);
        ui.snapshot(&Theme::built_in(None)).expect("it draws");
    }
}

#[test]
fn the_messages_skeleton_draws_in_a_window() {
    let view = messages_skeleton::<()>(front());
    let mut ui: Simulator<'_, (), Theme, Renderer> =
        Simulator::with_size(iced::Settings::default(), Size::new(600.0, 500.0), view);
    ui.snapshot(&Theme::built_in(None)).expect("it draws");
}
