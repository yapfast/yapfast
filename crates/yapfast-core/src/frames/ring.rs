//! `frames.rs`'s ring.

use super::*;

/// The UI's side: it creates the file and shows the newest frame.
pub struct Frames {
    /// What this set of frames is known by, for as long as it lives: the UI
    /// keeps what it opened for a stream beside it, and an address is handed
    /// out again as soon as the process makes a new one — by the allocator
    /// that just freed the old, typically at the same size.
    pub(crate) id: u64,
    /// The file, whose view [`Front::kept`] holds between takes.
    pub(crate) file: File,
    /// The slot the UI read last, which is the one it hands back when it takes
    /// the middle one.
    pub(crate) front: Mutex<Front>,
    pub(crate) name: Mutex<Option<Name>>,
}

/// The next number a set of frames is known by: never one an earlier set had.
pub(crate) static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// What the UI holds between takes: the slot it reads, and nothing of the file
/// itself, which is mapped again for each take.
pub(crate) struct Front {
    pub(crate) slot: u32,
    /// The view of the file this process reads through, kept between takes;
    /// see [`Reading`].
    pub(crate) kept: Option<Reading>,
    /// The socket the worker's pictures arrive on, and the pictures taken out of
    /// it that no take has asked for yet (Linux).
    #[cfg(target_os = "linux")]
    pub(crate) pictures: dmabuf::Reader,
}

/// What keeps the file's name until [`Frames::unlink`].
#[cfg(not(windows))]
pub(crate) type Name = tempfile::TempPath;

#[cfg(windows)]
pub(crate) struct Name {
    pub(crate) path: PathBuf,
    /// Opened to delete the file once every process closes it.
    pub(crate) _file: File,
}

#[cfg(windows)]
impl std::ops::Deref for Name {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.path
    }
}

#[cfg(not(windows))]
pub(crate) fn create_file(directory: PathBuf) -> std::io::Result<(File, Name)> {
    let file = tempfile::Builder::new()
        .prefix("yapfast-frames-")
        .tempfile_in(directory)?;
    Ok(file.into_parts())
}

