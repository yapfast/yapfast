/* Video files: demuxed by FFmpeg's libavformat; pictures decoded by
 * libavcodec (H264), libvpx (VP8, VP9) or dav1d (AV1), and scaled and
 * converted to RGBA by libyuv; sound decoded by libavcodec (AAC, Opus, MP3,
 * Vorbis, FLAC, PCM). All of it is inside Yapfast's libwebrtc archive — except
 * H264 on macOS, whose archive carries FFmpeg's audio decoders and no video
 * ones, where videotoolbox.c decodes it. The
 * caller supplies the file through two functions, so it can arrive while it
 * plays. Only 8-bit 4:2:0 pictures show. */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "dav1d/dav1d.h"
#include "libavcodec/avcodec.h"
#include "libavformat/avformat.h"
#include "libyuv/convert_argb.h"
#include "libyuv/scale.h"
#include "vpx/vp8dx.h"
#include "vpx/vpx_decoder.h"

#ifdef __APPLE__
#include "videotoolbox.h"
#include "audio_aac.h"
#endif

enum { DECODER_AVCODEC, DECODER_VPX, DECODER_DAV1D, DECODER_VIDEOTOOLBOX };

/* The widest or tallest picture Yapfast decodes. A file can claim any size,
 * and each picture would take that much memory. */
enum { MAX_SIDE = 8192 };

/* What yapfast_video_next returns. */
enum { END = 0, PICTURE = 1, SOUND = 2, FAILED = -1, UNSUPPORTED = -2 };

/* Reads up to `size` bytes: how many, 0 at the end of the file, or a
 * negative number to give up. */
typedef int (*yapfast_read)(void *source, uint8_t *buffer, int size);
/* Moves to `offset` as fseek does, returning the new position; with
 * AVSEEK_SIZE, returns the file's size. -1 when it can't. */
typedef int64_t (*yapfast_seek)(void *source, int64_t offset, int whence);

struct yapfast_video_info {
  int width;
  int height;
  /* -1 when the file doesn't say. */
  int64_t duration_ms;
  /* Both 0 without sound. */
  int sample_rate;
  int channels;
};

/* A decoded picture as I420 planes, and how to read its colours. */
struct planes {
  int width;
  int height;
  const uint8_t *data[3];
  int stride[3];
  int bt709;
  int full_range;
};

struct yapfast_video {
  void *source;
  yapfast_read read;
  yapfast_seek seek;
  AVIOContext *io;
  AVFormatContext *format;
  AVPacket *packet;
  /* Where times start, in AV_TIME_BASE units. */
  int64_t start;
  int stream;
  int decoder;
  AVCodecContext *context;
  AVFrame *frame;
  vpx_codec_ctx_t vpx;
  int vpx_ready;
  vpx_codec_iter_t vpx_iter;
  int64_t vpx_pts;
  Dav1dContext *dav1d;
  Dav1dData dav1d_pending;
  Dav1dPicture dav1d_picture;
  int dav1d_holds;
#ifdef __APPLE__
  /* H264, when FFmpeg has no decoder for it. */
  struct yapfast_vt *vt;
  /* AAC, likewise. */
  struct yapfast_aac *aac;
  /* Packets queued for it that haven't been asked about yet, and when the
   * oldest of them plays. */
  unsigned aac_queued;
  int64_t aac_pts;
#endif
  /* How many frames of sound receive_sound may be handed; 0 when the decoder
   * keeps its own buffer. */
  int sound_capacity;
  /* The picture yapfast_video_next gave last. */
  struct planes planes;
  int sound_stream;
  AVCodecContext *sound_context;
  /* The rate and channels of the samples below; a decoder may change them
   * from frame to frame, whatever the file said at the start. */
  int sample_rate;
  int channels;
  AVFrame *sound_frame;
  /* The sound yapfast_video_next gave last, as interleaved floats. */
  float *samples;
  size_t samples_size;
  int sample_frames;
  /* The demuxer reached the end and the decoders were told. */
  int sent_all;
  /* Planes scaled to the size asked for. */
  uint8_t *scaled;
  size_t scaled_size;
};

static int read_packet(void *opaque, uint8_t *buffer, int size) {
  struct yapfast_video *video = opaque;
  int got = video->read(video->source, buffer, size);
  if (got == 0) return AVERROR_EOF;
  return got < 0 ? AVERROR_EXIT : got;
}

