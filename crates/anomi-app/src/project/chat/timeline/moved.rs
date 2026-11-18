//! What changed beside the rows' messages that names rows to build again:
//! the reaction whose people the pointer asked for, and the message asked
//! to be confirmed deleted.

use std::sync::Arc;

use yapfast_core::application::AppState;
use yapfast_core::chat::ChatMessage;

use anomi_vm::chat::timeline::group;

use super::Shown;

/// The messages a timeline's rows are made from: the channel's, or the
/// thread beside it's, so that what changes in one names no row of the
/// other.
type Messages<'a> = &'a [Arc<ChatMessage>];

/// The rows of the reaction whose people the pointer asked for, as it
/// changes or its names come.
pub(super) fn tip(state: &AppState, messages: Messages<'_>, shown: &mut Shown) -> Vec<u64> {
    let tip = state
        .reaction_tip()
        .map(|(tip, _)| (tip.item.clone(), tip.key.clone(), tip.names.is_some()));
    if tip == shown.tip {
        return Vec::new();
    }
    let rows = [&shown.tip, &tip]
        .into_iter()
        .flatten()
        .filter_map(|(item, _, _)| key_of_item(messages, item))
        .collect();
    shown.tip = tip;
    rows
}

/// The rows of the message asked to be confirmed deleted, as it changes.
pub(super) fn confirming(state: &AppState, messages: Messages<'_>, shown: &mut Shown) -> Vec<u64> {
    if state.confirm_delete == shown.confirming {
        return Vec::new();
    }
    let rows = [&shown.confirming, &state.confirm_delete]
        .into_iter()
        .flatten()
        .filter_map(|item| key_of_item(messages, item))
        .collect();
    shown.confirming.clone_from(&state.confirm_delete);
    rows
}

/// The row of the message `item` among `messages`.
pub(super) fn key_of_item(messages: Messages<'_>, item: &str) -> Option<u64> {
    messages
        .iter()
        .find(|message| message.item == item)
        .map(|message| group::key(&message.id))
}
