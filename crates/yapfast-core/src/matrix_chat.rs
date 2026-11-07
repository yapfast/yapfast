//! Matrix in the chat actor: the signed-in session, its rooms and timelines.
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    pin::Pin,
    sync::Arc,
    time::Duration,
};

use futures_util::{Stream, StreamExt};
use matrix_sdk::{
    Client, ComposerDraft, ComposerDraftType, Room, RoomMemberships,
    attachment::{AttachmentInfo, BaseFileInfo, BaseImageInfo},
    event_handler::EventHandlerHandle,
    room::{
        MessagesOptions,
        edit::EditedContent,
        reply::{EnforceThread, Reply as ReplyTo},
    },
    ruma::{
        OwnedEventId, OwnedRoomId, OwnedTransactionId, OwnedUserId, UInt, UserId,
        api::client::receipt::create_receipt::v3::ReceiptType,
        events::{
            Mentions, SyncStateEvent,
            receipt::ReceiptThread,
            room::{
                MediaSource,
                member::RoomMemberEventContent,
                message::{
                    AddMentions, MessageType, ReplyWithinThread, RoomMessageEventContent,
                    RoomMessageEventContentWithoutRelation,
                },
                power_levels::UserPowerLevel,
            },
            space::child::SpaceChildEventContent,
        },
    },
    send_queue::SendQueueRoomError,
};
use matrix_sdk_ui::{
    sync_service::State,
    timeline::{
        AttachmentConfig, AttachmentSource, EventSendState, EventTimelineItem, MsgLikeKind,
        SendTarget, Timeline, TimelineBuilder, TimelineDetails, TimelineEventFocusThreadMode,
        TimelineEventItemId, TimelineFocus, TimelineItemContent,
    },
};
use tokio::{
    sync::{
        broadcast,
        mpsc::{UnboundedReceiver, UnboundedSender},
    },
    task::{AbortHandle, JoinSet},
    time::Instant,
};

use crate::{
    chat::{
        Attachment, ChatMember, ChatMessage, ChatRoom, ChatServer, Command, Contact, Delivery,
        Direction, Event, ForwardFailure, History, MAX_MESSAGE_BYTES, MAX_PREVIEW_DOWNLOAD_BYTES,
        Notification, Picture, Placement, Reaction, Reactor, Reply, Role, RoomIndex, Start,
        decode_preview, decode_view, notification_text, plain_snippet, sort_members,
    },
    matrix_media,
    network::Network,
    session::Session,
    voice,
};

/// Synapse's default upload limit; the send queue holds the whole file in memory.
const MAX_ATTACHMENT_BYTES: u64 = 50 * 1024 * 1024;
/// How long a recoverable send failure pauses a room's queue before Yapfast retries.
const QUEUE_RETRY: Duration = Duration::from_secs(10);
/// How many events a page of earlier history asks the homeserver for.
const HISTORY_PAGE: u16 = 30;

type Updates = Pin<Box<dyn Stream<Item = ()> + Send>>;

/// The open conversation and the background work that belongs to it.
struct OpenRoom {
    timeline: Arc<Timeline>,
    /// The timeline is a window of older messages around one jumped to,
    /// which live messages don't go in.
    focused: bool,
    /// The room's first message is in the timeline.
    start: bool,
    pictures: HashMap<String, Option<Arc<Picture>>>,
    requested: HashSet<String>,
    visible: HashSet<String>,
    // Dropping the set when the room closes cancels its downloads and requests.
    tasks: JoinSet<()>,
    /// The member list being read; a newer membership change replaces it.
    members: Option<AbortHandle>,
}

impl OpenRoom {
    fn id(&self) -> &str {
        self.timeline.room().room_id().as_str()
    }

    /// How the timeline reaches the room's ends, for [`Event::HistoryLoaded`].
    fn history(&self) -> History {
        History::Loaded {
            start: self.start,
            present: !self.focused,
        }
    }

    /// The oldest event in the timeline that the homeserver has.
    async fn first_event(&self) -> Option<OwnedEventId> {
        self.timeline
            .items()
            .await
            .iter()
            .find_map(|item| item.as_event()?.event_id().map(ToOwned::to_owned))
    }

    async fn has_event(&self, id: &OwnedEventId) -> bool {
        self.timeline
            .items()
            .await
            .iter()
            .any(|item| item.as_event().and_then(|event| event.event_id()) == Some(id))
    }

    /// Shows the messages around `event` in place of the timeline, or with
    /// none the room's newest messages, which follow live ones; `false` when
    /// the homeserver couldn't give them. The SDK keeps the newest messages
    /// in its event cache, so going back to them doesn't load them again.
    async fn refocus(&mut self, event: Option<OwnedEventId>, updates: &mut Updates) -> bool {
        let room = self.timeline.room().clone();
        let focused = event.is_some();
        let builder = match event {
            Some(target) => TimelineBuilder::new(&room).with_focus(TimelineFocus::Event {
                target,
                num_context_events: HISTORY_PAGE,
                thread_mode: TimelineEventFocusThreadMode::Automatic {
                    hide_threaded_events: false,
                },
            }),
            None => TimelineBuilder::new(&room),
        };
        let Ok(timeline) = builder.build().await else {
            return false;
        };
        let (_, stream) = timeline.subscribe().await;
        *updates = Box::pin(stream.map(|_| ()));
        self.timeline = Arc::new(timeline);
        self.focused = focused;
        self.start = false;
        true
    }
}

/// Restore the saved session or sign in.
pub(crate) async fn open(root: PathBuf, start: Start) -> anyhow::Result<Session> {
    match start {
        Start::Restore => Session::restore(root).await,
        Start::Login {
            homeserver,
            username,
            password,
        } => Session::login(root, &homeserver, &username, &password).await,
    }
}

/// Cancels a task when dropped.
struct AbortOnDrop(AbortHandle);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// What woke the Matrix side of the chat actor.
pub(crate) enum Wake {
    Rooms,
    Members(String),
    Timeline,
    Picture(String, String, Option<Arc<Picture>>),
    QueueError(Result<SendQueueRoomError, broadcast::error::RecvError>),
    RetryQueue,
    Sync(State),
}

/// The signed-in Matrix account: sync, the room list, the open room and the
/// send queue. The chat actor owns it beside Discord and voice.
pub(crate) struct Matrix {
    session: Session,
    client: Client,
    trusted: bool,
    queue_errors: broadcast::Receiver<SendQueueRoomError>,
    queue_retry: Option<Instant>,
    // The SDK's room list stream borrows the client, so a task forwards it.
    rooms_changes: UnboundedReceiver<()>,
    _rooms_task: AbortOnDrop,
    room_info: Updates,
    space_changes: UnboundedReceiver<()>,
    member_changes: UnboundedReceiver<String>,
    space_handler: EventHandlerHandle,
    member_handler: EventHandlerHandle,
    media_sender: UnboundedSender<(String, String, Option<Arc<Picture>>)>,
    media: UnboundedReceiver<(String, String, Option<Arc<Picture>>)>,
    sync: Option<crate::session::SessionSync>,
    sync_states: Pin<Box<dyn Stream<Item = State> + Send>>,
    open: Option<OpenRoom>,
    updates: Updates,
    online: bool,
    restart_sync: bool,
    stale_voice_checked: bool,
}

impl Matrix {
    pub(crate) async fn connect(session: Session, events: &UnboundedSender<Event>) -> Self {
        let client = session.client.clone();
        let trusted = session.device_is_trusted().await.unwrap_or(false);
        let _ = events.send(Event::Connected {
            user: client
                .user_id()
                .map(ToString::to_string)
                .unwrap_or_default(),
            trusted,
            storage: session.storage_notice(),
        });
        // Messages queued before Yapfast closed are sent again from the encrypted store.
        client.send_queue().set_enabled(true).await;
        client
            .send_queue()
            .respawn_tasks_for_rooms_with_unsent_requests()
            .await;
        let queue_errors = client.send_queue().subscribe_errors();
        let (rooms_changed, rooms_changes) = tokio::sync::mpsc::unbounded_channel();
        let rooms_client = client.clone();
        let rooms_task = tokio::spawn(async move {
            let (_, rooms) = rooms_client.rooms_stream();
            tokio::pin!(rooms);
            while rooms.next().await.is_some() && rooms_changed.send(()).is_ok() {}
        });
        // A missed update still changed something, so it refreshes too.
        let room_info = futures_util::stream::unfold(
            client.room_info_notable_update_receiver(),
            |mut receiver| async move {
                match receiver.recv().await {
                    Err(broadcast::error::RecvError::Closed) => None,
                    _ => Some(((), receiver)),
                }
            },
        );
        let (space_changed, space_changes) = tokio::sync::mpsc::unbounded_channel();
        let space_handler =
            client.add_event_handler(move |_: SyncStateEvent<SpaceChildEventContent>| {
                let changed = space_changed.clone();
                async move {
                    let _ = changed.send(());
                }
            });
        let (member_changed, member_changes) = tokio::sync::mpsc::unbounded_channel();
        let member_handler = client.add_event_handler(
            move |_: SyncStateEvent<RoomMemberEventContent>, room: Room| {
                let changed = member_changed.clone();
                async move {
                    let _ = changed.send(room.room_id().to_string());
                }
            },
        );
        // New messages the account's push rules say should notify. The first sync also
        // brings messages from before Yapfast started, which don't.
        let started = matrix_sdk::ruma::MilliSecondsSinceUnixEpoch::now();
        let notify = events.clone();
        client
            .register_notification_handler(move |notification, room: Room, client: Client| {
                let events = notify.clone();
                async move {
                    if let Some(notification) =
                        notification_of(notification, &room, &client, started).await
                    {
                        let _ = events.send(Event::Notify(notification));
                    }
                }
            })
            .await;
        let (media_sender, media) = tokio::sync::mpsc::unbounded_channel();
        let _ = events.send(Event::Rooms(room_index(&client).await));
        Self {
            session,
            client,
            trusted,
            queue_errors,
            queue_retry: None,
            rooms_changes,
            _rooms_task: AbortOnDrop(rooms_task.abort_handle()),
            room_info: Box::pin(room_info),
            space_changes,
            member_changes,
            space_handler,
            member_handler,
            media_sender,
            media,
            sync: None,
            sync_states: Box::pin(futures_util::stream::pending()),
            open: None,
            updates: Box::pin(futures_util::stream::pending()),
            online: false,
            restart_sync: true,
            stale_voice_checked: false,
        }
    }

    /// Starts sync, or restarts it after recovery or a retry.
    pub(crate) async fn restart_sync(&mut self, events: &UnboundedSender<Event>) {
        if !std::mem::take(&mut self.restart_sync) {
            return;
        }
        if let Some(previous) = self.sync.take() {
            previous.stop().await;
        }
        match self.session.start_sync().await {
            Ok(service) => {
                self.sync_states = Box::pin(service.states());
                self.sync = Some(service);
            }
            Err(_) => {
                let _ = events.send(Event::Sync(false));
            }
        }
    }

    /// Waits for the SDK. Cancel safe: the chat actor handles what it returns.
    pub(crate) async fn wait(&mut self) -> Wake {
        let retry_at = self.queue_retry;
        tokio::select! {
            Some(_) = self.rooms_changes.recv() => Wake::Rooms,
            Some(_) = self.room_info.next() => Wake::Rooms,
            Some(_) = self.space_changes.recv() => Wake::Rooms,
            Some(room) = self.member_changes.recv() => Wake::Members(room),
            Some(_) = self.updates.next() => Wake::Timeline,
            Some((room, key, picture)) = self.media.recv() => Wake::Picture(room, key, picture),
            error = self.queue_errors.recv() => Wake::QueueError(error),
            _ = tokio::time::sleep_until(retry_at.unwrap_or_else(Instant::now)), if retry_at.is_some() => Wake::RetryQueue,
            Some(state) = self.sync_states.next() => Wake::Sync(state),
            else => std::future::pending().await,
        }
    }

