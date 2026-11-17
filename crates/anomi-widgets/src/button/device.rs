//! A toggle with a caret beside it that opens its device menu, such as the
//! microphone and its list. The pair shares one ground while the menu is
//! open, so the caret reads as part of the control it configures. The Slint
//! side: `DeviceToggle` (`ui/controls.slint:179`): 52 px wide and 32 tall,
//! the toggle an [`IconButton`](super::icon_button) of radius 4, the caret
//! 20 px with a 16 px chevron that turns over when the menu is open.

use iced::widget::{row, stack};
use iced::{Color, Length};

use anomi_theme::prelude::*;
use anomi_theme::{Motion, Theme, metrics};

use super::face::Face;
use super::icon_button::{Pick, icon_button};
use super::look::Colours;
use crate::icon::Icon;
use crate::tip::Side;

/// The pair's size, and the caret's width and its chevron (`ui/controls.slint:179-218`).
const HEIGHT: f32 = 32.0;
const WIDTH: f32 = 52.0;
const CARET: f32 = 20.0;
const CHEVRON: f32 = 16.0;
/// How far the open pair's ground lifts the surface (`:197`).
const GROUND: f32 = 0.12;
const RADIUS: f32 = metrics::RADIUS_FIELD;
/// The caret's hover fill (`ui/controls.slint:213`).
const HOVER_LIFT: f32 = 0.1;

pub struct DeviceToggle<'a, Message> {
    pub icon: Icon,
    /// Names the toggle in its tooltip.
    pub label: &'a str,
    /// Names the caret in its tooltip.
    pub menu_label: &'a str,
    pub alert: bool,
    pub enabled: bool,
    /// The menu is open.
    pub open: bool,
    pub glyph: f32,
    pub tint: Pick,
    pub on_toggle: Option<Message>,
    pub on_menu: Option<Message>,
    pub motion: Motion,
}

/// A device toggle with its defaults: enabled, closed, a 20 px picture.
pub fn device_toggle<'a, Message>(
    icon: Icon,
    label: &'a str,
    menu_label: &'a str,
    motion: Motion,
) -> DeviceToggle<'a, Message> {
    DeviceToggle {
        icon,
        label,
        menu_label,
        alert: false,
        enabled: true,
        open: false,
        glyph: metrics::ICON_ROW,
        tint: |palette| palette.secondary,
        on_toggle: None,
        on_menu: None,
        motion,
    }
}

impl<'a, Message: Clone + 'a> DeviceToggle<'a, Message> {
    pub fn on_toggle(mut self, message: Message) -> Self {
        self.on_toggle = Some(message);
        self
    }

    pub fn on_menu(mut self, message: Message) -> Self {
        self.on_menu = Some(message);
        self
    }

    pub fn alert(mut self, alert: bool) -> Self {
        self.alert = alert;
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    /// The pair, to place in a row.
    pub fn view(self) -> impl View<Message> + 'a {
        let mut toggle = icon_button(self.icon, self.label, self.motion)
            .alert(self.alert)
            .enabled(self.enabled)
            .radius(RADIUS)
            .glyph(self.glyph)
            .tint(self.tint);
        if let Some(message) = self.on_toggle {
            toggle = toggle.on_press(message);
        }
        let caret = caret(self.menu_label, self.open, self.on_menu, self.motion);
        // The open pair shares one ground, under both halves, easing in and
        // out over `QUICK`; each half lights itself over it.
        stack![ground(self.open, self.motion), row![toggle.view(), caret]]
            .width(Length::Fixed(WIDTH))
            .height(Length::Fixed(HEIGHT))
    }
}

/// The pair's ground: a face that only fills, its `open` flag easing the fill
/// in and out.
fn ground<'a, Message>(open: bool, motion: Motion) -> Face<'a, Message> {
    let mut face = Face::new(motion, |theme: &Theme, _, _, open| {
        let fill = theme.palette().lift(GROUND);
        Colours::new(
            if open { fill } else { Color::TRANSPARENT },
            Color::TRANSPARENT,
        )
    });
    face.min = iced::Size::new(WIDTH, HEIGHT);
    face.fixed = true;
    face.radius = RADIUS;
    face.press = 1.0;
    face.open = open;
    // Nothing to point at: no hover, no press, no focus.
    face.enabled = false;
    face
}

/// The chevron: lit under the pointer and while open, turned over when open.
fn caret<'a, Message>(
    label: &'a str,
    open: bool,
    on_press: Option<Message>,
    motion: Motion,
) -> impl View<Message> + 'a
where
    Message: Clone + 'a,
{
    let mut face = Face::new(motion, move |theme: &Theme, hovered, _, open| {
        let palette = theme.palette();
        let fill = if hovered {
            palette.lift(HOVER_LIFT)
        } else {
            Color::TRANSPARENT
        };
        Colours::new(
            fill,
            if hovered || open {
                palette.text
            } else {
                palette.secondary
            },
        )
    });
    face.leading = Some(Icon::ChevronDown);
    face.glyph = CHEVRON;
    face.min = iced::Size::new(CARET, HEIGHT);
    face.fixed = true;
    face.radius = RADIUS;
    face.press = 1.0;
    face.open = open;
    face.turns = true;
    face.on_press = on_press;
    // The tooltip hides while the menu it would name is open.
    face.tipped(if open { "" } else { label }, Side::Above)
}
