//! The application's side of the plugin system: what the interface asks the
//! plugins to do, what they report back, and where the events the application
//! already sees reach the plugins that subscribed to them.
//!
//! Nothing here runs while no plugin is enabled. [`super::AppState::plugins`]
//! is `None` then, which is the one check every tap below makes first, and the
//! helper process doesn't exist at all. A tap that costs more than that —
//! building an event, serializing a message — happens only past it.

use std::path::Path;
use std::sync::Arc;

use serde_json::Value;
use tokio::sync::mpsc::UnboundedSender;
use yapfast_plugin_api::PluginId;
use yapfast_plugin_api::call::{CommandInvocation, HostReturn, ReactionCount};
use yapfast_plugin_api::event::{EventEnvelope, EventKind, Network as PluginNetwork, RoomView};
use yapfast_plugin_api::hook::{AttachmentPending, Draft};

use yapfast_plugin_api::manifest::{CommandDecl, OptionKind};
use yapfast_plugin_api::message::PluginStatus;

use super::{Action as AppAction, Application, Compose, Effect as AppEffect};
use crate::chat::Command;
use crate::chat::Event;
use crate::network::Network;
use crate::plugins::PluginState;
use crate::plugins::state::{Notice, PreSend};

/// Something a person or a plugin asks the plugin system to do.
pub enum Action {
    /// Turns a plugin on, which reads its manifest, grants what it asks for
    /// and starts its instance, or off, which drops it; no restart either way.
    SetEnabled { plugin: PluginId, enabled: bool },
    /// Answers a plugin's request to reach a site nobody has allowed: the
    /// dialog's two buttons and its "Don't ask again".
    AnswerConsent {
        plugin: PluginId,
        host: String,
        allow: bool,
        /// Written to `grants.json`, rather than kept for this run.
        remember: bool,
    },
    /// Runs a command a person typed in the composer, as the manifest
    /// declares it.
    ///
    /// The plugin system has no call that runs one yet, so this reaches the
    /// application and stops there: see `Application::plugins_run`.
    Command {
        plugin: PluginId,
        invocation: CommandInvocation,
    },
    /// The `message.pre_send` chain answered the send a hook held: this draft
    /// goes out in the conversation it names.
    SendHeld(Draft),
}

/// Something the plugin system did, for the interface to show.
pub enum Effect {
    /// A plugin's state changed: installed, enabled, active, faulted or
    /// suspended. The page that lists them redraws its row.
    Status {
        plugin: PluginId,
        status: PluginStatus,
    },
    /// The page's own state moved — a fetch finished, a review opened, an
    /// install wrote something — so it draws itself again. Nothing a plugin did.
    Page,
    /// A plugin asks to reach a site: the dialog to draw, or `None` once it has
    /// its answer. It is the application's own state, so the question outlives
    /// Settings, and it is asked wherever the person is.
    Consent(Option<ConsentAsk>),
    /// A plugin drew something in a slot or under a message, or what it drew
    /// was taken away: the interface shows the slots that moved, and no others.
    Slots,
    /// How the person on the profile card looks changed, so the card is drawn
    /// again from what the application now holds.
    Identity,
}

/// A plugin waiting for the person to say whether it may reach a site.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConsentAsk {
    /// The plugin that asked, for the answer to go back to.
    pub plugin: PluginId,
    /// The name the dialog shows, which is the manifest's and not the id.
    pub name: String,
    /// The host, as the request named it and the grant would be written.
    pub host: String,
}

/// What the plugin system tells the application, arriving as
/// [`crate::chat::Event::Plugin`] by the same path as every other event.
pub enum News {
    /// From the bridge: a hook's answer, or a plugin's state.
    Notice(Notice),
    /// From a tap: something the plugins subscribed to saw.
    Observed(EventEnvelope),
}

mod calls;
mod compose;
mod events;
pub(crate) mod page;

impl Application {
    /// Reads the plugin store and starts what must run at the start of a
    /// session, which is what a plugin that intercepts outgoing messages
    /// must: the first message can't go out before it is ready.
    ///
    /// The data directory is where the store and the enabled manifests are.
    /// `events` is the channel the rest of the application's events arrive
    /// on, which is how a hook's answer comes back as
    /// [`crate::chat::Event::Plugin`]: the application doesn't poll for one.
    ///
    /// The bridge runs the helper as a task, so this must be called with the
    /// background runtime entered. It is called once, at startup: before it,
    /// and in a build or a run without plugins, every tap is one check of
    /// [`super::AppState::plugins`].
    /// `version` is the program's own version, which a plugin reads as
    /// `app.version`; only the root package knows it.
    pub fn open_plugins(
        &mut self,
        data_dir: &Path,
        version: &str,
        events: UnboundedSender<Event>,
    ) -> Vec<AppEffect> {
        // `--no-plugins` reads nothing and starts nothing, as a build without
        // the feature does.
        if !crate::cli::plugins_allowed() {
            return vec![];
        }
        // The bridge starts a runtime of its own for the helper's task, and a
        // runtime can't be started from inside one: this is called from the
        // window's thread, which is outside every runtime.
        if tokio::runtime::Handle::try_current().is_ok() {
            tracing::warn!("the plugin system starts outside the background runtime");
            return vec![];
        }
        let wake: Arc<dyn Fn(Notice) + Send + Sync> = Arc::new(move |notice| {
            let _ = events.send(Event::Plugin(News::Notice(notice)));
        });
        let mut plugins = match PluginState::open(data_dir, version, wake) {
            Ok(plugins) => plugins,
            Err(error) => {
                tracing::warn!(
                    error = format!("{error:#}"),
                    "could not read the plugin store; starting without plugins"
                );
                return vec![];
            }
        };
        plugins.start();
        let statuses = plugins.statuses();
        let watching = plugins.enabled();
        self.state.plugins = Some(Box::new(plugins));
        self.watch_gateway(watching);
        statuses
            .into_iter()
            .map(|(plugin, status)| AppEffect::Plugin(Effect::Status { plugin, status }))
            .collect()
    }

