/* Decoding a watched stream on the graphics card with FFmpeg's VA-API
 * decoder, from vendor/ffmpeg-src, the Linux twin of native/hwdec.c: the same
 * access units in, and a picture left in a VA surface on the card, exported
 * as a DMA-BUF the tile pass imports. No picture comes back to the processor
 * here, and nothing is copied: av_hwframe_map with AV_HWFRAME_MAP_DIRECT hands
 * over the surface's own buffer, and the descriptor's fds stay open until the
 * caller says it is done with the picture.
 *
 * The surfaces are described once, when the stream's first frame says how
 * large its picture is, so a stream allocates no surfaces per frame; a stream
 * whose pictures aren't 8-bit 4:2:0 — a 10-bit one, which the tile pass has no
 * curve for — is refused before any surface is made.
 *
 * C, because it reads nothing but FFmpeg's C headers: FFmpeg's public API,
 * not libva's, and not any of FFmpeg's own VA-API internals. */

#include <limits.h>
#include <stdarg.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "libavcodec/avcodec.h"
#include "libavcodec/codec.h"
#include "libavutil/error.h"
#include "libavutil/frame.h"
#include "libavutil/hwcontext.h"
#include "libavutil/hwcontext_drm.h"
#include "libavutil/pixfmt.h"

#include "hwdec_vaapi.h"

/* The widest or tallest picture a stream may send, as in decode.c. */
enum { MAX_SIDE = 8192 };

/* The surfaces one stream's decoder holds: H264's sixteen reference pictures
 * at most, the rest giving the card room to decode ahead. Described once, when
 * the first frame gives the size, so a stream that keeps its size takes no
 * surface description after that. */
enum { POOL = 20 };

/* How many pictures a decoder may hold before it takes a frame: a reorder
 * queue is a handful deep, and a drain that never ends is a loop that never
 * ends. */
enum { MOST_HELD = 8 };

/* DRM's fourcc, which is what the descriptor's buffers are named in: the
 * picture's format as the kernel and the window system spell it. libdrm's
 * header isn't read here — this needs the four names and no more. */
#define YAPFAST_FOURCC(a, b, c, d)                                                 \
  ((uint32_t)(a) | ((uint32_t)(b) << 8) | ((uint32_t)(c) << 16) | ((uint32_t)(d) << 24))
#define YAPFAST_DRM_FORMAT_NV12 YAPFAST_FOURCC('N', 'V', '1', '2')
#define YAPFAST_DRM_FORMAT_R8 YAPFAST_FOURCC('R', '8', ' ', ' ')
#define YAPFAST_DRM_FORMAT_GR88 YAPFAST_FOURCC('G', 'R', '8', '8')
#define YAPFAST_DRM_FORMAT_P010 YAPFAST_FOURCC('P', '0', '1', '0')

struct yapfast_vadec {
  AVCodecContext *context;
  AVBufferRef *device_ref;
  AVPacket *packet;
  /* The newest picture the decoder gave, on the card, and the one after it
   * while the decoder's output is drained: a decoder that reorders hands over
   * the earlier frames first, and only the last one is shown. */
  AVFrame *frame;
  AVFrame *scratch;
  /* The newest picture's DMA-BUF: the DRM PRIME frame av_hwframe_map made of
   * `frame`, which keeps both the surface and the fds alive until the caller
   * says it is done with them. */
  AVFrame *drm;
  unsigned char *padded;
  size_t padded_size;
  int hint_width;
  int hint_height;
  /* Set where the surfaces couldn't be described because the stream's
   * pictures aren't ones the tile pass shows, which yapfast_vadec_decode
   * answers with -2 rather than as a plain failure. */
  int refused;
  char error[256];
};

/* The reason a decoder that couldn't be made wasn't, kept for the caller that
 * gets the null back: hwdec_vaapi.h promises it from yapfast_vadec_error(NULL). */
static _Thread_local char g_error[256];

/* FFmpeg's first error on this thread since it was cleared, for saying why a
 * device or a decoder didn't open: the cause, where later ones only say what
 * failed because of it. libva says the useful part — a driver it couldn't
 * load, a node it couldn't open — through FFmpeg's own log. */
static _Thread_local char g_reason[192];

