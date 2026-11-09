/* Encoding a picture where the card already holds it, with FFmpeg's VA-API
 * encoder from vendor/ffmpeg-src: the Linux twin of native/d3d11enc.c's
 * NVENC-on-a-texture path. A DMA-BUF is wrapped into a VA-API surface and the
 * encoder reads that surface, so no pixel of the picture comes back to the
 * processor and nothing is uploaded; the only copy a picture still costs is
 * its bitstream out of the driver.
 *
 * The import is the wrap and it never copies: the AVDRMFrameDescriptor this
 * file builds out of the caller's `struct yapfast_dmabuf_picture` names the one
 * DRM object the picture is in, and av_hwframe_map on a DRM PRIME frame brings
 * that object in as an NV12 VA-API surface on this encoder's display (FFmpeg's
 * vaapi_map_from_drm, in libavutil/hwcontext_vaapi.c), which is the surface
 * avcodec_send_frame is given. The kernel takes its own reference to the
 * buffer as it is imported, and the surface is destroyed when the encoder has
 * done with the picture, so the descriptor's fds stay the caller's: this file
 * never closes one it was handed.
 *
 * The settings are native/hwenc.c's VA-API branch (`open_encoder`), value for
 * value, because a stream's picture must not depend on which way its frames
 * reached the encoder: Main profile for H264, a keyframe every three seconds
 * and when asked, no B-frames, capped VBR over half a second, one picture in
 * flight. This file owns the session those settings open, so the two lists are
 * changed together, and `yapfast_hwenc_dmabuf_settings` is here for Rust to
 * hold this one to what the stream was measured with without a GPU in the
 * shape of the question.
 *
 * What this file can't take is a picture that isn't one NV12 picture in one
 * buffer: that is decided in Rust, before the card is asked
 * (`Encoder::importable`), since nothing here could import it as the same
 * picture. A machine whose VA-API encoder won't open says why through
 * `yapfast_hwenc_dmabuf_error`, and the stream falls back to the path fed from
 * the processor with one line in the log rather than a silent copy. */

#include <stdarg.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "libavcodec/avcodec.h"
#include "libavutil/error.h"
#include "libavutil/frame.h"
#include "libavutil/hwcontext.h"
#include "libavutil/hwcontext_drm.h"
#include "libavutil/mathematics.h"
#include "libavutil/opt.h"
#include "libavutil/pixdesc.h"
#include "libavutil/pixfmt.h"

/* The codecs, as Rust numbers them, and the picture in a DMA-BUF as Rust maps
 * it: native/hwenc.c's numbers and Rust's `DmabufPicture`. */
struct yapfast_dmabuf_picture {
  int width, height;
  /* The one DRM object the picture is in, how much of it the picture needs,
   * and the modifier the driver laid it out with. */
  int fd;
  uint64_t size;
  uint64_t modifier;
  /* NV12's two planes, in that object: where each starts and how many bytes
   * apart its rows are. */
  uint32_t offset[2];
  uint32_t pitch[2];
};

/* DRM's fourcc, as the kernel and the descriptor name a format: NV12 is the
 * one picture a VA-API surface is made of without a conversion, and libdrm's
 * header isn't read here — this needs the one name. */
#define YAPFAST_FOURCC(a, b, c, d)                                                 \
  ((uint32_t)(a) | ((uint32_t)(b) << 8) | ((uint32_t)(c) << 16) | ((uint32_t)(d) << 24))
#define YAPFAST_DRM_FORMAT_NV12 YAPFAST_FOURCC('N', 'V', '1', '2')

struct yapfast_hwenc_dmabuf {
  AVCodecContext *codec;
  /* The VA-API display the surfaces are imported on. */
  AVBufferRef *device;
  /* The picture as it arrived: a DRM PRIME frame whose data[0] is `desc`, and
   * the descriptor it points at, which lives as long as this encoder so that a
   * frame FFmpeg keeps a reference to never points at freed memory. Both are
   * rewritten for each picture: what an import reads of them it reads during
   * the map, and the surface it made keeps the buffer itself. */
  AVFrame *drm;
  AVDRMFrameDescriptor desc;
  /* That picture wrapped as a VA-API surface on `device`: the frame the
   * encoder reads, and the one reference the FFmpeg packet keeps alive until
   * the picture is encoded. */
  AVFrame *surface;
  AVPacket *packet;
  /* Pictures handed back for an earlier picture than the one given: a driver
   * holding one back, whose time has gone by, so it is dropped rather than sent
   * under the next picture's time. */
  unsigned late;
  /* Whether a picture was dropped that way: it is one the pictures after it
   * reference, and the viewer never got it, so the next picture is asked for as
   * a keyframe rather than as a delta built on what wasn't decoded. */
  int recover;
  /* The format, colour and time base the encoder took, and the rate asked for,
   * for Rust to log once. */
  char config[80];
  /* What went wrong, in words: the reason a picture wasn't encoded where it
   * lay, for the log line Rust writes rather than for a copy in its place. */
  char error[192];
  /* The access unit the last picture produced, kept until Rust copies it into
   * the buffer it holds: FFmpeg's packets are unreferenced as soon as they are
   * read, and encoding the picture again to get it into a larger buffer would
   * send a frame referencing one the receiver never got. */
  unsigned char *staged;
  size_t staged_capacity;
  size_t staged_size;
};

