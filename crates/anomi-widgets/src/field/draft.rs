//! What a person has typed into a field the app may refuse: the text, and
//! the reason it was refused. A refusal (a wrong code, a taken name, a failed
//! sign-in) keeps the text, so the person edits it rather than types it
//! again; the next edit takes the reason away, as the Slint dialogs clear
//! their error line when the field's `edited` fires.

/// A field's draft and the error shown under it. The app keeps the `Draft`
/// (it is not in `anomi-vm`, which cannot depend on this crate) and projects
/// its [`text`](Draft::text) and [`error`](Draft::error) into the view-model
/// as plain values; a screen then draws
/// `field(ph, &vm.text).error(vm.error.as_deref())`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Draft {
    text: String,
    error: Option<String>,
}

impl Draft {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            error: None,
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// The person typed: the new text, and the old refusal no longer applies.
    pub fn edit(&mut self, text: String) {
        self.text = text;
        self.error = None;
    }

    /// The app refused the draft: it stays as typed, with the reason.
    pub fn reject(&mut self, error: impl Into<String>) {
        self.error = Some(error.into());
    }

    /// The draft was accepted: its text, with the field left empty.
    pub fn take(&mut self) -> String {
        self.error = None;
        std::mem::take(&mut self.text)
    }

    /// Starts over with other text and no error.
    pub fn set(&mut self, text: impl Into<String>) {
        *self = Self::new(text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refused_draft_keeps_its_text_until_the_next_edit() {
        let mut draft = Draft::new("123456");
        draft.reject("That code is wrong");
        assert_eq!(draft.text(), "123456");
        assert_eq!(draft.error(), Some("That code is wrong"));
        draft.edit("12345".to_string());
        assert_eq!(draft.text(), "12345");
        assert_eq!(draft.error(), None);
    }

    #[test]
    fn an_accepted_draft_leaves_the_field_empty() {
        let mut draft = Draft::new("name");
        draft.reject("Taken");
        assert_eq!(draft.take(), "name");
        assert!(draft.is_empty());
        assert_eq!(draft.error(), None);
    }
}
