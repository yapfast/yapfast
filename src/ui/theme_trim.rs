//! A theme's trim on the window, through the Slint global `Trim`
//! (`ui/theme_trim.slint`): heading bars, striped rows, the conversation's
//! own bar, words on cards' tabs, the member list as tiles, profile card
//! sections as boxes, the small and time faces, marks by role and notice,
//! the status line and the sidebar as one tree. See `docs/themes.md`.
//!
//! [`reset`] puts every value back to stock, which `theme_look::reset` calls
//! first whenever a theme is drawn, cleared or hidden behind a protected
//! screen; a theme that sets none of these leaves them so, and the window
//! builds nothing for them. Art on the new surfaces (`header`, `label`,
//! `footer`) and lit body text come from `theme_art`.

use std::cell::Cell;

use slint::{Brush, Color, ComponentHandle, ModelRc, SharedString, VecModel};
use yapfast_core::themes::chrome::{Label, SystemKind, fill};
use yapfast_core::themes::layout::{MembersStyle, RailSide};
use yapfast_core::themes::package::{Paint, Resolved, Role, Shape, Surface};
use yapfast_ui::{ArtImage, ArtText, Frame, Trim, YapfastWindow};

thread_local! {
    /// Opening a folder in the tree closes the others.
    static EXPAND_ONE: Cell<bool> = const { Cell::new(false) };
}

/// Answers `Trim.tab` and `Trim.status-line`. Called once per window.
pub fn init(window: &YapfastWindow) {
    let trim = window.global::<Trim>();
    trim.on_tab(|template, name, time| fill(&template, &[("name", &name), ("time", &time)]).into());
    let weak = window.as_weak();
    trim.on_status_line(move |template, channel, server, typing, _tick| {
        let Some(window) = weak.upgrade() else {
            return SharedString::new();
        };
        status_line(&window, &template, &channel, &server, &typing).into()
    });
}

/// Whether opening a folder in the tree closes the others.
pub fn expand_one() -> bool {
    EXPAND_ONE.get()
}

/// Every value back to stock.
pub fn reset(window: &YapfastWindow) {
    let trim = window.global::<Trim>();
    let none = || ModelRc::new(VecModel::<ArtImage>::default());
    trim.set_label(Frame::default());
    trim.set_label_art(none());
    trim.set_stripe(Color::default());
    trim.set_stripe_channels(false);
    trim.set_stripe_members(false);
    trim.set_stripe_messages(false);
    trim.set_header(Frame::default());
    trim.set_header_art(none());
    trim.set_header_glass(false);
    trim.set_tab_on(false);
    trim.set_tab_text(SharedString::new());
    trim.set_tab_family(SharedString::new());
    trim.set_tab_size(12.);
    trim.set_tab_weight(700);
    trim.set_tab_ink(Color::default());
    trim.set_tab_ground(Color::default());
    trim.set_tab_upper(false);
    trim.set_grid(false);
    trim.set_grid_columns(3);
    trim.set_grid_size(56.);
    trim.set_section(Frame::default());
    trim.set_footer(none());
    trim.set_footer_height(0.);
    trim.set_font_small(SharedString::new());
    trim.set_size_small(-1.);
    trim.set_weight_small(-1);
    trim.set_font_time(SharedString::new());
    trim.set_size_time(-1.);
    trim.set_weight_time(-1);
    trim.set_role_marks(ModelRc::new(VecModel::<SharedString>::default()));
    trim.set_topic_before(SharedString::new());
    trim.set_topic_after(SharedString::new());
    trim.set_member_count(SharedString::new());
    trim.set_body(ArtText::default());
    trim.set_mark_join(SharedString::new());
    trim.set_mark_leave(SharedString::new());
    trim.set_mark_notice(SharedString::new());
    trim.set_status(SharedString::new());
    trim.set_status_above(false);
    trim.set_tree_root("~/".into());
    trim.set_tree_dms("dm".into());
    EXPAND_ONE.set(false);
}

