/* Zero-copy encoding for Go Live on a Linux desktop: NVENC reads the NV12
 * picture that native/cudaconvert.c has already made on the card, so a captured
 * frame reaches the encoder with its pixels where the encoder will read them.
 *
 * Windows has the other door, native/d3d11enc.c: the capture fills a D3D11
 * texture and NVENC reads that texture where it lies. NVENC's Linux input is
 * CUDA memory instead, and this is the twin of that file for it: a session
 * opened on the CUDA context that holds the picture, the picture registered and
 * mapped as `NV_ENC_INPUT_RESOURCE_TYPE_CUDADEVICEPTR`, and one
 * `nvEncEncodePicture` per frame. Nothing here converts, scales or uploads: the
 * converter wrote the stream's picture as NV12 into a `cuMemAlloc` buffer, and
 * the pitch it made is the pitch the driver is told.
 *
 * The one copy left is the bitstream, moved out of the driver's own buffer once
 * per frame because that buffer belongs to the driver and the caller's packet has
 * to outlive the call that locked it. The picture is never touched, and
 * `yapfast_cudaenc_copied` answers zero for as long as that stays true, so a
 * change that reintroduces a per-frame upload says so from the encoder rather
 * than from reading this file.
 *
 * What makes it fast is also what makes it delicate: a `cuMemAlloc` pointer is
 * only memory in the context that allocated it, and NVENC resolves an input
 * resource in the context that is current rather than in one it remembers. So
 * the caller hands over the converter's context, every call here pushes it
 * current and pops it again, and the open asks the driver whose memory the
 * buffer is before registering it — a session pointed at another context's
 * buffer would fail at the first frame with a status nobody could act on.
 *
 * None of this needs a CUDA toolkit or FFmpeg's CUDA hwcontext: the driver's own
 * libcuda.so.1 and libnvidia-encode.so.1 are dlopened when a share asks for this
 * path, which is also what lets the binary start, and the stream stay on the
 * processor, on a machine with no NVIDIA driver at all. The vendored ffnvcodec
 * header is declarations only.
 *
 * The settings are native/hwenc.c's `NVENC_TUNINGS` row for the tuning asked
 * for, said again here as NVENC's own parameters exactly as native/d3d11enc.c
 * says them: low-latency tuning, no B-frames, a frame out for each frame in, an
 * IDR on demand, and capped VBR over half a second's buffer. The parameters
 * belong to the session, not to the kind of memory it reads, so they are one
 * statement across the two GPU backends, and a test holds the three tables to one
 * answer (`yapfast_cudaenc_tuning_settings`). */
/* readlink's neighbours are POSIX's, and pthread_once is what loads the driver
 * once rather than per encoder; this file is GNU C, as the rest of native/ is. */
#include <dlfcn.h>
#include <pthread.h>
#include <stdarg.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* The vendored ffnvcodec header, which FFmpeg's NVENC encoder uses too: only
 * declarations, since every entry point is resolved from the driver's own
 * library. Relative to this file, so that the build and an editor's language
 * server resolve the same header without an include path either of them has to
 * know. */
#include "../../../vendor/ffmpeg-src/include/ffnvcodec/nvEncodeAPI.h"

/* The CUDA driver API, as far as this file needs it: the types are the driver's,
 * and the few entry points are looked up when a share asks for the card rather
 * than linked, so nothing here needs the toolkit's headers. The declarations
 * match cuda.h's. */
typedef int CUresult;
typedef int CUdevice;
typedef void *CUcontext;
typedef unsigned long long CUdeviceptr;

#define CUDA_SUCCESS 0

/* Where the driver keeps the context a device pointer was allocated in: how a
 * buffer from another context is caught before a session is opened on it. */
#define CU_POINTER_ATTRIBUTE_CONTEXT 1

/* The codecs, as Rust numbers them: 0 H.264, 1 HEVC, 2 AV1, the same numbers
 * native/hwenc.c and native/d3d11enc.c take. */
enum { CODEC_H264, CODEC_HEVC, CODEC_AV1, CODEC_COUNT };

/* Enough for a keyframe of a 1080p screen at the stream's cap, and drawn from
 * the driver's own heap: a bitstream buffer that is too small fails the encode,
 * and a keyframe of noise at 8 Mbit/s is under this. */
#define BITSTREAM_BYTES (4u << 20)

/* The tuning the stream asks a GPU's encoder for, in NVENC's terms: the same row
 * native/hwenc.c names, so what the stream asks for reads the same whichever
 * side encodes it.
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

/* The tuning Rust asked for by name, or the first row when it named none. A name
 * no row has is NULL: Rust names one from this set, so an unknown one is the two
 * sides disagreeing, which is worth refusing rather than encoding something
 * nobody chose. */
static const struct nvenc_tuning *nvenc_tuning(const char *name) {
  for (size_t i = 0; i < sizeof NVENC_TUNINGS / sizeof *NVENC_TUNINGS; i++)
    if (!name || !strcmp(name, NVENC_TUNINGS[i].name)) return &NVENC_TUNINGS[i];
  return NULL;
}

