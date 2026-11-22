//! The layers the view stacks: the window itself, then whatever is open
//! over it (the profile card, mention suggestions, the emoji picker, a
//! menu, the modal), each with a backdrop that takes a click outside; and
//! the theme it's drawn in.

use iced::{
    Element, Length, Point,
    widget::{Space, button, column, container, row, stack, text},
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
        let rail_side = self
            .themes
            .package_layout(&self.theme.id)
            .and_then(|layout| layout.rail);
        let rail_hidden = matches!(
            rail_side,
            Some(
                yapfast_core::themes::layout::RailSide::Hidden
                    | yapfast_core::themes::layout::RailSide::Top
                    | yapfast_core::themes::layout::RailSide::Tree
            )
        );
        let navigation_right =
            crate::conversation::navigation_right(self.themes.package_layout(&self.theme.id));
        // A theme whose glass has a gap floats its panes apart over the
        // window's art, as the Slint build does: the navigation and the
        // conversation `gap` from each other and the window's edge, and a
        // glass account card floating `gap` inside the navigation's foot.
        let gap = self.themes.glass_gap(&self.theme.id);
        let paned = gap > 0.0;
        let glass_account = paned
            && self
                .themes
                .glass(
                    &self.theme.id,
                    yapfast_core::themes::package::art::ArtSurface::Account,
                )
                .is_some();
        let has_dock = crate::call::dock::view(self.ctx()).is_some();
        // Room at the foot of the rail and the sidebar for the floating card.
        let foot_room = if glass_account && !has_dock {
            crate::account::HEIGHT + gap * 2.0
        } else {
            0.0
        };
        let rail_view: Element<'_, Message> = if rail_hidden {
            Space::new().width(0).into()
        } else {
            let rail = crate::theme_art::surface(
                container(crate::rail::view(&self.rail, self.ctx()))
                    .padding(iced::Padding::ZERO.bottom(foot_room)),
                self.ctx(),
                yapfast_core::themes::package::art::ArtSurface::Rail,
            );
            let rail = crate::theme_art::outlined(
                rail,
                self.ctx(),
                yapfast_core::themes::layout::BorderPlace::Panes,
                "rail",
                0.0,
                1.0,
                self.theme.outline,
            );
            container(rail)
                .width(crate::rail::resolved_width(
                    self.themes.package_layout(&self.theme.id),
                    self.themes.chrome(&self.theme.id),
                ))
                .into()
        };
        let mut rail_view = Some(rail_view);
        let sidebar_content: Element<'_, Message> =
            if rail_side == Some(yapfast_core::themes::layout::RailSide::Tree) {
                column![
                    crate::sidebar::header_view(&self.sidebar, self.ctx()),
                    crate::sidebar::tree_view(&self.sidebar, self.ctx()),
                ]
                .height(Length::Fill)
                .into()
            } else {
                crate::sidebar::view(&self.sidebar, self.ctx())
            };
        let sidebar_content = crate::theme_chrome::boxed(
            sidebar_content,
            self.ctx(),
            yapfast_core::themes::chrome::Pane::Sidebar,
            &self.sidebar.header.name,
            None,
        );
        let sidebar_content = crate::theme_art::surface(
            sidebar_content,
            self.ctx(),
            yapfast_core::themes::package::art::ArtSurface::Sidebar,
        );
        let sidebar_content = crate::theme_art::outlined(
            sidebar_content,
            self.ctx(),
            yapfast_core::themes::layout::BorderPlace::Panes,
            "sidebar",
            0.0,
            1.0,
            self.theme.outline,
        );
        // Under a theme, Discord's outline round the sidebar's top and left,
        // as the Slint client draws it (in the theme's outline and weight).
        let sidebar_content = match self.themes.package_layout(&self.theme.id) {
            // The sidebar's own top corner is 12 px round, as Slint's.
            Some(layout) => crate::theme_art::top_left_edges(
                sidebar_content,
                self.theme.outline,
                layout.outline_width.unwrap_or(1.0),
                self.theme.radius(12.0),
            ),
            _ => sidebar_content,
        };
        let sidebar_view =
            container(sidebar_content).width(crate::sidebar::width(&self.layout, &self.theme));
        let account: Element<'_, Message> = container(crate::theme_art::surface(
            crate::account::view(self.ctx()),
            self.ctx(),
            yapfast_core::themes::package::art::ArtSurface::Account,
        ))
        .height(crate::account::HEIGHT)
        .into();
        let account = if glass_account {
            container(crate::theme_art::pane(account, self.ctx(), None))
                .padding(iced::Padding {
                    top: gap,
                    right: gap,
                    bottom: gap,
                    left: gap,
                })
                .into()
        } else {
            account
        };
        let navigation_inner: Element<'_, Message> = if foot_room > 0.0 {
            // The rail runs the navigation's full height and the card floats
            // over its foot, across the rail and the sidebar.
            let sidebar_view = column![sidebar_view, Space::new().height(foot_room)];
            let navigation_body = if navigation_right {
                row![sidebar_view]
            } else {
                row![
                    rail_view
                        .take()
                        .unwrap_or_else(|| Space::new().width(0).into()),
                    sidebar_view
                ]
            }
            .height(Length::Fill);
            stack![
                navigation_body,
                column![Space::new().height(Length::Fill), account]
            ]
            .into()
        } else {
            let navigation_body = if navigation_right {
                row![sidebar_view]
            } else {
                row![
                    rail_view
                        .take()
                        .unwrap_or_else(|| Space::new().width(0).into()),
                    sidebar_view
                ]
            }
            .height(Length::Fill);
            column![navigation_body, foot.push(account)].into()
        };
        let navigation_inner = if paned {
            crate::theme_art::pane(navigation_inner, self.ctx(), None)
        } else {
            navigation_inner
        };
        let rail_ground = if paned
            && self
                .themes
                .glass(
                    &self.theme.id,
                    yapfast_core::themes::package::art::ArtSurface::Rail,
                )
                .is_some()
        {
            iced::Color::TRANSPARENT
        } else {
            rail
        };
        let navigation_width = crate::conversation::themed_navigation(
            &self.layout,
            &self.theme,
            self.themes.package_layout(&self.theme.id),
            self.themes.chrome(&self.theme.id),
        ) - crate::conversation::right_rail(
            self.themes.package_layout(&self.theme.id),
            self.themes.chrome(&self.theme.id),
        );
        // A floating navigation pane's ground rounds the pane's corners, and
        // without the rail in it, it's the sidebar's paint: the account card
        // under the list then shows the list's ground round its own corners,
        // not a square of the rail's.
        let navigation_style = if paned {
            let radius = self
                .themes
                .glass_style(&self.theme.id)
                .map_or(0.0, |style| style.radius);
            let ground: iced::Background = if navigation_right {
                self.theme.surface_paint("sidebar")
            } else {
                rail_ground.into()
            };
            iced::widget::container::Style {
                background: Some(ground),
                border: iced::Border::default().rounded(radius),
                ..Default::default()
            }
        } else {
            crate::style::fill(rail_ground)
        };
        let navigation = container(navigation_inner)
            .width(navigation_width)
            .height(Length::Fill)
            .style(move |_| navigation_style);
        // The sidebar's edge drags it wider or narrower.
        let splitter = crate::widgets::Splitter::new(
            iced::Color::TRANSPARENT,
            self.theme.line,
            self.theme.faint,
            move |by| Message::Side(crate::sidebar::Event::Resize(by)),
            Message::Side(crate::sidebar::Event::Resized),
            Message::Side(crate::sidebar::Event::ResetWidth),
        );
        let splitter = container(splitter)
            .width(navigation_width)
            .align_x(iced::Alignment::End);
        let navigation = stack![navigation, splitter];
        // The chat's ground, art and glass under the whole conversation,
        // header and member list included, as in the Slint build.
        let conversation = crate::theme_art::outlined(
            crate::theme_art::surface(
                crate::conversation::conversation(self),
                self.ctx(),
                yapfast_core::themes::package::art::ArtSurface::Chat,
            ),
            self.ctx(),
            yapfast_core::themes::layout::BorderPlace::Panes,
            "chat",
            0.0,
            1.0,
            self.theme.outline_chat,
        );
        let conversation = if paned {
            crate::theme_art::pane(conversation, self.ctx(), None)
        } else {
            conversation
        };
        let main: Element<'_, Message> = row![navigation, conversation]
            .spacing(gap)
            .padding(gap)
            .width(Length::Fill)
            .height(Length::Fill)
            .into();
        // A theme's rail on the right runs down the window's right edge,
        // beside the floating panes; the sidebar stays on the left.
        let main: Element<'_, Message> = match rail_view.take() {
            // In a theme whose panes float, the rail is a pane of its own
            // `gap` from the window's edge, as Slint lays it.
            Some(rail) if navigation_right && paned => row![
                main,
                container(crate::theme_art::pane(rail, self.ctx(), None)).padding(iced::Padding {
                    top: gap,
                    right: gap,
                    bottom: gap,
                    left: 0.0,
                })
            ]
            .width(Length::Fill)
            .height(Length::Fill)
            .into(),
            Some(rail) if navigation_right => row![main, rail]
                .width(Length::Fill)
                .height(Length::Fill)
                .into(),
            _ => main,
        };
        let base: Element<'_, Message> = if rail_side
            == Some(yapfast_core::themes::layout::RailSide::Top)
        {
            let top = crate::theme_art::surface(
                crate::rail::top_view(&self.rail, self.ctx()),
                self.ctx(),
                yapfast_core::themes::package::art::ArtSurface::Rail,
            );
            let top = crate::theme_art::outlined(
                top,
                self.ctx(),
                yapfast_core::themes::layout::BorderPlace::Panes,
                "rail",
                0.0,
                1.0,
                self.theme.outline,
            );
            let top = container(top)
                .width(Length::Fill)
                .height(crate::rail::themed_top_height(
                    self.themes.package_layout(&self.theme.id),
                ));
            column![top, main]
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
        } else {
            main
        };
        let base: Element<'_, Message> = match theme_topbar(self) {
            Some(bar) => column![bar, base]
                .width(Length::Fill)
                .height(Length::Fill)
                .into(),
            None => base,
        };
        let base = crate::theme_art::surface(
            base,
            self.ctx(),
            yapfast_core::themes::package::art::ArtSurface::Window,
        );
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
        // A theme on trial's banner, over everything as Slint's is.
        if let Some(banner) = crate::settings::trial_banner(self.ctx()) {
            layers = layers.push(banner);
        }
        // While the keys are on the messages a press says so, for one off
        // the list to take them (`timeline::keys::follow_press`).
        let layers: Element<'_, Message> = layers.width(Length::Fill).height(Length::Fill).into();
        let layers = crate::theme_effects::window_tint(
            crate::theme_art::overlays(layers, self.ctx()),
            self.ctx(),
        );
        let content = crate::widgets::RightPress::spot(layers, &self.pressed).telling(
            (self.keyed.is_some() || self.sidebar.tree_cursor.is_some())
                .then_some(Message::Nothing),
        );
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
        let surface = yapfast_core::themes::package::art::ArtSurface::Toast;
        let decorated =
            self.themes.decorates(&theme.id, surface) || theme.package_paint("raised").is_some();
        let tip = if decorated {
            iced::Color::TRANSPARENT
        } else {
            theme.tooltip
        };
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
            border: theme.surface_border("toast", 18.0, 0.0, iced::Color::TRANSPARENT),
            shadow: theme.surface_shadow("toast", crate::style::raised_shadow()),
            ..crate::style::rounded(tip, theme.surface_radius("toast", 18.0))
        });
        let note = crate::theme_art::surface(note, self.ctx(), surface);
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
    /// keeps the choice, as Appearance and Themes do. Returns whether there's such a
    /// theme. Pictures are rebuilt because an installed theme may opt into
    /// recolouring or dithering them; what the client drew in the old colours
    /// (icons, dots and styled names) is drawn again too.
    pub fn choose_theme(&mut self, id: &str) -> bool {
        let Some(theme) = self.themes.find(id) else {
            return false;
        };
        self.themes.choose(self.client.root(), &theme.id);
        self.show_theme(theme);
        self.theme_applied();
        true
    }

    /// Feedback for deliberately applying a theme. Live previews, option
    /// sliders, readability changes and cache refreshes use
    /// [`Self::show_theme`] without replaying this cue or particle burst.
    pub(crate) fn theme_applied(&self) {
        self.cue(yapfast_core::themes::media::SoundSlot::ThemeApplied);
        self.themes.particle_event(
            &self.theme.id,
            yapfast_core::themes::package::art::Trigger::ThemeApplied,
            None,
        );
    }

    /// Whether a protected screen is up: sign-in, a link's confirmation
    /// (leaving, OAuth, a gift) or a theme's review. While one is, the window
    /// shows the stock look with no theme's palette, faces, icons, art,
    /// overlays, tint or particles, so a theme can't dress up or paint over
    /// what it asks the person to judge.
    pub fn protected(&self) -> bool {
        let state = self.client.state();
        !crate::signin::workspace(self)
            || self.leaving.is_some()
            || state.oauth.is_some()
            || state.gift.is_some()
            || self
                .win
                .settings
                .get()
                .is_some_and(crate::settings::Settings::protected)
    }

    /// The theme the window should show now: the stock look on a protected
    /// screen, else what Settings previews, else the theme chosen or tried.
    pub fn wanted_theme(&self) -> Theme {
        if self.protected() {
            return crate::themes::stock_theme();
        }
        self.win
            .settings
            .get()
            .and_then(crate::settings::Settings::preview)
            .cloned()
            .unwrap_or_else(|| self.themes.shown())
    }

    /// Shows [`Self::wanted_theme`].
    pub fn show_wanted_theme(&mut self) {
        self.show_theme(self.wanted_theme());
    }

    /// Shows the stock look when a protected screen comes up and the
    /// theme again when it goes. Whether the look changed.
    pub(crate) fn follow_protected(&mut self) -> bool {
        let protected = self.protected();
        if protected == self.shown_protected {
            return false;
        }
        self.shown_protected = protected;
        self.show_wanted_theme();
        true
    }

    /// Shows the window in `theme` without keeping it, as the theme
    /// editor shows what's edited.
    pub fn show_theme(&mut self, theme: Theme) {
        self.win.icons.replace(self.themes.icons(&theme.id));
        self.themes.apply_sounds(&theme.id);
        self.accent_started = std::time::Instant::now();
        // A reinstalled package keeps its id: take its cues afresh.
        crate::themes::forget_synced_cues();
        self.sync_cues();
        // Content media and Performance can change without changing the
        // palette-valued Theme. Converted stills, fitter jobs and animation
        // players must all restart under the newly resolved content look.
        self.win.pictures.forget();
        // Companion metrics are deliberately kept beside the kit Theme so
        // its public shape stays Wired's. Two resolutions can therefore
        // compare equal here while changing fonts, spacing, shapes, layout
        // or art. Always invalidate laid-out/cached surfaces after resolving
        // a theme; palette equality is not theme-resolution equality.
        crate::fonts::set_default_family(body_family(&theme));
        let _old = std::mem::replace(&mut self.theme, theme);
        self.show_roster(false);
        // A companion can change faces, type sizes, row spacing, avatar
        // geometry or message layout while every palette field stays the
        // same. Re-measure both conversations instead of merely recolouring
        // the cached text from `old`.
        let none = super::Messages::default();
        self.measure(&none, true);
        crate::thread::theme_changed(self);
        self.dirty = true;
        self.header.topic = None;
        self.popovers.forget_laid();
        crate::search::forget_found(self);
        crate::forum::forget_guidelines(self);
        self.drafts.starter = None;
    }
}

