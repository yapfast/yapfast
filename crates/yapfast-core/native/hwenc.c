/* Hardware encoding for Go Live, through FFmpeg's encoders in
 * vendor/ffmpeg-src: NVIDIA's NVENC, then AMD's AMF on Windows and VA-API
 * (AMD and Intel) on Linux, and Intel's Quick Sync through oneVPL where the
 * build carries it.
 * The same interface as h264.c, so Rust drives both alike: I420 frames in,
 * Annex B access units out, each NAL unit after a start code. Opening tries
 * each encoder in turn and says which one opened, or why none did.
 *
 * Every encoder here reads a frame that reached the processor, except Quick
 * Sync on Windows: a stream whose capture hands over Direct3D 11 textures has
 * the encoder read those textures as they are, through
 * `yapfast_hwenc_encode_texture`, so a frame does not make the round trip to
 * memory and back. That path is offered first, and a driver or a device that
 * refuses it leaves the upload below exactly as it was.
 *
 * Settings, as measured on screen recordings and games
 * (docs/discord/go-live.md):
 * low-latency tuning, no B-frames, a frame out for each frame in, the
 * bitrate capped over half a second without padding a calm screen, H264's
 * Main profile, and parameter sets before each IDR. What NVENC is tuned
 * with is `NVENC_TUNINGS` below, a row per tuning the stream may ask for,
 * so a change to the picture's quality is read, compared and made in one
 * place rather than found among the settings each encoder needs. */
#define _POSIX_C_SOURCE 200809L
#include <stdarg.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#ifdef __linux__
#include <unistd.h>
#endif

#include "libavcodec/avcodec.h"
#include "libavutil/hwcontext.h"
#include "libavutil/mathematics.h"
#include "libavutil/opt.h"
#include "libavutil/pixdesc.h"
#include "libavutil/time.h"

/* Quick Sync's shared surfaces are Direct3D 11's: the device context here is
 * only what tells the derived device which child to be made from, and what
 * reads a texture's own description. Linux's capture arrives as a DMA-BUF,
 * which VA-API's encoder in hwenc_dmabuf.c reads where it lies already, so the
 * shared path is the desktop's that has a Direct3D 11 device, and nothing else
 * here names a platform. */
#ifdef _WIN32
#define YAPFAST_D3D11 1
#include "libavutil/hwcontext_d3d11va.h"
#include "libavutil/hwcontext_qsv.h"
#endif
/* oneVPL's own types, for the surface a Quick Sync session is handed and the
 * pairing a driver reads a texture out of, whether or not this build has the
 * path that makes them. */
#include <mfxvideo.h>

/* The codecs, as Rust numbers them. */
enum { CODEC_H264, CODEC_HEVC, CODEC_AV1 };

/* The most of a capture's surfaces a Quick Sync session remembers, by
 * (texture, subresource): a capture hands back a few surfaces in rotation, and
 * a description of a surface made per frame would be the per-frame allocation
 * this path exists to avoid. Four covers the ring a desktop duplicator keeps,
 * as native/d3d11enc.c's slots allow. */
#define QSV_SURFACES 4

/* One of the caller's textures as Quick Sync reads it: the surface the encoder
 * is handed, and the pairing the driver turns into its own resource out of,
 * in one allocation whose owner is the last frame that references it. The
 * texture is held here rather than borrowed, so a capture that lets go of a
 * surface can't pull it out from under an encode. */
struct qsv_slot {
  void *texture;
  unsigned subresource;
  /* The surface and the pairing beside it, freed — and the texture released —
   * when the last frame reading it is done with it. */
  AVBufferRef *ref;
  mfxFrameSurface1 *surface;
  unsigned long long used;
};

struct yapfast_hwenc {
  AVCodecContext *codec;
  /* FFmpeg's own name for the encoder that opened, such as "h264_nvenc",
   * which is what Rust matches its choice of backend back to: `config` and
   * the report name VA-API's device as well. */
  const char *engine;
  /* VA-API's or Quick Sync's device, and VA-API's frame its encoder reads,
   * on the GPU. */
  AVBufferRef *device;
  AVFrame *surface;
  /* Quick Sync's frames context when its encoder reads the capture's own
   * surfaces: a pool of none, so a frame is a surface with nothing allocated
   * behind it, and `qsv_slots` describes the surfaces handed in. NULL for
   * every other session, which is how the two encodes are told apart. */
  AVBufferRef *qsv_frames;
  /* The same idea for AMF, whose FFmpeg encoder reads a Direct3D 11 texture
   * rather than an MFX surface: a Direct3D 11 frames context with no pool, whose
   * frame carries the caller's texture. NULL for every session that doesn't read
   * the capture's own pictures. */
  AVBufferRef *amf_frames;
  struct qsv_slot qsv_slots[QSV_SURFACES];
  /* The clock the least-recently-used of those surfaces is chosen by. */
  unsigned long long qsv_used;
  /* Why Quick Sync's shared surfaces didn't open, when the stream asked for
   * them and the driver or the device refused: the encoder reads an upload
   * instead, and the open's report says what the frame lost. Empty when the
   * path opened, or when nothing asked for it. */
  char shared_why[112];
  /* Why the last frame wasn't encoded, when one wasn't and the reason isn't
   * one of the numbers this returns: the surface handed over wasn't one this
   * session could read, and a capture told that once fixes it rather than
   * losing every frame to a reason nobody can see. */
  char error[112];
  /* The frame handed over, in memory: I420 for NVENC, NV12 for VA-API, AMF
   * and Quick Sync. */
  AVFrame *frame;
  AVPacket *packet;
  /* Packets handed back for an earlier picture than the frame given: a
   * driver holding one back, whose time has gone by, so it is dropped
   * rather than sent under the next frame's time. */
  unsigned late;
  /* Whether a picture was dropped that way: it is one the frames after it
   * reference, and the viewer never got it, so the next frame is asked for
   * as a keyframe rather than as a delta built on what wasn't decoded. */
  int recover;
  /* The format, colour and time base the encoder took, and the rate asked
   * for, for Rust to log once: what the stream actually got. */
  char config[80];
  /* AMF hands a frame back a little after taking it, and is polled for it. */
  int poll;
  /* Whether Quick Sync opened: its cap and target are the pair
   * `set_qsv_rate` makes, which a new rate has to keep. */
  int qsv;
  /* The optional settings FFmpeg didn't have or its driver didn't keep,
   * separated by commas, and empty when it kept them all: Rust warns about
   * what the stream lost. */
  char notes[96];
  /* How hard the encoder works per frame, how its cap is spent and how
   * often a viewer waits for a keyframe, in the words of the tuning that
   * chose them, for Rust to log once: the settings a picture's quality is
   * compared by, which Rust chooses none of. */
  char tuning[96];
  /* The access unit the last frame produced, kept until Rust copies it into
   * the buffer it holds: encoding the picture again to get it into a larger
   * buffer would send a frame referencing one the receiver never got. A frame
   * is one packet, which is held by its own reference, so the unit is copied
   * once, straight into what is sent; only a frame that came out as several
   * packets is gathered into `staged`, which then holds one frame's worth. */
  AVPacket *held;
  int holding;
  unsigned char *staged;
  size_t staged_capacity;
  size_t staged_size;
};

/* Keeps `size` bytes of `bytes`, at `at` into the staged access unit, for Rust
 * to copy out. The buffer grows to the largest frame seen and no further, so a
 * stream asks for memory only until its frames settle. Returns 0 when memory
 * ran out. */
static int stage(struct yapfast_hwenc *h, size_t at, const unsigned char *bytes, size_t size) {
  if (at + size > h->staged_capacity) {
    unsigned char *larger = realloc(h->staged, at + size);
    if (!larger) return 0;
    h->staged = larger;
    h->staged_capacity = at + size;
  }
  memcpy(h->staged + at, bytes, size);
  return 1;
}

/* FFmpeg's first error on this thread since it was cleared, for saying why
 * an encoder didn't open: the cause, where later ones only say what failed
 * because of it. */
static _Thread_local char first_error[512];

static void keep_error(void *context, int level, const char *format, va_list args) {
  (void)context;
  if (level > AV_LOG_ERROR || first_error[0]) return;
  vsnprintf(first_error, sizeof first_error, format, args);
  size_t length = strlen(first_error);
  if (length && first_error[length - 1] == '\n') first_error[length - 1] = 0;
}

/* What a setting is worth to the stream: one it can't do without, or one
 * that only improves the picture or the latency. */
enum { OPTIONAL_SETTING, REQUIRED_SETTING };

/* One of an encoder's settings, for setting several in turn. */
struct setting {
  int worth;
  const char *name, *value;
};

/* Sets `name` to `value` on the encoder being opened, which is a number or
 * one of an option's names. A required setting that FFmpeg or the driver
 * refuses fails the open, and the reason it gives is what `try_open`
 * reports; an optional one it refuses is noted for Rust to warn about, and
 * the encoder goes on without it. Returns 0, or a required setting's
 * failure. */
