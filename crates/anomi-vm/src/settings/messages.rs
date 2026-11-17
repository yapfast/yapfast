//! The Messages settings page: what it shows and what it asks for. Task S04
//! (docs/iced/tasks/settings.md), after `MessageSettings`
//! (`ui/settings.slint:1634`) and the `on_preferences_changed` handler
//! (`src/ui/callbacks.rs:2303`). Every choice is reported as the value it
//! should have, and the page shows what it is given: the app keeps the
//! layout and projects it back.

/// Message text: `text-size` 0, 1, 2; the message font is 14, 15 and 18 px
/// (`ui/yapfast.slint:1467`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextSize {
    Small,
    #[default]
    Default,
    Large,
}

impl TextSize {
    pub const ALL: [TextSize; 3] = [TextSize::Small, TextSize::Default, TextSize::Large];

    /// The message font's size in px.
    pub fn px(self) -> f32 {
        match self {
            TextSize::Small => 14.0,
            TextSize::Default => 15.0,
            TextSize::Large => 18.0,
        }
    }
}

/// The date format's three orders (`clock::DateFormat::ALL`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DateFormat {
    DayFirst,
    MonthFirst,
    YearFirst,
}

impl DateFormat {
    pub const ALL: [DateFormat; 3] = [
        DateFormat::DayFirst,
        DateFormat::MonthFirst,
        DateFormat::YearFirst,
    ];
}

/// When animations play (`Animate`, `src/ui.rs:1518`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Animate {
    /// While the window has the focus.
    #[default]
    Focused,
    Never,
}

impl Animate {
    pub const ALL: [Animate; 2] = [Animate::Focused, Animate::Never];
}

/// The Messages settings page: what its screen draws. The window holds it as
/// `vm.settings.messages`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Messages {
    /// Compact display, not cozy with avatars.
    pub compact: bool,
    pub text_size: TextSize,
    pub twelve_hour: bool,
    pub seconds: bool,
    /// The date format; `None` follows the clock (month first with the
    /// 12-hour clock, day first with the 24-hour one).
    pub date_format: Option<DateFormat>,
    pub animate: Animate,
    pub link_previews: bool,
    pub collapse_long: bool,
    pub boost_gems: bool,
    pub share_typing: bool,
    pub reply_ping: bool,
    pub strip_tracking: bool,
    /// The networks in use, which the typing switch's description names.
    pub matrix: bool,
    pub discord: bool,
}

impl Messages {
    /// The date format the choices show as current.
    pub fn date_shown(&self) -> DateFormat {
        self.date_format.unwrap_or(if self.twelve_hour {
            DateFormat::MonthFirst
        } else {
            DateFormat::DayFirst
        })
    }
}

/// The Messages settings page: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    Compact(bool),
    TextSize(TextSize),
    TwelveHour(bool),
    Seconds(bool),
    DateFormat(DateFormat),
    Animate(Animate),
    LinkPreviews(bool),
    CollapseLong(bool),
    BoostGems(bool),
    ShareTyping(bool),
    ReplyPing(bool),
    StripTracking(bool),
}
