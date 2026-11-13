/* Showing a picture at the size it's shown, and sharing one at the size it's
 * sent: an I420 picture scaled, and converted to RGBA, through the libyuv inside
 * Yapfast's libwebrtc archive, into buffers the caller keeps, so no frame needs
 * a buffer of its own. */
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

/* Converts `argb` into the I420 planes given, `width` by `height`, every channel
 * of every pixel taken through `tone` on the way: 256 values, what each channel
 * has on an SDR screen.
 *
 * A capture of a desktop showing HDR hands its frames over as 8-bit codes with
 * nothing saying what they mean: libwebrtc's DesktopFrame is four bytes a pixel
 * whatever the screen shows, and no colour space comes with it. The table is
 * where the meaning is, and this is where it is read. It costs one pass over the
 * pixels and no copy of the frame to hold the tonemapped values, because
 * turning a frame into I420 reads every pixel anyway.
 *
 * The matrix is libyuv's own: BT.601's over the full-range pixels a screen
 * hands over, to the limited-range Y, U and V that the encoders are told they
 * get. */
void yapfast_tone_argb_to_i420(const unsigned char *argb, int stride, const unsigned char *tone,
                               unsigned char *y, int y_stride, unsigned char *u, int u_stride,
                               unsigned char *v, int v_stride, int width, int height) {
  int chroma = (width + 1) / 2;
  for (int row = 0; row < height; row += 2) {
    const unsigned char *top = argb + (size_t)row * stride;
    /* A frame with an odd number of rows ends on this row: the pair below is
     * the row itself, and its average is its own. */
    const unsigned char *bottom = row + 1 < height ? top + stride : top;
    unsigned char *top_y = y + (size_t)row * y_stride;
    for (int col = 0; col < width; ++col) {
      const unsigned char *pixel = top + (size_t)col * 4;
      int b = tone[pixel[0]], g = tone[pixel[1]], r = tone[pixel[2]];
      top_y[col] = (unsigned char)(((66 * r + 129 * g + 25 * b + 128) >> 8) + 16);
    }
    if (row + 1 < height) {
      unsigned char *bottom_y = top_y + y_stride;
      for (int col = 0; col < width; ++col) {
        const unsigned char *pixel = bottom + (size_t)col * 4;
        int b = tone[pixel[0]], g = tone[pixel[1]], r = tone[pixel[2]];
        bottom_y[col] = (unsigned char)(((66 * r + 129 * g + 25 * b + 128) >> 8) + 16);
      }
    }
    /* Chroma comes of this pair of rows: each 2 by 2 block of pixels averaged,
     * or the pixel beside it where the frame is that narrow, then BT.601's
     * matrix over the average. */
    unsigned char *row_u = u + (size_t)(row / 2) * u_stride;
    unsigned char *row_v = v + (size_t)(row / 2) * v_stride;
    for (int col = 0; col < chroma; ++col) {
      int x = col * 2, beside = x + 1 < width ? 1 : 0;
      const unsigned char *a = top + (size_t)x * 4, *b = a + (size_t)beside * 4;
      const unsigned char *c = bottom + (size_t)x * 4, *d = c + (size_t)beside * 4;
      int blue = (tone[a[0]] + tone[b[0]] + tone[c[0]] + tone[d[0]] + 2) >> 2;
      int green = (tone[a[1]] + tone[b[1]] + tone[c[1]] + tone[d[1]] + 2) >> 2;
      int red = (tone[a[2]] + tone[b[2]] + tone[c[2]] + tone[d[2]] + 2) >> 2;
      row_u[col] = (unsigned char)(((-38 * red - 74 * green + 112 * blue + 128) >> 8) + 128);
      row_v[col] = (unsigned char)(((112 * red - 94 * green - 18 * blue + 128) >> 8) + 128);
    }
  }
}

/* Scales `picture` into the I420 planes given, of `width` by `height`: a shared
 * frame's picture, in the buffer kept for the frames that follow it. */
void yapfast_i420_scale(const struct yapfast_picture *picture, unsigned char *y, int y_stride,
                        unsigned char *u, int u_stride, unsigned char *v, int v_stride, int width,
                        int height) {
  I420Scale(picture->y, picture->y_stride, picture->u, picture->u_stride, picture->v,
            picture->v_stride, picture->width, picture->height, y, y_stride, u, u_stride, v,
            v_stride, width, height, kFilterBilinear);
}
