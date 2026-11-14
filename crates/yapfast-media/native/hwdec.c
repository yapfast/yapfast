/* Decoding a watched stream on the graphics card with FFmpeg's own Direct3D
 * 11 decoder, from vendor/ffmpeg-src, the card's one decoder: the same access
 * units in, and a picture left in an array texture on the device the caller
 * brought, which native/d3d11dec.cc's video processor pass turns into the
 * tile. No picture comes back to the processor here.
 *
 * The device is the caller's — the one the encoder and the window already
 * share — so a decoded picture is never copied to another device. The
 * surfaces are allocated once, when the first frame says how large the
 * picture is, so a stream allocates no textures per frame.
 *
 * C, because it reads nothing but FFmpeg's and D3D11's C headers; the video
 * processor and its Direct3D interfaces stay in d3d11dec.cc. */

// Windows-only, like the SDK headers it reads: elsewhere this file is an
// empty unit, and build.rs defines the macro on the one target it builds
// for.
#if defined(YAPFAST_WINDOWS)
#include <limits.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

// d3d11.h's C wrappers (ID3D11Device_AddRef and the rest) are behind this
// macro; FFmpeg's own d3d11va code uses them the same way.
#define COBJMACROS
#include <d3d11.h>

#include "libavcodec/avcodec.h"
#include "libavutil/error.h"
#include "libavutil/frame.h"
#include "libavutil/hwcontext.h"
#include "libavutil/hwcontext_d3d11va.h"
#include "libavutil/pixfmt.h"

#include "hwdec.h"

/* The widest or tallest picture a stream may send, as in decode.c. */
enum { MAX_SIDE = 8192 };

/* The surfaces one stream's decoder holds: H264's sixteen reference pictures
 * at most, the rest giving the card room to decode ahead. Allocated in one
 * array texture when the first frame gives the size, so a stream that keeps
 * its size takes no texture after that. */
enum { POOL = 20 };

struct yapfast_ffdec {
  AVCodecContext *context;
  AVBufferRef *device_ref;
  AVPacket *packet;
  /* The newest picture the decoder gave, and the one after it while the
   * decoder's output is drained: a decoder that reorders hands over the earlier
   * frames first, and only the last one is shown. */
  AVFrame *frame;
  AVFrame *scratch;
  unsigned char *padded;
  size_t padded_size;
  int hint_width;
  int hint_height;
  /* Set where the surfaces couldn't be made because the stream's pictures
   * aren't ones the tile pass shows, which yapfast_ffdec_decode answers with
   * -2 rather than as a plain failure. */
  int refused;
  char error[256];
};

/* The reason a decoder that couldn't be made wasn't, kept for the caller that
 * gets the null back: hwdec.h promises it from yapfast_ffdec_error(NULL). */
static _Thread_local char g_error[256];

static void fail_text(struct yapfast_ffdec *decoder, const char *what) {
  snprintf(decoder->error, sizeof decoder->error, "%s", what);
}

