//! Settings: Appearance offers theme shortcuts, Themes manages installed and
//! custom themes, preferences are kept across a restart, a device is chosen
//! in Voice & video, and push to talk's key is recorded and kept.

use iced::{
    Point,
    keyboard::{Key, Location, key},
    mouse,
    widget::Id,
};
use yapfast_core::voice;

use super::{change, interact, local, preview, shows, start};
use crate::{
    app::{
        App, Message,
        layers::{Layer, Popup},
    },
    settings::{
        Event, Section, THEME_COLOUR_CODE, ThemeEvent, appearance_event, voice::Event as VoiceEvent,
    },
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

#[test]
fn interface_language_changes_live_and_survives_restart() {
    let (mut app, folder) = preview();
    open(&mut app, Section::LanguageTime);
    assert!(shows(&app, "Interface language"));
    interact(&mut app, |ui| {
        ui.click("System language").expect("language dropdown");
    });
    interact(&mut app, |ui| {
        ui.click("Русский").expect("Russian language option");
    });
    assert!(shows(&app, "Язык интерфейса"));
    assert_eq!(stored(&folder)["language"], "ru");

    open(&mut app, Section::Advanced);
    assert!(shows(&app, "Расширенные"));
    let _ = app.update(crate::app::settings(Event::Search("язык".into())));
    assert!(shows(&app, "Язык и время"));
    assert!(!shows(&app, "Appearance"));
    interact(&mut app, |ui| {
        ui.click("Язык и время").expect("translated section");
    });
    assert!(shows(&app, "Язык интерфейса"));

    let mut app = start(&folder, true);
    open(&mut app, Section::LanguageTime);
    assert!(shows(&app, "Язык интерфейса"));
    interact(&mut app, |ui| {
        ui.click("Русский").expect("current language");
    });
    interact(&mut app, |ui| {
        ui.click("Deutsch").expect("German language option");
    });
    assert!(shows(&app, "Sprache der Benutzeroberfläche"));
    assert_eq!(stored(&folder)["language"], "de");

    interact(&mut app, |ui| {
        ui.click("Deutsch").expect("current language");
    });
    interact(&mut app, |ui| {
        ui.click("Systemsprache").expect("system language option");
    });
    assert!(app.layout.prefs.language.is_none());
    assert!(stored(&folder)["language"].is_null());
}

#[test]
fn russian_settings_render_translations_for_the_reported_pages() {
    let (mut app, _folder) = preview();
    app.i18n.select(Some("ru"));
    change(&mut app, |state| {
        state.discord_linked = true;
        state.storage = Some("The Discord token is stored in the OS credential store.");
    });
    for (section, label) in [
        (
            Section::Account,
            "Токен Discord хранится в системном хранилище учётных данных.",
        ),
        (Section::Sounds, "Настроить громкость каждого звука"),
        (Section::Appearance, "Использовать системный заголовок окна"),
        (Section::Discord, "Показывать недоступные каналы"),
        (Section::Games, "Имя исполняемого файла или полный путь"),
        (Section::Themes, "Тем пока нет"),
        (
            Section::Advanced,
            "Повышенное качество предпросмотра трансляции",
        ),
        (Section::About, "Устанавливать обновления автоматически"),
    ] {
        open(&mut app, section);
        assert!(shows(&app, label), "{section:?}: {label}");
    }
    open(&mut app, Section::Sounds);
    interact(&mut app, |ui| {
        ui.click("Пользовательский набор звуков")
            .expect("custom sound switch");
    });
    assert!(shows(&app, "Входящий звонок"));
    assert!(shows(&app, "Участник завершает трансляцию"));
    assert!(shows(&app, "По умолчанию"));
}

#[test]
fn custom_sounds_show_call_tones_and_keep_files_when_turned_off() {
    use yapfast_core::themes::media::SoundSlot;

    use crate::custom_sounds::Choice;

    let (mut app, folder) = preview();
    open(&mut app, Section::Sounds);
    assert!(!app.layout.voice.adjust_each_sound);
    assert!(!shows(&app, "Incoming call"));
    interact(&mut app, |ui| {
        ui.click("Custom sound set").expect("the switch");
    });
    assert!(shows(&app, "Incoming call"));
    assert!(shows(&app, "Outgoing call"));
    assert!(!shows(&app, "Uploaded file"));

    interact(&mut app, |ui| {
        let row = super::last(ui, "Outgoing call");
        let source = ui
            .find(
                |candidate: iced_test::selector::Candidate<'_>| match candidate {
                    iced_test::selector::Candidate::Text {
                        content: "Default",
                        visible_bounds: Some(bounds),
                        ..
                    } if (bounds.center_y() - row.y).abs() < 8.0 => Some(bounds.center()),
                    _ => None,
                },
            )
            .expect("the outgoing call's source");
        ui.point_at(source);
        let _ = ui.simulate(iced_test::simulator::click());
    });
    // A file or the default, no other set's sounds: the only Glow is the
    // sound set's, above the rows.
    assert!(shows(&app, "Custom"));
    interact(&mut app, |ui| {
        assert!(super::last(ui, "Glow").y < super::last(ui, "Incoming call").y);
    });
    interact(&mut app, |ui| {
        ui.click("Custom").expect("the file picker option");
    });
    assert!(!shows(&app, "Custom"));
    assert_eq!(
        app.layout.voice.sound_choice(SoundSlot::Ringback),
        Choice::Default,
    );
    crate::custom_sounds::save(folder.path(), SoundSlot::Ring, &[100, -100]).unwrap();
    interact(&mut app, |ui| {
        ui.click("Custom sound set")
            .expect("turn the custom set off");
    });
    assert!(app.layout.voice.custom_sounds.is_empty());
    let mut reopened = start(&folder, true);
    open(&mut reopened, Section::Sounds);
    assert!(!reopened.layout.voice.uses_custom_sounds());
    assert!(reopened.layout.voice.custom_sounds.is_empty());
    interact(&mut reopened, |ui| {
        ui.click("Custom sound set")
            .expect("turn the custom set back on");
    });
    assert!(shows(&reopened, "Uploaded file"));
    assert_eq!(
        reopened.layout.voice.sound_choice(SoundSlot::Ring),
        Choice::File
    );
    assert_eq!(
        reopened.layout.voice.sound_choice(SoundSlot::Ringback),
        Choice::Default,
    );
    reopened.shell.frame.width = 800.0;
    crate::window_bar::fit_window(&mut reopened);
    assert!(shows(&reopened, "Outgoing call"));
    assert!(shows(&reopened, "Uploaded file"));
}