    /// Whether the plugin system is on, which every tap asks before it builds
    /// anything: one check of an `Option` and one of a field.
    fn plugins_on(&self) -> bool {
        self.state
            .plugins
            .as_ref()
            .is_some_and(|plugins| plugins.enabled())
    }

    /// Tells the Discord gateway's tap whether a plugin may be watching, and
    /// which kinds one subscribed to.
    ///
    /// The gateway reports every message and reaction the account may read,
    /// whether or not a conversation shows it, so that tap is the only place
    /// a plugin hears of a conversation that isn't on screen. It is off
    /// unless the plugin system says otherwise, which keeps that path at one
    /// relaxed load, and the mask it sends along keeps an event nobody
    /// subscribed to at one more.
    fn watch_gateway(&self, watching: bool) {
        #[cfg(feature = "discord")]
        crate::discord_text::watch_messages(watching, self.plugin_kinds());
        #[cfg(not(feature = "discord"))]
        let _ = watching;
    }

    /// The kinds of event the plugins that run subscribed to, as the bridge's
    /// own one-bit-per-kind mask.
    ///
    /// It is read from the same number the bridge reads before it writes an
    /// event, so the gateway builds a payload for exactly the kinds that would
    /// be handed on. The enabled set is the only thing that changes it, and the
    /// two places that change that call [`Self::watch_gateway`].
    #[cfg(feature = "discord")]
    fn plugin_kinds(&self) -> u64 {
        let Some(plugins) = self.state.plugins.as_deref() else {
            return 0;
        };
        EventKind::ALL
            .into_iter()
            .filter(|kind| plugins.subscribes(*kind))
            .map(crate::plugins::registry::Subscriptions::bit)
            .fold(0, |mask, bit| mask | bit)
    }

    /// Carries out what the interface asked of the plugin system.
    pub(super) fn plugins_action(&mut self, action: Action) -> Vec<AppEffect> {
        match action {
            Action::SetEnabled { plugin, enabled } => self.plugins_set_enabled(plugin, enabled),
            Action::AnswerConsent {
                plugin,
                host,
                allow,
                remember,
            } => self.plugins_answer_consent(&plugin, &host, allow, remember),
            Action::Command { plugin, invocation } => self.plugins_run(plugin, invocation),
            Action::SendHeld(draft) => self.plugins_send_held(draft),
        }
    }

    /// Turns a plugin on or off. The state it settles in comes back as a
    /// [`Notice::Status`], which the page shows, so nothing is guessed here.
    fn plugins_set_enabled(&mut self, plugin: PluginId, enabled: bool) -> Vec<AppEffect> {
        let (result, watching) = match self.state.plugins.as_mut() {
            Some(plugins) => (plugins.set_enabled(&plugin, enabled), plugins.enabled()),
            None => return vec![],
        };
        if let Err(error) = result {
            self.state.notice = Some(format!(
                "Couldn't {} {plugin}: {error}",
                if enabled { "enable" } else { "disable" }
            ));
            return vec![];
        }
        // Turning the last plugin off takes the gateway's tap with it.
        self.watch_gateway(watching);
        // Turning a plugin off closes its dialog: the request it was asking
        // about has already been refused, and the question is no longer one.
        if !enabled
            && self
                .state
                .plugin_consent
                .as_ref()
                .is_some_and(|ask| ask.plugin == plugin)
        {
            return self.close_consent();
        }
        vec![]
    }

    /// Answers a plugin's request to reach a host, which is what completes the
    /// `http.fetch` kept waiting for it.
    ///
    /// The answer is kept first and the requests let go second: a request let go
    /// before its answer is written asks the same question again.
    fn plugins_answer_consent(
        &mut self,
        plugin: &PluginId,
        host: &str,
        allow: bool,
        remember: bool,
    ) -> Vec<AppEffect> {
        let pattern = match yapfast_plugin_api::capability::HostPattern::parse(host) {
            Ok(pattern) => pattern,
            Err(error) => {
                self.state.notice = Some(format!("Couldn't answer about {host}: {error}"));
                return self.close_consent();
            }
        };
        let lasts = if remember {
            crate::plugins::grants::Lasts::Stored
        } else {
            crate::plugins::grants::Lasts::Session
        };
        if let Some(plugins) = self.state.plugins.as_ref()
            && let Err(error) = plugins.answer_consent(plugin, &pattern, allow, lasts)
        {
            self.state.notice = Some(format!("Couldn't answer about {host}: {error:#}"));
        }
        self.close_consent()
    }

    /// Takes the ask off the screen, because it has its answer or because
    /// nothing is left to ask about it.
    pub(super) fn close_consent(&mut self) -> Vec<AppEffect> {
        if self.state.plugin_consent.take().is_some() {
            return vec![AppEffect::Plugin(Effect::Consent(None))];
        }
        vec![]
    }

    /// Runs a command the plugin system declares.
    ///
    /// The answer arrives as [`Notice::Command`] on the application's own event
    /// path, which is why nothing is returned here: the composer is already
    /// empty by the time the plugin replies.
    fn plugins_run(&mut self, plugin: PluginId, invocation: CommandInvocation) -> Vec<AppEffect> {
        if let Some(plugins) = self.state.plugins.as_mut() {
            plugins.run_command(&plugin, invocation);
        }
        vec![]
    }

    /// Tells the plugins whether the interface is dark, which `app.theme_mode`
    /// answers. The theme is the window's, so this is called from there.
    pub fn plugins_theme(&mut self, dark: bool) {
        if let Some(plugins) = self.state.plugins.as_ref() {
            plugins.world().publish_theme(dark);
        }
    }

    /// The network of the conversation on screen, for the events that are
    /// about Yapfast rather than about a room: a plugin watching one network
    /// still hears that the window lost focus.
    fn on_screen_network(&self) -> PluginNetwork {
        self.state.selected.as_deref().map_or_else(
            || {
                plugin_network(Network::of(
                    self.state.account.as_deref().unwrap_or_default(),
                ))
            },
            |room| plugin_network(Network::of(room)),
        )
    }
}

