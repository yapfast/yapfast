//! The person's settings, and a server's and a channel's: one module each,
//! the view-model and `Intent` of one task (docs/iced/tasks.md). F01 keeps
//! this file: its fields and variants are wired for every module below, so a
//! task fills only its own module.

pub mod about;
pub mod account;
pub mod advanced;
pub mod appearance;
pub mod audit;
pub mod bans;
pub mod boosts;
pub mod channel;
pub mod create_channel;
pub mod discord;
pub mod emoji;
pub mod engagement;
pub mod frame;
pub mod games;
pub mod invites;
pub mod keybinds;
pub mod members;
pub mod messages;
pub mod mfa;
pub mod notifications;
pub mod ownership;
pub mod permissions;
pub mod role_editor;
pub mod roles;
pub mod safety;
pub mod server;
pub mod sounds;
pub mod stickers;
pub mod theme_editor;
pub mod voice;

/// What the person's settings, and a server's and a channel's show, by area.
#[derive(Clone, Debug, Default)]
pub struct Settings {
    pub frame: frame::Frame,
    pub account: account::Account,
    pub voice: voice::Voice,
    pub messages: messages::Messages,
    pub notifications: notifications::Notifications,
    pub sounds: sounds::Sounds,
    pub appearance: appearance::Appearance,
    pub theme_editor: theme_editor::ThemeEditor,
    pub discord: discord::Discord,
    pub games: games::Games,
    pub keybinds: keybinds::Keybinds,
    pub advanced: advanced::Advanced,
    pub about: about::About,
    pub mfa: mfa::Mfa,
    pub server: server::Server,
    pub engagement: engagement::Engagement,
    pub boosts: boosts::Boosts,
    pub members: members::Members,
    pub invites: invites::Invites,
    pub safety: safety::Safety,
    pub bans: bans::Bans,
    pub audit: audit::Audit,
    pub roles: roles::Roles,
    pub role_editor: role_editor::RoleEditor,
    pub emoji: emoji::Emoji,
    pub stickers: stickers::Stickers,
    pub ownership: ownership::Ownership,
    pub channel: channel::Channel,
    pub permissions: permissions::Permissions,
    pub create_channel: create_channel::CreateChannel,
}

/// What the person's settings, and a server's and a channel's ask for, by
/// area.
#[derive(Clone, Debug, PartialEq)]
pub enum Intent {
    Frame(frame::Intent),
    Account(account::Intent),
    Voice(voice::Intent),
    Messages(messages::Intent),
    Notifications(notifications::Intent),
    Sounds(sounds::Intent),
    Appearance(appearance::Intent),
    ThemeEditor(theme_editor::Intent),
    Discord(discord::Intent),
    Games(games::Intent),
    Keybinds(keybinds::Intent),
    Advanced(advanced::Intent),
    About(about::Intent),
    Mfa(mfa::Intent),
    Server(server::Intent),
    Engagement(engagement::Intent),
    Boosts(boosts::Intent),
    Members(members::Intent),
    Invites(invites::Intent),
    Safety(safety::Intent),
    Bans(bans::Intent),
    Audit(audit::Intent),
    Roles(roles::Intent),
    RoleEditor(role_editor::Intent),
    Emoji(emoji::Intent),
    Stickers(stickers::Intent),
    Ownership(ownership::Intent),
    Channel(channel::Intent),
    Permissions(permissions::Intent),
    CreateChannel(create_channel::Intent),
}

crate::parts!(Intent in Settings:
    Frame(frame::Intent),
    Account(account::Intent),
    Voice(voice::Intent),
    Messages(messages::Intent),
    Notifications(notifications::Intent),
    Sounds(sounds::Intent),
    Appearance(appearance::Intent),
    ThemeEditor(theme_editor::Intent),
    Discord(discord::Intent),
    Games(games::Intent),
    Keybinds(keybinds::Intent),
    Advanced(advanced::Intent),
    About(about::Intent),
    Mfa(mfa::Intent),
    Server(server::Intent),
    Engagement(engagement::Intent),
    Boosts(boosts::Intent),
    Members(members::Intent),
    Invites(invites::Intent),
    Safety(safety::Intent),
    Bans(bans::Intent),
    Audit(audit::Intent),
    Roles(roles::Intent),
    RoleEditor(role_editor::Intent),
    Emoji(emoji::Intent),
    Stickers(stickers::Intent),
    Ownership(ownership::Intent),
    Channel(channel::Intent),
    Permissions(permissions::Intent),
    CreateChannel(create_channel::Intent),
);
