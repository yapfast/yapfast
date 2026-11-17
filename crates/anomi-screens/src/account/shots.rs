//! The card and its popover for `anomi-shots`, each the whole window.

use anomi_vm::Vm;
use anomi_vm::account::Account;
use anomi_vm::fixtures::{account, shell, voice};
use anomi_vm::kind::Nameplate;
use anomi_vm::menu::Spot;
use anomi_vm::shell::Layer;
use anomi_widgets::nameplate::sample;
use iced::{Point, Rectangle, Size};

use crate::shots::{GOLDEN, Shot};

/// The shots: the popover over a server (`status/menu.png`), the card with
/// its nameplate, the card in a call with the microphone muted, offline and
/// in the preview, the popover unfolded, with a profile theme, and the microphone's pair
/// open under the voice dock.
pub fn shots() -> Vec<Shot> {
    let shot = |area, name, vm: fn() -> Vm| Shot {
        area,
        name,
        size: GOLDEN,
        theme: "bonfire",
        vm,
    };
    vec![
        shot("status", "menu", menu),
        shot("status", "submenu", submenu),
        shot("status", "menu_playing", || {
            with_popover(account::unfolded())
        }),
        shot("status", "menu_themed", || with_popover(account::themed())),
        shot("account", "card_plated", plated),
        shot("account", "muted", || card(account::muted())),
        shot("account", "preview", || card(account::preview())),
        shot("account", "offline", || card(account::offline())),
        shot("account", "mic_open", mic_open),
    ]
}

/// The window with `account` for its card.
fn card(account: Account) -> Vm {
    Vm {
        account,
        ..shell::server_home()
    }
}

/// `voice/mic_device_menu.png`: in a call under the voice dock, the
/// microphone's pair open: lifted ground, the caret up, square top corners.
fn mic_open() -> Vm {
    let mut vm = voice::dock::window(voice::dock::joined());
    // Alice's pink plate sits under the dock in the reference, so no rule.
    vm.account = plain_plate(Account {
        docked: true,
        in_call: true,
        input_open: true,
        ..account::account()
    });
    vm
}

/// The popover above the card, from the card's left edge.
fn with_popover(account: Account) -> Vm {
    let at = Rectangle::new(Point::new(8.0, 795.0), Size::new(358.0, 57.0));
    shell::open_at(card(account), Layer::Account, Spot::above(at))
}

fn menu() -> Vm {
    with_popover(plain_plate(account::account()))
}

/// `status/submenu.png`: the four statuses unfolded, Online chosen, no game.
fn submenu() -> Vm {
    let mut account = account::account();
    account.popover.expanded = true;
    with_popover(plain_plate(account))
}

/// Alice's card on the DM home, with her pink nameplate behind it.
fn plated() -> Vm {
    let mut vm = shell::dm_home();
    vm.account = plain_plate(account::account());
    vm
}

fn plain_plate(mut account: Account) -> Account {
    account.plate = Nameplate {
        picture: Some(sample::strip("bubble_gum")),
        palette: sample::palette("bubble_gum"),
    };
    account
}
