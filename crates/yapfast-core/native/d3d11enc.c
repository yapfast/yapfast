/* Zero-copy encoding for Go Live: NVENC reads a D3D11 texture the caller
 * owns, so a captured frame reaches the encoder without its pixels crossing
 * the processor.
 *
 * native/hwenc.c hands its encoder I420 in memory, which means every frame is
 * converted and uploaded before NVENC sees it: on a 1080p screen that is the
 * difference between about 11 ms of the processor per frame and about 0.5, and
 * it is why a stream and the game beside it compete for the same cores.
 * Nothing here changes hwenc.c's path: a caller that wants FFmpeg's encoders, or
 * one on the processor, still opens it as it always did.
 *
 * The one copy left is the bitstream: NVENC's output is mapped, moved into the
 * caller's buffer once per frame and unmapped, because the driver's buffer
 * belongs to the driver and the caller's packet has to outlive the lock. The
 * picture itself is never touched, and `yapfast_d3d11enc_copied` answers zero
 * for as long as that stays true, so a change that reintroduces a per-frame
 * upload says so from the encoder rather than from reading this file.
 *
 * What makes it fast is also what makes it delicate: the texture has to live on
 * the same device the session was opened on, in a concrete format (the driver
 * reads the real DXGI format and refuses a typeless texture), so
 * `yapfast_d3d11enc_device` hands the device out for a capture source to create
 * its surfaces on.
 *
 * The settings are native/hwenc.c's `NVENC_TUNINGS` row for the tuning asked
 * for, said again here as NVENC's own parameters rather than as FFmpeg's
 * options: low-latency tuning, no B-frames, a frame out for each frame in, an
 * IDR on demand, and capped VBR over half a second's buffer. A test holds the
 * two tables to one answer (`yapfast_d3d11enc_tuning_settings`). */
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <d3d11.h>
#include <d3dcompiler.h>
#include <dxgi1_2.h>
#include <stdarg.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* The vendored ffnvcodec header, which FFmpeg's NVENC encoder uses too: only
 * declarations, since every entry point is resolved from the driver's own DLL,
 * which is how the binary starts on a machine with no NVIDIA driver at all.
 * Relative to this file, so that the build and an editor's language server
 * resolve the same header without an include path either of them has to know. */
#include "../../../vendor/ffmpeg-src/include/ffnvcodec/nvEncodeAPI.h"

/* The codecs, as Rust numbers them. */
enum { CODEC_H264, CODEC_HEVC, CODEC_AV1, CODEC_COUNT };

/* How many textures are remembered, by (texture, subresource): a capture source
 * hands back a few surfaces in rotation rather than one, and registering and
 * mapping each of them again on every frame would cost more of the processor
 * than the encode does. Four covers the ring a desktop duplicator keeps. */
#define SLOTS 4

/* Enough for a keyframe of a 1080p screen at the stream's cap, and drawn from
 * the driver's own heap: a bitstream buffer that is too small fails the encode,
 * and a keyframe of noise at 8 Mbit/s is under this. */
#define BITSTREAM_BYTES (4u << 20)

/* The tuning the stream asks a GPU's encoder for, in NVENC's terms. One row per
 * tuning, the same rows native/hwenc.c names, so what the stream asks for reads
 * the same whichever side encodes it.
 *
 * `preset` is how much work NVENC does per frame, `p4` where screen recordings
 * and games landed; `tune` is the low-latency one, since the picture is one
 * somebody is waiting for; `rc` is how the cap is spent, `vbr` leaving a calm
 * screen below it rather than padding it out with bits nobody looks at;
 * `zerolatency` and `delay` decide whether NVENC may hold a frame back, and no
 * row does, because a held frame is delay in every viewer's picture;
 * `forced_idr` is what gives a viewer who joins, or whose picture broke, a
 * keyframe at once instead of waiting out the interval. */
struct nvenc_tuning {
  const char *name, *preset, *rc;
  const GUID *preset_guid;
  NV_ENC_TUNING_INFO tune;
  int zerolatency, delay, forced_idr;
};

static const struct nvenc_tuning NVENC_TUNINGS[] = {
    {"p4", "p4", "vbr", &NV_ENC_PRESET_P4_GUID, NV_ENC_TUNING_INFO_ULTRA_LOW_LATENCY, 1, 0, 1},
};

/* The tuning Rust asked for by name, or the first row when it named none. A
 * name no row has is NULL: Rust names one from this set, so an unknown one is
 * the two sides disagreeing, which is worth refusing rather than encoding
 * something nobody chose. */
static const struct nvenc_tuning *nvenc_tuning(const char *name) {
  for (size_t i = 0; i < sizeof NVENC_TUNINGS / sizeof *NVENC_TUNINGS; i++)
    if (!name || !strcmp(name, NVENC_TUNINGS[i].name)) return &NVENC_TUNINGS[i];
  return NULL;
}

/* The settings `tuning` opens NVENC with, whether the stream can't do without
 * each, and the order they are applied: the same list, in the same words, as
 * native/hwenc.c's `nvenc_settings`, so a test can hold the two backends to one
 * answer without a GPU in the shape of the question. */
static void tuning_settings(const struct nvenc_tuning *tuning, char *asked, size_t size) {
  snprintf(asked, size,
           "preset=%s(optional) tune=ll(required) rc=%s(optional) zerolatency=%d(optional) "
           "delay=%d(required) forced-idr=%d(required)",
           tuning->preset, tuning->rc, tuning->zerolatency, tuning->delay, tuning->forced_idr);
}

/* The codecs, in the words Rust and the log use. */
struct codec_row {
  const GUID *guid;
  const char *name, *short_name;
};

static const struct codec_row CODECS[CODEC_COUNT] = {
    [CODEC_H264] = {&NV_ENC_CODEC_H264_GUID, "H.264", "h264"},
    [CODEC_HEVC] = {&NV_ENC_CODEC_HEVC_GUID, "HEVC", "hevc"},
    [CODEC_AV1] = {&NV_ENC_CODEC_AV1_GUID, "AV1", "av1"},
};

/* Why a driver refused, in the driver's own words: a status number alone sends
 * whoever reads the log to a header. */
