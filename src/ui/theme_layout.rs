//! A theme's `layout` on the window, through the Slint global `Arrange`
//! (`ui/theme_layout.slint`) and `Look`'s outline width. See
//! `docs/themes.md`.
//!
//! [`reset`] puts every value back to stock, which `theme_look::reset` calls
//! first whenever a theme is drawn, cleared or hidden behind a protected
//! screen. A theme that uses neither layer leaves every value at its
//! default, so it costs nothing: the interface draws the stock window and
//! the ransom callback is never asked.

use std::cell::RefCell;
use std::hash::{Hash, Hasher};

use slint::{Color, ComponentHandle, ModelRc, SharedString, VecModel};
use yapfast_core::themes::Performance;
use yapfast_core::themes::layout::{BorderPlace, Layout, Stroke};
use yapfast_ui::{Arrange, Dash, Look, Scrap, YapfastWindow};

/// What the ransom callback cuts letters from.
#[derive(Default)]
struct Ransom {
    families: Vec<SharedString>,
    papers: Vec<(Color, Color)>,
}

thread_local! {
    static RANSOM: RefCell<Ransom> = RefCell::new(Ransom::default());
}

/// Letters a name is cut into; the rest is one plain piece.
const MOST_LETTERS: usize = 12;

/// The scraps' grounds and inks when a theme names none: newsprint, ink and a
/// pink.
const PAPERS: [(u32, u32); 3] = [
    (0xfff5_f1e8, 0xff14_1414),
    (0xff14_1414, 0xfff5_f1e8),
    (0xffff_5fa2, 0xff14_1414),
];

/// Answers `Arrange.scraps`. Called once per window.
pub fn init(window: &YapfastWindow) {
    window.global::<Arrange>().on_scraps(|text| scraps(&text));
}

/// Every value back to stock.
pub fn reset(window: &YapfastWindow) {
    let arrange = window.global::<Arrange>();
    arrange.set_rail(0);
    arrange.set_members(0);
    arrange.set_composer_top(false);
    arrange.set_messages(0);
    arrange.set_chat_max_width(0.);
    arrange.set_hide_avatars(false);
    arrange.set_irc_time(64.);
    arrange.set_irc_nick(128.);
    arrange.set_irc_separator("│".into());
    arrange.set_bubble_max(0.72);
    arrange.set_bubble_own_set(Color::default());
    arrange.set_bubble_other_set(Color::default());
    arrange.set_bubble_own_text_set(Color::default());
    arrange.set_bubble_radius(16.);
    arrange.set_cards_tilt(0.);
    arrange.set_cards_gap(8.);
    arrange.set_cards_padding(12.);
    arrange.set_cards_ground_set(Color::default());
    arrange.set_cards_shadow_blur(-1.);
    arrange.set_cards_shadow_x(0.);
    arrange.set_cards_shadow_y(0.);
    arrange.set_cards_shadow_color(Color::default());
    arrange.set_cards_over_top(0.);
    arrange.set_cards_over_bottom(0.);
    arrange.set_cards_bleed(0.);
    for place in BorderPlace::ALL {
        set_dash(&arrange, *place, Dash::default());
    }
    window.global::<Look>().set_outline_width(-1.);
    arrange.set_ransom_names(false);
    RANSOM.with_borrow_mut(|ransom| *ransom = Ransom::default());
}

