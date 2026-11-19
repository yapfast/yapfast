//! The layers the view stacks: the window itself, then whatever is open
//! over it (the profile card, mention suggestions, the emoji picker, a
//! menu, the modal), each with a backdrop that takes a click outside; and
//! the theme it's drawn in.

use iced::{
    Element, Length, Point,
    widget::{Space, column, container, row, stack},
};

use crate::{
    controls::outside,
    fonts,
    picker::{self, Target},
    theme::Theme,
    widgets::{Anchor, Place},
};

use super::{
    App, Message,
    layers::{Layer, Popup, Popup::Layer as L},
};

/// The zoom level's note, this far below the window's top or its bar.
const ZOOM_NOTE_TOP: f32 = 12.0;

impl App {
    pub fn view(&self) -> Element<'_, Message> {
        // Whatever this returns, what it no longer shows goes at its end.
        let _viewing = self.win.pictures.view();
        if !crate::signin::workspace(self) {
            return self.frame_layers(crate::signin::view(self.ctx()));
        }
        // The window's own background is the theme's chat (`window_style`).
        // The rail and the sidebar on the rail's ground, the account's card
        // across the foot of both.
        let rail = self.theme.rail;
        let mut foot = column![];
        if let Some(dock) = crate::call::dock::view(self.ctx()) {
            foot = foot.push(dock);
        }
        let navigation = container(column![
            row![
                crate::rail::view(&self.rail, self.ctx()),
                crate::sidebar::view(&self.sidebar, self.ctx())
            ]
            .height(Length::Fill),
            foot.push(crate::account::view(self.ctx())),
        ])
        .width(crate::conversation::navigation(&self.layout))
        .height(Length::Fill)
        .style(move |_| crate::style::fill(rail));
        // The sidebar's edge drags it wider or narrower.
        let splitter = crate::widgets::Splitter::new(
            iced::Color::TRANSPARENT,
            self.theme.line,
            self.theme.faint,
            |by| Message::Side(crate::sidebar::Event::Resize(by)),
            Message::Side(crate::sidebar::Event::Resized),
            Message::Side(crate::sidebar::Event::ResetWidth),
        );
        let navigation = stack![
            navigation,
            container(splitter)
                .width(crate::conversation::navigation(&self.layout))
                .align_x(iced::Alignment::End)
        ];
        let base: Element<'_, Message> = row![navigation, crate::conversation::conversation(self)]
            .width(Length::Fill)
            .height(Length::Fill)
            .into();
        // Always a stack, so the window's widget state stays as overlays
        // come and go: the card, the suggestions, the picker, a menu, the
        // modal, each over the last.
        let mut layers = stack![base];
        // A role's card, under the card of someone on it.
        if let Some(role) = crate::info_cards::role_layers(&self.cards, &self.profiles, self.ctx())
        {
            layers = layers.extend(
                role.into_iter()
                    .map(|layer| self.popup(L(Layer::RoleCard), layer)),
            );
        }
        if let Some(card) = crate::profile::card_view(self.ctx()) {
            layers = layers
                .push(outside(
                    Message::Profile(crate::profile::Event::Close),
                    false,
                ))
                .push(self.popup(L(Layer::Card), card));
        }
        // The picker for a dialog's emoji opens over the dialog.
        let mut over_dialogs = None;
        if let Some(picker) = self.win.picker.get() {
            let shown = picker::view(picker, self.ctx());
            let size = picker.size();
            let place = match &picker.target {
                // Over the emoji button, its right edge level with the button's.
                Target::Composer => {
                    let right = crate::conversation::chat_right(self) - 8.0 - 4.0;
                    let bottom = crate::composer::composer_buttons_top(self) - 8.0;
                    Place::new(shown, Point::new(right - size.width, bottom), 8.0)
                        .anchor(Anchor::Bottom)
                }
                // Over the edit's field, at the timeline's right.
                Target::Edit => {
                    let right = crate::conversation::chat_right(self) - 16.0;
                    let bottom = crate::edit::edit_top(self)
                        .unwrap_or(crate::composer::composer_top(self))
                        - 8.0;
                    Place::new(shown, Point::new(right - size.width, bottom), 8.0)
                        .anchor(Anchor::Bottom)
                }
                Target::React(_, at) | Target::Status(at) | Target::Poll(_, at) => {
                    Place::new(shown, *at, 8.0).anchor(Anchor::Pointer)
                }
            };
            if matches!(picker.target, Target::Status(_) | Target::Poll(..)) {
                over_dialogs = Some(place);
            } else {
                layers = layers
                    .push(outside(Message::Picker(picker::Event::Close), false))
                    .push(place);
            }
        }
        for layer in crate::call::dm::call_layers(self) {
            layers = layers.push(layer);
        }
        if let Some(viewer) = crate::call::corner::clip_viewer(self) {
            layers = layers.push(viewer);
        }
        if let Some(popup) = crate::call::popups::call_popup_view(self.ctx()) {
            layers = layers
                .push(outside(Message::Call(crate::call::Event::Close), false))
                .push(popup);
        }
        // Open, or closed and drawn going, as the tag's card below.
        if let Some(settings) = self.win.settings.get().or(self.win.settings.leaving()) {
            layers = layers.push(self.popup(
                L(Layer::Settings),
                crate::settings::view(settings, self.ctx()),
            ));
        }
        if let Some(settings) = self
            .server_screen
            .get()
            .and_then(|screen| crate::server_settings::view(screen, self.ctx()))
        {
            layers = layers.push(self.popup(L(Layer::ServerSettings), settings));
        }
        if let Some(settings) = self
            .channel_screen
            .get()
            .and_then(|screen| crate::channel_settings::view(screen, self.ctx()))
        {
            layers = layers.push(self.popup(L(Layer::ChannelSettings), settings));
        }
        // Over the settings, as a member's from Members.
        if let Some(full) = crate::profile::full_profile_view(self.ctx()) {
            layers = layers.push(self.popup(Popup::FullProfile, full));
        }
        if let Some(editor) = crate::profile::editor::profile_editor_view(self.ctx()) {
            layers = layers.push(self.popup(L(Layer::ProfileEditor), editor));
        }
        // A mutual server's, over the profile that lists it.
        if let Some(server) =
            crate::info_cards::server_layers(&self.cards, &self.profiles, self.ctx())
        {
            layers = layers.extend(
                server
                    .into_iter()
                    .map(|layer| self.popup(L(Layer::ServerCard), layer)),
            );
        }
        if let Some(expression) =
            crate::info_cards::expression_layers(&self.cards, &self.profiles, self.ctx())
        {
            layers = layers.extend(
                expression
                    .into_iter()
                    .map(|layer| self.popup(L(Layer::ExpressionCard), layer)),
            );
        }
        // A server tag's, over the profile or the messages it's on.
        if let Some(card) = self.win.tag_card.get().or(self.win.tag_card.leaving()) {
            layers = layers.extend(
                crate::tag_card::layers(card, self.ctx())
                    .into_iter()
                    .map(|layer| self.popup(L(Layer::TagCard), layer)),
            );
        }
        if let Some(popover) = crate::status::popover_view(&self.status, self.ctx()) {
            layers = layers
                .push(outside(Message::Status(crate::status::Event::Close), false))
                .push(popover);
        }
        if let Some(dialog) = crate::status::dialog_view(&self.status, self.ctx()) {
            layers = layers.push(self.popup(L(Layer::StatusEditor), dialog));
        }
        for (layer, dialog) in [
            (
                Layer::NewDm,
                crate::switcher::new_dm_view(&self.quick, self.ctx()),
            ),
            (
                Layer::Poll,
                crate::switcher::poll_view(&self.quick, self.ctx()),
            ),
            (
                Layer::Switcher,
                crate::switcher::switcher_view(&self.quick, self.ctx()),
            ),
            (
                Layer::ServerDialog,
                crate::server_dialogs::view(&self.server_dialogs, self.ctx()),
            ),
            (
                Layer::LinkDialog,
                crate::link_dialogs::view(&self.link_dialogs, self.ctx()),
            ),
        ] {
            if let Some(dialog) = dialog {
                layers = layers.push(self.popup(L(layer), dialog));
            }
        }
        if let Some(place) = over_dialogs {
            layers = layers
                .push(outside(Message::Picker(picker::Event::Close), false))
                .push(place);
        }
        if let Some(popout) = crate::search_popout::view(self.ctx()) {
            layers = layers.push(popout);
        }
        if let Some(dialog) = crate::search_filters::view(self.ctx()) {
            layers = layers.push(self.popup(L(Layer::SearchFilters), dialog));
        }
        if let Some(popover) =
            crate::header_popovers::view(&self.popovers, &self.header.name, self.ctx())
        {
            layers = layers.push(self.popup(L(Layer::HeaderPopover), popover));
        }
        if let Some(modal) = crate::bot_modal::view(&self.modal, self.ctx()) {
            layers = layers.push(modal);
        }
        if let Some(viewer) = crate::image_viewer::view(&self.viewer, self.ctx()) {
            layers = layers.push(viewer);
        }
        if let Some(menu) = self
            .win
            .menu
            .as_ref()
            .and_then(|menu| crate::menus::view(menu, self.ctx()))
        {
            layers = layers
                .push(outside(Message::Menu(crate::menus::Event::Close), false))
                .push(menu);
        }
        if let Some(window) = crate::reactions::view(&self.reactions, self.ctx()) {
            layers = layers.push(self.popup(L(Layer::Reactions), window));
        }
        if let Some(modal) = crate::dialogs::delete_view(self.confirm_focus, self.ctx()) {
            layers = layers.push(modal);
        }
        if let Some(dialog) = crate::person_menu::view(&self.person, self.ctx()) {
            layers = layers.push(self.popup(L(Layer::PersonDialog), dialog));
        }
        if let Some(dialog) = crate::friends::add_friend_view(self.ctx()) {
            layers = layers.push(self.popup(L(Layer::AddFriend), dialog));
        }
        if let Some(forward) = crate::forward::view(&self.forward, self.ctx()) {
            layers = layers.push(self.popup(L(Layer::Forwarding), forward));
        }
        if let Some(leaving) = crate::dialogs::leaving_view(self.leaving.as_deref(), self.ctx()) {
            layers = layers.push(self.popup(L(Layer::Leaving), leaving));
        }
        if let Some(drop) = crate::uploads::drop_view(self.ctx()) {
            layers = layers.push(drop);
        }
        if let Some(prompt) = crate::mfa::view(&self.mfa, self.ctx()) {
            layers = layers.push(self.popup(L(Layer::Mfa), prompt));
        }
        // While the keys are on the messages a press says so, for one off
        // the list to take them (`timeline::keys::follow_press`).
        let content = crate::widgets::RightPress::spot(
            layers.width(Length::Fill).height(Length::Fill),
            &self.pressed,
        )
        .telling(self.keyed.is_some().then_some(Message::Nothing));
        // A middle click scrolls the list under it towards the pointer.
        let theme = &self.theme;
        let arrow = |icon| {
            self.look()
                .icon(icon, 10, crate::theme::rgb(theme.secondary))
        };
        let content = crate::widgets::Autoscroll::new(
            content,
            &[crate::timeline::TIMELINE, crate::thread::LIST],
            crate::widgets::AutoscrollLook {
                ground: theme.raised,
                edge: theme.line,
                dot: theme.secondary,
                up: arrow(crate::pictures::Icon::ChevronUp),
                down: arrow(crate::pictures::Icon::ChevronDown),
            },
        );
        self.frame_layers(content)
    }

    /// The window's content under the title bar, when Yapfast draws one,
    /// where it takes the window's place (`self.window` is its size), and
    /// over it the zoom level's note, the inbox, the bar and the edges that
    /// resize the window. Ctrl and the wheel zoom over all of it.
    fn frame_layers<'a>(
        &'a self,
        content: impl Into<Element<'a, Message>>,
    ) -> Element<'a, Message> {
        let mut layers = stack![crate::widgets::Below::new(
            content,
            crate::window_bar::bar_height(self)
        )];
        if let Some(note) = self.zoom_note_view() {
            layers = layers.push(note);
        }
        if let Some(inbox) = crate::window_bar::inbox_view(self) {
            layers = layers.push(inbox);
        }
        if let Some(bar) = crate::window_bar::window_bar(self) {
            layers = layers.push(column![bar, Space::new().height(Length::Fill)]);
        }
        if let Some(edges) = crate::window_bar::resize_edges(self) {
            layers = layers.push(edges);
        }
        crate::widgets::CtrlWheel::new(layers, zoom_turn, yapfast_core::zoom::Wheel::reset).into()
    }

    /// The zoom level, a moment after it changes.
    fn zoom_note_view(&self) -> Option<Element<'_, Message>> {
        self.shell.zoom_note?;
        let theme = &self.theme;
        let tip = theme.tooltip;
        let note = container(
            iced::widget::text(format!("{:.0}%", self.shell.zoom * 100.0))
                .size(15)
                .font(fonts::BOLD)
                .color(theme.text),
        )
        .padding([0, 14])
        .height(36)
        .align_y(iced::Alignment::Center)
        .style(move |_| iced::widget::container::Style {
            shadow: crate::style::raised_shadow(),
            ..crate::style::rounded(tip, 18.0)
        });
        Some(
            column![
                Space::new().height(crate::window_bar::bar_height(self) + ZOOM_NOTE_TOP),
                container(note).center_x(Length::Fill)
            ]
            .into(),
        )
    }

    pub fn window_style(&self, _: &iced::Theme) -> iced::theme::Style {
        iced::theme::Style {
            background_color: self.theme.chat,
            text_color: self.theme.text,
        }
    }

    pub fn theme(&self) -> iced::Theme {
        self.theme.iced()
    }

    /// Shows the window in another theme, built in or the person's own, and
    /// keeps the choice, as Appearance does. Returns whether there's such a
    /// theme. Pictures stay as they are; only what the client drew in the
    /// old colours (icons, dots, the card's name) is drawn again, and the
    /// member list's styled names take the theme's palette.
    pub fn choose_theme(&mut self, id: &str) -> bool {
        let Some(theme) = self.themes.find(id) else {
            return false;
        };
        self.themes.choose(self.client.root(), &theme.id);
        self.show_theme(theme);
        true
    }

    /// Shows the window in `theme` without keeping it, as the theme
    /// editor shows what's edited.
    pub fn show_theme(&mut self, theme: Theme) {
        if theme == self.theme {
            return;
        }
        let old = std::mem::replace(&mut self.theme, theme);
        self.win.pictures.marks.borrow_mut().clear();
        self.win.icons.forget();
        self.show_roster(false);
        // Text laid out in the old theme's inks: the rows' again now, the
        // rest as each is next taken in.
        crate::timeline::recolour(self, &old);
        self.header.topic = None;
        self.popovers.forget_laid();
        crate::search::forget_found(self);
        crate::forum::forget_guidelines(self);
        self.drafts.starter = None;
    }
}

/// What a turn of the wheel with Ctrl held comes to: the zoom's step,
/// as `zoom::Wheel` makes it of a notch or 60 px of a touchpad.
fn zoom_turn(
    wheel: &mut yapfast_core::zoom::Wheel,
    delta: iced::mouse::ScrollDelta,
) -> Option<Message> {
    let step = match delta {
        iced::mouse::ScrollDelta::Lines { y, .. } => wheel.lines(f64::from(y)),
        iced::mouse::ScrollDelta::Pixels { y, .. } => wheel.pixels(f64::from(y)),
    };
    step.map(|step| Message::Shell(crate::shell::Event::ZoomStep(step)))
}
