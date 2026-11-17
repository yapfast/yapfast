//! Linking Discord: the way in of a build without Matrix, and
//! `DiscordLinkPanel` (`ui/discord.slint:29`): the risk to the account
//! first, then the code to scan with the phone app, then who scanned it
//! while they confirm. A token pasted instead links without the phone.
//!
//! Not ported: the token field's fade in (`animate enter`) and the check's
//! growing in on Scanned, which Slint draws with a layer's opacity; this
//! iced has none, so they arrive whole.

use iced::widget::{Id, qr_code};
use iced::widget::{Space, column, container, row, text};
use iced::{Alignment, Border, Color, Fill, Length};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::sign_in::{Code, Intent, Link, SignIn};
use anomi_widgets::button::{Kind, action_button};
use anomi_widgets::field::field;
use anomi_widgets::icon::{Icon, icon};
use yapfast_lang::tr;

/// The box the code or its stand-in sits in, and the code inside it
/// (`:104`, `:110`).
const BOX: f32 = 184.0;
const CODE: f32 = 176.0;
/// The check once the phone scanned the code (`:121`).
const CHECK: f32 = 40.0;
/// The warning's icon and text (`:51`, `:56`), the gap between them (`:49`),
/// and its tint and edge alphas (`:46-47`).
const WARNING_ICON: f32 = 18.0;
const WARNING_GAP: f32 = 10.0;
const WARNING_FILL: f32 = 0.1;
const WARNING_EDGE: f32 = 0.35;
/// A phase's heading (`font-size: 17px`, `:97`, `:138`).
const HEADING: f32 = 17.0;
/// The gap between the box and the words beside it (`:108`).
const SCAN_GAP: f32 = 20.0;

/// The left side of the card: heading, the panel, and what the app said.
pub fn way<'a>(sign_in: &'a SignIn, motion: Motion) -> Element<'a, Intent> {
    let mut way = column![
        super::heading(tr!("Link your Discord account with your phone.")),
        Space::new().height(metrics::SPACE_M),
    ]
    .spacing(metrics::SPACE_S)
    .width(Fill);
    if !sign_in.busy {
        way = way.push(panel(sign_in, motion));
    }
    let waiting = sign_in.busy || sign_in.link == Link::Idle;
    if !sign_in.notice.is_empty() && waiting {
        let tone: fn(&Theme) -> iced::widget::text::Style = if sign_in.busy {
            catalog::text::secondary
        } else {
            catalog::text::danger
        };
        way = way.push(
            text(sign_in.notice.as_str())
                .size(typography::size::BODY)
                .style(tone)
                .boxed(),
        );
    }
    way.boxed()
}

/// `DiscordLinkPanel`: what the link's phase shows.
fn panel<'a>(sign_in: &'a SignIn, motion: Motion) -> Element<'a, Intent> {
    match &sign_in.link {
        Link::Idle => start(sign_in, None, motion),
        Link::Failed(note) => start(sign_in, Some(note), motion),
        Link::Checking => text(tr!("Checking the token..."))
            .size(typography::size::BODY)
            .style(catalog::text::secondary)
            .boxed(),
        Link::Captcha => captcha(motion),
        Link::Starting | Link::Code(_) | Link::Scanned(_) => scan(sign_in),
    }
}

/// Phases 0 and 4: the warning, why linking failed, and the two ways to go
/// on, the token's field under them when it is open.
fn start<'a>(sign_in: &'a SignIn, note: Option<&'a String>, motion: Motion) -> Element<'a, Intent> {
    let retry = note.is_some() && !sign_in.token_open;
    let show = action_button(
        if retry {
            tr!("Try again")
        } else {
            tr!("Show the code")
        },
        if sign_in.token_open {
            Kind::Secondary
        } else {
            Kind::Primary
        },
        motion,
    )
    .id(Id::new(super::SHOW_CODE))
    .on_press(Intent::LinkDiscord);
    let mut buttons = row![show].spacing(metrics::SPACE_S);
    if !sign_in.token_open {
        buttons = buttons.push(
            action_button(tr!("Use a token"), Kind::Secondary, motion)
                .id(Id::new(super::USE_TOKEN))
                .on_press(Intent::OpenToken)
                .boxed(),
        );
    }
    let mut panel = column![warning()].spacing(metrics::SPACE_M);
    if let Some(note) = note {
        panel = panel.push(
            text(note.as_str())
                .size(typography::size::SMALL)
                .style(catalog::text::danger)
                .boxed(),
        );
    }
    panel = panel.push(buttons.boxed());
    if sign_in.token_open {
        panel = panel.push(token(sign_in, motion));
    }
    panel.boxed()
}

