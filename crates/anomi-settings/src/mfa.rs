//! The two-factor prompt: Slint's `MfaDialog` (`ui/mfa.slint:36`) over what
//! `show_mfa` fills (`src/ui/server_settings.rs:410`). Discord's
//! "Multi-Factor Authentication" prompt, for a request Discord holds back
//! until this account confirms: the list of ways, a code or the password in
//! a field, or Discord's warning before a backup code, whose button waits
//! ten seconds. Enter in the field confirms once it may; Escape, the X and
//! Cancel fail the request (Escape and a press outside are the shell's
//! layer, which asks the app to close it). The app focuses [`CODE`] when a
//! prompt that asks for one opens.

mod card;
#[cfg(feature = "shots")]
mod shots;

use iced::widget::Id;
use iced::widget::{column, container, row, space, text};
use iced::{Fill, Length};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::settings::mfa::{Intent, Mfa};
use anomi_widgets::button::{Face, Kind, action_button};
use anomi_widgets::field::field as input;
use anomi_widgets::menu;
use yapfast_lang::{tr, trn};

#[cfg(feature = "shots")]
pub use shots::shots;

/// The id of the code field, for the app's `field::focus`.
pub const CODE: &str = "mfa-code";
/// The ids of its buttons, so the keys and the tests can reach one.
pub const BACK: &str = "mfa-back";
pub const RESEND: &str = "mfa-resend";
pub const CANCEL: &str = "mfa-cancel";
pub const CONFIRM: &str = "mfa-confirm";
pub const SURE: &str = "mfa-sure";

/// The code field's inset (`pad: 10px`, `ui/mfa.slint:102`).
const FIELD_PAD: f32 = 10.0;
/// The warning's heading (`font-size: 16px; font-weight: 600`).
const WARNING_TITLE: f32 = typography::size::TITLE;

pub fn view(mfa: &Mfa, motion: Motion) -> impl View<Intent> + '_ {
    let mut parts = vec![card::title_row(motion)];
    if mfa.listing {
        parts.push(listing(mfa, motion));
    } else if mfa.warning {
        parts.extend(warning(mfa, motion));
    } else {
        parts.push(label(&mfa.text).boxed());
        if mfa.asks() {
            parts.push(code(mfa, motion));
        }
    }
    if !mfa.error.is_empty() {
        parts.push(
            text(&mfa.error)
                .size(typography::size::SMALL)
                .style(catalog::text::danger)
                .width(Fill)
                .boxed(),
        );
    }
    parts.push(button_row(mfa, motion));
    card::card(parts)
}

/// A muted line of explanation (`Label`, `ui/theme.slint:158`).
fn label(words: &str) -> impl View<Intent> + '_ {
    text(words)
        .size(typography::size::LABEL)
        .style(catalog::text::muted)
        .width(Fill)
}

/// "Choose how to confirm it's you." and the ways, two pixels apart.
fn listing(mfa: &Mfa, motion: Motion) -> Element<'_, Intent> {
    let mut ways = column![label(tr!("Choose how to confirm it's you.")).boxed()].spacing(2);
    for choice in &mfa.choices {
        ways = ways.push(menu::item(choice, motion).boxed());
    }
    ways.boxed()
}