/* The settings `tuning` opens NVENC with, whether the stream can't do without
 * each, and the order they are applied: the same list, in the same words, as
 * native/hwenc.c's `nvenc_settings`, so a test can hold the backends to one
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

/* --------------------------------------------------------------- the driver */

/* libnvidia-encode: the library NVIDIA's driver installs for NVENC, which is the
 * only thing on a Linux machine that knows the API this session is opened
 * through. The driver may be absent, so it is named rather than linked. */
struct nvenc_library {
  void *library;
  NV_ENCODE_API_FUNCTION_LIST functions;
  char why[192];
};

/* The library and its function table are made once for the process: they are the
 * same for every session, and loading them per encoder would be a race between
 * two encoders opening at once rather than anything useful. */
static struct nvenc_library library;
static pthread_once_t library_once = PTHREAD_ONCE_INIT;

/* The last thing dlopen or dlsym said went wrong, as a sentence: an absent
 * library's own reason is what tells a missing driver from a broken install. */
static const char *dl_reason(void) {
  const char *reason = dlerror();
  return reason ? reason : "the loader gave no reason";
}

static void load_library(void) {
  static const char *const names[] = {"libnvidia-encode.so.1", "libnvidia-encode.so", NULL};
  typedef NVENCSTATUS(NVENCAPI * create_instance)(NV_ENCODE_API_FUNCTION_LIST *);
  create_instance create;
  NVENCSTATUS status;
  for (int i = 0; names[i] && !library.library; i++)
    library.library = dlopen(names[i], RTLD_NOW | RTLD_LOCAL);
  if (!library.library) {
    snprintf(library.why, sizeof library.why,
             "libnvidia-encode.so.1 didn't load (%s): NVIDIA's driver is missing or older than "
             "520",
             dl_reason());
    return;
  }
  create = (create_instance)dlsym(library.library, "NvEncodeAPICreateInstance");
  if (!create) {
    snprintf(library.why, sizeof library.why,
             "libnvidia-encode.so.1 has no NvEncodeAPICreateInstance: NVIDIA's driver is older "
             "than this needs");
    return;
  }
  library.functions.version = NV_ENCODE_API_FUNCTION_LIST_VER;
  status = create(&library.functions);
  if (status != NV_ENC_SUCCESS)
    status_failure(library.why, sizeof library.why, "NvEncodeAPICreateInstance", status);
}

/* The driver's functions, or NULL with `why` saying what went wrong. */
static const NV_ENCODE_API_FUNCTION_LIST *driver(char *why, size_t size) {
  pthread_once(&library_once, load_library);
  if (!library.why[0]) return &library.functions;
  if (size) snprintf(why, size, "%s", library.why);
  return NULL;
}

/* ----------------------------------------------------------------- the CUDA */

/* The CUDA driver API, as far as an encoder needs it: making the caller's
 * context current, asking which card it is on and which context a buffer belongs
 * to, and putting a status into words. Nothing here allocates or copies — the
 * converter's file owns that side. */
struct cuda_api {
  CUresult (*cuInit)(unsigned);
  CUresult (*cuCtxPushCurrent)(CUcontext);
  CUresult (*cuCtxPopCurrent)(CUcontext *);
  CUresult (*cuCtxGetDevice)(CUdevice *);
  CUresult (*cuDeviceGetName)(char *, int, CUdevice);
  CUresult (*cuPointerGetAttribute)(void *, int, CUdeviceptr);
  CUresult (*cuGetErrorName)(CUresult, const char **);
};

struct cuda_library {
  void *library;
  struct cuda_api api;
  char why[192];
};

static struct cuda_library cuda;
static pthread_once_t cuda_once = PTHREAD_ONCE_INIT;

/* One of the driver's entry points, under the `_v2` name its signature moved to
 * after CUDA 3 or under the old spelling: cuda.h points the latter at the former,
 * and a driver may export either. */
static void *symbol(void *library, const char *name) {
  char versioned[64];
  void *found;
  if (!library) return NULL;
  snprintf(versioned, sizeof versioned, "%s_v2", name);
  found = dlsym(library, versioned);
  return found ? found : dlsym(library, name);
}

