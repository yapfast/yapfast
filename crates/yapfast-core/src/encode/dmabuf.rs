//! `encode.rs`'s dmabuf.

/// A picture in a DMA-BUF, as native/hwenc_dmabuf.c reads it: one DRM object
/// holding an NV12 picture, which is the shape a VA-API surface is made of
/// without a conversion. Laid out as its `struct yapfast_dmabuf_picture`.
#[cfg(target_os = "linux")]
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct DmabufPicture {
    pub(crate) width: c_int,
    pub(crate) height: c_int,
    /// The one descriptor the picture is in, and how much of it the picture
    /// needs: a descriptor carries no size of its own.
    pub(crate) fd: c_int,
    pub(crate) size: u64,
    /// The DRM modifier the driver laid the buffer out with.
    pub(crate) modifier: u64,
    /// The buffer's format, as DRM names it, and how many planes of it this
    /// names: NV12's two, or the one of a 32-bit picture.
    pub(crate) fourcc: u32,
    pub(crate) planes: u32,
    /// Where each plane starts in that buffer, and how many bytes apart its
    /// rows are.
    pub(crate) offset: [u32; 2],
    pub(crate) pitch: [u32; 2],
}

/// The VA-API encoder that reads the card's own pictures: a session with its
/// own surfaces, which FFmpeg imports a DMA-BUF into for each picture. It runs
/// beside the encoder that reads the processor's memory while both may be
/// needed, which is why a stream whose pictures are all the card's own gives
/// that one up ([`Encoder::give_up_planes`]).
#[cfg(dmabufenc)]
pub(crate) struct Dmabuf {
    pub(crate) raw: *mut Raw,
    /// Pictures this encoder answered late, dropped rather than sent under the
    /// next picture's time, as last read, so that a driver falling behind is
    /// warned about once for each.
    pub(crate) late: u32,
}

/// The card's video processor's arguments for a stream's pictures, and the shape
/// they were made for. A screen doesn't change size or format under a stream, so
/// the text is made once and kept rather than for each picture; it is C text
/// because that is how it crosses to native/hwenc_dmabuf.c.
#[cfg(all(target_os = "linux", dmabufenc, vaapivpp))]
pub(crate) struct Vpp {
    /// The arrived picture's format, width and height the arguments were made
    /// for: a picture of another shape is a graph of its own.
    pub(crate) from: (u32, u32, u32),
    /// The processor's own options, `scale_vaapi`'s.
    pub(crate) args: std::ffi::CString,
}

#[cfg(dmabufenc)]
impl Drop for Dmabuf {
    fn drop(&mut self) {
        // SAFETY: raw came from yapfast_hwenc_dmabuf_new and is freed once.
        unsafe { yapfast_hwenc_dmabuf_free(self.raw) };
    }
}
