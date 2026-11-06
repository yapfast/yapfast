// Yapfast: not part of upstream Slint. See docs/building.md in Yapfast.

//! Colouring parts of a `TextInput`'s text, as the composer shows the people its message
//! mentions: each part in its colour on a translucent pill of it, as a `StyledText` draws a
//! mention.
//!
//! The marks hold for one text in each of [`SLOTS`], one for each composer on screen. A
//! `TextInput` showing any other, such as the next keystroke's before the application marks it
//! again, or one composing with an input method, draws plain.

use alloc::boxed::Box;
use alloc::rc::Rc;
use core::ops::Range;
use core::pin::Pin;

use crate::Property;
use crate::SharedString;
use crate::graphics::Color;

/// Byte ranges of a text, each with its colour.
pub type Marks = Rc<[(Range<usize>, Color)]>;

#[derive(Clone, Default, PartialEq)]
struct Marked {
    text: SharedString,
    marks: Option<Marks>,
}

/// How many texts can hold marks at once.
pub const SLOTS: usize = 2;

std::thread_local! {
    /// The texts marked and their marks. The text inputs that shape while reading them shape
    /// again when they change.
    static MARKED: Pin<Box<Property<[Marked; SLOTS]>>> = Box::pin(Property::new(Default::default()));
}

/// Marks byte ranges of `text` with colours, in place of the marks `slot` held before. Empty
/// marks clear them.
pub fn mark(slot: usize, text: SharedString, marks: impl IntoIterator<Item = (Range<usize>, Color)>) {
    let marks: Marks = marks.into_iter().filter(|(range, _)| !range.is_empty()).collect();
    let marked = if marks.is_empty() {
        Marked::default()
    } else {
        Marked { text, marks: Some(marks) }
    };
    MARKED.with(|property| {
        let mut all = property.as_ref().get();
        all[slot] = marked;
        property.as_ref().set(all);
    });
}

/// The marks of `text`, if it's a text marked.
pub fn marks_of(text: &str) -> Option<Marks> {
    MARKED.with(|property| {
        property.as_ref().get().into_iter().find_map(|marked| {
            (marked.text == text).then_some(marked.marks).flatten()
        })
    })
}
