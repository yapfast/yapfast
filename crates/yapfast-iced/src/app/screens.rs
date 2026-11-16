//! The root's glue for each screen on the contract (`docs/iced-client.md`,
//! Screens): its message handed to its update with a [`Cx`], its Escape,
//! and, for one the core opens, its slot following the core's state as
//! the window syncs. What's asked of the root then is done by
//! `App::settle`.

use iced::Task;

use super::{App, Message};
use crate::ctx::cx;

impl App {
    /// A server tag's card: a tag clicked opens its card under the press,
    /// in place of one open.
    pub fn tag_card_event(&mut self, event: crate::tag_card::Event) -> Task<Message> {
        use crate::{
            ctx::{Focus, Request},
            slot::Why,
            tag_card::{Event, Shown},
        };
        let at = self.pressed.get();
        let mut cx = cx!(self);
        match event {
            Event::Open {
                server,
                text,
                badge,
            } => {
                self.win.tag_card.close(&mut cx, Why::User);
                self.win
                    .tag_card
                    .open(|| Shown::open(server, text, badge, at, &mut cx));
                // After the keys the closed one gave back: Escape reaches it.
                cx.request(Request::Focus(Focus::Nobody));
                Task::none()
            }
            event => match self.win.tag_card.get_mut() {
                Some(card) => card.update(event, &mut cx),
                None => Task::none(),
            },
        }
    }

    pub fn channel_settings_event(
        &mut self,
        event: crate::channel_settings::Event,
    ) -> Task<Message> {
        let mut cx = cx!(self);
        let Some(screen) = self.channel_screen.get_mut() else {
            return Task::none();
        };
        screen.update(event, &mut cx)
    }

    pub fn channel_settings_escape(&mut self) -> Task<Message> {
        let mut cx = cx!(self);
        let Some(screen) = self.channel_screen.get_mut() else {
            return Task::none();
        };
        screen.escape(&mut cx)
    }

    pub fn server_settings_event(&mut self, event: crate::server_settings::Event) -> Task<Message> {
        let mut cx = cx!(self);
        let Some(screen) = self.server_screen.get_mut() else {
            return Task::none();
        };
        screen.update(event, &mut cx)
    }

    /// Escape in a server's settings: a member's menu or profile over them,
    /// from Members, closes first; then the settings' own.
    pub fn server_settings_escape(&mut self) -> Task<Message> {
        if self.win.menu.take().is_some() {
            return Task::none();
        }
        if self
            .profiles
            .shown
            .as_ref()
            .is_some_and(|shown| shown.look.view.large)
        {
            return self.profile_event(crate::profile::Event::Close);
        }
        let mut cx = cx!(self);
        let Some(screen) = self.server_screen.get_mut() else {
            return Task::none();
        };
        screen.escape(&mut cx)
    }

    /// A server's settings follow the core's, as the window syncs.
    pub fn server_settings_sync(&mut self) -> Task<Message> {
        let mut cx = cx!(self);
        crate::server_settings::follow(&mut self.server_screen, &mut cx)
    }

    /// The channel's settings follow the core's, as the window syncs.
    pub fn channel_settings_sync(&mut self) -> Task<Message> {
        let mut cx = cx!(self);
        crate::channel_settings::follow(&mut self.channel_screen, &mut cx)
    }

    pub fn editor_event(&mut self, event: crate::profile::editor::Event) -> Task<Message> {
        let mut cx = cx!(self);
        let Some(editor) = self.profiles.editor.get_mut() else {
            return Task::none();
        };
        editor.update(event, &mut cx)
    }

    /// Escape in the editor: the name style window's first, then what's
    /// edited or open on the card, then the editor.
    pub fn editor_escape(&mut self) -> Task<Message> {
        let mut cx = cx!(self);
        let Some(editor) = self.profiles.editor.get_mut() else {
            return Task::none();
        };
        editor.escape(&mut cx)
    }

    pub fn editor_open(&self) -> bool {
        self.profiles.editor.is_open()
    }

    /// The switcher's, New message's and the poll's: Ctrl+K does nothing
    /// over Settings, and an answer's emoji opens the picker beside it.
    pub fn switcher_event(&mut self, event: crate::switcher::Event) -> Task<Message> {
        use crate::switcher::Event;
        match event {
            Event::Toggle if self.win.settings.is_open() => Task::none(),
            Event::PollEmoji(index) => {
                let at = self.pressed.get();
                self.open_picker(
                    crate::picker::Target::Poll(index, at),
                    crate::picker::Tab::Emoji,
                )
            }
            event => {
                let mut cx = cx!(self);
                self.quick.update(event, &mut cx)
            }
        }
    }

