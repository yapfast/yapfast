//! How wide each panel is and what fits beside the conversation, for a
//! window of a given width: the Slint window's `sidebar-shown`,
//! `members-shown`, `aside-shown`, `members-docked`, `profile-docked` and
//! the bounds its edges drag in (`ui/yapfast.slint:1470`, `:1706`, `:1712`,
//! `:1944`-`:1979`, `:4502`, `:4522`, `:4544`). The app reads the same for
//! the header's buttons (whether the member list and the profile show).

use anomi_theme::metrics;
use anomi_vm::conversation::{Aside, Beside};
use anomi_vm::shell::Shell;

/// The search results' width (`ui/yapfast.slint:4326`).
pub const SEARCH_WIDTH: f32 = 420.0;
/// The thread beside a channel: its own width, and the range dragging
/// keeps it in (`:1705`, `:1706`).
pub const ASIDE_WIDTH: f32 = 420.0;
pub const ASIDE_MIN: f32 = 320.0;
pub const ASIDE_MAX: f32 = 800.0;
/// The range dragging keeps the member list in (`:1712`).
pub const MEMBERS_MIN: f32 = 200.0;
pub const MEMBERS_MAX: f32 = 480.0;
/// The least a call above a DM's messages keeps when dragged, and the room
/// it leaves under it (`:1718`, `:3494`).
pub const CALL_MIN: f32 = 200.0;
const CALL_ROOM: f32 = 240.0;

/// What shows at a window's width.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Docking {
    /// The window's width.
    pub width: f32,
    /// The sidebar's width as shown.
    pub sidebar: f32,
    /// The conversation's: the window less the rail and the sidebar
    /// (`conversation-room`).
    pub room: f32,
    /// Search results show beside the conversation.
    pub search: bool,
    /// The thread beside a channel, or the form that starts one, and its
    /// width.
    pub aside: Option<(Aside, f32)>,
    /// The member list, and its width.
    pub members: Option<f32>,
    /// A DM's profile.
    pub profile: bool,
    /// The member list and the profile would leave the conversation its
    /// least (`members-room`, `profile-room`); without, they give way
    /// unless squeezed in.
    pub members_room: bool,
    pub profile_room: bool,
    /// What the docked panels want, shown or put away for want of room:
    /// dragging the sidebar keeps it (`docked-width`).
    pub wanted: f32,
}

impl Docking {
    pub fn new(shell: &Shell, beside: &Beside, width: f32) -> Self {
        let sidebar = shell
            .sidebar_width
            .min(metrics::SIDEBAR_MAX)
            .min(width - metrics::RAIL_WIDTH - metrics::CONVERSATION_MIN)
            .max(metrics::SIDEBAR_MIN);
        let room = width - metrics::RAIL_WIDTH - sidebar;
        let members_width = members_width(shell);
        let aside = (!beside.search)
            .then_some(beside.thread)
            .flatten()
            .map(|aside| (aside, aside_width(shell)));
        let docked = beside.search || aside.is_some();
        let members_wanted = beside.members && !docked;
        let profile_wanted = beside.profile && !docked;
        let members_room = room - members_width >= metrics::CONVERSATION_MIN;
        let profile_room = room - metrics::PROFILE_WIDTH >= metrics::CONVERSATION_MIN;
        let wanted = match aside {
            _ if beside.search => SEARCH_WIDTH,
            Some((_, width)) => width,
            None => {
                f32::from(u8::from(members_wanted)) * members_width
                    + f32::from(u8::from(profile_wanted)) * metrics::PROFILE_WIDTH
            }
        };
        Self {
            width,
            sidebar,
            room,
            search: beside.search,
            aside,
            members: (members_wanted && (members_room || shell.squeezed)).then_some(members_width),
            profile: profile_wanted && (profile_room || shell.squeezed),
            members_room,
            profile_room,
            wanted,
        }
    }

    /// The left column's width: the rail and the sidebar.
    pub fn left(&self) -> f32 {
        metrics::RAIL_WIDTH + self.sidebar
    }

    /// What the docked panels take beside the conversation.
    pub fn docked(&self) -> f32 {
        if self.search {
            return SEARCH_WIDTH;
        }
        if let Some((_, width)) = self.aside {
            return width;
        }
        self.members.unwrap_or(0.0) + self.profile_width()
    }

    /// The profile's width while it shows.
    pub fn profile_width(&self) -> f32 {
        if self.profile {
            metrics::PROFILE_WIDTH
        } else {
            0.0
        }
    }

    /// The range dragging the sidebar keeps it in: the conversation keeps
    /// its least beside what's docked (`:4502`).
    pub fn sidebar_bounds(&self) -> (f32, f32) {
        let most = metrics::SIDEBAR_MAX
            .min(self.width - metrics::RAIL_WIDTH - metrics::CONVERSATION_MIN - self.wanted);
        (metrics::SIDEBAR_MIN, most.max(metrics::SIDEBAR_MIN))
    }

