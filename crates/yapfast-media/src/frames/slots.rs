//! `frames.rs`'s slots.

use super::*;

/// One slot's picture, when the worker drew it on the GPU instead of writing
/// its pixels.
#[repr(C)]
pub struct Shared {
    /// The shared handle of the texture holding the picture, as it is in the
    /// UI's process, or zero for a slot of pixels: this is what tells a
    /// reader which of the two a slot holds.
    pub texture: AtomicU64,
    /// The value the fence reaches once the picture is drawn, which a reader
    /// waits for before showing what the texture holds.
    pub value: AtomicU64,
    /// The number of the picture, which only grows: a reader compares it with
    /// the one it showed to tell a new picture from the one before it.
    pub generation: AtomicU64,
    /// The number of the set of textures and fence this slot's picture is of,
    /// which only grows within the slot: a set is three textures and one fence,
    /// rebuilt when a stream's pictures change size or a share is stopped and
    /// started, and a reader that has opened a picture of one tells the sets
    /// apart by this rather than by a handle, since Windows hands a closed
    /// handle's value out again.
    ///
    /// It belongs to the slot rather than to the file because a slot is read
    /// only with the picture the worker published in it: the set the header
    /// held while the reader read a slot another picture was published into
    /// would pair this slot's handle with the number of a set it is not of.
    pub set: AtomicU64,
    /// The newest generation of this slot the reader has finished with, which
    /// only grows and is the reader's alone to write: a worker draws into the
    /// slot again only once this is the generation that is there. Zero until
    /// a reader has finished with one, and a generation of zero — a slot that
    /// never held a picture — is the worker's from the start.
    pub retired: AtomicU64,
    /// The newest generation of this slot the reader has taken, which only
    /// grows and is the reader's alone to write.
    ///
    /// It answers one thing, whether the picture in the slot has been taken at
    /// all. A generation newer than this is one nobody took: nothing shows it
    /// and nothing can be sampling it, so the slot is the worker's whatever
    /// [`Self::retired`] says. A generation this has reached is a picture on
    /// the screen until `retired` reaches it too, and a newer take of another
    /// slot says nothing about this one.
    pub took: AtomicU64,
}

/// A slot's picture: the pixels the worker wrote into the file, or one it
/// drew on the GPU and shared, which the UI shows without copying.
pub enum Frame<'a> {
    /// The worker's pixels, `width` by `height` RGBA bytes, in a slot the UI
    /// holds until its next take.
    Pixels {
        width: u32,
        height: u32,
        pixels: &'a [u8],
    },
    /// A picture in a texture the worker shares, and how to know it's drawn.
    Gpu(Picture),
    /// A picture in a DMA-BUF the worker shared as descriptors, which is how a
    /// GPU picture crosses on Linux: the memory is the worker's, and the
    /// descriptors that name this process's part of it are closed when the
    /// picture is dropped, which is as the slot is retired.
    #[cfg(target_os = "linux")]
    Dmabuf(dmabuf::Picture),
}

/// A picture the worker drew on the GPU, as its slot's header describes it.
/// Everything here is a handle or a number the UI can check without opening
/// anything, which is what makes a picture it can't show cheap to refuse.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Picture {
    /// The adapter the worker draws on, as its DXGI adapter's LUID, low half
    /// first: Direct3D opens a shared texture on the adapter that made it and
    /// on no other, so this decides whether the picture can be shown before
    /// anything is opened.
    pub luid: u64,
    /// The shared handle of the fence the picture is drawn behind, as it is
    /// in the UI's process.
    pub fence: u64,
    /// The shared handle of the texture holding the picture, as it is in the
    /// UI's process: the worker duplicated it there.
    pub texture: u64,
    /// The value the fence reaches when the picture is drawn.
    pub fence_value: u64,
    /// The number of the picture, which only grows.
    pub generation: u64,
    /// The number of the set of textures and fence it was drawn in, which only
    /// grows: a picture of a set the reader hasn't opened is a worker that
    /// rebuilt them, and what was opened for the set before it goes. A handle
    /// can't say that, because its value is handed out again once it's closed.
    pub set: u64,
    /// The slot it is in, which [`Frames::retire`] names it by: a picture's
    /// generation says which picture of that slot it is, and the two together
    /// are what the worker keeps a slot for.
    pub slot: u32,
    /// The picture's size, in pixels.
    pub width: u32,
    pub height: u32,
}