static void load_cuda(void) {
  static const char *const names[] = {"libcuda.so.1", "libcuda.so", NULL};
  for (int i = 0; names[i] && !cuda.library; i++)
    cuda.library = dlopen(names[i], RTLD_NOW | RTLD_LOCAL);
  if (!cuda.library) {
    snprintf(cuda.why, sizeof cuda.why,
             "libcuda.so.1 didn't load (%s): NVIDIA's driver is missing", dl_reason());
    return;
  }
  cuda.api.cuInit = (CUresult(*)(unsigned))symbol(cuda.library, "cuInit");
  cuda.api.cuCtxPushCurrent = (CUresult(*)(CUcontext))symbol(cuda.library, "cuCtxPushCurrent");
  cuda.api.cuCtxPopCurrent = (CUresult(*)(CUcontext *))symbol(cuda.library, "cuCtxPopCurrent");
  cuda.api.cuCtxGetDevice = (CUresult(*)(CUdevice *))symbol(cuda.library, "cuCtxGetDevice");
  cuda.api.cuDeviceGetName = (CUresult(*)(char *, int, CUdevice))symbol(cuda.library,
                                                                      "cuDeviceGetName");
  cuda.api.cuPointerGetAttribute = (CUresult(*)(void *, int, CUdeviceptr))symbol(
      cuda.library, "cuPointerGetAttribute");
  cuda.api.cuGetErrorName = (CUresult(*)(CUresult, const char **))symbol(cuda.library,
                                                                        "cuGetErrorName");
  if (!cuda.api.cuInit || !cuda.api.cuCtxPushCurrent || !cuda.api.cuCtxPopCurrent ||
      !cuda.api.cuCtxGetDevice || !cuda.api.cuDeviceGetName || !cuda.api.cuPointerGetAttribute ||
      !cuda.api.cuGetErrorName) {
    snprintf(cuda.why, sizeof cuda.why,
             "libcuda.so.1 hasn't the entry points a session is opened with: NVIDIA's driver is "
             "older than this needs");
    return;
  }
  /* The converter made a context already, so the driver has started, but stating
   * it here is what makes this file work from the driver's first call rather
   * than from the caller's, and it says so itself when it can't start at all. */
  CUresult result = cuda.api.cuInit(0);
  if (result != CUDA_SUCCESS) {
    const char *named = NULL;
    cuda.api.cuGetErrorName(result, &named);
    snprintf(cuda.why, sizeof cuda.why, "the CUDA driver wouldn't start: %s",
             named ? named : "an unknown CUDA error");
  }
}

/* The CUDA entry points, or NULL with `why` saying what went wrong. */
static const struct cuda_api *cuda_driver(char *why, size_t size) {
  pthread_once(&cuda_once, load_cuda);
  if (!cuda.why[0]) return &cuda.api;
  if (size) snprintf(why, size, "%s", cuda.why);
  return NULL;
}

/* One of CUDA's own failures, in the shape `nvenc_status` gives the encoder's:
 * the driver's name for it, which is what its documentation is indexed by. */
static const char *cuda_status(CUresult result) {
  const char *named = NULL;
  if (cuda.api.cuGetErrorName && cuda.api.cuGetErrorName(result, &named) == CUDA_SUCCESS && named)
    return named;
  return "an unknown CUDA error";
}

/* ------------------------------------------------------------------- encoder */

struct yapfast_cudaenc;

/* Letting go, declared before the open that can't finish calls it. */
void yapfast_cudaenc_free(struct yapfast_cudaenc *e);

struct yapfast_cudaenc {
  /* The context the picture belongs to, borrowed from the caller: pushed current
   * around every driver call and never made or destroyed here. It outlives the
   * encoder, since its buffer does. */
  CUcontext context;
  void *session;
  NV_ENC_OUTPUT_PTR bitstream;
  /* The caller's picture, registered and mapped for as long as the session is
   * open: the converter writes into that one buffer every frame, so a slot table
   * like native/d3d11enc.c's would hold one entry. A capture that handed back
   * several surfaces would want that table; this path hands back the same
   * address until the stream's size changes. */
  NV_ENC_REGISTERED_PTR registered;
  NV_ENC_INPUT_PTR mapped;
  void *nv12;
  unsigned pitch;
  /* What the encoder was opened with, kept so that a bitrate change can
   * reconfigure the same session instead of opening another. */
  NV_ENC_INITIALIZE_PARAMS init;
  NV_ENC_CONFIG settings;
  unsigned codec_id, width, height, fps, kbps;
  /* Which of the codecs the driver offered this session, as bits of codec ids:
   * AV1 needs an RTX 40 series or newer, and what a card refuses is worth saying
   * in the open's report rather than leaving to a status number. */
  unsigned supported;
  const struct nvenc_tuning *tuning;
  unsigned long long frames;
  /* Frames' pixels moved by the processor since this encoder opened: a count
   * the encode path keeps at zero, and says so. */
  unsigned long long copied;
  char error[192];
  char card[96];
  char codecs[64];
  char config[320];
  char tuning_text[96];
};

/* What went wrong, for Rust to log beside the frame it lost. */
const char *yapfast_cudaenc_error(struct yapfast_cudaenc *e) {
  if (!e) return "not opened";
  return e->error[0] ? e->error : "no error";
}

/* A sentence where the caller reads it, for the failures that happen before
 * there is an encoder to keep them in. */
static void put(char *buffer, size_t size, const char *format, ...) {
  va_list args;
  if (!size) return;
  va_start(args, format);
  vsnprintf(buffer, size, format, args);
  va_end(args);
}

/* Says why a call failed, in the encoder for the log and in the caller's buffer
 * for the error it reports: one sentence, written once. */
static void says(struct yapfast_cudaenc *e, char *error, size_t size, const char *format,
                 va_list args) {
  char sentence[192];
  vsnprintf(sentence, sizeof sentence, format, args);
  if (e) snprintf(e->error, sizeof e->error, "%s", sentence);
  if (size) snprintf(error, size, "%s", sentence);
}

/* Fails a call with a reason that travels to Rust, so the call sites that fail
 * read as one line. Every failure here is -1; the reason is what distinguishes
 * them. */