/// The text glow switch stops a theme's soft edge, and is kept.
#[test]
fn the_text_glow_switch_stops_the_soft_edge_and_is_kept() {
    let (mut app, folder) = preview();
    open(&mut app, Section::Appearance);
    assert!(shows(&app, "Draw text glow"), "the switch");
    interact(&mut app, |ui| {
        ui.click("Draw text glow").expect("the switch");
    });
    assert_eq!(app.layout.prefs.text_glow, Some(false), "off at once");
    assert_eq!(stored(&folder)["text_glow"], false, "and kept");
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

/// Showing connections on profile cards is off until it's turned on in
/// Appearance's PROFILES, and stays on after a restart.
#[test]
fn showing_connections_on_cards_is_kept() {
    let (mut app, folder) = preview();
    assert!(!app.layout.prefs.card_connections, "off by default");
    open(&mut app, Section::Appearance);
    interact(&mut app, |ui| {
        ui.click("Show connections on profile cards")
            .expect("the switch");
    });
    assert!(app.layout.prefs.card_connections);
    assert_eq!(stored(&folder)["card_connections"], true);
    let again = start(&folder, true);
    assert!(again.layout.prefs.card_connections);
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
fn a_new_themes_pill_and_header_follow_its_text_and_chat_until_set() {
    use crate::settings::{ThemeEvent, appearance_event as theme};
    use yapfast_core::theme::{Parts, Theme};
    let (mut app, folder) = preview();
    open(&mut app, Section::Appearance);
    interact(&mut app, |ui| {
        ui.click("New theme").expect("New theme");
    });
    let at = |name: &str| Theme::NAMES.iter().position(|known| *known == name);
    let (chat, text) = (at("chat").expect("chat"), at("text").expect("text"));
    let part = |token: &str| Parts::TOKENS.iter().position(|known| *known == token);
    let header = Theme::NAMES.len() + part("header").expect("header");
    assert!(
        shows(&app, "pill") && shows(&app, "header"),
        "rows of their own"
    );
    // Not set, they follow: the pill is the new text, and the header has
    // no ground of its own over the new chat.
    let _ = app.update(theme(ThemeEvent::Colour(chat, "#102030".into())));
    let _ = app.update(theme(ThemeEvent::Colour(text, "#f0e0d0".into())));
    assert_eq!(app.theme.pill(), crate::style::hex(0xf0e0d0));
    assert_eq!(app.theme.header_ground(), None);
    // Set, the header keeps its own through another chat; the pill still
    // follows the text.
    let _ = app.update(theme(ThemeEvent::Colour(header, "#0a141d".into())));
    let _ = app.update(theme(ThemeEvent::Colour(chat, "#000000".into())));
    let _ = app.update(theme(ThemeEvent::Colour(text, "#d7e7f2".into())));
    assert_eq!(
        app.theme.header_ground(),
        Some(crate::style::hex(0x0a141d).into())
    );
    assert_eq!(app.theme.pill(), crate::style::hex(0xd7e7f2));
    interact(&mut app, |ui| {
        ui.click("Save theme").expect("Save theme");
    });
    let colours = &stored(&folder)["custom_themes"][0]["colours"];
    assert_eq!(colours["header"], "#0a141d");
    assert!(colours.get("pill").is_none(), "the pill still follows");
    assert_eq!(
        app.theme.header_ground(),
        Some(crate::style::hex(0x0a141d).into()),
        "as saved"
    );
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
        parts: Default::default(),
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
        parts: Default::default(),
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

    // The unsaved edit exports as it stands, and Copy says it copied it.
    let _ = app.update(theme(ThemeEvent::Copy));
    assert!(shows(&app, "Copied the theme to the clipboard."));
    let (file_name, text) = file(&app).expect("a theme to export");
    assert_eq!(file_name, "Night - Shift.yapfast-theme.json");
    let path = files.path().join(&file_name);
    std::fs::write(&path, &text).expect("the file");
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

    // Pasting the same text, as Copy put it on the clipboard, adds one too;
    // a clipboard without text or with other text changes nothing but the
    // note.
    let pasted = std::fs::read_to_string(&path).expect("the file");
    let _ = other.update(theme(ThemeEvent::Pasted(Some(pasted))));
    assert_eq!(other.theme.id, "custom:3");
    assert_eq!(other.themes.custom[2].theme, made);
    let _ = other.update(theme(ThemeEvent::Pasted(None)));
    assert!(shows(&other, "The clipboard holds no text."));
    let _ = other.update(theme(ThemeEvent::Pasted(Some("hello".into()))));
    assert!(shows(&other, "That is not a Yapfast theme."));
    assert_eq!(other.themes.custom.len(), 3);

    // A file that isn't a theme, or can't be read, changes nothing but the
    // note.
    let junk = files.path().join("junk.json");
    std::fs::write(&junk, "{}").expect("the file");
    let _ = other.update(theme(ThemeEvent::Imported(junk)));
    assert!(shows(&other, "That is not a Yapfast theme."));
    assert_eq!(note(&other).map(|(_, wrong)| wrong), Some(true));
    let missing = files.path().join("missing.json");
    let _ = other.update(theme(ThemeEvent::Imported(missing)));
    assert!(note(&other).is_some_and(|(note, wrong)| wrong && note.starts_with("Couldn't read")));
    assert_eq!(other.theme.id, "custom:3");
    assert_eq!(other.themes.custom.len(), 3);

    // The editor owns the window's colours while it is open.
    interact(&mut other, |ui| {
        ui.click("New theme").expect("New theme");
    });
    let _ = other.update(theme(ThemeEvent::Imported(path)));
    assert!(shows(
        &other,
        "Save or cancel the theme being edited first."
    ));
    let _ = other.update(theme(ThemeEvent::Pasted(Some(text))));
    assert_eq!(other.themes.custom.len(), 3);
    interact(&mut other, |ui| {
        ui.click("Cancel").expect("Cancel");
    });
    assert_eq!(note(&other), None);
}

#[test]
fn the_theme_editors_colour_picker_opens_picks_and_dismisses() {
    let (mut app, _folder) = preview();
    open(&mut app, Section::Appearance);
    interact(&mut app, |ui| {
        ui.click("New theme").expect("New theme");
    });
    let chat = yapfast_core::theme::Theme::NAMES
        .iter()
        .position(|name| *name == "chat")
        .expect("chat");
    let picker = |app: &App| {
        app.win
            .settings
            .get()
            .and_then(|settings| settings.editor.as_ref())
            .and_then(|editor| editor.picker)
    };
    let swatch_at = |ui: &mut iced_test::Simulator<'_, Message>, name: &str| {
        let label = ui
            .find(name)
            .expect(name)
            .visible_bounds()
            .expect("in view");
        Point::new(label.x - 10.0 - 14.0, label.center_y())
    };
    let click_at = |ui: &mut iced_test::Simulator<'_, Message>, point: Point| {
        ui.point_at(point);
        ui.simulate([
            iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        ]);
    };

    // Clicking a colour's swatch opens its ColourPad card under it.
    assert_eq!(picker(&app), None);
    interact(&mut app, |ui| {
        let swatch = swatch_at(ui, "chat");
        click_at(ui, swatch);
    });
    assert_eq!(picker(&app), Some(chat));

    // Clicking on the ColourPad's hue strip and square updates the hex
    // field and live preview immediately.
    let before = app.theme.chat;
    interact(&mut app, |ui| {
        let code = ui
            .find(Id::new(THEME_COLOUR_CODE))
            .expect("the picker's code field")
            .bounds();
        // One third across the hue strip (10 px tall, 14 px above the code field).
        click_at(ui, Point::new(code.x + code.width / 3.0, code.y - 19.0));
        // Upper-right of the saturation/brightness square (150 px tall, 34 px above code).
        click_at(ui, Point::new(code.x + code.width - 2.0, code.y - 180.0));
    });
    assert_ne!(app.theme.chat, before, "live preview updated from pad");
    let pad_hex = app
        .win
        .settings
        .get()
        .and_then(|settings| settings.editor.as_ref())
        .expect("editor")
        .fields[chat]
        .1
        .clone();
    assert!(shows(&app, &pad_hex), "hex field updated from pad");

    // Exact HSV picks update hex + preview, and hue stays while on black/grey.
    let _ = app.update(appearance_event(ThemeEvent::Hsv([240.0, 1.0, 0.5])));
    assert_eq!(app.theme.chat, crate::style::hex(0x000080));
    assert!(shows(&app, "#000080"));
    let _ = app.update(appearance_event(ThemeEvent::Hsv([120.0, 0.0, 0.0])));
    assert_eq!(app.theme.chat, crate::style::hex(0x000000));
    let hsv_hue = app
        .win
        .settings
        .get()
        .and_then(|settings| settings.editor.as_ref())
        .expect("editor")
        .hsv[0];
    assert_eq!(hsv_hue, 120.0, "hue kept while on black");
    let _ = app.update(appearance_event(ThemeEvent::Hsv([120.0, 1.0, 1.0])));
    assert_eq!(app.theme.chat, crate::style::hex(0x00ff00));

    // Typing a hex code into the picker's field updates the preview and pad.
    interact(&mut app, |ui| {
        ui.click(Id::new(THEME_COLOUR_CODE))
            .expect("the picker's code field");
        ui.tap_key(Key::Named(key::Named::End));
        for _ in 0..7 {
            ui.tap_key(Key::Named(key::Named::Backspace));
        }
        ui.typewrite("#123456");
    });
    assert_eq!(app.theme.chat, crate::style::hex(0x123456));
    assert!(shows(&app, "#123456"));

    // Clicking outside the popover dismisses the picker without closing Settings.
    interact(&mut app, |ui| {
        click_at(ui, Point::new(10.0, 10.0));
    });
    assert_eq!(picker(&app), None);
    assert!(app.win.settings.is_open());
    assert_eq!(app.theme.chat, crate::style::hex(0x123456));

    // Reopening and pressing Escape closes the picker first, then Settings.
    interact(&mut app, |ui| {
        let swatch = swatch_at(ui, "chat");
        click_at(ui, swatch);
    });
    assert_eq!(picker(&app), Some(chat));
    let _ = app.update(Message::Key(
        Key::Named(key::Named::Escape),
        Default::default(),
    ));
    assert_eq!(picker(&app), None);
    assert!(app.win.settings.is_open(), "Settings stays open");
    let _ = app.update(Message::Key(
        Key::Named(key::Named::Escape),
        Default::default(),
    ));
    assert!(!app.win.settings.is_open());
}

