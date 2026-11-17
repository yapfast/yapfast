//! Making a channel or a category: Slint's `ChannelDialog`
//! (`ui/server.slint:516`) over what `show_channel_dialog` fills
//! (`src/ui/refresh.rs:3149`), for the asks that are the channel's: make one,
//! copy one, delete one. Title with the X, where it goes or what a delete
//! does, "CHANNEL TYPE" with the Text/Voice segments (a channel only), the
//! name's heading and field (not for a delete), why Discord refused in red,
//! then Cancel and the main button (red for a delete). Enter confirms, Escape
//! and a press outside close (the shell's layer), and nothing may be asked
//! again while it is sent. The name field takes the keys as it opens: the app
//! focuses [`NAME`] (`field::focus`).

mod card;
mod enter;

use iced::Fill;
use iced::widget::text;

use anomi_theme::prelude::*;
use anomi_theme::{catalog, typography};
use anomi_vm::settings::create_channel::{ChannelType, CreateChannel, Intent};
use anomi_widgets::button::Kind;
use anomi_widgets::field::field;
use yapfast_lang::tr;

use crate::kit::{choices, heading};
use card::{buttons, card, title_row};

/// The name field's id, for the task that focuses it when the dialog opens.
pub const NAME: &str = "create-channel-name";

/// The name field's `text-size: 15px; pad: 10px` (`ui/server.slint:560`).
const PAD: f32 = 10.0;

pub fn view(dialog: &CreateChannel, motion: Motion) -> impl View<Intent> + '_ {
    let ready = !dialog.saving;
    let mut parts = vec![title_row(&dialog.title, motion)];
    if !dialog.detail.is_empty() {
        parts.push(
            text(&dialog.detail)
                .size(typography::size::LABEL)
                .style(catalog::text::muted)
                .width(Fill)
                .boxed(),
        );
    }
    if !dialog.kinds.is_empty() {
        parts.push(heading(tr!("CHANNEL TYPE")).width(Fill).boxed());
        let index = usize::from(dialog.kind == ChannelType::Voice);
        parts.push(
            choices(&dialog.kinds, index, motion)
                .fill(true)
                .on_pick(|i| {
                    Intent::Kind(if i == 0 {
                        ChannelType::Text
                    } else {
                        ChannelType::Voice
                    })
                })
                .view(),
        );
    }
    if dialog.named {
        parts.push(heading(&dialog.field).width(Fill).boxed());
        // Enter saves only when the button would (`if root.ready`, :563).
        let mut name = field(&dialog.placeholder, &dialog.name)
            .id(NAME)
            .text_size(typography::size::BODY)
            .pad(PAD)
            .motion(motion)
            .on_input(Intent::Name);
        if ready {
            name = name.on_submit(Intent::Confirm);
        }
        parts.push(name.view());
    }
    if !dialog.error.is_empty() {
        parts.push(
            text(&dialog.error)
                .size(typography::size::SMALL)
                .style(catalog::text::danger)
                .width(Fill)
                .boxed(),
        );
    }
    let kind = if dialog.danger {
        Kind::Danger
    } else {
        Kind::Primary
    };
    parts.push(buttons(&dialog.confirm, kind, ready, motion));
    enter::enter(card(parts), ready.then_some(Intent::Confirm))
}

/// Its shots for `anomi-shots`: the golden screenshots' two dialogs, and the
/// others of its asks (compared with the Slint app under `scripts/ui-session`).
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    use crate::shots::{GOLDEN, Shot};
    use anomi_vm::fixtures::settings::create_channel as sample;

    fn shot(name: &'static str, vm: fn() -> anomi_vm::Vm) -> Shot {
        Shot {
            area: "dialog",
            name,
            size: GOLDEN,
            theme: "bonfire",
            vm,
        }
    }
    vec![
        shot("create_channel", || sample::asked(sample::channel())),
        shot("create_channel_voice", || sample::asked(sample::voice())),
        shot("create_category", || sample::asked(sample::category())),
        shot("duplicate_channel", || sample::asked(sample::duplicate())),
        shot(
            "create_channel_refused",
            || sample::asked(sample::refused()),
        ),
    ]
}
