//! Settings: Appearance offers theme shortcuts, Themes manages installed and
//! custom themes, preferences are kept across a restart, a device is chosen
//! in Voice & video, and push to talk's key is recorded and kept.

use iced::keyboard::{Key, Location, key};
use yapfast_core::voice;

use super::{change, interact, local, preview, shows, start};
use crate::{
    app::{
        App, Message,
        layers::{Layer, Popup},
    },
    settings::{Event, Section, voice::Event as VoiceEvent},
};

/// Settings on a section, in a window tall enough to show its whole page.
fn open(app: &mut App, section: Section) {
    app.shell.frame.height = 2400.0;
    crate::window_bar::fit_window(app);
    let _ = app.update(crate::app::settings(Event::Open(section)));
}

fn stored(folder: &tempfile::TempDir) -> serde_json::Value {
    serde_json::from_slice(&{
        crate::layout::flush(folder.path());
        std::fs::read(folder.path().join("layout.json")).expect("the layout")
    })
    .expect("JSON")
}

/// Settings open moving, once: their motion starts as they open, stays
/// through what's done in them, and turns to closing as they close, the
/// Settings closed kept, taking nothing, until they've been drawn going.
#[test]
fn settings_open_and_close_as_a_popup() {
    let (mut app, _folder) = preview();
    assert!(app.popups_moving().is_empty());
    open(&mut app, Section::Account);
    let opening = app.popups_moving();
    assert_eq!(opening.len(), 1, "Settings' motion");
    assert!(!opening[0].1, "opening");
    open(&mut app, Section::Appearance);
    assert_eq!(
        app.popups_moving(),
        opening,
        "the same motion, not a new one"
    );
    let _ = app.update(crate::app::settings(Event::Close));
    assert!(app.win.settings.get().is_none(), "closed");
    assert!(app.win.settings.leaving().is_some(), "drawn going");
    assert_eq!(app.popups_moving(), [(opening[0].0, true)], "closing");
    // Escape is the window's again while they go.
    assert!(app.layers().all(|layer| layer != Layer::Settings));
    let _ = app.update(Message::PopupGone(Popup::Layer(Layer::Settings)));
    assert!(app.win.settings.leaving().is_none(), "gone");
    assert!(app.popups_moving().is_empty());
}

/// Settings opened again as they close are the new ones, opening: the
/// ones going go at once.
#[test]
fn settings_opened_again_as_they_close_open_anew() {
    let (mut app, _folder) = preview();
    open(&mut app, Section::Account);
    let _ = app.update(crate::app::settings(Event::Close));
    open(&mut app, Section::Appearance);
    assert!(app.win.settings.leaving().is_none());
    let moving = app.popups_moving();
    assert_eq!(moving.len(), 1);
    assert!(!moving[0].1, "opening");
}

#[test]
fn a_theme_chosen_in_appearance_shows_at_once_and_is_kept() {
    let (mut app, folder) = preview();
    open(&mut app, Section::Account);
    assert!(shows(&app, "My account"));
    interact(&mut app, |ui| {
        ui.click("Appearance").expect("the section");
    });
    assert!(shows(&app, "Catppuccin Latte"), "the themes' tiles");
    interact(&mut app, |ui| {
        ui.click("Nord").expect("its tile");
    });
    assert_eq!(app.theme.id, "nord", "at once");
    assert_eq!(stored(&folder)["theme"], "nord");
    // Escape closes Settings and lets go of what it held.
    let _ = app.update(Message::Key(
        Key::Named(key::Named::Escape),
        Default::default(),
    ));
    assert!(!app.win.settings.is_open());
    assert_eq!(start(&folder, true).theme.id, "nord", "the next start too");
}

fn ctrl_shift_t(app: &mut App) {
    let _ = app.update(Message::Key(
        Key::Character("t".into()),
        iced::keyboard::Modifiers::CTRL | iced::keyboard::Modifiers::SHIFT,
    ));
}

/// A small package installed in `folder`, as `installed:dev.example.small`.
fn install_small_package(folder: &tempfile::TempDir) {
    let fetched = yapfast_core::themes::install::Fetched::new(
        "https://github.com/ada/small".into(),
        String::new(),
        "main".into(),
        "0123456789abcdef0123456789abcdef01234567".into(),
        yapfast_core::themes::check::Files {
            manifest: r##"{ "theme_api": 1, "id": "dev.example.small", "name": "Small",
                "version": "1.0.0", "variants": { "dark": { "base": "yap" } } }"##
                .into(),
            ..Default::default()
        },
    )
    .expect("a package");
    yapfast_core::themes::install::Themes::open(folder.path())
        .install(&fetched)
        .expect("installed");
}

#[test]
fn ctrl_shift_t_turns_a_package_off_even_over_settings() {
    let folder = tempfile::tempdir_in(local()).expect("test folder");
    install_small_package(&folder);
    let mut app = start(&folder, true);
    assert!(app.choose_theme("installed:dev.example.small"));
    assert!(app.theme.packaged());
    open(&mut app, Section::Themes);
    ctrl_shift_t(&mut app);
    assert_eq!(app.theme.id, "bonfire");
    assert!(!app.theme.packaged());
    let kept = stored(&folder);
    assert!(kept["theme"].is_null());
    assert_eq!(kept["previous_theme"], "installed:dev.example.small");
}

/// Only a package is turned off: a palette theme stays, and isn't kept
/// as anything else.
#[test]
fn ctrl_shift_t_leaves_a_palette_theme_as_it_is() {
    let (mut app, folder) = preview();
    assert!(app.choose_theme("sakura"));
    ctrl_shift_t(&mut app);
    assert_eq!(app.theme.id, "sakura");
    let kept = stored(&folder);
    assert_eq!(kept["theme"], "sakura");
}

/// A package on trial ends as Revert ends it, back to the palette theme
/// chosen, which stays kept.
#[test]
fn ctrl_shift_t_ends_a_trial_and_keeps_the_palette_theme() {
    let folder = tempfile::tempdir_in(local()).expect("test folder");
    install_small_package(&folder);
    let mut app = start(&folder, true);
    assert!(app.choose_theme("sakura"));
    app.themes
        .try_installed("installed:dev.example.small")
        .expect("on trial");
    app.layout
        .theme_sounds
        .insert("trial".into(), crate::themes::SoundPrefs::default());
    app.show_wanted_theme();
    assert!(app.theme.packaged());
    ctrl_shift_t(&mut app);
    assert_eq!(app.theme.id, "sakura");
    assert!(app.themes.trial().is_none());
    assert!(!app.layout.theme_sounds.contains_key("trial"));
    assert_eq!(stored(&folder)["theme"], "sakura");
}

/// `--theme-dev`'s theme stops, and the palette theme chosen shows again.
#[test]
fn ctrl_shift_t_stops_the_dev_theme_and_keeps_the_palette_theme() {
    let (mut app, folder) = preview();
    assert!(app.choose_theme("sakura"));
    let dev = tempfile::tempdir_in(local()).expect("dev folder");
    std::fs::write(
        dev.path().join(yapfast_core::themes::MANIFEST),
        r##"{ "theme_api": 1, "id": "dev.example.dev", "name": "Dev",
            "version": "1.0.0", "variants": { "dark": { "base": "yap" } } }"##,
    )
    .unwrap();
    app.themes
        .develop(dev.path(), &app.layout)
        .expect("developed");
    app.show_wanted_theme();
    assert!(app.theme.packaged());
    ctrl_shift_t(&mut app);
    assert_eq!(app.theme.id, "sakura");
    assert!(!app.theme.packaged());
    assert!(app.themes.dev_folder().is_none(), "no longer watched");
    assert_eq!(stored(&folder)["theme"], "sakura");
}

/// The creator's preview is a package too: the key lets go of it, and
/// the palette theme chosen shows again.
#[test]
fn ctrl_shift_t_lets_go_of_the_creators_preview() {
    use crate::settings::theme_creator::Event as CreatorEvent;
    use crate::settings::{ThemeEvent, appearance_event as theme};

    let (mut app, folder) = preview();
    assert!(app.choose_theme("sakura"));
    open(&mut app, Section::Themes);
    let _ = app.update(theme(ThemeEvent::Creator(CreatorEvent::Open)));
    let _ = app.update(theme(ThemeEvent::Creator(CreatorEvent::AccentPreset(
        0xff6b6b,
    ))));
    assert_eq!(app.theme.id, crate::themes::CREATOR_PREVIEW_ID);
    assert!(app.theme.packaged());
    ctrl_shift_t(&mut app);
    assert_eq!(app.theme.id, "sakura");
    assert!(!app.theme.packaged());
    assert!(app.themes.find(crate::themes::CREATOR_PREVIEW_ID).is_none());
    assert!(shows(&app, "Save theme"), "the creator stays open");
    assert_eq!(stored(&folder)["theme"], "sakura");
}

