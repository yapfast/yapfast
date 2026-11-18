//! The full profile's shots for `anomi-shots`, each the whole window with
//! the modal open: Bob's Activity tab and his mutual friends and servers, as
//! the golden screenshots show them, then the states a tab can be in, which
//! have none. Bob's decoration and effect, which the widgets draw, are added
//! here as on the popout's shots.

use anomi_vm::Vm;
use anomi_vm::fixtures::people::full_profile::{self as samples, open};
use anomi_vm::people::full_profile::FullProfile;

use crate::profile::card::shots::{dress, ring};
use crate::shots::{GOLDEN, Shot};

pub fn shots() -> Vec<Shot> {
    let shot = |name, vm: fn() -> Vm| Shot {
        area: "profile",
        name,
        size: GOLDEN,
        theme: "bonfire",
        vm,
    };
    vec![
        shot("full", || dressed(samples::bob())),
        shot("full_mutual_friends", || dressed(samples::on_friends())),
        shot("full_mutual_servers", || dressed(samples::on_servers())),
        shot("full_recent", || dressed(samples::recent())),
        shot("full_board", || dressed(samples::board())),
        shot("full_wishlist", || dressed(samples::wishlist())),
        shot("full_loading", || dressed(samples::loading())),
        shot("full_empty", || dressed(samples::empty())),
        shot("full_themed", || dressed(samples::themed())),
    ]
}

/// Bob as the golden screenshots show him: his avatar's pink ring and his
/// profile effect, as the card's shots dress him.
fn dressed(mut full: FullProfile) -> Vm {
    dress(&mut full.card, &ring());
    open(full)
}
