//! The pickers' intents on the core's sample conversations: where they
//! open, what a pick does, what a search reaches and what a closed picker
//! keeps.

use iced::{Point, Rectangle, Size};
use yapfast_core::application::Action;

use anomi_vm::chat::composer::{self, Expression};
use anomi_vm::chat::emoji_picker::{self, Emoji, Purpose};
use anomi_vm::chat::gif_picker;
use anomi_vm::chat::pickers::{self, Tab};
use anomi_vm::chat::{Intent as Chat, message};
use anomi_vm::menu::Spot;
use anomi_vm::shell::{self, Layer};

use super::{Due, open};
use crate::app::App;
use crate::message::Message;
use crate::project::Reach;
use crate::tests::start;

fn ask(app: &mut App, intent: impl Into<anomi_vm::Intent>) {
    drop(app.update(Message::Intent(intent.into())));
}

fn chat(app: &mut App, intent: Chat) {
    ask(app, intent);
}

fn shell(app: &mut App, intent: shell::Intent) {
    ask(app, intent);
}

/// #general open, with a message whose row is `key`.
fn general(app: &mut App) -> u64 {
    drop(app.dispatch(Action::Open("general".into())));
    let timeline = &app.vm.chat.timeline;
    (timeline.rows.iter())
        .map(|row| row.key)
        .find(|key| app.shown.timeline.message(*key).is_some())
        .expect("a message")
}

#[test]
fn the_reaction_picker_opens_where_asked_and_holds_nothing_once_gone() {
    let mut window = start("pickers-reaction", true);
    let app = &mut window.app;
    let key = general(app);
    let spot = Spot::pointer(Point::new(700.0, 300.0)).gap(4.0);
    chat(app, Chat::Message(message::Intent::Picker(key, spot)));
    assert!(app.vm.shell.is_open(Layer::EmojiPicker));
    let picker = &app.vm.chat.emoji_picker;
    assert_eq!(picker.purpose, Purpose::Reaction);
    assert_eq!(
        picker.columns,
        emoji_picker::columns(emoji_picker::POPUP_WIDTH)
    );
    assert!(
        picker.sections.len() > 9,
        "what is used, then the categories"
    );
    let item = app.shown.timeline.message(key).unwrap().item.clone();
    assert_eq!(open::reacting_to(app), Some(item.as_str()));
    // Typing searches here, and the window's other areas aren't shown again.
    let timeline = app.shown.timeline.message(key).map(|m| m.item.clone());
    chat(
        app,
        Chat::EmojiPicker(emoji_picker::Intent::Search("fire".into())),
    );
    assert!((app.vm.chat.emoji_picker.found.iter()).any(|emoji| emoji.key() == "🔥"));
    assert_eq!(
        app.shown.timeline.message(key).map(|m| m.item.clone()),
        timeline
    );
    // Escape closes it; once it played its way out it holds nothing.
    shell(app, shell::Intent::Close(Layer::EmojiPicker));
    assert!(!app.vm.shell.is_open(Layer::EmojiPicker));
    shell(app, shell::Intent::Gone(Layer::EmojiPicker));
    assert!(app.vm.chat.emoji_picker.sections.is_empty());
    assert!(app.vm.chat.emoji_picker.found.is_empty());
    assert_eq!(open::reacting_to(app), None);
}

#[test]
fn an_emoji_goes_in_where_the_composers_caret_was() {
    let mut window = start("pickers-composer", true);
    let app = &mut window.app;
    general(app);
    let room = app.vm.chat.composer.room;
    let edited = composer::Intent::Edited {
        room,
        text: "hello world".into(),
        caret: 5,
    };
    chat(app, Chat::Composer(edited));
    // No GIFs here: the emoji button opens the emoji alone, over it.
    let buttons = Rectangle::new(Point::new(1000.0, 800.0), Size::new(90.0, 40.0));
    let frame = Rectangle::new(Point::new(300.0, 780.0), Size::new(800.0, 70.0));
    let found = [Some(buttons), Some(frame)];
    let anchored = Due::Anchored {
        which: Expression::Emoji,
        found,
    };
    drop(app.update(Message::Pickers(anchored)));
    assert!(app.vm.shell.is_open(Layer::EmojiPicker));
    assert_eq!(app.vm.chat.emoji_picker.purpose, Purpose::Composer);
    let fire = Emoji::unicode("🔥").unwrap();
    chat(app, Chat::EmojiPicker(emoji_picker::Intent::Pick(fire)));
    assert_eq!(app.core.state.draft(), "hello🔥 world");
    assert_eq!(app.vm.chat.composer.draft, "hello🔥 world");
    assert!(
        !app.vm.shell.is_open(Layer::EmojiPicker),
        "a pick closes it"
    );
    // The next goes after it.
    assert_eq!(app.shown.composer.carets.channel, Some((room, 9)));
}

#[test]
fn a_gif_search_waits_for_the_last_keystroke() {
    let mut window = start("pickers-gifs", true);
    let app = &mut window.app;
    app.vm.shell.open(Layer::Expressions);
    app.vm.chat.pickers.tab = Tab::Gifs;
    let edit = |text: &str| Chat::GifPicker(gif_picker::Intent::Edit(text.into()));
    chat(app, edit("ca"));
    chat(app, edit("cat"));
    assert_eq!(app.vm.chat.pickers.gif.query, "cat");
    drop(app.update(Message::Pickers(Due::Search(1))));
    assert_eq!(
        app.core.state.gifs.query, "",
        "an older pause searches nothing"
    );
    drop(app.update(Message::Pickers(Due::Search(2))));
    assert_eq!(app.core.state.gifs.query, "cat");
    // A search, a page or the pictures in sight reach the picker alone.
    for action in [
        Action::SearchGifs("cat".into()),
        Action::SearchStickers("cat".into()),
        Action::StickersShown(Vec::new()),
    ] {
        assert_eq!(Reach::after(&action), Reach::PICKER);
    }
}

#[test]
fn the_dragged_edge_widens_the_grids_and_is_kept() {
    let mut window = start("pickers-width", true);
    let app = &mut window.app;
    app.vm.chat.pickers.room = 900.0;
    let drag = |intent| Chat::Pickers(intent);
    chat(app, drag(pickers::Intent::Drag(700.0)));
    let pickers = &app.vm.chat.pickers;
    assert_eq!(pickers.shown(), 700.0);
    assert_eq!(pickers.emoji.columns, emoji_picker::columns(700.0));
    assert_eq!(app.layout.picker_width, None, "kept once let go");
    chat(app, drag(pickers::Intent::Chosen(700.0)));
    assert_eq!(app.layout.picker_width, Some(700.0));
}