static int set_option(struct yapfast_hwenc *h, int worth, const char *name, const char *value) {
  int ret = av_opt_set(h->codec->priv_data, name, value, 0);
  if (ret >= 0) return 0;
  if (worth == REQUIRED_SETTING) {
    /* FFmpeg's option handling logs nothing, so the setting that the
     * stream needed has to be named here to say why it didn't open. */
    if (!first_error[0])
      snprintf(first_error, sizeof first_error, "%s=%s: %s", name, value,
               ret == AVERROR_OPTION_NOT_FOUND ? "this FFmpeg has no such setting" : "refused");
    return ret;
  }
  size_t used = strlen(h->notes);
  if (used < sizeof h->notes - 1)
    snprintf(h->notes + used, sizeof h->notes - used, "%s%s", used ? "," : "", name);
  return 0;
}

/* Sets each setting in turn, stopping at the first a required one refuses. */
static int set_options(struct yapfast_hwenc *h, const struct setting *settings, size_t count) {
  for (size_t i = 0; i < count; i++) {
    int ret = set_option(h, settings[i].worth, settings[i].name, settings[i].value);
    if (ret < 0) return ret;
  }
  return 0;
}

static void set_rate(AVCodecContext *codec, unsigned kbps) {
  codec->bit_rate = (int64_t)kbps * 1000;
  codec->rc_max_rate = (int64_t)kbps * 1000;
  codec->rc_buffer_size = (int)(kbps * 500);
}

/* The rate Quick Sync is opened with, and moved to. FFmpeg's Quick Sync reads
 * its rate control out of these numbers, and one whose target equals its cap
 * it takes as constant bitrate, which keeps a calm screen at the cap by
 * padding it. So the cap is the stream's target, over the same half second
 * the other encoders are given, and the average is a kbit/s under it: that is
 * MFX's VBR, spending what a picture needs up to the cap. The encoders here
 * choose none of this, and a measurement on Intel hardware is what would
 * change it (docs/discord/go-live.md). */
static void set_qsv_rate(AVCodecContext *codec, unsigned kbps) {
  set_rate(codec, kbps);
  codec->bit_rate = (int64_t)kbps * 1000 - 1000;
}

/* One complete set of the settings NVENC is opened with: a row per tuning
 * the stream can ask for, and the whole set in one place so that what makes
 * a picture better can be read and changed without hunting through the open
 * below. A new tuning is a row here and a name in Rust's `Tuning`, and the
 * open is untouched.
 *
 * `preset` is how much work NVENC does per frame: p1 is the fastest and p7
 * the slowest, and every step down that ladder is GPU time the same card
 * isn't spending on the game, so the price is paid in the game's frames as
 * well as the encoder's. p4 is where screen recordings and games landed
 * (docs/discord/go-live.md): slower presets looked no better at the bitrates
 * a Discord stream carried. That comparison has not been run again, so p4 is
 * what a share gets unless it asks for more, and p7 is the row the share
 * menu's "Higher quality" box asks for: the slower preset on the same
 * tuning, for a picture worth the card's time. Neither row moves `tune`,
 * `rc`, `zerolatency`, `delay` or `forced_idr`: what a viewer waits for is
 * the same either way.
 *
 * `tune` weights the pictures the encoder spends bits on; `ll` is the
 * low-latency one, and a stream that is a picture someone is waiting for is
 * not a candidate for any other.
 *
 * `rc` is how the cap is spent: `vbr` leaves a calm screen below it, where
 * `cbr` would fill a still screen out to the cap with bits nobody looks at.
 * Whether that calmness costs a burst its sharpness is a question for a
 * measurement, not for a default.
 *
 * `zerolatency` and `delay` are whether NVENC may hold a frame back to have
 * the next one to look at. No row here holds one: a held frame is delay in
 * every viewer's picture, so a row that changes these is a row that has to
 * say what its delay costs.
 *
 * `forced_idr` is what lets a viewer who joins, or one whose picture broke,
 * be given a keyframe at once instead of waiting out the interval.
 *
 * The keyframe interval itself (three seconds, in `open_encoder`) is not a
 * column: a shorter one costs a keyframe's bits more often, and a longer one
 * is what a viewer who joins waits for, so it moves with the experiment that
 * moves a preset rather than with the preset it sits in. */
struct nvenc_tuning {
  const char *name, *preset, *tune, *rc, *zerolatency, *delay, *forced_idr;
};

/* The tunings the stream may ask for; the first is the one it asks for when
 * it names none. Rust names one from its `Effort`: p4 for a share as it
 * comes, p7 for one whose menu box asked for the sharper picture. */
static const struct nvenc_tuning NVENC_TUNINGS[] = {
    {"p4", "p4", "ll", "vbr", "1", "0", "1"},
    {"p7", "p7", "ll", "vbr", "1", "0", "1"},
};

/* The tuning Rust asked for by name, or the first row when it named none:
 * a name that isn't here is NULL, since Rust names one from this set and an
 * unknown one is a mistake worth failing the open for rather than encoding
 * something nobody chose. */
static const struct nvenc_tuning *nvenc_tuning(const char *name) {
  for (size_t i = 0; i < sizeof NVENC_TUNINGS / sizeof *NVENC_TUNINGS; i++)
    if (!name || !strcmp(name, NVENC_TUNINGS[i].name)) return &NVENC_TUNINGS[i];
  return NULL;
}

/* The settings `tuning` opens NVENC with, in the order they are set, and
 * whether the stream can't do without each. The open and the report Rust
 * holds the shipped tuning to both read this one list, so a setting can't be
 * needed in one of them and droppable in the other. */
static size_t nvenc_settings(const struct nvenc_tuning *tuning, struct setting *settings) {
  const struct setting wanted[] = {
      {OPTIONAL_SETTING, "preset", tuning->preset},
      {REQUIRED_SETTING, "tune", tuning->tune},
      {OPTIONAL_SETTING, "rc", tuning->rc},
      {OPTIONAL_SETTING, "zerolatency", tuning->zerolatency},
      {REQUIRED_SETTING, "delay", tuning->delay},
      {REQUIRED_SETTING, "forced-idr", tuning->forced_idr},
  };
  memcpy(settings, wanted, sizeof wanted);
  return sizeof wanted / sizeof *wanted;
}

/* Opens NVENC with `tuning`. Low-latency tuning, no frame delay and IDR
 * frames on demand are what the stream can't do without, so a driver that
 * refuses one fails the open; a preset, a rate control or a zero-latency
 * hint this FFmpeg doesn't have or the driver refuses only costs the
 * picture, so it is noted and the encoder goes on without it. */
static int set_nvenc(struct yapfast_hwenc *h, const struct nvenc_tuning *tuning,
                     unsigned codec_id) {
  struct setting settings[8];
  size_t count = nvenc_settings(tuning, settings);
  int ret = set_options(h, settings, count);
  if (ret >= 0 && codec_id == CODEC_H264) ret = set_option(h, REQUIRED_SETTING, "profile", "main");
  return ret;
}

/* Opens Quick Sync with the settings it is tuned by, wherever it is opened
 * from: the upload below and the capture's own surfaces are the same session
 * on the same card, and what makes their picture is the same list. One frame in
 * flight, so a frame doesn't wait for the next; an IDR when one is asked for;
 * and the average obeyed frame by frame rather than banked. The stream needs
 * the first two, so a driver that can't do either fails the open; the medium
 * preset and the per-frame average only improve the picture and the latency. */
static int set_qsv(struct yapfast_hwenc *h, unsigned codec_id) {
  const struct setting qsv_settings[] = {
      {REQUIRED_SETTING, "async_depth", "1"},
      {REQUIRED_SETTING, "forced_idr", "1"},
      {OPTIONAL_SETTING, "preset", "medium"},
      {OPTIONAL_SETTING, "low_delay_brc", "1"},
  };
  int ret = set_options(h, qsv_settings, sizeof qsv_settings / sizeof *qsv_settings);
  /* Look-ahead is off in FFmpeg's default and is asked for by name where the
   * encoder has the switch, so a default that changed can't add a frame of
   * delay to the stream. AV1 has only a depth, which does nothing without
   * extended rate control. */
  if (ret >= 0 && codec_id != CODEC_AV1) ret = set_option(h, OPTIONAL_SETTING, "look_ahead", "0");
  /* AV1 encodes in Quick Sync's low-power engine, the only one it has. */
  if (ret >= 0 && codec_id == CODEC_AV1) ret = set_option(h, OPTIONAL_SETTING, "low_power", "1");
  return ret;
}

static void close_encoder(struct yapfast_hwenc *h) {
  avcodec_free_context(&h->codec);
  av_frame_free(&h->surface);
  av_frame_free(&h->frame);
  av_packet_free(&h->packet);
  av_packet_free(&h->held);
  h->holding = 0;
  av_buffer_unref(&h->device);
  /* The surfaces the capture's textures were read through, each holding its
   * texture until the last frame reading it is done: giving up the reference
   * here is all the release a surface needs. */
  for (size_t i = 0; i < QSV_SURFACES; i++) {
    av_buffer_unref(&h->qsv_slots[i].ref);
    memset(&h->qsv_slots[i], 0, sizeof h->qsv_slots[i]);
  }
  av_buffer_unref(&h->qsv_frames);
  av_buffer_unref(&h->amf_frames);
  free(h->staged);
  h->staged = NULL;
  h->staged_capacity = h->staged_size = 0;
}

/* A render node for VA-API, and the libva driver for it. */
struct va_device {
  const char *path, *driver;
};

