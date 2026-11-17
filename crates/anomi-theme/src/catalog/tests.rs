//! Each built-in widget draws with `Theme` in every built-in theme, and one
//! of each draws headless in Bonfire and Latte.

use std::borrow::Cow;

use iced::widget::{
    button, checkbox, column, combo_box, container, float, pane_grid, pick_list, progress_bar,
    qr_code, radio, row, rule, scrollable, slider, svg, table, text, text_editor, text_input,
    toggler,
};
use iced::{Length, Size};
use iced_test::Simulator;

use crate::prelude::*;
use crate::{Theme, builtin, catalog, metrics, typography};

type Item<'a> = Element<'a, ()>;

const ICON: &[u8] =
    br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 8 8"><rect width="8" height="8"/></svg>"#;

/// One of each look a catalog has, in a few rows.
fn looks<'a>(
    content: &'a text_editor::Content,
    qr: &'a qr_code::Data,
    panes: &'a pane_grid::State<u8>,
) -> Item<'a> {
    let buttons = row![
        button("a").style(catalog::button::primary).on_press(()),
        button("b").style(catalog::button::secondary).on_press(()),
        button("c").style(catalog::button::danger).on_press(()),
        button("d").style(catalog::button::ghost).on_press(()),
        button("e").style(catalog::button::success).on_press(()),
        button("f").style(catalog::button::plain).on_press(()),
        button("g").style(catalog::button::bare).on_press(()),
    ];
    let grounds = row![
        container("rail").style(catalog::container::rail),
        container("sidebar").style(catalog::container::sidebar),
        container("chat").style(catalog::container::chat),
        container("panel").style(catalog::container::panel),
        container("raised").style(catalog::container::raised),
        container("tip").style(catalog::container::tooltip),
        catalog::container::middle(text("middle")),
    ];
    column![
        buttons.spacing(4),
        grounds.spacing(4).height(Length::Shrink),
        rule::horizontal(1).style(catalog::rule::line),
        rule::horizontal(1).style(catalog::rule::outline),
        rule::horizontal(1).style(catalog::rule::outline_chat),
        scrollable(text("scrolled")).height(24),
        svg(svg::Handle::from_memory(ICON)).width(16).height(16),
        text_editor(content).placeholder("Message"),
        qr_code(qr).cell_size(2),
        table([table::column("n", |n: u8| text(n.to_string()))], [1u8, 2],),
        pane_grid(panes, |_, _, _| pane_grid::Content::new(text("pane"))).height(24),
        float(text("float")).style(catalog::float::lifted),
    ]
    .spacing(4)
    .boxed()
}

fn sample<'a>(
    options: &'a combo_box::State<String>,
    content: &'a text_editor::Content,
    qr: &'a qr_code::Data,
    panes: &'a pane_grid::State<u8>,
) -> Item<'a> {
    column![
        row![
            toggler(true).size(metrics::TOGGLE_HEIGHT).on_toggle(|_| ()),
            toggler(false)
                .size(metrics::TOGGLE_HEIGHT)
                .on_toggle(|_| ()),
            checkbox(true)
                .label("Checked")
                .size(metrics::MARK_SIZE)
                .on_toggle(|_| ()),
            checkbox(false)
                .label("Unchecked")
                .size(metrics::MARK_SIZE)
                .on_toggle(|_| ()),
            radio("A", 0, Some(0), |_| ()).size(metrics::MARK_SIZE),
            radio("B", 1, Some(0), |_| ()).size(metrics::MARK_SIZE),
        ]
        .spacing(12),
        slider(0..=200, 100, |_| ()).width(Length::Fill),
        progress_bar(0.0..=1.0, 0.6)
            .length(Length::Fill)
            .girth(metrics::PROGRESS_HEIGHT),
        progress_bar(0.0..=1.0, 0.4)
            .length(Length::Fill)
            .girth(metrics::METER_HEIGHT)
            .style(catalog::progress_bar::level),
        pick_list(Some("One"), ["One", "Two"], |item: &&str| item.to_string()).on_select(|_| ()),
        combo_box(options, "Find", None, |_| ()),
        text_input("Name", "value"),
        typography::label("A label"),
        looks(content, qr, panes),
    ]
    .spacing(12)
    .boxed()
}

fn settings() -> iced::Settings {
    iced::Settings {
        fonts: crate::fonts::ALL
            .iter()
            .map(|font| Cow::Borrowed(*font))
            .collect(),
        font: typography::UI,
        ..iced::Settings::default()
    }
}

struct Fixtures {
    options: combo_box::State<String>,
    content: text_editor::Content,
    qr: qr_code::Data,
    panes: pane_grid::State<u8>,
}

fn fixtures() -> Fixtures {
    Fixtures {
        options: combo_box::State::new(vec!["One".to_owned(), "Two".to_owned()]),
        content: text_editor::Content::new(),
        qr: qr_code::Data::new("anomi").expect("a code"),
        panes: pane_grid::State::new(0).0,
    }
}

