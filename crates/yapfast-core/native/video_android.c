/* Android video and sound file decoding for `video.rs` using NDK
 * AMediaExtractor + AMediaCodec (libmediandk.so) and libyuv.
 * Uses dual extractors over one synchronized random-access AMediaDataSource
 * bridge so picture and sound tracks stay independently seekable/decodable. */
#include <limits.h>
#include <media/NdkMediaCodec.h>
#include <media/NdkMediaDataSource.h>
#include <media/NdkMediaExtractor.h>
#include <media/NdkMediaFormat.h>
#include <pthread.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "libyuv/convert.h"
#include "libyuv/convert_argb.h"
#include "libyuv/scale.h"
#include "libyuv/scale_argb.h"

enum { MAX_SIDE = 8192 };
enum { END = 0, PICTURE = 1, SOUND = 2, HELD = 3, FAILED = -1, UNSUPPORTED = -2 };
enum { SOUND_ONLY = 2 };
enum { YAPFAST_SEEK_SIZE = 0x10000 };

enum {
  COLOR_YUV420_PLANAR = 19,
  COLOR_YUV420_PACKED_PLANAR = 20,
  COLOR_YUV420_SEMIPLANAR = 21,
  COLOR_YV12 = 0x32315659
};

enum {
  PCM_ENCODING_I16 = 2,
  PCM_ENCODING_FLOAT = 4
};

typedef int (*yapfast_read)(void *source, uint8_t *buffer, int size);
typedef int64_t (*yapfast_seek)(void *source, int64_t offset, int whence);

struct yapfast_video_info {
  int width;
  int height;
  int64_t duration_ms;
  int sample_rate;
  int channels;
};

struct source_bridge {
  pthread_mutex_t lock;
  bool closed;
  void *source;
  yapfast_read read;
  yapfast_seek seek;
  int64_t pos;
};

struct video_geometry {
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
};

struct yapfast_video {
  struct source_bridge *bridge;
  AMediaDataSource *video_ds;
  AMediaExtractor *video_ex;
  AMediaCodec *video_codec;
  bool video_in_eof;
  bool video_out_eof;
  struct video_geometry geom;
  int pic_width;
  int pic_height;
  uint8_t *pic_rgba;
  size_t pic_rgba_cap;
  uint8_t *scratch;
  size_t scratch_cap;

  AMediaDataSource *audio_ds;
  AMediaExtractor *audio_ex;
  AMediaCodec *audio_codec;
  bool audio_in_eof;
  bool audio_out_eof;
  int sample_rate;
  int channels;
  int pcm_encoding;
  float *sound_buf;
  size_t sound_cap;
  int sound_frames;
};

static ssize_t bridge_read_at(void *userdata, off64_t offset, void *buffer, size_t size) {
  if (size == 0) return 0;
  struct source_bridge *bridge = userdata;
  if (!bridge || offset < 0) return -1;

  pthread_mutex_lock(&bridge->lock);
  if (bridge->closed) {
    pthread_mutex_unlock(&bridge->lock);
    return -1;
  }
  if (bridge->pos != offset) {
    int64_t moved = bridge->seek(bridge->source, (int64_t)offset, 0);
    if (moved < 0) {
      pthread_mutex_unlock(&bridge->lock);
      return -1;
    }
    bridge->pos = moved;
  }

  size_t total = 0;
  uint8_t *out = buffer;
  while (total < size) {
    size_t rem = size - total;
    int chunk = rem > (size_t)INT_MAX ? INT_MAX : (int)rem;
    int got = bridge->read(bridge->source, out + total, chunk);
    if (got < 0) {
      pthread_mutex_unlock(&bridge->lock);
      return total > 0 ? (ssize_t)total : -1;
    }
    if (got == 0) {
      break;
    }
    total += (size_t)got;
    bridge->pos += got;
  }
  pthread_mutex_unlock(&bridge->lock);
  return total == 0 ? -1 : (ssize_t)total;
}