/// The handle of a texture shared into the UI's process, and the number of the
/// set of textures and fence it was shared in.
///
/// The two travel together because a handle's value names a texture only in the
/// process that holds it, and Windows gives a value out again once the handle
/// that had it is closed: a picture that carried the handle alone could be taken
/// for one of the set after it, and its reader would show the wrong texture or
/// open a number that is no texture at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SharedTexture {
    /// The handle, as the UI's process holds it.
    pub handle: u64,
    /// The number of the set it was duplicated in, which only grows.
    pub set: u64,
}

/// What a worker hands [`Writer::publish`]: the picture it drew, without the
/// number, which publishing gives it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Drawn {
    /// The adapter it drew on, as the LUID of its DXGI adapter, low half
    /// first; the UI opens the texture only on that adapter.
    pub luid: u64,
    /// The shared handle of the fence it signals once the picture is drawn,
    /// as it is in the UI's process.
    pub fence: u64,
    /// The shared handle of the texture holding the picture, as it is in the
    /// UI's process, with the number of the set it was shared in.
    pub texture: SharedTexture,
    /// The value that fence reaches when the picture is drawn: it must only
    /// grow, and the worker must signal it after the drawing, before it
    /// publishes.
    pub fence_value: u64,
    /// The picture's size, which the texture must have: Direct3D 12 trusts
    /// it, and a texture smaller than this is read past its end.
    pub width: u32,
    pub height: u32,
}

/// The worker's view of the file: the slots it writes, kept between frames and
/// let go for as long as the file is being made smaller.
pub struct Mapping {
    /// The view, or none while the file is shortened: a file can't be ended
    /// while any view of it exists, this one included.
    pub map: Option<MmapMut>,
    pub capacity: usize,
    /// How many mappings to make fail, so a test can drive the path a file
    /// system's refusal takes without one. A failed mapping must leave the
    /// worker its slots.
    #[cfg(test)]
    pub fail_remap: u32,
    /// Whether a refused mapping has been logged already, so a file that stays
    /// unmappable doesn't fill the log at frame rate.
    pub complained: bool,
}

impl Mapping {
    pub fn open(file: &File) -> Result<Self> {
        // SAFETY: only Yapfast's own processes open the file, and they touch the
        // slots through the triple buffer, never the same slot at once.
        let map = unsafe { MmapMut::map_mut(file) }.context("could not map the video frames")?;
        ensure!(map.len() >= HEADER, "the video frames file is too short");
        let mut mapping = Self {
            map: Some(map),
            capacity: 0,
            #[cfg(test)]
            fail_remap: 0,
            complained: false,
        };
        ensure!(mapping.header().magic == MAGIC, "not a video frames file");
        #[cfg(target_os = "linux")]
        ensure!(
            mapping.header().version.load(Ordering::Acquire) == VERSION,
            "not this version of a video frames file"
        );
        let capacity = mapping.capacity_now();
        ensure!(
            mapping.view().len() >= HEADER + SLOTS as usize * capacity,
            "the video frames file is too short"
        );
        mapping.capacity = capacity;
        Ok(mapping)
    }

    /// The view itself: every use of a mapping is a use of the slots in it, and
    /// the only time there is none is between letting the view go and mapping
    /// the shortened file.
    pub fn view(&self) -> &MmapMut {
        self.map.as_ref().expect("the slots are mapped")
    }

    /// The bytes a slot holds, as the header says: frames larger than those
    /// that wrote it moved every slot, and this mapping with them.
    pub fn capacity_now(&self) -> usize {
        self.header().capacity.load(Ordering::Acquire) as usize
    }

    /// Maps the file again at `length`, which is what a resize needs: a file
    /// can't be ended or lengthened under the view it already has. A failure
    /// leaves whatever view there was, and the slots where they were.
    pub fn remap(&mut self, file: &File, length: usize) -> Result<()> {
        #[cfg(test)]
        if self.fail_remap > 0 {
            self.fail_remap -= 1;
            return Err(anyhow::anyhow!("the video frames can't be mapped again"));
        }
        // SAFETY: as `open`.
        let map = unsafe { MmapMut::map_mut(file) }.context("could not map the video frames")?;
        ensure!(map.len() >= length, "the video frames file is too short");
        self.map = Some(map);
        Ok(())
    }

