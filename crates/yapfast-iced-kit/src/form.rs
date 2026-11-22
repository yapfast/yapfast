//! The form kit: what Settings' pages, and the editors and server settings
//! after them, are built of. A page is a column of groups ([`page`]); a
//! group is a [`heading`], a control and a [`note`] under it ([`group`]).
//! The controls: a [`Switch`] (a label, what it does, and the toggle at its
//! end, or a lock with why), a row of choices (`controls::choices`), a
//! [`Dropdown`], a slider (`controls::Slider`), a text field ([`field`]), a
//! key's [`capture`], a [`readout`] of a value set elsewhere, and a
//! colour's row ([`colour_row`]). Every colour is the theme's and every
//! size one of the constants here.

use crate::widgets::scroll_view;
use std::borrow::Cow;

use iced::{
    Alignment, Border, Color, Element, Length, Task,
    widget::{Space, button, column, container, row, text, text_input},
};

use crate::look::Look;
use crate::{
    controls::{self, Check, Item, Light, lights},
    fonts::{BOLD, SEMIBOLD},
    icons::Icon,
    popover::{Popover, Side, Width},
    style::rounded,
    theme::Theme,
};

/// A page's column: at most this wide, centred in the room there is with at
/// least [`PAGE_SIDE`] each side, [`PAGE_TOP`] above its title.
pub const COLUMN: f32 = 696.0;
pub const PAGE_SIDE: f32 = 24.0;
pub const PAGE_TOP: f32 = 64.0;
pub const PAGE_BOTTOM: f32 = 56.0;
/// Between a page's groups, and between the parts of a group.
pub const GAP: f32 = 16.0;
pub const TIGHT: f32 = 8.0;
/// A switch's toggle and the knob in it.
pub const TOGGLE_WIDTH: f32 = 48.0;
pub const TOGGLE_HEIGHT: f32 = 24.0;
pub const KNOB: f32 = 18.0;
/// A box's height: a readout, a key's capture; and a dropdown's.
pub const BOX: f32 = 36.0;
pub const DROPDOWN: f32 = 32.0;
/// A dropdown's list at its tallest.
const LIST: f32 = 320.0;

/// A page's title: 24 px bold.
pub fn title<'a, M: Clone + 'a>(
    theme: &Theme,
    label: impl text::IntoFragment<'a>,
) -> Element<'a, M> {
    text(label).size(24).font(BOLD).color(theme.text).into()
}

/// A group's heading: 12 px bold capitals in the muted ink.
pub fn heading<'a, M: Clone + 'a>(
    theme: &Theme,
    label: impl text::IntoFragment<'a>,
) -> Element<'a, M> {
    text(label)
        .size(12)
        .font(BOLD)
        .color(theme.muted)
        .wrapping(text::Wrapping::None)
        .into()
}

/// What a control does, under it: 13 px in the muted ink, wrapped.
pub fn note<'a, M: Clone + 'a>(
    theme: &Theme,
    words: impl text::IntoFragment<'a>,
) -> Element<'a, M> {
    note_in(words, theme.muted)
}

/// A [`note`] in another ink: an error's, the accent's.
pub fn note_in<'a, M: Clone + 'a>(
    words: impl text::IntoFragment<'a>,
    ink: Color,
) -> Element<'a, M> {
    text(words).size(13).color(ink).width(Length::Fill).into()
}

/// What went wrong, on a form or a dialog: 14 px in the danger's ink.
pub fn error<'a>(theme: &Theme, words: impl text::IntoFragment<'a>) -> iced::widget::Text<'a> {
    text(words).size(14).color(theme.danger)
}

/// The line between a page's parts.
pub fn rule<'a, M: Clone + 'a>(theme: &Theme) -> Element<'a, M> {
    controls::bar(theme.line, Length::Fill, 1).into()
}