static ssize_t bridge_get_size(void *userdata) {
  struct source_bridge *bridge = userdata;
  if (!bridge) return -1;
  pthread_mutex_lock(&bridge->lock);
  if (bridge->closed) {
    pthread_mutex_unlock(&bridge->lock);
    return -1;
  }
  int64_t size = bridge->seek(bridge->source, 0, YAPFAST_SEEK_SIZE);
  pthread_mutex_unlock(&bridge->lock);
  return size >= 0 ? (ssize_t)size : -1;
}

static void bridge_close(void *userdata) {
  struct source_bridge *bridge = userdata;
  if (!bridge) return;
  pthread_mutex_lock(&bridge->lock);
  bridge->closed = true;
  pthread_mutex_unlock(&bridge->lock);
}

static AMediaDataSource *make_data_source(struct source_bridge *bridge) {
  AMediaDataSource *ds = AMediaDataSource_new();
  if (!ds) return NULL;
  AMediaDataSource_setUserdata(ds, bridge);
  AMediaDataSource_setReadAt(ds, bridge_read_at);
  AMediaDataSource_setGetSize(ds, bridge_get_size);
  AMediaDataSource_setClose(ds, bridge_close);
  return ds;
}

void yapfast_video_free(struct yapfast_video *video) {
  if (!video) return;
  if (video->video_codec) {
    AMediaCodec_stop(video->video_codec);
    AMediaCodec_delete(video->video_codec);
  }
  if (video->audio_codec) {
    AMediaCodec_stop(video->audio_codec);
    AMediaCodec_delete(video->audio_codec);
  }
  if (video->video_ex) AMediaExtractor_delete(video->video_ex);
  if (video->audio_ex) AMediaExtractor_delete(video->audio_ex);
  if (video->video_ds) AMediaDataSource_delete(video->video_ds);
  if (video->audio_ds) AMediaDataSource_delete(video->audio_ds);
  if (video->bridge) {
    pthread_mutex_destroy(&video->bridge->lock);
    free(video->bridge);
  }
  free(video->pic_rgba);
  free(video->scratch);
  free(video->sound_buf);
  free(video);
}

static void update_video_geom(struct yapfast_video *video, AMediaFormat *fmt) {
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

  struct video_geometry *g = &video->geom;
  int32_t cl = 0, ct = 0, cr = 0, cb = 0;
  if (AMediaFormat_getInt32(fmt, "crop-left", &cl) &&
      AMediaFormat_getInt32(fmt, "crop-top", &ct) &&
      AMediaFormat_getInt32(fmt, "crop-right", &cr) &&
      AMediaFormat_getInt32(fmt, "crop-bottom", &cb) &&
      cr >= cl && cb >= ct && cr < w && cb < h) {
    g->crop_left = cl;
    g->crop_top = ct;
    g->crop_right = cr;
    g->crop_bottom = cb;
    g->has_crop = 1;
  } else {
    g->crop_left = 0;
    g->crop_top = 0;
    g->crop_right = w - 1;
    g->crop_bottom = h - 1;
    g->has_crop = 0;
  }
  g->width = w;
  g->height = h;
  g->stride = stride < w ? w : stride;
  g->slice_height = slice_h < h ? h : slice_h;
  g->color_format = cf;
}

static void update_audio_format(struct yapfast_video *video, AMediaFormat *fmt) {
  if (!fmt) return;
  int32_t rate = 0, channels = 0, enc = PCM_ENCODING_I16;
  if (AMediaFormat_getInt32(fmt, AMEDIAFORMAT_KEY_SAMPLE_RATE, &rate) && rate > 0) {
    video->sample_rate = rate;
  }
  if (AMediaFormat_getInt32(fmt, AMEDIAFORMAT_KEY_CHANNEL_COUNT, &channels) && channels > 0) {
    video->channels = channels;
  }
  if (AMediaFormat_getInt32(fmt, "pcm-encoding", &enc) &&
      (enc == PCM_ENCODING_FLOAT || enc == PCM_ENCODING_I16)) {
    video->pcm_encoding = enc;
  } else {
    video->pcm_encoding = PCM_ENCODING_I16;
  }
}

