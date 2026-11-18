//! What the sign-in and the two-factor prompt ask, handed to the core.

use yapfast_core::application::DiscordLink;
use yapfast_core::chat::MfaMethod;

use anomi_vm::settings::mfa::{Intent, Method, WARNING_WAIT};
use anomi_vm::shell::{self, Layer};
use anomi_vm::sign_in;

use super::{counting, handle, mfa};
use crate::tests::{mfa_prompt, start};

#[test]
fn the_fields_keep_what_was_typed_and_a_token_links() {
    let mut window = start("sign-in-typed", false);
    let app = &mut window.app;
    drop(handle(app, sign_in::Intent::Username("alice".into())));
    drop(handle(app, sign_in::Intent::Password("hunter2".into())));
    drop(handle(app, sign_in::Intent::Password("hunter22".into())));
    assert_eq!(app.vm.sign_in.username, "alice");
    assert_eq!(app.vm.sign_in.password, "hunter22");
    drop(handle(app, sign_in::Intent::OpenToken));
    assert!(app.vm.sign_in.token_open);
    drop(handle(app, sign_in::Intent::LinkToken));
    assert_eq!(app.core.state.discord_link, None, "no token, no linking");
    // Not a token Discord could take: refused before anything is sent.
    drop(handle(app, sign_in::Intent::Token("Bearer abc".into())));
    drop(handle(app, sign_in::Intent::LinkToken));
    if cfg!(feature = "discord") {
        assert_eq!(app.core.state.discord_link, Some(DiscordLink::Checking));
        assert_eq!(app.vm.sign_in.link, sign_in::Link::Checking);
    }
    assert_eq!(
        app.vm.sign_in.token, "Bearer abc",
        "kept while it's checked"
    );
}

#[cfg(feature = "discord")]
#[test]
fn linking_follows_its_latest_run_alone() {
    use yapfast_core::discord_link::Progress;

    use crate::message::Message;

    let mut window = start("sign-in-runs", false);
    let app = &mut window.app;
    app.core.state.discord_link = Some(DiscordLink::Starting);
    let run = app.linking.run();
    let address = "https://discord.com/ra/one";
    drop(app.update(Message::Linking(run, Progress::Code(address.into()))));
    assert!(
        matches!(&app.vm.sign_in.link, sign_in::Link::Code(Some(code)) if code.address() == address)
    );
    drop(app.update(Message::Linking(run, Progress::Scanned("alice".into()))));
    assert_eq!(app.vm.sign_in.link, sign_in::Link::Scanned("alice".into()));
    // Given up: what the stopped run still says is dropped.
    drop(app.dispatch(yapfast_core::application::Action::CancelDiscordLink));
    app.core.state.discord_link = Some(DiscordLink::Starting);
    drop(app.update(Message::Linking(run, Progress::Failed("late".into()))));
    assert_eq!(app.core.state.discord_link, Some(DiscordLink::Starting));
}

#[test]
fn the_graphics_notice_goes_when_put_away() {
    let mut window = start("sign-in-notice-away", false);
    let app = &mut window.app;
    app.vm.sign_in.graphics_notice = Some("GPU rendering failed to start.".into());
    drop(handle(app, sign_in::Intent::DismissNotice));
    assert_eq!(app.vm.sign_in.graphics_notice, None);
    app.refresh();
    assert_eq!(app.vm.sign_in.graphics_notice, None);
}

#[test]
fn the_prompt_writes_chooses_and_counts_down_its_warning() {
    let mut window = start("mfa-intents", false);
    let app = &mut window.app;
    let methods = vec![MfaMethod::App, MfaMethod::Backup];
    app.core
        .state
        .mfa
        .push(mfa_prompt(3, methods, Some(MfaMethod::App)));
    app.refresh();
    drop(mfa(app, Intent::Write("123 456".into())));
    assert_eq!(app.core.state.mfa[0].code, "123 456");
    assert_eq!(app.vm.settings.mfa.code, "123 456");
    assert!(app.vm.settings.mfa.ready);
    drop(mfa(app, Intent::Choose(Method::Backup)));
    let wait = |app: &crate::app::App| app.vm.settings.mfa.wait;
    assert!(app.vm.settings.mfa.warning);
    assert!(app.vm.settings.mfa.code.is_empty(), "a new way, a new code");
    assert_eq!(wait(app), WARNING_WAIT);
    drop(mfa(app, Intent::Tick));
    app.refresh();
    assert_eq!(wait(app), WARNING_WAIT - 1, "a showing leaves the count");
    drop(mfa(app, Intent::Back));
    assert!(app.vm.settings.mfa.listing);
    drop(mfa(app, Intent::Choose(Method::Backup)));
    assert_eq!(wait(app), WARNING_WAIT, "it starts over each time it shows");
    drop(mfa(app, Intent::Confirm));
    assert!(!app.vm.settings.mfa.warning, "past the warning");
    assert!(app.vm.settings.mfa.asks());
}

#[test]
fn the_warning_counts_only_in_front() {
    let mut window = start("mfa-ticks", false);
    let app = &mut window.app;
    let second = |app: &mut crate::app::App| {
        drop(crate::clocks::arm(app));
        app.clocks.running()[2]
    };
    let methods = vec![MfaMethod::Backup];
    app.core
        .state
        .mfa
        .push(mfa_prompt(4, methods, Some(MfaMethod::Backup)));
    app.refresh();
    assert!(counting(app));
    assert!(second(app));
    app.motion.front = false;
    assert!(!second(app), "behind another window");
    app.motion.front = true;
    app.vm.settings.mfa.wait = 0;
    assert!(!second(app), "the wait is over");
}

#[test]
fn escape_or_a_press_beside_the_prompt_fails_its_request() {
    let mut window = start("mfa-cancel", false);
    let app = &mut window.app;
    let prompt = |id| mfa_prompt(id, vec![MfaMethod::App], Some(MfaMethod::App));
    app.core.state.mfa.extend([prompt(5), prompt(6)]);
    app.refresh();
    let close = anomi_vm::Intent::Shell(shell::Intent::Close(Layer::Mfa));
    drop(crate::intent::handle(app, close.clone()));
    assert_eq!(app.vm.settings.mfa.id, 6, "the next one shows");
    assert!(app.vm.shell.is_open(Layer::Mfa));
    drop(mfa(app, Intent::Cancel));
    assert!(app.core.state.mfa.is_empty());
    assert!(!app.vm.shell.is_open(Layer::Mfa));
}
