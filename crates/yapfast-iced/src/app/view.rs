//! The layers the view stacks: the window itself, then whatever is open
//! over it (the profile card, mention suggestions, the emoji picker, a
//! menu, the modal), each with a backdrop that takes a click outside; and
//! the theme it's drawn in.

use iced::{
    Element, Length, Point,
    widget::{Space, Stack, column, container, row, stack},
};

use crate::{
    controls::outside,
    fonts,
    picker::{self, Target},
    theme::Theme,
    widgets::{Anchor, Place},
};

use super::{App, Message};

/// The zoom level's note, this far below the window's top or its bar.
const ZOOM_NOTE_TOP: f32 = 12.0;

impl App {
    pub fn view(&self) -> Element<'_, Message> {
        // Whatever this returns, what it no longer shows goes at its end.
        let _viewing = self.pictures.view();
        if !self.workspace() {
            return self.frame_layers(crate::signin::view(self.ctx())).into();
        }
        // The window's own background is the theme's chat (`window_style`).
        // The rail and the sidebar on the rail's ground, the account's card
        // across the foot of both.
        let rail = self.theme.rail;
        let mut foot = column![];
        if let Some(dock) = self.dock_view() {
            foot = foot.push(dock);
        }
        let navigation = container(column![
            row![self.rail_view(), self.sidebar_view()].height(Length::Fill),
            foot.push(self.account_view()),
        ])
        .width(crate::conversation::NAVIGATION)
        .height(Length::Fill)
        .style(move |_| crate::style::fill(rail));
        let base: Element<'_, Message> = row![navigation, self.conversation()]
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
            layers = layers.extend(role);
        }
        if let Some(card) = self.card_view() {
            layers = layers
                .push(outside(Message::CloseProfile, false))
                .push(card);
        }
        // The picker for a dialog's emoji opens over the dialog.
        let mut over_dialogs = None;
        if let Some(picker) = self.picker.get() {
            let shown = picker::view(picker, self.ctx());
            let size = picker.size();
            let place = match &picker.target {
                // Over the emoji button, its right edge level with the button's.
                Target::Composer => {
                    let right = self.chat_right() - 8.0 - 4.0;
                    let bottom = self.composer_buttons_top() - 8.0;
                    Place::new(shown, Point::new(right - size.width, bottom), 8.0)
                        .anchor(Anchor::Bottom)
                }
                // Over the edit's field, at the timeline's right.
                Target::Edit => {
                    let right = self.chat_right() - 16.0;
                    let bottom = self.edit_top().unwrap_or(self.composer_top()) - 8.0;
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
        for layer in self.call_layers() {
            layers = layers.push(layer);
        }
        if let Some(viewer) = self.clip_viewer() {
            layers = layers.push(viewer);
        }
        if let Some(popup) = self.call_popup_view() {
            layers = layers
                .push(outside(Message::Call(crate::call::Event::Close), false))
                .push(popup);
        }
        if let Some(settings) = self.settings.get() {
            layers = layers.push(crate::settings::view(settings, self.ctx()));
        }
        if let Some(settings) = self
            .server_screen
            .get()
            .and_then(|screen| crate::server_settings::view(screen, self.ctx()))
        {
            layers = layers.push(settings);
        }
        if let Some(settings) = self
            .channel_screen
            .get()
            .and_then(|screen| crate::channel_settings::view(screen, self.ctx()))
        {
            layers = layers.push(settings);
        }
        // Over the settings, as a member's from Members.
        if let Some(full) = self.full_profile_view() {
            layers = layers.push(full);
        }
        if let Some(editor) = self.profile_editor_view() {
            layers = layers.push(editor);
        }
        // A mutual server's, over the profile that lists it.
        if let Some(server) =
            crate::info_cards::server_layers(&self.cards, &self.profiles, self.ctx())
        {
            layers = layers.extend(server);
        }
        if let Some(expression) =
            crate::info_cards::expression_layers(&self.cards, &self.profiles, self.ctx())
        {
            layers = layers.extend(expression);
        }
        if let Some(popover) = self.popover_view() {
            layers = layers
                .push(outside(Message::Status(crate::status::Event::Close), false))
                .push(popover);
        }
        if let Some(dialog) = crate::status::dialog_view(&self.status, self.ctx()) {
            layers = layers.push(dialog);
        }
        for dialog in [
            crate::switcher::new_dm_view(&self.quick, self.ctx()),
            crate::switcher::poll_view(&self.quick, self.ctx()),
            crate::switcher::switcher_view(&self.quick, self.ctx()),
            crate::server_dialogs::view(&self.server_dialogs, self.ctx()),
            crate::link_dialogs::view(&self.link_dialogs, self.ctx()),
        ]
        .into_iter()
        .flatten()
        {
            layers = layers.push(dialog);
        }
        if let Some(place) = over_dialogs {
            layers = layers
                .push(outside(Message::Picker(picker::Event::Close), false))
                .push(place);
        }
        if let Some(popover) = self.popover_layer() {
            layers = layers.push(popover);
        }
        if let Some(modal) = self.modal_view() {
            layers = layers.push(modal);
        }
        if let Some(viewer) = crate::image_viewer::view(&self.viewer, self.ctx()) {
            layers = layers.push(viewer);
        }
        if let Some(menu) = self
            .menu
            .as_ref()
            .and_then(|menu| crate::menus::view(menu, self.ctx()))
        {
            layers = layers.push(outside(Message::CloseMenu, false)).push(menu);
        }
        if let Some(window) = crate::reactions::view(&self.reactions, self.ctx()) {
            layers = layers.push(window);
        }
        if let Some(modal) = crate::dialogs::delete_view(self.confirm_focus, self.ctx()) {
            layers = layers.push(modal);
        }
        if let Some(dialog) = crate::person::view(&self.person, self.ctx()) {
            layers = layers.push(dialog);
        }
        if let Some(dialog) = self.add_friend_view() {
            layers = layers.push(dialog);
        }
        if let Some(forward) = crate::forward::view(&self.forward, self.ctx()) {
            layers = layers.push(forward);
        }
        if let Some(leaving) = crate::dialogs::leaving_view(self.leaving.as_deref(), self.ctx()) {
            layers = layers.push(leaving);
        }
        if let Some(drop) = self.drop_view() {
            layers = layers.push(drop);
        }
        if let Some(prompt) = crate::mfa::view(&self.mfa, self.ctx()) {
            layers = layers.push(prompt);
        }
        let content = crate::widgets::RightPress::spot(
            layers.width(Length::Fill).height(Length::Fill),
            &self.pressed,
        );
        let content =
            crate::widgets::CtrlWheel::new(content, zoom_turn, yapfast_core::zoom::Wheel::reset);
        // A middle click scrolls the list under it towards the pointer.
        let theme = &self.theme;
        let arrow = |icon| self.icon(icon, 10, crate::theme::rgb(theme.secondary));
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
        self.frame_layers(content).into()
    }

    /// The window's content under the title bar, when Yapfast draws one,
    /// where it takes the window's place (`self.window` is its size), and
    /// over it the zoom level's note, the inbox, the bar and the edges that
    /// resize the window.
    fn frame_layers<'a>(&'a self, content: impl Into<Element<'a, Message>>) -> Stack<'a, Message> {
        let mut layers = stack![crate::widgets::Below::new(content, self.bar_height())];
        if let Some(note) = self.zoom_note_view() {
            layers = layers.push(note);
        }
        if let Some(inbox) = self.inbox_view() {
            layers = layers.push(inbox);
        }
        if let Some(bar) = self.window_bar() {
            layers = layers.push(column![bar, Space::new().height(Length::Fill)]);
        }
        if let Some(edges) = self.resize_edges() {
            layers = layers.push(edges);
        }
        layers
    }

    /// The zoom level, a moment after it changes, as the Slint client's.
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
                Space::new().height(self.bar_height() + ZOOM_NOTE_TOP),
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
        self.pictures.marks.borrow_mut().clear();
        self.icons.forget();
        self.show_roster(false);
        // Text laid out in the old theme's inks: the rows' again now, the
        // rest as each is next taken in.
        self.recolour(&old);
        self.header.topic = None;
        self.popovers.forget_laid();
        self.forget_found();
        self.forget_guidelines();
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
    step.map(Message::ZoomStep)
}