/* VA-API's device and a pool of NV12 surfaces for the encoder to read. */
static int open_vaapi(struct yapfast_hwenc *h, AVCodecContext *codec, struct va_device device) {
  AVDictionary *options = NULL;
  av_dict_set(&options, "driver", device.driver, 0);
  int ret = av_hwdevice_ctx_create(&h->device, AV_HWDEVICE_TYPE_VAAPI, device.path, options, 0);
  av_dict_free(&options);
  if (ret < 0) return ret;
  AVBufferRef *frames = av_hwframe_ctx_alloc(h->device);
  if (!frames) return AVERROR(ENOMEM);
  AVHWFramesContext *pool = (AVHWFramesContext *)frames->data;
  pool->format = AV_PIX_FMT_VAAPI;
  pool->sw_format = AV_PIX_FMT_NV12;
  pool->width = codec->width;
  pool->height = codec->height;
  pool->initial_pool_size = 2;
  ret = av_hwframe_ctx_init(frames);
  if (ret >= 0) codec->hw_frames_ctx = av_buffer_ref(frames);
  av_buffer_unref(&frames);
  if (ret < 0) return ret;
  h->surface = av_frame_alloc();
  return h->surface ? 0 : AVERROR(ENOMEM);
}

/* Quick Sync's own device and frames context when its encoder reads the
 * capture's surfaces instead of an upload: the session is derived from the
 * device a capture's textures live on, and its frame pool is a pool of none,
 * so a surface carries nothing but the pairing the driver reads the texture
 * out of. Returns 0, or a failure whose reason is in `first_error`.
 *
 * The frames context's `frame_type` is what makes the encoder ask the driver
 * for video memory rather than system memory (`MFX_IOPATTERN_IN_VIDEO_MEMORY`,
 * from its own reading of this field), and Quick Sync's `info` is FFmpeg's to
 * make: a dynamic pool is given the one surface description every surface here
 * copies, so it is read back from the frames context rather than written here.
 * Linux has no texture to hand in — its capture is a DMA-BUF, and VA-API's
 * encoder reads that where it lies — so this is the Windows' path and says so. */
/* AMF reads a Direct3D 11 texture where Quick Sync reads an MFX surface, and
 * FFmpeg's AMF encoders take one as `AV_PIX_FMT_D3D11`: the frame names the
 * texture and the subresource inside it, and `amf_surface_alloc` wraps that in
 * the AMFTexture the driver encodes from. So the frames context here is Direct3D
 * 11's own, with no pool — a frame with no buffer behind it is one the caller
 * filled, which is the whole of what makes this path copy nothing. */
static int open_amf_shared(struct yapfast_hwenc *h, AVCodecContext *codec, void *device) {
#if YAPFAST_D3D11
  AVBufferRef *frames = av_hwdevice_ctx_alloc(AV_HWDEVICE_TYPE_D3D11VA);
  if (!frames) return AVERROR(ENOMEM);
  AVD3D11VADeviceContext *hwctx = ((AVHWDeviceContext *)frames->data)->hwctx;
  /* The device context releases this reference when the session's device goes,
   * and the session holds it for as long as it can read a surface. */
  ((ID3D11Device *)device)->lpVtbl->AddRef((ID3D11Device *)device);
  hwctx->device = device;
  int ret = av_hwdevice_ctx_init(frames);
  if (ret < 0) {
    av_buffer_unref(&frames);
    return ret;
  }
  /* The session's device is the capture's, not one of its own: a texture can
   * only be read by the device that holds it. */
  av_buffer_unref(&h->device);
  h->device = frames;
  AVBufferRef *pool_ref = av_hwframe_ctx_alloc(h->device);
  if (!pool_ref) return AVERROR(ENOMEM);
  AVHWFramesContext *pool = (AVHWFramesContext *)pool_ref->data;
  pool->format = AV_PIX_FMT_D3D11;
  pool->sw_format = AV_PIX_FMT_NV12;
  pool->width = codec->width;
  pool->height = codec->height;
  /* No texture is pre-allocated, which is what makes the encoder take the one in
   * the frame it is given rather than one FFmpeg made. */
  pool->initial_pool_size = 0;
  ret = av_hwframe_ctx_init(pool_ref);
  if (ret >= 0) codec->hw_frames_ctx = av_buffer_ref(pool_ref);
  av_buffer_unref(&pool_ref);
  if (ret < 0 || !codec->hw_frames_ctx) return ret < 0 ? ret : AVERROR(ENOMEM);
  codec->pix_fmt = AV_PIX_FMT_D3D11;
  h->amf_frames = av_buffer_ref(codec->hw_frames_ctx);
  return h->amf_frames ? 0 : AVERROR(ENOMEM);
#else
  (void)h;
  (void)codec;
  (void)device;
  snprintf(first_error, sizeof first_error,
           "AMF's shared surfaces are Direct3D 11's, and this machine's capture is a DMA-BUF");
  return AVERROR(ENOSYS);
#endif
}

static int open_qsv_shared(struct yapfast_hwenc *h, AVCodecContext *codec, void *device) {
#if YAPFAST_D3D11
  AVBufferRef *child = av_hwdevice_ctx_alloc(AV_HWDEVICE_TYPE_D3D11VA);
  if (!child) return AVERROR(ENOMEM);
  AVD3D11VADeviceContext *child_hwctx = ((AVHWDeviceContext *)child->data)->hwctx;
  /* Held for as long as the session can read a surface from the device: the
   * device context releases the reference added here, and the Quick Sync device
   * below holds that context for as long as it lives. */
  ((ID3D11Device *)device)->lpVtbl->AddRef((ID3D11Device *)device);
  child_hwctx->device = device;
  int ret = av_hwdevice_ctx_init(child);
  if (ret >= 0) ret = av_hwdevice_ctx_create_derived(&h->device, AV_HWDEVICE_TYPE_QSV, child, 0);
  av_buffer_unref(&child);
  if (ret < 0) return ret;
  AVBufferRef *frames = av_hwframe_ctx_alloc(h->device);
  if (!frames) return AVERROR(ENOMEM);
  AVHWFramesContext *pool = (AVHWFramesContext *)frames->data;
  AVQSVFramesContext *qsv = pool->hwctx;
  pool->format = AV_PIX_FMT_QSV;
  pool->sw_format = AV_PIX_FMT_NV12;
  pool->width = codec->width;
  pool->height = codec->height;
  /* No surface is pre-allocated, which is what makes the encoder take the one
   * in the frame it is given: a fixed pool would have FFmpeg look the surface
   * up in it and hand the driver one of its own instead. */
  pool->initial_pool_size = 0;
  /* A capture's texture is a processor's render target, and the driver is told
   * so before the frames context is made, since that is where the flag is read
   * for what kind of surface the surfaces are. */
  qsv->frame_type = MFX_MEMTYPE_VIDEO_MEMORY_PROCESSOR_TARGET | MFX_MEMTYPE_SHARED_RESOURCE;
  ret = av_hwframe_ctx_init(frames);
  if (ret >= 0) codec->hw_frames_ctx = av_buffer_ref(frames);
  av_buffer_unref(&frames);
  if (ret < 0 || !codec->hw_frames_ctx) return ret < 0 ? ret : AVERROR(ENOMEM);
  /* The encoder is told what the frames context says, which is what it checks
   * against on the way in: the surfaces are Quick Sync's, and the picture in
   * them is NV12. */
  codec->pix_fmt = AV_PIX_FMT_QSV;
  h->qsv_frames = av_buffer_ref(codec->hw_frames_ctx);
  return h->qsv_frames ? 0 : AVERROR(ENOMEM);
#else
  (void)h;
  (void)codec;
  (void)device;
  snprintf(first_error, sizeof first_error,
           "Quick Sync's shared surfaces are Direct3D 11's, and this machine's capture is a "
           "DMA-BUF, which VA-API's encoder reads where it lies");
  return AVERROR(ENOSYS);
#endif
}

/* Gives back one of the capture's surfaces and the texture behind it: the
 * surface and its pairing are one allocation, and the reference the slot took
 * on the texture is the one released here, so a capture that dropped a surface
 * while a frame was still reading it is safe. Only a session with a texture to
 * describe makes one of these, so only that build has the callback. */
#if YAPFAST_D3D11
static void qsv_surface_free(void *opaque, uint8_t *data) {
  (void)opaque;
  mfxFrameSurface1 *surface = (mfxFrameSurface1 *)data;
  ID3D11Texture2D *texture = (ID3D11Texture2D *)((mfxHDLPair *)surface->Data.MemId)->first;
  if (texture) texture->lpVtbl->Release(texture);
  av_free(data);
}
#endif

/* The surface the encoder reads `texture` through: made the first time that
 * texture is offered and kept while it keeps coming back, since a capture hands
 * back a few surfaces in rotation and a description of one per frame would be
 * the per-frame allocation this path is here to avoid. NULL when the texture
 * isn't one this session can read, with the reason in `h->error`. */