/* The reason a picture wasn't encoded where it lay, for Rust's log line, or
 * the encoder's own last word about it. */
static _Thread_local char g_error[192];

/* FFmpeg's first error on this thread since it was cleared, for saying why a
 * device or an encoder didn't open: the cause, where later ones only say what
 * failed because of it. libva says the useful part — a driver it couldn't load,
 * a node it couldn't open — through FFmpeg's own log. */
static _Thread_local char g_reason[160];

static void keep_reason(void *context, int level, const char *format, va_list args) {
  (void)context;
  if (level > AV_LOG_ERROR || g_reason[0]) return;
  vsnprintf(g_reason, sizeof g_reason, format, args);
  size_t length = strlen(g_reason);
  if (length && g_reason[length - 1] == '\n') g_reason[length - 1] = 0;
}

/* FFmpeg's log level is one for the whole library, and native/hwenc.c sets its
 * own: each call of ours that may log runs at ERROR and puts back the level the
 * process had, so that a stream encoded here quiets nobody else. */
static int hush(void) {
  const int level = av_log_get_level();
  av_log_set_level(AV_LOG_ERROR);
  return level;
}

static void unhush(int level) { av_log_set_level(level); }

/* `what`, and the reason FFmpeg or libva gave, where one was logged. */
static void fail_code(struct yapfast_hwenc_dmabuf *h, const char *what, int code) {
  char why[96] = {0};
  av_strerror(code, why, sizeof why);
  if (g_reason[0]) {
    snprintf(h->error, sizeof h->error, "%s (%s: %s)", what, why, g_reason);
  } else {
    snprintf(h->error, sizeof h->error, "%s (%s)", what, why);
  }
  g_reason[0] = 0;
}

static void fail_text(struct yapfast_hwenc_dmabuf *h, const char *what) {
  snprintf(h->error, sizeof h->error, "%s", what);
}

/* Keeps `size` bytes of `bytes`, at `at` into the staged access unit, for Rust
 * to copy out. The buffer grows to the largest picture seen and no further, so
 * a stream asks for memory only until its pictures settle. Returns 0 when
 * memory ran out. */
static int stage(struct yapfast_hwenc_dmabuf *h, size_t at, const unsigned char *bytes,
                 size_t size) {
  if (at + size > h->staged_capacity) {
    unsigned char *larger = realloc(h->staged, at + size);
    if (!larger) return 0;
    h->staged = larger;
    h->staged_capacity = at + size;
  }
  memcpy(h->staged + at, bytes, size);
  return 1;
}

/* native/hwenc.c's VA-API settings, in the order they are set: capped VBR,
 * which leaves a calm screen below the cap rather than padding it, and one
 * picture in flight so a picture doesn't wait for the next. Both are settings
 * the stream can't do without, which is how that file opens them too: a driver
 * that can't do either fails the open rather than quietly encoding worse or
 * later. */
static const char *const VAAPI_SETTINGS[][2] = {
    {"rc_mode", "VBR"},
    {"async_depth", "1"},
};

/* The keyframe interval, in frames: three seconds, which is how long a viewer
 * who joins waits for a picture, as native/hwenc.c opens it. */
static int gop_frames(unsigned fps) { return (int)fps * 3; }

/* How long the cap is spent over, in milliseconds: half a second, which is
 * `kbps` thousand bits a second for that long, in bits. */
enum { BUFFER_MS = 500 };

/* The rate the stream aims at: the target, and the same number as the cap over
 * half a second, which is the capped VBR native/hwenc.c opens with. */
