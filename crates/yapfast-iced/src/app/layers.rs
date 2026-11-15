//! What's open over the window, in one list: the window's keys, its Escape
//! and the subscription that hands a field's Escape on all read it, so a
//! new dialog is one [`Layer`] and one line in [`LAYERS`].

use iced::{
    Task,
    keyboard::{self, key},
    widget::operation,
};
use yapfast_core::application::Action;

use crate::{
    app::{App, Message},
    field, mfa, picker,
};

/// Something open over the window. [`LAYERS`] holds them in the order they
/// take keys and Escape, topmost first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layer {
    Inbox,
    Mfa,
    ConfirmDelete,
    /// A bot's select menu, over the app's form it may be on.
    SelectMenu,
    /// An app's form, unless its select menu is open over it.
    Modal,
    Viewer,
    Switcher,
    NewDm,
    Poll,
    LinkDialog,
    ServerDialog,
    StatusEditor,
    AccountPopover,
    HeaderPopover,
    ProfileEditor,
    Settings,
    ServerSettings,
    ChannelSettings,
    Forwarding,
    SwitchVoice,
    PersonDialog,
    Reactions,
    AddFriend,
    Leaving,
    Menu,
    CallPopup,
    ClipFull,
    FullCall,
    Picker,
    ServerCard,
    ExpressionCard,
    Card,
    RoleCard,
    Selection,
    TagsMenu,
    /// Then in Discord's order: the search, an edit, a reply.
    Search,
    Editing,
    Compose,
    /// The thread beside's New Thread form, then its reply, as the Slint
    /// client's `escape-composing`.
    AsideForm,
    AsideReply,
}

/// Every layer, in the order they take keys and Escape.
const LAYERS: [Layer; 40] = [
    Layer::Inbox,
    Layer::Mfa,
    Layer::ConfirmDelete,
    Layer::SelectMenu,
    Layer::Modal,
    Layer::Viewer,
    Layer::Switcher,
    Layer::NewDm,
    Layer::Poll,
    Layer::LinkDialog,
    Layer::ServerDialog,
    Layer::StatusEditor,
    Layer::AccountPopover,
    Layer::HeaderPopover,
    Layer::ProfileEditor,
    Layer::Settings,
    Layer::ServerSettings,
    Layer::ChannelSettings,
    Layer::Forwarding,
    Layer::SwitchVoice,
    Layer::PersonDialog,
    Layer::Reactions,
    Layer::AddFriend,
    Layer::Leaving,
    Layer::Menu,
    Layer::CallPopup,
    Layer::ClipFull,
    Layer::FullCall,
    Layer::Picker,
    Layer::ServerCard,
    Layer::ExpressionCard,
    Layer::Card,
    Layer::RoleCard,
    Layer::Selection,
    Layer::TagsMenu,
    Layer::Search,
    Layer::Editing,
    Layer::Compose,
    Layer::AsideForm,
    Layer::AsideReply,
];

impl Layer {
    /// Whether it takes every key no field took while it's open; the
    /// others take only Escape, the rest of the keys going on to the
    /// timeline and the composer.
    fn takes_keys(self) -> bool {
        !matches!(
            self,
            Layer::Inbox
                | Layer::SelectMenu
                | Layer::Forwarding
                | Layer::CallPopup
                | Layer::ClipFull
                | Layer::FullCall
                | Layer::Picker
                | Layer::ServerCard
                | Layer::ExpressionCard
                | Layer::Card
                | Layer::RoleCard
                | Layer::Selection
                | Layer::TagsMenu
                | Layer::Search
                | Layer::Editing
                | Layer::Compose
                | Layer::AsideForm
                | Layer::AsideReply
        )
    }

    /// Whether an Escape a field passed on ([`App::escape`]) closes it:
    /// those that take only Escape, the menu, and the switch-voice prompt,
    /// which no field is in.
    fn closes_on_escape(self) -> bool {
        !self.takes_keys() || matches!(self, Layer::Menu | Layer::SwitchVoice)
    }

    /// Whether a field in it keeps Escape to itself, only letting go of the
    /// keys: while one is open, a captured Escape closes it all the same,
    /// as the Slint client's do.
    fn field_keeps_escape(self) -> bool {
        matches!(
            self,
            Layer::AddFriend
                | Layer::PersonDialog
                | Layer::Forwarding
                | Layer::Settings
                | Layer::ServerSettings
                | Layer::ChannelSettings
                | Layer::ProfileEditor
                | Layer::StatusEditor
                | Layer::NewDm
                | Layer::Poll
                | Layer::Switcher
                | Layer::ServerDialog
                | Layer::LinkDialog
                | Layer::HeaderPopover
                | Layer::Search
                | Layer::TagsMenu
                | Layer::Modal
        )
    }
}