static void keep_reason(void *context, int level, const char *format, va_list args) {
  (void)context;
  if (level > AV_LOG_ERROR || g_reason[0]) return;
  vsnprintf(g_reason, sizeof g_reason, format, args);
  size_t length = strlen(g_reason);
  if (length && g_reason[length - 1] == '\n') g_reason[length - 1] = 0;
}

static void fail_text(struct yapfast_vadec *decoder, const char *what) {
  snprintf(decoder->error, sizeof decoder->error, "%s", what);
}

/* `what`, and the reason FFmpeg or libva gave, where one was logged: a stream
 * that fell to the software decoder says why the card's didn't take it. */
static void fail_code(struct yapfast_vadec *decoder, const char *what, int code) {
  char why[128] = {0};
  av_strerror(code, why, sizeof why);
  if (g_reason[0]) {
    snprintf(decoder->error, sizeof decoder->error, "%s (%s: %s)", what, why, g_reason);
  } else {
    snprintf(decoder->error, sizeof decoder->error, "%s (%s)", what, why);
  }
  g_reason[0] = 0;
}

/* FFmpeg's log level is one for the whole library, and native/hevc.c and
 * native/hwenc.c set their own: each call of ours that may log runs at ERROR
 * and puts back the level the process had, so that a stream decoded here quiets
 * nobody else. */
static int hush(void) {
  const int level = av_log_get_level();
  av_log_set_level(AV_LOG_ERROR);
  return level;
}

static void unhush(int level) { av_log_set_level(level); }

static const char *codec_name(int codec) {
  switch (codec) {
    case YAPFAST_VA_H264:
      return "H264";
    case YAPFAST_VA_HEVC:
      return "H265";
    case YAPFAST_VA_AV1:
      return "AV1";
    default:
      return "this codec";
  }
}

static enum AVCodecID codec_id(int codec) {
  switch (codec) {
    case YAPFAST_VA_H264:
      return AV_CODEC_ID_H264;
    case YAPFAST_VA_HEVC:
      return AV_CODEC_ID_HEVC;
    case YAPFAST_VA_AV1:
      return AV_CODEC_ID_AV1;
    default:
      return AV_CODEC_ID_NONE;
  }
}

/* Whether this FFmpeg was built with the codec's VA-API decoder, which is a
 * hwaccel in its configure (--enable-hwaccel=h264_vaapi and the rest) and says
 * so through the decoder's own hardware configs. Asked before a device is
 * opened, so a build without it says exactly that rather than trying and
 * failing somewhere FFmpeg's reason reads like the card's. */
static int has_card_decoder(const AVCodec *implementation) {
  for (int index = 0;; index++) {
    const AVCodecHWConfig *config = avcodec_get_hw_config(implementation, index);
    if (!config) return 0;
    if (config->pix_fmt == AV_PIX_FMT_VAAPI &&
        (config->methods & AV_CODEC_HW_CONFIG_METHOD_HW_DEVICE_CTX)) {
      return 1;
    }
  }
}

/* What the stream's pictures are: the coded size — the picture rounded up to
 * whole macroblocks, 1088 rows for a 1080p H264 stream — which is what the
 * decoder codes into and what the surfaces are made of. NV12 is the surface
 * format, since that is what the tile pass reads; a stream whose pictures are
 * anything else is refused here, before a surface exists, and the reason says
 * which. 0 says the stream hasn't said, or said one the pass can't show. */
static int surface_params(struct yapfast_vadec *decoder, const AVCodecContext *context, int *width,
                          int *height, enum AVPixelFormat *format) {
  decoder->refused = 0;
  int coded_width = context->coded_width;
  int coded_height = context->coded_height;
  if (coded_width <= 0) coded_width = context->width > 0 ? context->width : decoder->hint_width;
  if (coded_height <= 0) {
    coded_height = context->height > 0 ? context->height : decoder->hint_height;
  }
  if (coded_width <= 0 || coded_height <= 0 || coded_width > MAX_SIDE || coded_height > MAX_SIDE) {
    fail_text(decoder, "the stream never said how large its pictures are");
    return 0;
  }
  *width = coded_width;
  *height = coded_height;
  /* The format the decoder would give in the processor's memory, which
   * ff_get_format recorded before it asked this file for one. A 10-bit
   * stream's is P010, and 9- and 12-bit, 4:2:2 and 4:4:4 streams have theirs:
   * pictures the tile pass has no curve, matrix or layout for, which the
   * software decoders hand back as SDR already. */
  switch (context->sw_pix_fmt) {
    case AV_PIX_FMT_YUV420P:
    case AV_PIX_FMT_YUVJ420P:
    case AV_PIX_FMT_NV12:
      *format = AV_PIX_FMT_NV12;
      return 1;
    default:
      decoder->refused = 1;
      fail_text(decoder, "the card's picture isn't 8-bit 4:2:0, which the tile pass shows");
      return 0;
  }
}

