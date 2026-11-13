//! One persistent voice connection, owned by a native thread rather than the UI.
//! Matrix calls live in `matrix_voice` and Discord calls in `discord_voice`;
//! this module runs whichever one is joined, and lists the devices both use.
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    thread,
};

pub use yapfast_media::device::{Device, NAMED, device_id};
/// How large a shared screen is sent; see the media crate's geometry.
pub use yapfast_media::geometry::Resolution;

use anyhow::{Context, Result};
use livekit::PlatformAudio;
use tokio::sync::{mpsc, watch};
#[cfg(feature = "discord")]
use yapfast_discord::service::Discord;

#[cfg(feature = "matrix")]
use crate::session::SessionLease;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Phase {
    #[default]
    Disconnected,
    Joining,
    Connected,
    Reconnecting,
    Leaving,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Participant {
    pub id: String,
    pub user: String,
    /// Who they are across calls, by Discord user id or Matrix user id, as
    /// [`crate::volume`] remembers them.
    pub person: String,
    pub local: bool,
    pub muted: bool,
    /// Not hearing the call. MatrixRTC doesn't share this, so in Yapfast calls
    /// only this account's own shows.
    pub deafened: bool,
    /// Muted or deafened by a Discord server, which shows in red; `muted`
    /// and `deafened` say so too.
    pub server_muted: bool,
    pub server_deafened: bool,
    pub speaking: bool,
    /// False after this participant's media connection disappeared, for
    /// example when their client crashed. Their membership stays listed until
    /// it expires. A participant who never unmuted shows no media connection.
    pub connected: bool,
    /// Why audio may not flow with this participant, if Yapfast can tell.
    pub problem: Option<String>,
    /// Sharing their screen.
    pub streaming: bool,
    /// Their camera, while it's on and the call can show it.
    pub video: Option<crate::picture::Feed>,
    /// The address of their avatar; see [`crate::chat::DiscordCommand::Pictures`].
    pub avatar: Option<String>,
}

/// Sharing this account's screen in the call.
#[derive(Clone, Debug)]
pub struct Sharing {
    pub phase: SharePhase,
    /// What's being shared, as the UI previews it.
    pub frames: Arc<crate::frames::Frames>,
    /// Who watches it, by user id, as Discord lists them.
    pub viewers: Vec<String>,
}

/// Watching someone's stream in the call.
#[derive(Clone, Debug)]
pub struct Watching {
    /// Their user id, as in [`Participant::id`].
    pub user: String,
    pub name: String,
    /// Until the first picture shows.
    pub opening: bool,
    /// Their stream's pictures, once the worker has started.
    pub frames: Option<Arc<crate::frames::Frames>>,
    /// Who watches it, this account included, by user id, as Discord lists
    /// them.
    pub viewers: Vec<String>,
}

/// How a screen is shared: chosen before going live, and remembered.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct StreamQuality {
    pub resolution: Resolution,
    /// Frames a second: 15, 30 or 60.
    pub fps: u32,
    /// A sharper picture from more of the encoder's work, at about twice
    /// the CPU, for the encoder on the processor: a GPU's has one tuning, which
    /// the measurements leave it.
    #[serde(default)]
    pub high_quality: bool,
    /// What the share may take from the uplink, which is what its ceiling is
    /// declared as.
    #[serde(default)]
    pub ceiling: Ceiling,
}

impl Default for StreamQuality {
    fn default() -> Self {
        Self {
            resolution: Resolution::P1080,
            fps: 30,
            high_quality: false,
            ceiling: Ceiling::default(),
        }
    }
}

/// What a share may take from the uplink: bits per pixel a frame, which is what
/// its ceiling is declared as. Measured at 3440x1440 and 60 fps
/// (`docs/measurements/README.md`): 0.03 is transparent on a desktop at 57.2 dB
/// of PSNR-Y and 40.4 dB on motion, 0.07 is what the picture is worth (59.0 and
/// 43.5), and 0.12 is 48.2 dB on motion for the uplink to carry.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Ceiling {
    /// 0.03 bits a pixel a frame: for a slow or busy uplink.
    Lean,
    /// 0.07, what the picture is worth, and the share's unless it asks
    /// otherwise.
    #[default]
    Balanced,
    /// 0.12: for motion, at the dearest to the uplink.
    Sharp,
}

