//! The card itself (`ui/yapfast.slint:3014`-`:3130`): 57 px high, 8 px
//! round, its nameplate over the panel ground, the avatar cell 50 px wide,
//! the name's box beside it, then the buttons 5 px apart and 14 px from
//! the right edge.

use iced::widget::{column, container, row, space, stack, text};
use iced::{Alignment, Fill, Length, Padding};

use anomi_theme::prelude::*;
use anomi_theme::{Palette, Theme, catalog, metrics, typography};
use anomi_vm::account::{Account, Intent};
use anomi_vm::kind::Presence;
use anomi_vm::menu::Spot;
use anomi_widgets::avatar::{Ground, portrait};
use anomi_widgets::button::{device_toggle, icon_button};
use anomi_widgets::icon::Icon;
use anomi_widgets::menu::opener;
use anomi_widgets::nameplate::{Place, nameplate};
use anomi_widgets::presence::status_text;
use yapfast_lang::tr;

use super::anchored::anchored;
use super::pointed::pointed;

/// The avatar's cell, and the dot of the sync's own state in it.
const CELL: f32 = 50.0;
const AVATAR_X: f32 = 8.0;
const DOT: f32 = 14.0;
const DOT_CORE: f32 = 8.0;
const DOT_X: f32 = 35.0;
/// The caret's strip at the end of a device toggle (`ui/controls.slint:179`).
const CARET: f32 = 20.0;
/// Discord's glyphs are white and a little larger than the default.
const EARS_GLYPH: f32 = 21.0;
const COG_GLYPH: f32 = 24.0;
/// The dock's rule stops short of the card's sides (`x: 12px`).
const RULE_INSET: f32 = 12.0;
const SPACING: f32 = 5.0;
const PAD_RIGHT: f32 = 14.0;

pub fn view(account: &Account, motion: Motion) -> impl View<Intent> + '_ {
    // At rest nothing moves: the avatar, its decoration and the status's
    // emoji play while the pointer is on the card (`animate: account-panel.hot`).
    let still = Motion::default();
    let rest = lines(name(account), status_line(account, still));
    // Under the pointer the status gives way to the username, as Discord's
    // does; with none to show the pointer changes nothing.
    let hovered: Element<'_, Intent> = if account.username.is_empty() {
        lines(name(account), status_line(account, motion)).boxed()
    } else {
        let username = text(account.username.as_str())
            .size(typography::size::LABEL)
            .style(catalog::text::muted)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End);
        lines(name(account), username).boxed()
    };
    let identity = pointed(
        lead(account, still).boxed(),
        lead(account, motion).boxed(),
        rest.boxed(),
        hovered,
        account.linked,
        motion,
    );
    let identity: Element<'_, Intent> = if account.linked {
        opener(identity)
            .on_press(|card| Intent::Popover(Spot::above(card)))
            .boxed()
    } else {
        identity.boxed()
    };
    let mut buttons = row![identity].spacing(SPACING).align_y(Alignment::Center);
    buttons = buttons.push(microphone(account, motion).boxed());
    buttons = buttons.push(headphones(account, motion).boxed());
    if account.offline() {
        buttons = buttons.push(
            icon_button(Icon::RefreshCw, tr!("Reconnect"), motion)
                .enabled(!account.busy)
                .on_press(Intent::Reconnect)
                .view()
                .boxed(),
        );
    }
    buttons = buttons.push(
        icon_button(Icon::Settings, tr!("Settings"), motion)
            .radius(metrics::RADIUS_FIELD)
            .glyph(COG_GLYPH)
            .tint(|palette| palette.text)
            .on_press(Intent::Settings)
            .view()
            .boxed(),
    );
    let corner = if account.docked {
        0.0
    } else {
        metrics::RADIUS_CARD
    };
    let content = container(buttons.height(Fill))
        .padding(Padding::default().right(PAD_RIGHT))
        .width(Fill)
        .height(metrics::ACCOUNT_HEIGHT);
    // The nameplate fills the card, its top corners as square as the
    // card's under the call's dock.
    let plate = nameplate(content, Place::Account, motion)
        .plate(&account.plate)
        .radius(metrics::RADIUS_CARD)
        .top_radius(corner)
        .ground(|palette: &Palette| palette.panel);
    // Under the call's dock a rule divides them, unless a nameplate's own
    // colour does (`yapfast.slint:3036`).
    let mut layers = stack![plate];
    if account.docked && !account.plate.is_some() {
        layers = layers.push(
            container(
                container(space())
                    .width(Fill)
                    .height(1)
                    .style(|theme: &Theme| container::Style {
                        background: Some(theme.palette().line.into()),
                        ..container::Style::default()
                    }),
            )
            .padding(Padding::default().left(RULE_INSET).right(RULE_INSET))
            .width(Fill)
            .boxed(),
        );
    }
    layers.width(Fill).height(metrics::ACCOUNT_HEIGHT)
}