static int64_t seek(void *opaque, int64_t offset, int whence) {
  struct yapfast_video *video = opaque;
  return video->seek(video->source, offset, whence & ~AVSEEK_FORCE);
}

void yapfast_video_free(struct yapfast_video *video) {
  if (!video) return;
  avformat_close_input(&video->format);
  if (video->io) av_freep(&video->io->buffer);
  avio_context_free(&video->io);
  av_packet_free(&video->packet);
  avcodec_free_context(&video->context);
  av_frame_free(&video->frame);
  if (video->vpx_ready) vpx_codec_destroy(&video->vpx);
#ifdef __APPLE__
  if (video->vt) yapfast_vt_free(video->vt);
  if (video->aac) yapfast_aac_free(video->aac);
#endif
  if (video->dav1d) {
    if (video->dav1d_holds) dav1d_picture_unref(&video->dav1d_picture);
    dav1d_data_unref(&video->dav1d_pending);
    dav1d_close(&video->dav1d);
  }
  avcodec_free_context(&video->sound_context);
  av_frame_free(&video->sound_frame);
  free(video->samples);
  free(video->scaled);
  free(video);
}

static int open_decoder(struct yapfast_video *video, const AVCodecParameters *parameters,
                        unsigned threads) {
  switch (parameters->codec_id) {
    case AV_CODEC_ID_H264: {
#ifdef __APPLE__
      /* FFmpeg's H264 decoder isn't in the archive here, so the system's is.
       * A file's pictures are decoded out of the order they play, so this
       * decoder holds them and gives them back in order, which is what
       * `low_delay` being off asks for. The file's own parameter sets let it
       * start before the first frame repeats them. */
      (void)threads;
      video->decoder = DECODER_VIDEOTOOLBOX;
      video->vt = yapfast_vt_new(0);
      if (!video->vt) return FAILED;
      if (!parameters->extradata || parameters->extradata_size <= 0) return 0;
      return yapfast_vt_set_extradata(video->vt, parameters->extradata,
                                      (size_t)parameters->extradata_size) < 0
                 ? FAILED
                 : 0;
#else
      const AVCodec *codec = avcodec_find_decoder(parameters->codec_id);
      video->decoder = DECODER_AVCODEC;
      video->context = codec ? avcodec_alloc_context3(codec) : NULL;
      video->frame = av_frame_alloc();
      if (!video->context || !video->frame) return FAILED;
      if (avcodec_parameters_to_context(video->context, parameters) < 0) return FAILED;
      video->context->thread_count = (int)threads;
      video->context->max_pixels = (int64_t)MAX_SIDE * MAX_SIDE;
      return avcodec_open2(video->context, codec, NULL) < 0 ? FAILED : 0;
#endif
    }
    case AV_CODEC_ID_VP8:
    case AV_CODEC_ID_VP9: {
      vpx_codec_dec_cfg_t config = {.threads = threads};
      vpx_codec_iface_t *codec =
          parameters->codec_id == AV_CODEC_ID_VP8 ? vpx_codec_vp8_dx() : vpx_codec_vp9_dx();
      video->decoder = DECODER_VPX;
      if (vpx_codec_dec_init(&video->vpx, codec, &config, 0)) return FAILED;
      video->vpx_ready = 1;
      return 0;
    }
    case AV_CODEC_ID_AV1: {
      Dav1dSettings settings;
      dav1d_default_settings(&settings);
      settings.n_threads = (int)threads;
      settings.frame_size_limit = (unsigned)MAX_SIDE * MAX_SIDE;
      video->decoder = DECODER_DAV1D;
      return dav1d_open(&video->dav1d, &settings) < 0 ? FAILED : 0;
    }
    default:
      return UNSUPPORTED;
  }
}

/* Opens the sound, when the file has some Yapfast can decode; without it, the
 * video plays silently. */