static const char *nvenc_status(NVENCSTATUS status) {
  switch (status) {
  case NV_ENC_SUCCESS: return "NV_ENC_SUCCESS";
  case NV_ENC_ERR_NO_ENCODE_DEVICE: return "NV_ENC_ERR_NO_ENCODE_DEVICE";
  case NV_ENC_ERR_UNSUPPORTED_DEVICE: return "NV_ENC_ERR_UNSUPPORTED_DEVICE";
  case NV_ENC_ERR_INVALID_ENCODERDEVICE: return "NV_ENC_ERR_INVALID_ENCODERDEVICE";
  case NV_ENC_ERR_INVALID_DEVICE: return "NV_ENC_ERR_INVALID_DEVICE";
  case NV_ENC_ERR_DEVICE_NOT_EXIST: return "NV_ENC_ERR_DEVICE_NOT_EXIST";
  case NV_ENC_ERR_INVALID_PTR: return "NV_ENC_ERR_INVALID_PTR";
  case NV_ENC_ERR_INVALID_EVENT: return "NV_ENC_ERR_INVALID_EVENT";
  case NV_ENC_ERR_INVALID_PARAM: return "NV_ENC_ERR_INVALID_PARAM";
  case NV_ENC_ERR_INVALID_CALL: return "NV_ENC_ERR_INVALID_CALL";
  case NV_ENC_ERR_OUT_OF_MEMORY: return "NV_ENC_ERR_OUT_OF_MEMORY";
  case NV_ENC_ERR_ENCODER_NOT_INITIALIZED: return "NV_ENC_ERR_ENCODER_NOT_INITIALIZED";
  case NV_ENC_ERR_UNSUPPORTED_PARAM: return "NV_ENC_ERR_UNSUPPORTED_PARAM";
  case NV_ENC_ERR_LOCK_BUSY: return "NV_ENC_ERR_LOCK_BUSY";
  case NV_ENC_ERR_NOT_ENOUGH_BUFFER: return "NV_ENC_ERR_NOT_ENOUGH_BUFFER";
  case NV_ENC_ERR_INVALID_VERSION: return "NV_ENC_ERR_INVALID_VERSION";
  case NV_ENC_ERR_MAP_FAILED: return "NV_ENC_ERR_MAP_FAILED";
  case NV_ENC_ERR_NEED_MORE_INPUT: return "NV_ENC_ERR_NEED_MORE_INPUT";
  case NV_ENC_ERR_ENCODER_BUSY: return "NV_ENC_ERR_ENCODER_BUSY";
  case NV_ENC_ERR_EVENT_NOT_REGISTERD: return "NV_ENC_ERR_EVENT_NOT_REGISTERD";
  case NV_ENC_ERR_GENERIC: return "NV_ENC_ERR_GENERIC";
  case NV_ENC_ERR_INCOMPATIBLE_CLIENT_KEY: return "NV_ENC_ERR_INCOMPATIBLE_CLIENT_KEY";
  case NV_ENC_ERR_UNIMPLEMENTED: return "NV_ENC_ERR_UNIMPLEMENTED";
  case NV_ENC_ERR_RESOURCE_REGISTER_FAILED: return "NV_ENC_ERR_RESOURCE_REGISTER_FAILED";
  case NV_ENC_ERR_RESOURCE_NOT_REGISTERED: return "NV_ENC_ERR_RESOURCE_NOT_REGISTERED";
  case NV_ENC_ERR_RESOURCE_NOT_MAPPED: return "NV_ENC_ERR_RESOURCE_NOT_MAPPED";
  default: return "an NVENC status this doesn't know";
  }
}

/* One of the driver's own errors, as a sentence naming what failed, the status
 * and the number: the number alone is what a log gets when nobody thought about
 * who reads it. */
static void status_failure(char *error, size_t size, const char *what, NVENCSTATUS status) {
  if (size) snprintf(error, size, "%s failed: %s (%d)", what, nvenc_status(status), (int)status);
}

/* The processor time this process has used, in 100 ns units, for timing an
 * encode from either side of the call: a wall clock would count the driver
 * waiting on the GPU as if the processor had done it. */
unsigned long long yapfast_d3d11enc_cpu_time(void) {
  FILETIME created, exited, kernel, user;
  ULARGE_INTEGER k, u;
  if (!GetProcessTimes(GetCurrentProcess(), &created, &exited, &kernel, &user)) return 0;
  k.LowPart = kernel.dwLowDateTime;
  k.HighPart = kernel.dwHighDateTime;
  u.LowPart = user.dwLowDateTime;
  u.HighPart = user.dwHighDateTime;
  return k.QuadPart + u.QuadPart;
}

/* How many D3D11 devices this process has created, so that "the next frame
 * reused the session" is a number from the encoder rather than a hope about the
 * code. Counted atomically, since two encoders can open on two threads. */
static LONG devices_created;
unsigned yapfast_d3d11enc_devices_created(void) {
  return (unsigned)InterlockedCompareExchange(&devices_created, 0, 0);
}

/* --------------------------------------------------------------- the driver */

/* The driver's entry points, resolved once for the process: the DLL handle and
 * the function table are the same for every session, and loading them per
 * encoder would be a race between two encoders opening at once rather than
 * anything useful. */
struct nvenc_library {
  HMODULE dll;
  NV_ENCODE_API_FUNCTION_LIST functions;
  char why[192];
};

static struct nvenc_library library;
static INIT_ONCE library_once = INIT_ONCE_STATIC_INIT;

static BOOL CALLBACK load_library(PINIT_ONCE once, PVOID parameter, PVOID *context) {
  typedef NVENCSTATUS(NVENCAPI * create_instance)(NV_ENCODE_API_FUNCTION_LIST *);
  create_instance create;
  NVENCSTATUS status;
  (void)once;
  (void)parameter;
  (void)context;
  /* Naming the DLL rather than linking it is what lets the binary start, and
   * the stream open, on a machine with no NVIDIA driver at all. */
  library.dll = LoadLibraryA("nvEncodeAPI64.dll");
  if (!library.dll) {
    snprintf(library.why, sizeof library.why,
             "nvEncodeAPI64.dll didn't load (error %lu): NVIDIA's driver is missing or older "
             "than 520",
             (unsigned long)GetLastError());
    return TRUE;
  }
  create = (create_instance)(void *)GetProcAddress(library.dll, "NvEncodeAPICreateInstance");
  if (!create) {
    snprintf(library.why, sizeof library.why,
             "nvEncodeAPI64.dll has no NvEncodeAPICreateInstance: NVIDIA's driver is older than "
             "this needs");
    return TRUE;
  }
  library.functions.version = NV_ENCODE_API_FUNCTION_LIST_VER;
  status = create(&library.functions);
  if (status != NV_ENC_SUCCESS)
    status_failure(library.why, sizeof library.why, "NvEncodeAPICreateInstance", status);
  return TRUE;
}

/* The driver's functions, or NULL with `why` saying what went wrong. */
static const NV_ENCODE_API_FUNCTION_LIST *driver(char *why, size_t size) {
  InitOnceExecuteOnce(&library_once, load_library, NULL, NULL);
  if (!library.why[0]) return &library.functions;
  if (size) snprintf(why, size, "%s", library.why);
  return NULL;
}

/* ---------------------------------------------------------------- the device */

/* The D3D11 device the session encodes from, and the adapter it sits on. A
 * machine with integrated graphics presents more than one adapter and the WDDM
 * device has to be the one NVENC lives on, so the NVIDIA one is picked by
 * vendor id rather than by being first. */