    pub(crate) async fn handle(
        &mut self,
        wake: Wake,
        voice: &voice::Controller,
        events: &UnboundedSender<Event>,
    ) {
        let client = &self.client;
        match wake {
            Wake::Rooms => {
                let _ = events.send(Event::Rooms(room_index(client).await));
            }
            Wake::Members(room) => {
                if let Some(active) = &mut self.open
                    && active.id() == room
                {
                    load_members(active, events);
                }
            }
            Wake::Timeline => {
                if let Some(active) = &mut self.open {
                    publish_timeline(active, client, &self.media_sender, events).await;
                }
            }
            Wake::Picture(room, key, picture) => {
                if let Some(active) = &mut self.open
                    && active.id() == room
                    && active.requested.contains(&key)
                {
                    active.pictures.insert(key, picture);
                    publish_timeline(active, client, &self.media_sender, events).await;
                }
            }
            Wake::QueueError(error) => match error {
                Ok(error) if error.is_recoverable => {
                    tracing::info!(room = %error.room_id, "send queue paused after a recoverable error");
                    self.queue_retry
                        .get_or_insert_with(|| Instant::now() + QUEUE_RETRY);
                }
                Ok(error) => {
                    tracing::warn!(room = %error.room_id, "the server refused a queued message")
                }
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    self.queue_retry
                        .get_or_insert_with(|| Instant::now() + QUEUE_RETRY);
                }
                Err(broadcast::error::RecvError::Closed) => {
                    self.queue_errors = client.send_queue().subscribe_errors()
                }
            },
            Wake::RetryQueue => {
                self.queue_retry = None;
                if self.online {
                    client.send_queue().set_enabled(true).await;
                } else {
                    self.queue_retry = Some(Instant::now() + QUEUE_RETRY);
                }
            }
            Wake::Sync(state) => {
                self.online = matches!(state, State::Running);
                let _ = events.send(Event::Sync(self.online));
                if self.online {
                    // Sync is back, so messages paused by a connection problem can go out.
                    client.send_queue().set_enabled(true).await;
                }
                if self.online && !self.stale_voice_checked {
                    // Synced sticky memberships now show whether Yapfast was
                    // killed during a call; remove what this device left.
                    self.stale_voice_checked = true;
                    voice.command(voice::Command::LeaveStale);
                }
                if self.online {
                    if let Some(user) = client.user_id() {
                        let _ = client.encryption().request_user_identity(user).await;
                    }
                    let refreshed = self.session.device_is_trusted().await.unwrap_or(false);
                    if refreshed != self.trusted {
                        if !refreshed {
                            voice.command(voice::Command::Leave);
                        }
                        self.trusted = refreshed;
                        let _ = events.send(Event::Trust {
                            trusted: refreshed,
                            error: None,
                        });
                    }
                }
            }
        }
    }

    /// Carries out a command for a Matrix room or the account. Returns false
    /// once signed out, which ends the chat actor.
    pub(crate) async fn command(
        &mut self,
        command: Command,
        voice: &mut voice::Controller,
        events: &UnboundedSender<Event>,
    ) -> bool {
        let session = &self.session;
        let client = &self.client;
        let open = &mut self.open;
        match command {
            Command::CloseRoom => {
                *open = None;
                self.updates = Box::pin(futures_util::stream::pending());
            }
            Command::Open(id) => {
                *open = None;
                self.updates = Box::pin(futures_util::stream::pending());
                // Another network's room opening closes this one, and that's all.
                if Network::of(&id) == Network::Discord {
                    return true;
                }
                let room = client.get_room(&match OwnedRoomId::try_from(id.clone()) {
                    Ok(id) => id,
                    Err(_) => return true,
                });
                let Some(room) = room else {
                    let _ = events.send(Event::RoomError {
                        room: id,
                        message: "This room is no longer joined. Select another conversation."
                            .into(),
                    });
                    return true;
                };
                if let Ok(Some(draft)) = room.load_composer_draft(None).await
                    && matches!(draft.draft_type, ComposerDraftType::NewMessage)
                    && !draft.plain_text.trim().is_empty()
                {
                    let _ = events.send(Event::Draft {
                        room: id.clone(),
                        body: draft.plain_text,
                    });
                }
                match TimelineBuilder::new(&room).build().await {
                    Ok(active) => {
                        let (_, stream) = active.subscribe().await;
                        self.updates = Box::pin(stream.map(|_| ()));
                        let mut active = OpenRoom {
                            timeline: Arc::new(active),
                            focused: false,
                            start: false,
                            pictures: HashMap::new(),
                            requested: HashSet::new(),
                            visible: HashSet::new(),
                            tasks: JoinSet::new(),
                            members: None,
                        };
                        load_members(&mut active, events);
                        watch_typing(&mut active, &room, events);
                        publish_timeline(&mut active, client, &self.media_sender, events).await;
                        // Sync often delivers only the latest event; fill the first screen.
                        if active.timeline.items().await.len() < usize::from(HISTORY_PAGE) {
                            active.start = active
                                .timeline
                                .paginate_backwards(HISTORY_PAGE)
                                .await
                                .unwrap_or(false);
                            publish_timeline(&mut active, client, &self.media_sender, events).await;
                        }
                        *open = Some(active);
                    }
                    Err(_) => {
                        let _ = events.send(Event::RoomError {
                            room: id,
                            message: "Could not open this conversation. Select it again to retry."
                                .into(),
                        });
                    }
                }
            }
            Command::Typing { room, typing } => typing_notice(client, &room, typing),
            Command::MarkRead { room, .. } => mark_read(client, &room),
            Command::FindPeople { query } => find_people(client, query, events),
            Command::StartDm { id, .. } => start_dm(client, id, events),
            Command::Send {
                room,
                body,
                reply,
                options,
                files,
            } => {
                typing_notice(client, &room, false);
                // A Matrix message carries one file, so each goes on its own,
                // then the text. Matrix has no spoiler for a file.
                let mut result = Ok(());
                for file in &files {
                    result = attach(session, open.as_ref(), &room, &file.path).await;
                    if result.is_err() {
                        break;
                    }
                }
                if result.is_ok() && (files.is_empty() || !body.trim().is_empty()) {
                    let reply = reply.as_deref().map(|item| (item, !options.quiet_reply));
                    result = send(session, open.as_ref(), &room, &body, reply).await;
                }
                let _ = events.send(Event::SendResult { room, body, result });
            }
            Command::Edit { room, item, body } => {
                let result = edit(session, open.as_ref(), &room, &item, &body)
                    .await
                    .map_err(Into::into);
                let _ = events.send(Event::EditResult { room, item, result });
            }
            Command::Forward {
                room,
                item,
                target,
                note,
                ..
            } => {
                let result = forward(session, open.as_ref(), &room, &item, &target, note).await;
                let _ = events.send(Event::Forwarded {
                    room: target,
                    result,
                });
            }
            Command::React { room, item, key } => {
                let result = match (active_timeline(open.as_ref(), &room), parse_item(&item)) {
                    (Some(timeline), Some(id)) if !key.is_empty() && key.len() <= 64 => timeline
                        .toggle_reaction(&id, &key)
                        .await
                        .map(|_| ())
                        .map_err(|_| ()),
                    _ => Err(()),
                };
                if result.is_err() {
                    let _ = events.send(Event::RoomError {
                        room,
                        message:
                            "Could not change the reaction. Check your connection and try again."
                                .into(),
                    });
                }
            }
            Command::Reactors {
                room,
                item,
                key,
                after,
                limit,
            } => {
                let result = match active_timeline(open.as_ref(), &room) {
                    Some(timeline) => {
                        Ok(reactors(timeline, &item, &key, after.as_deref(), limit).await)
                    }
                    None => Err("Open this conversation again, then retry.".into()),
                };
                let _ = events.send(Event::Reactors {
                    room,
                    item,
                    key,
                    after,
                    limit,
                    result,
                });
            }
            // Yapfast removes others' Matrix reactions only as Discord's.
            Command::RemoveReactor { .. } => {}
            // Yapfast shows only Discord's polls.
            Command::Vote { .. }
            | Command::SendPoll { .. }
            | Command::EndPoll { .. }
            | Command::Voters { .. } => {}
            // Yapfast pins only Discord's messages.
            Command::Pin { .. } => {}
            // Matrix's attachments have no address, which copying asks for.
            Command::CopyAttachment { .. } => {}
            Command::Delete { room, item } => {
                let result = match (active_timeline(open.as_ref(), &room), parse_item(&item)) {
                    (Some(timeline), Some(id)) => timeline.redact(&id, None).await.map_err(|_| ()),
                    _ => Err(()),
                };
                if result.is_err() {
                    let _ = events.send(Event::RoomError {
                        room,
                        message:
                            "Could not delete the message. Check your connection and try again."
                                .into(),
                    });
                }
            }
            Command::Retry { room, item } => {
                if let (Some(timeline), Some(id)) =
                    (active_timeline(open.as_ref(), &room), parse_item(&item))
                {
                    // A recoverable failure paused the whole room; resume it too.
                    timeline.room().send_queue().set_enabled(true);
                    let _ = timeline.retry_send(&id, SendTarget::Event).await;
                }
            }
            Command::Cancel { room, item } => {
                let result = match (active_timeline(open.as_ref(), &room), parse_item(&item)) {
                    (Some(timeline), Some(id)) => timeline
                        .abort_send(&id, SendTarget::Event)
                        .await
                        .map_err(|_| ()),
                    _ => Err(()),
                };
                if !matches!(result, Ok(true)) {
                    let _ = events.send(Event::RoomError {
                        room,
                        message: "That message was already sent, so it could not be cancelled."
                            .into(),
                    });
                }
            }
            // Matrix messages carry one attachment.
            Command::SaveAttachment {
                room, item, path, ..
            } => {
                let name = path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let result = save_attachment(client, open.as_ref(), &room, &item, &path).await;
                let _ = events.send(Event::Saved { room, name, result });
            }
            Command::ViewImage { room, item, index } => {
                view_image(client, open, room, item, index, events).await;
            }
            Command::SaveDraft { room, body } => {
                if let Ok(id) = OwnedRoomId::try_from(room)
                    && let Some(room) = client.get_room(&id)
                {
                    let _ = if body.trim().is_empty() {
                        room.clear_composer_draft(None).await
                    } else {
                        room.save_composer_draft(
                            ComposerDraft {
                                plain_text: body,
                                html_text: None,
                                draft_type: ComposerDraftType::NewMessage,
                                attachments: vec![],
                            },
                            None,
                        )
                        .await
                    };
                }
            }
            Command::Earlier(id) => {
                // The actor carries out one command at a time, so a room
                // loads one page at once.
                let history = match open.as_mut().filter(|active| active.id() == id) {
                    Some(active) => match active.timeline.paginate_backwards(HISTORY_PAGE).await {
                        Ok(start) => {
                            active.start = start;
                            publish_timeline(active, client, &self.media_sender, events).await;
                            active.history()
                        }
                        Err(_) => {
                            let _ = events.send(Event::RoomError { room: id.clone(), message: "Could not load older messages. Check your connection and try again.".into() });
                            History::Failed
                        }
                    },
                    None => History::Failed,
                };
                let _ = events.send(Event::HistoryLoaded {
                    room: id,
                    direction: Direction::Earlier,
                    history,
                });
            }
            Command::Later(id) => {
                let history = match open.as_mut().filter(|active| active.id() == id) {
                    Some(active) if active.focused => {
                        match active.timeline.paginate_forwards(HISTORY_PAGE).await {
                            Ok(end) => {
                                // A window doesn't follow live messages, so
                                // at the newest one the live timeline takes
                                // its place. It starts from the newest the
                                // SDK keeps, and earlier pages load until it
                                // has the window's first message again: rows
                                // going from above the screen would move it.
                                if end {
                                    let first = active.first_event().await;
                                    let pages = active.timeline.items().await.len()
                                        / usize::from(HISTORY_PAGE)
                                        + 1;
                                    let start = active.start;
                                    if active.refocus(None, &mut self.updates).await {
                                        active.start = start;
                                        for _ in 0..pages {
                                            let reached = match &first {
                                                Some(first) => active.has_event(first).await,
                                                None => true,
                                            };
                                            if reached {
                                                break;
                                            }
                                            match active
                                                .timeline
                                                .paginate_backwards(HISTORY_PAGE)
                                                .await
                                            {
                                                Ok(false) => {}
                                                Ok(true) => {
                                                    active.start = true;
                                                    break;
                                                }
                                                Err(_) => break,
                                            }
                                        }
                                    }
                                }
                                publish_timeline(active, client, &self.media_sender, events).await;
                                active.history()
                            }
                            Err(_) => {
                                let _ = events.send(Event::RoomError {
                                    room: id.clone(),
                                    message: "Could not load newer messages. Check your connection and try again.".into(),
                                });
                                History::Failed
                            }
                        }
                    }
                    Some(active) => active.history(),
                    None => History::Failed,
                };
                let _ = events.send(Event::HistoryLoaded {
                    room: id,
                    direction: Direction::Later,
                    history,
                });
            }
            Command::Around { room, item } => {
                let Some(active) = open.as_mut().filter(|active| active.id() == room) else {
                    let _ = events.send(Event::HistoryLoaded {
                        room,
                        direction: Direction::Around,
                        history: History::Failed,
                    });
                    return true;
                };
                let event = match parse_item(&item) {
                    Some(TimelineEventItemId::EventId(event)) => Some(event),
                    _ => None,
                };
                let found = match event {
                    Some(event) => active.refocus(Some(event), &mut self.updates).await,
                    None => false,
                };
                if !found {
                    let _ = events.send(Event::RoomError {
                        room: room.clone(),
                        message: "Yapfast couldn't find that message.".into(),
                    });
                }
                // Said before the window is published; see Command::Around.
                let _ = events.send(Event::HistoryLoaded {
                    room,
                    direction: Direction::Around,
                    history: if found {
                        active.history()
                    } else {
                        History::Failed
                    },
                });
                publish_timeline(active, client, &self.media_sender, events).await;
            }
            Command::Present(room) => {
                if let Some(active) = open
                    .as_mut()
                    .filter(|active| active.id() == room && active.focused)
                    && active.refocus(None, &mut self.updates).await
                {
                    publish_timeline(active, client, &self.media_sender, events).await;
                }
            }
            Command::VisibleMessages { room, items, .. } => {
                if let Some(active) = open.as_mut().filter(|active| active.id() == room) {
                    let visible = items.into_iter().collect();
                    if active.visible != visible {
                        active.visible = visible;
                        publish_timeline(active, client, &self.media_sender, events).await;
                    }
                }
            }
            Command::Recover(key) => {
                if voice.states.borrow().active() {
                    let _ = events.send(Event::Error(
                        "Leave voice before recovering this device.".into(),
                    ));
                    return true;
                }
                if let Some(previous) = self.sync.take() {
                    previous.stop().await;
                }
                let result = session.recover(&key).await;
                self.trusted = session.device_is_trusted().await.unwrap_or(false);
                let _ = events.send(Event::Trust {
                    trusted: self.trusted,
                    error: result.err().map(
                        |_| "Recovery failed. Check your recovery key and connection, then retry.",
                    ),
                });
                self.restart_sync = true;
            }
            Command::RefreshTrust => {
                if let Some(user) = client.user_id() {
                    let _ = client.encryption().request_user_identity(user).await;
                }
                let result = session.device_is_trusted().await;
                self.trusted = result.as_ref().copied().unwrap_or(false);
                if !self.trusted {
                    voice.command(voice::Command::Leave);
                }
                let _ = events.send(Event::Trust {
                    trusted: result.as_ref().copied().unwrap_or(false),
                    error: result.err().map(
                        |_| "Could not check device verification. Check your connection and retry.",
                    ),
                });
            }
            Command::RetrySync => self.restart_sync = true,
            Command::Logout => {
                voice.stop().await;
                if let Some(previous) = self.sync.take() {
                    previous.stop().await;
                }
                match session.logout().await {
                    Ok(()) => {
                        let _ = events.send(Event::LoggedOut);
                        return false;
                    }
                    Err(_) => {
                        voice.restart(session.lease());
                        let _ = events.send(Event::Voice(voice::State::default()));
                        let _ = events.send(Event::Error("Sign-out failed. Your session is kept. Check the connection and try again.".into()));
                        self.restart_sync = true;
                    }
                }
            }
            // `Command::route` sends these elsewhere.
            // Matrix videos are files to save for now.
            Command::Voice(_)
            | Command::Discord(_)
            | Command::Aside(_)
            | Command::PlayVideo { .. }
            | Command::Video(_) => {}
        }
        true
    }

    pub(crate) async fn stop(mut self) {
        if let Some(service) = self.sync.take() {
            service.stop().await;
        }
        self.client.remove_event_handler(self.space_handler);
        self.client.remove_event_handler(self.member_handler);
    }
}

