//! A server's Safety Setup page: Slint's `SafetySection`
//! (`ui/moderation.slint:143`) over what `safety_prompt` fills
//! (`src/ui/server_settings.rs:838`): the verification level, whether
//! moderators need two-factor authentication, and the sensitive content
//! filter, each asked for as it is chosen. Security Actions, which Invites'
//! button opens, is [`actions`].

mod actions;
mod choices;

use iced::Length;
use iced::widget::{column, container, text};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::settings::safety::{Intent, Safety};
use yapfast_lang::tr;

use crate::kit::{heading, switch};

pub use actions::actions;

/// The gap between the page's three groups, and inside a group.
const GAP: f32 = 16.0;
const TIGHT: f32 = 8.0;
/// The note's inset (`height: preferred + 20px`, `x: 12px`).
const NOTE_PAD: [f32; 2] = [10.0, 12.0];

/// A line of explanation under a heading (Slint's `Label`).
fn label<'a>(words: &'a str) -> Element<'a, Intent> {
    text(words)
        .size(typography::size::LABEL)
        .line_height(choices::LINE)
        .style(catalog::text::muted)
        .width(Length::Fill)
        .boxed()
}

fn note(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(theme.palette().sunken.into()),
        border: iced::Border::default().rounded(metrics::RADIUS_ROW),
        ..container::Style::default()
    }
}

pub fn view(safety: &Safety, motion: Motion) -> Element<'_, Intent> {
    let ready = !safety.saving;
    let mut mfa = column![
        switch(
            tr!("Require 2FA for moderator actions"),
            safety.mfa_on,
            motion
        )
        .description(&safety.mfa_text)
        .enabled(ready && safety.mfa_lock.is_empty())
        .locked(&safety.mfa_lock)
        .on_toggle(Intent::RequireMfa)
        .view()
    ]
    .spacing(TIGHT);
    if !safety.mfa_note.is_empty() {
        mfa = mfa.push(
            container(
                text(&safety.mfa_note)
                    .size(typography::size::SMALL)
                    .line_height(choices::LINE)
                    .style(catalog::text::secondary)
                    .width(Length::Fill),
            )
            .padding(NOTE_PAD)
            .width(Length::Fill)
            .style(note)
            .boxed(),
        );
    }
    column![
        column![
            heading(tr!("VERIFICATION LEVEL")),
            label(&safety.verification_text),
            choices::choices(
                tr!("Verification Level"),
                &safety.verification_options,
                safety.verification,
                ready,
                Intent::Verification,
                motion
            ),
        ]
        .spacing(TIGHT),
        mfa,
        column![
            heading(tr!("SENSITIVE CONTENT FILTERS")),
            label(tr!(
                "Choose if server members can share image-based media detected by Discord's sensitive content filters. This setting will apply to channels that are not age-restricted."
            )),
            choices::choices(
                tr!("Sensitive content filters"),
                &safety.filter_options,
                safety.filter,
                ready,
                Intent::Filter,
                motion
            ),
        ]
        .spacing(TIGHT),
    ]
    .spacing(GAP)
    .boxed()
}

/// Its shots for `anomi-shots`: the page as the golden screenshot has it,
/// and a Community server's with an option not offered and the note.
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    use crate::shots::{GOLDEN, Shot};
    use anomi_vm::fixtures::settings::safety;
    vec![
        Shot {
            area: "ssettings",
            name: "security",
            size: GOLDEN,
            theme: "bonfire",
            vm: safety::safety,
        },
        Shot {
            area: "ssettings",
            name: "security_community",
            size: GOLDEN,
            theme: "bonfire",
            vm: safety::safety_community,
        },
    ]
}
