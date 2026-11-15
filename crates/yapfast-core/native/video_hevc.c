/* File playback needs hvcC extradata, reordered timestamps and draining,
 * unlike the low-delay Go Live decoder in hevc.c. */
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
  decoder->context->max_pixels = (int64_t)8192 * 8192;
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
  if (!data) return avcodec_send_packet(decoder->context, NULL) < 0 ? -1 : 0;
  /* Own the packet in this FFmpeg build, including the bitstream reader's padding. */
  av_packet_unref(decoder->packet);
  if (size < 0 || av_new_packet(decoder->packet, size) < 0) return -1;
  memcpy(decoder->packet->data, data, (size_t)size);
  decoder->packet->pts = pts;
  decoder->packet->dts = dts;
  decoder->packet->duration = duration;
  int sent = avcodec_send_packet(decoder->context, decoder->packet);
  av_packet_unref(decoder->packet);
  return sent < 0 && sent != AVERROR_INVALIDDATA ? -1 : 0;
}

int yapfast_file_hevc_receive(struct yapfast_file_hevc *decoder, struct yapfast_hevc_frame *picture) {
  AVFrame *frame = decoder->frame;
  av_frame_unref(frame);
  int got = avcodec_receive_frame(decoder->context, frame);
  if (got == AVERROR(EAGAIN) || got == AVERROR_EOF) return 0;
  if (got < 0) return -1;
  const AVPixFmtDescriptor *format = av_pix_fmt_desc_get(frame->format);
  if (!format || format->nb_components != 3 || !(format->flags & AV_PIX_FMT_FLAG_PLANAR) ||
      (format->flags & (AV_PIX_FMT_FLAG_RGB | AV_PIX_FMT_FLAG_BE | AV_PIX_FMT_FLAG_FLOAT))) return -2;
  if (frame->width <= 0 || frame->height <= 0 || frame->width > 8192 || frame->height > 8192) return -2;
  *picture = (struct yapfast_hevc_frame){
      .width = frame->width, .height = frame->height, .depth = format->comp[0].depth,
      .x_shift = format->log2_chroma_w, .y_shift = format->log2_chroma_h,
      .bt709 = frame->colorspace == AVCOL_SPC_UNSPECIFIED ? -1 : frame->colorspace == AVCOL_SPC_BT709,
      .full_range = frame->color_range == AVCOL_RANGE_JPEG,
      .timestamp = frame->best_effort_timestamp,
  };
  for (int plane = 0; plane < 3; plane++) {
    picture->data[plane] = frame->data[plane];
    picture->stride[plane] = frame->linesize[plane];
  }
  return 1;
}

void yapfast_file_hevc_flush(struct yapfast_file_hevc *decoder) {
  avcodec_flush_buffers(decoder->context);
  av_frame_unref(decoder->frame);
}