#[test]
fn one_of_each_draws_in_every_theme() {
    let fixed = fixtures();
    for built in &builtin::BUILT_IN {
        let theme = Theme::built_in(Some(built.id));
        let mut ui = Simulator::with_settings(
            settings(),
            sample(&fixed.options, &fixed.content, &fixed.qr, &fixed.panes),
        );
        ui.draw(&theme);
    }
}

#[test]
fn bonfire_and_latte_snapshot() {
    let fixed = fixtures();
    for id in ["bonfire", "catppuccin-latte"] {
        let theme = Theme::built_in(Some(id));
        let mut ui = Simulator::with_size(
            settings(),
            Size::new(480.0, 640.0),
            sample(&fixed.options, &fixed.content, &fixed.qr, &fixed.panes),
        );
        // Open the pick list, so its menu and `Opened` faces are drawn too.
        assert!(ui.click("One").is_ok(), "{id}: the pick list");
        assert!(ui.snapshot(&theme).is_ok(), "{id}");
    }
}

#[test]
fn the_switch_is_the_slint_switch() {
    use iced::widget::toggler::Status;
    for built in &builtin::BUILT_IN {
        let theme = Theme::built_in(Some(built.id));
        let palette = theme.palette();
        let on = catalog::toggler::default(&theme, Status::Active { is_toggled: true });
        let off = catalog::toggler::default(&theme, Status::Active { is_toggled: false });
        assert_eq!(on.background, palette.accent.into(), "{}", built.id);
        assert_eq!(
            off.background,
            palette.button_default.into(),
            "{}",
            built.id
        );
        assert_eq!(on.foreground, palette.on_accent.into(), "{}", built.id);
        assert_eq!(
            on.padding_ratio * metrics::TOGGLE_HEIGHT,
            metrics::TOGGLE_KNOB_INSET
        );
        let gone = catalog::toggler::default(&theme, Status::Disabled { is_toggled: true });
        assert_eq!(gone.background, palette.accent.scale_alpha(0.5).into());
    }
}

#[test]
fn the_marks_are_ringed_and_filled_by_state() {
    use iced::widget::checkbox::Status;
    let theme = Theme::default();
    let palette = theme.palette();
    let on = catalog::checkbox::default(&theme, Status::Active { is_checked: true });
    let off = catalog::checkbox::default(&theme, Status::Active { is_checked: false });
    assert_eq!(on.background, palette.accent.into());
    assert_eq!(on.border.color, palette.accent);
    assert_eq!(off.border.color, palette.secondary);
    assert_eq!(off.border.width, metrics::MARK_RING);
}

#[test]
fn the_slider_fills_with_the_accent() {
    use iced::widget::slider::Status;
    let theme = Theme::default();
    let style = catalog::slider::default(&theme, Status::Active);
    assert_eq!(style.rail.backgrounds.0, theme.palette().accent.into());
    assert_eq!(style.rail.width, metrics::SLIDER_RAIL);
}

/// The `field: rgb(..)` and `field: rgba(..)` pairs of `const NAME: TYPE = TYPE {`
/// in a source, in order.
fn tokens(source: &str, name: &str, kind: &str) -> Vec<(String, String)> {
    let head = format!("const {name}: {kind} = {kind} {{");
    let start = source
        .find(&head)
        .unwrap_or_else(|| panic!("{name} is not a {kind}"))
        + head.len();
    let body = &source[start..];
    let body = &body[..body.find("\n};").expect("the block ends")];
    body.lines()
        .filter_map(|line| {
            let (field, value) = line.trim().split_once(':')?;
            let value = value.trim().trim_end_matches(',');
            (value.starts_with("rgb(") || value.starts_with("rgba("))
                .then(|| (field.trim().to_owned(), value.replace('_', "")))
        })
        .collect()
}

/// The twelve palettes are the Slint interface's: each token of each is the
/// token of the same name in `src/ui/theme.rs`.
#[test]
fn builtin_colours_are_the_slint_themes() {
    let slint = include_str!("../../../../src/ui/theme.rs");
    let mut checked = 0;
    for source in [
        include_str!("../builtin/graphite.rs"),
        include_str!("../builtin/community.rs"),
        include_str!("../builtin/light.rs"),
    ] {
        for line in source.lines() {
            let Some(name) = line
                .strip_prefix("pub const ")
                .or_else(|| line.strip_prefix("const "))
                .and_then(|rest| rest.strip_suffix(": Palette = Palette {"))
            else {
                continue;
            };
            let ours = tokens(source, name, "Palette");
            let theirs = tokens(slint, name, "Theme");
            assert!(!ours.is_empty(), "{name}");
            assert_eq!(ours, theirs, "{name}");
            checked += 1;
        }
    }
    assert_eq!(checked, 12);
}