/// A group: its parts [`TIGHT`] apart.
pub fn group<'a, M: Clone + 'a>(parts: impl IntoIterator<Item = Element<'a, M>>) -> Element<'a, M> {
    column(parts).spacing(TIGHT).width(Length::Fill).into()
}

/// A page: its title and groups [`GAP`] apart.
pub fn page<'a, M: Clone + 'a>(parts: impl IntoIterator<Item = Element<'a, M>>) -> Element<'a, M> {
    column(parts).spacing(GAP).width(Length::Fill).into()
}

/// A row of buttons and the like, `TIGHT` apart, from the start.
pub fn buttons<'a, M: Clone + 'a>(
    parts: impl IntoIterator<Item = Element<'a, M>>,
) -> Element<'a, M> {
    row(parts).spacing(TIGHT).align_y(Alignment::Center).into()
}

/// Something beside a thing that stretches: a label and its button.
pub fn beside<'a, M: Clone + 'a>(
    stretched: impl Into<Element<'a, M>>,
    end: impl Into<Element<'a, M>>,
) -> Element<'a, M> {
    row![container(stretched).width(Length::Fill), end.into()]
        .spacing(12)
        .align_y(Alignment::Center)
        .into()
}

/// Two columns that take half the width each, whatever is in them, as
/// Voice's devices sit.
pub fn halves<'a, M: Clone + 'a>(
    left: impl Into<Element<'a, M>>,
    right: impl Into<Element<'a, M>>,
) -> Element<'a, M> {
    row![
        container(left).width(Length::FillPortion(1)),
        container(right).width(Length::FillPortion(1))
    ]
    .spacing(GAP)
    .into()
}

/// A switch: its label (15 px) with what it does under it, and the toggle
/// at the row's end. A click anywhere on the row flips it. Without a
/// message it can't change, greyed, with a lock after its label saying why
/// (`locked`).
pub struct Switch<'a, M> {
    label: Cow<'a, str>,
    description: Option<Cow<'a, str>>,
    on: bool,
    message: Option<M>,
    locked: Option<&'a str>,
    fresh: bool,
}

/// The small NEW that follows a new setting's name: the accent's pill, 16
/// px tall.
pub fn new_tag<'a, M: Clone + 'a>(theme: &crate::theme::Theme) -> Element<'a, M> {
    let accent = theme.accent;
    container(
        text("NEW")
            .size(10)
            .font(crate::fonts::BOLD)
            .color(theme.on_accent),
    )
    .padding([0, 5])
    .height(16)
    .align_y(Alignment::Center)
    .style(move |_| crate::style::rounded(accent, 4))
    .into()
}

/// A [`Switch`] showing `on`, sending `message` when flipped.
pub fn switch<'a, M: Clone + 'a>(
    label: impl Into<Cow<'a, str>>,
    on: bool,
    message: Option<M>,
) -> Switch<'a, M> {
    Switch {
        label: label.into(),
        description: None,
        on,
        message,
        locked: None,
        fresh: false,
    }
}

impl<'a, M: Clone + 'a> Switch<'a, M> {
    pub fn description(mut self, words: impl Into<Cow<'a, str>>) -> Self {
        let words = words.into();
        self.description = (!words.is_empty()).then_some(words);
        self
    }

    /// A NEW tag after its label, while the setting is new to this person
    /// (`yapfast_core::new_tags`).
    pub fn fresh(mut self, fresh: bool) -> Self {
        self.fresh = fresh;
        self
    }

    /// Why it can't change, in a tooltip on the lock after its label.
    pub fn locked(mut self, why: &'a str) -> Self {
        self.locked = Some(why);
        self
    }

    pub fn view(self, look: Look<'_>) -> Element<'a, M> {
        let theme = look.theme;
        let enabled = self.message.is_some();
        let mut label =
            row![
                text(self.label)
                    .size(15)
                    .color(if enabled { theme.text } else { theme.muted })
            ]
            .push(self.fresh.then(|| new_tag(theme)))
            .spacing(6)
            .align_y(Alignment::Center);
        if let (false, Some(why)) = (enabled, self.locked) {
            label = label.push(iced::widget::tooltip(
                look.glyph(Icon::Lock, 14, theme.muted),
                controls::tip(theme, why),
                iced::widget::tooltip::Position::Top,
            ));
        }
        let line = row![
            container(label).width(Length::Fill),
            toggle(theme, self.on, self.message.clone())
        ]
        .spacing(12)
        .align_y(Alignment::Center);
        let face = button(line)
            .padding(0)
            .width(Length::Fill)
            .on_press_maybe(self.message)
            .style(|_, _| button::Style::default());
        match self.description {
            Some(words) => column![face, note(theme, words)]
                .spacing(4)
                .width(Length::Fill)
                .into(),
            None => face.into(),
        }
    }
}