/// A package's tiling-window-manager-style bar. It uses the existing rail
/// navigation messages, so its numbered workspaces are fully usable even
/// when the rest of Wired's layout remains stock.
fn theme_topbar(app: &App) -> Option<Element<'_, Message>> {
    use yapfast_core::{application::Navigation, themes::chrome};

    let chrome = app.themes.chrome(&app.theme.id)?;
    let bar = chrome.topbar.as_ref()?;
    let state = app.client.state();
    let current_server = match &state.navigation {
        Navigation::Server(id) => Some(id.as_str()),
        _ => None,
    };
    let server_name = current_server
        .and_then(|id| state.servers.iter().find(|server| server.id == id))
        .map_or("", |server| server.name.as_str());
    let now_playing = state
        .discord_playing
        .as_ref()
        .map_or("", |playing| playing.name.as_str());
    let time = yapfast_core::clock::time_on(
        yapfast_core::clock::now_ms() / 1000,
        app.layout.prefs.twelve_hour_clock,
    );
    let values = [
        ("name", app.account.name.as_str()),
        ("server", server_name),
        ("channel", app.header.name.as_str()),
        ("now_playing", now_playing),
        ("time", time.as_str()),
    ];
    let centre = chrome::fill(&bar.centre, &values);
    let right = chrome::fill(&bar.right, &values);
    let ground = bar.ground.map_or(app.theme.rail, crate::themes::argb);
    let muted = bar.text.map_or(app.theme.muted, crate::themes::argb);
    let current = bar.current.map_or(app.theme.text, crate::themes::argb);
    let underline = bar.underline.map_or(app.theme.accent, crate::themes::argb);
    let line = bar.line.map_or(app.theme.line, crate::themes::argb);
    let mut workspaces = row![].spacing(2).align_y(iced::Alignment::Center);
    if let Some(home) = bar.home.as_deref() {
        let selected = matches!(state.navigation, Navigation::DirectMessages);
        workspaces = workspaces.push(topbar_workspace(
            home.to_owned(),
            bar.size,
            if selected { current } else { muted },
            underline,
            selected,
            Message::Rail(crate::rail::Event::Home),
            muted,
            current,
        ));
    }
    let spaces = topbar_server_workspaces(&state.servers, current_server, bar.from);
    for (index, (server, selected)) in spaces.into_iter().take(bar.workspaces as usize).enumerate()
    {
        workspaces = workspaces.push(topbar_workspace(
            (index + 1).to_string(),
            bar.size,
            if selected { current } else { muted },
            underline,
            selected,
            Message::Rail(crate::rail::Event::Server(server)),
            muted,
            current,
        ));
    }
    // As in Slint: the workspaces at the left, the centre label in the
    // window's middle, the right label at its right.
    let font = crate::theme_chrome::mono(app.ctx());
    Some(
        container(stack![
            row![
                workspaces,
                Space::new().width(Length::Fill),
                text(right).size(bar.size).font(font).color(muted)
            ]
            .spacing(10)
            .height(Length::Fill)
            .align_y(iced::Alignment::Center),
            container(text(centre).size(bar.size).font(font).color(muted))
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(iced::Alignment::Center)
                .align_y(iced::Alignment::Center),
        ])
        .height(bar.height)
        .padding([0, 8])
        .style(move |_| iced::widget::container::Style {
            background: Some(ground.into()),
            border: iced::Border {
                color: line,
                width: 1.0,
                radius: 0.0.into(),
            },
            ..Default::default()
        })
        .into(),
    )
}

