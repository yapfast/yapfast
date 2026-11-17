//! The sign-in screen and linking Discord: task N09's tests, driving
//! `anomi_screens::sign_in::view` over its fixtures in the simulator
//! (`super::support`).

use iced::Widget as _;
use iced_test::Simulator;

use anomi_screens::sign_in::{self, verify_banner, view};
use anomi_vm::fixtures::sign_in as sample;
use anomi_vm::sign_in::{Code, Intent, Link, SignIn, Verify};

use crate::support::{SIZE, asked, front, settings};

/// A button by its id.
fn id(name: &'static str) -> impl iced_test::Selector<Output = iced_test::selector::Target> {
    iced_test::selector::id(iced::widget::Id::new(name))
}

/// The screen over a sign-in state, as the window shows it.
fn screen(sign_in: &SignIn) -> Simulator<'_, Intent, anomi_theme::Theme, anomi_theme::Renderer> {
    Simulator::with_size(settings(), SIZE, view(sign_in, front()).boxed())
}

fn state(vm: anomi_vm::Vm) -> SignIn {
    vm.sign_in
}

#[test]
fn linking_discord_starts_from_the_warning() {
    let sign_in = state(sample::idle());
    let mut ui = screen(&sign_in);
    ui.find("Link your Discord account with your phone.")
        .expect("its line");
    ui.find("Discord doesn't allow apps like Yapfast on personal accounts. Linking puts your account at some risk of being suspended.")
        .expect("the warning");
    ui.click(id(sign_in::SHOW_CODE)).expect("the button");
    assert_eq!(asked(ui), [Intent::LinkDiscord]);
}

#[test]
fn the_token_and_the_preview_are_asked_for() {
    let sign_in = state(sample::idle());
    let mut ui = screen(&sign_in);
    ui.click(id(sign_in::USE_TOKEN)).expect("the button");
    ui.click(id(sign_in::PREVIEW)).expect("the button");
    assert_eq!(asked(ui), [Intent::OpenToken, Intent::ExplorePreview]);
}

#[test]
fn an_open_token_field_links_what_was_pasted() {
    let sign_in = state(sample::token());
    let mut ui = screen(&sign_in);
    assert!(
        ui.find(id(sign_in::USE_TOKEN)).is_err(),
        "its button gave way"
    );
    ui.find("TOKEN").expect("its heading");
    ui.find("Anyone with your token can use your account. Paste it only here, and never share it.")
        .expect("the reminder");
    ui.click(id(sign_in::LINK)).expect("the button");
    assert_eq!(asked(ui), [Intent::LinkToken("mfa.sample-token".into())]);
}

#[test]
fn link_waits_for_a_token() {
    let sign_in = SignIn {
        token_open: true,
        ..SignIn::default()
    };
    let mut ui = screen(&sign_in);
    // A disabled button is not in the tree's focus order, nor does it press.
    assert!(ui.find(id(sign_in::LINK)).is_err());
    assert_eq!(asked(ui), []);
}

#[test]
fn typing_a_token_asks_for_each_edit() {
    let sign_in = SignIn {
        token_open: true,
        ..SignIn::default()
    };
    let mut ui = screen(&sign_in);
    ui.click("Discord token").expect("the field");
    ui.typewrite("ab");
    assert_eq!(
        asked(ui),
        [Intent::Token("a".into()), Intent::Token("ab".into())]
    );
}

#[test]
fn a_failure_says_why_and_offers_another_go() {
    let sign_in = state(sample::failed());
    let mut ui = screen(&sign_in);
    ui.find("Linking failed. Try again.").expect("the reason");
    ui.click(id(sign_in::SHOW_CODE)).expect("the button");
    assert_eq!(asked(ui), [Intent::LinkDiscord]);
}

#[test]
fn a_failure_with_the_token_open_still_has_its_button() {
    let sign_in = SignIn {
        token_open: true,
        link: Link::Failed("no".into()),
        ..SignIn::default()
    };
    let ui = screen(&sign_in);
    assert!(
        { ui }.find(id(sign_in::SHOW_CODE)).is_ok(),
        "the button's own row stays"
    );
}