static struct qsv_slot *qsv_surface(struct yapfast_hwenc *h, void *texture,
                                    unsigned subresource) {
#if YAPFAST_D3D11
  ID3D11Texture2D *surface = texture;
  struct qsv_slot *empty = NULL, *oldest = NULL;
  for (size_t i = 0; i < QSV_SURFACES; i++) {
    struct qsv_slot *slot = &h->qsv_slots[i];
    if (!slot->texture) {
      if (!empty) empty = slot;
      continue;
    }
    if (slot->texture == texture && slot->subresource == subresource) {
      slot->used = ++h->qsv_used;
      return slot;
    }
    if (!oldest || slot->used < oldest->used) oldest = slot;
  }
  D3D11_TEXTURE2D_DESC desc;
  surface->lpVtbl->GetDesc(surface, &desc);
  /* The size is the stream's, read back rather than assumed: a capture of a
   * screen that isn't the stream's size has to be scaled before it is encoded,
   * and a surface that says so here is a frame the caller converts and hands
   * over again rather than one the encoder stretches to fit. */
  if (desc.Width != (UINT)h->codec->width || desc.Height != (UINT)h->codec->height) {
    snprintf(h->error, sizeof h->error,
             "the capture's surface is %ux%u and the stream %dx%d, so it has to be scaled "
             "before it is encoded",
             desc.Width, desc.Height, h->codec->width, h->codec->height);
    return NULL;
  }
  if (desc.Format != DXGI_FORMAT_NV12) {
    snprintf(h->error, sizeof h->error,
             "the capture's surface is DXGI format %u, and a session reading surfaces "
             "reads NV12",
             (unsigned)desc.Format);
    return NULL;
  }
  if (subresource >= desc.MipLevels * desc.ArraySize) {
    snprintf(h->error, sizeof h->error, "the capture's surface has no subresource %u",
             subresource);
    return NULL;
  }
  /* The surface the encoder is handed and the pairing the driver reads the
   * texture out of, in one buffer: FFmpeg's encoder copies the surface out of
   * the frame it is given and reads the pairing where it points, so both have
   * to live as long as the frame does, and the frame that outlives them all is
   * the one this buffer is referenced by. */
  size_t bytes = sizeof(mfxFrameSurface1) + sizeof(mfxHDLPair);
  mfxFrameSurface1 *built = av_mallocz(bytes);
  if (!built) {
    snprintf(h->error, sizeof h->error, "no memory for a description of a surface");
    return NULL;
  }
  mfxHDLPair *pair = (mfxHDLPair *)(built + 1);
  /* Every surface is described the way FFmpeg described the pool it isn't
   * allocating from: the width and height padded to what the driver reads in
   * whole blocks, the crop the stream's own size, NV12's fourcc and 4:2:0. A
   * frames context that opened with a pool of none always has one, and asking
   * rather than assuming costs a branch a frame. */
  const AVHWFramesContext *frames = (const AVHWFramesContext *)h->qsv_frames->data;
  const mfxFrameInfo *info = ((const AVQSVFramesContext *)frames->hwctx)->info;
  if (!info) {
    av_free(built);
    snprintf(h->error, sizeof h->error,
             "the encoder's frames context describes no surface to hand it one");
    return NULL;
  }
  built->Info = *info;
  /* The texture's own size, not the frames context's aligned one: a stream sized
   * to a screen that isn't a multiple of sixteen would otherwise be read at a
   * pitch wider than the subresource has. */
  built->Info.Width = desc.Width;
  built->Info.Height = desc.Height;
  pair->first = texture;
  /* A render target is addressed as the whole surface; one that isn't is
   * addressed by the subresource inside it, the way FFmpeg addresses the
   * textures it makes for an encoder. */
  pair->second = (desc.BindFlags & D3D11_BIND_RENDER_TARGET) ? (mfxMemId)MFX_INFINITE
                                                             : (mfxMemId)(size_t)subresource;
  built->Data.MemId = pair;
  AVBufferRef *ref = av_buffer_create((uint8_t *)built, bytes, qsv_surface_free, NULL, 0);
  if (!ref) {
    av_free(built);
    snprintf(h->error, sizeof h->error, "no memory for a description of a surface");
    return NULL;
  }
  struct qsv_slot *reuse = empty ? empty : oldest;
  /* The reference is taken before the slot's is let go of: a slot naming this
   * same texture would otherwise be released to nothing and then written to. */
  surface->lpVtbl->AddRef(surface);
  av_buffer_unref(&reuse->ref);
  reuse->texture = texture;
  reuse->subresource = subresource;
  reuse->ref = ref;
  reuse->surface = built;
  reuse->used = ++h->qsv_used;
  return reuse;
#else
  (void)h;
  (void)texture;
  (void)subresource;
  return NULL;
#endif
}

/* Whether this machine has the card NVENC encodes on, asked once: CUDA is
 * NVIDIA's, and FFmpeg's NVENC reaches the card through it. d3d11enc.c asks
 * the adapter it was handed for its VendorId, and this side is handed no
 * adapter to ask. A build with no CUDA device type can't answer, so it says
 * yes and lets the open report what the driver said. */
static int nvidia_card(void) {
  static int present = -1;
  if (present >= 0) return present;
  AVBufferRef *device = NULL;
  int ret = av_hwdevice_ctx_create(&device, AV_HWDEVICE_TYPE_CUDA, NULL, NULL, 0);
  av_buffer_unref(&device);
  present = ret >= 0 || ret == AVERROR(ENOSYS);
  return present;
}