/// Server ids represented by the top bar, in the rail's order. Folder
/// workspaces collapse each contiguous folder to the selected server in it,
/// or its first server when none is selected.
fn topbar_server_workspaces(
    servers: &[std::sync::Arc<yapfast_core::chat::ChatServer>],
    current: Option<&str>,
    from: yapfast_core::themes::chrome::Workspaces,
) -> Vec<(String, bool)> {
    let mut spaces = Vec::new();
    match from {
        yapfast_core::themes::chrome::Workspaces::Servers => {
            spaces.extend(
                servers
                    .iter()
                    .map(|server| (server.id.clone(), current == Some(server.id.as_str()))),
            );
        }
        yapfast_core::themes::chrome::Workspaces::Folders => {
            let mut servers = servers.iter().peekable();
            while let Some(server) = servers.next() {
                let Some(folder) = &server.folder else {
                    spaces.push((server.id.clone(), current == Some(server.id.as_str())));
                    continue;
                };
                let mut target = server;
                let mut selected = current == Some(server.id.as_str());
                while let Some(next) = servers.next_if(|next| next.folder.as_ref() == Some(folder))
                {
                    if current == Some(next.id.as_str()) {
                        target = next;
                        selected = true;
                    }
                }
                spaces.push((target.id.clone(), selected));
            }
        }
    }
    spaces
}

