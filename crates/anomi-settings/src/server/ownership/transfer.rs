//! Transfer Ownership: `TransferOwnershipDialog`
//! (`ui/server_ownership.slint:104`). Two steps: who gets the server with
//! the acknowledgement, then the emailed code.

use iced::widget::{column, container, row, space, svg, text};
use iced::{Alignment, Background, Border, Color, Fill, Length, Padding};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, typography};
use anomi_vm::icon::Icon;
use anomi_vm::kind::Presence;
use anomi_vm::settings::ownership::{Intent, Ownership};
use anomi_widgets::avatar::{Ground, avatar};
use anomi_widgets::icon::icon;
use anomi_widgets::paragraph::paragraph;
use yapfast_lang::tr;

use super::CODE;
use super::card::{buttons, card, problem, title, typed, words};
use crate::kit::{mix, pressable};

const AVATAR: f32 = 80.0;
const CHEVRON: f32 = 24.0;
const OWNERS_GAP: f32 = 24.0;
/// The heading, the field, its error and "Resend it" (`spacing: 6px`).
const FIELD_GAP: f32 = 6.0;
/// The warning: radius 6, danger at 12 %, 10 px inside, 13 px text.
const WARNING_RADIUS: f32 = 6.0;
const WARNING_ALPHA: f32 = 0.12;
const WARNING_PAD: f32 = 10.0;
/// The acknowledgement's box: 18 px, radius 4, a 1.5 px ring, a 12 px tick,
/// 10 px before the words, 2 px down.
const BOX: f32 = 18.0;
const BOX_RADIUS: f32 = 4.0;
const BOX_RING: f32 = 1.5;
const TICK: f32 = 12.0;
const BOX_GAP: f32 = 10.0;
const BOX_TOP: f32 = 2.0;
/// A Verified server's acknowledgement: `opacity: 0.5`.
const DIM: f32 = 0.5;
/// The keys' ring around a pressable part.
const RING_RADIUS: f32 = 4.0;

pub fn view(ownership: &Ownership, motion: Motion) -> impl View<Intent> + '_ {
    let mut parts = vec![title(&ownership.title)];
    if ownership.code {
        parts.extend(code(ownership, motion));
    } else {
        parts.push(words(ownership));
        parts.push(owners(ownership));
        if ownership.verified {
            parts.push(warning());
        }
        parts.push(acknowledgement(ownership, motion));
        if !ownership.error.is_empty() {
            parts.push(problem(&ownership.error, typography::size::SMALL));
        }
    }
    let confirm = if ownership.sending {
        tr!("Transferring...")
    } else {
        tr!("Transfer Ownership")
    };
    parts.push(buttons(confirm, ownership.may_confirm, ownership, motion));
    card(parts)
}

/// Both owners' avatars with a chevron between them.
fn owners(ownership: &Ownership) -> Element<'_, Intent> {
    let pair = row![
        avatar(&ownership.owner, AVATAR, Presence::None, Ground::Raised),
        icon(Icon::ChevronRight, CHEVRON).style(|theme: &Theme, _| svg::Style {
            color: Some(theme.palette().muted),
        }),
        avatar(&ownership.new_owner, AVATAR, Presence::None, Ground::Raised),
    ]
    .spacing(OWNERS_GAP)
    .align_y(Alignment::Center);
    container(pair).center_x(Fill).boxed()
}

/// Why a Verified or Partnered server can't be handed over.
fn warning<'a>() -> Element<'a, Intent> {
    container(
        text(tr!(
            "Partnered or Verified servers must reach out to the Community team for ownership transfer requests. However, our Partner and Verified Server Programs are currently paused and not able to handle any requests."
        ))
        .size(typography::size::LABEL)
        .style(catalog::text::primary)
        .width(Fill),
    )
    .padding(WARNING_PAD)
    .width(Fill)
    .style(|theme: &Theme| container::Style {
        background: Some(Background::Color(Color {
            a: WARNING_ALPHA,
            ..theme.palette().danger
        })),
        border: Border::default().rounded(WARNING_RADIUS),
        ..container::Style::default()
    })
    .boxed()
}