static int open_device(IDXGIAdapter1 **adapter_out, ID3D11Device **device_out,
                       ID3D11DeviceContext **context_out, char *why, size_t size) {
  IDXGIFactory1 *factory = NULL;
  IDXGIAdapter1 *chosen = NULL, *each = NULL;
  HRESULT status = CreateDXGIFactory1(&IID_IDXGIFactory1, (void **)&factory);
  /* Every adapter is named in the failure, not just the fact that none was
   * NVIDIA: a machine whose card wasn't found says which cards it does have,
   * rather than leaving it to be guessed from the hardware. */
  char seen[192] = "";
  size_t used = 0;
  if (FAILED(status)) {
    if (size) snprintf(why, size, "CreateDXGIFactory1 failed (0x%08lX)", (unsigned long)status);
    return -1;
  }
  for (UINT i = 0; factory->lpVtbl->EnumAdapters1(factory, i, &each) != DXGI_ERROR_NOT_FOUND; i++) {
    DXGI_ADAPTER_DESC1 desc;
    if (FAILED(each->lpVtbl->GetDesc1(each, &desc))) {
      each->lpVtbl->Release(each);
      each = NULL;
      continue;
    }
    if (used < sizeof seen - 1) {
      int written = snprintf(seen + used, sizeof seen - used, "%s%ls (0x%04lX)",
                             used ? ", " : "", desc.Description, (unsigned long)desc.VendorId);
      if (written > 0) used += (size_t)written;
      if (used >= sizeof seen) used = sizeof seen - 1;
    }
    if (!chosen && desc.VendorId == 0x10DE && !(desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE)) {
      chosen = each;
    } else {
      each->lpVtbl->Release(each);
    }
    each = NULL;
  }
  factory->lpVtbl->Release(factory);
  if (!chosen) {
    if (size)
      snprintf(why, size, "no NVIDIA adapter to open an NVENC session on; this machine has %s",
               seen[0] ? seen : "none");
    return -1;
  }
  /* BGRA is what a screen arrives in, so the device has to support it; a driver
   * that won't take the flag still encodes RGBA, so it is asked for first and
   * then let go of. */
  D3D_FEATURE_LEVEL levels[] = {D3D_FEATURE_LEVEL_12_1, D3D_FEATURE_LEVEL_12_0,
                                D3D_FEATURE_LEVEL_11_1, D3D_FEATURE_LEVEL_11_0};
  D3D_FEATURE_LEVEL got = 0;
  status = D3D11CreateDevice((IDXGIAdapter *)chosen, D3D_DRIVER_TYPE_UNKNOWN, NULL,
                             D3D11_CREATE_DEVICE_BGRA_SUPPORT, levels, ARRAYSIZE(levels),
                             D3D11_SDK_VERSION, device_out, &got, context_out);
  if (FAILED(status))
    status = D3D11CreateDevice((IDXGIAdapter *)chosen, D3D_DRIVER_TYPE_UNKNOWN, NULL, 0, levels,
                               ARRAYSIZE(levels), D3D11_SDK_VERSION, device_out, &got,
                               context_out);
  if (FAILED(status)) {
    if (size) snprintf(why, size, "D3D11CreateDevice failed (0x%08lX)", (unsigned long)status);
    chosen->lpVtbl->Release(chosen);
    return -1;
  }
  InterlockedIncrement(&devices_created);
  *adapter_out = chosen;
  return 0;
}

/* The texture's concrete format as a name, and as the buffer format the driver
 * registers it under: a typeless texture is refused, so this reads the format
 * the surface really has. */
static const char *texture_format(DXGI_FORMAT format, NV_ENC_BUFFER_FORMAT *out) {
  switch (format) {
  case DXGI_FORMAT_B8G8R8A8_UNORM:
    *out = NV_ENC_BUFFER_FORMAT_ARGB;
    return "BGRA";
  case DXGI_FORMAT_R8G8B8A8_UNORM:
    *out = NV_ENC_BUFFER_FORMAT_ABGR;
    return "RGBA";
  case DXGI_FORMAT_NV12:
    *out = NV_ENC_BUFFER_FORMAT_NV12;
    return "NV12";
  default: return NULL;
  }
}

/* ------------------------------------------------------------------- encoder */

/* One texture registered and mapped for the encoder to read, kept for as long as
 * it keeps coming back: the driver's registration is keyed on the resource, so
 * doing it once per surface rather than once per frame is the difference
 * between a capture loop this keeps up with and one it doesn't. */
struct slot {
  ID3D11Texture2D *texture;
  unsigned subresource;
  NV_ENC_REGISTERED_PTR registered;
  NV_ENC_INPUT_PTR mapped;
  NV_ENC_BUFFER_FORMAT format;
  unsigned long long used;
};

struct yapfast_d3d11enc;

struct yapfast_d3d11enc_source *yapfast_d3d11enc_source_new(struct yapfast_d3d11enc *e,
                                                            unsigned frames, char *why,
                                                            size_t size);
void yapfast_d3d11enc_source_free(struct yapfast_d3d11enc_source *source);
void yapfast_d3d11enc_free(struct yapfast_d3d11enc *e);

struct yapfast_d3d11enc {
  IDXGIAdapter1 *adapter;
  ID3D11Device *device;
  ID3D11DeviceContext *context;
  void *session;
  NV_ENC_OUTPUT_PTR bitstream;
  /* What the encoder was opened with, kept so that a bitrate change can
   * reconfigure the same session instead of opening another. */
  NV_ENC_INITIALIZE_PARAMS init;
  NV_ENC_CONFIG settings;
  unsigned codec_id, width, height, fps, kbps;
  /* Which of the codecs the driver offered this session, as bits of codec ids:
   * AV1 needs an RTX 40 series or newer, and what a card refuses is worth
   * saying in the open's report rather than leaving to a status number. */
  unsigned supported;
  const struct nvenc_tuning *tuning;
  unsigned long long frames;
  /* Frames' pixels moved by the processor since this encoder opened: a count
   * the encode path keeps at zero, and says so. */
  unsigned long long copied;
  struct slot slots[SLOTS];
  char error[192];
  char config[128];
  char tuning_text[96];
  char codecs[64];
};

/* What went wrong, for Rust to log beside the frame it lost. */
void yapfast_d3d11enc_error(struct yapfast_d3d11enc *e, char *error, size_t size) {
  if (!size) return;
  snprintf(error, size, "%s", e && e->error[0] ? e->error : "no error");
}

/* Fails a call with a reason that travels to Rust, and returns `value`, so the
 * call sites that fail read as one line. */
static long fail(struct yapfast_d3d11enc *e, long value, const char *format, ...) {
  if (e) {
    va_list args;
    va_start(args, format);
    vsnprintf(e->error, sizeof e->error, format, args);
    va_end(args);
  }
  return value;
}

/* The codecs this session's driver will encode, as bits of codec ids and in
 * words: what the stream can offer its viewers, read from the card that would
 * encode them rather than from a list of cards. */
static void note_codecs(struct yapfast_d3d11enc *e) {
  GUID found[16];
  uint32_t count = 0, given = 0;
  size_t at = 0;
  e->codecs[0] = 0;
  e->supported = 0;
  if (library.functions.nvEncGetEncodeGUIDCount(e->session, &count) != NV_ENC_SUCCESS || !count ||
      library.functions.nvEncGetEncodeGUIDs(e->session, found, ARRAYSIZE(found), &given) !=
          NV_ENC_SUCCESS)
    return;
  for (unsigned id = 0; id < CODEC_COUNT; id++) {
    for (uint32_t i = 0; i < given; i++) {
      if (memcmp(&found[i], CODECS[id].guid, sizeof found[i])) continue;
      e->supported |= 1u << id;
      if (at < sizeof e->codecs - 1)
        at += (size_t)snprintf(e->codecs + at, sizeof e->codecs - at, "%s%s", at ? " " : "",
                               CODECS[id].short_name);
      break;
    }
  }
}

