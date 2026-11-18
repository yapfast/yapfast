//! The Permissions tab: a search with Clear permissions, and Discord's
//! permissions in their groups, each a switch with its words
//! (`ui/roles.slint:521`-`:565`). Only the rows in sight are built.

use iced::widget::{column, container, row, space, text};
use iced::{Alignment, Length, Padding};

use anomi_theme::prelude::*;
use anomi_theme::typography;
use anomi_vm::settings::role_editor::{Intent, Permission, RoleEditor};
use anomi_widgets::button::{Kind, action_button};
use anomi_widgets::field::field;
use anomi_widgets::list::list;
use anomi_widgets::tip::{Side, tip};
use yapfast_lang::tr;

use super::{GAP, label};
use crate::kit::{heading, switch};

const BESIDE: f32 = 12.0;
const CONTROL: f32 = 36.0;
/// A row's room: beside the scrollbar, above and below; a heading's room.
const RIGHT: f32 = 12.0;
const ABOVE: f32 = 4.0;
const BELOW: f32 = 12.0;
const HEADING_ABOVE: f32 = 16.0;
const HEADING_BELOW: f32 = 8.0;
const PARTS: f32 = 4.0;
const ESTIMATE: f32 = 72.0;

fn rule<'a>() -> Element<'a, Intent> {
    container(space())
        .width(Length::Fill)
        .height(1)
        .style(|theme: &Theme| container::Style {
            background: Some(theme.palette().line.into()),
            ..container::Style::default()
        })
        .boxed()
}

fn line<'a>(editor: &'a RoleEditor, row: &'a Permission, motion: Motion) -> Element<'a, Intent> {
    if row.heading {
        return container(heading(row.title.as_str()))
            .padding(Padding {
                top: HEADING_ABOVE,
                bottom: HEADING_BELOW,
                right: RIGHT,
                left: 0.0,
            })
            .width(Length::Fill)
            .boxed();
    }
    let bit = row.bit;
    // A locked role says why for all of them; a locked switch for itself.
    let why = if editor.locked {
        editor.notice.as_str()
    } else {
        row.lock.as_str()
    };
    let mut parts = column![
        switch(row.title.as_str(), row.on, motion)
            .description(row.description.as_str())
            .enabled(editor.editable() && row.lock.is_empty())
            .locked(why)
            .on_toggle(move |_| Intent::Permission(bit))
            .view()
    ]
    .spacing(PARTS);
    if !row.warning.is_empty() {
        parts = parts.push(
            text(row.warning.as_str())
                .size(typography::size::SMALL)
                .font(typography::BOLD)
                .style(anomi_theme::catalog::text::primary)
                .width(Length::Fill)
                .boxed(),
        );
    }
    column![parts.boxed(), rule()]
        .spacing(PARTS)
        .padding(Padding {
            top: ABOVE,
            bottom: BELOW,
            right: RIGHT,
            left: 0.0,
        })
        .boxed()
}

fn clear<'a>(editor: &'a RoleEditor, motion: Motion) -> Element<'a, Intent> {
    let button = action_button(tr!("Clear permissions"), Kind::Ghost, motion)
        .enabled(editor.editable() && editor.may_clear)
        .on_press(Intent::ClearPermissions)
        .boxed();
    if editor.clear_lock.is_empty() {
        button
    } else {
        tip(button, editor.clear_lock.as_str(), Side::Above, motion).boxed()
    }
}

pub fn view<'a>(editor: &'a RoleEditor, motion: Motion) -> Element<'a, Intent> {
    let rows = &editor.permissions;
    let search = row![
        container(
            field(tr!("Search permissions"), editor.permission_search.as_str())
                .on_input(Intent::PermissionSearch)
                .text_size(typography::size::SMALL)
                .height(CONTROL)
                .motion(motion)
                .view()
        )
        .width(Length::Fill),
        clear(editor, motion),
    ]
    .spacing(BESIDE)
    .align_y(Alignment::Center);
    let mut page = column![search.boxed()].spacing(GAP).height(Length::Fill);
    if rows.is_empty() {
        page = page.push(label(tr!("No permissions found")));
    }
    page.push(
        list(
            rows.len(),
            |at| {
                let row = &rows[at];
                if row.heading {
                    (1 << 32) + at as u64
                } else {
                    u64::from(row.bit)
                }
            },
            move |at| line(editor, &rows[at], motion),
            motion,
        )
        .estimate(ESTIMATE)
        .height(Length::Fill)
        .boxed(),
    )
    .boxed()
}