#[test]
fn themes_is_its_own_settings_section_while_appearance_keeps_shortcuts() {
    let (mut app, _folder) = preview();
    open(&mut app, Section::Appearance);
    assert!(shows(&app, "Catppuccin Latte"), "Appearance's shortcuts");
    assert!(!shows(&app, "Marketplace"));
    assert!(!shows(&app, "PERFORMANCE"));
    interact(&mut app, |ui| {
        ui.click("Themes").expect("the Themes section");
    });
    assert!(shows(&app, "Marketplace"), "the page's tabs");
    assert!(shows(&app, "PERFORMANCE"));
    assert!(shows(&app, "No themes yet"), "what to do with none");
}

#[test]
fn an_installed_theme_applies_and_turns_off_from_themes() {
    let folder = tempfile::tempdir_in(local()).expect("test folder");
    let source = folder.path().join("plum-source");
    std::fs::create_dir_all(source.join("sounds")).unwrap();
    std::fs::write(
        source.join("sounds/tick.wav"),
        include_bytes!("../../../../themes/template/sounds/tick.wav"),
    )
    .unwrap();
    std::fs::write(
        source.join(yapfast_core::themes::MANIFEST),
        r##"{
            "theme_api": 1,
            "id": "dev.example.plum",
            "name": "Plum package",
            "version": "1.0.0",
            "authors": ["Ada", "Lin"],
            "description": "A quiet plum theme.",
            "options": [{
                "id": "roomy", "label": "Roomy messages", "kind": "toggle",
                "default": false, "on": { "spacing.message_gap": 12 }
            }],
            "future_sparkle": true,
            "art": {
                "particles": [{ "preset": "sparkles", "on": "theme.applied" }]
            },
            "sounds": {
                "message": "sounds/tick.wav",
                "hover": "sounds/tick.wav"
            },
            "variants": {
                "dark": {
                    "base": "yap",
                    "colours": { "chat": "#2a1a3a" },
                    "layout": {
                        "composer": "top",
                        "messages": "bubbles",
                        "chat_max_width": 720,
                        "chat_avatars": false,
                        "bubbles": { "radius": 18, "padding": [14, 7] },
                        "rail_tiles": {
                            "shape": "ticket", "width": 56, "height": 40,
                            "radius": 6, "gap": 7, "pad": 3,
                            "label": "inside", "label_size": 9,
                            "label_colour": "#f4eaff", "label_ground": "#341e4f",
                            "label_case": "upper", "pill": "unread",
                            "hover": "lift", "hover_amount": 2,
                            "selected": { "offset": [2, 1] },
                            "badge": { "ground": "#ff7eb6", "ink": "#341e4f", "radius": 5 }
                        },
                        "channel_rows": {
                            "height": 41, "gap": 5, "size": 14, "case": "upper", "inset": 10,
                            "reserve": 12,
                            "separator": { "colour": "#5a476e", "width": 1, "inset": [4, 6] },
                            "ink": { "selected": "#ffffff" },
                            "icon": { "size": 18, "gap": 6 },
                            "selected": { "underline": { "colour": "#ff7eb6", "width": 2, "offset": 3 } },
                            "meta": { "voice": "voice · {n}", "size": 10,
                                "badge": { "ground": "#ff7eb6", "ink": "#341e4f", "radius": 5 } }
                        },
                        "member_rows": {
                            "height": 52, "gap": 4, "size": 15, "case": "lower", "reserve": 15,
                            "separator": { "colour": "#5a476e", "width": 1, "inset": [5, 7] },
                            "avatar": { "size": 36, "gap": 9 },
                            "margin": [6, 7], "inset": [10, 11], "activity": "hidden",
                            "status": "text", "sub": "none",
                            "meta": { "online": "here", "playing": "playing {activity}", "size": 10 },
                            "badge": { "text": "P{online}", "ground": "#ff7eb6", "ink": "#341e4f" }
                        }
                    }
                },
                "light": { "base": "sakura" }
            }
        }"##,
    )
    .unwrap();
    let fetched = yapfast_core::themes::install::Fetched::local(&source).unwrap();
    yapfast_core::themes::install::Themes::open(folder.path())
        .install(&fetched)
        .unwrap();

    let mut app = start(&folder, true);
    open(&mut app, Section::Themes);
    assert!(shows(&app, "Plum package"));
    assert!(shows(&app, "by Ada, Lin · v1.0.0"));
    assert!(shows(&app, "Dark and light"), "its tag");
    // The package's ignored-field note is retained and shown verbatim.
    let ignored = "\"future_sparkle\" isn't something this Yapfast reads; ignored";
    assert!(
        app.themes.installed[0]
            .notes
            .iter()
            .any(|note| note == ignored)
    );
    // What this Yapfast leaves out shows with the rest of the cost.
    interact(&mut app, |ui| {
        ui.click("Details").expect("Details");
    });
    assert!(shows(&app, "Left out by this Yapfast"));
    assert!(shows(&app, ignored));
    interact(&mut app, |ui| {
        ui.click("Apply").expect("Apply");
    });
    assert_eq!(app.theme.id, "installed:dev.example.plum");
    assert_eq!(app.theme.chat, crate::style::hex(0x2a1a3a));
    let particle_events = std::sync::Arc::clone(
        app.themes
            .particles("installed:dev.example.plum")
            .expect("theme applied particles")
            .1,
    );
    particle_events.lock().unwrap().clear();
    app.timeline.heights = vec![1234.0];
    let _ = app.update(crate::settings::appearance_event(
        crate::settings::ThemeEvent::ThemeOption(
            "installed:dev.example.plum".into(),
            "roomy".into(),
            serde_json::Value::Bool(true),
        ),
    ));
    assert_eq!(app.theme.spacing().message_gap, Some(12.0));
    assert_ne!(
        app.timeline.heights,
        vec![1234.0],
        "a metric-only option remeasures cached timeline rows"
    );
    assert!(
        particle_events.lock().unwrap().is_empty(),
        "an option resolution is not a deliberate theme apply"
    );
    let package_layout = app
        .themes
        .package_layout(&app.theme.id)
        .expect("resolved package layout");
    assert_eq!(
        package_layout.composer,
        Some(yapfast_core::themes::layout::ComposerSide::Top)
    );
    assert_eq!(
        package_layout.messages,
        Some(yapfast_core::themes::layout::MessageStyle::Bubbles)
    );
    assert_eq!(package_layout.chat_max_width, Some(720.0));
    assert_eq!(package_layout.chat_avatars, Some(false));
    assert_eq!(package_layout.bubbles.radius, Some(18.0));
    assert_eq!(package_layout.bubbles.padding, Some((14.0, 7.0)));
    let rail = package_layout.rail_tiles.as_ref().expect("rail tiles");
    assert_eq!(rail.width, 56.0);
    assert_eq!(rail.height, 40.0);
    assert_eq!(rail.label_size, 9.0);
    assert_eq!(rail.selected.offset, (2.0, 1.0));
    assert_eq!(rail.badge.radius, Some(5.0));
    assert_eq!(crate::rail::themed_width(Some(package_layout)), 74.0);
    let channels = package_layout.channel_rows.as_ref().expect("channel rows");
    assert_eq!(channels.height, Some(41.0));
    assert_eq!(channels.gap, Some(5.0));
    assert_eq!(channels.reserve, 12.0);
    assert_eq!(channels.icon.size, Some(18.0));
    assert_eq!(channels.icon.gap, Some(6.0));
    assert_eq!(channels.selected.underline.expect("underline").offset, 3.0);
    assert_eq!(channels.meta.voice, "voice · {n}");
    let members = package_layout.member_rows.as_ref().expect("member rows");
    assert_eq!(members.height, Some(52.0));
    assert_eq!(members.gap, Some(4.0));
    assert_eq!(members.reserve, 15.0);
    assert_eq!(members.avatar.size, Some(36.0));
    assert_eq!(members.meta.online.as_deref(), Some("here"));
    assert_eq!(
        members.badge.as_ref().expect("member badge").text,
        "P{online}"
    );
    assert!(!members.activity);
    assert_eq!(stored(&folder)["theme"], "installed:dev.example.plum");
    assert!(shows(&app, "Plum package"), "the applied theme's group");
    assert!(shows(&app, "Notification sounds"));
    let interface = "Interface sounds — clicks, hovers, menus, send";
    assert!(shows(&app, interface));
    // The volume shows once the sounds it sets are on.
    assert!(!shows(&app, "Interface sound volume"));
    interact(&mut app, |ui| {
        ui.click("Notification sounds")
            .expect("notification sounds");
        ui.click(interface).expect("interface sounds");
        ui.click("Transitions between channels")
            .expect("theme transitions");
    });
    assert!(shows(&app, "Interface sound volume"));
    let id = "installed:dev.example.plum";
    let _ = app.update(crate::settings::appearance_event(
        crate::settings::ThemeEvent::ThemeInterfaceVolume(id.into(), 73),
    ));
    assert!(
        app.theme.has_hover_cue(),
        "the theme shown takes the cue armed"
    );
    assert!(!app.layout.theme_sounds[id].on);
    assert!(app.layout.theme_sounds[id].interface);
    assert_eq!(app.layout.theme_sounds[id].volume, 73);
    assert!(app.layout.theme_transitions);
    let kept = stored(&folder);
    assert_eq!(kept["theme_sounds"][id]["on"], false);
    assert_eq!(kept["theme_sounds"][id]["interface"], true);
    assert_eq!(kept["theme_sounds"][id]["volume"], 73);
    assert_eq!(kept["theme_transitions"], true);

    // Returning every sound choice to its theme default removes this
    // package's map entry instead of keeping redundant settings.
    for event in [
        crate::settings::ThemeEvent::ThemeNotificationSounds(id.into(), true),
        crate::settings::ThemeEvent::ThemeInterfaceSounds(id.into(), false),
        crate::settings::ThemeEvent::ThemeInterfaceVolume(id.into(), 50),
    ] {
        let _ = app.update(crate::settings::appearance_event(event));
    }
    assert!(!app.layout.theme_sounds.contains_key(id));
    assert!(!app.theme.has_hover_cue());
    assert!(
        stored(&folder)["theme_sounds"]
            .as_object()
            .unwrap()
            .is_empty()
    );

    interact(&mut app, |ui| {
        ui.click("Turn off theme").expect("Turn off theme");
    });
    assert_eq!(app.theme.id, "bonfire");
    assert!(stored(&folder)["theme"].is_null());
    assert_eq!(start(&folder, true).theme.id, "bonfire");

    assert!(
        shows(&app, "Back to Plum package"),
        "Turn off names the theme Back will restore"
    );
    interact(&mut app, |ui| {
        ui.click("Back to Plum package").expect("Back to theme");
    });
    assert_eq!(app.theme.id, "installed:dev.example.plum");
    let layout = stored(&folder);
    assert_eq!(layout["theme"], "installed:dev.example.plum");
    assert!(layout["previous_theme"].is_null());
}