/* The codecs the driver offered this session, such as "h264 hevc av1", and
 * empty when it wouldn't say. */
void yapfast_d3d11enc_codecs(struct yapfast_d3d11enc *e, char *codecs, size_t size) {
  if (size) snprintf(codecs, size, "%s", e->codecs);
}

/* What this encoder took, in the words a log and a measurement quote: the card
 * it runs on, what the caller's textures are, and what the stream's frames are
 * encoded as. */
void yapfast_d3d11enc_config(struct yapfast_d3d11enc *e, char *config, size_t size) {
  if (size) snprintf(config, size, "%s", e->config);
}

/* The tuning it was opened with, as native/hwenc.c reports the same thing, so a
 * stream's picture is compared with the encoder that made it. */
void yapfast_d3d11enc_tuning(struct yapfast_d3d11enc *e, char *tuning, size_t size) {
  if (size) snprintf(tuning, size, "%s", e->tuning_text);
}

/* The name of the row the stream asked for, or of the one it was given when it
 * named none: the ask, rather than the settings it came out as. */
void yapfast_d3d11enc_asked(struct yapfast_d3d11enc *e, char *asked, size_t size) {
  if (size) snprintf(asked, size, "%s", e && e->tuning ? e->tuning->name : "");
}

/* How many frames' pixels the processor moved: zero, for as long as the picture
 * goes to the encoder on the GPU, which is what this backend is for and the one
 * thing a careless change would take away without any frame looking different. */
unsigned long long yapfast_d3d11enc_copied(struct yapfast_d3d11enc *e) {
  return e ? e->copied : 0;
}

/* The device the caller's textures have to live on, as an ID3D11Device: a
 * capture source creates its surfaces on it, since a texture from another device
 * can't be read by this session. */
void *yapfast_d3d11enc_device(struct yapfast_d3d11enc *e) { return e ? e->device : NULL; }

/* The settings the tuning named `name` opens NVENC with, and whether the stream
 * can't do without each, into `asked`; the first row's when `name` is NULL. The
 * same words as native/hwenc.c's, so a test holds the two backends to one answer
 * without a GPU in the shape of the question. Returns 0, or -1 for a name no row
 * has. */
int yapfast_d3d11enc_tuning_settings(const char *name, char *asked, size_t size) {
  const struct nvenc_tuning *tuning = nvenc_tuning(name);
  if (!tuning) return -1;
  if (size) tuning_settings(tuning, asked, size);
  return 0;
}

/* The settings the encoder is opened with, in NVENC's parameters: the row's
 * preset and tuning, a frame out for each frame in, an IDR every three seconds
 * and on demand, the cap spent as the row says over half a second, and the
 * colour every frame is declared in. */
static void configure(struct yapfast_d3d11enc *e, const NV_ENC_PRESET_CONFIG *preset) {
  const struct nvenc_tuning *tuning = e->tuning;
  unsigned gop = e->fps * 3;
  e->settings = preset->presetCfg;
  e->settings.version = NV_ENC_CONFIG_VER;
  /* A new viewer waits at most three seconds for a picture, as native/hwenc.c's
   * gop_size says, and the same interval is the IDR period so a picture that
   * broke is repaired by the next keyframe either way. */
  e->settings.gopLength = gop;
  /* No B-frames, one frame in flight: a frame comes out for each frame in, which
   * is what a stream somebody is watching can't do without. */
  e->settings.frameIntervalP = 1;
  e->settings.rcParams.rateControlMode =
      tuning->rc[0] == 'v' ? NV_ENC_PARAMS_RC_VBR : NV_ENC_PARAMS_RC_CBR;
  e->settings.rcParams.averageBitRate = e->kbps * 1000;
  /* Capped at the target, as FFmpeg's rc_max_rate is: a frame may spend less,
   * never more, so a burst can't run past Discord's cap. */
  e->settings.rcParams.maxBitRate = e->kbps * 1000;
  /* Half a second, as hwenc.c's rc_buffer_size: a cap a burst can spend and no
   * longer, where a whole second's let a burst reach twice the cap. */
  e->settings.rcParams.vbvBufferSize = e->kbps * 500;
  e->settings.rcParams.vbvInitialDelay = e->settings.rcParams.vbvBufferSize;
  e->settings.rcParams.zeroReorderDelay = tuning->delay ? 0 : 1;
  e->settings.rcParams.enableLookahead = tuning->zerolatency ? 0 : 1;
  e->settings.rcParams.lookaheadDepth = tuning->zerolatency ? 0 : 8;
  e->settings.rcParams.lowDelayKeyFrameScale = 1;

  /* The per-codec settings, in the one layout of the three the union holds
   * room for: `encodeCodecConfig` is a union of H.264's, HEVC's and AV1's
   * configurations, so writing all three would leave the last one standing in
   * the others' bytes, and the driver reading a codec's settings out of another
   * codec's layout is exactly the invalid parameter it reports. One codec is
   * chosen per instance, so one layout is written.
   *
   * The colour is the same in all three, and the same claim native/hwenc.c makes
   * to FFmpeg: BT.601's primaries, transfer and matrix over its limited range,
   * which is the conversion libyuv's ABGR to I420 writes on the paths that still
   * convert. NVENC converts a BGRA texture with that same matrix and range, so
   * both halves of the stream's colour claim are one statement. */
  switch (e->codec_id) {
  case CODEC_H264: {
    NV_ENC_CONFIG_H264 *h264 = &e->settings.encodeCodecConfig.h264Config;
    h264->chromaFormatIDC = 1;
    h264->idrPeriod = gop;
    h264->repeatSPSPPS = 1;
    /* One slice a frame, cut by NVENC, which is what a packetizer and a lost
     * packet want. */
    h264->sliceMode = 3;
    h264->sliceModeData = 1;
    /* Left to the driver, as FFmpeg leaves it: it knows the level this size and
     * rate need, and a level asked for by hand would only cap them. */
    h264->level = 0;
    /* Annex B, as native/h264.c and native/hwenc.c write it: start codes, which
     * is what the packetizer's nal_units reads. NVENC writes H.264 and HEVC in
     * Annex B without being asked, and only AV1 has a switch to turn it off. */
    h264->h264VUIParameters.videoSignalTypePresentFlag = 1;
    h264->h264VUIParameters.videoFullRangeFlag = 0;
    h264->h264VUIParameters.colourDescriptionPresentFlag = 1;
    h264->h264VUIParameters.colourPrimaries = NV_ENC_VUI_COLOR_PRIMARIES_SMPTE170M;
    h264->h264VUIParameters.transferCharacteristics =
        NV_ENC_VUI_TRANSFER_CHARACTERISTIC_SMPTE170M;
    h264->h264VUIParameters.colourMatrix = NV_ENC_VUI_MATRIX_COEFFS_SMPTE170M;
    /* Main profile is Discord's client's, as native/h264.c and native/hwenc.c
     * ask for. */
    e->settings.profileGUID = NV_ENC_H264_PROFILE_MAIN_GUID;
    break;
  }
  case CODEC_HEVC: {
    NV_ENC_CONFIG_HEVC *hevc = &e->settings.encodeCodecConfig.hevcConfig;
    hevc->chromaFormatIDC = 1;
    hevc->idrPeriod = gop;
    hevc->repeatSPSPPS = 1;
    hevc->sliceMode = 3;
    hevc->sliceModeData = 1;
    hevc->hevcVUIParameters.videoSignalTypePresentFlag = 1;
    hevc->hevcVUIParameters.videoFullRangeFlag = 0;
    hevc->hevcVUIParameters.colourDescriptionPresentFlag = 1;
    hevc->hevcVUIParameters.colourPrimaries = NV_ENC_VUI_COLOR_PRIMARIES_SMPTE170M;
    hevc->hevcVUIParameters.transferCharacteristics =
        NV_ENC_VUI_TRANSFER_CHARACTERISTIC_SMPTE170M;
    hevc->hevcVUIParameters.colourMatrix = NV_ENC_VUI_MATRIX_COEFFS_SMPTE170M;
    e->settings.profileGUID = NV_ENC_HEVC_PROFILE_MAIN_GUID;
    break;
  }
  default: {
    NV_ENC_CONFIG_AV1 *av1 = &e->settings.encodeCodecConfig.av1Config;
    av1->chromaFormatIDC = 1;
    av1->idrPeriod = gop;
    /* A sequence header with every keyframe, so a viewer who joins mid-stream
     * gets the parameters it needs with the picture it needs them for. */
    av1->repeatSeqHdr = 1;
    av1->colorPrimaries = NV_ENC_VUI_COLOR_PRIMARIES_SMPTE170M;
    av1->transferCharacteristics = NV_ENC_VUI_TRANSFER_CHARACTERISTIC_SMPTE170M;
    av1->matrixCoefficients = NV_ENC_VUI_MATRIX_COEFFS_SMPTE170M;
    /* Studio range, AV1's spelling of limited. */
    av1->colorRange = 0;
    /* AV1's temporal units are packed OBUs, not start codes: the packetizer
     * reads their sizes, and Annex B would make them unreadable. */
    av1->outputAnnexBFormat = 0;
    e->settings.profileGUID = NV_ENC_AV1_PROFILE_MAIN_GUID;
    break;
  }
  }
}

