//! What opens from the call's controls, above the control: the device
//! menus (`QuickDeviceMenu`, with the microphone's `InputLevel` and noise
//! suppression), the share menu (`ShareMenu`), the soundboard
//! (`Soundboard`); and someone's voice menu, their volumes (`VolumeMenu`)
//! over their rows. As the Slint client's.

use std::borrow::Cow;

use iced::{
    Element, Point, Task,
    widget::{container, opaque, operation},
};
use yapfast_core::{meter::InputMeter, voice::Command};

use super::Popup;
use crate::{
    app::{App, Message, NOBODY},
    controls::{Check, Item, Light, menu_panel},
    pictures::Icon,
    widgets::{Anchor, Place},
};

mod device;
mod person;
mod share;
mod soundboard;
use device::DEVICE_WIDTH;
pub use device::device_rows;
pub use person::Volumes;
#[cfg(test)]
pub use share::Listing;
use share::{SHARE_NARROW, SHARE_WIDE, list_sources};
pub use share::{Share, ShareEvent};
pub(crate) use soundboard::SOUNDBOARD_WIDTH;

impl App {
    pub(super) fn open_popup(&mut self, popup: Popup) -> Task<Message> {
        // A second click closes it, or cancels the share menu's listing.
        let listing = popup == Popup::Share && self.call.share_listing.is_some();
        if self.call.popup == Some(popup) || listing {
            self.close_popup();
            return Task::none();
        }
        self.close_popup();
        match popup {
            Popup::Input | Popup::Output => {
                self.client
                    .dispatch(yapfast_core::application::Action::Voice(Command::Devices));
                if popup == Popup::Input {
                    let microphone = self.client.state().voice.microphone.clone();
                    self.call.meter = Some(InputMeter::start(microphone.as_deref()));
                }
                self.call.popup = Some(popup);
            }
            Popup::Soundboard => {
                let pictures: Vec<String> = self
                    .client
                    .state()
                    .voice
                    .soundboard
                    .iter()
                    .flat_map(|groups| groups.iter())
                    .flat_map(|group| &group.sounds)
                    .filter_map(|sound| sound.picture.clone())
                    .collect();
                if !pictures.is_empty() {
                    self.client
                        .dispatch(yapfast_core::application::Action::LoadPictures(pictures));
                }
                self.call.popup = Some(popup);
            }
            // Shown once its sources are listed, so it opens at its height;
            // meanwhile it's as good as open (Escape or a click elsewhere
            // cancels it).
            Popup::Share => {
                self.call.listings += 1;
                self.call.share_listing = Some(self.call.listings);
                return Task::batch([list_sources(self.call.listings), operation::focus(NOBODY)]);
            }
            Popup::Link => {
                self.call.popup = Some(popup);
                self.read_link();
                return Task::none();
            }
        }
        Task::batch([self.sync(), operation::focus(NOBODY)])
    }

    pub fn close_popup(&mut self) {
        let link = self.call.popup == Some(Popup::Link);
        self.call.popup = None;
        if link {
            self.call.details.tab = super::LinkTab::Connection;
            self.read_link();
        }
        self.call.share = None;
        self.call.share_listing = None;
        self.call.listing = false;
        self.call.meter = None;
    }

    /// The popup open from a control, above it.
    pub fn call_popup_view(&self) -> Option<Element<'_, Message>> {
        let popup = self.call.popup?;
        let (panel, width) = match popup {
            Popup::Input => (self.device_menu(true), DEVICE_WIDTH),
            Popup::Output => (self.device_menu(false), DEVICE_WIDTH),
            Popup::Share => {
                let share = self.call.share.as_ref()?;
                let width = if share.screens.is_empty() {
                    SHARE_NARROW
                } else {
                    SHARE_WIDE
                };
                (self.share_menu(share, width), width)
            }
            Popup::Soundboard => (self.soundboard(), SOUNDBOARD_WIDTH),
            // Under its signal, or above it without room, as the Slint
            // client's `call-link-menu`.
            Popup::Link => {
                let spot = self.call.spot.get();
                return Some(
                    Place::new(opaque(self.link_card()), spot.position(), 8.0)
                        .anchor(Anchor::Span(spot.height))
                        .into(),
                );
            }
        };
        let spot = self.call.spot.get();
        let panel = container(panel).width(width).style(menu_panel(&self.theme));
        Some(
            Place::new(opaque(panel), Point::new(spot.x - 6.0, spot.y - 6.0), 8.0)
                .anchor(Anchor::Bottom)
                .into(),
        )
    }

    /// A row of a popup, as the Slint client's `MenuItem`, lit under the
    /// pointer.
    fn popup_row<'a>(
        &self,
        label: impl Into<Cow<'a, str>>,
        subtitle: Option<Cow<'a, str>>,
        icon: Option<Icon>,
        check: Check,
        press: Option<Message>,
    ) -> Element<'a, Message> {
        let item = Item {
            label: label.into(),
            icon,
            danger: false,
            slot: false,
            disabled: press.is_none(),
            subtitle,
            check,
        };
        self.menu_item(item, Light::Pointer, press.unwrap_or(Message::Nothing))
    }
}