#[test]
fn a_refused_installed_theme_stays_visible_with_the_precise_reason() {
    let folder = tempfile::tempdir_in(local()).expect("test folder");
    let source = folder.path().join("changed-source");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(
        source.join(yapfast_core::themes::MANIFEST),
        r##"{
            "theme_api": 1,
            "id": "dev.example.changed",
            "name": "Changed",
            "version": "1.0.0",
            "variants": { "dark": { "base": "yap" } }
        }"##,
    )
    .unwrap();
    let fetched = yapfast_core::themes::install::Fetched::local(&source).unwrap();
    let themes = yapfast_core::themes::install::Themes::open(folder.path());
    themes.install(&fetched).unwrap();
    let installed = themes.installed();
    let stored_manifest = installed[0].folder.join(yapfast_core::themes::MANIFEST);
    std::fs::write(stored_manifest, "{}").unwrap();

    let mut app = start(&folder, true);
    open(&mut app, Section::Themes);
    assert!(app.themes.installed.is_empty());
    assert_eq!(app.themes.refused.len(), 1);
    assert!(shows(&app, "THEMES THAT COULDN'T BE OPENED"));
    assert!(shows(&app, "dev.example.changed"));
    assert!(shows(&app, "its theme.json changed since it was installed"));
}

#[test]
fn a_custom_theme_is_made_edited_and_deleted_in_themes() {
    use crate::settings::appearance_event as theme;
    let (mut app, folder) = preview();
    open(&mut app, Section::Appearance);
    interact(&mut app, |ui| {
        ui.click("New theme").expect("New theme");
    });
    assert_eq!(
        app.win.settings.get().map(|settings| settings.section()),
        Some(Section::Themes),
        "Appearance's shortcut opens the Themes creator"
    );
    assert!(shows(&app, "START FROM") && shows(&app, "EVERY COLOUR"));
    // A colour typed shows at once; one that isn't a colour says so.
    let chat = yapfast_core::theme::Theme::NAMES
        .iter()
        .position(|name| *name == "chat")
        .expect("chat");
    let _ = app.update(theme(crate::settings::ThemeEvent::Colour(
        chat,
        "#102030".into(),
    )));
    assert_eq!(app.theme.chat, crate::style::hex(0x102030));
    let _ = app.update(theme(crate::settings::ThemeEvent::Colour(
        chat,
        "#1020".into(),
    )));
    assert!(shows(&app, "Not a colour"));
    assert_eq!(
        app.theme.chat,
        crate::style::hex(0x102030),
        "the last good one"
    );
    let _ = app.update(theme(crate::settings::ThemeEvent::Colour(
        chat,
        "#203040".into(),
    )));
    let _ = app.update(theme(crate::settings::ThemeEvent::Name("Harbour".into())));
    interact(&mut app, |ui| {
        ui.click("Save theme").expect("Save theme");
    });
    assert_eq!(app.theme.id, "custom:1", "chosen once saved");
    let layout = stored(&folder);
    assert_eq!(layout["theme"], "custom:1");
    assert_eq!(layout["custom_themes"][0]["name"], "Harbour");
    assert_eq!(layout["custom_themes"][0]["colours"]["chat"], "#203040");
    assert!(shows(&app, "Harbour"), "among the tiles");
    // Edited, then cancelled: the theme from before comes back.
    let _ = app.update(theme(crate::settings::ThemeEvent::Edit("custom:1".into())));
    assert!(shows(&app, "EVERY COLOUR") && shows(&app, "Save"));
    let _ = app.update(theme(crate::settings::ThemeEvent::Colour(
        chat,
        "#ffffff".into(),
    )));
    assert_eq!(app.theme.chat, crate::style::hex(0xffffff));
    interact(&mut app, |ui| {
        ui.click("Cancel").expect("Cancel");
    });
    assert_eq!(app.theme.chat, crate::style::hex(0x203040));
    // Deleted, asked once more: the chosen theme goes, and Yap shows.
    let _ = app.update(theme(crate::settings::ThemeEvent::Edit("custom:1".into())));
    interact(&mut app, |ui| {
        ui.click("Delete theme").expect("Delete theme");
    });
    assert!(shows(&app, "Delete it for good"));
    interact(&mut app, |ui| {
        ui.click("Delete it for good").expect("Delete it for good");
    });
    assert_eq!(app.theme.id, "yap");
    assert_eq!(stored(&folder)["custom_themes"], serde_json::json!([]));
}