/// The commands the enabled plugins declare, which a `/` in the composer is
/// matched against.
///
/// The network an event came from, as the plugin API names it.
fn plugin_network(network: Network) -> PluginNetwork {
    match network {
        Network::Discord => PluginNetwork::Discord,
        Network::Matrix => PluginNetwork::Matrix,
    }
}

/// The server a room id belongs to, as the plugin API names one: nothing for a
/// DM or a room no server holds, which is one answer for that person
/// everywhere.
fn plugin_server(room: &str) -> Option<String> {
    crate::discord::parse_room(room)
        .map(|(server, _)| server.to_owned())
        .filter(|server| server != crate::discord::DM_GUILD)
}

/// Draws a profile as a plugin says its person looks.
///
/// A field the plugin left out stays as Yapfast had it, which is why every
/// surface can apply an answer over what it already knows. The name colour is a
/// style of one colour, the way the profile editor writes one.
fn decorate_profile(
    profile: &mut crate::chat::Profile,
    identity: &yapfast_plugin_api::identity::Identity,
) {
    if let Some(name) = &identity.display_name {
        profile.name = name.clone();
    }
    if let Some(colour) = identity.name_colour {
        profile.style = Some(Arc::new(crate::chat::NameStyle::new(0, 1, &[colour])));
    }
    if let Some(avatar) = &identity.avatar {
        profile.avatar = Some(avatar.clone());
    }
    if let Some(banner) = &identity.banner {
        profile.banner = Some(banner.clone());
    }
    if let Some(decoration) = &identity.decoration {
        profile.decoration = Some(Arc::from(decoration.as_str()));
    }
    if let Some(pronouns) = &identity.pronouns {
        profile.pronouns = pronouns.clone();
    }
}

/// The interface redraws the slots plugins draw in, or nothing does.
///
/// A tree that came again unchanged, or a slot emptied twice, draws nothing: the
/// interface is left alone until something it shows has changed.
fn redraw_if(changed: bool) -> Vec<AppEffect> {
    if changed {
        vec![AppEffect::Plugin(Effect::Slots)]
    } else {
        Vec::new()
    }
}

/// The interface redraws the profile card, because how its person looks has
/// changed under it.
fn redraw_identity() -> Vec<AppEffect> {
    vec![AppEffect::Plugin(Effect::Identity)]
}

/// One event, with its payload written out here: the bridge hands the same
/// bytes to every subscriber, so this is the only place it's written.
pub(crate) fn envelope(
    kind: EventKind,
    network: PluginNetwork,
    room: Option<String>,
    author: Option<String>,
    own: bool,
    payload: Value,
) -> EventEnvelope {
    EventEnvelope {
        kind,
        network,
        at: crate::clock::now_ms(),
        room,
        author,
        own,
        payload,
    }
}

/// A Discord message as a plugin sees it, and whether the account wrote it.
#[cfg(feature = "discord")]
pub(crate) fn message_view(
    discord: &yapfast_discord::service::Discord,
    room: &str,
    message: &yapfast_discord::message::Message,
) -> (yapfast_plugin_api::event::MessageView, bool) {
    use yapfast_plugin_api::event::{MessageView, PersonView};
    let snapshot = discord.snapshots().borrow().clone();
    let me = snapshot.user_id.clone().unwrap_or_default();
    let guild = crate::discord::parse_room(room)
        .map_or(yapfast_discord::ready::PRIVATE, |(guild, _)| guild);
    let server = snapshot.servers.iter().find(|server| server.id == guild);
    let own = message.author_id == me;
    let author = PersonView {
        id: message.author_id.clone(),
        name: discord
            .name(guild, &message.author_id)
            .unwrap_or_else(|| message.author.clone()),
        display_name: None,
        own,
        bot: message.bot,
    };
    let view = MessageView {
        id: message.id.clone(),
        room: room.to_owned(),
        author,
        // As Discord wrote it, its mentions still as `<@id>`: a plugin that
        // wants names calls `people.get`, rather than guessing at Markdown.
        body: message.content.clone(),
        at: message.timestamp() * 1000,
        edited: message.edited,
        deleted: false,
        // Files are `media.get`'s, and a plugin gets no token with which to
        // fetch one: an event names a message, not its files' addresses.
        attachments: Vec::new(),
        reply_to: match &message.reply {
            Some(yapfast_discord::message::Reply::Message { id, .. }) => Some(id.clone()),
            Some(yapfast_discord::message::Reply::Unknown { id }) if !id.is_empty() => {
                Some(id.clone())
            }
            _ => None,
        },
        mentions_me: message.mentions_me(&me, server.map(|server| &*server.marks)),
    };
    (view, own)
}

/// The command `/name option…` names among those `declared`, with its options
/// as the manifest types them.
///
/// Options are `key=value` pairs separated by spaces, and a value may be
/// quoted to keep its spaces: `/note text="two words"`. An option the
/// declaration doesn't list, one whose value isn't its type, or a required one
/// left out leaves the whole line as text, because a `/` the composer doesn't
/// understand has always gone to the conversation as it was written.
fn parse_command(
    text: &str,
    declared: &[(PluginId, CommandDecl)],
) -> Option<(PluginId, CommandInvocation)> {
    let rest = text.trim_start().strip_prefix('/')?;
    let (name, rest) = match rest.find(char::is_whitespace) {
        Some(cut) => (&rest[..cut], &rest[cut..]),
        None => (rest, ""),
    };
    let (plugin, command) = declared.iter().find(|(_, command)| command.name == name)?;
    let mut options = serde_json::Map::new();
    for (key, value) in options_of(rest)? {
        let declared = command.options.iter().find(|option| option.name == key)?;
        if options.contains_key(key) {
            return None;
        }
        options.insert(key.to_owned(), typed(value, declared.kind)?);
    }
    if command
        .options
        .iter()
        .any(|option| option.required && !options.contains_key(&option.name))
    {
        return None;
    }
    Some((
        plugin.clone(),
        CommandInvocation {
            name: command.name.clone(),
            options,
        },
    ))
}