/// Sets a theme's trim. `fonts` are the registered families of its type
/// roles. Called by `theme_look::paint` after [`reset`], before the art.
pub fn apply(window: &YapfastWindow, resolved: &Resolved, fonts: &[(Role, String)]) {
    let trim = window.global::<Trim>();
    let text = token(resolved, "text");
    if let Some(shape) = resolved.shapes.get(&Surface::Label) {
        trim.set_label(frame(shape, token(resolved, "sidebar"), text));
    }
    if let Some(shape) = resolved.shapes.get(&Surface::Header) {
        trim.set_header(frame(shape, token(resolved, "chat"), text));
    }
    if let Some(shape) = resolved.shapes.get(&Surface::CardSection) {
        trim.set_section(frame(shape, token(resolved, "raised"), text));
    }
    if let Some(stripe) = resolved
        .shapes
        .get(&Surface::Row)
        .and_then(|shape| shape.stripe)
    {
        trim.set_stripe(Color::from_argb_encoded(stripe.colour));
        trim.set_stripe_channels(stripe.channels);
        trim.set_stripe_members(stripe.members);
        trim.set_stripe_messages(stripe.messages);
    }
    let layout = &resolved.layout;
    if let Some(tab) = &layout.cards.tab {
        trim.set_tab_on(true);
        trim.set_tab_text(tab.text.as_str().into());
        let role = tab.role.unwrap_or(Role::Labels);
        if let Some((_, family)) = fonts.iter().find(|(font_role, _)| *font_role == role) {
            trim.set_tab_family(family.as_str().into());
        }
        let kind = resolved.types.get(&role);
        trim.set_tab_size(tab.size.or(kind.and_then(|kind| kind.size)).unwrap_or(12.));
        trim.set_tab_weight(
            tab.weight
                .or(kind.and_then(|kind| kind.weight))
                .unwrap_or(700),
        );
        trim.set_tab_ink(Color::from_argb_encoded(tab.colour.unwrap_or(text)));
        trim.set_tab_ground(Color::from_argb_encoded(tab.ground.unwrap_or(0)));
        trim.set_tab_upper(tab.uppercase);
    }
    if layout.members_style == Some(MembersStyle::Grid) {
        trim.set_grid(true);
        trim.set_grid_columns(layout.grid.columns as i32);
        trim.set_grid_size(layout.grid.size);
    }
    if layout.rail == Some(RailSide::Tree) {
        trim.set_tree_root(layout.tree.root.as_str().into());
        trim.set_tree_dms(layout.tree.dms.as_str().into());
        EXPAND_ONE.set(!layout.tree.expand_many);
    }
    for (role, family) in fonts {
        match role {
            Role::Small => trim.set_font_small(family.as_str().into()),
            Role::Time => trim.set_font_time(family.as_str().into()),
            _ => {}
        }
    }
    for (role, kind) in &resolved.types {
        let (size, weight) = (kind.size.unwrap_or(-1.), kind.weight.unwrap_or(-1));
        match role {
            Role::Small => {
                trim.set_size_small(size);
                trim.set_weight_small(weight);
            }
            Role::Time => {
                trim.set_size_time(size);
                trim.set_weight_time(weight);
            }
            _ => {}
        }
    }
    let chrome = &resolved.chrome;
    if !chrome.member_by_role.is_empty() {
        let marks: Vec<SharedString> = chrome
            .member_by_role
            .iter()
            .map(|mark| kept(mark).into())
            .collect();
        trim.set_role_marks(ModelRc::new(VecModel::from(marks)));
    }
    if let Some(topic) = chrome.labels.get(&Label::Topic) {
        let (before, after) = topic.split_once("{topic}").unwrap_or((topic, ""));
        trim.set_topic_before(kept(before).into());
        trim.set_topic_after(kept(after).into());
    }
    if let Some(count) = chrome.labels.get(&Label::MemberCount) {
        trim.set_member_count(kept(count).into());
    }
    for (kind, mark) in &chrome.system {
        let mark: SharedString = kept(mark).into();
        match kind {
            SystemKind::Join => trim.set_mark_join(mark),
            SystemKind::Leave => trim.set_mark_leave(mark),
            SystemKind::Notice => trim.set_mark_notice(mark),
        }
    }
    if let Some(line) = &chrome.status_line {
        trim.set_status(kept(&line.text).into());
        trim.set_status_above(line.above);
    }
}

