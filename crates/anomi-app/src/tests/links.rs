//! Links on the preview's real core: a web page opens in the browser, a
//! masked link asks first naming where it really goes, whether clicked in a
//! message, a topic or the message's menu, and anything but a web page
//! stays shut; a badge's link closes its popover.

use yapfast_core::application::Action;

use anomi_vm::Intent;
use anomi_vm::chat::menu::Intent as MessageMenu;
use anomi_vm::chat::message::Intent as MessageIntent;
use anomi_vm::menu::Spot;
use anomi_vm::places::leaving_link;
use anomi_vm::shell::Layer;
use anomi_vm::text::Link;

use super::start;
use crate::app::App;

fn handle(app: &mut App, intent: impl Into<Intent>) {
    drop(crate::intent::handle(app, intent.into()));
}

/// The preview's #general open; the key of its newest message.
fn general(app: &mut App) -> u64 {
    drop(app.dispatch(Action::Open("general".into())));
    let rows = &app.vm.chat.timeline.rows;
    rows.last().expect("a message").key
}

fn click(app: &mut App, key: u64, link: Link) {
    handle(
        app,
        anomi_vm::chat::Intent::from(MessageIntent::Link(key, link)),
    );
}

#[test]
fn a_web_link_opens_and_a_masked_one_asks_first() {
    let mut window = start("links-masked", true);
    let app = &mut window.app;
    let key = general(app);
    click(app, key, Link::Url("https://example.com/a".into()));
    assert_eq!(app.asked.pages, ["https://example.com/a"]);
    // Only web pages: a message can't open files or start programs.
    click(app, key, Link::Url("file:///etc/passwd".into()));
    click(
        app,
        key,
        Link::Url("https://discord.com@evil.example/".into()),
    );
    assert_eq!(app.asked.pages.len(), 1);
    // A masked link shows the real address and its site, and waits.
    let masked = "https://evil.example/login?x=1";
    click(app, key, Link::Leave(masked.into()));
    assert_eq!(app.asked.pages.len(), 1, "nothing opens before Visit");
    assert!(app.vm.shell.is_open(Layer::LeavingLink));
    assert_eq!(app.vm.places.leaving_link.link, masked);
    assert_eq!(app.vm.places.leaving_link.site, "evil.example");
    handle(
        app,
        anomi_vm::places::Intent::from(leaving_link::Intent::Visit),
    );
    assert!(!app.vm.shell.is_open(Layer::LeavingLink));
    assert_eq!(app.asked.pages.last().map(String::as_str), Some(masked));
    // Go Back opens nothing.
    click(app, key, Link::Leave(masked.into()));
    handle(
        app,
        anomi_vm::places::Intent::from(leaving_link::Intent::Close),
    );
    assert_eq!(app.asked.pages.len(), 2);
}

#[test]
fn the_menus_open_link_asks_for_a_masked_link_too() {
    let mut window = start("links-menu", true);
    let app = &mut window.app;
    let key = general(app);
    let masked = "https://evil.example/x";
    handle(
        app,
        anomi_vm::chat::Intent::from(MessageIntent::LinkMenu(key, Link::Leave(masked.into()))),
    );
    handle(
        app,
        anomi_vm::chat::Intent::from(MessageIntent::Menu(key, Spot::default())),
    );
    assert_eq!(app.vm.chat.menu.facts.address, masked);
    handle(
        app,
        anomi_vm::chat::Intent::from(MessageMenu::OpenLink(masked.into())),
    );
    assert!(app.asked.pages.is_empty());
    assert!(app.vm.shell.is_open(Layer::LeavingLink));
    assert_eq!(app.vm.places.leaving_link.link, masked);
}

#[test]
fn a_topics_masked_link_asks_and_a_badges_link_opens() {
    let mut window = start("links-topic", true);
    let app = &mut window.app;
    let masked = "https://evil.example/x";
    handle(
        app,
        anomi_vm::header::Intent::Link(Link::Leave(masked.into())),
    );
    assert!(app.vm.shell.is_open(Layer::LeavingLink));
    assert_eq!(app.vm.places.leaving_link.site, "evil.example");
    app.vm.shell.open(Layer::Account);
    let badge = "https://discord.com/settings/premium";
    handle(
        app,
        anomi_vm::account::Intent::Badge(anomi_vm::people::badges::Intent::OpenLink(badge.into())),
    );
    assert!(!app.vm.shell.is_open(Layer::Account));
    assert_eq!(app.asked.pages, [badge]);
}
