/* Decoding a watched stream with NVDEC, the NVIDIA card's answer to what
 * native/hwdec_vaapi.h does for AMD's and Intel's: the same whole frames in,
 * and a picture left on the card as a DMA-BUF the tile pass imports, so no
 * pixel of it comes back to the processor.
 *
 * Plain C values cross here, so that decode.rs needs none of the driver's
 * types, and this file needs none of Rust's. */
#ifndef YAPFAST_HWDEC_CUDA_H
#define YAPFAST_HWDEC_CUDA_H

#include <stddef.h>
#include <stdint.h>

/* The codecs this shim decodes, numbered as decode.rs numbers them: the three
 * Discord's streams are. */
enum {
  YAPFAST_CU_H264 = 0,
  YAPFAST_CU_HEVC = 1,
  YAPFAST_CU_AV1 = 2,
};

/* The planes the tile pass reads: NV12 is luma and one interleaved chroma
 * plane, the one shape this shim exports, and the only one it decodes. */
enum { YAPFAST_CU_PLANES = 2 };

struct yapfast_cudec;

/* One plane of a picture's DMA-BUF, as native/hwdec_vaapi.h describes one: the
 * buffer holding it, where the plane starts in that buffer, how far apart its
 * rows are, and the buffer's size.
 *
 * Both planes are in one buffer here, since that is what an NVDEC surface and
 * the shim's own copy of it are, so the two entries name the same descriptor. */
struct yapfast_cuplane {
  int fd;
  uint32_t offset;
  uint32_t stride;
  uint64_t size;
  /* The modifier the memory was laid out with: linear, which is the layout
   * both a decoded surface and the shim's copy have. */
  uint64_t modifier;
};

/* The newest picture as a DMA-BUF. */
struct yapfast_cupicture {
  /* The visible picture, which is smaller than the padded surface it lies in
   * where the codec codes in whole macroblocks. */
  int width;
  int height;
  /* The buffer's format, as DRM names it: NV12, the one shape the tile pass
   * shows, and the only one this shim exports. */
  uint32_t fourcc;
  uint32_t planes;
  struct yapfast_cuplane plane[YAPFAST_CU_PLANES];
};

/* What decoding moved, for a log to quote: how many pictures the card decoded,
 * how many of them it exported where it decoded them, how many were copied into
 * the shim's own buffer on the card, and how many bytes of a picture crossed to
 * the processor, which is none of them. */
struct yapfast_cucounts {
  unsigned long long pictures;
  unsigned long long exported;
  unsigned long long copied;
  unsigned long long processor_bytes;
};

/* A decoder for `codec` on the machine's NVIDIA card — device 0, which
 * CUDA_VISIBLE_DEVICES names another of. Null when this build or this machine
 * has no such decoder: no driver, no libnvcuvid, or a card too old for the
 * codec, and the reason is yapfast_cuvadec_error(NULL).
 *
 * The card's decoder itself is made when the stream's first sequence header
 * says how large its pictures are, so no size is asked for here. */
struct yapfast_cudec *yapfast_cuvadec_new(int codec);
/* Feeds one whole frame, and exports the newest picture as a DMA-BUF: 1 says one
 * is ready, 0 that the decoder kept the frame for the pictures after it, -2 that
 * the stream's pictures are ones the tile pass can't show (anything but 8-bit
 * 4:2:0), and any other negative number that the frame wasn't decoded. A
 * picture exported by the call before is released here. */
int yapfast_cuvadec_decode(struct yapfast_cudec *decoder, const unsigned char *data, size_t size,
                           int keyframe);
/* The newest picture's descriptor, as yapfast_cuvadec_decode left it. 0 when
 * there is one, and the reason in the decoder's error when there isn't. */
int yapfast_cuvadec_picture(struct yapfast_cudec *decoder, struct yapfast_cupicture *picture);
/* Says the caller is done with the picture yapfast_cuvadec_picture described. A
 * picture the driver exported where it decoded it holds that surface against the
 * decoder until here, and its descriptor is closed; a picture the shim copied
 * onto the card lies in a buffer of the shim's own, whose descriptor outlives
 * the picture and is closed when the picture's size changes or the decoder is
 * freed. Asking twice asks about a picture that is already gone. */
void yapfast_cuvadec_release(struct yapfast_cudec *decoder);
/* What this decoder has moved so far; see struct yapfast_cucounts. */
void yapfast_cuvadec_counts(const struct yapfast_cudec *decoder,
                            struct yapfast_cucounts *counts);
/* Why the last call failed, for a log to quote. */
const char *yapfast_cuvadec_error(const struct yapfast_cudec *decoder);
void yapfast_cuvadec_free(struct yapfast_cudec *decoder);

#endif /* YAPFAST_HWDEC_CUDA_H */