static void set_rate(AVCodecContext *codec, unsigned kbps) {
  codec->bit_rate = (int64_t)kbps * 1000;
  codec->rc_max_rate = (int64_t)kbps * 1000;
  codec->rc_buffer_size = (int)(kbps * BUFFER_MS);
}

/* Sets each setting in turn, stopping at the first the driver refuses, which is
 * the open's failure with that setting named in the reason. */
static int set_options(struct yapfast_hwenc_dmabuf *h) {
  for (size_t i = 0; i < sizeof VAAPI_SETTINGS / sizeof *VAAPI_SETTINGS; i++) {
    int ret = av_opt_set(h->codec->priv_data, VAAPI_SETTINGS[i][0], VAAPI_SETTINGS[i][1], 0);
    if (ret >= 0) continue;
    /* FFmpeg's option handling logs nothing, so the setting the stream needed
     * has to be named here to say why it didn't open. */
    if (!g_reason[0])
      snprintf(g_reason, sizeof g_reason, "%s=%s: %s", VAAPI_SETTINGS[i][0], VAAPI_SETTINGS[i][1],
               ret == AVERROR_OPTION_NOT_FOUND ? "this FFmpeg has no such setting" : "refused");
    return ret;
  }
  return 0;
}

static void close_encoder(struct yapfast_hwenc_dmabuf *h) {
  avcodec_free_context(&h->codec);
  av_frame_free(&h->surface);
  av_frame_free(&h->drm);
  av_packet_free(&h->packet);
  av_buffer_unref(&h->device);
  free(h->staged);
  h->staged = NULL;
  h->staged_capacity = h->staged_size = 0;
}

void yapfast_hwenc_dmabuf_free(struct yapfast_hwenc_dmabuf *h) {
  if (!h) return;
  close_encoder(h);
  av_free(h);
}

/* The knobs this encoder is opened with, as far as FFmpeg can be told them, for
 * Rust to hold them to native/hwenc.c's VA-API branch: the profile, the
 * keyframe interval, the B-frames, the two priv-data settings, and the rate
 * shape that file's `set_rate` makes. Returns 0, or -1 when the text doesn't
 * fit. */
int yapfast_hwenc_dmabuf_settings(unsigned fps, unsigned kbps, char *out, size_t size) {
  if (!out || !size) return 0;
  int written = snprintf(out, size,
                         "profile=main(required) gop=%d(required) b_frames=0(required) "
                         "rc_mode=%s(required) async_depth=%s(required) "
                         "rate=%u kbit/s capped at %u kbit/s over %d ms(required)",
                         gop_frames(fps), VAAPI_SETTINGS[0][1], VAAPI_SETTINGS[1][1], kbps, kbps,
                         (int)BUFFER_MS);
  return written > 0 && (size_t)written < size ? 0 : -1;
}

/* The VA-API display this encoder imports on: `device`, a render node, and
 * `driver`, the libva driver for its kernel driver, which are the two
 * native/hwenc.c opened the stream's encoder on. Naming the driver overrides
 * LIBVA_DRIVER_NAME, which NixOS and others set to nvidia for browsers. Both
 * NULL leaves FFmpeg to open the machine's device and libva to pick the driver
 * from that variable, which is what a report without a device in it gets. */
static int open_device(struct yapfast_hwenc_dmabuf *h, const char *device, const char *driver) {
  AVDictionary *options = NULL;
  if (driver) av_dict_set(&options, "driver", driver, 0);
  int ret = av_hwdevice_ctx_create(&h->device, AV_HWDEVICE_TYPE_VAAPI, device, options, 0);
  av_dict_free(&options);
  if (ret < 0) {
    fail_code(h, "the card's VA-API device wasn't opened for the picture where it lies", ret);
    return ret;
  }
  return 0;
}

/* The surfaces this encoder's pictures are imported as: VA-API frames of NV12
 * named for the picture's size, on the device above. The encoder holds this to
 * describe the frames it is given and to find its device, and a surface is made
 * per picture by the import rather than taken from a pool here. */
static int describe_surfaces(struct yapfast_hwenc_dmabuf *h, unsigned width, unsigned height) {
  AVBufferRef *frames = av_hwframe_ctx_alloc(h->device);
  if (!frames) {
    fail_text(h, "the card's surfaces weren't described");
    return AVERROR(ENOMEM);
  }
  AVHWFramesContext *pool = (AVHWFramesContext *)frames->data;
  pool->format = AV_PIX_FMT_VAAPI;
  pool->sw_format = AV_PIX_FMT_NV12;
  pool->width = (int)width;
  pool->height = (int)height;
  pool->initial_pool_size = 2;
  const int level = hush();
  g_reason[0] = 0;
  int ret = av_hwframe_ctx_init(frames);
  unhush(level);
  if (ret >= 0) h->codec->hw_frames_ctx = frames;
  else av_buffer_unref(&frames);
  if (ret < 0) fail_code(h, "the card wouldn't hold the stream's surfaces", ret);
  return ret;
}

