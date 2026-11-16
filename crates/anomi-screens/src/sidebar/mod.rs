//! The column beside the rail: a header (the search field, or the server's
//! name), then the DM list or the server's channels. The Slint side: the
//! sidebar in `ui/yapfast.slint` and the lists in `ui/rail.slint`.

mod channels;
mod dm_row;
mod dms;

use iced::widget::{column, container, rule, scrollable, text};
use iced::{Fill, Padding};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::sidebar::{Intent, Place, Sidebar};
use anomi_widgets::field;
use yapfast_lang::tr;

pub fn view(sidebar: &Sidebar) -> impl View<Intent> + '_ {
    let header = match &sidebar.place {
        Place::Dms(_) => container(
            field::search(tr!("Find a conversation"), &sidebar.filter, Intent::Filter).width(Fill),
        )
        .padding([0.0, metrics::SPACE_S + 2.0])
        .center_y(metrics::HEADER_HEIGHT - 1.0)
        .boxed(),
        Place::Server(server) => container(
            text(server.name.as_str())
                .size(typography::size::TITLE)
                .font(typography::SEMIBOLD),
        )
        .padding([0.0, metrics::SPACE_L])
        .center_y(metrics::HEADER_HEIGHT - 1.0)
        .boxed(),
    };
    let body = match &sidebar.place {
        Place::Dms(dms) => dms::view(dms).boxed(),
        Place::Server(server) => channels::view(server).boxed(),
    };
    // Room at the foot for the account card, which floats over it.
    let foot = metrics::ACCOUNT_HEIGHT + 2.0 * metrics::ACCOUNT_INSET;
    let list = scrollable(container(body).padding(Padding::new(metrics::ROW_INSET).bottom(foot)))
        .height(Fill)
        .width(Fill);
    column![
        header,
        rule::horizontal(1).style(catalog::rule::outline),
        list
    ]
    .width(Fill)
    .height(Fill)
}
