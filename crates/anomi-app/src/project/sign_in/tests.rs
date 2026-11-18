//! The sign-in, linking and the two-factor prompt from the core's state.

use yapfast_core::application::DiscordLink;
use yapfast_core::chat::MfaMethod;

use anomi_vm::shell::{self, Layer};
use anomi_vm::sign_in::Link;

use crate::tests::{mfa_prompt, start};

#[test]
fn linking_shows_each_phase_and_makes_the_code_once() {
    let mut window = start("sign-in-phases", false);
    let app = &mut window.app;
    assert!(app.vm.shell.signed_out);
    assert_eq!(app.vm.sign_in.link, Link::Idle);
    let address = "https://discord.com/ra/fingerprint";
    app.core.state.discord_link = Some(DiscordLink::Code(address.into()));
    app.refresh();
    let Link::Code(Some(code)) = &app.vm.sign_in.link else {
        panic!("the code, not {:?}", app.vm.sign_in.link);
    };
    assert_eq!(code.address(), address);
    let first: *const _ = code.data();
    app.refresh();
    let Link::Code(Some(code)) = &app.vm.sign_in.link else {
        panic!("still the code");
    };
    assert!(
        std::ptr::eq(first, code.data()),
        "made once for its address"
    );
    app.core.state.discord_link = Some(DiscordLink::Scanned("alice".into()));
    app.refresh();
    assert_eq!(app.vm.sign_in.link, Link::Scanned("alice".into()));
    app.core.state.discord_link = Some(DiscordLink::Failed("Expired.".into()));
    app.refresh();
    assert_eq!(app.vm.sign_in.link, Link::Failed("Expired.".into()));
    app.core.state.discord_link = Some(DiscordLink::Captcha("https://example.org".into()));
    app.refresh();
    assert_eq!(app.vm.sign_in.link, Link::Captcha);
    assert!(
        !app.vm.shell.is_open(Layer::Link),
        "signed out, the sign-in screen shows it"
    );
}

#[test]
fn the_token_stays_until_linked_then_goes() {
    let mut window = start("sign-in-token", false);
    let app = &mut window.app;
    app.vm.sign_in.token = "mfa.secret".into();
    app.vm.sign_in.token_open = true;
    app.core.state.discord_link = Some(DiscordLink::Failed("Refused.".into()));
    app.refresh();
    assert_eq!(
        app.vm.sign_in.token, "mfa.secret",
        "kept to fix a refused one"
    );
    app.core.state.discord_link = None;
    app.core.state.discord_linked = true;
    app.refresh();
    assert!(app.vm.sign_in.token.is_empty());
    assert!(!app.vm.sign_in.token_open);
}

#[test]
fn the_screen_says_what_the_core_says() {
    let mut window = start("sign-in-notice", false);
    let app = &mut window.app;
    assert_eq!(app.vm.sign_in.product, yapfast_lang::PRODUCT);
    app.core.state.notice = Some("No saved session.".into());
    app.core.state.busy = true;
    app.refresh();
    assert_eq!(app.vm.sign_in.notice, "No saved session.");
    assert!(app.vm.sign_in.busy && app.vm.sign_in.verify.busy);
    assert_eq!(app.vm.sign_in.matrix_build, cfg!(feature = "matrix"));
}

#[test]
fn an_unverified_device_shows_the_banner_but_the_preview_never() {
    let mut window = start("sign-in-verify", false);
    let app = &mut window.app;
    app.core.state.account = Some("@alice:example.org".into());
    app.core.state.trusted = false;
    app.refresh();
    assert!(app.vm.sign_in.verify.unverified);
    app.core.state.trusted = true;
    app.refresh();
    assert!(!app.vm.sign_in.verify.unverified);
    let preview = start("sign-in-verify-preview", true);
    assert!(!preview.app.vm.sign_in.verify.unverified);
}

#[test]
fn linking_signed_in_shows_over_the_settings_and_stops_with_them() {
    let mut window = start("sign-in-settings", false);
    let app = &mut window.app;
    app.core.state.account = Some("@alice:example.org".into());
    app.vm.shell.open(Layer::Settings);
    app.core.state.discord_link = Some(DiscordLink::Starting);
    app.refresh();
    assert!(app.vm.shell.is_open(Layer::Link));
    assert_eq!(app.vm.sign_in.link, Link::Starting);
    let close = |layer| anomi_vm::Intent::Shell(shell::Intent::Close(layer));
    drop(crate::intent::handle(app, close(Layer::Settings)));
    assert_eq!(app.core.state.discord_link, None, "it stopped");
    assert!(!app.vm.shell.is_open(Layer::Link));
    assert!(!app.vm.shell.is_open(Layer::Settings));
    // Its own X stops it too.
    app.vm.shell.open(Layer::Settings);
    app.core.state.discord_link = Some(DiscordLink::Failed("No.".into()));
    app.refresh();
    assert!(app.vm.shell.is_open(Layer::Link));
    drop(crate::intent::handle(app, close(Layer::Link)));
    assert_eq!(app.core.state.discord_link, None);
    assert!(!app.vm.shell.is_open(Layer::Link));
    assert!(app.vm.shell.is_open(Layer::Settings), "the settings stay");
}

#[test]
fn a_held_request_opens_the_prompt_and_its_field_takes_the_keys_once() {
    let mut window = start("mfa-prompt", false);
    let app = &mut window.app;
    let methods = vec![MfaMethod::App, MfaMethod::Backup, MfaMethod::Passkey];
    app.core
        .state
        .mfa
        .push(mfa_prompt(7, methods, Some(MfaMethod::App)));
    app.refresh();
    assert!(app.vm.shell.is_open(Layer::Mfa));
    let mfa = &app.vm.settings.mfa;
    assert_eq!(mfa.id, 7);
    assert!(!mfa.listing && !mfa.warning && mfa.asks());
    assert_eq!(
        mfa.text,
        "Enter the 6-digit code from your authenticator app."
    );
    assert_eq!(mfa.field, "6-digit code");
    let ways: Vec<&str> = mfa.choices.iter().map(|row| row.text.as_str()).collect();
    assert_eq!(ways, ["Use your authenticator app", "Use a backup code"]);
    assert_eq!(app.shown.focus, Some(anomi_settings::mfa::CODE));
    app.shown.focus = None;
    app.refresh();
    assert_eq!(app.shown.focus, None, "as it opens, not on each showing");
    app.core.state.mfa[0].method = None;
    app.refresh();
    assert!(app.vm.settings.mfa.listing);
    app.core.state.mfa.clear();
    app.refresh();
    assert!(!app.vm.shell.is_open(Layer::Mfa));
}

#[test]
fn a_text_says_where_it_went_and_may_go_again() {
    let mut window = start("mfa-text", false);
    let app = &mut window.app;
    let mut prompt = mfa_prompt(2, vec![MfaMethod::Sms], Some(MfaMethod::Sms));
    prompt.phone = Some("*******1234".into());
    prompt.error = Some("Invalid code.".into());
    app.core.state.mfa.push(prompt);
    app.refresh();
    let mfa = &app.vm.settings.mfa;
    assert_eq!(
        mfa.text,
        "Enter the 6-digit code Discord sent to *******1234."
    );
    assert!(mfa.resend && !mfa.password);
    assert_eq!(mfa.error, "Invalid code.");
}