/// The `key=value` pairs written after a command's name, with the quotes taken
/// off a value that has them.
fn options_of(mut rest: &str) -> Option<Vec<(&str, &str)>> {
    let mut pairs = Vec::new();
    rest = rest.trim();
    while !rest.is_empty() {
        let (key, after) = rest.split_once('=')?;
        let key = key.trim();
        if key.is_empty() || key.contains(char::is_whitespace) {
            return None;
        }
        let after = after.trim_start();
        if let Some(quoted) = after.strip_prefix('"') {
            let (value, after) = quoted.split_once('"')?;
            pairs.push((key, value));
            rest = after.trim_start();
        } else {
            rest = match after.find(char::is_whitespace) {
                Some(cut) => {
                    pairs.push((key, &after[..cut]));
                    &after[cut..]
                }
                None => {
                    pairs.push((key, after));
                    ""
                }
            }
            .trim_start();
        }
    }
    Some(pairs)
}

/// One option's value, as the type its declaration gives it.
fn typed(value: &str, kind: OptionKind) -> Option<Value> {
    match kind {
        OptionKind::String | OptionKind::Choice | OptionKind::Person | OptionKind::Channel => {
            Some(Value::String(value.to_owned()))
        }
        OptionKind::Bool => match value.to_ascii_lowercase().as_str() {
            "true" | "yes" | "on" => Some(Value::Bool(true)),
            "false" | "no" | "off" => Some(Value::Bool(false)),
            _ => None,
        },
        OptionKind::Integer => value.parse::<i64>().ok().map(Value::from),
        OptionKind::Number => value
            .parse::<f64>()
            .ok()
            .and_then(serde_json::Number::from_f64)
            .map(Value::Number),
    }
}

/// The type a file names, from its extension, for the
/// `attachment.pre_upload` chain: the same name the network would guess, said
/// once here so a plugin can decide without reading the file.
fn content_type(name: &str) -> Option<String> {
    let extension = name
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())?;
    let kind = match extension.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "txt" | "log" => "text/plain",
        "md" => "text/markdown",
        "json" => "application/json",
        "pdf" => "application/pdf",
        "mp3" => "audio/mpeg",
        "ogg" => "audio/ogg",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "zip" => "application/zip",
        _ => return None,
    };
    Some(kind.to_owned())
}

/// A plugin's picture where the reader that prepares an emoji or a sticker
/// takes one: a file, as a person's own picker hands it over, removed as soon
/// as it's read.
#[cfg(feature = "discord")]
fn with_brought_picture<T>(
    image: &[u8],
    read: impl FnOnce(&Path) -> Result<T, String>,
) -> Result<T, String> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "yapfast-plugin-{}-{}.{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed),
        // The reader makes the picture's name part of what it says when it
        // refuses one, so the name says what the picture is.
        match image {
            [0x89, b'P', b'N', b'G', ..] => "png",
            [b'G', b'I', b'F', ..] => "gif",
            [0xff, 0xd8, ..] => "jpg",
            [
                b'R',
                b'I',
                b'F',
                b'F',
                _,
                _,
                _,
                _,
                b'W',
                b'E',
                b'B',
                b'P',
                ..,
            ] => "webp",
            _ => "img",
        }
    ));
    std::fs::write(&path, image)
        .map_err(|error| format!("could not read that picture: {error}"))?;
    let read = read(&path);
    let _ = std::fs::remove_file(&path);
    read
}

/// A call the application carried out, from what it came to.
fn answered(outcome: Result<Vec<AppEffect>, String>) -> (Vec<AppEffect>, HostReturn) {
    match outcome {
        Ok(effects) => (effects, HostReturn::Ok(serde_json::Value::Bool(true))),
        Err(message) => (Vec::new(), HostReturn::Error { message }),
    }
}

/// The answer to a plugin that named something this account can't see.
fn not_listed(place: &str) -> HostReturn {
    HostReturn::Error {
        message: format!("{place} isn't a conversation or server this account can see"),
    }
}

