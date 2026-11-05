//! Sound through WASAPI in shared mode. The audio engine converts between a
//! stream's format and the device's, and signals an event as it plays or
//! records; reading and writing wait for it on the caller's thread, copy
//! straight between the caller's samples and the engine's buffer, and give
//! up within a slice when the stream that owns them stops.
//!
//! WebRTC's Windows device module names each device by its endpoint id, as
//! `IMMDevice::GetId` gives it, so a device id opens the endpoint directly.

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use anyhow::{Context as _, Result, anyhow, ensure};
use windows::{
    Win32::{
        Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT},
        Media::{
            Audio::{
                AUDCLNT_BUFFERFLAGS_DATA_DISCONTINUITY, AUDCLNT_BUFFERFLAGS_SILENT,
                AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM,
                AUDCLNT_STREAMFLAGS_EVENTCALLBACK, AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
                DEVICE_STATE_ACTIVE, EDataFlow, IAudioCaptureClient, IAudioClient,
                IAudioRenderClient, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator,
                WAVEFORMATEX, WAVEFORMATEXTENSIBLE, WAVEFORMATEXTENSIBLE_0, eCapture, eConsole,
                eRender,
            },
            KernelStreaming::{
                KSDATAFORMAT_SUBTYPE_PCM, SPEAKER_FRONT_CENTER, SPEAKER_FRONT_LEFT,
                SPEAKER_FRONT_RIGHT, WAVE_FORMAT_EXTENSIBLE,
            },
            Multimedia::KSDATAFORMAT_SUBTYPE_IEEE_FLOAT,
        },
        System::{
            Com::{
                CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
            },
            Threading::{CreateEventW, WaitForSingleObject},
        },
    },
    core::HSTRING,
};

use super::{
    Format, News, Packet, Step, Waited, drain_deadline, drain_must_wait, packet_news, wait_step,
    write_step,
};
use crate::voice::NAMED;

/// What a stream without a queue of its own keeps queued.
const DEFAULT_QUEUE: Duration = Duration::from_millis(200);
/// The least a microphone's buffer holds, so a reader that's a little late
/// loses nothing.
const RECORDING: Duration = Duration::from_millis(200);
/// The engine signals every few milliseconds while a stream runs; this long
/// without a signal means the device went away.
const SILENCE: Duration = Duration::from_secs(2);
/// How long a wait for the engine lasts before it looks at the stop of the
/// stream it waits for again: short enough that a stop is seen at once, long
/// enough that a stream which isn't stopping wakes a few times a second
/// rather than constantly.
const STOP_CHECK_MS: u32 = 100;
/// How long past a drained queue the engine has to get going before the
/// device counts as stopped.
const DRAIN_GRACE: Duration = Duration::from_secs(1);

/// Each thread that opens or uses a stream joins COM's multithreaded
/// apartment, in which the engine's objects work from any of its threads.
struct Apartment(bool);

impl Apartment {
    fn join() -> Self {
        // SAFETY: balanced by CoUninitialize when the thread ends; a thread
        // already in a single-threaded apartment stays there.
        Self(unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok())
    }
}

impl Drop for Apartment {
    fn drop(&mut self) {
        if self.0 {
            // SAFETY: this thread's CoInitializeEx succeeded.
            unsafe { CoUninitialize() };
        }
    }
}

thread_local! {
    static APARTMENT: Apartment = Apartment::join();
}

fn com() {
    APARTMENT.with(|_| {});
}

/// The endpoint behind a device id, or the default one for `None`.
fn device(id: Option<&str>, flow: EDataFlow) -> Result<IMMDevice> {
    com();
    // SAFETY: plain COM calls on this thread's apartment.
    unsafe {
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                .context("no audio device service")?;
        // Ids made from a name come from PulseAudio, not from here.
        match id.filter(|id| !id.starts_with(NAMED)) {
            None => enumerator
                .GetDefaultAudioEndpoint(flow, eConsole)
                .context("no default device"),
            Some(id) => {
                let device = enumerator
                    .GetDevice(&HSTRING::from(id))
                    .context("device not found")?;
                ensure!(
                    device.GetState()? == DEVICE_STATE_ACTIVE,
                    "device not connected"
                );
                Ok(device)
            }
        }
    }
}