/// A permission's three choices in a channel, as Discord's: deny it, leave
/// it to the roles, or allow it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tri {
    Deny,
    Pass,
    Allow,
}

/// The tri-state's size.
pub const TRI_WIDTH: f32 = 108.0;
pub const TRI_HEIGHT: f32 = 28.0;
const TRI_GLYPH: u32 = 16;

/// Discord's three buttons for a permission: X, a slash and a tick in a
/// box outlined in the line's colour, the chosen one filled (red, faint,
/// green) with a white mark. Without `on` it can't change, its marks faint.
pub fn tri_state<'a, M: Clone + 'a>(
    look: Look<'_>,
    state: Tri,
    on: Option<&(dyn Fn(Tri) -> M + 'a)>,
) -> Element<'a, M> {
    let theme = look.theme;
    let enabled = on.is_some();
    let mut choices = row![];
    for (choice, icon, fill) in [
        (Tri::Deny, Icon::X, theme.danger),
        (Tri::Pass, Icon::Slash, theme.faint),
        (Tri::Allow, Icon::Check, theme.live),
    ] {
        let chosen = choice == state;
        let ink = if chosen {
            Color::WHITE
        } else if enabled {
            theme.secondary
        } else {
            theme.faint
        };
        let hover = theme.hover;
        choices = choices.push(
            button(look.glyph(icon, TRI_GLYPH, ink).boxed(Length::Fill))
                .padding(0)
                .width(Length::Fill)
                .height(Length::Fill)
                .on_press_maybe(on.map(|on| on(choice)))
                .style(move |_, status| button::Style {
                    background: if chosen {
                        Some(fill.into())
                    } else if enabled && lights(status) {
                        Some(hover.into())
                    } else {
                        None
                    },
                    border: Border::default().rounded(5),
                    ..Default::default()
                }),
        );
    }
    let line = theme.line;
    container(choices)
        .padding(1)
        .width(TRI_WIDTH)
        .height(TRI_HEIGHT)
        .style(move |_| container::Style {
            border: crate::style::edge(line, 6.0),
            ..Default::default()
        })
        .into()
}

/// A permission's line: its title (with a lock saying why while `locked`
/// says so) and its [`tri_state`] on one line, what it does under them,
/// and a rule.
pub fn tri_line<'a, M: Clone + 'a>(
    look: Look<'_>,
    title: &'a str,
    description: &'a str,
    state: Tri,
    locked: Option<&'a str>,
    on: Option<&(dyn Fn(Tri) -> M + 'a)>,
) -> Element<'a, M> {
    let theme = look.theme;
    let usable = on.is_some() && locked.is_none();
    let mut label = row![
        text(title)
            .size(15)
            .color(if usable { theme.text } else { theme.muted })
    ]
    .spacing(6)
    .align_y(Alignment::Center);
    if let Some(why) = locked {
        label = label.push(iced::widget::tooltip(
            look.glyph(Icon::Lock, 14, theme.muted),
            controls::tip(theme, why),
            iced::widget::tooltip::Position::Top,
        ));
    }
    column![
        row![
            container(label).width(Length::Fill),
            tri_state(look, state, on.filter(|_| usable))
        ]
        .spacing(12)
        .align_y(Alignment::Center),
        text(description).size(13).color(theme.secondary),
        rule(theme),
    ]
    .spacing(4)
    .padding(iced::Padding {
        bottom: 12.0,
        right: 12.0,
        ..iced::Padding::ZERO
    })
    .into()
}