/// The name a renamed upload goes under: the last part of what the chain
/// answered, so a plugin can't send a file called `../../x`, and the file's own
/// name when what it answered has no usable part.
fn upload_name(answered: &str, fallback: &str) -> String {
    let last = answered
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or_default()
        .trim();
    if last.is_empty() || last == "." || last == ".." {
        return fallback.to_owned();
    }
    // A name is bytes on every filesystem Yapfast writes to, and one answer is
    // one frame: a plugin can't hand over a name no file can have.
    let mut end = last.len().min(255);
    while end > 0 && !last.is_char_boundary(end) {
        end -= 1;
    }
    last[..end].to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::{Action as AppAction, preview::preview_room};
    use crate::chat::{Command, RoomIndex};
    use yapfast_plugin_api::manifest::CommandOption;

    /// An application whose one conversation is open and may take a message.
    fn connected() -> Application {
        let mut app = Application::default();
        app.apply(Event::Connected {
            user: "@alice:test".into(),
            trusted: true,
            storage: "test",
        });
        app.apply(Event::Sync(true));
        app.apply(Event::Rooms(RoomIndex {
            rooms: vec![preview_room("dm", "DM", true, 2)],
            servers: vec![],
        }));
        app.apply(Event::Timeline {
            room: "dm".into(),
            messages: vec![],
        });
        app
    }

    /// A call the application carries out, answered with what it did.
    #[test]
    fn a_plugin_sends_a_message_and_keeps_the_persons_draft() {
        let mut app = connected();
        app.state.drafts.insert("dm".into(), "mine".into());
        let plugin = yapfast_plugin_api::PluginId::parse("dev.local.sender").expect("an id");
        let (effects, answer) = app.plugins_call(
            &plugin,
            crate::plugins::state::AppCall::Send {
                room: "dm".into(),
                body: "from the plugin".into(),
            },
        );
        assert_eq!(sent(&effects).as_deref(), Some("from the plugin"));
        assert!(
            matches!(answer, yapfast_plugin_api::call::HostReturn::Ok(_)),
            "the plugin is told it went"
        );
        assert_eq!(
            app.state.drafts.get("dm").map(String::as_str),
            Some("mine"),
            "what the person was writing is still there"
        );
    }

    /// A call the application carries out, answered with what it did.
    #[test]
    fn a_plugin_reads_and_writes_the_composer_and_opens_a_page() {
        let mut app = connected();
        app.state.drafts.insert("dm".into(), "hello".into());
        let plugin = yapfast_plugin_api::PluginId::parse("dev.local.sender").expect("an id");

        let (_, answer) = app.plugins_call(
            &plugin,
            crate::plugins::state::AppCall::GetDraft { room: "dm".into() },
        );
        assert_eq!(
            answer,
            yapfast_plugin_api::call::HostReturn::Ok(serde_json::Value::String("hello".into())),
            "the draft as the person left it"
        );

        app.plugins_call(
            &plugin,
            crate::plugins::state::AppCall::Insert {
                room: "dm".into(),
                text: " there".into(),
            },
        );
        assert_eq!(
            app.state.drafts.get("dm").map(String::as_str),
            Some("hello there")
        );

        app.plugins_call(
            &plugin,
            crate::plugins::state::AppCall::SetDraft {
                room: "dm".into(),
                body: "replaced".into(),
            },
        );
        assert_eq!(
            app.state.drafts.get("dm").map(String::as_str),
            Some("replaced")
        );

        let (effects, _) = app.plugins_call(
            &plugin,
            crate::plugins::state::AppCall::Copy {
                text: "copied".into(),
            },
        );
        assert!(
            matches!(effects.as_slice(), [AppEffect::CopyText(text)] if text == "copied"),
            "the clipboard is the application's to write"
        );
        let (effects, _) = app.plugins_call(
            &plugin,
            crate::plugins::state::AppCall::Open {
                url: "https://example.com/".into(),
            },
        );
        assert!(
            matches!(effects.as_slice(), [AppEffect::OpenPage(url)] if url.ends_with("example.com/")),
            "the page opens the way a message's link does"
        );
    }

    /// The text of the message an effect sends, if one does.
    fn sent(effects: &[AppEffect]) -> Option<String> {
        effects.iter().find_map(|effect| match effect {
            AppEffect::Backend(Command::Send { body, .. }) => Some(body.clone()),
            _ => None,
        })
    }

    /// A draft as a chain answers with it.
    fn draft(body: &str) -> Draft {
        Draft {
            room: "dm".into(),
            body: body.into(),
            reply_to: None,
            attachments: Vec::new(),
        }
    }

    #[test]
    fn a_send_with_no_interceptor_is_unchanged() {
        let mut app = connected();
        app.dispatch(AppAction::SetDraft("hello".into()));
        let effects = app.dispatch(AppAction::Send);
        assert_eq!(sent(&effects).as_deref(), Some("hello"));
        assert_eq!(app.state.sending.as_deref(), Some("dm"));
    }

    #[test]
    fn a_held_send_goes_when_the_chain_answers() {
        let mut app = connected();
        app.dispatch(AppAction::SetDraft("hello".into()));
        assert!(app.plugins_send("dm").is_none(), "nothing intercepts here");
        let held = app.plugins_answer("dm", PreSend::Held { id: 1 });
        assert_eq!(held.map(|effects| effects.len()), Some(0));
        assert_eq!(app.state.draft(), "", "the composer empties at once");
        assert!(
            app.dispatch(AppAction::Send).is_empty(),
            "one send at a time"
        );
        let effects = app.apply(Event::Plugin(News::Notice(Notice::SendReady {
            id: 1,
            draft: draft("hello there"),
        })));
        assert_eq!(sent(&effects).as_deref(), Some("hello there"));
        app.apply(Event::SendResult {
            room: "dm".into(),
            body: "hello there".into(),
            result: Ok(()),
        });
        assert_eq!(app.state.draft(), "");
        assert!(app.state.sending.is_none());
    }

    #[test]
    fn a_blocked_send_does_not_send_and_says_why() {
        let mut app = connected();
        app.dispatch(AppAction::SetDraft("hello".into()));
        app.plugins_answer("dm", PreSend::Held { id: 2 });
        let effects = app.apply(Event::Plugin(News::Notice(Notice::SendBlocked {
            id: 2,
            reason: "That word isn't allowed here.".into(),
        })));
        assert!(sent(&effects).is_none());
        assert_eq!(
            app.state.notice.as_deref(),
            Some("That word isn't allowed here.")
        );
        assert_eq!(app.state.draft(), "hello", "the text comes back");
        assert!(app.state.sending.is_none(), "the room may send again");
    }

    #[test]
    fn a_send_the_person_typed_over_keeps_their_text() {
        let mut app = connected();
        app.dispatch(AppAction::SetDraft("hello".into()));
        app.plugins_answer("dm", PreSend::Held { id: 3 });
        app.dispatch(AppAction::SetDraft("what I said next".into()));
        let effects = app.apply(Event::Plugin(News::Notice(Notice::SendReady {
            id: 3,
            draft: draft("hello"),
        })));
        assert_eq!(sent(&effects).as_deref(), Some("hello"));
        assert_eq!(app.state.draft(), "what I said next");
    }

    /// The text of the edit an effect saves, if one does.
    fn edited(effects: &[AppEffect]) -> Option<String> {
        effects.iter().find_map(|effect| match effect {
            AppEffect::Backend(Command::Edit { body, .. }) => Some(body.clone()),
            _ => None,
        })
    }

    #[test]
    fn an_edit_chain_that_answers_at_once_saves_its_text() {
        let mut app = with_message(a_message("m1"));
        app.dispatch(AppAction::Edit("m1".into()));
        assert_eq!(
            app.state.edit().map(|edit| edit.text.as_str()),
            Some("something")
        );
        let answer = app.plugins_edit_answer(PreSend::Send(draft("something else")));
        assert!(answer.is_none(), "the edit goes on as its answer says");
        assert_eq!(
            app.state.edit().map(|edit| edit.text.as_str()),
            Some("something else"),
            "the chain's text is what the edit holds"
        );
        let effects = app.dispatch(AppAction::SaveEdit);
        assert_eq!(edited(&effects).as_deref(), Some("something else"));
    }

    #[test]
    fn a_held_edit_goes_when_the_chain_answers() {
        let mut app = with_message(a_message("m1"));
        app.dispatch(AppAction::Edit("m1".into()));
        let held = app.plugins_edit_answer(PreSend::Held { id: 4 });
        assert_eq!(held.map(|effects| effects.len()), Some(0));
        assert_eq!(
            app.state.edit().map(|edit| edit.text.as_str()),
            Some("something"),
            "the edit stays as it was while the chain runs"
        );
        assert!(
            app.dispatch(AppAction::SaveEdit).is_empty(),
            "one save at a time"
        );
        let effects = app.apply(Event::Plugin(News::Notice(Notice::SendReady {
            id: 4,
            draft: draft("something else"),
        })));
        assert_eq!(edited(&effects).as_deref(), Some("something else"));
        assert!(app.state.sending.is_some(), "the network has it now");
        assert_eq!(app.state.draft(), "", "the room's own draft is untouched");
        app.apply(Event::EditResult {
            room: "dm".into(),
            item: "m1".into(),
            result: Ok(()),
        });
        assert!(app.state.edit().is_none(), "the saved edit closes");
        assert!(app.state.sending.is_none());
    }

    #[test]
    fn a_blocked_edit_stays_open_and_says_why() {
        let mut app = with_message(a_message("m1"));
        app.dispatch(AppAction::Edit("m1".into()));
        app.plugins_edit_answer(PreSend::Held { id: 5 });
        let effects = app.apply(Event::Plugin(News::Notice(Notice::SendBlocked {
            id: 5,
            reason: "That word isn't allowed here.".into(),
        })));
        assert!(effects.is_empty());
        assert_eq!(
            app.state.notice.as_deref(),
            Some("That word isn't allowed here.")
        );
        assert_eq!(
            app.state.edit().map(|edit| edit.text.as_str()),
            Some("something"),
            "the edit is still there to change"
        );
        assert!(app.state.sending.is_none(), "the edit may be saved again");
    }

    #[test]
    fn a_slash_command_parses_into_the_invocation_its_manifest_declares() {
        let commands = [(
            PluginId::parse("com.example.dice").unwrap(),
            CommandDecl {
                name: "roll".into(),
                description: Some("Rolls dice.".into()),
                options: vec![
                    CommandOption {
                        name: "sides".into(),
                        kind: OptionKind::Integer,
                        required: true,
                        description: None,
                        choices: Vec::new(),
                    },
                    CommandOption {
                        name: "note".into(),
                        kind: OptionKind::String,
                        required: false,
                        description: None,
                        choices: Vec::new(),
                    },
                ],
            },
        )];
        let (plugin, invocation) =
            parse_command("/roll sides=20 note=\"two words\"", &commands).unwrap();
        assert_eq!(plugin.as_str(), "com.example.dice");
        assert_eq!(invocation.name, "roll");
        assert_eq!(invocation.options["sides"], serde_json::json!(20));
        assert_eq!(invocation.options["note"], serde_json::json!("two words"));
        // Without options the required one is missing, so it isn't one.
        assert!(parse_command("/roll", &commands).is_none());
    }

    #[test]
    fn a_slash_that_names_no_command_stays_text() {
        let commands = [(
            PluginId::parse("com.example.dice").unwrap(),
            CommandDecl {
                name: "roll".into(),
                description: None,
                options: vec![CommandOption {
                    name: "sides".into(),
                    kind: OptionKind::Integer,
                    required: true,
                    description: None,
                    choices: Vec::new(),
                }],
            },
        )];
        // An unknown name, a value of the wrong type, and no slash at all.
        assert!(parse_command("/nothing", &commands).is_none());
        assert!(parse_command("/roll sides=many", &commands).is_none());
        assert!(parse_command("roll sides=20", &commands).is_none());
        // The composer leaves it alone: nothing claims it.
        let mut app = connected();
        app.dispatch(AppAction::SetDraft("/nothing".into()));
        assert!(app.plugins_typed_command("dm").is_none());
        assert_eq!(app.state.draft(), "/nothing");
    }

    #[test]
    fn a_quoted_option_keeps_its_spaces_and_bare_ones_end_at_a_space() {
        assert_eq!(
            options_of(" a=1 b=\"two words\" c=3 ").unwrap(),
            [("a", "1"), ("b", "two words"), ("c", "3")]
        );
        assert_eq!(options_of("").unwrap(), Vec::new());
        assert!(options_of("bare").is_none());
    }

    /// A person's answer reaches the runtime, closes the question, and is what
    /// the gate finds when the plugin asks again.
    #[test]
    fn answering_the_dialog_records_the_answer_and_closes_the_question() {
        let directory = tempfile::tempdir().expect("a data directory");
        let mut app = Application::default();
        let (events, _received) = tokio::sync::mpsc::unbounded_channel();
        app.open_plugins(directory.path(), "0.3.0", events);
        let plugin = PluginId::parse("com.example.weather").expect("a plugin id");
        let host = "api.example.com";
        app.state.plugin_consent = Some(ConsentAsk {
            plugin: plugin.clone(),
            name: "Weather".to_owned(),
            host: host.to_owned(),
        });

        let effects = app.dispatch(AppAction::Plugin(Action::AnswerConsent {
            plugin: plugin.clone(),
            host: host.to_owned(),
            allow: true,
            remember: true,
        }));

        assert!(
            app.state.plugin_consent.is_none(),
            "the question has its answer"
        );
        assert!(
            matches!(
                effects.as_slice(),
                [AppEffect::Plugin(Effect::Consent(None))]
            ),
            "the dialog is taken away"
        );
        let hosts = yapfast_plugin_api::capability::HostPattern::parse(host).expect("a host");
        assert_eq!(
            app.state
                .plugins
                .as_ref()
                .expect("the plugin system is open")
                .grants()
                .ask(&plugin, &hosts),
            crate::plugins::grants::Ask::Allowed,
            "the answer is what the gate finds on the plugin's next request"
        );
    }

    /// A conversation whose timeline holds `message`, with a Discord account
    /// linked behind it.
    fn with_message(message: crate::chat::ChatMessage) -> Application {
        let mut app = connected();
        app.state.discord_user = Some("1".into());
        app.apply(Event::Timeline {
            room: "dm".into(),
            messages: vec![Arc::new(message)],
        });
        app
    }

    /// A message of the open conversation, as a timeline sends one.
    fn a_message(item: &str) -> crate::chat::ChatMessage {
        crate::chat::ChatMessage {
            id: item.into(),
            item: item.into(),
            author: "Bob".into(),
            body: "something".into(),
            can_reply: true,
            can_edit: true,
            can_react: true,
            ..Default::default()
        }
    }

    fn plugin() -> yapfast_plugin_api::PluginId {
        yapfast_plugin_api::PluginId::parse("dev.local.actions").expect("an id")
    }

    /// §17.2's presence group: what a plugin sets is what the person's own
    /// switches set.
    #[test]
    fn a_plugins_presence_goes_where_the_persons_own_switches_go() {
        let mut app = connected();
        app.state.discord_user = Some("1".into());

        let (effects, answer) = app.plugins_call(
            &plugin(),
            crate::plugins::state::AppCall::SetStatus {
                status: "dnd".into(),
            },
        );
        assert!(matches!(
            effects.as_slice(),
            [AppEffect::Backend(Command::Discord(
                crate::chat::DiscordCommand::SetStatus(status)
            ))] if status == "dnd"
        ));
        assert!(matches!(answer, HostReturn::Ok(_)));

        let (effects, _) = app.plugins_call(
            &plugin(),
            crate::plugins::state::AppCall::SetCustomStatus {
                emoji: Some("🐛".into()),
                text: " debugging ".into(),
                clear_after: Some(600),
            },
        );
        let [
            AppEffect::Backend(Command::Discord(crate::chat::DiscordCommand::SetCustomStatus(
                Some(status),
            ))),
        ] = effects.as_slice()
        else {
            panic!("a custom status is one command");
        };
        assert_eq!(status.text, "debugging");
        assert_eq!(status.emoji.as_deref(), Some("🐛"));
        assert!(status.expires_at_ms.is_some(), "it clears when asked");

        // Nothing written and no emoji clears it, as the person's own field
        // does.
        let (effects, _) = app.plugins_call(
            &plugin(),
            crate::plugins::state::AppCall::SetCustomStatus {
                emoji: None,
                text: "   ".into(),
                clear_after: None,
            },
        );
        assert!(matches!(
            effects.as_slice(),
            [AppEffect::Backend(Command::Discord(
                crate::chat::DiscordCommand::SetCustomStatus(None)
            ))]
        ));

        // The game Discord sees this account play is the one activity Yapfast
        // has, so it is the one a plugin shows and clears.
        let (effects, _) = app.plugins_call(
            &plugin(),
            crate::plugins::state::AppCall::SetActivity { enabled: true },
        );
        assert!(app.state.discord_show_game);
        assert!(matches!(
            effects.as_slice(),
            [AppEffect::Backend(Command::Discord(
                crate::chat::DiscordCommand::ShowGame(true)
            ))]
        ));
        let (effects, _) = app.plugins_call(
            &plugin(),
            crate::plugins::state::AppCall::SetActivity { enabled: false },
        );
        assert!(!app.state.discord_show_game);
        assert!(matches!(
            effects.as_slice(),
            [AppEffect::Backend(Command::Discord(
                crate::chat::DiscordCommand::ShowGame(false)
            ))]
        ));

        // Without an account behind it, nothing is claimed.
        let mut unlinked = connected();
        let (_, answer) = unlinked.plugins_call(
            &plugin(),
            crate::plugins::state::AppCall::SetStatus {
                status: "idle".into(),
            },
        );
        assert!(matches!(answer, HostReturn::Error { .. }));
    }

    /// A conversation with something unread, and a Discord DM beside it.
    fn unread_and_a_discord_dm() -> Application {
        let mut app = Application::default();
        app.state.discord_user = Some("1".into());
        app.apply(Event::Connected {
            user: "@alice:test".into(),
            trusted: true,
            storage: "test",
        });
        app.apply(Event::Sync(true));
        app.apply(Event::Rooms(crate::chat::RoomIndex {
            rooms: vec![
                crate::chat::ChatRoom {
                    unread: 3,
                    ..preview_room("dm", "DM", true, 2)
                },
                crate::chat::ChatRoom {
                    id: "discord:@me:7".into(),
                    network: crate::network::Network::Discord,
                    person: Some("7".into()),
                    ..preview_room("discord:@me:7", "Sam", true, 2)
                },
            ],
            servers: vec![],
        }));
        app
    }

    /// §17.2's reading state group: the same commands the person's own menu
    /// sends, and a place nobody can see is refused.
    #[test]
    fn a_plugin_marks_a_conversation_read_and_mutes_it() {
        let mut app = unread_and_a_discord_dm();
        let (effects, answer) = app.plugins_call(
            &plugin(),
            crate::plugins::state::AppCall::MarkRead { room: "dm".into() },
        );
        assert!(matches!(
            effects.as_slice(),
            [AppEffect::Backend(Command::MarkRead { room, .. })] if room == "dm"
        ));
        assert!(matches!(answer, HostReturn::Ok(_)));

        let (effects, _) = app.plugins_call(
            &plugin(),
            crate::plugins::state::AppCall::SetMute {
                room: "discord:@me:7".into(),
                muting: yapfast_plugin_api::call::Mute::Minutes(60),
            },
        );
        assert!(matches!(
            effects.as_slice(),
            [AppEffect::Backend(Command::Discord(crate::chat::DiscordCommand::Mute {
                room,
                muting,
            }))] if room == "discord:@me:7"
                && *muting == crate::chat::Muting::Minutes(60)
        ));

        let (effects, answer) = app.plugins_call(
            &plugin(),
            crate::plugins::state::AppCall::MarkRead {
                room: "nowhere".into(),
            },
        );
        assert!(effects.is_empty());
        assert!(
            matches!(answer, HostReturn::Error { .. }),
            "a place this account can't see is refused"
        );
    }

    /// §17.2's navigation group opens a room where a click in the sidebar
    /// does, and refuses one nobody can see.
    #[test]
    fn a_plugin_opens_a_room() {
        let mut app = unread_and_a_discord_dm();
        let (_, answer) = app.plugins_call(
            &plugin(),
            crate::plugins::state::AppCall::OpenRoom {
                room: "discord:@me:7".into(),
            },
        );
        assert!(matches!(answer, HostReturn::Ok(_)));
        assert_eq!(app.state.selected.as_deref(), Some("discord:@me:7"));

        let (_, answer) = app.plugins_call(
            &plugin(),
            crate::plugins::state::AppCall::OpenRoom {
                room: "nowhere".into(),
            },
        );
        assert!(matches!(answer, HostReturn::Error { .. }));
    }

    /// §17.2's navigation group, which writes nothing itself: it leaves the
    /// composer in the state a click on Reply or Edit leaves it in.
    #[test]
    fn a_plugin_starts_a_reply_and_an_edit() {
        let mut app = with_message(a_message("m1"));
        let (_, answer) = app.plugins_call(
            &plugin(),
            crate::plugins::state::AppCall::StartReply {
                room: "dm".into(),
                message: "m1".into(),
            },
        );
        assert!(matches!(answer, HostReturn::Ok(_)));
        assert!(matches!(
            app.state.compose.get("dm"),
            Some(Compose::Reply { item, .. }) if item == "m1"
        ));

        app.plugins_call(
            &plugin(),
            crate::plugins::state::AppCall::StartEdit {
                room: "dm".into(),
                message: "m1".into(),
            },
        );
        assert!(matches!(
            app.state.edit(),
            Some(edit) if edit.item == "m1"
        ));

        // A message the open conversation doesn't hold is refused rather than
        // guessed at.
        let (_, answer) = app.plugins_call(
            &plugin(),
            crate::plugins::state::AppCall::StartReply {
                room: "dm".into(),
                message: "not-here".into(),
            },
        );
        assert!(matches!(answer, HostReturn::Error { .. }));
    }

    /// §17.2's messages group: the reactions Yapfast holds for a message.
    #[test]
    fn a_plugin_reads_the_reactions_the_open_timeline_holds() {
        let mut message = a_message("m1");
        message.reactions = vec![crate::chat::Reaction {
            key: "👍".into(),
            count: 2,
            mine: true,
        }];
        let mut app = with_message(message);
        let (_, answer) = app.plugins_call(
            &plugin(),
            crate::plugins::state::AppCall::Reactors {
                room: "dm".into(),
                message: "m1".into(),
            },
        );
        assert_eq!(
            answer,
            HostReturn::Ok(serde_json::json!([
                { "emoji": "👍", "count": 2, "me": true }
            ]))
        );

        let (_, answer) = app.plugins_call(
            &plugin(),
            crate::plugins::state::AppCall::Reactors {
                room: "dm".into(),
                message: "elsewhere".into(),
            },
        );
        assert!(matches!(answer, HostReturn::Error { .. }));
    }

    /// §17.2's servers group: Yapfast's own permissions decide, so a plugin
    /// gets no further than the person would.
    #[test]
    fn a_plugin_changes_a_server_only_where_the_person_may() {
        let mut app = connected();
        app.apply(Event::Rooms(crate::chat::RoomIndex {
            rooms: vec![preview_room("dm", "DM", true, 2)],
            servers: vec![crate::chat::ChatServer {
                id: "discord:10".into(),
                name: "Friends".into(),
                discord: true,
                ..Default::default()
            }],
        }));
        for request in [
            crate::plugins::state::AppCall::PauseInvites {
                server: "discord:10".into(),
                invites: true,
                dms: false,
                hours: 1,
            },
            crate::plugins::state::AppCall::AddEmoji {
                server: "discord:10".into(),
                name: "bug".into(),
                image: b"\x89PNG\r\n\x1a\n".to_vec(),
            },
            crate::plugins::state::AppCall::AddSticker {
                server: "discord:10".into(),
                name: "bug".into(),
                tags: "🐛".into(),
                description: String::new(),
                image: b"\x89PNG\r\n\x1a\n".to_vec(),
            },
        ] {
            let (effects, answer) = app.plugins_call(&plugin(), request);
            assert!(effects.is_empty(), "nothing is asked of Discord");
            assert!(
                matches!(answer, HostReturn::Error { .. }),
                "no permission there, so no change"
            );
        }
    }

    /// Audio: what isn't a sound, and words that aren't any, are refused here
    /// rather than at the speakers.
    #[test]
    fn a_sound_that_isnt_one_is_refused_and_so_is_silence_spoken() {
        let mut app = connected();
        let (effects, answer) = app.plugins_call(
            &plugin(),
            crate::plugins::state::AppCall::PlaySound {
                sound: b"not a sound".to_vec(),
                volume: None,
            },
        );
        assert!(effects.is_empty());
        assert!(matches!(answer, HostReturn::Error { .. }));

        let (_, answer) = app.plugins_call(
            &plugin(),
            crate::plugins::state::AppCall::Speak { text: "   ".into() },
        );
        assert!(matches!(answer, HostReturn::Error { .. }));
    }
}
