//! What's open over the window, in one list: the window's keys, its Escape
//! and the subscription that hands a field's Escape on all read it, so a
//! new dialog is one [`Layer`] and one line in [`LAYERS`].

use std::{cell::Cell, time::Instant};

use iced::{
    Task,
    keyboard::{self, key},
    widget::operation,
};
use yapfast_core::application::Action;

use crate::{
    app::{App, Message},
    field, mfa, picker,
    widgets::motion,
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
    /// A search's Filters dialog.
    SearchFilters,
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
    TagCard,
    Card,
    RoleCard,
    Selection,
    TagsMenu,
    /// Then in Discord's order: the search, an edit, a reply.
    Search,
    Editing,
    Compose,
    /// The thread beside's New Thread form, then its reply.
    AsideForm,
    AsideReply,
}

/// Every layer, in the order they take keys and Escape.
const LAYERS: [Layer; 42] = [
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
    Layer::SearchFilters,
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
    Layer::TagCard,
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

/// The popups that open and close in motion (`widgets::motion`), layer by
/// layer. A profile's full view opens on its own, as
/// [`Popup::FullProfile`], over the card it may open from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Popup {
    Layer(Layer),
    FullProfile,
}

impl Popup {
    /// Its bit in [`Opening`]'s mask: a layer's place in [`LAYERS`], the
    /// full profile's past them.
    fn bit(self) -> u32 {
        match self {
            Popup::Layer(layer) => LAYERS
                .iter()
                .position(|each| *each == layer)
                .expect("every layer is in LAYERS") as u32,
            Popup::FullProfile => LAYERS.len() as u32,
        }
    }
}

/// The popups moving: each open one, and each closed one still drawn
/// going ([`crate::slot::Slot::leaving`]). Kept here rather than in the
/// widget tree, whose state goes with the layers' places as one under
/// them opens or closes.
#[derive(Default)]
pub struct Opening {
    open: u64,
    leaving: u64,
    moving: Vec<Moving>,
}

/// A popup opening or closing: its bit, when it was first drawn so (empty
/// until then), which its `Motion` reads.
struct Moving {
    bit: u32,
    start: Cell<Option<Instant>>,
    closing: bool,
}