/* Opens `encoder` — FFmpeg's name for the VA-API encoder this stream runs, such
 * as "h264_vaapi" — with native/hwenc.c's VA-API settings, reading surfaces on
 * the device `device` and `driver` name. The report says which encoder and
 * which device, as native/hwenc.c's own does, and the error says why not. */
struct yapfast_hwenc_dmabuf *yapfast_hwenc_dmabuf_new(unsigned codec_id, unsigned width,
                                                      unsigned height, unsigned fps,
                                                      unsigned kbps, const char *encoder,
                                                      const char *device, const char *driver,
                                                      char *report, size_t report_size) {
  if (report_size) report[0] = 0;
  g_error[0] = 0;
  g_reason[0] = 0;
  /* Installed for the whole process's FFmpeg, as native/hwenc.c installs its
   * own: what this file's open and pictures log is kept for the reason a
   * picture wasn't encoded where it lay, rather than printed beside the
   * stream's own log. */
  av_log_set_level(AV_LOG_ERROR);
  av_log_set_callback(keep_reason);
  if (codec_id > 2) {
    if (report_size) snprintf(report, report_size, "no such codec as %u", codec_id);
    return NULL;
  }
  const AVCodec *found = encoder ? avcodec_find_encoder_by_name(encoder) : NULL;
  if (!found) {
    if (report_size)
      snprintf(report, report_size, "%s: not in this FFmpeg, so it can't read a DMA-BUF",
               encoder ? encoder : "that encoder");
    return NULL;
  }
  struct yapfast_hwenc_dmabuf *h = av_mallocz(sizeof *h);
  if (!h) {
    if (report_size) snprintf(report, report_size, "out of memory");
    return NULL;
  }
  h->codec = avcodec_alloc_context3(found);
  if (!h->codec) {
    fail_text(h, "the encoder's memory wasn't there");
    goto failed;
  }
  AVCodecContext *codec = h->codec;
  codec->width = (int)width;
  codec->height = (int)height;
  codec->time_base = (AVRational){1, (int)fps};
  codec->framerate = (AVRational){(int)fps, 1};
  /* A new viewer waits at most this long for a picture. */
  codec->gop_size = gop_frames(fps);
  codec->max_b_frames = 0;
  codec->pix_fmt = AV_PIX_FMT_VAAPI;
  /* What the pixels are, which a picture in a DMA-BUF carries just as one from
   * the processor does: native/hwenc.c declares the same four numbers for the
   * same reason, and a conversion that changed would have to change both. */
  codec->color_primaries = AVCOL_PRI_SMPTE170M;
  codec->color_trc = AVCOL_TRC_SMPTE170M;
  codec->colorspace = AVCOL_SPC_SMPTE170M;
  codec->color_range = AVCOL_RANGE_MPEG;
  set_rate(codec, kbps);
  if (codec_id == 0) codec->profile = AV_PROFILE_H264_MAIN;
  int ret = set_options(h);
  if (ret < 0) {
    fail_code(h, "the card's encoder refused a setting the stream needs", ret);
    goto failed;
  }
  if (open_device(h, device, driver) < 0) goto failed;
  if (describe_surfaces(h, width, height) < 0) goto failed;
  h->drm = av_frame_alloc();
  h->surface = av_frame_alloc();
  h->packet = av_packet_alloc();
  if (!h->drm || !h->surface || !h->packet) {
    fail_text(h, "the encoder's memory wasn't there");
    goto failed;
  }
  /* Frames are timed by the stream's clock in 90 kHz units, rescaled into this
   * time base, which has to exist to rescale into. */
  if (codec->time_base.num <= 0 || codec->time_base.den <= 0) {
    fail_text(h, "the encoder kept no time base");
    goto failed;
  }
  const int level = hush();
  g_reason[0] = 0;
  ret = avcodec_open2(codec, found, NULL);
  unhush(level);
  if (ret < 0) {
    fail_code(h, "the card's encoder wouldn't open to read a picture where it lies", ret);
    goto failed;
  }
  /* Read back rather than assumed, as native/hwenc.c reads it: an encoder that
   * kept none of what it was told is reported as claiming no colour at all, so
   * the log never names a colour the stream doesn't have. */
  const char *space = av_color_space_name(codec->colorspace);
  if (codec->colorspace == AVCOL_SPC_UNSPECIFIED) space = NULL;
  const char *range = codec->color_range == AVCOL_RANGE_JPEG   ? "full"
                      : codec->color_range == AVCOL_RANGE_MPEG ? "limited"
                                                              : "no";
  snprintf(h->config, sizeof h->config, "%s, %s %s range, %d/%d, %u kbit/s",
           av_get_pix_fmt_name(AV_PIX_FMT_NV12), space ? space : "no colour space", range,
           codec->time_base.num, codec->time_base.den, kbps);
  if (report_size) {
    const char *slash = device ? strrchr(device, '/') : NULL;
    if (device && driver)
      snprintf(report, report_size, "%s (%s, %s)", found->name, slash ? slash + 1 : device,
               driver);
    else
      snprintf(report, report_size, "%s", found->name);
  }
  return h;

failed:
  if (h->error[0]) snprintf(g_error, sizeof g_error, "%s", h->error);
  if (report_size) snprintf(report, report_size, "%s", h->error);
  close_encoder(h);
  av_free(h);
  return NULL;
}