static void open_sound(struct yapfast_video *video) {
  int stream = av_find_best_stream(video->format, AVMEDIA_TYPE_AUDIO, -1, video->stream, NULL, 0);
  if (stream < 0) return;
  const AVCodecParameters *parameters = video->format->streams[stream]->codecpar;
  const AVCodec *codec = avcodec_find_decoder(parameters->codec_id);
#ifdef __APPLE__
  if (!codec && parameters->codec_id == AV_CODEC_ID_AAC) {
    /* FFmpeg's AAC decoder isn't in the archive here, and AAC is the sound
     * most video files have, so the system decodes it instead. */
    int channels = parameters->ch_layout.nb_channels;
    if (parameters->sample_rate <= 0 || channels <= 0) return;
    video->aac = yapfast_aac_new((double)parameters->sample_rate, channels, parameters->extradata,
                                 (size_t)(parameters->extradata_size > 0
                                              ? parameters->extradata_size
                                              : 0));
    if (!video->aac) return;
    size_t bytes = sizeof(float) * (size_t)YAPFAST_AAC_FRAMES * (size_t)channels;
    float *samples = realloc(video->samples, bytes);
    if (!samples) {
      yapfast_aac_free(video->aac);
      video->aac = NULL;
      return;
    }
    video->samples = samples;
    video->samples_size = bytes;
    video->sound_capacity = YAPFAST_AAC_FRAMES;
    video->sound_stream = stream;
    video->sample_rate = parameters->sample_rate;
    video->channels = channels;
    return;
  }
#endif
  AVCodecContext *context = codec ? avcodec_alloc_context3(codec) : NULL;
  AVFrame *frame = av_frame_alloc();
  if (!context || !frame || avcodec_parameters_to_context(context, parameters) < 0 ||
      avcodec_open2(context, codec, NULL) < 0 || context->sample_rate <= 0 ||
      context->ch_layout.nb_channels <= 0) {
    avcodec_free_context(&context);
    av_frame_free(&frame);
    return;
  }
  video->sound_stream = stream;
  video->sound_context = context;
  video->sound_frame = frame;
  video->sample_rate = context->sample_rate;
  video->channels = context->ch_layout.nb_channels;
}

/* Opens the first video in the file `source`, or returns NULL. With
 * `sound`, its sound comes too. What it is goes in `info`. */
struct yapfast_video *yapfast_video_open(void *source, yapfast_read read, yapfast_seek seek_to,
                                   unsigned threads, int sound, struct yapfast_video_info *info) {
  /* FFmpeg writes to the standard error; Yapfast reports what fails itself. */
  av_log_set_level(AV_LOG_QUIET);
  struct yapfast_video *video = calloc(1, sizeof *video);
  if (!video) return NULL;
  video->source = source;
  video->read = read;
  video->seek = seek_to;
  video->sound_stream = -1;
  const int buffer_size = 1 << 16;
  unsigned char *buffer = av_malloc(buffer_size);
  video->io =
      buffer ? avio_alloc_context(buffer, buffer_size, 0, video, read_packet, NULL, seek) : NULL;
  video->format = avformat_alloc_context();
  video->packet = av_packet_alloc();
  if (!video->io) av_free(buffer);
  if (!video->io || !video->format || !video->packet) {
    yapfast_video_free(video);
    return NULL;
  }
  video->format->pb = video->io;
  video->format->flags |= AVFMT_FLAG_CUSTOM_IO;
  /* It frees the context when it fails. */
  if (avformat_open_input(&video->format, NULL, NULL, NULL) < 0) {
    video->format = NULL;
    yapfast_video_free(video);
    return NULL;
  }
  video->stream = av_find_best_stream(video->format, AVMEDIA_TYPE_VIDEO, -1, -1, NULL, 0);
  if (video->stream < 0) {
    yapfast_video_free(video);
    return NULL;
  }
  const AVStream *stream = video->format->streams[video->stream];
  const AVCodecParameters *parameters = stream->codecpar;
  if (parameters->width <= 0 || parameters->height <= 0 || parameters->width > MAX_SIDE ||
      parameters->height > MAX_SIDE || open_decoder(video, parameters, threads) < 0) {
    yapfast_video_free(video);
    return NULL;
  }
  if (sound) open_sound(video);
  for (unsigned index = 0; index < video->format->nb_streams; index++) {
    if ((int)index != video->stream && (int)index != video->sound_stream) {
      video->format->streams[index]->discard = AVDISCARD_ALL;
    }
  }
  video->start = video->format->start_time == AV_NOPTS_VALUE ? 0 : video->format->start_time;
  info->width = parameters->width;
  info->height = parameters->height;
  if (video->format->duration != AV_NOPTS_VALUE) {
    info->duration_ms = video->format->duration / (AV_TIME_BASE / 1000);
  } else if (stream->duration != AV_NOPTS_VALUE) {
    info->duration_ms = av_rescale_q(stream->duration, stream->time_base, (AVRational){1, 1000});
  } else {
    info->duration_ms = -1;
  }
  info->sample_rate = video->sample_rate;
  info->channels = video->channels;
  return video;
}