impl App {
    /// Whether `layer` is open now.
    fn is_open(&self, layer: Layer) -> bool {
        let state = self.client.state();
        match layer {
            Layer::Inbox => self.inbox.is_some(),
            Layer::Mfa => self.client.app.mfa_prompt().is_some(),
            Layer::ConfirmDelete => state.confirm_delete.is_some(),
            Layer::SelectMenu => state.select_menu.is_some(),
            Layer::Modal => state.interaction_modal.is_some() && state.select_menu.is_none(),
            Layer::Viewer => state.viewer.is_some() && self.menu.is_none(),
            Layer::Switcher => state.switcher.is_some(),
            Layer::NewDm => state.new_dm.is_some(),
            Layer::Poll => state.poll_draft.is_some(),
            Layer::LinkDialog => self.link_dialog_open(),
            Layer::ServerDialog => self.server_dialog_open(),
            Layer::StatusEditor => state.status_editor.is_some(),
            Layer::AccountPopover => self.account_popover.is_some(),
            Layer::HeaderPopover => self.popover_open(),
            Layer::ProfileEditor => self.editor_open(),
            Layer::Settings => self.settings.is_some(),
            Layer::ServerSettings => self.server_screen.is_some(),
            Layer::ChannelSettings => self.channel_screen.is_some(),
            Layer::Forwarding => state.forwarding.is_some(),
            Layer::SwitchVoice => state.switch_voice.is_some(),
            Layer::PersonDialog => state.person_dialog.is_some(),
            Layer::Reactions => state.reactions.is_some(),
            Layer::AddFriend => state.add_friend.is_some(),
            Layer::Leaving => self.leaving.is_some(),
            Layer::Menu => self.menu.is_some(),
            Layer::CallPopup => self.call.popup.is_some() || self.call.share_listing.is_some(),
            Layer::ClipFull => self.clip_full,
            Layer::FullCall => self.full_shown(),
            Layer::Picker => self.picker.is_some(),
            Layer::ServerCard => self.cards.server.is_some(),
            Layer::ExpressionCard => self.cards.expression.is_some(),
            Layer::Card => self.card.is_some() || self.profiles.shown.is_some(),
            Layer::RoleCard => state.role.is_some(),
            Layer::Selection => self.selection.is_some(),
            Layer::TagsMenu => self.forum_post.tags_menu,
            Layer::Search => self.search_shown(),
            Layer::Editing => self.editing.is_some(),
            Layer::Compose => state.compose().is_some(),
            Layer::AsideForm => self.aside_shown() && state.new_thread().is_some(),
            Layer::AsideReply => self.aside_shown() && state.aside_compose().is_some(),
        }
    }

