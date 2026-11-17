//! A server's or channel's invite dialog: what it shows and what it asks
//! for. The Slint side: `InvitePrompt` and `InviteDialog`
//! (`ui/server.slint:12`, `:27`), filled by `show_invite`
//! (`src/ui/refresh.rs:2717`) and answered by `on_make_invite`
//! (`src/ui/callbacks.rs:1362`). The window holds it as `vm.places.invite`.

/// How long a link may last, in the order the dialog offers them
/// (`INVITE_AGES`): the choice is an index into these.
pub const AGES: usize = 7;
/// How often a link may be used (`INVITE_USES`).
pub const USES: usize = 7;

/// The invite dialog: whom it invites where, the link once Discord made it,
/// and the choices for the next one.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Invite {
    /// "Invite friends to {server}".
    pub title: String,
    /// Where the link lands those who follow it, such as "They'll land in
    /// #general."; empty for none.
    pub detail: String,
    /// The link; empty while Discord makes it.
    pub link: String,
    /// How long the link lasts and how often it works, as said under it.
    pub note: String,
    /// Discord is making a link.
    pub making: bool,
    /// Why making one failed; empty for none.
    pub error: String,
    /// The choice for the next link's life: an index below [`AGES`]
    /// (30 min, 1 hr, 6 hr, 12 hr, 1 day, 7 days, Never).
    pub age: usize,
    /// The choice for its uses: an index below [`USES`] (No limit, 1, 5,
    /// 10, 25, 50, 100).
    pub uses: usize,
    /// The link was copied: Copy reads "Copied" until a new link comes. The
    /// app sets it on [`Intent::Copy`] and clears it with a new link.
    pub copied: bool,
}

impl Invite {
    /// What Generate a new link asks for, from the choices as they stand.
    pub fn remake(&self) -> Intent {
        Intent::Remake {
            age: self.age,
            uses: self.uses,
        }
    }
}

/// The invite dialog: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// The X, Done: close the dialog (Escape and a press outside are the
    /// shell's).
    Close,
    /// Put the link on the clipboard.
    Copy(String),
    /// Choose how long the next link lasts.
    Age(usize),
    /// Choose how often the next link may be used.
    Uses(usize),
    /// Make a new link with these choices.
    Remake { age: usize, uses: usize },
}
