//! The column beside the rail: a header (the search field, or the server's
//! header), then the DM list or the server's channels. The Slint side: the
//! sidebar in `ui/yapfast.slint` and the lists in `ui/rail.slint`.
//!
//! F01 keeps this file, which composes the parts: [`dms`] (N03),
//! [`channels`] (N04) and [`server_header`] (N05) fill their own modules.
//! Each list scrolls itself (F07's virtual list replaces the skeleton's
//! `scrollable`) and leaves room at its foot for the account card.

pub mod channels;
pub mod dms;
pub mod server_header;

use iced::Fill;
use iced::widget::{column, container, rule};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics};
use anomi_vm::sidebar::{Intent, Place, Sidebar};
use anomi_widgets::field;
use yapfast_lang::tr;

pub fn view(sidebar: &Sidebar, motion: Motion) -> impl View<Intent> + '_ {
    let header = match &sidebar.place {
        Place::Dms(_) => container(field::search(
            tr!("Find a conversation"),
            &sidebar.filter,
            Intent::Filter,
            motion,
        ))
        .padding([0.0, metrics::SPACE_S + 2.0])
        .center_y(metrics::HEADER_HEIGHT - 1.0)
        .boxed(),
        Place::Server(server) => server_header::view(&server.header, motion)
            .map(Intent::from)
            .boxed(),
    };
    let list = match &sidebar.place {
        Place::Dms(dms) => dms::view(dms, motion).map(Intent::from).boxed(),
        Place::Server(server) => channels::view(&server.channels, motion)
            .map(Intent::from)
            .boxed(),
    };
    // A server's header draws its own rules (N05: full width under a plain
    // name, 16 px in under the events row); the DMs' search has this one.
    let rule = matches!(sidebar.place, Place::Dms(_))
        .then(|| rule::horizontal(1).style(catalog::rule::outline).boxed());
    column![header]
        .extend(rule)
        .push(list)
        .width(Fill)
        .height(Fill)
}
