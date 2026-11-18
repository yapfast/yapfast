//! Signing in and the two-factor prompt in the window, clicked and typed
//! into as a person would.

use yapfast_core::application::DiscordLink;
use yapfast_core::chat::MfaMethod;

use anomi_screens::sign_in::{LINK, SHOW_CODE, TOKEN, USE_TOKEN};
use anomi_settings::mfa::{CANCEL, CODE, CONFIRM};
use anomi_vm::shell::Layer;

use super::{id, interact, mfa_prompt, shows, start};

#[test]
fn a_folder_without_a_session_shows_sign_in_and_links_with_a_token() {
    let mut window = start("window-sign-in", false);
    let app = &mut window.app;
    assert!(shows(app, "Link your Discord account with your phone."));
    assert!(shows(app, "Explore local preview"));
    interact(app, |ui| {
        ui.click(id(USE_TOKEN)).expect("Use a token");
    });
    assert!(shows(
        app,
        "Anyone with your token can use your account. Paste it only here, and never share it."
    ));
    interact(app, |ui| {
        ui.click(id(TOKEN)).expect("the token's field");
        let _ = ui.typewrite("Bearer abc");
    });
    assert_eq!(app.vm.sign_in.token, "Bearer abc");
    // Link works once something is typed, in the view made after.
    interact(app, |ui| {
        ui.click(id(LINK)).expect("Link");
    });
    if cfg!(feature = "discord") {
        assert_eq!(app.core.state.discord_link, Some(DiscordLink::Checking));
        assert!(shows(app, "Checking the token..."));
    }
}

#[test]
fn the_code_shows_and_try_again_after_a_failure() {
    let mut window = start("window-code", false);
    let app = &mut window.app;
    app.core.state.discord_link = Some(DiscordLink::Code("https://discord.com/ra/x".into()));
    app.refresh();
    assert!(shows(app, "Scan it with the Discord app"));
    app.core.state.discord_link = Some(DiscordLink::Scanned("alice".into()));
    app.refresh();
    assert!(shows(
        app,
        "Linking alice. Tap Log in on your phone to finish."
    ));
    app.core.state.discord_link = Some(DiscordLink::Failed("The code expired.".into()));
    app.refresh();
    assert!(shows(app, "The code expired."));
    assert!(shows(app, "Try again"));
    // Clicking it would reach Discord's login service; the button's there.
    interact(app, |ui| {
        ui.find(id(SHOW_CODE)).expect("Try again");
    });
}

#[test]
fn the_prompt_takes_a_code_and_cancel_fails_it() {
    let mut window = start("window-mfa", true);
    let app = &mut window.app;
    let prompt = mfa_prompt(9, vec![MfaMethod::App], Some(MfaMethod::App));
    app.core.state.mfa.push(prompt);
    app.refresh();
    assert!(shows(
        app,
        "Enter the 6-digit code from your authenticator app."
    ));
    interact(app, |ui| {
        ui.click(id(CODE)).expect("the code's field");
        let _ = ui.typewrite("123456");
    });
    assert_eq!(app.core.state.mfa[0].code, "123456");
    assert!(app.vm.settings.mfa.ready);
    interact(app, |ui| {
        ui.find(id(CONFIRM))
            .expect("Confirm, which may now be pressed");
        ui.click(id(CANCEL)).expect("Cancel");
    });
    assert!(app.core.state.mfa.is_empty());
    assert!(!app.vm.shell.is_open(Layer::Mfa));
}