/// One top-bar workspace, with the current workspace's checked underline
/// colour. The action remains the rail's ordinary navigation message.
#[allow(
    clippy::too_many_arguments,
    reason = "each part it draws is its own input"
)]
fn topbar_workspace<'a>(
    words: String,
    size: f32,
    ink: iced::Color,
    underline: iced::Color,
    selected: bool,
    message: Message,
    rest: iced::Color,
    lit: iced::Color,
) -> Element<'a, Message> {
    // The underline as wide as the words (a Fill bar would widen the
    // workspace to the whole bar).
    let rule: Element<'a, Message> = if selected {
        crate::controls::bar(
            underline,
            Length::Fixed(words.chars().count() as f32 * size * 0.62 + 2.0),
            2,
        )
        .into()
    } else {
        Space::new().height(2).into()
    };
    button(
        column![text(words).size(size).color(ink), rule]
            .spacing(1)
            .align_x(iced::Alignment::Center),
    )
    .padding([1, 6])
    .on_press(message)
    .style(crate::controls::ink_style(rest, lit))
    .into()
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
/// A theme's body face, which Slint makes the default face of every text
/// that names none (`default-font-family`); `None` for the app's own.
pub fn body_family(theme: &crate::theme::Theme) -> Option<&'static str> {
    match theme.font("body", crate::fonts::REGULAR).family {
        iced::font::Family::Name(name) => Some(name),
        _ => None,
    }
}

#[cfg(test)]
mod chrome_tests {
    use super::*;
    use yapfast_core::{
        chat::{ChatServer, ServerFolder},
        themes::chrome::Workspaces,
    };

    fn server(id: &str, folder: Option<(&str, &str)>) -> ChatServer {
        ChatServer {
            id: id.into(),
            name: id.into(),
            folder: folder.map(|(id, name)| ServerFolder {
                id: id.into(),
                name: Some(name.into()),
                color: None,
            }),
            ..ChatServer::default()
        }
    }

    #[test]
    fn folder_workspaces_choose_the_open_server_and_keep_top_level_order() {
        let servers = [
            std::sync::Arc::new(server("a", Some(("folder", "Folder")))),
            std::sync::Arc::new(server("b", Some(("folder", "Folder")))),
            std::sync::Arc::new(server("c", None)),
        ];
        assert_eq!(
            topbar_server_workspaces(&servers, Some("b"), Workspaces::Folders),
            [("b".into(), true), ("c".into(), false)]
        );
        assert_eq!(
            topbar_server_workspaces(&servers, Some("b"), Workspaces::Servers),
            [("a".into(), false), ("b".into(), true), ("c".into(), false)]
        );
    }
}