/* The frames context: NV12 surfaces on the caller's device, described when the
 * stream's first frame says how large its pictures are, so every picture after
 * it comes out of those surfaces. The pool grows as the decoder asks for
 * surfaces, up to POOL. */
static int make_surfaces(struct yapfast_vadec *decoder, AVCodecContext *context) {
  int width = 0;
  int height = 0;
  enum AVPixelFormat format = AV_PIX_FMT_NONE;
  if (!surface_params(decoder, context, &width, &height, &format)) return -1;
  AVBufferRef *frames = av_hwframe_ctx_alloc(decoder->device_ref);
  if (!frames) {
    fail_text(decoder, "the card's surfaces weren't described");
    return -1;
  }
  AVHWFramesContext *surface = (AVHWFramesContext *)frames->data;
  surface->format = AV_PIX_FMT_VAAPI;
  surface->sw_format = format;
  surface->width = width;
  surface->height = height;
  surface->initial_pool_size = POOL;
  const int level = hush();
  g_reason[0] = 0;
  const int result = av_hwframe_ctx_init(frames);
  unhush(level);
  if (result < 0) {
    av_buffer_unref(&frames);
    fail_code(decoder, "the card wouldn't hold the stream's surfaces", result);
    return -1;
  }
  av_buffer_unref(&context->hw_frames_ctx);
  context->hw_frames_ctx = frames;
  return 0;
}

/* Picks the card's format out of what a decoder offers, describing the
 * surfaces the size the stream has settled on by then. AV_PIX_FMT_NONE says
 * this decoder can't decode on the card, which ends the decode and falls back
 * to the software decoder. */
static enum AVPixelFormat choose_format(AVCodecContext *context,
                                        const enum AVPixelFormat *formats) {
  struct yapfast_vadec *decoder = context->opaque;
  const AVHWFramesContext *held =
      context->hw_frames_ctx ? (const AVHWFramesContext *)context->hw_frames_ctx->data : NULL;
  for (const enum AVPixelFormat *format = formats; *format != AV_PIX_FMT_NONE; ++format) {
    if (*format != AV_PIX_FMT_VAAPI) continue;
    int width = 0;
    int height = 0;
    enum AVPixelFormat shape = AV_PIX_FMT_NONE;
    if (!surface_params(decoder, context, &width, &height, &shape)) return AV_PIX_FMT_NONE;
    /* A stream that changed size, or came back in another format, needs
     * surfaces for the new one. */
    if (!held || held->sw_format != shape || (int)held->width != width ||
        (int)held->height != height) {
      if (make_surfaces(decoder, context) < 0) return AV_PIX_FMT_NONE;
    }
    return AV_PIX_FMT_VAAPI;
  }
  return formats[0];
}

/* The card's VA-API device: `device`, a render node, or the one FFmpeg finds
 * by trying each the machine has, with the libva driver LIBVA_DRIVER_NAME
 * names. FFmpeg opens a display on the node and initialises libva on it, which
 * is where a machine with no VA-API driver for that node says so. */
static int take_device(struct yapfast_vadec *decoder, const char *device) {
  const int level = hush();
  g_reason[0] = 0;
  const int result =
      av_hwdevice_ctx_create(&decoder->device_ref, AV_HWDEVICE_TYPE_VAAPI, device, NULL, 0);
  unhush(level);
  if (result < 0) {
    decoder->device_ref = NULL;
    fail_code(decoder, "the card's VA-API device wasn't opened", result);
    return -1;
  }
  return 0;
}

static const char *fourcc_name(uint32_t fourcc) {
  switch (fourcc) {
    case YAPFAST_DRM_FORMAT_NV12:
      return "NV12";
    case YAPFAST_DRM_FORMAT_R8:
      return "R8";
    case YAPFAST_DRM_FORMAT_GR88:
      return "GR88";
    case YAPFAST_DRM_FORMAT_P010:
      return "P010";
    default:
      return "a format this build has no name for";
  }
}

