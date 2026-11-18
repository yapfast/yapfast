//! What a thousand paragraphs hold: their texts and their shapes, counted
//! by an allocator that counts only on a thread that asks it to, against
//! the 51 KB a built message row of the version scout held
//! (`docs/iced/iced-notes.md`).

use anomi_vm::fixtures::text as samples;

use super::*;
use crate::counting::measure;

/// What the version scout measured a built message row at.
const ROW: isize = 51 * 1024;

#[test]
fn a_thousand_paragraphs_hold_less_than_a_row_each() {
    // The font system's caches fill once, whatever comes after.
    for index in 0..8 {
        let text = samples::history(index);
        let _ = tree_of(&mut linked(&text), SIZE.width);
    }
    let (built, held, _) = measure(|| {
        (0..1000)
            .map(|index| {
                let text = samples::history(index);
                let (tree, _) = tree_of(&mut linked(&text), SIZE.width);
                (text, tree)
            })
            .collect::<Vec<(Text, Tree)>>()
    });
    let each = held / built.len() as isize;
    eprintln!("a paragraph holds {each} bytes");
    assert!(each < ROW, "{each} bytes a paragraph");
    drop(built);
}
