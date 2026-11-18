//! Every clock: one timer to the next whole minute and one to the next
//! midnight, each re-armed from its own message, armed only while the window
//! is in front and something shows the time; and idle after 15 s. Task I21
//! (docs/iced/tasks/integration.md) fills it; F01's stub until then.