/* One picture, as the card holds it: the DRM object the caller's descriptor
 * names, with NV12's two planes in it, which is the one shape FFmpeg imports as
 * a VA-API surface without a conversion. */
static void describe_picture(struct yapfast_hwenc_dmabuf *h,
                             const struct yapfast_dmabuf_picture *picture) {
  AVDRMFrameDescriptor *desc = &h->desc;
  memset(desc, 0, sizeof *desc);
  desc->nb_objects = 1;
  desc->objects[0].fd = picture->fd;
  desc->objects[0].size = picture->size;
  desc->objects[0].format_modifier = picture->modifier;
  desc->nb_layers = 1;
  desc->layers[0].format = YAPFAST_DRM_FORMAT_NV12;
  desc->layers[0].nb_planes = 2;
  for (int plane = 0; plane < 2; plane++) {
    desc->layers[0].planes[plane].object_index = 0;
    desc->layers[0].planes[plane].offset = picture->offset[plane];
    desc->layers[0].planes[plane].pitch = picture->pitch[plane];
  }
  h->drm->format = AV_PIX_FMT_DRM_PRIME;
  h->drm->width = picture->width;
  h->drm->height = picture->height;
  h->drm->data[0] = (uint8_t *)desc;
}

/* Encodes one picture the card holds, whose time is `pts` in 90 kHz units: the
 * stream's own clock, which Rust also labels the picture's packets with. The
 * descriptor is imported as a VA-API surface on this encoder's display and the
 * encoder is given that surface, so no pixel of the picture is copied; what it
 * produced is kept for `yapfast_hwenc_dmabuf_drain` to copy the bitstream out
 * of. Returns the size produced, 0 when the encoder gave no picture, or a
 * negative number on failure, which `yapfast_hwenc_dmabuf_error` explains. */
