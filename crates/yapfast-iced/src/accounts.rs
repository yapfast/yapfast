//! Manage Accounts: the dialog that lists the Discord accounts linked on
//! this device, each but the one in use with Switch, each with a menu to
//! log it out, and crate::i18n::tr!(cx.i18n, "Add an account"), which links another beside them with
//! the sign-in screen's steps (`signin::link_panel`), worded as Discord's
//! client words them. The core keeps whether it's open and
//! whose menu is open (`AppState::manage_accounts`); this keeps the Add
//! Account step and draws them.

use iced::{
    Alignment, Element, Length, Padding, Task,
    widget::{column, container, row, text},
};
use yapfast_core::{
    application::{Action, AppState},
    chat::DiscordAccount,
};

use crate::{
    app::{App, Message},
    controls::{self, Check, Item, Kind, Light, action},
    ctx::Ctx,
    fonts::{BOLD, SEMIBOLD},
    pictures::Icon,
    style::rounded,
};

/// How wide the dialog is, at most.
const WIDTH: f32 = 480.0;
/// An account's card, and the menu that opens under it.
const CARD: f32 = 64.0;
const MENU: f32 = 200.0;

/// What's done in the dialog.
#[derive(Clone, Debug)]
pub enum Event {
    /// Make the account the one in use.
    Switch(String),
    /// Open the account's menu, or close it when it's open; empty closes.
    Menu(String),
    LogOut(String),
    /// Open Add Account's step, and leave it for the accounts again.
    Add,
    Back,
    Close,
}

/// What the window keeps of the dialog: whether Add Account's step shows
/// and how many accounts were linked as it began, to tell another came.
#[derive(Debug, Default)]
pub struct Accounts {
    adding: bool,
    linked: usize,
    /// The dialog was open at the last sync.
    shown: bool,
}

impl Accounts {
    /// Whether the linking steps show in place of the accounts: Add
    /// Account was pressed, or linking is under way.
    fn linking(&self, state: &AppState) -> bool {
        self.adding || state.discord_link.is_some()
    }
}

pub fn update(app: &mut App, event: Event) -> Task<Message> {
    match event {
        Event::Switch(id) => app.client.dispatch(Action::SwitchDiscordAccount(id)),
        Event::Menu(id) => app.client.dispatch(Action::OpenAccountMenu(id)),
        Event::LogOut(id) => app.client.dispatch(Action::LogOutDiscordAccount(id)),
        Event::Add => {
            app.client.dispatch(Action::OpenAccountMenu(String::new()));
            app.accounts.adding = true;
            app.accounts.linked = app.client.state().discord_accounts.len();
        }
        Event::Back => back(app),
        Event::Close => {
            // Closing stops the linking, too.
            app.accounts.adding = false;
            app.sign_in.drop_token();
            app.client.dispatch(Action::CloseManageAccounts);
            return Task::batch([app.sync(), crate::composer::refocus(app)]);
        }
    }
    app.sync()
}

/// Leaves Add Account's step: linking stops, and a token pasted goes.
fn back(app: &mut App) {
    app.accounts.adding = false;
    app.sign_in.drop_token();
    app.client.dispatch(Action::CancelDiscordLink);
}

/// Escape: the account's menu first, then Add Account's step, then the
/// dialog.
pub fn escape(app: &mut App) -> Task<Message> {
    let state = app.client.state();
    let event = if state
        .manage_accounts
        .as_ref()
        .is_some_and(|dialog| dialog.menu.is_some())
    {
        Event::Menu(String::new())
    } else if app.accounts.linking(state) {
        Event::Back
    } else {
        Event::Close
    };
    update(app, event)
}

/// Takes in the core's dialog: as it opens it takes the keys, and Add
/// Account's step ends once another account is linked, as it goes with
/// the dialog.
pub fn sync(app: &mut App) -> Task<Message> {
    let state = app.client.state();
    let open = state.manage_accounts.is_some();
    let linked = state.discord_accounts.len();
    let accounts = &mut app.accounts;
    if open != accounts.shown {
        accounts.shown = open;
        accounts.adding = false;
        return if open {
            iced::widget::operation::focus(crate::app::NOBODY)
        } else {
            Task::none()
        };
    }
    if accounts.adding && linked > accounts.linked {
        accounts.adding = false;
        app.sign_in.drop_token();
    }
    Task::none()
}