/* Opens an encoder for one codec on a new D3D11 device, `width` by `height` at
 * `fps` and `kbps`, tuned by `tuning_name`. `report` gets the name of what
 * opened, or why nothing did: one backend, so one line either way, in words that
 * name the driver's refusal rather than its status number. */
struct yapfast_d3d11enc *yapfast_d3d11enc_new(unsigned codec_id, unsigned width, unsigned height,
                                              unsigned fps, unsigned kbps, const char *tuning_name,
                                              char *report, size_t report_size) {
  struct yapfast_d3d11enc *e;
  NV_ENC_OPEN_ENCODE_SESSION_EX_PARAMS open;
  NV_ENC_PRESET_CONFIG preset;
  NVENCSTATUS status;
  char why[192] = "";

  if (report_size) report[0] = 0;
  if (codec_id >= CODEC_COUNT) {
    if (report_size) snprintf(report, report_size, "%u isn't a codec this encoder has", codec_id);
    return NULL;
  }
  const struct nvenc_tuning *tuning = nvenc_tuning(tuning_name);
  if (!tuning) {
    if (report_size) snprintf(report, report_size, "no tuning called %s", tuning_name);
    return NULL;
  }
  if (!width || !height || (width & 1) || (height & 1)) {
    if (report_size)
      snprintf(report, report_size, "%ux%u isn't an even size NVENC encodes", width, height);
    return NULL;
  }
  if (!driver(why, sizeof why)) {
    if (report_size) snprintf(report, report_size, "%s", why);
    return NULL;
  }
  const NV_ENCODE_API_FUNCTION_LIST *api = &library.functions;
  e = calloc(1, sizeof *e);
  if (!e) {
    if (report_size) snprintf(report, report_size, "no memory for the encoder");
    return NULL;
  }
  e->codec_id = codec_id;
  e->width = width;
  e->height = height;
  e->fps = fps;
  e->kbps = kbps;
  e->tuning = tuning;

  if (open_device(&e->adapter, &e->device, &e->context, why, sizeof why)) goto failed;

  memset(&open, 0, sizeof open);
  open.version = NV_ENC_OPEN_ENCODE_SESSION_EX_PARAMS_VER;
  open.deviceType = NV_ENC_DEVICE_TYPE_DIRECTX;
  open.device = e->device;
  open.apiVersion = NVENCAPI_VERSION;
  status = api->nvEncOpenEncodeSessionEx(&open, &e->session);
  if (status != NV_ENC_SUCCESS) {
    status_failure(why, sizeof why, "nvEncOpenEncodeSessionEx on the D3D11 device", status);
    goto failed;
  }
  /* The driver is asked which codecs it encodes before one is opened: a card
   * whose driver has no encoder for the codec asked for says so in a sentence,
   * rather than failing later with a status nobody can act on. */
  note_codecs(e);
  if (e->supported && !(e->supported & (1u << codec_id))) {
    snprintf(why, sizeof why, "this driver has no %s encoder (it has %s)%s", CODECS[codec_id].name,
             e->codecs, codec_id == CODEC_AV1 ? ", and AV1 needs an RTX 40 series or newer" : "");
    goto failed;
  }

  memset(&preset, 0, sizeof preset);
  preset.version = NV_ENC_PRESET_CONFIG_VER;
  preset.presetCfg.version = NV_ENC_CONFIG_VER;
  status = api->nvEncGetEncodePresetConfigEx(e->session, *CODECS[codec_id].guid,
                                             *tuning->preset_guid, tuning->tune, &preset);
  if (status != NV_ENC_SUCCESS) {
    status_failure(why, sizeof why, "nvEncGetEncodePresetConfigEx", status);
    goto failed;
  }
  configure(e, &preset);

  memset(&e->init, 0, sizeof e->init);
  e->init.version = NV_ENC_INITIALIZE_PARAMS_VER;
  e->init.encodeGUID = *CODECS[codec_id].guid;
  e->init.presetGUID = *tuning->preset_guid;
  e->init.tuningInfo = tuning->tune;
  e->init.encodeWidth = width;
  e->init.encodeHeight = height;
  e->init.darWidth = width;
  e->init.darHeight = height;
  e->init.frameRateNum = fps;
  e->init.frameRateDen = 1;
  /* Synchronous: a frame in is a packet out on the same call, so the capture
   * loop never holds a picture it can't account for. */
  e->init.enableEncodeAsync = 0;
  e->init.enablePTD = 1;
  e->init.maxEncodeWidth = width;
  e->init.maxEncodeHeight = height;
  e->init.encodeConfig = &e->settings;
  status = api->nvEncInitializeEncoder(e->session, &e->init);
  if (status != NV_ENC_SUCCESS) {
    status_failure(why, sizeof why, "nvEncInitializeEncoder", status);
    goto failed;
  }

  NV_ENC_CREATE_BITSTREAM_BUFFER bitstream;
  memset(&bitstream, 0, sizeof bitstream);
  bitstream.version = NV_ENC_CREATE_BITSTREAM_BUFFER_VER;
  bitstream.size = BITSTREAM_BYTES;
  bitstream.memoryHeap = NV_ENC_MEMORY_HEAP_AUTOSELECT;
  status = api->nvEncCreateBitstreamBuffer(e->session, &bitstream);
  if (status != NV_ENC_SUCCESS) {
    status_failure(why, sizeof why, "nvEncCreateBitstreamBuffer", status);
    goto failed;
  }
  e->bitstream = bitstream.bitstreamBuffer;

  DXGI_ADAPTER_DESC1 desc;
  char card[128] = "";
  if (SUCCEEDED(e->adapter->lpVtbl->GetDesc1(e->adapter, &desc))) snprintf(card, sizeof card, "%ls, ", desc.Description);
  snprintf(e->config, sizeof e->config, "%sBGRA in, NV12 4:2:0 out, bt601 limited range", card);
  snprintf(e->tuning_text, sizeof e->tuning_text,
           "%s preset, %s rate control, keyframe every %u frames", tuning->preset, tuning->rc,
           e->settings.gopLength);
  if (report_size) snprintf(report, report_size, "%s_nvenc/d3d11", CODECS[codec_id].short_name);
  return e;

failed:
  if (report_size) snprintf(report, report_size, "%s", why);
  yapfast_d3d11enc_free(e);
  return NULL;
}