/// The endpoint a stream for `id` would open now, by its id: the device's
/// own while it's connected, and otherwise the default's, which Windows
/// changes as devices come and go while streams stay where they opened.
/// None while the system can't say, which is no news that a stream should
/// move.
#[cfg(feature = "discord")]
pub fn endpoint(id: Option<&str>, output: bool) -> Option<String> {
    let flow = if output { eRender } else { eCapture };
    let device = device(id, flow).or_else(|_| device(None, flow)).ok()?;
    // SAFETY: GetId lends a string the caller frees.
    unsafe {
        let name = device.GetId().ok()?;
        let id = name.to_string().ok();
        windows::Win32::System::Com::CoTaskMemFree(Some(name.0 as *const _));
        id
    }
}

fn wave_format(format: Format) -> WAVEFORMATEXTENSIBLE {
    let bits = if format.float { 32 } else { 16 };
    let block = format.frame_bytes() as u16;
    WAVEFORMATEXTENSIBLE {
        Format: WAVEFORMATEX {
            wFormatTag: WAVE_FORMAT_EXTENSIBLE as u16,
            nChannels: format.channels,
            nSamplesPerSec: format.rate,
            nAvgBytesPerSec: format.rate * u32::from(block),
            nBlockAlign: block,
            wBitsPerSample: bits,
            cbSize: (size_of::<WAVEFORMATEXTENSIBLE>() - size_of::<WAVEFORMATEX>()) as u16,
        },
        Samples: WAVEFORMATEXTENSIBLE_0 {
            wValidBitsPerSample: bits,
        },
        dwChannelMask: if format.channels == 1 {
            SPEAKER_FRONT_CENTER
        } else {
            SPEAKER_FRONT_LEFT | SPEAKER_FRONT_RIGHT
        },
        SubFormat: if format.float {
            KSDATAFORMAT_SUBTYPE_IEEE_FLOAT
        } else {
            KSDATAFORMAT_SUBTYPE_PCM
        },
    }
}

/// In WASAPI's 100 ns units.
fn hundred_nanoseconds(duration: Duration) -> i64 {
    (duration.as_nanos() / 100) as i64
}

/// An initialized shared-mode stream and the event its engine signals.
struct Stream {
    client: IAudioClient,
    event: HANDLE,
    format: Format,
    /// The engine's buffer, in frames.
    buffer: u32,
    running: bool,
}

// SAFETY: the engine's objects live in the multithreaded apartment, which
// every thread using them joins first (`com`).
unsafe impl Send for Stream {}

impl Stream {
    fn open(device: &IMMDevice, format: Format, buffer: Duration) -> Result<Self> {
        // SAFETY: a plain COM call.
        let client =
            unsafe { device.Activate(CLSCTX_ALL, None) }.context("the device refused a stream")?;
        Self::initialize(client, format, buffer, 0)
    }

    /// Starts `client` as a stream, with `flags` besides the usual ones.
    fn initialize(
        client: IAudioClient,
        format: Format,
        buffer: Duration,
        flags: u32,
    ) -> Result<Self> {
        // SAFETY: plain COM calls; the format outlives Initialize.
        unsafe {
            let wave = wave_format(format);
            client
                .Initialize(
                    AUDCLNT_SHAREMODE_SHARED,
                    flags
                        | AUDCLNT_STREAMFLAGS_EVENTCALLBACK
                        | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM
                        | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
                    hundred_nanoseconds(buffer),
                    0,
                    (&raw const wave).cast::<WAVEFORMATEX>(),
                    None,
                )
                .context("the device refused the stream's format")?;
            let event = CreateEventW(None, false, false, None)?;
            // Built first, so dropping it closes the event on an error.
            let mut stream = Self {
                buffer: 0,
                client,
                event,
                format,
                running: false,
            };
            stream.client.SetEventHandle(stream.event)?;
            stream.buffer = stream.client.GetBufferSize()?;
            Ok(stream)
        }
    }

