//! A server's Bans: what it shows and what it asks for. Task X06
//! (docs/iced/tasks/settings.md). The Slint side: `BansSection` and
//! `BanDialog` (`ui/moderation.slint:193`, `:265`) over `BansPrompt`.

use crate::kind::Avatar;

/// One banned person of the page.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Row {
    /// The user's id: what a click on the row asks to open.
    pub id: String,
    pub avatar: Avatar,
    /// The name shown, and the username under it when it differs.
    pub name: String,
    pub username: String,
}

/// A ban shown in its dialog.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Open {
    pub name: String,
    pub reason: String,
    /// Why Discord didn't lift the ban.
    pub error: String,
    pub unbanning: bool,
    /// This account may lift bans: Ban Members.
    pub may_unban: bool,
}

/// A server's Bans: what its screen draws. The window holds it as
/// `vm.settings.bans`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Bans {
    /// The bans on the page shown.
    pub rows: Vec<Row>,
    /// From 0, and how many pages there are.
    pub page: usize,
    pub pages: usize,
    /// Discord is sending a batch or a search's bans.
    pub fetching: bool,
    /// The search field's text.
    pub query: String,
    /// The id of the row under the pointer.
    pub hovered: Option<String>,
    /// A ban in its dialog, over the settings (`dialog::dialog`).
    pub open: Option<Open>,
}

impl Bans {
    /// The pointer entered (`inside`) or left the row of user `id`
    /// ([`Intent::Hover`]). Leaving clears only that row's hover: crossing
    /// to the row above enters it before this one is left.
    pub fn hover(&mut self, id: String, inside: bool) {
        if inside {
            self.hovered = Some(id);
        } else if self.hovered.as_ref() == Some(&id) {
            self.hovered = None;
        }
    }
}

/// A server's Bans: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// The search field's text changed.
    Query(String),
    /// Search by what the field holds (Enter or the button).
    Search,
    /// Go to this page, from 0.
    Page(usize),
    /// Show the ban of this user id.
    Open(String),
    /// The pointer entered (`true`) or left (`false`) the row of this user
    /// id: [`Bans::hover`].
    Hover(String, bool),
    /// Revoke the open ban.
    Unban,
    /// Close the dialog.
    Close,
}

#[cfg(test)]
mod tests {
    use super::Bans;

    #[test]
    fn crossing_to_the_row_above_keeps_its_hover() {
        let mut page = Bans::default();
        page.hover("202".into(), true);
        page.hover("201".into(), true);
        page.hover("202".into(), false);
        assert_eq!(page.hovered.as_deref(), Some("201"));
        page.hover("201".into(), false);
        assert_eq!(page.hovered, None);
    }
}