static int open_encoder(struct yapfast_hwenc *h, const AVCodec *found, struct va_device device,
                        void *capture_device, const struct nvenc_tuning *nvenc,
                        unsigned codec_id, unsigned width, unsigned height, unsigned fps,
                        unsigned kbps) {
  int vaapi = device.path != NULL, amf = strstr(found->name, "_amf") != NULL,
      qsv = strstr(found->name, "_qsv") != NULL;
  AVCodecContext *codec = h->codec = avcodec_alloc_context3(found);
  if (!codec) return AVERROR(ENOMEM);
  codec->width = (int)width;
  codec->height = (int)height;
  codec->time_base = (AVRational){1, (int)fps};
  codec->framerate = (AVRational){(int)fps, 1};
  /* A new viewer waits at most this long for a picture. */
  codec->gop_size = (int)fps * 3;
  codec->max_b_frames = 0;
  codec->pix_fmt =
      vaapi ? AV_PIX_FMT_VAAPI : (amf || qsv) ? AV_PIX_FMT_NV12 : AV_PIX_FMT_YUV420P;
  /* What the pixels are, not what a player might prefer them to be: the
   * screen, the camera and the clip all reach an encoder through libyuv's
   * ABGR to I420, whose matrix is BT.601's over its limited range, so that
   * is what the encoder is told. h264.c tells x264 the same four numbers;
   * a conversion that changed would have to change both. */
  codec->color_primaries = AVCOL_PRI_SMPTE170M;
  codec->color_trc = AVCOL_TRC_SMPTE170M;
  codec->colorspace = AVCOL_SPC_SMPTE170M;
  codec->color_range = AVCOL_RANGE_MPEG;
  /* Quick Sync's cap and target are a pair its rate control is chosen from,
   * so the one place that sets them is the one that knows. */
  if (qsv)
    set_qsv_rate(codec, kbps);
  else
    set_rate(codec, kbps);
  if (codec_id == CODEC_H264) codec->profile = AV_PROFILE_H264_MAIN;
  /* What the tuning that opened the encoder said its quality is, for Rust to
   * log: FFmpeg reports no more of it back than it was told, and whatever a
   * driver refused is in `notes` instead. */
  const char *preset = "no", *rate = "no";
  int ret = 0;
  if (vaapi) {
    /* Capped VBR, which leaves a calm screen below the cap rather than
     * padding it, and one frame in flight so a frame doesn't wait: the
     * stream needs both, so a driver that can't do either fails the open. */
    const struct setting vaapi_settings[] = {
        {REQUIRED_SETTING, "rc_mode", "VBR"},
        {REQUIRED_SETTING, "async_depth", "1"},
    };
    ret = set_options(h, vaapi_settings, sizeof vaapi_settings / sizeof *vaapi_settings);
    if (ret < 0) return ret;
    rate = "VBR";
    ret = open_vaapi(h, codec, device);
    if (ret < 0) return ret;
  } else if (qsv) {
    /* Quick Sync is fed from the processor by default, as AMF is: NV12 frames
     * in system memory, which FFmpeg copies into the surfaces its session
     * encodes from. A stream whose capture already holds its pictures on the
     * card is offered the path below first, where a frame never reaches memory
     * at all; everything after it is the upload, unchanged, for a stream with
     * no surfaces to hand over and for a driver or a device that refused them.
     *
     * The session is asked for Intel's GPU through oneVPL, which is the device
     * `-init_hw_device qsv` makes: a machine can hold another maker's card as
     * well, and which implementation oneVPL picks by itself is not this side's
     * to guess. A build with no QSV device to make leaves FFmpeg to open its
     * own internal session, which is the same upload. */
    if (capture_device) {
      int shared = open_qsv_shared(h, codec, capture_device);
      if (shared < 0) {
        /* Not a reason to stop: the upload below encodes the same picture, one
         * frame behind the processor's copy of it. Why the surfaces weren't
         * taken goes in the report, since it is a frame's worth of copying per
         * frame that the stream lost. */
        if (!first_error[0]) av_strerror(shared, first_error, sizeof first_error);
        snprintf(h->shared_why, sizeof h->shared_why,
                 "reading the capture's own surfaces wasn't possible (%s), so every frame is "
                 "copied to the processor",
                 first_error);
        first_error[0] = 0;
        av_buffer_unref(&h->device);
      }
    }
    if (!h->qsv_frames) {
      ret = av_hwdevice_ctx_create(&h->device, AV_HWDEVICE_TYPE_QSV, NULL, NULL, 0);
      if (ret >= 0) {
        codec->hw_device_ctx = av_buffer_ref(h->device);
        if (!codec->hw_device_ctx) return AVERROR(ENOMEM);
      } else {
        /* Not a reason to stop: the encoder may still open without it, and if
         * it can't, its own error is the one the report wants. */
        first_error[0] = 0;
      }
    }
    ret = set_qsv(h, codec_id);
    if (ret < 0) return ret;
    preset = "medium";
    rate = "VBR";
    h->qsv = 1;
  } else if (amf) {
    /* The capture's own texture where the machine has one, as Quick Sync is
     * offered above: a frame that never reaches memory. The upload below is
     * what a stream with no surfaces, or a device that refused them, gets. */
    if (capture_device) {
      int shared = open_amf_shared(h, codec, capture_device);
      if (shared < 0) {
        if (!first_error[0]) av_strerror(shared, first_error, sizeof first_error);
        snprintf(h->shared_why, sizeof h->shared_why,
                 "reading the capture's own surfaces wasn't possible (%s), so every frame is "
                 "copied to the processor",
                 first_error);
        first_error[0] = 0;
        av_buffer_unref(&h->device);
      }
    }
    /* Or FFmpeg holds each frame's packet back until the next frame. */
    codec->flags |= AV_CODEC_FLAG_LOW_DELAY;
    /* AMF's usage and rate control are the stream's latency and its cap
     * without padding; its balanced quality preset and no filler padding
     * only improve the picture. */
    const struct setting amf_settings[] = {
        {REQUIRED_SETTING, "usage", "lowlatency"},
        {OPTIONAL_SETTING, "quality", "balanced"},
        {REQUIRED_SETTING, "rc", "vbr_peak"},
        {REQUIRED_SETTING, "enforce_hrd", "1"},
        {OPTIONAL_SETTING, "filler_data", "0"},
        {REQUIRED_SETTING, "async_depth", "1"},
        {REQUIRED_SETTING, "forced_idr", "1"},
    };
    ret = set_options(h, amf_settings, sizeof amf_settings / sizeof *amf_settings);
    if (ret < 0) return ret;
    preset = "balanced";
    rate = "vbr_peak";
    /* av1_amf has no `aud`, and asking it for one only fails the open. */
    if (codec_id != CODEC_AV1) {
      ret = set_option(h, REQUIRED_SETTING, "aud", "0");
      if (ret < 0) return ret;
    }
    /* Each codec names its own latency mode (a mode for AV1, a flag for the
     * others) and parameter sets before its keyframes; no B-frames, so a
     * frame comes out for each frame in. */
    if (codec_id == CODEC_AV1) {
      const struct setting av1_settings[] = {
          {REQUIRED_SETTING, "latency", "real_time"},
          {REQUIRED_SETTING, "header_insertion_mode", "frame"},
          {REQUIRED_SETTING, "bf", "0"},
      };
      ret = set_options(h, av1_settings, sizeof av1_settings / sizeof *av1_settings);
    } else {
      ret = set_option(h, REQUIRED_SETTING, "latency", "1");
    }
    if (ret >= 0 && codec_id == CODEC_HEVC)
      ret = set_option(h, REQUIRED_SETTING, "header_insertion_mode", "idr");
    if (ret >= 0 && codec_id == CODEC_H264) {
      /* Main profile is Discord's client's; a driver that won't take it
       * still encodes, so it's only asked for. */
      char spacing[16];
      snprintf(spacing, sizeof spacing, "%d", codec->gop_size);
      const struct setting h264_settings[] = {
          {OPTIONAL_SETTING, "profile", "main"},
          {REQUIRED_SETTING, "bf", "0"},
          {REQUIRED_SETTING, "header_spacing", spacing},
      };
      ret = set_options(h, h264_settings, sizeof h264_settings / sizeof *h264_settings);
    }
    if (ret < 0) return ret;
    h->poll = 1;
  } else {
    /* NVENC is NVIDIA's alone, and it is the one backend here with no device
     * of its own to ask: AMF is given AMD's runtime and VA-API a render node,
     * so each of those names its vendor's absence itself, while an NVENC name
     * on another maker's card fails inside FFmpeg's driver with no card named.
     * So a machine without one is told that before the driver is loaded. */
    if (!nvidia_card()) {
      snprintf(first_error, sizeof first_error,
               "no NVIDIA card on this machine, which is the only kind NVENC encodes on");
      return AVERROR(ENODEV);
    }
    ret = set_nvenc(h, nvenc, codec_id);
    if (ret < 0) return ret;
    preset = nvenc->preset;
    rate = nvenc->rc;
  }
  snprintf(h->tuning, sizeof h->tuning, "%s preset, %s rate control, keyframe every %d frames",
           preset, rate, codec->gop_size);
  ret = avcodec_open2(codec, found, NULL);
  if (ret < 0) return ret;
  /* Rust hands over I420, and the encoders that read their frames from the
   * GPU take NV12, with the chroma interleaved instead. Whichever the
   * encoder took is the one `fill` lays out, so it is read back rather than
   * assumed: a format `fill` can't write would encode the wrong picture at
   * the wrong size. */
  const AVHWFramesContext *surfaces =
      codec->hw_frames_ctx ? (const AVHWFramesContext *)codec->hw_frames_ctx->data : NULL;
  enum AVPixelFormat pixels = surfaces ? surfaces->sw_format : codec->pix_fmt;
  const char *format = av_get_pix_fmt_name(pixels);
  if (pixels != AV_PIX_FMT_NV12 && pixels != AV_PIX_FMT_YUV420P) {
    snprintf(first_error, sizeof first_error, "the encoder takes %s, not I420 or NV12",
             format ? format : "a format with no name");
    return AVERROR(EINVAL);
  }
  /* Frames are timed by the stream's clock in 90 kHz units, rescaled into
   * this time base, which has to exist to rescale into. */
  if (codec->time_base.num <= 0 || codec->time_base.den <= 0) {
    snprintf(first_error, sizeof first_error, "the encoder kept no time base");
    return AVERROR(EINVAL);
  }
  h->frame = av_frame_alloc();
  h->packet = av_packet_alloc();
  h->held = av_packet_alloc();
  if (!h->frame || !h->packet || !h->held) return AVERROR(ENOMEM);
  /* A session reading the capture's own surfaces is handed one of them in
   * `yapfast_hwenc_encode_texture`, which puts the surface in the frame itself:
   * there is no picture in memory here to give a buffer to, and the one
   * `fill` would have written is the frame that path would have uploaded. */
  h->frame->format = h->qsv_frames   ? AV_PIX_FMT_QSV
                     : h->amf_frames ? AV_PIX_FMT_D3D11
                                     : pixels;
  h->frame->width = (int)width;
  h->frame->height = (int)height;
  /* Read back rather than assumed, as the format is: an encoder that kept
   * none of what it was told is reported as claiming no colour at all, so
   * the log never names a colour the stream doesn't have. */
  const char *space = av_color_space_name(codec->colorspace);
  if (codec->colorspace == AVCOL_SPC_UNSPECIFIED) space = NULL;
  const char *range = codec->color_range == AVCOL_RANGE_JPEG   ? "full"
                      : codec->color_range == AVCOL_RANGE_MPEG ? "limited"
                                                              : "no";
  snprintf(h->config, sizeof h->config, "%s, %s %s range, %d/%d, %u kbit/s", format,
           space ? space : "no colour space", range, codec->time_base.num,
           codec->time_base.den, kbps);
  return (h->qsv_frames || h->amf_frames) ? 0 : av_frame_get_buffer(h->frame, 0);
}

/* The encoders to try for each codec, best first, which is this build's own
 * order for a machine whose adapters Rust couldn't list; each system builds in
 * those it has.
 *
 * Quick Sync is named here whether or not this FFmpeg carries it: a build with
 * `--enable-libvpl` and oneVPL beside it (README.yapfast.md) opens Intel's
 * encoders here, and one without says "not in this FFmpeg" for h264_qsv as it
 * does for any encoder it lacks, so a machine with Intel's GPU is told which
 * of the two happened rather than nothing at all.
 *
 * Quick Sync is last behind VA-API rather than in front of it: an Intel GPU on
 * Linux encodes through VA-API today, and this order keeps that machine on the
 * encoder its picture was measured with, with Quick Sync tried after it where
 * no render node opens. On Windows Rust names the encoders it wants in the
 * order its adapters gave them, so this row is what a machine whose adapters
 * couldn't be listed falls back to. */
static const char *const ENCODERS[][5] = {
    [CODEC_H264] = {"h264_nvenc", "h264_amf", "h264_vaapi", "h264_qsv", NULL},
    [CODEC_HEVC] = {"hevc_nvenc", "hevc_amf", "hevc_vaapi", "hevc_qsv", NULL},
    [CODEC_AV1] = {"av1_nvenc", "av1_amf", "av1_vaapi", "av1_qsv", NULL},
};

/* The encoders to try, in the order asked for: Rust names them where it chose
 * an adapter, and this build's own order is used where it named none. Names are
 * copied out of a comma-separated string so each can be asked about, and hold
 * well under `text`'s width, which is a name FFmpeg knows rather than a
 * sentence. */
enum { MAX_ENCODERS = 8 };
struct encoders {
  const char *names[MAX_ENCODERS + 1];
  char text[MAX_ENCODERS][24];
};

/* Fills `e` with the encoders to try: the comma-separated `list`, best first,
 * or this codec's own order when `list` is NULL. Names beyond the room are
 * dropped rather than failed on: they are a mistake on Rust's side, and the
 * encoders that fit are still ones worth opening. */
