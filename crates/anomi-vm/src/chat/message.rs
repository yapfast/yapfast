//! A message: what it shows and what it asks for. Task C02
//! (docs/iced/tasks/chat.md) fills it; F01's stub until then.

/// A message: what its screen draws.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Message {
    /// Its reactions: C03. Wired by F01; keep it.
    pub reactions: super::reactions::Reactions,
    /// Its pictures, videos, files and stickers: C04. Wired by F01; keep it.
    pub attachments: super::attachments::Attachments,
    /// Its embeds: C05. Wired by F01; keep it.
    pub embeds: super::embeds::Embeds,
    /// A bot's components: C06. Wired by F01; keep it.
    pub components: super::components::Components,
    /// Its poll: C07. Wired by F01; keep it.
    pub poll: super::poll::Poll,
    /// The editor it turns into while edited: C13. Wired by F01; keep it.
    pub editing: super::inline_edit::InlineEdit,
}

/// A message: what a click or a key on it asks for. Keep `#[non_exhaustive]`:
/// the app matches it with a wildcard until its I task handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {}