impl Layer {
    /// Whether it opens as a popup, growing and its dim fading in
    /// ([`Popup`]).
    fn moves(self) -> bool {
        matches!(
            self,
            Layer::Mfa
                | Layer::Switcher
                | Layer::NewDm
                | Layer::Poll
                | Layer::LinkDialog
                | Layer::ServerDialog
                | Layer::SearchFilters
                | Layer::StatusEditor
                | Layer::HeaderPopover
                | Layer::ProfileEditor
                | Layer::Settings
                | Layer::ServerSettings
                | Layer::ChannelSettings
                | Layer::Forwarding
                | Layer::SwitchVoice
                | Layer::PersonDialog
                | Layer::Reactions
                | Layer::AddFriend
                | Layer::Leaving
                | Layer::ServerCard
                | Layer::ExpressionCard
                | Layer::TagCard
                | Layer::Card
                | Layer::RoleCard
        )
    }

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
                | Layer::FullCall
                | Layer::Picker
                | Layer::ServerCard
                | Layer::ExpressionCard
                | Layer::TagCard
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
    /// keys: while one is open, a captured Escape closes it all the same.
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
                | Layer::SearchFilters
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
            Layer::Viewer => state.viewer.is_some() && self.win.menu.is_none(),
            Layer::Switcher => state.switcher.is_some(),
            Layer::NewDm => state.new_dm.is_some(),
            Layer::Poll => state.poll_draft.is_some(),
            Layer::LinkDialog => crate::link_dialogs::is_open(state),
            Layer::SearchFilters => crate::search_filters::is_open(self),
            Layer::ServerDialog => crate::server_dialogs::is_open(state),
            Layer::StatusEditor => state.status_editor.is_some(),
            Layer::AccountPopover => self.status.popover.is_some(),
            Layer::HeaderPopover => crate::header_popovers::popover_open(self),
            Layer::ProfileEditor => self.editor_open(),
            Layer::Settings => self.win.settings.is_open(),
            Layer::ServerSettings => self.server_screen.is_open(),
            Layer::ChannelSettings => self.channel_screen.is_open(),
            Layer::Forwarding => state.forwarding.is_some(),
            Layer::SwitchVoice => state.switch_voice.is_some(),
            Layer::PersonDialog => state.person_dialog.is_some(),
            Layer::Reactions => state.reactions.is_some(),
            Layer::AddFriend => state.add_friend.is_some(),
            Layer::Leaving => self.leaving.is_some(),
            Layer::Menu => self.win.menu.is_some(),
            Layer::CallPopup => self.call.popup.is_some() || self.call.share_listing.is_some(),
            Layer::ClipFull => self.playback.clip_full,
            Layer::FullCall => crate::call::stage::full_shown(self),
            Layer::Picker => self.win.picker.is_open(),
            Layer::ServerCard => self.cards.server.is_some(),
            Layer::ExpressionCard => self.cards.expression.is_some(),
            Layer::TagCard => self.win.tag_card.is_open(),
            Layer::Card => self.profiles.card.is_some() || self.profiles.shown.is_some(),
            Layer::RoleCard => state.role.is_some(),
            Layer::Selection => self.timeline.selection.is_some(),
            Layer::TagsMenu => self.forum_post.tags_menu,
            Layer::Search => crate::search::search_shown(self),
            Layer::Editing => self.composer.editing.is_some(),
            Layer::Compose => state.compose().is_some(),
            Layer::AsideForm => crate::thread::aside_shown(self) && state.new_thread().is_some(),
            Layer::AsideReply => {
                crate::thread::aside_shown(self) && state.aside_compose().is_some()
            }
        }
    }

    /// Notes the popups that opened and closed since the last message: one
    /// that opened starts its motion as it's first drawn, one closed that's
    /// drawn going starts closing, and the rest forget theirs. Nothing
    /// changes while the same popups stay open and the same go.
    pub(super) fn follow_popups(&mut self) {
        let mut open = 0u64;
        for (at, layer) in LAYERS.iter().enumerate() {
            if layer.moves() && self.is_open(*layer) {
                open |= 1 << at;
            }
        }
        if self
            .profiles
            .shown
            .as_ref()
            .is_some_and(|shown| shown.look.view.large)
        {
            open |= 1 << Popup::FullProfile.bit();
        }
        let leaving = self.popups_leaving();
        let opening = &mut self.opening;
        if open == opening.open && leaving == opening.leaving {
            return;
        }
        opening.moving.retain_mut(|moving| {
            let bit = 1 << moving.bit;
            if open & bit != 0 || moving.closing {
                return open & bit != 0 || leaving & bit != 0;
            }
            if leaving & bit == 0 {
                return false;
            }
            // Just closed, and drawn going.
            moving.start = Cell::new(None);
            moving.closing = true;
            true
        });
        let now = Instant::now();
        for bit in 0..u64::BITS {
            if open & !opening.open & (1 << bit) == 0 {
                continue;
            }
            // One opened again as it closes grows on from there.
            let start = match opening.moving.iter().position(|each| each.bit == bit) {
                Some(at) => match opening.moving.remove(at).start.get() {
                    Some(closing) => Some(motion::reopened(closing, now)),
                    None => now.checked_sub(motion::DURATION),
                },
                None => None,
            };
            opening.moving.push(Moving {
                bit,
                start: Cell::new(start),
                closing: false,
            });
        }
        opening.open = open;
        opening.leaving = leaving;
    }

    /// The popups closed and still drawn going, by their bits: the
    /// screens the window keeps closed ([`crate::slot::Closes::LEAVES`]).
    fn popups_leaving(&self) -> u64 {
        let mut leaving = 0;
        if self.win.settings.leaving().is_some() {
            leaving |= 1 << Popup::Layer(Layer::Settings).bit();
        }
        if self.win.tag_card.leaving().is_some() {
            leaving |= 1 << Popup::Layer(Layer::TagCard).bit();
        }
        leaving
    }

    /// `popup` closed has been drawn going: what it showed goes.
    pub(super) fn popup_gone(&mut self, popup: Popup) {
        match popup {
            Popup::Layer(Layer::Settings) => self.win.settings.gone(),
            Popup::Layer(Layer::TagCard) => self.win.tag_card.gone(),
            _ => {}
        }
        let bit = popup.bit();
        self.opening
            .moving
            .retain(|moving| !(moving.closing && moving.bit == bit));
    }

    /// The popups moving, by their bits and whether each is closing, for
    /// the tests.
    #[cfg(test)]
    pub(crate) fn popups_moving(&self) -> Vec<(u32, bool)> {
        self.opening
            .moving
            .iter()
            .map(|moving| (moving.bit, moving.closing))
            .collect()
    }

    /// The call's switch prompt, opening as a popup.
    pub fn switch_popup<'a>(
        &'a self,
        layer: iced::Element<'a, Message>,
    ) -> iced::Element<'a, Message> {
        self.popup(Popup::Layer(Layer::SwitchVoice), layer)
    }

    /// `layer`, opening or closing as `popup` does ([`Popup`]).
    pub(super) fn popup<'a>(
        &'a self,
        popup: Popup,
        layer: iced::Element<'a, Message>,
    ) -> iced::Element<'a, Message> {
        let bit = popup.bit();
        match self.opening.moving.iter().find(|each| each.bit == bit) {
            Some(moving) if moving.closing => {
                motion::leave(layer, &moving.start, Message::PopupGone(popup))
            }
            Some(moving) => motion::motion(layer, &moving.start),
            None => layer,
        }
    }

    /// What's open, in the order they take keys and Escape.
    pub fn layers(&self) -> impl Iterator<Item = Layer> + '_ {
        LAYERS.into_iter().filter(|layer| self.is_open(*layer))
    }

    /// Whether a layer that takes every key is open: a dialog, a menu, the
    /// settings.
    pub fn keys_held(&self) -> bool {
        self.layers().any(Layer::takes_keys)
    }

    /// Whether an open layer's field keeps Escape, which the window then
    /// takes as captured (`App::subscription`).
    pub fn field_keeps_escape(&self) -> bool {
        self.layers().any(Layer::field_keeps_escape)
    }

    /// Keys no field took: the first open layer that takes every key has
    /// them; else a voice message's volume open has its own, then the
    /// keyboard's steps through messages, Ctrl+C for the
    /// timeline's selection, Escape for the first thing it closes, and
    /// typing and Ctrl+V for the composer.
    pub fn key(&mut self, key: keyboard::Key, modifiers: keyboard::Modifiers) -> Task<Message> {
        let named = match &key {
            keyboard::Key::Named(named) => Some(*named),
            _ => None,
        };
        let taker = self.layers().find(|layer| layer.takes_keys());
        if let Some(layer) = taker {
            return self.layer_key(layer, named, modifiers);
        }
        if let Some(task) = crate::picker::gifs_key(self, &key, modifiers) {
            return task;
        }
        if let Some(task) = crate::playback::voice_key(self, named) {
            return task;
        }
        if self.keyed.is_some()
            && let Some(task) = crate::timeline::keys::browse_key(self, &key, modifiers)
        {
            return task;
        }
        if modifiers.command()
            && key.as_ref() == keyboard::Key::Character("c")
            && let Some(copied) = crate::timeline::selection::copied_selection(self)
        {
            return iced::clipboard::write(copied);
        }
        if named == Some(key::Named::Escape) {
            return self.escape();
        }
        crate::composer::type_anywhere(self, &key, modifiers)
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
            (Layer::ConfirmDelete, _) => crate::dialogs::confirm_key(self, named),
            (Layer::Viewer, _) => {
                let mut cx = crate::ctx::cx!(self);
                self.viewer.key(named, &mut cx)
            }
            (Layer::Switcher, _) => {
                let mut cx = crate::ctx::cx!(self);
                self.quick.key(named, modifiers.shift(), &mut cx)
            }
            (Layer::LinkDialog, _) => {
                let mut cx = crate::ctx::cx!(self);
                self.link_dialogs.key(named, &mut cx)
            }
            (Layer::ServerDialog, _) => {
                let mut cx = crate::ctx::cx!(self);
                self.server_dialogs.key(named, &mut cx)
            }
            (Layer::PersonDialog, _) => {
                let mut cx = crate::ctx::cx!(self);
                self.person.key(named, &mut cx)
            }
            (Layer::Menu, _) => crate::menus::menu_key(self, named),
            (Layer::ClipFull, _) => crate::playback::clip_key(self, named),
            (Layer::SwitchVoice, Some(key::Named::Enter)) => {
                crate::call::update(self, crate::call::Event::Switch(true))
            }
            (Layer::Leaving, Some(key::Named::Enter)) => crate::dialogs::leave(self, true),
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
            Layer::Mfa => {
                let mut cx = crate::ctx::cx!(self);
                self.mfa.update(mfa::Event::Cancel, &mut cx)
            }
            Layer::SelectMenu => {
                let mut cx = crate::ctx::cx!(self);
                crate::bot_select::update(crate::bot_select::Event::Close, &mut cx)
            }
            Layer::Modal => {
                let mut cx = crate::ctx::cx!(self);
                self.modal.update(crate::bot_modal::Event::Close, &mut cx)
            }
            Layer::NewDm => self.switcher_event(crate::switcher::Event::CloseNewDm),
            // The picker over a poll or the status editor goes first.
            Layer::Poll | Layer::StatusEditor if self.win.picker.is_open() => {
                crate::picker::close_picker(self)
            }
            Layer::Poll => self.switcher_event(crate::switcher::Event::ClosePoll),
            Layer::StatusEditor => self.status_event(crate::status::Event::Cancel),
            Layer::AccountPopover => self.status_event(crate::status::Event::Close),
            Layer::HeaderPopover => {
                crate::header_popovers::popover_event(self, crate::header_popovers::Event::Close)
            }
            Layer::ProfileEditor => self.editor_escape(),
            Layer::Settings => self.settings_escape(),
            Layer::ServerSettings => self.server_settings_escape(),
            Layer::ChannelSettings => self.channel_settings_escape(),
            Layer::Forwarding => {
                let mut cx = crate::ctx::cx!(self);
                self.forward.update(crate::forward::Event::Close, &mut cx)
            }
            Layer::SwitchVoice => crate::call::update(self, crate::call::Event::Switch(false)),
            Layer::Reactions => {
                let mut cx = crate::ctx::cx!(self);
                self.reactions
                    .update(crate::reactions::Event::Close, &mut cx)
            }
            Layer::AddFriend => crate::friends::update(self, crate::friends::Event::CloseAdd),
            Layer::SearchFilters => {
                crate::search_filters::update(self, crate::search_filters::FiltersEvent::Close)
            }
            Layer::Leaving => crate::dialogs::leave(self, false),
            Layer::Menu => {
                self.win.menu = None;
                crate::composer::refocus(self)
            }
            Layer::CallPopup => {
                crate::call::popups::close_popup(self);
                crate::composer::refocus(self)
            }
            Layer::ClipFull => {
                crate::playback::clip_event(self, crate::playback::Clip::Full(false))
            }
            Layer::FullCall => crate::call::update(self, crate::call::Event::Full(false)),
            Layer::Picker => {
                crate::picker::picker_event(self, picker::Event::Field(field::Event::Escape))
            }
            Layer::ServerCard => self.info_event(crate::info_cards::Event::CloseServer),
            Layer::ExpressionCard => self.info_event(crate::info_cards::Event::CloseExpression),
            Layer::TagCard => self.tag_card_event(crate::tag_card::Event::Close),
            Layer::Card => {
                self.profiles.revert_note();
                self.update(Message::Profile(crate::profile::Event::Close))
            }
            Layer::RoleCard => self.info_event(crate::info_cards::Event::CloseRole),
            Layer::Selection => {
                self.timeline.selection = None;
                Task::none()
            }
            Layer::TagsMenu => crate::forum::update(self, crate::forum::Event::TagsMenu(false)),
            Layer::Search => {
                let mut cx = crate::ctx::cx!(self);
                self.searching.update(crate::search::Event::Clear, &mut cx)
            }
            Layer::Editing => {
                self.client.dispatch(Action::CancelEdit);
                Task::batch([self.sync(), operation::focus(crate::composer::COMPOSER)])
            }
            Layer::Compose => {
                self.client.dispatch(Action::CancelCompose);
                self.sync()
            }
            Layer::AsideForm => crate::thread::update(self, crate::thread::Event::CloseForm),
            Layer::AsideReply => crate::thread::update(self, crate::thread::Event::CancelReply),
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