    /// Maps the file again when a shortening left no view, taking the capacity
    /// the file really holds: the header's number is trusted only as far as the
    /// file backs it, so a file that was shortened and not mapped again still
    /// gives the worker slots to write in rather than a panic.
    pub fn remap_any(&mut self, file: &File) -> Result<()> {
        self.remap(file, HEADER)?;
        let held = (self.view().len() - HEADER) / SLOTS as usize;
        let said = self.capacity_now();
        self.capacity = said.min(held);
        if self.capacity != said {
            // Every slot is where the file puts it now, so the header says the
            // stride the slots really have, and the frame that moved with the
            // room is not read at the old one.
            let header = self.header();
            header
                .capacity
                .store(u32::try_from(self.capacity)?, Ordering::Release);
            header.middle.fetch_and(!FRESH, Ordering::AcqRel);
        }
        // The shortening that left no view is over: readers may keep theirs.
        self.header().release.store(0, Ordering::Release);
        Ok(())
    }

    /// Grows the file and this mapping to hold slots of `capacity` bytes,
    /// when they hold smaller ones, and says so in the header. Only growing
    /// keeps a mapping another process holds whole: the file it read stays
    /// where it was, and the header is what tells it to map again.
    pub fn grow(&mut self, file: &File, capacity: usize) -> Result<()> {
        if self.map.is_none() {
            // A shortening that couldn't map the file again left no view: map
            // it where it is now before asking it for more room.
            self.remap_any(file)?;
        }
        if capacity <= self.capacity {
            return Ok(());
        }
        let length = HEADER + SLOTS as usize * capacity;
        file.set_len(length as u64)
            .context("could not size the video frames file")?;
        self.remap(file, length)?;
        self.capacity = capacity;
        self.header()
            .capacity
            .store(u32::try_from(capacity)?, Ordering::Release);
        Ok(())
    }

    /// Maps the file again when a shortening left no view, and says whether
    /// there is one now: the paths that read the header ask this first, so a
    /// mapping the file system refuses answers "no slot" rather than taking
    /// the worker's thread down with it.
    pub fn mapped(&mut self, file: &File) -> bool {
        if self.map.is_some() {
            return true;
        }
        match self.remap_any(file) {
            Ok(()) => true,
            Err(error) => {
                // Once per mapping, not once per frame: this is asked at frame
                // rate, and a file that can't be mapped would fill the log.
                if !self.complained {
                    self.complained = true;
                    tracing::warn!(
                        error = format!("{error:#}"),
                        "could not map the video frames again"
                    );
                }
                false
            }
        }
    }

    /// Makes the slots `capacity` bytes, and this view with them, when they
    /// hold more of the file than the frames need: the room a share used when
    /// it was shown large goes back to the machine rather than being held for
    /// the stream's life.
    ///
    /// The caller holds the file to itself for the whole of this — every reader
    /// takes the file shared for as long as it has a view of it — so no view but
    /// this one exists, which is what makes ending the file safe rather than
    /// merely possible: Windows won't end a file any process has a view of, and
    /// a view of a file that shrank reads past its end. Answers whether the file
    /// gave its room back; a length the file system won't end keeps its room and
    /// loses no frame.
    pub fn shorten(&mut self, file: &File, capacity: usize) -> Result<bool> {
        if capacity >= self.capacity {
            return Ok(false);
        }
        let old_length = HEADER + SLOTS as usize * self.capacity;
        // This view goes first: it is a view of the file like any other, and
        // one of them is enough to keep the file its length.
        self.map = None;
        let length = HEADER + SLOTS as usize * capacity;
        if let Err(error) = file.set_len(length as u64) {
            tracing::debug!(
                error = format!("{error:#}"),
                "the video frames keep their room"
            );
            self.remap(file, old_length)?;
            return Ok(false);
        }
        // The room is gone from the file; mapping it again is what makes the
        // slots smaller. A mapping that fails here must not leave the writer
        // without slots, so the room goes back on the file and it is mapped at
        // it: no room given back rather than no frames. A file that can't be
        // mapped even at its old length is mapped again by `grow` before the
        // next frame, which skips that frame rather than panicking.
        if let Err(error) = self.remap(file, length) {
            if file.set_len(old_length as u64).is_ok() && self.remap(file, old_length).is_ok() {
                return Ok(false);
            }
            self.capacity = 0;
            return Err(error);
        }
        self.capacity = capacity;
        let header = self.header();
        header
            .capacity
            .store(u32::try_from(capacity)?, Ordering::Release);
        // Every slot moved with the file, so a frame the reader hasn't taken is
        // no longer where its size says it is: the freshness goes with the room,
        // and the next frame written is the first read at the new stride.
        header.middle.fetch_and(!FRESH, Ordering::AcqRel);
        Ok(true)
    }

