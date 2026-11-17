//! The Notifications settings page: what it shows and what it asks for. Task
//! S05 (docs/iced/tasks/settings.md). The Slint side: `NotificationSettings`
//! (`ui/settings.slint:1786`), set from `src/ui/callbacks.rs:2400`.

/// The Notifications settings page: what its screen draws. The window holds
/// it as `vm.settings.notifications`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Notifications {
    /// Desktop notifications for new messages while the window isn't in front.
    pub notifications: bool,
    /// What a notification says: off shows only who wrote and where.
    pub text: bool,
    /// Flash the taskbar for a message that notifies.
    pub flash_taskbar: bool,
    /// Whether a Matrix account is in use: its rules decide what notifies.
    pub matrix: bool,
    /// Whether a Discord account is in use: each server's settings decide.
    pub discord: bool,
}

/// The Notifications settings page: what a click or a key on it asks for.
/// Each carries the value the switch should now have; the app keeps it in
/// the layout and saves the preferences. Keep `#[non_exhaustive]`: the app
/// matches it with a wildcard until its I task handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// Show desktop notifications.
    Notifications(bool),
    /// Show what messages say.
    Text(bool),
    /// Flash the taskbar.
    FlashTaskbar(bool),
}

impl Notifications {
    /// What an intent of the page changes in it, for the app's update and
    /// for the simulator's tests.
    pub fn apply(&mut self, intent: &Intent) {
        match *intent {
            Intent::Notifications(on) => self.notifications = on,
            Intent::Text(on) => self.text = on,
            Intent::FlashTaskbar(on) => self.flash_taskbar = on,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_intent_sets_its_switch() {
        let mut page = Notifications::default();
        page.apply(&Intent::Text(true));
        page.apply(&Intent::FlashTaskbar(true));
        assert_eq!(
            (page.notifications, page.text, page.flash_taskbar),
            (false, true, true)
        );
    }
}
