//! The expression picker: the custom emoji's pictures asked for and let go
//! of as the emoji grid scrolls and closes, and the keys, the rail's names,
//! the tooltips and the GIFs' Trending as a person uses them.

use std::sync::Arc;

use iced::{
    Point, Rectangle, Size,
    keyboard::{self, key},
    mouse,
    widget::Id,
};
use iced_test::Simulator;
use yapfast_core::{
    application::{Action, GifView},
    chat::{Command, CustomStatus, DiscordCommand, Gif, Picture},
    yapfast_discord::ready,
};

use super::{change, hover_draws, interact, preview, shows};
use crate::{
    app::{App, Message},
    ctx::{Focus, Request},
    picker::{Event, Tab, Target, kit::FIELD},
};

/// What the open picker's foot says at rest: the first emoji's name, or its
/// hint when there is none.
fn resting_foot(app: &App) -> String {
    app.win
        .picker
        .get()
        .and_then(|picker| picker.resting_name())
        .unwrap_or_else(|| "Pick an emoji for your message.".into())
}

/// The test server's #general open, and the composer's picker on its GIFs.
fn gifs_open(app: &mut App) {
    change(app, |state| {
        state.selected = Some("discord:10:11".into());
        state.room_ready = true;
    });
    let _ = app.update(Message::OpenPicker(Target::Composer, Tab::Gifs));
    assert_eq!(
        app.win.picker.get().map(|picker| picker.tab),
        Some(Tab::Gifs)
    );
}

/// The GIFs showing `view`, as the core says it.
fn gifs_view(app: &mut App, view: GifView, results: Vec<Gif>) {
    change(app, |state| {
        state.gifs.view = view;
        state.gifs.loading = false;
        state.gifs.results = results;
    });
}

/// Whether the picker asked for the keys to go back to its search.
fn keys_to_search(app: &App) -> bool {
    app.requests
        .iter()
        .any(|request| matches!(request, Request::Focus(Focus::Id(id)) if *id == FIELD))
}

/// The window on the test server holding `count` custom emoji, and their
/// pictures' addresses in the picker's order.
fn server_with_emoji(count: usize) -> (App, tempfile::TempDir, Vec<String>) {
    let (app, folder) = super::server::discord_server(0, |server, _| {
        server.emojis = Arc::new(
            (0..count)
                .map(|index| ready::Emoji {
                    id: format!("{}", 5000 + index),
                    name: format!("e{index:03}"),
                    animated: false,
                    available: true,
                })
                .collect(),
        );
    });
    let urls = app
        .client
        .app
        .emoji_sets()
        .into_iter()
        .flat_map(|set| set.emoji)
        .map(|emoji| emoji.picture)
        .collect();
    (app, folder, urls)
}