    pub fn header(&self) -> &Header {
        header_of(self.view())
    }

    pub fn slot(&self, index: u32) -> *mut u8 {
        slot_of(self.view(), self.capacity, index)
    }
}

/// A reader's view of the file, kept from take to take and let go when the
/// worker asks for the file, with the shared lock that goes with it: the worker
/// shortens the file only while it can hold the file to itself, and it asks by
/// raising [`Header::release`] and then taking the lock. The lock is the open
/// file's, which every view in this process would share, so the process keeps
/// one view, in [`Front`], and lets it go only through [`Self::let_go`].
pub struct Reading {
    /// Whether the file is held shared for as long as this view exists. A file
    /// system that can't lock the file is one where the room isn't given back
    /// either — the worker's own attempt fails there too — so a refusal is not
    /// a reason to stop showing frames.
    pub locked: bool,
    pub map: MmapMut,
    /// The bytes a slot held when this view was made, which is the stride its
    /// slots are read at.
    pub capacity: usize,
}

impl Reading {
    /// Maps the file for reading: the lock comes first and the view after it,
    /// so a view never exists while the worker could be shortening the file.
    pub fn open(file: &File) -> Result<Self> {
        // A lock error is not a reason to stop showing frames: it is the same
        // answer the writer gets when it asks, so nothing shortens this file.
        let locked = file.lock_shared().is_ok();
        // SAFETY: only Yapfast's own processes map the file, and they touch the
        // slots through the triple buffer, never the same slot at once.
        let map = match unsafe { MmapMut::map_mut(file) } {
            Ok(map) => map,
            Err(error) => {
                if locked {
                    let _ = file.unlock();
                }
                return Err(error).context("could not map the video frames");
            }
        };
        let mut reading = Self {
            locked,
            map,
            capacity: 0,
        };
        let checked = (|| {
            ensure!(
                reading.map.len() >= HEADER,
                "the video frames file is too short"
            );
            ensure!(reading.header().magic == MAGIC, "not a video frames file");
            #[cfg(target_os = "linux")]
            ensure!(
                reading.header().version.load(Ordering::Acquire) == VERSION,
                "not this version of a video frames file"
            );
            reading.capacity = reading.capacity_now();
            ensure!(
                reading.map.len() >= HEADER + SLOTS as usize * reading.capacity,
                "the video frames file is too short"
            );
            Ok(())
        })();
        // A file that isn't the frames file is let go of like one that is.
        if let Err(error) = checked {
            reading.let_go(file);
            return Err(error);
        }
        Ok(reading)
    }

    /// The view goes before the lock: the worker shortens the file only while
    /// nothing has a view of it, and a lock let go with the view still up would
    /// say the file was the worker's to shorten.
    pub fn let_go(self, file: &File) {
        let locked = self.locked;
        drop(self.map);
        if locked {
            let _ = file.unlock();
        }
    }

    pub fn header(&self) -> &Header {
        header_of(&self.map)
    }

    pub fn capacity_now(&self) -> usize {
        self.header().capacity.load(Ordering::Acquire) as usize
    }

    /// Whether the worker wants the file to itself.
    pub fn asked_for(&self) -> bool {
        self.header().release.load(Ordering::Acquire) != 0
    }

    pub fn slot(&self, index: u32) -> *mut u8 {
        slot_of(&self.map, self.capacity, index)
    }
}

/// The view `kept` holds, made now if there is none or the one there is no
/// longer fits: the worker grew the file since, or wants it to itself.
pub fn reading<'a>(kept: &'a mut Option<Reading>, file: &File) -> Result<&'a Reading> {
    if let Some(view) = kept.as_ref()
        && (view.asked_for() || view.capacity_now() != view.capacity)
    {
        kept.take().expect("a view was kept").let_go(file);
    }
    if kept.is_none() {
        *kept = Some(Reading::open(file)?);
    }
    Ok(kept.as_ref().expect("a view was just made"))
}

/// After a use of the view: let go of it when the worker has asked for the file,
/// so it gets it before the next take rather than waiting on a view nobody is
/// reading.
pub fn done_reading(kept: &mut Option<Reading>, file: &File) {
    if kept.as_ref().is_some_and(Reading::asked_for) {
        kept.take().expect("a view was kept").let_go(file);
    }
}
