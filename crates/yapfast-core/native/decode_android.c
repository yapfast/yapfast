/* Decoding Go Live video on Android: VP8 through libvpx inside libwebrtc.a
 * and H264 through Android NDK MediaCodec (libmediandk.so), outputting I420
 * planes that stay valid until the next call on the decoder. */
#include <limits.h>
#include <media/NdkMediaCodec.h>
#include <media/NdkMediaFormat.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#include "libyuv/convert.h"
#include "vpx/vp8dx.h"
#include "vpx/vpx_decoder.h"

enum { YAPFAST_VP8 = 0, YAPFAST_H264 = 1 };

/* Cap maximum frame side to avoid absurd allocations from malformed headers. */
enum { MAX_SIDE = 8192 };

enum {
  COLOR_YUV420_PLANAR = 19,
  COLOR_YUV420_PACKED_PLANAR = 20,
  COLOR_YUV420_SEMIPLANAR = 21,
  COLOR_YV12 = 0x32315659
};

struct yapfast_decoder {
  int codec;
  vpx_codec_ctx_t vpx;
  vpx_codec_iter_t iter;
  AMediaCodec *media;
  int width;
  int height;
  int stride;
  int slice_height;
  int crop_left;
  int crop_top;
  int crop_right;
  int crop_bottom;
  int has_crop;
  int color_format;
  uint8_t *i420;
  size_t i420_cap;
  int64_t pts_us;
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

void yapfast_decoder_free(struct yapfast_decoder *decoder) {
  if (!decoder) return;
  if (decoder->codec == YAPFAST_VP8) {
    vpx_codec_destroy(&decoder->vpx);
  } else {
    if (decoder->media) {
      AMediaCodec_stop(decoder->media);
      AMediaCodec_delete(decoder->media);
    }
    free(decoder->i420);
  }
  free(decoder);
}

static void update_h264_format(struct yapfast_decoder *decoder) {
  AMediaFormat *fmt = AMediaCodec_getOutputFormat(decoder->media);
  if (!fmt) return;
  int32_t w = 0, h = 0, stride = 0, slice_h = 0, cf = 0;
  AMediaFormat_getInt32(fmt, AMEDIAFORMAT_KEY_WIDTH, &w);
  AMediaFormat_getInt32(fmt, AMEDIAFORMAT_KEY_HEIGHT, &h);
  if (!AMediaFormat_getInt32(fmt, AMEDIAFORMAT_KEY_STRIDE, &stride) || stride <= 0) {
    stride = w;
  }
  if (!AMediaFormat_getInt32(fmt, "slice-height", &slice_h) || slice_h <= 0) {
    slice_h = h;
  }
  AMediaFormat_getInt32(fmt, AMEDIAFORMAT_KEY_COLOR_FORMAT, &cf);

  int32_t cl = 0, ct = 0, cr = 0, cb = 0;
  if (AMediaFormat_getInt32(fmt, "crop-left", &cl) &&
      AMediaFormat_getInt32(fmt, "crop-top", &ct) &&
      AMediaFormat_getInt32(fmt, "crop-right", &cr) &&
      AMediaFormat_getInt32(fmt, "crop-bottom", &cb) &&
      cr >= cl && cb >= ct && cr < w && cb < h) {
    decoder->crop_left = cl;
    decoder->crop_top = ct;
    decoder->crop_right = cr;
    decoder->crop_bottom = cb;
    decoder->has_crop = 1;
  } else {
    decoder->crop_left = 0;
    decoder->crop_top = 0;
    decoder->crop_right = w - 1;
    decoder->crop_bottom = h - 1;
    decoder->has_crop = 0;
  }

  decoder->width = w;
  decoder->height = h;
  decoder->stride = stride < w ? w : stride;
  decoder->slice_height = slice_h < h ? h : slice_h;
  decoder->color_format = cf;
  AMediaFormat_delete(fmt);
}

struct yapfast_decoder *yapfast_decoder_new(int codec, unsigned threads) {
  struct yapfast_decoder *decoder = calloc(1, sizeof *decoder);
  if (!decoder) return NULL;
  decoder->codec = codec;
  if (codec == YAPFAST_VP8) {
    vpx_codec_dec_cfg_t cfg = {.threads = threads ? threads : 1};
    if (vpx_codec_dec_init(&decoder->vpx, vpx_codec_vp8_dx(), &cfg, 0)) {
      free(decoder);
      return NULL;
    }
    return decoder;
  }