impl Ceiling {
    /// The menu's choices, in the order it lists them.
    pub const ALL: [Self; 3] = [Self::Lean, Self::Balanced, Self::Sharp];

    /// Bits per pixel a frame a share of this ceiling declares.
    pub const fn bits_per_pixel(self) -> f64 {
        match self {
            Self::Lean => 0.03,
            Self::Balanced => 0.07,
            Self::Sharp => 0.12,
        }
    }

    /// How the choice is written, in the log and the menu.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Lean => "lean",
            Self::Balanced => "balanced",
            Self::Sharp => "sharp",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SharePhase {
    /// The system's dialog is asking what to share.
    Choosing,
    /// Captured, while Discord starts the stream and the video connects.
    Starting,
    /// Others in the call can watch.
    Live,
}

/// How the call's connection is doing, as Discord's client shows it: the
/// pings over the last few minutes, oldest first, in milliseconds; the
/// percentage of this account's packets lost on the way; and the voice
/// privacy code, 30 digits, while the call is end-to-end encrypted.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LinkStats {
    pub pings: Vec<u16>,
    pub loss: Option<f32>,
    pub privacy_code: Option<String>,
    /// The screen this account shares, while it's live.
    pub stream: Option<StreamStats>,
    /// The stream this account watches, whose worker says how it is decoded
    /// and shown: the half of a watch no packet shows, and what to look at
    /// when a stream arrives wrong.
    pub watching: Option<StreamStats>,
}

/// How this account's shared screen goes out, as its stream worker reports
/// it each second: the encoder ("h264_nvenc", "x264"), the codec Discord
/// chose ("H264"), the size and frame rate sent, the kbit/s aimed for and
/// sent in the last second, and the keyframes viewers asked for and the
/// packets resent to them since the stream started. What a bound had to
/// give up is here too, so a stream that stutters says why: the frames a
/// full send queue skipped, and the shared sound a backlog dropped. And so
/// are the optional settings the encoder's driver dropped when it opened:
/// they say why the picture may differ from the tuning the stream asked for.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct StreamStats {
    pub engine: String,
    pub codec: String,
    /// What is captured, as the way that captured it and the size of it:
    /// "Windows Graphics Capture · 3440×1440 screen", or "the processor's
    /// capturer" for a screen, window or camera taken in memory. It is the
    /// half of a share no packet shows — the picture looks the same whichever
    /// way made it — so a share that looks wrong says which way it came.
    pub capture: String,
    /// The optional settings the encoder's driver didn't keep when it
    /// opened, as it named them ("preset,tune"); empty when it kept them
    /// all, and then the Stream tab has no row to show for them. A handful
    /// of names, read once at the open rather than grown per frame.
    pub settings_dropped: String,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub target_kbps: u32,
    pub sent_kbps: u32,
    pub keyframe_requests: u64,
    pub resent: u64,
    /// Frames the send queue was too full to take, since the stream started:
    /// the encode that would have been thrown away was skipped instead.
    pub skipped: u64,
    /// The most any application's shared sound waited to be mixed, in
    /// milliseconds: delay before viewers hear it. Unknown while no sound is
    /// shared with the screen, or while the sound couldn't be captured.
    pub sound_held_ms: Option<u32>,
    /// Shared sound a full backlog dropped since the stream started, in
    /// milliseconds. Unknown where `sound_held_ms` is.
    pub sound_dropped_ms: Option<u64>,
    /// What decodes the stream, as it names itself, when this account is
    /// watching one: "h265 (d3d11va)" is the card's decoder, "libvpx" the
    /// processor's. Unknown while nothing decodes.
    pub decoder: Option<String>,
    /// How the pictures reach the window, which is the difference between a
    /// copy of every frame through the processor and none.
    pub presentation: Option<Presentation>,
    /// The adapter the capture ran on, the one the encoder ran on and the one
    /// the window draws on, as their DXGI LUIDs: a picture is shared only
    /// between the last two when they are the same, so a share that copies
    /// every frame says here whether that is why. The capture's and the
    /// encoder's are one device on the card's paths, and are known once a
    /// picture has been shared with the window — the share's own register names
    /// it. Unknown where nothing has, and on the processor's path, which asks
    /// the card nothing.
    pub capture_luid: Option<u64>,
    pub encode_luid: Option<u64>,
    pub present_luid: Option<u64>,
    /// Why the stream isn't on the path it asked for, in one line from the
    /// code that fell back: "sharing from the processor" is the card's
    /// capture refusing, and an empty line is a stream with nothing to
    /// explain.
    pub fallback: Option<String>,
    /// Whether a preview is being made for the UI's tile now, which is the
    /// work a hidden window's share doesn't do.
    pub preview: bool,
    /// How long a frame waited between the capture having it and the encoder
    /// being given it, in milliseconds: the delay this side adds before the
    /// network's own. Unknown while nothing is sent.
    pub queue_delay_ms: Option<u32>,
    /// The bytes the encoder's output buffers hold now, and the most they
    /// have held: what a pool that grew for one large frame kept.
    pub pool_live: Option<u64>,
    pub pool_peak: Option<u64>,
    /// Whether a stream is giving memory back because the machine is short,
    /// and which signal said so. `None` is a stream with the room to work.
    pub memory: Option<String>,
    /// Where a watched stream's sound is, once the device has said: the
    /// milliseconds it has played since the player started, and the ones it
    /// still holds queued. Unknown while nothing plays or no device can say.
    pub sound_heard_ms: Option<u32>,
    pub sound_queued_ms: Option<u32>,
    /// How far a watched stream's picture runs from its sound on the
    /// streamer's own timeline, in milliseconds, positive for the picture
    /// ahead: `arrival` compares what arrived of each, before any device
    /// buffer, and `presentation` compares the sound the device says it has
    /// played against the picture last handed to the window — a submission, so
    /// that one is an estimate. Unknown while a mapping is missing or older
    /// than five seconds, and for a share, which nothing times against anything.
    pub arrival_skew_ms: Option<i64>,
    pub presentation_skew_ms: Option<i64>,
    /// Pictures the audible sound had already passed by more than the 80 ms
    /// bound, dropped rather than shown late since the watch started. Unknown
    /// for a share.
    pub late_pictures: Option<u64>,
}

