/* Decoding a watched stream with oneVPL (MFX), Intel's own decoder API: the
 * third shim beside native/hwdec_vaapi.h and native/hwdec_cuda.h, and the same
 * bargain — whole frames in, and a picture left on the card as a DMA-BUF the
 * tile pass imports, so no pixel comes back to the processor.
 *
 * VA-API is what an Intel machine normally answers first, and this is what is
 * left when it doesn't: an iHD driver that won't initialise, a build whose libva
 * is missing from the path, a graphics generation whose VA driver is broken
 * while the card's own oneVPL engine works. The surfaces are VA surfaces all the
 * same — that is what oneVPL allocates on a VA display — so the export is the
 * same one the VA-API shim does.
 *
 * Plain C values cross here, so that decode.rs needs none of the driver's types
 * and this file needs none of Rust's. */
#ifndef YAPFAST_HWDEC_MFX_H
#define YAPFAST_HWDEC_MFX_H

#include <stddef.h>
#include <stdint.h>

/* The codecs this shim decodes, numbered as decode.rs numbers them. */
enum {
  YAPFAST_MFX_H264 = 0,
  YAPFAST_MFX_HEVC = 1,
  YAPFAST_MFX_AV1 = 2,
};

/* The planes the tile pass reads: NV12's luma and its interleaved chroma, the
 * one shape this shim exports. */
enum { YAPFAST_MFX_PLANES = 2 };

struct yapfast_mfxdec;

/* One plane of a picture's DMA-BUF: the buffer holding it, where the plane
 * starts in that buffer, how far apart its rows are, and the buffer's size. */
struct yapfast_mfxplane {
  int fd;
  uint32_t offset;
  uint32_t stride;
  uint64_t size;
  /* The modifier the memory was laid out with, as DRM PRIME 2 reports it. */
  uint64_t modifier;
};

/* The newest picture as a DMA-BUF. */
struct yapfast_mfxpicture {
  int width;
  int height;
  /* The buffer's format, as DRM names it: NV12. */
  uint32_t fourcc;
  uint32_t planes;
  struct yapfast_mfxplane plane[YAPFAST_MFX_PLANES];
};

/* What decoding moved, for a log to quote: pictures the card decoded, pictures
 * exported where they lay, and bytes of a picture that crossed to the processor,
 * which is none of them. */
struct yapfast_mfxfacts {
  unsigned long long decoded;
  unsigned long long exported;
  unsigned long long copied;
  unsigned long long processor_bytes;
};

/* A decoder for `codec` on the machine's Intel card, through oneVPL's own
 * dispatcher. Null when there is no such decoder — no oneVPL, no VA display, no
 * card, a codec it doesn't take — and the reason is yapfast_mfxdec_error(NULL).
 *
 * The card's decoder itself is made when the stream's first sequence header says
 * how large its pictures are, so no size is asked for here. */
struct yapfast_mfxdec *yapfast_mfxdec_new(int codec);
/* Feeds one whole frame and exports the newest picture as a DMA-BUF: 1 says one
 * is ready, 0 that the decoder kept the frame for the pictures after it, and a
 * negative number that the frame wasn't decoded. A picture exported by the call
 * before is released here. */
int yapfast_mfxdec_decode(struct yapfast_mfxdec *decoder, const unsigned char *data, size_t size,
                          int keyframe);
/* The newest picture's descriptor, as yapfast_mfxdec_decode left it. */
int yapfast_mfxdec_picture(struct yapfast_mfxdec *decoder, struct yapfast_mfxpicture *picture);
/* Says the caller is done with the picture yapfast_mfxdec_picture described: the
 * surface goes back to the decoder's pool and the descriptor is closed. */
void yapfast_mfxdec_release(struct yapfast_mfxdec *decoder);
/* What this decoder has moved so far; see struct yapfast_mfxfacts. */
void yapfast_mfxdec_facts(const struct yapfast_mfxdec *decoder, struct yapfast_mfxfacts *facts);
/* Why the last call failed, for a log to quote. */
const char *yapfast_mfxdec_error(const struct yapfast_mfxdec *decoder);
void yapfast_mfxdec_free(struct yapfast_mfxdec *decoder);

#endif /* YAPFAST_HWDEC_MFX_H */
