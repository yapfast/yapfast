use super::*;

/// What the application's calls hand the supervisor.
pub(super) enum Request {
    /// Load and activate these plugins.
    Activate { plugins: Vec<PluginId> },
    /// Drop one plugin's instance, keeping what it handed over.
    Deactivate { plugin: PluginId },
    /// One event for one subscribed plugin.
    Event {
        plugin: PluginId,
        event: Arc<SharedEvent>,
    },
    /// A send waiting on a hook chain.
    Hook {
        held: HeldSend,
        request: HookRequest,
    },
    /// A command a person typed.
    Command {
        plugin: PluginId,
        invocation: CommandInvocation,
    },
    /// What a plugin draws under a message on screen, asked on the row's
    /// behalf: the answer comes back as [`FromHelper::Decorated`].
    Decorate {
        plugin: PluginId,
        request: yapfast_plugin_api::message::DecorateRequest,
    },
    /// How a plugin says one person looks, asked once per person and server:
    /// the answer comes back as [`FromHelper::Identity`].
    Identity {
        plugin: PluginId,
        request: yapfast_plugin_api::identity::IdentityRequest,
    },
    /// What a plugin badges one person in the member list with: the answer
    /// comes back as [`FromHelper::MemberDecorated`].
    DecorateMember {
        plugin: PluginId,
        request: yapfast_plugin_api::message::DecorateMemberRequest,
    },
    /// What a plugin makes of one code block on screen: the answer comes back as
    /// [`FromHelper::CodeBlock`].
    CodeBlock {
        plugin: PluginId,
        request: yapfast_plugin_api::code::CodeBlockRequest,
    },
    /// Replace a plugin with what the store holds now, with the plugins that
    /// need it stopped first and started again after.
    Swap {
        plugin: PluginId,
        /// The dependents, outermost first: the order to stop them in.
        stop: Vec<PluginId>,
        /// The same plugins the other way round: the order to start them in.
        start: Vec<PluginId>,
    },
    /// Watch a plugin's folder and reload it when its files change.
    Watch { plugin: PluginId, folder: PathBuf },
    /// Drop these instances now, for a reason the helper can't know.
    Evict { plugins: Vec<PluginId> },
    /// A plugin's settings changed, or a widget of one was used: the answer to
    /// the `settings.on_change` call it asked with.
    SettingsNews {
        plugin: PluginId,
        news: SettingsNews,
    },
    /// What the application did with a call it answered, handed back to the
    /// guest under its own number.
    CallAnswer {
        plugin: PluginId,
        call: u64,
        answer: HostReturn,
    },
    /// The application is going away.
    Shutdown,
}

/// How a session ended.
pub(super) enum Ending {
    /// The parent asked for it.
    Asked,
    /// Nothing was left to run, so the helper went, as it is meant to.
    Idle,
    /// It went when it shouldn't have.
    Gone(String),
}