#[test]
fn the_theme_colour_picker_preserves_alpha_on_translucent_tokens() {
    let (mut app, _folder) = preview();
    open(&mut app, Section::Appearance);
    interact(&mut app, |ui| {
        ui.click("New theme").expect("New theme");
    });
    let accent_soft = yapfast_core::theme::Theme::NAMES
        .iter()
        .position(|name| *name == "accent_soft")
        .expect("accent_soft");
    let _ = app.update(appearance_event(ThemeEvent::Picker(Some(accent_soft))));
    let _ = app.update(appearance_event(ThemeEvent::Hsv([120.0, 1.0, 1.0])));
    let hex = &app
        .win
        .settings
        .get()
        .and_then(|settings| settings.editor.as_ref())
        .expect("editor")
        .fields[accent_soft]
        .1;
    assert_eq!(hex, "#00ff0026", "alpha kept on translucent token");
    assert_eq!(
        app.theme.accent_soft,
        iced::Color::from_rgba8(0, 255, 0, f32::from(0x26u8) / 255.0)
    );
    // Shorthand #rgb and #rrggbbaa work too, and picking preserves the typed alpha.
    let _ = app.update(appearance_event(ThemeEvent::Colour(
        accent_soft,
        "#345".into(),
    )));
    assert_eq!(app.theme.accent_soft, crate::style::hex(0x334455));
    let _ = app.update(appearance_event(ThemeEvent::Colour(
        accent_soft,
        "#11223380".into(),
    )));
    let _ = app.update(appearance_event(ThemeEvent::Hsv([0.0, 1.0, 1.0])));
    let hex = &app
        .win
        .settings
        .get()
        .and_then(|settings| settings.editor.as_ref())
        .expect("editor")
        .fields[accent_soft]
        .1;
    assert_eq!(hex, "#ff000080");
    // Switching the Start From template closes any open picker.
    let _ = app.update(appearance_event(ThemeEvent::Start(0)));
    assert_eq!(
        app.win
            .settings
            .get()
            .and_then(|settings| settings.editor.as_ref())
            .and_then(|editor| editor.picker),
        None
    );
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
    open(&mut app, Section::LanguageTime);
    interact(&mut app, |ui| {
        ui.click("Auto").expect("the time format dropdown");
    });
    interact(&mut app, |ui| {
        ui.click("12-hour").expect("the clock");
    });
    open(&mut app, Section::Messages);
    interact(&mut app, |ui| {
        ui.click("Show seconds").expect("the switch");
    });
    interact(&mut app, |ui| {
        ui.click("Collapse long messages").expect("the switch");
    });
    open(&mut app, Section::Accessibility);
    let _ = app.update(crate::settings::pref_message(
        crate::settings::Pref::TextPx(18),
    ));
    let _ = app.update(crate::settings::pref_message(
        crate::settings::Pref::KeepTextPx,
    ));
    assert!(app.layout.prefs.twelve_hour_clock() && app.layout.prefs.clock_seconds);
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
    let mut again = start(&folder, true);
    let prefs = &again.layout.prefs;
    assert!(prefs.twelve_hour_clock() && prefs.clock_seconds);
    assert!(again.layout.collapse_long && again.layout.developer_mode);
    assert_eq!(prefs.text_px(), 18);
    assert_eq!(prefs.flash_taskbar, Some(false));
    let layout = stored(&folder);
    assert_eq!(layout["time_format"], "12-hour");
    assert_eq!(layout["flash_taskbar"], false);

    open(&mut again, Section::LanguageTime);
    interact(&mut again, |ui| {
        ui.click("12-hour").expect("the saved time format");
    });
    interact(&mut again, |ui| {
        ui.click("24-hour").expect("the clock");
    });
    assert!(!again.layout.prefs.twelve_hour_clock());
    assert_eq!(stored(&folder)["time_format"], "24-hour");
    interact(&mut again, |ui| {
        ui.click("24-hour").expect("the time format dropdown");
    });
    interact(&mut again, |ui| {
        ui.click("Auto").expect("the system time format");
    });
    assert_eq!(
        again.layout.prefs.time_format(),
        crate::time_format::TimeFormat::Auto
    );
    assert_eq!(stored(&folder)["time_format"], "auto");
}

