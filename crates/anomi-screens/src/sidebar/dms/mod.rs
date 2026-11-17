//! The DM home's list: Friends and Requests, the "Direct messages" heading,
//! then the DMs. The Slint side: `DmHeading` in `ui/yapfast/dm-heading.slint`
//! and the DM rows of the sidebar's list. Task N03
//! (docs/iced/tasks/shell.md) takes it further.

mod dm_row;

use iced::widget::{Column, button, column, container, row, scrollable, space, text};
use iced::{Alignment, Background, Fill, Padding};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::sidebar::dms::{Dms, Intent};
use anomi_widgets::badge::badge;
use anomi_widgets::icon::{Icon, icon};
use yapfast_lang::tr;

pub fn view(dms: &Dms, _motion: Motion) -> impl View<Intent> + '_ {
    let friends = nav_row(
        Icon::Users,
        tr!("Friends"),
        "",
        dms.friends_selected,
        Intent::Friends,
    );
    let requests = nav_row(
        Icon::Inbox,
        tr!("Requests"),
        &dms.requests_badge,
        dms.requests_selected,
        Intent::Requests,
    );
    let heading = row![
        text(tr!("Direct messages"))
            .size(typography::size::HEADING)
            .font(typography::SEMIBOLD)
            .style(catalog::text::muted),
        space().width(Fill),
        icon(Icon::UserPlus, 16.0),
        icon(Icon::SquarePen, 16.0),
    ]
    .spacing(metrics::SPACE_M)
    .align_y(Alignment::Center)
    .padding(iced::Padding {
        top: metrics::SPACE_L,
        bottom: metrics::SPACE_S,
        left: metrics::SPACE_S,
        right: metrics::SPACE_XS,
    });
    let rows = Column::with_children(dms.rows.iter().map(dm_row::view)).spacing(2);
    let list = column![friends, requests, heading, rows].spacing(2);
    // Room at the foot for the account card, which floats over the list.
    let foot = metrics::ACCOUNT_HEIGHT + 2.0 * metrics::ACCOUNT_INSET;
    scrollable(container(list).padding(Padding::new(metrics::ROW_INSET).bottom(foot)))
        .height(Fill)
        .width(Fill)
}

/// Friends or Requests: an icon, a label and a count.
fn nav_row<'a>(
    glyph: Icon,
    label: &'a str,
    count: &'a str,
    selected: bool,
    intent: Intent,
) -> impl View<Intent> + 'a {
    let content = row![
        icon(glyph, metrics::ICON_ROW).style(anomi_theme::catalog::svg::icon),
        text(label)
            .size(typography::size::BODY)
            .font(typography::MEDIUM),
        space().width(Fill),
    ]
    .push_maybe_badge(count)
    .spacing(metrics::SPACE_M)
    .align_y(Alignment::Center);
    button(container(content).center_y(metrics::NAV_ROW_HEIGHT))
        .padding([0.0, metrics::SPACE_S])
        .width(Fill)
        .on_press(intent)
        .style(move |theme: &Theme, status| row_style(theme, status, selected, false))
}

/// A sidebar row's look: the active fill when selected, the hover fill under
/// the pointer, and its text brighter as either, or when unread.
fn row_style(theme: &Theme, status: button::Status, selected: bool, unread: bool) -> button::Style {
    let palette = theme.palette();
    let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
    let background = if selected {
        Some(Background::Color(palette.active))
    } else if hovered {
        Some(Background::Color(palette.hover))
    } else {
        None
    };
    button::Style {
        background,
        text_color: if selected || unread {
            palette.text
        } else if hovered {
            palette.secondary
        } else {
            palette.muted
        },
        border: iced::Border::default().rounded(metrics::RADIUS_ROW),
        ..button::Style::default()
    }
}

/// Appends a count badge to a row when there is one.
trait PushBadge<'a> {
    fn push_maybe_badge(self, count: &'a str) -> Self;
}

impl<'a> PushBadge<'a> for iced::widget::Row<Element<'a, Intent>> {
    fn push_maybe_badge(self, count: &'a str) -> Self {
        if count.is_empty() {
            self
        } else {
            self.push(badge(count).boxed())
        }
    }
}

/// Its shots for `anomi-shots`: none until N03 adds them, each the whole
/// window from a fixture (see `crate::shots`).
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    Vec::new()
}
