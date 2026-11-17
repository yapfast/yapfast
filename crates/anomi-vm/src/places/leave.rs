//! Leaving a server or group DM, and the move-sync confirmation: what it
//! shows and what it asks for. Both are Slint's `ChannelDialog` over a
//! `ChannelPrompt` (`ui/server.slint:489`, `:517`), filled by `show_leave`
//! and `show_move_sync` (`src/ui/refresh.rs:3530`, `:3566`), so one
//! view-model holds either, already translated. The window holds it as
//! `vm.places.leave`, and `Layer::Leave` opens it for both.

/// What the dialog draws: the fields of Slint's `ChannelPrompt` these two
/// confirmations use.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Leave {
    /// "Leave 'Server'", or "Sync permissions?".
    pub title: String,
    /// What it asks, below the title.
    pub detail: String,
    /// The button that carries it out, such as "Leave server".
    pub confirm: String,
    /// A button in place of Cancel that answers the other way, such as
    /// "Keep Current Permissions"; empty for Cancel.
    pub alternative: String,
    /// A box to tick below the detail, such as leaving a group without
    /// telling its members; empty for none.
    pub option: String,
    pub ticked: bool,
    /// Destructive: the confirm button is red.
    pub danger: bool,
    /// It was sent and waits for Discord: nothing may be asked again.
    pub saving: bool,
    /// Why Discord refused; empty for none.
    pub error: String,
}

/// What a click or a key on it asks for (`ui/server.slint:518`: `save`,
/// `alternative`, `tick`, `close`). While the leave dialog is open they are
/// `Action::Leave`, `Action::ToggleSilentLeave` and `Action::CloseLeave`
/// (`src/ui/callbacks.rs:1578`); for the move-sync one the answer is
/// `Action::AnswerMoveSync(Some(true))`, `Some(false)` and `None`
/// (`:3027`, `:3034`).
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// The red or accent button, or Enter.
    Confirm,
    /// The button in place of Cancel ("Keep Current Permissions").
    Alternative,
    /// The box below the detail.
    Tick,
    /// Cancel, the X, Escape or a press outside.
    Close,
}
