//! Switch Accounts in the account's popover and Manage Accounts: choosing
//! the account in use, logging one out, and linking another beside them.

use iced::keyboard::{Key, key};
use yapfast_core::chat::{Command, DiscordAccount, DiscordCommand, Event};

use super::{change, interact, preview, shows};
use crate::{
    accounts::Event as AccountsEvent,
    app::{App, Message},
    status::Event as StatusEvent,
};

fn account(id: &str, name: &str) -> DiscordAccount {
    DiscordAccount {
        id: id.into(),
        username: name.into(),
        name: name.into(),
        avatar: None,
    }
}

/// Two accounts linked, the first in use.
fn two(app: &mut App) {
    change(app, |state| state.discord_user = Some("1".into()));
    app.client.apply(Event::DiscordAccounts {
        accounts: vec![account("1", "alex_main"), account("2", "alex_alt")],
        active: "1".into(),
    });
    let _ = app.sync();
}

fn escape(app: &mut App) {
    let _ = app.update(Message::Key(
        Key::Named(key::Named::Escape),
        Default::default(),
    ));
}

/// What the core sent its backend about accounts.
fn sent(app: &App) -> Vec<DiscordCommand> {
    app.client
        .sent
        .iter()
        .filter_map(|command| match command {
            Command::Discord(
                command @ (DiscordCommand::SwitchAccount(_) | DiscordCommand::RemoveAccount(_)),
            ) => Some(command.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn switch_accounts_lists_the_accounts_and_switches_to_one() {
    let (mut app, _folder) = preview();
    let _ = app.update(Message::Status(StatusEvent::Toggle));
    assert!(!shows(&app, "Switch Accounts"), "none linked");
    two(&mut app);
    assert!(shows(&app, "Switch Accounts"));
    assert!(!shows(&app, "Manage Accounts"), "the list is closed");
    // The pointer on the row opens the list beside the card.
    let _ = app.update(Message::Status(StatusEvent::Accounts));
    assert!(shows(&app, "alex_main") && shows(&app, "alex_alt"));
    assert!(shows(&app, "Manage Accounts"));
    interact(&mut app, |ui| {
        ui.click("alex_alt").expect("the other account");
    });
    assert!(app.status.popover.is_none(), "the popover goes");
    assert!(matches!(sent(&app).as_slice(), [DiscordCommand::SwitchAccount(id)] if id == "2"));
}

#[test]
fn manage_accounts_opens_from_the_list_and_logs_an_account_out() {
    let (mut app, _folder) = preview();
    two(&mut app);
    let _ = app.update(Message::Status(StatusEvent::Toggle));
    let _ = app.update(Message::Status(StatusEvent::Accounts));
    interact(&mut app, |ui| {
        ui.click("Manage Accounts").expect("Manage Accounts");
    });
    assert!(app.status.popover.is_none(), "the popover goes");
    assert!(app.client.state().manage_accounts.is_some());
    assert!(shows(&app, "Manage Accounts") && shows(&app, "Add Account"));
    assert!(shows(&app, "Active Account"), "the one in use says so");
    // The account in use has Log Out only; the other can be switched to.
    let _ = app.update(Message::Accounts(AccountsEvent::Menu("1".into())));
    assert!(shows(&app, "Log Out") && !shows(&app, "Switch Account"));
    let _ = app.update(Message::Accounts(AccountsEvent::Menu("2".into())));
    assert!(shows(&app, "Log Out") && shows(&app, "Switch Account"));
    // Escape closes the menu, then the dialog.
    escape(&mut app);
    assert!(!shows(&app, "Log Out"));
    assert!(app.client.state().manage_accounts.is_some());
    let _ = app.update(Message::Accounts(AccountsEvent::Menu("2".into())));
    interact(&mut app, |ui| {
        ui.click("Log Out").expect("Log Out");
    });
    assert!(matches!(sent(&app).as_slice(), [DiscordCommand::RemoveAccount(id)] if id == "2"));
    assert!(!shows(&app, "Log Out"), "the menu closes");
    escape(&mut app);
    assert!(app.client.state().manage_accounts.is_none());
    assert!(!shows(&app, "Manage Accounts"));
}

#[test]
#[cfg(not(feature = "matrix"))]
fn add_account_links_another_in_the_dialog_and_back_leaves_it() {
    use yapfast_core::discord_link::Progress;
    let folder = tempfile::tempdir_in(super::local()).expect("test folder");
    let mut app = super::start(&folder, false);
    app.client.app.state.account = Some("1".into());
    app.client.app.state.discord_linked = true;
    two(&mut app);
    app.client
        .dispatch(yapfast_core::application::Action::OpenManageAccounts);
    let _ = app.sync();
    assert!(shows(&app, "Manage Accounts"));
    interact(&mut app, |ui| {
        ui.click("Add Account").expect("Add Account");
    });
    // The linking steps take the dialog's place.
    assert!(shows(
        &app,
        "Link another Discord account to switch between them."
    ));
    assert!(shows(&app, "Show the code") && shows(&app, "Back"));
    assert!(!shows(&app, "alex_main"));
    // A code shows as it does on the sign-in screen, though an account is in
    // use, and a token pasted stays to fix.
    let _ = app.update(Message::SignIn(crate::signin::Event::OpenToken));
    let _ = app.update(Message::SignIn(crate::signin::Event::Token(
        "a-token".into(),
    )));
    app.client.app.state.discord_link = Some(yapfast_core::application::DiscordLink::Starting);
    let _ = app.sync();
    let _ = app.update(Message::Linking(Progress::Code(
        "https://discord.com/ra/a-code".into(),
    )));
    assert!(app.sign_in.code.is_some(), "the QR code drawn");
    assert!(shows(&app, "Scan it with the Discord app"));
    assert_eq!(app.sign_in.token, "a-token");
    // Back stops the linking, forgets the token and lists the accounts.
    interact(&mut app, |ui| {
        ui.click("Back").expect("Back");
    });
    assert!(app.client.state().discord_link.is_none());
    assert!(app.sign_in.token.is_empty() && !app.sign_in.token_open);
    assert!(app.sign_in.code.is_none(), "the code let go");
    assert!(shows(&app, "alex_main") && shows(&app, "Add Account"));
    assert!(app.client.state().manage_accounts.is_some());
    // Escape in the step goes back first, then closes.
    let _ = app.update(Message::Accounts(AccountsEvent::Add));
    assert!(shows(&app, "Show the code"));
    escape(&mut app);
    assert!(shows(&app, "alex_main"), "back to the accounts");
    escape(&mut app);
    assert!(app.client.state().manage_accounts.is_none());
}

#[test]
fn switching_account_closes_manage_accounts() {
    let (mut app, _folder) = preview();
    two(&mut app);
    app.client
        .dispatch(yapfast_core::application::Action::OpenManageAccounts);
    let _ = app.sync();
    assert!(shows(&app, "Manage Accounts"));
    app.client.apply(Event::DiscordSwitched);
    let _ = app.sync();
    assert!(!shows(&app, "Manage Accounts"));
}
