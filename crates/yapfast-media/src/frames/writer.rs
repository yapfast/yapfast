//! `frames.rs`'s writer.

use super::*;

/// The worker's side: it fills frames.
pub struct Writer {
    /// The file the slots are mapped from, kept to lengthen it when a frame
    /// needs more room than it has and to shorten it once the frames need less.
    pub file: File,
    pub mapping: Mapping,
    /// The socket this writer's pictures' descriptors go to the window on, as
    /// this worker inherited it (Linux); nothing while the window made none.
    #[cfg(target_os = "linux")]
    pub pictures: dmabuf::Sender,
    pub back: u32,
    /// How many pictures this writer has published, which numbers them.
    published: u64,
    /// The largest frame written since the frames went small, and when the
    /// first of them was; see [`Self::give_room_back`].
    pub small: Option<(usize, Instant)>,
    /// Since when the reader has held the writer's own slot, which is a
    /// picture nothing is retiring; see [`Self::claim`] and [`HELD_FOR`].
    pub held_since: Option<Instant>,
    /// Whether the reader held that slot for so long that this share's window
    /// writes pixels from now on: a share's pictures belong to one window and
    /// one file, so a window that stopped retiring pictures doesn't come back.
    pub stalled: bool,
    /// How long a slot is given before it counts as one nothing will let go,
    /// which a test sets so it doesn't wait; a stream leaves [`HELD_FOR`].
    pub held_for: Duration,
}

