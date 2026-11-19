use super::*;

fn labels() -> Labels {
    Labels {
        title: "Output Device".into(),
        empty: "No speakers".into(),
        settings: "Voice Settings".into(),
        none: "None found".into(),
        default: "System default".into(),
    }
}

fn names() -> Vec<String> {
    vec!["Speakers".into(), "Headphones".into()]
}

fn profiles() -> [String; 3] {
    ["Voice Isolation".into(), "Studio".into(), "Custom".into()]
}

fn input() -> Devices {
    Devices::new(Kind::Input, labels(), &names(), Some(1)).profile(
        "Input Profile",
        profiles(),
        Profile::Studio,
    )
}

#[test]
fn the_head_names_the_device_in_use_and_its_submenu_marks_it() {
    let menu = Devices::new(Kind::Output, labels(), &names(), Some(1));
    assert_eq!(menu.device.subtitle, "Headphones");
    let rows = &menu.device.submenu;
    assert!(rows.iter().all(|row| row.radio));
    assert!(rows[1].checked && !rows[0].checked);
    assert_eq!(rows[0].intent, Some(Intent::Select(0)));
    assert!(menu.profile.is_none(), "the output menu has no profile");
}

#[test]
fn no_choice_is_the_system_default_and_none_found_says_so() {
    let head = |names: &[String], current| {
        Devices::new(Kind::Input, labels(), names, current)
            .device
            .subtitle
    };
    assert_eq!(head(&names(), None), "System default");
    assert_eq!(head(&[], None), "None found");
    // An index past the list is no choice.
    let menu = Devices::new(Kind::Input, labels(), &names(), Some(7));
    assert!(menu.device.submenu.iter().all(|row| !row.checked));
    // With nothing found, the submenu says so in a row nobody can choose.
    let menu = Devices::new(Kind::Input, labels(), &[], None);
    let [row] = &menu.device.submenu[..] else {
        panic!("one row")
    };
    assert!(row.disabled && row.subtitle == "No speakers");
}

#[test]
fn the_profile_row_names_the_profile_and_marks_it() {
    let menu = input();
    let profile = menu.profile.as_ref().unwrap();
    assert_eq!(profile.subtitle, "Studio");
    let chosen: Vec<bool> = profile.submenu.iter().map(|row| row.checked).collect();
    assert_eq!(chosen, [false, true, false]);
    assert_eq!(
        profile.submenu[2].intent,
        Some(Intent::Profile(Profile::Custom))
    );
}

#[test]
fn hovering_a_row_opens_its_submenu_and_the_volume_closes_it() {
    let mut menu = input();
    menu.apply(&Intent::Hover(Some(Part::Profile)));
    assert_eq!(menu.open, Some(Part::Profile));
    assert!(menu.lit(Part::Profile) && !menu.lit(Part::Device));
    menu.apply(&Intent::Hover(Some(Part::Settings)));
    assert_eq!((menu.open, menu.current), (None, Some(Part::Settings)));
    menu.apply(&Intent::Hover(Some(Part::Device)));
    menu.apply(&Intent::Hover(None));
    assert_eq!((menu.open, menu.current), (None, None));
}

#[test]
fn the_keys_move_open_choose_and_close() {
    let mut menu = input();
    let press = |menu: &mut Devices, key| menu.apply(&Intent::Key(key));
    assert_eq!(press(&mut menu, Key::Down), None);
    assert_eq!(menu.current, Some(Part::Device));
    // Right opens the submenu at its first row; Down moves in it, wrapping.
    press(&mut menu, Key::Right);
    assert_eq!((menu.open, menu.sub), (Some(Part::Device), Some(0)));
    press(&mut menu, Key::Down);
    press(&mut menu, Key::Down);
    assert_eq!(menu.sub, Some(0));
    press(&mut menu, Key::Up);
    assert_eq!(press(&mut menu, Key::Enter), Some(Intent::Select(1)));
    // Left closes it and the keys are back on its row.
    press(&mut menu, Key::Left);
    assert_eq!((menu.open, menu.sub), (None, None));
    assert!(menu.lit(Part::Device));
    press(&mut menu, Key::Down);
    press(&mut menu, Key::Enter);
    assert_eq!(menu.open, Some(Part::Profile), "Enter opens it too");
    press(&mut menu, Key::Escape);
    assert_eq!(menu.open, None);
    press(&mut menu, Key::Down);
    assert_eq!(press(&mut menu, Key::Enter), Some(Intent::Settings));
    // Up from the first row wraps to the last.
    press(&mut menu, Key::Down);
    assert_eq!(menu.current, Some(Part::Device));
    press(&mut menu, Key::Up);
    assert_eq!(menu.current, Some(Part::Settings));
}

#[test]
fn moving_off_a_row_closes_its_submenu_and_nothing_found_chooses_nothing() {
    let mut menu = Devices::new(Kind::Output, labels(), &[], None);
    menu.apply(&Intent::Hover(Some(Part::Device)));
    menu.apply(&Intent::Key(Key::Right));
    assert_eq!(menu.apply(&Intent::Key(Key::Enter)), None);
    menu.apply(&Intent::Key(Key::Left));
    menu.apply(&Intent::Hover(Some(Part::Device)));
    assert_eq!(menu.open, Some(Part::Device));
    menu.apply(&Intent::Key(Key::Down));
    assert_eq!((menu.current, menu.open), (Some(Part::Settings), None));
}

#[test]
fn made_again_it_keeps_the_open_submenu_as_far_as_its_rows_go() {
    let mut was = input();
    was.apply(&Intent::Hover(Some(Part::Device)));
    was.apply(&Intent::Key(Key::Right));
    was.apply(&Intent::Key(Key::Down));
    let one = vec!["Speakers".to_owned()];
    let mut menu = Devices::new(Kind::Input, labels(), &one, Some(0));
    menu.keep(&was);
    assert_eq!((menu.open, menu.sub), (Some(Part::Device), Some(0)));
    // A profile gone away takes its submenu with it.
    was.apply(&Intent::Key(Key::Left));
    was.apply(&Intent::Key(Key::Down));
    was.apply(&Intent::Key(Key::Right));
    assert_eq!(was.open, Some(Part::Profile));
    menu.keep(&was);
    assert_eq!((menu.current, menu.open, menu.sub), (None, None, None));
}
