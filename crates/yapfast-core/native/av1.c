/* Decoding AV1 someone streams through Go Live, with the dav1d inside
 * Yapfast's libwebrtc archive, which video.c plays AV1 files with. The same
 * interface as decode.c: whole temporal units in, I420 planes out, valid
 * until the next call on the decoder. A unit's last OBU may give no size,
 * as DAVE leaves it; dav1d reads it to the end of the unit. */
#include <errno.h>
#include <stdlib.h>
#include <string.h>

#include "dav1d/dav1d.h"

/* The widest or tallest picture a stream may send, as in decode.c. */
enum { MAX_SIDE = 8192 };

struct yapfast_av1 {
  Dav1dContext *context;
  Dav1dPicture picture;
  int holds;
};

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

void yapfast_av1_free(struct yapfast_av1 *decoder) {
  if (!decoder) return;
  if (decoder->holds) dav1d_picture_unref(&decoder->picture);
  dav1d_close(&decoder->context);
  free(decoder);
}

struct yapfast_av1 *yapfast_av1_new(unsigned threads) {
  struct yapfast_av1 *decoder = calloc(1, sizeof *decoder);
  if (!decoder) return NULL;
  Dav1dSettings settings;
  dav1d_default_settings(&settings);
  settings.n_threads = (int)threads;
  /* A picture out for each unit in, as soon as it's decoded. */
  settings.max_frame_delay = 1;
  settings.frame_size_limit = (unsigned)MAX_SIDE * MAX_SIDE;
  /* Only the highest spatial layer, should a stream have more. */
  settings.all_layers = 0;
  /* Damaged units return errors; dav1d needn't print them too. */
  settings.logger.callback = NULL;
  if (dav1d_open(&decoder->context, &settings) < 0) {
    free(decoder);
    return NULL;
  }
  return decoder;
}

/* Decodes one temporal unit. Returns 1 with a picture, 0 when the decoder
 * has none yet, or a negative number when the unit couldn't be decoded. */
int yapfast_av1_decode(struct yapfast_av1 *decoder, const unsigned char *data, size_t size,
                       struct yapfast_picture *picture) {
  if (decoder->holds) {
    dav1d_picture_unref(&decoder->picture);
    decoder->holds = 0;
  }
  if (!size) return -1;
  Dav1dData unit;
  uint8_t *copy = dav1d_data_create(&unit, size);
  if (!copy) return -3;
  memcpy(copy, data, size);
  int sent = dav1d_send_data(decoder->context, &unit);
  int got = dav1d_get_picture(decoder->context, &decoder->picture);
  /* dav1d held a picture back from before: with it taken, the unit fits. */
  if (sent == DAV1D_ERR(EAGAIN) && got == 0) {
    sent = dav1d_send_data(decoder->context, &unit);
  }
  dav1d_data_unref(&unit);
  if (got == 0) decoder->holds = 1;
  if (sent < 0 && sent != DAV1D_ERR(EAGAIN)) return -1;
  if (got == DAV1D_ERR(EAGAIN)) return 0;
  if (got < 0) return -1;
  const Dav1dPicture *p = &decoder->picture;
  /* Main profile's 8-bit 4:2:0, as GPU encoders stream it; 10-bit pictures
   * don't show. */
  if (p->p.bpc != 8 || p->p.layout != DAV1D_PIXEL_LAYOUT_I420) return -2;
  if (p->p.w > MAX_SIDE || p->p.h > MAX_SIDE) return -2;
  picture->width = p->p.w;
  picture->height = p->p.h;
  picture->y = p->data[0];
  picture->u = p->data[1];
  picture->v = p->data[2];
  picture->y_stride = (int)p->stride[0];
  picture->u_stride = (int)p->stride[1];
  picture->v_stride = (int)p->stride[1];
  return 1;
}
