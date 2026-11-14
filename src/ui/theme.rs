//! The window's colours in Slint: the core's themes (`yapfast_core::theme`),
//! set on the `Theme` global of `ui/theme.slint` to change them all at once.

use std::{cell::Cell, rc::Rc};

use slint::{Color, ComponentHandle, Model, ModelRc, VecModel};
pub use yapfast_core::theme::{
    BUILT_IN, Colour, Custom, Theme, chosen, hex, lookup, new_id, parse, starts, tidy, token,
};
#[cfg(test)]
pub use yapfast_core::theme::{YAP, find};
use yapfast_ui::YapfastWindow;

/// A theme's colour as Slint's.
pub fn colour(colour: Colour) -> Color {
    Color::from_argb_encoded(colour.argb())
}

/// Declares the window's getter and setter for each of the theme's colours.
macro_rules! tokens {
    ($($name:ident ($get:ident, $set:ident),)*) => {
        /// Sets only the colours that differ from those the window shows:
        /// nothing when it starts in Yap, its defaults.
        fn set(theme: &Theme, tokens: &yapfast_ui::Theme<'_>) {
            if tokens.get_dark() != theme.dark {
                tokens.set_dark(theme.dark);
            }
            $(if tokens.$get() != colour(theme.$name) {
                tokens.$set(colour(theme.$name));
            })*
        }

        /// The colours the window shows now. Every colour is named, so a
        /// colour the core's `Theme` gains must be named here too.
        #[cfg(test)]
        pub fn shown(window: &YapfastWindow) -> Theme {
            let tokens = window.global::<yapfast_ui::Theme>();
            let back = |colour: Color| Colour::from_argb(colour.as_argb_encoded());
            Theme { dark: tokens.get_dark(), $($name: back(tokens.$get()),)* }
        }
    };
}

tokens! {
    rail(get_rail, set_rail),
    sidebar(get_sidebar, set_sidebar),
    panel(get_panel, set_panel),
    chat(get_chat, set_chat),
    stage(get_stage, set_stage),
    raised(get_raised, set_raised),
    field(get_field, set_field),
    sunken(get_sunken, set_sunken),
    hover(get_hover, set_hover),
    active(get_active, set_active),
    row_hover(get_row_hover, set_row_hover),
    line(get_line, set_line),
    edge(get_edge, set_edge),
    outline(get_outline, set_outline),
    outline_chat(get_outline_chat, set_outline_chat),
    outline_field(get_outline_field, set_outline_field),
    text(get_text, set_text),
    secondary(get_secondary, set_secondary),
    muted(get_muted, set_muted),
    faint(get_faint, set_faint),
    accent(get_accent, set_accent),
    accent_hover(get_accent_hover, set_accent_hover),
    on_accent(get_on_accent, set_on_accent),
    accent_soft(get_accent_soft, set_accent_soft),
    danger(get_danger, set_danger),
    danger_hover(get_danger_hover, set_danger_hover),
    on_danger(get_on_danger, set_on_danger),
    live(get_live, set_live),
    loud(get_loud, set_loud),
    link(get_link, set_link),
    button_default(get_button_default, set_button_default),
    button_default_hover(get_button_default_hover, set_button_default_hover),
    overlay(get_overlay, set_overlay),
    shimmer(get_shimmer, set_shimmer),
    reply_spine(get_reply_spine, set_reply_spine),
    tooltip(get_tooltip, set_tooltip),
    avatar(get_avatar, set_avatar),
}

thread_local! {
    /// The colour of a mention without one of its own, as `0xrrggbb`: the
    /// theme's link colour. Rows build their markup with it, outside the
    /// window, on the window's thread.
    static MENTION: Cell<u32> = const { Cell::new(0x64adf6) };
    /// The theme's text and muted colours, the same way, for a notice's
    /// name, links and time.
    static TEXT: Cell<u32> = const { Cell::new(0xf2f3f5) };
    static MUTED: Cell<u32> = const { Cell::new(0x949ba4) };
}

pub fn mention_color() -> u32 {
    MENTION.get()
}