/* Reads a DRM PRIME export as the picture the tile pass imports: one buffer's
 * NV12 — which libva exports either as one NV12 layer of two planes or as
 * separate R8 and GR88 layers, one plane each — and says why for anything
 * else, so a picture the pass can't show is refused rather than imported
 * wrongly. Every plane's fd is the descriptor's, borrowed: it lives as long as
 * the frame it came from. */
static int describe(const AVDRMFrameDescriptor *desc, struct yapfast_vapicture *picture,
                    char *error, size_t error_size) {
  if (!desc || desc->nb_layers <= 0 || desc->nb_objects <= 0) {
    snprintf(error, error_size, "the picture was exported with nothing in it");
    return -1;
  }
  const AVDRMPlaneDescriptor *planes[2];
  if (desc->nb_layers == 1 && desc->layers[0].nb_planes == 2 &&
      desc->layers[0].format == YAPFAST_DRM_FORMAT_NV12) {
    planes[0] = &desc->layers[0].planes[0];
    planes[1] = &desc->layers[0].planes[1];
  } else if (desc->nb_layers == 2 && desc->layers[0].nb_planes == 1 &&
             desc->layers[1].nb_planes == 1 &&
             desc->layers[0].format == YAPFAST_DRM_FORMAT_R8 &&
             desc->layers[1].format == YAPFAST_DRM_FORMAT_GR88) {
    planes[0] = &desc->layers[0].planes[0];
    planes[1] = &desc->layers[1].planes[0];
  } else {
    snprintf(error, error_size,
             "the picture is exported as %s, in %d layer%s, not the NV12 the tile pass shows",
             fourcc_name(desc->layers[0].format), desc->nb_layers,
             desc->nb_layers == 1 ? "" : "s");
    return -1;
  }
  /* One modifier is what the picture is carried as, so its planes have to be
   * one buffer: a driver that split them would be told so rather than have
   * the second plane read at the first buffer's modifier. */
  if (planes[0]->object_index != planes[1]->object_index) {
    snprintf(error, error_size,
             "the picture's planes are in different buffers, which one modifier can't describe");
    return -1;
  }
  picture->fourcc = YAPFAST_DRM_FORMAT_NV12;
  picture->planes = 2;
  for (int plane = 0; plane < 2; plane++) {
    const AVDRMPlaneDescriptor *descriptor = planes[plane];
    const int object = descriptor->object_index;
    if (object < 0 || object >= desc->nb_objects) {
      snprintf(error, error_size, "the picture's plane %d names a buffer it hasn't got", plane);
      return -1;
    }
    picture->plane[plane].fd = desc->objects[object].fd;
    picture->plane[plane].offset = (uint32_t)descriptor->offset;
    picture->plane[plane].stride = (uint32_t)descriptor->pitch;
    picture->plane[plane].size = desc->objects[object].size;
    picture->plane[plane].modifier = desc->objects[object].format_modifier;
  }
  return 0;
}

int yapfast_vadec_describe(const void *drm, struct yapfast_vapicture *picture, char *error,
                           size_t error_size) {
  if (!picture) return -1;
  memset(picture, 0, sizeof *picture);
  return describe(drm, picture, error, error_size);
}

void yapfast_vadec_release(struct yapfast_vadec *decoder) {
  if (!decoder) return;
  /* The export's own unmap closes its fds, and the reference it keeps of the
   * surface goes with it. Unreferencing a frame that was never mapped, or
   * that has been released already, releases nothing. */
  av_frame_unref(decoder->drm);
}

void yapfast_vadec_free(struct yapfast_vadec *decoder) {
  if (!decoder) return;
  yapfast_vadec_release(decoder);
  avcodec_free_context(&decoder->context);
  av_buffer_unref(&decoder->device_ref);
  av_packet_free(&decoder->packet);
  av_frame_free(&decoder->frame);
  av_frame_free(&decoder->scratch);
  av_frame_free(&decoder->drm);
  free(decoder->padded);
  free(decoder);
}

