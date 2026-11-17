//! An emoji as a button (`EmojiButton`, `ui/messages.slint:179`): the emoji
//! on a tile with 6 px corners that lifts under the pointer, greyed with a
//! lock over its corner when this account may not use it, its name in a
//! tooltip above it, and in the picker a star in its other corner that
//! keeps it among the favourites, shown while the pointer is over it or
//! while it is one. The grid's are 56 px with 36 px emoji and no tooltip
//! (`:1374`-`:1392`); a message's menu shows quick reactions with it too
//! (`:1514`), 44 px on the field's ground.
//!
//! iced's button takes no keyboard focus at this pin, so Tab doesn't reach
//! these; Enter in the picker's search picks its first match.

use iced::widget::{button, container, hover, mouse_area, stack, svg};
use iced::{Background, Border, Color, Fill, Padding, Size};

use anomi_theme::prelude::*;
use anomi_vm::chat::emoji_picker::{Emoji, Intent};
use anomi_widgets::icon::{Icon, icon};
use anomi_widgets::tip::{Side, tip};

use super::face;

/// The button's and its emoji's sizes unless given (`:182`-`:183`).
const SIZE: f32 = 32.0;
const GLYPH: f32 = 20.0;
/// The tile's corners and its lift under the pointer (`:203`-`:205`).
const RADIUS: f32 = 6.0;
const LIFT: f32 = 0.12;
/// A locked emoji's opacity (`:212`).
const LOCKED: f32 = 0.4;
/// The lock over its corner: 14 px, 4 px in (`:216`-`:218`).
const LOCK: f32 = 14.0;
const LOCK_INSET: f32 = 4.0;
/// The star's chip: 18 px with 4 px corners on the panel's ground, its
/// star 12 px (`:249`-`:257`).
const CHIP: f32 = 18.0;
const CHIP_RADIUS: f32 = 4.0;
const STAR: f32 = 12.0;

/// What pressing `emoji` in the picker asks for: it, or Nitro for a locked
/// one.
pub fn press(emoji: &Emoji) -> Intent {
    if emoji.usable() {
        Intent::Pick(emoji.clone())
    } else {
        Intent::Locked
    }
}

/// A ground's colour from the theme.
type Ground<'a> = Box<dyn Fn(&Theme) -> Color + 'a>;

/// An emoji's button; see [`emoji_button`].
pub struct EmojiButton<'a, M> {
    emoji: &'a Emoji,
    size: f32,
    glyph: f32,
    fill: Option<Ground<'a>>,
    tip: Option<&'a str>,
    /// Whether it's a favourite, and what its star sends.
    star: Option<(bool, M)>,
    on_press: Option<M>,
    on_locked: Option<M>,
    on_hover: Option<M>,
    motion: Motion,
}

/// `emoji` on a 32 px button, 20 px across, its custom picture moving
/// while `motion` plays.
pub fn emoji_button<'a, M>(emoji: &'a Emoji, motion: Motion) -> EmojiButton<'a, M> {
    EmojiButton {
        emoji,
        size: SIZE,
        glyph: GLYPH,
        fill: None,
        tip: None,
        star: None,
        on_press: None,
        on_locked: None,
        on_hover: None,
        motion,
    }
}