impl Writer {
    pub fn open(path: &Path) -> Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .context("could not open the video frames file")?;
        let mapping = Mapping::open(&file)?;
        // The socket the window made for this stream's pictures, as this
        // process inherited it: the header names the descriptor, and a header
        // that names none — or names one that isn't this stream's socket — is a
        // stream whose pictures go out as pixels.
        #[cfg(target_os = "linux")]
        let pictures = dmabuf::Sender::adopt(
            mapping.header().socket.load(Ordering::Acquire),
            mapping.header().ui_pid.load(Ordering::Acquire),
        );
        Ok(Self {
            mapping,
            file,
            #[cfg(target_os = "linux")]
            pictures,
            back: 0,
            published: 0,
            small: None,
            held_since: None,
            stalled: false,
            held_for: HELD_FOR,
        })
    }
    /// The UI's process, which the worker duplicates its shared handles into
    /// before [`Self::publish_into`] takes them: a handle's value means nothing
    /// outside the process that took it, so the ones the UI opens are the
    /// ones duplicated there. Zero while the file can't be mapped, which reads
    /// as no UI at all.
    pub fn ui_process(&mut self) -> u32 {
        if !self.mapping.mapped(&self.file) {
            return 0;
        }
        self.mapping.header().ui_pid.load(Ordering::Acquire)
    }

    /// The adapter the UI's window draws on, when it has said its pictures can
    /// be shown there: what a stream reports so a share that copies every frame
    /// can say whether the two adapters are why. `None` while it hasn't, and
    /// while the file can't be mapped.
    pub fn presented_on(&mut self) -> Option<u64> {
        if !self.mapping.mapped(&self.file) {
            return None;
        }
        let header = self.mapping.header();
        (header.ui.load(Ordering::Acquire) == UI_USABLE)
            .then(|| header.ui_luid.load(Ordering::Acquire))
    }

    /// The device the window shows this stream's DMA-BUF pictures on, once it
    /// has said it can: what a worker asks before it exports a picture at all,
    /// so that a window with no importer — or one that refused them — is not
    /// drawn for. `None` while the window hasn't answered, while it refused
    /// them, and while the file can't be mapped.
    ///
    /// The number is [`Self::presented_on`]'s: on Linux it is what the window's
    /// own Vulkan device is known by, and the worker compares it with its own
    /// device before it publishes, as it compares LUIDs on Windows.
    pub fn dmabuf_showable(&mut self) -> Option<u64> {
        self.presented_on()
    }

    /// The largest frame, in pixels, that fits.
    pub fn fits(&self, width: u32, height: u32) -> bool {
        capacity(width, height) <= self.mapping.capacity
    }

    /// Makes room for frames of `width` by `height`, growing the file and the
    /// slots in it when they hold smaller ones: a stream shown larger than
    /// the file was made for grows it once, instead of being scaled down to
    /// what it holds.
    pub fn reserve(&mut self, width: u32, height: u32) -> Result<()> {
        let wanted = capacity(width, height);
        self.mapping.grow(&self.file, wanted)?;
        self.give_room_back(wanted, ROOM_BACK_AFTER);
        Ok(())
    }

    /// Gives the file's room back when the frames have needed far less of it
    /// for long enough: a share that went full-screen once and came back to a
    /// tile holds three slots of four-K pixels, about a hundred megabytes, it
    /// never writes again.
    ///
    /// A frame that wants more than a quarter of the room is not small, and
    /// starts the wait over; one that wants less starts it, and the largest of
    /// the frames in that wait is the size the file settles at, so a frame of
    /// the size it settled at doesn't grow it again. `waited` is how long the
    /// frames have to have stayed small, which a stream leaves to
    /// [`ROOM_BACK_AFTER`]. The file is held to this writer while it is
    /// shortened, by taking the lock every reader holds for as long as it has
    /// the file mapped: the room goes back only when nobody is reading, and a
    /// reader that is keeps its view whole.
    pub fn give_room_back(&mut self, wanted: usize, waited: Duration) {
        let held = self.mapping.capacity;
        if wanted == 0 || wanted.saturating_mul(ROOM_SLACK) > held {
            self.small = None;
            return;
        }
        let (largest, since) = match self.small {
            Some((largest, since)) => (largest.max(wanted), since),
            None => (wanted, Instant::now()),
        };
        self.small = Some((largest, since));
        if since.elapsed() < waited {
            return;
        }
        // Readers keep their view between takes, so the file is asked for first:
        // a reader lets its view go after the take it is in once it sees this.
        self.mapping.header().release.store(1, Ordering::Release);
        match self.file.try_lock() {
            Ok(()) => {}
            // A reader is still in a take, or hasn't taken since it was asked:
            // the next frame asks again, and the wait doesn't start over.
            Err(TryLockError::WouldBlock) => return,
            // A file system that can't lock the file is one no reader is
            // holding for the writer either, so nothing here is certain enough
            // to end a file over.
            Err(TryLockError::Error(error)) => {
                tracing::debug!(
                    error = format!("{error:#}"),
                    "the video frames keep their room"
                );
                self.mapping.header().release.store(0, Ordering::Release);
                self.small = Some((largest, Instant::now()));
                return;
            }
        }
        let shortened = self.mapping.shorten(&self.file, largest);
        // Whatever the shortening did, readers may keep their views again. A
        // shortening that left no view says so the next time a frame needs one,
        // and its header is the file's either way.
        if self.mapping.map.is_some() {
            self.mapping.header().release.store(0, Ordering::Release);
        }
        let _ = self.file.unlock();
        self.small = None;
        match shortened {
            Ok(true) => tracing::debug!(
                bytes = largest * SLOTS as usize,
                "the video frames gave their room back"
            ),
            Ok(false) => {}
            Err(error) => tracing::warn!(
                error = format!("{error:#}"),
                "could not give the video frames' room back"
            ),
        }
    }

    /// Writes a frame with `fill`, which gets its RGBA pixels, then shows it.
    pub fn write(&mut self, width: u32, height: u32, fill: impl FnOnce(&mut [u8])) {
        self.write_timed(width, height, 0, fill);
    }

    /// As [`Self::write`], for a frame of a video file showing at `time_ms`.
    pub fn write_timed(
        &mut self,
        width: u32,
        height: u32,
        time_ms: u64,
        fill: impl FnOnce(&mut [u8]),
    ) {
        self.write_timed_rendered(width, height, time_ms, |pixels| {
            fill(pixels);
            true
        });
    }
    /// Writes a frame with `fill`, which gets its RGBA pixels and says whether
    /// it filled them, and shows it only if it did: a picture that couldn't be
    /// made leaves the frame before it showing, rather than a slot half of one
    /// frame and half of another.
    pub fn write_rendered(
        &mut self,
        width: u32,
        height: u32,
        fill: impl FnOnce(&mut [u8]) -> bool,
    ) -> bool {
        self.write_timed_rendered(width, height, 0, fill)
    }
    /// As [`Self::write_rendered`], for a frame showing at `time_ms`.
    pub fn write_timed_rendered(
        &mut self,
        width: u32,
        height: u32,
        time_ms: u64,
        fill: impl FnOnce(&mut [u8]) -> bool,
    ) -> bool {
        // A frame larger than the slots grows them, as one shown in a larger
        // tile or window is; one that can't grow them is skipped, leaving the
        // frame before it showing rather than writing over the slots.
        if let Err(error) = self.reserve(width, height) {
            tracing::warn!(
                error = format!("{error:#}"),
                "could not grow the video frames"
            );
            return false;
        }
        // SAFETY: the back slot is the worker's alone until it's swapped in.
        let pixels = unsafe {
            std::slice::from_raw_parts_mut(self.mapping.slot(self.back), capacity(width, height))
        };
        if !fill(pixels) {
            return false;
        }
        let header = self.mapping.header();
        // A slot that held a picture the worker drew on the GPU holds pixels
        // now, and its handle goes first: no reader takes this frame to be
        // the texture of the one before it.
        header.pictures[self.back as usize]
            .texture
            .store(0, Ordering::Relaxed);
        let [w, h] = &header.sizes[self.back as usize];
        w.store(width, Ordering::Relaxed);
        h.store(height, Ordering::Relaxed);
        header.times[self.back as usize].store(time_ms, Ordering::Relaxed);
        self.back = header.middle.swap(self.back | FRESH, Ordering::AcqRel) & !FRESH;
        true
    }

    /// The slot a GPU picture goes in next, when the reader has finished with
    /// the one there: `None` means the UI still holds what that slot holds, and
    /// this frame's preview is dropped rather than drawn over a picture
    /// somebody is looking at. A slot that never held a picture is always free.
    ///
    /// It is the reader's own record that answers, a slot at a time: a picture
    /// taken is a picture on the screen until the next one replaces it and the
    /// rendering queue has drawn that, which can be a frame or more after the
    /// take — so only the reader knows, and it says so with [`Frames::retire`].
    /// A *newer* take of another slot is not that answer: the window may still
    /// be sampling the older picture's texture, so it stays the reader's.
    ///
    /// The answer is always the writer's own back slot, and never another: with
    /// three slots, the one that is neither the middle nor the reader's front is
    /// the writer's, and drawing anywhere else would paint over the slot the
    /// reader is reading or is about to be handed. The three stay apart because
    /// each side only ever swaps its own slot with the middle (see
    /// [`Self::publish_into`]), so the back is never the middle nor the front.
    ///
    /// A slot held longer than [`HELD_FOR`] is one nothing is retiring, and
    /// answering `None` for it forever would stop the ring turning for the rest
    /// of the share: this writer gives the picture path up instead, so the
    /// frames that follow are pixels — which touch no texture and swap into the
    /// middle as ever — and [`Self::given_up`] says so. A file the writer can't
    /// map has no slot either, and is mapped again by the claim after it.
    pub fn claim(&mut self) -> Option<u32> {
        if self.stalled || !self.mapping.mapped(&self.file) {
            return None;
        }
        let header = self.mapping.header();
        debug_assert_ne!(
            self.back,
            header.middle.load(Ordering::Acquire) & !FRESH,
            "the writer's back is never the slot the reader is handed next"
        );
        let shared = &header.pictures[self.back as usize];
        let generation = shared.generation.load(Ordering::Acquire);
        // Two things free the slot: a picture nobody has taken — nothing shows
        // it and nothing can be sampling it, whatever `retired` says — and a
        // picture the reader took and has finished with, which `retired` says.
        // A picture it took and hasn't finished with is on the screen, and a
        // newer take of another slot says nothing about it.
        let took = shared.took.load(Ordering::Acquire);
        let retired = shared.retired.load(Ordering::Acquire);
        if retired >= generation || took < generation {
            self.held_since = None;
            return Some(self.back);
        }
        let held = *self.held_since.get_or_insert_with(Instant::now);
        if held.elapsed() < self.held_for {
            return None;
        }
        self.stalled = true;
        tracing::info!(
            slot = self.back,
            "the window hasn't finished with a picture slot for a second; this share writes pixels from now on"
        );
        None
    }

    /// Whether the reader held a picture slot so long that this writer gave the
    /// picture path up: the frames this share shows are pixels from then on,
    /// which is what a window that stopped retiring pictures leaves it.
    pub fn given_up(&self) -> bool {
        self.stalled
    }

    /// Publishes a picture drawn on the GPU into the slot [`Self::claim`]
    /// answered, and says whether it did: `false` means the UI can't show such
    /// pictures — it looked and said so, or it draws on another adapter — and
    /// the caller writes the frame's pixels instead.
    ///
    /// The worker must, before calling this:
    /// * draw into a `DXGI_FORMAT_B8G8R8A8_UNORM` texture of the picture's
    ///   size, one mip, one sample, made with
    ///   `D3D11_RESOURCE_MISC_SHARED_NTHANDLE | D3D11_RESOURCE_MISC_SHARED_KEYEDMUTEX`
    ///   (the second is what Direct3D 11 requires of a shared NT handle) and,
    ///   through `IDXGIResource1::CreateSharedHandle`, one handle to it;
    /// * make one shared fence for the stream,
    ///   `ID3D11Device5::CreateFence(0, D3D11_FENCE_FLAG_SHARED, IID_ID3D12Fence)`,
    ///   and one handle to it;
    /// * duplicate both handles into the UI's process, [`Self::ui_process`],
    ///   and keep the values they have there: those are what this takes;
    /// * after drawing the picture, signal the fence with a value that only
    ///   grows, and pass that value here. The GPU has not finished the
    ///   drawing when this returns, so the value is what says it did, and the
    ///   UI shows nothing until the fence reaches it.
    ///
    /// The slots still hold a frame's size, and a file whose slots are too
    /// small for this picture is not grown: a GPU picture isn't in the file,
    /// and the pixel path above grows the file for itself when it writes one.
    ///
    /// A picture of a set older than the one this slot holds is refused, `false`
    /// and nothing written: the number a set is known by only grows, and a
    /// reader that has opened the newer one would take a handle of the older for
    /// one of its own.
    ///
    /// It goes into the writer's own back slot, which is the one
    /// [`Self::claim`] answered and no other: that slot is never the middle and
    /// never the reader's front, whatever the reader did with the middle since.
    /// The reader's swap of the middle may land between the claim and here,
    /// which is why nothing is decided by the middle the claim was made
    /// against — the swap below takes the middle as it is at that moment, and
    /// `back` is the writer's alone until then.
    pub fn publish_into(&mut self, drawn: &Drawn) -> bool {
        if !self.mapping.mapped(&self.file) {
            return false;
        }
        let slot = self.back;
        let header = self.mapping.header();
        // Nothing said yet is not an answer — the worker may publish before
        // the UI has had a picture to look at — but a refusal is, and so is
        // an adapter the UI doesn't draw on.
        match header.ui.load(Ordering::Acquire) {
            UI_UNSET => {}
            UI_USABLE if header.ui_luid.load(Ordering::Relaxed) == drawn.luid => {}
            _ => return false,
        }
        let shared = &header.pictures[slot as usize];
        if drawn.texture.set < shared.set.load(Ordering::Acquire) {
            return false;
        }
        header.luid.store(drawn.luid, Ordering::Relaxed);
        header.fence.store(drawn.fence, Ordering::Relaxed);
        self.published += 1;
        // The set before the handle that is of it, so a reader never pairs a
        // picture with the number of another set.
        shared.set.store(drawn.texture.set, Ordering::Relaxed);
        shared
            .texture
            .store(drawn.texture.handle, Ordering::Relaxed);
        shared.value.store(drawn.fence_value, Ordering::Relaxed);
        shared.generation.store(self.published, Ordering::Relaxed);
        let [w, h] = &header.sizes[slot as usize];
        w.store(drawn.width, Ordering::Relaxed);
        h.store(drawn.height, Ordering::Relaxed);
        // A stream's pictures have no time in a file: only a video's frames
        // do, and a video's frames are pixels.
        header.times[slot as usize].store(0, Ordering::Relaxed);
        self.back = header.middle.swap(slot | FRESH, Ordering::AcqRel) & !FRESH;
        true
    }

    /// Publishes a picture the worker drew into a DMA-BUF into the slot
    /// [`Self::claim`] answered, and says whether it did: `false` means the
    /// window has said it can't show such pictures, the socket is gone, or the
    /// descriptors didn't match the planes, and the caller writes the frame's
    /// pixels instead.
    ///
    /// The worker must, before calling this:
    /// * have the picture in memory that can be shared as file descriptors — a
    ///   card's surface exported as a DMA-BUF, or a buffer a PipeWire stream
    ///   filled — and keep it as long as the window may still be sampling it,
    ///   which is until it retires the slot;
    /// * have finished writing it. There is no fence on this path, unlike the
    ///   shared texture's: the driver's own implicit fence travels with the
    ///   descriptors, and is what an importer waits on;
    /// * have given the same number of descriptors as planes, one per plane.
    ///
    /// The descriptors go to the window over the stream's socket, one message a
    /// picture, before the header says the slot holds it: a reader takes a slot
    /// for a picture only when it has that picture's message, and a message for
    /// a generation the header hasn't announced waits for the take that asks
    /// for it. The slot is the writer's own back, as on the shared texture's
    /// path, and the reader's [`Frames::retire`] is what gives it back.
    #[cfg(target_os = "linux")]
    pub fn publish_dmabuf(&mut self, picture: &dmabuf::Descriptor<'_>) -> bool {
        if !self.mapping.mapped(&self.file) {
            return false;
        }
        let slot = self.back;
        let header = self.mapping.header();
        // Nothing said yet is not an answer — see [`Self::publish_into`] — but a
        // refusal is, and it is the window saying it can't show these at all.
        if header.ui.load(Ordering::Acquire) == UI_REFUSED {
            return false;
        }
        self.published += 1;
        if !self.pictures.send(slot, self.published, picture) {
            return false;
        }
        let shared = &header.pictures[slot as usize];
        // The fields a shared texture's handle and fence would be in say nothing
        // on this path: the memory the picture is in is named by the descriptors
        // that went over the socket.
        shared.value.store(0, Ordering::Relaxed);
        shared.set.store(0, Ordering::Relaxed);
        shared.texture.store(PICTURE, Ordering::Relaxed);
        shared.generation.store(self.published, Ordering::Relaxed);
        let [w, h] = &header.sizes[slot as usize];
        w.store(picture.width, Ordering::Relaxed);
        h.store(picture.height, Ordering::Relaxed);
        // A stream's pictures have no time in a file.
        header.times[slot as usize].store(0, Ordering::Relaxed);
        self.back = header.middle.swap(slot | FRESH, Ordering::AcqRel) & !FRESH;
        true
    }
}