/// A switch's toggle, as Discord's: a 48 by 24 pill, the accent's while on
/// (lighter under the pointer), the field's while off, its knob at the end
/// it's on.
pub fn toggle<'a, M: Clone + 'a>(theme: &Theme, on: bool, message: Option<M>) -> Element<'a, M> {
    let enabled = message.is_some();
    let knob_ink = match (enabled, on) {
        (false, _) => theme.faint,
        (true, true) => theme.on_accent,
        (true, false) => theme.secondary,
    };
    let knob = container(Space::new())
        .width(KNOB)
        .height(KNOB)
        .style(move |_| rounded(knob_ink, KNOB / 2.0));
    let (field, active, accent, accent_hover, line) = (
        theme.field,
        theme.active,
        theme.accent,
        theme.accent_hover,
        theme.line,
    );
    button(
        container(knob)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_y(Alignment::Center)
            .align_x(if on { Alignment::End } else { Alignment::Start }),
    )
    .padding([0.0, (TOGGLE_HEIGHT - KNOB) / 2.0])
    .width(TOGGLE_WIDTH)
    .height(TOGGLE_HEIGHT)
    .on_press_maybe(message)
    .style(move |_, status| {
        let lit = lights(status);
        let ground = match (enabled, on, lit) {
            (false, _, _) => field,
            (true, true, false) => accent,
            (true, true, true) => accent_hover,
            (true, false, false) => field,
            (true, false, true) => active,
        };
        button::Style {
            background: Some(ground.into()),
            border: Border {
                color: if on && enabled {
                    Color::TRANSPARENT
                } else {
                    line
                },
                width: 1.0,
                radius: (TOGGLE_HEIGHT / 2.0).into(),
            },
            ..Default::default()
        }
    })
    .into()
}

/// A dropdown: the chosen option in a box with a chevron, and while `open`
/// the options in a list under it (over it without room), the chosen one
/// ticked. Its owner keeps which is open: `toggle` opens and closes it, a
/// press outside closes it, and `pick` says the option chosen. With no
/// options it's greyed.
pub struct Dropdown<'a, M> {
    options: Vec<String>,
    current: Option<usize>,
    open: bool,
    toggle: M,
    pick: Box<dyn Fn(usize) -> M + 'a>,
    width: Length,
    height: f32,
    enabled: bool,
}

pub fn dropdown<'a, M: Clone + 'a>(
    options: Vec<String>,
    current: Option<usize>,
    open: bool,
    toggle: M,
    pick: impl Fn(usize) -> M + 'a,
) -> Dropdown<'a, M> {
    Dropdown {
        options,
        current,
        open,
        toggle,
        pick: Box::new(pick),
        width: Length::Fill,
        height: DROPDOWN,
        enabled: true,
    }
}

impl<'a, M: Clone + 'a> Dropdown<'a, M> {
    /// Taller than [`DROPDOWN`], as one a row's field stretches.
    pub fn height(mut self, height: f32) -> Self {
        self.height = height;
        self
    }

    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }

    /// Greyed, its list closed, while not `enabled`.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn view(self, look: Look<'a>) -> Element<'a, M> {
        let theme = look.theme;
        let enabled = self.enabled && !self.options.is_empty();
        let shown = self
            .current
            .and_then(|index| self.options.get(index))
            .cloned()
            .unwrap_or_default();
        let ink = if enabled { theme.text } else { theme.faint };
        // The chevron keeps its place however long the choice's name.
        let face = row![
            container(
                text(shown)
                    .size(14)
                    .color(ink)
                    .wrapping(text::Wrapping::None)
            )
            .width(Length::Fill)
            .clip(true),
            look.glyph(
                if self.open {
                    Icon::ChevronUp
                } else {
                    Icon::ChevronDown
                },
                16,
                if enabled {
                    theme.secondary
                } else {
                    theme.faint
                },
            )
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        let (ground, lit, edge, accent) = (
            theme.active,
            crate::theme::mix(theme.overlay, theme.active, 0.04),
            theme.wash(0.06),
            theme.accent,
        );
        let open = self.open;
        let base = button(
            container(face)
                .height(Length::Fill)
                .align_y(Alignment::Center)
                .clip(true),
        )
        .padding([0, 12])
        .width(self.width)
        .height(self.height)
        .on_press_maybe(enabled.then(|| self.toggle.clone()))
        .style(move |_, status| button::Style {
            background: Some(if lights(status) { lit } else { ground }.into()),
            border: crate::style::edge(if open { accent } else { edge }, 4.0),
            ..Default::default()
        });
        let list = (self.open && enabled).then(|| {
            let mut rows = column![].spacing(2);
            for (index, option) in self.options.iter().enumerate() {
                let item = Item {
                    label: option.clone().into(),
                    icon: None,
                    danger: false,
                    slot: false,
                    disabled: false,
                    subtitle: None,
                    check: if Some(index) == self.current {
                        Check::Current
                    } else {
                        Check::None
                    },
                };
                rows = rows.push(look.menu_item(item, Light::Pointer, (self.pick)(index)));
            }
            iced::widget::opaque(
                container(
                    scroll_view(rows)
                        .style(crate::style::scrollbar(theme))
                        .height(Length::Shrink),
                )
                .padding(6)
                .max_height(LIST)
                .width(Length::Fill)
                .style(controls::menu_panel(theme)),
            )
        });
        Popover::new(base, list, Side::Below)
            .width(Width::Same)
            .gap(4.0)
            .outside(self.toggle)
            .into()
    }
}