/// The dialog over the dimmed window, while the core has it open.
pub fn view<'a>(accounts: &'a Accounts, cx: Ctx<'a>) -> Option<Element<'a, Message>> {
    let manage = cx.state.manage_accounts.as_ref()?;
    let theme = cx.look.theme;
    let linking = accounts.linking(cx.state);
    let mut parts: Vec<Element<'a, Message>> = vec![
        cx.look.dialog_head(
            if linking {
                "Add Account"
            } else {
                "Manage Accounts"
            },
            Message::Accounts(Event::Close),
        ),
        text(if linking {
            "Logging in to another account will let you easily switch between accounts on this device."
        } else {
            "Switch accounts, sign in, sign out, go wild."
        })
        .size(14)
        .color(theme.secondary)
        .width(Length::Fill)
        .into(),
    ];
    if linking {
        parts.push(crate::signin::link_panel(cx));
        parts.push(
            row![action(
                theme,
                crate::i18n::tr!(cx.i18n, "Back"),
                Kind::Plain,
                Some(Message::Accounts(Event::Back))
            )]
            .into(),
        );
    } else {
        let mut list = column![].spacing(8).width(Length::Fill);
        for account in &cx.state.discord_accounts {
            let menu = manage.menu.as_deref() == Some(account.id.as_str());
            list = list.push(card(cx, account, menu));
        }
        parts.push(list.into());
        parts.push(
            action(
                theme,
                crate::i18n::tr!(cx.i18n, "Add an account"),
                Kind::Plain,
                Some(Message::Accounts(Event::Add)),
            )
            .wide(40.0)
            .into(),
        );
    }
    let width = WIDTH.min(cx.look.window.width - 48.0);
    Some(
        cx.look
            .framed(parts, width, Message::Accounts(Event::Close)),
    )
}

/// One account: its picture and username, crate::i18n::tr!(cx.i18n, "Active account") under the one
/// in use, Switch on the others, and the button for its menu, which opens
/// under it.
fn card<'a>(cx: Ctx<'a>, account: &'a DiscordAccount, menu: bool) -> Element<'a, Message> {
    let theme = cx.look.theme;
    let active = account.id == cx.state.discord_active;
    let mut names = column![
        crate::elide::elide(account.label())
            .size(16)
            .font(BOLD)
            .color(theme.text)
    ]
    .width(Length::Fill);
    if active {
        names = names.push(
            text(crate::i18n::tr!(cx.i18n, "Active account"))
                .size(13)
                .font(SEMIBOLD)
                .color(theme.live),
        );
    }
    let more = cx
        .look
        .icon_button(
            Icon::Ellipsis,
            format!("More options for {}", account.label()),
            Message::Accounts(Event::Menu(account.id.clone())),
        )
        .size(32.0, 20);
    let mut parts = row![
        cx.avatar(account.avatar.as_deref(), account.label(), 40.0, 20.0, 1.0),
        names
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    if !active {
        parts = parts.push(
            action(
                theme,
                crate::i18n::tr!(cx.i18n, "Switch"),
                Kind::Plain,
                Some(Message::Accounts(Event::Switch(account.id.clone()))),
            )
            .small(),
        );
    }
    let ground = theme.active;
    let line = container(parts.push(more))
        .padding(Padding {
            left: 16.0,
            right: 12.0,
            ..Padding::ZERO
        })
        .height(CARD)
        .width(Length::Fill)
        .align_y(Alignment::Center)
        .style(move |_| rounded(ground, 8));
    if !menu {
        return line.into();
    }
    let rows = column![cx.look.menu_item(
        item("Log out", Icon::LogOut, true),
        Light::Pointer,
        Message::Accounts(Event::LogOut(account.id.clone())),
    )]
    .spacing(2)
    .width(MENU);
    let panel = container(rows)
        .padding(8)
        .style(controls::menu_panel(theme));
    column![
        line,
        container(panel).width(Length::Fill).align_x(Alignment::End)
    ]
    .spacing(4)
    .into()
}

/// A row of the account's menu.
fn item(label: &'static str, icon: Icon, danger: bool) -> Item<'static> {
    Item {
        label: label.into(),
        icon: Some(icon),
        danger,
        slot: true,
        disabled: false,
        subtitle: None,
        check: Check::None,
    }
}
