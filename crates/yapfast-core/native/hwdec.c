/* Decoding a watched stream on the graphics card with FFmpeg's own Direct3D
 * 11 decoder, from vendor/ffmpeg-src, in place of Media Foundation's MFTs:
 * the same access units in, and a picture left in an array texture on the
 * device the caller brought, which native/d3d11dec.cc's video processor pass
 * turns into the tile. No picture comes back to the processor here.
 *
 * The device is the caller's — the one the encoder and the window already
 * share — so a decoded picture is never copied to another device. The
 * surfaces are allocated once, when the first frame says how large the
 * picture is, so a stream allocates no textures per frame.
 *
 * C, because it reads nothing but FFmpeg's and D3D11's C headers; the COM and
 * the processor stay in d3d11dec.cc. */

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
  AVFrame *frame;
  unsigned char *padded;
  size_t padded_size;
  int hint_width;
  int hint_height;
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

/* The frames context: one array texture of `POOL` NV12 surfaces on the
 * caller's device, made when the stream's first frame says how large its
 * pictures are, so every picture after it is a slice of that texture. */
static int make_surfaces(struct yapfast_ffdec *decoder, AVCodecContext *context) {
  const int width = context->width > 0 ? context->width : decoder->hint_width;
  const int height = context->height > 0 ? context->height : decoder->hint_height;
  if (width <= 0 || height <= 0 || width > MAX_SIDE || height > MAX_SIDE) {
    fail_text(decoder, "the stream never said how large its pictures are");
    return -1;
  }
  AVBufferRef *frames = av_hwframe_ctx_alloc(decoder->device_ref);
  if (!frames) {
    fail_text(decoder, "the card's surfaces weren't described");
    return -1;
  }
  AVHWFramesContext *surface = (AVHWFramesContext *)frames->data;
  AVD3D11VAFramesContext *d3d11 = surface->hwctx;
  surface->format = AV_PIX_FMT_D3D11;
  surface->sw_format = AV_PIX_FMT_NV12;
  surface->width = width;
  surface->height = height;
  surface->initial_pool_size = POOL;
  /* The decoder reads these surfaces; the tile pass makes its own input view
   * of the slice each picture lands in. */
  d3d11->BindFlags = D3D11_BIND_DECODER;
  const int result = av_hwframe_ctx_init(frames);
  if (result < 0) {
    av_buffer_unref(&frames);
    fail_code(decoder, "the card wouldn't hold the stream's surfaces", result);
    return -1;
  }
  av_buffer_unref(&context->hw_frames_ctx);
  context->hw_frames_ctx = frames;
  return 0;
}

/* Picks the card's format out of what a decoder offers, making the surfaces
 * the size the stream has settled on by then. AV_PIX_FMT_NONE says this
 * decoder can't decode on the card, which ends the decode and falls back. */
static enum AVPixelFormat choose_format(AVCodecContext *context, const enum AVPixelFormat *formats) {
  struct yapfast_ffdec *decoder = context->opaque;
  for (const enum AVPixelFormat *format = formats; *format != AV_PIX_FMT_NONE; ++format) {
    if (*format != AV_PIX_FMT_D3D11) continue;
    const int width = context->width > 0 ? context->width : decoder->hint_width;
    const int height = context->height > 0 ? context->height : decoder->hint_height;
    const AVHWFramesContext *held =
        context->hw_frames_ctx ? (const AVHWFramesContext *)context->hw_frames_ctx->data : NULL;
    /* A stream that changed size needs surfaces for the new one. */
    if (!held || (int)held->width != width || (int)held->height != height) {
      if (make_surfaces(decoder, context) < 0) return AV_PIX_FMT_NONE;
    }
    return AV_PIX_FMT_D3D11;
  }
  return formats[0];
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
  const int result = av_hwdevice_ctx_init(reference);
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
    case YAPFAST_FF_VP9:
      return AV_CODEC_ID_VP9;
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
  free(decoder->padded);
  free(decoder);
}

struct yapfast_ffdec *yapfast_ffdec_new(int codec, int hint_width, int hint_height, void *device) {
  struct yapfast_ffdec *decoder = calloc(1, sizeof *decoder);
  if (!decoder) return NULL;
  av_log_set_level(AV_LOG_ERROR);
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
  if (take_device(decoder, device) < 0) goto failed;
  decoder->context = avcodec_alloc_context3(implementation);
  decoder->packet = av_packet_alloc();
  decoder->frame = av_frame_alloc();
  if (!decoder->context || !decoder->packet || !decoder->frame) {
    fail_text(decoder, "a decoder's memory wasn't there");
    goto failed;
  }
  decoder->hint_width = hint_width;
  decoder->hint_height = hint_height;
  decoder->context->opaque = decoder;
  decoder->context->get_format = choose_format;
  decoder->context->hw_device_ctx = av_buffer_ref(decoder->device_ref);
  decoder->context->max_pixels = (int64_t)MAX_SIDE * MAX_SIDE;
  /* The card decodes the pictures: processor threads would only hold them
   * back from the tile a watched stream shows now. */
  decoder->context->thread_count = 1;
  decoder->context->flags |= AV_CODEC_FLAG_LOW_DELAY;
  const int result = avcodec_open2(decoder->context, implementation, NULL);
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

int yapfast_ffdec_decode(struct yapfast_ffdec *decoder, const unsigned char *data, size_t size,
                         int keyframe) {
  (void)keyframe;
  if (!decoder || !decoder->context) return -1;
  decoder->error[0] = 0;
  if (size > INT_MAX - AV_INPUT_BUFFER_PADDING_SIZE) return -1;
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
  int result = avcodec_send_packet(decoder->context, decoder->packet);
  if (result < 0) {
    fail_code(decoder, "the decoder refused a frame", result);
    return -1;
  }
  av_frame_unref(decoder->frame);
  result = avcodec_receive_frame(decoder->context, decoder->frame);
  if (result == AVERROR(EAGAIN) || result == AVERROR_EOF) return 0;
  if (result < 0) {
    fail_code(decoder, "the card couldn't decode a frame", result);
    return -1;
  }
  if (decoder->frame->format != AV_PIX_FMT_D3D11) {
    fail_text(decoder, "the decoder gave a picture in the processor's memory, not the card's");
    return -1;
  }
  if (!decoder->frame->hw_frames_ctx) {
    fail_text(decoder, "the picture has no surfaces to lie in");
    return -1;
  }
  const AVHWFramesContext *surface = (const AVHWFramesContext *)decoder->frame->hw_frames_ctx->data;
  if (surface->sw_format != AV_PIX_FMT_NV12) {
    /* P010 and the rest would have to be tonemapped to 8-bit SDR, which the
     * tile pass has no curve for: such a stream stays with the software
     * decoders, which already hand back SDR. */
    fail_text(decoder, "the card decoded a picture deeper than 8 bits, which the tile pass can't show");
    return -2;
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
