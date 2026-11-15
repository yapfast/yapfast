/* The HEVC decoder for video files, driven by video.c. Files need hvcC
 * extradata, reordered timestamps and draining, unlike the low-delay Go Live
 * decoder in hevc.c. */
#include <stdlib.h>
#include <string.h>
#include "libavcodec/avcodec.h"
#include "libavutil/pixdesc.h"
#include "video_hevc.h"

struct yapfast_file_hevc {
  AVCodecContext *context;
  AVPacket *packet;
  AVFrame *frame;
};

void yapfast_file_hevc_free(struct yapfast_file_hevc *decoder) {
  if (!decoder) return;
  avcodec_free_context(&decoder->context);
  av_packet_free(&decoder->packet);
  av_frame_free(&decoder->frame);
  free(decoder);
}

struct yapfast_file_hevc *yapfast_file_hevc_new(unsigned threads, const uint8_t *extra, int size) {
  if (size < 0 || (size > 0 && !extra)) return NULL;
  struct yapfast_file_hevc *decoder = calloc(1, sizeof *decoder);
  if (!decoder) return NULL;
  const AVCodec *codec = avcodec_find_decoder(AV_CODEC_ID_HEVC);
  decoder->context = codec ? avcodec_alloc_context3(codec) : NULL;
  decoder->packet = av_packet_alloc();
  decoder->frame = av_frame_alloc();
  if (!decoder->context || !decoder->packet || !decoder->frame) goto failed;
  decoder->context->thread_count = (int)threads;
  decoder->context->max_pixels = (int64_t)YAPFAST_VIDEO_MAX_SIDE * YAPFAST_VIDEO_MAX_SIDE;
  if (size > 0) {
    decoder->context->extradata = av_mallocz((size_t)size + AV_INPUT_BUFFER_PADDING_SIZE);
    if (!decoder->context->extradata) goto failed;
    memcpy(decoder->context->extradata, extra, (size_t)size);
    decoder->context->extradata_size = size;
  }
  if (avcodec_open2(decoder->context, codec, NULL) < 0) goto failed;
  return decoder;
failed:
  yapfast_file_hevc_free(decoder);
  return NULL;
}

int yapfast_file_hevc_send(struct yapfast_file_hevc *decoder, const uint8_t *data, int size,
                           int64_t pts, int64_t dts, int64_t duration) {
  /* The decoder keeps packets past this call, and reads past their end:
   * each is copied into one of this FFmpeg's own, padded. */
  av_packet_unref(decoder->packet);
  if (size < 0 || av_new_packet(decoder->packet, size) < 0) return YAPFAST_VIDEO_FAILED;
  if (size > 0) memcpy(decoder->packet->data, data, (size_t)size);
  decoder->packet->pts = pts;
  decoder->packet->dts = dts;
  decoder->packet->duration = duration;
  int sent = avcodec_send_packet(decoder->context, decoder->packet);
  av_packet_unref(decoder->packet);
  return sent < 0 && sent != AVERROR_INVALIDDATA ? YAPFAST_VIDEO_FAILED : 0;
}

void yapfast_file_hevc_end(struct yapfast_file_hevc *decoder) {
  /* If it fails, what the decoder already has still comes out. */
  avcodec_send_packet(decoder->context, NULL);
}

int yapfast_file_hevc_receive(struct yapfast_file_hevc *decoder, struct yapfast_frame *picture) {
  AVFrame *frame = decoder->frame;
  av_frame_unref(frame);
  int got = avcodec_receive_frame(decoder->context, frame);
  if (got == AVERROR(EAGAIN) || got == AVERROR_EOF) return 0;
  if (got < 0) return YAPFAST_VIDEO_FAILED;
  return yapfast_frame_read(frame, picture);
}

void yapfast_file_hevc_flush(struct yapfast_file_hevc *decoder) {
  avcodec_flush_buffers(decoder->context);
  av_frame_unref(decoder->frame);
}