static long fail(struct yapfast_cudaenc *e, char *error, size_t size, const char *format, ...) {
  va_list args;
  va_start(args, format);
  says(e, error, size, format, args);
  va_end(args);
  return -1;
}

/* Makes the caller's context current for the length of one driver call. NVENC
 * resolves a CUDA input resource in the context that is current, and this encoder
 * may run on another thread than the one the converter pushed its context on, so
 * the context is pushed and popped around each call rather than left current
 * between them. Returns 0, or -1 with the reason in the caller's buffer. */
static int enter(struct yapfast_cudaenc *e, char *why, size_t size) {
  CUresult result = cuda.api.cuCtxPushCurrent(e->context);
  if (result == CUDA_SUCCESS) return 0;
  put(why, size, "making the CUDA context current failed: %s", cuda_status(result));
  return -1;
}

/* Gives the context up again. Pushed in pairs, so this doesn't wait to be asked
 * whether the push worked: a call that failed before one never gets here. */
static void leave(void) {
  CUcontext previous = NULL;
  cuda.api.cuCtxPopCurrent(&previous);
}

/* The card the caller's context is on, in its driver's own words: what the
 * share's report names, and the only way to tell two NVIDIA cards apart from
 * here. A card that won't name itself is still one to encode on, so this only
 * fails when the context isn't one the driver knows. */
static int name_card(struct yapfast_cudaenc *e, char *why, size_t size) {
  CUdevice device = 0;
  CUresult result = cuda.api.cuCtxGetDevice(&device);
  if (result != CUDA_SUCCESS) {
    put(why, size, "the CUDA context handed in isn't one the driver knows: %s",
        cuda_status(result));
    return -1;
  }
  if (cuda.api.cuDeviceGetName(e->card, (int)sizeof e->card, device) != CUDA_SUCCESS)
    snprintf(e->card, sizeof e->card, "%s", "an NVIDIA card");
  return 0;
}

/* Whether the buffer really is the one the converter wrote. A `cuMemAlloc`
 * pointer is only memory in the context that allocated it, so a buffer from
 * another device's context is refused here, by name, rather than by the status
 * nvEncRegisterResource gives a pointer it can't resolve. A driver that won't
 * answer the question leaves the answer to that registration. */
static int same_context(struct yapfast_cudaenc *e, char *why, size_t size) {
  CUcontext owner = NULL;
  CUresult result =
      cuda.api.cuPointerGetAttribute(&owner, CU_POINTER_ATTRIBUTE_CONTEXT,
                                     (CUdeviceptr)(uintptr_t)e->nv12);
  if (result != CUDA_SUCCESS || !owner || owner == e->context) return 0;
  put(why, size,
      "the picture is memory of another CUDA context than the one handed in: a cuMemAlloc "
      "pointer is only readable in the context that allocated it");
  return -1;
}

/* The codecs this session's driver will encode, as bits of codec ids and in
 * words: what the stream can offer its viewers, read from the card that would
 * encode them rather than from a list of cards. */
