/* Favourite GIFs' thumbnails as VP9, through the libvpx inside Yapfast's
 * libwebrtc archive: a file encoder at a constant quality, unlike vp8.c's
 * realtime one. Rust wraps the frames in WebM. */
#include <stdlib.h>
#include <string.h>

#include "vpx/vp8cx.h"
#include "vpx/vpx_encoder.h"

struct yapfast_clip {
  vpx_codec_ctx_t codec;
  unsigned width;
  unsigned height;
};

/* Takes each encoded frame: its bytes, its time in milliseconds, and whether
 * it's a keyframe. A frame shown after hidden ones carries them too, as one
 * VP9 superframe. */
typedef void (*yapfast_clip_packet)(void *user, const unsigned char *data, size_t size,
                                 long long pts, int keyframe);

/* An encoder of `width` by `height` frames timed in milliseconds, at
 * `quality`, libvpx's constant quality level: 0 best, 63 smallest. */
struct yapfast_clip *yapfast_clip_new(unsigned width, unsigned height, unsigned quality,
                                unsigned threads) {
  vpx_codec_enc_cfg_t config;
  if (vpx_codec_enc_config_default(vpx_codec_vp9_cx(), &config, 0)) return NULL;
  config.g_w = width;
  config.g_h = height;
  config.g_timebase.num = 1;
  config.g_timebase.den = 1000;
  config.g_threads = threads;
  config.g_pass = VPX_RC_ONE_PASS;
  /* Looking ahead lets libvpx add hidden reference frames. */
  config.g_lag_in_frames = 25;
  config.rc_end_usage = VPX_Q;
  config.rc_min_quantizer = 0;
  config.rc_max_quantizer = 63;
  config.rc_dropframe_thresh = 0;
  config.kf_mode = VPX_KF_AUTO;

  struct yapfast_clip *clip = calloc(1, sizeof *clip);
  if (!clip) return NULL;
  if (vpx_codec_enc_init(&clip->codec, vpx_codec_vp9_cx(), &config, 0)) {
    free(clip);
    return NULL;
  }
  vpx_codec_control(&clip->codec, VP8E_SET_CQ_LEVEL, quality);
  vpx_codec_control(&clip->codec, VP8E_SET_CPUUSED, 2);
  vpx_codec_control(&clip->codec, VP8E_SET_ENABLEAUTOALTREF, 1);
  vpx_codec_control(&clip->codec, VP9E_SET_ROW_MT, 1);
  clip->width = width;
  clip->height = height;
  return clip;
}

static int drain(struct yapfast_clip *clip, yapfast_clip_packet packet, void *user) {
  vpx_codec_iter_t iter = NULL;
  const vpx_codec_cx_pkt_t *out;
  int count = 0;
  while ((out = vpx_codec_get_cx_data(&clip->codec, &iter))) {
    if (out->kind != VPX_CODEC_CX_FRAME_PKT) continue;
    packet(user, out->data.frame.buf, out->data.frame.sz, (long long)out->data.frame.pts,
           (out->data.frame.flags & VPX_FRAME_IS_KEY) != 0);
    count++;
  }
  return count;
}

/* Encodes an I420 frame shown at `pts` for `duration` milliseconds, or with
 * `y` null, the frames still held back, handing what comes out to `packet`.
 * Returns how many frames came out, or a negative number on failure. */
int yapfast_clip_encode(struct yapfast_clip *clip, const unsigned char *y, int y_stride,
                     const unsigned char *u, int u_stride, const unsigned char *v,
                     int v_stride, long long pts, unsigned long duration,
                     yapfast_clip_packet packet, void *user) {
  if (!y) {
    /* Each call lets out some of those held back, until none come. */
    int total = 0, count;
    do {
      if (vpx_codec_encode(&clip->codec, NULL, 0, 0, 0, VPX_DL_GOOD_QUALITY)) return -2;
      count = drain(clip, packet, user);
      total += count;
    } while (count > 0);
    return total;
  }
  vpx_image_t image;
  if (!vpx_img_wrap(&image, VPX_IMG_FMT_I420, clip->width, clip->height, 1,
                    (unsigned char *)y))
    return -1;
  image.planes[VPX_PLANE_Y] = (unsigned char *)y;
  image.planes[VPX_PLANE_U] = (unsigned char *)u;
  image.planes[VPX_PLANE_V] = (unsigned char *)v;
  image.stride[VPX_PLANE_Y] = y_stride;
  image.stride[VPX_PLANE_U] = u_stride;
  image.stride[VPX_PLANE_V] = v_stride;
  if (vpx_codec_encode(&clip->codec, &image, pts, duration, 0, VPX_DL_GOOD_QUALITY))
    return -2;
  return drain(clip, packet, user);
}

void yapfast_clip_free(struct yapfast_clip *clip) {
  vpx_codec_destroy(&clip->codec);
  free(clip);
}