    /// The range dragging the thread beside keeps it in (`:4522`).
    pub fn aside_bounds(&self) -> (f32, f32) {
        let most = ASIDE_MAX.min(self.room - metrics::CONVERSATION_MIN);
        (ASIDE_MIN, most.max(ASIDE_MIN))
    }

    /// The range dragging the member list keeps it in, left of a profile
    /// docked beside it (`:4544`).
    pub fn members_bounds(&self) -> (f32, f32) {
        let most = MEMBERS_MAX.min(self.room - self.profile_width() - metrics::CONVERSATION_MIN);
        (MEMBERS_MIN, most.max(MEMBERS_MIN))
    }

    /// Whether the header shows the member list's button as on: chosen,
    /// and shown or squeezed in (`members-showing`, `:1958`).
    pub fn members_showing(&self, chosen: bool, squeezed: bool) -> bool {
        chosen && (self.members_room || squeezed)
    }

    /// The same for a DM's profile (`profile-showing`, `:1959`).
    pub fn profile_showing(&self, chosen: bool, squeezed: bool) -> bool {
        chosen && (self.profile_room || squeezed)
    }
}

/// The member list's width: as dragged, or the style's.
pub fn members_width(shell: &Shell) -> f32 {
    shell.members_width.map_or(metrics::MEMBERS_WIDTH, |width| {
        width.clamp(MEMBERS_MIN, MEMBERS_MAX)
    })
}

/// The thread beside's width: as dragged, or its own.
pub fn aside_width(shell: &Shell) -> f32 {
    shell
        .aside_width
        .map_or(ASIDE_WIDTH, |width| width.clamp(ASIDE_MIN, ASIDE_MAX))
}

/// The most a call above a DM's messages may take in a workspace this tall
/// (`call-limit`, `:1718`).
pub fn call_limit(height: f32) -> f32 {
    (height - metrics::HEADER_HEIGHT - CALL_ROOM).max(CALL_MIN)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn beside(members: bool, profile: bool) -> Beside {
        Beside {
            members,
            profile,
            ..Beside::default()
        }
    }

    #[test]
    fn a_wide_window_docks_the_member_list() {
        let docking = Docking::new(&Shell::default(), &beside(true, false), 1400.0);
        assert_eq!(docking.sidebar, 304.0);
        assert_eq!(docking.room, 1024.0);
        assert_eq!(docking.members, Some(metrics::MEMBERS_WIDTH));
    }

    #[test]
    fn a_narrow_window_puts_the_panels_away() {
        let members = Docking::new(&Shell::default(), &beside(true, false), 1000.0);
        assert_eq!(members.members, None);
        assert!(!members.members_room);
        let profile = Docking::new(&Shell::default(), &beside(false, true), 1000.0);
        assert!(!profile.profile);
    }

    #[test]
    fn squeezed_in_they_show_anyway() {
        let shell = Shell {
            squeezed: true,
            ..Shell::default()
        };
        let docking = Docking::new(&shell, &beside(true, false), 1000.0);
        assert_eq!(docking.members, Some(metrics::MEMBERS_WIDTH));
        assert!(docking.members_showing(true, true));
        assert!(!docking.members_showing(true, false));
    }

    #[test]
    fn the_sidebar_gives_way_to_the_conversation() {
        let shell = Shell {
            sidebar_width: 380.0,
            ..Shell::default()
        };
        let docking = Docking::new(&shell, &beside(false, false), 800.0);
        assert_eq!(docking.sidebar, 800.0 - 72.0 - 420.0);
        let tiny = Docking::new(&shell, &beside(false, false), 640.0);
        assert_eq!(tiny.sidebar, metrics::SIDEBAR_MIN);
    }

    #[test]
    fn search_and_a_thread_take_the_member_lists_place() {
        let search = Beside {
            search: true,
            thread: Some(Aside::Thread),
            ..beside(true, false)
        };
        let docking = Docking::new(&Shell::default(), &search, 1400.0);
        assert!(docking.search && docking.aside.is_none() && docking.members.is_none());
        assert_eq!(docking.docked(), SEARCH_WIDTH);
        let thread = Beside {
            thread: Some(Aside::Thread),
            ..beside(true, false)
        };
        let docking = Docking::new(&Shell::default(), &thread, 1400.0);
        assert_eq!(docking.aside, Some((Aside::Thread, ASIDE_WIDTH)));
        assert_eq!(docking.members, None);
    }

    #[test]
    fn edges_drag_within_their_bounds() {
        let docking = Docking::new(&Shell::default(), &beside(true, false), 1400.0);
        assert_eq!(
            docking.sidebar_bounds(),
            (180.0, 380.0_f32.min(1400.0 - 72.0 - 420.0 - 265.0))
        );
        assert_eq!(
            docking.members_bounds(),
            (200.0, 480.0_f32.min(1024.0 - 420.0))
        );
        assert_eq!(docking.aside_bounds(), (320.0, 1024.0 - 420.0));
        assert_eq!(call_limit(828.0), 828.0 - 50.0 - 240.0);
        assert_eq!(call_limit(300.0), CALL_MIN);
    }
}
