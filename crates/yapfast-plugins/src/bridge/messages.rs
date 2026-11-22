//! What crosses the pipe's far end back to the application: the answer to a
//! send, a host call only the application can carry out, and how a send was held.
//!
//! The application owns the effects these name; this crate only says what
//! happened, so a plugin's answer reaches the composer without the application
//! polling for one.

use yapfast_plugin_api::PluginId;
use yapfast_plugin_api::capability::Capability;
use yapfast_plugin_api::hook::Draft;
use yapfast_plugin_api::message::PluginStatus;

/// Something the plugin system did, for the application to act on.
///
/// The bridge hands these to the closure it was opened with, which the
/// application turns into `Event::Plugin`: that is how a hook's answer reaches
/// the composer without the application polling anything.
#[derive(Clone, Debug, PartialEq)]
pub enum Notice {
    /// A `message.pre_send` chain finished: send this draft.
    SendReady { id: u64, draft: Draft },
    /// A `message.pre_send` chain stopped the send, and why.
    SendBlocked { id: u64, reason: String },
    /// A `typing.pre_send` chain let a typing indicator go: the application
    /// sends it now, which is the only way a plugin may withhold one without
    /// the composer waiting.
    Typing { id: u64, room: String },
    /// An `attachment.pre_upload` chain answered about a file the composer is
    /// taking: the name the upload should go under, or the reason it doesn't.
    Attachment {
        id: u64,
        name: Option<String>,
        blocked: Option<String>,
    },
    /// A plugin in the `attachment.pre_upload` chain kept the file to work on
    /// its bytes: the composer says so until [`Notice::UploadFinished`].
    UploadHeld {
        id: u64,
        plugin: PluginId,
        name: String,
    },
    /// The plugin that held a file said what goes: this file, under this
    /// name, or none, and text for the composer.
    UploadFinished {
        id: u64,
        file: Option<(std::path::PathBuf, u64)>,
        name: String,
        insert: Option<String>,
    },
    /// A plugin wants the person to sign in to a service with its own app:
    /// the leaving dialog shows the service's page, named as the plugin's,
    /// and the browser opens it on the person's say.
    SignIn {
        plugin: PluginId,
        provider: String,
        url: String,
    },
    /// What a plugin's overlay shows now, or none when it took it away.
    Overlay {
        plugin: PluginId,
        layer: Option<yapfast_plugin_api::call::OverlaySet>,
    },
    /// A plugin's recording ended, so the red bar goes.
    RecordingEnded { plugin: PluginId },
    /// A plugin's decorations should be asked again: what it keeps changed.
    Redecorate { plugin: PluginId },
    /// How a person looks, or everyone when `person` is none, should be asked
    /// again.
    IdentityRefresh {
        plugin: PluginId,
        person: Option<String>,
    },
    /// A plugin's state changed, for the page that lists them.
    Status {
        plugin: PluginId,
        status: PluginStatus,
    },
    /// A command answered with a line for the person, or failed with a reason.
    Command { plugin: PluginId, text: String },
    /// A fetch finished, for the page's Add from URL: the plugin it found, or
    /// the sentence saying why there is none.
    Fetched {
        url: String,
        result: Result<crate::installing::Fetched, String>,
    },
    /// The marketplace was read, or why it couldn't be: its listings and
    /// icons, which are held only while the page that asked is open.
    Catalog {
        source: String,
        result: Result<Box<crate::marketplace::Catalog>, String>,
    },
    /// A guest asked Yapfast to do something only the application can: send a
    /// message, write the composer, notify, copy or open a page. The answer goes
    /// back through [`PluginState::answer_call`], so nothing waits on the
    /// interface thread.
    Call {
        plugin: PluginId,
        call: u64,
        request: AppCall,
    },
    /// A plugin wants a host the person hasn't allowed, so a page can ask.
    ///
    /// The gate asks rather than holding the request open, and the plugin asks
    /// again once the answer is in; without this the ask had nowhere to go and
    /// the plugin waited on a dialog that was never shown.
    Consent { plugin: PluginId, host: String },
    /// A plugin used a powerful capability for the first time: the person is
    /// asked, in a red dialog of its own, whether it may.
    Powerful {
        plugin: PluginId,
        capability: Capability,
    },
    /// A plugin sent a message as an agent, without a keystroke: the window
    /// shows that it is doing so, with a way to stop it.
    AgentSent { plugin: PluginId },
    /// What a plugin drew in a slot, or under a message, for the interface to
    /// keep and show. The runtime coalesces these, so a plugin that draws once
    /// per event costs one update per frame at most.
    Slot(crate::slots::Output),
    /// How a plugin says one person looks: the application keeps it under the
    /// pair the plugin was asked about, and every surface that shows that
    /// person reads it from there. `rank` is where the plugin sits in the
    /// chain, so which plugin wins doesn't depend on which answer arrived
    /// first.
    Identity {
        plugin: PluginId,
        person: String,
        server: Option<String>,
        rank: usize,
        identity: Option<yapfast_plugin_api::identity::Identity>,
    },
    /// What a plugin made of one code block on screen: spans that spell the
    /// block again, kept by the block's content hash. `rank` is where the plugin
    /// sits in the chain, so the earlier plugin's colours win whatever order two
    /// answers arrive in.
    CodeBlock {
        plugin: PluginId,
        block: String,
        rank: usize,
        spans: Option<Vec<yapfast_plugin_api::code::Span>>,
    },
    /// A plugin offers to open the link the person clicked in an app: the
    /// leaving dialog shows `label` beside Visit site, named as the plugin's.
    /// `id` is the one [`crate::bridge::Bridge::link_open`] answered.
    LinkOffer {
        id: u64,
        plugin: PluginId,
        label: String,
        uri: String,
    },
    /// A plugin put the quick switcher's places for `query` in its own order,
    /// leaving out the ones it didn't name.
    SwitcherRanked {
        plugin: PluginId,
        query: String,
        order: Vec<String>,
    },
    /// A picture from the web a plugin's drawing shows, fetched through its
    /// gate and decoded: the pixels, or why there are none.
    WebPicture {
        plugin: PluginId,
        url: String,
        picture: Result<std::sync::Arc<crate::web_picture::WebPicture>, String>,
    },
    /// A plugin set or took away the words after the window's title.
    WindowTitle {
        plugin: PluginId,
        suffix: Option<String>,
    },
    /// Take away everything one plugin drew, because it was turned off or
    /// suspended: a plugin that isn't running shows nothing.
    SlotsGone(PluginId),
    /// Take away everything every plugin drew, because the store changed under
    /// them: an install, an uninstall or a roll back.
    SlotsReset,
}

