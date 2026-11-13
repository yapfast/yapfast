/* FFmpeg's own Direct3D 11 decoder for a watched stream, the half of the path
 * that native/d3d11dec.cc doesn't own: a picture arrives in an array texture
 * on the device the caller hands over, and the tile pass in d3d11dec.cc turns
 * a slice of it into the tile. Plain C values cross here, so that the decoder
 * needs none of d3d11dec.cc's Direct3D types, and d3d11dec.cc needs none of
 * FFmpeg's headers.
 *
 * Included plainly by hwdec.c, and inside `extern "C"` by d3d11dec.cc: the
 * one C++ file that includes it, which is why the declarations below carry no
 * language guard of their own. */
#ifndef YAPFAST_HWDEC_H
#define YAPFAST_HWDEC_H

#include <stddef.h>

/* The codecs this shim decodes, numbered as decode.rs numbers them: the three
 * Discord's streams are. */
enum {
  YAPFAST_FF_H264 = 0,
  YAPFAST_FF_HEVC = 1,
  YAPFAST_FF_AV1 = 2,
};

struct yapfast_ffdec;

/* The newest picture: where it lies, and the colour the stream said it is in.
 * `texture` is borrowed from the decoder — the caller takes a reference of its
 * own to keep it past the next decode. */
struct yapfast_ffpicture {
  /* The ID3D11Texture2D the picture is in, and the array slice of it. */
  void *texture;
  unsigned index;
  /* The visible picture, which is smaller than the padded surface it lies in
   * where the codec codes in whole macroblocks. */
  int width;
  int height;
  /* The stream's colour, read from the frame that carried it. */
  int full_range;
  int bt709;
  int bt2020;
  int pq;
};

/* A decoder for `codec` that leaves its pictures in `device`, an
 * ID3D11Device the caller keeps alive; the size is only a hint until the
 * stream's first frame says its own. Null when this build has no decoder for
 * the codec, or the card won't decode on that device, and the reason is
 * yapfast_ffdec_error(NULL). */
struct yapfast_ffdec *yapfast_ffdec_new(int codec, int hint_width, int hint_height, void *device);
/* Feeds one whole frame. 1 says a picture is ready, 0 that the decoder kept
 * the frame for the pictures after it, -2 that the stream's pictures are ones
 * the tile pass can't show (anything but 8-bit 4:2:0), and any other negative
 * number that the frame wasn't decoded. */
int yapfast_ffdec_decode(struct yapfast_ffdec *decoder, const unsigned char *data, size_t size,
                         int keyframe);
/* The newest picture, as yapfast_ffdec_decode left it. 0 when there is one. */
int yapfast_ffdec_picture(struct yapfast_ffdec *decoder, struct yapfast_ffpicture *picture);
/* Why the last call failed, for a log to quote. */
const char *yapfast_ffdec_error(const struct yapfast_ffdec *decoder);
void yapfast_ffdec_free(struct yapfast_ffdec *decoder);

#endif /* YAPFAST_HWDEC_H */
