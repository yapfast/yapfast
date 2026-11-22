/* Decoding H265 someone streams through Go Live, with FFmpeg's HEVC decoder
 * from vendor/ffmpeg-src: libwebrtc's FFmpeg, which decode.c uses for H264,
 * has none. The same interface as decode.c: whole access units in, I420
 * planes out, valid until the next call on the decoder. */
#include <limits.h>
#include <stdlib.h>
#include <string.h>

#include "libavcodec/avcodec.h"

/* The widest or tallest picture a stream may send, as in decode.c. */
enum { MAX_SIDE = 8192 };

struct yapfast_hevc {
  AVCodecContext *context;
  AVPacket *packet;
  AVFrame *frame;
  unsigned char *padded;
  size_t padded_size;
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

void yapfast_hevc_free(struct yapfast_hevc *decoder) {
  if (!decoder) return;
  avcodec_free_context(&decoder->context);
  av_packet_free(&decoder->packet);
  av_frame_free(&decoder->frame);
  free(decoder->padded);
  free(decoder);
}

struct yapfast_hevc *yapfast_hevc_new(unsigned threads) {
  struct yapfast_hevc *decoder = calloc(1, sizeof *decoder);
  if (!decoder) return NULL;
  av_log_set_level(AV_LOG_ERROR);
  const AVCodec *hevc = avcodec_find_decoder(AV_CODEC_ID_HEVC);
  decoder->context = hevc ? avcodec_alloc_context3(hevc) : NULL;
  decoder->packet = av_packet_alloc();
  decoder->frame = av_frame_alloc();
  if (!decoder->context || !decoder->packet || !decoder->frame) {
    yapfast_hevc_free(decoder);
    return NULL;
  }
  decoder->context->thread_count = (int)threads;
  decoder->context->max_pixels = (int64_t)MAX_SIDE * MAX_SIDE;
  /* Show each frame as soon as it's decoded. */
  decoder->context->flags |= AV_CODEC_FLAG_LOW_DELAY;
  if (avcodec_open2(decoder->context, hevc, NULL) < 0) {
    yapfast_hevc_free(decoder);
    return NULL;
  }
  return decoder;
}

/* Decodes one frame. Returns 1 with a picture, 0 when the decoder has none
 * yet, or a negative number when the frame couldn't be decoded. */
int yapfast_hevc_decode(struct yapfast_hevc *decoder, const unsigned char *data, size_t size,
                        struct yapfast_picture *picture) {
  if (size > INT_MAX - AV_INPUT_BUFFER_PADDING_SIZE) return -1;
  /* FFmpeg's bitstream readers may read a little past the end. */
  size_t needed = size + AV_INPUT_BUFFER_PADDING_SIZE;
  if (decoder->padded_size < needed) {
    unsigned char *grown = realloc(decoder->padded, needed);
    if (!grown) return -3;
    decoder->padded = grown;
    decoder->padded_size = needed;
  }
  memcpy(decoder->padded, data, size);
  memset(decoder->padded + size, 0, AV_INPUT_BUFFER_PADDING_SIZE);
  decoder->packet->data = decoder->padded;
  decoder->packet->size = (int)size;
  if (avcodec_send_packet(decoder->context, decoder->packet) < 0) return -1;
  av_frame_unref(decoder->frame);
  int received = avcodec_receive_frame(decoder->context, decoder->frame);
  if (received == AVERROR(EAGAIN)) return 0;
  if (received < 0) return -1;
  AVFrame *frame = decoder->frame;
  /* Main profile's 8-bit 4:2:0, as Yapfast's encoders send; Main 10
   * pictures don't show. */
  if (frame->format != AV_PIX_FMT_YUV420P && frame->format != AV_PIX_FMT_YUVJ420P) return -2;
  if (frame->width > MAX_SIDE || frame->height > MAX_SIDE) return -2;
  picture->width = frame->width;
  picture->height = frame->height;
  picture->y = frame->data[0];
  picture->u = frame->data[1];
  picture->v = frame->data[2];
  picture->y_stride = frame->linesize[0];
  picture->u_stride = frame->linesize[1];
  picture->v_stride = frame->linesize[2];
  return 1;
}
