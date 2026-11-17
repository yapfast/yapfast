//! A server's settings frame and its profile page. Task X01
//! (docs/iced/tasks/settings.md). The Slint side: `ServerSettingsScreen`
//! (`ui/server_settings.slint:320`).
//!
//! - [`window`] draws the whole server's settings: the frame (the same
//!   [`pane`](crate::frame::pane) as the person's settings) with the
//!   sections of the account's rights on its left and, under a bar with the
//!   page's title, the page `section` names from the pages' own view-models.
//!   The shell calls it for `Layer::ServerSettings`; until it does, it calls
//!   [`view`], which draws the profile and leaves another page empty.
//! - **Escape** and a click on the dimmed window close it: the layer's.
//! - Members and Invites take a wide column; a page that scrolls on its own
//!   (`fill`) takes the frame's height. A bar for unsaved changes lies over
//!   the page's bottom.

pub mod audit;
pub mod bans;
pub mod boosts;
pub mod emoji;
pub mod engagement;
pub mod invites;
pub mod members;
pub mod ownership;
mod profile;
pub mod roles;
pub mod safety;
pub mod stickers;

use iced::widget::{column, container, rule, text};
use iced::{Length, Padding, alignment};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, typography};
use anomi_vm::icon::Icon;
use anomi_vm::settings::server::{Intent as ServerIntent, Section, Server};
use anomi_vm::settings::{self, Settings};
use anomi_widgets::list::scroll_view;
use yapfast_lang::tr;

use crate::frame::pane;
use crate::kit::{WIDTH, chat_ground, heading, nav_row};

/// The gap between the page's lines (`spacing: 16px`).
const GAP: f32 = 16.0;
/// The sections' insets, as [`kit::sections`](crate::kit::sections) has
/// them: 16 px beside, 12 above, 16 below; a heading's inset, its room above
/// past the first and its height; the line before Delete Server.
const SIDES: f32 = 16.0;
const TOP: f32 = 12.0;
const BOTTOM: f32 = 16.0;
const HEADING_INSET: f32 = 8.0;
const HEADING_GAP: f32 = 16.0;
const HEADING_FIRST: f32 = 24.0;
const HEADING_NEXT: f32 = 40.0;
const LINE_SIDES: f32 = 8.0;
const LINE_ABOVE: f32 = 12.0;

/// The section's name in the sections.
fn name(section: Section) -> &'static str {
    match section {
        Section::Profile => tr!("Server Profile"),
        Section::Engagement => tr!("Engagement"),
        Section::BoostPerks => tr!("Boost Perks"),
        Section::Members => tr!("Members"),
        Section::Roles => tr!("Roles"),
        Section::Invites => tr!("Invites"),
        Section::Safety => tr!("Safety Setup"),
        Section::AuditLog => tr!("Audit Log"),
        Section::Bans => tr!("Bans"),
        Section::Emoji => tr!("Emoji"),
        Section::Stickers => tr!("Stickers"),
    }
}

/// The section's title in the bar.
fn title(section: Section) -> &'static str {
    match section {
        Section::Bans => tr!("Server Ban List"),
        Section::Members => tr!("Server Members"),
        _ => name(section),
    }
}

/// The section's icon in the sections (`src/ui.rs`' `Icons`).
fn icon(section: Section) -> Icon {
    match section {
        Section::Profile => Icon::Pencil,
        Section::Engagement => Icon::Bell,
        Section::BoostPerks => Icon::Pin,
        Section::Members => Icon::Users,
        Section::Roles => Icon::Shield,
        Section::Invites => Icon::Link,
        Section::Safety => Icon::Lock,
        Section::AuditLog => Icon::History,
        Section::Bans => Icon::Gavel,
        Section::Emoji => Icon::Smile,
        Section::Stickers => Icon::StickyNote,
    }
}