#[test]
fn the_guided_creator_previews_undoes_exports_and_keeps_its_recipe() {
    use crate::settings::theme_creator::{Event as CreatorEvent, PictureArrival, Step};
    use crate::settings::{ThemeEvent, appearance_event as theme};
    use yapfast_core::themes::creator::{Density, Messages};

    let (mut app, folder) = preview();
    open(&mut app, Section::Themes);
    let _ = app.update(theme(ThemeEvent::Creator(CreatorEvent::Open)));
    assert!(shows(&app, "Save theme"));
    assert!(shows(&app, "START FROM A THEME"));
    let generation = app
        .win
        .settings
        .get()
        .and_then(|settings| settings.creator.as_ref())
        .map(|creator| creator.generation())
        .expect("creator generation");

    let opened_accent = app.theme.accent;
    let _ = app.update(theme(ThemeEvent::Creator(CreatorEvent::AccentPreset(
        0xff6b6b,
    ))));
    assert_ne!(
        app.theme.accent, opened_accent,
        "the window previews colours"
    );
    let _ = app.update(theme(ThemeEvent::Creator(CreatorEvent::Undo)));
    assert_eq!(app.theme.accent, opened_accent, "Undo restores the preview");
    let _ = app.update(theme(ThemeEvent::Creator(CreatorEvent::AccentPreset(
        0xff6b6b,
    ))));

    let _ = app.update(theme(ThemeEvent::Creator(CreatorEvent::Step(Step::Shape))));
    assert!(shows(&app, "MESSAGES") && shows(&app, "CORNERS"));
    for event in [
        CreatorEvent::Messages(Messages::Cards),
        CreatorEvent::Corners(80),
        CreatorEvent::Density(Density::Roomy),
        CreatorEvent::Outline(2),
        CreatorEvent::Glass(true),
    ] {
        let _ = app.update(theme(ThemeEvent::Creator(event)));
    }

    let _ = app.update(theme(ThemeEvent::Creator(CreatorEvent::Step(Step::Fonts))));
    assert!(shows(&app, "FONT") && shows(&app, "Headings"));
    let _ = app.update(theme(ThemeEvent::Creator(CreatorEvent::Body(
        "tech".into(),
    ))));
    let _ = app.update(theme(ThemeEvent::Creator(CreatorEvent::Headings(Some(
        "slab".into(),
    )))));
    let _ = app.update(theme(ThemeEvent::Creator(CreatorEvent::FontReady(
        generation,
        true,
        Ok(
            include_bytes!("../../../../crates/yapfast-iced-kit/fonts/DisplayName-Medieval.otf")
                .to_vec(),
        ),
    ))));
    let imported_font = app
        .win
        .settings
        .get()
        .and_then(|settings| settings.creator.as_ref())
        .and_then(|creator| creator.spec.fonts.first())
        .map(|font| font.file.clone())
        .expect("the imported font");

    let picture =
        PictureArrival::prepare(include_bytes!("../../../../themes/template/preview.png"))
            .expect("the example picture");
    let _ = app.update(theme(ThemeEvent::Creator(CreatorEvent::PictureReady(
        generation,
        Ok(picture),
    ))));
    let _ = app.update(theme(ThemeEvent::Creator(CreatorEvent::Blur(62))));
    let _ = app.update(theme(ThemeEvent::Creator(CreatorEvent::Dim(47))));
    assert_eq!(app.theme.id, crate::themes::CREATOR_PREVIEW_ID);
    let stock = crate::themes::theme("stock", &yapfast_core::theme::YAP);
    assert_ne!(
        app.theme.surface_radius("button", 6.0),
        stock.surface_radius("button", 6.0),
        "shape tokens preview before Save"
    );
    assert_ne!(
        app.theme.header_font(crate::fonts::BOLD).family,
        crate::fonts::BOLD.family,
        "the imported heading font previews before Save"
    );
    assert!(
        app.themes
            .glass(
                crate::themes::CREATOR_PREVIEW_ID,
                yapfast_core::themes::package::art::ArtSurface::Sidebar,
            )
            .is_some(),
        "picture glass previews before Save"
    );

    let _ = app.update(theme(ThemeEvent::Creator(CreatorEvent::Step(
        Step::Advanced,
    ))));
    assert!(shows(&app, "EVERY COLOUR"));
    let chat = yapfast_core::theme::Theme::NAMES
        .iter()
        .position(|name| *name == "chat")
        .expect("chat");
    let _ = app.update(theme(ThemeEvent::Creator(CreatorEvent::AdvancedColour(
        chat,
        "#102030".into(),
    ))));
    assert_eq!(app.theme.chat, crate::style::hex(0x102030));
    let _ = app.update(theme(ThemeEvent::Creator(CreatorEvent::Name(
        "Guided Plum".into(),
    ))));

    let exported = tempfile::tempdir_in(local()).expect("export folder");
    let _ = app.update(theme(ThemeEvent::Creator(CreatorEvent::ExportTo(
        generation,
        Some(exported.path().to_owned()),
    ))));
    let exported = exported.path().join("guided-plum");
    assert!(exported.join("theme.json").is_file());
    assert!(exported.join("LICENSE").is_file());
    assert!(exported.join("fonts/DisplayName-Mainframe.ttf").is_file());
    assert!(exported.join(&imported_font).is_file());
    assert!(exported.join("art/picture.jpg").is_file());

    let _ = app.update(theme(ThemeEvent::Creator(CreatorEvent::Save)));
    assert_eq!(app.theme.id, "custom:1", "the saved theme is chosen");
    assert_ne!(
        app.theme.surface_radius("button", 6.0),
        stock.surface_radius("button", 6.0),
        "the saved creator shape applies without a restart"
    );
    assert!(
        app.themes
            .glass(
                "custom:1",
                yapfast_core::themes::package::art::ArtSurface::Sidebar,
            )
            .is_some(),
        "the saved creator glass applies without a restart"
    );
    let layout = stored(&folder);
    let custom = &layout["custom_themes"][0];
    assert_eq!(custom["name"], "Guided Plum");
    assert_eq!(custom["folder"], "themes/creator/1");
    assert_eq!(custom["creator"]["messages"], "cards");
    assert_eq!(custom["creator"]["corners"], 80);
    assert_eq!(custom["creator"]["density"], "roomy");
    assert_eq!(custom["creator"]["outline"], 2);
    assert_eq!(custom["creator"]["glass"], true);
    assert_eq!(custom["creator"]["body"], "tech");
    assert_eq!(custom["creator"]["headings"], imported_font);
    assert_eq!(custom["creator"]["background"], "picture");
    assert_eq!(custom["creator"]["blur"], 62);
    assert_eq!(custom["creator"]["dim"], 47);
    assert_eq!(custom["colours"]["chat"], "#102030");
    assert!(folder.path().join("themes/creator/1/theme.json").is_file());
    assert!(folder.path().join("themes/creator/1/LICENSE").is_file());

    let mut restarted = start(&folder, true);
    assert_eq!(restarted.theme.id, "custom:1");
    assert_ne!(
        restarted.theme.surface_radius("button", 6.0),
        stock.surface_radius("button", 6.0),
        "the creator shape resolves after restart"
    );
    assert!(
        restarted
            .themes
            .glass(
                "custom:1",
                yapfast_core::themes::package::art::ArtSurface::Sidebar,
            )
            .is_some(),
        "the creator picture/glass assets resolve after restart"
    );
    assert_eq!(
        restarted.themes.custom[0].creator.as_ref().unwrap().corners,
        80
    );
    assert_eq!(
        restarted.themes.custom[0].folder.as_deref(),
        Some("themes/creator/1")
    );

    open(&mut restarted, Section::Themes);
    let _ = restarted.update(theme(ThemeEvent::Edit("custom:1".into())));
    assert!(
        restarted
            .win
            .settings
            .get()
            .is_some_and(|settings| settings.creator.is_some() && settings.editor.is_none()),
        "a creator theme reopens in the guided creator"
    );
    let _ = restarted.update(theme(ThemeEvent::Creator(CreatorEvent::Corners(20))));
    let _ = restarted.update(theme(ThemeEvent::Creator(CreatorEvent::Name(
        "Guided Plum edited".into(),
    ))));
    let _ = restarted.update(theme(ThemeEvent::Creator(CreatorEvent::Save)));
    assert_eq!(
        restarted.themes.custom.len(),
        1,
        "Edit updates, not duplicates"
    );
    assert_eq!(restarted.themes.custom[0].id, "custom:1");
    assert_eq!(restarted.themes.custom[0].name, "Guided Plum edited");
    assert_eq!(
        restarted.themes.custom[0].creator.as_ref().unwrap().corners,
        20
    );
    assert!(
        folder
            .path()
            .join("themes/creator/1/art/picture.jpg")
            .is_file()
    );

    let _ = restarted.update(theme(ThemeEvent::Edit("custom:1".into())));
    let _ = restarted.update(theme(ThemeEvent::Creator(CreatorEvent::Delete)));
    let _ = restarted.update(theme(ThemeEvent::Creator(CreatorEvent::Delete)));
    assert!(restarted.themes.custom.is_empty());
    assert_eq!(restarted.theme.id, "yap");
    assert!(stored(&folder)["previous_theme"].is_null());
    assert!(
        !folder.path().join("themes/creator/1").exists(),
        "Delete removes only the creator-owned package folder"
    );
}