static void parse_encoders(struct encoders *e, const char *list, unsigned codec_id) {
  size_t count = 0;
  if (!list) {
    for (const char *const *name = ENCODERS[codec_id]; *name && count < MAX_ENCODERS; name++)
      e->names[count++] = *name;
  } else {
    const char *start = list;
    while (*start && count < MAX_ENCODERS) {
      while (*start == ',' || *start == ' ') start++;
      if (!*start) break;
      const char *end = start;
      while (*end && *end != ',') end++;
      size_t length = (size_t)(end - start);
      while (length && start[length - 1] == ' ') length--;
      if (length >= sizeof e->text[count]) length = sizeof e->text[count] - 1;
      memcpy(e->text[count], start, length);
      e->text[count][length] = 0;
      e->names[count] = e->text[count];
      count++;
      start = *end ? end + 1 : end;
    }
  }
  e->names[count] = NULL;
}

#ifdef __linux__
/* The libva drivers for each kernel driver whose GPUs encode, best first.
 * Naming the driver overrides LIBVA_DRIVER_NAME, which NixOS and others
 * set to nvidia for browsers, and which would fail every other GPU. NVIDIA's
 * own VA-API driver only decodes; NVENC encodes there. */
static const char *const VA_DRIVERS[][4] = {
    {"amdgpu", "radeonsi", NULL},
    {"radeon", "r600", NULL},
    {"i915", "iHD", "i965"},
    {"xe", "iHD", NULL},
    {"virtio_gpu", "virtio_gpu", NULL},
};

/* The kernel driver of /dev/dri/renderD<128 + n>, or an empty string. It is
 * sysfs that says which driver a node has, and sysfs is Linux's, so this is
 * Linux's alone: off Linux there are no VA-API devices to find. */
static void kernel_driver(int n, char *name, size_t size) {
  name[0] = 0;
  char link[64], target[256];
  snprintf(link, sizeof link, "/sys/class/drm/renderD%d/device/driver", 128 + n);
  ssize_t length = readlink(link, target, sizeof target - 1);
  if (length <= 0) return;
  target[length] = 0;
  const char *base = strrchr(target, '/');
  base = base ? base + 1 : target;
    if (strlen(base) < size) memcpy(name, base, strlen(base) + 1);
}

/* The vendor id of the GPU behind render node `n`, from sysfs: 0x10DE,
 * 0x1002 or 0x8086 for the three makers whose GPUs encode, and 0 when it can't
 * be read, which `preferred` below matches with no preference: a node whose
 * vendor isn't known is tried after the ones the stream asked for, not first
 * and not never. */
static unsigned node_vendor(int n) {
  char path[64], text[16];
  snprintf(path, sizeof path, "/sys/class/drm/renderD%d/device/vendor", 128 + n);
  FILE *file = fopen(path, "r");
  if (!file) return 0;
  if (!fgets(text, sizeof text, file)) text[0] = 0;
  fclose(file);
  return (unsigned)strtoul(text, NULL, 16);
}

/* Whether a GPU of `vendor` is the kind `preference` asks for, which is
 * gpu::Preference's rule on this side of the call: the card with memory of its
 * own, or the GPU built into the processor. A word this doesn't know, or none
 * at all, matches every GPU, so nothing is lost where the setting isn't
 * passed. */
static int preferred(const char *preference, unsigned vendor) {
  if (!preference || !preference[0]) return 1;
  int card = vendor == 0x10DE || vendor == 0x1002, die = vendor == 0x8086;
  if (!strcmp(preference, "dedicated")) return card;
  if (!strcmp(preference, "integrated")) return die;
  return 1;
}

/* Whether this machine has a GPU of `vendor` at all, read off the render nodes
 * it has. An encoder whose maker isn't here has no card to open: asking anyway
 * loads that maker's runtime to be told so, and NVIDIA's CUDA library writes
 * "cannot load libcuda.so.1" to the log when it is looked for on an AMD or
 * Intel machine. */
static int gpu_of_vendor(unsigned vendor) {
  for (int n = 0; n < 8; n++)
    if (node_vendor(n) == vendor) return 1;
  return 0;
}

/* Why an encoder is one this machine hasn't the GPU for, or NULL when it may be
 * tried: NVIDIA's NVENC and Intel's Quick Sync each need their maker's card,
 * and asking for one that isn't here loads its runtime to be told so. */
static const char *missing_gpu(const char *name) {
  if (strstr(name, "_nvenc") && !gpu_of_vendor(0x10DE)) return "no NVIDIA GPU here";
  if (strstr(name, "_qsv") && !gpu_of_vendor(0x8086)) return "no Intel GPU here";
  return NULL;
}
#else
/* Every encoder is that maker's on a platform with no render nodes to read: the
 * choice is the hardware it runs on, and it says for itself what it hasn't got. */
static const char *missing_gpu(const char *name) {
  (void)name;
  return NULL;
}
#endif

struct opening {
  unsigned codec_id, width, height, fps, kbps;
  /* The NVENC tuning each encoder is offered, which is the same for all of
   * them: it says how hard the one that opens should work, not which one to
   * try. */
  const struct nvenc_tuning *nvenc;
  /* "dedicated" or "integrated", the kind of GPU the stream asked for: the
   * render nodes VA-API chooses between itself are tried in that order first.
   * NULL, or a word this doesn't know, prefers every GPU alike. */
  const char *preference;
  /* The encoders to try, best first: Rust's order where it chose an adapter,
   * and this build's own where it named none. */
  struct encoders encoders;
  /* The device a capture's surfaces live on, which only Quick Sync's shared
   * path reads: an ID3D11Device on Windows, and NULL where the stream has no
   * surfaces of its own to hand over. */
  void *device;
  char *report;
  size_t report_size, used;
};

/* Adds one encoder's reason for not opening to the report, and nothing at all
 * once the report is full: the buffer is Rust's, of a fixed size, and what
 * fits in it is what the log can say. */
static void skipped(struct opening *o, const char *name, const char *why) {
  if (o->used + 1 >= o->report_size) return;
  int written = snprintf(o->report + o->used, o->report_size - o->used, "%s%s: %s",
                         o->used ? "; " : "", name, why);
  if (written < 0) return;
  o->used += (size_t)written;
  if (o->used >= o->report_size) o->used = o->report_size - 1;
}

/* Opens `found` on `device`, or adds why it didn't open to the report. */
static struct yapfast_hwenc *try_open(struct opening *o, const AVCodec *found,
                                      struct va_device device) {
  const char *name = found->name;
  char label[96];
  if (device.path)
    snprintf(label, sizeof label, "%s (%s, %s)", name, device.path + strlen("/dev/dri/"),
             device.driver);
  else
    snprintf(label, sizeof label, "%s", name);
  struct yapfast_hwenc *h = av_mallocz(sizeof *h);
  if (!h) return NULL;
  first_error[0] = 0;
  int ret = open_encoder(h, found, device, o->device, o->nvenc, o->codec_id, o->width, o->height,
                         o->fps, o->kbps);
  if (ret >= 0) {
    h->engine = name;
    /* A stream that asked for the capture's own surfaces and didn't get them
     * says so beside the encoder that did open: the picture is the upload's,
     * and why it isn't the other one is what a reader of the log is after. */
    if (h->shared_why[0])
      snprintf(o->report, o->report_size, "%s; %s", label, h->shared_why);
    else
      snprintf(o->report, o->report_size, "%s", label);
    return h;
  }
  if (!first_error[0]) av_strerror(ret, first_error, sizeof first_error);
  skipped(o, label, first_error);
  close_encoder(h);
  av_free(h);
  return NULL;
}

/* What Rust asks for, beyond the picture: the frame's shape and rate, the
 * bitrate to aim at, and the choice the adapter preference makes. */
struct yapfast_hwenc_open {
  unsigned codec_id, width, height, fps, kbps;
  /* The NVENC tuning asked for by name, or NULL for the shipped one. */
  const char *tuning;
  /* FFmpeg's encoders to try, best first and separated by commas, which is how
   * Rust puts the adapter it chose first; NULL for this build's own order,
   * which is what a platform that can't list its adapters is given. */
  const char *encoders;
  /* "dedicated" or "integrated": the kind of GPU the stream asked for, for the
   * render nodes VA-API chooses between itself; NULL for any. */
  const char *preference;
  /* The device a capture's surfaces live on, which only Quick Sync's shared
   * path reads: an ID3D11Device on Windows, and NULL where the stream has no
   * surfaces of its own to hand over. Borrowed for as long as the encoder
   * lives, and referenced while it does. */
  void *device;
};

/* Opens the first encoder of the request's codec that works, in the order the
 * request asks for them — Rust's, when it chose an adapter, and this build's
 * own otherwise — VA-API's on each GPU that has a driver for it, and writes the
 * one that opened to `report`, or each one's failure when none does. Every
 * encoder the list names is accounted for, including the ones that were passed
 * over before they could be tried: a stream that fell back to x264 says which
 * encoders this machine hasn't got, as well as which it has and wouldn't
 * open. */