/// Appearance's Show activity cards says what the member list's eye left:
/// one click after the eye hid them shows them again.
#[test]
fn show_activity_cards_follows_the_member_lists_eye() {
    let (mut app, folder) = preview();
    let _ = app.update(Message::Members(crate::members::Event::HideActivity));
    assert!(app.client.state().activity_cards_hidden, "the eye hid them");
    open(&mut app, Section::Appearance);
    interact(&mut app, |ui| {
        ui.click("Show activity cards").expect("the switch");
    });
    assert!(!app.client.state().activity_cards_hidden, "shown again");
    assert_eq!(stored(&folder)["activity_cards_hidden"], false);
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
    assert!(shows(&app, "No Keybind Set"));
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
    let _ = app.update(keybind(KeybindEvent::Enabled(0, false)));
    assert!(app.keybinds.watched.is_empty());
    assert_eq!(stored(&folder)["keybinds"][0]["enabled"], false);
    assert!(
        app.layout.keybinds[0].keys.is_some(),
        "disabling preserves the keys"
    );
    let _ = app.update(keybind(KeybindEvent::Enabled(0, true)));
    assert_eq!(app.keybinds.watched.len(), 1);
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

/// Accessibility shows Discord's sections, the text size at the one older
/// versions chose, and the animations Messages had; the text size follows
/// the slider's marks, and the rows take it.
#[test]
fn accessibility_keeps_the_text_size_and_the_animations() {
    let folder = tempfile::tempdir_in(local()).expect("test folder");
    std::fs::write(
        folder.path().join("layout.json"),
        br#"{"text_size":2,"animate":"never"}"#,
    )
    .expect("an older layout");
    let mut app = start(&folder, true);
    let _ = app.sync();
    open(&mut app, Section::Accessibility);
    for words in [
        "TEXT READABILITY",
        "Text size in chat",
        "18px",
        "REDUCED MOTION",
        "Enable Reduced Motion",
        "Sync with computer setting",
        "PLAY GIFS, EMOJI AND STICKERS",
    ] {
        assert!(shows(&app, words), "{words}");
    }
    open(&mut app, Section::Messages);
    assert!(!shows(&app, "PLAY GIFS, EMOJI AND STICKERS"), "moved");
    open(&mut app, Section::Accessibility);
    interact(&mut app, |ui| {
        ui.click("Always").expect("the animations");
    });
    assert_eq!(app.layout.prefs.animate, crate::layout::Animate::Always);
    // A size between two marks goes to the nearest, and is kept as the
    // slider's let go.
    let _ = app.update(crate::settings::pref_message(
        crate::settings::Pref::TextPx(21),
    ));
    assert_eq!(app.layout.prefs.text_px(), 20);
    assert!(
        stored(&folder).get("chat_text_size").is_none(),
        "not while dragged"
    );
    let _ = app.update(crate::settings::pref_message(
        crate::settings::Pref::KeepTextPx,
    ));
    assert_eq!(stored(&folder)["chat_text_size"], 20);
    assert_eq!(crate::message::text_size(app.ctx()), 20.0);
}

#[test]
fn accessibility_keeps_scroll_speed() {
    let (mut app, folder) = preview();
    assert_eq!(app.layout.prefs.scroll_speed(), 100);
    open(&mut app, Section::Accessibility);
    interact(&mut app, |ui| {
        ui.click("300%").expect("the scroll speed preset");
    });
    assert_eq!(app.layout.prefs.scroll_speed(), 300);
    assert_eq!(stored(&folder)["scroll_speed"], 300);
    assert_eq!(start(&folder, true).layout.prefs.scroll_speed(), 300);
}

/// Enable Reduced Motion stills GIFs, emoji and stickers as Discord's
/// client does, until they're chosen again; off, they play as chosen.
/// Synced to the desktop, the desktop's wish decides, and unsyncing keeps
/// what it said.
#[test]
fn reduced_motion_stills_animations_and_can_follow_the_desktop() {
    use crate::layout::Animate;
    let (mut app, folder) = preview();
    app.layout.prefs.animate = Animate::Always;
    open(&mut app, Section::Accessibility);
    assert!(!app.layout.reduced_motion(), "off, as today");
    assert!(!app.layout.sync_reduced_motion, "not synced, as today");
    interact(&mut app, |ui| {
        ui.click("Enable Reduced Motion").expect("the switch");
    });
    assert!(app.layout.reduced_motion());
    assert_eq!(app.layout.animate(), Animate::Never, "stilled");
    assert!(shows(
        &app,
        "This setting has been automatically disabled because of your Reduced Motion setting."
    ));
    assert_eq!(stored(&folder)["reduce_motion"], true);
    // Chosen again, they play with reduced motion on.
    interact(&mut app, |ui| {
        ui.click("Always").expect("the animations");
    });
    assert_eq!(app.layout.animate(), Animate::Always);
    interact(&mut app, |ui| {
        ui.click("Enable Reduced Motion").expect("the switch");
    });
    interact(&mut app, |ui| {
        ui.click("Enable Reduced Motion").expect("the switch");
    });
    assert_eq!(app.layout.animate(), Animate::Never, "stilled again");
    interact(&mut app, |ui| {
        ui.click("Enable Reduced Motion").expect("the switch");
    });
    assert_eq!(app.layout.animate(), Animate::Always, "as chosen");

    // Synced, the desktop's wish counts.
    interact(&mut app, |ui| {
        ui.click("Sync with computer setting").expect("the switch");
    });
    assert!(app.layout.sync_reduced_motion);
    let _ = app.update(Message::Shell(crate::shell::Event::SystemMotion(Some(
        true,
    ))));
    assert!(app.layout.reduced_motion());
    assert_eq!(app.layout.animate(), Animate::Never);
    // Unsynced, the choice is what the desktop last said.
    interact(&mut app, |ui| {
        ui.click("Sync with computer setting").expect("the switch");
    });
    assert!(!app.layout.sync_reduced_motion);
    assert!(app.layout.reduced_motion() && app.layout.reduce_motion);
    // Choosing it stops following the desktop.
    interact(&mut app, |ui| {
        ui.click("Sync with computer setting").expect("the switch");
    });
    interact(&mut app, |ui| {
        ui.click("Enable Reduced Motion").expect("the switch");
    });
    assert!(!app.layout.sync_reduced_motion && !app.layout.reduced_motion());
    let layout = stored(&folder);
    assert_eq!(layout["sync_reduced_motion"], false);
    assert_eq!(layout["reduce_motion"], false);
    assert_eq!(layout["animate_stilled"], false);
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

/// Appearance's Font lists no file until one is chosen. A file that is no
/// font is left out, with the reason under the buttons, and isn't kept for
/// the next start. (A real font isn't loaded here: the font database is the
/// process's, and the other tests measure their text in DM Sans.)
#[test]
fn a_file_that_is_no_font_is_left_out_of_appearances_font() {
    use crate::settings::{FontEvent, font_message};
    let (mut app, folder) = preview();
    open(&mut app, Section::Appearance);
    assert!(shows(&app, "FONT") && shows(&app, "Choose font files"));
    assert!(!shows(&app, "Reset to default"), "nothing to reset yet");
    let words = folder.path().join("notes.ttf");
    std::fs::write(&words, b"only words, no font tables in here").expect("a file");
    let _ = app.update(font_message(FontEvent::Chosen(vec![
        words.to_str().expect("a path").to_owned(),
    ])));
    assert!(
        shows(
            &app,
            "notes.ttf: Not a font file. Yapfast reads TTF, OTF, TTC and WOFF."
        ),
        "the note names the file and says why"
    );
    assert!(!shows(&app, "Reset to default"), "nothing is loaded");
    assert_eq!(stored(&folder)["ui_font_files"], serde_json::json!([]));
    assert!(app.layout.prefs.ui_font_files.is_empty());
    // Going back to DM Sans with nothing loaded is no change either.
    let _ = app.update(font_message(FontEvent::Reset));
    assert!(!shows(
        &app,
        "notes.ttf: Not a font file. Yapfast reads TTF, OTF, TTC and WOFF."
    ));
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
fn the_detected_games_menu_lists_each_game_and_choosing_one_never_shows_it() {
    use crate::menus::MenuAct;
    use yapfast_core::yapfast_discord::game_detection::custom_games::set_detected;
    let (mut app, _folder) = preview();
    // Nothing found: one row, which can't be chosen.
    set_detected(Vec::new());
    let _ = crate::settings::open_detected_games_menu(&mut app, iced::Point::ORIGIN);
    let menu = app.win.menu.take().expect("a menu opened");
    assert_eq!(menu.rows.len(), 1);
    assert!(menu.rows[0].disabled, "there is nothing to choose");
    // Two found: a row for each, by the name that shows after Playing.
    set_detected(vec!["Roblox".into(), "Zelda".into()]);
    let _ = crate::settings::open_detected_games_menu(&mut app, iced::Point::ORIGIN);
    let menu = app.win.menu.take().expect("a menu opened");
    let rows: Vec<_> = menu
        .rows
        .iter()
        .map(|row| (row.label.to_string(), row.key.clone()))
        .collect();
    assert_eq!(
        rows,
        [
            ("Roblox".to_owned(), "Roblox".to_owned()),
            ("Zelda".to_owned(), "Zelda".to_owned())
        ]
    );
    assert!(
        menu.rows
            .iter()
            .all(|row| row.act == MenuAct::BlockGame && !row.disabled)
    );
    set_detected(Vec::new());
    // Choosing one lists it as never shown, once.
    crate::settings::block_game(&mut app, "Roblox");
    crate::settings::block_game(&mut app, "roblox");
    assert_eq!(app.layout.prefs.blocked_games.len(), 1);
    assert_eq!(app.layout.prefs.blocked_games[0].name, "Roblox");
    assert!(app.layout.prefs.blocked_games[0].on, "chosen, it is hidden");
    yapfast_core::discord::set_blocked_games(&[]);
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