/// Says who else is typing in the open room until it closes.
fn watch_typing(active: &mut OpenRoom, room: &Room, events: &UnboundedSender<Event>) {
    let (room, events) = (room.clone(), events.clone());
    active.tasks.spawn(async move {
        let (_guard, mut typing) = room.subscribe_to_typing_notifications();
        let id = room.room_id().to_string();
        loop {
            let users = match typing.recv().await {
                Ok(users) => users,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => break,
            };
            let mut names = Vec::new();
            for user in users {
                let name = match room.get_member_no_sync(&user).await {
                    Ok(Some(member)) => member.name().to_owned(),
                    _ => user.localpart().to_owned(),
                };
                names.push(name);
            }
            names.sort();
            let _ = events.send(Event::Typing {
                room: id.clone(),
                names,
            });
        }
    });
}

/// Looks for people to start a DM with in the homeserver's user directory; a full
/// Matrix id is offered even when the directory doesn't list it.
fn find_people(client: &Client, query: String, events: &UnboundedSender<Event>) {
    let query = query.trim().to_owned();
    if query.is_empty() {
        let _ = events.send(Event::People {
            query,
            discord: false,
            people: vec![],
        });
        return;
    }
    let (client, events) = (client.clone(), events.clone());
    tokio::spawn(async move {
        let me = client.user_id().map(ToOwned::to_owned);
        let mut people: Vec<Contact> = match client.search_users(&query, 10).await {
            Ok(found) => found
                .results
                .into_iter()
                .filter(|user| Some(&user.user_id) != me.as_ref())
                .map(|user| Contact {
                    name: user
                        .display_name
                        .unwrap_or_else(|| user.user_id.localpart().to_owned()),
                    detail: user.user_id.to_string(),
                    id: user.user_id.to_string(),
                    avatar: None,
                    discord: false,
                })
                .collect(),
            Err(error) => {
                tracing::debug!(%error, "could not search the Matrix user directory");
                vec![]
            }
        };
        if let Ok(user) = UserId::parse(&query)
            && Some(&user) != me.as_ref()
            && !people.iter().any(|person| person.id == user.as_str())
        {
            people.insert(
                0,
                Contact {
                    name: user.localpart().to_owned(),
                    detail: user.to_string(),
                    id: user.to_string(),
                    avatar: None,
                    discord: false,
                },
            );
        }
        let _ = events.send(Event::People {
            query,
            discord: false,
            people,
        });
    });
}

/// Opens the DM with someone, starting an encrypted one if there's none yet.
fn start_dm(client: &Client, id: String, events: &UnboundedSender<Event>) {
    let (client, events) = (client.clone(), events.clone());
    tokio::spawn(async move {
        let result = match UserId::parse(&id) {
            Ok(user) => match client.get_dm_room(&user) {
                Some(room) => Ok(room.room_id().to_string()),
                None => client
                    .create_dm(&user)
                    .await
                    .map(|room| room.room_id().to_string())
                    .map_err(|error| {
                        tracing::warn!(%error, "could not start a Matrix DM");
                        "Could not start the DM. Check the Matrix id and your connection.".into()
                    }),
            },
            Err(_) => Err("That isn't a Matrix id, such as @name:example.org.".into()),
        };
        let _ = events.send(Event::DmStarted(result));
    });
}

/// Marks a room read with a receipt on its newest event, which the
/// homeserver names, as the room may not be open. The receipt also clears
/// the room's unread flag.
fn mark_read(client: &Client, room: &str) {
    let Some(room) = OwnedRoomId::try_from(room)
        .ok()
        .and_then(|id| client.get_room(&id))
    else {
        return;
    };
    tokio::spawn(async move {
        let mut options = MessagesOptions::backward();
        options.limit = UInt::from(1u32);
        let newest = match room.messages(options).await {
            Ok(messages) => messages
                .chunk
                .iter()
                .find_map(|event| event.event_id().map(ToOwned::to_owned)),
            Err(error) => {
                tracing::debug!(%error, "could not find a room's newest event to mark it read");
                None
            }
        };
        if let Some(event) = newest
            && let Err(error) = room
                .send_single_receipt(ReceiptType::Read, ReceiptThread::Unthreaded, event)
                .await
        {
            tracing::debug!(%error, "could not mark a room read");
        }
    });
}

/// Tells the room this account is typing, or stopped; matrix-sdk sends it only
/// when the room needs to hear it again.
fn typing_notice(client: &Client, room: &str, typing: bool) {
    let Some(room) = OwnedRoomId::try_from(room)
        .ok()
        .and_then(|id| client.get_room(&id))
    else {
        return;
    };
    tokio::spawn(async move {
        if let Err(error) = room.typing_notice(typing).await {
            tracing::debug!(%error, "could not send a typing notice");
        }
    });
}

/// Read the open room's joined members in the background. The first read fetches
/// the full list from the homeserver, because sync loads members lazily.
fn load_members(open: &mut OpenRoom, events: &UnboundedSender<Event>) {
    if let Some(previous) = open.members.take() {
        previous.abort();
    }
    let room = open.timeline.room().clone();
    let events = events.clone();
    open.members = Some(open.tasks.spawn(async move {
        let members = room
            .members(RoomMemberships::JOIN)
            .await
            .map(|members| {
                let mut members: Vec<_> = members
                    .iter()
                    .map(|member| ChatMember {
                        id: member.user_id().to_string(),
                        name: member
                            .display_name()
                            .map_or_else(|| member.user_id().to_string(), ToOwned::to_owned),
                        username: None,
                        role: match member.power_level() {
                            UserPowerLevel::Int(level) => match i64::from(level) {
                                100.. => Role::Admin,
                                50.. => Role::Moderator,
                                _ => Role::Member,
                            },
                            // A room creator's level is infinite.
                            _ => Role::Admin,
                        },
                        avatar: None,
                        group: None,
                        group_icon: None,
                        role_icon: None,
                        presence: None,
                        color: None,
                        activity: None,
                        activities: Default::default(),
                        roles: Default::default(),
                        timed_out_until: None,
                        tag: None,
                        decoration: None,
                        nameplate: None,
                        style: None,
                    })
                    .collect();
                sort_members(&mut members);
                members
            })
            .map_err(|_| "Could not load the member list. Open the channel again to retry.");
        let _ = events.send(Event::Members {
            room: room.room_id().to_string(),
            members,
        });
    }));
}

async fn room_index(client: &Client) -> RoomIndex {
    let mut result = Vec::new();
    let mut servers = Vec::new();
    let mut children = HashMap::new();
    for space in client
        .joined_rooms()
        .into_iter()
        .filter(|room| room.is_space())
    {
        let links = space
            .get_state_events_static::<SpaceChildEventContent>()
            .await
            .unwrap_or_default()
            .into_iter()
            .filter_map(|raw| {
                let event = raw.deserialize().ok()?;
                if event.as_sync()?.as_original()?.content.via.is_empty() {
                    return None;
                }
                Some(event.state_key().to_string())
            })
            .collect::<Vec<_>>();
        children.insert(space.room_id().to_string(), links);
        servers.push(ChatServer {
            id: space.room_id().to_string(),
            name: space
                .cached_display_name()
                .map(|name| name.to_string())
                .unwrap_or_else(|| space.room_id().to_string()),
            channels: Vec::new(),
            discord: false,
            muted: false,
            unavailable: false,
            icon: None,
            folder: None,
            hide_muted: false,
            community: false,
            pinned: Vec::new(),
            ..Default::default()
        });
    }
    for room in client
        .joined_rooms()
        .into_iter()
        .filter(|room| !room.is_space())
    {
        let direct = room.is_direct().await.unwrap_or(false);
        // A DM's other person, when it's with exactly one Matrix user.
        let targets = room.direct_targets();
        let person = match (direct, targets.len()) {
            (true, 1) => targets
                .iter()
                .map(ToString::to_string)
                .find(|target| target.starts_with('@')),
            _ => None,
        };
        result.push(ChatRoom {
            id: room.room_id().to_string(),
            network: Network::Matrix,
            capabilities: Network::Matrix.capabilities(),
            name: room
                .cached_display_name()
                .map(|name| name.to_string())
                .unwrap_or_else(|| room.room_id().to_string()),
            direct,
            voice: room.is_call(),
            encrypted: room.encryption_state().is_encrypted(),
            members: room.joined_members_count(),
            // Counted on this device from read receipts. For a room that isn't open,
            // sync often delivers only the newest event, so this shows that something
            // is unread rather than how much; encrypted mentions are found the same way.
            unread: room.num_unread_messages(),
            mentions: room.num_unread_mentions(),
            muted: false,
            last_read: room
                .read_receipts()
                .latest_active
                .map(|receipt| format!("e:{}", receipt.event_id)),
            locked: false,
            join_locked: false,
            read_only: false,
            slowed: false,
            timed_out_until: None,
            category: String::new(),
            category_id: None,
            request: false,
            person,
            people: vec![],
            ringing: vec![],
            incoming: false,
            avatar: None,
            upload_limit: MAX_ATTACHMENT_BYTES,
            presence: None,
            activity: None,
            activities: Default::default(),
            parent: None,
            forum: false,
            unlisted: false,
            tag: None,
            decoration: None,
            nameplate: None,
            style: None,
            official: false,
            empty: false,
            preview: None,
        });
    }
    result.sort_by(|a, b| {
        b.direct
            .cmp(&a.direct)
            .then_with(|| a.voice.cmp(&b.voice))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    let joined: HashSet<_> = result.iter().map(|room| room.id.as_str()).collect();
    for server in &mut servers {
        let mut visited = HashSet::new();
        let mut pending = vec![server.id.clone()];
        while let Some(id) = pending.pop() {
            if !visited.insert(id.clone()) {
                continue;
            }
            if let Some(links) = children.get(&id) {
                pending.extend(links.iter().cloned());
            } else if joined.contains(id.as_str()) {
                server.channels.push(id);
            }
        }
        server.channels.sort();
    }
    servers.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then_with(|| a.id.cmp(&b.id))
    });
    RoomIndex {
        rooms: result,
        servers,
    }
}

pub fn item_key(id: &TimelineEventItemId) -> String {
    match id {
        TimelineEventItemId::EventId(id) => format!("e:{id}"),
        TimelineEventItemId::TransactionId(id) => format!("t:{id}"),
    }
}

fn parse_item(key: &str) -> Option<TimelineEventItemId> {
    if let Some(id) = key.strip_prefix("e:") {
        OwnedEventId::try_from(id)
            .ok()
            .map(TimelineEventItemId::EventId)
    } else {
        key.strip_prefix("t:")
            .filter(|id| !id.is_empty())
            .map(|id| TimelineEventItemId::TransactionId(OwnedTransactionId::from(id)))
    }
}

/// Who reacted to `item` with `key`, in the order they reacted: up to
/// `limit` of them, after the one with id `after`. The timeline holds them
/// all, so only their names are looked up.
async fn reactors(
    timeline: &Timeline,
    item: &str,
    key: &str,
    after: Option<&str>,
    limit: u32,
) -> Vec<Reactor> {
    let senders: Vec<OwnedUserId> = {
        let items = timeline.items().await;
        let Some(event) = items
            .iter()
            .filter_map(|item| item.as_event())
            .find(|event| item_key(&event.identifier()) == item)
        else {
            return Vec::new();
        };
        let Some(senders) = event.reactions().get(key) else {
            return Vec::new();
        };
        let start = after
            .and_then(|after| senders.keys().position(|sender| sender.as_str() == after))
            .map_or(0, |at| at + 1);
        senders
            .keys()
            .skip(start)
            .take(limit as usize)
            .cloned()
            .collect()
    };
    let own = timeline.room().own_user_id().to_owned();
    let own = Some(&*own);
    let mut people = Vec::with_capacity(senders.len());
    for sender in senders {
        let name = match timeline.room().get_member_no_sync(&sender).await {
            Ok(Some(member)) => member.name().to_owned(),
            _ => sender.localpart().to_owned(),
        };
        people.push(Reactor {
            id: sender.to_string(),
            name,
            username: sender.to_string(),
            avatar: String::new(),
            me: own == Some(&*sender),
        });
    }
    people
}

fn active_timeline<'a>(open: Option<&'a OpenRoom>, room: &str) -> Option<&'a Timeline> {
    open.filter(|open| open.id() == room)
        .map(|open| open.timeline.as_ref())
}

/// Yapfast sends only from a cross-signed device, into joined encrypted rooms.
async fn writable<'a>(
    session: &Session,
    open: Option<&'a OpenRoom>,
    room: &str,
) -> Result<&'a Timeline, &'static str> {
    if !session.device_is_trusted().await.unwrap_or(false) {
        return Err(
            "Verify this device before sending. Use your recovery key or another Matrix client.",
        );
    }
    let timeline =
        active_timeline(open, room).ok_or("Open this conversation again, then retry.")?;
    let room: &Room = timeline.room();
    if room.state() != matrix_sdk::RoomState::Joined || !room.encryption_state().is_encrypted() {
        return Err("Sending is available only in joined, encrypted rooms.");
    }
    Ok(timeline)
}

fn check_text(body: &str) -> Result<(), &'static str> {
    if body.trim().is_empty() || body.len() > MAX_MESSAGE_BYTES {
        Err("Messages must contain text and be under 16 KB.")
    } else {
        Ok(())
    }
}