static uint8_t *ensure_buf(uint8_t **buf, size_t *cap, size_t need) {
  if (need <= *cap) return *buf;
  uint8_t *grown = realloc(*buf, need);
  if (!grown) return NULL;
  *buf = grown;
  *cap = need;
  return grown;
}

static int convert_output_to_rgba(struct yapfast_video *video, const uint8_t *src, size_t avail) {
  const struct video_geometry *g = &video->geom;
  int crop_x = g->has_crop ? g->crop_left : 0;
  int crop_y = g->has_crop ? g->crop_top : 0;
  int w = g->has_crop ? (g->crop_right - g->crop_left + 1) : g->width;
  int h = g->has_crop ? (g->crop_bottom - g->crop_top + 1) : g->height;
  if (w <= 0 || h <= 0 || w > MAX_SIDE || h > MAX_SIDE) return -1;

  size_t rgba_bytes = (size_t)w * (size_t)h * 4;
  uint8_t *rgba = ensure_buf(&video->pic_rgba, &video->pic_rgba_cap, rgba_bytes);
  if (!rgba) return -1;

  if (g->color_format == COLOR_YUV420_PLANAR ||
      g->color_format == COLOR_YUV420_PACKED_PLANAR ||
      g->color_format == COLOR_YV12) {
    int cstride = (g->stride + 1) / 2;
    int cslice_h = (g->slice_height + 1) / 2;
    size_t y_plane = (size_t)g->stride * (size_t)g->slice_height;
    size_t uv_plane = (size_t)cstride * (size_t)cslice_h;
    if (avail < y_plane + 2 * uv_plane) return -1;
    bool swap_uv = (g->color_format == COLOR_YV12);
    const uint8_t *sy = src + (size_t)crop_y * (size_t)g->stride + (size_t)crop_x;
    const uint8_t *p1 = src + y_plane;
    const uint8_t *p2 = p1 + uv_plane;
    const uint8_t *su = (swap_uv ? p2 : p1) + (size_t)(crop_y / 2) * (size_t)cstride + (size_t)(crop_x / 2);
    const uint8_t *sv = (swap_uv ? p1 : p2) + (size_t)(crop_y / 2) * (size_t)cstride + (size_t)(crop_x / 2);
    if (I420ToABGR(sy, g->stride, su, cstride, sv, cstride, rgba, w * 4, w, h) < 0) {
      return -1;
    }
  } else {
    size_t y_plane = (size_t)g->stride * (size_t)g->slice_height;
    size_t uv_rows = (size_t)((h + crop_y + 1) / 2);
    if (avail < y_plane + uv_rows * (size_t)g->stride) return -1;
    const uint8_t *sy = src + (size_t)crop_y * (size_t)g->stride + (size_t)crop_x;
    const uint8_t *suv = src + y_plane + (size_t)(crop_y / 2) * (size_t)g->stride + (size_t)(crop_x & ~1);
    if (NV12ToABGR(sy, g->stride, suv, g->stride, rgba, w * 4, w, h) < 0) {
      return -1;
    }
  }

  video->pic_width = w;
  video->pic_height = h;
  return 0;
}