/// Sets a theme's layout. `families` are the ransom fonts' registered
/// names, in the theme's order; `names` the names face's, if it has one.
pub fn apply(
    window: &YapfastWindow,
    layout: &Layout,
    families: &[String],
    names: Option<&str>,
    performance: Performance,
) {
    let arrange = window.global::<Arrange>();
    if let Some(rail) = layout.rail {
        arrange.set_rail(rail.index());
    }
    if let Some(members) = layout.members {
        arrange.set_members(members.index());
    }
    if let Some(composer) = layout.composer {
        arrange.set_composer_top(composer.index() == 1);
    }
    if let Some(messages) = layout.messages {
        arrange.set_messages(messages.index());
    }
    if let Some(width) = layout.chat_max_width {
        arrange.set_chat_max_width(width);
    }
    arrange.set_hide_avatars(layout.chat_avatars == Some(false));
    if let Some(width) = layout.irc.time_width {
        arrange.set_irc_time(width);
    }
    if let Some(width) = layout.irc.nick_width {
        arrange.set_irc_nick(width);
    }
    if let Some(separator) = &layout.irc.separator {
        arrange.set_irc_separator(separator.as_str().into());
    }
    if let Some(share) = layout.bubbles.max_width {
        arrange.set_bubble_max(share / 100.);
    }
    if let Some(colour) = layout.bubbles.own {
        arrange.set_bubble_own_set(Color::from_argb_encoded(colour));
    }
    if let Some(colour) = layout.bubbles.other {
        arrange.set_bubble_other_set(Color::from_argb_encoded(colour));
    }
    if let Some(colour) = layout.bubbles.own_text {
        arrange.set_bubble_own_text_set(Color::from_argb_encoded(colour));
    }
    if let Some(radius) = layout.bubbles.radius {
        arrange.set_bubble_radius(radius);
    }
    // A lean turns a card's ground, which Light leaves straight.
    if let Some(tilt) = layout.cards.tilt
        && performance != Performance::Light
    {
        arrange.set_cards_tilt(tilt);
    }
    if let Some(gap) = layout.cards.gap {
        arrange.set_cards_gap(gap);
    }
    if let Some(padding) = layout.cards.padding {
        arrange.set_cards_padding(padding);
    }
    if let Some(colour) = layout.cards.ground {
        arrange.set_cards_ground_set(Color::from_argb_encoded(colour));
    }
    // A card's shadow reaches past its edges by its offset and blur; each
    // row's slice of the card draws that much more, so it shows whole.
    if let Some(shadow) = layout.cards.shadow {
        arrange.set_cards_shadow_blur(shadow.blur);
        arrange.set_cards_shadow_x(shadow.x);
        arrange.set_cards_shadow_y(shadow.y);
        arrange.set_cards_shadow_color(Color::from_argb_encoded(shadow.colour));
        let reach = shadow.blur + shadow.x.abs().max(shadow.y.abs());
        arrange.set_cards_over_top((shadow.blur - shadow.y).max(0.));
        arrange.set_cards_over_bottom((shadow.blur + shadow.y).max(0.));
        arrange.set_cards_bleed(reach);
    }
    for (place, border) in &layout.borders {
        set_dash(
            &arrange,
            *place,
            Dash {
                on: border.stroke != Stroke::Solid,
                dotted: border.stroke == Stroke::Dotted,
                dash: border.dash,
                gap: border.gap,
                width: border.width.unwrap_or(-1.),
                ink: border
                    .colour
                    .map_or(Color::default(), Color::from_argb_encoded),
            },
        );
    }
    if let Some(width) = layout.outline_width {
        window.global::<Look>().set_outline_width(width);
    }

    // A ransom name is a few dozen small items a row: Light draws the stock
    // name instead.
    if let Some(cut) = &layout.names
        && performance != Performance::Light
    {
        arrange.set_ransom_names(true);
        let mut chosen: Vec<SharedString> = families
            .iter()
            .map(|family| family.as_str().into())
            .collect();
        if chosen.is_empty() {
            chosen.push(names.unwrap_or("").into());
            chosen.push(SharedString::new());
        }
        let papers = if cut.papers.is_empty() {
            PAPERS.to_vec()
        } else {
            cut.papers.clone()
        };
        RANSOM.with_borrow_mut(|ransom| {
            ransom.families = chosen;
            ransom.papers = papers
                .into_iter()
                .map(|(ground, ink)| {
                    (
                        Color::from_argb_encoded(ground),
                        Color::from_argb_encoded(ink),
                    )
                })
                .collect();
        });
    }
}

fn set_dash(arrange: &Arrange<'_>, place: BorderPlace, dash: Dash) {
    match place {
        BorderPlace::Composer => arrange.set_dash_composer(dash),
        BorderPlace::Actions => arrange.set_dash_actions(dash),
        BorderPlace::Card => arrange.set_dash_card(dash),
        BorderPlace::Menu => arrange.set_dash_menu(dash),
        BorderPlace::Dialog => arrange.set_dash_dialog(dash),
        BorderPlace::Tooltip => arrange.set_dash_tooltip(dash),
        BorderPlace::Message => arrange.set_dash_message(dash),
        BorderPlace::Embed => arrange.set_dash_embed(dash),
        BorderPlace::Code => arrange.set_dash_code(dash),
        BorderPlace::Tile => arrange.set_dash_tile(dash),
        BorderPlace::Divider => arrange.set_dash_divider(dash),
    }
}

