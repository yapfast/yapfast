//! The profile editor: what it shows and what it asks for. Task P14
//! (docs/iced/tasks/people.md) fills it; F01's stub until then.

/// The profile editor: what its screen draws. The window holds it as
/// `vm.people.editor`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Editor {
    /// The live preview beside the rows, and its colours: P15. Wired by F01; keep it.
    pub preview: super::editor_preview::EditorPreview,
}

/// The profile editor: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {}