static bool feed_extractor(AMediaExtractor *ex, AMediaCodec *codec, bool *in_eof) {
  if (*in_eof) return false;
  ssize_t idx = AMediaCodec_dequeueInputBuffer(codec, 2000);
  if (idx < 0) return false;
  size_t cap = 0;
  uint8_t *buf = AMediaCodec_getInputBuffer(codec, (size_t)idx, &cap);
  if (!buf || cap == 0) return false;

  ssize_t sample_size = AMediaExtractor_readSampleData(ex, buf, cap);
  if (sample_size < 0) {
    AMediaCodec_queueInputBuffer(codec, (size_t)idx, 0, 0, 0,
                                 AMEDIACODEC_BUFFER_FLAG_END_OF_STREAM);
    *in_eof = true;
    return true;
  }
  int64_t pts = AMediaExtractor_getSampleTime(ex);
  uint32_t flags = AMediaExtractor_getSampleFlags(ex);
  AMediaCodec_queueInputBuffer(codec, (size_t)idx, 0, (size_t)sample_size,
                               pts >= 0 ? (uint64_t)pts : 0, flags);
  AMediaExtractor_advance(ex);
  return true;
}

static int open_track(AMediaExtractor *ex, size_t track, AMediaCodec **codec_out,
                      AMediaFormat **fmt_out) {
  AMediaFormat *fmt = AMediaExtractor_getTrackFormat(ex, track);
  if (!fmt) return -1;
  const char *mime = NULL;
  if (!AMediaFormat_getString(fmt, AMEDIAFORMAT_KEY_MIME, &mime) || !mime) {
    AMediaFormat_delete(fmt);
    return -1;
  }
  AMediaCodec *codec = AMediaCodec_createDecoderByType(mime);
  if (!codec) {
    AMediaFormat_delete(fmt);
    return -1;
  }
  if (AMediaCodec_configure(codec, fmt, NULL, NULL, 0) != AMEDIA_OK ||
      AMediaCodec_start(codec) != AMEDIA_OK ||
      AMediaExtractor_selectTrack(ex, track) != AMEDIA_OK) {
    AMediaCodec_delete(codec);
    AMediaFormat_delete(fmt);
    return -1;
  }
  *codec_out = codec;
  *fmt_out = fmt;
  return 0;
}