async fn send(
    session: &Session,
    open: Option<&OpenRoom>,
    room: &str,
    body: &str,
    reply: Option<(&str, bool)>,
) -> Result<(), &'static str> {
    check_text(body)?;
    let timeline = writable(session, open, room).await?;
    let mut content = RoomMessageEventContentWithoutRelation::text_markdown(body);
    content.mentions = Some(mentioned(body));
    let queued = match reply {
        Some((item, ping)) => {
            let Some(TimelineEventItemId::EventId(event)) = parse_item(item) else {
                return Err("You can reply once that message has been sent.");
            };
            if ping {
                timeline.send_reply(content, event).await
            } else {
                // As send_reply does, without mentioning who's replied to.
                let reply = ReplyTo {
                    event_id: event,
                    enforce_thread: if timeline.is_threaded() {
                        EnforceThread::Threaded(ReplyWithinThread::Yes)
                    } else {
                        EnforceThread::MaybeThreaded
                    },
                    add_mentions: AddMentions::No,
                };
                let content = timeline
                    .room()
                    .make_reply_event(content, reply)
                    .await
                    .map_err(|_| "Yapfast could not find the message to reply to.")?;
                timeline.send(content.into()).await
            }
        }
        None => timeline.send(content.with_relation(None).into()).await,
    };
    queued.map_err(|_| "Yapfast could not queue this message. Your draft is kept.")?;
    // The queue owns the message now, including across restarts.
    let _ = timeline.room().clear_composer_draft(None).await;
    Ok(())
}

async fn edit(
    session: &Session,
    open: Option<&OpenRoom>,
    room: &str,
    item: &str,
    body: &str,
) -> Result<(), &'static str> {
    check_text(body)?;
    let timeline = writable(session, open, room).await?;
    let id = parse_item(item).ok_or("This message can no longer be edited.")?;
    let mut content = RoomMessageEventContentWithoutRelation::text_markdown(body);
    content.mentions = Some(mentioned(body));
    timeline
        .edit(&id, EditedContent::RoomMessage(content))
        .await
        .map_err(|_| "This message can no longer be edited.")
}

/// The people a message's Markdown links to, as the composer's mentions do, so
/// they're mentioned on purpose and notified.
fn mentioned(body: &str) -> Mentions {
    const LINK: &str = "](https://matrix.to/#/";
    let users = body.match_indices(LINK).filter_map(|(at, _)| {
        let rest = &body[at + LINK.len()..];
        UserId::parse(&rest[..rest.find(')')?]).ok()
    });
    Mentions::with_user_ids(users)
}

/// Forward a message's current content, without its reply or edit relations,
/// into another joined encrypted room, then `note` as a message of its own.
/// An attachment keeps its encrypted file, whose key travels inside the new,
/// encrypted event.
async fn forward(
    session: &Session,
    open: Option<&OpenRoom>,
    room: &str,
    item: &str,
    target: &str,
    note: Option<String>,
) -> Result<(), ForwardFailure> {
    let target = forward_message(session, open, room, item, target)
        .await
        .map_err(|why| ForwardFailure::Forward(why.into()))?;
    let Some(note) = note else {
        return Ok(());
    };
    check_text(&note).map_err(|why| ForwardFailure::Note(why.into()))?;
    let mut content = RoomMessageEventContent::text_markdown(&note);
    content.mentions = Some(mentioned(&note));
    target
        .send_queue()
        .send(content.into())
        .await
        .map(|_| ())
        .map_err(|_| {
            ForwardFailure::Note(
                "Yapfast could not queue the message you added. Check your connection.".into(),
            )
        })
}

/// Queues a copy of a message's current content in `target`, which it
/// gives back.
async fn forward_message(
    session: &Session,
    open: Option<&OpenRoom>,
    room: &str,
    item: &str,
    target: &str,
) -> Result<matrix_sdk::Room, &'static str> {
    if !session.device_is_trusted().await.unwrap_or(false) {
        return Err(
            "Verify this device before forwarding. Use your recovery key or another Matrix client.",
        );
    }
    let timeline =
        active_timeline(open, room).ok_or("Open this conversation again, then retry.")?;
    let id = parse_item(item).ok_or("This message can't be forwarded.")?;
    let content = timeline
        .items()
        .await
        .iter()
        .filter_map(|item| item.as_event())
        .find(|event| event.identifier() == id)
        .and_then(|event| match event.content() {
            TimelineItemContent::MsgLike(content) => match &content.kind {
                MsgLikeKind::Message(message) => Some(message.msgtype().clone()),
                _ => None,
            },
            _ => None,
        })
        .ok_or("This message can't be forwarded.")?;
    let target = OwnedRoomId::try_from(target)
        .ok()
        .and_then(|id| timeline.room().client().get_room(&id))
        .ok_or("That conversation is no longer joined.")?;
    if target.state() != matrix_sdk::RoomState::Joined || !target.encryption_state().is_encrypted()
    {
        return Err("Forwarding is available only into joined, encrypted rooms.");
    }
    target
        .send_queue()
        .send(RoomMessageEventContent::new(content).into())
        .await
        .map_err(
            |_| "Yapfast could not queue the forwarded message. Check your connection and retry.",
        )?;
    Ok(target)
}

fn mime_for(path: &Path) -> mime::Mime {
    let extension = path
        .extension()
        .map(|extension| extension.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    match extension.as_str() {
        "png" => mime::IMAGE_PNG,
        "jpg" | "jpeg" => mime::IMAGE_JPEG,
        "gif" => mime::IMAGE_GIF,
        "webp" => "image/webp"
            .parse()
            .unwrap_or(mime::APPLICATION_OCTET_STREAM),
        "txt" | "log" | "md" => mime::TEXT_PLAIN_UTF_8,
        "pdf" => mime::APPLICATION_PDF,
        "json" => mime::APPLICATION_JSON,
        "mp3" => "audio/mpeg"
            .parse()
            .unwrap_or(mime::APPLICATION_OCTET_STREAM),
        "ogg" | "opus" => "audio/ogg"
            .parse()
            .unwrap_or(mime::APPLICATION_OCTET_STREAM),
        "mp4" => "video/mp4"
            .parse()
            .unwrap_or(mime::APPLICATION_OCTET_STREAM),
        "webm" => "video/webm"
            .parse()
            .unwrap_or(mime::APPLICATION_OCTET_STREAM),
        "zip" => "application/zip"
            .parse()
            .unwrap_or(mime::APPLICATION_OCTET_STREAM),
        _ => mime::APPLICATION_OCTET_STREAM,
    }
}

async fn attach(
    session: &Session,
    open: Option<&OpenRoom>,
    room: &str,
    path: &Path,
) -> Result<(), &'static str> {
    let timeline = writable(session, open, room).await?;
    let metadata = tokio::fs::metadata(path)
        .await
        .map_err(|_| "Yapfast could not read that file.")?;
    if !metadata.is_file() {
        return Err("Choose a file to send.");
    }
    if metadata.len() > MAX_ATTACHMENT_BYTES {
        return Err("Files must be 50 MB or smaller.");
    }
    let size = UInt::new(metadata.len());
    let mime = mime_for(path);
    let info = if mime.type_() == mime::IMAGE {
        let file = path.to_owned();
        let dimensions = tokio::task::spawn_blocking(move || {
            image::ImageReader::open(file)
                .ok()?
                .with_guessed_format()
                .ok()?
                .into_dimensions()
                .ok()
        })
        .await
        .ok()
        .flatten();
        AttachmentInfo::Image(BaseImageInfo {
            width: dimensions.and_then(|(width, _)| UInt::new(width.into())),
            height: dimensions.and_then(|(_, height)| UInt::new(height.into())),
            size,
            blurhash: None,
            is_animated: None,
        })
    } else {
        AttachmentInfo::File(BaseFileInfo { size })
    };
    // The send queue encrypts and uploads the file, survives restarts, and
    // shows a local echo until the server confirms it.
    timeline
        .send_attachment(
            AttachmentSource::File(path.to_owned()),
            mime,
            AttachmentConfig {
                info: Some(info),
                ..Default::default()
            },
        )
        .use_send_queue()
        .await
        .map_err(|_| "Yapfast could not queue that file. Check that it is readable and retry.")
}

/// The media source and display name of an item's attachment.
fn attachment_source(item: &EventTimelineItem) -> Option<(MediaSource, String)> {
    let TimelineItemContent::MsgLike(content) = item.content() else {
        return None;
    };
    let MsgLikeKind::Message(message) = &content.kind else {
        return None;
    };
    match message.msgtype() {
        MessageType::Image(content) => Some((content.source.clone(), content.filename().into())),
        MessageType::File(content) => Some((content.source.clone(), content.filename().into())),
        MessageType::Video(content) => Some((content.source.clone(), content.filename().into())),
        MessageType::Audio(content) => Some((content.source.clone(), content.filename().into())),
        _ => None,
    }
}

/// The full-size source of an item's image.
fn image_source(item: &EventTimelineItem) -> Option<MediaSource> {
    let TimelineItemContent::MsgLike(content) = item.content() else {
        return None;
    };
    let MsgLikeKind::Message(message) = &content.kind else {
        return None;
    };
    match message.msgtype() {
        MessageType::Image(content) => Some(content.source.clone()),
        _ => None,
    }
}

/// Downloads an item's image and decodes it for the viewer, in the open
/// room's tasks, so closing the room cancels it.
async fn view_image(
    client: &Client,
    open: &mut Option<OpenRoom>,
    room: String,
    item: String,
    index: usize,
    events: &UnboundedSender<Event>,
) {
    let source = match (active_timeline(open.as_ref(), &room), parse_item(&item)) {
        (Some(timeline), Some(id)) if index == 0 => timeline
            .items()
            .await
            .iter()
            .filter_map(|item| item.as_event())
            .find(|event| event.identifier() == id)
            .and_then(image_source),
        _ => None,
    };
    let (Some(open), Some(source)) = (open.as_mut(), source) else {
        let _ = events.send(Event::Image {
            room,
            item,
            index,
            picture: Err("This image is no longer available."),
        });
        return;
    };
    let (client, events) = (client.clone(), events.clone());
    open.tasks.spawn(async move {
        let picture =
            match matrix_media::content(&client, &source, MAX_PREVIEW_DOWNLOAD_BYTES).await {
                Ok(data) => tokio::task::spawn_blocking(move || decode_view(&data))
                    .await
                    .ok()
                    .flatten()
                    .map(Arc::new)
                    .ok_or("Yapfast can't show this image. Save it to open it elsewhere."),
                Err(matrix_media::Failed::TooLarge) => {
                    Err("This image is too large to show here. Save it to open it elsewhere.")
                }
                Err(_) => Err(
                    "Yapfast couldn't download this image. Check your connection and try again.",
                ),
            };
        let _ = events.send(Event::Image {
            room,
            item,
            index,
            picture,
        });
    });
}

async fn save_attachment(
    client: &Client,
    open: Option<&OpenRoom>,
    room: &str,
    item: &str,
    path: &Path,
) -> Result<(), &'static str> {
    let timeline =
        active_timeline(open, room).ok_or("Open this conversation again, then retry.")?;
    let id = parse_item(item).ok_or("This file is no longer available.")?;
    let source = timeline
        .items()
        .await
        .iter()
        .filter_map(|item| item.as_event())
        .find(|event| event.identifier() == id)
        .and_then(attachment_source)
        .map(|(source, _)| source)
        .ok_or("This file is no longer available.")?;
    matrix_media::save(client, &source, MAX_SAVE_BYTES, path)
        .await
        .map_err(|failed| match failed {
            matrix_media::Failed::TooLarge => "This file is larger than Yapfast saves.",
            matrix_media::Failed::Write => {
                "Yapfast could not write the file there. Choose another folder."
            }
            matrix_media::Failed::Download | matrix_media::Failed::Decrypt => {
                "Yapfast could not download or decrypt this file. Check your connection and retry."
            }
        })
}

/// The largest attachment Yapfast saves. It goes to disk as it arrives; a
/// sender can't make it larger by saying it's small.
const MAX_SAVE_BYTES: u64 = 4 << 30;

fn media_key(source: &MediaSource) -> String {
    match source {
        MediaSource::Plain(uri) => uri.to_string(),
        MediaSource::Encrypted(file) => file.url.to_string(),
    }
}

/// A Matrix notification as Yapfast shows it: a new message the push rules say should
/// notify, sent by someone else since Yapfast started.
async fn notification_of(
    notification: matrix_sdk::sync::Notification,
    room: &Room,
    client: &Client,
    started: matrix_sdk::ruma::MilliSecondsSinceUnixEpoch,
) -> Option<Notification> {
    use matrix_sdk::{
        deserialized_responses::RawAnySyncOrStrippedTimelineEvent,
        ruma::events::{AnySyncMessageLikeEvent, AnySyncTimelineEvent, SyncMessageLikeEvent},
    };
    if !notification
        .actions
        .iter()
        .any(|action| action.should_notify())
    {
        return None;
    }
    let RawAnySyncOrStrippedTimelineEvent::Sync(raw) = &notification.event else {
        return None;
    };
    let event = raw.deserialize().ok()?;
    if event.origin_server_ts() < started || client.user_id() == Some(event.sender()) {
        return None;
    }
    let AnySyncTimelineEvent::MessageLike(event) = event else {
        return None;
    };
    let body = match &event {
        AnySyncMessageLikeEvent::RoomMessage(SyncMessageLikeEvent::Original(message)) => {
            match &message.content.msgtype {
                MessageType::Image(_) => "Sent a picture".into(),
                MessageType::File(_) | MessageType::Video(_) | MessageType::Audio(_) => {
                    "Sent a file".into()
                }
                other => notification_text(other.body()),
            }
        }
        AnySyncMessageLikeEvent::RoomEncrypted(_) => "Encrypted message".into(),
        _ => return None,
    };
    let author = match room.get_member_no_sync(event.sender()).await {
        Ok(Some(member)) => member.name().to_owned(),
        _ => event.sender().to_string(),
    };
    Some(Notification {
        room: room.room_id().to_string(),
        author,
        body,
        avatar: None,
        mention: notification
            .actions
            .iter()
            .any(|action| action.is_highlight()),
    })
}

fn display_name(
    sender: &UserId,
    profile: &TimelineDetails<matrix_sdk_ui::timeline::Profile>,
) -> String {
    match profile {
        TimelineDetails::Ready(profile) => profile
            .display_name
            .clone()
            .unwrap_or_else(|| sender.to_string()),
        _ => sender.to_string(),
    }
}

