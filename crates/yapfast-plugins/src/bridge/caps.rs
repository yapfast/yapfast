use super::*;

/// Whether a method is one the application answers rather than this thread.
pub(super) fn is_app_call(method: &str) -> bool {
    matches!(
        method,
        call::method::MESSAGES_SEND
            | call::method::MESSAGES_EDIT
            | call::method::MESSAGES_DELETE
            | call::method::MESSAGES_REACT
            | call::method::MESSAGES_JUMP_TO
            | call::method::COMPOSER_GET_DRAFT
            | call::method::COMPOSER_SET_DRAFT
            | call::method::COMPOSER_INSERT
            | call::method::NOTIFY_SEND
            | call::method::CLIPBOARD_WRITE
            | call::method::OPEN_URL
            | call::method::DISCORD_SET_STATUS
            | call::method::DISCORD_SET_CUSTOM_STATUS
            | call::method::DISCORD_SET_ACTIVITY
            | call::method::DISCORD_CLEAR_ACTIVITY
            | call::method::DISCORD_MARK_READ
            | call::method::DISCORD_SET_MUTE
            | call::method::DISCORD_OPEN_ROOM
            | call::method::DISCORD_START_REPLY
            | call::method::DISCORD_START_EDIT
            | call::method::DISCORD_REACTORS
            | call::method::DISCORD_PAUSE_INVITES
            | call::method::DISCORD_ADD_EMOJI
            | call::method::DISCORD_ADD_STICKER
            | call::method::AUDIO_PLAY
            | call::method::AUDIO_SPEAK
    )
}

/// The capability a call the application carries out needs, when it needs one.
pub(super) fn app_capability(call: &AppCall) -> Option<Capability> {
    match call {
        AppCall::Send { .. }
        | AppCall::Edit { .. }
        | AppCall::Delete { .. }
        | AppCall::React { .. } => Some(Capability::MessagesSend),
        AppCall::Notify { .. } => Some(Capability::Notifications),
        AppCall::Copy { .. } => Some(Capability::ClipboardWrite),
        AppCall::Open { .. } => Some(Capability::OpenApp),
        AppCall::SetStatus { .. }
        | AppCall::SetCustomStatus { .. }
        | AppCall::SetActivity { .. } => Some(Capability::PresenceWrite),
        AppCall::MarkRead { .. } | AppCall::SetMute { .. } => Some(Capability::NotificationsManage),
        // Who reacted to a message is what people said, read the way the
        // history and the message calls read it.
        AppCall::Reactors { .. } => Some(Capability::MessagesRead),
        AppCall::PauseInvites { .. } | AppCall::AddEmoji { .. } | AppCall::AddSticker { .. } => {
            Some(Capability::ServersManage)
        }
        AppCall::PlaySound { .. } => Some(Capability::AudioPlay),
        AppCall::Speak { .. } => Some(Capability::AudioSpeak),
        // Navigation and the composer need no capability; the plan gives them
        // a recent gesture instead, and the composer is the person's own text.
        AppCall::JumpTo { .. }
        | AppCall::GetDraft { .. }
        | AppCall::SetDraft { .. }
        | AppCall::Insert { .. }
        | AppCall::OpenRoom { .. }
        | AppCall::StartReply { .. }
        | AppCall::StartEdit { .. } => None,
    }
}

/// Whether a call stands behind a person's own gesture.
///
/// §9.1: opening a room, starting a reply, starting an edit and writing into
/// the composer need no capability, and they do need a click of the person's
/// behind them: a plugin must not move the window or put text in front of
/// someone who is not there. So does handing a URL to the system, because that
/// is the one call that can start another program.
pub(super) fn needs_gesture(call: &AppCall) -> bool {
    matches!(
        call,
        AppCall::OpenRoom { .. }
            | AppCall::StartReply { .. }
            | AppCall::StartEdit { .. }
            | AppCall::Insert { .. }
            | AppCall::Open { .. }
    )
}

/// Whether an `open.app` call stays inside the schemes its manifest allows.
///
/// §9.1 gives `open.app` the web, and a manifest lists under
/// `permissions.schemes` any other scheme it may hand to the system. Without
/// this, a plugin that declares only `open.app` could launch any registered
/// handler — a file manager, an SSH client, an IDE's deep link — and a
/// background event is enough to do it, since a gesture is now required too.
pub(super) fn opened_scheme_allowed(
    permissions: &yapfast_plugin_api::capability::Permissions,
    call: &AppCall,
) -> Result<(), String> {
    let AppCall::Open { url } = call else {
        return Ok(());
    };
    let scheme = url
        .split([':', '/'])
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if matches!(scheme.as_str(), "http" | "https") {
        return Ok(());
    }
    if permissions
        .schemes
        .iter()
        .any(|allowed| allowed.eq_ignore_ascii_case(&scheme))
    {
        return Ok(());
    }
    Err(format!(
        "a `{scheme}:` address, which this plugin's manifest doesn't list: `open.app` opens \
         the web, and a manifest lists any other scheme under `permissions.schemes`"
    ))
}
