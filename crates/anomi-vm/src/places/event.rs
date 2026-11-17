//! A scheduled event's dialog: what it shows and what it asks for. The
//! Slint side: `EventPrompt` and `EventDialog` (`ui/server.slint:218`,
//! `:237`), filled by `show_event` (`src/ui/refresh.rs:2860`) and answered
//! by `on_toggle_event_interest` (`src/ui/callbacks.rs:1353`). The window
//! holds it as `vm.places.event`.

/// The event, once found.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Found {
    pub name: String,
    /// When it is; empty for none.
    pub when: String,
    /// Where it is, by whom, and how many are interested; empty for none.
    pub detail: String,
}

/// A scheduled event's dialog: when and where it is, what it says, and how
/// changing interest in it goes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Event {
    /// "Event".
    pub title: String,
    pub found: Option<Found>,
    /// What the event says about itself; empty for none.
    pub description: String,
    /// What the main button says: "Interested" or "Remove interest"; empty
    /// for none.
    pub confirm: String,
    pub busy: bool,
    /// How it's going, such as "This event is going on now."; empty for none.
    pub status: String,
    pub error: String,
}

/// A scheduled event's dialog: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    Close,
    /// Mark the event interesting, or not.
    ToggleInterest,
}