/// A name cut into ransom letters: each of its first letters on a scrap of
/// its own, from the fonts in turn, leaning a little; what's past them as
/// one scrap in the first font. A name with letters outside Latin script is
/// one scrap whole, in the stock face, so no letter falls back to a box.
fn scraps(text: &str) -> ModelRc<Scrap> {
    RANSOM.with_borrow(|ransom| {
        let mut scraps = Vec::new();
        let (first_paper, first_ink) = ransom
            .papers
            .first()
            .copied()
            .unwrap_or((Color::default(), Color::default()));
        let latin = text
            .chars()
            .all(|letter| letter.is_ascii() || ('\u{a0}'..='\u{24f}').contains(&letter));
        if ransom.families.is_empty() || !latin {
            scraps.push(Scrap {
                text: text.into(),
                family: SharedString::new(),
                rotation: 0.,
                paper: first_paper,
                ink: first_ink,
                rest: true,
            });
            return ModelRc::new(VecModel::from(scraps));
        }
        let mut letters = text.char_indices();
        let mut index = 0usize;
        for (at, letter) in letters.by_ref() {
            if index == MOST_LETTERS {
                scraps.push(Scrap {
                    text: text[at..].into(),
                    family: ransom.families[0].clone(),
                    rotation: 0.,
                    paper: first_paper,
                    ink: first_ink,
                    rest: true,
                });
                break;
            }
            if letter == ' ' {
                scraps.push(Scrap {
                    text: " ".into(),
                    family: SharedString::new(),
                    rotation: 0.,
                    paper: Color::default(),
                    ink: Color::default(),
                    rest: false,
                });
                index += 1;
                continue;
            }
            let seed = seed(text, index);
            let (paper, ink) = ransom.papers[(seed as usize / 7) % ransom.papers.len()];
            scraps.push(Scrap {
                text: letter.to_string().into(),
                family: ransom.families[index % ransom.families.len()].clone(),
                // Up to 6° either way.
                rotation: ((seed % 13) as f32 - 6.) * 0.9,
                paper,
                ink,
                rest: false,
            });
            index += 1;
        }
        ModelRc::new(VecModel::from(scraps))
    })
}

/// The same lean and scrap for a name's letter every time it's drawn.
fn seed(text: &str, index: usize) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hasher);
    index.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn letters(model: &ModelRc<Scrap>) -> Vec<String> {
        use slint::Model as _;
        model.iter().map(|scrap| scrap.text.to_string()).collect()
    }

    #[test]
    fn cuts_a_name_into_letters_from_the_fonts_in_turn() {
        RANSOM.with_borrow_mut(|ransom| {
            ransom.families = vec!["A".into(), "B".into()];
            ransom.papers = vec![(Color::default(), Color::default())];
        });
        let model = scraps("wired");
        assert_eq!(letters(&model), ["w", "i", "r", "e", "d"]);
        use slint::Model as _;
        let families: Vec<String> = model.iter().map(|scrap| scrap.family.to_string()).collect();
        assert_eq!(families, ["A", "B", "A", "B", "A"]);
        // The same name leans the same way each time.
        let again = scraps("wired");
        assert_eq!(
            model.iter().map(|scrap| scrap.rotation).collect::<Vec<_>>(),
            again.iter().map(|scrap| scrap.rotation).collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_long_or_non_latin_name_keeps_its_rest_whole() {
        RANSOM.with_borrow_mut(|ransom| {
            ransom.families = vec!["A".into(), "B".into()];
            ransom.papers = vec![(Color::default(), Color::default())];
        });
        let long = "abcdefghijklmnopqrstuvwxyz0123";
        let model = scraps(long);
        let cut = letters(&model);
        assert_eq!(cut.len(), MOST_LETTERS + 1);
        assert_eq!(cut.last().unwrap(), "mnopqrstuvwxyz0123");
        assert_eq!(letters(&scraps("ワイヤード")), ["ワイヤード"]);
    }
}
