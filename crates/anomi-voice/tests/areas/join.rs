//! Joining a voice channel or a DM's call: task V07's tests, driving
//! `anomi_voice::join::view` over its fixtures in the simulator
//! (`super::support`).

use iced::{Point, Size};
use iced_test::Simulator;

use anomi_theme::{Renderer, Theme};
use anomi_vm::fixtures::voice::join as samples;
use anomi_vm::voice::join::{Intent, Join};
use anomi_voice::join;

use super::support::{asked, front, settings};

/// A DM's bar as the conversation gives it: 210 px tall.
const BAR: Size = Size::new(680.0, 210.0);
const SCREEN: Size = Size::new(680.0, 600.0);

fn simulate(vm: &Join, size: Size) -> Simulator<'_, Intent, Theme, Renderer> {
    Simulator::with_size(settings(), size, join::view(vm, front()))
}

/// Clicks down the middle where the channel screen's button sits (about 398
/// px, the column being centred in 600): what it sent, each time.
fn swept(vm: &Join) -> Vec<Intent> {
    let mut ui = simulate(vm, SCREEN);
    for y in (380..=420).step_by(4) {
        ui.point_at(Point::new(340.0, y as f32));
        ui.simulate(iced_test::simulator::click());
    }
    asked(ui)
}

fn pressed(vm: &Join, size: Size, at: Point) -> Vec<Intent> {
    let mut ui = simulate(vm, size);
    ui.point_at(at);
    ui.simulate(iced_test::simulator::click());
    asked(ui)
}

#[test]
fn a_voice_channel_joins_from_its_button() {
    let vm = Join::Channel(samples::channel());
    let mut ui = simulate(&vm, SCREEN);
    assert!(ui.find("Lounge").is_ok());
    drop(ui);
    let sent = swept(&vm);
    assert!(!sent.is_empty());
    assert!(sent.iter().all(|intent| *intent == Intent::Join));
}

#[test]
fn another_call_makes_it_a_switch() {
    let vm = Join::Channel(samples::switching());
    assert!(swept(&vm).contains(&Intent::Join));
}

#[test]
fn an_unencrypted_channel_says_so_and_cannot_be_joined() {
    let vm = Join::Channel(samples::unencrypted());
    let mut ui = simulate(&vm, SCREEN);
    assert!(
        ui.find("This channel is unencrypted. Yapfast only joins encrypted voice channels.")
            .is_ok()
    );
    drop(ui);
    assert!(swept(&vm).is_empty());
}

#[test]
fn an_encrypted_channel_and_the_preview_have_no_notice() {
    for channel in [samples::channel(), samples::preview()] {
        let vm = Join::Channel(channel);
        let mut ui = simulate(&vm, SCREEN);
        assert!(
            ui.find("This channel is unencrypted. Yapfast only joins encrypted voice channels.")
                .is_err()
        );
    }
}

#[test]
fn a_locked_channel_gives_its_reason_and_no_join() {
    let vm = Join::Channel(samples::locked());
    let mut ui = simulate(&vm, SCREEN);
    assert!(
        ui.find("Your roles on this Discord server don't let you join this channel.")
            .is_ok()
    );
    drop(ui);
    assert!(swept(&vm).is_empty());
}

#[test]
fn a_call_in_a_dm_joins_by_its_pill() {
    let vm = Join::Call(samples::call());
    let mut ui = simulate(&vm, BAR);
    assert!(ui.find("Bob in the call.").is_ok());
    drop(ui);
    // One pill, centred in the bar and its content.
    let asked = pressed(&vm, BAR, Point::new(340.0, 125.0));
    assert_eq!(
        asked,
        [Intent::Answer {
            room: "bob".into(),
            accept: true
        }]
    );
}

#[test]
fn a_ringing_call_offers_decline_and_accept_apart() {
    let vm = Join::Call(samples::ringing());
    let mut ui = simulate(&vm, BAR);
    assert!(ui.find("Bob calling you.").is_ok());
    drop(ui);
    // Two 64 px pills 16 apart, centred at 340: Decline left, Accept right.
    let declined = pressed(&vm, BAR, Point::new(340.0 - 40.0, 125.0));
    let accepted = pressed(&vm, BAR, Point::new(340.0 + 40.0, 125.0));
    assert_eq!(
        declined,
        [Intent::Answer {
            room: "bob".into(),
            accept: false
        }]
    );
    assert_eq!(
        accepted,
        [Intent::Answer {
            room: "bob".into(),
            accept: true
        }]
    );
}

#[test]
fn a_call_nobody_can_join_asks_nothing() {
    let mut call = samples::call();
    call.can_join = false;
    let vm = Join::Call(call);
    assert!(pressed(&vm, BAR, Point::new(340.0, 125.0)).is_empty());
}

#[test]
fn nothing_to_join_draws_nothing() {
    let vm = Join::None;
    let mut ui = simulate(&vm, BAR);
    assert!(ui.find("Join voice").is_err());
    assert!(ui.find("Join the call").is_err());
}