/// Text as a theme wrote it, with the spaces at its ends kept: Slint's
/// layout drops a line's spaces at its ends, so they become no-break spaces.
pub fn kept(text: &str) -> String {
    let start = text.len() - text.trim_start_matches(' ').len();
    let end = text.trim_end_matches(' ').len().max(start);
    let mut out = String::with_capacity(text.len() + 4);
    out.extend(std::iter::repeat_n('\u{a0}', start));
    out.push_str(&text[start..end]);
    out.extend(std::iter::repeat_n('\u{a0}', text.len() - end));
    out
}

/// A colour token's value in the theme, ARGB.
fn token(resolved: &Resolved, name: &str) -> u32 {
    super::theme_look::token_value(resolved, name).unwrap_or(0xff00_0000)
}

/// A framed surface's look: its ground (the stock one when the theme sets
/// none), its text, corners, border and shadow, and a section's bar.
fn frame(shape: &Shape, ground: u32, text: u32) -> Frame {
    let paint = |paint: Option<&Paint>, stock: u32| match paint {
        Some(paint) => super::theme_look::brush(paint),
        None => Brush::SolidColor(Color::from_argb_encoded(stock)),
    };
    let (blur, x, y, colour) = shape.shadow.map_or((-1., 0., 0., 0), |shadow| {
        (shadow.blur, shadow.x, shadow.y, shadow.colour)
    });
    let ink = shape.text.unwrap_or(text);
    Frame {
        on: true,
        ground: paint(shape.ground.as_ref(), ground),
        text: Color::from_argb_encoded(ink),
        radius: shape.radius.unwrap_or(0.),
        border: shape.border.unwrap_or(0.),
        border_color: Color::from_argb_encoded(shape.border_colour.unwrap_or(0)),
        shadow_blur: blur,
        shadow_x: x,
        shadow_y: y,
        shadow_color: Color::from_argb_encoded(colour),
        bar: paint(shape.bar.as_ref(), 0),
        bar_text: Color::from_argb_encoded(shape.bar_text.unwrap_or(ink)),
        case: i32::from(shape.case.unwrap_or(0)),
    }
}

/// The status line's text with its placeholders filled.
fn status_line(
    window: &YapfastWindow,
    template: &str,
    channel: &str,
    server: &str,
    typing: &str,
) -> String {
    use slint::Model as _;
    let time = yapfast_core::clock::time_on(
        yapfast_core::clock::now_ms() / 1000,
        window.get_twelve_hour_clock(),
    );
    let name = window.get_account_name();
    let unread: Vec<String> = window
        .get_rooms()
        .iter()
        .filter(|room| {
            room.unread && !room.selected && room.heading.is_empty() && !room.id.is_empty()
        })
        .map(|room| room.name.to_string())
        .collect();
    fill(
        template,
        &[
            ("time", &time),
            ("name", &name),
            ("channel", channel),
            ("server", server),
            ("typing", typing),
            ("unread_channels", &unread.join(",")),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spaces_at_the_ends_are_kept() {
        assert_eq!(kept("  $ who "), "\u{a0}\u{a0}$ who\u{a0}");
        assert_eq!(kept("plain"), "plain");
        assert_eq!(kept("   "), "\u{a0}\u{a0}\u{a0}");
    }
}
