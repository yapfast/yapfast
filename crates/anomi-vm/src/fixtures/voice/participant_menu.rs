//! Samples of a participant's menu for tests and shots, after the fake
//! Discord's data: task V11's. The card is `vm.voice.participant_menu`; the
//! rows under it are the window's context menu for the target
//! `in-voice:<room>/<user>` (`src/ui/menus.rs`), as `menu/voice_person.png`
//! has them, each sending the choice its id names.

use iced::{Point, Size};
use yapfast_lang::tr;

use crate::Vm;
use crate::fixtures::shell;
use crate::icon::Icon;
use crate::menu::{Menu, Row, Spot};
use crate::shell::Layer;
use crate::voice::participant_menu::ParticipantMenu;

use super::controls;

/// Where `voice/participant_menu.png` right-clicked Carol's tile, and the
/// rows' size and the window's there: the card lands at (670, 8).
pub const POINTER: Point = Point::new(670.0, 550.0);
const ROWS: Size = Size::new(WIDTH, 644.0);
const WINDOW: Size = Size::new(1400.0, 860.0);

/// The width the rows give the card in the golden window.
pub const WIDTH: f32 = 274.0;

/// Lounge's room id and Carol's user id, as the fake has them.
const ROOM: &str = "discord:c3";
pub const CAROL: &str = "discord:u3";

fn target() -> String {
    format!("in-voice:{ROOM}/{CAROL}")
}

fn row(text: &str, id: &str) -> Row {
    Row::new(text, crate::menu::Choice::person(target(), id).into())
}

/// What a call's menu offers someone this account is friends with, in
/// the order `voice/participant_menu.png` shows (`menu` and `label` in
/// `src/ui/menus.rs`), with the moderator's rows at the end.
pub fn rows(server_muted: bool) -> Vec<Row> {
    vec![
        row(tr!("Profile"), "profile").icon(Icon::User),
        row(tr!("Mention"), "mention").icon(Icon::AtSign),
        row(yapfast_lang::text_in("Menu", "Message"), "message").icon(Icon::MessageCircle),
        row(tr!("Start call"), "call").icon(Icon::Phone),
        row(tr!("Add note"), "add-note")
            .subtitle(tr!("Only visible to you"))
            .icon(Icon::StickyNote),
        row(tr!("Add friend nickname"), "add-nickname").icon(Icon::UserPen),
        row(tr!("Mute"), "local-mute")
            .icon(Icon::MicOff)
            .checkbox(false)
            .separated(),
        row(tr!("Mute Soundboard"), "soundboard-mute")
            .icon(Icon::Soundboard)
            .checkbox(false),
        Row::opens(
            tr!("Invite to server"),
            vec![row("Server", "invite-to:discord:g1")],
        )
        .icon(Icon::UserPlus)
        .separated(),
        row(tr!("Remove friend"), "unfriend")
            .icon(Icon::UserMinus)
            .danger(),
        row(tr!("Ignore"), "ignore").icon(Icon::EyeOff),
        row(tr!("Block"), "block").icon(Icon::Ban).danger(),
        Row::opens(
            tr!("Roles"),
            vec![row("Moderator", "role:r1").checkbox(false)],
        )
        .icon(Icon::Shield)
        .separated(),
        Row::opens(tr!("Move to"), vec![row("Lounge 2", "move-to:discord:c4")]).icon(Icon::Forward),
        row(tr!("Server mute"), "server-mute")
            .icon(Icon::MicOff)
            .checkbox(server_muted)
            .danger()
            .separated(),
        row(tr!("Server deafen"), "server-deafen")
            .icon(Icon::HeadphoneOff)
            .checkbox(false)
            .danger(),
        row(tr!("Disconnect"), "disconnect")
            .icon(Icon::PhoneOff)
            .danger(),
    ]
}

/// The card for Carol, at her usual volumes.
pub fn card() -> ParticipantMenu {
    ParticipantMenu {
        person: CAROL.into(),
        name: "Carol".into(),
        joined: true,
        width: WIDTH,
        ..ParticipantMenu::default()
    }
}

/// The window with the card open at the pointer and the rows right under.
fn open(card: ParticipantMenu, rows: Vec<Row>) -> Vm {
    let mut vm = controls::window(controls::joined());
    let (top, below) = card.place(POINTER, ROWS, WINDOW);
    vm.menu = Menu::new(1, Spot::pointer(below), rows);
    vm.voice.participant_menu = card;
    shell::open_at(vm, Layer::ParticipantMenu, Spot::pointer(top))
}

/// `voice/participant_menu.png`: Carol's menu.
pub fn carol() -> Vm {
    open(card(), rows(false))
}

/// Someone sharing their screen: a second slider, for their stream.
pub fn sharing() -> Vm {
    open(
        ParticipantMenu {
            live: true,
            stream_volume: 60,
            ..card()
        },
        rows(false),
    )
}

/// Only the card, as it shows without a menu under it (a call that offers
/// no rows to this account).
pub fn alone() -> Vm {
    let mut vm = controls::window(controls::joined());
    let card = ParticipantMenu {
        joined: false,
        width: crate::voice::participant_menu::WIDTH,
        ..card()
    };
    let (top, _) = card.place(POINTER, Size::ZERO, WINDOW);
    vm.voice.participant_menu = card;
    shell::open_at(vm, Layer::ParticipantMenu, Spot::pointer(top))
}
