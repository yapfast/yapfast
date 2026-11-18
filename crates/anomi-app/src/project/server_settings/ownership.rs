//! Delete Server and Transfer Ownership from the core's dialog
//! (`ownership_prompt`, `src/ui/server_settings.rs:333`): `Layer::Ownership`
//! is open while the core asks one. Its field takes the keys as the dialog
//! opens asking for the server's name, and as the emailed code's step comes.

use yapfast_core::application::{Application, Ownership as Asked, ServerSettings};
use yapfast_lang::tr;

use anomi_vm::settings::Settings;
use anomi_vm::settings::ownership::{Kind, Ownership};
use anomi_vm::shell::{Layer, Shell};
use anomi_vm::text::{Builder, Style, Text};

use crate::project::rows;

/// Marks where a name goes in a translated sentence, to set it in bold.
const FIRST: &str = "\u{1}";
const SECOND: &str = "\u{2}";

/// A translated sentence with `FIRST` and `SECOND` standing for names, the
/// names set in bold.
fn bold(sentence: &str, first: &str, second: &str) -> Text {
    let strong = Style {
        bold: true,
        ..Style::default()
    };
    let mut builder = Builder::new();
    for (index, part) in sentence.split(FIRST).enumerate() {
        if index > 0 {
            builder.text(first, strong);
        }
        for (inner, words) in part.split(SECOND).enumerate() {
            if inner > 0 {
                builder.text(second, strong);
            }
            if !words.is_empty() {
                builder.text(words, Style::default());
            }
        }
    }
    builder.finish()
}

/// The dialog, while the core asks one.
pub(super) fn show(
    core: &Application,
    settings: &ServerSettings,
    all: &mut Settings,
    shell: &mut Shell,
    focus: &mut Option<&'static str>,
) {
    let vm = &mut all.ownership;
    let Some(asked) = &settings.ownership else {
        if shell.is_open(Layer::Ownership) {
            shell.close(Layer::Ownership);
        }
        return;
    };
    let state = &core.state;
    let server = state
        .servers
        .iter()
        .find(|known| known.id == settings.server)
        .map_or("", |known| known.name.as_str());
    let was = (vm.kind, vm.code);
    let now = match asked {
        Asked::Delete(ask) => Ownership {
            kind: Kind::Delete,
            title: tr!("Delete '{}'", ask.name),
            text: bold(
                &tr!(
                    "Are you sure you want to delete {}? This action cannot be undone.",
                    FIRST
                ),
                &ask.name,
                "",
            ),
            by_name: ask.by_name,
            wrong: ask.wrong,
            typed: ask.typed.clone(),
            sending: ask.sending,
            may_confirm: !ask.sending,
            error: ask.error.clone().unwrap_or_default(),
            ..Ownership::default()
        },
        Asked::Transfer(ask) => {
            let to = &ask.to;
            // Discord's tag: the username.
            let tag = to.username.as_deref().unwrap_or(&to.name);
            let me = state.account_name.as_deref().unwrap_or_default();
            let mine = state.account_avatar.as_deref();
            Ownership {
                kind: Kind::Transfer,
                title: tr!("Transfer Ownership").to_owned(),
                text: bold(
                    &tr!(
                        "This will transfer ownership of {} to {}. This cannot be undone!",
                        FIRST,
                        SECOND
                    ),
                    server,
                    tag,
                ),
                typed: ask.code.clone().unwrap_or_default(),
                owner: rows::avatar(state, me, me, mine),
                new_owner: rows::avatar(state, &to.name, &to.id, Some(&to.avatar)),
                acknowledge: bold(
                    &tr!(
                        "I acknowledge that by transferring ownership of this server to {}, it officially belongs to them.",
                        FIRST
                    ),
                    tag,
                    "",
                ),
                agreed: ask.agreed,
                verified: ask.verified,
                code: ask.code.is_some(),
                resending: ask.resending,
                resent: ask.resent,
                sending: ask.sending,
                may_confirm: ask.ready(),
                error: ask.error.clone().unwrap_or_default(),
                ..Ownership::default()
            }
        }
    };
    if *vm != now {
        *vm = now;
    }
    let opened = !shell.is_open(Layer::Ownership);
    if opened {
        shell.open(Layer::Ownership);
    }
    let name = anomi_settings::server::ownership::NAME;
    let code = anomi_settings::server::ownership::CODE;
    if opened && vm.by_name {
        *focus = Some(name);
    } else if vm.code && (opened || was != (Kind::Transfer, true)) {
        *focus = Some(code);
    }
}
