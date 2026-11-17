//! The Account settings page. The Slint side: `AccountSettings`
//! (`ui/settings.slint:654`): the card with the picture, the name and the
//! status, where the login is kept, and the buttons, which a narrow window
//! takes onto a second line. The title is the frame's bar's.

use iced::Length;
use iced::widget::{Column, column, container, row, text};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::icon::Icon;
use anomi_vm::settings::account::{Account, Intent};
use anomi_widgets::avatar::portrait;
use anomi_widgets::button::{Kind, action_button};
use anomi_widgets::presence::status_text;
use yapfast_lang::tr;

const PICTURE: f32 = 64.0;
const NAME: f32 = typography::size::DIALOG_TITLE;

pub fn view(account: &Account, motion: Motion) -> impl View<Intent> + '_ {
    let name = text(account.name.as_str())
        .size(NAME)
        .font(typography::BOLD)
        .wrapping(text::Wrapping::None)
        .ellipsis(text::Ellipsis::End)
        .style(catalog::text::primary);
    let words = column![name, status_text(&account.status, motion).view()]
        .spacing(2)
        .width(Length::Fill);
    let card = container(
        row![
            container(
                portrait(&account.avatar, PICTURE)
                    .decoration(account.decoration.as_ref())
                    .motion(motion)
                    .view()
            )
            .center_y(Length::Shrink),
            container(words)
                .center_y(Length::Shrink)
                .width(Length::Fill)
        ]
        .spacing(16)
        .align_y(iced::Alignment::Center),
    )
    .padding(16)
    .width(Length::Fill)
    .style(|theme: &Theme| container::Style {
        border: iced::Border::default().rounded(metrics::RADIUS_CARD),
        ..catalog::container::sidebar(theme)
    });

    let mut page = Column::new().spacing(16).push(card.boxed());
    if !account.storage.is_empty() {
        page = page.push(
            text(account.storage.as_str())
                .size(typography::size::LABEL)
                .style(catalog::text::muted)
                .boxed(),
        );
    }
    page.push(buttons(account, motion))
}

fn buttons(account: &Account, motion: Motion) -> Element<'_, Intent> {
    let mut buttons = Vec::with_capacity(4);
    if account.offline() {
        buttons.push(
            action_button(tr!("Reconnect"), Kind::Secondary, motion)
                .leading(Icon::RefreshCw)
                .enabled(!account.busy)
                .on_press(Intent::Reconnect)
                .boxed(),
        );
    }
    if account.linked {
        buttons.push(
            action_button(tr!("Edit profile"), Kind::Secondary, motion)
                .leading(Icon::UserPen)
                .on_press(Intent::EditProfile)
                .boxed(),
        );
        let words = if account.has_custom_status {
            tr!("Change custom status")
        } else {
            tr!("Set a custom status")
        };
        buttons.push(
            action_button(words, Kind::Secondary, motion)
                .leading(Icon::SmilePlus)
                .on_press(Intent::SetCustomStatus)
                .boxed(),
        );
    }
    let (words, kind) = if account.preview {
        (tr!("Back to sign in"), Kind::Secondary)
    } else {
        (tr!("Sign out"), Kind::Danger)
    };
    buttons.push(
        action_button(words, kind, motion)
            .leading(Icon::LogOut)
            .enabled(account.may_sign_out())
            .on_press(Intent::SignOut)
            .boxed(),
    );
    row(buttons).spacing(8).wrap().vertical_spacing(8).boxed()
}

/// Its shots for `anomi-shots`: the golden `settings/account.png` is the
/// frame's (S01); this adds the offline and the preview pages.
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    use anomi_vm::fixtures::settings::account as sample;

    use crate::shots::{GOLDEN, Shot};
    use anomi_vm::fixtures::settings::frame::account as window;

    fn with(page: anomi_vm::settings::account::Account) -> anomi_vm::Vm {
        let mut vm = window();
        vm.settings.account = page;
        vm
    }
    vec![
        Shot {
            area: "settings",
            name: "account_offline",
            size: GOLDEN,
            theme: "bonfire",
            vm: || with(sample::offline()),
        },
        Shot {
            area: "settings",
            name: "account_preview",
            size: GOLDEN,
            theme: "bonfire",
            vm: || with(sample::preview()),
        },
    ]
}