/// A choice of [`options`]: its title (in a colour of its own, as
/// Discord's verification levels), what it means under it, and whether it
/// may be chosen.
pub struct Choice<'a> {
    pub title: Cow<'a, str>,
    pub description: Cow<'a, str>,
    pub colour: Option<Color>,
    pub offered: bool,
}

/// The radio button's ring and its dot.
const RADIO: f32 = 20.0;
const RADIO_DOT: f32 = 10.0;

/// Exclusive choices as Discord's Safety Setup lists them: each a row with
/// its radio button, title and what it means, the chosen one on a light
/// wash, lit under the pointer; a choice not offered (or all while not
/// `enabled`) can't be chosen, and one not offered is faded.
pub fn options<'a, M: Clone + 'a>(
    theme: &Theme,
    choices: Vec<Choice<'a>>,
    chosen: Option<usize>,
    enabled: bool,
    on_choose: impl Fn(usize) -> M,
) -> Element<'a, M> {
    let mut list = column![].spacing(4).width(Length::Fill);
    for (index, choice) in choices.into_iter().enumerate() {
        let on = chosen == Some(index);
        let usable = enabled && choice.offered;
        let fade = if choice.offered { 1.0 } else { 0.5 };
        let faded = |colour: Color| crate::style::faded(colour, fade);
        let (accent, ring) = (faded(theme.accent), faded(theme.secondary));
        let dot = container(Space::new())
            .width(RADIO_DOT)
            .height(RADIO_DOT)
            .style(move |_| rounded(accent, RADIO_DOT / 2.0));
        let radio = container(if on {
            Element::from(dot)
        } else {
            Space::new().into()
        })
        .center(RADIO)
        .style(move |_| container::Style {
            border: Border {
                color: if on { accent } else { ring },
                width: 2.0,
                radius: (RADIO / 2.0).into(),
            },
            ..Default::default()
        });
        let words = column![
            text(choice.title)
                .size(15)
                .font(SEMIBOLD)
                .color(faded(choice.colour.unwrap_or(theme.text))),
            text(choice.description)
                .size(13)
                .color(faded(theme.secondary))
                .width(Length::Fill),
        ]
        .spacing(2);
        let (chosen_ground, lit) = (theme.wash(0.08), theme.wash(0.05));
        list = list.push(
            button(row![radio, words].spacing(12).align_y(Alignment::Center))
                .padding(10)
                .width(Length::Fill)
                .on_press_maybe(usable.then(|| on_choose(index)))
                .style(move |_, status| button::Style {
                    background: if on {
                        Some(chosen_ground.into())
                    } else if lights(status) {
                        Some(lit.into())
                    } else {
                        None
                    },
                    border: Border::default().rounded(6),
                    ..Default::default()
                }),
        );
    }
    list.into()
}

