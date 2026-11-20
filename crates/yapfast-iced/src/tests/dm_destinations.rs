//! The DM sidebar's Nitro, Shop and Quests screens: native in-app screens
//! opened from the DM navigation rows alongside Friends and Requests.

use iced::{Settings, widget::Id};
use iced_test::Simulator;
use yapfast_core::application::{Action, DmScreen, Navigation};

use super::{change, interact, preview, shows};
use crate::{
    app::{App, Message},
    browser,
    sidebar::{
        self,
        destinations::{NITRO_URL, QUESTS_URL, SHOP_URL},
    },
};

const SCREENS: [(&str, DmScreen, &str, &str); 3] = [
    ("Nitro", DmScreen::Nitro, "PERKS IN YAPFAST", NITRO_URL),
    (
        "Shop",
        DmScreen::Shop,
        "COSMETICS SHOWN IN YAPFAST",
        SHOP_URL,
    ),
    (
        "Quests",
        DmScreen::Quests,
        "YOUR GAME & QUEST ACTIVITY",
        QUESTS_URL,
    ),
];

fn dm_home(app: &mut App) {
    let _ = app.update(Message::Rail(crate::rail::Event::Home));
    change(app, |state| {
        state.account = Some("1".into());
        state.preview = false;
        state.discord_linked = true;
    });
    browser::take_opened();
}

#[test]
fn dm_screens_open_natively_in_yapfast_from_the_sidebar() {
    let (mut app, _folder) = preview();
    dm_home(&mut app);
    for (label, screen, heading, _) in SCREENS {
        interact(&mut app, |ui| {
            ui.click(label).expect("the DM sidebar row");
        });
        assert_eq!(app.client.state().dm_screen, Some(screen));
        assert_eq!(app.client.state().selected, None);
        assert!(shows(&app, heading), "{label} screen shows {heading}");
        assert!(
            browser::take_opened().is_empty(),
            "clicking {label} opens in Yapfast, not the browser"
        );
    }
}

#[test]
fn dm_screens_participate_in_back_and_forward_navigation_and_select_rooms() {
    let (mut app, _folder) = preview();
    dm_home(&mut app);
    interact(&mut app, |ui| {
        ui.click("Friends").expect("Friends row");
    });
    assert_eq!(app.client.state().dm_screen, Some(DmScreen::Friends));

    for (label, screen, heading, _) in SCREENS {
        interact(&mut app, |ui| {
            ui.click(label).expect("DM screen row");
        });
        assert_eq!(app.client.state().dm_screen, Some(screen));
        assert!(shows(&app, heading));
    }

    app.client.dispatch(Action::NavigateBack);
    assert_eq!(app.client.state().dm_screen, Some(DmScreen::Shop));
    app.client.dispatch(Action::NavigateForward);
    assert_eq!(app.client.state().dm_screen, Some(DmScreen::Quests));

    interact(&mut app, |ui| {
        ui.click("Sam").expect("Sam's DM");
    });
    assert_eq!(app.client.state().navigation, Navigation::DirectMessages);
    assert_eq!(app.client.state().dm_screen, None);
    assert!(app.client.state().selected.is_some());
}

#[test]
fn dm_screens_offer_native_controls_and_optional_web_handoffs() {
    let (mut app, _folder) = preview();
    dm_home(&mut app);

    interact(&mut app, |ui| {
        ui.click("Shop").expect("Shop row");
    });
    assert!(app.layout.prefs.cosmetics().decorations);
    interact(&mut app, |ui| {
        ui.click("Avatar decorations").expect("cosmetic switch");
    });
    assert!(!app.layout.prefs.cosmetics().decorations);

    for (label, _, _, url) in SCREENS {
        interact(&mut app, |ui| {
            ui.click(label).expect("DM screen row");
        });
        let button_label = match label {
            "Nitro" => "Open Nitro on discord.com",
            "Shop" => "Open Shop on discord.com",
            _ => "Open Quests on discord.com",
        };
        interact(&mut app, |ui| {
            ui.click(button_label).expect("optional web handoff");
        });
        assert_eq!(browser::take_opened(), [url]);
    }
}

#[test]
fn dm_screens_require_a_linked_account_and_fit_every_sidebar_width() {
    let (mut app, _folder) = preview();
    let _ = app.update(Message::Rail(crate::rail::Event::Home));
    for (label, ..) in SCREENS {
        assert!(!shows(&app, label), "not in the preview");
    }
    dm_home(&mut app);
    for width in [180.0, sidebar::WIDTH, 380.0] {
        app.layout.sidebar_width = Some(width);
        let mut ui = Simulator::with_size(Settings::default(), app.shell.frame, app.view());
        let _ = ui.find(Id::new(sidebar::LIST)).expect("the sidebar list");
        for (label, ..) in SCREENS {
            let found = ui.find(label).expect("the sidebar row label");
            assert!(found.visible_bounds().is_some());
        }
    }
    change(&mut app, |state| state.discord_linked = false);
    for (label, ..) in SCREENS {
        assert!(!shows(&app, label), "not when unlinked");
    }
}