struct yapfast_video *yapfast_video_open(void *source, yapfast_read read, yapfast_seek seek_to,
                                         unsigned threads, int sound,
                                         struct yapfast_video_info *info) {
  (void)threads;
  if (!source || !read || !seek_to || !info) return NULL;
  *info = (struct yapfast_video_info){.duration_ms = -1};

  struct yapfast_video *video = calloc(1, sizeof *video);
  struct source_bridge *bridge = calloc(1, sizeof *bridge);
  if (!video || !bridge) {
    free(video);
    free(bridge);
    return NULL;
  }
  pthread_mutex_init(&bridge->lock, NULL);
  bridge->source = source;
  bridge->read = read;
  bridge->seek = seek_to;
  bridge->pos = -1;
  video->bridge = bridge;

  AMediaDataSource *ds1 = make_data_source(bridge);
  AMediaExtractor *ex1 = AMediaExtractor_new();
  if (!ds1 || !ex1 || AMediaExtractor_setDataSourceCustom(ex1, ds1) != AMEDIA_OK) {
    if (ex1) AMediaExtractor_delete(ex1);
    if (ds1) AMediaDataSource_delete(ds1);
    yapfast_video_free(video);
    return NULL;
  }

  size_t tracks = AMediaExtractor_getTrackCount(ex1);
  ssize_t vid_idx = -1;
  ssize_t aud_idx = -1;
  for (size_t i = 0; i < tracks; i++) {
    AMediaFormat *tfmt = AMediaExtractor_getTrackFormat(ex1, i);
    if (!tfmt) continue;
    const char *mime = NULL;
    if (AMediaFormat_getString(tfmt, AMEDIAFORMAT_KEY_MIME, &mime) && mime) {
      if (vid_idx < 0 && strncmp(mime, "video/", 6) == 0) vid_idx = (ssize_t)i;
      if (aud_idx < 0 && strncmp(mime, "audio/", 6) == 0) aud_idx = (ssize_t)i;
    }
    AMediaFormat_delete(tfmt);
  }

  if (sound == SOUND_ONLY) {
    if (aud_idx < 0) {
      AMediaExtractor_delete(ex1);
      AMediaDataSource_delete(ds1);
      yapfast_video_free(video);
      return NULL;
    }
    video->audio_ds = ds1;
    video->audio_ex = ex1;
    AMediaFormat *afmt = NULL;
    if (open_track(ex1, (size_t)aud_idx, &video->audio_codec, &afmt) < 0) {
      yapfast_video_free(video);
      return NULL;
    }
    update_audio_format(video, afmt);
    int64_t dur_us = -1;
    if (AMediaFormat_getInt64(afmt, AMEDIAFORMAT_KEY_DURATION, &dur_us) && dur_us >= 0) {
      info->duration_ms = dur_us / 1000;
    }
    AMediaFormat_delete(afmt);
    info->sample_rate = video->sample_rate;
    info->channels = video->channels;
    return video;
  }

  if (vid_idx < 0) {
    AMediaExtractor_delete(ex1);
    AMediaDataSource_delete(ds1);
    yapfast_video_free(video);
    return NULL;
  }
  video->video_ds = ds1;
  video->video_ex = ex1;
  AMediaFormat *vfmt = NULL;
  if (open_track(ex1, (size_t)vid_idx, &video->video_codec, &vfmt) < 0) {
    yapfast_video_free(video);
    return NULL;
  }
  update_video_geom(video, vfmt);
  int64_t dur_us = -1;
  if (AMediaFormat_getInt64(vfmt, AMEDIAFORMAT_KEY_DURATION, &dur_us) && dur_us >= 0) {
    info->duration_ms = dur_us / 1000;
  }
  AMediaFormat_delete(vfmt);

  int out_w = video->geom.has_crop
                  ? (video->geom.crop_right - video->geom.crop_left + 1)
                  : video->geom.width;
  int out_h = video->geom.has_crop
                  ? (video->geom.crop_bottom - video->geom.crop_top + 1)
                  : video->geom.height;
  if (out_w <= 0 || out_h <= 0 || out_w > MAX_SIDE || out_h > MAX_SIDE) {
    yapfast_video_free(video);
    return NULL;
  }
  info->width = out_w;
  info->height = out_h;

  if (sound && aud_idx >= 0) {
    AMediaDataSource *ds2 = make_data_source(bridge);
    AMediaExtractor *ex2 = AMediaExtractor_new();
    AMediaFormat *afmt = NULL;
    if (ds2 && ex2 && AMediaExtractor_setDataSourceCustom(ex2, ds2) == AMEDIA_OK &&
        open_track(ex2, (size_t)aud_idx, &video->audio_codec, &afmt) == 0) {
      video->audio_ds = ds2;
      video->audio_ex = ex2;
      update_audio_format(video, afmt);
      if (info->duration_ms < 0 &&
          AMediaFormat_getInt64(afmt, AMEDIAFORMAT_KEY_DURATION, &dur_us) && dur_us >= 0) {
        info->duration_ms = dur_us / 1000;
      }
      AMediaFormat_delete(afmt);
      info->sample_rate = video->sample_rate;
      info->channels = video->channels;
    } else {
      if (afmt) AMediaFormat_delete(afmt);
      if (ex2) AMediaExtractor_delete(ex2);
      if (ds2) AMediaDataSource_delete(ds2);
    }
  }
  return video;
}

