/* Video files: demuxed by FFmpeg's libavformat; pictures decoded by
 * libavcodec (H264), libvpx (VP8, VP9) or dav1d (AV1), and scaled and
 * converted to RGBA by libyuv; sound decoded by libavcodec (AAC, Opus, MP3,
 * Vorbis, FLAC, PCM). All of it is inside Yapfast's libwebrtc archive. The
 * caller supplies the file through two functions, so it can arrive while it
 * plays. Pictures show in 4:2:0, 4:2:2 or 4:4:4 at 8 or 10 bits, or in
 * 4:2:0 at 12, as far as libyuv goes. */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "dav1d/dav1d.h"
#include "libavcodec/avcodec.h"
#include "libavformat/avformat.h"
#include "libavutil/pixdesc.h"
#include "libyuv/convert_argb.h"
#include "libyuv/scale.h"
#include "vpx/vp8dx.h"
#include "vpx/vpx_decoder.h"

enum { DECODER_AVCODEC, DECODER_VPX, DECODER_DAV1D };

/* The widest or tallest picture Yapfast decodes. A file can claim any size,
 * and each picture would take that much memory. */
enum { MAX_SIDE = 8192 };

/* What yapfast_video_next returns. */
enum { END = 0, PICTURE = 1, SOUND = 2, HELD = 3, FAILED = -1, UNSUPPORTED = -2 };

/* The most pictures' packets, and bytes of them, held back while reading on
 * to the sound: in a file, sound can trail its pictures by a second or so. */
enum { HELD_PACKETS = 256, HELD_BYTES = 4 << 20 };

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

/* How far a picture's colour planes are halved: across and down, across
 * only, or not at all. */
enum { LAYOUT_420, LAYOUT_422, LAYOUT_444 };

/* A decoded picture as planes of Y, U and V, and how to read its colours.
 * Deeper than 8 bits, a sample takes two bytes. */
