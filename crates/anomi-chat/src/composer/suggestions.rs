//! The composer's suggestions, just above it and as wide as it
//! (`SuggestionFrame`, `MentionList`, `EmojiSuggestions`,
//! `ui/composer.slint:104`-`:195`): members, roles and channels for `@` and
//! `#`, or emoji for `:`, each row 36 px, the highlighted one on the active
//! ground and the one under the pointer on the hover ground. Up, Down, Tab,
//! Enter and Escape are the field's (`Intent::Suggest` of the frame); a click
//! is [`Intent::Choose`].
//!
//! The list takes no room in the composer's column: it floats up over the
//! timeline with its bottom edge [`GAP`] above the reply bar, or above the
//! box when there is no bar (Slint's `y: composer-box.y - (compose-label !=
//! "" ? 36px : 0px) - self.height - 8px`, `ui/yapfast.slint:4657`), through
//! the same `lift` as the command list, so the timeline never moves when it
//! shows, grows or hides. It has no entry animation either (iced at this pin
//! has no layer opacity or scale, and a list of pictures cannot fade
//! whole), so it ticks nothing.

mod emoji;
// The command list's own file, until it is exposed from `commands`.
#[path = "commands/lift.rs"]
mod lift;
mod mention;
mod row;

use iced::widget::{column, container, space, text};
use iced::{Color, Fill, Padding, Shadow, Vector};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, typography};
use anomi_vm::chat::suggestions::{Intent, Offer, Suggestions};

/// The frame's padding (`padding: 6px`) and the gutter the composer keeps at
/// its sides, which the list lines up with.
const PAD: f32 = 6.0;
const GUTTER: f32 = 8.0;
/// `- 8px`: the list's bottom edge is this far over the reply bar or box.
const GAP: f32 = 8.0;
/// `SuggestionHeading`: 24 px high, 11 px bold; 6 px in from the rows.
const HEADING: f32 = 24.0;
const HEADING_SIZE: f32 = 11.0;
const HEADING_INSET: f32 = 6.0;
/// The shadow under the frame: `#00000080`, 24 px blur, 8 px down.
const SHADOW: Color = Color::from_rgba(0.0, 0.0, 0.0, 0.5);

pub fn view(suggestions: &Suggestions, _motion: Motion) -> impl View<Intent> + '_ {
    let rows = match &suggestions.offer {
        Offer::Mentions { rows, index } if !rows.is_empty() => mention::rows(rows, *index),
        Offer::Emoji {
            query,
            cells,
            index,
        } if !cells.is_empty() => emoji::rows(query, cells, *index),
        _ => return space().height(0).boxed(),
    };
    container(lift::lift(frame(rows), GAP))
        .padding(Padding {
            top: 0.0,
            right: GUTTER,
            bottom: 0.0,
            left: GUTTER,
        })
        .width(Fill)
        .boxed()
}

/// `SuggestionFrame`: the raised card with its edge and shadow.
fn frame(rows: Vec<Element<'_, Intent>>) -> Element<'_, Intent> {
    container(column(rows).width(Fill))
        .padding(PAD)
        .width(Fill)
        .style(|theme: &Theme| container::Style {
            shadow: Shadow {
                color: SHADOW,
                offset: Vector::new(0.0, 8.0),
                blur_radius: 24.0,
            },
            ..catalog::container::raised(theme)
        })
        .boxed()
}

/// A heading over the rows under it (`SuggestionHeading`).
fn heading<'a>(says: impl text::IntoFragment<'a>) -> Element<'a, Intent> {
    container(
        text(says)
            .size(HEADING_SIZE)
            .font(typography::BOLD)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End)
            .style(catalog::text::muted),
    )
    .padding(Padding {
        left: HEADING_INSET,
        ..Padding::ZERO
    })
    .center_y(HEADING)
    .width(Fill)
    .boxed()
}

/// Its shots for `anomi-shots`, each the whole window from a fixture (see
/// `crate::shots`), named after the golden screenshots. `reply_attachment_mention`
/// is also listed by the composer's frame, whose fixture leaves the
/// suggestions empty; this one fills them.
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    use crate::shots::{GOLDEN, Shot};
    use anomi_vm::fixtures::chat::suggestions as fixture;
    let shot = |name, vm: fn() -> anomi_vm::Vm| Shot {
        area: "composer",
        name,
        size: GOLDEN,
        theme: "bonfire",
        vm,
    };
    vec![
        shot("mention_suggestions", fixture::mention),
        shot("emoji_autocomplete", fixture::emoji_autocomplete),
    ]
}