#[test]
fn creator_assets_follow_undo_start_over_and_async_generations() {
    use crate::settings::theme_creator::{Event as CreatorEvent, PictureArrival, Step, Suggested};
    use crate::settings::{ThemeEvent, appearance_event as theme};

    let (mut app, folder) = preview();
    open(&mut app, Section::Themes);
    let _ = app.update(theme(ThemeEvent::Creator(CreatorEvent::Open)));
    let generation = app
        .win
        .settings
        .get()
        .and_then(|settings| settings.creator.as_ref())
        .map(|creator| creator.generation())
        .unwrap();
    let _ = app.update(theme(ThemeEvent::Creator(CreatorEvent::Cancel)));
    let _ = app.update(theme(ThemeEvent::Creator(CreatorEvent::Open)));
    let current = app
        .win
        .settings
        .get()
        .and_then(|settings| settings.creator.as_ref())
        .map(|creator| creator.generation())
        .unwrap();
    assert_ne!(generation, current);
    let suggested = Suggested::read(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../themes/template/preview.png"),
    )
    .expect("suggested picture thumbnail");
    let _ = app.update(theme(ThemeEvent::Creator(CreatorEvent::Suggestions(
        current,
        vec![suggested],
    ))));
    let _ = app.update(theme(ThemeEvent::Creator(CreatorEvent::Step(Step::Start))));
    assert!(shows(&app, "OR FROM A PICTURE"));

    let old_picture =
        PictureArrival::prepare(include_bytes!("../../../../themes/template/preview.png")).unwrap();
    let _ = app.update(theme(ThemeEvent::Creator(CreatorEvent::PictureReady(
        generation,
        Ok(old_picture),
    ))));
    let _ = app.update(theme(ThemeEvent::Creator(CreatorEvent::FontReady(
        generation,
        false,
        Ok(
            include_bytes!("../../../../crates/yapfast-iced-kit/fonts/DisplayName-Medieval.otf")
                .to_vec(),
        ),
    ))));
    let creator = app.win.settings.get().unwrap().creator.as_ref().unwrap();
    assert!(creator.spec.picture.is_none() && creator.spec.fonts.is_empty());
    assert!(
        !folder
            .path()
            .join("themes/creator/draft/art/picture.jpg")
            .exists()
    );

    let picture =
        PictureArrival::prepare(include_bytes!("../../../../themes/template/preview.png")).unwrap();
    let _ = app.update(theme(ThemeEvent::Creator(CreatorEvent::PictureReady(
        current,
        Ok(picture),
    ))));
    let _ = app.update(theme(ThemeEvent::Creator(CreatorEvent::FontReady(
        current,
        false,
        Ok(
            include_bytes!("../../../../crates/yapfast-iced-kit/fonts/DisplayName-Medieval.otf")
                .to_vec(),
        ),
    ))));
    let font = app
        .win
        .settings
        .get()
        .unwrap()
        .creator
        .as_ref()
        .unwrap()
        .spec
        .fonts[0]
        .file
        .clone();
    assert!(
        folder
            .path()
            .join("themes/creator/draft/art/picture.jpg")
            .is_file()
    );
    assert!(
        folder
            .path()
            .join("themes/creator/draft")
            .join(&font)
            .is_file()
    );

    let _ = app.update(theme(ThemeEvent::Creator(CreatorEvent::Start(0))));
    assert!(
        !folder
            .path()
            .join("themes/creator/draft/art/picture.jpg")
            .exists()
    );
    let _ = app.update(theme(ThemeEvent::Creator(CreatorEvent::Undo)));
    let restored = app.win.settings.get().unwrap().creator.as_ref().unwrap();
    assert!(restored.spec.picture.is_some());
    assert_eq!(restored.spec.body, font);
    assert!(
        folder
            .path()
            .join("themes/creator/draft/art/picture.jpg")
            .is_file()
    );
    assert!(
        folder
            .path()
            .join("themes/creator/draft")
            .join(&font)
            .is_file()
    );

    let _ = app.update(theme(ThemeEvent::Creator(CreatorEvent::StartOver)));
    let reset = app.win.settings.get().unwrap().creator.as_ref().unwrap();
    assert!(reset.spec.picture.is_none() && reset.spec.fonts.is_empty());
    assert!(
        !folder
            .path()
            .join("themes/creator/draft/art/picture.jpg")
            .exists()
    );
    assert!(
        !folder
            .path()
            .join("themes/creator/draft")
            .join(font)
            .exists()
    );
    let _ = app.update(theme(ThemeEvent::Creator(CreatorEvent::Corners(90))));
    let preview = app.theme.clone();
    assert_eq!(preview.id, crate::themes::CREATOR_PREVIEW_ID);
    let stock = crate::themes::theme("stock", &yapfast_core::theme::YAP);
    assert_ne!(preview.surface_radius("button", 6.0), stock.radius(6.0));
    let _ = app.update(theme(ThemeEvent::Creator(CreatorEvent::Cancel)));
    assert!(
        app.themes.find(crate::themes::CREATOR_PREVIEW_ID).is_none(),
        "cancel removes the draft package"
    );
    assert_eq!(
        app.theme.surface_radius("button", 6.0),
        stock.radius(6.0),
        "the window lets go of the preview's metrics"
    );
    assert!(!app.theme.packaged());
}

#[test]
fn creator_delete_refuses_a_folder_outside_its_owned_tree() {
    use crate::settings::theme_creator::Event as CreatorEvent;
    use crate::settings::{ThemeEvent, appearance_event as theme};
    use yapfast_core::theme::Custom;

    let folder = tempfile::tempdir_in(local()).unwrap();
    std::fs::create_dir_all(folder.path().join("themes/creator")).unwrap();
    let external = tempfile::tempdir_in(local()).unwrap();
    std::fs::write(external.path().join("keep"), b"mine").unwrap();
    let custom = Custom {
        id: "custom:1".into(),
        name: "Outside".into(),
        base: "yap",
        theme: yapfast_core::theme::YAP,
        creator: Some(yapfast_core::themes::creator::Spec::default()),
        folder: Some(external.path().display().to_string()),
    };
    crate::layout::store(folder.path(), "custom_themes", [&custom]);
    crate::layout::store(folder.path(), "theme", "custom:1");
    crate::layout::flush(folder.path());

    let mut app = start(&folder, true);
    open(&mut app, Section::Themes);
    let _ = app.update(theme(ThemeEvent::Edit("custom:1".into())));
    let _ = app.update(theme(ThemeEvent::Creator(CreatorEvent::Delete)));
    let _ = app.update(theme(ThemeEvent::Creator(CreatorEvent::Delete)));
    assert_eq!(
        app.themes.custom.len(),
        1,
        "refused deletion keeps the theme"
    );
    assert!(
        external.path().join("keep").is_file(),
        "external data is untouched"
    );
    assert!(
        app.win
            .settings
            .get()
            .and_then(|settings| settings.creator.as_ref())
            .and_then(|creator| creator.note.as_ref())
            .is_some_and(|(note, bad)| *bad && note.contains("not owned by the theme creator"))
    );
}

#[test]
fn creator_save_migrates_an_external_folder_without_touching_it() {
    use crate::settings::theme_creator::Event as CreatorEvent;
    use crate::settings::{ThemeEvent, appearance_event as theme};
    use yapfast_core::theme::Custom;

    let folder = tempfile::tempdir_in(local()).unwrap();
    let external = tempfile::tempdir_in(local()).unwrap();
    std::fs::write(external.path().join("keep"), b"mine").unwrap();
    let custom = Custom {
        id: "custom:1".into(),
        name: "Outside".into(),
        base: "yap",
        theme: yapfast_core::theme::YAP,
        creator: Some(yapfast_core::themes::creator::Spec::default()),
        folder: Some(external.path().display().to_string()),
    };
    crate::layout::store(folder.path(), "custom_themes", [&custom]);
    crate::layout::store(folder.path(), "theme", "custom:1");
    crate::layout::flush(folder.path());

    let mut app = start(&folder, true);
    open(&mut app, Section::Themes);
    let _ = app.update(theme(ThemeEvent::Edit("custom:1".into())));
    let _ = app.update(theme(ThemeEvent::Creator(CreatorEvent::Name(
        "Migrated".into(),
    ))));
    let _ = app.update(theme(ThemeEvent::Creator(CreatorEvent::Save)));
    assert!(
        external.path().join("keep").is_file(),
        "external data is untouched"
    );
    assert_eq!(app.themes.custom.len(), 1);
    assert_eq!(
        app.themes.custom[0].folder.as_deref(),
        Some("themes/creator/1")
    );
    assert!(folder.path().join("themes/creator/1/theme.json").is_file());
}

#[test]
fn a_custom_theme_is_exported_from_the_editor_and_imported_by_another_install() {
    use crate::settings::{ThemeEvent, appearance_event as theme};
    use yapfast_core::theme::Theme;
    let files = tempfile::tempdir_in(super::local()).expect("test folder");
    let (mut app, _folder) = preview();
    open(&mut app, Section::Appearance);
    let file = |app: &App| app.win.settings.get().expect("Settings").theme_file();
    let note = |app: &App| app.win.settings.get().expect("Settings").theme_note.clone();

    // Nothing to export outside the editor, or while a field holds no colour.
    assert!(file(&app).is_none());
    let _ = app.update(theme(ThemeEvent::Choose("tokyo-night".into())));
    interact(&mut app, |ui| {
        ui.click("New theme").expect("New theme");
    });
    let _ = app.update(theme(ThemeEvent::Name("Night / Shift".into())));
    let chat = Theme::NAMES
        .iter()
        .position(|name| *name == "chat")
        .expect("chat");
    let _ = app.update(theme(ThemeEvent::Colour(chat, "#102030".into())));
    let _ = app.update(theme(ThemeEvent::Dark(false)));
    let made = app
        .win
        .settings
        .get()
        .and_then(|settings| settings.editor.as_ref())
        .map(|editor| editor.theme)
        .expect("the editor");
    let _ = app.update(theme(ThemeEvent::Colour(chat, "#10".into())));
    assert!(file(&app).is_none());
    let _ = app.update(theme(ThemeEvent::Colour(chat, "#102030".into())));

    // The unsaved edit exports as it stands.
    let (file_name, text) = file(&app).expect("a theme to export");
    assert_eq!(file_name, "Night - Shift.yapfast-theme.json");
    let path = files.path().join(&file_name);
    std::fs::write(&path, text).expect("the file");
    let _ = app.update(theme(ThemeEvent::Exported(file_name.clone(), Ok(()))));
    assert!(shows(&app, "Exported to Night - Shift.yapfast-theme.json."));
    assert_eq!(note(&app).map(|(_, wrong)| wrong), Some(false));
    let _ = app.update(theme(ThemeEvent::Exported(
        file_name,
        Err("disk full".into()),
    )));
    assert!(shows(&app, "Couldn't save the theme file: disk full"));
    assert_eq!(note(&app).map(|(_, wrong)| wrong), Some(true));
    interact(&mut app, |ui| {
        ui.click("Cancel").expect("Cancel");
    });
    assert_eq!(note(&app), None, "gone with the editor");

    // Another install loads it as a theme of its own, chosen, and keeps it.
    let (mut other, other_folder) = preview();
    open(&mut other, Section::Appearance);
    let _ = other.update(theme(ThemeEvent::Imported(path.clone())));
    assert_eq!(other.theme.id, "custom:1");
    assert_eq!(other.theme.chat, crate::style::hex(0x102030));
    assert!(!other.theme.dark);
    assert_eq!(other.themes.custom.len(), 1);
    assert_eq!(other.themes.custom[0].name, "Night / Shift");
    assert_eq!(other.themes.custom[0].base, "tokyo-night");
    assert_eq!(other.themes.custom[0].theme, made);
    assert!(shows(&other, "Night / Shift"), "among the tiles");
    assert!(shows(&other, "Imported Night / Shift."));
    assert_eq!(note(&other).map(|(_, wrong)| wrong), Some(false));
    let layout = stored(&other_folder);
    assert_eq!(layout["theme"], "custom:1");
    assert_eq!(layout["custom_themes"][0]["base"], "tokyo-night");
    assert_eq!(layout["custom_themes"][0]["colours"]["chat"], "#102030");

    // Importing it again adds another with its own id.
    let _ = other.update(theme(ThemeEvent::Imported(path.clone())));
    assert_eq!(other.theme.id, "custom:2");
    assert_eq!(other.themes.custom.len(), 2);

    // A file that isn't a theme, or can't be read, changes nothing but the
    // note.
    let junk = files.path().join("junk.json");
    std::fs::write(&junk, "{}").expect("the file");
    let _ = other.update(theme(ThemeEvent::Imported(junk)));
    assert!(shows(&other, "That file is not a Yapfast theme."));
    assert_eq!(note(&other).map(|(_, wrong)| wrong), Some(true));
    let missing = files.path().join("missing.json");
    let _ = other.update(theme(ThemeEvent::Imported(missing)));
    assert!(note(&other).is_some_and(|(note, wrong)| wrong && note.starts_with("Couldn't read")));
    assert_eq!(other.theme.id, "custom:2");
    assert_eq!(other.themes.custom.len(), 2);

    // The editor owns the window's colours while it is open.
    interact(&mut other, |ui| {
        ui.click("New theme").expect("New theme");
    });
    let _ = other.update(theme(ThemeEvent::Imported(path)));
    assert!(shows(
        &other,
        "Save or cancel the theme being edited first."
    ));
    assert_eq!(other.themes.custom.len(), 2);
    interact(&mut other, |ui| {
        ui.click("Cancel").expect("Cancel");
    });
    assert_eq!(note(&other), None);
}