/// The group heading a section stands under: Discord's Expression, People
/// and Moderation; the server's own name for the first three.
fn group(section: Section) -> Option<&'static str> {
    match section {
        Section::Emoji | Section::Stickers => Some(tr!("EXPRESSION")),
        Section::Members | Section::Roles | Section::Invites => Some(tr!("PEOPLE")),
        Section::Safety | Section::AuditLog | Section::Bans => Some(tr!("MODERATION")),
        _ => None,
    }
}

/// A line of the page in red: what Discord refused or failed at.
fn problem<'a, M: 'a>(words: &'a str) -> Element<'a, M> {
    text(words)
        .size(typography::size::SMALL)
        .style(catalog::text::danger)
        .width(Length::Fill)
        .boxed()
}

/// The page the section names, from its own module's view; the stubs of
/// the other tasks draw nothing until they are done.
fn page<'a>(settings: &'a Settings, motion: Motion) -> Element<'a, settings::Intent> {
    use settings::Intent;
    let server = &settings.server;
    match server.section {
        Section::Profile => profile::view(server, motion).map(Intent::from).boxed(),
        Section::Engagement => engagement::view(&settings.engagement, motion)
            .map(Intent::from)
            .boxed(),
        Section::BoostPerks => boosts::view(&settings.boosts, motion)
            .map(Intent::from)
            .boxed(),
        // An open role has its own title: its editor takes the page.
        Section::Roles if server.title.is_some() => {
            roles::editor::view(&settings.role_editor, motion)
                .map(Intent::from)
                .boxed()
        }
        Section::Roles => roles::list::view(&settings.roles, motion)
            .map(Intent::from)
            .boxed(),
        Section::Safety => safety::view(&settings.safety, motion)
            .map(Intent::from)
            .boxed(),
        Section::Bans => bans::view(&settings.bans, motion).map(Intent::from).boxed(),
        Section::AuditLog => audit::view(&settings.audit, motion)
            .map(Intent::from)
            .boxed(),
        Section::Members => members::view(&settings.members, motion)
            .map(Intent::from)
            .boxed(),
        Section::Invites => invites::view(&settings.invites, motion)
            .map(Intent::from)
            .boxed(),
        Section::Emoji => emoji::view(&settings.emoji, motion)
            .map(Intent::from)
            .boxed(),
        Section::Stickers => stickers::view(&settings.stickers, motion)
            .map(Intent::from)
            .boxed(),
    }
}

/// The page's column: "Loading..." while the data is on its way, the page
/// once it is here, then what was refused or failed.
fn body<'a>(
    server: &'a Server,
    page: Option<Element<'a, settings::Intent>>,
) -> Element<'a, settings::Intent> {
    let mut lines: Vec<Element<'a, settings::Intent>> = Vec::with_capacity(4);
    if server.loading {
        lines.push(
            text(tr!("Loading..."))
                .size(typography::size::HEADING)
                .style(catalog::text::muted)
                .boxed(),
        );
    }
    lines.extend(page);
    if !server.problem.is_empty() {
        lines.push(problem(&server.problem));
    }
    if !server.error.is_empty() {
        lines.push(problem(&server.error));
    }
    column(lines).spacing(GAP).width(Length::Fill).boxed()
}

/// The whole server's settings: the frame with the page the server names.
pub fn window(settings: &Settings, motion: Motion) -> impl View<settings::Intent> + '_ {
    let server = &settings.server;
    frame(
        server,
        move || body(server, server.loaded.then(|| page(settings, motion))),
        motion,
    )
}

/// The frame with the page this module's view-model holds, the profile;
/// another section's page stays empty: what the shell shows until it calls
/// [`window`].
pub fn view(server: &Server, motion: Motion) -> impl View<settings::Intent> + '_ {
    frame(
        server,
        move || {
            let page = server.loaded && server.section == Section::Profile;
            body(
                server,
                page.then(|| {
                    profile::view(server, motion)
                        .map(settings::Intent::from)
                        .boxed()
                }),
            )
        },
        motion,
    )
}