impl<'a, M: Clone + 'a> EmojiButton<'a, M> {
    /// The button's side (`size`).
    pub fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    /// The emoji's side (`glyph`).
    pub fn glyph(mut self, glyph: f32) -> Self {
        self.glyph = glyph;
        self
    }

    /// Its ground at rest (`fill`), as the menu's on the field's colour.
    pub fn fill(mut self, fill: impl Fn(&Theme) -> Color + 'a) -> Self {
        self.fill = Some(Box::new(fill));
        self
    }

    /// Its name above it while the pointer is on it (`tip`).
    pub fn tip(mut self, name: &'a str) -> Self {
        self.tip = Some(name);
        self
    }

    /// The star of the picker's emoji, filled on a `favorite`, which sends
    /// `on_favorite` (`favoritable`, `favorite`, `favorited`).
    pub fn star(mut self, favorite: bool, on_favorite: M) -> Self {
        self.star = Some((favorite, on_favorite));
        self
    }

    /// Sent by a press, when it may be used (`clicked`).
    pub fn on_press(mut self, message: M) -> Self {
        self.on_press = Some(message);
        self
    }

    /// Sent by a press when it's locked (`locked`).
    pub fn on_locked(mut self, message: M) -> Self {
        self.on_locked = Some(message);
        self
    }

    /// Sent when the pointer comes over it (`hovered`).
    pub fn on_hover(mut self, message: M) -> Self {
        self.on_hover = Some(message);
        self
    }

    pub fn view(self) -> Element<'a, M> {
        let usable = self.emoji.usable();
        let fade = if usable { 1.0 } else { LOCKED };
        let room = Size::new(self.size, self.size);
        let face = face::emoji(self.emoji, self.glyph, room, fade, self.motion);
        let content: Element<'a, M> = if usable {
            face
        } else {
            stack![face, lock(fade)].boxed()
        };
        let fill = self.fill;
        let mut tile = button(content)
            .padding(0)
            .width(self.size)
            .height(self.size)
            .style(move |theme: &Theme, status| {
                let ground = fill.as_ref().map(|fill| fill(theme));
                tile(theme, status, ground, fade)
            });
        if let Some(message) = if usable {
            self.on_press
        } else {
            self.on_locked
        } {
            tile = tile.on_press(message);
        }
        let tile: Element<'a, M> = match self.tip {
            Some(name) => tip(tile, name, Side::Above, self.motion).boxed(),
            None => tile.boxed(),
        };
        let body: Element<'a, M> = match self.star {
            Some((true, message)) => stack![tile, star(true, fade, message)].boxed(),
            Some((false, message)) => hover(tile, star(false, fade, message)).boxed(),
            None => tile,
        };
        match self.on_hover {
            Some(message) => mouse_area(body).on_enter(message).boxed(),
            None => body,
        }
    }
}

/// The tile: its `ground` at rest, the overlay under the pointer, at
/// `fade`.
fn tile(theme: &Theme, status: button::Status, ground: Option<Color>, fade: f32) -> button::Style {
    let palette = theme.palette();
    let lifted = matches!(status, button::Status::Hovered | button::Status::Pressed);
    let ground = if lifted {
        Some(palette.lift(LIFT))
    } else {
        ground
    };
    button::Style {
        background: ground.map(|ground| Background::Color(ground.scale_alpha(fade))),
        text_color: palette.text,
        border: Border::default().rounded(RADIUS),
        ..button::Style::default()
    }
}

/// `color` at `fade` over the picker's ground: an icon's colour, which
/// tiny-skia draws without its alpha.
fn faded(theme: &Theme, color: Color, fade: f32) -> Color {
    let ground = theme.palette().raised;
    let blend = |over: f32, under: f32| over * fade + under * (1.0 - fade);
    Color::from_rgb(
        blend(color.r, ground.r),
        blend(color.g, ground.g),
        blend(color.b, ground.b),
    )
}

/// The lock over a locked emoji's corner, in the text colour at `fade`.
fn lock<'a, M: 'a>(fade: f32) -> Element<'a, M> {
    let glyph = icon(Icon::Lock, LOCK).style(move |theme: &Theme, _| svg::Style {
        color: Some(faded(theme, theme.palette().text, fade)),
    });
    container(glyph)
        .padding(LOCK_INSET)
        .align_right(Fill)
        .align_bottom(Fill)
        .boxed()
}

/// The star in the tile's corner: filled gold on a favourite, an outline
/// in the text colour otherwise; a press sends `message`.
fn star<'a, M: Clone + 'a>(favorite: bool, fade: f32, message: M) -> Element<'a, M> {
    let glyph = if favorite {
        Icon::StarFilled
    } else {
        Icon::Star
    };
    let glyph = icon(glyph, STAR).style(move |theme: &Theme, _| {
        let ink = if favorite {
            face::GOLD
        } else {
            theme.palette().text
        };
        svg::Style {
            color: Some(faded(theme, ink, fade)),
        }
    });
    let chip = button(container(glyph).center(CHIP))
        .on_press(message)
        .padding(Padding::ZERO)
        .width(CHIP)
        .height(CHIP)
        .style(move |theme: &Theme, _| button::Style {
            background: Some(Background::Color(theme.palette().panel.scale_alpha(fade))),
            border: Border::default().rounded(CHIP_RADIUS),
            ..button::Style::default()
        });
    container(chip).align_right(Fill).boxed()
}