#[test]
fn preferences_are_kept_across_a_restart() {
    let (mut app, folder) = preview();
    // The newest message sent at a time of its own, as Discord's are.
    change(&mut app, |state| {
        let mut messages = (*state.messages).clone();
        let last = messages.last_mut().expect("a message");
        let mut message = (**last).clone();
        message.timestamp = 1_790_000_000;
        *last = std::sync::Arc::new(message);
        state.messages = std::sync::Arc::new(messages);
    });
    open(&mut app, Section::Messages);
    interact(&mut app, |ui| {
        ui.click("12-hour").expect("the clock");
    });
    interact(&mut app, |ui| {
        ui.click("Show seconds").expect("the switch");
    });
    interact(&mut app, |ui| {
        ui.click("Collapse long messages").expect("the switch");
    });
    interact(&mut app, |ui| {
        ui.click("Large").expect("the text size");
    });
    assert!(app.layout.prefs.twelve_hour_clock && app.layout.prefs.clock_seconds);
    // The timeline's times follow at once: 9:41:05 PM, say.
    let time = &app.timeline.looks.last().expect("a message").time;
    assert!(
        time.matches(':').count() == 2 && (time.ends_with("AM") || time.ends_with("PM")),
        "{time}"
    );
    open(&mut app, Section::Notifications);
    interact(&mut app, |ui| {
        ui.click("Flash the taskbar").expect("the switch");
    });
    open(&mut app, Section::Advanced);
    interact(&mut app, |ui| {
        ui.click("Developer Mode").expect("the switch");
    });
    let again = start(&folder, true);
    let prefs = &again.layout.prefs;
    assert!(prefs.twelve_hour_clock && prefs.clock_seconds);
    assert!(again.layout.collapse_long && again.layout.developer_mode);
    assert_eq!(prefs.text_size, Some(2));
    assert_eq!(prefs.flash_taskbar, Some(false));
    // `layout.json` keeps them under the keys older versions wrote.
    let layout = stored(&folder);
    assert_eq!(layout["twelve_hour_clock"], true);
    assert_eq!(layout["flash_taskbar"], false);
}

#[test]
fn a_device_chosen_in_voice_settings_is_the_one_used() {
    let (mut app, _folder) = preview();
    change(&mut app, |state| {
        let device = |id: &str, name: &str| voice::Device {
            id: id.into(),
            name: name.into(),
        };
        state.voice.microphones = vec![device("mic-a", "Desk"), device("mic-b", "Headset")];
        state.voice.microphone = Some("mic-a".into());
    });
    open(&mut app, Section::Voice);
    assert!(shows(&app, "Desk"), "the one chosen");
    assert!(!shows(&app, "Headset"), "the rest only in the list");
    interact(&mut app, |ui| {
        ui.click("Desk").expect("the dropdown");
    });
    interact(&mut app, |ui| {
        ui.click("Headset").expect("its option");
    });
    assert_eq!(
        app.client.state().voice.microphone.as_deref(),
        Some("mic-b")
    );
    assert!(
        app.win.settings.get().is_some_and(|s| s.open.is_none()),
        "closed"
    );
}

#[test]
fn the_soundboard_volume_moves_with_its_drag() {
    let (mut app, _folder) = preview();
    change(&mut app, |state| state.discord_linked = true);
    open(&mut app, Section::Voice);
    assert!(shows(&app, "Soundboard volume"));
    assert!(!shows(&app, "37%"));
    let voice = |event| crate::app::settings(Event::Voice(event));
    let _ = app.update(voice(VoiceEvent::SoundboardVolume(37, false)));
    assert!(shows(&app, "37%"), "the slider follows the drag");
    // Let go, what the drag left is sent, not what the slider was built
    // with, and the slider shows the core's volume again.
    let _ = app.update(voice(VoiceEvent::SoundboardVolume(80, true)));
    assert!(
        app.win
            .settings
            .get()
            .is_some_and(|settings| settings.voice.soundboard.is_none())
    );
    assert!(!shows(&app, "37%"));
}

#[test]
fn a_recorded_push_to_talk_key_is_named_and_kept() {
    use crate::settings::keybinds::recorded;
    use iced::keyboard::key::{Code, Physical};
    let v = recorded(
        &Key::Character("v".into()),
        Physical::Code(Code::KeyV),
        Location::Standard,
    )
    .expect("a key");
    assert_eq!(v.name, "V");
    if cfg!(target_os = "linux") {
        assert_eq!(v.scancode, 47, "evdev's, as winit gives it");
        assert_eq!(v.trigger.as_deref(), Some("v"));
    }
    let ctrl = recorded(
        &Key::Named(key::Named::Control),
        Physical::Code(Code::ControlRight),
        Location::Right,
    )
    .expect("a key");
    assert_eq!(ctrl.name, "Right Ctrl");
    if cfg!(target_os = "linux") {
        assert_eq!(ctrl.trigger.as_deref(), Some("Control_R"));
    }

    let (mut app, folder) = preview();
    open(&mut app, Section::Voice);
    interact(&mut app, |ui| {
        ui.click("Push to talk").expect("the input mode");
    });
    interact(&mut app, |ui| {
        ui.click("Record keybind").expect("the button");
    });
    assert!(app.recording_keybind());
    assert!(shows(&app, "Press a key..."));
    // The keys pressed together are the keybind once they're all up, and
    // show as they're pressed.
    let heard = |key: &yapfast_core::keybinds::Key, pressed| {
        crate::app::settings(Event::Heard(crate::settings::keybinds::Heard::Key {
            key: key.clone(),
            pressed,
            escape: false,
        }))
    };
    let _ = app.update(heard(&ctrl, true));
    let _ = app.update(heard(&v, true));
    assert!(shows(&app, "Right Ctrl + V") || shows(&app, "Ctrl + V"));
    let _ = app.update(heard(&v, false));
    assert!(app.recording_keybind(), "a key is still down");
    let _ = app.update(heard(&ctrl, false));
    assert!(!app.recording_keybind());
    assert!(shows(&app, "Edit keybind"));
    let layout = stored(&folder);
    assert_eq!(layout["input_mode"], "push_to_talk");
    assert_eq!(layout["push_to_talk_key"]["name"], "V");
    assert_eq!(layout["push_to_talk_key"]["with"][0]["name"], "Right Ctrl");
    // Escape, pressed first, stops without changing it.
    interact(&mut app, |ui| {
        ui.click("Edit keybind").expect("the button");
    });
    let escape = yapfast_core::keybinds::Key {
        scancode: 1,
        name: "Escape".into(),
        trigger: None,
    };
    let _ = app.update(crate::app::settings(Event::Heard(
        crate::settings::keybinds::Heard::Key {
            key: escape,
            pressed: true,
            escape: true,
        },
    )));
    assert!(!app.recording_keybind());
    assert_eq!(stored(&folder)["push_to_talk_key"]["name"], "V");
    // Where the key works, as its watcher tells from its own thread,
    // shows without anything else happening; a word from a watcher gone
    // since doesn't.
    let told = app.keybinds.count;
    let status = |reach| {
        crate::keybinds::Told::Status(yapfast_core::keybinds::Status {
            reach,
            given: Default::default(),
        })
    };
    let everywhere = "Hold the key in any app to talk.";
    let _ = app.update(Message::Keybinds(
        told,
        status(yapfast_core::keybinds::Reach::Everywhere),
    ));
    assert!(shows(&app, everywhere));
    let focused = yapfast_core::keybinds::Reach::Focused("Your desktop has no global keys");
    let _ = app.update(Message::Keybinds(told - 1, status(focused.clone())));
    assert!(shows(&app, everywhere));
    // Kept while Settings are closed, for when they open.
    let _ = app.close_settings();
    let _ = app.update(Message::Keybinds(told, status(focused)));
    open(&mut app, Section::Voice);
    assert!(shows(
        &app,
        "Your desktop has no global keys, so push to talk works only while Yapfast's window is in front."
    ));
    let again = start(&folder, true);
    assert_eq!(
        again
            .layout
            .voice
            .push_to_talk_key
            .as_ref()
            .map(yapfast_core::keybinds::Shortcut::name),
        Some("Ctrl + V".to_owned())
    );
}

