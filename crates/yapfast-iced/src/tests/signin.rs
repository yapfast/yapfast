//! Sign-in: the screen without an account, linking Discord by code or
//! token, two-factor prompts and a saved account restoring.

use iced::{
    keyboard::{self, key},
    widget::Id,
};
use yapfast_core::chat::{Event, MfaMethod};

use super::{a_picture_shown, change, interact, local, preview, shows, start};
#[cfg(not(feature = "matrix"))]
use crate::app::Message;
use crate::{mfa, signin};

#[test]
fn without_an_account_the_sign_in_screen_shows_and_the_preview_leaves_it() {
    let folder = tempfile::tempdir_in(local()).expect("test folder");
    let mut app = start(&folder, false);
    assert!(!crate::signin::workspace(&app));
    assert!(shows(&app, "Sign in to Yapfast"));
    if signin::MATRIX {
        assert!(shows(&app, "Use your existing Matrix account."));
    } else {
        assert!(shows(&app, "Show the code"));
        interact(&mut app, |ui| {
            ui.click("Use a token").expect("Use a token");
        });
        interact(&mut app, |ui| {
            ui.click(Id::new(signin::TOKEN)).expect("the token's field");
            ui.typewrite("a-token");
        });
        assert_eq!(app.sign_in.token, "a-token");
        assert!(shows(&app, "Link"));
        // Neither a log line of what's typed nor of the screen tells it.
        let typed = signin::Event::Token("a-token".into());
        assert!(!format!("{typed:?}").contains("a-token"));
        assert!(!format!("{:?}", app.sign_in).contains("a-token"));
    }
    interact(&mut app, |ui| {
        ui.click("Explore local preview").expect("the preview");
    });
    assert!(crate::signin::workspace(&app));
    assert!(shows(&app, "general"));
}

#[test]
#[cfg(not(feature = "matrix"))]
fn says_a_pasted_token_can_sign_its_app_out_and_that_unlinking_left_it() {
    let folder = tempfile::tempdir_in(local()).expect("test folder");
    let mut app = start(&folder, false);
    let note = "Pasting a token from another app can make Discord sign that app out. \
                The QR code is the safe way to link.";
    let kept = "Yapfast forgot your login on this computer. If you pasted a token, \
                the app or browser you copied it from is still signed in.";
    assert!(shows(&app, note));
    assert!(!shows(&app, kept));
    // Signed out of an account linked with a pasted token.
    app.client.apply(Event::LoggedOut);
    app.client.apply(Event::DiscordSessionKept);
    let _ = app.sync();
    assert!(shows(&app, kept));
}

#[test]
#[cfg(not(feature = "matrix"))]
fn a_pasted_token_goes_once_the_account_is_linked() {
    let folder = tempfile::tempdir_in(local()).expect("test folder");
    let mut app = start(&folder, false);
    let _ = app.update(Message::SignIn(signin::Event::OpenToken));
    let _ = app.update(Message::SignIn(signin::Event::Token("a-token".into())));
    // Refused, it stays to fix.
    let _ = app.sync();
    assert_eq!(app.sign_in.token, "a-token");
    // Linked, it goes, and its field with it.
    app.client.app.state.account = Some("1".into());
    let _ = app.sync();
    assert!(app.sign_in.token.is_empty());
    assert!(!app.sign_in.token_open);
}

#[test]
#[cfg(not(feature = "matrix"))]
fn linking_shows_the_code_then_who_scanned_it() {
    use yapfast_core::{application::DiscordLink, discord_link::Progress};
    let folder = tempfile::tempdir_in(local()).expect("test folder");
    let mut app = start(&folder, false);
    // As "Show the code" starts it, without reaching Discord.
    app.client.app.state.discord_link = Some(DiscordLink::Starting);
    let _ = app.sync();
    assert!(shows(&app, "Getting a code..."));
    let _ = app.update(Message::Linking(Progress::Code(
        "https://discord.com/ra/a-code".into(),
    )));
    assert!(app.sign_in.code.is_some(), "the QR code drawn");
    assert!(shows(&app, "Scan it with the Discord app"));
    let _ = app.update(Message::Linking(Progress::Scanned("riley".into())));
    assert!(app.sign_in.code.is_none(), "the code let go");
    assert!(shows(&app, "Scanned"));
    assert!(shows(&app, "Confirm on your phone"));
    let _ = app.update(Message::Linking(Progress::Failed("It broke.".into())));
    assert!(shows(&app, "It broke."));
    assert!(shows(&app, "Try again"));
}

