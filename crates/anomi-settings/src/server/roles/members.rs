//! The Manage Members tab: a search with Add Members, and the people who
//! hold the role, each with the button that takes it from them. Task X09's
//! (docs/iced/tasks/settings.md). The Slint side: the third tab of
//! `RolesSection` (`ui/roles.slint:566`) and `RoleMemberLine` (`:312`).

use iced::widget::{column, container, row, text};
use iced::{Alignment, Length, Padding};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, typography};
use anomi_vm::icon::Icon;
use anomi_vm::settings::role_editor::{Intent, Member, RoleEditor};
use anomi_widgets::avatar::circle;
use anomi_widgets::button::{Kind, action_button, icon_button};
use anomi_widgets::field::field;
use anomi_widgets::list::list;
use yapfast_lang::tr;

use super::editor::label;

/// Between the page's parts, the members' lines among them.
const BESIDE: f32 = 12.0;
const CONTROL: f32 = 36.0;
const LINE: f32 = 44.0;
const AVATAR: f32 = 32.0;
const GAP: f32 = 10.0;
const NAMES: f32 = 6.0;
const RIGHT: f32 = 4.0;
const NAME: f32 = 15.0;

/// The names of a person: the name, then the username in quieter words.
pub(super) fn names<'a>(member: &'a Member) -> Element<'a, Intent> {
    row![
        text(member.name.as_str())
            .size(NAME)
            .font(typography::SEMIBOLD)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End)
            .style(catalog::text::primary),
        text(member.username.as_str())
            .size(typography::size::SMALL)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End)
            .style(catalog::text::muted),
    ]
    .spacing(NAMES)
    .width(Length::Fill)
    .boxed()
}

/// A member on the list: avatar, names, and the button that takes the role.
fn line<'a>(member: &'a Member, removable: bool, motion: Motion) -> Element<'a, Intent> {
    let tip = if removable {
        tr!("Remove member")
    } else {
        tr!("Members cannot be manually added or removed from this role")
    };
    let mut remove = icon_button(Icon::X, tip, motion).enabled(removable);
    if removable {
        remove = remove.on_press(Intent::RemoveMember(member.id.clone()));
    }
    row![
        circle(&member.avatar, AVATAR).boxed(),
        names(member),
        remove.view().boxed(),
    ]
    .spacing(GAP)
    .align_y(Alignment::Center)
    .padding(Padding::ZERO.right(RIGHT))
    .height(LINE)
    .boxed()
}

pub fn view<'a>(editor: &'a RoleEditor, motion: Motion) -> Element<'a, Intent> {
    let removable = !editor.saving && editor.may_add;
    let mut page = column![
        row![
            container(
                field(tr!("Search Members"), editor.member_search.as_str())
                    .on_input(Intent::MemberSearch)
                    .text_size(typography::size::SMALL)
                    .height(CONTROL)
                    .motion(motion)
                    .view()
            )
            .width(Length::Fill),
            action_button(tr!("Add Members"), Kind::Primary, motion)
                .enabled(removable)
                .on_press(Intent::AddMembers)
                .boxed(),
        ]
        .spacing(BESIDE)
        .align_y(Alignment::Center)
        .boxed()
    ]
    .spacing(BESIDE);
    if editor.not_all {
        page = page.push(label(tr!(
            "Not all members are shown, use Search to find specific members"
        )));
    }
    if editor.members_loading {
        page = page.push(label(tr!("Loading...")));
    }
    if !editor.members_error.is_empty() {
        page = page.push(
            text(editor.members_error.as_str())
                .size(typography::size::SMALL)
                .style(catalog::text::danger)
                .width(Length::Fill)
                .boxed(),
        );
    }
    let members = &editor.members;
    if !editor.members_loading && editor.members_error.is_empty() && members.is_empty() {
        page = page.push(label(tr!("No members were found.")));
    }
    page.push(
        list(
            members.len(),
            |at| members[at].id.parse().unwrap_or(at as u64),
            move |at| line(&members[at], removable, motion),
            motion,
        )
        .height(Length::Shrink)
        .spacing(BESIDE)
        .scrollbar(false)
        .estimate(LINE)
        .boxed(),
    )
    .boxed()
}
