//! What a guest may read of the application: the state as the application last
//! published it, in the wire's own shapes.
//!
//! The bridge answers a guest's reads through this trait, so it never names the
//! application's model. The application implements it over whatever it holds —
//! [`World`](../../yapfast-core/src/plugins/world.rs) is Yapfast's — and pushes
//! the values it published before the helper is asked anything.
//!
//! Every method is a read of what was already published: nothing here waits on
//! the network, and nothing asks Yapfast to fetch.

use yapfast_plugin_api::call::{AccountView, AppInfo, MemberView};
use yapfast_plugin_api::event::{MessageView, PersonView, RoomView};

pub trait World: Send + Sync {
    /// `app.version`, `app.locale` and `app.theme_mode`.
    fn app(&self) -> AppInfo;

    /// `account.me`.
    fn me(&self) -> Option<AccountView>;

    /// `rooms.selected`.
    fn selected(&self) -> Option<RoomView>;

    /// `rooms.get`.
    fn room(&self, id: &str) -> Option<RoomView>;

    /// `rooms.list`, in the order the sidebar lists them.
    fn rooms(&self, limit: Option<usize>) -> Vec<RoomView>;

    /// `messages.get`.
    fn message(&self, room: &str, id: &str) -> Option<MessageView>;

    /// `messages.history`: a page of the open room's timeline, oldest first.
    fn history(
        &self,
        room: &str,
        before: Option<&str>,
        limit: Option<usize>,
    ) -> Option<Vec<MessageView>>;

    /// `people.get`.
    fn person(&self, id: &str) -> Option<PersonView>;

    /// `people.member`: one person as one server sees them.
    fn member(&self, server: &str, id: &str) -> MemberAnswer;
}

/// What `people.member` found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MemberAnswer {
    /// The server's members are the open conversation's, and this is who is
    /// there, when anyone with that id is.
    Open(Option<MemberView>),
    /// The members of another server aren't in memory: only the conversation on
    /// screen has its people loaded.
    NotOpen,
}

impl MemberAnswer {
    /// The member, or nothing, when the server's list is the open one.
    pub fn open(self) -> Option<Option<MemberView>> {
        match self {
            MemberAnswer::Open(found) => Some(found),
            MemberAnswer::NotOpen => None,
        }
    }
}