    /// What's open, in the order they take keys and Escape.
    pub fn layers(&self) -> impl Iterator<Item = Layer> + '_ {
        LAYERS.into_iter().filter(|layer| self.is_open(*layer))
    }

    /// Whether an open layer's field keeps Escape, which the window then
    /// takes as captured (`App::subscription`).
    pub fn field_keeps_escape(&self) -> bool {
        self.layers().any(Layer::field_keeps_escape)
    }

    /// Keys no field took: the first open layer that takes every key has
    /// them; else the keyboard's steps through messages, Ctrl+C for the
    /// timeline's selection, Escape for the first thing it closes, and
    /// typing for the composer.
    pub fn key(&mut self, key: keyboard::Key, modifiers: keyboard::Modifiers) -> Task<Message> {
        let named = match &key {
            keyboard::Key::Named(named) => Some(*named),
            _ => None,
        };
        let taker = self.layers().find(|layer| layer.takes_keys());
        if let Some(layer) = taker {
            return self.layer_key(layer, named, modifiers);
        }
        if self.keyed.is_some()
            && let Some(task) = self.browse_key(&key, modifiers)
        {
            return task;
        }
        if modifiers.command()
            && key.as_ref() == keyboard::Key::Character("c")
            && let Some(copied) = self.copied()
        {
            return iced::clipboard::write(copied);
        }
        if named == Some(key::Named::Escape) {
            return self.escape();
        }
        self.type_anywhere(&key, modifiers)
    }

    /// A key for a layer that takes them all: those with keys of their own
    /// have their handler, the rest close on Escape, and the switch-voice
    /// prompt and the leave dialog say yes on Enter.
    fn layer_key(
        &mut self,
        layer: Layer,
        named: Option<key::Named>,
        modifiers: keyboard::Modifiers,
    ) -> Task<Message> {
        match (layer, named) {
            (Layer::ConfirmDelete, _) => self.confirm_key(named),
            (Layer::Viewer, _) => self.viewer_key(named),
            (Layer::Switcher, _) => self.switcher_key(named, modifiers.shift()),
            (Layer::LinkDialog, _) => self.link_dialog_key(named),
            (Layer::ServerDialog, _) => self.server_dialog_key(named),
            (Layer::PersonDialog, _) => self.person_key(named),
            (Layer::Menu, _) => self.menu_key(named),
            (Layer::SwitchVoice, Some(key::Named::Enter)) => {
                self.call_event(crate::call::Event::Switch(true))
            }
            (Layer::Leaving, Some(key::Named::Enter)) => self.leave(true),
            (_, Some(key::Named::Escape)) => self.shut(layer),
            _ => Task::none(),
        }
    }

    /// Escape where nothing else took it: the first open thing it closes.
    pub fn escape(&mut self) -> Task<Message> {
        let first = self.layers().find(|layer| layer.closes_on_escape());
        match first {
            Some(layer) => self.shut(layer),
            None => Task::none(),
        }
    }

    /// Closes `layer` as Escape does.
    fn shut(&mut self, layer: Layer) -> Task<Message> {
        match layer {
            Layer::Inbox => {
                self.inbox = None;
                Task::none()
            }
            Layer::Mfa => self.mfa_event(mfa::Event::Cancel),
            Layer::SelectMenu => self.select_event(crate::bot_select::Event::Close),
            Layer::Modal => self.modal_event(crate::modal::Event::Close),
            Layer::NewDm => self.switcher_event(crate::switcher::Event::CloseNewDm),
            // The picker over a poll or the status editor goes first.
            Layer::Poll | Layer::StatusEditor if self.picker.is_some() => self.close_picker(),
            Layer::Poll => self.switcher_event(crate::switcher::Event::ClosePoll),
            Layer::StatusEditor => self.status_event(crate::status::Event::Cancel),
            Layer::AccountPopover => self.status_event(crate::status::Event::Close),
            Layer::HeaderPopover => self.popover_event(crate::popovers::Event::Close),
            Layer::ProfileEditor => self.editor_escape(),
            Layer::Settings => self.settings_escape(),
            Layer::ServerSettings => self.server_settings_escape(),
            Layer::ChannelSettings => self.channel_settings_escape(),
            Layer::Forwarding => self.forward_event(crate::forward::Event::Close),
            Layer::SwitchVoice => self.call_event(crate::call::Event::Switch(false)),
            Layer::Reactions => self.reactions_event(crate::reactions::Event::Close),
            Layer::AddFriend => self.friends_event(crate::friends::Event::CloseAdd),
            Layer::Leaving => self.leave(false),
            Layer::Menu => {
                self.menu = None;
                self.refocus()
            }
            Layer::CallPopup => {
                self.close_popup();
                self.refocus()
            }
            Layer::ClipFull => self.clip_event(crate::playback::Clip::Full(false)),
            Layer::FullCall => self.call_event(crate::call::Event::Full(false)),
            Layer::Picker => self.picker_event(picker::Event::Field(field::Event::Escape)),
            Layer::ServerCard => self.info_event(crate::info_cards::Event::CloseServer),
            Layer::ExpressionCard => self.info_event(crate::info_cards::Event::CloseExpression),
            Layer::Card => {
                self.revert_note();
                self.update(Message::CloseProfile)
            }
            Layer::RoleCard => self.info_event(crate::info_cards::Event::CloseRole),
            Layer::Selection => {
                self.selection = None;
                Task::none()
            }
            Layer::TagsMenu => self.forum_event(crate::forum::Event::TagsMenu(false)),
            Layer::Search => self.search_event(crate::search::Event::Clear),
            Layer::Editing => {
                self.client.dispatch(Action::CancelEdit);
                Task::batch([self.sync(), operation::focus(crate::composer::COMPOSER)])
            }
            Layer::Compose => {
                self.client.dispatch(Action::CancelCompose);
                self.sync()
            }
            Layer::AsideForm => self.thread_event(crate::thread::Event::CloseForm),
            Layer::AsideReply => self.thread_event(crate::thread::Event::CancelReply),
            // These take Escape in their own key handlers (`layer_key`).
            Layer::ConfirmDelete
            | Layer::Viewer
            | Layer::Switcher
            | Layer::LinkDialog
            | Layer::ServerDialog
            | Layer::PersonDialog => Task::none(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_layer_is_listed_once() {
        for (at, layer) in LAYERS.iter().enumerate() {
            assert!(!LAYERS[at + 1..].contains(layer), "{layer:?} twice");
        }
    }
}