fn name(account: &Account) -> impl View<Intent> + '_ {
    text(account.name.as_str())
        .size(typography::size::BODY)
        .font(typography::BOLD)
        .style(catalog::text::primary)
        .wrapping(text::Wrapping::None)
        .ellipsis(text::Ellipsis::End)
}

/// Their custom status or what they play, with its emoji or icon.
fn status_line(account: &Account, motion: Motion) -> impl View<Intent> + '_ {
    status_text(&account.status, motion)
        .bullet(account.bullet)
        .view()
}

/// The name above its second line, centred in the box.
fn lines<'a>(
    name: impl View<Intent> + 'a,
    second: impl View<Intent> + 'a,
) -> impl View<Intent> + 'a {
    container(column![name, second])
        .width(Fill)
        .height(Fill)
        .center_y(Fill)
        .clip(true)
}

/// The avatar with its presence in a cell of its own; without Discord's
/// status the dot shows the sync's: the accent in the preview, green
/// online, grey offline.
fn lead(account: &Account, motion: Motion) -> impl View<Intent> + '_ {
    let face = portrait(&account.avatar, metrics::AVATAR_ACCOUNT)
        .ground(Ground::Panel)
        .presence(account.presence)
        // Slint's 16 px `StatusDot` at x 34, y height / 2 + 3 (`:3043`).
        .dot(16.0, (26.0, 23.0))
        .decoration(account.decoration.as_ref())
        .speaking(account.speaking)
        .motion(motion)
        .view();
    let cell = container(face)
        .padding(Padding::default().left(AVATAR_X))
        .width(CELL)
        .height(metrics::ACCOUNT_HEIGHT)
        .center_y(metrics::ACCOUNT_HEIGHT);
    let dot: Element<'_, Intent> = if account.presence == Presence::None {
        let (preview, online) = (account.preview, account.online);
        container(
            container(space())
                .width(DOT_CORE)
                .height(DOT_CORE)
                .style(move |theme: &Theme| sync_dot(theme, preview, online)),
        )
        .center(DOT)
        .style(|theme: &Theme| container::Style {
            background: Some(theme.palette().panel.into()),
            border: iced::Border::default().rounded(DOT / 2.0),
            ..container::Style::default()
        })
        .boxed()
    } else {
        space().boxed()
    };
    // Its centre sits 4 px under the cell's.
    let dot = container(dot).padding(Padding {
        top: metrics::ACCOUNT_HEIGHT / 2.0 + 4.0,
        left: DOT_X,
        ..Padding::ZERO
    });
    stack![cell, dot]
        .width(Length::Fixed(CELL))
        .height(Length::Fixed(metrics::ACCOUNT_HEIGHT))
}

fn sync_dot(theme: &Theme, preview: bool, online: bool) -> container::Style {
    let palette = theme.palette();
    let color = if preview {
        palette.accent
    } else if online {
        palette.live
    } else {
        palette.faint
    };
    container::Style {
        background: Some(color.into()),
        border: iced::Border::default().rounded(DOT_CORE / 2.0),
        ..container::Style::default()
    }
}

fn microphone(account: &Account, motion: Motion) -> impl View<Intent> + '_ {
    let label = match (account.in_call, account.muted) {
        (true, true) => tr!("Unmute"),
        (true, false) => tr!("Mute"),
        (false, true) => tr!("Join calls unmuted"),
        (false, false) => tr!("Join calls muted"),
    };
    let mut pair = device_toggle(
        if account.muted {
            Icon::MicOff
        } else {
            Icon::Mic
        },
        label,
        tr!("Input options"),
        motion,
    )
    .on_toggle(Intent::ToggleMute)
    .alert(account.muted)
    .enabled(account.ready || !account.in_call)
    .open(account.input_open);
    pair.tint = |palette| palette.text;
    let pair = pair.view();
    anchored(pair, CARET, |pair| Intent::InputMenu(Spot::above(pair)))
}

fn headphones(account: &Account, motion: Motion) -> impl View<Intent> + '_ {
    let label = match (account.in_call, account.deafened) {
        (true, true) => tr!("Undeafen"),
        (true, false) => tr!("Deafen"),
        (false, true) => tr!("Join calls undeafened"),
        (false, false) => tr!("Join calls deafened"),
    };
    let mut pair = device_toggle(
        if account.deafened {
            Icon::HeadphoneOff
        } else {
            Icon::Headphones
        },
        label,
        tr!("Output options"),
        motion,
    )
    .on_toggle(Intent::ToggleDeafen)
    .alert(account.deafened)
    .enabled(account.ready || !account.in_call)
    .open(account.output_open);
    pair.tint = |palette| palette.text;
    pair.glyph = EARS_GLYPH;
    anchored(pair.view(), CARET, |pair| {
        Intent::OutputMenu(Spot::above(pair))
    })
}