/// The risk to the account, in a red-edged box.
fn warning<'a>() -> Element<'a, Intent> {
    let glyph = icon(Icon::TriangleAlert, WARNING_ICON).style(|theme: &Theme, _status| {
        iced::widget::svg::Style {
            color: Some(theme.palette().danger),
        }
    });
    let line = text(tr!(
        "Discord doesn't allow apps like Yapfast on personal accounts. Linking puts your account at some risk of being suspended."
    ))
    .size(typography::size::SMALL)
    .width(Fill)
    .style(catalog::text::primary);
    container(row![glyph, line].spacing(WARNING_GAP))
        .padding(metrics::SPACE_M)
        .width(Fill)
        .style(|theme: &Theme| {
            let danger = theme.palette().danger;
            iced::widget::container::Style {
                background: Some(
                    Color {
                        a: WARNING_FILL,
                        ..danger
                    }
                    .into(),
                ),
                border: Border::default()
                    .rounded(metrics::RADIUS_CARD)
                    .width(1)
                    .color(Color {
                        a: WARNING_EDGE,
                        ..danger
                    }),
                ..Default::default()
            }
        })
        .boxed()
}

/// The token's field with Link beside it, and the reminder under them.
fn token<'a>(sign_in: &'a SignIn, motion: Motion) -> Element<'a, Intent> {
    let typed = sign_in.token.as_str();
    let well = field(tr!("Discord token"), typed)
        .password(true)
        .id(super::TOKEN)
        .motion(motion)
        .on_input(Intent::Token)
        .on_submit(Intent::LinkToken(typed.to_owned()))
        .view();
    let link = action_button(tr!("Link"), Kind::Primary, motion)
        .id(Id::new(super::LINK))
        .enabled(!typed.is_empty())
        .on_press(Intent::LinkToken(typed.to_owned()));
    column![
        super::caption(tr!("TOKEN")),
        row![well, link]
            .spacing(metrics::SPACE_S)
            .align_y(Alignment::Start),
        text(tr!(
            "Anyone with your token can use your account. Paste it only here, and never share it."
        ))
        .size(typography::size::LABEL)
        .style(catalog::text::secondary),
    ]
    .spacing(metrics::SPACE_S)
    .boxed()
}

/// Phase 5: the check waits in the browser.
fn captcha<'a>(motion: Motion) -> Element<'a, Intent> {
    column![
        text(tr!("Solve the captcha in your browser"))
            .size(HEADING)
            .font(typography::BOLD)
            .style(catalog::text::primary),
        text(tr!(
            "Discord wants to check you're a person before it links your account. Yapfast opened the check in your browser; linking finishes once it's solved."
        ))
        .size(typography::size::SMALL)
        .style(catalog::text::secondary),
        container(action_button(tr!("Open it again"), Kind::Secondary, motion).id(Id::new(super::REOPEN))
                .on_press(Intent::OpenCaptcha))
            .padding(iced::Padding::ZERO.top(metrics::SPACE_S)),
    ]
    .spacing(metrics::SPACE_S)
    .boxed()
}

/// Phases 1 to 3: the box, and what to do beside it.
fn scan<'a>(sign_in: &'a SignIn) -> Element<'a, Intent> {
    let white = matches!(sign_in.link, Link::Code(_));
    let inside: Element<'a, Intent> = match &sign_in.link {
        Link::Code(Some(code)) => seen(code),
        Link::Code(None) => Space::new().boxed(),
        Link::Scanned(_) => column![
            icon(Icon::Check, CHECK).style(|theme: &Theme, _status| iced::widget::svg::Style {
                color: Some(theme.palette().live),
            }),
            text(tr!("Scanned"))
                .size(typography::size::BODY)
                .font(typography::BOLD)
                .style(catalog::text::primary),
        ]
        .spacing(metrics::SPACE_S)
        .align_x(Alignment::Center)
        .boxed(),
        _ => text(tr!("Getting a code..."))
            .size(typography::size::SMALL)
            .style(catalog::text::muted)
            .boxed(),
    };
    let pad = container(inside)
        .width(BOX)
        .height(BOX)
        .center_x(BOX)
        .center_y(BOX)
        .style(move |theme: &Theme| iced::widget::container::Style {
            background: Some(
                if white {
                    Color::WHITE
                } else {
                    theme.palette().sunken
                }
                .into(),
            ),
            border: Border::default().rounded(metrics::RADIUS_DIALOG),
            ..Default::default()
        });
    let (title, line) = match &sign_in.link {
        Link::Scanned(name) => (
            tr!("Confirm on your phone"),
            tr!("Linking {}. Tap Log in on your phone to finish.", name),
        ),
        _ => (
            tr!("Scan it with the Discord app"),
            tr!(
                "On your phone, open Discord, go to Settings, tap Scan QR Code, and point the camera here. A new code replaces this one after a couple of minutes."
            )
            .to_owned(),
        ),
    };
    let words = column![
        text(title)
            .size(HEADING)
            .font(typography::BOLD)
            .style(catalog::text::primary),
        text(line)
            .size(typography::size::SMALL)
            .style(catalog::text::secondary),
    ]
    .spacing(metrics::SPACE_S)
    .width(Fill);
    row![
        pad,
        container(words)
            .center_y(Length::Shrink.min(BOX))
            .width(Fill)
    ]
    .spacing(SCAN_GAP)
    .boxed()
}

/// The code, 176 px square with its quiet zone (the box around it is white).
fn seen<'a>(code: &'a Code) -> Element<'a, Intent> {
    qr_code(code.data()).total_size(CODE).boxed()
}
