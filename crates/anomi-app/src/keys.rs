//! The window's keys, the app's half. The window's
//! [`focus::scope`](anomi_widgets::focus::scope) catches Ctrl+K, Alt+Left
//! and Alt+Right and the mouse's side buttons, Escape nothing took and
//! typing anywhere (task F17, `anomi-widgets/src/focus.rs`); this decides
//! when each is on and what each asks of the core, as `watch_window`
//! (`src/ui.rs:660`-`:768`) and the window's `type-into-composer` and
//! `escape-composing` (`ui/yapfast.slint:1750`, `:1765`) do for the Slint
//! window.
//!
//! Who calls what: the shell gives the scope a key's message only while
//! [`keys`] says it's on, where typing goes from [`typing`] (both through
//! the projection, I01), and the push-to-talk keys while a call hears the
//! microphone, which then don't type (`src/ui.rs:883`, I18). The handlers
//! below run from the intents the keys send: [`back`] and [`forward`] from
//! the title bar's arrows' (`chrome::Intent::Back`, `Forward`, I14),
//! [`switcher`] from Ctrl+K's (I06), [`escape`] from Escape's (I11).
//!
//! Escape in the header's search is the search's own, not [`escape`]
//! (`HeaderSearch`, `ui/header.slint:117`-`:126`), and I04 does it so:
//! - with text in the field, it empties the field, and so ends the search:
//!   `Action::Search(String::new())`, as the field's `clear` does
//!   (`ui/yapfast.slint:3437`; an empty search ends it in the core). With
//!   the field empty it ends nothing;
//! - either way it then leaves the field (`leave-search`, `:1801`, from
//!   the field's `escape(cleared)` at `:3438`), which runs
//!   `type-into-composer`: the focus goes to the composer used last, if
//!   there is one, its caret at the end
//!   ([`focus::into`](anomi_widgets::focus::into) on [`typing`]'s target).
//!   The field lets go of the keyboard even when no composer takes it:
//!   `leave-search` always says it took the key, and the field then calls
//!   `clear-focus`. A reply or an edit pending in that composer stays, for
//!   the composer's own Escape.
//!
//! The field captures the key itself, so that it doesn't reach the scope
//! as Escape nothing took.

use iced::Task;
use iced::advanced::widget::Id;
use yapfast_core::application::{Action, AppState};

use anomi_widgets::focus;

use crate::app::App;
use crate::message::Message;

/// What the window shows over the conversation that the core doesn't keep,
/// and which turns keys off.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Covers {
    /// The person's settings are open (`settings-open`,
    /// `ui/yapfast.slint:1274`); a server's or a channel's leave the keys
    /// on, as in the Slint window.
    pub settings: bool,
    /// A masked link asks before it opens.
    pub leaving_link: bool,
}

/// Which of the window's shortcuts are on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Keys {
    /// Ctrl+K opens and closes the quick switcher: on the account's
    /// screens, not over the settings or a masked link's question, which
    /// the window keeps (`src/ui.rs:660`).
    pub switcher: bool,
    /// Alt+Left and Alt+Right and the mouse's side buttons go back and
    /// forward through the conversations visited: on the account's
    /// screens, not over the settings (`src/ui.rs:732`, `:756`).
    pub navigate: bool,
}

/// Which of the window's shortcuts are on.
pub fn keys(state: &AppState, covers: Covers) -> Keys {
    let workspace = workspace(state);
    Keys {
        switcher: workspace && !covers.settings && !covers.leaving_link,
        navigate: workspace && !covers.settings,
    }
}

/// The account's screens show: a session's, or the preview's
/// (`show_workspace`, `src/ui/refresh.rs:960`).
fn workspace(state: &AppState) -> bool {
    state.preview || state.account.is_some()
}

/// Where typing anywhere goes: the composer used last, as in Discord
/// (`type-into-composer`, `ui/yapfast.slint:1750`). While the thread
/// beside the channel (or the new thread's form) is docked there, with no
/// search over it, and was used last: its composer if it may be written
/// in, else nothing (`:1752`-`:1755`). Otherwise the channel's, if
/// `composer`: it shows and may be written in (I11 knows).
///
/// `aside_last` is the app's, and changes wherever Slint changes
/// `aside-composer-last`:
/// - each time the thread's being docked (`aside_docked` below) changes,
///   it takes that value (`ui/yapfast.slint:1745`): true as the thread or
///   the new thread's form opens beside the channel, and again as the
///   search over it closes; false as the search opens over it, or as it
///   closes;
/// - false as the channel's composer takes the focus (`:4249`) or a
///   message's menu opens in the channel (`:2464`);
/// - true as the thread's or the new thread's composer takes the focus
///   (`:4351`, `:4374`) or a menu opens on one of the thread's messages
///   (`:4428`-`:4429`).
///
/// What covers the conversation needs no rule here: the scope reaches no
/// composer under a layer.
pub fn typing(state: &AppState, composer: bool, aside_last: bool) -> Option<Id> {
    let aside = aside_docked(state).then(|| {
        state.new_thread().is_some()
            || state
                .aside
                .as_ref()
                .is_some_and(|aside| state.can_write_in(&aside.room))
    });
    typing_to(workspace(state), composer, aside, aside_last)
}

/// The thread beside the channel, or the new thread's form, is docked
/// beside it: the search takes its place (`aside-docked`,
/// `ui/yapfast.slint:1964`).
fn aside_docked(state: &AppState) -> bool {
    (state.aside.is_some() || state.new_thread().is_some()) && state.search_shown().is_none()
}