/// How a stream's pictures reach the window.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub enum Presentation {
    /// Copied out of the frames file as pixels, which costs a copy of every
    /// frame through the processor.
    Copy,
    /// Shown from the worker's own texture, shared with the window's process:
    /// no pixel of it is copied anywhere.
    Shared,
}

impl Presentation {
    /// What the Stream tab calls it.
    pub fn name(self) -> &'static str {
        match self {
            Self::Copy => "a copy of the picture",
            Self::Shared => "the worker's own texture",
        }
    }
}

/// Reads the call's [`LinkStats`], which the call measures all along and
/// the UI copies out only while it shows them.
#[derive(Clone)]
pub struct Link(Arc<dyn Fn() -> LinkStats + Send + Sync>);

impl Link {
    pub fn new(read: impl Fn() -> LinkStats + Send + Sync + 'static) -> Self {
        Self(Arc::new(read))
    }

    pub fn read(&self) -> LinkStats {
        (self.0)()
    }
}

impl std::fmt::Debug for Link {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Link")
    }
}

/// How good the call's connection is, as Discord's client grades it for its
/// signal icon.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Quality {
    #[default]
    Unknown,
    Fine,
    Average,
    Bad,
}

impl Quality {
    /// Bad past 500 ms or 10% of packets lost, average past 250 ms or 5%.
    pub fn of(ping: Option<u16>, loss: Option<f32>) -> Self {
        let loss = loss.unwrap_or(0.0);
        match ping {
            None => Self::Unknown,
            Some(ping) if ping > 500 || loss > 10.0 => Self::Bad,
            Some(ping) if ping > 250 || loss > 5.0 => Self::Average,
            Some(_) => Self::Fine,
        }
    }
}

/// This account's camera in a call.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CameraPhase {
    #[default]
    Off,
    /// Opening the camera, until its first picture.
    Starting,
    /// Filming into the call.
    On,
}

