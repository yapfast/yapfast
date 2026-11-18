//! The dialogs a clicked Discord link opens: a scheduled event, a gift, an
//! application's authorization. What they say is the core's
//! (`AppState::{event_prompt, gift_prompt, oauth_prompt}`), which
//! `show_event`, `show_gift` and `show_oauth` (`src/ui/refresh.rs:2860`,
//! `:2880`, `:2900`) copy into the window; here into the view-models, set
//! when they differ.

use yapfast_core::application::AppState;

use anomi_vm::places::event::{self, Event};
use anomi_vm::places::gift::{self, Gift};
use anomi_vm::places::oauth::{self, OAuth, Servers};
use anomi_vm::shell::{Layer, Shell};

use super::follow;
use crate::pictures::Requests;
use crate::project::rows;

pub(super) fn event(state: &AppState, twelve_hour: bool, shell: &mut Shell, event: &mut Event) {
    let prompt = state.event_prompt(twelve_hour);
    follow(shell, Layer::Event, prompt.is_some());
    let Some(prompt) = prompt else {
        return;
    };
    let shown = Event {
        title: prompt.title.to_owned(),
        found: prompt.found.then_some(event::Found {
            name: prompt.name,
            when: prompt.when,
            detail: prompt.detail,
        }),
        description: prompt.description,
        confirm: prompt.confirm.to_owned(),
        busy: prompt.busy,
        status: prompt.status.to_owned(),
        error: prompt.error,
    };
    if *event != shown {
        *event = shown;
    }
}

pub(super) fn gift(state: &AppState, shell: &mut Shell, gift: &mut Gift) {
    let prompt = state.gift_prompt();
    follow(shell, Layer::Gift, prompt.is_some());
    let Some(prompt) = prompt else {
        return;
    };
    let shown = Gift {
        title: prompt.title.to_owned(),
        found: prompt.found.then_some(gift::Found {
            name: prompt.name,
            detail: prompt.detail,
        }),
        confirm: prompt.confirm.to_owned(),
        browser: prompt.browser,
        busy: prompt.busy,
        status: prompt.status.to_owned(),
        error: prompt.error,
    };
    if *gift != shown {
        *gift = shown;
    }
}

/// The authorization: the application's picture is wanted, and Copy reads
/// "Copied" until the address changes.
pub(super) fn oauth(
    state: &AppState,
    shell: &mut Shell,
    oauth: &mut OAuth,
    requests: &mut Requests,
) {
    let prompt = state.oauth_prompt();
    let opened = follow(shell, Layer::OAuth, prompt.is_some());
    let Some(prompt) = prompt else {
        return;
    };
    if let Some(icon) = prompt.icon.as_deref() {
        requests.want(state, [icon]);
    }
    let app = prompt.found.then(|| oauth::App {
        avatar: rows::avatar(state, &prompt.name, &prompt.name, prompt.icon.as_deref()),
        name: prompt.name.clone(),
        detail: prompt.detail.clone(),
    });
    let servers = prompt.guild_picker.then(|| Servers {
        names: prompt.guilds.iter().map(|(_, name)| name.clone()).collect(),
        chosen: prompt.guild_chosen,
    });
    let shown = OAuth {
        title: prompt.title,
        app,
        wants: prompt.wants,
        warning: prompt.warning,
        servers,
        server: prompt.guild_name,
        copied: oauth.copied && !opened && oauth.address == prompt.address,
        address: prompt.address,
        confirm: prompt.confirm.to_owned(),
        busy: prompt.busy,
        status: prompt.status.to_owned(),
        error: prompt.error,
    };
    if *oauth != shown {
        *oauth = shown;
    }
}
