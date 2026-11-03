//! Sound through WASAPI in shared mode. The audio engine converts between a
//! stream's format and the device's, and signals an event as it plays or
//! records; reading and writing wait for it on the caller's thread, and copy
//! straight between the caller's samples and the engine's buffer.
//!
//! WebRTC's Windows device module names each device by its endpoint id, as
//! `IMMDevice::GetId` gives it, so a device id opens the endpoint directly.

use std::time::{Duration, Instant};

use anyhow::{Context as _, Result, anyhow, ensure};
use windows::{
    Win32::{
        Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0},
        Media::{
            Audio::{
                AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_SHAREMODE_SHARED,
                AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM, AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
                AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY, DEVICE_STATE_ACTIVE, EDataFlow,
                IAudioCaptureClient, IAudioClient, IAudioRenderClient, IMMDevice,
                IMMDeviceEnumerator, MMDeviceEnumerator, WAVEFORMATEX, WAVEFORMATEXTENSIBLE,
                WAVEFORMATEXTENSIBLE_0, eCapture, eConsole, eRender,
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

use super::Format;
use crate::voice::NAMED;

/// What a stream without a queue of its own keeps queued.
const DEFAULT_QUEUE: Duration = Duration::from_millis(200);
/// The least a microphone's buffer holds, so a reader that's a little late
/// loses nothing.
const RECORDING: Duration = Duration::from_millis(200);
/// The engine signals every few milliseconds while a stream runs; this long
/// without a signal means the device went away.
const SILENCE_MS: u32 = 2000;

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
/// Empty if there's none.
#[cfg(feature = "discord")]
pub fn endpoint(id: Option<&str>, output: bool) -> String {
    let flow = if output { eRender } else { eCapture };
    let Ok(device) = device(id, flow).or_else(|_| device(None, flow)) else {
        return String::new();
    };
    // SAFETY: GetId lends a string the caller frees.
    unsafe {
        let Ok(name) = device.GetId() else {
            return String::new();
        };
        let id = name.to_string().unwrap_or_default();
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

    /// Waits for the engine to play or record more.
    fn wait(&self) -> Result<()> {
        // SAFETY: the event lives as long as the stream.
        let waited = unsafe { WaitForSingleObject(self.event, SILENCE_MS) };
        ensure!(waited == WAIT_OBJECT_0, "the device stopped");
        Ok(())
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
        })
    }

    pub fn write(&mut self, bytes: &[u8]) -> Result<()> {
        com();
        let frame_bytes = self.stream.format.frame_bytes();
        let mut rest = bytes;
        while rest.len() >= frame_bytes {
            let padding = self.stream.padding()?;
            let room = self.queue.saturating_sub(padding);
            if room == 0 {
                // Full: it plays once it has all it may queue.
                self.stream.start()?;
                self.stream.wait()?;
                continue;
            }
            let count = room.min((rest.len() / frame_bytes) as u32);
            let length = count as usize * frame_bytes;
            // SAFETY: the engine lends `count` frames of room, which we fill
            // from the caller's samples before giving them back.
            unsafe {
                let buffer = self.render.GetBuffer(count)?;
                std::ptr::copy_nonoverlapping(rest.as_ptr(), buffer, length);
                self.render.ReleaseBuffer(count, 0)?;
            }
            rest = &rest[length..];
        }
        Ok(())
    }

    pub fn drain(&mut self) -> Result<()> {
        com();
        if self.stream.padding()? == 0 {
            return Ok(());
        }
        self.stream.start()?;
        let deadline = Instant::now()
            + Duration::from_secs_f64(f64::from(self.queue) / f64::from(self.stream.format.rate))
            + Duration::from_secs(1);
        while self.stream.padding()? > 0 {
            ensure!(Instant::now() < deadline, "the speakers stopped playing");
            self.stream.wait()?;
        }
        self.stream.stop();
        Ok(())
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
}

// SAFETY: as for `Stream`.
unsafe impl Send for Input {}

impl Input {
    pub fn open(
        microphone: Option<&str>,
        _name: &'static str,
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
        })
    }

    pub fn read(&mut self, bytes: &mut [u8]) -> Result<()> {
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
                self.stream.wait()?;
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
        Ok(())
    }
}

#[cfg(feature = "discord")]
pub use others::others;

/// Other applications' sound, through process loopback: one stream of
/// everything the system plays but Yapfast's own processes, from Windows 10's
/// 2004 update on. It can't leave out other applications, so `skip` has no
/// effect here.
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
            Foundation::WAIT_TIMEOUT,
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
    /// How often the capture thread looks for `stop` while nothing plays.
    const STOP_CHECK_MS: u32 = 100;

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
                let stream = activate(yapfast_process())
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
        // For packets the engine hands over at an odd address, which it
        // shouldn't: a copy the size of its buffer, made once.
        let mut unaligned = vec![0f32; stream.buffer as usize * 2];
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
                // SAFETY: the engine lends `count` frames of stereo floats
                // at `data` until ReleaseBuffer.
                unsafe {
                    capture.GetBuffer(&mut data, &mut count, &mut flags, None, None)?;
                    let length = count as usize * 2;
                    // Nothing playing: the mixer hears nothing rather than silence.
                    let silent = flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0 || data.is_null();
                    if !silent && length > 0 {
                        let samples = data.cast::<f32>();
                        let samples: &[f32] = if samples.is_aligned() {
                            std::slice::from_raw_parts(samples, length)
                        } else {
                            let room = unaligned.len();
                            let into = &mut unaligned[..length.min(room)];
                            std::ptr::copy_nonoverlapping(
                                data,
                                into.as_mut_ptr().cast(),
                                size_of_val(into),
                            );
                            into
                        };
                        heard(Heard::Samples { source: 0, samples });
                    }
                    capture.ReleaseBuffer(count)?;
                }
            }
        }
        Ok(())
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
    fn yapfast_process() -> u32 {
        let own = std::process::id();
        // SAFETY: a snapshot this function owns, and entries sized as asked.
        let parent = unsafe {
            let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
                return own;
            };
            let find = |process: u32| {
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
            let parent = find(own).and_then(|this| {
                let parent = find(this.th32ParentProcessID)?;
                let name = |entry: &PROCESSENTRY32W| {
                    let length = entry.szExeFile.iter().position(|&unit| unit == 0);
                    String::from_utf16_lossy(&entry.szExeFile[..length.unwrap_or(260)])
                        .to_lowercase()
                };
                (name(&parent) == name(&this)).then_some(parent.th32ProcessID)
            });
            let _ = CloseHandle(snapshot);
            parent
        };
        parent.unwrap_or(own)
    }
}