#[cfg(windows)]
pub(crate) fn create_file(directory: PathBuf) -> std::io::Result<(File, Name)> {
    use std::{os::windows::fs::OpenOptionsExt, sync::atomic::AtomicUsize};

    const FILE_ATTRIBUTE_TEMPORARY: u32 = 0x100;
    const FILE_FLAG_DELETE_ON_CLOSE: u32 = 0x0400_0000;
    static CREATED: AtomicUsize = AtomicUsize::new(0);
    loop {
        let path = directory.join(format!(
            "yapfast-frames-{}-{}",
            std::process::id(),
            CREATED.fetch_add(1, Ordering::Relaxed)
        ));
        // The worker opens it with the standard library's sharing, which lets
        // it be deleted.
        let opened = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .attributes(FILE_ATTRIBUTE_TEMPORARY)
            .custom_flags(FILE_FLAG_DELETE_ON_CLOSE)
            .open(&path);
        match opened {
            Ok(file) => {
                let name = file.try_clone()?;
                return Ok((file, Name { path, _file: name }));
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }
}

impl std::fmt::Debug for Frames {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // A frame being read holds the file, and a log line doesn't wait for
        // it: what it holds is printed when it can be read.
        let capacity = self.front.try_lock().ok().and_then(|mut front| {
            let kept = &mut front.kept;
            let capacity = reading(kept, &self.file).ok().map(|view| view.capacity);
            done_reading(kept, &self.file);
            capacity
        });
        formatter
            .debug_struct("Frames")
            .field("capacity", &capacity)
            .finish_non_exhaustive()
    }
}

impl Frames {
    /// Creates a file for frames up to `width` by `height` pixels.
    pub fn create(width: u32, height: u32) -> Result<Self> {
        let directory = std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .filter(|directory| directory.is_dir())
            .unwrap_or_else(std::env::temp_dir);
        let (file, name) =
            create_file(directory).context("could not create the video frames file")?;
        let capacity = capacity(width, height);
        file.set_len((HEADER + SLOTS as usize * capacity) as u64)
            .context("could not size the video frames file")?;
        {
            // SAFETY: the file was just created and no one else has it yet.
            let mut map =
                unsafe { MmapMut::map_mut(&file) }.context("could not map the video frames")?;
            map[..4].copy_from_slice(&MAGIC.to_ne_bytes());
            map[4..8].copy_from_slice(&u32::try_from(capacity)?.to_ne_bytes());
            // The worker starts with slot 0 and the UI with slot 2.
            map[8..12].copy_from_slice(&1u32.to_ne_bytes());
            // The version of the header this writes, which a reader of another
            // one refuses the file for.
            #[cfg(target_os = "linux")]
            map[12..16].copy_from_slice(&VERSION.to_ne_bytes());
        }
        let mapping = Mapping::open(&file)?;
        // The worker duplicates its shared handles into this process, and its
        // value means nothing anywhere else.
        mapping
            .header()
            .ui_pid
            .store(std::process::id(), Ordering::Release);
        Ok(Self {
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            file,
            front: Mutex::new(Front {
                slot: 2,
                kept: None,
                #[cfg(target_os = "linux")]
                pictures: dmabuf::Reader::default(),
            }),
            name: Mutex::new(Some(name)),
        })
    }

    /// Makes the socket this stream's pictures travel on, on Linux: one end is
    /// kept here, the other is given to the process about to be started for the
    /// worker, and the header names the descriptor that end has so the worker
    /// finds it there. Says which descriptor that is, which the spawn keeps
    /// open across the `exec`; see [`crate::dmabuf`].
    ///
    /// A stream whose socket can't be made is a stream of pixels, which is what
    /// Linux has been until now: nothing an error here costs more than the
    /// pictures that would have crossed it.
    #[cfg(target_os = "linux")]
    pub fn open_pictures(&self) -> Result<u32> {
        let mut front = self
            .front
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let fd = front
            .pictures
            .open()
            .context("could not make the pictures socket")?;
        let fd = u32::try_from(fd)?;
        // The header names it before the process that inherits it starts, so a
        // worker that finds no descriptor there is one whose window made none.
        let kept = &mut front.kept;
        reading(kept, &self.file)?
            .header()
            .socket
            .store(fd, Ordering::Release);
        done_reading(kept, &self.file);
        Ok(fd)
    }

    /// Lets go of this process's copy of the worker's end of that socket, once
    /// its process is running: from here the reading end sees the socket close
    /// when the worker goes, and the frames after it are the frame before it.
    #[cfg(target_os = "linux")]
    pub fn pictures_spawned(&self) {
        self.front
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .pictures
            .spawned();
    }

    /// What this set of frames is known by, for as long as it lives: unique
    /// among the sets this process makes, including ones that have gone.
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Where the worker opens the file, until [`Self::unlink`].
    pub fn path(&self) -> Option<PathBuf> {
        self.name
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .as_deref()
            .map(Path::to_owned)
    }

    /// Removes the file's name once the worker has it open. On Windows the
    /// name stays until both processes close the file.
    pub fn unlink(&self) {
        self.name
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take();
    }

    /// Calls `show` with the newest frame, if one arrived since the last
    /// call: its pixels, or the picture the worker drew on the GPU.
    pub fn take_frame<T>(&self, show: impl FnOnce(Frame<'_>) -> T) -> Option<T> {
        self.take_frame_timed(show).map(|(shown, _)| shown)
    }

    /// As [`Self::take_frame`], with when the frame shows in a video file.
    pub fn take_frame_timed<T>(&self, show: impl FnOnce(Frame<'_>) -> T) -> Option<(T, u64)> {
        let mut guard = self
            .front
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        // Every way out of the take, a frame shown or none, ends with the view
        // let go if the worker asked for the file meanwhile.
        let taken = self.take_with(&mut guard, show);
        done_reading(&mut guard.kept, &self.file);
        taken
    }

    pub(crate) fn take_with<T>(
        &self,
        front: &mut Front,
        show: impl FnOnce(Frame<'_>) -> T,
    ) -> Option<(T, u64)> {
        // What the worker sent over the stream's socket since the last take,
        // taken out of it here whatever this take goes on to show: a picture's
        // descriptors arrive with the message that announces them, and a message
        // nobody reads is memory the window holds for nothing.
        #[cfg(target_os = "linux")]
        front.pictures.drain();
        // The view kept from the take before, or one made now: a file that grew
        // is mapped again, and one the worker wants to itself is let go of after
        // this take, since a view of a file that shrank reads another frame's
        // pixels.
        let reading = match reading(&mut front.kept, &self.file) {
            Ok(reading) => reading,
            Err(error) => {
                // The frames are lost with this view, but a stream's sound and
                // the call go on: the next take maps the file again.
                tracing::warn!(
                    error = format!("{error:#}"),
                    "could not map the video frames"
                );
                return None;
            }
        };
        let held = reading.capacity;
        if reading.header().middle.load(Ordering::Acquire) & FRESH == 0 {
            return None;
        }
        front.slot = reading.header().middle.swap(front.slot, Ordering::AcqRel) & !FRESH;
        // The worker may have grown the file since: the frame just taken was
        // written at a slot size this view doesn't hold, so it's let go for the
        // frame after it, which is read where it was written. A file that
        // shrank can't be seen here: the reader holds it shared while it reads,
        // and the worker shortens it only while it has the file to itself.
        if reading.capacity_now() != held {
            return None;
        }
        let header = reading.header();
        let slot = front.slot;
        let [width, height] = &header.sizes[slot as usize];
        let (width, height) = (
            width.load(Ordering::Relaxed),
            height.load(Ordering::Relaxed),
        );
        let shared = &header.pictures[slot as usize];
        let texture = shared.texture.load(Ordering::Relaxed);
        let generation = shared.generation.load(Ordering::Relaxed);
        // The picture as the header describes it, which is what a slot holds on
        // Windows, and on Linux what a slot holds when nothing describes its
        // memory better: a picture published before there was a socket to send
        // its descriptors on, which the window refuses for what it can't open.
        let described = || Picture {
            luid: header.luid.load(Ordering::Relaxed),
            fence: header.fence.load(Ordering::Relaxed),
            texture,
            fence_value: shared.value.load(Ordering::Relaxed),
            generation,
            set: shared.set.load(Ordering::Relaxed),
            slot,
            width,
            height,
        };
        // A handle is what says a slot holds a picture instead of pixels, and
        // a slot the worker fills with pixels has none: the two can't be taken
        // for each other.
        let frame = if texture == 0 {
            let length = capacity(width, height).min(held);
            // SAFETY: the worker no longer writes this slot: it's the UI's
            // front until the UI swaps it back.
            let pixels = unsafe { std::slice::from_raw_parts(reading.slot(slot), length) };
            Frame::Pixels {
                width,
                height,
                pixels,
            }
        } else {
            #[cfg(target_os = "linux")]
            let frame = match front.pictures.take(slot, generation) {
                Some(picture) => Frame::Dmabuf(picture),
                // A picture whose descriptors were sent and haven't arrived, or
                // whose socket has gone: the frame before it stays up rather
                // than a slot taken for one nothing can show.
                None if texture == PICTURE => return None,
                None => Frame::Gpu(described()),
            };
            #[cfg(not(target_os = "linux"))]
            let frame = Frame::Gpu(described());
            frame
        };
        // The picture the reader took, slot by slot: a picture it never took
        // is one nobody showed and nothing can be sampling, so the worker may
        // draw over it, and one it did take is its own to keep until the
        // rendering queue says it is done with it.
        if texture != 0 {
            shared.took.fetch_max(generation, Ordering::AcqRel);
        }
        let time = header.times[front.slot as usize].load(Ordering::Relaxed);
        Some((show(frame), time))
    }

    /// Calls `show` with the newest frame's width, height and RGBA pixels, if
    /// one arrived since the last call. A picture the worker drew on the GPU
    /// has no pixels in the file, so it's passed over here: a reader that can
    /// show one asks for the frame instead.
    pub fn take<T>(&self, show: impl FnOnce(u32, u32, &[u8]) -> T) -> Option<T> {
        self.take_frame(|frame| match frame {
            Frame::Pixels {
                width,
                height,
                pixels,
            } => Some(show(width, height, pixels)),
            Frame::Gpu(_) => None,
            #[cfg(target_os = "linux")]
            Frame::Dmabuf(_) => None,
        })
        .flatten()
    }

    /// As [`Self::take`], with when the frame shows in a video file.
    pub fn take_timed<T>(&self, show: impl FnOnce(u32, u32, &[u8]) -> T) -> Option<(T, u64)> {
        self.take_frame_timed(|frame| match frame {
            Frame::Pixels {
                width,
                height,
                pixels,
            } => Some(show(width, height, pixels)),
            Frame::Gpu(_) => None,
            #[cfg(target_os = "linux")]
            Frame::Dmabuf(_) => None,
        })
        .and_then(|(shown, time)| shown.map(|shown| (shown, time)))
    }

    /// Says the reader has finished with slot `slot`'s picture `generation`,
    /// which the worker may draw into again from then on: the picture is off
    /// the screen and the rendering queue has passed it, so nothing here reads
    /// that texture any more.
    ///
    /// Taking a picture is not finishing with it. A picture the reader couldn't
    /// show — a fence that didn't arrive in time, a texture it couldn't open —
    /// is still the one on the screen, and the slot it is in is the reader's
    /// until the picture it replaced is off it. Says whether this was the
    /// first time the slot heard it.
    pub fn retire(&self, slot: u32, generation: u64) -> bool {
        self.with_header(|header| {
            let shared = &header.pictures[slot as usize];
            if shared.generation.load(Ordering::Acquire) != generation {
                // A slot the worker has drawn into since is not this picture's
                // to retire: the worker only draws there after a retirement, so
                // this is a retirement of something already finished with.
                return false;
            }
            shared.retired.fetch_max(generation, Ordering::AcqRel) < generation
        })
        .unwrap_or(false)
    }

    /// Runs `use_header` on the header, through the view this process keeps.
    /// Held under `front`, so that one reader uses the view at a time: the lock
    /// it takes is the file's, and two views of one process would share it.
    pub(crate) fn with_header<T>(&self, use_header: impl FnOnce(&Header) -> T) -> Option<T> {
        let mut front = self
            .front
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let kept = &mut front.kept;
        let answer = reading(kept, &self.file)
            .ok()
            .map(|view| use_header(view.header()));
        done_reading(kept, &self.file);
        answer
    }

    /// Says the UI can show this stream's pictures from the GPU, on `luid`,
    /// the adapter its window draws on: the worker publishes them from its
    /// next frame on. Says whether that changed what the header said.
    pub fn accept_gpu(&self, luid: u64) -> bool {
        self.accept_pictures(luid)
    }

    /// As [`Self::accept_gpu`] for the pictures a stream publishes as DMA-BUFs,
    /// which is how a card-drawn picture crosses on Linux: the window takes the
    /// descriptors over the stream's socket and imports them on the device
    /// `luid` names, so the worker knows it can publish them from its next
    /// frame on. Says whether that changed what the header said.
    ///
    /// The answer is the one [`Self::accept_gpu`] writes, because it is the
    /// same answer: a window that can show the one kind of picture can show the
    /// kind its worker publishes. `luid` is the LUID of the adapter on Windows,
    /// and on Linux the number the window's own device is known by — what the
    /// worker compares with its own device before it draws.
    pub fn accept_dmabuf(&self, luid: u64) -> bool {
        self.accept_pictures(luid)
    }

    /// Writes the UI's answer into the header: it can show this stream's
    /// pictures, drawn on `luid`.
    pub(crate) fn accept_pictures(&self, luid: u64) -> bool {
        self.with_header(|header| {
            // The adapter before the answer, so a worker that reads the answer
            // reads the adapter it goes with.
            let moved = header.ui_luid.swap(luid, Ordering::Relaxed) != luid;
            let said = header.ui.swap(UI_USABLE, Ordering::AcqRel) != UI_USABLE;
            said || moved
        })
        .unwrap_or(false)
    }

    /// Says it can't, once: whether a stream's pictures can be shown is a
    /// property of the pair of adapters and the renderer, which don't change
    /// while the stream lasts, so there's nothing to retry. Says whether this
    /// was the first refusal, so the reason is logged once.
    pub fn refuse_gpu(&self) -> bool {
        self.with_header(|header| header.ui.swap(UI_REFUSED, Ordering::AcqRel) != UI_REFUSED)
            .unwrap_or(false)
    }
}