#[test]
fn the_code_and_who_scanned_it_show_in_turn() {
    let sign_in = state(sample::starting());
    assert!(screen(&sign_in).find("Getting a code...").is_ok());
    let sign_in = state(sample::code());
    let mut ui = screen(&sign_in);
    ui.find("Scan it with the Discord app").expect("its title");
    assert!(
        ui.find(id(sign_in::SHOW_CODE)).is_err(),
        "the warning is gone"
    );
    let sign_in = state(sample::scanned());
    let mut ui = screen(&sign_in);
    ui.find("Scanned").expect("the check's label");
    ui.find("Confirm on your phone").expect("its title");
    ui.find("Linking Alice. Tap Log in on your phone to finish.")
        .expect("whom it links");
}

#[test]
fn the_captcha_can_be_opened_again() {
    let sign_in = state(sample::captcha());
    let mut ui = screen(&sign_in);
    ui.find("Solve the captcha in your browser").expect("title");
    ui.click(id(sign_in::REOPEN)).expect("the button");
    assert_eq!(asked(ui), [Intent::OpenCaptcha]);
}

#[test]
fn checking_a_token_shows_the_panel_s_line_and_keeps_the_preview() {
    let sign_in = state(sample::checking());
    let mut ui = screen(&sign_in);
    ui.find("Checking the token...").expect("the panel's line");
    ui.click(id(sign_in::PREVIEW)).expect("the preview is on");
    assert_eq!(asked(ui), [Intent::ExplorePreview]);
}

#[test]
fn connecting_shows_the_app_s_line_and_turns_the_preview_off() {
    let sign_in = state(sample::connecting());
    let mut ui = screen(&sign_in);
    ui.find("Connecting to Discord...").expect("the notice");
    assert!(ui.find(id(sign_in::PREVIEW)).is_err(), "busy: it is off");
    assert_eq!(asked(ui), []);
}

#[test]
fn the_notice_is_red_beside_the_start_and_absent_beside_a_code() {
    let sign_in = SignIn {
        notice: "Could not reach Discord.".into(),
        ..SignIn::default()
    };
    assert!(screen(&sign_in).find("Could not reach Discord.").is_ok());
    let sign_in = SignIn {
        notice: "Could not reach Discord.".into(),
        link: Link::Code(Code::new("https://discord.com/ra/x")),
        ..SignIn::default()
    };
    assert!(screen(&sign_in).find("Could not reach Discord.").is_err());
}

#[test]
fn the_graphics_notice_can_be_dismissed() {
    let sign_in = state(sample::graphics_notice());
    let mut ui = screen(&sign_in);
    ui.click(iced_test::selector::id("banner-dismiss"))
        .expect("its button");
    assert_eq!(asked(ui), [Intent::DismissNotice]);
}

#[test]
fn a_code_equals_one_for_the_same_address() {
    let a = Code::new("https://discord.com/ra/x").expect("a code");
    let b = Code::new("https://discord.com/ra/x").expect("a code");
    assert_eq!(a, b);
    assert_ne!(Some(a), Code::new("https://discord.com/ra/y"));
}

#[test]
fn matrix_signs_in_by_password() {
    let sign_in = state(sample::matrix());
    let mut ui = screen(&sign_in);
    ui.find("Use your existing Matrix account.").expect("line");
    assert!(ui.find(id(sign_in::RETRY)).is_err());
    ui.click(id(sign_in::SIGN_IN)).expect("the button");
    assert_eq!(asked(ui), [Intent::Login]);
}

#[test]
fn matrix_fields_ask_for_each_edit() {
    let sign_in = SignIn {
        matrix_build: true,
        ..SignIn::default()
    };
    let mut ui = screen(&sign_in);
    ui.click(id(sign_in::HOMESERVER)).expect("the field");
    ui.typewrite("m");
    ui.click("@you:matrix.org").expect("the field");
    ui.typewrite("a");
    ui.click(id(sign_in::PASSWORD)).expect("the field");
    ui.typewrite("p");
    assert_eq!(
        asked(ui),
        [
            Intent::Homeserver("m".into()),
            Intent::Username("a".into()),
            Intent::Password("p".into()),
        ]
    );
}