    fn start(&mut self) -> Result<()> {
        if !self.running {
            // SAFETY: an initialized client.
            unsafe { self.client.Start() }.context("the device wouldn't start")?;
            self.running = true;
        }
        Ok(())
    }

    fn stop(&mut self) {
        if self.running {
            // SAFETY: as above.
            let _ = unsafe { self.client.Stop() };
            self.running = false;
        }
    }

    /// Frames waiting in the engine's buffer.
    fn padding(&self) -> Result<u32> {
        // SAFETY: as above.
        unsafe { self.client.GetCurrentPadding() }.context("the device went away")
    }

    /// Waits for the engine to play or record more, or for the stop of the
    /// stream it waits for: a slice says nothing on its own, since the
    /// engine signals every few milliseconds while it runs, so only the
    /// whole of [`SILENCE`] quiet is a device that went away.
    fn wait(&self, stop: Option<&AtomicBool>) -> Result<Waited> {
        let quiet_since = Instant::now();
        loop {
            // The stream ends whatever the device does, so a stop is seen
            // without waiting for the device at all.
            if stop.is_some_and(|stop| stop.load(Ordering::Relaxed)) {
                return Ok(Waited::Cancelled);
            }
            // SAFETY: the event lives as long as the stream.
            let waited = unsafe { WaitForSingleObject(self.event, STOP_CHECK_MS) };
            ensure!(
                waited == WAIT_OBJECT_0 || waited == WAIT_TIMEOUT,
                "the device stopped"
            );
            if let Some(waited) =
                wait_step(waited == WAIT_OBJECT_0, quiet_since.elapsed(), SILENCE)?
            {
                return Ok(waited);
            }
        }
    }
}

impl Drop for Stream {
    fn drop(&mut self) {
        com();
        self.stop();
        // SAFETY: the stream owns its event, and the engine stopped using it.
        let _ = unsafe { CloseHandle(self.event) };
    }
}

pub struct Output {
    stream: Stream,
    render: IAudioRenderClient,
    /// The most kept queued, in frames: what was asked, though the engine's
    /// buffer may be larger.
    queue: u32,
    /// What ends a wait before the device answers, from the thread that
    /// stops the stream.
    stop: Option<Arc<AtomicBool>>,
}

impl Output {
    pub fn open(
        speaker: Option<&str>,
        _name: &'static str,
        format: Format,
        queue: Option<Duration>,
    ) -> Result<Self> {
        let device = device(speaker, eRender).context("these speakers are no longer connected")?;
        let queue = queue.unwrap_or(DEFAULT_QUEUE);
        let stream = Stream::open(&device, format, queue)?;
        // SAFETY: an initialized client.
        let render = unsafe { stream.client.GetService::<IAudioRenderClient>() }?;
        let queue = ((format.bytes(queue) / format.frame_bytes()) as u32).clamp(1, stream.buffer);
        Ok(Self {
            stream,
            render,
            queue,
            stop: None,
        })
    }

    /// Ends a wait for the engine as soon as `stop` is set, as the thread
    /// that stops these speakers sets it.
    pub fn stop_with(&mut self, stop: Arc<AtomicBool>) {
        self.stop = Some(stop);
    }

    pub fn write(&mut self, bytes: &[u8]) -> Result<Waited> {
        com();
        let frame_bytes = self.stream.format.frame_bytes();
        debug_assert_eq!(bytes.len() % frame_bytes, 0, "whole frames of samples");
        let mut rest = bytes;
        loop {
            let frames_left = (rest.len() / frame_bytes) as u32;
            if frames_left == 0 {
                return Ok(Waited::Signalled);
            }
            let step = write_step(
                self.stream.padding()?,
                self.queue,
                self.stream.running,
                frames_left,
            );
            match step {
                Step::Wait => {
                    // Full: it plays what it has as it takes more.
                    self.stream.start()?;
                    if self.stream.wait(self.stop.as_deref())? == Waited::Cancelled {
                        // The stream is ending: what's left of this sound
                        // ends with it rather than holding the stop back.
                        return Ok(Waited::Cancelled);
                    }
                }
                Step::Write { frames: 0, .. } => return Ok(Waited::Signalled),
                Step::Write { frames, start } => {
                    let length = frames as usize * frame_bytes;
                    // SAFETY: the engine lends `frames` frames of room, which
                    // we fill from the caller's samples before giving them
                    // back.
                    unsafe {
                        let buffer = self.render.GetBuffer(frames)?;
                        std::ptr::copy_nonoverlapping(rest.as_ptr(), buffer, length);
                        self.render.ReleaseBuffer(frames, 0)?;
                    }
                    rest = &rest[length..];
                    // Heard as soon as any of it is queued, however little.
                    if start {
                        self.stream.start()?;
                    }
                }
            }
        }
    }

