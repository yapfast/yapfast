//! The Keybinds settings page: what the keybinds do, and a card for each
//! with its action, its keys and the recorder. The Slint side:
//! `KeybindSettings` and `KeyRecorder` (`ui/settings.slint:756`, `:713`).

use iced::widget::{column, container, row};
use iced::{Length, alignment};

use anomi_theme::prelude::*;
use anomi_theme::{metrics, typography};
use anomi_vm::icon::Icon;
use anomi_vm::settings::keybinds::{Action, Bind, Intent, Keybinds};
use anomi_widgets::button::{Kind, action_button, icon_button};
use yapfast_lang::tr;

use crate::kit::{heading, picker, recorder};

/// Between the page's parts (`spacing: 16px`), a card's rows (`8px`) and its
/// controls (`12px`); a card's padding.
const SPACING: f32 = 16.0;
const INSIDE: f32 = 8.0;
const CONTROLS: f32 = 12.0;
const PAD: f32 = 16.0;
/// The action column's width, the keys' box's width and its smallest.
const ACTION: f32 = 240.0;
/// A control's height.
const HEIGHT: f32 = 36.0;

/// The actions' names, in the order of [`Action::ALL`]. The drop-down
/// borrows its options for as long as the view lives, so each language's
/// list is kept once for the life of the program.
fn names() -> &'static [&'static str; 8] {
    static KEPT: std::sync::Mutex<Vec<&'static [&'static str; 8]>> =
        std::sync::Mutex::new(Vec::new());
    let now = words();
    let mut kept = KEPT.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(&same) = kept.iter().find(|&&list| *list == now) {
        return same;
    }
    let list: &'static [&'static str; 8] = Box::leak(Box::new(now));
    kept.push(list);
    list
}

fn words() -> [&'static str; 8] {
    [
        tr!("Unassigned"),
        tr!("Push to Talk (Normal)"),
        tr!("Push to Mute"),
        tr!("Toggle Mute"),
        tr!("Toggle Deafen"),
        tr!("Toggle VAD"),
        tr!("Toggle Go Live"),
        tr!("Disconnect from Voice Channel"),
    ]
}

/// What an action does, as Discord's settings say under it.
fn description(action: Action) -> &'static str {
    match action {
        Action::Unassigned => {
            tr!("This action doesn't do anything! Please choose one from the drop down.")
        }
        Action::PushToTalk => {
            tr!("Hold to temporarily enable your microphone while in Push to Talk mode.")
        }
        Action::PushToMute => {
            tr!("Hold to temporarily disable your microphone while in Voice Activity mode.")
        }
        Action::ToggleMute => tr!("Toggle your microphone's transmission on and off."),
        Action::ToggleDeafen => tr!(
            "Toggle your speaker's playback on and off. Also disables your microphone while deafened."
        ),
        Action::ToggleVoiceMode => {
            tr!("Toggle to switch between Push To Talk and Voice Activity mode.")
        }
        Action::ToggleGoLive => {
            tr!("Start sharing your screen in the call, or stop it while it runs.")
        }
        Action::LeaveCall => tr!("Leave the voice channel you're in."),
    }
}

pub fn view(page: &Keybinds, motion: Motion) -> impl View<Intent> + '_ {
    let note = if page.note.is_empty() {
        tr!(
            "Keybinds mute, deafen, talk, share your screen or leave a call from any app. Add one, choose what it does, and record its keys."
        )
    } else {
        page.note.as_str()
    };
    let top = row![
        typography::label(note).width(Length::Fill),
        container(action_button(tr!("Add a Keybind"), Kind::Primary, motion).on_press(Intent::Add))
            .align_y(alignment::Vertical::Top),
    ]
    .spacing(SPACING);
    let names = names();
    let mut parts: Vec<Element<'_, Intent>> = Vec::with_capacity(page.rows.len() + 1);
    parts.push(top.boxed());
    parts.extend(
        page.rows
            .iter()
            .enumerate()
            .map(|(index, bind)| card(page, names, index, bind, motion)),
    );
    column(parts).spacing(SPACING).width(Length::Fill)
}

/// A keybind in a rimmed card.
fn card<'a>(
    page: &'a Keybinds,
    names: &'static [&'static str; 8],
    index: usize,
    bind: &'a Bind,
    motion: Motion,
) -> Element<'a, Intent> {
    let action = column![
        heading(tr!("ACTION")),
        picker(
            names,
            bind.action.index(),
            Some(move |action| Intent::Action(index, action)),
        ),
    ]
    .spacing(INSIDE)
    .width(Length::Fill);
    let action = container(action).width(ACTION);
    let keys = column![
        heading(tr!("KEYBIND")),
        recorder(
            &bind.keys,
            page.records(index),
            &page.recorded,
            move |on| Intent::Record(index, on),
            Length::Fill,
            motion,
        ),
    ]
    .spacing(INSIDE)
    .width(Length::FillPortion(1));
    // A 32 px face with a 16 px glyph, centred on the controls' 36 px line.
    let remove = icon_button(Icon::X, tr!("Remove Keybind"), motion)
        .size(32.0)
        .glyph(16.0)
        .on_press(Intent::Remove(index))
        .view();
    let controls = row![
        action,
        keys,
        container(remove)
            .height(HEIGHT)
            .align_y(alignment::Vertical::Center)
    ]
    .spacing(CONTROLS)
    .align_y(alignment::Vertical::Bottom);
    let mut body = vec![
        controls.boxed(),
        typography::label(description(bind.action)).boxed(),
    ];
    if !bind.desktop.is_empty() {
        body.push(typography::label(bind.desktop.as_str()).boxed());
    }
    container(column(body).spacing(INSIDE))
        .padding(PAD)
        .width(Length::Fill)
        .style(|theme: &Theme| iced::widget::container::Style {
            border: iced::Border::default()
                .rounded(metrics::RADIUS_CARD)
                .width(1)
                .color(theme.palette().line),
            ..Default::default()
        })
        .boxed()
}

/// Its shots for `anomi-shots`: `settings/keybinds.png` as the Slint app
/// showed it (no keybind), and the page with rows, one recording.
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    use anomi_vm::fixtures::settings::keybinds::{listed, page};

    use crate::shots::{GOLDEN, Shot};

    [
        ("keybinds", page as fn() -> anomi_vm::Vm),
        ("keybinds_listed", listed),
    ]
    .into_iter()
    .map(|(name, vm)| Shot {
        area: "settings",
        name,
        size: GOLDEN,
        theme: "bonfire",
        vm,
    })
    .collect()
}