#[test]
fn enter_in_a_field_submits() {
    let enter = iced::keyboard::Key::Named(iced::keyboard::key::Named::Enter);
    let sign_in = SignIn {
        matrix_build: true,
        ..SignIn::default()
    };
    let mut ui = screen(&sign_in);
    ui.click(id(sign_in::PASSWORD)).expect("the field");
    ui.tap_key(enter.clone());
    assert_eq!(asked(ui), [Intent::Login]);

    let sign_in = state(sample::token());
    let mut ui = screen(&sign_in);
    ui.click(id(sign_in::TOKEN)).expect("the field");
    ui.tap_key(enter.clone());
    assert_eq!(asked(ui), [Intent::LinkToken("mfa.sample-token".into())]);

    let verify = Verify {
        unverified: true,
        busy: false,
        recovery: String::new(),
    };
    let mut ui = Simulator::with_size(settings(), SIZE, verify_banner(&verify, front()).boxed());
    ui.click("Recovery key").expect("the field");
    ui.tap_key(enter);
    assert_eq!(asked(ui), [Intent::Recover]);
}

#[test]
fn typing_a_recovery_key_asks_for_each_edit() {
    let verify = Verify {
        unverified: true,
        ..Verify::default()
    };
    let mut ui = Simulator::with_size(settings(), SIZE, verify_banner(&verify, front()).boxed());
    ui.click("Recovery key").expect("the field");
    ui.typewrite("ab");
    assert_eq!(
        asked(ui),
        [Intent::Recovery("a".into()), Intent::Recovery("ab".into())]
    );
}

#[test]
fn matrix_refused_keeps_the_fields_and_offers_the_saved_session() {
    let sign_in = state(sample::matrix_refused());
    let mut ui = screen(&sign_in);
    ui.find("Wrong user name or password.").expect("the notice");
    ui.click(id(sign_in::RETRY)).expect("the button");
    assert_eq!(asked(ui), [Intent::Restore]);
}

#[test]
fn matrix_busy_turns_everything_off() {
    let sign_in = state(sample::matrix_busy());
    let mut ui = screen(&sign_in);
    assert!(ui.find(id(sign_in::SIGN_IN)).is_err());
    assert!(ui.find(id(sign_in::PREVIEW)).is_err());
    assert_eq!(asked(ui), []);
}

#[test]
fn the_verify_banner_asks_to_recover_or_check_again() {
    let verify = Verify {
        unverified: true,
        busy: false,
        recovery: "EsTc 1234".into(),
    };
    let mut ui = Simulator::with_size(settings(), SIZE, verify_banner(&verify, front()).boxed());
    ui.find("Verify this device to send messages")
        .expect("title");
    ui.click(id(sign_in::RECOVER)).expect("the button");
    ui.click(id(sign_in::CHECK_AGAIN)).expect("the button");
    assert_eq!(asked(ui), [Intent::Recover, Intent::RefreshTrust]);
}

#[test]
fn the_verify_banner_is_off_while_busy() {
    let verify = Verify {
        unverified: true,
        busy: true,
        recovery: String::new(),
    };
    let mut ui = Simulator::with_size(settings(), SIZE, verify_banner(&verify, front()).boxed());
    assert!(ui.find(id(sign_in::RECOVER)).is_err());
    assert!(ui.find(id(sign_in::CHECK_AGAIN)).is_err());
    assert_eq!(asked(ui), []);
}

#[test]
fn the_window_shows_sign_in_when_signed_out() {
    let vm = anomi_vm::fixtures::shell::signed_out();
    let mut ui = crate::support::simulate(&vm);
    ui.find("Just looking?").expect("the preview");
}