struct yapfast_hwenc *yapfast_hwenc_new(const struct yapfast_hwenc_open *ask, char *report,
                                        size_t report_size) {
  if (report_size) report[0] = 0;
  if (!ask || ask->codec_id > CODEC_AV1) return NULL;
  unsigned codec_id = ask->codec_id;
  /* The tuning Rust named, before any encoder is tried: a name from outside
   * this set says the two sides disagree, which is worth refusing rather
   * than encoding with the tuning nobody asked for. */
  const struct nvenc_tuning *nvenc = nvenc_tuning(ask->tuning);
  if (!nvenc) {
    if (report_size) snprintf(report, report_size, "no tuning called %s", ask->tuning);
    return NULL;
  }
  av_log_set_level(AV_LOG_ERROR);
  av_log_set_callback(keep_error);
  struct opening o = {.codec_id = codec_id,
                      .width = ask->width,
                      .height = ask->height,
                      .fps = ask->fps,
                      .kbps = ask->kbps,
                      .nvenc = nvenc,
                      .preference = ask->preference,
                      .device = ask->device,
                      .report = report,
                      .report_size = report_size};
  parse_encoders(&o.encoders, ask->encoders, codec_id);
  for (const char *const *name = o.encoders.names; *name; name++) {
    const AVCodec *found = avcodec_find_encoder_by_name(*name);
    if (!found) {
      /* An encoder this FFmpeg wasn't built with. It is named anyway, so a
       * machine with that GPU is told it hasn't got it rather than nothing
       * at all. */
      skipped(&o, *name, "not in this FFmpeg");
      continue;
    }
    struct yapfast_hwenc *h = NULL;
    if (!strstr(*name, "_vaapi")) {
      const char *missing = missing_gpu(*name);
      if (missing) {
        skipped(&o, *name, missing);
        continue;
      }
      h = try_open(&o, found, (struct va_device){NULL, NULL});
      if (h) return h;
      continue;
    }
#ifdef __linux__
    /* Each GPU's render node, and the libva driver that encodes on it: the kind
     * of GPU the stream asked for first, since a preference is which GPU
     * encodes and not whether one does. Without a preference every GPU is that
     * kind, so the second pass finds nothing to try. */
    int tried = 0;
    for (int pass = 0; pass < 2; pass++) {
      for (int n = 0; n < 8; n++) {
        char kernel[32], path[32];
        kernel_driver(n, kernel, sizeof kernel);
        if (!kernel[0]) continue;
        if (preferred(o.preference, node_vendor(n)) != (pass == 0)) continue;
        snprintf(path, sizeof path, "/dev/dri/renderD%d", 128 + n);
        for (size_t i = 0; i < sizeof VA_DRIVERS / sizeof *VA_DRIVERS; i++) {
          if (strcmp(kernel, VA_DRIVERS[i][0])) continue;
          for (const char *const *driver = VA_DRIVERS[i] + 1; *driver; driver++) {
            tried++;
            h = try_open(&o, found, (struct va_device){path, *driver});
            if (h) return h;
          }
        }
      }
    }
    /* A GPU nothing here drives is one this can't encode on: an Intel GPU
     * with no iHD, or a card whose kernel driver has no libva driver named
     * for it. It goes in the report rather than unmentioned. */
    if (!tried) skipped(&o, *name, "no GPU here with a VA-API driver");
#else
    /* VA-API is Linux's, and so are the render nodes it would be given. */
    skipped(&o, *name, "VA-API is Linux's");
#endif
  }
  return NULL;
}

/* Copies an I420 picture into the frame the encoder reads: as it is for
 * NVENC, with its chroma planes interleaved for VA-API's, AMF's and Quick
 * Sync's NV12. `open_encoder` checked the frame's format, so the other layout
 * is I420 rather than whatever the driver felt like. */
static int fill(struct yapfast_hwenc *h, const uint8_t *y, int y_stride, const uint8_t *u,
                int u_stride, const uint8_t *v, int v_stride) {
  AVFrame *frame = h->frame;
  int ret = av_frame_make_writable(frame);
  if (ret < 0) return ret;
  int width = frame->width, height = frame->height;
  for (int row = 0; row < height; row++)
    memcpy(frame->data[0] + row * frame->linesize[0], y + row * y_stride, (size_t)width);
  for (int row = 0; row < height / 2; row++) {
    const uint8_t *u_row = u + row * u_stride, *v_row = v + row * v_stride;
    if (frame->format == AV_PIX_FMT_NV12) {
      uint8_t *uv = frame->data[1] + row * frame->linesize[1];
      for (int x = 0; x < width / 2; x++) {
        uv[2 * x] = u_row[x];
        uv[2 * x + 1] = v_row[x];
      }
    } else {
      memcpy(frame->data[1] + row * frame->linesize[1], u_row, (size_t)width / 2);
      memcpy(frame->data[2] + row * frame->linesize[2], v_row, (size_t)width / 2);
    }
  }
  return 0;
}

/* Hands `frame`, timed `at`, to the encoder, and keeps the access unit it
 * produced for `yapfast_hwenc_drain` to copy into the caller's buffer. Both
 * encodes go through this, so a frame from memory and a frame that never left
 * the card are timed, made into keyframes and told apart from a driver's late
 * packet the same way. Returns the size produced, 0 when the encoder gave no
 * frame, or -1 on failure. */
static long encode_frame(struct yapfast_hwenc *h, AVFrame *frame, int64_t at, int keyframe,
                         int *is_key) {
  int ret = 0;
  frame->pts = at;
  /* A dropped picture is one the frames after it are built on, so the next
   * frame starts a new sequence: asked for as a keyframe, a viewer has a
   * picture to decode again instead of waiting out the keyframe interval on
   * frames whose reference it never received. */
  frame->pict_type =
      keyframe || h->recover ? AV_PICTURE_TYPE_I : AV_PICTURE_TYPE_NONE;
  h->recover = 0;
  if (avcodec_send_frame(h->codec, frame) < 0) return -1;
  size_t size = 0;
  /* Whatever the last call left is this frame's to replace, copied out or
   * not: one frame is held at a time. */
  h->staged_size = 0;
  if (h->holding) av_packet_unref(h->held);
  h->holding = 0;
  *is_key = 0;
  /* AMF waits a millisecond at most for each receive: polled for the
   * frame's packet for up to 100 ms, after which it comes with the next. */
  int64_t deadline = h->poll ? av_gettime_relative() + 100000 : 0;
  for (;;) {
    ret = avcodec_receive_packet(h->codec, h->packet);
    if (ret == AVERROR(EAGAIN) && !size && av_gettime_relative() < deadline) {
      /* Older drivers don't wait in AMF's receive at all. */
      av_usleep(250);
      continue;
    }
    if (ret < 0) break;
    /* The encoder takes one frame and gives back one packet for it, timed
     * as the frame was. A packet timed for another is one a driver held
     * back: its frame is already gone and its time with it, so sending it
     * now would pass it off as this frame's, and it's dropped instead. */
    if (h->packet->pts != AV_NOPTS_VALUE && h->packet->pts != at) {
      h->late++;
      h->recover = 1;
      av_packet_unref(h->packet);
      continue;
    }
    const size_t length = (size_t)h->packet->size;
    *is_key |= (h->packet->flags & AV_PKT_FLAG_KEY) != 0;
    if (!size) {
      /* The frame's packet, held where FFmpeg made it. */
      av_packet_move_ref(h->held, h->packet);
      h->holding = 1;
    } else {
      /* A second packet for one frame: the first joins it in `staged`. */
      if (h->holding) {
        if (!stage(h, 0, h->held->data, (size_t)h->held->size)) {
          av_packet_unref(h->packet);
          return -1;
        }
        av_packet_unref(h->held);
        h->holding = 0;
      }
      if (!stage(h, size, h->packet->data, length)) {
        av_packet_unref(h->packet);
        return -1;
      }
      av_packet_unref(h->packet);
    }
    size += length;
  }
  if (ret != AVERROR(EAGAIN)) return -1;
  h->staged_size = h->holding ? 0 : size;
  return (long)size;
}

/* Encodes one I420 frame whose time is `pts` in 90 kHz units: the stream's
 * own clock, which Rust also labels the frame's packets with. The encoder is
 * given the same time rescaled into its time base, so a packet can be told
 * apart from another frame's, and an encoder opened again mid-stream carries
 * on from the stream's time rather than from a count of its own. What the
 * frame produced is kept for `yapfast_hwenc_drain` to copy into the caller's
 * buffer: the size it reports may be larger than that buffer, which grows to
 * hold it without the frame being encoded a second time. Returns the size
 * produced, 0 when the encoder gave no frame, or a negative number on
 * failure. `duration` is unused: frames are timed by the frame rate. A session
 * reading the capture's own surfaces has no picture in memory to give this, so
 * its frames go through `yapfast_hwenc_encode_texture` instead. */
long yapfast_hwenc_encode(struct yapfast_hwenc *h, const unsigned char *y, int y_stride,
                          const unsigned char *u, int u_stride, const unsigned char *v,
                          int v_stride, long long pts, unsigned long duration, int keyframe,
                          int *is_key) {
  (void)duration;
  /* A frame from the card can't be filled from the processor, and pretending
   * it can would write through pointers this path never made. */
  if (h->qsv_frames || h->amf_frames) return -1;
  int64_t at = av_rescale_q((int64_t)pts, (AVRational){1, 90000}, h->codec->time_base);
  int ret = fill(h, y, y_stride, u, u_stride, v, v_stride);
  if (ret < 0) return -1;
  AVFrame *frame = h->frame;
  if (h->surface) {
    av_frame_unref(h->surface);
    ret = av_hwframe_get_buffer(h->codec->hw_frames_ctx, h->surface, 0);
    if (ret >= 0) ret = av_hwframe_transfer_data(h->surface, h->frame, 0);
    if (ret < 0) return -1;
    frame = h->surface;
  }
  return encode_frame(h, frame, at, keyframe, is_key);
}