pub fn text_color() -> u32 {
    TEXT.get()
}

pub fn muted_color() -> u32 {
    MUTED.get()
}

/// Shows the window in `theme`'s colours. Returns whether colours that rows
/// carry in their markup change, as rows built before have the old ones.
pub fn apply(theme: &Theme, window: &YapfastWindow) -> bool {
    set(theme, &window.global::<yapfast_ui::Theme>());
    let mention_colour = theme.mention().rgb_part();
    let mention = MENTION.replace(mention_colour) != mention_colour;
    let text = TEXT.replace(theme.text.rgb_part()) != theme.text.rgb_part();
    let muted = MUTED.replace(theme.muted.rgb_part()) != theme.muted.rgb_part();
    mention || text || muted
}

/// Appearance's cards: each theme's name and the colours its preview draws
/// with, the built-in ones first.
pub fn cards(custom: &[Custom]) -> ModelRc<yapfast_ui::ThemeCard> {
    let card = |id: &str, name: &str, custom: bool, theme: &Theme| yapfast_ui::ThemeCard {
        id: id.into(),
        name: name.into(),
        custom,
        rail: colour(theme.rail),
        sidebar: colour(theme.sidebar),
        chat: colour(theme.chat),
        raised: colour(theme.raised),
        accent: colour(theme.accent),
        text: colour(theme.text),
        muted: colour(theme.muted),
    };
    let cards: Vec<_> = BUILT_IN
        .iter()
        .map(|built_in| card(built_in.id, built_in.name, false, &built_in.theme))
        .chain(
            custom
                .iter()
                .map(|custom| card(&custom.id, &custom.name, true, &custom.theme)),
        )
        .collect();
    ModelRc::new(VecModel::from(cards))
}

/// Appearance's theme editor while it's open: the theme as edited so far,
/// and a row for each colour, the model it lends the window until it closes.
pub struct Editor {
    /// The custom theme being edited, by id; `None` for a new one.
    pub editing: Option<String>,
    pub base: &'static str,
    pub theme: Theme,
    rows: Rc<VecModel<yapfast_ui::ThemeColour>>,
}

impl Editor {
    pub fn new(editing: Option<String>, base: &'static str, theme: Theme) -> Self {
        let editor = Self {
            editing,
            base,
            theme,
            rows: Rc::default(),
        };
        editor.fill();
        editor
    }

    /// Writes every row again, which also gives each field its text anew.
    fn fill(&self) {
        let rows: Vec<_> = Theme::NAMES
            .iter()
            .enumerate()
            .filter_map(|(index, name)| {
                let shown = self.theme.colour(index)?;
                Some(yapfast_ui::ThemeColour {
                    name: token(name).into(),
                    colour: colour(shown),
                    hex: hex(shown).into(),
                    wrong: false,
                })
            })
            .collect();
        self.rows.set_vec(rows);
    }

    pub fn rows(&self) -> ModelRc<yapfast_ui::ThemeColour> {
        ModelRc::from(self.rows.clone())
    }

    /// Starts again from another theme's colours.
    pub fn restart(&mut self, base: &'static str, theme: Theme) {
        self.base = base;
        self.theme = theme;
        self.fill();
    }

    /// Takes a colour's text as typed: the theme takes the colour if it is
    /// one, and the row says so if not. Returns whether the theme changed.
    pub fn edit(&mut self, index: usize, text: &str) -> bool {
        let Some(mut row) = self.rows.row_data(index) else {
            return false;
        };
        let parsed = parse(text);
        row.hex = text.into();
        row.wrong = parsed.is_none();
        row.colour = parsed.map(colour).unwrap_or(row.colour);
        self.rows.set_row_data(index, row);
        match (parsed, self.theme.colour_mut(index)) {
            (Some(parsed), Some(slot)) if *slot != parsed => {
                *slot = parsed;
                true
            }
            _ => false,
        }
    }

    /// Whether every field holds a colour.
    pub fn valid(&self) -> bool {
        !self.rows.iter().any(|row| row.wrong)
    }
}