/// A value set elsewhere, in a sunken box: the entrance sound chosen, a
/// key. `empty` shows in the muted ink while there's none.
pub fn readout<'a, M: Clone + 'a>(
    theme: &Theme,
    value: impl Into<Cow<'a, str>>,
    empty: impl Into<Cow<'a, str>>,
    ring: bool,
) -> container::Container<'a, M> {
    let value = value.into();
    let (shown, ink, font) = if value.is_empty() {
        (empty.into(), theme.muted, iced::Font::default())
    } else {
        (value, theme.text, SEMIBOLD)
    };
    let (sunken, accent) = (theme.sunken, theme.accent);
    container(
        text(shown)
            .size(14)
            .font(font)
            .color(ink)
            .wrapping(text::Wrapping::None),
    )
    .padding([0, 12])
    .height(BOX)
    .align_y(Alignment::Center)
    .clip(true)
    .style(move |_| container::Style {
        background: Some(sunken.into()),
        border: Border {
            color: accent,
            width: if ring { 2.0 } else { 0.0 },
            radius: 8.0.into(),
        },
        ..Default::default()
    })
}

/// A keybind's keys in their box, `width` wide: while recording
/// (`recorded`), the keys pressed so far, or "Press a key..." till one is,
/// in the muted ink, ringed in the accent.
pub fn capture<'a, M: Clone + 'a>(
    theme: &Theme,
    keys: impl Into<Cow<'a, str>>,
    recorded: Option<String>,
    width: f32,
) -> Element<'a, M> {
    let box_ = match recorded {
        Some(recorded) if recorded.is_empty() => readout(theme, "", "Press a key...", true),
        Some(recorded) => readout(theme, "", recorded, true),
        None => readout(theme, keys, "No keybind set", false),
    };
    box_.width(width).into()
}

/// A single-line field in a form: `controls::field`, 15 px.
pub fn field<'a, M: Clone + 'a>(
    theme: &Theme,
    id: impl Into<iced::widget::Id>,
    placeholder: &'a str,
    value: &str,
) -> text_input::TextInput<'a, M> {
    controls::field(theme, id, placeholder, value, false, true).size(15)
}

/// A single-line field as a server's settings have them: 16 px text, 40 px
/// tall.
pub fn entry<'a, M: Clone + 'a>(
    theme: &Theme,
    id: impl Into<iced::widget::Id>,
    placeholder: &'a str,
    value: &'a str,
) -> text_input::TextInput<'a, M> {
    controls::field(theme, id, placeholder, value, false, true).padding([9.6, 12.0])
}

/// A colour's pad over its code's field, on a raised card 240 px wide, as
/// a theme colour's handle and the Display Name Style window open it.
pub const COLOUR_CARD: f32 = 240.0;

pub fn colour_card<'a, M: Clone + 'a>(
    theme: &Theme,
    pad: crate::widgets::ColourPad<'a, M>,
    code: text_input::TextInput<'a, M>,
) -> Element<'a, M> {
    let (raised, edge) = (theme.raised, theme.edge);
    iced::widget::opaque(
        container(column![pad, code.size(14).padding([12, 12])].spacing(14))
            .padding(10)
            .width(Length::Fill)
            .style(move |_| container::Style {
                border: crate::style::edge(edge, 12.0),
                shadow: crate::style::card_shadow(),
                ..rounded(raised, 12)
            }),
    )
}

/// A colour in an editor: a swatch, its name (with "Not a colour" under it
/// while its text isn't one), and its hex in a field tinted red while it's
/// wrong.
pub fn colour_row<'a, M: Clone + 'a>(
    theme: &Theme,
    name: &'a str,
    colour: Color,
    hex: &'a str,
    wrong: bool,
    on_input: impl Fn(String) -> M + 'a,
) -> Element<'a, M> {
    let edge = theme.line;
    let swatch = container(Space::new())
        .width(28)
        .height(28)
        .style(move |_| container::Style {
            background: Some(colour.into()),
            border: crate::style::edge(edge, 6.0),
            ..Default::default()
        });
    let mut words = column![
        text(name)
            .size(14)
            .color(theme.text)
            .wrapping(text::Wrapping::None)
    ];
    if wrong {
        words = words.push(text("Not a colour").size(12).color(theme.danger));
    }
    let ground = if wrong {
        Color {
            a: 0.16,
            ..theme.danger
        }
    } else {
        theme.sunken
    };
    let field = text_input("", hex)
        .on_input(on_input)
        .size(14)
        .padding(8)
        .width(116)
        .style(controls::field_style(theme, ground, true));
    row![
        swatch,
        container(words).width(Length::Fill).clip(true),
        field
    ]
    .spacing(10)
    .align_y(Alignment::Center)
    .height(46)
    .into()
}