/// A group's heading in the sections, the first one closer to the top.
fn group_heading<'a>(words: impl text::IntoFragment<'a>, first: bool) -> Element<'a, ServerIntent> {
    container(heading(words))
        .padding(Padding {
            left: HEADING_INSET,
            top: if first { 0.0 } else { HEADING_GAP },
            ..Padding::ZERO
        })
        .height(if first { HEADING_FIRST } else { HEADING_NEXT })
        .align_y(alignment::Vertical::Center)
        .boxed()
}

/// The sections, laid out as [`kit::sections`](crate::kit::sections) does:
/// the server's name, then each group under its heading and, for the owner,
/// Delete Server after a line. A heading with nothing under it goes, as the
/// server's name does for someone who may only manage roles.
fn navigation(vm: &Server, motion: Motion) -> Element<'_, ServerIntent> {
    let mut entries: Vec<Element<'_, ServerIntent>> = Vec::with_capacity(vm.sections.len() + 6);
    let mut under = None;
    if let Some(&first) = vm.sections.first()
        && group(first).is_none()
    {
        entries.push(group_heading(vm.name.to_uppercase(), true));
    }
    for &section in &vm.sections {
        let heading = group(section);
        if heading != under {
            under = heading;
            if let Some(words) = heading {
                entries.push(group_heading(words, entries.is_empty()));
            }
        }
        entries.push(nav_row(
            icon(section),
            name(section),
            section == vm.section,
            false,
            false,
            ServerIntent::Choose(section),
            motion,
        ));
    }
    if vm.may_delete {
        entries.push(
            container(rule::horizontal(1).style(catalog::rule::line))
                .padding(Padding {
                    top: LINE_ABOVE,
                    bottom: LINE_ABOVE,
                    left: LINE_SIDES,
                    right: LINE_SIDES,
                })
                .boxed(),
        );
        entries.push(nav_row(
            Icon::Trash,
            tr!("Delete Server"),
            false,
            true,
            false,
            ServerIntent::Delete,
            motion,
        ));
    }
    let list = column(entries).padding(Padding {
        top: TOP,
        bottom: BOTTOM,
        left: SIDES,
        right: SIDES,
    });
    container(scroll_view(list, motion))
        .width(Length::Fixed(WIDTH))
        .height(Length::Fill)
        .style(chat_ground)
        .boxed()
}

fn frame<'a>(
    vm: &'a Server,
    page: impl Fn() -> Element<'a, settings::Intent> + 'a,
    motion: Motion,
) -> impl View<settings::Intent> + 'a {
    let from = settings::Intent::from;
    let title = vm.title.as_deref().unwrap_or_else(|| title(vm.section));
    let wide = matches!(vm.section, Section::Members | Section::Invites);
    pane(
        move || navigation(vm, motion).map(from).boxed(),
        page,
        title,
        &vm.close,
        from(ServerIntent::Close),
        motion,
    )
    .detail(&vm.detail)
    .wide(wide)
    .fill(vm.loaded && vm.fill)
    .key(vm.section as u64)
    .unsaved(
        vm.unsaved.as_ref(),
        from(ServerIntent::Undo),
        from(ServerIntent::Save),
    )
    .view()
}

/// Its shots for `anomi-shots`: `ssettings/profile.png` as the Slint app
/// showed it, and the owner's profile, Delete Server in the sections, with
/// the bar for changes not saved.
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    use crate::shots::{GOLDEN, Shot};
    use anomi_vm::fixtures::settings::server::{edited, profile};
    vec![
        Shot {
            area: "ssettings",
            name: "profile",
            size: GOLDEN,
            theme: "bonfire",
            vm: profile,
        },
        Shot {
            area: "ssettings",
            name: "unsaved",
            size: GOLDEN,
            theme: "bonfire",
            vm: || {
                let mut vm = profile();
                vm.settings.server = edited();
                vm
            },
        },
    ]
}
