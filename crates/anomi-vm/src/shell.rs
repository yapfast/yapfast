//! The window around the areas: the sidebar's width, and what the window
//! says while it connects.

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