    pub fn drain(&mut self) -> Result<Waited> {
        com();
        // Stopped either way, so the next write starts afresh rather than
        // playing at once or not depending on which way the queue emptied.
        let drained = self.drain_queued();
        self.stream.stop();
        // A stop ends the drain with sound still queued, which nobody is
        // waiting for any more: dropped here, so a stream that starts again
        // after an outage plays what's new rather than what it missed.
        if matches!(drained, Ok(Waited::Cancelled)) {
            self.flush();
        }
        drained
    }

    /// Waits until the engine has played what's queued, starting it if it
    /// isn't playing.
    fn drain_queued(&mut self) -> Result<Waited> {
        if !drain_must_wait(self.stream.padding()?) {
            return Ok(Waited::Signalled);
        }
        self.stream.start()?;
        let deadline =
            Instant::now() + drain_deadline(self.queue, self.stream.format.rate, DRAIN_GRACE);
        while drain_must_wait(self.stream.padding()?) {
            ensure!(Instant::now() < deadline, "the speakers stopped playing");
            if self.stream.wait(self.stop.as_deref())? == Waited::Cancelled {
                return Ok(Waited::Cancelled);
            }
        }
        Ok(Waited::Signalled)
    }

    pub fn flush(&mut self) {
        com();
        self.stream.stop();
        // SAFETY: a stopped client.
        let _ = unsafe { self.stream.client.Reset() };
    }

    pub fn latency(&self) -> Duration {
        com();
        let queued = self.stream.padding().unwrap_or(0);
        // SAFETY: an initialized client.
        let engine = unsafe { self.stream.client.GetStreamLatency() }.unwrap_or(0);
        Duration::from_secs_f64(f64::from(queued) / f64::from(self.stream.format.rate))
            + Duration::from_nanos(engine.max(0) as u64 * 100)
    }
}

// SAFETY: as for `Stream`.
unsafe impl Send for Output {}

pub struct Input {
    stream: Stream,
    capture: IAudioCaptureClient,
    /// The end of the last packet, which the last read had no room for.
    left: Vec<u8>,
    taken: usize,
    /// What the last packet said about the device, so a run of packets that
    /// say it is one line in the log rather than one per packet.
    said: Packet,
    /// The stream's name, as the system lists it: what a line about this
    /// microphone names.
    name: &'static str,
    /// What ends a wait before the device answers, from the thread that
    /// stops the stream.
    stop: Option<Arc<AtomicBool>>,
}

/// Says what a recorded packet said that the one before it didn't, once per
/// change: a device giving silence is worth knowing in a call, samples lost
/// are worth a warning, and the sound coming back ends the run either one
/// began. None of the three is the device stopping, which is a call that
/// fails instead.
fn say(name: &'static str, news: News) {
    match news {
        News::Silence => tracing::debug!(stream = name, "the microphone gives silence"),
        News::Gap => tracing::warn!(stream = name, "the microphone lost samples"),
        News::Sound => tracing::debug!(stream = name, "the microphone records again"),
    }
}

// SAFETY: as for `Stream`.
unsafe impl Send for Input {}

impl Input {
    pub fn open(
        microphone: Option<&str>,
        name: &'static str,
        format: Format,
        fragment: Duration,
    ) -> Result<Self> {
        let device =
            device(microphone, eCapture).context("this microphone is no longer connected")?;
        let mut stream = Stream::open(&device, format, (fragment * 4).max(RECORDING))?;
        // SAFETY: an initialized client.
        let capture = unsafe { stream.client.GetService::<IAudioCaptureClient>() }?;
        let left = Vec::with_capacity(stream.buffer as usize * format.frame_bytes());
        stream.start()?;
        Ok(Self {
            stream,
            capture,
            left,
            taken: 0,
            said: Packet::default(),
            name,
            stop: None,
        })
    }