/* Encodes one surface the caller already holds, at `pts` in 90 kHz units: a
 * Direct3D 11 texture on the card, and the subresource inside it. Nothing of
 * the picture reaches the processor: the texture is described as a Quick Sync
 * surface, the encoder is handed that surface, and the driver reads the texture
 * where it lies. `is_key` is what the frame produced, as above. Returns the
 * size produced, 0 when the encoder gave no frame, or -1 on failure, with the
 * reason in `yapfast_hwenc_error` when the surface wasn't one it could read. */
long yapfast_hwenc_encode_texture(struct yapfast_hwenc *h, void *texture, unsigned subresource,
                                  long long pts, int keyframe, int *is_key) {
  if (!h || !texture || !is_key) return -1;
  h->error[0] = 0;
  if (h->amf_frames) {
    /* The frame names the caller's texture and the subresource inside it; FFmpeg's
     * AMF encoder wraps it in an AMFTexture itself, so nothing is copied and no
     * surface of FFmpeg's is involved. */
    int64_t at = av_rescale_q((int64_t)pts, (AVRational){1, 90000}, h->codec->time_base);
    AVFrame *frame = h->frame;
    av_frame_unref(frame);
    frame->format = AV_PIX_FMT_D3D11;
    frame->width = h->codec->width;
    frame->height = h->codec->height;
    frame->hw_frames_ctx = av_buffer_ref(h->amf_frames);
    if (!frame->hw_frames_ctx) return -1;
    frame->data[0] = texture;
    frame->data[1] = (uint8_t *)(uintptr_t)subresource;
    return encode_frame(h, frame, at, keyframe, is_key);
  }
  if (!h->qsv_frames) return -1;
  struct qsv_slot *slot = qsv_surface(h, texture, subresource);
  if (!slot) return -1;
  int64_t at = av_rescale_q((int64_t)pts, (AVRational){1, 90000}, h->codec->time_base);
  AVFrame *frame = h->frame;
  av_frame_unref(frame);
  frame->format = AV_PIX_FMT_QSV;
  frame->width = h->codec->width;
  frame->height = h->codec->height;
  /* FFmpeg's encoder copies the surface out of the frame it is given, and
   * keeps the frame — and so this reference — until the surface is free again:
   * which is what keeps the texture and its pairing alive for as long as the
   * driver is reading them. */
  frame->buf[0] = av_buffer_ref(slot->ref);
  if (!frame->buf[0]) return -1;
  frame->data[3] = (uint8_t *)slot->surface;
  return encode_frame(h, frame, at, keyframe, is_key);
}

/* Copies the access unit the last encode produced into `out`, which holds it,
 * and gives up the copy: the buffer stays for the next frame. Rust grows its
 * buffer to the size the encode reported before calling this once, so a frame
 * is never encoded twice to fit one. Returns the size copied, or -1 when there
 * is nothing to copy or `out` is too small. */
long yapfast_hwenc_drain(struct yapfast_hwenc *h, unsigned char *out, size_t capacity) {
  if (h->holding) {
    const size_t size = (size_t)h->held->size;
    if (!size || size > capacity) return -1;
    memcpy(out, h->held->data, size);
    av_packet_unref(h->held);
    h->holding = 0;
    return (long)size;
  }
  size_t size = h->staged_size;
  if (!size || size > capacity) return -1;
  memcpy(out, h->staged, size);
  h->staged_size = 0;
  return (long)size;
}

/* The memory this encoder keeps for gathering a frame that came out as several
 * packets, reused for the next one: one frame's worth at most, and none while
 * every frame is one packet, which is what a measurement of a stream's memory
 * holds it to. */
size_t yapfast_hwenc_staging(struct yapfast_hwenc *h) {
  return h->staged_capacity;
}

/* How many packets the encoder handed back for an earlier frame than the one
 * it was given, and which were dropped rather than sent under a later
 * frame's time: a driver that falls behind, worth warning about. */
unsigned yapfast_hwenc_late(struct yapfast_hwenc *h) { return h->late; }

/* The format, colour and time base this encoder took, and the rate asked
 * for, into `config`: what Rust can't see, since it hands over I420 at a
 * rate and the encoder decides the rest. */
void yapfast_hwenc_config(struct yapfast_hwenc *h, char *config, size_t size) {
  if (size) snprintf(config, size, "%s", h->config);
}

/* FFmpeg's own name for the encoder that opened, such as "h264_nvenc", which
 * is what Rust matches its choice of backend back to: the report beside it and
 * `yapfast_hwenc_config` name VA-API's device as well. */
void yapfast_hwenc_engine(struct yapfast_hwenc *h, char *name, size_t size) {
  if (size) snprintf(name, size, "%s", h->engine ? h->engine : "");
}

/* Whether this FFmpeg was built with the encoder `name`, which is how Rust
 * learns what the shipped build has rather than what its configure line was
 * meant to enable: an encoder the build hasn't got is one no stream can offer,
 * and one it has may still fail to open on a machine. */
int yapfast_hwenc_has(const char *name) { return avcodec_find_encoder_by_name(name) != NULL; }

/* Whether this machine has the card NVENC encodes on, which FFmpeg's own CUDA
 * device answers once: what a Linux share asks before it opens one of FFmpeg's
 * NVENC sessions to find out, the picture's own path on that card being the one
 * it wants. */
int yapfast_hwenc_nvidia(void) { return nvidia_card(); }

/* Changes the bitrate aimed for, from the next frame on, without a
 * keyframe: patches/ in vendor/ffmpeg-src make NVENC, AMF and VA-API follow
 * it, and FFmpeg's own Quick Sync applies it to its running session. Returns
 * 0: the context's aim is what is set here, and the driver's answer comes
 * with the frames after, as FFmpeg's own error. Quick Sync's cap and target
 * are the pair it chose its rate control from at the open, so a new rate is
 * made into the same pair rather than a target level with the cap. */
int yapfast_hwenc_set_bitrate(struct yapfast_hwenc *h, unsigned kbps) {
  if (h->qsv)
    set_qsv_rate(h->codec, kbps);
  else
    set_rate(h->codec, kbps);
  return 0;
}

/* Whether this session reads the capture's own surfaces rather than an upload:
 * 1 when it does, and 0 for every other session. Rust asks before it decides
 * how a frame reaches the encoder, since the two encodes are not
 * interchangeable, and logs it beside the encoder's name. */
int yapfast_hwenc_shared(struct yapfast_hwenc *h) {
  return h->qsv_frames != NULL || h->amf_frames != NULL;
}

/* Why the last frame wasn't encoded, when one wasn't, into `why`: empty when
 * the frame before it was. The surface handed over is the only thing that
 * fails here for a reason Rust can act on — a capture of another size, or of a
 * format this session doesn't read — so it says which, rather than leaving a
 * stream to lose every frame silently. */
void yapfast_hwenc_error(struct yapfast_hwenc *h, char *why, size_t size) {
  if (size) snprintf(why, size, "%s", h->error);
}

/* The optional settings this encoder's FFmpeg didn't have or its driver
 * didn't keep, by name and separated by commas, into `notes`: empty when it
 * kept them all. */
void yapfast_hwenc_notes(struct yapfast_hwenc *h, char *notes, size_t size) {
  if (size) snprintf(notes, size, "%s", h->notes);
}

/* The settings this encoder was opened with, as far as FFmpeg will say: how
 * hard it works per frame, how its cap is spent, and how often a viewer waits
 * for a keyframe. Rust logs it once, at the open, and a measurement quotes it
 * beside its numbers, so a picture is compared with the encoder that made it
 * rather than with whatever tuning the reader assumed. */
void yapfast_hwenc_tuning(struct yapfast_hwenc *h, char *tuning, size_t size) {
  if (size) snprintf(tuning, size, "%s", h->tuning);
}

/* The settings the tuning named `name` opens NVENC with, and whether the
 * stream can't do without each, into `asked`; the first row's when `name` is
 * NULL. It is the same list `set_nvenc` opens with, so Rust holds its shipped
 * tuning to what the stream actually sends, and does it without a GPU at hand:
 * a tuning that changed without a measurement behind it is a failing test
 * rather than a picture nobody compared. Returns 0, or -1 for a name no row
 * has. */
int yapfast_hwenc_nvenc_settings(const char *name, char *asked, size_t size) {
  const struct nvenc_tuning *tuning = nvenc_tuning(name);
  if (!tuning) return -1;
  if (!size) return 0;
  struct setting settings[8];
  size_t count = nvenc_settings(tuning, settings), used = 0;
  asked[0] = 0;
  for (size_t i = 0; i < count; i++) {
    int written = snprintf(asked + used, size - used, "%s%s=%s%s", used ? " " : "",
                           settings[i].name, settings[i].value,
                           settings[i].worth == OPTIONAL_SETTING ? "(optional)" : "(required)");
    if (written < 0) break;
    used += (size_t)written;
    if (used >= size) {
      used = size - 1;
      break;
    }
  }
  return 0;
}

void yapfast_hwenc_free(struct yapfast_hwenc *h) {
  close_encoder(h);
  av_free(h);
}
