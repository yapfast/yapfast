/* Showing a picture at the size it's shown: an I420 picture scaled and
 * converted to RGBA through the libyuv inside Yapfast's libwebrtc archive, into
 * buffers the caller keeps, so no frame needs a buffer of its own. */
#include "libyuv/convert_argb.h"
#include "libyuv/scale.h"

#include "turn.h"

/* The same layout as decode.c's. */
struct yapfast_picture {
  int width;
  int height;
  const unsigned char *y;
  const unsigned char *u;
  const unsigned char *v;
  int y_stride;
  int u_stride;
  int v_stride;
};

/* The bytes of the strip a picture `width` wide turns through. */
size_t yapfast_strip_bytes(int width) {
  return (size_t)width * 4 * YAPFAST_STRIP_ROWS;
}

/* Scales `picture` into `scratch`, an I420 buffer of `width` by `height`,
 * unless it's that size already, and converts it to RGBA in `rgba`, turned
 * upright by `rotation` and `flip` (turn.h), `turned_width * 4` bytes a row:
 * `height` by `width` after a quarter turn. A picture that turns goes through
 * `strip`, of yapfast_strip_bytes; one that doesn't needs none. */
void yapfast_i420_to_rgba(const struct yapfast_picture *picture, unsigned char *scratch,
                       unsigned char *strip, unsigned char *rgba, int width, int height,
                       int rotation, int flip) {
  const unsigned char *y = picture->y, *u = picture->u, *v = picture->v;
  int y_stride = picture->y_stride, u_stride = picture->u_stride, v_stride = picture->v_stride;
  if (picture->width != width || picture->height != height) {
    int chroma = (width + 1) / 2;
    unsigned char *sy = scratch;
    unsigned char *su = sy + (size_t)width * height;
    unsigned char *sv = su + (size_t)chroma * ((height + 1) / 2);
    I420Scale(y, y_stride, u, u_stride, v, v_stride, picture->width, picture->height, sy,
              width, su, chroma, sv, chroma, width, height, kFilterBilinear);
    y = sy;
    u = su;
    v = sv;
    y_stride = width;
    u_stride = chroma;
    v_stride = chroma;
  }
  /* libyuv's ABGR is R, G, B, A in memory. */
  if (rotation == 0) {
    I420ToABGR(y, y_stride, u, u_stride, v, v_stride, rgba, width * 4, width,
               flip ? -height : height);
    return;
  }
  for (int first = 0; first < height; first += YAPFAST_STRIP_ROWS) {
    int rows = height - first < YAPFAST_STRIP_ROWS ? height - first : YAPFAST_STRIP_ROWS;
    I420ToABGR(y + (ptrdiff_t)first * y_stride, y_stride, u + (ptrdiff_t)(first / 2) * u_stride,
               u_stride, v + (ptrdiff_t)(first / 2) * v_stride, v_stride, strip, width * 4, width,
               flip ? -rows : rows);
    yapfast_turn_strip(strip, width, height, flip ? height - first - rows : first, rows, rgba,
                       rotation);
  }
}