/// Discord's warning before a backup code, and its button on a row of its
/// own, off while the wait lasts.
fn warning(mfa: &Mfa, motion: Motion) -> Vec<Element<'_, Intent>> {
    let mut parts = vec![
        text(tr!("Slow down! Are you sure?"))
            .size(WARNING_TITLE)
            .font(typography::SEMIBOLD)
            .style(catalog::text::primary)
            .width(Fill)
            .boxed(),
        label(tr!(
            "Backup codes can only be used one time each and are for emergency access to your account!"
        ))
        .boxed(),
    ];
    if mfa.choices.len() > 1 {
        parts.push(
            label(tr!(
                "If you have access to any of your other methods, select ‘Verify with something else’ to go back."
            ))
            .boxed(),
        );
    }
    let wait = if mfa.wait > 0 {
        trn!(
            "If you do not have access to any other methods, the button below will activate in {n} second.",
            "If you do not have access to any other methods, the button below will activate in {n} seconds.",
            mfa.wait
        )
    } else {
        tr!(
            "If you do not have access to any other methods, use the other button below to proceed."
        )
        .to_owned()
    };
    parts.push(
        text(wait)
            .size(typography::size::LABEL)
            .style(catalog::text::muted)
            .width(Fill)
            .boxed(),
    );
    parts.push(
        action_button(tr!("I’m sure, use a backup code"), Kind::Primary, motion)
            .width(Fill)
            .enabled(mfa.wait == 0)
            .id(Id::new(SURE))
            .on_press(Intent::Confirm)
            .boxed(),
    );
    parts
}

/// The field for the code or the password: disabled while it is sent, and
/// Enter confirms once it may (`accepted`).
fn code(mfa: &Mfa, motion: Motion) -> Element<'_, Intent> {
    let mut field = input(&mfa.field, &mfa.code)
        .id(CODE)
        .password(mfa.password)
        .text_size(typography::size::BODY)
        .pad(FIELD_PAD)
        .motion(motion);
    if !mfa.busy {
        field = field.on_input(Intent::Write);
    }
    if mfa.ready {
        field = field.on_submit(Intent::Confirm);
    }
    field.view()
}

/// The buttons laid out at their natural width, which sets the card's width
/// as Slint's `content.min-width` does. In a shrinking column a Fill-width
/// row is given the width of the other parts, so the row's own width counts
/// only through a part of natural width: a copy of the row, clipped to no
/// height, drawn and answering to nothing, beside the real row, which fills
/// what the copy sets (and at least the card's least width).
fn button_row(mfa: &Mfa, motion: Motion) -> Element<'_, Intent> {
    let probe = container(buttons(mfa, motion, true))
        .width(Length::Shrink.min(card::CONTENT_MIN))
        .height(Length::Fixed(0.0))
        .clip(true);
    let _ = probe;
    buttons(mfa, motion, false)
}

/// "Verify with something else" and "Resend Code" at the start, Cancel and
/// Confirm at the end. A `probe` has no ids and no intents.
fn buttons(mfa: &Mfa, motion: Motion, probe: bool) -> Element<'_, Intent> {
    let mut line: Vec<Element<'_, Intent>> = Vec::new();
    if !mfa.listing && mfa.choices.len() > 1 {
        line.push(wire(
            action_button(tr!("Verify with something else"), Kind::Ghost, motion)
                .small()
                .enabled(!mfa.busy),
            BACK,
            Intent::Back,
            probe,
        ));
    }
    if !mfa.listing && mfa.resend {
        line.push(wire(
            action_button(tr!("Resend Code"), Kind::Ghost, motion)
                .small()
                .enabled(!mfa.busy),
            RESEND,
            Intent::Resend,
            probe,
        ));
    }
    line.push(space().width(Fill).boxed());
    line.push(wire(
        action_button(tr!("Cancel"), Kind::Ghost, motion),
        CANCEL,
        Intent::Cancel,
        probe,
    ));
    if mfa.asks() {
        let words = if mfa.busy {
            tr!("Confirming...")
        } else {
            tr!("Confirm")
        };
        line.push(wire(
            action_button(words, Kind::Primary, motion).enabled(mfa.ready),
            CONFIRM,
            Intent::Confirm,
            probe,
        ));
    }
    row(line)
        .width(if probe { Length::Shrink } else { Fill })
        .spacing(metrics::SPACE_S)
        .align_y(iced::Alignment::Center)
        .boxed()
}

/// A button with its id and intent, unless it is the probe's copy.
fn wire<'a>(
    button: Face<'a, Intent>,
    id: &'static str,
    intent: Intent,
    probe: bool,
) -> Element<'a, Intent> {
    if probe {
        button.boxed()
    } else {
        button.id(Id::new(id)).on_press(intent).boxed()
    }
}