/* Whether colours are BT.709 rather than BT.601, when the file says; HD
 * pictures are, when it doesn't. */
static int bt709(int said, int height) {
  return said < 0 ? height >= 720 : said;
}

/* A time in `stream`'s units as milliseconds from the start, or -1. Sound
 * an encoder primed with starts a little before; it counts as the start. */
static int64_t milliseconds(const struct yapfast_video *video, int stream, int64_t pts) {
  if (pts == AV_NOPTS_VALUE) return -1;
  int64_t time = av_rescale_q(pts, video->format->streams[stream]->time_base, AV_TIME_BASE_Q);
  return FFMAX(time - video->start, 0) / (AV_TIME_BASE / 1000);
}

/* A time in the picture stream's units as microseconds from the start, which
 * is what the Mac's decoder carries through a picture and hands back with it.
 * A millisecond would do for showing, but two pictures an instant apart would
 * come back with the same time and the decoder would have nothing to order
 * them by. */
static int64_t microseconds(const struct yapfast_video *video, int64_t pts) {
  if (pts == AV_NOPTS_VALUE) return -1;
  int64_t time =
      av_rescale_q(pts, video->format->streams[video->stream]->time_base, AV_TIME_BASE_Q);
  return FFMAX(time - video->start, 0);
}

/* Takes the picture decoder's next picture: 1 with one, 0 when it wants data. */
static int receive_picture(struct yapfast_video *video, int64_t *time_ms) {
  struct planes *planes = &video->planes;
  /* Until a new picture comes, there's none: the last may be freed. */
  *planes = (struct planes){0};
  switch (video->decoder) {
#ifdef __APPLE__
    case DECODER_VIDEOTOOLBOX: {
      struct yapfast_vt_frame frame;
      int64_t pts_us;
      if (!yapfast_vt_next(video->vt, &frame, &pts_us)) return 0;
      *planes = (struct planes){
          .width = frame.width,
          .height = frame.height,
          /* The picture says what its colours are when it says at all, and
           * the height decides otherwise, as it does for FFmpeg's. */
          .bt709 = bt709(frame.bt709, frame.height),
          .full_range = frame.full_range,
      };
      for (int plane = 0; plane < 3; plane++) {
        planes->data[plane] = frame.data[plane];
        planes->stride[plane] = frame.stride[plane];
      }
      *time_ms = pts_us < 0 ? -1 : pts_us / (AV_TIME_BASE / 1000);
      return 1;
    }
#endif
    case DECODER_AVCODEC: {
      AVFrame *frame = video->frame;
      av_frame_unref(frame);
      int received = avcodec_receive_frame(video->context, frame);
      if (received == AVERROR(EAGAIN) || received == AVERROR_EOF) return 0;
      if (received < 0) return FAILED;
      if (frame->format != AV_PIX_FMT_YUV420P && frame->format != AV_PIX_FMT_YUVJ420P) {
        return UNSUPPORTED;
      }
      *planes = (struct planes){
          .width = frame->width,
          .height = frame->height,
          .bt709 = bt709(frame->colorspace == AVCOL_SPC_UNSPECIFIED
                             ? -1
                             : frame->colorspace == AVCOL_SPC_BT709,
                         frame->height),
          .full_range =
              frame->color_range == AVCOL_RANGE_JPEG || frame->format == AV_PIX_FMT_YUVJ420P,
      };
      for (int plane = 0; plane < 3; plane++) {
        planes->data[plane] = frame->data[plane];
        planes->stride[plane] = frame->linesize[plane];
      }
      *time_ms = milliseconds(video, video->stream, frame->best_effort_timestamp);
      return 1;
    }
    case DECODER_VPX: {
      vpx_image_t *image = vpx_codec_get_frame(&video->vpx, &video->vpx_iter);
      if (!image) return 0;
      if (image->fmt != VPX_IMG_FMT_I420) return UNSUPPORTED;
      *planes = (struct planes){
          .width = (int)image->d_w,
          .height = (int)image->d_h,
          .bt709 = bt709(image->cs == VPX_CS_UNKNOWN ? -1 : image->cs == VPX_CS_BT_709,
                         (int)image->d_h),
          .full_range = image->range == VPX_CR_FULL_RANGE,
      };
      for (int plane = 0; plane < 3; plane++) {
        planes->data[plane] = image->planes[plane];
        planes->stride[plane] = image->stride[plane];
      }
      *time_ms = milliseconds(video, video->stream, video->vpx_pts);
      return 1;
    }
    default: {
      if (video->dav1d_holds) {
        dav1d_picture_unref(&video->dav1d_picture);
        video->dav1d_holds = 0;
      }
      if (video->dav1d_pending.sz > 0) {
        int sent = dav1d_send_data(video->dav1d, &video->dav1d_pending);
        if (sent < 0 && sent != DAV1D_ERR(EAGAIN)) return FAILED;
      }
      Dav1dPicture *picture = &video->dav1d_picture;
      int got = dav1d_get_picture(video->dav1d, picture);
      if (got == DAV1D_ERR(EAGAIN)) return 0;
      if (got < 0) return FAILED;
      video->dav1d_holds = 1;
      if (picture->p.layout != DAV1D_PIXEL_LAYOUT_I420 || picture->p.bpc != 8) {
        return UNSUPPORTED;
      }
      const Dav1dSequenceHeader *sequence = picture->seq_hdr;
      *planes = (struct planes){
          .width = picture->p.w,
          .height = picture->p.h,
          .bt709 = bt709(sequence->mtrx == DAV1D_MC_UNKNOWN ? -1 : sequence->mtrx == DAV1D_MC_BT709,
                         picture->p.h),
          .full_range = sequence->color_range,
      };
      planes->data[0] = picture->data[0];
      planes->data[1] = picture->data[1];
      planes->data[2] = picture->data[2];
      planes->stride[0] = (int)picture->stride[0];
      planes->stride[1] = (int)picture->stride[1];
      planes->stride[2] = (int)picture->stride[1];
      *time_ms = milliseconds(video, video->stream, picture->m.timestamp);
      return 1;
    }
  }
}