#[test]
fn keybinds_are_added_chosen_recorded_and_removed() {
    let (mut app, folder) = preview();
    open(&mut app, Section::Keybinds);
    assert!(shows(&app, "Add a Keybind"));
    interact(&mut app, |ui| {
        ui.click("Add a Keybind").expect("the button");
    });
    assert!(shows(
        &app,
        "This action doesn't do anything! Please choose one from the drop down."
    ));
    let keybind = |event| crate::app::settings(Event::Keybind(event));
    use crate::settings::keybinds::{Event as KeybindEvent, Heard, Target};
    let _ = app.update(keybind(KeybindEvent::Action(0, 4)));
    assert!(shows(&app, "Toggle Deafen"));
    let _ = app.update(crate::app::settings(Event::Record(Target::Row(0), true)));
    assert!(app.recording_keybind());
    let d = yapfast_core::keybinds::Key {
        scancode: 32,
        name: "D".into(),
        trigger: Some("d".into()),
    };
    for pressed in [true, false] {
        let _ = app.update(crate::app::settings(Event::Heard(Heard::Key {
            key: d.clone(),
            pressed,
            escape: false,
        })));
    }
    assert!(!app.recording_keybind());
    let layout = stored(&folder);
    assert_eq!(layout["keybinds"][0]["action"], "toggle_deafen");
    assert_eq!(layout["keybinds"][0]["keys"]["name"], "D");
    assert_eq!(layout["keybinds"][0]["id"], 1);
    // The watcher watches it: a toggle from it deafens.
    assert!(
        app.keybinds
            .watched
            .iter()
            .any(|watched| watched.id == "keybind-1")
    );
    // As the account card's button: out of a call, the next one starts
    // deafened.
    let deafened =
        |app: &crate::app::App| (app.client.state().voice.deafened, app.layout.deafen_on_join);
    let before = deafened(&app);
    let _ = app.update(Message::Keybinds(
        app.keybinds.count,
        crate::keybinds::Told::Toggled(yapfast_core::keybinds::Action::ToggleDeafen),
    ));
    assert_ne!(deafened(&app), before);
    let _ = app.update(keybind(KeybindEvent::Remove(0)));
    assert!(!shows(&app, "Toggle Deafen"));
    assert_eq!(stored(&folder)["keybinds"], serde_json::json!([]));
    assert!(app.keybinds.watched.is_empty());
}

#[test]
fn closing_settings_lets_go_of_an_edited_theme() {
    let (mut app, _folder) = preview();
    open(&mut app, Section::Themes);
    let _ = app.update(crate::settings::appearance_event(
        crate::settings::ThemeEvent::New,
    ));
    let _ = app.update(crate::settings::appearance_event(
        crate::settings::ThemeEvent::Dark(false),
    ));
    assert!(!app.theme.dark, "shown while edited");
    let _ = app.update(crate::app::settings(Event::Close));
    assert!(!app.win.settings.is_open());
    assert_eq!(app.theme.id, "bonfire", "the theme from before");
}

#[test]
fn a_new_setting_is_tagged_until_its_page_is_left() {
    use crate::settings::{Event as Settings, Section};
    // A layout from before the tags: everything tagged since is new.
    let folder = tempfile::tempdir_in(super::local()).expect("test folder");
    std::fs::write(folder.path().join("layout.json"), b"{}").expect("layout");
    let mut app = start(&folder, true);
    // Use the same full-page fixture as the other Settings view checks. With
    // Themes restored to the Slint build, the deliberately short
    // preview viewport otherwise clips About's row before this checks its tag.
    open(&mut app, Section::Account);
    assert!(shows(&app, "NEW"), "About's row says so");
    let _ = app.update(crate::app::settings(Settings::Open(Section::About)));
    assert!(shows(&app, "Check for updates") && shows(&app, "NEW"));
    // Leaving About sees them, and the layout keeps it.
    let _ = app.update(crate::app::settings(Settings::Open(Section::Account)));
    assert!(!shows(&app, "NEW"));
    let layout: serde_json::Value = serde_json::from_slice(&{
        crate::layout::flush(folder.path());
        std::fs::read(folder.path().join("layout.json")).expect("the layout")
    })
    .expect("JSON");
    let seen: Vec<&str> = layout["seen_new"]
        .as_array()
        .expect("seen")
        .iter()
        .filter_map(|key| key.as_str())
        .collect();
    assert_eq!(seen, ["check-updates", "update-nightlies"]);
    // A fresh install has met them all.
    let (mut fresh, _folder) = preview();
    let _ = fresh.update(crate::app::settings(Settings::Open(Section::About)));
    assert!(!shows(&fresh, "NEW"));
}

/// Every key the core tags has its tag in Settings' pages.
#[test]
fn every_tagged_setting_has_its_tag() {
    let pages = [
        include_str!("../settings/pages/mod.rs"),
        include_str!("../settings/pages/about.rs"),
        include_str!("../settings/pages/account.rs"),
        include_str!("../settings/pages/advanced.rs"),
        include_str!("../settings/pages/discord.rs"),
        include_str!("../settings/pages/games.rs"),
        include_str!("../settings/pages/messages.rs"),
        include_str!("../settings/pages/notifications.rs"),
        include_str!("../settings/pages/sounds.rs"),
    ]
    .concat();
    for tag in &yapfast_core::new_tags::NEW {
        assert!(
            pages.contains(&format!("self.fresh(\"{}\")", tag.key)),
            "{}",
            tag.key
        );
    }
}

#[test]
fn a_theme_switch_lays_the_timelines_text_out_in_the_new_inks() {
    use yapfast_core::chat::{ChatMessage, Notice, NoticeKind};
    let (mut app, _folder) = preview();
    super::change(&mut app, |state| {
        state.messages = std::sync::Arc::new(vec![std::sync::Arc::new(ChatMessage {
            item: "n0".into(),
            author: "Ann".into(),
            time: "12:01".into(),
            notice: Some(Notice {
                kind: NoticeKind::Pinned(Some("m1".into())),
                text: String::new(),
            }),
            ..Default::default()
        })]);
    });
    let ink = |app: &crate::app::App| {
        app.timeline.looks[0].body.parts[0]
            .runs
            .iter()
            .find(|run| run.text.contains("Ann"))
            .and_then(|run| run.color)
    };
    assert_eq!(ink(&app), Some(crate::theme::rgb(app.theme.text)));
    assert!(app.choose_theme("catppuccin-latte"), "a light theme");
    let _ = app.sync();
    assert_eq!(
        ink(&app),
        Some(crate::theme::rgb(app.theme.text)),
        "the author's name in the light theme's ink"
    );
}

#[test]
fn the_theme_command_switches_the_colours_and_keeps_the_choice() {
    let (mut app, folder) = preview();
    assert_eq!(app.theme.id, "bonfire", "the default with the Discord look");
    let _ = app.update(Message::Control("theme sakura".into()));
    assert_eq!(app.theme.id, "sakura");
    assert!(!app.theme.dark);
    assert_eq!(app.theme().palette().background, app.theme.chat);
    let layout: serde_json::Value = serde_json::from_slice(&{
        crate::layout::flush(folder.path());
        std::fs::read(folder.path().join("layout.json")).expect("the layout")
    })
    .expect("JSON");
    assert_eq!(layout["theme"], "sakura");
    // A theme Yapfast doesn't have changes nothing.
    let _ = app.update(Message::Control("theme gone".into()));
    assert_eq!(app.theme.id, "sakura");
    // The next start shows the one chosen.
    assert_eq!(start(&folder, true).theme.id, "sakura");
}

