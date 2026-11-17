//! The inbox popout under the bar's Inbox button: Discord's first two tabs,
//! Unreads and Mentions, over the rooms with something waiting. The Slint
//! side: `InboxPopout` in `ui/yapfast/inbox.slint:76`. The shell places it,
//! closes it on Escape or a press outside, and keeps focus in it. Task N08
//! (docs/iced/tasks/shell.md).

mod line;
mod tab;

use iced::widget::{column, container, row, space, text};
use iced::{Alignment, Background, Border, Color, Fill, Shadow, Vector};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::inbox::{Inbox, Intent, Tab};
use anomi_widgets::list::list;
use yapfast_lang::tr;

/// Discord's popout: 440 wide, 560 tall.
pub const WIDTH: f32 = 440.0;
pub const HEIGHT: f32 = 560.0;
/// The title bar of the popout, and its side padding.
const BAR: f32 = 48.0;
/// The empty notice's room and its inset from the popout's sides.
const NOTICE: f32 = 120.0;

const SHADOW: Shadow = Shadow {
    color: Color::from_rgba(0.0, 0.0, 0.0, 0.4),
    offset: Vector::new(0.0, 4.0),
    blur_radius: 16.0,
};

pub fn view(inbox: &Inbox, motion: Motion) -> impl View<Intent> + '_ {
    let bar = row![
        text(tr!("Inbox"))
            .size(typography::size::TITLE)
            .font(typography::BOLD),
        space().width(Fill),
        tab::view(tr!("Unreads"), inbox.tab == Tab::Unreads, Tab::Unreads),
        tab::view(tr!("Mentions"), inbox.tab == Tab::Mentions, Tab::Mentions),
    ]
    .spacing(metrics::SPACE_S)
    .align_y(Alignment::Center)
    .padding([0.0, metrics::SPACE_L])
    .height(BAR);
    let rule = container(space())
        .width(Fill)
        .height(1)
        .style(|theme: &Theme| container::Style {
            background: Some(Background::Color(theme.palette().line)),
            ..container::Style::default()
        });
    let body: Element<'_, Intent> = if inbox.rows.is_empty() {
        container(
            text(match inbox.tab {
                Tab::Unreads => tr!("Unread messages from your unmuted channels show up here."),
                Tab::Mentions => tr!("Nothing mentions you right now."),
            })
            .size(typography::size::SMALL)
            .style(catalog::text::muted)
            .align_x(iced::alignment::Horizontal::Center)
            .width(Fill),
        )
        .padding(metrics::SPACE_L)
        .height(NOTICE + 2.0 * metrics::SPACE_L)
        .width(Fill)
        .align_y(iced::alignment::Vertical::Center)
        .boxed()
    } else {
        list(
            inbox.rows.len(),
            |at| line::key(&inbox.rows[at]),
            |at| line::view(&inbox.rows[at]),
            motion,
        )
        .id(iced::widget::Id::new(match inbox.tab {
            Tab::Unreads => "inbox-unreads",
            Tab::Mentions => "inbox-mentions",
        }))
        .boxed()
    };
    container(column![bar, rule, container(body).height(Fill)])
        .width(WIDTH)
        .height(HEIGHT)
        .clip(true)
        .style(|theme: &Theme| {
            let palette = theme.palette();
            container::Style {
                background: Some(Background::Color(palette.raised)),
                border: Border::default()
                    .rounded(metrics::RADIUS_CARD)
                    .width(1)
                    .color(palette.line),
                shadow: SHADOW,
                ..container::Style::default()
            }
        })
}

/// Its shots for `anomi-shots`: no golden shot, the Slint app's popout has
/// no entry in `golden/index.md`.
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    use anomi_vm::fixtures::{GOLDEN, Shot, inbox};

    fn shot(name: &'static str, vm: fn() -> anomi_vm::Vm) -> Shot {
        Shot {
            area: "inbox",
            name,
            size: GOLDEN,
            theme: "bonfire",
            vm,
        }
    }
    vec![
        shot("unreads", || inbox::open(inbox::unreads())),
        shot("mentions", || inbox::open(inbox::mentions())),
        shot("empty_unreads", || inbox::open(inbox::empty(Tab::Unreads))),
        shot("empty_mentions", || {
            inbox::open(inbox::empty(Tab::Mentions))
        }),
    ]
}