  AMediaCodec *media = AMediaCodec_createDecoderByType("video/avc");
  if (!media) {
    free(decoder);
    return NULL;
  }
  AMediaFormat *fmt = AMediaFormat_new();
  if (!fmt) {
    AMediaCodec_delete(media);
    free(decoder);
    return NULL;
  }
  AMediaFormat_setString(fmt, AMEDIAFORMAT_KEY_MIME, "video/avc");
  AMediaFormat_setInt32(fmt, AMEDIAFORMAT_KEY_WIDTH, 1280);
  AMediaFormat_setInt32(fmt, AMEDIAFORMAT_KEY_HEIGHT, 720);
  AMediaFormat_setInt32(fmt, "low-latency", 1);
  media_status_t st = AMediaCodec_configure(media, fmt, NULL, NULL, 0);
  AMediaFormat_delete(fmt);
  if (st != AMEDIA_OK) {
    AMediaCodec_delete(media);
    free(decoder);
    return NULL;
  }
  if (AMediaCodec_start(media) != AMEDIA_OK) {
    AMediaCodec_delete(media);
    free(decoder);
    return NULL;
  }
  decoder->media = media;
  return decoder;
}

static int ensure_i420_buf(struct yapfast_decoder *decoder, int out_w, int out_h) {
  if (out_w <= 0 || out_h <= 0 || out_w > MAX_SIDE || out_h > MAX_SIDE) return -1;
  size_t cw = (size_t)(out_w + 1) / 2;
  size_t ch = (size_t)(out_h + 1) / 2;
  size_t need = (size_t)out_w * (size_t)out_h + 2 * cw * ch;
  if (need > decoder->i420_cap) {
    uint8_t *grown = realloc(decoder->i420, need);
    if (!grown) return -1;
    decoder->i420 = grown;
    decoder->i420_cap = need;
  }
  return 0;
}

static int copy_planar_to_i420(const uint8_t *src, size_t src_size, int stride, int slice_h,
                               int crop_x, int crop_y, int w, int h, int swap_uv,
                               uint8_t *dst_y, uint8_t *dst_u, uint8_t *dst_v) {
  int cw = (w + 1) / 2;
  int ch = (h + 1) / 2;
  int cstride = (stride + 1) / 2;
  int cslice_h = (slice_h + 1) / 2;
  size_t y_plane_bytes = (size_t)stride * (size_t)slice_h;
  size_t uv_plane_bytes = (size_t)cstride * (size_t)cslice_h;
  if (src_size < y_plane_bytes + 2 * uv_plane_bytes) return -1;

  const uint8_t *sy = src + (size_t)crop_y * (size_t)stride + (size_t)crop_x;
  const uint8_t *p1 = src + y_plane_bytes;
  const uint8_t *p2 = p1 + uv_plane_bytes;
  const uint8_t *su = (swap_uv ? p2 : p1) + (size_t)(crop_y / 2) * (size_t)cstride + (size_t)(crop_x / 2);
  const uint8_t *sv = (swap_uv ? p1 : p2) + (size_t)(crop_y / 2) * (size_t)cstride + (size_t)(crop_x / 2);

  for (int row = 0; row < h; row++) {
    memcpy(dst_y + (size_t)row * (size_t)w, sy + (size_t)row * (size_t)stride, (size_t)w);
  }
  for (int row = 0; row < ch; row++) {
    memcpy(dst_u + (size_t)row * (size_t)cw, su + (size_t)row * (size_t)cstride, (size_t)cw);
    memcpy(dst_v + (size_t)row * (size_t)cw, sv + (size_t)row * (size_t)cstride, (size_t)cw);
  }
  return 0;
}

int yapfast_decoder_decode(struct yapfast_decoder *decoder, const unsigned char *data,
                           size_t size, struct yapfast_picture *picture) {
  if (!decoder || !picture || (!data && size > 0)) return -1;
  if (size > (size_t)INT_MAX) return -1;

  if (decoder->codec == YAPFAST_VP8) {
    if (vpx_codec_decode(&decoder->vpx, data, (unsigned int)size, NULL, 0)) {
      return -1;
    }
    decoder->iter = NULL;
    vpx_image_t *img = vpx_codec_get_frame(&decoder->vpx, &decoder->iter);
    if (!img) return 0;
    if (img->d_w == 0 || img->d_h == 0 || img->d_w > MAX_SIDE || img->d_h > MAX_SIDE) {
      return -1;
    }
    *picture = (struct yapfast_picture){
        .width = (int)img->d_w,
        .height = (int)img->d_h,
        .y = img->planes[0],
        .u = img->planes[1],
        .v = img->planes[2],
        .y_stride = img->stride[0],
        .u_stride = img->stride[1],
        .v_stride = img->stride[2],
    };
    return 1;
  }

  size_t offset = 0;
  while (offset < size) {
    ssize_t in_idx = AMediaCodec_dequeueInputBuffer(decoder->media, 50000);
    if (in_idx < 0) return -1;
    size_t cap = 0;
    uint8_t *in_buf = AMediaCodec_getInputBuffer(decoder->media, (size_t)in_idx, &cap);
    if (!in_buf || cap == 0) return -1;
    size_t chunk = size - offset;
    if (chunk > cap) chunk = cap;
    memcpy(in_buf, data + offset, chunk);
    offset += chunk;
    if (AMediaCodec_queueInputBuffer(decoder->media, (size_t)in_idx, 0, chunk,
                                     (uint64_t)(decoder->pts_us += 16667), 0) != AMEDIA_OK) {
      return -1;
    }
  }

  AMediaCodecBufferInfo info;
  for (int attempt = 0; attempt < 16; attempt++) {
    ssize_t out_idx = AMediaCodec_dequeueOutputBuffer(decoder->media, &info, attempt == 0 ? 12000 : 4000);
    if (out_idx == AMEDIACODEC_INFO_OUTPUT_FORMAT_CHANGED) {
      update_h264_format(decoder);
      continue;
    }
    if (out_idx == AMEDIACODEC_INFO_OUTPUT_BUFFERS_CHANGED) {
      continue;
    }
    if (out_idx == AMEDIACODEC_INFO_TRY_AGAIN_LATER) {
      return 0;
    }
    if (out_idx < 0) {
      return -1;
    }

    if (decoder->width <= 0 || decoder->height <= 0) {
      update_h264_format(decoder);
    }

    size_t out_size = 0;
    const uint8_t *out_buf = AMediaCodec_getOutputBuffer(decoder->media, (size_t)out_idx, &out_size);
    int crop_x = decoder->has_crop ? decoder->crop_left : 0;
    int crop_y = decoder->has_crop ? decoder->crop_top : 0;
    int out_w = decoder->has_crop ? (decoder->crop_right - decoder->crop_left + 1) : decoder->width;
    int out_h = decoder->has_crop ? (decoder->crop_bottom - decoder->crop_top + 1) : decoder->height;

    if (!out_buf || out_size <= (size_t)info.offset || ensure_i420_buf(decoder, out_w, out_h) < 0) {
      AMediaCodec_releaseOutputBuffer(decoder->media, (size_t)out_idx, false);
      return -1;
    }

    const uint8_t *src = out_buf + info.offset;
    size_t avail = out_size - (size_t)info.offset;
    int cw = (out_w + 1) / 2;
    int ch = (out_h + 1) / 2;
    uint8_t *dst_y = decoder->i420;
    uint8_t *dst_u = dst_y + (size_t)out_w * (size_t)out_h;
    uint8_t *dst_v = dst_u + (size_t)cw * (size_t)ch;

    int conv = -1;
    if (decoder->color_format == COLOR_YUV420_PLANAR ||
        decoder->color_format == COLOR_YUV420_PACKED_PLANAR ||
        decoder->color_format == COLOR_YV12) {
      conv = copy_planar_to_i420(src, avail, decoder->stride, decoder->slice_height,
                                 crop_x, crop_y, out_w, out_h,
                                 decoder->color_format == COLOR_YV12,
                                 dst_y, dst_u, dst_v);
    } else {
      size_t y_plane = (size_t)decoder->stride * (size_t)decoder->slice_height;
      size_t uv_rows = (size_t)((out_h + crop_y + 1) / 2);
      if (avail >= y_plane + uv_rows * (size_t)decoder->stride) {
        const uint8_t *sy = src + (size_t)crop_y * (size_t)decoder->stride + (size_t)crop_x;
        const uint8_t *suv = src + y_plane + (size_t)(crop_y / 2) * (size_t)decoder->stride +
                             (size_t)(crop_x & ~1);
        conv = NV12ToI420(sy, decoder->stride, suv, decoder->stride,
                          dst_y, out_w, dst_u, cw, dst_v, cw, out_w, out_h);
      }
    }

    AMediaCodec_releaseOutputBuffer(decoder->media, (size_t)out_idx, false);
    if (conv < 0) return -1;

    *picture = (struct yapfast_picture){
        .width = out_w,
        .height = out_h,
        .y = dst_y,
        .u = dst_u,
        .v = dst_v,
        .y_stride = out_w,
        .u_stride = cw,
        .v_stride = cw,
    };
    return 1;
  }
  return 0;
}
