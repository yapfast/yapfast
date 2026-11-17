//! The confirmation before leaving for a link whose text shows something
//! other than where it goes, naming the site it really leads to: Slint's
//! `LeavingDialog` (`ui/dialogs.slint:518`), set by `src/ui/callbacks.rs:2170`.

/// What the dialog draws. The window holds it as `vm.places.leaving_link`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LeavingLink {
    /// Where the link goes, whole.
    pub link: String,
    /// The site it leads to (`links::host`), in bold.
    pub site: String,
}

/// What a click or a key on it asks for (`visit`, `close`:
/// `src/ui/callbacks.rs:2179`, `:2186`).
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// "Visit site", or Enter: open the link and close the dialog.
    Visit,
    /// "Go back", Escape or a press outside.
    Close,
}
