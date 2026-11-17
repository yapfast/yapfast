//! A thread beside its channel: what it shows and what it asks for. The Slint
//! side: `ThreadPanel` (`ui/thread.slint:13`), which Discord opens from a
//! message, or from a forum's post, in place of the member list; the glue is
//! `show_aside` (`src/ui/refresh.rs:3979`) and the `aside-*` callbacks
//! (`src/ui/callbacks.rs:755`, `:1131`, `:2738`).
//!
//! The panel is a header (the thread's name and three buttons), its own
//! timeline and its own composer. Both are the channel's types, filled from
//! the thread's room: [`Timeline`] (its rows, its edges, "This is the start
//! of the {} thread."), [`Composer`] (its draft, reply bar, staged files and
//! suggestions). The aside has no "New" divider, no bar to the unread and no
//! GIF or sticker buttons: the projection leaves those fields empty.
//!
//! Everything the timeline's rows and the composer ask for is the same
//! intent the channel's would send; the panel wraps it in [`Intent::Within`],
//! so the app applies it to the thread's room and not the channel's (the
//! Slint `aside_*` callbacks, `Action::SendAside`, `AsideEarlier`, ...).

use super::composer::Composer;
use super::timeline::Timeline;
use crate::menu::Spot;

/// A thread beside its channel: what its screen draws. The window holds it as
/// `vm.chat.thread_panel`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ThreadPanel {
    /// The thread's name, in the header (`aside-name`).
    pub name: String,
    /// Its messages, in the thread's room (`aside-messages`, `aside-ready`,
    /// `aside-loading`, `aside-start`, `aside-failed`).
    pub timeline: Timeline,
    /// Its composer (`aside-draft`, `aside-staged`, `aside-compose-label`,
    /// `aside-reply-ping`, `aside-composer-enabled`, `aside-can-attach`,
    /// `aside-can-mark-spoilers`).
    pub composer: Composer,
}

/// A thread beside its channel: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// The cross (`close`, `Action::CloseAside`).
    Close,
    /// Show the thread in place of the channel (`expand`,
    /// `Action::ExpandAside`).
    Expand,
    /// The "..." button, from where it sits: the window's menu opens
    /// beneath it with [`ThreadPanel::menu_rows`] (`more`).
    More(Spot),
    /// The menu's "Leave thread" (`MenuChoice::LeaveThread`).
    Leave,
    /// The menu's "Copy link" (`MenuChoice::CopyLink`).
    CopyLink,
    /// What a row of the timeline or the composer asked for, as the channel's
    /// own would send it, but of the thread's room.
    Within(Box<super::Intent>),
}

impl Intent {
    /// `intent` of the thread's timeline or composer.
    pub fn within(intent: impl Into<super::Intent>) -> Self {
        Self::Within(Box::new(intent.into()))
    }
}

impl ThreadPanel {
    /// The rows of the "..." menu, Discord's order: Leave thread, then Copy
    /// link (`src/ui/menus.rs:430`, `:472`). The app builds the same.
    ///
    /// Translated here, so only with the `fixtures` feature until the crate
    /// takes `yapfast-lang` for good (F01's `Cargo.toml`).
    #[cfg(feature = "fixtures")]
    pub fn menu_rows(&self) -> Vec<crate::menu::Row> {
        use crate::icon::Icon;
        use crate::menu::Row;
        use yapfast_lang::tr;
        vec![
            Row::new(tr!("Leave thread"), Intent::Leave.into()).icon(Icon::LogOut),
            Row::new(tr!("Copy link"), Intent::CopyLink.into()).icon(Icon::Copy),
        ]
    }
}
