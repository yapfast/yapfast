//! The Messages settings page: task S04's tests, driving
//! `anomi_settings::messages::view` over its fixtures in the simulator
//! (`super::support`).

use iced_test::Simulator;

use anomi_settings::messages::view;
use anomi_vm::fixtures::settings::messages::{compact, page};
use anomi_vm::settings::messages::{Animate, DateFormat, Intent, Messages, TextSize};

use super::support::{SIZE, asked, front, settings};

fn click(page: &Messages, text: &str) -> Vec<Intent> {
    let mut ui = Simulator::with_size(settings(), SIZE, view(page, front()));
    ui.click(text).unwrap_or_else(|_| panic!("{text}"));
    asked(ui)
}

#[test]
fn display_and_size_report_the_choice() {
    assert_eq!(click(&page(), "Compact"), vec![Intent::Compact(true)]);
    assert_eq!(click(&compact(), "Cozy"), vec![Intent::Compact(false)]);
    assert_eq!(
        click(&page(), "Large"),
        vec![Intent::TextSize(TextSize::Large)]
    );
}

#[test]
fn clock_and_date_report_the_choice() {
    assert_eq!(click(&page(), "12-hour"), vec![Intent::TwelveHour(true)]);
    assert_eq!(
        click(&page(), "Year-Month-Day"),
        vec![Intent::DateFormat(DateFormat::YearFirst)]
    );
    assert_eq!(
        click(&page(), "Never"),
        vec![Intent::Animate(Animate::Never)]
    );
}

#[test]
fn switches_report_the_flip() {
    assert_eq!(click(&page(), "Show seconds"), vec![Intent::Seconds(true)]);
    assert_eq!(
        click(&page(), "Show link previews"),
        vec![Intent::LinkPreviews(false)]
    );
    assert_eq!(
        click(&page(), "Collapse long messages"),
        vec![Intent::CollapseLong(true)]
    );
}

#[test]
fn the_other_switches_report_the_flip() {
    let off = Messages {
        boost_gems: false,
        share_typing: false,
        reply_ping: false,
        strip_tracking: false,
        ..page()
    };
    assert_eq!(
        click(&off, "Show boost gems"),
        vec![Intent::BoostGems(true)]
    );
    assert_eq!(
        click(&page(), "Let others see when you're typing"),
        vec![Intent::ShareTyping(false)]
    );
    assert_eq!(
        click(&off, "Mention people you reply to"),
        vec![Intent::ReplyPing(true)]
    );
    assert_eq!(
        click(&page(), "Remove tracking from links you send"),
        vec![Intent::StripTracking(false)]
    );
}

#[test]
fn the_typing_switch_names_the_networks() {
    let both = compact();
    let mut ui = Simulator::with_size(settings(), SIZE, view(&both, front()));
    assert!(ui.find("Discord and Matrix show \"typing...\" while you write. Others still show here when they type.").is_ok());
    let matrix = Messages {
        discord: false,
        ..compact()
    };
    let mut ui = Simulator::with_size(settings(), SIZE, view(&matrix, front()));
    assert!(
        ui.find(
            "Matrix shows \"typing...\" while you write. Others still show here when they type."
        )
        .is_ok()
    );
}

#[test]
fn the_date_follows_the_clock_until_chosen() {
    let twelve = Messages {
        twelve_hour: true,
        ..page()
    };
    assert_eq!(twelve.date_shown(), DateFormat::MonthFirst);
    assert_eq!(page().date_shown(), DateFormat::DayFirst);
    let chosen = Messages {
        date_format: Some(DateFormat::YearFirst),
        ..twelve
    };
    assert_eq!(chosen.date_shown(), DateFormat::YearFirst);
}

#[test]
fn the_sample_follows_the_choices() {
    let compact = compact();
    let mut ui = Simulator::with_size(settings(), SIZE, view(&compact, front()));
    assert!(ui.find("9:41:05").is_ok());
    let page = page();
    let mut ui = Simulator::with_size(settings(), SIZE, view(&page, front()));
    assert!(ui.find("21:41").is_ok());
}
