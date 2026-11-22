/* Showing a picture at the size it's shown: an I420 picture scaled and
 * converted to RGBA through the libyuv inside Yapfast's libwebrtc archive, into
 * buffers the caller keeps, so no frame needs a buffer of its own. */
#include "libyuv/convert_argb.h"
#include "libyuv/scale.h"

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

/* Scales `picture` into `scratch`, an I420 buffer of `width` by `height`,
 * unless it's that size already, and converts it to RGBA in `rgba`, whose
 * rows are `stride` bytes apart. */
void yapfast_i420_to_rgba(const struct yapfast_picture *picture, unsigned char *scratch,
                       unsigned char *rgba, int stride, int width, int height) {
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
  I420ToABGR(y, y_stride, u, u_stride, v, v_stride, rgba, stride, width, height);
}
