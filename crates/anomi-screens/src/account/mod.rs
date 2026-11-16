//! The account card at the foot of the rail and the sidebar: the avatar
//! with its presence, the name and status line, then the microphone,
//! headphones and settings. The Slint side: `AccountCard` in
//! `ui/yapfast.slint`, filled by `show_account` in `src/ui/refresh.rs`.

use iced::widget::{button, column, container, row, text};
use iced::{Alignment, Fill};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::account::{Account, Intent};
use anomi_widgets::avatar::{Ground, avatar};
use anomi_widgets::icon::{Icon, icon};
use yapfast_lang::tr;

pub fn view(account: &Account) -> impl View<Intent> + '_ {
    let lines = column![
        text(account.name.as_str())
            .size(typography::size::SMALL)
            .font(typography::SEMIBOLD)
            .style(catalog::text::primary)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End),
        text(account.line.as_str())
            .size(typography::size::CAPTION)
            .style(catalog::text::muted)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End),
    ]
    .width(Fill);
    let mic = if account.muted {
        Icon::MicOff
    } else {
        Icon::Mic
    };
    let ears = if account.deafened {
        Icon::HeadphoneOff
    } else {
        Icon::Headphones
    };
    let content = row![
        avatar(
            &account.avatar,
            metrics::AVATAR_ACCOUNT,
            account.presence,
            Ground::Panel
        ),
        lines,
        tool(
            mic,
            yapfast_lang::text_in("Microphone", "Mute"),
            Intent::ToggleMute
        ),
        tool(ears, tr!("Deafen"), Intent::ToggleDeafen),
        tool(Icon::Settings, tr!("User Settings"), Intent::Settings),
    ]
    .spacing(metrics::SPACE_S)
    .align_y(Alignment::Center);
    container(content)
        .padding([0.0, metrics::SPACE_S])
        .width(Fill)
        .center_y(metrics::ACCOUNT_HEIGHT)
        .style(catalog::container::panel)
}

/// One of the card's buttons: an icon, with its label for a tooltip.
fn tool<'a>(glyph: Icon, label: &'a str, intent: Intent) -> impl View<Intent> + 'a {
    let tip = container(text(label).size(typography::size::SMALL))
        .padding([6.0, 10.0])
        .style(catalog::container::tooltip);
    iced::widget::tooltip(
        button(icon(glyph, metrics::ICON_ROW))
            .padding(6)
            .on_press(intent)
            .style(catalog::button::plain),
        tip,
    )
    .position(iced::widget::tooltip::Position::Top)
}
