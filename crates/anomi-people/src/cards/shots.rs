//! The cards for `anomi-shots`, each the whole window with one card open
//! beside the member list's third row. No golden screenshot has them; they
//! are checked against the Slint app under `scripts/ui-session`.

use anomi_vm::fixtures::people::cards as samples;

use crate::shots::{GOLDEN, Shot};

pub fn shots() -> Vec<Shot> {
    let shot = |name, vm| Shot {
        area: "card",
        name,
        size: GOLDEN,
        theme: "bonfire",
        vm,
    };
    vec![
        shot("server", || {
            samples::server_over(samples::showing_server(samples::server()))
        }),
        shot("server_plain", || {
            samples::server_over(samples::showing_server(samples::server_plain()))
        }),
        shot("tag", || {
            samples::server_over(samples::showing_tag(samples::tag()))
        }),
        shot("tag_loading", || {
            samples::server_over(samples::showing_tag(samples::tag_loading()))
        }),
        shot("tag_limited", || {
            samples::server_over(samples::showing_tag(samples::tag_limited()))
        }),
        shot("tag_failed", || {
            samples::server_over(samples::showing_tag(samples::tag_failed()))
        }),
        shot("tag_stranger", || {
            samples::server_over(samples::showing_tag(samples::tag_stranger()))
        }),
        shot("role", || {
            samples::role_over(samples::showing_role(samples::role()))
        }),
        shot("role_plain", || {
            samples::role_over(samples::showing_role(samples::role_plain()))
        }),
        shot("role_loading", || {
            samples::role_over(samples::showing_role(samples::role_loading()))
        }),
        shot("role_empty", || {
            samples::role_over(samples::showing_role(samples::role_empty()))
        }),
        shot("role_many", || {
            samples::role_over(samples::showing_role(samples::role_many()))
        }),
    ]
}
