//! The sign-in screen and linking Discord. The Slint side: `SignInScreen`
//! (`ui/sign-in.slint:8`): the app's icon and name at the top left, then a
//! card centred on the darkest ground with the way in on its left (linking
//! Discord, or a Matrix password) and the local preview beside it. Its parts
//! are one module each: [`discord`] (`DiscordLinkPanel`), [`matrix`],
//! [`preview`] and [`verify`] (`VerifyBanner`). Task N09
//! (docs/iced/tasks/shell.md).
//!
//! The card keeps the Slint one's room: 760 px at most, 32 px inside, the
//! way in at least 300 px wide and the preview 220 px, with a rule between.
//! It is centred in what the window leaves under the header, and scrolls
//! when the window is shorter than it.

mod discord;
mod matrix;
mod preview;
#[cfg(feature = "shots")]
mod shots;
mod verify;

use iced::widget::{Column, column, container, responsive, row, rule, text};
use iced::{Alignment, Border, Color, Fill, Length, Shadow, Vector};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::sign_in::{Intent, SignIn};
use anomi_widgets::brand;
use anomi_widgets::cover::banner;
use anomi_widgets::list::scroll_view;
use anomi_widgets::picture::picture;
use yapfast_lang::tr;

pub use verify::verify_banner;

/// Its shots for `anomi-shots`, each the whole window from a fixture (see
/// `crate::shots`).
#[cfg(feature = "shots")]
pub use shots::shots;

/// The field ids the app focuses (`field::focus`): the Matrix sign-in's
/// Matrix ID when it is asked for (`sign-in-focus`), the token when
/// `Intent::OpenToken` opens it (`init => focus-input`).
pub const USERNAME: &str = "sign-in-username";
pub const HOMESERVER: &str = "sign-in-homeserver";
pub const PASSWORD: &str = "sign-in-password";
pub const TOKEN: &str = "sign-in-token";

/// The buttons' ids, which tests and `focusable::focus` find them by.
pub const SHOW_CODE: &str = "sign-in-show-code";
pub const USE_TOKEN: &str = "sign-in-use-token";
pub const LINK: &str = "sign-in-link";
pub const REOPEN: &str = "sign-in-reopen";
pub const SIGN_IN: &str = "sign-in-matrix";
pub const RETRY: &str = "sign-in-retry";
pub const PREVIEW: &str = "sign-in-preview";
pub const RECOVER: &str = "verify-recover";
pub const CHECK_AGAIN: &str = "verify-check-again";

/// The icon and name at the top left: 36 px square at (28, 18), 10 px
/// apart (`:25`); the screen's content starts 64 px down.
const LOGO: f32 = 36.0;
const LOGO_RADIUS: f32 = 9.0;
const HEADER: f32 = 64.0;
const HEADER_LEFT: f32 = 28.0;
const HEADER_TOP: f32 = 18.0;
/// The name's size (`font-size: 24px`).
const PRODUCT: f32 = 24.0;
/// The card: its widest, its inside, the gap between its sides (`:54-57`).
const CARD_WIDTH: f32 = 760.0;
const CARD_PAD: f32 = 32.0;
const CARD_GAP: f32 = 32.0;
/// The card's shadow (`drop-shadow-*`, `:53`, colour `:56`).
const SHADOW_BLUR: f32 = 32.0;
const SHADOW_DROP: f32 = 12.0;
const SHADOW_ALPHA: f32 = 0x66 as f32 / 255.0;
/// A heading in the card (`font-size: 26px`).
const TITLE: f32 = 26.0;

pub fn view(sign_in: &SignIn, motion: Motion) -> impl View<Intent> + '_ {
    let logo = container(
        picture(brand::logo())
            .side(LOGO)
            .radius(LOGO_RADIUS)
            .motion(motion),
    );
    let name = text(sign_in.product.as_str())
        .size(PRODUCT)
        .font(typography::weight(iced::font::Weight::Black))
        .style(|theme: &Theme| iced::widget::text::Style {
            color: Some(theme.palette().accent),
            selection: None,
        });
    let header = container(
        row![logo, name]
            .spacing(metrics::SPACE_M - 2.0)
            .align_y(Alignment::Center)
            .height(LOGO),
    )
    .padding(iced::Padding {
        top: HEADER_TOP,
        left: HEADER_LEFT,
        ..iced::Padding::ZERO
    })
    .height(HEADER);
    let body = responsive(move |size| {
        let content = container(page(sign_in, motion, size.width))
            .padding(iced::Padding {
                bottom: metrics::SPACE_XL,
                left: metrics::SPACE_XL,
                right: metrics::SPACE_XL,
                top: 0.0,
            })
            .center_x(Fill)
            .center_y(Length::Shrink.min(size.height));
        scroll_view(content, motion).boxed()
    });
    container(column![header, body])
        .width(Fill)
        .height(Fill)
        .style(catalog::container::rail)
}

/// The graphics notice over the card, if any, and the card.
fn page(sign_in: &SignIn, motion: Motion, window: f32) -> Element<'_, Intent> {
    let width = (window - 2.0 * metrics::SPACE_XL).clamp(0.0, CARD_WIDTH);
    let mut page: Column<Element<'_, Intent>> = Column::new()
        .spacing(metrics::SPACE_L)
        .align_x(Alignment::Center);
    if let Some(notice) = &sign_in.graphics_notice {
        page = page.push(
            container(
                container(banner(notice.as_str(), Intent::DismissNotice, motion)).width(width),
            )
            .center_x(Fill)
            .boxed(),
        );
    }
    page.push(card(sign_in, motion, width)).boxed()
}

/// The raised card: the way in, a rule, the preview.
fn card(sign_in: &SignIn, motion: Motion, width: f32) -> Element<'_, Intent> {
    let way = if sign_in.matrix_build {
        matrix::view(sign_in, motion)
    } else {
        discord::way(sign_in, motion)
    };
    let sides = row![
        container(way).width(Fill),
        rule::vertical(1).style(catalog::rule::line),
        container(preview::view(sign_in, motion)).width(preview::WIDTH),
    ]
    .spacing(CARD_GAP)
    .align_y(Alignment::Center)
    .width(Fill)
    .height(Length::Shrink);
    container(sides)
        .padding(CARD_PAD)
        .width(width)
        .style(|theme: &Theme| iced::widget::container::Style {
            background: Some(theme.palette().chat.into()),
            border: Border::default().rounded(metrics::RADIUS_DIALOG),
            shadow: Shadow {
                color: Color::from_rgba(0.0, 0.0, 0.0, SHADOW_ALPHA),
                offset: Vector::new(0.0, SHADOW_DROP),
                blur_radius: SHADOW_BLUR,
            },
            ..Default::default()
        })
        .boxed()
}

/// "Sign in to Yapfast" and the line under it, both centred.
fn heading<'a>(line: &'a str) -> Element<'a, Intent> {
    column![
        text(tr!("Sign in to Yapfast"))
            .size(TITLE)
            .font(typography::BOLD)
            .width(Fill)
            .align_x(Alignment::Center)
            .style(catalog::text::primary),
        text(line)
            .size(typography::size::BODY)
            .width(Fill)
            .align_x(Alignment::Center)
            .style(catalog::text::secondary),
    ]
    .spacing(metrics::SPACE_S)
    .boxed()
}

/// A field's small heading in capitals (`font-size: 12px; weight 700`).
fn caption<'a>(label: &'a str) -> Element<'a, Intent> {
    text(label)
        .size(typography::size::CAPTION)
        .font(typography::BOLD)
        .style(catalog::text::secondary)
        .boxed()
}
