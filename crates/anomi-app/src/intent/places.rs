//! The dialogs about places: the quick switcher, a new DM, a server's
//! invite, joining one, the dialogs a clicked link opens, leaving a server
//! or the move-sync question, and the masked link's question
//! (`src/ui/callbacks.rs:1280`, `:1312`-`:1380`, `:1578`, `:1636`,
//! `:1815`-`:1840`, `:2179`-`:2190`, `:3027`-`:3034`).
//!
//! What the core keeps becomes its `Action`; what the window kept in Slint
//! (a field's text, the invite's choices, the switcher's chosen place, the
//! "Copied" of a Copy button) stays in the view-model. A dialog closes
//! through the core, and the projection closes its layer as the core lets
//! it go ([`close`], from the shell's Escape and press outside).

#[cfg(test)]
mod tests;

use iced::Task;
use yapfast_core::application::{Action, INVITE_AGES, INVITE_USES};

use anomi_vm::Intent;
use anomi_vm::new_dm::Intent as NewDm;
use anomi_vm::places::{self, event, gift, invite, join, leave, leaving_link, oauth};
use anomi_vm::shell::Layer;
use anomi_vm::switcher::{Entry, Heading, Intent as Switcher};
use anomi_widgets::list;

use crate::app::App;
use crate::keys;
use crate::message::Message;

/// Their intents, as the window's `Intent`: `Switcher`, `NewDm` or `Places`.
pub fn handle(app: &mut App, intent: Intent) -> Task<Message> {
    match intent {
        Intent::Switcher(intent) => switcher(app, intent),
        Intent::NewDm(intent) => new_dm(app, intent),
        Intent::Places(intent) => self::places(app, intent),
        intent => ignored(&intent),
    }
}

/// Closes one of these dialogs' layers: through the core, which lets the
/// dialog go; the masked link's, the window's own, at once.
pub fn close(app: &mut App, layer: Layer) -> Task<Message> {
    let action = match layer {
        // The composer has the keys again (`ui/yapfast.slint:908`).
        Layer::Switcher => return app.dispatch(Action::CloseSwitcher).chain(keys::refocus()),
        Layer::NewDm => Action::CloseNewDm,
        Layer::Invite => Action::CloseInvite,
        Layer::Join => Action::CloseJoin,
        Layer::Event => Action::CloseEvent,
        Layer::Gift => Action::CloseGift,
        Layer::OAuth => Action::CloseOAuth,
        Layer::Leave => return close_leave(app),
        layer => {
            app.vm.shell.close(layer);
            return Task::none();
        }
    };
    app.dispatch(action)
}

fn ignored(intent: &impl std::fmt::Debug) -> Task<Message> {
    tracing::debug!(?intent, "not handled by the iced interface yet");
    Task::none()
}

/// The quick switcher: a search lists again from the first place, at the
/// list's top; the arrows choose and scroll the place into view; Enter or a
/// click goes there, and the composer has the keys again.
fn switcher(app: &mut App, intent: Switcher) -> Task<Message> {
    match intent {
        Switcher::Search(query) => {
            let switcher = &mut app.vm.switcher;
            switcher.query.clone_from(&query);
            switcher.chosen = 0;
            let task = app.dispatch(Action::SearchSwitcher(query));
            Task::batch([
                task,
                list::operation::to_start(anomi_screens::switcher::LIST),
            ])
        }
        Switcher::Choose(at) => choose(app, at),
        Switcher::Go(id) => app.dispatch(Action::SwitchTo(id)).chain(keys::refocus()),
        intent => ignored(&intent),
    }
}

/// Chooses the place at `at` and scrolls it into view: going up to the
/// first of a section, its heading shows too (`reveal`,
/// `ui/dialogs.slint:394`).
fn choose(app: &mut App, at: usize) -> Task<Message> {
    let switcher = &mut app.vm.switcher;
    let up = switcher.current().is_some_and(|before| at < before);
    switcher.chosen = at;
    let mentions = switcher.mentions.min(switcher.places.len());
    let key = match at {
        _ if !up || !switcher.headed() => Entry::Place(at).key(),
        0 if mentions > 0 => Entry::Heading(Heading::Mentions).key(),
        0 => Entry::Heading(Heading::Previous).key(),
        at if at == mentions => Entry::Heading(Heading::Previous).key(),
        at => Entry::Place(at).key(),
    };
    list::operation::jump_to(anomi_screens::switcher::LIST, key, list::Place::Reveal(0.0))
}

/// The new DM: the field searches, a row starts that person's DM, once at
/// a time.
fn new_dm(app: &mut App, intent: NewDm) -> Task<Message> {
    match intent {
        NewDm::Query(query) => {
            app.vm.new_dm.query.clone_from(&query);
            app.dispatch(Action::NewDmQuery(query))
        }
        NewDm::Start(id) => {
            let contact = (app.core.state.new_dm.as_ref())
                .filter(|dialog| !dialog.starting)
                .and_then(|dialog| dialog.people().find(|contact| contact.id == id))
                .cloned();
            contact.map_or_else(Task::none, |contact| app.dispatch(Action::StartDm(contact)))
        }
        NewDm::Close => app.dispatch(Action::CloseNewDm),
        intent => ignored(&intent),
    }
}