/* One sample of `frame`'s channel `channel` at `index`, as a float. */
static float sample(const AVFrame *frame, int planar, int channels, int channel, int index) {
  int at = planar ? index : index * channels + channel;
  const uint8_t *data = frame->extended_data[planar ? channel : 0];
  switch (av_get_packed_sample_fmt(frame->format)) {
    case AV_SAMPLE_FMT_U8:
      return (data[at] - 128) / 128.0f;
    case AV_SAMPLE_FMT_S16:
      return ((const int16_t *)data)[at] / 32768.0f;
    case AV_SAMPLE_FMT_S32:
      return (float)(((const int32_t *)data)[at] / 2147483648.0);
    case AV_SAMPLE_FMT_FLT:
      return ((const float *)data)[at];
    case AV_SAMPLE_FMT_DBL:
      return (float)((const double *)data)[at];
    default:
      return 0;
  }
}

/* How many packets to have before asking the converter for samples. It makes
 * none from fewer, and it stops for good once it asks and finds nothing, so a
 * run has to be waiting. Four of them is what one call asks for, so what goes
 * in and what comes out keep pace. */
enum { AAC_RUN = 4 };

/* Takes the sound decoder's next samples: 1 with some, 0 when it wants data. */
static int receive_sound(struct yapfast_video *video, int64_t *time_ms) {
#ifdef __APPLE__
  if (video->aac) {
    /* The converter only makes samples from a run of packets at a time, and it
     * stops for good when it asks for more and finds none, so they wait until
     * there's a run of them, or until the file has given all it has. */
    if (!video->aac_queued) return 0;
    if (video->aac_queued < AAC_RUN && !video->sent_all) return 0;
    video->aac_queued = 0;
    int frames = yapfast_aac_next(video->aac, video->samples, video->sound_capacity);
    if (frames <= 0) return 0;
    video->sample_frames = frames;
    *time_ms = milliseconds(video, video->sound_stream, video->aac_pts);
    return 1;
  }
#endif
  if (!video->sound_context) return 0;
  AVFrame *frame = video->sound_frame;
  av_frame_unref(frame);
  int received = avcodec_receive_frame(video->sound_context, frame);
  if (received == AVERROR(EAGAIN) || received == AVERROR_EOF) return 0;
  /* Sound that can't be decoded is left out. */
  if (received < 0) return 0;
  int channels = frame->ch_layout.nb_channels;
  if (channels <= 0 || frame->nb_samples <= 0 || frame->sample_rate <= 0) return 0;
  size_t needed = (size_t)frame->nb_samples * (size_t)channels * sizeof(float);
  if (video->samples_size < needed) {
    float *grown = realloc(video->samples, needed);
    if (!grown) return FAILED;
    video->samples = grown;
    video->samples_size = needed;
  }
  int planar = av_sample_fmt_is_planar(frame->format);
  for (int index = 0; index < frame->nb_samples; index++) {
    for (int channel = 0; channel < channels; channel++) {
      video->samples[index * channels + channel] = sample(frame, planar, channels, channel, index);
    }
  }
  video->sample_frames = frame->nb_samples;
  video->sample_rate = frame->sample_rate;
  video->channels = channels;
  *time_ms = milliseconds(video, video->sound_stream, frame->best_effort_timestamp);
  return 1;
}