long yapfast_hwenc_dmabuf_encode(struct yapfast_hwenc_dmabuf *h,
                                 const struct yapfast_dmabuf_picture *picture, long long pts,
                                 int keyframe, int *is_key) {
  if (!h || !h->codec || !picture) return -1;
  h->error[0] = 0;
  const int64_t at = av_rescale_q((int64_t)pts, (AVRational){1, 90000}, h->codec->time_base);
  /* The picture as it arrived, then that picture wrapped: the map waits for
   * nothing and copies nothing, since the surface is the buffer it names. A
   * mapping that would have to copy is one this refuses, as MAP_DIRECT says. */
  av_frame_unref(h->drm);
  describe_picture(h, picture);
  av_frame_unref(h->surface);
  h->surface->format = AV_PIX_FMT_VAAPI;
  h->surface->hw_frames_ctx = av_buffer_ref(h->codec->hw_frames_ctx);
  if (!h->surface->hw_frames_ctx) {
    fail_text(h, "the picture had no surfaces to be imported into");
    return -1;
  }
  const int level = hush();
  g_reason[0] = 0;
  int ret = av_hwframe_map(h->surface, h->drm, AV_HWFRAME_MAP_READ | AV_HWFRAME_MAP_DIRECT);
  unhush(level);
  if (ret < 0) {
    fail_code(h, "the picture wasn't imported as a surface on the card", ret);
    return -1;
  }
  h->surface->pts = at;
  /* A dropped picture is one the pictures after it are built on, so the next
   * picture starts a new sequence: asked for as a keyframe, a viewer has a
   * picture to decode again instead of waiting out the interval on pictures
   * whose reference it never received. */
  h->surface->pict_type =
      keyframe || h->recover ? AV_PICTURE_TYPE_I : AV_PICTURE_TYPE_NONE;
  h->recover = 0;
  const int sent = hush();
  g_reason[0] = 0;
  ret = avcodec_send_frame(h->codec, h->surface);
  unhush(sent);
  if (ret < 0) {
    fail_code(h, "the card's encoder wouldn't take the picture where it lies", ret);
    return -1;
  }
  size_t size = 0;
  /* Whatever the last call left is this picture's to replace, copied out or
   * not: one picture is held at a time. */
  h->staged_size = 0;
  *is_key = 0;
  for (;;) {
    ret = avcodec_receive_packet(h->codec, h->packet);
    if (ret < 0) break;
    /* The encoder takes one picture and gives back one packet for it, timed as
     * the picture was. A packet timed for another is one a driver held back:
     * its picture is already gone and its time with it, so sending it now would
     * pass it off as this picture's, and it is dropped instead. */
    if (h->packet->pts != AV_NOPTS_VALUE && h->packet->pts != at) {
      h->late++;
      h->recover = 1;
      av_packet_unref(h->packet);
      continue;
    }
    if (!stage(h, size, h->packet->data, (size_t)h->packet->size)) {
      av_packet_unref(h->packet);
      fail_text(h, "the picture's bitstream didn't fit");
      return -1;
    }
    size += (size_t)h->packet->size;
    *is_key |= (h->packet->flags & AV_PKT_FLAG_KEY) != 0;
    av_packet_unref(h->packet);
  }
  if (ret != AVERROR(EAGAIN)) {
    fail_code(h, "the card's encoder gave no picture back", ret);
    return -1;
  }
  h->staged_size = size;
  return (long)size;
}

/* Copies the access unit the last picture produced into `out`, which holds it,
 * and gives up the copy: the buffer stays for the next picture. Rust grows its
 * buffer to the size the encode reported before calling this once, so a picture
 * is never encoded twice to fit one. Returns the size copied, or -1 when there
 * is nothing to copy or `out` is too small. */
long yapfast_hwenc_dmabuf_drain(struct yapfast_hwenc_dmabuf *h, unsigned char *out,
                                size_t capacity) {
  size_t size = h->staged_size;
  if (!size || size > capacity) return -1;
  memcpy(out, h->staged, size);
  h->staged_size = 0;
  return (long)size;
}

/* The memory this encoder keeps for the access unit it produces, reused for the
 * next one: one picture's worth, however many pictures go through it. */
size_t yapfast_hwenc_dmabuf_staging(struct yapfast_hwenc_dmabuf *h) {
  return h->staged_capacity;
}

/* How many packets the encoder handed back for an earlier picture than the one
 * it was given, and which were dropped rather than sent under a later
 * picture's time: a driver that falls behind, worth warning about. */
unsigned yapfast_hwenc_dmabuf_late(struct yapfast_hwenc_dmabuf *h) { return h->late; }

/* Changes the bitrate aimed for, from the next picture on, without a keyframe:
 * patches/ in vendor/ffmpeg-src make VA-API's running session follow it, as
 * native/hwenc.c does. Returns 0: the context's aim is what is set here, and
 * the driver's answer comes with the pictures after, as FFmpeg's own error. */
int yapfast_hwenc_dmabuf_set_bitrate(struct yapfast_hwenc_dmabuf *h, unsigned kbps) {
  if (!h || !h->codec) return -1;
  set_rate(h->codec, kbps);
  return 0;
}

/* The format, colour and time base this encoder took, and the rate asked for,
 * into `config`: what Rust can't see, since it hands over pictures the card
 * made and the encoder decides the rest. */
void yapfast_hwenc_dmabuf_config(struct yapfast_hwenc_dmabuf *h, char *config, size_t size) {
  if (size) snprintf(config, size, "%s", h ? h->config : "");
}

/* Why the last picture wasn't encoded where it lay, or why this encoder didn't
 * open, in words: one line for the log of the caller that falls back to the
 * path fed from the processor. */
const char *yapfast_hwenc_dmabuf_error(const struct yapfast_hwenc_dmabuf *h) {
  if (h && h->error[0]) return h->error;
  return g_error;
}