/* A slot with no texture, or the least recently used one: a capture source with
 * more surfaces than this keeps re-registering them, which is slower but never
 * wrong. */
static struct slot *free_slot(struct yapfast_d3d11enc *e) {
  struct slot *oldest = &e->slots[0];
  for (size_t i = 0; i < SLOTS; i++) {
    struct slot *slot = &e->slots[i];
    if (!slot->texture) return slot;
    if (slot->used < oldest->used) oldest = slot;
  }
  /* The surface that went away is unregistered before the new one takes its
   * place: the driver holds a reference to it until then. */
  if (oldest->mapped) library.functions.nvEncUnmapInputResource(e->session, oldest->mapped);
  if (oldest->registered)
    library.functions.nvEncUnregisterResource(e->session, oldest->registered);
  if (oldest->texture) oldest->texture->lpVtbl->Release(oldest->texture);
  memset(oldest, 0, sizeof *oldest);
  return oldest;
}

/* Registers and maps the caller's `texture` so the encoder can read it, and
 * returns the slot holding it. Everything that can go wrong here is said in
 * `why`, since a frame that can't be read is a frame the stream loses. */
static struct slot *slot_register(struct yapfast_d3d11enc *e, void *texture, unsigned subresource,
                                  char *why, size_t size) {
  NV_ENC_REGISTER_RESOURCE register_resource;
  NV_ENC_MAP_INPUT_RESOURCE map;
  D3D11_TEXTURE2D_DESC desc;
  ID3D11Device *owner = NULL;
  ID3D11Texture2D *surface = texture;
  NVENCSTATUS status;

  /* Which device owns it, asked rather than assumed: a texture from another
   * device is memory this session can't read, and the driver would fail the
   * registration rather than the frame. */
  surface->lpVtbl->GetDevice(surface, &owner);
  if (!owner) {
    snprintf(why, size, "the texture isn't a D3D11 texture this session can read");
    return NULL;
  }
  int ours = owner == e->device;
  owner->lpVtbl->Release(owner);
  if (!ours) {
    snprintf(why, size,
             "the texture lives on another D3D11 device than the session's; a capture source "
             "creates its textures on the device yapfast_d3d11enc_device hands out");
    return NULL;
  }
  surface->lpVtbl->GetDesc(surface, &desc);
  if (desc.Width != e->width || desc.Height != e->height) {
    snprintf(why, size, "the texture is %ux%u and the encoder %ux%u", desc.Width, desc.Height,
             e->width, e->height);
    return NULL;
  }
  if (subresource >= desc.MipLevels * desc.ArraySize) {
    snprintf(why, size, "the texture has no subresource %u", subresource);
    return NULL;
  }
  struct slot *slot = free_slot(e);
  if (!texture_format(desc.Format, &slot->format)) {
    snprintf(why, size,
             "the texture is DXGI format %u, and this reads BGRA, RGBA or NV12; a typeless "
             "texture can't be registered",
             (unsigned)desc.Format);
    return NULL;
  }

  memset(&register_resource, 0, sizeof register_resource);
  register_resource.version = NV_ENC_REGISTER_RESOURCE_VER;
  register_resource.resourceType = NV_ENC_INPUT_RESOURCE_TYPE_DIRECTX;
  register_resource.width = e->width;
  register_resource.height = e->height;
  /* Zero for a DirectX resource: the driver reads the surface's own layout. */
  register_resource.pitch = 0;
  register_resource.subResourceIndex = subresource;
  register_resource.resourceToRegister = surface;
  register_resource.bufferFormat = slot->format;
  register_resource.bufferUsage = NV_ENC_INPUT_IMAGE;
  status = library.functions.nvEncRegisterResource(e->session, &register_resource);
  if (status != NV_ENC_SUCCESS) {
    status_failure(why, size, "nvEncRegisterResource", status);
    return NULL;
  }
  memset(&map, 0, sizeof map);
  map.version = NV_ENC_MAP_INPUT_RESOURCE_VER;
  map.registeredResource = register_resource.registeredResource;
  status = library.functions.nvEncMapInputResource(e->session, &map);
  if (status != NV_ENC_SUCCESS) {
    library.functions.nvEncUnregisterResource(e->session, register_resource.registeredResource);
    status_failure(why, size, "nvEncMapInputResource", status);
    return NULL;
  }
  slot->texture = surface;
  slot->subresource = subresource;
  slot->registered = register_resource.registeredResource;
  slot->mapped = map.mappedResource;
  slot->used = e->frames;
  /* Held for as long as the driver has it registered, so a capture source that
   * lets go of a surface can't pull it out from under the encode. */
  surface->lpVtbl->AddRef(surface);
  return slot;
}

static void close_slots(struct yapfast_d3d11enc *e) {
  for (size_t i = 0; i < SLOTS; i++) {
    struct slot *slot = &e->slots[i];
    if (slot->mapped) library.functions.nvEncUnmapInputResource(e->session, slot->mapped);
    if (slot->registered)
      library.functions.nvEncUnregisterResource(e->session, slot->registered);
    if (slot->texture) slot->texture->lpVtbl->Release(slot->texture);
    memset(slot, 0, sizeof *slot);
  }
}

void yapfast_d3d11enc_free(struct yapfast_d3d11enc *e) {
  if (!e) return;
  if (e->session) {
    close_slots(e);
    if (e->bitstream) library.functions.nvEncDestroyBitstreamBuffer(e->session, e->bitstream);
    library.functions.nvEncDestroyEncoder(e->session);
  }
  if (e->context) e->context->lpVtbl->Release(e->context);
  if (e->device) e->device->lpVtbl->Release(e->device);
  if (e->adapter) e->adapter->lpVtbl->Release(e->adapter);
  free(e);
}

