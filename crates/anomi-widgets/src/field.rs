//! Text fields of the interface's own looks.

use iced::widget::{TextInput, text_input};

use anomi_theme::{Theme, catalog, typography};

/// The search field at the top of the sidebar, as "Find a conversation".
pub fn search<'a, Message: Clone + 'a>(
    placeholder: &'a str,
    value: &'a str,
    on_input: impl Fn(String) -> Message + 'a,
) -> TextInput<'a, Message, Theme> {
    text_input(placeholder, value)
        .on_input(on_input)
        .size(typography::size::SMALL)
        .padding([6.0, 8.0])
        .style(catalog::text_input::search)
}
