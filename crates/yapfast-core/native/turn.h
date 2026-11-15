/* Turning a picture upright while it's converted to RGBA, a strip of rows at
 * a time: the strip stays in the processor's cache while libyuv turns it into
 * its place in the caller's picture, so no second picture is ever held. */
#ifndef YAPFAST_TURN_H
#define YAPFAST_TURN_H
#include <stddef.h>
#include <stdint.h>

#include "libyuv/rotate_argb.h"

/* The rows converted at a time: even, as 4:2:0 shares a row of colour between
 * two of the picture, and a multiple of four, which libyuv's quarter turn
 * wants for its fast path. */
enum { YAPFAST_STRIP_ROWS = 16 };

/* How a picture turns upright: flipped top to bottom first, with `flip`,
 * then turned `rotation` degrees clockwise, 0, 90, 180 or 270. Those eight
 * are every way a picture can lie. One with no rotation needs no strip:
 * libyuv converts bottom up when it's given a negative height. */
struct yapfast_turn {
  int rotation;
  int flip;
};

/* Whether the turned picture is as wide as the stored one is tall. */
static inline int yapfast_turn_sideways(struct yapfast_turn turn) {
  return turn.rotation == 90 || turn.rotation == 270;
}

/* Turns `strip`, the `rows` rows from `first` down of a picture `width` by
 * `height`, into where they belong in `rgba`, the whole picture turned by
 * `rotation`, which isn't 0. With a flip, the caller converts the strip
 * bottom up and counts `first` from the picture's bottom. */
static inline void yapfast_turn_strip(const uint8_t *strip, int width, int height, int first,
                                      int rows, uint8_t *rgba, int rotation) {
  int stride = (rotation == 180 ? width : height) * 4;
  uint8_t *to = rotation == 180  ? rgba + (size_t)(height - first - rows) * (size_t)stride
                : rotation == 90 ? rgba + (size_t)(height - first - rows) * 4
                                 : rgba + (size_t)first * 4;
  ARGBRotate(strip, width * 4, to, stride, width, rows, (enum RotationMode)rotation);
}

#endif
