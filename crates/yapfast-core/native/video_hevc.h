/* Only plain values cross between libwebrtc's FFmpeg and the separately
 * built, symbol-prefixed HEVC decoder. Their FFmpeg structs are not ABI compatible. */
#ifndef YAPFAST_VIDEO_HEVC_H
#define YAPFAST_VIDEO_HEVC_H
#include <stdint.h>

struct yapfast_file_hevc;
struct yapfast_hevc_frame {
  int width, height, depth;
  unsigned x_shift, y_shift;
  int bt709, full_range;
  const uint8_t *data[3];
  int stride[3];
  int64_t timestamp;
};

struct yapfast_file_hevc *yapfast_file_hevc_new(unsigned threads, const uint8_t *extra, int size);
void yapfast_file_hevc_free(struct yapfast_file_hevc *decoder);
/* NULL data drains delayed pictures. Times are in the demuxer's stream units. */
int yapfast_file_hevc_send(struct yapfast_file_hevc *decoder, const uint8_t *data, int size,
                           int64_t pts, int64_t dts, int64_t duration);
/* 1 for a picture, 0 for more input or end, negative for an error.
 * Planes remain valid until the next receive or flush. */
int yapfast_file_hevc_receive(struct yapfast_file_hevc *decoder, struct yapfast_hevc_frame *frame);
void yapfast_file_hevc_flush(struct yapfast_file_hevc *decoder);
#endif