/// Whether a message calls on `own` other than by replying: with @room,
/// or by its user ID in the text or a link to it in the formatted text, as
/// a mention pill is. A reply lists whom it answers in its mentions, so
/// those alone don't tell.
fn calls_by_name(text: &matrix_sdk_ui::timeline::Message, own: &UserId) -> bool {
    let formatted = match text.msgtype() {
        MessageType::Text(content) => content.formatted.as_ref(),
        MessageType::Notice(content) => content.formatted.as_ref(),
        MessageType::Emote(content) => content.formatted.as_ref(),
        _ => None,
    };
    let own = own.as_str();
    // A matrix.to link may escape the ID's `@` and `:`.
    let escaped = own.replace('@', "%40").replace(':', "%3A");
    text.mentions().is_some_and(|mentions| mentions.room)
        || text.body().contains(own)
        || formatted.is_some_and(|formatted| {
            formatted.body.contains(own) || formatted.body.contains(&escaped)
        })
}

fn summary(content: &TimelineItemContent) -> String {
    match content {
        TimelineItemContent::MsgLike(content) => match &content.kind {
            MsgLikeKind::Message(message) => match message.msgtype() {
                MessageType::Image(_) => "Image".into(),
                MessageType::File(content) => content.filename().into(),
                _ => plain_snippet(message.body()),
            },
            MsgLikeKind::Redacted => "Message deleted".into(),
            MsgLikeKind::UnableToDecrypt(_) => "Encrypted message".into(),
            _ => "Message".into(),
        },
        _ => "Message".into(),
    }
}