static void note_codecs(struct yapfast_cudaenc *e) {
  GUID found[16];
  uint32_t count = 0, given = 0;
  size_t at = 0;
  e->codecs[0] = 0;
  e->supported = 0;
  if (library.functions.nvEncGetEncodeGUIDCount(e->session, &count) != NV_ENC_SUCCESS || !count ||
      library.functions.nvEncGetEncodeGUIDs(e->session, found, 16, &given) != NV_ENC_SUCCESS)
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

/* The settings this encoder would open NVENC with, and whether the stream can't
 * do without each, into `out`. The stream names no tuning across this shim — the
 * shipped row is the one — so the row is the same one native/hwenc.c and
 * native/d3d11enc.c open with, in the same words and first, so a test holds the
 * three to one answer by comparing up to the profile. What follows it is what
 * the open derives from the stream: the profile, the keyframe interval, and the
 * cap with the buffer it is spent over. No driver is touched. */
void yapfast_cudaenc_tuning_settings(unsigned codec, unsigned fps, unsigned kbps, char *out,
                                     size_t size) {
  const struct nvenc_tuning *tuning;
  char asked[192];
  if (!size) return;
  if (codec >= CODEC_COUNT) {
    put(out, size, "codec %u isn't one this encoder has", codec);
    return;
  }
  tuning = nvenc_tuning(NULL);
  tuning_settings(tuning, asked, sizeof asked);
  /* Every one of the three is encoded at its main profile, as
   * native/d3d11enc.c's profileGUIDs say: one statement for all of them. */
  put(out, size, "%s profile=main(required) gop=%u(3s) bitrate=%ukbit/s cap=%ukbit/s "
                 "buffer=%ukbit(0.5s)",
      asked, fps * 3, kbps, kbps, kbps * 500);
}

/* The settings the encoder is opened with, in NVENC's parameters: the row's
 * preset and tuning, a frame out for each frame in, an IDR every three seconds
 * and on demand, the cap spent as the row says over half a second, and the colour
 * every frame is declared in. These are native/d3d11enc.c's choices, parameter
 * for parameter: they are the session's, and a session is the same one whichever
 * kind of memory its picture is in. */
static void configure(struct yapfast_cudaenc *e, const NV_ENC_PRESET_CONFIG *preset) {
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

  /* The per-codec settings, in the one layout of the three the union holds room
   * for: `encodeCodecConfig` is a union of H.264's, HEVC's and AV1's
   * configurations, so writing all three would leave the last one standing in the
   * others' bytes, and the driver reading a codec's settings out of another
   * codec's layout is exactly the invalid parameter it reports. One codec is
   * chosen per instance, so one layout is written.
   *
   * The colour is the same in all three, and the same claim native/cudaconvert.c
   * makes with its kernels and native/hwenc.c makes to FFmpeg: BT.601's
   * primaries, transfer and matrix over its limited range. NVENC converts nothing
   * here — the picture arrives as NV12 — so this only declares what those pixels
   * already are, and a kernel that changed its matrix would have to change this
   * with it. */
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

/* Checks what the open is given, before the driver is loaded or a session
 * attempted: a caller that got one of these wrong is told without a card having
 * been asked. 0, or -1 with `why` set. */
static int argcheck(unsigned codec_id, unsigned width, unsigned height, unsigned fps,
                    unsigned kbps, const void *nv12, unsigned pitch, const void *context,
                    char *why, size_t size) {
  if (codec_id >= CODEC_COUNT) {
    put(why, size, "%u isn't a codec this encoder has", codec_id);
    return -1;
  }
  if (!width || !height || (width & 1) || (height & 1)) {
    put(why, size, "%ux%u isn't an even size NVENC encodes", width, height);
    return -1;
  }
  if (!fps) {
    put(why, size, "no frames a second to encode at");
    return -1;
  }
  if (!kbps) {
    put(why, size, "no bitrate to aim for");
    return -1;
  }
  if (!nv12) {
    put(why, size, "no picture to encode: the caller's NV12 buffer isn't there");
    return -1;
  }
  /* The pitch is the row stride the converter allocated at, and the driver reads
   * luma rows of that many bytes with the interleaved chroma after them: a pitch
   * narrower than the picture shears it sideways, and the driver takes only a
   * multiple of four. */
  if (pitch < width || (pitch & 3)) {
    put(why, size,
        "a pitch of %u bytes for a %u wide picture: NVENC reads rows of at least the width, in a "
        "multiple of four",
        pitch, width);
    return -1;
  }
  if (!context) {
    put(why, size,
        "no CUDA context to open the session on: the picture's memory is only readable in the "
        "context that allocated it");
    return -1;
  }
  return 0;
}

/* Opens the NVENC session on `e->context` and registers the caller's picture on
 * it: the codecs this driver offers, the settings the tuning row names, the
 * session, the bitstream buffer every frame is written into, and the input
 * resource the driver reads. Everything here runs with the context current, and
 * nothing below makes or destroys a context. 0, or -1 with `why` set. */
static int start_session(struct yapfast_cudaenc *e, char *why, size_t size) {
  const NV_ENCODE_API_FUNCTION_LIST *api = &library.functions;
  NV_ENC_OPEN_ENCODE_SESSION_EX_PARAMS open;
  NV_ENC_REGISTER_RESOURCE resource;
  NV_ENC_MAP_INPUT_RESOURCE map;
  NV_ENC_CREATE_BITSTREAM_BUFFER bitstream;
  NV_ENC_PRESET_CONFIG preset;
  NVENCSTATUS status;

  memset(&open, 0, sizeof open);
  open.version = NV_ENC_OPEN_ENCODE_SESSION_EX_PARAMS_VER;
  open.deviceType = NV_ENC_DEVICE_TYPE_CUDA;
  /* The context, not a device ordinal: every CUDA pointer's meaning is this
   * context's, so the session has to be opened on the one the picture's memory
   * was allocated in. */
  open.device = e->context;
  open.apiVersion = NVENCAPI_VERSION;
  status = api->nvEncOpenEncodeSessionEx(&open, &e->session);
  if (status == NV_ENC_ERR_UNSUPPORTED_DEVICE || status == NV_ENC_ERR_NO_ENCODE_DEVICE) {
    put(why, size, "%s, the card the picture is on, has no NVENC to encode with",
        e->card[0] ? e->card : "this device");
    snprintf(e->error, sizeof e->error, "%s", why);
    return -1;
  }
  if (status != NV_ENC_SUCCESS) {
    fail(e, why, size, "nvEncOpenEncodeSessionEx on the CUDA context failed: %s (%d)",
         nvenc_status(status), (int)status);
    return -1;
  }
  /* The driver is asked which codecs it encodes before one is opened: a card
   * whose driver has no encoder for the codec asked for says so in a sentence,
   * rather than failing later with a status nobody can act on. */
  note_codecs(e);
  if (e->supported && !(e->supported & (1u << e->codec_id))) {
    fail(e, why, size, "this driver has no %s encoder (it has %s)%s", CODECS[e->codec_id].name,
         e->codecs, e->codec_id == CODEC_AV1 ? ", and AV1 needs an RTX 40 series or newer" : "");
    return -1;
  }

  memset(&preset, 0, sizeof preset);
  preset.version = NV_ENC_PRESET_CONFIG_VER;
  preset.presetCfg.version = NV_ENC_CONFIG_VER;
  status = api->nvEncGetEncodePresetConfigEx(e->session, *CODECS[e->codec_id].guid,
                                            *e->tuning->preset_guid, e->tuning->tune, &preset);
  if (status != NV_ENC_SUCCESS) {
    fail(e, why, size, "nvEncGetEncodePresetConfigEx failed: %s (%d)", nvenc_status(status),
         (int)status);
    return -1;
  }
  configure(e, &preset);

  memset(&e->init, 0, sizeof e->init);
  e->init.version = NV_ENC_INITIALIZE_PARAMS_VER;
  e->init.encodeGUID = *CODECS[e->codec_id].guid;
  e->init.presetGUID = *e->tuning->preset_guid;
  e->init.tuningInfo = e->tuning->tune;
  e->init.encodeWidth = e->width;
  e->init.encodeHeight = e->height;
  e->init.darWidth = e->width;
  e->init.darHeight = e->height;
  e->init.frameRateNum = e->fps;
  e->init.frameRateDen = 1;
  /* Synchronous: a frame in is a packet out on the same call, so the capture
   * loop never holds a picture it can't account for. */
  e->init.enableEncodeAsync = 0;
  e->init.enablePTD = 1;
  e->init.maxEncodeWidth = e->width;
  e->init.maxEncodeHeight = e->height;
  e->init.encodeConfig = &e->settings;
  status = api->nvEncInitializeEncoder(e->session, &e->init);
  if (status != NV_ENC_SUCCESS) {
    fail(e, why, size, "nvEncInitializeEncoder failed: %s (%d)", nvenc_status(status),
         (int)status);
    return -1;
  }

  memset(&bitstream, 0, sizeof bitstream);
  bitstream.version = NV_ENC_CREATE_BITSTREAM_BUFFER_VER;
  bitstream.size = BITSTREAM_BYTES;
  bitstream.memoryHeap = NV_ENC_MEMORY_HEAP_AUTOSELECT;
  status = api->nvEncCreateBitstreamBuffer(e->session, &bitstream);
  if (status != NV_ENC_SUCCESS) {
    fail(e, why, size, "nvEncCreateBitstreamBuffer failed: %s (%d)", nvenc_status(status),
         (int)status);
    return -1;
  }
  e->bitstream = bitstream.bitstreamBuffer;

  /* The caller's picture, registered and mapped once: the converter fills that
   * same buffer every frame, so this is the work native/d3d11enc.c does per
   * surface of a capture's ring rather than per frame. */
  memset(&resource, 0, sizeof resource);
  resource.version = NV_ENC_REGISTER_RESOURCE_VER;
  resource.resourceType = NV_ENC_INPUT_RESOURCE_TYPE_CUDADEVICEPTR;
  resource.width = e->width;
  resource.height = e->height;
  /* The buffer's own row stride: NVENC is told the geometry, unlike a DirectX
   * surface whose layout it reads for itself. */
  resource.pitch = e->pitch;
  resource.subResourceIndex = 0;
  resource.resourceToRegister = e->nv12;
  resource.bufferFormat = NV_ENC_BUFFER_FORMAT_NV12;
  resource.bufferUsage = NV_ENC_INPUT_IMAGE;
  status = api->nvEncRegisterResource(e->session, &resource);
  if (status != NV_ENC_SUCCESS) {
    fail(e, why, size, "nvEncRegisterResource on the caller's NV12 buffer failed: %s (%d)",
         nvenc_status(status), (int)status);
    return -1;
  }
  e->registered = resource.registeredResource;
  memset(&map, 0, sizeof map);
  map.version = NV_ENC_MAP_INPUT_RESOURCE_VER;
  map.registeredResource = e->registered;
  status = api->nvEncMapInputResource(e->session, &map);
  if (status != NV_ENC_SUCCESS) {
    fail(e, why, size, "nvEncMapInputResource on the caller's NV12 buffer failed: %s (%d)",
         nvenc_status(status), (int)status);
    return -1;
  }
  e->mapped = map.mappedResource;
  return 0;
}

/* Says what opened, in the words a log quotes: the card the picture is on, what
 * arrived at the session and what it comes out as, which of the codecs the driver
 * offered, and the backend's name. Written once at the open, since none of it
 * changes while the stream runs. */
static void describe_open(struct yapfast_cudaenc *e) {
  snprintf(e->config, sizeof e->config,
           "%s, NV12 on the card in, NV12 4:2:0 out, bt601 limited range, %u bytes a row at "
           "%u kbit/s, nvenc/cuda, driver encodes %s",
           e->card, e->pitch, e->kbps, e->codecs[0] ? e->codecs : "nothing it named");
  snprintf(e->tuning_text, sizeof e->tuning_text,
           "%s preset, %s rate control, keyframe every %u frames", e->tuning->preset,
           e->tuning->rc, e->settings.gopLength);
}

struct yapfast_cudaenc *yapfast_cudaenc_new(unsigned codec, unsigned width, unsigned height,
                                            unsigned fps, unsigned kbps, void *nv12,
                                            unsigned pitch, void *cuda_context, char *error,
                                            size_t size) {
  struct yapfast_cudaenc *e;
  char why[192] = "";
  int entered = 0;

  if (size) error[0] = 0;
  if (argcheck(codec, width, height, fps, kbps, nv12, pitch, cuda_context, error, size))
    return NULL;
  const struct nvenc_tuning *tuning = nvenc_tuning(NULL);
  if (!driver(why, sizeof why)) {
    put(error, size, "%s", why);
    return NULL;
  }
  if (!cuda_driver(why, sizeof why)) {
    put(error, size, "%s", why);
    return NULL;
  }
  e = calloc(1, sizeof *e);
  if (!e) {
    put(error, size, "no memory for the encoder");
    return NULL;
  }
  e->codec_id = codec;
  e->width = width;
  e->height = height;
  e->fps = fps;
  e->kbps = kbps;
  e->nv12 = nv12;
  e->pitch = pitch;
  /* Borrowed, as a device handed to native/d3d11enc.c's open is: the converter
   * made this context and frees the buffer it holds, so this pushes it current
   * and never destroys it. */
  e->context = (CUcontext)cuda_context;
  e->tuning = tuning;

  if (enter(e, why, sizeof why)) goto failed;
  entered = 1;
  if (name_card(e, why, sizeof why)) goto failed;
  /* Asked before a session is opened, so that a picture from another context is
   * a sentence about the picture rather than a status from the driver. */
  if (same_context(e, why, sizeof why)) goto failed;
  if (start_session(e, why, sizeof why)) goto failed;
  describe_open(e);
  leave();
  return e;

failed:
  /* Only what was pushed is popped: a context that wouldn't come current must
   * not leave the caller's current one popped out from under it. */
  if (entered) leave();
  put(error, size, "%s", why);
  yapfast_cudaenc_free(e);
  return NULL;
}

/* Encodes the picture now in the buffer this was opened on, whose time is `pts`
 * in 90 kHz units — the stream's own clock, which labels the frame's packets
 * too — and copies the access unit into `out`, which holds `capacity` bytes.
 * `keyframe` asks for an IDR on this frame, which is what a viewer who joins, or
 * whose picture broke, waits for. Returns the access unit's length, 0 when this
 * call produced no unit (the driver may hand one back a frame late), or -1 with
 * the reason in `error` and `yapfast_cudaenc_error`. The copy out of the
 * driver's locked bitstream is the only copy this path makes: the picture's
 * pixels stay where the converter put them. */
long yapfast_cudaenc_encode(struct yapfast_cudaenc *e, unsigned long long pts, int keyframe,
                            unsigned char *out, size_t capacity, char *error, size_t size) {
  NV_ENC_PIC_PARAMS picture;
  NV_ENC_LOCK_BITSTREAM locked;
  NVENCSTATUS status;
  char why[192] = "";

  if (!e) {
    put(error, size, "no CUDA encoder");
    return -1;
  }
  if (!out || !capacity)
    return fail(e, error, size, "the encoder was given nowhere to put a frame");
  if (enter(e, why, sizeof why)) return fail(e, error, size, "%s", why);

  memset(&picture, 0, sizeof picture);
  picture.version = NV_ENC_PIC_PARAMS_VER;
  picture.inputWidth = e->width;
  picture.inputHeight = e->height;
  /* The buffer's stride, as it was registered with: a DirectX surface carries its
   * own, and this one is told. */
  picture.inputPitch = e->pitch;
  picture.inputBuffer = e->mapped;
  picture.outputBitstream = e->bitstream;
  picture.bufferFmt = NV_ENC_BUFFER_FORMAT_NV12;
  picture.pictureStruct = NV_ENC_PIC_STRUCT_FRAME;
  picture.frameIdx = (uint32_t)e->frames;
  picture.inputTimeStamp = (uint64_t)pts;
  picture.pictureType = NV_ENC_PIC_TYPE_UNKNOWN;
  /* The same flag native/d3d11enc.c forces for every codec it encodes, so a
   * viewer who joins gets a keyframe at once from either backend. */
  if (keyframe && e->tuning->forced_idr) picture.encodePicFlags = NV_ENC_PIC_FLAG_FORCEIDR;
  status = library.functions.nvEncEncodePicture(e->session, &picture);
  /* Synchronous, so this is rare: a driver that is still filling its pipeline
   * says the picture isn't out yet, which is the empty answer rather than a lost
   * frame. */
  if (status == NV_ENC_ERR_NEED_MORE_INPUT) {
    leave();
    return 0;
  }
  if (status != NV_ENC_SUCCESS) {
    long answer = fail(e, error, size, "nvEncEncodePicture on frame %llu failed: %s (%d)",
                       e->frames, nvenc_status(status), (int)status);
    leave();
    return answer;
  }
  e->frames++;

  memset(&locked, 0, sizeof locked);
  locked.version = NV_ENC_LOCK_BITSTREAM_VER;
  locked.outputBitstream = e->bitstream;
  /* Waits for the frame rather than polling: the encode is synchronous, so this
   * returns once and the packet is whole. */
  locked.doNotWait = 0;
  status = library.functions.nvEncLockBitstream(e->session, &locked);
  if (status != NV_ENC_SUCCESS) {
    long answer = fail(e, error, size, "nvEncLockBitstream failed: %s (%d)", nvenc_status(status),
                       (int)status);
    leave();
    return answer;
  }
  size_t length = locked.bitstreamSizeInBytes;
  long answer = 0;
  if (!length) {
    /* Nothing came out of this frame and nothing went wrong: the answer the
     * caller's loop expects for a frame the driver is holding. */
  } else if (length > capacity) {
    /* The picture is encoded and the packet exists, so re-encoding it into a
     * larger buffer would send a frame referencing one the receiver never got.
     * The unit is the caller's to hold, and the size it needed is what the log
     * says so the next buffer is big enough. */
    answer = fail(e, error, size, "the frame is %zu bytes and the buffer holds %zu", length,
                  capacity);
  } else {
    memcpy(out, locked.bitstreamBufferPtr, length);
    answer = (long)length;
  }
  library.functions.nvEncUnlockBitstream(e->session, locked.outputBitstream);
  leave();
  return answer;
}

/* Aims for `kbps` from the next frame on, without a keyframe: the session is
 * reconfigured rather than reopened, so the frames a viewer is watching stay
 * references and the stream's picture doesn't blink. Returns 0, or -1 with the
 * driver's reason. */
int yapfast_cudaenc_set_bitrate(struct yapfast_cudaenc *e, unsigned kbps, char *error,
                                size_t size) {
  NV_ENC_RECONFIGURE_PARAMS reconfigure;
  NVENCSTATUS status;
  char why[192] = "";
  if (!e) {
    put(error, size, "no CUDA encoder");
    return -1;
  }
  e->settings.rcParams.averageBitRate = kbps * 1000;
  e->settings.rcParams.maxBitRate = kbps * 1000;
  e->settings.rcParams.vbvBufferSize = kbps * 500;
  e->settings.rcParams.vbvInitialDelay = e->settings.rcParams.vbvBufferSize;
  if (enter(e, why, sizeof why)) return (int)fail(e, error, size, "%s", why);
  memset(&reconfigure, 0, sizeof reconfigure);
  reconfigure.version = NV_ENC_RECONFIGURE_PARAMS_VER;
  reconfigure.reInitEncodeParams = e->init;
  /* The same session and the same picture: no keyframe is inserted, and the
   * frames the encoder has already sent stay references. */
  reconfigure.resetEncoder = 0;
  reconfigure.forceIDR = 0;
  status = library.functions.nvEncReconfigureEncoder(e->session, &reconfigure);
  if (status != NV_ENC_SUCCESS) {
    long answer = fail(e, error, size, "nvEncReconfigureEncoder to %u kbit/s failed: %s (%d)",
                       kbps, nvenc_status(status), (int)status);
    leave();
    return (int)answer;
  }
  e->kbps = kbps;
  /* The report names the rate the session is at now, and a share that changed it
   * twice quotes the last one. */
  describe_open(e);
  leave();
  return 0;
}

/* Frames' pixels the processor moved: zero, for as long as the picture goes to
 * the encoder on the card, which is what this backend is for and the one thing a
 * careless change would take away without any frame looking different. */
unsigned long long yapfast_cudaenc_copied(struct yapfast_cudaenc *e) {
  return e ? e->copied : 0;
}

/* The tuning it was opened with, as native/hwenc.c and native/d3d11enc.c report
 * the same thing, so a stream's picture is compared with the encoder that made
 * it. */
void yapfast_cudaenc_tuning(struct yapfast_cudaenc *e, char *out, size_t size) {
  if (size) snprintf(out, size, "%s", e ? e->tuning_text : "");
}

/* What this encoder took, in the words a log and a measurement quote: the card it
 * runs on, what arrived at the session, and which codecs the driver offered. */
void yapfast_cudaenc_config(struct yapfast_cudaenc *e, char *out, size_t size) {
  if (size) snprintf(out, size, "%s", e ? e->config : "");
}

void yapfast_cudaenc_free(struct yapfast_cudaenc *e) {
  int current;
  if (!e) return;
  /* The driver's own memory is given back with its context current, and the
   * context belongs to the caller: pushed here and popped again, never destroyed.
   * A context that won't come current leaves nothing of the driver's to free,
   * since the open that made the session made it on that same context. */
  current = e->context && cuda.api.cuCtxPushCurrent &&
            cuda.api.cuCtxPushCurrent(e->context) == CUDA_SUCCESS;
  if (e->session) {
    if (e->mapped) library.functions.nvEncUnmapInputResource(e->session, e->mapped);
    if (e->registered) library.functions.nvEncUnregisterResource(e->session, e->registered);
    if (e->bitstream) library.functions.nvEncDestroyBitstreamBuffer(e->session, e->bitstream);
    library.functions.nvEncDestroyEncoder(e->session);
  }
  if (current) leave();
  /* The picture is the converter's, and the context is the caller's: neither is
   * freed or destroyed here. */
  free(e);
}