static int step_video(struct yapfast_video *video, int64_t *time_ms) {
  if (!video->video_codec || video->video_out_eof) return END;
  AMediaCodecBufferInfo info;
  for (int idle = 0; idle < 500;) {
    feed_extractor(video->video_ex, video->video_codec, &video->video_in_eof);
    ssize_t idx = AMediaCodec_dequeueOutputBuffer(video->video_codec, &info, 4000);
    if (idx == AMEDIACODEC_INFO_OUTPUT_FORMAT_CHANGED) {
      AMediaFormat *fmt = AMediaCodec_getOutputFormat(video->video_codec);
      update_video_geom(video, fmt);
      if (fmt) AMediaFormat_delete(fmt);
      continue;
    }
    if (idx == AMEDIACODEC_INFO_OUTPUT_BUFFERS_CHANGED) {
      continue;
    }
    if (idx == AMEDIACODEC_INFO_TRY_AGAIN_LATER) {
      idle++;
      continue;
    }
    if (idx < 0) return FAILED;

    bool eos = (info.flags & AMEDIACODEC_BUFFER_FLAG_END_OF_STREAM) != 0;
    if (info.size > 0) {
      size_t out_size = 0;
      const uint8_t *out_buf = AMediaCodec_getOutputBuffer(video->video_codec, (size_t)idx, &out_size);
      int ok = -1;
      if (out_buf && out_size >= (size_t)info.offset + (size_t)info.size) {
        if (video->geom.width <= 0 || video->geom.height <= 0) {
          AMediaFormat *fmt = AMediaCodec_getOutputFormat(video->video_codec);
          update_video_geom(video, fmt);
          if (fmt) AMediaFormat_delete(fmt);
        }
        ok = convert_output_to_rgba(video, out_buf + info.offset, (size_t)info.size);
      }
      AMediaCodec_releaseOutputBuffer(video->video_codec, (size_t)idx, false);
      if (eos) video->video_out_eof = true;
      if (ok < 0) return FAILED;
      *time_ms = info.presentationTimeUs >= 0 ? info.presentationTimeUs / 1000 : -1;
      return PICTURE;
    }
    AMediaCodec_releaseOutputBuffer(video->video_codec, (size_t)idx, false);
    if (eos) {
      video->video_out_eof = true;
      return END;
    }
  }
  return FAILED;
}

static int step_audio(struct yapfast_video *video, int64_t *time_ms) {
  if (!video->audio_codec || video->audio_out_eof) return END;
  AMediaCodecBufferInfo info;
  for (int idle = 0; idle < 500;) {
    feed_extractor(video->audio_ex, video->audio_codec, &video->audio_in_eof);
    ssize_t idx = AMediaCodec_dequeueOutputBuffer(video->audio_codec, &info, 4000);
    if (idx == AMEDIACODEC_INFO_OUTPUT_FORMAT_CHANGED) {
      AMediaFormat *fmt = AMediaCodec_getOutputFormat(video->audio_codec);
      update_audio_format(video, fmt);
      if (fmt) AMediaFormat_delete(fmt);
      continue;
    }
    if (idx == AMEDIACODEC_INFO_OUTPUT_BUFFERS_CHANGED) {
      continue;
    }
    if (idx == AMEDIACODEC_INFO_TRY_AGAIN_LATER) {
      idle++;
      continue;
    }
    if (idx < 0) return FAILED;

    bool eos = (info.flags & AMEDIACODEC_BUFFER_FLAG_END_OF_STREAM) != 0;
    if (info.size > 0 && video->channels > 0) {
      size_t out_size = 0;
      const uint8_t *out_buf = AMediaCodec_getOutputBuffer(video->audio_codec, (size_t)idx, &out_size);
      if (out_buf && out_size >= (size_t)info.offset + (size_t)info.size) {
        size_t bps = (video->pcm_encoding == PCM_ENCODING_FLOAT) ? 4 : 2;
        size_t total_samples = (size_t)info.size / bps;
        int frames = (int)(total_samples / (size_t)video->channels);
        if (frames > 0) {
          size_t need = (size_t)frames * (size_t)video->channels;
          if (need > video->sound_cap) {
            float *grown = realloc(video->sound_buf, need * sizeof(float));
            if (!grown) {
              AMediaCodec_releaseOutputBuffer(video->audio_codec, (size_t)idx, false);
              return FAILED;
            }
            video->sound_buf = grown;
            video->sound_cap = need;
          }
          const uint8_t *pcm = out_buf + info.offset;
          if (video->pcm_encoding == PCM_ENCODING_FLOAT) {
            memcpy(video->sound_buf, pcm, need * sizeof(float));
          } else {
            const int16_t *s16 = (const int16_t *)pcm;
            for (size_t s = 0; s < need; s++) {
              video->sound_buf[s] = (float)s16[s] / 32768.0f;
            }
          }
          video->sound_frames = frames;
          AMediaCodec_releaseOutputBuffer(video->audio_codec, (size_t)idx, false);
          if (eos) video->audio_out_eof = true;
          *time_ms = info.presentationTimeUs >= 0 ? info.presentationTimeUs / 1000 : -1;
          return SOUND;
        }
      }
    }
    AMediaCodec_releaseOutputBuffer(video->audio_codec, (size_t)idx, false);
    if (eos) {
      video->audio_out_eof = true;
      return END;
    }
  }
  return FAILED;
}

