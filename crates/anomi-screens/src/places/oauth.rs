//! Authorizing a Discord application from its link, as Discord's
//! authorization page does: what it asks for shows before anyone agrees.
//! The Slint side: `OAuthDialog` (`ui/server.slint:395`).

use iced::widget::pick_list;
use iced::{Length, Padding};

use anomi_theme::metrics::SPACE_M;
use anomi_theme::prelude::*;
use anomi_theme::typography;
use anomi_vm::places::oauth::{Intent, OAuth, Servers};
use yapfast_lang::tr;

use super::invite::frame::{
    WELL, body, buttons, confirm, copyable, dialog, failure, found, heading, leave, line, note,
    warning, well,
};

/// The card's width at least (`:411`).
const WIDTH: f32 = 440.0;

/// The picker's padding above and below its text: a control's 36 px with the
/// line (`min-height: 36px`, `ui/settings.slint:206`).
const VERTICAL: f32 = 8.9;

/// A server in the picker: its place in the list, which is what is sent,
/// and its name, which is what shows.
#[derive(Clone, PartialEq)]
struct Pick<'a> {
    index: usize,
    name: &'a str,
}

/// The drop-down of servers it could be added to (`Picker`).
fn picker<'a>(servers: &'a Servers, busy: bool) -> Element<'a, Intent> {
    let names: Vec<Pick<'a>> = servers
        .names
        .iter()
        .enumerate()
        .map(|(index, name)| Pick { index, name })
        .collect();
    let chosen = names.get(servers.chosen).cloned();
    let mut list = pick_list(chosen, names, |pick: &Pick<'_>| pick.name.to_owned())
        .width(Length::Fill)
        .padding(Padding::from([VERTICAL, SPACE_M]))
        .text_size(typography::size::SMALL)
        .handle(pick_list::Handle::Arrow {
            size: Some(12.0.into()),
        });
    if !busy {
        list = list.on_select(|pick: Pick<'_>| Intent::ChooseServer(pick.index));
    }
    list.boxed()
}

pub fn view(oauth: &OAuth, motion: Motion) -> impl View<Intent> + '_ {
    let mut parts: Vec<Element<'_, Intent>> = Vec::new();
    if let Some(app) = &oauth.app {
        parts.push(found(Some(&app.avatar), &app.name, &[&app.detail]));
    }
    if !oauth.warning.is_empty() {
        parts.push(warning(&oauth.warning));
    }
    if !oauth.wants.is_empty() {
        parts.push(body(&oauth.wants));
    }
    if oauth.servers.is_some() || !oauth.server.is_empty() {
        parts.push(heading(tr!("ADD TO SERVER")));
    }
    if let Some(servers) = &oauth.servers {
        parts.push(picker(servers, oauth.busy));
    } else if !oauth.server.is_empty() {
        let name = line(&oauth.server, typography::size::BODY, false);
        let padded: Element<'_, Intent> = iced::widget::container(name)
            .padding(Padding::from([0.0, SPACE_M]))
            .boxed();
        parts.push(well(padded, Some(WELL), false));
    }
    if !oauth.status.is_empty() {
        parts.push(note(&oauth.status));
    }
    if !oauth.address.is_empty() {
        parts.push(copyable(
            line(&oauth.address, typography::size::BODY, false),
            false,
            oauth.copied,
            true,
            Intent::Copy(oauth.address.clone()),
            motion,
        ));
    }
    if !oauth.error.is_empty() {
        parts.push(failure(&oauth.error));
    }
    let mut ends = vec![leave(oauth.confirm.is_empty(), Intent::Close, motion)];
    if !oauth.confirm.is_empty() {
        ends.push(confirm(
            &oauth.confirm,
            oauth.busy,
            Intent::Authorize,
            motion,
        ));
    }
    parts.push(buttons(ends));
    dialog(&oauth.title, WIDTH, Intent::Close, motion, parts)
}

#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    Vec::new()
}
