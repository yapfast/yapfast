/* Decoding Go Live video someone else streams: VP8 through libvpx, and H264
 * through FFmpeg inside Yapfast's libwebrtc archive. On macOS the same archive
 * carries FFmpeg's audio decoders and no video ones, so videotoolbox.c decodes
 * H264 there instead. Pictures come out as I420 planes that stay valid until
 * the next call on the decoder. */
#include <limits.h>
#include <stdlib.h>
#include <string.h>

#include "vpx/vp8dx.h"
#include "vpx/vpx_decoder.h"

#ifdef __APPLE__
#include "videotoolbox.h"
#else
#include "libavcodec/avcodec.h"
#endif

enum { YAPFAST_VP8 = 0, YAPFAST_H264 = 1 };

/* The widest or tallest picture a stream may send: a 4K stream fits, and a
 * stream can't make Yapfast take more memory than that by claiming a bigger one. */
enum { MAX_SIDE = 8192 };

struct yapfast_decoder {
  int codec;
  vpx_codec_ctx_t vpx;
#ifdef __APPLE__
  struct yapfast_vt *h264;
#else
  AVCodecContext *context;
  AVPacket *packet;
  AVFrame *frame;
  /* The frame, with the padding FFmpeg's parsers may read past the end of. */
  unsigned char *padded;
  size_t padded_size;
#endif
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
  } else if (decoder->codec == YAPFAST_H264) {
#ifdef __APPLE__
    yapfast_vt_free(decoder->h264);
#else
    avcodec_free_context(&decoder->context);
    av_packet_free(&decoder->packet);
    av_frame_free(&decoder->frame);
    free(decoder->padded);
#endif
  }
  free(decoder);
}

struct yapfast_decoder *yapfast_decoder_new(int codec, unsigned threads) {
  struct yapfast_decoder *decoder = calloc(1, sizeof *decoder);
  if (!decoder) return NULL;
  decoder->codec = codec;
  if (codec == YAPFAST_VP8) {
    vpx_codec_dec_cfg_t config = {.threads = threads};
    if (vpx_codec_dec_init(&decoder->vpx, vpx_codec_vp8_dx(), &config, 0)) {
      free(decoder);
      return NULL;
    }
    return decoder;
  }
  if (codec != YAPFAST_H264) {
    free(decoder);
    return NULL;
  }
#ifdef __APPLE__
  /* VideoToolbox takes a whole frame at a time, on the GPU or on threads of
   * its own, so `threads` asks nothing of it. One access unit in and one
   * picture out, with nothing held back, is what a live stream is. */
  (void)threads;
  decoder->h264 = yapfast_vt_new(1);
  if (!decoder->h264) {
    free(decoder);
    return NULL;
  }
  return decoder;
#else
  const AVCodec *h264 = avcodec_find_decoder(AV_CODEC_ID_H264);
  decoder->context = h264 ? avcodec_alloc_context3(h264) : NULL;
  decoder->packet = av_packet_alloc();
  decoder->frame = av_frame_alloc();
  if (!decoder->context || !decoder->packet || !decoder->frame) {
    yapfast_decoder_free(decoder);
    return NULL;
  }
  decoder->context->thread_count = (int)threads;
  decoder->context->max_pixels = (int64_t)MAX_SIDE * MAX_SIDE;
  /* Show each frame as soon as it's decoded. */
  decoder->context->flags |= AV_CODEC_FLAG_LOW_DELAY;
  if (avcodec_open2(decoder->context, h264, NULL) < 0) {
    yapfast_decoder_free(decoder);
    return NULL;
  }
  return decoder;
#endif
}

/* Decodes one frame. Returns 1 with a picture, 0 when the decoder has none
 * yet, or a negative number when the frame couldn't be decoded. */
int yapfast_decoder_decode(struct yapfast_decoder *decoder, const unsigned char *data,
                           size_t size, struct yapfast_picture *picture) {
  if (decoder->codec == YAPFAST_VP8) {
    if (size > UINT_MAX) return -1;
    if (vpx_codec_decode(&decoder->vpx, data, (unsigned int)size, NULL, 0)) return -1;
    vpx_codec_iter_t iter = NULL;
    vpx_image_t *image = vpx_codec_get_frame(&decoder->vpx, &iter);
    if (!image) return 0;
    if (image->fmt != VPX_IMG_FMT_I420) return -2;
    if (image->d_w > MAX_SIDE || image->d_h > MAX_SIDE) return -2;
    picture->width = (int)image->d_w;
    picture->height = (int)image->d_h;
    picture->y = image->planes[VPX_PLANE_Y];
    picture->u = image->planes[VPX_PLANE_U];
    picture->v = image->planes[VPX_PLANE_V];
    picture->y_stride = image->stride[VPX_PLANE_Y];
    picture->u_stride = image->stride[VPX_PLANE_U];
    picture->v_stride = image->stride[VPX_PLANE_V];
    return 1;
  }
#ifdef __APPLE__
  /* A live stream says nothing about when its pictures play. */
  if (yapfast_vt_decode(decoder->h264, data, size, 0 /* Annex-B */, -1) < 0) return -1;
  struct yapfast_vt_frame frame;
  if (!yapfast_vt_next(decoder->h264, &frame, NULL)) return 0;
  picture->width = frame.width;
  picture->height = frame.height;
  picture->y = frame.data[0];
  picture->u = frame.data[1];
  picture->v = frame.data[2];
  picture->y_stride = frame.stride[0];
  picture->u_stride = frame.stride[1];
  picture->v_stride = frame.stride[2];
  return 1;
#else
  if (size > INT_MAX - AV_INPUT_BUFFER_PADDING_SIZE) return -1;
  /* FFmpeg's parsers may read a little past the end. */
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
#endif
}