struct planes {
  int width;
  int height;
  int depth;
  /* A LAYOUT_ value, or -1 for one Yapfast doesn't show. */
  int layout;
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
  /* Pictures' packets read ahead of the sound, oldest at `held_first`. */
  AVPacket *held[HELD_PACKETS];
  int held_first;
  int held_count;
  size_t held_bytes;
  /* The demuxer reached the end, and the sound's decoder was told. */
  int read_all;
  /* The picture decoder was told too. */
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

/* Lets go of the pictures' packets held back. */
static void drop_held(struct yapfast_video *video) {
  for (int index = 0; index < video->held_count; index++) {
    av_packet_free(&video->held[(video->held_first + index) % HELD_PACKETS]);
  }
  video->held_first = 0;
  video->held_count = 0;
  video->held_bytes = 0;
}

void yapfast_video_free(struct yapfast_video *video) {
  if (!video) return;
  drop_held(video);
  avformat_close_input(&video->format);
  if (video->io) av_freep(&video->io->buffer);
  avio_context_free(&video->io);
  av_packet_free(&video->packet);
  avcodec_free_context(&video->context);
  av_frame_free(&video->frame);
  if (video->vpx_ready) vpx_codec_destroy(&video->vpx);
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
      const AVCodec *codec = avcodec_find_decoder(parameters->codec_id);
      video->decoder = DECODER_AVCODEC;
      video->context = codec ? avcodec_alloc_context3(codec) : NULL;
      video->frame = av_frame_alloc();
      if (!video->context || !video->frame) return FAILED;
      if (avcodec_parameters_to_context(video->context, parameters) < 0) return FAILED;
      video->context->thread_count = (int)threads;
      video->context->max_pixels = (int64_t)MAX_SIDE * MAX_SIDE;
      return avcodec_open2(video->context, codec, NULL) < 0 ? FAILED : 0;
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

/* What yapfast_video_open's `sound` asks for: the video alone, its sound
 * too, or only the sound of a file that may have no pictures, such as an MP3
 * or an Ogg file. */
enum { PICTURES_ONLY = 0, WITH_SOUND = 1, SOUND_ONLY = 2 };

/* Opens the first video in the file `source`, or returns NULL. With
 * `sound`, its sound comes too, or with SOUND_ONLY, only its sound, which
 * yapfast_video_next gives without pictures. What it is goes in `info`. */
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
  if (sound == SOUND_ONLY) {
    video->stream = -1;
    /* An MP3's rate and channels are in its frames, not in a header. */
    if (avformat_find_stream_info(video->format, NULL) < 0) {
      yapfast_video_free(video);
      return NULL;
    }
    open_sound(video);
    if (video->sound_stream < 0) {
      yapfast_video_free(video);
      return NULL;
    }
    for (unsigned index = 0; index < video->format->nb_streams; index++) {
      if ((int)index != video->sound_stream) video->format->streams[index]->discard = AVDISCARD_ALL;
    }
    video->start = video->format->start_time == AV_NOPTS_VALUE ? 0 : video->format->start_time;
    info->duration_ms = video->format->duration != AV_NOPTS_VALUE
                            ? video->format->duration / (AV_TIME_BASE / 1000)
                            : -1;
    info->sample_rate = video->sample_rate;
    info->channels = video->channels;
    return video;
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

/* The layout whose colour planes are halved `x_shift` times across and
 * `y_shift` times down, or -1. */
static int layout(unsigned x_shift, unsigned y_shift) {
  if (x_shift == 0) return y_shift == 0 ? LAYOUT_444 : -1;
  if (x_shift == 1) return y_shift == 1 ? LAYOUT_420 : y_shift == 0 ? LAYOUT_422 : -1;
  return -1;
}

/* Whether libyuv can draw the picture: it has no 12-bit 4:2:2 or 4:4:4. */
static int drawable(const struct planes *planes) {
  if (planes->layout < 0) return 0;
  return planes->depth == 8 || planes->depth == 10 ||
         (planes->depth == 12 && planes->layout == LAYOUT_420);
}

/* Takes the picture decoder's next picture: 1 with one, 0 when it wants data. */
static int receive_picture(struct yapfast_video *video, int64_t *time_ms) {
  struct planes *planes = &video->planes;
  /* Until a new picture comes, there's none: the last may be freed. */
  *planes = (struct planes){0};
  switch (video->decoder) {
    case DECODER_AVCODEC: {
      AVFrame *frame = video->frame;
      av_frame_unref(frame);
      int received = avcodec_receive_frame(video->context, frame);
      if (received == AVERROR(EAGAIN) || received == AVERROR_EOF) return 0;
      if (received < 0) return FAILED;
      /* Three planes, in this machine's byte order. */
      const AVPixFmtDescriptor *format = av_pix_fmt_desc_get(frame->format);
      if (!format || format->nb_components != 3 || !(format->flags & AV_PIX_FMT_FLAG_PLANAR) ||
          (format->flags & (AV_PIX_FMT_FLAG_RGB | AV_PIX_FMT_FLAG_BE | AV_PIX_FMT_FLAG_FLOAT))) {
        return UNSUPPORTED;
      }
      int jpeg = frame->format == AV_PIX_FMT_YUVJ420P || frame->format == AV_PIX_FMT_YUVJ422P ||
                 frame->format == AV_PIX_FMT_YUVJ444P;
      *planes = (struct planes){
          .width = frame->width,
          .height = frame->height,
          .depth = format->comp[0].depth,
          .layout = layout(format->log2_chroma_w, format->log2_chroma_h),
          .bt709 = bt709(frame->colorspace == AVCOL_SPC_UNSPECIFIED
                             ? -1
                             : frame->colorspace == AVCOL_SPC_BT709,
                         frame->height),
          .full_range = frame->color_range == AVCOL_RANGE_JPEG || jpeg,
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
      *planes = (struct planes){
          .width = (int)image->d_w,
          .height = (int)image->d_h,
          .depth = image->fmt & VPX_IMG_FMT_HIGHBITDEPTH ? (int)image->bit_depth : 8,
          .layout = layout(image->x_chroma_shift, image->y_chroma_shift),
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
      const Dav1dSequenceHeader *sequence = picture->seq_hdr;
      *planes = (struct planes){
          .width = picture->p.w,
          .height = picture->p.h,
          .depth = picture->p.bpc,
          .layout = picture->p.layout == DAV1D_PIXEL_LAYOUT_I420   ? LAYOUT_420
                    : picture->p.layout == DAV1D_PIXEL_LAYOUT_I422 ? LAYOUT_422
                    : picture->p.layout == DAV1D_PIXEL_LAYOUT_I444 ? LAYOUT_444
                                                                   : -1,
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

/* Takes the sound decoder's next samples: 1 with some, 0 when it wants data. */
static int receive_sound(struct yapfast_video *video, int64_t *time_ms) {
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

/* Reads the file's next packet into `packet`: 1 with the pictures', 0 after
 * handing the sound's to its decoder or at the end, or FAILED. */
static int demux(struct yapfast_video *video, AVPacket *packet) {
  for (;;) {
    int read = av_read_frame(video->format, packet);
    if (read == AVERROR_EXIT) return FAILED;
    /* A damaged file shows what came before the damage. */
    if (read < 0) {
      video->read_all = 1;
      if (video->sound_context) avcodec_send_packet(video->sound_context, NULL);
      return 0;
    }
    if (packet->stream_index == video->stream) return 1;
    if (packet->stream_index == video->sound_stream) {
      /* Damaged sound is left out. */
      avcodec_send_packet(video->sound_context, packet);
      av_packet_unref(packet);
      return 0;
    }
    av_packet_unref(packet);
  }
}

/* Hands the picture decoder its next packet, held back or read from the
 * file, or tells it the file ended. */
static int feed(struct yapfast_video *video) {
  /* dav1d refused data yet had no picture to give back. */
  if (video->decoder == DECODER_DAV1D && video->dav1d_pending.sz > 0) return FAILED;
  AVPacket *packet = video->packet;
  if (video->held_count > 0) {
    AVPacket **held = &video->held[video->held_first];
    video->held_bytes -= (size_t)(*held)->size;
    av_packet_move_ref(packet, *held);
    av_packet_free(held);
    video->held_first = (video->held_first + 1) % HELD_PACKETS;
    video->held_count--;
  } else if (video->read_all) {
    video->sent_all = 1;
    if (video->decoder == DECODER_AVCODEC) avcodec_send_packet(video->context, NULL);
    return 0;
  } else {
    int read = demux(video, packet);
    if (read <= 0) return read;
  }
  int result = 0;
  switch (video->decoder) {
    case DECODER_AVCODEC: {
      int sent = avcodec_send_packet(video->context, packet);
      /* A damaged packet is skipped. */
      if (sent < 0 && sent != AVERROR_INVALIDDATA) result = FAILED;
      break;
    }
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

/* Reads on towards the sound, holding back the pictures' packets: 0 after a
 * packet, HELD when no more can be held or the sound ended, or FAILED. */
static int feed_sound(struct yapfast_video *video) {
  if (!video->sound_context || video->read_all || video->held_count == HELD_PACKETS ||
      video->held_bytes >= HELD_BYTES) {
    return HELD;
  }
  AVPacket *packet = video->packet;
  int read = demux(video, packet);
  if (read <= 0) return read;
  AVPacket *held = av_packet_alloc();
  if (!held) {
    av_packet_unref(packet);
    return FAILED;
  }
  av_packet_move_ref(held, packet);
  video->held[(video->held_first + video->held_count) % HELD_PACKETS] = held;
  video->held_count++;
  video->held_bytes += (size_t)held->size;
  return 0;
}

/* Decodes up to the next picture or sound, in the order the file has them,
 * with when it plays in milliseconds from the start, or -1 when the file
 * doesn't say. Returns PICTURE or SOUND, which stay until the next call; END;
 * or a negative number when the video can't play. Without `pictures`, only
 * sound comes, the pictures' packets held back for later calls, or HELD. */
int yapfast_video_next(struct yapfast_video *video, int64_t *time_ms, int pictures) {
  for (;;) {
    if (pictures) {
      int got = receive_picture(video, time_ms);
      if (got < 0) return got;
      if (got) {
        if (!drawable(&video->planes)) return UNSUPPORTED;
        if (video->planes.width <= 0 || video->planes.height <= 0 ||
            video->planes.width > MAX_SIDE || video->planes.height > MAX_SIDE) {
          return FAILED;
        }
        return PICTURE;
      }
    }
    int got = receive_sound(video, time_ms);
    if (got < 0) return got;
    if (got) return SOUND;
    if (!pictures) {
      int fed = feed_sound(video);
      if (fed != 0) return fed;
      continue;
    }
    if (video->sent_all) return END;
    int fed = feed(video);
    if (fed < 0) return fed;
  }
}

typedef int (*scale_8)(const uint8_t *, int, const uint8_t *, int, const uint8_t *, int, int, int,
                       uint8_t *, int, uint8_t *, int, uint8_t *, int, int, int, enum FilterMode);
typedef int (*scale_16)(const uint16_t *, int, const uint16_t *, int, const uint16_t *, int, int,
                        int, uint16_t *, int, uint16_t *, int, uint16_t *, int, int, int,
                        enum FilterMode);
typedef int (*to_argb_8)(const uint8_t *, int, const uint8_t *, int, const uint8_t *, int,
                         uint8_t *, int, const struct YuvConstants *, int, int);
typedef int (*to_argb_16)(const uint16_t *, int, const uint16_t *, int, const uint16_t *, int,
                          uint8_t *, int, const struct YuvConstants *, int, int);

/* libyuv's functions for each layout. */
static const scale_8 SCALE_8[] = {I420Scale, I422Scale, I444Scale};
static const scale_16 SCALE_16[] = {I420Scale_16, I422Scale_16, I444Scale_16};
static const to_argb_8 TO_ARGB_8[] = {I420ToARGBMatrix, I422ToARGBMatrix, I444ToARGBMatrix};
static const to_argb_16 TO_ARGB_10[] = {I010ToARGBMatrix, I210ToARGBMatrix, I410ToARGBMatrix};

/* Writes the picture yapfast_video_next gave last into `rgba`, `width * 4`
 * bytes a row, scaled to `width` by `height`. */
int yapfast_video_picture(struct yapfast_video *video, unsigned char *rgba, int width, int height) {
  const struct planes *planes = &video->planes;
  const uint8_t *y = planes->data[0], *u = planes->data[1], *v = planes->data[2];
  int y_stride = planes->stride[0], u_stride = planes->stride[1], v_stride = planes->stride[2];
  if (!y) return FAILED;
  int layout = planes->layout;
  /* A sample's bytes. libyuv counts a deeper picture's strides in samples. */
  int bytes = planes->depth > 8 ? 2 : 1;
  if (width != planes->width || height != planes->height) {
    int chroma_width = layout == LAYOUT_444 ? width : (width + 1) / 2;
    int chroma_height = layout == LAYOUT_420 ? (height + 1) / 2 : height;
    size_t luma = (size_t)width * (size_t)height * (size_t)bytes;
    size_t chroma = (size_t)chroma_width * (size_t)chroma_height * (size_t)bytes;
    if (video->scaled_size < luma + 2 * chroma) {
      uint8_t *grown = realloc(video->scaled, luma + 2 * chroma);
      if (!grown) return FAILED;
      video->scaled = grown;
      video->scaled_size = luma + 2 * chroma;
    }
    uint8_t *scaled_y = video->scaled, *scaled_u = scaled_y + luma, *scaled_v = scaled_u + chroma;
    int failed =
        bytes == 1
            ? SCALE_8[layout](y, y_stride, u, u_stride, v, v_stride, planes->width, planes->height,
                              scaled_y, width, scaled_u, chroma_width, scaled_v, chroma_width,
                              width, height, kFilterBox)
            : SCALE_16[layout]((const uint16_t *)y, y_stride / 2, (const uint16_t *)u,
                               u_stride / 2, (const uint16_t *)v, v_stride / 2, planes->width,
                               planes->height, (uint16_t *)scaled_y, width, (uint16_t *)scaled_u,
                               chroma_width, (uint16_t *)scaled_v, chroma_width, width, height,
                               kFilterBox);
    if (failed) return FAILED;
    y = scaled_y, u = scaled_u, v = scaled_v;
    y_stride = width * bytes, u_stride = chroma_width * bytes, v_stride = chroma_width * bytes;
  }
  const struct YuvConstants *matrix =
      planes->bt709 ? (planes->full_range ? &kYvuF709Constants : &kYvuH709Constants)
                    : (planes->full_range ? &kYvuJPEGConstants : &kYvuI601Constants);
  /* libyuv's ABGR is R, G, B, A in memory: its ARGB with U and V swapped. */
  if (bytes == 1) {
    return TO_ARGB_8[layout](y, y_stride, v, v_stride, u, u_stride, rgba, width * 4, matrix, width,
                             height)
               ? FAILED
               : 0;
  }
  /* yapfast_video_next gives 12-bit pictures only in 4:2:0. */
  to_argb_16 to_argb = planes->depth == 10 ? TO_ARGB_10[layout] : I012ToARGBMatrix;
  return to_argb((const uint16_t *)y, y_stride / 2, (const uint16_t *)v, v_stride / 2,
                 (const uint16_t *)u, u_stride / 2, rgba, width * 4, matrix, width, height)
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
  drop_held(video);
  video->read_all = 0;
  video->sent_all = 0;
  return 0;
}