static void fail_code(struct yapfast_ffdec *decoder, const char *what, int code) {
  char why[128] = {0};
  av_strerror(code, why, sizeof why);
  snprintf(decoder->error, sizeof decoder->error, "%s (%s)", what, why);
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

/* What one of this stream's surfaces is: the coded size — the picture rounded
 * up to whole macroblocks, 1088 rows for a 1080p H264 stream — aligned as
 * FFmpeg's own Direct3D 11 path aligns it, and the format the tile pass shows.
 * FFmpeg sizes both in ff_dxva2_common_frame_params, which never runs for a
 * frames context the caller supplies, so this does the same work: the coded
 * size, because a surface smaller than the picture the decoder codes loses its
 * last rows, and NV12, which is the only thing the pass reads. 0 says the
 * stream hasn't said, or said one the pass can't show, and the reason is the
 * decoder's error. */
static int surface_params(struct yapfast_ffdec *decoder, const AVCodecContext *context, int *width,
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
  /* The HEVC and AV1 DXVA2 specs ask for surfaces aligned to 128, so that every
   * coding tool has room to work; the rest ask for 16. */
  const int alignment =
      context->codec_id == AV_CODEC_ID_HEVC || context->codec_id == AV_CODEC_ID_AV1 ? 128 : 16;
  *width = (coded_width + alignment - 1) / alignment * alignment;
  *height = (coded_height + alignment - 1) / alignment * alignment;
  /* The format the decoder would give in the processor's memory, which
   * ff_get_format recorded before it asked this file for one. A 10-bit stream's
   * is P010, and 9- and 12-bit, 4:2:2 and 4:4:4 streams have theirs: pictures
   * the tile pass has no curve or matrix for, which the software decoders hand
   * back as SDR already. */
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

/* The frames context: one array texture of `POOL` NV12 surfaces on the
 * caller's device, made when the stream's first frame says how large its
 * pictures are, so every picture after it is a slice of that texture. */
static int make_surfaces(struct yapfast_ffdec *decoder, AVCodecContext *context) {
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
  AVD3D11VAFramesContext *d3d11 = surface->hwctx;
  surface->format = AV_PIX_FMT_D3D11;
  surface->sw_format = format;
  surface->width = width;
  surface->height = height;
  surface->initial_pool_size = POOL;
  /* The decoder reads these surfaces; the tile pass makes its own input view
   * of the slice each picture lands in. */
  d3d11->BindFlags = D3D11_BIND_DECODER;
  const int level = hush();
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

/* Whether `codec` has a Direct3D 11 hwaccel that decodes into the frames this
 * file makes: the one whose config takes a device context and names the
 * AV_PIX_FMT_D3D11 surfaces. FFmpeg offers that format in the list get_format
 * chooses from whether or not the decoder carries such a hwaccel — H264 offers
 * both of Direct3D 11's decoders' pictures under the older one's switch — and a
 * hardware format chosen without one leaves avctx->hwaccel null: FFmpeg then
 * decodes the macroblocks in the processor's memory, into frames that have
 * none, and writes each prediction through plane pointers that were never
 * there. That is an access violation rather than a picture, so the format is
 * only taken where the decoder says it decodes into it. */
static int decodes_on_the_card(const AVCodec *codec) {
  for (int index = 0;; index++) {
    const AVCodecHWConfig *config = avcodec_get_hw_config(codec, index);
    if (!config) return 0;
    if (config->pix_fmt == AV_PIX_FMT_D3D11 &&
        config->device_type == AV_HWDEVICE_TYPE_D3D11VA &&
        (config->methods & AV_CODEC_HW_CONFIG_METHOD_HW_DEVICE_CTX)) {
      return 1;
    }
  }
}

/* Picks the card's format out of what a decoder offers, making the surfaces
 * the size the stream has settled on by then. AV_PIX_FMT_NONE says this
 * decoder can't decode on the card, which ends the decode and falls back; the
 * software format — FFmpeg puts it last — says the same for a decoder whose
 * list holds no card format at all. */
static enum AVPixelFormat choose_format(AVCodecContext *context, const enum AVPixelFormat *formats) {
  struct yapfast_ffdec *decoder = context->opaque;
  const AVHWFramesContext *held =
      context->hw_frames_ctx ? (const AVHWFramesContext *)context->hw_frames_ctx->data : NULL;
  for (const enum AVPixelFormat *format = formats; *format != AV_PIX_FMT_NONE; ++format) {
    if (*format != AV_PIX_FMT_D3D11) continue;
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
    return AV_PIX_FMT_D3D11;
  }
  const enum AVPixelFormat *software = formats;
  while (software[1] != AV_PIX_FMT_NONE) ++software;
  return *software;
}

/* The device context over the caller's device, so that FFmpeg's decoder makes
 * its surfaces on the device the encoder and the window use. */
static int take_device(struct yapfast_ffdec *decoder, void *device) {
  if (!device) {
    fail_text(decoder, "no graphics card device to decode on");
    return -1;
  }
  AVBufferRef *reference = av_hwdevice_ctx_alloc(AV_HWDEVICE_TYPE_D3D11VA);
  if (!reference) {
    fail_text(decoder, "the card's device wasn't described");
    return -1;
  }
  AVHWDeviceContext *hardware = (AVHWDeviceContext *)reference->data;
  AVD3D11VADeviceContext *d3d11 = hardware->hwctx;
  /* One reference goes to the device context, which releases it with itself. */
  d3d11->device = (ID3D11Device *)device;
  ID3D11Device_AddRef(d3d11->device);
  const int level = hush();
  const int result = av_hwdevice_ctx_init(reference);
  unhush(level);
  if (result < 0) {
    av_buffer_unref(&reference);
    fail_code(decoder, "the card's device wasn't taken", result);
    return -1;
  }
  decoder->device_ref = reference;
  return 0;
}

static enum AVCodecID codec_id(int codec) {
  switch (codec) {
    case YAPFAST_FF_H264:
      return AV_CODEC_ID_H264;
    case YAPFAST_FF_HEVC:
      return AV_CODEC_ID_HEVC;
    case YAPFAST_FF_AV1:
      return AV_CODEC_ID_AV1;
    default:
      return AV_CODEC_ID_NONE;
  }
}

void yapfast_ffdec_free(struct yapfast_ffdec *decoder) {
  if (!decoder) return;
  avcodec_free_context(&decoder->context);
  av_buffer_unref(&decoder->device_ref);
  av_packet_free(&decoder->packet);
  av_frame_free(&decoder->frame);
  av_frame_free(&decoder->scratch);
  free(decoder->padded);
  free(decoder);
}

struct yapfast_ffdec *yapfast_ffdec_new(int codec, int hint_width, int hint_height, void *device) {
  struct yapfast_ffdec *decoder = calloc(1, sizeof *decoder);
  if (!decoder) return NULL;
  const enum AVCodecID id = codec_id(codec);
  if (id == AV_CODEC_ID_NONE) {
    fail_text(decoder, "this build has no decoder for the codec");
    goto failed;
  }
  const AVCodec *implementation = avcodec_find_decoder(id);
  if (!implementation) {
    fail_text(decoder, "FFmpeg was built without this decoder");
    goto failed;
  }
  if (!decodes_on_the_card(implementation)) {
    fail_text(decoder, "this build of FFmpeg has no Direct3D 11 decoder for the codec");
    goto failed;
  }
  if (take_device(decoder, device) < 0) goto failed;
  decoder->context = avcodec_alloc_context3(implementation);
  decoder->packet = av_packet_alloc();
  decoder->frame = av_frame_alloc();
  decoder->scratch = av_frame_alloc();
  if (!decoder->context || !decoder->packet || !decoder->frame || !decoder->scratch) {
    fail_text(decoder, "a decoder's memory wasn't there");
    goto failed;
  }
  decoder->hint_width = hint_width;
  decoder->hint_height = hint_height;
  decoder->context->opaque = decoder;
  decoder->context->get_format = choose_format;
  decoder->context->hw_device_ctx = av_buffer_ref(decoder->device_ref);
  if (!decoder->context->hw_device_ctx) {
    fail_text(decoder, "the card's device wasn't kept for the decoder");
    goto failed;
  }
  decoder->context->max_pixels = (int64_t)MAX_SIDE * MAX_SIDE;
  /* The card decodes the pictures: processor threads would only hold them
   * back from the tile a watched stream shows now. */
  decoder->context->thread_count = 1;
  decoder->context->flags |= AV_CODEC_FLAG_LOW_DELAY;
  const int level = hush();
  const int result = avcodec_open2(decoder->context, implementation, NULL);
  unhush(level);
  if (result < 0) {
    fail_code(decoder, "the card wouldn't start the decoder", result);
    goto failed;
  }
  return decoder;
failed:
  snprintf(g_error, sizeof g_error, "%s", decoder->error);
  yapfast_ffdec_free(decoder);
  return NULL;
}

/* Takes every picture the decoder has ready and keeps the newest one: FFmpeg's
 * contract asks for the output to be drained until EAGAIN before the next
 * packet goes in, and a decoder that reorders hands the earlier frames over
 * first. 1 says the decoder holds a picture, 0 that it gave none, -1 that it
 * failed, which the error says. */
static int take_newest(struct yapfast_ffdec *decoder) {
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

/* How many pictures a decoder may hold before it takes a frame: a reorder queue
 * is a handful deep, and a drain that never ends is a loop that never ends. */
#define kMostHeld 8

/* Sends this frame's packet and takes the pictures it gives. 1 says the decoder
 * holds a picture, 0 that it kept the frame for the pictures after it, -1 that
 * it failed, which the error says. */
static int push(struct yapfast_ffdec *decoder) {
  const int level = hush();
  int held = 0;
  int result = avcodec_send_packet(decoder->context, decoder->packet);
  /* A picture from one of the frames before this one is still waiting: take
   * it, so that this frame goes in now it is out of the way. Only the newest
   * picture is shown, here as everywhere. A decoder that reorders by more than
   * one picture answers EAGAIN again the first time, so its output is drained
   * until it takes the frame — as many times as a reorder queue can hold one,
   * which the drain's own end bounds. */
  for (int drained = 0; result == AVERROR(EAGAIN) && drained < kMostHeld; drained++) {
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

int yapfast_ffdec_decode(struct yapfast_ffdec *decoder, const unsigned char *data, size_t size,
                         int keyframe) {
  (void)keyframe;
  if (!decoder || !decoder->context) return -1;
  decoder->error[0] = 0;
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
  /* -2 is hwdec.h's answer for a picture the tile pass can't show, which is
   * why the surfaces weren't made: the stream stays with the software
   * decoders, which already hand back SDR. */
  if (result < 0) return decoder->refused ? -2 : result;
  if (result == 0) return 0;
  if (decoder->frame->format != AV_PIX_FMT_D3D11) {
    fail_text(decoder, "the decoder gave a picture in the processor's memory, not the card's");
    return -1;
  }
  if (!decoder->frame->hw_frames_ctx) {
    fail_text(decoder, "the picture has no surfaces to lie in");
    return -1;
  }
  return 1;
}

int yapfast_ffdec_picture(struct yapfast_ffdec *decoder, struct yapfast_ffpicture *picture) {
  if (!decoder || !decoder->frame || !decoder->frame->hw_frames_ctx) return -1;
  const AVFrame *frame = decoder->frame;
  /* The D3D11 frames context keeps the texture in data[0] and the array slice
   * in data[1], as AVD3D11FrameDescriptor documents. */
  picture->texture = frame->data[0];
  picture->index = (unsigned)(uintptr_t)frame->data[1];
  picture->width = frame->width;
  picture->height = frame->height;
  picture->full_range = frame->color_range == AVCOL_RANGE_JPEG;
  picture->bt709 = frame->colorspace == AVCOL_SPC_BT709;
  picture->bt2020 =
      frame->colorspace == AVCOL_SPC_BT2020_NCL || frame->colorspace == AVCOL_SPC_BT2020_CL;
  picture->pq = frame->color_trc == AVCOL_TRC_SMPTE2084;
  return picture->texture ? 0 : -1;
}

const char *yapfast_ffdec_error(const struct yapfast_ffdec *decoder) {
  return decoder ? decoder->error : g_error;
}

#endif  // YAPFAST_WINDOWS