/// A guest call the application itself has to carry out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AppCall {
    Send {
        room: String,
        body: String,
        /// Files from its folder, resolved inside it: `messages.attach`.
        files: Vec<std::path::PathBuf>,
        /// Files it made, or its package carries: `messages.attach`.
        made: Vec<MadeFile>,
        /// Sent without a keystroke, under `messages.send.agent`.
        agent: bool,
    },
    Edit {
        room: String,
        message: String,
        body: String,
    },
    Delete {
        room: String,
        message: String,
    },
    React {
        room: String,
        message: String,
        emoji: String,
    },
    JumpTo {
        room: String,
        message: String,
    },
    GetDraft {
        room: String,
    },
    SetDraft {
        room: String,
        body: String,
    },
    Insert {
        room: String,
        text: String,
    },
    Notify {
        text: String,
    },
    Copy {
        text: String,
    },
    Open {
        url: String,
    },
    /// The curated actions of PLAN §17.2: each wraps something Yapfast
    /// already does, so the token, the rate limiter and the person's own
    /// permissions stay here.
    SetStatus {
        status: String,
    },
    SetCustomStatus {
        emoji: Option<String>,
        text: String,
        clear_after: Option<u64>,
    },
    /// Whether the game Discord sees this account play shows on it; a
    /// plugin's own activity is [`AppCall::SetPluginActivity`].
    SetActivity {
        enabled: bool,
    },
    MarkRead {
        room: String,
    },
    SetMute {
        room: String,
        muting: yapfast_plugin_api::call::Mute,
    },
    OpenRoom {
        room: String,
    },
    StartReply {
        room: String,
        message: String,
    },
    StartEdit {
        room: String,
        message: String,
    },
    /// The reactions the open timeline holds for a message, with their
    /// counts. Who gave them is a request Yapfast makes, not state it keeps.
    Reactors {
        room: String,
        message: String,
    },
    PauseInvites {
        server: String,
        invites: bool,
        dms: bool,
        hours: u8,
    },
    AddEmoji {
        server: String,
        name: String,
        /// The picture, as the plugin brought it; the format is read from its
        /// own first bytes.
        image: Vec<u8>,
    },
    AddSticker {
        server: String,
        name: String,
        tags: String,
        description: String,
        image: Vec<u8>,
    },
    /// A sound file to play on the speakers, and how loud.
    PlaySound {
        sound: Vec<u8>,
        volume: Option<u8>,
    },
    /// Text for the OS's own speech.
    Speak {
        text: String,
        voice: Option<String>,
        /// Percent of the system's pace, 50 to 200.
        rate: Option<u16>,
        /// 0 to 100.
        volume: Option<u8>,
    },
    /// A file for the person to save where they choose, in the system's save
    /// dialog: `files.save`. `name` is the one suggested, a file name already
    /// checked to be no path.
    SaveFile {
        name: String,
        bytes: Vec<u8>,
    },
    /// The activity a plugin shows on the account's presence, or none to take
    /// it away: `activity.set` and `activity.clear`. Already checked against
    /// Discord's limits.
    SetPluginActivity {
        activity: Option<Box<yapfast_plugin_api::account::ActivitySet>>,
    },
    /// The account's bio, pronouns or custom status: `profile.set`, after the
    /// person allowed it the first time.
    SetProfile(yapfast_plugin_api::account::ProfileSet),
    /// A search through Discord's search: `messages.search`.
    Search(yapfast_plugin_api::account::SearchRequest),
    /// The drafts the person left unsent: `drafts.list`.
    Drafts,
    /// The devices signed in to the account: `account.sessions`.
    Sessions,
    /// Who Discord ranks closest to the account: `relationships.affinities`.
    Affinities(yapfast_plugin_api::account::AffinitiesRequest),
    /// The linked Spotify: read or control its playback.
    Spotify(yapfast_plugin_api::account::SpotifyAction),
    /// Record the microphone, after the person's yes, while a red bar says
    /// so: `microphone.start`, answered when the recording ends.
    Record {
        format: yapfast_plugin_api::call::RecordingFormat,
        max_seconds: Option<u32>,
    },
    /// A message, or a page of messages, of a conversation that isn't the
    /// one on screen: `messages.get` and `messages.history`, asked of Discord.
    Messages {
        room: String,
        /// One message, by id; a page when none.
        message: Option<String>,
        before: Option<String>,
        limit: Option<usize>,
    },
    /// Stop the plugin's recording: `microphone.stop`.
    RecordStop,
    /// Send a recording as a voice message: `messages.voice`, under the
    /// send's rails. The file is Ogg Opus, already checked to be the plugin's.
    SendVoice {
        room: String,
        path: std::path::PathBuf,
        duration_ms: u64,
        waveform: String,
    },
}

/// A file a plugin sends that it made, or that its package carries: a name,
/// checked to be no path, and the bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MadeFile {
    pub name: String,
    pub bytes: Vec<u8>,
    pub spoiler: bool,
}

/// What `pre_send` decided about an outgoing message.
pub enum PreSend {
    /// No plugin intercepts sends, or the chain already answered: send this
    /// draft unchanged, without waiting for anything.
    Send(Draft),
    /// Interceptors are running. The composer is cleared now and the send
    /// completes when the answer arrives as `Event::Plugin`; a spinner shows
    /// only past 100 ms.
    Held { id: u64 },
}
