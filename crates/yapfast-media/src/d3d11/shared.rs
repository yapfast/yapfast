//! `d3d11.rs`'s shared.

use super::*;

/// A picture drawn on the card for the UI's window to show where it lies: three
/// tile-sized BGRA textures and one fence on an encoder's device, sharing the
/// handles of all four into the UI's process.
///
/// The UI's window draws on a Direct3D 12 device of its own and opens the handle
/// of the texture a picture was published under, so what crosses the process
/// boundary is a handle and two numbers rather than the tile's pixels. Three
/// textures, one per slot of the frames file, so the one the UI is showing is
/// never the one being drawn, and the fence value that comes back with each
/// picture is what says it is drawn: see [`crate::frames::Drawn`], which is what
/// a caller publishes.
///
/// It is a set, and it has a number that only grows: a stream that changes size,
/// or stops and starts again, is another three textures and another fence, and
/// the UI lets go of the ones it opened for the set before. A handle's value is
/// no use for that, since Windows hands a closed handle's value out again.
///
/// It holds textures on its encoder's device, so it goes before the encoder it
/// draws for, and it is worth nothing without the UI's process being alive:
/// every failure here is a stream that writes pixels instead.
pub struct Shared {
    pub(crate) raw: *mut RawShared,
    pub(crate) size: (u32, u32),
    pub(crate) luid: u64,
    pub(crate) fence: u64,
    pub(crate) set: u64,
}

// SAFETY: the textures and the fence are the device's, and every call that
// draws takes the share by `&mut self` while the handle shared out is a number.
unsafe impl Send for Shared {}

impl Shared {
    /// Three pictures of `width` by `height` and one fence on `tile`'s device,
    /// with the handles of all four duplicated into `ui`, the process that opens
    /// them. Answers why not where the card or the UI's process won't have them:
    /// a caller that gets an error writes the tile's pixels as it always did.
    ///
    /// A set that can't be finished leaves nothing behind: the handles it
    /// duplicated into the UI's process so far are closed there before this
    /// answers, since no picture is ever published of a set that wasn't made.
    pub fn new(tile: &D3d11Encoder, width: u32, height: u32, ui: u32) -> Result<Self> {
        let (mut fence, mut luid) = (0, 0);
        let mut why = [0u8; 192];
        // SAFETY: raw is an open encoder, and the out-pointers are two locals and
        // a buffer of the size given.
        let raw = unsafe {
            yapfast_d3d11enc_shared_new(
                tile.raw,
                width,
                height,
                ui,
                &mut fence,
                &mut luid,
                why.as_mut_ptr().cast(),
                why.len(),
            )
        };
        if raw.is_null() {
            bail!(
                "a {width}x{height} picture couldn't be shared with the UI: {}",
                c_text(&why)
            );
        }
        Ok(Self {
            raw,
            size: (width, height),
            luid,
            fence,
            // SAFETY: raw is the set that was just made, and it lives as long as
            // this does.
            set: unsafe { yapfast_d3d11enc_shared_set(raw) },
        })
    }

    /// The size of each picture, which is the size it is drawn at and the size
    /// the UI opens it at.
    pub fn size(&self) -> (u32, u32) {
        self.size
    }

    /// The adapter the pictures were made on, as the UI's window compares its
    /// own device against.
    pub fn luid(&self) -> u64 {
        self.luid
    }

    /// The fence's handle as the UI's process holds it: one fence for a stream's
    /// pictures, waited on by the value each of them is published with.
    pub fn fence(&self) -> u64 {
        self.fence
    }

    /// The number of this set of pictures and fence, which only grows: the UI
    /// tells a set it has already opened from the one after it by this, and not
    /// by a handle, since a handle's value is given out again once it is closed.
    pub fn set(&self) -> u64 {
        self.set
    }

