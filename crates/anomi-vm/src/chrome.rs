//! The window's own frame, with its title bar, caption buttons and resize
//! edges: what it shows and what it asks for. The Slint side: `WindowBar`,
//! `TitleBar` and `ResizeEdges` (`ui/yapfast/window-bar.slint:74`,
//! `ui/title_bar.slint:28`, `:87`) and `src/ui/title_bar.rs`; the iced
//! interface draws the Discord style's bar alone.
//!
//! The app fills [`Chrome`] from the layout's title-bar choice and the
//! window's state (task I14); the views draw it and send [`Intent`]s, which
//! the app turns into `iced::window` tasks.

/// The window's own frame: what its screen draws. The window holds it as
/// `vm.chrome`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Chrome {
    /// Anomi draws the frame, not the desktop (`own-title-bar`). Off, the
    /// view draws nothing.
    pub framed: bool,
    /// Full screen takes the frame away (`title-height`, `ui/yapfast.slint:109`).
    pub full_screen: bool,
    /// The window has the focus: the title shows in its brighter ink.
    pub focused: bool,
    /// The button in the middle restores rather than maximizes, and the
    /// window has no resize edges.
    pub maximized: bool,
    /// The account's screens show: the inbox and help buttons are for them.
    pub workspace: bool,
    /// Where the person is, shown in the middle of the bar.
    pub place: String,
    pub can_go_back: bool,
    pub can_go_forward: bool,
    /// The inbox popover is open: its button shows pressed.
    pub inbox_open: bool,
}

impl Chrome {
    /// The bar shows: Anomi draws the frame and the window isn't full screen.
    pub fn bar(&self) -> bool {
        self.framed && !self.full_screen
    }

    /// The window's edges resize it: the bar shows and the window isn't
    /// maximized (`ui/yapfast.slint:5630`).
    pub fn edges(&self) -> bool {
        self.bar() && !self.maximized
    }
}

/// An edge or corner of the window that resizes it, as `ResizeEdges` names
/// them (winit's `ResizeDirection`, in the order of the Slint callback).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    East,
    North,
    NorthEast,
    NorthWest,
    South,
    SouthEast,
    SouthWest,
    West,
}

impl Edge {
    /// The eight, in the order of `ResizeEdges`' direction numbers 0 to 7.
    pub const ALL: [Edge; 8] = [
        Edge::East,
        Edge::North,
        Edge::NorthEast,
        Edge::NorthWest,
        Edge::South,
        Edge::SouthEast,
        Edge::SouthWest,
        Edge::West,
    ];
}

/// The window's own frame: what a click, a press or a key on it asks for.
/// Keep `#[non_exhaustive]`: the app matches it with a wildcard until its I
/// task handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// A press on the bar moved past the slop: the desktop takes the pointer
    /// and moves the window (`drag_window`).
    Move,
    /// A press on an edge or corner: the desktop resizes from there
    /// (`drag_resize_window`).
    Resize(Edge),
    Minimize,
    /// The middle caption button or a double click on the bar.
    ToggleMaximize,
    /// Closes as the desktop's button does: to the tray, or quit.
    Close,
    Back,
    Forward,
    ToggleInbox,
    OpenHelp,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bar_and_the_edges_follow_the_window() {
        let mut chrome = Chrome::default();
        assert!(!chrome.bar() && !chrome.edges());
        chrome.framed = true;
        assert!(chrome.bar() && chrome.edges());
        chrome.maximized = true;
        assert!(chrome.bar() && !chrome.edges());
        chrome.full_screen = true;
        assert!(!chrome.bar());
    }
}
