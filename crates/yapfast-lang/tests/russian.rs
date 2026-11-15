//! The Russian file through the crate's own doors. The language is chosen
//! for the whole process, so this is the only test here.

use yapfast_lang::{text_in, tr, trn};

#[test]
fn russian_text_comes_from_its_file() {
    assert_eq!(yapfast_lang::choose("ru_RU.UTF-8"), "ru");
    assert_eq!(tr!("Language"), "Язык");
    assert_eq!(tr!("Not in any file"), "Not in any file");
    assert_eq!(tr!("Kick {}", "Sam"), "Выгнать Sam");
    let members = |count| trn!("{n} member", "{n} members", count);
    assert_eq!(
        [members(1), members(3), members(5), members(21)],
        ["1 участник", "3 участника", "5 участников", "21 участник"]
    );
    // A context's own entry, and the plain one where the context has none.
    assert_eq!(text_in("Tray", "Disconnect"), "Отключиться");
    assert_eq!(text_in("Nowhere", "Disconnect"), tr!("Disconnect"));
    assert_eq!(yapfast_lang::choose("de_DE"), "en");
    assert_eq!(tr!("Language"), "Language");
}