/* Hands a decoder the file's next packet, or tells them both it ended. */
static int feed(struct yapfast_video *video) {
  /* dav1d refused data yet had no picture to give back. */
  if (video->decoder == DECODER_DAV1D && video->dav1d_pending.sz > 0) return FAILED;
  AVPacket *packet = video->packet;
  for (;;) {
    int read = av_read_frame(video->format, packet);
    if (read == AVERROR_EXIT) return FAILED;
    /* A damaged file shows what came before the damage. */
    if (read < 0) {
      video->sent_all = 1;
      if (video->decoder == DECODER_AVCODEC) avcodec_send_packet(video->context, NULL);
#ifdef __APPLE__
      /* Let go of whatever the decoder held back to put in order. */
      if (video->decoder == DECODER_VIDEOTOOLBOX) yapfast_vt_flush(video->vt);
#endif
      if (video->sound_context) avcodec_send_packet(video->sound_context, NULL);
      return 0;
    }
    if (packet->stream_index == video->stream) break;
    if (packet->stream_index == video->sound_stream) {
      /* Damaged sound is left out. */
#ifdef __APPLE__
      if (video->aac) {
        /* The converter takes a run of packets and gives back the samples of
         * all of them, so they wait until it's asked. */
        if (!video->aac_queued) video->aac_pts = packet->pts;
        if (!yapfast_aac_push(video->aac, packet->data, (size_t)packet->size))
          video->aac_queued++;
      } else {
        avcodec_send_packet(video->sound_context, packet);
      }
#else
      avcodec_send_packet(video->sound_context, packet);
#endif
      av_packet_unref(packet);
      return 0;
    }
    av_packet_unref(packet);
  }
  int result = 0;
  switch (video->decoder) {
    case DECODER_AVCODEC: {
      int sent = avcodec_send_packet(video->context, packet);
      /* A damaged packet is skipped. */
      if (sent < 0 && sent != AVERROR_INVALIDDATA) result = FAILED;
      break;
    }
#ifdef __APPLE__
    case DECODER_VIDEOTOOLBOX: {
      /* A file's samples hold each NAL unit's length already. What comes out
       * waits in the decoder for receive_picture to take it. */
      if (yapfast_vt_decode(video->vt, packet->data, (size_t)packet->size, 1,
                            microseconds(video, packet->pts)) < 0) {
        result = FAILED;
      }
      break;
    }
#endif
    case DECODER_VPX:
      video->vpx_iter = NULL;
      video->vpx_pts = packet->pts;
      vpx_codec_decode(&video->vpx, packet->data, (unsigned)packet->size, NULL, 0);
      break;
    default: {
      /* dav1d takes no empty data. */
      if (packet->size <= 0) break;
      uint8_t *data = dav1d_data_create(&video->dav1d_pending, (size_t)packet->size);
      if (!data) {
        result = FAILED;
        break;
      }
      memcpy(data, packet->data, (size_t)packet->size);
      video->dav1d_pending.m.timestamp = packet->pts;
      break;
    }
  }
  av_packet_unref(packet);
  return result;
}

/* Decodes up to the next picture or sound, in the order the file has them,
 * with when it plays in milliseconds from the start, or -1 when the file
 * doesn't say. Returns PICTURE or SOUND, which stay until the next call; END;
 * or a negative number when the video can't play. */
