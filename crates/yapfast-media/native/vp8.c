/* VP8 encoding for Discord Go Live, through the libvpx inside Yapfast's
 * libwebrtc archive. Rust calls these few functions instead of libvpx's
 * configuration structs, whose layout only the headers know. */
#include <stdlib.h>
#include <string.h>

#include "vpx/vp8cx.h"
#include "vpx/vpx_encoder.h"

struct yapfast_vp8 {
  vpx_codec_ctx_t codec;
  vpx_codec_enc_cfg_t config;
  unsigned width;
  unsigned height;
  /* The frame the last call produced, kept until Rust copies it into the
   * buffer it holds: libvpx's own packet lives only until the next encode,
   * and encoding the picture again to get it into a larger buffer would send
   * a frame referencing one the receiver never got. Reused from frame to
   * frame, so it holds one frame's worth rather than a stream's. */
  unsigned char *staged;
  size_t staged_capacity;
  size_t staged_size;
};

/* Keeps `size` bytes of `bytes`, at `at` into the staged frame, for Rust to
 * copy out. The buffer grows to the largest frame seen and no further, so a
 * stream asks for memory only until its frames settle. Returns 0 when memory
 * ran out. */
static int stage(struct yapfast_vp8 *encoder, size_t at, const unsigned char *bytes,
                 size_t size) {
  if (at + size > encoder->staged_capacity) {
    unsigned char *larger = realloc(encoder->staged, at + size);
    if (!larger) return 0;
    encoder->staged = larger;
    encoder->staged_capacity = at + size;
  }
  memcpy(encoder->staged + at, bytes, size);
  return 1;
}

/* A realtime encoder tuned for screen content, timed in 90 kHz RTP units. */
struct yapfast_vp8 *yapfast_vp8_new(unsigned width, unsigned height, unsigned fps,
                              unsigned kbps, unsigned threads) {
  vpx_codec_enc_cfg_t config;
  if (vpx_codec_enc_config_default(vpx_codec_vp8_cx(), &config, 0)) return NULL;
  config.g_w = width;
  config.g_h = height;
  config.g_timebase.num = 1;
  config.g_timebase.den = 90000;
  config.g_threads = threads;
  config.g_lag_in_frames = 0;
  config.g_error_resilient = VPX_ERROR_RESILIENT_DEFAULT;
  config.rc_end_usage = VPX_CBR;
  config.rc_target_bitrate = kbps;
  config.rc_min_quantizer = 2;
  config.rc_max_quantizer = 52;
  config.rc_undershoot_pct = 100;
  config.rc_overshoot_pct = 15;
  config.rc_buf_initial_sz = 500;
  config.rc_buf_optimal_sz = 600;
  config.rc_buf_sz = 1000;
  config.kf_mode = VPX_KF_AUTO;
  /* A new viewer waits at most this long for a picture. */
  config.kf_max_dist = fps * 3;

  struct yapfast_vp8 *encoder = calloc(1, sizeof *encoder);
  if (!encoder) return NULL;
  if (vpx_codec_enc_init(&encoder->codec, vpx_codec_vp8_cx(), &config, 0)) {
    free(encoder);
    return NULL;
  }
  vpx_codec_control(&encoder->codec, VP8E_SET_CPUUSED, -8);
  vpx_codec_control(&encoder->codec, VP8E_SET_SCREEN_CONTENT_MODE, 1);
  vpx_codec_control(&encoder->codec, VP8E_SET_STATIC_THRESHOLD, 1u);
  vpx_codec_control(&encoder->codec, VP8E_SET_MAX_INTRA_BITRATE_PCT, 300u);
  encoder->config = config;
  encoder->width = width;
  encoder->height = height;
  return encoder;
}

/* Encodes one I420 frame, and keeps what came out for
 * `yapfast_vp8_drain` to copy into the caller's buffer: the size it reports
 * may be larger than that buffer, which grows to hold it without the frame
 * being encoded a second time. Returns the size produced, 0 when the encoder
 * dropped the frame, or a negative number on failure. */
long yapfast_vp8_encode(struct yapfast_vp8 *encoder, const unsigned char *y,
                     int y_stride, const unsigned char *u, int u_stride,
                     const unsigned char *v, int v_stride, long long pts,
                     unsigned long duration, int keyframe, int *is_key) {
  vpx_image_t image;
  if (!vpx_img_wrap(&image, VPX_IMG_FMT_I420, encoder->width, encoder->height,
                    1, (unsigned char *)y))
    return -1;
  image.planes[VPX_PLANE_Y] = (unsigned char *)y;
  image.planes[VPX_PLANE_U] = (unsigned char *)u;
  image.planes[VPX_PLANE_V] = (unsigned char *)v;
  image.stride[VPX_PLANE_Y] = y_stride;
  image.stride[VPX_PLANE_U] = u_stride;
  image.stride[VPX_PLANE_V] = v_stride;
  if (vpx_codec_encode(&encoder->codec, &image, pts, duration,
                       keyframe ? VPX_EFLAG_FORCE_KF : 0, VPX_DL_REALTIME))
    return -2;
  size_t written = 0;
  /* Whatever the last call left is this frame's to replace, copied out or
   * not: one frame is held at a time. */
  encoder->staged_size = 0;
  *is_key = 0;
  vpx_codec_iter_t iter = NULL;
  const vpx_codec_cx_pkt_t *packet;
  while ((packet = vpx_codec_get_cx_data(&encoder->codec, &iter))) {
    if (packet->kind != VPX_CODEC_CX_FRAME_PKT) continue;
    if (!stage(encoder, written, packet->data.frame.buf, packet->data.frame.sz))
      return -1;
    written += packet->data.frame.sz;
    if (packet->data.frame.flags & VPX_FRAME_IS_KEY) *is_key = 1;
  }
  encoder->staged_size = written;
  return (long)written;
}

/* Copies the frame the last encode produced into `out`, which holds it, and
 * gives up the copy: the buffer stays for the next frame. Rust grows its
 * buffer to the size the encode reported before calling this once, so a frame
 * is never encoded twice to fit one. Returns the size copied, or -1 when
 * there is nothing to copy or `out` is too small. */
long yapfast_vp8_drain(struct yapfast_vp8 *encoder, unsigned char *out, size_t capacity) {
  size_t size = encoder->staged_size;
  if (!size || size > capacity) return -1;
  memcpy(out, encoder->staged, size);
  encoder->staged_size = 0;
  return (long)size;
}

/* The memory this encoder keeps for the frame it produces, reused for the
 * next one: one frame's worth, however many frames go through it, which is
 * what a measurement of a stream's memory holds it to. */
size_t yapfast_vp8_staging(struct yapfast_vp8 *encoder) {
  return encoder->staged_capacity;
}

/* Changes the bitrate the encoder aims for, as the network allows. Returns
 * 0, or nonzero on failure. */
int yapfast_vp8_set_bitrate(struct yapfast_vp8 *encoder, unsigned kbps) {
  vpx_codec_enc_cfg_t config = encoder->config;
  config.rc_target_bitrate = kbps;
  if (vpx_codec_enc_config_set(&encoder->codec, &config)) return 1;
  encoder->config = config;
  return 0;
}

void yapfast_vp8_free(struct yapfast_vp8 *encoder) {
  vpx_codec_destroy(&encoder->codec);
  free(encoder->staged);
  free(encoder);
}