#[derive(Clone, Debug)]
pub struct State {
    pub phase: Phase,
    pub room: Option<String>,
    pub name: String,
    pub muted: bool,
    pub deafened: bool,
    pub participants: Vec<Participant>,
    pub microphones: Vec<Device>,
    pub speakers: Vec<Device>,
    pub cameras: Vec<Device>,
    pub microphone: Option<String>,
    pub speaker: Option<String>,
    /// The chosen camera's id; none for the first one.
    pub camera: Option<String>,
    pub error: Option<String>,
    /// A problem that leaves the call usable, such as a participant who has
    /// not received this device's audio key.
    pub warning: Option<String>,
    pub sharing: Option<Sharing>,
    pub watching: Option<Box<Watching>>,
    /// How the next screen share is sent.
    pub quality: StreamQuality,
    /// This account's camera, turned on or starting.
    pub camera_phase: CameraPhase,
    /// The call carries camera video, so the camera can be turned on.
    pub can_film: bool,
    /// The call's connection, while it measures how it's doing, and how
    /// good it is.
    pub link: Option<Link>,
    pub link_quality: Quality,
    /// Media leaves end-to-end encrypted.
    pub end_to_end: bool,
    /// The soundboard's sounds this account may play in the call, in
    /// groups; none where the call has no soundboard, and empty while they
    /// load.
    pub soundboard: Option<Arc<[SoundGroup]>>,
}

/// A join sound the Discord account's settings give.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JoinSound {
    /// The server it plays in; none for every server without its own.
    pub server: Option<String>,
    pub sound: String,
    /// The sound's own server; none for Discord's own.
    pub guild: Option<String>,
}

/// A group of soundboard sounds: the account's favourites, a server's, or
/// Discord's own.
#[derive(Clone, Debug, PartialEq)]
pub struct SoundGroup {
    pub name: String,
    pub sounds: Vec<SoundChoice>,
}

/// A soundboard sound, as the soundboard offers it.
#[derive(Clone, Debug, PartialEq)]
pub struct SoundChoice {
    pub id: String,
    pub name: String,
    /// Its Unicode emoji, or empty.
    pub emoji: String,
    /// Its custom emoji's picture.
    pub picture: Option<String>,
    /// Among the account's favourites.
    pub favorite: bool,
    /// The server it's from; none for Discord's own.
    pub guild: Option<String>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            phase: Phase::Disconnected,
            room: None,
            name: String::new(),
            muted: true,
            deafened: false,
            participants: vec![],
            microphones: vec![],
            speakers: vec![],
            cameras: vec![],
            microphone: None,
            speaker: None,
            camera: None,
            error: None,
            warning: None,
            sharing: None,
            watching: None,
            quality: StreamQuality::default(),
            camera_phase: CameraPhase::Off,
            can_film: false,
            link: None,
            link_quality: Quality::Unknown,
            end_to_end: false,
            soundboard: None,
        }
    }
}

impl State {
    pub fn active(&self) -> bool {
        self.phase != Phase::Disconnected
    }
    fn disconnected(&mut self) {
        self.phase = Phase::Disconnected;
        self.room = None;
        self.participants.clear();
        self.muted = true;
        self.deafened = false;
        self.warning = None;
        self.sharing = None;
        self.watching = None;
        self.camera_phase = CameraPhase::Off;
        self.can_film = false;
        self.link = None;
        self.link_quality = Quality::Unknown;
        self.end_to_end = false;
        self.soundboard = None;
    }
}

