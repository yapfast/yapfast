/* H264 encoding for Discord Go Live and the camera, through x264
 * (vendor/x264-src). The same interface as vp8.c, so Rust drives both alike,
 * with an effort besides. Frames come out in Annex B form, each NAL unit
 * after a start code. */
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#include "x264.h"

struct yapfast_h264 {
  x264_t *encoder;
  /* What it was opened with, changed for each new bitrate. */
  x264_param_t param;
  /* The access unit the last call produced, where x264 wrote it: its NAL
   * units are one run of bytes that lives until the next encode, and Rust
   * drains it before then, into a buffer it has grown to the size the encode
   * reported. So the unit is copied once, into what is sent, and nothing here
   * keeps a copy of it; encoding the picture again to fit a larger buffer
   * would send a frame referencing one the receiver never got. */
  const unsigned char *unit;
  size_t unit_size;
};

/* Aims for `kbps`, never above it over half a second's buffer. */
static void set_rate(x264_param_t *param, unsigned kbps) {
  param->rc.i_bitrate = (int)kbps;
  param->rc.i_vbv_max_bitrate = (int)kbps;
  param->rc.i_vbv_buffer_size = kbps > 1 ? (int)(kbps / 2) : 1;
}

/* A realtime encoder, as measured on screen recordings and games (see
 * docs/discord/go-live.md): x264's superfast preset, or faster at `effort`
 * 1, with zero latency: a frame out for each frame in, no B-frames or
 * lookahead, and a slice for each thread. Main profile, which Discord's own client
 * sends; adaptive quantization off, which kept text sharper and games no
 * worse; parameter sets before each IDR. */
struct yapfast_h264 *yapfast_h264_new(unsigned width, unsigned height, unsigned fps,
                                      unsigned kbps, unsigned threads, unsigned effort) {
  struct yapfast_h264 *h264 = calloc(1, sizeof *h264);
  if (!h264) return NULL;
  x264_param_t *param = &h264->param;
  if (x264_param_default_preset(param, effort ? "faster" : "superfast", "zerolatency") < 0) {
    free(h264);
    return NULL;
  }
  param->i_log_level = X264_LOG_NONE;
  param->i_width = (int)width;
  param->i_height = (int)height;
  param->i_csp = X264_CSP_I420;
  param->i_fps_num = fps;
  param->i_fps_den = 1;
  /* Timed by the frame rate: timing frames by their own times would hold
   * each one back until the next came, to learn how long it shows. */
  param->b_vfr_input = 0;
  param->i_threads = (int)threads;
  /* A new viewer waits at most this long for a picture. */
  param->i_keyint_max = (int)fps * 3;
  param->b_repeat_headers = 1;
  param->b_annexb = 1;
  param->rc.i_rc_method = X264_RC_ABR;
  set_rate(param, kbps);
  param->rc.i_aq_mode = X264_AQ_NONE;
  /* libyuv converts captured pixels with BT.601's limited range. */
  param->vui.i_colorprim = 6;
  param->vui.i_transfer = 6;
  param->vui.i_colmatrix = 6;
  param->vui.b_fullrange = 0;
  if (x264_param_apply_profile(param, "main") < 0) {
    free(h264);
    return NULL;
  }
  h264->encoder = x264_encoder_open(param);
  if (!h264->encoder) {
    free(h264);
    return NULL;
  }
  return h264;
}

/* Encodes one I420 frame, with `pts` in 90 kHz units, and keeps what came out
 * for `yapfast_h264_drain` to copy into the caller's buffer: the size it
 * reports may be larger than that buffer, which grows to hold it without the
 * frame being encoded a second time. Returns the size produced, 0 when the
 * encoder gave no frame, or a negative number on failure. `duration` is
 * unused: x264 times frames by the frame rate. */
long yapfast_h264_encode(struct yapfast_h264 *h264, const unsigned char *y,
                         int y_stride, const unsigned char *u, int u_stride,
                         const unsigned char *v, int v_stride, long long pts,
                         unsigned long duration, int keyframe, int *is_key) {
  (void)duration;
  x264_picture_t picture, encoded;
  x264_picture_init(&picture);
  picture.img.i_csp = X264_CSP_I420;
  picture.img.i_plane = 3;
  picture.img.i_stride[0] = y_stride;
  picture.img.i_stride[1] = u_stride;
  picture.img.i_stride[2] = v_stride;
  picture.img.plane[0] = (uint8_t *)y;
  picture.img.plane[1] = (uint8_t *)u;
  picture.img.plane[2] = (uint8_t *)v;
  picture.i_pts = pts;
  picture.i_type = keyframe ? X264_TYPE_IDR : X264_TYPE_AUTO;
  x264_nal_t *nals = NULL;
  int count = 0;
  int size = x264_encoder_encode(h264->encoder, &nals, &count, &picture, &encoded);
  if (size < 0) return -1;
  /* Whatever the last call left is this frame's to replace, copied out or
   * not: x264 has already reused its memory. */
  h264->unit = NULL;
  h264->unit_size = 0;
  if (size == 0 || count == 0) return 0;
  /* x264 lays a frame's NAL units out one after another. */
  h264->unit = nals[0].p_payload;
  h264->unit_size = (size_t)size;
  *is_key = encoded.b_keyframe;
  return size;
}

/* Copies the access unit the last encode produced into `out`, which holds it,
 * straight out of x264's memory. Rust grows its
 * buffer to the size the encode reported before calling this once, so a frame
 * is never encoded twice to fit one. Returns the size copied, or -1 when
 * there is nothing to copy or `out` is too small. */
long yapfast_h264_drain(struct yapfast_h264 *h264, unsigned char *out, size_t capacity) {
  size_t size = h264->unit_size;
  if (!size || size > capacity) return -1;
  memcpy(out, h264->unit, size);
  h264->unit = NULL;
  h264->unit_size = 0;
  return (long)size;
}

/* The memory this encoder keeps for the access unit it produces: none, since
 * the unit is drained from x264's own. */
size_t yapfast_h264_staging(struct yapfast_h264 *h264) {
  (void)h264;
  return 0;
}

/* Changes the bitrate the encoder aims for, as the network allows. Returns
 * 0, or nonzero on failure. */
int yapfast_h264_set_bitrate(struct yapfast_h264 *h264, unsigned kbps) {
  set_rate(&h264->param, kbps);
  return x264_encoder_reconfig(h264->encoder, &h264->param) < 0;
}

void yapfast_h264_free(struct yapfast_h264 *h264) {
  x264_encoder_close(h264->encoder);
  free(h264);
}
