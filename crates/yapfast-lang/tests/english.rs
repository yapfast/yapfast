//! English chosen: the code's own text, with the program's name from
//! lang/en.po. The language is chosen for the whole process, so this is the
//! only test here.

use yapfast_lang::{tr, trn};

#[test]
fn english_names_the_program_and_leaves_the_rest() {
    assert_eq!(tr!("Open Yapfast"), "Open Yapfast", "nothing chosen yet");
    assert_eq!(yapfast_lang::choose("en"), "en");
    assert_eq!(
        tr!("Open Yapfast"),
        format!("Open {}", yapfast_lang::PRODUCT)
    );
    assert_eq!(tr!("Language"), "Language");
    assert_eq!(tr!("Kick {}", "Sam"), "Kick Sam");
    assert_eq!(trn!("{n} member", "{n} members", 2), "2 members");
    assert_eq!(
        trn!(
            "{n} permission Yapfast can't name",
            "{n} permissions Yapfast can't name",
            2
        ),
        format!("2 permissions {} can't name", yapfast_lang::PRODUCT)
    );
    // An empty code goes back to the code's own text.
    assert_eq!(yapfast_lang::choose(""), "en");
    assert_eq!(tr!("Open Yapfast"), "Open Yapfast");
}
