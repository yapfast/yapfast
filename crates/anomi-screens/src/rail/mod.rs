//! The server rail: the DM home, a divider, the servers, and adding one.
//! The Slint side: the rail in `ui/rail.slint` (`RailItem`, the list of
//! `ServerRow`s), its rows from `rail_rows` in `src/ui/refresh.rs`.

use iced::widget::scrollable::{Direction, Scrollbar};
use iced::widget::{Column, column, container, rule, scrollable, text, tooltip};
use iced::{Fill, Length};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::rail::{Intent, Rail, Server};
use anomi_widgets::brand;
use anomi_widgets::icon::Icon;
use anomi_widgets::rail_item::{Face, rail_item};
use yapfast_lang::tr;

pub fn view(rail: &Rail, motion: Motion) -> impl View<Intent> + '_ {
    let home = rail_item(Face::Picture(brand::logo()), motion)
        .selected(rail.home.selected)
        .unread(rail.home.unread)
        .badge(&rail.home.badge)
        .on_press(Intent::Home);
    let divider = container(rule::horizontal(2).style(catalog::rule::line))
        .width(metrics::RAIL_WIDTH)
        .center_x(metrics::RAIL_WIDTH)
        .padding([0.0, 24.0]);
    let servers = Column::with_children(
        rail.servers
            .iter()
            .map(|server| server_item(server, motion)),
    )
    .spacing(metrics::RAIL_GAP);
    let list = column![tip(home, tr!("Direct messages")), divider, servers]
        .spacing(metrics::RAIL_GAP)
        .padding([metrics::RAIL_GAP, 0.0]);
    let add = rail_item(Face::Icon(Icon::Plus.handle()), motion);
    let scroll = scrollable(list)
        .direction(Direction::Vertical(Scrollbar::hidden()))
        .smooth_scroll(motion.playing())
        .height(Fill);
    container(column![scroll, tip(add, tr!("Join a server"))].padding(
        iced::Padding::default().bottom(metrics::ACCOUNT_HEIGHT + 2.0 * metrics::ACCOUNT_INSET),
    ))
    .width(Length::Fixed(metrics::RAIL_WIDTH))
    .height(Fill)
    .style(catalog::container::rail)
}

fn server_item(server: &Server, motion: Motion) -> Element<'_, Intent> {
    let face = match &server.avatar.picture {
        Some(picture) => Face::Picture(picture.clone()),
        None => Face::Initials(&server.avatar.initial),
    };
    let item = rail_item(face, motion)
        .selected(server.tile.selected)
        .unread(server.tile.unread)
        .badge(&server.tile.badge)
        .on_press(Intent::Server(server.id.clone()));
    tip(item, &server.name).boxed()
}

/// The window's tooltip to the right of a rail item.
fn tip<'a>(item: impl View<Intent> + 'a, label: &'a str) -> impl View<Intent> + 'a {
    let label = container(
        text(label)
            .size(typography::size::SMALL)
            .font(typography::SEMIBOLD),
    )
    .padding([6.0, 10.0])
    .style(catalog::container::tooltip);
    tooltip(item, label)
        .position(tooltip::Position::Right)
        .gap(metrics::SPACE_XS)
}