/// A card on the sidebar's ground, as My account's and About's: 16 px in,
/// round corners.
pub fn card<'a, M: Clone + 'a>(theme: &Theme, body: impl Into<Element<'a, M>>) -> Element<'a, M> {
    let ground = theme.sidebar;
    container(body)
        .padding(16)
        .width(Length::Fill)
        .style(move |_| rounded(ground, 8))
        .into()
}

/// The bar an editor shows once something changed: what it says, Reset and
/// Save Changes ("Saving..." while it saves), `pad` in from its left. While
/// `shake` says, it shakes red, as after leaving was refused for the
/// changes. Its corners are `radius` round, as it floats over a settings
/// page.
pub struct Unsaved<M> {
    pub pad: f32,
    pub radius: f32,
    pub saving: bool,
    pub may_reset: bool,
    pub may_save: bool,
    pub shake: Shake,
    pub undo: M,
    pub save: M,
}

impl<M: Clone> Unsaved<M> {
    pub fn view<'a>(self, theme: &Theme) -> Element<'a, M>
    where
        M: 'a,
    {
        let Self {
            pad,
            radius,
            saving,
            may_reset,
            may_save,
            shake,
            undo,
            save,
        } = self;
        let red = shake.red;
        let (raised, edge, danger) = (theme.raised, theme.edge, theme.danger);
        let offset = shake.offset();
        let bar = row![
            text("Careful — you have unsaved changes!")
                .size(14)
                .font(SEMIBOLD)
                .color(if red { theme.on_danger } else { theme.text })
                .width(Length::Fill),
            controls::action(
                theme,
                "Reset",
                controls::Kind::Ghost,
                (may_reset && !saving).then_some(undo),
            ),
            controls::action(
                theme,
                if saving { "Saving..." } else { "Save Changes" },
                controls::Kind::Primary,
                (may_save && !saving).then_some(save),
            ),
        ]
        .spacing(TIGHT)
        .align_y(Alignment::Center)
        .padding(iced::Padding {
            top: 12.0,
            right: 12.0 - offset,
            bottom: 12.0,
            left: pad + offset,
        });
        container(bar)
            .width(Length::Fill)
            .height(60)
            .align_y(Alignment::Center)
            .style(move |_| container::Style {
                background: Some(if red { danger } else { raised }.into()),
                border: iced::Border::default().rounded(radius),
                shadow: iced::Shadow {
                    color: edge,
                    offset: iced::Vector::new(0.0, -1.0),
                    blur_radius: 0.0,
                },
                ..Default::default()
            })
            .into()
    }
}

/// How the unsaved bar shakes: each time leaving is refused for the
/// changes (the core's nudge goes up) it goes red for a little over a
/// second and jumps side to side six times. Its owner keeps it, hands
/// [`Shake::start`] each nudge and [`Shake::step`] each step the tasks send
/// back, through one event of its own that carries a [`ShakeStep`].
#[derive(Clone, Copy, Debug, Default)]
pub struct Shake {
    seen: u32,
    left: u8,
    pub red: bool,
}

/// What a shake's timer says: move again, or the red is over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShakeStep {
    Move,
    Calm,
}

const SHAKE_STEP: std::time::Duration = std::time::Duration::from_millis(60);
const SHAKE_RED: std::time::Duration = std::time::Duration::from_millis(1200);

