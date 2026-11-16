//! What a view reads besides its own state: the look, the pictures made to
//! draw, and the core's state, borrowed together and passed by value, so a
//! closure kept in an element can hold it. `App::ctx` makes one.

use yapfast_core::application::{AppState, Application};

use crate::{look::Look, pictures::Pictures};

#[derive(Clone, Copy)]
pub struct Ctx<'a> {
    pub look: Look<'a>,
    /// The pictures made to draw, and the moving ones playing.
    pub pictures: &'a Pictures,
    /// The core, and its state.
    pub app: &'a Application,
    pub state: &'a AppState,
    /// Whether moving pictures play (`App::plays`).
    pub plays: bool,
}