/* Encodes the picture in `texture`'s `subresource` into `out`, whose time is
 * `pts` in 90 kHz units: the stream's own clock, which labels the frame's
 * packets too. `keyframe` asks for an IDR on this frame, which is what a viewer
 * who joins, or whose picture broke, waits for. Returns the access unit's size,
 * 0 when the encoder made none, -2 when it doesn't fit `capacity` (a larger
 * buffer will hold it, as native/hwenc.c says), or -1 on failure, whose reason
 * `yapfast_d3d11enc_error` gives. `out` stays the caller's: this writes it and
 * keeps no reference to it, and the only copy of a frame's bytes is this one,
 * out of the driver's mapped bitstream. */
long yapfast_d3d11enc_encode(struct yapfast_d3d11enc *e, void *texture, unsigned subresource,
                             long long pts, int keyframe, unsigned char *out, size_t capacity,
                             int *is_key) {
  NV_ENC_PIC_PARAMS picture;
  NV_ENC_LOCK_BITSTREAM locked;
  NVENCSTATUS status;
  char why[192] = "";

  if (!e) return -1;
  if (!texture || !out || !is_key) return fail(e, -1, "the encoder was given nothing to encode");
  *is_key = 0;
  struct slot *slot = NULL;
  for (size_t i = 0; i < SLOTS && !slot; i++)
    if (e->slots[i].texture == texture && e->slots[i].subresource == subresource)
      slot = &e->slots[i];
  if (!slot) {
    slot = slot_register(e, texture, subresource, why, sizeof why);
    if (!slot) return fail(e, -1, "%s", why);
  }
  slot->used = e->frames;

  memset(&picture, 0, sizeof picture);
  picture.version = NV_ENC_PIC_PARAMS_VER;
  picture.inputWidth = e->width;
  picture.inputHeight = e->height;
  /* Zero: the mapped resource already carries its surface's layout. */
  picture.inputPitch = 0;
  picture.inputBuffer = slot->mapped;
  picture.outputBitstream = e->bitstream;
  picture.bufferFmt = slot->format;
  picture.pictureStruct = NV_ENC_PIC_STRUCT_FRAME;
  picture.frameIdx = (uint32_t)e->frames;
  picture.inputTimeStamp = (uint64_t)pts;
  picture.pictureType = NV_ENC_PIC_TYPE_UNKNOWN;
  if (keyframe && e->tuning->forced_idr) picture.encodePicFlags = NV_ENC_PIC_FLAG_FORCEIDR;
  status = library.functions.nvEncEncodePicture(e->session, &picture);
  if (status != NV_ENC_SUCCESS)
    return fail(e, -1, "nvEncEncodePicture on frame %llu failed: %s (%d)", e->frames,
                nvenc_status(status), (int)status);
  e->frames++;

  memset(&locked, 0, sizeof locked);
  locked.version = NV_ENC_LOCK_BITSTREAM_VER;
  locked.outputBitstream = e->bitstream;
  /* Waits for the frame rather than polling: the encode is synchronous, so this
   * returns once and the packet is whole. */
  locked.doNotWait = 0;
  status = library.functions.nvEncLockBitstream(e->session, &locked);
  if (status != NV_ENC_SUCCESS)
    return fail(e, -1, "nvEncLockBitstream failed: %s (%d)", nvenc_status(status), (int)status);
  size_t size = locked.bitstreamSizeInBytes;
  if (size > capacity) {
    library.functions.nvEncUnlockBitstream(e->session, locked.outputBitstream);
    return -2;
  }
  if (size) memcpy(out, locked.bitstreamBufferPtr, size);
  *is_key = locked.pictureType == NV_ENC_PIC_TYPE_IDR || locked.pictureType == NV_ENC_PIC_TYPE_I;
  library.functions.nvEncUnlockBitstream(e->session, locked.outputBitstream);
  return (long)size;
}

/* Aims for `kbps` from the next frame on, without a keyframe: the session is
 * reconfigured rather than reopened, so the frames a viewer is watching stay
 * references and the stream's picture doesn't blink. Returns 0, or -1 with the
 * driver's reason. */
int yapfast_d3d11enc_set_bitrate(struct yapfast_d3d11enc *e, unsigned kbps) {
  NV_ENC_RECONFIGURE_PARAMS reconfigure;
  NVENCSTATUS status;
  if (!e) return -1;
  e->settings.rcParams.averageBitRate = kbps * 1000;
  e->settings.rcParams.maxBitRate = kbps * 1000;
  e->settings.rcParams.vbvBufferSize = kbps * 500;
  e->settings.rcParams.vbvInitialDelay = e->settings.rcParams.vbvBufferSize;
  memset(&reconfigure, 0, sizeof reconfigure);
  reconfigure.version = NV_ENC_RECONFIGURE_PARAMS_VER;
  reconfigure.reInitEncodeParams = e->init;
  /* The same session and the same surfaces: no keyframe is inserted, and the
   * frames the encoder has already sent stay references. */
  reconfigure.resetEncoder = 0;
  reconfigure.forceIDR = 0;
  status = library.functions.nvEncReconfigureEncoder(e->session, &reconfigure);
  if (status != NV_ENC_SUCCESS) {
    fail(e, -1, "nvEncReconfigureEncoder to %u kbit/s failed: %s (%d)", kbps,
         nvenc_status(status), (int)status);
    return -1;
  }
  e->kbps = kbps;
  return 0;
}

/* -------------------------------------------------- the stand-in capture source
 *
 * The capture path doesn't make textures yet, so this draws the frames a test
 * encodes: one BGRA texture on the encoder's device, refilled by a compute
 * shader. On the GPU, deliberately — a picture uploaded from the processor would
 * be exactly the copy this backend exists to avoid, and a measurement of the
 * encode would be a measurement of the upload. When the capture source produces
 * textures of its own, this is what it replaces, and the encoder is unchanged: a
 * texture, its subresource, and a frame's time. */

struct yapfast_d3d11enc_source {
  struct yapfast_d3d11enc *encoder;
  ID3D11Texture2D *texture;
  ID3D11UnorderedAccessView *view;
  ID3D11ComputeShader *shader;
  /* One 32-byte constant buffer per frame, holding the frame's index and the
   * texture's size to animate on and clip against: made up front, so a frame
   * costs a bind and a dispatch and no write of its own. */
  ID3D11Buffer **params;
  unsigned frames;
};

/* A moving picture, so the frames differ from each other the way a screen
 * somebody is using does: an encoder given one still image thirty times
 * compresses almost nothing, and the measurement would say less than it seems
 * to. */