int yapfast_video_next(struct yapfast_video *video, int64_t *time_ms, int pictures) {
  if (!video || !time_ms) return FAILED;
  *time_ms = -1;
  if (!pictures) {
    return step_audio(video, time_ms);
  }
  bool can_video = video->video_codec && !video->video_out_eof;
  bool can_audio = video->audio_codec && !video->audio_out_eof;
  if (!can_video && !can_audio) return END;

  if (can_video && can_audio) {
    int64_t vts = video->video_in_eof ? INT64_MAX : AMediaExtractor_getSampleTime(video->video_ex);
    int64_t ats = video->audio_in_eof ? INT64_MAX : AMediaExtractor_getSampleTime(video->audio_ex);
    if (ats >= 0 && (vts < 0 || ats < vts)) {
      int r = step_audio(video, time_ms);
      if (r != END) return r;
      return step_video(video, time_ms);
    }
    int r = step_video(video, time_ms);
    if (r != END) return r;
    return step_audio(video, time_ms);
  }
  if (can_video) return step_video(video, time_ms);
  return step_audio(video, time_ms);
}

int yapfast_video_picture(struct yapfast_video *video, unsigned char *rgba, int width, int height) {
  if (!video || !rgba || !video->pic_rgba || width <= 0 || height <= 0) return -1;
  if (video->pic_width == width && video->pic_height == height) {
    memcpy(rgba, video->pic_rgba, (size_t)width * (size_t)height * 4);
    return 0;
  }
  /* ABGR and ARGB are 4 bytes per pixel; channel permutation is preserved through linear filtering. */
  return ARGBScale(video->pic_rgba, video->pic_width * 4, video->pic_width, video->pic_height,
                   rgba, width * 4, width, height, kFilterBilinear);
}

const float *yapfast_video_sound(const struct yapfast_video *video, int *frames, int *rate,
                                 int *channels) {
  if (!video || !video->sound_buf || video->sound_frames <= 0) return NULL;
  *frames = video->sound_frames;
  *rate = video->sample_rate;
  *channels = video->channels;
  return video->sound_buf;
}

int yapfast_video_seek(struct yapfast_video *video, int64_t time_ms) {
  if (!video) return -1;
  int64_t time_us = time_ms * 1000;
  if (video->video_ex && video->video_codec) {
    if (AMediaExtractor_seekTo(video->video_ex, time_us, AMEDIAEXTRACTOR_SEEK_CLOSEST_SYNC) !=
        AMEDIA_OK) {
      return -1;
    }
    AMediaCodec_flush(video->video_codec);
    video->video_in_eof = false;
    video->video_out_eof = false;
    video->pic_width = 0;
    video->pic_height = 0;
  }
  if (video->audio_ex && video->audio_codec) {
    if (AMediaExtractor_seekTo(video->audio_ex, time_us, AMEDIAEXTRACTOR_SEEK_CLOSEST_SYNC) !=
        AMEDIA_OK) {
      return -1;
    }
    AMediaCodec_flush(video->audio_codec);
    video->audio_in_eof = false;
    video->audio_out_eof = false;
    video->sound_frames = 0;
  }
  return 0;
}