/// The pictures the core asked its backend for since `sent` was emptied,
/// each batch in its order.
fn asked(app: &App) -> Vec<Vec<String>> {
    app.client
        .sent
        .iter()
        .filter_map(|command| match command {
            Command::Discord(DiscordCommand::Pictures(urls)) => Some(urls.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn the_emoji_picker_asks_for_what_shows_first_and_lets_go_as_it_scrolls_and_closes() {
    let (mut app, _folder, urls) = server_with_emoji(400);
    assert_eq!(urls.len(), 400);
    // The account's status wears a custom emoji, which its card shows.
    let status = yapfast_core::discord::emoji_picture("9999").expect("a picture");
    change(&mut app, |state| {
        state.online = true;
        state.custom_status = Some(CustomStatus {
            emoji_id: Some("9999".into()),
            text: "Out".into(),
            ..Default::default()
        });
        state
            .pictures
            .insert(status.clone(), Arc::new(Picture::default()));
    });
    app.client.sent.clear();
    let _ = app.update(Message::OpenPicker(Target::Composer, Tab::Emoji));
    let first = asked(&app);
    let first = first.first().expect("the grid's pictures asked for");
    assert_eq!(first[0], urls[0], "those in sight first");
    assert!(first.contains(&urls[60]), "and those a scroll away");
    assert!(!first.contains(&urls[399]), "not those far down");
    // They come.
    change(&mut app, |state| {
        for url in [&urls[0], &urls[1], &urls[399]] {
            state
                .pictures
                .insert(url.clone(), Arc::new(Picture::default()));
        }
    });
    let held = |app: &App, url: &String| app.client.state().pictures.contains_key(url);
    // Far down in Unicode's categories, the server's go but the one its
    // rail shows, and the status's stays.
    let _ = app.update(Message::Picker(Event::Jump(4)));
    assert!(!held(&app, &urls[1]) && !held(&app, &urls[399]));
    assert!(held(&app, &urls[0]), "the rail's");
    assert!(held(&app, &status), "the status's");
    // Back at the server's, the missing are asked for again.
    app.client.sent.clear();
    let _ = app.update(Message::Picker(Event::Jump(0)));
    assert_eq!(
        asked(&app).first().map(|urls| urls[0].clone()),
        Some(urls[1].clone())
    );
    // One picked for the custom status being written shows in its dialog.
    let picked = yapfast_core::discord::emoji_picture("5002").expect("a picture");
    change(&mut app, |state| {
        state.status_editor = Some(yapfast_core::application::StatusEditor {
            saving: false,
            error: None,
        });
        state
            .pictures
            .insert(picked.clone(), Arc::new(Picture::default()));
    });
    app.status.draft = Some(crate::status::Draft {
        emoji: Some("e002:5002".into()),
        ..Default::default()
    });
    // Closed, every custom emoji goes but what the window shows elsewhere.
    let _ = crate::picker::close_picker(&mut app);
    assert!(!held(&app, &urls[0]));
    assert!(held(&app, &status), "the status's");
    assert!(held(&app, &picked), "the one picked for the status");
}

#[test]
fn ctrl_g_closes_the_gif_picker_its_search_passing_it_on() {
    let (mut app, _folder) = super::server::discord_server(0, |_, _| {});
    gifs_open(&mut app);
    let g = keyboard::Key::Character("g".into());
    let mut passed = Vec::new();
    interact(&mut app, |ui| {
        ui.click(Id::new(FIELD)).expect("the GIFs' search");
        passed = ui.simulate([iced::Event::Keyboard(keyboard::Event::KeyPressed {
            key: g.clone(),
            modified_key: g.clone(),
            physical_key: key::Physical::Code(key::Code::KeyG),
            location: keyboard::Location::Standard,
            modifiers: keyboard::Modifiers::CTRL,
            repeat: false,
            text: None,
        })]);
    });
    assert_eq!(
        passed,
        [iced::event::Status::Ignored],
        "the field passes it on"
    );
    let on_g = key::Physical::Code(key::Code::KeyG);
    let _ = app.update(Message::Pressed(g, keyboard::Modifiers::CTRL, on_g));
    assert!(app.win.picker.get().is_none(), "Ctrl+G closed it");
    // A Russian layout's key in G's place closes it as it opened it.
    gifs_open(&mut app);
    let pe = keyboard::Key::Character("п".into());
    let _ = app.update(Message::Pressed(pe, keyboard::Modifiers::CTRL, on_g));
    assert!(
        app.win.picker.get().is_none(),
        "the key in G's place closed it"
    );
}

#[test]
fn a_click_in_the_picker_leaves_the_keys_with_its_search() {
    let (mut app, _folder) = preview();
    let _ = app.update(Message::OpenPicker(Target::Composer, Tab::Emoji));
    // A press on its foot, which takes no click itself. At rest it names
    // the first emoji, as Discord's does.
    let rest = resting_foot(&app);
    let pressed: Vec<Message> = {
        let mut ui = Simulator::with_size(Default::default(), app.shell.frame, app.view());
        ui.click(rest.as_str()).expect("the foot");
        ui.into_messages().collect()
    };
    assert!(
        pressed
            .iter()
            .any(|message| matches!(message, Message::Picker(Event::Pressed))),
        "the picker hears the press"
    );
    assert!(
        !pressed
            .iter()
            .any(|message| matches!(message, Message::Picker(Event::Close))),
        "and nothing under it does"
    );
    for event in [Event::Pressed, Event::Jump(1), Event::Star(1)] {
        app.requests.clear();
        let _ = crate::picker::picker_event(&mut app, event.clone());
        assert!(keys_to_search(&app), "{event:?} gives the keys back");
    }
    let _ = app.update(Message::Nothing);
    assert!(app.win.picker.is_open(), "and it stays open");
}

#[test]
fn trending_gifs_keep_their_search_out_of_sight_so_typing_searches() {
    let (mut app, _folder) = super::server::discord_server(0, |_, _| {});
    gifs_open(&mut app);
    app.client.dispatch(Action::ShowGifs(GifView::Trending));
    gifs_view(&mut app, GifView::Trending, Vec::new());
    assert!(shows(&app, "Trending GIFs"));
    let mut ui = Simulator::with_size(Default::default(), app.shell.frame, app.view());
    let field = ui
        .find(Id::new(FIELD))
        .expect("the search, there to type into")
        .bounds();
    let title = ui.find("Trending GIFs").expect("the title").bounds();
    assert!(
        field.contains(title.center()),
        "under the title: {field:?} {title:?}"
    );
    drop(ui);
    // The search leaves Trending, whose header goes: the field keeps the keys.
    app.requests.clear();
    let _ = crate::picker::picker_event(&mut app, Event::SearchDue(99));
    assert!(!keys_to_search(&app), "a search overtaken asks nothing");
    let _ = crate::picker::picker_event(&mut app, Event::Field(crate::field::Event::Send));
    assert!(keys_to_search(&app), "the search keeps the keys");
}

#[test]
fn the_rail_names_the_section_under_the_pointer_at_the_foot() {
    let (mut app, _folder, _) = server_with_emoji(3);
    gifs_open(&mut app);
    let _ = app.update(Message::Picker(Event::Tab(Tab::Emoji)));
    let hint = resting_foot(&app);
    let hint = hint.as_str();
    assert!(shows(&app, hint));
    // The pointer down the rail, under the search, comes onto each
    // section's button in turn.
    let hovered: Vec<Message> = {
        let mut ui = Simulator::with_size(Default::default(), app.shell.frame, app.view());
        let field = ui.find(Id::new(FIELD)).expect("the search").bounds();
        for down in (10..200).step_by(40) {
            let at = Point::new(field.x + 14.0, field.y + field.height + down as f32);
            ui.point_at(at);
            ui.simulate([iced::Event::Mouse(mouse::Event::CursorMoved {
                position: at,
            })]);
        }
        ui.into_messages().collect()
    };
    let mut named = Vec::new();
    for message in hovered {
        let on = matches!(message, Message::Picker(Event::RailHover(_)));
        let _ = app.update(message);
        if on {
            assert!(!shows(&app, hint), "a section named in the hint's place");
            named.extend(
                ["Smileys & emotion", "People & body"]
                    .into_iter()
                    .filter(|name| shows(&app, name)),
            );
        }
    }
    assert!(
        named.contains(&"Smileys & emotion") && named.contains(&"People & body"),
        "{named:?}"
    );
    let _ = app.update(Message::Picker(Event::RailLeft(4)));
    assert!(shows(&app, hint), "and the hint again once it leaves");
}

#[test]
fn emoji_skin_tone_is_kept_and_applied_to_the_search_pick() {
    let (mut app, folder) = preview();
    let _ = app.update(Message::OpenPicker(Target::Composer, Tab::Emoji));
    interact(&mut app, |ui| {
        ui.click("Skin tone").expect("skin tone chooser");
    });
    for label in [
        "Default",
        "Light",
        "Medium light",
        "Medium",
        "Medium dark",
        "Dark",
    ] {
        assert!(shows(&app, label), "{label}");
    }
    interact(&mut app, |ui| {
        ui.click("Medium").expect("medium skin tone");
    });
    assert_eq!(
        crate::layout::Layout::read(folder.path()).emoji_skin_tone,
        3
    );
    interact(&mut app, |ui| {
        ui.click(Id::new(FIELD)).expect("the picker's search");
        ui.typewrite("clapping hands");
    });
    interact(&mut app, |ui| {
        ui.click(Id::new(FIELD)).expect("the picker's search");
        ui.tap_key(keyboard::Key::Named(key::Named::Enter));
    });
    assert!(app.composer.draft.text().contains("👏🏽"));
}

#[test]
fn add_emoji_opens_the_servers_upload_page_for_allowed_accounts() {
    use yapfast_core::{
        chat::{Event as CoreEvent, GuildSettings, ServerSettingsPage, SettingsSection},
        yapfast_discord::ready::permission,
    };

    let (mut app, _folder) = super::server::discord_server(0, |_, _| {});
    let _ = app.update(Message::OpenPicker(Target::Composer, Tab::Emoji));
    assert!(!shows(&app, "Add Emoji"));
    let _ = app.update(Message::Picker(Event::EmojiServer("discord:10".into())));
    assert!(app.client.state().server_settings.is_none());

    let (mut app, _folder) = super::server::discord_server(
        permission::MANAGE_GUILD | permission::CREATE_GUILD_EXPRESSIONS,
        |_, _| {},
    );
    let _ = app.update(Message::OpenPicker(Target::Composer, Tab::Emoji));
    interact(&mut app, |ui| {
        ui.click("Add Emoji").expect("add a custom emoji");
    });
    assert!(!app.win.picker.is_open());
    let page = app
        .client
        .state()
        .server_settings
        .as_ref()
        .expect("Emoji settings");
    assert_eq!(page.server, "discord:10");
    assert_eq!(page.section, SettingsSection::Emoji);
    app.client.apply(CoreEvent::ServerSettingsLoaded {
        server: "discord:10".into(),
        section: SettingsSection::Emoji,
        result: Ok(ServerSettingsPage::Emoji {
            emojis: Vec::new(),
            settings: GuildSettings::default(),
        }),
    });
    let _ = app.sync();
    assert!(shows(&app, "Upload Emoji"));
}

#[test]
fn the_emoji_picker_shows_the_open_servers_ranked_emoji() {
    let (mut app, _folder, _) = server_with_emoji(3);
    change(&mut app, |state| {
        state.selected = Some("discord:10:11".into());
        state.room_ready = true;
    });
    app.client.sent.clear();
    let _ = app.update(Message::OpenPicker(Target::Composer, Tab::Emoji));
    assert!(app.client.sent.iter().any(|command| matches!(
        command,
        Command::Discord(DiscordCommand::PopularEmoji(server)) if server == "discord:10"
    )));
    assert!(!shows(&app, "Popular in Test"));
    app.client.apply(yapfast_core::chat::Event::PopularEmoji {
        server: "discord:10".into(),
        result: Ok(vec!["8888".into(), "5002".into(), "5001".into()]),
    });
    let _ = app.sync();
    assert!(shows(&app, "Popular in Test"));
    assert_eq!(resting_foot(&app), ":e002:");
    let _ = app.update(Message::Picker(Event::Pick(1)));
    assert!(app.composer.draft.text().contains(":e002:"));

    app.client.sent.clear();
    let _ = app.update(Message::OpenPicker(Target::Composer, Tab::Emoji));
    assert!(
        !app.client
            .sent
            .iter()
            .any(|command| matches!(command, Command::Discord(DiscordCommand::PopularEmoji(_))))
    );
}

#[test]
fn the_gifs_back_and_star_say_what_they_do_under_the_pointer() {
    let (mut app, _folder) = super::server::discord_server(0, |_, _| {});
    gifs_open(&mut app);
    gifs_view(&mut app, GifView::Trending, Vec::new());
    {
        let mut ui = Simulator::with_size(Default::default(), app.shell.frame, app.view());
        let title = ui
            .find("Trending GIFs")
            .expect("the title")
            .visible_bounds()
            .expect("in view");
        let back = Point::new(title.x - 8.0 - 17.0, title.center_y());
        let above = Rectangle::new(
            Point::new(back.x - 12.0, back.y - 17.0 - 34.0),
            Size::new(24.0, 24.0),
        );
        assert!(hover_draws(&mut ui, back, above), "Back's tooltip");
    }
    let gif = Gif {
        url: "https://klipy.com/gifs/1".into(),
        src: "https://static.klipy.com/1.webp".into(),
        width: 200,
        height: 200,
        ..Default::default()
    };
    gifs_view(&mut app, GifView::Search, vec![gif]);
    let _ = app.update(Message::Picker(Event::Hover(0)));
    let mut ui = Simulator::with_size(Default::default(), app.shell.frame, app.view());
    let field = ui.find(Id::new(FIELD)).expect("the search").bounds();
    // The first GIF's corner, 12 px into the grid under the header.
    let (left, grid) = (field.x - 8.0 - 34.0 - 16.0, field.center_y() + 17.0 + 12.0);
    let star = Point::new(left + 12.0 + 231.0 - 4.0 - 16.0, grid + 12.0 + 4.0 + 16.0);
    let above = Rectangle::new(
        Point::new(star.x - 10.0, star.y - 16.0 - 30.0),
        Size::new(20.0, 20.0),
    );
    assert!(hover_draws(&mut ui, star, above), "the star's tooltip");
}

#[test]
fn the_emoji_pickers_animated_emoji_play_in_their_cells_until_it_closes() {
    let (mut app, _folder) = super::server::discord_server(0, |server, _| {
        server.emojis = Arc::new(vec![
            ready::Emoji {
                id: "6001".into(),
                name: "party".into(),
                animated: true,
                available: true,
            },
            ready::Emoji {
                id: "6002".into(),
                name: "calm".into(),
                animated: false,
                available: true,
            },
        ]);
    });
    app.layout.prefs.animate = crate::layout::Animate::Always;
    let party = super::animated_emoji();
    let calm = Arc::new(Picture {
        width: 64,
        height: 64,
        rgba: yapfast_core::chat::Pixels::new(64, 64, vec![200; 64 * 64 * 4]),
        ..Default::default()
    });
    let _ = app.update(Message::OpenPicker(Target::Composer, Tab::Emoji));
    let emoji: Vec<_> = app
        .client
        .app
        .emoji_sets()
        .into_iter()
        .flat_map(|set| set.emoji)
        .collect();
    assert_eq!(emoji.len(), 2);
    change(&mut app, |state| {
        for emoji in emoji {
            let picture = if emoji.animated { &party } else { &calm };
            state.pictures.insert(emoji.picture, picture.clone());
        }
    });
    let playing = |app: &App| {
        super::redraw(app);
        app.win
            .picker
            .get()
            .and_then(|picker| picker.emoji_tab())
            .map(|tab| tab.players().keys())
            .unwrap_or_default()
    };
    let keys = playing(&app);
    assert_eq!(keys.len(), 1, "the animated one plays, the still doesn't");
    assert_eq!(keys[0].0, Arc::as_ptr(&party) as usize);
    // Held still by Animate's Never, it has no player.
    app.layout.prefs.animate = crate::layout::Animate::Never;
    assert!(playing(&app).is_empty());
    app.layout.prefs.animate = crate::layout::Animate::Always;
    assert_eq!(playing(&app).len(), 1);
    // Closed, nothing holds its picture but this test: its player went
    // with the close, before the heap is given back and with no view
    // since, and the core let go of it.
    let _ = crate::picker::close_picker(&mut app);
    assert_eq!(Arc::strong_count(&party), 1, "its player went");
}
