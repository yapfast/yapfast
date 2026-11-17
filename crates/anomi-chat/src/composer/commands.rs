//! The composer's slash commands: the list over the composer (`CommandList`)
//! and the command being written as pills over the field's text
//! (`CommandPills`). The Slint side: `ui/app_commands.slint:22`-`:160`; the
//! window's wiring is `src/ui/app_commands.rs` and the list's place
//! `ui/yapfast.slint:4649`.
//!
//! [`view`] is the list, which F01 wired above the composer's frame: it
//! takes no room in the column and floats up over the timeline with its
//! bottom [`GAP`] above the frame ([`lift`]), growing in as it shows
//! ([`entry`]). [`pills`] and [`command`] are
//! for the field: C14's `field` gives the composer widget the pills with
//! `.pills(commands::pills(..))` and the keys' state with
//! `.command(commands::command(..))`, as Slint's `ComposerField` takes
//! `command-name`, `command-options` and `command-focus`.
//!
//! The list keeps the chosen row in view with `Place::Reveal`; the app,
//! which moves the chosen row, follows it with [`reveal`].

mod entry;
pub(super) mod lift;
mod list;
mod pills;

use iced::advanced::widget::Id;
use iced::widget::space;
use iced_runtime::Task;

use anomi_theme::prelude::*;
use anomi_vm::chat::commands::{Commands, Intent};
use anomi_widgets::composer::Command;
use anomi_widgets::list::{Place, operation};

pub use pills::pills;

/// `- 8px`: the list's bottom edge is this far over the composer.
pub const GAP: f32 = 8.0;

/// The list's id, for [`reveal`].
fn id() -> Id {
    Id::new("slash-commands")
}

/// Scrolls the list to its chosen row, just into view.
pub fn reveal<T: Send + 'static>(chosen: usize) -> Task<T> {
    operation::jump_to(id(), chosen as u64, Place::Reveal(0.0))
}

/// The list over the composer: the commands of the room's apps, or the values
/// the focused option may take, with a heading, or in place of rows why
/// there are none. Nothing while it is not [`shown`](Commands::shown).
pub fn view(commands: &Commands, motion: Motion) -> impl View<Intent> + '_ {
    let shown: Element<'_, Intent> = if commands.shown {
        let floating = lift::lift(entry::entry(list::frame(commands, motion), motion), GAP);
        iced::widget::container(floating)
            .padding(iced::Padding::ZERO.left(super::GUTTER).right(super::GUTTER))
            .width(iced::Length::Fill)
            .boxed()
    } else {
        space().height(0).boxed()
    };
    shown
}

/// What the composer's keys need of the command being written: how many
/// options it has, the one the text writes, and whether that one is a file
/// with none yet.
pub fn command(commands: &Commands) -> Option<Command> {
    let written = commands.written.as_ref()?;
    let wants_file = written
        .options
        .get(written.focused)
        .is_some_and(|pill| pill.file && pill.value.is_empty());
    Some(Command {
        options: written.options.len(),
        focus: written.focused,
        wants_file,
    })
}

/// Its shots for `anomi-shots`, each the whole window from a fixture (see
/// `crate::shots`), named after the golden screenshots.
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    use crate::shots::{GOLDEN, Shot};
    use anomi_vm::fixtures::chat::commands as fixture;
    vec![Shot {
        area: "composer",
        name: "slash_suggestions",
        size: GOLDEN,
        theme: "bonfire",
        vm: fixture::slash_suggestions,
    }]
}