async fn publish_timeline(
    open: &mut OpenRoom,
    client: &Client,
    media: &UnboundedSender<(String, String, Option<Arc<Picture>>)>,
    events: &UnboundedSender<Event>,
) {
    let room = open.id().to_owned();
    let own = client.user_id().map(ToOwned::to_owned);
    let items = open.timeline.items().await;
    let mut messages = Vec::new();
    let mut wanted = HashSet::new();
    for item in items.iter() {
        let Some(event) = item.as_event() else {
            continue;
        };
        let TimelineItemContent::MsgLike(content) = event.content() else {
            continue;
        };
        let mut message = ChatMessage {
            id: item.unique_id().0.clone(),
            item: item_key(&event.identifier()),
            author: display_name(event.sender(), event.sender_profile()),
            sender: event.sender().to_string(),
            own: event.is_own(),
            // Push rules decide what mentions this account in Matrix.
            mentions_me: !event.is_own() && event.is_highlighted(),
            ..Default::default()
        };
        match &content.kind {
            MsgLikeKind::Message(text) => {
                message.edited = text.is_edited();
                match text.msgtype() {
                    MessageType::Text(content) => {
                        message.body = content.body.clone();
                        message.formatted = content.formatted.is_some();
                    }
                    MessageType::Notice(content) => {
                        message.body = content.body.clone();
                        message.formatted = content.formatted.is_some();
                    }
                    MessageType::Emote(content) => {
                        message.body = format!("{} {}", message.author, content.body);
                    }
                    MessageType::Image(content) => {
                        message.body = content.caption().unwrap_or_default().into();
                        let size = content
                            .info
                            .as_ref()
                            .and_then(|info| info.size)
                            .map(u64::from);
                        // Prefer the sender's thumbnail; it is encrypted like the file.
                        let source = content
                            .info
                            .as_ref()
                            .and_then(|info| info.thumbnail_source.clone())
                            .unwrap_or_else(|| content.source.clone());
                        let key = media_key(&source);
                        let visible = open.visible.contains(&message.item);
                        if visible {
                            wanted.insert(key.clone());
                        }
                        let cached = visible.then(|| open.pictures.get(&key).cloned()).flatten();
                        if visible && cached.is_none() && open.requested.insert(key.clone()) {
                            let client = client.clone();
                            let media = media.clone();
                            let room = room.clone();
                            let too_large = size
                                .is_some_and(|size| size > MAX_PREVIEW_DOWNLOAD_BYTES as u64)
                                && key == media_key(&content.source);
                            open.tasks.spawn(async move {
                                let picture = if too_large {
                                    None
                                } else {
                                    match matrix_media::content(
                                        &client,
                                        &source,
                                        MAX_PREVIEW_DOWNLOAD_BYTES,
                                    )
                                    .await
                                    {
                                        Ok(data) => tokio::task::spawn_blocking(move || {
                                            decode_preview(&data)
                                        })
                                        .await
                                        .ok()
                                        .flatten()
                                        .map(Arc::new),
                                        _ => None,
                                    }
                                };
                                let _ = media.send((room, key, picture));
                            });
                        }
                        let dimensions = content.info.as_ref().and_then(|info| {
                            let side = |side: Option<UInt>| u32::try_from(u64::from(side?)).ok();
                            Some((side(info.width)?, side(info.height)?))
                        });
                        message.attachments = vec![Attachment {
                            name: content.filename().into(),
                            size,
                            image: true,
                            dimensions,
                            preview_failed: matches!(cached, Some(None)),
                            placement: Placement::Message,
                            gif: None,
                            url: None,
                            spoiler: false,
                            video: false,
                            preview: cached.flatten(),
                        }];
                    }
                    MessageType::File(content) => {
                        message.body = content.caption().unwrap_or_default().into();
                        message.attachments = vec![Attachment {
                            name: content.filename().into(),
                            size: content
                                .info
                                .as_ref()
                                .and_then(|info| info.size)
                                .map(u64::from),
                            image: false,
                            dimensions: None,
                            preview: None,
                            preview_failed: false,
                            placement: Placement::Message,
                            gif: None,
                            url: None,
                            spoiler: false,
                            video: false,
                        }];
                    }
                    MessageType::Video(content) => {
                        message.body = content.caption().unwrap_or_default().into();
                        message.attachments = vec![Attachment {
                            name: content.filename().into(),
                            size: content
                                .info
                                .as_ref()
                                .and_then(|info| info.size)
                                .map(u64::from),
                            image: false,
                            dimensions: None,
                            preview: None,
                            preview_failed: false,
                            placement: Placement::Message,
                            gif: None,
                            url: None,
                            spoiler: false,
                            video: false,
                        }];
                    }
                    MessageType::Audio(content) => {
                        message.body = content.caption().unwrap_or_default().into();
                        message.attachments = vec![Attachment {
                            name: content.filename().into(),
                            size: content
                                .info
                                .as_ref()
                                .and_then(|info| info.size)
                                .map(u64::from),
                            image: false,
                            dimensions: None,
                            preview: None,
                            preview_failed: false,
                            placement: Placement::Message,
                            gif: None,
                            url: None,
                            spoiler: false,
                            video: false,
                        }];
                    }
                    _ => message.body = text.body().to_owned(),
                }
            }
            MsgLikeKind::UnableToDecrypt(_) => {
                message.body = "Unable to decrypt. Recover your keys to read this message.".into();
            }
            MsgLikeKind::Redacted => {
                message.body = "Message deleted".into();
                message.deleted = true;
            }
            _ => continue,
        }
        if let Some(reply) = &content.in_reply_to {
            let item = Some(format!("e:{}", reply.event_id));
            message.reply = Some(match &reply.event {
                TimelineDetails::Ready(embedded) => {
                    message.replies_to_me =
                        !message.own && own.as_deref() == Some(&*embedded.sender);
                    // A reply names whom it answers in its mentions, which
                    // highlights it; it stays a mention only when its text
                    // names them too.
                    if message.replies_to_me && message.mentions_me {
                        message.mentions_me = matches!(&content.kind,
                            MsgLikeKind::Message(text) if own.as_deref().is_some_and(|own| calls_by_name(text, own)));
                    }
                    Reply {
                        author: display_name(&embedded.sender, &embedded.sender_profile),
                        avatar: None,
                        body: summary(&embedded.content),
                        item,
                        color: None,
                    }
                }
                TimelineDetails::Error(_) => Reply {
                    author: String::new(),
                    avatar: None,
                    body: "The original message could not be loaded.".into(),
                    item,
                    color: None,
                },
                _ => {
                    if open.requested.insert(reply.event_id.to_string()) {
                        let timeline = open.timeline.clone();
                        let event = reply.event_id.clone();
                        // The timeline publishes the details as an update when they arrive.
                        open.tasks.spawn(async move {
                            let _ = timeline.fetch_details_for_event(&event).await;
                        });
                    }
                    Reply {
                        author: String::new(),
                        avatar: None,
                        body: "Loading the original message...".into(),
                        item,
                        color: None,
                    }
                }
            });
        }
        message.reactions = event
            .reactions()
            .iter()
            .map(|(key, senders)| Reaction {
                key: key.clone(),
                count: senders.len(),
                mine: own.as_ref().is_some_and(|own| senders.contains_key(own)),
            })
            .collect();
        message.delivery = match event.send_state() {
            Some(EventSendState::NotSentYet { .. }) => Delivery::Sending,
            Some(EventSendState::SendingFailed {
                is_recoverable: true,
                ..
            }) => Delivery::Waiting,
            Some(EventSendState::SendingFailed { .. }) => Delivery::Failed,
            Some(EventSendState::Sent { .. }) | None => Delivery::Sent,
        };
        let readable = !message.deleted && !matches!(content.kind, MsgLikeKind::UnableToDecrypt(_));
        message.can_reply = readable && event.can_be_replied_to();
        message.can_edit = readable && event.is_editable();
        message.can_delete = !message.deleted && message.own;
        message.can_react = readable;
        message.can_forward = readable
            && matches!(content.kind, MsgLikeKind::Message(_))
            && event.event_id().is_some();
        let seconds: u64 = event.timestamp().as_secs().into();
        message.time = crate::clock::time(seconds);
        message.timestamp = seconds;
        messages.push(Arc::new(message));
    }
    // A window of older messages doesn't read the room; the newest do.
    let unread = !open.focused && open.timeline.room().num_unread_messages() > 0;
    open.pictures.retain(|key, _| wanted.contains(key));
    // Reply details use this same set; keep their event ids as well.
    open.requested
        .retain(|key| wanted.contains(key) || key.starts_with('$'));
    while open.tasks.try_join_next().is_some() {}
    let _ = events.send(Event::Timeline { room, messages });
    if unread {
        // Showing a conversation reads it; the receipt stays on this account's devices too.
        let timeline = open.timeline.clone();
        open.tasks.spawn(async move {
            let _ = timeline.mark_as_read(ReceiptType::Read).await;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::{PREVIEW_HEIGHT, PREVIEW_WIDTH};
    use matrix_sdk::ruma::{
        api::client::{
            account::register,
            room::{create_room, get_room_event},
            uiaa::{AuthData, Dummy},
        },
        events::room::message::RoomMessageEventContent,
        serde::Raw,
    };

    const HOMESERVER: &str = "http://localhost:18008";

    #[test]
    fn mentions_are_the_people_linked() {
        let mentions = mentioned(
            "[Sam](https://matrix.to/#/@sam:test) and [Riley](https://matrix.to/#/@riley:test), \
             not [a room](https://matrix.to/#/#games:test)",
        );
        let users: Vec<_> = mentions.user_ids.iter().map(|user| user.as_str()).collect();
        assert_eq!(users, ["@riley:test", "@sam:test"]);
        assert!(!mentions.room);
    }

    async fn receive<T>(
        events: &mut UnboundedReceiver<Event>,
        mut select: impl FnMut(Event) -> Option<T>,
    ) -> T {
        tokio::time::timeout(Duration::from_secs(25), async {
            while let Some(event) = events.recv().await {
                if let Some(value) = select(event) {
                    return value;
                }
            }
            panic!("chat worker stopped before the expected event");
        })
        .await
        .expect("chat event deadline exceeded")
    }

    /// Wait for a timeline publication whose messages satisfy `check`.
    async fn timeline_where(
        events: &mut UnboundedReceiver<Event>,
        mut check: impl FnMut(&[Arc<ChatMessage>]) -> bool,
    ) -> Vec<Arc<ChatMessage>> {
        receive(events, |event| match event {
            Event::Timeline { messages, .. } if check(&messages) => Some(messages),
            Event::RoomError { message, .. } => panic!("{message}"),
            _ => None,
        })
        .await
    }

    async fn register() -> String {
        let account = format!("yapfast_chat_{}", uuid::Uuid::new_v4().simple());
        let registration = Client::builder()
            .homeserver_url(HOMESERVER)
            .build()
            .await
            .unwrap();
        let mut request = register::v3::Request::new();
        request.username = Some(account.clone());
        request.password = Some("talk-test-only".into());
        request.inhibit_login = true;
        request.auth = Some(AuthData::Dummy(Dummy::new()));
        registration.send(request).await.unwrap();
        account
    }

    async fn encrypted_room(session: &Session, name: &str) -> OwnedRoomId {
        let mut request = create_room::v3::Request::new();
        request.name = Some(name.into());
        request.initial_state = vec![Raw::from_json_string(serde_json::json!({"type":"m.room.encryption","state_key":"","content":{"algorithm":"m.megolm.v1.aes-sha2"}}).to_string()).unwrap()];
        session
            .client
            .create_room(request)
            .await
            .unwrap()
            .room_id()
            .to_owned()
    }

    async fn opened(session: &Session, id: &OwnedRoomId) -> OpenRoom {
        let room = session.client.get_room(id).unwrap();
        OpenRoom {
            timeline: Arc::new(TimelineBuilder::new(&room).build().await.unwrap()),
            focused: false,
            start: false,
            pictures: HashMap::new(),
            requested: HashSet::new(),
            visible: HashSet::new(),
            tasks: JoinSet::new(),
            members: None,
        }
    }

    async fn create_space(session: &Session, name: &str, children: &[&OwnedRoomId]) -> OwnedRoomId {
        let mut request = create_room::v3::Request::new();
        request.name = Some(name.into());
        request.creation_content =
            Some(Raw::from_json_string(serde_json::json!({"type":"m.space"}).to_string()).unwrap());
        request.initial_state = children
            .iter()
            .map(|id| {
                Raw::from_json_string(
                    serde_json::json!({
                        "type":"m.space.child", "state_key":id, "content":{"via":["synapse"]}
                    })
                    .to_string(),
                )
                .unwrap()
            })
            .collect();
        session
            .client
            .create_room(request)
            .await
            .unwrap()
            .room_id()
            .to_owned()
    }

    /// Start the chat actor on a saved session and wait until it syncs.
    async fn start(
        root: &Path,
    ) -> (
        UnboundedSender<Command>,
        UnboundedReceiver<Event>,
        tokio::task::JoinHandle<()>,
    ) {
        let (commands, receiver) = tokio::sync::mpsc::unbounded_channel();
        let (sender, mut events) = tokio::sync::mpsc::unbounded_channel();
        let actor = tokio::spawn(crate::chat::run(
            root.to_owned(),
            Start::Restore,
            receiver,
            sender,
        ));
        let trusted = receive(&mut events, |event| match event {
            Event::Connected { trusted, .. } => Some(trusted),
            Event::Error(error) => panic!("{error}"),
            _ => None,
        })
        .await;
        assert!(trusted);
        (commands, events, actor)
    }

    /// A loopback TCP relay to the local homeserver that tests can take offline.
    struct Relay {
        port: u16,
        task: Option<tokio::task::JoinHandle<()>>,
    }

    impl Relay {
        async fn new() -> Self {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let port = listener.local_addr().unwrap().port();
            let mut relay = Self { port, task: None };
            relay.serve(listener);
            relay
        }

        fn url(&self) -> String {
            format!("http://127.0.0.1:{}", self.port)
        }

        fn serve(&mut self, listener: tokio::net::TcpListener) {
            self.task = Some(tokio::spawn(async move {
                let mut connections = JoinSet::new();
                loop {
                    let Ok((mut inbound, _)) = listener.accept().await else {
                        break;
                    };
                    connections.spawn(async move {
                        if let Ok(mut outbound) =
                            tokio::net::TcpStream::connect("127.0.0.1:18008").await
                        {
                            let _ =
                                tokio::io::copy_bidirectional(&mut inbound, &mut outbound).await;
                        }
                    });
                }
            }));
        }

        /// Close the listener and every open connection, as a lost network does.
        fn down(&mut self) {
            if let Some(task) = self.task.take() {
                task.abort();
            }
        }

        async fn up(&mut self) {
            for _ in 0..50 {
                if let Ok(listener) = tokio::net::TcpListener::bind(("127.0.0.1", self.port)).await
                {
                    self.serve(listener);
                    return;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            panic!("could not reopen the relay port");
        }
    }

    #[test]
    #[ignore = "requires scripts/test-stack up; creates disposable accounts"]
    fn encrypted_chat_restore_send_retry_and_security_gates() {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        crate::logging::init();
        let runtime = crate::runtime::BackgroundRuntime::new().unwrap();
        runtime.block_on(async {
            let root = tempfile::tempdir().unwrap();
            let account = register().await;
            let session = Session::login(
                root.path().join("first"),
                HOMESERVER,
                &account,
                "talk-test-only",
            )
            .await
            .unwrap();
            session.synchronize().await.unwrap();
            assert!(session.device_is_trusted().await.unwrap());
            let encrypted = encrypted_room(&session, "Chat integration").await;
            let plain = session
                .client
                .create_room(create_room::v3::Request::new())
                .await
                .unwrap()
                .room_id()
                .to_owned();
            let first_server = create_space(&session, "First server", &[&encrypted]).await;
            let second_server = create_space(&session, "Second server", &[&plain]).await;
            // Flatten joined subspaces, tolerate cycles, and list shared channels only once.
            let nested = create_space(&session, "Nested space", &[&first_server, &encrypted]).await;
            session
                .client
                .get_room(&first_server)
                .unwrap()
                .send_state_event_for_key(
                    &nested,
                    SpaceChildEventContent::new(vec![
                        matrix_sdk::ruma::server_name!("synapse").to_owned(),
                    ]),
                )
                .await
                .unwrap();
            session.synchronize().await.unwrap();
            let index = room_index(&session.client).await;
            assert_eq!(index.servers.len(), 3);
            assert!(index.rooms.iter().all(|room| {
                ![&first_server, &second_server, &nested]
                    .iter()
                    .any(|id| room.id == id.as_str())
            }));
            for server in &index.servers {
                let expected = if server.id == second_server.as_str() {
                    &plain
                } else {
                    &encrypted
                };
                assert_eq!(server.channels, vec![expected.to_string()]);
            }
            let plain_room = opened(&session, &plain).await;
            assert!(
                send(
                    &session,
                    Some(&plain_room),
                    plain.as_str(),
                    "must not send",
                    None
                )
                .await
                .is_err()
            );
            assert!(
                attach(
                    &session,
                    Some(&plain_room),
                    plain.as_str(),
                    Path::new("Cargo.toml")
                )
                .await
                .is_err()
            );
            drop(plain_room);
            let unverified = Session::login(
                root.path().join("second"),
                HOMESERVER,
                &account,
                "talk-test-only",
            )
            .await
            .unwrap();
            unverified.synchronize().await.unwrap();
            assert!(!unverified.device_is_trusted().await.unwrap());
            let unverified_room = opened(&unverified, &encrypted).await;
            assert!(
                send(
                    &unverified,
                    Some(&unverified_room),
                    encrypted.as_str(),
                    "must not send",
                    None
                )
                .await
                .is_err()
            );
            drop(unverified_room);
            let client = session.client.clone();
            let user = client.user_id().unwrap().to_string();
            // Keep no Session/Room lease alive while the actor acquires the store.
            drop(session);
            drop(client);
            let (commands, mut events, actor) = start(&root.path().join("first")).await;
            receive(&mut events, |event| match event {
                Event::Rooms(index)
                    if index.rooms.iter().any(|room| room.id == encrypted.as_str()) =>
                {
                    Some(())
                }
                _ => None,
            })
            .await;
            receive(&mut events, |event| match event {
                Event::Sync(true) => Some(()),
                _ => None,
            })
            .await;
            // A relation update from another device must refresh the live server index.
            unverified
                .client
                .get_room(&second_server)
                .unwrap()
                .send_state_event_for_key(&plain, SpaceChildEventContent::new(vec![]))
                .await
                .unwrap();
            receive(&mut events, |event| match event {
                Event::Rooms(index)
                    if index.servers.iter().any(|server| {
                        server.id == second_server.as_str() && server.channels.is_empty()
                    }) =>
                {
                    Some(())
                }
                _ => None,
            })
            .await;
            commands.send(Command::Open(encrypted.to_string())).unwrap();
            // Opening a room publishes its timeline and, separately, its members.
            let (mut opened, mut members) = (false, None);
            while !opened || members.is_none() {
                receive(&mut events, |event| match event {
                    Event::Timeline { room, .. } if room == encrypted.as_str() => {
                        opened = true;
                        Some(())
                    }
                    Event::Members {
                        room,
                        members: list,
                    } if room == encrypted.as_str() => {
                        members = Some(list);
                        Some(())
                    }
                    _ => None,
                })
                .await;
            }
            let members = members.unwrap().expect("the member list loads");
            assert_eq!(members.len(), 1);
            assert_eq!(members[0].id, user);
            assert_eq!(
                members[0].role,
                Role::Admin,
                "a room's creator is its admin"
            );
            commands
                .send(Command::Send {
                    room: encrypted.to_string(),
                    body: "encrypted chat round trip".into(),
                    reply: None,
                    options: Default::default(),
                    files: vec![],
                })
                .unwrap();
            receive(&mut events, |event| match event {
                Event::SendResult { result, .. } => {
                    assert!(result.is_ok());
                    Some(())
                }
                _ => None,
            })
            .await;
            let messages = timeline_where(&mut events, |messages| {
                messages.iter().any(|m| {
                    m.body == "encrypted chat round trip"
                        && m.delivery == Delivery::Sent
                        && m.item.starts_with("e:")
                })
            })
            .await;
            assert_eq!(
                messages
                    .iter()
                    .filter(|m| m.body == "encrypted chat round trip")
                    .count(),
                1
            );
            drop(commands);
            tokio::time::timeout(Duration::from_secs(10), actor)
                .await
                .unwrap()
                .unwrap();
            unverified.logout().await.unwrap();
            drop(unverified);
            let restored = Session::restore(root.path().join("first")).await.unwrap();
            restored.synchronize().await.unwrap();
            let room = restored.client.get_room(&encrypted).unwrap();
            let sent = room
                .send(RoomMessageEventContent::text_plain("wire encryption check"))
                .await
                .unwrap();
            let raw = restored
                .client
                .send(get_room_event::v3::Request::new(
                    encrypted.clone(),
                    sent.response.event_id.clone(),
                ))
                .await
                .unwrap();
            assert_eq!(
                raw.event.get_field::<String>("type").unwrap().as_deref(),
                Some("m.room.encrypted")
            );
            let decrypted = room.event(&sent.response.event_id, None).await.unwrap();
            assert_eq!(
                decrypted
                    .raw()
                    .get_field::<String>("type")
                    .unwrap()
                    .as_deref(),
                Some("m.room.message")
            );
            drop(room);
            restored.logout().await.unwrap();
            assert!(!root.path().join("first/active.json").exists());
        });
    }

    #[test]
    #[ignore = "requires scripts/test-stack up; creates disposable accounts"]
    fn opening_a_room_shows_messages_missed_while_it_was_closed() {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        crate::logging::init();
        let runtime = crate::runtime::BackgroundRuntime::new().unwrap();
        runtime.block_on(async {
            let root = tempfile::tempdir().unwrap();
            let data = root.path().join("alice");
            let alice = Session::login(
                data.clone(),
                HOMESERVER,
                &register().await,
                "talk-test-only",
            )
            .await
            .unwrap();
            let bob_account = register().await;
            let bob = Session::login(
                root.path().join("bob"),
                HOMESERVER,
                &bob_account,
                "talk-test-only",
            )
            .await
            .unwrap();
            alice.synchronize().await.unwrap();
            bob.synchronize().await.unwrap();
            let encrypted = encrypted_room(&alice, "Missed messages").await;
            alice
                .client
                .get_room(&encrypted)
                .unwrap()
                .invite_user_by_id(bob.client.user_id().unwrap())
                .await
                .unwrap();
            bob.client.join_room_by_id(&encrypted).await.unwrap();
            alice.synchronize().await.unwrap();
            drop(alice);
            let (commands, mut events, actor) = start(&data).await;
            receive(&mut events, |event| match event {
                Event::Sync(true) => Some(()),
                _ => None,
            })
            .await;
            commands.send(Command::Open(encrypted.to_string())).unwrap();
            timeline_where(&mut events, |_| true).await;
            commands
                .send(Command::Send {
                    room: encrypted.to_string(),
                    body: "before closing".into(),
                    reply: None,
                    options: Default::default(),
                    files: vec![],
                })
                .unwrap();
            timeline_where(&mut events, |messages| {
                messages
                    .iter()
                    .any(|m| m.body == "before closing" && m.delivery == Delivery::Sent)
            })
            .await;
            commands.send(Command::CloseRoom).unwrap();
            bob.synchronize().await.unwrap();
            let room = bob.client.get_room(&encrypted).unwrap();
            let missed: Vec<String> = (1..=8).map(|n| format!("missed {n}")).collect();
            for body in &missed {
                room.send(
                    RoomMessageEventContentWithoutRelation::text_plain(body).with_relation(None),
                )
                .await
                .unwrap();
            }
            receive(&mut events, |event| match event {
                Event::Rooms(index)
                    if index
                        .rooms
                        .iter()
                        .any(|r| r.id == encrypted.as_str() && r.unread > 0) =>
                {
                    Some(())
                }
                _ => None,
            })
            .await;
            commands.send(Command::Open(encrypted.to_string())).unwrap();
            let messages = timeline_where(&mut events, |messages| {
                messages.iter().any(|m| m.body == "missed 8")
            })
            .await;
            let bodies: Vec<&str> = messages.iter().map(|m| m.body.as_str()).collect();
            let start = bodies
                .iter()
                .position(|body| *body == "before closing")
                .expect("history before the gap");
            assert_eq!(
                &bodies[start + 1..],
                missed
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>()
                    .as_slice(),
                "{bodies:?}"
            );
            // Showing the room reads it.
            receive(&mut events, |event| match event {
                Event::Rooms(index)
                    if index
                        .rooms
                        .iter()
                        .any(|r| r.id == encrypted.as_str() && r.unread == 0) =>
                {
                    Some(())
                }
                _ => None,
            })
            .await;
            drop(commands);
            tokio::time::timeout(Duration::from_secs(10), actor)
                .await
                .unwrap()
                .unwrap();
        });
    }

    #[test]
    #[ignore = "requires scripts/test-stack up; creates disposable accounts"]
    fn jumping_shows_the_messages_around_one_until_newer_pages_reach_the_present() {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        crate::logging::init();
        let runtime = crate::runtime::BackgroundRuntime::new().unwrap();
        runtime.block_on(async {
            let root = tempfile::tempdir().unwrap();
            let data = root.path().join("alice");
            let alice = Session::login(
                data.clone(),
                HOMESERVER,
                &register().await,
                "talk-test-only",
            )
            .await
            .unwrap();
            alice.synchronize().await.unwrap();
            let encrypted = encrypted_room(&alice, "Deep history").await;
            let room = alice.client.get_room(&encrypted).unwrap();
            let mut sent = Vec::new();
            for n in 1..=80 {
                let response = room
                    .send(RoomMessageEventContent::text_plain(format!("message {n}")))
                    .await
                    .unwrap();
                sent.push(response.response.event_id);
            }
            drop(room);
            drop(alice);
            let (commands, mut events, actor) = start(&data).await;
            receive(&mut events, |event| match event {
                Event::Sync(true) => Some(()),
                _ => None,
            })
            .await;
            commands.send(Command::Open(encrypted.to_string())).unwrap();
            timeline_where(&mut events, |messages| {
                messages.iter().any(|m| m.body == "message 80")
            })
            .await;
            let has = |messages: &[Arc<ChatMessage>], body: &str| {
                messages.iter().any(|message| message.body == body)
            };

            // The messages around an early one show in place of the newest,
            // said before they come.
            let around = |n: usize| Command::Around {
                room: encrypted.to_string(),
                item: format!("e:{}", sent[n - 1]),
            };
            commands.send(around(5)).unwrap();
            let history = receive(&mut events, |event| match event {
                Event::HistoryLoaded {
                    direction: Direction::Around,
                    history,
                    ..
                } => Some(history),
                Event::Timeline { messages, .. } if has(&messages, "message 5") => {
                    panic!("the window came before it was said")
                }
                _ => None,
            })
            .await;
            assert_eq!(
                history,
                History::Loaded {
                    start: false,
                    present: false
                }
            );
            let window = timeline_where(&mut events, |messages| has(messages, "message 5")).await;
            assert!(!has(&window, "message 80"), "the newest aren't in it");

            // Newer pages load until they reach the newest, which then
            // follow live messages again.
            let mut pages = 0;
            loop {
                commands
                    .send(Command::Later(encrypted.to_string()))
                    .unwrap();
                let history = receive(&mut events, |event| match event {
                    Event::HistoryLoaded {
                        direction: Direction::Later,
                        history,
                        ..
                    } => Some(history),
                    _ => None,
                })
                .await;
                pages += 1;
                match history {
                    History::Loaded { present: true, .. } => break,
                    History::Loaded { .. } if pages < 6 => {}
                    history => panic!("{history:?} after {pages} pages"),
                }
            }
            commands
                .send(Command::Send {
                    room: encrypted.to_string(),
                    body: "after the window".into(),
                    reply: None,
                    options: Default::default(),
                    files: vec![],
                })
                .unwrap();
            // The live timeline that took the window's place still has its
            // first messages, so the rows on screen stayed.
            let live = timeline_where(&mut events, |messages| {
                has(messages, "after the window") && has(messages, "message 80")
            })
            .await;
            assert!(has(&window, "message 3"));
            assert!(has(&live, "message 3"), "the window's first messages stay");

            // Back to the present from a window, the newest show again.
            commands.send(around(10)).unwrap();
            timeline_where(&mut events, |messages| {
                has(messages, "message 10") && !has(messages, "message 80")
            })
            .await;
            commands
                .send(Command::Present(encrypted.to_string()))
                .unwrap();
            timeline_where(&mut events, |messages| has(messages, "after the window")).await;

            // One that isn't in the room says so, and what showed stays.
            commands
                .send(Command::Around {
                    room: encrypted.to_string(),
                    item: "e:$missing:localhost".into(),
                })
                .unwrap();
            receive(&mut events, |event| match event {
                Event::RoomError { message, .. } => {
                    assert!(message.contains("couldn't find"), "{message}");
                    Some(())
                }
                _ => None,
            })
            .await;
            drop(commands);
            tokio::time::timeout(Duration::from_secs(10), actor)
                .await
                .unwrap()
                .unwrap();
        });
    }

    /// Opening a Discord channel closes the Matrix room that was open, so
    /// that room's new messages stay unread. Run it with the fake Discord:
    /// `bun testing/fake_discord.ts --port 18400` and
    /// `YAPFAST_DISCORD_TEST_SERVER=http://127.0.0.1:18400`.
    #[cfg(feature = "discord")]
    #[test]
    #[ignore = "requires scripts/test-stack up and testing/fake_discord.ts; creates disposable accounts"]
    fn opening_a_discord_channel_leaves_the_matrix_room_unread() {
        assert!(
            yapfast_discord::client::test_server().is_some(),
            "set YAPFAST_DISCORD_TEST_SERVER to the fake Discord"
        );
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        crate::logging::init();
        let runtime = crate::runtime::BackgroundRuntime::new().unwrap();
        runtime.block_on(async {
            let root = tempfile::tempdir().unwrap();
            let data = root.path().join("alice");
            let alice = Session::login(
                data.clone(),
                HOMESERVER,
                &register().await,
                "talk-test-only",
            )
            .await
            .unwrap();
            let bob = Session::login(
                root.path().join("bob"),
                HOMESERVER,
                &register().await,
                "talk-test-only",
            )
            .await
            .unwrap();
            alice.synchronize().await.unwrap();
            bob.synchronize().await.unwrap();
            let encrypted = encrypted_room(&alice, "Left for Discord").await;
            alice
                .client
                .get_room(&encrypted)
                .unwrap()
                .invite_user_by_id(bob.client.user_id().unwrap())
                .await
                .unwrap();
            bob.client.join_room_by_id(&encrypted).await.unwrap();
            alice.synchronize().await.unwrap();
            drop(alice);
            let (commands, mut events, actor) = start(&data).await;
            receive(&mut events, |event| match event {
                Event::Sync(true) => Some(()),
                _ => None,
            })
            .await;
            commands.send(Command::Open(encrypted.to_string())).unwrap();
            timeline_where(&mut events, |_| true).await;
            receive(&mut events, |event| match event {
                Event::Discord(snapshot)
                    if snapshot.status == yapfast_discord::service::Status::Online =>
                {
                    Some(())
                }
                _ => None,
            })
            .await;
            // The fake's #general.
            let general = crate::discord::room_id("1400000000000000100", "1400000000000000102");
            commands.send(Command::Open(general.clone())).unwrap();
            receive(&mut events, |event| match event {
                Event::Timeline { room, .. } if room == general => Some(()),
                Event::RoomError { message, .. } => panic!("{message}"),
                _ => None,
            })
            .await;

            bob.synchronize().await.unwrap();
            bob.client
                .get_room(&encrypted)
                .unwrap()
                .send(
                    RoomMessageEventContentWithoutRelation::text_plain("while on Discord")
                        .with_relation(None),
                )
                .await
                .unwrap();
            let unread = |index: &RoomIndex| {
                index
                    .rooms
                    .iter()
                    .find(|room| room.id == encrypted.as_str())
                    .map(|room| room.unread)
            };
            // A room still open would show the message, then read it.
            let shown = "the closed Matrix room showed a new message";
            receive(&mut events, |event| match event {
                Event::Rooms(index) if unread(&index) > Some(0) => Some(()),
                Event::Timeline { room, .. } if room == encrypted.as_str() => panic!("{shown}"),
                _ => None,
            })
            .await;
            let settle = tokio::time::sleep(Duration::from_secs(3));
            tokio::pin!(settle);
            loop {
                tokio::select! {
                    () = &mut settle => break,
                    event = events.recv() => match event.expect("the chat actor stopped") {
                        Event::Timeline { room, .. } if room == encrypted.as_str() => {
                            panic!("{shown}")
                        }
                        Event::Rooms(index) if unread(&index) == Some(0) => {
                            panic!("the closed Matrix room was marked read")
                        }
                        _ => {}
                    },
                }
            }
            drop(commands);
            tokio::time::timeout(Duration::from_secs(10), actor)
                .await
                .unwrap()
                .unwrap();
        });
    }

    #[test]
    #[ignore = "requires scripts/test-stack up; creates disposable accounts"]
    fn markdown_replies_reactions_edits_attachments_and_drafts() {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        crate::logging::init();
        let runtime = crate::runtime::BackgroundRuntime::new().unwrap();
        runtime.block_on(async {
            let root = tempfile::tempdir().unwrap();
            let data = root.path().join("alice");
            let account = register().await;
            let session = Session::login(data.clone(), HOMESERVER, &account, "talk-test-only")
                .await
                .unwrap();
            session.synchronize().await.unwrap();
            let encrypted = encrypted_room(&session, "Conversation features").await;
            let other = encrypted_room(&session, "Forward target").await;
            session.synchronize().await.unwrap();
            drop(session);
            let room = encrypted.to_string();
            let (commands, mut events, actor) = start(&data).await;
            receive(&mut events, |event| match event {
                Event::Sync(true) => Some(()),
                _ => None,
            })
            .await;
            commands.send(Command::Open(room.clone())).unwrap();
            timeline_where(&mut events, |_| true).await;

            commands
                .send(Command::Send {
                    room: room.clone(),
                    body: "**bold** and `code`".into(),
                    reply: None,
                    options: Default::default(),
                    files: vec![],
                })
                .unwrap();
            let messages = timeline_where(&mut events, |messages| {
                messages.iter().any(|m| {
                    m.formatted && m.delivery == Delivery::Sent && m.item.starts_with("e:")
                })
            })
            .await;
            let original = messages.iter().find(|m| m.formatted).unwrap().clone();
            assert_eq!(original.body, "**bold** and `code`");
            assert!(
                original.can_reply
                    && original.can_edit
                    && original.can_delete
                    && original.can_react
            );

            commands
                .send(Command::Send {
                    room: room.clone(),
                    body: "plain reply".into(),
                    reply: Some(original.item.clone()),
                    options: Default::default(),
                    files: vec![],
                })
                .unwrap();
            let messages = timeline_where(&mut events, |messages| {
                messages.iter().any(|m| {
                    m.body == "plain reply"
                        && m.reply
                            .as_ref()
                            .is_some_and(|reply| reply.body.contains("bold"))
                })
            })
            .await;
            assert!(
                !messages
                    .iter()
                    .find(|m| m.body == "plain reply")
                    .unwrap()
                    .formatted
            );

            commands
                .send(Command::React {
                    room: room.clone(),
                    item: original.item.clone(),
                    key: "👍".into(),
                })
                .unwrap();
            timeline_where(&mut events, |messages| {
                messages.iter().any(|m| {
                    m.item == original.item
                        && m.reactions
                            == vec![Reaction {
                                key: "👍".into(),
                                count: 1,
                                mine: true,
                            }]
                })
            })
            .await;
            commands
                .send(Command::React {
                    room: room.clone(),
                    item: original.item.clone(),
                    key: "👍".into(),
                })
                .unwrap();
            timeline_where(&mut events, |messages| {
                messages
                    .iter()
                    .any(|m| m.item == original.item && m.reactions.is_empty())
            })
            .await;

            commands
                .send(Command::Edit {
                    room: room.clone(),
                    item: original.item.clone(),
                    body: "edited *text*".into(),
                })
                .unwrap();
            receive(&mut events, |event| match event {
                Event::EditResult { result, .. } => {
                    assert!(result.is_ok());
                    Some(())
                }
                _ => None,
            })
            .await;
            timeline_where(&mut events, |messages| {
                messages
                    .iter()
                    .any(|m| m.item == original.item && m.edited && m.body == "edited *text*")
            })
            .await;

            commands
                .send(Command::Delete {
                    room: room.clone(),
                    item: original.item.clone(),
                })
                .unwrap();
            timeline_where(&mut events, |messages| {
                messages
                    .iter()
                    .any(|m| m.item == original.item && m.deleted && !m.can_delete)
            })
            .await;

            // An encrypted image round trip: queued upload, decrypted preview, saved file.
            let picture = root.path().join("square.png");
            image::RgbaImage::from_pixel(640, 480, image::Rgba([200, 40, 40, 255]))
                .save(&picture)
                .unwrap();
            commands
                .send(Command::Send {
                    room: room.clone(),
                    body: String::new(),
                    reply: None,
                    options: Default::default(),
                    files: vec![picture.clone().into()],
                })
                .unwrap();
            receive(&mut events, |event| match event {
                Event::SendResult { result, .. } => {
                    assert!(result.is_ok());
                    Some(())
                }
                _ => None,
            })
            .await;
            let metadata = timeline_where(&mut events, |messages| {
                messages.iter().any(|m| {
                    m.delivery == Delivery::Sent
                        && m.item.starts_with("e:")
                        && m.attachments.first().is_some_and(|a| a.image)
                })
            })
            .await;
            assert!(
                metadata
                    .iter()
                    .flat_map(|m| &m.attachments)
                    .all(|a| a.preview.is_none()),
                "off-screen Matrix images stay as metadata"
            );
            commands
                .send(Command::VisibleMessages {
                    room: room.clone(),
                    items: metadata.iter().map(|m| m.item.clone()).collect(),
                    at_end: false,
                })
                .unwrap();
            let messages = timeline_where(&mut events, |messages| {
                messages.iter().any(|m| {
                    m.delivery == Delivery::Sent
                        && m.item.starts_with("e:")
                        && m.attachments.first().is_some_and(|a| a.preview.is_some())
                })
            })
            .await;
            let sent = messages.iter().find(|m| !m.attachments.is_empty()).unwrap();
            let reply = messages
                .iter()
                .find(|m| m.body == "plain reply")
                .unwrap()
                .item
                .clone();
            assert!(reply.starts_with("e:"));
            let attachment = sent.attachments.first().unwrap();
            let preview = attachment.preview.as_ref().unwrap();
            assert!(attachment.image && attachment.name == "square.png");
            assert!(preview.width <= PREVIEW_WIDTH && preview.height <= PREVIEW_HEIGHT);
            assert_eq!(&preview.rgba[..4], &[200, 40, 40, 255]);
            let saved = root.path().join("saved.png");
            commands
                .send(Command::SaveAttachment {
                    room: room.clone(),
                    item: sent.item.clone(),
                    index: 0,
                    path: saved.clone(),
                })
                .unwrap();
            receive(&mut events, |event| match event {
                Event::Saved { result, .. } => {
                    assert!(result.is_ok());
                    Some(())
                }
                _ => None,
            })
            .await;
            assert_eq!(
                std::fs::read(&saved).unwrap(),
                std::fs::read(&picture).unwrap()
            );
            // Forwarding sends the reply's text, without its quote, and the
            // encrypted image again, with a message added to it, into another
            // room, where both decrypt. The deleted message can't be forwarded.
            commands
                .send(Command::Forward {
                    room: room.clone(),
                    item: original.item.clone(),
                    target: other.to_string(),
                    note: Some("not sent".into()),
                    silent: false,
                })
                .unwrap();
            receive(&mut events, |event| match event {
                Event::Forwarded { result, .. } => {
                    assert_eq!(
                        result,
                        Err(ForwardFailure::Forward(
                            "This message can't be forwarded.".into()
                        ))
                    );
                    Some(())
                }
                _ => None,
            })
            .await;
            for item in [&reply, &sent.item] {
                commands
                    .send(Command::Forward {
                        room: room.clone(),
                        item: item.clone(),
                        target: other.to_string(),
                        note: (item == &sent.item).then(|| "have a look".into()),
                        silent: false,
                    })
                    .unwrap();
                receive(&mut events, |event| match event {
                    Event::Forwarded { room, result } if room == other.as_str() => {
                        assert_eq!(result, Ok(()));
                        Some(())
                    }
                    _ => None,
                })
                .await;
            }
            commands
                .send(Command::VisibleMessages {
                    room: room.clone(),
                    items: vec![],
                    at_end: false,
                })
                .unwrap();
            timeline_where(&mut events, |messages| {
                messages.iter().any(|m| m.item == sent.item)
                    && messages
                        .iter()
                        .flat_map(|m| &m.attachments)
                        .all(|a| a.preview.is_none())
            })
            .await;
            commands.send(Command::Open(other.to_string())).unwrap();
            let metadata = timeline_where(&mut events, |messages| {
                messages
                    .iter()
                    .any(|m| m.body == "plain reply" && m.delivery == Delivery::Sent)
                    && messages
                        .iter()
                        .any(|m| m.body == "have a look" && m.delivery == Delivery::Sent)
                    && messages.iter().any(|m| {
                        m.delivery == Delivery::Sent
                            && m.item.starts_with("e:")
                            && m.attachments.first().is_some_and(|a| a.image)
                    })
            })
            .await;
            commands
                .send(Command::VisibleMessages {
                    room: other.to_string(),
                    items: metadata.iter().map(|m| m.item.clone()).collect(),
                    at_end: false,
                })
                .unwrap();
            timeline_where(&mut events, |messages| {
                messages.iter().any(|m| {
                    m.body == "plain reply" && m.reply.is_none() && m.delivery == Delivery::Sent
                }) && messages.iter().any(|m| {
                    m.delivery == Delivery::Sent
                        && m.attachments.first().is_some_and(|a| {
                            a.preview
                                .as_ref()
                                .is_some_and(|p| p.rgba[..4] == [200, 40, 40, 255])
                        })
                })
            })
            .await;
            commands.send(Command::Open(room.clone())).unwrap();
            timeline_where(&mut events, |messages| {
                messages.iter().any(|m| m.item == sent.item)
            })
            .await;
            let file = root.path().join("notes.txt");
            std::fs::write(&file, "attachment text").unwrap();
            commands
                .send(Command::Send {
                    room: room.clone(),
                    body: String::new(),
                    reply: None,
                    options: Default::default(),
                    files: vec![file.into()],
                })
                .unwrap();
            timeline_where(&mut events, |messages| {
                messages.iter().any(|m| {
                    m.delivery == Delivery::Sent
                        && m.attachments.first().is_some_and(|a| {
                            !a.image && a.name == "notes.txt" && a.size == Some(15)
                        })
                })
            })
            .await;

            commands
                .send(Command::SaveDraft {
                    room: room.clone(),
                    body: "unfinished thought".into(),
                })
                .unwrap();
            commands.send(Command::CloseRoom).unwrap();
            drop(commands);
            tokio::time::timeout(Duration::from_secs(10), actor)
                .await
                .unwrap()
                .unwrap();

            let (commands, mut events, actor) = start(&data).await;
            commands.send(Command::Open(room.clone())).unwrap();
            let draft = receive(&mut events, |event| match event {
                Event::Draft { body, .. } => Some(body),
                Event::Timeline { .. } => panic!("the draft must arrive before the timeline"),
                _ => None,
            })
            .await;
            assert_eq!(draft, "unfinished thought");
            commands
                .send(Command::SaveDraft {
                    room: room.clone(),
                    body: String::new(),
                })
                .unwrap();
            commands
                .send(Command::Send {
                    room: room.clone(),
                    body: "after the draft".into(),
                    reply: None,
                    options: Default::default(),
                    files: vec![],
                })
                .unwrap();
            receive(&mut events, |event| match event {
                Event::SendResult { result, .. } => {
                    assert!(result.is_ok());
                    Some(())
                }
                _ => None,
            })
            .await;

            // The SDK stores the image encrypted: the server only sees an encrypted event and file.
            // The actor holds the session lock until it stops.
            drop(commands);
            tokio::time::timeout(Duration::from_secs(10), actor)
                .await
                .unwrap()
                .unwrap();
            let restored = Session::restore(data.clone()).await.unwrap();
            restored.synchronize().await.unwrap();
            let id = OwnedEventId::try_from(sent.item.strip_prefix("e:").unwrap()).unwrap();
            let raw = restored
                .client
                .send(get_room_event::v3::Request::new(
                    encrypted.clone(),
                    id.clone(),
                ))
                .await
                .unwrap();
            assert_eq!(
                raw.event.get_field::<String>("type").unwrap().as_deref(),
                Some("m.room.encrypted")
            );
            let event = restored
                .client
                .get_room(&encrypted)
                .unwrap()
                .event(&id, None)
                .await
                .unwrap();
            let content = event
                .raw()
                .get_field::<serde_json::Value>("content")
                .unwrap()
                .unwrap();
            assert!(
                content.get("file").is_some() && content.get("url").is_none(),
                "{content}"
            );
            assert!(
                restored
                    .client
                    .get_room(&encrypted)
                    .unwrap()
                    .load_composer_draft(None)
                    .await
                    .unwrap()
                    .is_none()
            );
        });
    }

    #[test]
    #[ignore = "requires scripts/test-stack up; creates disposable accounts"]
    fn offline_messages_wait_survive_restart_and_send_once() {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        crate::logging::init();
        let runtime = crate::runtime::BackgroundRuntime::new().unwrap();
        runtime.block_on(async {
            let root = tempfile::tempdir().unwrap();
            let data = root.path().join("alice");
            let mut relay = Relay::new().await;
            let account = register().await;
            let session = Session::login(data.clone(), &relay.url(), &account, "talk-test-only")
                .await
                .unwrap();
            session.synchronize().await.unwrap();
            let encrypted = encrypted_room(&session, "Offline queue").await;
            session.synchronize().await.unwrap();
            drop(session);
            let room = encrypted.to_string();
            let (commands, mut events, actor) = start(&data).await;
            receive(&mut events, |event| match event {
                Event::Sync(true) => Some(()),
                _ => None,
            })
            .await;
            commands.send(Command::Open(room.clone())).unwrap();
            timeline_where(&mut events, |_| true).await;

            relay.down();
            receive(&mut events, |event| match event {
                Event::Sync(false) => Some(()),
                _ => None,
            })
            .await;
            commands
                .send(Command::Send {
                    room: room.clone(),
                    body: "written offline".into(),
                    reply: None,
                    options: Default::default(),
                    files: vec![],
                })
                .unwrap();
            receive(&mut events, |event| match event {
                Event::SendResult { result, .. } => {
                    assert!(result.is_ok());
                    Some(())
                }
                _ => None,
            })
            .await;
            let messages = timeline_where(&mut events, |messages| {
                messages
                    .iter()
                    .any(|m| m.body == "written offline" && m.delivery == Delivery::Waiting)
            })
            .await;
            assert!(
                messages
                    .iter()
                    .find(|m| m.body == "written offline")
                    .unwrap()
                    .item
                    .starts_with("t:")
            );
            commands
                .send(Command::Send {
                    room: room.clone(),
                    body: "cancel me".into(),
                    reply: None,
                    options: Default::default(),
                    files: vec![],
                })
                .unwrap();
            let messages = timeline_where(&mut events, |messages| {
                messages.iter().any(|m| m.body == "cancel me")
            })
            .await;
            let cancelled = messages
                .iter()
                .find(|m| m.body == "cancel me")
                .unwrap()
                .item
                .clone();
            commands
                .send(Command::Cancel {
                    room: room.clone(),
                    item: cancelled,
                })
                .unwrap();
            timeline_where(&mut events, |messages| {
                !messages.iter().any(|m| m.body == "cancel me")
            })
            .await;

            // Close Yapfast while offline; the next run sends from the encrypted store.
            commands.send(Command::CloseRoom).unwrap();
            drop(commands);
            tokio::time::timeout(Duration::from_secs(10), actor)
                .await
                .unwrap()
                .unwrap();
            let (commands, mut events, actor) = start(&data).await;
            commands.send(Command::Open(room.clone())).unwrap();
            timeline_where(&mut events, |messages| {
                messages
                    .iter()
                    .any(|m| m.body == "written offline" && m.delivery != Delivery::Sent)
            })
            .await;
            relay.up().await;
            let restored = std::time::Instant::now();
            timeline_where(&mut events, |messages| {
                messages.iter().any(|m| {
                    m.body == "written offline"
                        && m.delivery == Delivery::Sent
                        && m.item.starts_with("e:")
                })
            })
            .await;
            println!(
                "queued message sent {:.1}s after the network returned",
                restored.elapsed().as_secs_f64()
            );
            drop(commands);
            tokio::time::timeout(Duration::from_secs(10), actor)
                .await
                .unwrap()
                .unwrap();

            // Only one copy reached the server, and the cancelled message never did.
            let session = Session::restore(data.clone()).await.unwrap();
            session.synchronize().await.unwrap();
            let open = opened(&session, &encrypted).await;
            let _ = open.timeline.paginate_backwards(50).await;
            let bodies: Vec<_> = open
                .timeline
                .items()
                .await
                .iter()
                .filter_map(|item| item.as_event())
                .filter(|event| event.is_remote_event())
                .filter_map(|event| {
                    event
                        .content()
                        .as_message()
                        .map(|message| message.body().to_owned())
                })
                .collect();
            assert_eq!(
                bodies
                    .iter()
                    .filter(|body| *body == "written offline")
                    .count(),
                1,
                "{bodies:?}"
            );
            assert!(!bodies.iter().any(|body| body == "cancel me"), "{bodies:?}");
        });
    }

    /// Post sample conversation content as the voice demo's Bob, for UI checks.
    #[test]
    #[ignore = "UI fixture helper; needs YAPFAST_TEST_VOICE_ROOT from --voice-demo"]
    fn fixture_conversation() {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        let root = PathBuf::from(std::env::var("YAPFAST_TEST_VOICE_ROOT").expect("fixture root"));
        let manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(root.join("rooms.json")).unwrap()).unwrap();
        let room = manifest["text"].as_str().unwrap().to_owned();
        let runtime = crate::runtime::BackgroundRuntime::new().unwrap();
        runtime.block_on(async {
            let (commands, mut events, actor) = start(&root.join("bob")).await;
            receive(&mut events, |event| match event {
                Event::Sync(true) => Some(()),
                _ => None,
            })
            .await;
            commands.send(Command::Open(room.clone())).unwrap();
            timeline_where(&mut events, |_| true).await;
            commands
                .send(Command::Send {
                    room: room.clone(),
                    body: "Welcome to **Yapfast**, with `code` and a [link](https://matrix.org)"
                        .into(),
                    reply: None,
                    options: Default::default(),
                    files: vec![],
                })
                .unwrap();
            let messages = timeline_where(&mut events, |messages| {
                messages
                    .iter()
                    .any(|m| m.formatted && m.item.starts_with("e:"))
            })
            .await;
            let welcome = messages.iter().find(|m| m.formatted).unwrap().item.clone();
            let picture = root.join("sunset.png");
            image::RgbaImage::from_fn(800, 500, |x, y| {
                image::Rgba([(x / 4) as u8, (y / 3) as u8, 160, 255])
            })
            .save(&picture)
            .unwrap();
            commands
                .send(Command::Send {
                    room: room.clone(),
                    body: String::new(),
                    reply: None,
                    options: Default::default(),
                    files: vec![picture.into()],
                })
                .unwrap();
            timeline_where(&mut events, |messages| {
                messages.iter().any(|m| {
                    m.delivery == Delivery::Sent && m.attachments.first().is_some_and(|a| a.image)
                })
            })
            .await;
            let notes = root.join("trip-notes.txt");
            std::fs::write(&notes, "Bring snacks.").unwrap();
            commands
                .send(Command::Send {
                    room: room.clone(),
                    body: String::new(),
                    reply: None,
                    options: Default::default(),
                    files: vec![notes.into()],
                })
                .unwrap();
            timeline_where(&mut events, |messages| {
                messages.iter().any(|m| {
                    m.delivery == Delivery::Sent && m.attachments.first().is_some_and(|a| !a.image)
                })
            })
            .await;
            commands
                .send(Command::Send {
                    room: room.clone(),
                    body: "The welcome has the plan".into(),
                    reply: Some(welcome),
                    options: Default::default(),
                    files: vec![],
                })
                .unwrap();
            timeline_where(&mut events, |messages| {
                messages
                    .iter()
                    .any(|m| m.reply.is_some() && m.delivery == Delivery::Sent)
            })
            .await;
            // YAPFAST_TEST_EXTRA_MESSAGES adds a long history, for scrolling tests.
            let extra = std::env::var("YAPFAST_TEST_EXTRA_MESSAGES")
                .ok()
                .and_then(|count| count.parse::<usize>().ok())
                .unwrap_or(0);
            for index in 0..extra {
                let body = match index % 4 {
                    0 => format!("Message {index}: short one"),
                    1 => format!(
                        "Message {index}: a longer line about the plan for the weekend, so it \
                         wraps across the timeline at ordinary window widths"
                    ),
                    2 => format!("Message {index} with **bold**, `code` and a list:\n- one\n- two"),
                    _ => format!("Message {index}: 👍 sounds good"),
                };
                commands
                    .send(Command::Send {
                        room: room.clone(),
                        body,
                        reply: None,
                        options: Default::default(),
                        files: vec![],
                    })
                    .unwrap();
                // Each wait has its own deadline, so wait for every twentieth message.
                if index % 20 == 19 {
                    let prefixes = [format!("Message {index}:"), format!("Message {index} ")];
                    timeline_where(&mut events, |messages| {
                        messages.iter().any(|m| {
                            m.delivery == Delivery::Sent
                                && prefixes.iter().any(|prefix| m.body.starts_with(prefix))
                        })
                    })
                    .await;
                }
            }
            commands
                .send(Command::Send {
                    room: room.clone(),
                    body: "Last message from **Bob** with `code`".into(),
                    reply: None,
                    options: Default::default(),
                    files: vec![],
                })
                .unwrap();
            timeline_where(&mut events, |messages| {
                messages
                    .iter()
                    .any(|m| m.body.starts_with("Last message") && m.delivery == Delivery::Sent)
            })
            .await;
            drop(commands);
            tokio::time::timeout(Duration::from_secs(10), actor)
                .await
                .unwrap()
                .unwrap();
        });
    }
}
