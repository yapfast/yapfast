//! A call in the conversation, on the stage's ground: filling a voice
//! channel's body, or above a DM's messages, where it starts at its least
//! and takes less than half of what is left beside the composer (more
//! while a tile is focused, little while offering to join); its lower edge
//! resizes it, a double click going back to those proportions. Not in it,
//! V07's `join` (`JoinVoice`, `CallInvite`); in it, the line saying who is
//! being called or that nobody else is here, over V04's `stage`. The Slint
//! side: `ui/yapfast.slint:3473`-`:3551`.

use iced::widget::{Container, column, container, rule, space, stack, text};
use iced::{Alignment, Background, Fill, Padding};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::conversation::Call;
use anomi_vm::shell::{Layer, Panel};
use anomi_vm::{Intent, Vm};
use anomi_voice as voice;

use crate::shell::{CALL_MIN, Grows, Strip, call_limit, edge};

/// A call above the messages while offering to join, and the least it
/// keeps while in it unless dragged (`:3478`).
const ELSEWHERE: f32 = 210.0;
const HERE_MIN: f32 = 260.0;
/// Its share of the room left beside its least, against the messages' 1:
/// 0.8, 1.8 with a tile focused (`vertical-stretch`, `:3477`).
const SHARE: f32 = 0.8;
const SHARE_FOCUSED: f32 = 1.8;
/// The strip at the call's foot that resizes it (`:3490`).
const FOOT: f32 = 6.0;
/// The line over the stage: padded 16 px on top and at its sides
/// (`:3515`).
const NOTE_PAD: f32 = metrics::SPACE_L;

/// A voice channel's call, filling the body.
pub fn panel(vm: &Vm, motion: Motion) -> Element<'_, Intent> {
    ground(content(vm, motion)).width(Fill).height(Fill).boxed()
}

/// A call above a DM's messages, where it and the messages share `room`,
/// in a workspace `workspace` tall.
pub fn above(vm: &Vm, motion: Motion, room: f32, workspace: f32) -> Element<'_, Intent> {
    let line = column![
        space().height(Fill),
        rule::horizontal(1).style(|theme: &Theme| rule::Style {
            color: theme.palette().edge,
            ..catalog::rule::line(theme)
        }),
    ];
    let mut call = stack![ground(content(vm, motion)).width(Fill).height(Fill), line];
    let Call::Here { focused } = vm.conversation.call else {
        return call.width(Fill).height(ELSEWHERE).boxed();
    };
    let limit = call_limit(workspace);
    // On whole pixels, so its foot's line and strip are crisp.
    let height = match vm.shell.call_height {
        Some(dragged) => dragged.min(limit).max(CALL_MIN),
        None => shared(room, focused),
    }
    .round();
    if vm.shell.opened(Layer::FullStage).is_none() {
        call = call.push(
            edge(
                Panel::Call,
                Grows::Down,
                Strip::Foot(FOOT),
                (CALL_MIN, limit),
                motion,
            )
            .boxed(),
        );
    }
    call.width(Fill).height(height).boxed()
}

/// A call's height not dragged, sharing `room` with the messages: Slint's
/// box layout starts it at its least and gives it its share of the rest,
/// the messages having none of their own.
fn shared(room: f32, focused: bool) -> f32 {
    let share = if focused { SHARE_FOCUSED } else { SHARE };
    HERE_MIN + (room - HERE_MIN).max(0.0) * share / (share + 1.0)
}

/// The line and the stage while in the call, the stage left to the full
/// screen while it shows there; the way to join otherwise.
fn content<'a>(vm: &'a Vm, motion: Motion) -> Element<'a, Intent> {
    match vm.conversation.call {
        Call::Here { .. } => {
            let note = text(vm.conversation.description.as_str())
                .size(typography::size::SMALL)
                .width(Fill)
                .align_x(Alignment::Center)
                .style(catalog::text::muted);
            let stage: Element<'a, Intent> = if vm.shell.opened(Layer::FullStage).is_some() {
                space().height(Fill).boxed()
            } else {
                voice::stage::view(&vm.voice.stage, motion)
                    .map(Intent::from)
                    .boxed()
            };
            column![
                container(note).padding(Padding::new(NOTE_PAD).bottom(0.0)),
                container(stage).width(Fill).height(Fill),
            ]
            .width(Fill)
            .height(Fill)
            .boxed()
        }
        Call::None | Call::Elsewhere => voice::join::view(&vm.voice.join, motion)
            .map(Intent::from)
            .boxed(),
    }
}

fn ground<'a>(content: Element<'a, Intent>) -> Container<'a, Element<'a, Intent>, Theme> {
    container(content).style(|theme: &Theme| container::Style {
        background: Some(Background::Color(theme.palette().stage)),
        ..container::Style::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dm_call_starts_at_its_least_and_shares_the_rest() {
        // The golden `dmcall/connected_bob_dm.png`: 810 px under the
        // header, less the composer's 66, puts the call's foot 475 px down.
        assert_eq!(shared(810.0 - 66.0, false).round(), 475.0);
        assert!((shared(744.0, true) - (260.0 + 484.0 * 1.8 / 2.8)).abs() < 0.01);
        // Too little room keeps its least.
        assert_eq!(shared(200.0, false), HERE_MIN);
    }
}
