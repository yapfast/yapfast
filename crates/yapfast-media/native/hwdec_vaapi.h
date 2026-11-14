/* FFmpeg's own VA-API decoder for a watched stream, the Linux twin of
 * hwdec.h: an access unit in, and a picture left in a VA surface on the
 * graphics card, exported as a DMA-BUF the tile pass imports. Plain C values
 * cross here, so that this file's callers need none of FFmpeg's headers. */
#ifndef YAPFAST_HWDEC_VAAPI_H
#define YAPFAST_HWDEC_VAAPI_H

#include <stddef.h>
#include <stdint.h>

/* The codecs this shim decodes, numbered as decode.rs numbers them, and as
 * hwdec.h numbers them: the three Discord's streams are. */
enum {
  YAPFAST_VA_H264 = 0,
  YAPFAST_VA_HEVC = 1,
  YAPFAST_VA_AV1 = 2,
};

/* The most planes a picture's DMA-BUF is exported with, which is DRM's own
 * limit; the pictures the tile pass shows have two. */
enum { YAPFAST_VA_PLANES = 4 };

/* How a picture's samples became colour, as the tile pass reads it: the range
 * its codes use, the matrix luma and chroma are combined with, and the curve
 * they are in. The numbers are decode.rs's and the tile pass's, so they are
 * written down here: a picture's colour is three of these. */
enum {
  /* 16..235 luma and 16..240 chroma, as every 8-bit stream Discord carries is,
   * or the full range of the sample's own depth. */
  YAPFAST_VA_LIMITED = 0,
  YAPFAST_VA_FULL = 1,
};
enum {
  YAPFAST_VA_BT709 = 0,
  /* BT.2020's non-constant-luminance matrix, which HDR10 is coded with. */
  YAPFAST_VA_BT2020_NCL = 1,
};
enum {
  /* A display-referred curve: BT.709's for 8-bit, BT.1886's gamma for 10-bit. */
  YAPFAST_VA_SDR = 0,
  /* SMPTE ST 2084, which HDR10 is coded with. */
  YAPFAST_VA_PQ = 1,
};

/* The colour signal a picture was coded with, in the numbers above. */
struct yapfast_vacolour {
  int range;
  int matrix;
  int transfer;
};

struct yapfast_vadec;

/* One plane of the picture's buffer: which buffer, where the plane starts in
 * it, how far apart its rows are, and the buffer's size and format modifier.
 * `fd` is the decoder's, borrowed — it is open until yapfast_vadec_release. */
struct yapfast_vaplane {
  int fd;
  uint32_t offset;
  uint32_t stride;
  uint64_t size;
  uint64_t modifier;
};

/* The newest picture as a DMA-BUF: one buffer's format, the planes the tile
 * pass reads, and the descriptor holding them. The fds are borrowed from the
 * decoder, and stay open until yapfast_vadec_release. */
struct yapfast_vapicture {
  /* The visible picture, which is smaller than the padded surface it lies in
   * where the codec codes in whole macroblocks. */
  int width;
  int height;
  /* The buffer's format, as DRM names it: NV12 for an 8-bit picture, P010 for
   * a 10-bit one — the two shapes the tile pass shows, both 4:2:0. */
  uint32_t fourcc;
  uint32_t planes;
  struct yapfast_vaplane plane[YAPFAST_VA_PLANES];
  /* The matrix, range and curve the samples are in, which the tile pass needs
   * to turn them into the colour that was coded. */
  struct yapfast_vacolour colour;
};

/* A decoder for `codec` that leaves its pictures on the graphics card, on the
 * VA-API device `device` names — a render node such as /dev/dri/renderD128 —
 * or, for NULL, the one FFmpeg finds by trying each render node the machine
 * has, with the libva driver LIBVA_DRIVER_NAME names, if it names one. The
 * size is only a hint until the stream's first frame says its own. Null when
 * this build or this machine has no decoder for the codec, and the reason is
 * yapfast_vadec_error(NULL). */
struct yapfast_vadec *yapfast_vadec_new(int codec, int hint_width, int hint_height,
                                        const char *device);
/* Feeds one whole frame, and exports the newest picture as a DMA-BUF: 1 says
 * one is ready, 0 that the decoder kept the frame for the pictures after it,
 * -2 that the stream's pictures are ones the tile pass can't show (anything
 * but 4:2:0 at 8 or 10 bits, and a 10-bit picture whose colour signal the
 * pass has no curve for), and any other negative number that the frame wasn't
 * decoded. A picture exported by the call before is released here. */
int yapfast_vadec_decode(struct yapfast_vadec *decoder, const unsigned char *data, size_t size,
                         int keyframe);
/* The newest picture's descriptor, as yapfast_vadec_decode left it. 0 when
 * there is one, and the reason in the decoder's error when there isn't. */
int yapfast_vadec_picture(struct yapfast_vadec *decoder, struct yapfast_vapicture *picture);
/* Says the caller is done with the picture yapfast_vadec_picture described:
 * its fds are closed here, once. Asking twice is asking about a picture that
 * is already gone, and closing nothing twice. */
void yapfast_vadec_release(struct yapfast_vadec *decoder);
/* Why the last call failed, for a log to quote. */
const char *yapfast_vadec_error(const struct yapfast_vadec *decoder);
void yapfast_vadec_free(struct yapfast_vadec *decoder);

/* yapfast_vadec_picture's descriptor, read from a DRM PRIME frame's
 * AVDRMFrameDescriptor — which is what `drm` points at — without a decoder or
 * a driver: a test builds the descriptor a driver would hand over, and this
 * says whether the tile pass could import it and why not. 0 when it could, and
 * `error` says why when it couldn't. */
int yapfast_vadec_describe(const void *drm, struct yapfast_vapicture *picture, char *error,
                           size_t error_size);

#endif /* YAPFAST_HWDEC_VAAPI_H */