    /// The account's popover's and the custom status's: User Settings
    /// opens Settings, and the emoji's button the picker beside it.
    pub fn status_event(&mut self, event: crate::status::Event) -> Task<Message> {
        use crate::status::Event;
        let settings = matches!(event, Event::Settings);
        if matches!(event, Event::Emoji) {
            let at = self.pressed.get();
            return self.open_picker(crate::picker::Target::Status(at), crate::picker::Tab::Emoji);
        }
        let mut cx = cx!(self);
        let task = self.status.update(event, &mut cx);
        if settings {
            return self.open_settings(crate::settings::Section::Account);
        }
        task
    }

    /// The picture viewer's: Save asks where, and a picture's menu opens
    /// with the menus.
    pub fn image_event(&mut self, event: crate::image_viewer::Event) -> Task<Message> {
        use crate::image_viewer::Event;
        match event {
            Event::Save(item, index) => crate::image_viewer::save(item, index, self.client.state()),
            Event::Menu(item, index, video) => self.media_menu(item, index, video),
            event => {
                let mut cx = cx!(self);
                self.viewer.update(event, &mut cx)
            }
        }
    }

    /// Opens `user`'s card at `at`.
    pub fn open_card(&mut self, user: String, at: iced::Point) -> Task<Message> {
        let mut cx = cx!(self);
        self.profiles.open_card(user, at, &mut cx);
        self.settle()
    }

    /// A profile's: an activity's menu opens with the menus, Manage
    /// Privacy opens Settings, a server's card opens over the profile,
    /// and the editor's go to it.
    pub fn profile_event(&mut self, event: crate::profile::Event) -> Task<Message> {
        use crate::profile::Event;
        match event {
            Event::ActivityMenu(privacy) => {
                let mut rows = vec![crate::menus::MenuRow::plain(
                    crate::menus::MenuAct::AllActivity,
                    "View All Activity",
                )];
                if privacy {
                    rows.push(crate::menus::MenuRow::plain(
                        crate::menus::MenuAct::ActivityPrivacy,
                        "Manage Privacy",
                    ));
                }
                let at = self.pressed.get();
                self.win.menu = Some(
                    crate::menus::Menu::new(iced::Point::new(at.x + 4.0, at.y - 8.0), rows)
                        .width(crate::profile::ACTIVITY_MENU),
                );
                iced::widget::operation::focus(crate::app::NOBODY)
            }
            Event::ActivityPrivacy => {
                self.client
                    .dispatch(yapfast_core::application::Action::CloseProfile);
                self.profiles.card = None;
                self.open_settings(crate::settings::Section::Games)
            }
            // Its card over the profile, which goes there.
            Event::Server(server) => self.info_event(crate::info_cards::Event::Server(server)),
            Event::Editor(event) => self.editor_event(event),
            event => {
                let mut cx = cx!(self);
                self.profiles.update(event, &mut cx)
            }
        }
    }

    /// The info cards': someone on a role's card opens their card, and a
    /// mutual server's or a sticker's opens at the press.
    pub fn info_event(&mut self, event: crate::info_cards::Event) -> Task<Message> {
        use crate::info_cards::{Event, Expression};
        match event {
            Event::RoleMember(user) => {
                let at = self.pressed.get();
                self.open_card(user, iced::Point::new(at.x + 8.0, at.y))
            }
            Event::Server(server) => {
                self.cards.server = Some((server, self.pressed.get()));
                self.ask()
            }
            Event::Sticker(item, index) => self.show_expression(Expression::Sticker(item, index)),
            event => {
                if matches!(event, Event::GoToServer(_)) {
                    self.profiles.card = None;
                }
                let mut cx = cx!(self);
                self.cards.update(event, &mut cx)
            }
        }
    }

    /// A role mention clicked at `at`: its card there.
    pub fn show_role(&mut self, role: String, at: iced::Point) -> Task<Message> {
        let mut cx = cx!(self);
        let task = self.cards.show_role(role, at, &mut cx);
        Task::batch([task, self.settle()])
    }

    /// An emoji's or sticker's card, under what was pressed
    /// (`App::link_spot`, from the press).
    pub fn show_expression(&mut self, expression: crate::info_cards::Expression) -> Task<Message> {
        let spot = self.pressed_spot(self.link_spot.get());
        let mut cx = cx!(self);
        let task = self.cards.show_expression(expression, spot, &mut cx);
        Task::batch([task, self.settle()])
    }

    /// A place noted from the press (`widgets::Spot::by_press`, a link's
    /// in text), in the window.
    pub fn pressed_spot(&self, from_press: iced::Rectangle) -> iced::Rectangle {
        from_press + (self.pressed.get() - iced::Point::ORIGIN)
    }
}
