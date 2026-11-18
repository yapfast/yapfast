//! A voice message's player: task C11's tests, driving
//! `anomi_chat::voice_message::view` over its fixtures in the simulator
//! (`super::support`).

use iced_test::Simulator;

use anomi_chat::voice_message::{view, widest};
use anomi_theme::Motion;
use anomi_vm::chat::voice_message::{Intent, VoiceMessage};
use anomi_vm::fixtures::chat::voice_message as fixtures;

use super::support;

const SIZE: iced::Size = iced::Size::new(400.0, 120.0);

/// Clicks `label` with animations off, so the speed and mute show.
fn click(voice: &VoiceMessage, label: &str) -> Vec<Intent> {
    let never = Motion {
        never: true,
        ..support::front()
    };
    let mut ui = Simulator::with_size(support::settings(), SIZE, view(voice, never));
    let _ = ui.click(label);
    support::asked(ui)
}

/// The speaker's centre: 28 px, 4 after the speed's face, which is 6 px
/// wider than its text on each side.
fn speaker(
    ui: &mut Simulator<'_, Intent, anomi_theme::Theme, anomi_theme::Renderer>,
) -> iced::Point {
    let speed = ui.find("1x").expect("the speed").bounds();
    iced::Point::new(speed.x + speed.width + 6.0 + 4.0 + 14.0, 26.0)
}

#[test]
fn the_round_button_toggles() {
    let voice = fixtures::idle();
    let mut ui = Simulator::with_size(support::settings(), SIZE, view(&voice, support::front()));
    ui.point_at(iced::Point::new(26.0, 26.0));
    ui.simulate(iced_test::simulator::click());
    assert_eq!(
        support::asked(ui),
        [Intent::Toggle {
            item: fixtures::ITEM
        }]
    );
}

#[test]
fn a_preview_cannot_be_played() {
    let voice = fixtures::preview();
    let mut ui = Simulator::with_size(support::settings(), SIZE, view(&voice, support::front()));
    ui.point_at(iced::Point::new(26.0, 26.0));
    ui.simulate(iced_test::simulator::click());
    assert!(
        !support::asked(ui)
            .iter()
            .any(|i| matches!(i, Intent::Toggle { .. }))
    );
}

#[test]
fn the_waveform_seeks_and_pauses_while_held() {
    let voice = fixtures::playing();
    let mut ui = Simulator::with_size(support::settings(), SIZE, view(&voice, support::front()));
    // Half way along the bars, which start after the button and its gap.
    let x = 8.0 + 36.0 + 12.0 + 40.0 * 6.0 / 2.0;
    ui.point_at(iced::Point::new(x, 26.0));
    ui.simulate(iced_test::simulator::click());
    let asked = support::asked(ui);
    let item = fixtures::ITEM;
    assert_eq!(asked.first(), Some(&Intent::Toggle { item }));
    assert!(matches!(asked[1], Intent::Seek { at, .. } if (at - 0.5).abs() < 0.05));
    assert_eq!(asked.last(), Some(&Intent::Toggle { item }));
}

#[test]
fn the_speed_button_cycles_it() {
    let voice = fixtures::playing();
    assert_eq!(click(&voice, "1x"), [Intent::CycleSpeed]);
}

#[test]
fn a_muted_player_shows_its_speed() {
    let voice = fixtures::muted();
    assert_eq!(click(&voice, "1.5x"), [Intent::CycleSpeed]);
}

#[test]
fn the_time_counts_down_or_shows_the_error() {
    assert_eq!(fixtures::playing().shown_time(), "0:08");
    assert_eq!(fixtures::failed().shown_time(), "Couldn't play");
    assert_eq!(fixtures::idle().played(), 0.0);
    assert_eq!(fixtures::paused().played(), 0.5);
}

#[test]
fn the_time_keeps_its_widest_width() {
    assert_eq!(widest("1:00:00"), "0:00:00");
    assert_eq!(widest("10:00"), "00:00");
    assert_eq!(widest("0:12"), "0:00");
}

#[test]
fn the_speed_and_mute_come_out_under_the_pointer() {
    // Behind another window nothing moves: they are out at once.
    let voice = fixtures::idle();
    let mut ui = Simulator::with_size(support::settings(), SIZE, view(&voice, Motion::default()));
    let _ = ui.click("1x");
    let _ = ui.click("1x");
    let at = speaker(&mut ui);
    ui.point_at(at);
    ui.simulate(iced_test::simulator::click());
    assert_eq!(support::asked(ui), [Intent::CycleSpeed, Intent::Mute(true)]);
}

#[test]
fn the_wheel_on_the_speaker_turns_the_volume() {
    let voice = fixtures::idle();
    let never = Motion {
        never: true,
        ..support::front()
    };
    let mut ui = Simulator::with_size(support::settings(), SIZE, view(&voice, never));
    // Over the waveform the wheel is the timeline's.
    ui.point_at(iced::Point::new(120.0, 26.0));
    let up = iced::Event::Mouse(iced::mouse::Event::WheelScrolled {
        delta: iced::mouse::ScrollDelta::Lines { x: 0.0, y: 1.0 },
    });
    ui.simulate([up.clone()]);
    let at = speaker(&mut ui);
    ui.point_at(at);
    ui.simulate([up]);
    let asked = support::asked(ui);
    assert!(matches!(asked[..], [Intent::Volume(v)] if (v - 0.85).abs() < 1e-4));
}

#[test]
fn a_muted_volume_is_shown_as_nothing() {
    assert_eq!(fixtures::muted().shown_volume(), 0.0);
    assert_eq!(fixtures::idle().shown_volume(), 0.8);
}