#[derive(Clone)]
pub enum Command {
    Join(String),
    Leave,
    Mute(bool),
    Deafen(bool),
    Devices,
    Microphone(String),
    Speaker(String),
    /// Film with the camera with this id from now on.
    Camera(String),
    /// Turn this account's camera on or off in the call.
    Video(bool),
    /// Share a screen or window with Discord's Go Live, at this quality,
    /// which is remembered for next time.
    GoLive(StreamQuality),
    /// Share the screen or window picked last in place of the one shared,
    /// keeping the stream; in a Wayland session, what the system's dialog picks.
    SwitchLive,
    /// Show the preview of the shared screen in the UI, or stop: the worker
    /// makes one only while the UI displays it.
    Preview(bool),
    StopLive,
    /// Play the soundboard sound with this id for everyone in the call.
    Soundboard(String),
    /// Play the soundboard sound with this id here only, to hear it first.
    PreviewSound(String),
    /// Add the soundboard sound with this id to the account's favourites,
    /// or take it out.
    FavoriteSound(String, bool),
    /// Watch the participant with this id's stream.
    Watch(String),
    StopWatching,
    /// Show the watched stream's pictures, or stop: a hidden or minimised
    /// window shows none, so the worker decodes on without converting or
    /// writing frames nobody can see.
    WatchShown(bool),
    /// The size, in pixels, of the tile showing the watched stream, as the
    /// UI says it when that changes and when the stream starts showing: the
    /// worker scales the stream to it, so a small tile costs a small frame
    /// and a large one grows the frames file once. A zero or the size the
    /// worker already has says nothing.
    WatchSize {
        width: u32,
        height: u32,
    },
    /// The size, in pixels, of the tile showing this account's own share, as
    /// the UI says it when that changes and when the share starts showing: the
    /// preview is written at that size, so a small tile costs a small frame and
    /// a large one grows the frames file once. The stream that goes out is the
    /// same whatever it says. A zero says nothing.
    ShareSize {
        width: u32,
        height: u32,
    },
    /// Remove memberships this device left in voice channels, for example
    /// after Yapfast was killed. Ignored during a call.
    LeaveStale,
}

/// The networks whose voice channels can be joined, one call at a time.
pub(crate) struct Calls {
    /// The session lease holds the SDK store lock until native departure finishes.
    #[cfg(feature = "matrix")]
    pub session: SessionLease,
    /// The linked Discord account, which linking or unlinking changes while
    /// Yapfast runs.
    #[cfg(feature = "discord")]
    pub discord: watch::Receiver<Option<Discord>>,
}

pub struct Controller {
    commands: Option<mpsc::UnboundedSender<Command>>,
    pub states: watch::Receiver<State>,
    thread: Option<thread::JoinHandle<()>>,
    // What `restart` starts again with.
    #[cfg(feature = "matrix")]
    devices: Option<PathBuf>,
    #[cfg(all(feature = "matrix", feature = "discord"))]
    discord: watch::Receiver<Option<Discord>>,
}

impl Controller {
    #[cfg(all(test, feature = "matrix"))]
    pub(crate) fn start(session: SessionLease) -> Result<Self> {
        let calls = Calls {
            session,
            #[cfg(feature = "discord")]
            discord: watch::channel(None).1,
        };
        Self::start_remembering(calls, None)
    }

    /// Starts with the devices chosen earlier on this machine, and saves each
    /// new choice to `devices`, a file that never leaves it.
    pub(crate) fn start_remembering(calls: Calls, devices: Option<PathBuf>) -> Result<Self> {
        #[cfg(all(feature = "matrix", feature = "discord"))]
        let discord = calls.discord.clone();
        #[cfg(feature = "matrix")]
        let remembered = devices.clone();
        let (commands, receiver) = mpsc::unbounded_channel();
        let mut initial = State::default();
        if let Some(path) = &devices {
            let chosen = Chosen::read(path);
            initial.microphone = chosen.microphone;
            initial.speaker = chosen.speaker;
            initial.camera = chosen.camera;
            initial.quality = chosen.quality;
        }
        let (states, snapshots) = watch::channel(initial.clone());
        let worker = thread::Builder::new()
            .name("yapfast-voice".into())
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build();
                match runtime {
                    Ok(runtime) => runtime.block_on(
                        tokio::task::LocalSet::new()
                            .run_until(worker(calls, receiver, states, initial, devices)),
                    ),
                    Err(_) => {
                        states.send_modify(|state| {
                            state.error =
                                Some("Could not start voice. Restart Yapfast and try again.".into())
                        });
                    }
                }
            })
            .context("could not start voice worker")?;
        Ok(Self {
            commands: Some(commands),
            states: snapshots,
            thread: Some(worker),
            #[cfg(feature = "matrix")]
            devices: remembered,
            #[cfg(all(feature = "matrix", feature = "discord"))]
            discord,
        })
    }

    /// Starts the worker again after `stop`, as after a failed sign-out.
    #[cfg(feature = "matrix")]
    pub(crate) fn restart(&mut self, session: SessionLease) {
        let calls = Calls {
            session,
            #[cfg(feature = "discord")]
            discord: self.discord.clone(),
        };
        if let Ok(controller) = Self::start_remembering(calls, self.devices.clone()) {
            *self = controller;
        }
    }

    pub fn command(&self, command: Command) -> bool {
        self.commands
            .as_ref()
            .is_some_and(|sender| sender.send(command).is_ok())
    }

    pub async fn stop(&mut self) {
        self.commands.take();
        if let Some(worker) = self.thread.take() {
            let _ = tokio::task::spawn_blocking(move || worker.join()).await;
        }
    }
}