fn places(app: &mut App, intent: places::Intent) -> Task<Message> {
    use places::Intent as P;
    match intent {
        P::Invite(intent) => self::invite(app, intent),
        P::Join(intent) => self::join(app, intent),
        P::Event(event::Intent::Close) => app.dispatch(Action::CloseEvent),
        P::Event(event::Intent::ToggleInterest) => app.dispatch(Action::ToggleEventInterest),
        P::Gift(gift::Intent::Close) => app.dispatch(Action::CloseGift),
        P::Gift(gift::Intent::Redeem) => app.dispatch(Action::RedeemGift),
        P::Gift(gift::Intent::OpenInBrowser) => app.dispatch(Action::OpenGiftInBrowser),
        P::OAuth(intent) => self::oauth(app, intent),
        P::Leave(intent) => self::leave(app, intent),
        P::LeavingLink(intent) => leaving(app, intent),
        intent => ignored(&intent),
    }
}

/// The invite: its choices are the window's until a new link is asked
/// for with them (`on_make_invite`).
fn invite(app: &mut App, intent: invite::Intent) -> Task<Message> {
    use invite::Intent as I;
    let invite = &mut app.vm.places.invite;
    match intent {
        I::Close => app.dispatch(Action::CloseInvite),
        I::Copy(link) => copy(link, &mut invite.copied),
        I::Age(age) => {
            invite.age = age.min(invite::AGES - 1);
            Task::none()
        }
        I::Uses(uses) => {
            invite.uses = uses.min(invite::USES - 1);
            Task::none()
        }
        I::Remake { age, uses } => match (INVITE_AGES.get(age), INVITE_USES.get(uses)) {
            (Some(&max_age), Some(&max_uses)) => {
                app.dispatch(Action::MakeInvite { max_age, max_uses })
            }
            _ => Task::none(),
        },
        intent => ignored(&intent),
    }
}

/// Puts `text` on the clipboard, and the button says it was copied.
fn copy(text: String, copied: &mut bool) -> Task<Message> {
    if text.is_empty() {
        return Task::none();
    }
    *copied = true;
    iced::clipboard::write(text).discard()
}

/// Join a Server: the field's text is the window's.
fn join(app: &mut App, intent: join::Intent) -> Task<Message> {
    use join::Intent as J;
    let action = match intent {
        J::Close => Action::CloseJoin,
        J::Link(text) => {
            app.vm.places.join.link = text;
            return Task::none();
        }
        J::Find(text) => Action::FindInvite(text),
        J::Join => Action::JoinServer,
        J::OpenCaptcha => Action::OpenJoinCaptcha,
        J::OpenForm => Action::OpenJoinForm,
        J::JoinInBrowser => Action::JoinInBrowser,
        intent => return ignored(&intent),
    };
    app.dispatch(action)
}

/// The authorization: a server is chosen by its place among those it can
/// be added to (`on_choose_oauth_guild`).
fn oauth(app: &mut App, intent: oauth::Intent) -> Task<Message> {
    use oauth::Intent as O;
    match intent {
        O::Close => app.dispatch(Action::CloseOAuth),
        O::Authorize => app.dispatch(Action::AuthorizeOAuth),
        O::ChooseServer(index) => {
            let guild = (app.core.state.oauth.as_ref())
                .and_then(|dialog| dialog.preview.as_ref())
                .and_then(|preview| preview.guilds.iter().filter(|g| g.can_add).nth(index))
                .map(|guild| guild.id.clone());
            guild.map_or_else(Task::none, |guild| {
                app.dispatch(Action::ChooseOAuthGuild(guild))
            })
        }
        O::Copy(address) => copy(address, &mut app.vm.places.oauth.copied),
        intent => ignored(&intent),
    }
}

/// Leaving a server or group DM, or the move-sync question, whichever the
/// core asks.
fn leave(app: &mut App, intent: leave::Intent) -> Task<Message> {
    use leave::Intent as L;
    let state = &app.core.state;
    let (leaving, syncing) = (state.leave.is_some(), state.move_sync.is_some());
    let action = match intent {
        L::Confirm if leaving => Action::Leave,
        L::Confirm if syncing => Action::AnswerMoveSync(Some(true)),
        L::Alternative if syncing && !leaving => Action::AnswerMoveSync(Some(false)),
        L::Tick if leaving => Action::ToggleSilentLeave,
        L::Close => return close_leave(app),
        _ => return Task::none(),
    };
    app.dispatch(action)
}

/// Cancel on either: nothing is left, and the dragged channel stays where
/// it was.
fn close_leave(app: &mut App) -> Task<Message> {
    let state = &app.core.state;
    if state.leave.is_some() {
        app.dispatch(Action::CloseLeave)
    } else if state.move_sync.is_some() {
        app.dispatch(Action::AnswerMoveSync(None))
    } else {
        app.vm.shell.close(Layer::Leave);
        Task::none()
    }
}

/// The masked link's question: "Visit site" opens it; either way the
/// question closes.
fn leaving(app: &mut App, intent: leaving_link::Intent) -> Task<Message> {
    app.vm.shell.close(Layer::LeavingLink);
    match intent {
        leaving_link::Intent::Visit => {
            let link = app.vm.places.leaving_link.link.clone();
            crate::platform::links::open_page(app, &link)
        }
        _ => Task::none(),
    }
}
