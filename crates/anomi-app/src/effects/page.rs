//! Opening pages: `Effect::OpenPage`. Task I15
//! (docs/iced/tasks/integration.md) fills it; F01's stub until then.

use iced::Task;
use yapfast_core::application::Effect;

use crate::app::App;
use crate::message::Message;

pub(super) fn run(_app: &mut App, effect: Effect) -> Task<Message> {
    super::waiting(&effect)
}