struct yapfast_vadec *yapfast_vadec_new(int codec, int hint_width, int hint_height,
                                        const char *device) {
  struct yapfast_vadec *decoder = calloc(1, sizeof *decoder);
  if (!decoder) return NULL;
  /* Installed for the whole process's FFmpeg, as native/hwenc.c installs its
   * own: what this file's opens and decodes log is kept for the reason a
   * stream was refused, rather than printed beside the stream's own log. */
  av_log_set_level(AV_LOG_ERROR);
  av_log_set_callback(keep_reason);
  const char *what = codec_name(codec);
  const enum AVCodecID id = codec_id(codec);
  if (id == AV_CODEC_ID_NONE) {
    snprintf(decoder->error, sizeof decoder->error, "the graphics card has no %s decoder", what);
    goto failed;
  }
  const AVCodec *implementation = avcodec_find_decoder(id);
  if (!implementation) {
    snprintf(decoder->error, sizeof decoder->error, "FFmpeg was built without the %s decoder",
             what);
    goto failed;
  }
  if (!has_card_decoder(implementation)) {
    snprintf(decoder->error, sizeof decoder->error,
             "this FFmpeg wasn't built with its VA-API %s decoder", what);
    goto failed;
  }
  if (take_device(decoder, device) < 0) goto failed;
  decoder->context = avcodec_alloc_context3(implementation);
  decoder->packet = av_packet_alloc();
  decoder->frame = av_frame_alloc();
  decoder->scratch = av_frame_alloc();
  decoder->drm = av_frame_alloc();
  if (!decoder->context || !decoder->packet || !decoder->frame || !decoder->scratch ||
      !decoder->drm) {
    snprintf(decoder->error, sizeof decoder->error, "a decoder's memory wasn't there");
    goto failed;
  }
  decoder->hint_width = hint_width;
  decoder->hint_height = hint_height;
  decoder->context->opaque = decoder;
  decoder->context->get_format = choose_format;
  decoder->context->hw_device_ctx = av_buffer_ref(decoder->device_ref);
  if (!decoder->context->hw_device_ctx) {
    snprintf(decoder->error, sizeof decoder->error, "the card's device wasn't kept for the decoder");
    goto failed;
  }
  decoder->context->max_pixels = (int64_t)MAX_SIDE * MAX_SIDE;
  /* The card decodes the pictures: processor threads would only hold them
   * back from the tile a watched stream shows now. */
  decoder->context->thread_count = 1;
  decoder->context->flags |= AV_CODEC_FLAG_LOW_DELAY;
  const int level = hush();
  g_reason[0] = 0;
  const int result = avcodec_open2(decoder->context, implementation, NULL);
  unhush(level);
  if (result < 0) {
    fail_code(decoder, "the card wouldn't start the decoder", result);
    goto failed;
  }
  return decoder;
failed:
  snprintf(g_error, sizeof g_error, "%s", decoder->error);
  yapfast_vadec_free(decoder);
  return NULL;
}

/* Takes every picture the decoder has ready and keeps the newest one: FFmpeg's
 * contract asks for the output to be drained until EAGAIN before the next
 * packet goes in, and a decoder that reorders hands the earlier frames over
 * first. 1 says the decoder holds a picture, 0 that it gave none, -1 that it
 * failed, which the error says. */
static int take_newest(struct yapfast_vadec *decoder) {
  int held = 0;
  for (;;) {
    av_frame_unref(decoder->scratch);
    const int result = avcodec_receive_frame(decoder->context, decoder->scratch);
    if (result == AVERROR(EAGAIN) || result == AVERROR_EOF) return held;
    if (result < 0) {
      if (!decoder->refused) fail_code(decoder, "the card couldn't decode a frame", result);
      return -1;
    }
    av_frame_unref(decoder->frame);
    av_frame_move_ref(decoder->frame, decoder->scratch);
    held = 1;
  }
}

/* Sends this frame's packet and takes the pictures it gives. 1 says the decoder
 * holds a picture, 0 that it kept the frame for the pictures after it, -1 that
 * it failed, which the error says. */
