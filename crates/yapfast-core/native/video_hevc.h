/* What video.c and video_hevc.c share. video.c is built against libwebrtc's
 * FFmpeg and video_hevc.c against the separately built, symbol-prefixed one
 * with the HEVC decoder (vendor/ffmpeg-src), whose structs aren't ABI
 * compatible: only the plain values here cross between them. Each file
 * includes its own FFmpeg's avcodec.h and pixdesc.h first, and
 * yapfast_frame_read is compiled against those. */
#ifndef YAPFAST_VIDEO_HEVC_H
#define YAPFAST_VIDEO_HEVC_H
#include <stdint.h>

/* How a decoder's call fails: the file is damaged past playing, or its
 * pictures are in a format Yapfast doesn't show. yapfast_video_next returns
 * them too. */
enum { YAPFAST_VIDEO_FAILED = -1, YAPFAST_VIDEO_UNSUPPORTED = -2 };

/* The widest or tallest picture Yapfast decodes. A file can claim any size,
 * and each picture would take that much memory. */
enum { YAPFAST_VIDEO_MAX_SIDE = 8192 };

/* A picture either FFmpeg decoded: three planes of Y, U and V. */
struct yapfast_frame {
  int width, height, depth;
  /* How many times the colour planes are halved across and down. */
  unsigned x_shift, y_shift;
  /* Whether its colours are BT.709, or -1 when it doesn't say. */
  int bt709;
  int full_range;
  const uint8_t *data[3];
  int stride[3];
  /* In the stream's units. */
  int64_t timestamp;
};

/* Describes `frame` in `picture`, whose planes stay the frame's: 1, or
 * YAPFAST_VIDEO_UNSUPPORTED unless it's three planes of YUV in this
 * machine's byte order. */
static inline int yapfast_frame_read(const AVFrame *frame, struct yapfast_frame *picture) {
  const AVPixFmtDescriptor *format = av_pix_fmt_desc_get(frame->format);
  if (!format || format->nb_components != 3 || !(format->flags & AV_PIX_FMT_FLAG_PLANAR) ||
      (format->flags & (AV_PIX_FMT_FLAG_RGB | AV_PIX_FMT_FLAG_BE | AV_PIX_FMT_FLAG_FLOAT))) {
    return YAPFAST_VIDEO_UNSUPPORTED;
  }
  /* The older names of the full-range formats. */
  int jpeg = frame->format == AV_PIX_FMT_YUVJ420P || frame->format == AV_PIX_FMT_YUVJ422P ||
             frame->format == AV_PIX_FMT_YUVJ444P;
  *picture = (struct yapfast_frame){
      .width = frame->width,
      .height = frame->height,
      .depth = format->comp[0].depth,
      .x_shift = format->log2_chroma_w,
      .y_shift = format->log2_chroma_h,
      .bt709 = frame->colorspace == AVCOL_SPC_UNSPECIFIED ? -1
                                                          : frame->colorspace == AVCOL_SPC_BT709,
      .full_range = frame->color_range == AVCOL_RANGE_JPEG || jpeg,
      .timestamp = frame->best_effort_timestamp,
  };
  for (int plane = 0; plane < 3; plane++) {
    picture->data[plane] = frame->data[plane];
    picture->stride[plane] = frame->linesize[plane];
  }
  return 1;
}

struct yapfast_file_hevc;

/* A decoder on up to `threads` threads for the stream whose hvcC is `extra`,
 * or NULL. It refuses pictures of more than YAPFAST_VIDEO_MAX_SIDE squared
 * pixels, as video.c's other decoders do. */
struct yapfast_file_hevc *yapfast_file_hevc_new(unsigned threads, const uint8_t *extra, int size);
void yapfast_file_hevc_free(struct yapfast_file_hevc *decoder);
/* Hands over a packet, which it copies, its times in the stream's units: 0,
 * also for a damaged packet, which is skipped, or YAPFAST_VIDEO_FAILED. */
int yapfast_file_hevc_send(struct yapfast_file_hevc *decoder, const uint8_t *data, int size,
                           int64_t pts, int64_t dts, int64_t duration);
/* Says the file ended, so the pictures the decoder still holds come out. */
void yapfast_file_hevc_end(struct yapfast_file_hevc *decoder);
/* 1 with a picture, 0 when the decoder wants more or has given its last, or
 * one of the errors above. The planes stay until the next receive or flush. */
int yapfast_file_hevc_receive(struct yapfast_file_hevc *decoder, struct yapfast_frame *picture);
void yapfast_file_hevc_flush(struct yapfast_file_hevc *decoder);
#endif