impl Shake {
    /// Takes the core's nudge; when it went up the shake starts, its
    /// steps coming back as `send` makes them.
    pub fn start<M: Clone + Send + 'static>(
        &mut self,
        nudge: u32,
        send: impl Fn(ShakeStep) -> M,
    ) -> Task<M> {
        if nudge == self.seen {
            return Task::none();
        }
        self.seen = nudge;
        self.left = 6;
        self.red = true;
        Task::batch([
            crate::timer::after(SHAKE_STEP, send(ShakeStep::Move)),
            crate::timer::after(SHAKE_RED, send(ShakeStep::Calm)),
        ])
    }

    /// One of its steps: the next move, if another is due.
    pub fn step<M: Clone + Send + 'static>(
        &mut self,
        step: ShakeStep,
        send: impl Fn(ShakeStep) -> M,
    ) -> Task<M> {
        match step {
            ShakeStep::Move => {
                self.left = self.left.saturating_sub(1);
                if self.left > 0 {
                    return crate::timer::after(SHAKE_STEP, send(ShakeStep::Move));
                }
            }
            ShakeStep::Calm => self.red = false,
        }
        Task::none()
    }

    /// How far the bar's contents stand from their place.
    fn offset(self) -> f32 {
        match self.left {
            0 => 0.0,
            left if left % 2 == 0 => -6.0,
            _ => 6.0,
        }
    }
}

/// An area's lines: four, 20 px each, in its 96 px with 8 px over and under.
const AREA_LINE: f32 = 20.0;
const AREA_LINES: f32 = 4.0;
const AREA_HEIGHT: f32 = 96.0;
const AREA_RADIUS: f32 = 6.0;

/// A text of a form several lines high: 96 px of the sunken ground, ringed
/// while it has the keys, with how many characters are `left` under it
/// ("Too long by" in red once below none). Its owner keeps the `draft`.
pub fn area<'a, M: Clone + 'a>(
    theme: &Theme,
    draft: &'a crate::field::Draft,
    id: &'static str,
    placeholder: &str,
    left: i64,
    enabled: bool,
    on: fn(crate::field::Event) -> M,
) -> Element<'a, M> {
    let colors = crate::field::Colors {
        ground: theme.sunken,
        text: if enabled { theme.text } else { theme.muted },
        placeholder: theme.muted,
        selection: theme.selection,
    };
    let field = crate::field::Field::new(
        draft,
        iced::widget::Id::new(id),
        placeholder.to_owned(),
        on,
        colors,
    )
    .enabled(enabled)
    .size(14.0)
    .line_height(AREA_LINE)
    .lines(AREA_LINES)
    .padding(iced::Padding::from([8.0, 10.0]))
    .radius(AREA_RADIUS)
    .ring(crate::style::faded(theme.accent, 0.8));
    let sunken = theme.sunken;
    let count = if left < 0 {
        text(format!("Too long by {}", -left))
            .size(12)
            .color(theme.danger)
    } else {
        text(format!("{left} left")).size(12).color(theme.muted)
    };
    column![
        container(field)
            .height(AREA_HEIGHT)
            .width(Length::Fill)
            .clip(true)
            .style(move |_| rounded(sunken, AREA_RADIUS)),
        container(count).align_right(Length::Fill)
    ]
    .spacing(4)
    .into()
}

#[cfg(test)]
mod tests {
    use super::{Shake, ShakeStep};
    use crate::testing::Message;

    #[test]
    fn a_shake_starts_on_a_new_nudge_moves_six_times_and_calms() {
        let send = |_| Message::Nothing;
        let mut shake = Shake::default();
        assert_eq!(shake.start(0, send).units(), 0, "no nudge yet");
        assert!(shake.start(1, send).units() > 0);
        assert!(shake.red);
        assert_eq!(shake.start(1, send).units(), 0, "the same nudge again");
        let moves = (0..10)
            .take_while(|_| shake.step(ShakeStep::Move, send).units() > 0)
            .count();
        assert_eq!(moves, 5, "six places, the last at rest");
        assert_eq!(shake.offset(), 0.0);
        let _ = shake.step(ShakeStep::Calm, send);
        assert!(!shake.red);
    }
}