    /// Ends a wait for the engine as soon as `stop` is set, as the thread
    /// that stops this microphone sets it.
    pub fn stop_with(&mut self, stop: Arc<AtomicBool>) {
        self.stop = Some(stop);
    }

    pub fn read(&mut self, bytes: &mut [u8]) -> Result<Waited> {
        com();
        let left = &self.left[self.taken..];
        let mut filled = left.len().min(bytes.len());
        bytes[..filled].copy_from_slice(&left[..filled]);
        self.taken += filled;
        while filled < bytes.len() {
            // SAFETY: an initialized, running client.
            let packet =
                unsafe { self.capture.GetNextPacketSize() }.context("the microphone went away")?;
            if packet == 0 {
                if self.stream.wait(self.stop.as_deref())? == Waited::Cancelled {
                    // Stopping: silence rather than the last block over
                    // again, which the stream that's ending would hear twice.
                    bytes[filled..].fill(0);
                    return Ok(Waited::Cancelled);
                }
                continue;
            }
            let (mut data, mut count, mut flags) = (std::ptr::null_mut(), 0, 0);
            // SAFETY: the engine lends `count` frames at `data` until
            // ReleaseBuffer; they're copied out before that.
            unsafe {
                self.capture
                    .GetBuffer(&mut data, &mut count, &mut flags, None, None)
                    .context("the microphone went away")?;
                let length = count as usize * self.stream.format.frame_bytes();
                let silent = flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0 || data.is_null();
                // Silence and a gap are what the engine says about the device
                // without either being the device stopping: it stopped when a
                // call fails or a wait stays quiet, and says so then and
                // there. Each is a line here when it starts, not per packet.
                if let Some(news) = packet_news(
                    Packet {
                        silent,
                        gap: flags & AUDCLNT_BUFFERFLAGS_DATA_DISCONTINUITY.0 as u32 != 0,
                    },
                    &mut self.said,
                ) {
                    say(self.name, news);
                }
                let packet: &[u8] = if silent {
                    &[]
                } else {
                    std::slice::from_raw_parts(data, length)
                };
                let wanted = (bytes.len() - filled).min(length);
                let into = &mut bytes[filled..filled + wanted];
                match packet.get(..wanted) {
                    Some(samples) => into.copy_from_slice(samples),
                    None => into.fill(0),
                }
                filled += wanted;
                self.left.clear();
                self.taken = 0;
                if wanted < length {
                    match packet.get(wanted..) {
                        Some(rest) => self.left.extend_from_slice(rest),
                        None => self.left.resize(length - wanted, 0),
                    }
                }
                self.capture
                    .ReleaseBuffer(count)
                    .map_err(|error| anyhow!("{error}"))?;
            }
        }
        Ok(Waited::Signalled)
    }
}

#[cfg(feature = "discord")]
pub use others::others;

