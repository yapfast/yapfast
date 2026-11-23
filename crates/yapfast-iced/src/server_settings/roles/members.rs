//! A role's Manage Members tab and Add members over the settings.

use crate::widgets::scroll_view;
use iced::{
    Alignment, Color, Element, Length, Padding, Task,
    widget::{Space, button, column, container, operation, row, text},
};
use yapfast_core::{
    application::{ADD_MEMBERS_LIMIT, RoleInput, RoleLock, RolesPage},
    chat::{RoleDetail, RoleMember},
};

use super::{
    ADD_HEIGHT, ADD_ROOM, BUTTON, CANDIDATE_AVATAR, CANDIDATES, DIALOG, GLYPH, MEMBER,
    MEMBER_AVATAR, MEMBER_SEARCH, input, search,
};
use crate::server_settings::View;
use crate::{
    app::Message,
    controls::{self, Kind, action},
    fonts::{BOLD, SEMIBOLD},
    form,
    pictures::Icon,
    style::scrollbar,
};

/// Add members' search, which takes the keys as it opens.
pub fn focus_candidates() -> Task<Message> {
    operation::focus(iced::widget::Id::new(CANDIDATES))
}

impl<'a> View<'a> {
    /// Manage Members: the search, Add Members, and those who hold it,
    /// each with the button that takes it from them.
    pub(super) fn role_members(
        self,
        page: &'a RolesPage,
        role: &'a RoleDetail,
        lock: Option<RoleLock>,
        everyone: bool,
        ready: bool,
    ) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let Some(draft) = &page.open else {
            return Space::new().into();
        };
        let may_add = lock.is_none() && !everyone && role.managed.is_none();
        let mut parts = column![
            row![
                search(
                    theme,
                    MEMBER_SEARCH,
                    "Search Members",
                    &draft.member_search,
                    RoleInput::MemberSearch
                ),
                action(
                    theme,
                    "Add Members",
                    Kind::Primary,
                    (ready && may_add).then(|| input(RoleInput::AddMembers)),
                )
            ]
            .spacing(12)
            .align_y(Alignment::Center)
        ]
        .spacing(12);
        let not_all = draft.member_search.is_empty()
            && matches!(&draft.members, Some(Ok(members)) if (members.len() as u64) < role.members);
        if not_all {
            parts = parts.push(form::note(
                theme,
                "Not all members are shown, use Search to find specific members",
            ));
        }
        let shown = page.shown_members();
        match &draft.members {
            None => parts = parts.push(form::note(theme, "Loading...")),
            Some(Err(error)) => {
                parts = parts.push(form::error(theme, error.as_str()));
            }
            Some(Ok(_)) if shown.is_empty() => {
                parts = parts.push(form::note(theme, "No members were found."));
            }
            Some(Ok(_)) => {}
        }
        let removable = ready && may_add;
        for member in shown {
            parts = parts.push(self.role_member(member, removable));
        }
        parts.into()
    }

    /// A member on Manage Members: avatar, names, and the X that takes the
    /// role from them.
    pub(super) fn role_member(
        self,
        member: &'a RoleMember,
        removable: bool,
    ) -> Element<'a, Message> {
        row![
            self.cx.avatar(
                Some(member.avatar.as_str()).filter(|url| !url.is_empty()),
                &member.name,
                MEMBER_AVATAR,
                MEMBER_AVATAR / 2.0,
                1.0
            ),
            self.member_names(member),
            self.cx
                .look
                .icon_button(
                    Icon::X,
                    if removable {
                        "Remove member"
                    } else {
                        "Members cannot be manually added or removed from this role"
                    },
                    removable.then(|| input(RoleInput::RemoveMember(member.id.clone())))
                )
                .size(BUTTON, GLYPH)
                .fill(Color::TRANSPARENT),
        ]
        .spacing(10)
        .padding(Padding {
            right: 4.0,
            ..Padding::ZERO
        })
        .height(MEMBER)
        .align_y(Alignment::Center)
        .into()
    }

    /// A member's name and username on one line.
    pub(super) fn member_names(self, member: &'a RoleMember) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let mut names = row![
            text(member.name.as_str())
                .size(15)
                .font(SEMIBOLD)
                .color(theme.text)
                .wrapping(text::Wrapping::None)
        ]
        .spacing(6)
        .align_y(Alignment::Center);
        if let Some(username) = &member.username {
            names = names.push(
                text(username.as_str())
                    .size(14)
                    .color(theme.muted)
                    .wrapping(text::Wrapping::None),
            );
        }
        container(names).width(Length::Fill).clip(true).into()
    }

    /// Add members: a search of the server's members, each with a box to
    /// pick them, up to thirty, then Add.
    pub(super) fn add_members(self, page: &'a RolesPage) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let Some(adding) = &page.adding else {
            return Space::new().into();
        };
        let role = page.open.as_ref().map_or("", |draft| draft.name.as_str());
        let chosen = |id: &str| adding.chosen.iter().any(|member| member.id == id);
        let extra = adding
            .chosen
            .iter()
            .filter(|member| !adding.found.iter().any(|found| found.id == member.id));
        let list = column(
            adding
                .found
                .iter()
                .chain(extra)
                .map(|member| self.candidate(member, chosen(&member.id))),
        );
        let picked = adding.chosen.len();
        let height = ADD_HEIGHT.min(self.cx.look.window.height - ADD_ROOM);
        let body = column![
            text("Add members").size(20).font(BOLD).color(theme.text),
            text(format!("Select up to 30 members to add to role {role}"))
                .size(14)
                .color(theme.secondary),
            search(
                theme,
                CANDIDATES,
                "Search members",
                &adding.query,
                RoleInput::FindCandidates
            ),
            scroll_view(list)
                .style(scrollbar(theme))
                .height(Length::Fill),
            row![
                Space::new().width(Length::Fill),
                action(
                    theme,
                    "Cancel",
                    Kind::Ghost,
                    Some(input(RoleInput::CloseAdding))
                ),
                action(
                    theme,
                    "Add",
                    Kind::Primary,
                    (picked > 0 && picked <= ADD_MEMBERS_LIMIT)
                        .then(|| input(RoleInput::AddPicked)),
                ),
            ]
            .spacing(8)
        ]
        .spacing(12)
        .padding(16)
        .height(Length::Fill);
        crate::theme_art::panel_dialog(
            self.cx,
            body,
            DIALOG,
            Some(height),
            input(RoleInput::CloseAdding),
        )
    }

    /// Someone in Add members: their box, avatar and names; a click picks
    /// them or puts them back.
    pub(super) fn candidate(self, member: &'a RoleMember, chosen: bool) -> Element<'a, Message> {
        let lit = self.cx.look.theme.wash(0.06);
        let face = row![
            self.cx.look.checkbox(chosen, false),
            self.cx.avatar(
                Some(member.avatar.as_str()).filter(|url| !url.is_empty()),
                &member.name,
                CANDIDATE_AVATAR,
                CANDIDATE_AVATAR / 2.0,
                1.0
            ),
            self.member_names(member),
        ]
        .spacing(10)
        .align_y(Alignment::Center);
        button(
            container(face)
                .height(Length::Fill)
                .align_y(Alignment::Center),
        )
        .padding([0, 6])
        .width(Length::Fill)
        .height(MEMBER)
        .on_press(input(RoleInput::Pick(member.id.clone())))
        .style(controls::lit_style(None, lit, 4.0))
        .into()
    }
}