/// [`typing`]'s rule: `aside` is whether the docked thread's composer may
/// be written in, `None` when none is docked.
fn typing_to(workspace: bool, composer: bool, aside: Option<bool>, aside_last: bool) -> Option<Id> {
    if !workspace {
        return None;
    }
    if let Some(writable) = aside
        && aside_last
    {
        return writable.then_some(focus::ASIDE_COMPOSER);
    }
    composer.then_some(focus::COMPOSER)
}

/// Alt+Left, the mouse's back button, the title bar's arrow: back to the
/// conversation visited before this one.
pub fn back(app: &mut App) -> Task<Message> {
    app.dispatch(Action::NavigateBack)
}

/// Alt+Right, the mouse's forward button: forward again after going back.
pub fn forward(app: &mut App) -> Task<Message> {
    app.dispatch(Action::NavigateForward)
}

/// Ctrl+K: the quick switcher opens, or closes. As it closes the composer
/// has the focus again, as in Discord (`ui/yapfast.slint:908`); I06 does
/// the same with [`refocus`] when it closes otherwise.
pub fn switcher(app: &mut App) -> Task<Message> {
    let open = app.core.state.switcher.is_some();
    let task = app.dispatch(Action::ToggleSwitcher);
    if open && app.core.state.switcher.is_none() {
        task.chain(refocus())
    } else {
        task
    }
}

/// The channel's composer has the focus again, its caret where it was.
pub fn refocus() -> Task<Message> {
    focus::to(focus::COMPOSER)
}

/// Escape nothing took, from anywhere: undoes one thing of what's being
/// written, in Discord's order (`escape-composing`,
/// `ui/yapfast.slint:1765`). The header's search field empties with the
/// search (I04 keeps its text).
pub fn escape(app: &mut App) -> Task<Message> {
    match undo(&app.core.state) {
        Some(action) => app.dispatch(action),
        None => Task::none(),
    }
}

/// What Escape undoes now, if anything.
fn undo(state: &AppState) -> Option<Action> {
    if !workspace(state) {
        return None;
    }
    let writing = Writing {
        search: state.search_shown().is_some(),
        reply: state.compose().is_some(),
        new_thread: state.new_thread().is_some(),
        aside_reply: state.aside.is_some() && state.aside_compose().is_some(),
    };
    Some(match writing.first()? {
        Undo::Search => Action::Search(String::new()),
        Undo::Reply => Action::CancelCompose,
        Undo::NewThread => Action::CloseNewThread,
        Undo::AsideReply => Action::CancelAsideCompose,
    })
}

/// What's being written that Escape may undo.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Writing {
    /// The conversation's search shows.
    search: bool,
    /// The channel's composer replies to a message.
    reply: bool,
    /// The new thread's form is open beside the channel.
    new_thread: bool,
    /// The thread beside the channel replies to a message.
    aside_reply: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Undo {
    Search,
    Reply,
    NewThread,
    AsideReply,
}

impl Writing {
    /// The search, then the channel's reply, then beside it the new
    /// thread's form, then the thread's reply. The search takes the
    /// thread's place, so behind it the thread has nothing to undo.
    fn first(self) -> Option<Undo> {
        if self.search {
            Some(Undo::Search)
        } else if self.reply {
            Some(Undo::Reply)
        } else if self.new_thread {
            Some(Undo::NewThread)
        } else if self.aside_reply {
            Some(Undo::AsideReply)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shortcuts_need_the_accounts_screens_and_nothing_over_them() {
        let mut state = AppState::default();
        assert_eq!(keys(&state, Covers::default()), Keys::default());
        state.preview = true;
        let all = Keys {
            switcher: true,
            navigate: true,
        };
        assert_eq!(keys(&state, Covers::default()), all);
        let settings = Covers {
            settings: true,
            ..Covers::default()
        };
        assert_eq!(keys(&state, settings), Keys::default());
        let link = Covers {
            leaving_link: true,
            ..Covers::default()
        };
        assert_eq!(
            keys(&state, link),
            Keys {
                switcher: false,
                navigate: true,
            }
        );
    }

    #[test]
    fn typing_goes_to_the_composer_used_last() {
        // The panel's from when it opens.
        assert_eq!(
            typing_to(true, true, Some(true), true),
            Some(focus::ASIDE_COMPOSER)
        );
        // The channel's once it has had the keyboard.
        assert_eq!(
            typing_to(true, true, Some(true), false),
            Some(focus::COMPOSER)
        );
        // A thread used last that may not be written in takes nothing.
        assert_eq!(typing_to(true, true, Some(false), true), None);
        // No thread docked: the channel's, if it may be written in.
        assert_eq!(typing_to(true, true, None, true), Some(focus::COMPOSER));
        assert_eq!(typing_to(true, false, None, false), None);
        // Not before signing in.
        assert_eq!(typing_to(false, true, None, false), None);
    }

    #[test]
    fn escape_undoes_one_thing_in_discords_order() {
        let all = Writing {
            search: true,
            reply: true,
            new_thread: true,
            aside_reply: true,
        };
        assert_eq!(all.first(), Some(Undo::Search));
        let all = Writing {
            search: false,
            ..all
        };
        assert_eq!(all.first(), Some(Undo::Reply));
        let all = Writing {
            reply: false,
            ..all
        };
        assert_eq!(all.first(), Some(Undo::NewThread));
        let all = Writing {
            new_thread: false,
            ..all
        };
        assert_eq!(all.first(), Some(Undo::AsideReply));
        assert_eq!(Writing::default().first(), None);
    }

    #[test]
    fn escape_undoes_nothing_with_nothing_written() {
        let mut state = AppState::default();
        assert!(undo(&state).is_none());
        state.preview = true;
        assert!(undo(&state).is_none());
    }
}
