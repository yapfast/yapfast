//! The server's header over its channels: its name, badge, banner, invite
//! button, boost goal and events row (`show_sidebar`,
//! `src/ui/refresh.rs:1071`-`:1124`). Discord's look draws all of them, as
//! the iced interface always does.

use yapfast_core::application::{AppState, Application, MenuChoice};
use yapfast_lang::tr;

use anomi_vm::kind::Expected;
use anomi_vm::sidebar::server_header::ServerHeader;

use crate::pictures;

pub(super) fn build(core: &Application, id: &str) -> ServerHeader {
    let state = &core.state;
    let Some(server) = state.servers.iter().find(|server| server.id == id) else {
        return ServerHeader {
            name: tr!("Server").to_owned(),
            ..ServerHeader::default()
        };
    };
    let banner = match server.banner.as_deref() {
        None | Some("") => Expected::None,
        Some(address) => {
            pictures::find(state, Some(address)).map_or(Expected::Waiting, Expected::Ready)
        }
    };
    ServerHeader {
        name: server.name.clone(),
        community: server.community,
        verified: server.verified,
        banner,
        invite: core.header_choices(id).contains(&MenuChoice::Invite),
        boosts: server.boosts,
        // A Community server always has the row, with none as "Events".
        events: (server.community || !server.events.is_empty())
            .then(|| u32::try_from(server.events.len()).unwrap_or(u32::MAX)),
        ..ServerHeader::default()
    }
}

/// The banner's address, to ask for as the header shows it.
pub(super) fn banner<'a>(state: &'a AppState, id: &str) -> Option<&'a str> {
    state
        .servers
        .iter()
        .find(|server| server.id == id)
        .and_then(|server| server.banner.as_deref())
}