static int push(struct yapfast_vadec *decoder) {
  const int level = hush();
  int held = 0;
  g_reason[0] = 0;
  int result = avcodec_send_packet(decoder->context, decoder->packet);
  /* A picture from one of the frames before this one is still waiting: take
   * it, so that this frame goes in now it is out of the way. Only the newest
   * picture is shown, here as everywhere. A decoder that reorders by more than
   * one picture answers EAGAIN again the first time, so its output is drained
   * until it takes the frame — as many times as a reorder queue can hold one,
   * which the drain's own end bounds. */
  for (int drained = 0; result == AVERROR(EAGAIN) && drained < MOST_HELD; drained++) {
    const int taken = take_newest(decoder);
    if (taken < 0) {
      unhush(level);
      return taken;
    }
    held = held || taken;
    result = avcodec_send_packet(decoder->context, decoder->packet);
  }
  if (result < 0) {
    if (!decoder->refused) fail_code(decoder, "the decoder refused a frame", result);
    unhush(level);
    return -1;
  }
  result = take_newest(decoder);
  unhush(level);
  if (result < 0) return result;
  /* A picture taken before this frame's send is still the newest one where the
   * send's own drain gave none. */
  return result > 0 || held;
}

int yapfast_vadec_decode(struct yapfast_vadec *decoder, const unsigned char *data, size_t size,
                         int keyframe) {
  (void)keyframe;
  if (!decoder || !decoder->context) return -1;
  decoder->error[0] = 0;
  /* The caller had its picture: the surface goes back to the card as soon as
   * FFmpeg's own decoder releases it, and its fds are closed with it. */
  yapfast_vadec_release(decoder);
  if (size > INT_MAX - AV_INPUT_BUFFER_PADDING_SIZE) {
    fail_text(decoder, "the frame is longer than a packet can be");
    return -1;
  }
  /* FFmpeg's parsers may read a little past the end. */
  const size_t needed = size + AV_INPUT_BUFFER_PADDING_SIZE;
  if (decoder->padded_size < needed) {
    unsigned char *grown = realloc(decoder->padded, needed);
    if (!grown) {
      fail_text(decoder, "the frame didn't fit");
      return -3;
    }
    decoder->padded = grown;
    decoder->padded_size = needed;
  }
  memcpy(decoder->padded, data, size);
  memset(decoder->padded + size, 0, AV_INPUT_BUFFER_PADDING_SIZE);
  decoder->packet->data = decoder->padded;
  decoder->packet->size = (int)size;
  const int result = push(decoder);
  /* -2 is hwdec_vaapi.h's answer for a picture the tile pass can't show, which
   * is why the surfaces weren't made: the stream stays with the software
   * decoders, which already hand back SDR. */
  if (result < 0) return decoder->refused ? -2 : result;
  if (result == 0) return 0;
  if (decoder->frame->format != AV_PIX_FMT_VAAPI) {
    fail_text(decoder, "the decoder gave a picture in the processor's memory, not the card's");
    return -1;
  }
  if (!decoder->frame->hw_frames_ctx) {
    fail_text(decoder, "the picture has no surfaces to lie in");
    return -1;
  }
  /* The surface as a DMA-BUF, its own buffer and no copy of it: MAP_READ waits
   * for the decode to finish and exports the surface read-only, and DIRECT
   * keeps the export tied to these surfaces rather than a copy of them. The
   * mapped frame keeps the surface and holds the fds open until it is
   * unreferenced. */
  const int level = hush();
  g_reason[0] = 0;
  const int mapped = av_hwframe_map(decoder->drm, decoder->frame,
                                    AV_HWFRAME_MAP_READ | AV_HWFRAME_MAP_DIRECT);
  unhush(level);
  if (mapped < 0) {
    fail_code(decoder, "the picture wasn't exported as a DMA-BUF", mapped);
    return -1;
  }
  if (decoder->drm->format != AV_PIX_FMT_DRM_PRIME) {
    fail_text(decoder, "the picture was exported as something other than a DMA-BUF");
    return -1;
  }
  return 1;
}

int yapfast_vadec_picture(struct yapfast_vadec *decoder, struct yapfast_vapicture *picture) {
  if (!decoder || !picture) return -1;
  if (!decoder->drm->data[0] || decoder->drm->format != AV_PIX_FMT_DRM_PRIME) {
    fail_text(decoder, "no picture was exported");
    return -1;
  }
  memset(picture, 0, sizeof *picture);
  /* The visible picture, which the export carries: the surface can be padded
   * out to whole macroblocks, and the tile shows the picture in it. */
  picture->width = decoder->drm->width;
  picture->height = decoder->drm->height;
  return describe((const AVDRMFrameDescriptor *)decoder->drm->data[0], picture, decoder->error,
                  sizeof decoder->error);
}

const char *yapfast_vadec_error(const struct yapfast_vadec *decoder) {
  return decoder ? decoder->error : g_error;
}
