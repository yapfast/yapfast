//! The window around the areas: the sidebar's width, and what the window
//! says while it connects. Task N01 (docs/iced/tasks/shell.md) takes it
//! further: panels whose edges drag, the layers each area opens.

#[derive(Clone, Debug, PartialEq)]
pub struct Shell {
    /// The sidebar's width, as its edge was last dragged.
    pub sidebar_width: f32,
    /// A notice over the conversation, such as the connection stopping.
    pub notice: Option<String>,
    /// The account is connecting: the lists are still empty.
    pub connecting: bool,
}

impl Default for Shell {
    fn default() -> Self {
        Self {
            sidebar_width: 304.0,
            notice: None,
            connecting: true,
        }
    }
}

/// What the window's frame asks for: an edge dragged, a layer closed. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {}