/// `color` at half its alpha on a Verified server, where nothing can be
/// agreed.
fn faded(color: Color, dim: bool) -> Color {
    Color {
        a: if dim { color.a * DIM } else { color.a },
        ..color
    }
}

/// The box and its words, pressed as one, with the keys too; inert on a
/// Verified server and while it is sent.
fn acknowledgement(ownership: &Ownership, motion: Motion) -> Element<'_, Intent> {
    let dim = ownership.verified;
    let words = paragraph(&ownership.acknowledge)
        .size(typography::size::SMALL)
        .color(move |palette| faded(palette.secondary, dim))
        .width(Fill);
    let content = row![
        container(mark(ownership.agreed, dim)).padding(Padding::ZERO.top(BOX_TOP)),
        words
    ]
    .spacing(BOX_GAP)
    .width(Fill)
    .boxed();
    if dim || ownership.sending {
        return content;
    }
    pressable(
        content,
        tr!("Acknowledge the transfer"),
        Intent::Agree,
        RING_RADIUS,
        |theme, _| theme.palette().secondary,
        motion,
    )
}

fn mark<'a>(on: bool, dim: bool) -> Element<'a, Intent> {
    let inner: Element<'a, Intent> = if on {
        icon(Icon::Check, TICK)
            .style(move |theme: &Theme, _| svg::Style {
                color: Some(faded(theme.palette().on_accent, dim)),
            })
            .boxed()
    } else {
        space().boxed()
    };
    container(inner)
        .center(Length::Fixed(BOX))
        .style(move |theme: &Theme| {
            let palette = theme.palette();
            let ring = if on { palette.accent } else { palette.muted };
            container::Style {
                background: on.then_some(Background::Color(faded(palette.accent, dim))),
                border: Border {
                    color: faded(ring, dim),
                    width: BOX_RING,
                    radius: BOX_RADIUS.into(),
                },
                ..container::Style::default()
            }
        })
        .boxed()
}

/// The emailed code's step: Discord's note, the field, why it was refused,
/// and "Resend it", which says "sent" once it has come.
fn code(ownership: &Ownership, motion: Motion) -> Vec<Element<'_, Intent>> {
    let wrong = (!ownership.error.is_empty()).then_some(ownership.error.as_str());
    vec![
        text(tr!(
            "Check your email: we sent you a verification code. Enter it here to verify you’re really you."
        ))
        .size(typography::size::BODY)
        .style(catalog::text::secondary)
        .width(Fill)
        .boxed(),
        column![
            typed(
                tr!("VERIFICATION CODE"),
                CODE,
                ownership,
                wrong,
                motion
            ),
            resend(ownership, motion),
        ]
        .spacing(FIELD_GAP)
        .width(Fill)
        .boxed(),
    ]
}

/// "Resend it" in the link colour, brightening under the pointer; green
/// once sent; inert while a code or the transfer is on its way.
fn resend(ownership: &Ownership, motion: Motion) -> Element<'_, Intent> {
    let sent = ownership.resent;
    let words = if sent {
        tr!("Verification code sent.")
    } else {
        tr!("Didn’t receive a code or it expired? Resend it.")
    };
    let ink = move |theme: &Theme, lit: f32| {
        let palette = theme.palette();
        if sent {
            palette.live
        } else {
            mix(palette.link, palette.accent_hover, lit)
        }
    };
    let content = text(words).size(typography::size::LABEL);
    if ownership.sending || ownership.resending {
        return content
            .style(move |theme: &Theme| text::Style {
                color: Some(ink(theme, 0.0)),
                selection: None,
            })
            .boxed();
    }
    pressable(
        content.style(catalog::text::inherit).boxed(),
        words,
        Intent::Resend,
        RING_RADIUS,
        ink,
        motion,
    )
}
