//! A rail folder's settings (`anomi_screens::rail::folder`), in the
//! simulator (`super::support`): typing names the folder, Done and Enter
//! apply, the X closes, and in the whole window Escape closes the dialog,
//! or the colour picker alone while it shows.

use iced::Widget as _;
use iced::advanced::widget::operation::focusable;
use iced::advanced::widget::{Id, Operation};
use iced::keyboard::key::Named;
use iced_test::Simulator;

use anomi_screens::rail::folder::view;
use anomi_vm::fixtures::rail as sample;
use anomi_vm::rail::folder::{FolderSettings, Intent};
use anomi_vm::shell::{Intent as ShellIntent, Layer};

use super::support::{self, SIZE, asked, front, settings};

type Ui<'a> = Simulator<'a, Intent, anomi_theme::Theme, anomi_theme::Renderer>;

fn simulate(folder: &FolderSettings) -> Ui<'_> {
    Simulator::with_size(settings(), SIZE, view(folder, front()).boxed())
}

fn named() -> FolderSettings {
    sample::folder_settings_named().rail.settings
}

fn untouched() -> FolderSettings {
    sample::folder_settings_untouched().rail.settings
}

#[test]
fn it_shows_the_name_and_the_colour() {
    let folder = named();
    let mut ui = simulate(&folder);
    for words in ["Folder settings", "Folder name", "Folder colour", "Done"] {
        assert!(ui.find(words).is_ok(), "{words}");
    }
    // Untouched, the field shows the name the rail gives it.
    let folder = untouched();
    let mut ui = simulate(&folder);
    assert!(ui.find("Alpha, Beta, Gamma, …").is_ok());
}

#[test]
fn typing_names_the_folder() {
    let folder = untouched();
    let mut ui = simulate(&folder);
    ui.click("Alpha, Beta, Gamma, …").expect("the field");
    ui.typewrite("ab");
    assert_eq!(
        asked(ui),
        [Intent::Name("a".into()), Intent::Name("ab".into())]
    );
}

#[test]
fn done_and_enter_apply_it() {
    let folder = named();
    let mut ui = simulate(&folder);
    ui.click("Done").expect("Done");
    assert_eq!(asked(ui), [Intent::Done]);
    let mut ui = simulate(&folder);
    ui.tap_key(Named::Enter);
    assert_eq!(asked(ui), [Intent::Done]);
}

/// The X is the first focusable: it sits in the title row.
#[test]
fn the_x_closes_it() {
    struct First {
        seen: usize,
    }
    impl Operation for First {
        fn focusable(
            &mut self,
            _id: Option<&Id>,
            _bounds: iced::Rectangle,
            state: &mut dyn focusable::Focusable,
        ) {
            if self.seen == 0 {
                state.focus();
            } else {
                state.unfocus();
            }
            self.seen += 1;
        }

        fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
            operate(self);
        }
    }
    let folder = named();
    let mut ui = simulate(&folder);
    let mut first = First { seen: 0 };
    ui.operate(&mut first as &mut dyn Operation);
    ui.tap_key(Named::Space);
    assert_eq!(asked(ui), [Intent::Close]);
}

#[test]
fn escape_closes_the_dialog_or_the_picker_over_it() {
    let closes = |layer| anomi_vm::Intent::Shell(ShellIntent::Close(layer));
    let vm = sample::folder_settings_named();
    let mut ui = support::simulate(&vm);
    ui.tap_key(Named::Escape);
    assert_eq!(asked(ui), [closes(Layer::FolderSettings)]);
    let vm = sample::folder_settings_picker();
    let mut ui = support::simulate(&vm);
    ui.tap_key(Named::Escape);
    assert_eq!(asked(ui), [closes(Layer::FolderColour)]);
}