/// Other applications' sound, through process loopback: one stream of
/// everything the system plays but Yapfast's own processes, from Windows 10's
/// 2004 update on. It can't leave out other applications, so `skip` has no
/// effect here: another Discord client's call reaches the stream too.
///
/// Each packet is copied out of the engine's buffer before it's passed on,
/// since what hears it mixes, which waits.
#[cfg(feature = "discord")]
mod others {
    use std::{
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
            mpsc,
        },
        thread::JoinHandle,
    };

    use windows::{
        Win32::{
            Media::Audio::{
                AUDCLNT_STREAMFLAGS_LOOPBACK, AUDIOCLIENT_ACTIVATION_PARAMS,
                AUDIOCLIENT_ACTIVATION_PARAMS_0, AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK,
                AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS, ActivateAudioInterfaceAsync,
                IActivateAudioInterfaceAsyncOperation, IActivateAudioInterfaceCompletionHandler,
                IActivateAudioInterfaceCompletionHandler_Impl,
                PROCESS_LOOPBACK_MODE_EXCLUDE_TARGET_PROCESS_TREE,
                VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK,
            },
            System::{
                Com::{
                    BLOB,
                    StructuredStorage::{
                        PROPVARIANT, PROPVARIANT_0, PROPVARIANT_0_0, PROPVARIANT_0_0_0,
                    },
                },
                Diagnostics::ToolHelp::{
                    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
                    TH32CS_SNAPPROCESS,
                },
                Variant::VT_BLOB,
            },
        },
        core::{HRESULT, Interface, Ref, implement},
    };

    use super::*;
    use crate::audio_io::{Hear, Heard, RATE};

    /// What a packet of other applications' sound holds at most.
    const LISTENING: Duration = Duration::from_millis(100);

    /// How many processes above this one a walk looks through for Yapfast's
    /// own: more than any launcher's chain, and enough that a snapshot naming
    /// a cycle of them ends.
    const LOOK_UP: usize = 64;

    pub fn others(
        _skip: fn(&str) -> bool,
        mut heard: Hear,
        stop: Arc<AtomicBool>,
    ) -> Result<JoinHandle<()>> {
        let (opened, opening) = mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("yapfast-stream-hear".into())
            .spawn(move || {
                com();
                let format = Format {
                    rate: RATE,
                    channels: 2,
                    float: true,
                };
                let stream = yapfast_process()
                    .and_then(activate)
                    .and_then(|client| {
                        Stream::initialize(client, format, LISTENING, AUDCLNT_STREAMFLAGS_LOOPBACK)
                    })
                    .and_then(|mut stream| {
                        // SAFETY: an initialized client.
                        let capture = unsafe { stream.client.GetService::<IAudioCaptureClient>() }?;
                        stream.start()?;
                        Ok((stream, capture))
                    });
                let (stream, capture) = match stream {
                    Ok(stream) => {
                        let _ = opened.send(Ok(()));
                        stream
                    }
                    Err(error) => {
                        let _ = opened.send(Err(error));
                        return;
                    }
                };
                heard(Heard::Started {
                    source: 0,
                    process: None,
                });
                if let Err(error) = hear(&stream, &capture, &mut *heard, &stop) {
                    tracing::warn!(
                        error = format!("{error:#}"),
                        "stopped hearing other applications"
                    );
                }
                heard(Heard::Stopped { source: 0 });
            })
            .context("could not start hearing other applications")?;
        match opening.recv() {
            Ok(Ok(())) => Ok(thread),
            Ok(Err(error)) => {
                let _ = thread.join();
                Err(error.context("Windows won't share other applications' sound"))
            }
            Err(_) => {
                let _ = thread.join();
                Err(anyhow!("could not hear other applications"))
            }
        }
    }

    /// Passes on what the stream hears until `stop`.
    fn hear(
        stream: &Stream,
        capture: &IAudioCaptureClient,
        heard: &mut dyn FnMut(Heard<'_>),
        stop: &AtomicBool,
    ) -> Result<()> {
        // A packet, copied out of the engine's: the engine lends it only until
        // ReleaseBuffer, and the mixer that hears it waits on its own lock,
        // which the engine must never wait on too. The room is made once, as
        // big as the largest packet, so a packet allocates nothing.
        let mut packet: Vec<f32> = Vec::new();
        while !stop.load(Ordering::Relaxed) {
            // SAFETY: the event lives as long as the stream.
            let waited = unsafe { WaitForSingleObject(stream.event, STOP_CHECK_MS) };
            ensure!(
                waited == WAIT_OBJECT_0 || waited == WAIT_TIMEOUT,
                "the sound stopped"
            );
            loop {
                // SAFETY: an initialized, running client.
                if unsafe { capture.GetNextPacketSize() }? == 0 {
                    break;
                }
                let (mut data, mut count, mut flags) = (std::ptr::null_mut(), 0, 0);
                let mut playing = false;
                // SAFETY: the engine lends `count` frames of stereo floats
                // at `data` until ReleaseBuffer, which they leave before
                // anything hears them.
                unsafe {
                    capture.GetBuffer(&mut data, &mut count, &mut flags, None, None)?;
                    // Nothing playing: the mixer hears nothing rather than silence.
                    let silent = flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0 || data.is_null();
                    if !silent {
                        copy_packet(data.cast(), count as usize * 2, &mut packet);
                        playing = !packet.is_empty();
                    }
                    capture.ReleaseBuffer(count)?;
                }
                if playing {
                    heard(Heard::Samples {
                        source: 0,
                        samples: &packet,
                    });
                }
            }
        }
        Ok(())
    }

    /// Copies a packet of `length` stereo floats out of the engine's buffer
    /// into `packet`, and leaves it holding them.
    fn copy_packet(data: *const u8, length: usize, packet: &mut Vec<f32>) {
        packet.clear();
        packet.reserve(length);
        // SAFETY: the caller read the packet from GetBuffer, which lends that
        // many floats until ReleaseBuffer; the room for them was just reserved,
        // and floats are copied as bytes, which an address of any alignment
        // suits.
        unsafe {
            std::ptr::copy_nonoverlapping(
                data,
                packet.as_mut_ptr().cast::<u8>(),
                size_of::<f32>() * length,
            );
            packet.set_len(length);
        }
    }

    /// Signals once Windows has made the loopback stream.
    #[implement(IActivateAudioInterfaceCompletionHandler)]
    struct Activated(mpsc::Sender<()>);

    impl IActivateAudioInterfaceCompletionHandler_Impl for Activated_Impl {
        fn ActivateCompleted(
            &self,
            _: Ref<IActivateAudioInterfaceAsyncOperation>,
        ) -> windows::core::Result<()> {
            let _ = self.0.send(());
            Ok(())
        }
    }

    /// A loopback stream of everything but `process` and the processes it
    /// started.
    fn activate(process: u32) -> Result<IAudioClient> {
        let mut parameters = AUDIOCLIENT_ACTIVATION_PARAMS {
            ActivationType: AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK,
            Anonymous: AUDIOCLIENT_ACTIVATION_PARAMS_0 {
                ProcessLoopbackParams: AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS {
                    TargetProcessId: process,
                    ProcessLoopbackMode: PROCESS_LOOPBACK_MODE_EXCLUDE_TARGET_PROCESS_TREE,
                },
            },
        };
        // Never cleared, which would free the parameters it points to.
        let variant = std::mem::ManuallyDrop::new(PROPVARIANT {
            Anonymous: PROPVARIANT_0 {
                Anonymous: std::mem::ManuallyDrop::new(PROPVARIANT_0_0 {
                    vt: VT_BLOB,
                    Anonymous: PROPVARIANT_0_0_0 {
                        blob: BLOB {
                            cbSize: size_of_val(&parameters) as u32,
                            pBlobData: (&raw mut parameters).cast(),
                        },
                    },
                    ..Default::default()
                }),
            },
        });
        let (done, activated) = mpsc::channel();
        let handler: IActivateAudioInterfaceCompletionHandler = Activated(done).into();
        // SAFETY: the parameters outlive the call, which copies them, and
        // the operation's result is read only once it completes.
        unsafe {
            let operation = ActivateAudioInterfaceAsync(
                VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK,
                &IAudioClient::IID,
                Some(&raw const *variant),
                &handler,
            )?;
            activated
                .recv_timeout(Duration::from_secs(5))
                .context("Windows didn't answer")?;
            let (mut result, mut client) = (HRESULT(0), None);
            operation.GetActivateResult(&mut result, &mut client)?;
            result.ok()?;
            Ok(client.context("no stream")?.cast()?)
        }
    }

    /// The process whose tree is Yapfast's own: the Yapfast that started this
    /// stream worker, or this process when it's Yapfast itself.
    ///
    /// Everything that tree plays is left out of what's heard, so it must never
    /// be smaller than Yapfast's own: leaving out only this worker would hear
    /// the window's call and share it back to the people in it. So a process
    /// list Windows won't give is an error here, not a guess at this process
    /// alone.
    fn yapfast_process() -> Result<u32> {
        let own = std::process::id();
        // SAFETY: a snapshot this function owns, and entries sized as asked; it's
        // closed before returning, whichever way that is.
        unsafe {
            let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).context(
                "Windows wouldn't list its processes, so there's no telling which sound is Yapfast's",
            )?;
            let listed = |process: u32| {
                let mut entry = PROCESSENTRY32W {
                    dwSize: size_of::<PROCESSENTRY32W>() as u32,
                    ..Default::default()
                };
                let mut found = Process32FirstW(snapshot, &mut entry).is_ok();
                while found && entry.th32ProcessID != process {
                    found = Process32NextW(snapshot, &mut entry).is_ok();
                }
                found.then_some(entry)
            };
            let name = |entry: &PROCESSENTRY32W| {
                let length = entry.szExeFile.iter().position(|&unit| unit == 0);
                String::from_utf16_lossy(&entry.szExeFile[..length.unwrap_or(260)]).to_lowercase()
            };
            let mut here = listed(own).context(
                "Windows wouldn't list this process, so there's no telling which sound is Yapfast's",
            )?;
            let running = name(&here);
            // The processes above this one, nearest first, as many as the
            // snapshot lists: one that has already exited isn't there, so the
            // walk ends at it.
            let mut above = Vec::new();
            for _ in 0..LOOK_UP {
                let Some(parent) = listed(here.th32ParentProcessID) else {
                    break;
                };
                above.push((parent.th32ProcessID, name(&parent)));
                here = parent;
            }
            let _ = CloseHandle(snapshot);
            Ok(yapfasts(own, &running, above))
        }
    }

    /// Yapfast's own process: the nearest process in `above`, which lists the
    /// ones above this one nearest first with the program each runs, that runs
    /// Yapfast's own program. That's the window that started this stream
    /// worker; nothing between the two plays Yapfast's sound, so a shell or a
    /// debugger passes. When nothing above does, this process is Yapfast's
    /// whole tree itself.
    fn yapfasts(own: u32, running: &str, above: Vec<(u32, String)>) -> u32 {
        above
            .into_iter()
            .find(|(_, program)| program == running)
            .map_or(own, |(process, _)| process)
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::audio_io::bytes;
        /// The engine may lend a packet at an address no float goes at, and it's
        /// still every sample it holds: they're copied as bytes. The room made
        /// for the largest packet is kept, so the next one allocates nothing.
        #[test]
        fn a_packet_is_copied_whatever_its_address() {
            let samples = [0.25f32, -0.5, 1.0, -0.0];
            let mut lent = vec![0u8; size_of_val(&samples) + 1];
            lent[1..].copy_from_slice(bytes(&samples));
            let mut packet: Vec<f32> = Vec::new();
            copy_packet(lent[1..].as_ptr().cast(), samples.len(), &mut packet);
            assert_eq!(&packet[..], &samples[..]);
            let room = packet.capacity();
            copy_packet(lent[1..].as_ptr().cast(), 2, &mut packet);
            assert_eq!(&packet[..], &samples[..2]);
            assert_eq!(packet.capacity(), room, "a packet copied into no new room");
        }
        /// The window that started the worker is Yapfast's own process, found
        /// past whatever else is between them; a worker started by anything
        /// else is Yapfast's whole tree itself.
        #[test]
        fn the_window_that_started_the_worker_is_yapfast() {
            let above = |processes: &[(u32, &str)]| {
                processes
                    .iter()
                    .map(|(process, program)| (*process, program.to_string()))
                    .collect()
            };
            assert_eq!(
                yapfasts(400, "yapfast.exe", above(&[(300, "yapfast.exe")])),
                300,
                "the window that started the worker"
            );
            assert_eq!(
                yapfasts(
                    400,
                    "yapfast.exe",
                    above(&[(300, "cmd.exe"), (200, "yapfast.exe")])
                ),
                200,
                "a shell between them plays nothing of Yapfast's"
            );
            assert_eq!(
                yapfasts(400, "yapfast.exe", above(&[(300, "explorer.exe")])),
                400,
                "started by something else, the worker is all of Yapfast there is"
            );
            assert_eq!(yapfasts(400, "yapfast.exe", above(&[])), 400);
        }
    }
}