int yapfast_video_next(struct yapfast_video *video, int64_t *time_ms) {
  for (;;) {
    int got = receive_picture(video, time_ms);
    if (got < 0) return got;
    if (got) {
      if (video->planes.width <= 0 || video->planes.height <= 0 ||
          video->planes.width > MAX_SIDE || video->planes.height > MAX_SIDE) {
        return FAILED;
      }
      return PICTURE;
    }
    got = receive_sound(video, time_ms);
    if (got < 0) return got;
    if (got) return SOUND;
    if (video->sent_all) return END;
    int fed = feed(video);
    if (fed < 0) return fed;
  }
}

/* Writes the picture yapfast_video_next gave last into `rgba`, `width * 4`
 * bytes a row, scaled to `width` by `height`. */
int yapfast_video_picture(struct yapfast_video *video, unsigned char *rgba, int width, int height) {
  const struct planes *planes = &video->planes;
  const uint8_t *y = planes->data[0], *u = planes->data[1], *v = planes->data[2];
  int y_stride = planes->stride[0], u_stride = planes->stride[1], v_stride = planes->stride[2];
  if (!y) return FAILED;
  if (width != planes->width || height != planes->height) {
    int chroma_width = (width + 1) / 2, chroma_height = (height + 1) / 2;
    size_t luma = (size_t)width * (size_t)height;
    size_t chroma = (size_t)chroma_width * (size_t)chroma_height;
    if (video->scaled_size < luma + 2 * chroma) {
      uint8_t *grown = realloc(video->scaled, luma + 2 * chroma);
      if (!grown) return FAILED;
      video->scaled = grown;
      video->scaled_size = luma + 2 * chroma;
    }
    uint8_t *scaled_y = video->scaled, *scaled_u = scaled_y + luma, *scaled_v = scaled_u + chroma;
    if (I420Scale(y, y_stride, u, u_stride, v, v_stride, planes->width, planes->height, scaled_y,
                  width, scaled_u, chroma_width, scaled_v, chroma_width, width, height,
                  kFilterBox)) {
      return FAILED;
    }
    y = scaled_y, u = scaled_u, v = scaled_v;
    y_stride = width, u_stride = chroma_width, v_stride = chroma_width;
  }
  const struct YuvConstants *matrix =
      planes->bt709 ? (planes->full_range ? &kYvuF709Constants : &kYvuH709Constants)
                    : (planes->full_range ? &kYvuJPEGConstants : &kYvuI601Constants);
  /* libyuv's ABGR is R, G, B, A in memory: its ARGB with U and V swapped. */
  return I420ToARGBMatrix(y, y_stride, v, v_stride, u, u_stride, rgba, width * 4, matrix, width,
                          height)
             ? FAILED
             : 0;
}

/* The sound yapfast_video_next gave last: frames of interleaved floats, one
 * a channel, and their rate and channels. */
const float *yapfast_video_sound(const struct yapfast_video *video, int *frames, int *rate,
                              int *channels) {
  *frames = video->sample_frames;
  *rate = video->sample_rate;
  *channels = video->channels;
  return video->samples;
}

/* Moves to the last keyframe at or before `time_ms`; what comes before the
 * time is the caller's to skip. */
int yapfast_video_seek(struct yapfast_video *video, int64_t time_ms) {
  int64_t target = video->start + time_ms * (AV_TIME_BASE / 1000);
  if (avformat_seek_file(video->format, -1, INT64_MIN, target, target, 0) < 0) return FAILED;
  if (video->context) avcodec_flush_buffers(video->context);
  if (video->sound_context) avcodec_flush_buffers(video->sound_context);
  video->vpx_iter = NULL;
  if (video->dav1d) {
    dav1d_data_unref(&video->dav1d_pending);
    if (video->dav1d_holds) {
      dav1d_picture_unref(&video->dav1d_picture);
      video->dav1d_holds = 0;
    }
    dav1d_flush(video->dav1d);
  }
  video->planes = (struct planes){0};
  video->sent_all = 0;
#ifdef __APPLE__
  /* Both decoders hold work from before the target, and it plays no part in
   * what comes after it. */
  if (video->vt) yapfast_vt_reset(video->vt);
  if (video->aac) yapfast_aac_reset(video->aac);
  video->aac_queued = 0;
#endif
  return 0;
}