static const char *pattern_shader =
    "cbuffer Params : register(b0) { float4 g; float4 extent; };\n"
    "RWTexture2D<float4> dst : register(u0);\n"
    "[numthreads(8,8,1)]\n"
    "void CSMain(uint3 id : SV_DispatchThreadID) {\n"
    "  if (id.x >= (uint)extent.x || id.y >= (uint)extent.y) return;\n"
    "  float2 f = float2(id.xy);\n"
    "  float t = g.x * 0.09;\n"
    "  float3 c;\n"
    "  c.r = 0.5 + 0.5*sin(f.x*0.022 + t);\n"
    "  c.g = 0.5 + 0.5*sin(f.y*0.031 + t*1.3);\n"
    "  c.b = 0.5 + 0.5*sin((f.x+f.y)*0.013 + t*0.7);\n"
    "  float2 centre = float2(extent.x*0.5 + extent.x*0.34*cos(t*0.9),\n"
    "                         extent.y*0.5 + extent.y*0.34*sin(t*0.9));\n"
    "  c = lerp(c, float3(1.0, 0.15, 0.1), saturate((140.0 - distance(f, centre))/50.0));\n"
    "  if (frac(f.x/96.0) < 0.02) c *= 0.2;\n"
    "  dst[id.xy] = float4(c, 1.0);\n"
    "}\n";

/* A texture on the encoder's device, the shader that refills it, and the
 * per-frame constants the shader animates on. `why` says what failed: the device
 * is the encoder's, so a failure here is one the stream's caller has to see. */
struct yapfast_d3d11enc_source *yapfast_d3d11enc_source_new(struct yapfast_d3d11enc *e,
                                                            unsigned frames, char *why,
                                                            size_t size) {
  if (why && size) why[0] = 0;
  if (!e) return NULL;
  struct yapfast_d3d11enc_source *source = calloc(1, sizeof *source);
  if (!source) return NULL;
  source->encoder = e;
  source->frames = frames ? frames : 1;

  D3D11_TEXTURE2D_DESC desc;
  memset(&desc, 0, sizeof desc);
  desc.Width = e->width;
  desc.Height = e->height;
  desc.MipLevels = 1;
  desc.ArraySize = 1;
  /* Concrete, never typeless: the driver reads the surface's own format when it
   * registers it. */
  desc.Format = DXGI_FORMAT_B8G8R8A8_UNORM;
  desc.SampleDesc.Count = 1;
  desc.Usage = D3D11_USAGE_DEFAULT;
  desc.BindFlags = D3D11_BIND_UNORDERED_ACCESS | D3D11_BIND_SHADER_RESOURCE;
  HRESULT status = e->device->lpVtbl->CreateTexture2D(e->device, &desc, NULL, &source->texture);
  if (FAILED(status)) {
    if (size) snprintf(why, size, "CreateTexture2D failed (0x%08lX)", (unsigned long)status);
    yapfast_d3d11enc_source_free(source);
    return NULL;
  }

  ID3DBlob *blob = NULL, *errors = NULL;
  status = D3DCompile(pattern_shader, strlen(pattern_shader), "yapfast.hlsl", NULL, NULL, "CSMain",
                      "cs_5_0", D3DCOMPILE_OPTIMIZATION_LEVEL3, 0, &blob, &errors);
  if (FAILED(status)) {
    if (size)
      snprintf(why, size, "D3DCompile failed (0x%08lX) %.96s", (unsigned long)status,
               errors ? (const char *)errors->lpVtbl->GetBufferPointer(errors) : "");
    if (errors) errors->lpVtbl->Release(errors);
    yapfast_d3d11enc_source_free(source);
    return NULL;
  }
  if (errors) errors->lpVtbl->Release(errors);
  status = e->device->lpVtbl->CreateComputeShader(e->device, blob->lpVtbl->GetBufferPointer(blob),
                                                 blob->lpVtbl->GetBufferSize(blob), NULL,
                                                 &source->shader);
  blob->lpVtbl->Release(blob);
  if (FAILED(status)) {
    if (size) snprintf(why, size, "CreateComputeShader failed (0x%08lX)", (unsigned long)status);
    yapfast_d3d11enc_source_free(source);
    return NULL;
  }

  D3D11_UNORDERED_ACCESS_VIEW_DESC view;
  memset(&view, 0, sizeof view);
  view.Format = DXGI_FORMAT_B8G8R8A8_UNORM;
  view.ViewDimension = D3D11_UAV_DIMENSION_TEXTURE2D;
  status = e->device->lpVtbl->CreateUnorderedAccessView(e->device, (ID3D11Resource *)source->texture,
                                                       &view, &source->view);
  if (FAILED(status)) {
    if (size)
      snprintf(why, size, "CreateUnorderedAccessView failed (0x%08lX)", (unsigned long)status);
    yapfast_d3d11enc_source_free(source);
    return NULL;
  }

  source->params = calloc(source->frames, sizeof *source->params);
  if (!source->params) {
    yapfast_d3d11enc_source_free(source);
    return NULL;
  }
  for (unsigned frame = 0; frame < source->frames; frame++) {
    float values[8] = {(float)frame, 0.f, 0.f, 0.f, (float)e->width, (float)e->height, 0.f, 0.f};
    D3D11_BUFFER_DESC buffer;
    D3D11_SUBRESOURCE_DATA data;
    memset(&buffer, 0, sizeof buffer);
    buffer.ByteWidth = sizeof values;
    buffer.Usage = D3D11_USAGE_IMMUTABLE;
    buffer.BindFlags = D3D11_BIND_CONSTANT_BUFFER;
    memset(&data, 0, sizeof data);
    data.pSysMem = values;
    status = e->device->lpVtbl->CreateBuffer(e->device, &buffer, &data, &source->params[frame]);
    if (FAILED(status)) {
      if (size)
        snprintf(why, size, "CreateBuffer(constant) failed (0x%08lX)", (unsigned long)status);
      yapfast_d3d11enc_source_free(source);
      return NULL;
    }
  }
  return source;
}

/* Refills the texture on the GPU with the picture `frame` shows: three commands,
 * and nothing of the frame crosses the processor. */
void yapfast_d3d11enc_source_draw(struct yapfast_d3d11enc_source *source, unsigned frame) {
  if (!source) return;
  struct yapfast_d3d11enc *e = source->encoder;
  ID3D11DeviceContext *context = e->context;
  ID3D11Buffer *params = source->params[frame % source->frames];
  context->lpVtbl->CSSetConstantBuffers(context, 0, 1, &params);
  context->lpVtbl->CSSetUnorderedAccessViews(context, 0, 1, &source->view, NULL);
  context->lpVtbl->CSSetShader(context, source->shader, NULL, 0);
  context->lpVtbl->Dispatch(context, (e->width + 7) / 8, (e->height + 7) / 8, 1);
}

/* The texture the source draws into, as the ID3D11Texture2D the encoder reads: a
 * texture the source owns and the caller passes on, which is what a capture
 * source will hand over instead. */
void *yapfast_d3d11enc_source_texture(struct yapfast_d3d11enc_source *source) {
  return source ? source->texture : NULL;
}

void yapfast_d3d11enc_source_free(struct yapfast_d3d11enc_source *source) {
  if (!source) return;
  if (source->params) {
    for (unsigned frame = 0; frame < source->frames; frame++)
      if (source->params[frame]) source->params[frame]->lpVtbl->Release(source->params[frame]);
    free(source->params);
  }
  if (source->view) source->view->lpVtbl->Release(source->view);
  if (source->shader) source->shader->lpVtbl->Release(source->shader);
  if (source->texture) source->texture->lpVtbl->Release(source->texture);
  free(source);
}