impl Drop for Controller {
    fn drop(&mut self) {
        self.commands.take();
        // Shutdown is bounded by the join and departure deadlines. Keep the
        // process alive long enough to leave Matrix and release audio devices.
        if let Some(worker) = self.thread.take() {
            let _ = worker.join();
        }
    }
}

async fn worker(
    calls: Calls,
    mut commands: mpsc::UnboundedReceiver<Command>,
    snapshots: watch::Sender<State>,
    mut state: State,
    remembered: Option<PathBuf>,
) {
    while let Some(command) = commands.recv().await {
        match command {
            Command::Join(id) => {
                state.error = None;
                state.room = Some(id.clone());
                state.name.clear();
                state.phase = Phase::Joining;
                state.muted = true;
                state.deafened = false;
                snapshots.send_replace(state.clone());
                let result = join(
                    &calls,
                    id,
                    &mut state,
                    &mut commands,
                    &snapshots,
                    remembered.as_deref(),
                )
                .await;
                if let Err(error) = result {
                    state.error = Some(error.to_string());
                }
                state.disconnected();
                trim_after_call();
            }
            Command::Devices => match devices(&mut state) {
                Ok(()) => state.error = None,
                Err(error) => state.error = Some(error.to_string()),
            },
            Command::Microphone(id) => {
                state.microphone = Some(id);
                Chosen::write(remembered.as_deref(), &state);
            }
            Command::Speaker(id) => {
                state.speaker = Some(id);
                Chosen::write(remembered.as_deref(), &state);
            }
            Command::Camera(id) => {
                state.camera = Some(id);
                Chosen::write(remembered.as_deref(), &state);
            }
            #[cfg(feature = "matrix")]
            Command::LeaveStale => crate::matrix_voice::leave_stale(&calls.session).await,
            _ => {}
        }
        snapshots.send_replace(state.clone());
    }
}

/// glibc keeps the heap a call freed for reuse, 17-20 MiB in a fake Discord
/// call with the microphone on, so Yapfast would stay at its size in the call.
/// Return it once the call has finished freeing it: its audio threads and
/// connections close on their own after it ends, and a Discord call had freed
/// everything 2 s after leaving. Turning a camera off frees its picture
/// buffers the same way.
pub(crate) fn trim_after_call() {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    tokio::task::spawn_local(async {
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        unsafe extern "C" {
            fn malloc_trim(pad: usize) -> std::ffi::c_int;
        }
        // SAFETY: malloc_trim only returns unused allocator pages to the OS.
        unsafe { malloc_trim(0) };
    });
}

/// One call, from joining until leaving, on the network the channel is from.
#[allow(unused_variables)]
async fn join(
    calls: &Calls,
    id: String,
    state: &mut State,
    commands: &mut mpsc::UnboundedReceiver<Command>,
    snapshots: &watch::Sender<State>,
    remembered: Option<&Path>,
) -> Result<()> {
    #[cfg(feature = "discord")]
    if crate::network::Network::of(&id) == crate::network::Network::Discord {
        let discord = calls.discord.borrow().clone();
        return match (discord, crate::discord::parse_room(&id)) {
            (Some(discord), Some((guild, channel))) => {
                crate::discord_voice::call(
                    &discord, guild, channel, state, commands, snapshots, remembered,
                )
                .await
            }
            _ => Err(anyhow::anyhow!("Discord isn't linked on this device.")),
        };
    }
    #[cfg(feature = "matrix")]
    return crate::matrix_voice::call(&calls.session, id, state, commands, snapshots, remembered)
        .await;
    #[cfg(not(feature = "matrix"))]
    Err(anyhow::anyhow!(
        "This build of Yapfast has no Matrix voice."
    ))
}

