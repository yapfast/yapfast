//! Signing in and linking Discord from the core's state: the sign-in
//! screen, the verify banner, the linking dialog over the settings and the
//! two-factor prompt ([`mfa`]). The Slint side: `show_workspace`
//! (`src/ui/refresh.rs:960`, the link's phase, name, note and code) and
//! the sign-in's focus (`:589`).
//!
//! What was typed (the Matrix fields, the token, the recovery key) is the
//! window's own, kept in `vm.sign_in` by `intent::sign_in`; this sets the
//! rest, a handful of fields each compared before it is copied. The code
//! is made once for each address the core hands over. A secret goes once
//! it served: the token once the account is linked or the linking given
//! up while signed in, the password and the recovery key as their sign-in
//! starts (`intent::sign_in`); [`forget`] overwrites its bytes first.

mod mfa;

pub(crate) use mfa::method as mfa_method;

use std::path::Path;

use yapfast_core::application::{AppState, Application, DiscordLink};

use anomi_vm::Vm;
use anomi_vm::shell::Layer;
use anomi_vm::sign_in::{Code, Link, SignIn};

/// A Matrix build signs in with a password; one without links Discord.
const MATRIX: bool = cfg!(feature = "matrix");

/// What the sign-in showed last.
#[derive(Default)]
pub struct Shown {
    /// The address the code shown carries.
    code: Option<String>,
    /// The reset and the busy a saved session was looked for at.
    restore: Option<(u64, bool)>,
    /// The reset the sign-in screen showed at, for its first field to take
    /// the keys as it comes back.
    reset: Option<u64>,
    mfa: mfa::Shown,
}

/// Fills `vm.sign_in`, opens or closes the linking dialog over the
/// settings and the two-factor prompt, and says in `focus` which field
/// takes the keys, if one should now.
pub(super) fn show(
    core: &Application,
    root: &Path,
    vm: &mut Vm,
    shown: &mut Shown,
    focus: &mut Option<&'static str>,
) {
    let state = &core.state;
    let signed_out = !(state.preview || state.account.is_some());
    let sign_in = &mut vm.sign_in;
    if sign_in.product != yapfast_lang::PRODUCT {
        yapfast_lang::PRODUCT.clone_into(&mut sign_in.product);
    }
    sign_in.busy = state.busy;
    sign_in.matrix_build = MATRIX;
    let notice = state.notice.as_deref().unwrap_or_default();
    if sign_in.notice != notice {
        notice.clone_into(&mut sign_in.notice);
    }
    link(state, sign_in, &mut shown.code);
    // Linked, or the linking given up while signed in: the token, kept
    // until now to fix a refused one, goes with its field.
    let done = state.discord_linked || state.account.is_some();
    if done && state.discord_link.is_none() && !sign_in.token.is_empty() {
        forget(&mut sign_in.token);
        sign_in.token_open = false;
    }
    // Only the Matrix form offers it, and finding out reads the disk.
    let looked = (state.reset_version, state.busy);
    if MATRIX && signed_out && shown.restore != Some(looked) {
        shown.restore = Some(looked);
        sign_in.restore_available = yapfast_core::chat::can_restore(root);
    }
    // `!root.preview && !root.trusted` (`ui/yapfast.slint:3464`).
    sign_in.verify.unverified = !state.preview && !state.trusted;
    sign_in.verify.busy = state.busy;
    // Linking while signed in started on the settings' Discord page.
    let over_settings = !signed_out && state.discord_link.is_some();
    if over_settings != vm.shell.is_open(Layer::Link) {
        if over_settings {
            vm.shell.open(Layer::Link);
        } else {
            vm.shell.close(Layer::Link);
        }
    }
    // Back on the sign-in screen with no account, Matrix's ID takes the
    // keys (`focus-sign-in`).
    if signed_out && shown.reset != Some(state.reset_version) {
        shown.reset = Some(state.reset_version);
        if MATRIX {
            *focus = Some(anomi_screens::sign_in::USERNAME);
        }
    }
    mfa::show(
        core,
        &mut vm.shell,
        &mut vm.settings.mfa,
        &mut shown.mfa,
        focus,
    );
}

/// The two-factor prompt alone, after a keystroke in it.
pub(super) fn show_mfa(
    core: &Application,
    vm: &mut Vm,
    shown: &mut Shown,
    focus: &mut Option<&'static str>,
) {
    mfa::show(
        core,
        &mut vm.shell,
        &mut vm.settings.mfa,
        &mut shown.mfa,
        focus,
    );
}

/// Where linking has got to (`link-phase`, `-name`, `-note`, `-code`): the
/// code made again only for a new address.
fn link(state: &AppState, sign_in: &mut SignIn, shown: &mut Option<String>) {
    let same = match (&state.discord_link, &sign_in.link) {
        (None, Link::Idle)
        | (Some(DiscordLink::Starting), Link::Starting)
        | (Some(DiscordLink::Checking), Link::Checking)
        | (Some(DiscordLink::Captcha(_)), Link::Captcha) => true,
        (Some(DiscordLink::Code(address)), Link::Code(_)) => shown.as_ref() == Some(address),
        (Some(DiscordLink::Scanned(name)), Link::Scanned(shown)) => name == shown,
        (Some(DiscordLink::Failed(note)), Link::Failed(shown)) => note == shown,
        _ => false,
    };
    if same {
        return;
    }
    *shown = None;
    sign_in.link = match &state.discord_link {
        None => Link::Idle,
        Some(DiscordLink::Starting) => Link::Starting,
        Some(DiscordLink::Checking) => Link::Checking,
        Some(DiscordLink::Captcha(_)) => Link::Captcha,
        Some(DiscordLink::Code(address)) => {
            *shown = Some(address.clone());
            Link::Code(Code::new(address))
        }
        Some(DiscordLink::Scanned(name)) => Link::Scanned(name.clone()),
        Some(DiscordLink::Failed(note)) => Link::Failed(note.clone()),
    };
}

/// Lets go of a secret typed in, its bytes overwritten first so the freed
/// memory doesn't keep it.
pub(crate) fn forget(secret: &mut String) {
    let mut bytes = std::mem::take(secret).into_bytes();
    bytes.fill(0);
    // The zeroes are read once, so the writes above stay.
    std::hint::black_box(&bytes);
}

#[cfg(test)]
mod tests;
