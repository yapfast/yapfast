//! The conversation's areas: the timeline, the message and its parts, the
//! composer, the pickers, threads, forums, search and pins: one module each,
//! the view-model and `Intent` of one task (docs/iced/tasks.md). F01 keeps
//! this file: its fields and variants are wired for every module below, so a
//! task fills only its own module.
//!
//! A module whose data another's view-model holds has a variant here but no
//! field: `message::Message` holds a message's reactions, attachments,
//! embeds, components, poll and editor; `composer::Composer` its suggestions
//! and commands; the timeline holds the messages; attachments and embeds
//! hold their videos and voice messages. The Reactions window, the poll
//! dialog and a video full screen are fields of their own
//! (`reactions::Dialog`, `poll::Dialog`, `video::FullScreen`).

pub mod attachments;
pub mod bot_forms;
pub mod cards;
pub mod commands;
pub mod components;
pub mod composer;
pub mod embeds;
pub mod emoji_picker;
pub mod forum;
pub mod forward;
pub mod gif_picker;
pub mod inline_edit;
pub mod menu;
pub mod message;
pub mod new_post;
pub mod new_thread;
pub mod pickers;
pub mod pins;
pub mod poll;
pub mod reactions;
pub mod search;
pub mod sticker_picker;
pub mod suggestions;
pub mod thread_panel;
pub mod threads;
pub mod timeline;
pub mod video;
pub mod viewer;
pub mod voice_message;

/// What the conversation's areas show, by area.
#[derive(Clone, Debug, Default)]
pub struct Chat {
    pub timeline: timeline::Timeline,
    pub reactions: reactions::Dialog,
    pub poll: poll::Dialog,
    pub cards: cards::Cards,
    pub video: video::FullScreen,
    pub viewer: viewer::Viewer,
    pub menu: menu::Menu,
    pub composer: composer::Composer,
    pub emoji_picker: emoji_picker::EmojiPicker,
    pub pickers: pickers::Pickers,
    pub bot_forms: bot_forms::BotForms,
    pub search: search::Search,
    pub pins: pins::Pins,
    pub threads: threads::Threads,
    pub thread_panel: thread_panel::ThreadPanel,
    pub new_thread: new_thread::NewThread,
    pub forum: forum::Forum,
    pub forward: forward::Forward,
}

/// What the conversation's areas ask for, by area.
#[derive(Clone, Debug, PartialEq)]
pub enum Intent {
    Timeline(timeline::Intent),
    Message(message::Intent),
    Reactions(reactions::Intent),
    Attachments(attachments::Intent),
    Embeds(embeds::Intent),
    Components(components::Intent),
    Poll(poll::Intent),
    Cards(cards::Intent),
    Video(video::Intent),
    Viewer(viewer::Intent),
    VoiceMessage(voice_message::Intent),
    Menu(menu::Intent),
    InlineEdit(inline_edit::Intent),
    Composer(composer::Intent),
    Suggestions(suggestions::Intent),
    Commands(commands::Intent),
    EmojiPicker(emoji_picker::Intent),
    Pickers(pickers::Intent),
    GifPicker(gif_picker::Intent),
    StickerPicker(sticker_picker::Intent),
    BotForms(bot_forms::Intent),
    Search(search::Intent),
    Pins(pins::Intent),
    Threads(threads::Intent),
    ThreadPanel(thread_panel::Intent),
    NewThread(new_thread::Intent),
    Forum(forum::Intent),
    NewPost(new_post::Intent),
    Forward(forward::Intent),
}

crate::parts!(Intent in Chat:
    Timeline(timeline::Intent),
    Message(message::Intent),
    Reactions(reactions::Intent),
    Attachments(attachments::Intent),
    Embeds(embeds::Intent),
    Components(components::Intent),
    Poll(poll::Intent),
    Cards(cards::Intent),
    Video(video::Intent),
    Viewer(viewer::Intent),
    VoiceMessage(voice_message::Intent),
    Menu(menu::Intent),
    InlineEdit(inline_edit::Intent),
    Composer(composer::Intent),
    Suggestions(suggestions::Intent),
    Commands(commands::Intent),
    EmojiPicker(emoji_picker::Intent),
    Pickers(pickers::Intent),
    GifPicker(gif_picker::Intent),
    StickerPicker(sticker_picker::Intent),
    BotForms(bot_forms::Intent),
    Search(search::Intent),
    Pins(pins::Intent),
    Threads(threads::Intent),
    ThreadPanel(thread_panel::Intent),
    NewThread(new_thread::Intent),
    Forum(forum::Intent),
    NewPost(new_post::Intent),
    Forward(forward::Intent),
);