/// The file in Yapfast's data folder that remembers the chosen devices.
pub const DEVICES: &str = "devices.json";

/// The devices and stream quality chosen on this machine, kept beside its
/// other local files.
#[derive(Default, serde::Deserialize, serde::Serialize)]
pub(crate) struct Chosen {
    microphone: Option<String>,
    speaker: Option<String>,
    #[serde(default)]
    camera: Option<String>,
    #[serde(default)]
    quality: StreamQuality,
}

impl Chosen {
    pub(crate) fn read(path: &Path) -> Self {
        std::fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    pub(crate) fn write(path: Option<&Path>, state: &State) {
        let Some(path) = path else { return };
        let chosen = Self {
            microphone: state.microphone.clone(),
            speaker: state.speaker.clone(),
            camera: state.camera.clone(),
            quality: state.quality,
        };
        let result = serde_json::to_vec_pretty(&chosen)
            .map_err(std::io::Error::other)
            .and_then(|bytes| std::fs::write(path, bytes));
        if let Err(error) = result {
            tracing::warn!(%error, "could not remember the chosen devices and stream quality");
        }
    }
}

/// The system's microphones, speakers and cameras.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Devices {
    pub microphones: Vec<Device>,
    pub speakers: Vec<Device>,
    pub cameras: Vec<Device>,
}

/// The system's devices, listed without opening any of them.
pub fn list_devices() -> Result<Devices> {
    let mut state = State::default();
    devices(&mut state)?;
    Ok(Devices {
        microphones: state.microphones,
        speakers: state.speakers,
        cameras: state.cameras,
    })
}

pub(crate) fn devices(state: &mut State) -> Result<()> {
    state.cameras = crate::camera::list();
    let audio = audio()?;
    collect_devices(&audio, state);
    Ok(())
}

pub(crate) fn audio() -> Result<PlatformAudio> {
    crate::leveling::install();
    PlatformAudio::new().map_err(|_| {
        anyhow::anyhow!(
            "Audio devices are unavailable. Check your system's audio service and permissions."
        )
    })
}

pub(crate) fn collect_devices(audio: &PlatformAudio, state: &mut State) {
    state.microphones = audio
        .recording_devices()
        .map(|device| Device {
            id: device_id(device.id.as_str(), &device.name),
            name: device.name,
        })
        .collect();
    state.speakers = audio
        .playout_devices()
        .map(|device| Device {
            id: device_id(device.id.as_str(), &device.name),
            name: device.name,
        })
        .collect();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn devices_without_a_guid_are_known_by_name_and_choices_stay_on_disk() {
        assert_eq!(device_id("", "Null Output"), "name:Null Output");
        assert_eq!(device_id("{0.0.1}", "Speakers"), "{0.0.1}");
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join(DEVICES);
        assert!(Chosen::read(&path).microphone.is_none());
        let state = State {
            microphone: Some("name:USB Microphone".into()),
            speaker: Some("name:Null Output".into()),
            camera: Some("usb-0000:00:14.0-1".into()),
            quality: StreamQuality {
                resolution: Resolution::Source,
                fps: 60,
                high_quality: true,
            },
            ..Default::default()
        };
        Chosen::write(Some(&path), &state);
        let chosen = Chosen::read(&path);
        assert_eq!(chosen.microphone.as_deref(), Some("name:USB Microphone"));
        assert_eq!(chosen.speaker.as_deref(), Some("name:Null Output"));
        assert_eq!(chosen.camera.as_deref(), Some("usb-0000:00:14.0-1"));
        assert_eq!(chosen.quality, state.quality);
        // Choices saved before there was a quality to choose still load.
        std::fs::write(
            &path,
            br#"{"microphone": null, "speaker": "name:Null Output"}"#,
        )
        .unwrap();
        assert_eq!(Chosen::read(&path).quality, StreamQuality::default());
        std::fs::write(&path, b"not json").unwrap();
        assert!(Chosen::read(&path).speaker.is_none());
    }
}