    /// Scales `texture` into shared picture `at` on the GPU, in one pass
    /// through the driver's video processor or its shader, and answers that
    /// picture's handle as the UI's process holds it with the fence value that
    /// says it is drawn. The handle travels with the number of the set it is of,
    /// which is what the UI keys the pictures it has opened by.
    ///
    /// The caller names the picture because the frames file's slot decides
    /// which one may be drawn into: a slot is only drawn into once its reader
    /// has moved past the picture there, so a picture a person is looking at
    /// is never written over.
    ///
    /// The value is signalled after the scale is queued and nothing here waits
    /// for it: the UI does, and so does the next draw into the same picture.
    ///
    /// `texture` carries the same requirement as it does for
    /// [`D3d11Encoder::scale`]: one this tile's device can read, and its lease
    /// alive.
    pub fn draw(
        &mut self,
        at: u32,
        tile: &mut D3d11Encoder,
        texture: Texture<'_>,
    ) -> Result<(SharedTexture, u64)> {
        let (mut handle, mut value) = (0, 0);
        // SAFETY: raw and the encoder are both open, the texture is the caller's
        // and outlives the call, and the out-pointers are two locals.
        let drawn = unsafe {
            yapfast_d3d11enc_shared_draw(
                tile.raw,
                self.raw,
                at,
                texture.handle,
                texture.subresource,
                &mut handle,
                &mut value,
            )
        };
        if drawn <= 0 {
            bail!(
                "{:?} couldn't draw a {}x{} picture for the UI: {}",
                tile.codec,
                self.size.0,
                self.size.1,
                tile.error()
            );
        }
        Ok((
            SharedTexture {
                handle,
                set: self.set,
            },
            value,
        ))
    }

    /// The picture the last draw wrote, as a texture on the encoder's device:
    /// what a test reads back to see the tile the UI would show, and what the
    /// stream never needs, since it publishes the handle instead. It is this
    /// share's own texture, drawn into again by the next draw, so it is good
    /// only while the share is — the lease is the borrow of this set, which also
    /// keeps a draw from writing over a picture somebody is still reading.
    pub fn picture(&self) -> Texture<'_> {
        // SAFETY: raw is an open share, and the texture it names lives as long as
        // it does.
        Texture::of(unsafe { yapfast_d3d11enc_shared_picture(self.raw) }, self)
    }
}

impl Drop for Shared {
    fn drop(&mut self) {
        // SAFETY: raw came from the shim's constructor and is freed once. The
        // handles duplicated into the UI's process are that process's to close as
        // it opens them, and the shim closes the ones it duplicated there and
        // nothing opened before it lets the set go.
        unsafe { yapfast_d3d11enc_shared_free(self.raw) };
    }
}

/// The frames a [`D3d11Encoder`] reads before a capture source fills its
/// textures: one BGRA texture on the encoder's device, which a compute shader
/// refills there, so nothing of a frame crosses the processor and a measurement
/// of the encode is the encode's own cost.
///
/// It holds a texture on its encoder's device, so it goes before the encoder it
/// draws for: a texture whose device has been released can't be read.
pub struct Source {
    pub(crate) raw: *mut RawSource,
}

impl Source {
    /// Refills the texture on the GPU with the picture `frame` shows.
    pub fn draw(&self, frame: u32) {
        // SAFETY: raw is a source that hasn't been freed, and the frame index is
        // plain data.
        unsafe { yapfast_d3d11enc_source_draw(self.raw, frame) };
    }

    /// The texture to encode next, which the source keeps and refills: it is the
    /// source's own, good only while the source is, and its device is the one the
    /// source was made on. The lease is the borrow of this source, so a texture
    /// held past it, or past the encoder it draws for, is a compile error.
    pub fn texture(&self) -> Texture<'_> {
        // SAFETY: raw is a source that hasn't been freed, and its texture lives
        // as long as it does.
        Texture::of(unsafe { yapfast_d3d11enc_source_texture(self.raw) }, self)
    }
    /// Draws four solid quadrants instead of the moving picture: one known colour
    /// in each corner of the texture, which is the picture a scaler is held to.
    pub fn quads(&self) -> Result<()> {
        let mut why = [0u8; 192];
        // SAFETY: raw is a source that hasn't been freed, and a buffer of the size
        // given, which the shim ends with a zero.
        let drew =
            unsafe { yapfast_d3d11enc_source_quads(self.raw, why.as_mut_ptr().cast(), why.len()) };
        if drew != 0 {
            bail!("no quadrants to draw: {}", c_text(&why));
        }
        Ok(())
    }
}

impl Drop for Source {
    fn drop(&mut self) {
        // SAFETY: raw came from the shim's constructor and is freed once.
        unsafe { yapfast_d3d11enc_source_free(self.raw) };
    }
}