#[test]
fn theme_performance_reduced_motion_and_readable_text_are_kept_in_the_layout() {
    let (mut app, folder) = preview();
    open(&mut app, Section::Themes);
    let _ = app.update(crate::settings::appearance_event(
        crate::settings::ThemeEvent::Performance(2),
    ));
    let _ = app.update(crate::settings::appearance_event(
        crate::settings::ThemeEvent::ReduceMotion(true),
    ));
    let _ = app.update(crate::settings::appearance_event(
        crate::settings::ThemeEvent::ReadableText(true),
    ));
    assert_eq!(
        app.layout.theme_performance,
        yapfast_core::themes::Performance::Light
    );
    assert!(app.layout.reduce_motion);
    assert!(app.layout.readable_text);
    crate::layout::flush(folder.path());
    let layout: serde_json::Value =
        serde_json::from_slice(&std::fs::read(folder.path().join("layout.json")).unwrap()).unwrap();
    assert_eq!(layout["theme_performance"], "light");
    assert_eq!(layout["reduce_motion"], true);
    assert_eq!(layout["readable_text"], true);
}

#[test]
fn a_custom_theme_in_the_layout_shows_from_the_start() {
    let folder = tempfile::tempdir_in(local()).expect("test folder");
    std::fs::write(
        folder.path().join("layout.json"),
        r##"{"theme": "custom:1", "custom_themes": [{"id": "custom:1", "name": "Plum",
            "base": "yap", "colours": {"chat": "#2a1a3a", "accent": "#c04080"}}]}"##,
    )
    .expect("the layout");
    let app = start(&folder, true);
    assert_eq!(app.theme.id, "custom:1");
    assert_eq!(app.theme.chat, crate::style::hex(0x2a1a3a));
    assert_eq!(app.theme.accent, crate::style::hex(0xc04080));
    assert_eq!(app.theme.selection.a, 0.35, "worked out from its accent");
}

#[test]
fn a_layout_key_of_the_wrong_kind_loses_only_itself() {
    let folder = tempfile::tempdir_in(local()).expect("test folder");
    std::fs::write(
        folder.path().join("layout.json"),
        r#"{"theme": "sakura", "collapse_long": "yes", "members_hidden": true}"#,
    )
    .expect("the layout");
    let app = start(&folder, true);
    assert_eq!(app.theme.id, "sakura");
    assert!(app.layout.members_hidden);
    assert!(!app.layout.collapse_long);
}

#[test]
fn edit_profile_from_settings_closes_them_under_the_editor() {
    let (mut app, _folder) = preview();
    change(&mut app, |state| state.discord_user = Some("42".into()));
    open(&mut app, Section::Account);
    let _ = app.update(crate::app::settings(Event::EditProfile));
    assert!(
        app.client.state().profile_editor.is_some(),
        "the editor opens"
    );
    assert!(app.win.settings.get().is_none(), "Settings close under it");
}

/// A call going on in Lounge, joined as the preview joins, unmuted
/// unless `muted`: after the join, which turns the microphone on.
fn in_a_call(app: &mut App, muted: bool) {
    for command in [
        voice::Command::Join("lounge".into()),
        voice::Command::Mute(muted),
    ] {
        app.client
            .dispatch(yapfast_core::application::Action::Voice(command));
        let _ = app.sync();
    }
    let voice = &app.client.state().voice;
    assert_eq!(
        (voice.active(), voice.muted, voice.deafened),
        (true, muted, false)
    );
}

fn testing(app: &App) -> bool {
    app.win
        .settings
        .get()
        .is_some_and(|settings| settings.voice.testing && settings.voice.gate.is_some())
}

#[test]
fn the_mic_test_deafens_a_call_while_it_plays_back_as_discords_does() {
    let (mut app, _folder) = preview();
    in_a_call(&mut app, false);
    open(&mut app, Section::Voice);
    assert!(shows(
        &app,
        "You'll be muted and deafened in your call while you test."
    ));
    interact(&mut app, |ui| {
        ui.click("Let's check").expect("the mic test's button");
    });
    assert!(testing(&app), "the meter plays the microphone back");
    let call = |app: &App| {
        let voice = &app.client.state().voice;
        (voice.muted, voice.deafened)
    };
    assert_eq!(call(&app), (true, true), "deafened, and so muted");
    assert!(shows(&app, "Stop testing"));
    interact(&mut app, |ui| {
        ui.click("Stop testing").expect("the button");
    });
    assert!(!testing(&app));
    assert_eq!(call(&app), (false, false), "as it was");
    // Undeafened while it plays, the test ends, as Discord's does.
    let _ = app.update(crate::app::settings(Event::Voice(VoiceEvent::MicTest)));
    assert!(testing(&app));
    app.client
        .dispatch(yapfast_core::application::Action::Voice(
            voice::Command::Deafen(false),
        ));
    let _ = app.sync();
    assert!(!testing(&app));
    assert!(!app.client.state().voice.deafened);
}

#[test]
fn leaving_voice_or_settings_ends_the_mic_test_and_the_call_hears_again() {
    let (mut app, _folder) = preview();
    in_a_call(&mut app, true);
    open(&mut app, Section::Voice);
    let mic_test = crate::app::settings(Event::Voice(VoiceEvent::MicTest));
    let _ = app.update(mic_test.clone());
    assert!(app.client.state().voice.deafened);
    let _ = app.update(crate::app::settings(Event::Open(Section::Advanced)));
    let voice = &app.client.state().voice;
    assert_eq!(
        (voice.muted, voice.deafened),
        (true, false),
        "muted as before"
    );
    open(&mut app, Section::Voice);
    assert!(!testing(&app), "it doesn't start again");
    let _ = app.update(mic_test);
    assert!(app.client.state().voice.deafened);
    let _ = app.update(crate::app::settings(Event::Close));
    assert!(!app.client.state().voice.deafened, "closing ends it too");
}

#[test]
fn the_mic_test_without_a_call_deafens_nothing() {
    let (mut app, _folder) = preview();
    open(&mut app, Section::Voice);
    assert!(!shows(
        &app,
        "You'll be muted and deafened in your call while you test."
    ));
    let call = |app: &App| {
        let voice = &app.client.state().voice;
        (voice.muted, voice.deafened)
    };
    let before = call(&app);
    let _ = app.update(crate::app::settings(Event::Voice(VoiceEvent::MicTest)));
    assert!(testing(&app));
    assert_eq!(call(&app), before);
}

#[test]
fn themes_page_shows_empty_state_actions_and_gallery_toggle_with_cost() {
    let (mut app, _folder) = preview();
    open(&mut app, Section::Themes);
    assert!(shows(&app, "Browse the Marketplace"));
    assert!(shows(&app, "Add from URL"));
    assert!(shows(&app, "Create"));

    let _ = app.update(crate::app::settings(Event::Marketplace(
        crate::settings::theme_marketplace::Event::ToggleGallery(true),
    )));
    assert!(
        app.win
            .settings
            .get()
            .is_some_and(|s| s.marketplace.gallery),
        "gallery view state toggles cleanly"
    );
    let _ = app.update(crate::app::settings(Event::Marketplace(
        crate::settings::theme_marketplace::Event::ToggleCostDetails("review".into()),
    )));
    assert!(
        app.win
            .settings
            .get()
            .is_some_and(|s| s.marketplace.cost_details.contains("review")),
        "cost details disclosure state toggles cleanly"
    );
}

/// A theme's review shows the whole window in the stock look, not only its
/// card: the applied theme's art and overlays can't draw over it. Closing
/// the review shows the theme again.
#[test]
fn a_theme_review_shows_the_whole_window_in_the_stock_look() {
    use crate::settings::theme_marketplace::{Event as Market, Review};
    let (mut app, _folder) = preview();
    assert!(app.choose_theme("nord"));
    open(&mut app, Section::Themes);
    let fetched = yapfast_core::themes::install::Fetched::new(
        "https://github.com/ada/plum".into(),
        String::new(),
        "main".into(),
        "0123456789abcdef0123456789abcdef01234567".into(),
        yapfast_core::themes::check::Files {
            manifest: r##"{ "theme_api": 1, "id": "com.example.plum", "name": "Plum",
                "version": "1.0.0", "variants": { "dark": { "base": "yap" } } }"##
                .into(),
            ..Default::default()
        },
    )
    .expect("a package");
    app.win
        .settings
        .get_mut()
        .expect("Settings")
        .marketplace
        .review = Some(Review::new(fetched, Vec::new()));
    let _ = app.update(Message::Nothing);
    assert_eq!(app.theme.id, crate::themes::stock_theme().id);
    let _ = app.update(crate::app::settings(Event::Marketplace(
        Market::CancelReview,
    )));
    assert_eq!(app.theme.id, "nord");
}