#[test]
fn a_held_request_asks_for_a_code_and_cancelling_closes_the_prompt() {
    let (mut app, _folder) = preview();
    app.client.apply(Event::MfaRequired {
        id: 7,
        methods: vec![MfaMethod::App, MfaMethod::Backup],
    });
    let _ = app.sync();
    assert!(shows(&app, "Multi-Factor Authentication"));
    assert!(shows(
        &app,
        "Enter the 6-digit code from your authenticator app."
    ));
    interact(&mut app, |ui| {
        ui.click(Id::new(mfa::CODE)).expect("the code's field");
        ui.typewrite("123456");
    });
    // Enter sends once the code is whole, in the view built after it.
    interact(&mut app, |ui| {
        ui.click(Id::new(mfa::CODE)).expect("the code's field");
        ui.tap_key(keyboard::Key::Named(key::Named::Enter));
    });
    let prompt = app.client.app.mfa_prompt().expect("the prompt");
    assert_eq!(prompt.code, "123456");
    assert!(prompt.busy, "confirming");
    assert!(shows(&app, "Confirming..."));
    // Discord turned it down: the prompt stays for another try.
    app.client.apply(Event::MfaFinished {
        id: 7,
        result: Err("Invalid two-factor code".into()),
    });
    let _ = app.sync();
    assert!(shows(&app, "Invalid two-factor code"));
    // The other way lists the methods, and a backup code warns first.
    interact(&mut app, |ui| {
        ui.click("Verify with something else")
            .expect("the other way");
    });
    interact(&mut app, |ui| {
        ui.click("Use a backup code").expect("a backup code");
    });
    assert!(shows(&app, "Slow down! Are you sure?"));
    assert_eq!(app.mfa.wait.left, 10, "the button waits");
    interact(&mut app, |ui| {
        ui.click("Cancel").expect("Cancel");
    });
    assert!(app.client.app.mfa_prompt().is_none());
    assert!(!shows(&app, "Multi-Factor Authentication"));
}

#[test]
fn a_saved_account_restoring_at_start_shows_only_the_notice_until_it_fails() {
    let folder = tempfile::tempdir_in(local()).expect("test folder");
    let mut app = start(&folder, false);
    // As a start on a saved account leaves the core, without reaching it.
    let state = &mut app.client.app.state;
    state.busy = true;
    state.notice = Some("Connecting to Discord...".into());
    app.sign_in.starting = true;
    let _ = app.sync();
    assert!(shows(&app, "Connecting to Discord..."));
    assert!(!shows(&app, "Sign in to Yapfast"));
    let state = &mut app.client.app.state;
    state.busy = false;
    state.notice = Some("Discord said no.".into());
    let _ = app.sync();
    assert!(!app.sign_in.starting);
    assert!(shows(&app, "Sign in to Yapfast"));
}

#[test]
fn the_sign_in_screen_lets_go_of_the_timelines_pictures() {
    let (mut app, _folder) = preview();
    let _picture = a_picture_shown(&mut app);
    change(&mut app, |state| state.preview = false);
    assert!(!crate::signin::workspace(&app));
    drop(app.view());
    assert!(app.win.pictures.fitter.borrow().is_empty(), "the copy went");
}

#[test]
fn the_sign_in_screen_says_the_gpu_failed_until_dismissed() {
    let folder = tempfile::tempdir_in(local()).expect("test folder");
    let mut app = start(&folder, false);
    assert!(!shows(&app, crate::graphics::NOTICE));
    app.shell.gpu_notice = true;
    assert!(shows(&app, crate::graphics::NOTICE));
    assert!(shows(&app, "Sign in to Yapfast"), "over the card");
    interact(&mut app, |ui| {
        ui.click("Dismiss").expect("Dismiss");
    });
    assert!(!shows(&app, crate::graphics::NOTICE));
}

#[test]
fn back_on_the_sign_in_screen_the_matrix_id_takes_the_keys() {
    let (mut app, _folder) = preview();
    app.client
        .dispatch(yapfast_core::application::Action::Logout);
    assert!(!crate::signin::workspace(&app), "the preview is left");
    // Its field is focused once, as the screen comes back.
    let focus = app.sign_in.sync(app.client.state(), app.client.root());
    assert_eq!(focus.units(), usize::from(signin::MATRIX));
    let again = app.sign_in.sync(app.client.state(), app.client.root());
    assert_eq!(again.units(), 0);
}
