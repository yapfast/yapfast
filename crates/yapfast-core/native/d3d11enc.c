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
 * reads the real DXGI format and refuses a typeless texture). A capture source
 * and an encoder therefore have to share one D3D11 device, and this file has two
 * ways to get one: `yapfast_d3d11enc_new` makes a device on the adapter its LUID
 * names, as it always did, and `yapfast_d3d11enc_new_on` opens on a device the
 * caller already has — a capture source's — so that a captured texture is the
 * encoder's texture and a stream costs one device rather than two.
 *
 * Ownership of that device is a rule, not a guess, because COM counts nothing
 * for us: whoever calls D3D11CreateDevice owns the device and is the one that
 * releases it. An encoder here that made its device releases it in
 * `yapfast_d3d11enc_free`; an encoder opened on a device it was handed does not
 * AddRef it and does not release it, and the caller keeps it alive for at least
 * as long as the encoder — which is what a capture source that outlives the
 * encoder, or an encoder that outlives the capture, has to arrange. Nothing of
 * the device's is written to here, so two encoders opened on one device are
 * fine; the immediate context is got and released by each, so each also holds a
 * reference of its own to that.
 *
 * The settings are native/hwenc.c's `NVENC_TUNINGS` row for the tuning asked
 * for, said again here as NVENC's own parameters rather than as FFmpeg's
 * options: low-latency tuning, no B-frames, a frame out for each frame in, an
 * IDR on demand, and capped VBR over half a second's buffer. A test holds the
 * two tables to one answer (`yapfast_d3d11enc_tuning_settings`). */
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <d3d11.h>
/* D3D11.3's shader resource views, which are the only ones that can name a
 * plane of a planar surface: how a shader reads NV12's chroma. */
#include <d3d11_3.h>
/* D3D11.4's fence, which is what a picture shared with the UI's window is drawn
 * behind: the value that says the copy is finished, waited on over there. */
#include <d3d11_4.h>
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

/* The most textures a session may remember, by (texture, subresource): a
 * capture source hands back a few surfaces in rotation rather than one, and
 * registering and mapping each of them again on every frame would cost more of
 * the processor than the encode does. Four covers the ring a desktop
 * duplicator keeps, and a session is opened with how many of them it holds —
 * four for one encoding a stream, and one for one that only reads a preview,
 * because every surface held is a full-size one the capture can't fill while
 * it is. */
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
 * and games landed — and the only measured row, since p7 buys 0.18 dB for half
 * again the wall time (docs/measurements/README.md); `tune` is the low-latency
 * one, since the picture is one somebody is waiting for; `rc` is how the cap is
 * spent, `vbr` leaving a calm screen below it rather than padding it out with
 * bits nobody looks at; `zerolatency` and `delay` decide whether NVENC may hold
 * a frame back, and no row does, because a held frame is delay in every viewer's
 * picture; `forced_idr` is what gives a viewer who joins, or whose picture
 * broke, a keyframe at once instead of waiting out the interval. */
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

/* The D3D11 device the session encodes from, on exactly the adapter its LUID
 * names. Which adapter that is was decided in Rust, from the GPU setting, and
 * arrives here as that adapter's LUID: a machine with integrated graphics
 * presents more than one adapter, and the device has to be the one the choice
 * was made on rather than the first that looks like a card. A LUID is DXGI's own
 * name for an adapter, the same across enumerations and runs. */
static int open_device(LUID want, IDXGIAdapter1 **adapter_out, ID3D11Device **device_out,
                       ID3D11DeviceContext **context_out, char *why, size_t size) {
  IDXGIFactory1 *factory = NULL;
  IDXGIAdapter1 *chosen = NULL, *each = NULL;
  DXGI_ADAPTER_DESC1 found;
  HRESULT status = CreateDXGIFactory1(&IID_IDXGIFactory1, (void **)&factory);
  /* Every adapter is named in the failure, not just the fact that the wanted
   * one wasn't found: a machine whose card has gone away says which adapters it
   * does have, rather than leaving it to be guessed from the hardware. */
  char seen[192] = "";
  size_t used = 0;
  memset(&found, 0, sizeof found);
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
    if (!chosen && desc.AdapterLuid.LowPart == want.LowPart &&
        desc.AdapterLuid.HighPart == want.HighPart) {
      chosen = each;
      found = desc;
    } else {
      each->lpVtbl->Release(each);
    }
    each = NULL;
  }
  factory->lpVtbl->Release(factory);
  if (!chosen) {
    if (size)
      snprintf(why, size,
               "the adapter Yapfast chose (LUID %08lX:%08lX) isn't on this machine; it has %s",
               (unsigned long)want.HighPart, (unsigned long)want.LowPart,
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
    if (size)
      snprintf(why, size, "D3D11CreateDevice on %ls failed (0x%08lX)", found.Description,
               (unsigned long)status);
    chosen->lpVtbl->Release(chosen);
    return -1;
  }
  InterlockedIncrement(&devices_created);
  *adapter_out = chosen;
  return 0;
}

/* Told below where the rest of the failures are, and used before there because
 * the first thing an open does with a borrowed device is ask DXGI about it. */
static void hr_failure(char *error, size_t size, const char *what, HRESULT status);

/* The adapter a device was made on, and what NVENC needs of it: a session only
 * exists on NVIDIA's cards, so a device made on anything else is refused here in
 * the words a caller can act on rather than by a status number from
 * nvEncOpenEncodeSessionEx. The adapter is asked of the device rather than
 * searched for by LUID, since the device is the thing that has to be right and a
 * LUID that matched a different object would say nothing about it. */
static int adapter_of(ID3D11Device *device, IDXGIAdapter1 **adapter_out, DXGI_ADAPTER_DESC1 *desc,
                      char *why, size_t size) {
  IDXGIDevice *dxgi = NULL;
  IDXGIAdapter *adapter = NULL;
  IDXGIAdapter1 *found = NULL;
  HRESULT status = device->lpVtbl->QueryInterface(device, &IID_IDXGIDevice, (void **)&dxgi);
  if (FAILED(status)) {
    hr_failure(why, size, "QueryInterface(IDXGIDevice) on the device handed in", status);
    return -1;
  }
  status = dxgi->lpVtbl->GetAdapter(dxgi, &adapter);
  dxgi->lpVtbl->Release(dxgi);
  if (FAILED(status)) {
    hr_failure(why, size, "IDXGIDevice::GetAdapter on the device handed in", status);
    return -1;
  }
  status = adapter->lpVtbl->QueryInterface(adapter, &IID_IDXGIAdapter1, (void **)&found);
  adapter->lpVtbl->Release(adapter);
  if (FAILED(status)) {
    hr_failure(why, size, "QueryInterface(IDXGIAdapter1) on the device's adapter", status);
    return -1;
  }
  memset(desc, 0, sizeof *desc);
  status = found->lpVtbl->GetDesc1(found, desc);
  if (FAILED(status)) {
    found->lpVtbl->Release(found);
    hr_failure(why, size, "IDXGIAdapter1::GetDesc1 on the device's adapter", status);
    return -1;
  }
  if (desc->VendorId != 0x10DE) {
    if (size)
      snprintf(why, size,
               "no NVENC on %ls (0x%04lX), the adapter the device handed in was made on: only "
               "NVIDIA's cards carry one",
               desc->Description, (unsigned long)desc->VendorId);
    found->lpVtbl->Release(found);
    return -1;
  }
  *adapter_out = found;
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
/* One of the caller's textures as a scaler reads it: the views either scaler
 * makes of a surface, kept for as long as the same surface keeps coming back.
 * Both are made on first use and released with the entry, in the same
 * least-recently-used way as the encoder's slots — a capture source hands back
 * a few surfaces in rotation, and a view made per frame would be exactly the
 * per-frame allocation this file exists to avoid. */
struct input {
  ID3D11Texture2D *texture;
  unsigned subresource;
  DXGI_FORMAT format;
  ID3D11VideoProcessorInputView *video;
  /* A shader reads a picture's planes: one for BGRA or RGBA, two for NV12,
   * whose luma and chroma are separate planes of the one surface. */
  ID3D11ShaderResourceView *planes[2];
  unsigned long long used;
};
/* The scaler, and the textures it reads back through. Everything here is made
 * on the first frame that needs it and kept while the size it was made for
 * comes back, so a stream capturing one screen pays for it once rather than per
 * frame: one destination texture, one staging texture, one video processor, and
 * one input view per surface the capture rotates through. */
struct scale {
  /* The scaled picture, as the encoder reads it, and the size it was made for. */
  ID3D11Texture2D *picture;
  unsigned width, height;
  /* The video processor, when this device has one that takes these formats. The
   * enumerator and the processor are made for one pair of sizes and remade when
   * a capture's size changes, which is when the screen does and not when a frame
   * does. */
  ID3D11VideoDevice *video;
  ID3D11VideoContext *video_context;
  ID3D11VideoProcessorEnumerator *enumerator;
  ID3D11VideoProcessor *processor;
  ID3D11VideoProcessorOutputView *output;
  ID3D11RenderTargetView *target;
  unsigned source_width, source_height;
  /* The shader that stands in when the driver has no video processor, or none
   * that takes these formats: one program for a colour picture and one for a
   * planar one, since what a shader is given is known when it is compiled and
   * not per frame. */
  ID3D11VertexShader *vertex;
  ID3D11PixelShader *rgb;
  ID3D11PixelShader *planar;
  ID3D11SamplerState *sampler;
  ID3D11Buffer *sizes;
  /* D3D11.3's device, which is what makes a view of one plane of a planar
   * surface: asked for the first time a shader is handed NV12. */
  ID3D11Device3 *planes_of;
  /* The staging texture a readback maps, and the size it was made for. */
  ID3D11Texture2D *staging;
  unsigned staging_width, staging_height;
  struct input inputs[SLOTS];
  /* How many destination and staging textures this has made, so that "the
   * hundredth readback reused the first one's" is a number rather than a hope. */
  unsigned pictures, stagings;
  /* The clock the input views turn on: a preview that never encodes would leave
   * them all looking equally old, and every frame would remake the views of a
   * surface that hadn't changed. */
  unsigned long long scaled;
  /* Which filter scaled the last picture, in words, for a log to quote. */
  const char *filter;
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
  /* Whether this encoder made the device: it releases only what it made, and a
   * device handed to `yapfast_d3d11enc_new_on` is borrowed for as long as the
   * caller keeps it alive. */
  int owns_device;
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
  /* How many of a capture's surfaces this session keeps registered and
   * mapped, and how many views of them the scaler keeps: `SLOTS` at most, and
   * one for a session that reads a preview rather than encoding a stream. */
  unsigned held;
  struct slot slots[SLOTS];
  /* What a capture of another size goes through to reach the encoder, and what
   * a preview is read back through: see `struct scale`. */
  struct scale scaler;
  char error[192];
  char config[128];
  char tuning_text[96];
  char codecs[64];
  /* The access unit the last frame produced, kept until Rust copies it into the
   * buffer it holds: the driver's bitstream is mapped for the length of one
   * call, so a unit too big for that buffer is copied here and handed over once
   * rather than lost, which would have meant encoding the picture a second time
   * against a reference the receiver never got. Reused from frame to frame, so
   * it holds one frame's worth rather than a stream's. */
  unsigned char *staged;
  size_t staged_capacity;
  size_t staged_size;
};

/* What went wrong, for Rust to log beside the frame it lost. */
void yapfast_d3d11enc_error(struct yapfast_d3d11enc *e, char *error, size_t size) {
  if (!size) return;
  snprintf(error, size, "%s", e && e->error[0] ? e->error : "no error");
}

/* Says why a call failed, in the encoder's own words: what Rust logs beside the
 * frame it lost. */
static void says(struct yapfast_d3d11enc *e, const char *format, va_list args) {
  if (e) vsnprintf(e->error, sizeof e->error, format, args);
}
/* Fails a call with a reason that travels to Rust, and returns `value`, so the
 * call sites that fail read as one line. */
static long fail(struct yapfast_d3d11enc *e, long value, const char *format, ...) {
  va_list args;
  va_start(args, format);
  says(e, format, args);
  va_end(args);
  return value;
}
/* The same for the calls that answer with a pointer: a texture this couldn't
 * produce and one it hasn't produced are the same nothing to a caller, and the
 * reason is what tells them apart. */
static void *nothing(struct yapfast_d3d11enc *e, const char *format, ...) {
  va_list args;
  va_start(args, format);
  says(e, format, args);
  va_end(args);
  return NULL;
}
/* One of Direct3D's own failures, in the shape `status_failure` gives the
 * driver's: what failed, and the code that says which way. */
static void hr_failure(char *error, size_t size, const char *what, HRESULT status) {
  if (size) snprintf(error, size, "%s failed (0x%08lX)", what, (unsigned long)status);
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
/* Which filter scaled the last picture, in words: the video processor, the pixel
 * shader that stands in when the driver has none, or a copy where the two sizes
 * were already the same. Empty until something is scaled, and worth a line in a
 * log beside the numbers, since a driver that declines the processor is the
 * difference between a preview that is nearly free and one that is cheap. */
void yapfast_d3d11enc_filter(struct yapfast_d3d11enc *e, char *filter, size_t size) {
  if (size) snprintf(filter, size, "%s", e && e->scaler.filter ? e->scaler.filter : "");
}
/* How many destination textures the scaler has made since this encoder opened:
 * one, however many frames went through it, and one per new size a capture
 * asks for. A number that climbs every frame is a size that keeps changing. */
unsigned yapfast_d3d11enc_scaled_textures(struct yapfast_d3d11enc *e) {
  return e ? e->scaler.pictures : 0;
}
/* How many staging textures a readback has made: one, however many previews
 * were read through it. This is the count a hundred readbacks leave alone. */
unsigned yapfast_d3d11enc_staging_textures(struct yapfast_d3d11enc *e) {
  return e ? e->scaler.stagings : 0;
}

/* The device the caller's textures have to live on, as an ID3D11Device: a
 * capture source creates its surfaces on it or opens on it — see
 * `yapfast_wgc_open_on` and `yapfast_dup_open_on` — since a texture from another
 * device can't be read by this session. It belongs to this encoder when the
 * encoder made it and to the caller otherwise; either way the caller must not
 * release it, and handing it back to `yapfast_d3d11enc_new_on` is what saves the
 * second device. */
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

/* Checks what both ways in are given, before a device is made or borrowed: none
 * of it depends on which device the session ends up on, so a caller that got one
 * wrong is told without a device having been opened. 0, or -1 with `report` set. */
static int argcheck(unsigned codec_id, unsigned width, unsigned height, const char *tuning_name,
                    char *report, size_t report_size) {
  if (codec_id >= CODEC_COUNT) {
    if (report_size) snprintf(report, report_size, "%u isn't a codec this encoder has", codec_id);
    return -1;
  }
  if (!nvenc_tuning(tuning_name)) {
    if (report_size) snprintf(report, report_size, "no tuning called %s", tuning_name);
    return -1;
  }
  if (!width || !height || (width & 1) || (height & 1)) {
    if (report_size)
      snprintf(report, report_size, "%ux%u isn't an even size NVENC encodes", width, height);
    return -1;
  }
  return 0;
}

/* Opens the NVENC session on `e->device` and everything an encode is opened
 * with: the codecs this driver offers, the settings the tuning row names, the
 * session, and the bitstream buffer every frame is written into. The device is
 * already decided by the caller of this — made here or handed in — and nothing
 * below asks for another one, which is the point of the split. 0, or -1 with
 * `why` set. */
static int start_session(struct yapfast_d3d11enc *e, char *why, size_t size) {
  const NV_ENCODE_API_FUNCTION_LIST *api = &library.functions;
  NV_ENC_OPEN_ENCODE_SESSION_EX_PARAMS open;
  NV_ENC_PRESET_CONFIG preset;
  NVENCSTATUS status;

  memset(&open, 0, sizeof open);
  open.version = NV_ENC_OPEN_ENCODE_SESSION_EX_PARAMS_VER;
  open.deviceType = NV_ENC_DEVICE_TYPE_DIRECTX;
  open.device = e->device;
  open.apiVersion = NVENCAPI_VERSION;
  status = api->nvEncOpenEncodeSessionEx(&open, &e->session);
  if (status != NV_ENC_SUCCESS) {
    status_failure(why, size, "nvEncOpenEncodeSessionEx on the D3D11 device", status);
    return -1;
  }
  /* The driver is asked which codecs it encodes before one is opened: a card
   * whose driver has no encoder for the codec asked for says so in a sentence,
   * rather than failing later with a status nobody can act on. */
  note_codecs(e);
  if (e->supported && !(e->supported & (1u << e->codec_id))) {
    snprintf(why, size, "this driver has no %s encoder (it has %s)%s",
             CODECS[e->codec_id].name, e->codecs,
             e->codec_id == CODEC_AV1 ? ", and AV1 needs an RTX 40 series or newer" : "");
    return -1;
  }

  memset(&preset, 0, sizeof preset);
  preset.version = NV_ENC_PRESET_CONFIG_VER;
  preset.presetCfg.version = NV_ENC_CONFIG_VER;
  status = api->nvEncGetEncodePresetConfigEx(e->session, *CODECS[e->codec_id].guid,
                                            *e->tuning->preset_guid, e->tuning->tune, &preset);
  if (status != NV_ENC_SUCCESS) {
    status_failure(why, size, "nvEncGetEncodePresetConfigEx", status);
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
    status_failure(why, size, "nvEncInitializeEncoder", status);
    return -1;
  }

  NV_ENC_CREATE_BITSTREAM_BUFFER bitstream;
  memset(&bitstream, 0, sizeof bitstream);
  bitstream.version = NV_ENC_CREATE_BITSTREAM_BUFFER_VER;
  bitstream.size = BITSTREAM_BYTES;
  bitstream.memoryHeap = NV_ENC_MEMORY_HEAP_AUTOSELECT;
  status = api->nvEncCreateBitstreamBuffer(e->session, &bitstream);
  if (status != NV_ENC_SUCCESS) {
    status_failure(why, size, "nvEncCreateBitstreamBuffer", status);
    return -1;
  }
  e->bitstream = bitstream.bitstreamBuffer;
  return 0;
}

/* Says what opened, in the words a log quotes: the card the device is on, what
 * the session was tuned to, and the backend's name. */
static void describe_open(struct yapfast_d3d11enc *e, char *report, size_t report_size) {
  DXGI_ADAPTER_DESC1 desc;
  char card[128] = "";
  if (e->adapter && SUCCEEDED(e->adapter->lpVtbl->GetDesc1(e->adapter, &desc)))
    snprintf(card, sizeof card, "%ls, ", desc.Description);
  snprintf(e->config, sizeof e->config, "%sBGRA in, NV12 4:2:0 out, bt601 limited range", card);
  snprintf(e->tuning_text, sizeof e->tuning_text,
           "%s preset, %s rate control, keyframe every %u frames", e->tuning->preset,
           e->tuning->rc, e->settings.gopLength);
  if (report_size) snprintf(report, report_size, "%s_nvenc/d3d11", CODECS[e->codec_id].short_name);
}

/* Opens an encoder for one codec on a new D3D11 device, `width` by `height` at
 * `fps` and `kbps`, tuned by `tuning_name`, on the adapter whose LUID is
 * `adapter_low`:`adapter_high`. `report` gets the name of what opened, or why
 * nothing did: one backend, so one line either way, in words that name the
 * driver's refusal rather than its status number.
 *
 * This is the open for a caller that has no device to offer, and it makes one on
 * the adapter the GPU setting chose. A caller that already has a device — a
 * capture source's, so that the texture it fills is a texture this reads — uses
 * `yapfast_d3d11enc_new_on` instead. */
struct yapfast_d3d11enc *yapfast_d3d11enc_new(unsigned codec_id, unsigned width, unsigned height,
                                              unsigned fps, unsigned kbps, unsigned adapter_low,
                                              int adapter_high, const char *tuning_name,
                                              char *report, size_t report_size) {
  struct yapfast_d3d11enc *e;
  char why[192] = "";

  if (report_size) report[0] = 0;
  if (argcheck(codec_id, width, height, tuning_name, report, report_size)) return NULL;
  const struct nvenc_tuning *tuning = nvenc_tuning(tuning_name);
  if (!driver(why, sizeof why)) {
    if (report_size) snprintf(report, report_size, "%s", why);
    return NULL;
  }
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
  /* An encoder that made its own device is one encoding a share, and reads
   * the ring the capture fills. */
  e->held = SLOTS;

  /* The adapter is named, not searched for: Rust has already chosen it from the
   * GPU setting and from what this machine has, and this opens the device on
   * that one or on none. */
  LUID wanted;
  wanted.LowPart = adapter_low;
  wanted.HighPart = adapter_high;
  if (open_device(wanted, &e->adapter, &e->device, &e->context, why, sizeof why)) goto failed;
  /* Made here, and so released in `yapfast_d3d11enc_free` and nowhere else. */
  e->owns_device = 1;
  if (start_session(e, why, sizeof why)) goto failed;
  describe_open(e, report, report_size);
  return e;

failed:
  if (report_size) snprintf(report, report_size, "%s", why);
  yapfast_d3d11enc_free(e);
  return NULL;
}

/* Opens an encoder for one codec on `device`, a D3D11 device the caller already
 * has — in a stream, the one a capture source is filling textures on — so that a
 * captured texture is a texture this session can read with nothing copied in
 * between. Everything else is `yapfast_d3d11enc_new`'s: the same codec, size,
 * rate, tuning and report, and the same contract for the frames it encodes.
 *
 * `device` is borrowed: this does not AddRef it and `yapfast_d3d11enc_free` does
 * not release it, so the caller keeps it alive for at least as long as the
 * encoder and releases its own reference afterwards. The adapter is read from
 * the device rather than named, so the open still reports the card the session
 * is really on, and a device made on anything but NVIDIA's is refused in words
 * before a session is attempted. NULL with `report` filled in when the device
 * isn't one NVENC can encode on, or when the session refuses it.
 *
 * `surfaces` is how many of the capture's surfaces the session keeps registered
 * and mapped between frames: four for one encoding a share, where the capture
 * hands back a ring and registering each surface per frame would cost more than
 * the encode, and one for a session that reads a frame in ten to show a preview
 * and has no ring to keep up with. Every surface held is a full-size one the
 * capture can't fill while it is held, which at 4K is 33 MB apiece. */
struct yapfast_d3d11enc *yapfast_d3d11enc_new_on(void *device, unsigned codec_id, unsigned width,
                                                 unsigned height, unsigned fps, unsigned kbps,
                                                 unsigned surfaces, const char *tuning_name,
                                                 char *report, size_t report_size) {
  struct yapfast_d3d11enc *e;
  DXGI_ADAPTER_DESC1 desc;
  char why[192] = "";

  if (report_size) report[0] = 0;
  if (argcheck(codec_id, width, height, tuning_name, report, report_size)) return NULL;
  const struct nvenc_tuning *tuning = nvenc_tuning(tuning_name);
  if (!device) {
    if (report_size) snprintf(report, report_size, "no D3D11 device to open the encoder on");
    return NULL;
  }
  if (!driver(why, sizeof why)) {
    if (report_size) snprintf(report, report_size, "%s", why);
    return NULL;
  }
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
  /* One surface is the least a session can keep, and `SLOTS` the most: what a
   * caller asks for is what it reads, and a count outside that range is one of
   * the two ends rather than a session that remembers nothing or writes past
   * its array. */
  e->held = surfaces < 1 ? 1 : surfaces > SLOTS ? SLOTS : surfaces;
  e->device = (ID3D11Device *)device;
  /* Borrowed, so not released here: see the ownership note at the top of this
   * file. */
  e->owns_device = 0;
  if (adapter_of(e->device, &e->adapter, &desc, why, sizeof why)) goto failed;
  /* The context is this encoder's own reference to the device's, and so is
   * released with the encoder whichever way the device arrived. */
  e->device->lpVtbl->GetImmediateContext(e->device, &e->context);
  if (!e->context) {
    snprintf(why, sizeof why, "the device handed in has no immediate context");
    goto failed;
  }
  if (start_session(e, why, sizeof why)) goto failed;
  describe_open(e, report, report_size);
  return e;

failed:
  if (report_size) snprintf(report, report_size, "%s", why);
  yapfast_d3d11enc_free(e);
  return NULL;
}

/* A slot with no texture, or the least recently used one: a capture source with
 * more surfaces than this keeps re-registering them, which is slower but never
 * wrong. A session opened with one surface has one candidate, so the surface it
 * held goes back before the new one takes its place. */
static struct slot *free_slot(struct yapfast_d3d11enc *e) {
  struct slot *oldest = &e->slots[0];
  for (size_t i = 0; i < e->held; i++) {
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
   * registration rather than the frame. The comparison is the device pointer as
   * asked back, and not a LUID or a name: two devices can be made on one adapter
   * and neither can read the other's textures, so an adapter that matched would
   * say nothing. */
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
             "opens on the encoder's device (yapfast_d3d11enc_device), or the encoder opens on "
             "the capture's (yapfast_d3d11enc_new_on)");
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
  for (size_t i = 0; i < e->held; i++) {
    struct slot *slot = &e->slots[i];
    if (slot->mapped) library.functions.nvEncUnmapInputResource(e->session, slot->mapped);
    if (slot->registered)
      library.functions.nvEncUnregisterResource(e->session, slot->registered);
    if (slot->texture) slot->texture->lpVtbl->Release(slot->texture);
    memset(slot, 0, sizeof *slot);
  }
}
/* One cached view of a surface, and the surface it holds: the view and the
 * texture both release, so a capture source that has moved on to other surfaces
 * doesn't keep the ones it replaced alive. */
static void close_input(struct input *input) {
  if (input->video) input->video->lpVtbl->Release(input->video);
  for (size_t plane = 0; plane < 2; plane++)
    if (input->planes[plane]) input->planes[plane]->lpVtbl->Release(input->planes[plane]);
  if (input->texture) input->texture->lpVtbl->Release(input->texture);
  memset(input, 0, sizeof *input);
}
/* The shader that stands in for a video processor, and the sampler and constants
 * that go with it: released as a set, since they are made as one and a half-made
 * set is retried from the start rather than used. */
static void close_shader(struct yapfast_d3d11enc *e) {
  if (e->scaler.vertex) e->scaler.vertex->lpVtbl->Release(e->scaler.vertex);
  if (e->scaler.rgb) e->scaler.rgb->lpVtbl->Release(e->scaler.rgb);
  if (e->scaler.planar) e->scaler.planar->lpVtbl->Release(e->scaler.planar);
  if (e->scaler.sampler) e->scaler.sampler->lpVtbl->Release(e->scaler.sampler);
  if (e->scaler.sizes) e->scaler.sizes->lpVtbl->Release(e->scaler.sizes);
  e->scaler.vertex = NULL;
  e->scaler.rgb = NULL;
  e->scaler.planar = NULL;
  e->scaler.sampler = NULL;
  e->scaler.sizes = NULL;
}
/* Everything the scaler made, released with the encoder: the picture that holds
 * the scaled frame, the staging texture a readback maps, and the processor or
 * the shader that filled them. */
static void close_scale(struct yapfast_d3d11enc *e) {
  for (size_t i = 0; i < e->held; i++) close_input(&e->scaler.inputs[i]);
  if (e->scaler.picture) e->scaler.picture->lpVtbl->Release(e->scaler.picture);
  if (e->scaler.staging) e->scaler.staging->lpVtbl->Release(e->scaler.staging);
  if (e->scaler.target) e->scaler.target->lpVtbl->Release(e->scaler.target);
  if (e->scaler.output) e->scaler.output->lpVtbl->Release(e->scaler.output);
  if (e->scaler.processor) e->scaler.processor->lpVtbl->Release(e->scaler.processor);
  if (e->scaler.enumerator) e->scaler.enumerator->lpVtbl->Release(e->scaler.enumerator);
  if (e->scaler.video_context) e->scaler.video_context->lpVtbl->Release(e->scaler.video_context);
  if (e->scaler.video) e->scaler.video->lpVtbl->Release(e->scaler.video);
  if (e->scaler.planes_of) e->scaler.planes_of->lpVtbl->Release(e->scaler.planes_of);
  close_shader(e);
}

void yapfast_d3d11enc_free(struct yapfast_d3d11enc *e) {
  if (!e) return;
  if (e->session) {
    close_slots(e);
    if (e->bitstream) library.functions.nvEncDestroyBitstreamBuffer(e->session, e->bitstream);
    library.functions.nvEncDestroyEncoder(e->session);
  }
  close_scale(e);
  free(e->staged);
  if (e->context) e->context->lpVtbl->Release(e->context);
  /* Only what this encoder made: a device it was opened on belongs to the caller
   * that made it, and releasing that would take the capture source's device out
   * from under it. */
  if (e->owns_device && e->device) e->device->lpVtbl->Release(e->device);
  if (e->adapter) e->adapter->lpVtbl->Release(e->adapter);
  free(e);
}

/* Keeps `size` bytes of `bytes`, at `at` into the staged access unit, for Rust to
 * copy out. The buffer grows to the largest frame seen and no further, so a
 * stream asks for memory only until its frames settle. Returns 0 when memory ran
 * out. */
static int stage(struct yapfast_d3d11enc *e, size_t at, const unsigned char *bytes, size_t size) {
  if (at + size > e->staged_capacity) {
    unsigned char *larger = realloc(e->staged, at + size);
    if (!larger) return 0;
    e->staged = larger;
    e->staged_capacity = at + size;
  }
  memcpy(e->staged + at, bytes, size);
  return 1;
}

/* Encodes the picture in `texture`'s `subresource`, whose time is `pts` in 90
 * kHz units: the stream's own clock, which labels the frame's packets too.
 * `keyframe` asks for an IDR on this frame, which is what a viewer who joins,
 * or whose picture broke, waits for. What came out is kept for
 * `yapfast_d3d11enc_drain` to copy into the caller's buffer: the size it
 * reports may be larger than that buffer, which grows to hold it without the
 * picture being encoded a second time. Returns the access unit's size, 0 when
 * the encoder made none, or -1 on failure, whose reason
 * `yapfast_d3d11enc_error` gives. The copy out of the driver's mapped bitstream
 * is the one this call makes; the drain is the second and last, and the
 * picture's pixels still never cross the processor. */
long yapfast_d3d11enc_encode(struct yapfast_d3d11enc *e, void *texture, unsigned subresource,
                             long long pts, int keyframe, int *is_key) {
  NV_ENC_PIC_PARAMS picture;
  NV_ENC_LOCK_BITSTREAM locked;
  NVENCSTATUS status;
  char why[192] = "";

  if (!e) return -1;
  if (!texture || !is_key) return fail(e, -1, "the encoder was given nothing to encode");
  *is_key = 0;
  struct slot *slot = NULL;
  for (size_t i = 0; i < e->held && !slot; i++)
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
  /* Whatever the last call left is this frame's to replace, copied out or not:
   * one frame is held at a time. */
  e->staged_size = 0;
  if (size && !stage(e, 0, locked.bitstreamBufferPtr, size)) {
    library.functions.nvEncUnlockBitstream(e->session, locked.outputBitstream);
    return fail(e, -1, "no memory for a %zu byte frame", size);
  }
  e->staged_size = size;
  *is_key = locked.pictureType == NV_ENC_PIC_TYPE_IDR || locked.pictureType == NV_ENC_PIC_TYPE_I;
  library.functions.nvEncUnlockBitstream(e->session, locked.outputBitstream);
  return (long)size;
}

/* Copies the access unit the last encode produced into `out`, which holds it,
 * and gives up the copy: the buffer stays for the next frame. Rust grows its
 * buffer to the size the encode reported before calling this once, so a frame is
 * never encoded twice to fit one. Returns the size copied, or -1 when there is
 * nothing to copy or `out` is too small. */
long yapfast_d3d11enc_drain(struct yapfast_d3d11enc *e, unsigned char *out, size_t capacity) {
  if (!e) return -1;
  size_t size = e->staged_size;
  if (!size || size > capacity) return -1;
  memcpy(out, e->staged, size);
  e->staged_size = 0;
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
/* ----------------------------------------------------------------- the scaler
 *
 * A capture is whatever size the screen is and a stream is whatever size the
 * stream is, and on a display wider than 1080p those are almost never the same
 * number. Without this the textured path could only encode a capture that
 * happened to come out at the stream's size, which is why a share of a big
 * screen went back to the processor's converter — a full-frame read and upload
 * of every frame, which is the copy everything else in this file avoids.
 *
 * So the same three steps happen on the GPU instead: the picture is scaled into
 * a texture the encoder keeps, the encoder registers that texture exactly as it
 * registers a capture's own, and a preview is read from the same texture through
 * a staging one at the size the preview asks for. A capture that is already the
 * stream's size skips all of it: the sizes match and the picture is copied,
 * which is exact and is one GPU-side copy.
 *
 * The filter is the driver's video processor where it will take the texture and
 * hand back BGRA — the scaler it plays video through, multi-tap and made for
 * exactly this — and a pixel shader where it won't, which is a real case rather
 * than a courtesy: a video processor input view needs the surface to have been
 * made with one of a few bind flags, and a capture source's surfaces are the
 * capture source's to make. `yapfast_d3d11enc_filter` says which one ran. */
/* The entry for the caller's texture, made on the first frame it arrives and
 * kept while it keeps arriving: the same least-recently-used rotation as the
 * encoder's slots, since a capture source hands back a few surfaces in rotation
 * rather than one — and the same count, so that a session that reads one frame
 * in ten holds one surface rather than four. Never fails — the entry is one of
 * a fixed array — and a view that can't be made of this surface is said by
 * whoever asked for it. */
static struct input *input_for(struct yapfast_d3d11enc *e, ID3D11Texture2D *texture,
                               unsigned subresource, DXGI_FORMAT format) {
  struct input *oldest = NULL;
  for (size_t i = 0; i < e->held; i++) {
    struct input *input = &e->scaler.inputs[i];
    if (input->texture == texture && input->subresource == subresource &&
        input->format == format) {
      input->used = e->scaler.scaled;
      return input;
    }
    if (!input->texture) {
      oldest = input;
      break;
    }
    if (!oldest || input->used < oldest->used) oldest = input;
  }
  close_input(oldest);
  oldest->texture = texture;
  oldest->subresource = subresource;
  oldest->format = format;
  oldest->used = e->scaler.scaled;
  /* Held for as long as a view names it, so a capture source that lets go of a
   * surface can't pull it out from under the scaler. */
  texture->lpVtbl->AddRef(texture);
  return oldest;
}
/* The texture a scaled picture lands in, at `width` by `height`: made on the
 * first frame of that size and kept until a frame of another size replaces it,
 * since a capture's size changes when the screen does and not between frames. A
 * render target, because that is what both scalers write it as, and BGRA,
 * because that is what NVENC registers from this file and what a preview reads
 * a tile of. */
static int destination(struct yapfast_d3d11enc *e, unsigned width, unsigned height, char *why,
                       size_t size) {
  D3D11_TEXTURE2D_DESC desc;
  HRESULT status;
  if (e->scaler.picture && e->scaler.width == width && e->scaler.height == height) return 0;
  if (e->scaler.picture) {
    e->scaler.picture->lpVtbl->Release(e->scaler.picture);
    e->scaler.picture = NULL;
  }
  /* Both views name the texture they draw into, so they go with it. */
  if (e->scaler.output) e->scaler.output->lpVtbl->Release(e->scaler.output);
  e->scaler.output = NULL;
  if (e->scaler.target) e->scaler.target->lpVtbl->Release(e->scaler.target);
  e->scaler.target = NULL;
  memset(&desc, 0, sizeof desc);
  desc.Width = width;
  desc.Height = height;
  desc.MipLevels = 1;
  desc.ArraySize = 1;
  desc.Format = DXGI_FORMAT_B8G8R8A8_UNORM;
  desc.SampleDesc.Count = 1;
  desc.Usage = D3D11_USAGE_DEFAULT;
  desc.BindFlags = D3D11_BIND_RENDER_TARGET;
  status = e->device->lpVtbl->CreateTexture2D(e->device, &desc, NULL, &e->scaler.picture);
  if (FAILED(status)) {
    e->scaler.picture = NULL;
    hr_failure(why, size, "CreateTexture2D for the scaled picture", status);
    return -1;
  }
  e->scaler.width = width;
  e->scaler.height = height;
  e->scaler.pictures++;
  return 0;
}
/* The video processor, ready to take a picture of `source_width` by
 * `source_height` in `format` and put it in the destination's size: the
 * enumerator and the processor are made for one pair of sizes and one pair of
 * formats and remade when either changes. Answers 0 when there is one to blit
 * through, and -1 with `why` when there isn't, which is the caller's cue to use
 * the shader. */
static int video_processor(struct yapfast_d3d11enc *e, unsigned source_width,
                           unsigned source_height, DXGI_FORMAT format, char *why, size_t size) {
  D3D11_VIDEO_PROCESSOR_CONTENT_DESC content;
  D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC view;
  RECT rect;
  UINT support = 0;
  HRESULT status;
  if (!e->scaler.video) {
    /* Asked of the device the encoder opened rather than a second one: a
     * processor on another device can't read these textures, and a device made
     * with the video flag from the start would be a change to the device the
     * encode path opens. A driver that has no processor is what the shader is
     * for. */
    status = e->device->lpVtbl->QueryInterface(e->device, &IID_ID3D11VideoDevice,
                                               (void **)&e->scaler.video);
    if (FAILED(status) || !e->scaler.video) {
      e->scaler.video = NULL;
      hr_failure(why, size, "this device has no video device (QueryInterface)", status);
      return -1;
    }
    status = e->context->lpVtbl->QueryInterface(e->context, &IID_ID3D11VideoContext,
                                                (void **)&e->scaler.video_context);
    if (FAILED(status) || !e->scaler.video_context) {
      e->scaler.video_context = NULL;
      hr_failure(why, size, "this context has no video context (QueryInterface)", status);
      return -1;
    }
  }
  if (e->scaler.processor && e->scaler.output && e->scaler.source_width == source_width &&
      e->scaler.source_height == source_height) {
    status =
        e->scaler.enumerator->lpVtbl->CheckVideoProcessorFormat(e->scaler.enumerator, format, &support);
    if (!FAILED(status) && (support & D3D11_VIDEO_PROCESSOR_FORMAT_SUPPORT_INPUT)) return 0;
    if (size)
      snprintf(why, size, "the video processor won't take DXGI format %u as a picture",
               (unsigned)format);
    return -1;
  }
  if (e->scaler.output) e->scaler.output->lpVtbl->Release(e->scaler.output);
  e->scaler.output = NULL;
  if (e->scaler.processor) e->scaler.processor->lpVtbl->Release(e->scaler.processor);
  e->scaler.processor = NULL;
  if (e->scaler.enumerator) e->scaler.enumerator->lpVtbl->Release(e->scaler.enumerator);
  e->scaler.enumerator = NULL;
  /* Both rates the same: the processor is told frames arrive as fast as they
   * leave, so it scales one frame into one frame instead of converting between
   * rates on frames handed to it at no fixed rate at all. */
  memset(&content, 0, sizeof content);
  content.InputFrameFormat = D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE;
  content.InputFrameRate.Numerator = 60;
  content.InputFrameRate.Denominator = 1;
  content.InputWidth = source_width;
  content.InputHeight = source_height;
  content.OutputFrameRate = content.InputFrameRate;
  content.OutputWidth = e->scaler.width;
  content.OutputHeight = e->scaler.height;
  content.Usage = D3D11_VIDEO_USAGE_PLAYBACK_NORMAL;
  status = e->scaler.video->lpVtbl->CreateVideoProcessorEnumerator(e->scaler.video, &content,
                                                                   &e->scaler.enumerator);
  if (FAILED(status)) {
    hr_failure(why, size, "CreateVideoProcessorEnumerator", status);
    return -1;
  }
  status =
      e->scaler.enumerator->lpVtbl->CheckVideoProcessorFormat(e->scaler.enumerator, format, &support);
  if (FAILED(status) || !(support & D3D11_VIDEO_PROCESSOR_FORMAT_SUPPORT_INPUT)) {
    if (size)
      snprintf(why, size, "the video processor won't take %ux%u of DXGI format %u as a picture",
               source_width, source_height, (unsigned)format);
    return -1;
  }
  status = e->scaler.enumerator->lpVtbl->CheckVideoProcessorFormat(
      e->scaler.enumerator, DXGI_FORMAT_B8G8R8A8_UNORM, &support);
  if (FAILED(status) || !(support & D3D11_VIDEO_PROCESSOR_FORMAT_SUPPORT_OUTPUT)) {
    if (size) snprintf(why, size, "the video processor won't write BGRA");
    return -1;
  }
  status = e->scaler.video->lpVtbl->CreateVideoProcessor(e->scaler.video, e->scaler.enumerator, 0,
                                                         &e->scaler.processor);
  if (FAILED(status)) {
    hr_failure(why, size, "CreateVideoProcessor", status);
    return -1;
  }
  /* The whole source into the whole destination, said rather than left to the
   * defaults: the whole of one to the whole of the other is the stretch that is
   * the scale, and a driver's idea of a border has no place in the picture. */
  SetRect(&rect, 0, 0, (LONG)e->scaler.width, (LONG)e->scaler.height);
  e->scaler.video_context->lpVtbl->VideoProcessorSetOutputTargetRect(e->scaler.video_context,
                                                                     e->scaler.processor, TRUE, &rect);
  SetRect(&rect, 0, 0, (LONG)source_width, (LONG)source_height);
  e->scaler.video_context->lpVtbl->VideoProcessorSetStreamSourceRect(
      e->scaler.video_context, e->scaler.processor, 0, TRUE, &rect);
  memset(&view, 0, sizeof view);
  view.ViewDimension = D3D11_VPOV_DIMENSION_TEXTURE2D;
  view.Texture2D.MipSlice = 0;
  status = e->scaler.video->lpVtbl->CreateVideoProcessorOutputView(
      e->scaler.video, (ID3D11Resource *)e->scaler.picture, e->scaler.enumerator, &view,
      &e->scaler.output);
  if (FAILED(status)) {
    e->scaler.output = NULL;
    hr_failure(why, size, "CreateVideoProcessorOutputView", status);
    return -1;
  }
  e->scaler.source_width = source_width;
  e->scaler.source_height = source_height;
  return 0;
}
/* The picture in `texture`, through the video processor, into the destination:
 * a view of the surface made once and kept, and then one blit per frame. */
static int processor_blit(struct yapfast_d3d11enc *e, ID3D11Texture2D *texture,
                          unsigned subresource, DXGI_FORMAT format, char *why, size_t size) {
  struct input *input = input_for(e, texture, subresource, format);
  D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC desc;
  D3D11_VIDEO_PROCESSOR_STREAM stream;
  HRESULT status;
  if (!input->video) {
    memset(&desc, 0, sizeof desc);
    /* FourCC zero: the surface's own format, which is the one the encoder would
     * register it as. The subresource is an array slice, since a capture's
     * surfaces have one mip each. */
    desc.FourCC = 0;
    desc.ViewDimension = D3D11_VPIV_DIMENSION_TEXTURE2D;
    desc.Texture2D.MipSlice = 0;
    desc.Texture2D.ArraySlice = subresource;
    status = e->scaler.video->lpVtbl->CreateVideoProcessorInputView(
        e->scaler.video, (ID3D11Resource *)texture, e->scaler.enumerator, &desc, &input->video);
    if (FAILED(status)) {
      input->video = NULL;
      hr_failure(why, size, "CreateVideoProcessorInputView of the capture's texture", status);
      return -1;
    }
  }
  memset(&stream, 0, sizeof stream);
  stream.Enable = TRUE;
  stream.OutputIndex = 0;
  stream.InputFrameOrField = 0;
  stream.pInputSurface = input->video;
  status = e->scaler.video_context->lpVtbl->VideoProcessorBlt(
      e->scaler.video_context, e->scaler.processor, e->scaler.output, 0, 1, &stream);
  if (FAILED(status)) {
    hr_failure(why, size, "VideoProcessorBlt", status);
    return -1;
  }
  return 0;
}
/* The picture the shader scales with: four bilinear taps at the corners of a box
 * a quarter of the source's footprint across, which is what a downscale needs.
 * One tap of a frame four times smaller keeps one pixel of the sixteen it covers
 * and the picture crawls with whatever the samples happened to land on; four
 * taps spread over the footprint are the cheap end of a box filter, and what is
 * between them is the hardware's bilinear filter, which costs nothing. The same
 * source compiles twice — once with NV12 defined, for a surface it reads as two
 * planes — so the two paths can't drift apart. */
static const char *scale_shader =
    "Texture2D<float4> luma : register(t0);\n"
    "Texture2D<float4> chroma : register(t1);\n"
    "SamplerState tap : register(s0);\n"
    "cbuffer Sizes : register(b0) { float2 source; float2 target; };\n"
    "struct Varying { float4 position : SV_Position; float2 uv : TEXCOORD0; };\n"
    /* One triangle over the whole target: three vertices, no vertex buffer and
     * nothing uploaded per frame, with the corner of the target at the corner of
     * the picture. */
    "Varying VSMain(uint vertex : SV_VertexID) {\n"
    "  float2 corner = float2((vertex << 1) & 2, vertex & 2);\n"
    "  Varying varying;\n"
    "  varying.position = float4(corner * float2(2, -2) + float2(-1, 1), 0, 1);\n"
    "  varying.uv = corner;\n"
    "  return varying;\n"
    "}\n"
    "float4 PSMain(Varying varying) : SV_Target {\n"
    /* A quarter of the footprint, so the four taps are at the corners of a box
     * half the footprint across. A picture that is growing keeps a footprint of
     * one texel, so a scale up is a bilinear stretch rather than a blur forty
     * texels wide. */
    "  float2 step = 0.25 * max(source / target, 1.0) / source;\n"
    "  static const float2 corners[4] = { float2(-1, -1), float2(1, -1),\n"
    "                                     float2(-1, 1), float2(1, 1) };\n"
    "  float4 picture = 0;\n"
    "  [unroll] for (uint i = 0; i < 4; i++)\n"
    "    picture += luma.Sample(tap, varying.uv + corners[i] * step);\n"
    "  picture *= 0.25;\n"
    "#ifdef NV12\n"
    "  float2 colour = 0;\n"
    "  [unroll] for (uint j = 0; j < 4; j++)\n"
    "    colour += chroma.Sample(tap, varying.uv + corners[j] * step).rg;\n"
    "  colour *= 0.25;\n"
    /* BT.601's matrix over its limited range, the claim this file makes to NVENC
     * in every codec's VUI and the one hwenc.c makes to FFmpeg: a picture that
     * passes through the scaler says what a picture that doesn't says. */
    "  float brightness = saturate((picture.r - 16.0 / 255.0) * (255.0 / 219.0));\n"
    "  colour = (colour - 128.0 / 255.0) * (255.0 / 224.0);\n"
    "  return float4(saturate(float3(brightness + 1.402 * colour.y,\n"
    "                                brightness - 0.344136 * colour.x - 0.714136 * colour.y,\n"
    "                                brightness + 1.772 * colour.x)),\n"
    "                1.0);\n"
    "#else\n"
    /* Colour in, the same colour out: a shader's channels are R, G, B whatever
     * order the surface keeps them in, so a BGRA or an RGBA texture lands in the
     * BGRA destination with its colours where it put them. */
    "  return float4(picture.rgb, 1.0);\n"
    "#endif\n"
    "}\n";
/* One entry point of the scale's shader, compiled for `profile`: D3DCompile
 * either writes the program or says, in its own words, which line it didn't
 * like. */
static int shader_stage(const char *entry, const char *profile, int planar, ID3DBlob **out,
                        char *why, size_t size) {
  D3D_SHADER_MACRO macros[2] = {{"NV12", "1"}, {NULL, NULL}};
  ID3DBlob *errors = NULL;
  HRESULT status = D3DCompile(scale_shader, strlen(scale_shader), "yapfast-scale.hlsl",
                              planar ? macros : NULL, NULL, entry, profile,
                              D3DCOMPILE_OPTIMIZATION_LEVEL3, 0, out, &errors);
  if (FAILED(status)) {
    if (size)
      snprintf(why, size, "D3DCompile(%s) failed (0x%08lX) %.96s", entry, (unsigned long)status,
               errors ? (const char *)errors->lpVtbl->GetBufferPointer(errors) : "");
    if (errors) errors->lpVtbl->Release(errors);
    return -1;
  }
  if (errors) errors->lpVtbl->Release(errors);
  return 0;
}
/* One stage of that shader, made on the device: the vertex stage, or the pixel
 * stage for a colour picture or a planar one. */
static int make_stage(struct yapfast_d3d11enc *e, const char *entry, const char *profile,
                      int planar, const char *what, void **out, char *why, size_t size) {
  ID3DBlob *code = NULL;
  HRESULT status;
  if (shader_stage(entry, profile, planar, &code, why, size)) return -1;
  if (profile[0] == 'v')
    status = e->device->lpVtbl->CreateVertexShader(e->device, code->lpVtbl->GetBufferPointer(code),
                                                   code->lpVtbl->GetBufferSize(code), NULL,
                                                   (ID3D11VertexShader **)out);
  else
    status = e->device->lpVtbl->CreatePixelShader(e->device, code->lpVtbl->GetBufferPointer(code),
                                                  code->lpVtbl->GetBufferSize(code), NULL,
                                                  (ID3D11PixelShader **)out);
  code->lpVtbl->Release(code);
  if (FAILED(status)) {
    *out = NULL;
    hr_failure(why, size, what, status);
    return -1;
  }
  return 0;
}
/* The shader, its sampler and the constants the taps are worked out from, made
 * once. A half-made set is released rather than kept, so the next frame tries
 * again from the start instead of drawing with a NULL. */
static int shader_ready(struct yapfast_d3d11enc *e, char *why, size_t size) {
  D3D11_SAMPLER_DESC sampler;
  D3D11_BUFFER_DESC sizes;
  HRESULT status;
  if (e->scaler.vertex) return 0;
  if (make_stage(e, "VSMain", "vs_5_0", 0, "CreateVertexShader", (void **)&e->scaler.vertex, why,
                 size))
    goto failed;
  if (make_stage(e, "PSMain", "ps_5_0", 0, "CreatePixelShader", (void **)&e->scaler.rgb, why,
                 size))
    goto failed;
  if (make_stage(e, "PSMain", "ps_5_0", 1, "CreatePixelShader for NV12",
                 (void **)&e->scaler.planar, why, size))
    goto failed;
  /* Bilinear and clamped: the taps decide what is read, the filter between them
   * is the hardware's, and a texel outside the picture is the picture's edge. */
  memset(&sampler, 0, sizeof sampler);
  sampler.Filter = D3D11_FILTER_MIN_MAG_LINEAR_MIP_POINT;
  sampler.AddressU = D3D11_TEXTURE_ADDRESS_CLAMP;
  sampler.AddressV = D3D11_TEXTURE_ADDRESS_CLAMP;
  sampler.AddressW = D3D11_TEXTURE_ADDRESS_CLAMP;
  sampler.ComparisonFunc = D3D11_COMPARISON_NEVER;
  sampler.MaxLOD = D3D11_FLOAT32_MAX;
  status = e->device->lpVtbl->CreateSamplerState(e->device, &sampler, &e->scaler.sampler);
  if (FAILED(status)) {
    e->scaler.sampler = NULL;
    hr_failure(why, size, "CreateSamplerState", status);
    goto failed;
  }
  /* Sixteen bytes: the source's size and the target's, written per frame, which
   * is a register write and not an allocation. */
  memset(&sizes, 0, sizeof sizes);
  sizes.ByteWidth = 16;
  sizes.Usage = D3D11_USAGE_DEFAULT;
  sizes.BindFlags = D3D11_BIND_CONSTANT_BUFFER;
  status = e->device->lpVtbl->CreateBuffer(e->device, &sizes, NULL, &e->scaler.sizes);
  if (FAILED(status)) {
    e->scaler.sizes = NULL;
    hr_failure(why, size, "CreateBuffer for the scale's sizes", status);
    goto failed;
  }
  return 0;
failed:
  close_shader(e);
  return -1;
}
/* One plane of a planar surface as a shader's view. D3D11.3 makes it as a
 * ShaderResourceView1, which is a shader resource view as well, so the older
 * interface is asked for — the one the texture's cache holds — and the newer
 * one released. */
static int plane_view(ID3D11Device3 *device, ID3D11Texture2D *texture,
                      const D3D11_SHADER_RESOURCE_VIEW_DESC1 *desc, ID3D11ShaderResourceView **out,
                      char *why, size_t size) {
  ID3D11ShaderResourceView1 *view = NULL;
  HRESULT status =
      device->lpVtbl->CreateShaderResourceView1(device, (ID3D11Resource *)texture, desc, &view);
  if (FAILED(status) || !view) {
    *out = NULL;
    hr_failure(why, size, "CreateShaderResourceView1 of a plane", status);
    return -1;
  }
  status = view->lpVtbl->QueryInterface(view, &IID_ID3D11ShaderResourceView, (void **)out);
  view->lpVtbl->Release(view);
  if (FAILED(status)) {
    *out = NULL;
    hr_failure(why, size, "QueryInterface for the plane's shader resource view", status);
    return -1;
  }
  return 0;
}
/* The views a shader reads `input`'s surface through: one for BGRA or RGBA, and
 * one per plane for NV12, whose second plane only D3D11.3's view can name. */
static int source_views(struct yapfast_d3d11enc *e, struct input *input, DXGI_FORMAT format,
                        char *why, size_t size) {
  D3D11_SHADER_RESOURCE_VIEW_DESC1 desc;
  HRESULT status;
  if (input->planes[0]) return 0;
  if (format != DXGI_FORMAT_NV12) {
    status = e->device->lpVtbl->CreateShaderResourceView(e->device,
                                                         (ID3D11Resource *)input->texture, NULL,
                                                         &input->planes[0]);
    if (FAILED(status)) {
      input->planes[0] = NULL;
      if (size)
        snprintf(why, size,
                 "CreateShaderResourceView of the capture's texture failed (0x%08lX): a shader "
                 "reads a texture its source made with D3D11_BIND_SHADER_RESOURCE",
                 (unsigned long)status);
      return -1;
    }
    return 0;
  }
  if (!e->scaler.planes_of) {
    status = e->device->lpVtbl->QueryInterface(e->device, &IID_ID3D11Device3,
                                               (void **)&e->scaler.planes_of);
    if (FAILED(status) || !e->scaler.planes_of) {
      e->scaler.planes_of = NULL;
      hr_failure(why, size, "QueryInterface for D3D11.3's device, which reads a plane of NV12",
                 status);
      return -1;
    }
  }
  memset(&desc, 0, sizeof desc);
  desc.ViewDimension = D3D11_SRV_DIMENSION_TEXTURE2D;
  desc.Texture2D.MipLevels = 1;
  desc.Format = DXGI_FORMAT_R8_UNORM;
  if (plane_view(e->scaler.planes_of, input->texture, &desc, &input->planes[0], why, size)) return -1;
  desc.Format = DXGI_FORMAT_R8G8_UNORM;
  desc.Texture2D.PlaneSlice = 1;
  if (plane_view(e->scaler.planes_of, input->texture, &desc, &input->planes[1], why, size)) {
    /* Both go: a luma view with no chroma beside it is a picture the next frame
     * would sample half of rather than one this frame refused. */
    input->planes[0]->lpVtbl->Release(input->planes[0]);
    input->planes[0] = NULL;
    return -1;
  }
  return 0;
}
/* The drawing state a blit borrows and gives back: a capture source that was
 * midway through compositing its own frame finds the context as it left it,
 * rather than drawing into the scaler's target or through its shader. */
struct drawing {
  ID3D11RenderTargetView *target;
  ID3D11DepthStencilView *depth;
  ID3D11BlendState *blend;
  FLOAT factor[4];
  UINT mask;
  ID3D11DepthStencilState *depth_stencil;
  UINT stencil;
  ID3D11RasterizerState *rasterizer;
  D3D11_VIEWPORT viewport;
  UINT viewports;
};
static void save_drawing(ID3D11DeviceContext *context, struct drawing *drawing) {
  memset(drawing, 0, sizeof *drawing);
  context->lpVtbl->OMGetRenderTargets(context, 1, &drawing->target, &drawing->depth);
  context->lpVtbl->OMGetBlendState(context, &drawing->blend, drawing->factor, &drawing->mask);
  context->lpVtbl->OMGetDepthStencilState(context, &drawing->depth_stencil, &drawing->stencil);
  drawing->viewports = 1;
  context->lpVtbl->RSGetViewports(context, &drawing->viewports, &drawing->viewport);
  context->lpVtbl->RSGetState(context, &drawing->rasterizer);
}
static void restore_drawing(ID3D11DeviceContext *context, struct drawing *drawing) {
  context->lpVtbl->OMSetRenderTargets(context, 1, &drawing->target, drawing->depth);
  context->lpVtbl->OMSetBlendState(context, drawing->blend, drawing->factor, drawing->mask);
  context->lpVtbl->OMSetDepthStencilState(context, drawing->depth_stencil, drawing->stencil);
  context->lpVtbl->RSSetViewports(context, drawing->viewports, &drawing->viewport);
  context->lpVtbl->RSSetState(context, drawing->rasterizer);
  if (drawing->target) drawing->target->lpVtbl->Release(drawing->target);
  if (drawing->depth) drawing->depth->lpVtbl->Release(drawing->depth);
  if (drawing->blend) drawing->blend->lpVtbl->Release(drawing->blend);
  if (drawing->depth_stencil) drawing->depth_stencil->lpVtbl->Release(drawing->depth_stencil);
  if (drawing->rasterizer) drawing->rasterizer->lpVtbl->Release(drawing->rasterizer);
}
/* The picture in `texture`, through the shader, into the destination: one draw
 * of three vertices and no geometry at all. */
static int shader_blit(struct yapfast_d3d11enc *e, ID3D11Texture2D *texture, unsigned subresource,
                       DXGI_FORMAT format, unsigned source_width, unsigned source_height,
                       char *why, size_t size) {
  ID3D11DeviceContext *context = e->context;
  struct input *input = input_for(e, texture, subresource, format);
  ID3D11ShaderResourceView *planes[2];
  ID3D11ShaderResourceView *unbound[2] = {NULL, NULL};
  ID3D11Buffer *sizes = e->scaler.sizes;
  ID3D11SamplerState *sampler = e->scaler.sampler;
  ID3D11PixelShader *shader = format == DXGI_FORMAT_NV12 ? e->scaler.planar : e->scaler.rgb;
  float extents[4] = {(float)source_width, (float)source_height, (float)e->scaler.width,
                      (float)e->scaler.height};
  D3D11_VIEWPORT viewport;
  struct drawing drawing;
  HRESULT status;
  if (source_views(e, input, format, why, size)) return -1;
  if (!e->scaler.target) {
    status = e->device->lpVtbl->CreateRenderTargetView(
        e->device, (ID3D11Resource *)e->scaler.picture, NULL, &e->scaler.target);
    if (FAILED(status)) {
      e->scaler.target = NULL;
      hr_failure(why, size, "CreateRenderTargetView of the scaled picture", status);
      return -1;
    }
  }
  planes[0] = input->planes[0];
  planes[1] = input->planes[1];
  context->lpVtbl->UpdateSubresource(context, (ID3D11Resource *)sizes, 0, NULL, extents, 0, 0);
  save_drawing(context, &drawing);
  memset(&viewport, 0, sizeof viewport);
  viewport.Width = (float)e->scaler.width;
  viewport.Height = (float)e->scaler.height;
  viewport.MaxDepth = 1.f;
  context->lpVtbl->OMSetRenderTargets(context, 1, &e->scaler.target, NULL);
  /* The defaults, which the triangle was wound for: opaque, no depth test, and
   * the back face culled so the three vertices cover the target rather than half
   * of it. */
  context->lpVtbl->OMSetBlendState(context, NULL, NULL, 0xFFFFFFFF);
  context->lpVtbl->OMSetDepthStencilState(context, NULL, 0);
  context->lpVtbl->RSSetState(context, NULL);
  context->lpVtbl->RSSetViewports(context, 1, &viewport);
  context->lpVtbl->IASetPrimitiveTopology(context, D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
  context->lpVtbl->IASetInputLayout(context, NULL);
  context->lpVtbl->VSSetShader(context, e->scaler.vertex, NULL, 0);
  context->lpVtbl->VSSetConstantBuffers(context, 0, 1, &sizes);
  context->lpVtbl->PSSetShader(context, shader, NULL, 0);
  context->lpVtbl->PSSetConstantBuffers(context, 0, 1, &sizes);
  context->lpVtbl->PSSetShaderResources(context, 0, 2, planes);
  context->lpVtbl->PSSetSamplers(context, 0, 1, &sampler);
  context->lpVtbl->Draw(context, 3, 0);
  /* The source unbound and the caller's drawing state put back before anything
   * reads the destination: a texture left bound as a shader's input is one the
   * driver would rather not be asked to copy or encode. */
  context->lpVtbl->PSSetShaderResources(context, 0, 2, unbound);
  restore_drawing(context, &drawing);
  return 0;
}
/* Puts the picture in `texture`'s `subresource` into the encoder's destination
 * texture at `width` by `height`: an exact copy where it is already that size
 * and already BGRA, the video processor where the driver has one that takes
 * these formats, and the shader where it hasn't. Answers 0, or -1 with `why`
 * naming what the driver said about both. */
static int scale_picture(struct yapfast_d3d11enc *e, ID3D11Texture2D *texture, unsigned subresource,
                         DXGI_FORMAT format, unsigned source_width, unsigned source_height,
                         unsigned width, unsigned height, char *why, size_t size) {
  char processor_why[128] = "the video processor wasn't tried";
  char shader_why[128] = "the shader wasn't tried";
  if (texture == e->scaler.picture) {
    if (size)
      snprintf(why, size,
               "the %ux%u picture the scaler produced is the texture it would write into: a "
               "capture's own texture is what it scales, and the picture it fills is for the "
               "encoder or for a readback at its own size",
               e->scaler.width, e->scaler.height);
    return -1;
  }
  if (destination(e, width, height, why, size)) return -1;
  e->scaler.scaled++;
  if (width == source_width && height == source_height && format == DXGI_FORMAT_B8G8R8A8_UNORM) {
    e->context->lpVtbl->CopySubresourceRegion(e->context, (ID3D11Resource *)e->scaler.picture, 0, 0,
                                              0, 0, (ID3D11Resource *)texture, subresource, NULL);
    e->scaler.filter = "a copy, the sizes being the same";
    return 0;
  }
  if (!video_processor(e, source_width, source_height, format, processor_why,
                       sizeof processor_why) &&
      !processor_blit(e, texture, subresource, format, processor_why, sizeof processor_why)) {
    e->scaler.filter = "the video processor";
    return 0;
  }
  if (!shader_ready(e, shader_why, sizeof shader_why) &&
      !shader_blit(e, texture, subresource, format, source_width, source_height, shader_why,
                   sizeof shader_why)) {
    e->scaler.filter = "a pixel shader";
    return 0;
  }
  if (size) snprintf(why, size, "%s, and %s", processor_why, shader_why);
  return -1;
}
/* Scales the picture in `texture`'s `subresource` to `width` by `height` on the
 * GPU, into a texture this encoder keeps and reuses, and answers that texture as
 * the ID3D11Texture2D the encoder then reads: how a capture of another size than
 * the stream's reaches the encoder without a frame's pixels crossing the
 * processor. The texture belongs to the encoder, and the next scale to the same
 * size overwrites it, so a caller encodes a picture before scaling another.
 *
 * `upscale` allows a destination larger than the source; zero refuses one, since
 * a capture bigger than the stream is what this exists for, and a picture
 * stretched past its own pixels is a change nobody asked for. Answers NULL with
 * `yapfast_d3d11enc_error` saying why. */
void *yapfast_d3d11enc_scale(struct yapfast_d3d11enc *e, void *texture, unsigned subresource,
                             unsigned width, unsigned height, int upscale) {
  ID3D11Texture2D *source = texture;
  D3D11_TEXTURE2D_DESC desc;
  NV_ENC_BUFFER_FORMAT registered;
  char why[192] = "";
  if (!e) return NULL;
  if (!source) return nothing(e, "the scaler was given nothing to scale");
  if (!width || !height)
    return nothing(e, "%ux%u isn't a size anything can be scaled to", width, height);
  source->lpVtbl->GetDesc(source, &desc);
  if (subresource >= desc.MipLevels * desc.ArraySize)
    return nothing(e, "the texture has no subresource %u", subresource);
  if (!texture_format(desc.Format, &registered))
    return nothing(e,
                   "the texture is DXGI format %u, and this scales BGRA, RGBA or NV12; a "
                   "typeless texture has no format to scale from",
                   (unsigned)desc.Format);
  if (!upscale && (width > desc.Width || height > desc.Height))
    return nothing(e,
                   "scaling %ux%u up to %ux%u isn't asked for: this only shrinks a picture "
                   "unless told otherwise",
                   desc.Width, desc.Height, width, height);
  if (scale_picture(e, source, subresource, desc.Format, desc.Width, desc.Height, width, height,
                    why, sizeof why))
    return nothing(e, "%s", why);
  return e->scaler.picture;
}
/* The staging texture a readback is mapped through, at `width` by `height`: made
 * once for the size a preview asks for and reused by every readback after it,
 * which is what makes the hundredth tile cost a map and a copy rather than an
 * allocation. */
static int staging_texture(struct yapfast_d3d11enc *e, unsigned width, unsigned height, char *why,
                           size_t size) {
  D3D11_TEXTURE2D_DESC desc;
  HRESULT status;
  if (e->scaler.staging && e->scaler.staging_width == width &&
      e->scaler.staging_height == height)
    return 0;
  if (e->scaler.staging) {
    e->scaler.staging->lpVtbl->Release(e->scaler.staging);
    e->scaler.staging = NULL;
  }
  memset(&desc, 0, sizeof desc);
  desc.Width = width;
  desc.Height = height;
  desc.MipLevels = 1;
  desc.ArraySize = 1;
  desc.Format = DXGI_FORMAT_B8G8R8A8_UNORM;
  desc.SampleDesc.Count = 1;
  /* Staging and nothing else: a texture the processor reads and never draws, so
   * a tile is one copy and one map of the size the caller asked for. */
  desc.Usage = D3D11_USAGE_STAGING;
  desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ;
  status = e->device->lpVtbl->CreateTexture2D(e->device, &desc, NULL, &e->scaler.staging);
  if (FAILED(status)) {
    e->scaler.staging = NULL;
    hr_failure(why, size, "CreateTexture2D for the readback's staging texture", status);
    return -1;
  }
  e->scaler.staging_width = width;
  e->scaler.staging_height = height;
  e->scaler.stagings++;
  return 0;
}
/* Reads the picture in `texture`'s `subresource` back as BGRA at `width` by
 * `height`, rows packed one after another with no padding into the caller's
 * `pixels`, which has `size` bytes: a preview's tile, not the whole screen. It
 * is scaled on the GPU into the encoder's destination, copied there into the
 * one staging texture this keeps, and mapped — so what crosses the processor is
 * the tile and nothing else, and nothing of it is allocated per call. Never
 * larger than the picture: a tile is a smaller look at a screen.
 *
 * Answers the bytes written, which is the tile's `width * height * 4`, or -1
 * with `yapfast_d3d11enc_error` saying why. */
long yapfast_d3d11enc_readback(struct yapfast_d3d11enc *e, void *texture, unsigned subresource,
                               unsigned width, unsigned height, void *pixels, size_t size) {
  ID3D11Texture2D *source = texture;
  ID3D11DeviceContext *context;
  D3D11_TEXTURE2D_DESC desc;
  D3D11_MAPPED_SUBRESOURCE mapped;
  HRESULT status;
  NV_ENC_BUFFER_FORMAT registered;
  char why[192] = "";
  size_t needed;
  ID3D11Texture2D *from;
  int direct;
  if (!e) return -1;
  if (!source || !pixels) return fail(e, -1, "the readback was given nothing to read");
  if (!width || !height)
    return fail(e, -1, "%ux%u isn't a size anything can be read back at", width, height);
  needed = (size_t)width * (size_t)height * 4;
  if (size < needed)
    return fail(e, -1, "a %ux%u picture is %zu bytes and the readback was given %zu", width,
                height, needed, size);
  source->lpVtbl->GetDesc(source, &desc);
  if (subresource >= desc.MipLevels * desc.ArraySize)
    return fail(e, -1, "the texture has no subresource %u", subresource);
  if (!texture_format(desc.Format, &registered))
    return fail(e,
                -1,
                "the texture is DXGI format %u, and this reads BGRA, RGBA or NV12",
                (unsigned)desc.Format);
  if (width > desc.Width || height > desc.Height)
    return fail(e, -1,
                "a %ux%u readback of a %ux%u picture is larger than the picture, and a tile is "
                "a smaller look at a screen rather than a bigger one",
                width, height, desc.Width, desc.Height);
  /* A picture that is already the size the caller asked for and already BGRA is
   * its own source: the copy is exact, so a readback of a texture's own pixels
   * is those pixels and not a scaler's idea of them. */
  direct = width == desc.Width && height == desc.Height && desc.Format == DXGI_FORMAT_B8G8R8A8_UNORM;
  if (!direct && scale_picture(e, source, subresource, desc.Format, desc.Width, desc.Height, width,
                               height, why, sizeof why))
    return fail(e, -1, "%s", why);
  if (staging_texture(e, width, height, why, sizeof why)) return fail(e, -1, "%s", why);
  context = e->context;
  from = direct ? source : e->scaler.picture;
  if (direct)
    context->lpVtbl->CopySubresourceRegion(context, (ID3D11Resource *)e->scaler.staging, 0, 0, 0, 0,
                                           (ID3D11Resource *)from, subresource, NULL);
  else
    context->lpVtbl->CopyResource(context, (ID3D11Resource *)e->scaler.staging,
                                  (ID3D11Resource *)from);
  memset(&mapped, 0, sizeof mapped);
  status = context->lpVtbl->Map(context, (ID3D11Resource *)e->scaler.staging, 0, D3D11_MAP_READ, 0,
                                &mapped);
  if (FAILED(status))
    return fail(e, -1, "Map of the readback's staging texture failed (0x%08lX)",
                (unsigned long)status);
  /* A texture's rows are padded to the pitch the driver likes; the caller's are
   * not, so a tile is copied a row at a time into the buffer it was given. */
  for (unsigned row = 0; row < height; row++)
    memcpy((unsigned char *)pixels + (size_t)row * width * 4,
           (const unsigned char *)mapped.pData + (size_t)row * mapped.RowPitch, (size_t)width * 4);
  context->lpVtbl->Unmap(context, (ID3D11Resource *)e->scaler.staging, 0);
  return (long)needed;
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
  /* The four quadrants a test holds the scaler to, compiled when it is first
   * asked for: a picture with known colours in known corners, where the moving
   * one is a measurement. */
  ID3D11ComputeShader *quads;
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

/* A picture refilled on the GPU by one of the source's shaders: three commands,
 * and nothing of the frame crosses the processor. `params` is the frame's
 * constants, or none for a picture that is the same every time. */
static void dispatch(struct yapfast_d3d11enc_source *source, ID3D11ComputeShader *shader,
                     ID3D11Buffer *params) {
  struct yapfast_d3d11enc *e = source->encoder;
  ID3D11DeviceContext *context = e->context;
  ID3D11UnorderedAccessView *unbound = NULL;
  context->lpVtbl->CSSetConstantBuffers(context, 0, 1, &params);
  context->lpVtbl->CSSetShader(context, shader, NULL, 0);
  context->lpVtbl->CSSetUnorderedAccessViews(context, 0, 1, &source->view, NULL);
  context->lpVtbl->Dispatch(context, (e->width + 7) / 8, (e->height + 7) / 8, 1);
  /* Unbound once the picture is drawn, so the texture can be read by the scaler
   * or registered by the encoder without the context still holding it for
   * writing. */
  context->lpVtbl->CSSetUnorderedAccessViews(context, 0, 1, &unbound, NULL);
}
void yapfast_d3d11enc_source_draw(struct yapfast_d3d11enc_source *source, unsigned frame) {
  if (!source) return;
  dispatch(source, source->shader, source->params[frame % source->frames]);
}

/* Four solid quadrants, which is the picture a scaler is held to: every corner of
 * the picture keeps its own colour, so a filter that mixed the quarters or put
 * the seam in the wrong place says so here where a moving picture would hide it.
 * The size is written into the source rather than passed in a constant buffer,
 * since the picture is a fixed one and the clip against the texture's size is all
 * it needs. Answers 0, or -1 with `why`. */
int yapfast_d3d11enc_source_quads(struct yapfast_d3d11enc_source *source, char *why, size_t size) {
  char text[768];
  ID3DBlob *blob = NULL, *errors = NULL;
  HRESULT status;
  if (why && size) why[0] = 0;
  if (!source) return -1;
  if (!source->quads) {
    struct yapfast_d3d11enc *e = source->encoder;
    snprintf(text, sizeof text,
             "RWTexture2D<float4> dst : register(u0);\n"
             "[numthreads(8,8,1)]\n"
             "void CSMain(uint3 id : SV_DispatchThreadID) {\n"
             "  if (id.x >= %u || id.y >= %u) return;\n"
             "  bool right = id.x * 2 >= %u;\n"
             "  bool bottom = id.y * 2 >= %u;\n"
             "  float3 c = right ? (bottom ? float3(0.8, 0.8, 0.8) : float3(0.8, 0.2, 0.2))\n"
             "                   : (bottom ? float3(0.2, 0.8, 0.2) : float3(0.2, 0.2, 0.8));\n"
             "  dst[id.xy] = float4(c, 1.0);\n"
             "}\n",
             e->width, e->height, e->width, e->height);
    status = D3DCompile(text, strlen(text), "yapfast-quads.hlsl", NULL, NULL, "CSMain", "cs_5_0",
                        D3DCOMPILE_OPTIMIZATION_LEVEL3, 0, &blob, &errors);
    if (FAILED(status)) {
      if (size)
        snprintf(why, size, "D3DCompile(quadrants) failed (0x%08lX) %.96s",
                 (unsigned long)status,
                 errors ? (const char *)errors->lpVtbl->GetBufferPointer(errors) : "");
      if (errors) errors->lpVtbl->Release(errors);
      return -1;
    }
    if (errors) errors->lpVtbl->Release(errors);
    status = e->device->lpVtbl->CreateComputeShader(
        e->device, blob->lpVtbl->GetBufferPointer(blob), blob->lpVtbl->GetBufferSize(blob), NULL,
        &source->quads);
    blob->lpVtbl->Release(blob);
    if (FAILED(status)) {
      source->quads = NULL;
      hr_failure(why, size, "CreateComputeShader for the quadrants", status);
      return -1;
    }
  }
  dispatch(source, source->quads, NULL);
  return 0;
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
  if (source->quads) source->quads->lpVtbl->Release(source->quads);
  if (source->texture) source->texture->lpVtbl->Release(source->texture);
  free(source);
}

/* ------------------------------------------------------- the shared picture */

/* The picture the UI's window shows where the card drew it.
 *
 * The window draws on a Direct3D 12 device in the UI's process, and a texture
 * this device shares is one it opens and shows without a pixel of it crossing
 * the processor. Three of them, one for each slot of the frames file: a shared
 * handle names a texture that the UI opens and keeps for as long as it shows it,
 * so one texture would be overwritten under the picture somebody is looking at,
 * and a slot is only drawn into once its reader has moved past the picture that
 * was there.
 *
 * The set is numbered, and the number only grows: a stream whose pictures change
 * size, or that stops and starts again, is another three textures and another
 * fence, and the number is what the UI tells the two apart by. A handle can't say
 * it, because Windows hands a closed handle's value out again.
 *
 * The fence is the promise. A value is signalled on the GPU after the copy into
 * one of the three, and it is the value, not the publish, that says the picture
 * is drawn: the UI waits for it before showing anything, and this file waits for
 * the last value a texture was written at before writing it again. The keyed
 * mutex Direct3D 11 requires beside an NT handle is taken for the copy and given
 * back at once, since the Direct3D 12 view of the resource has no mutex to take,
 * and the fence is what orders the two devices. */
/* The pictures a worker draws a stream's preview into, one for each slot of the
 * frames file: a slot is drawn into only when its reader has moved on, so the
 * number of pictures has to match the number of slots. One fence is shared by
 * all three, and it is what says a copy into any of them is finished. */
#define SHARED_PICTURES 3

struct yapfast_d3d11enc_shared {
  ID3D11Texture2D *pictures[SHARED_PICTURES];
  IDXGIKeyedMutex *mutexes[SHARED_PICTURES];
  /* The handles, as the UI's process holds them: what is published is these and
   * never the ones this process made, which mean nothing over there. */
  unsigned long long textures[SHARED_PICTURES];
  /* This process's own handle of each of those objects, kept until the set goes:
   * the UI closes the handle it opens, and Windows hands a closed value out
   * again, so this is what says whether the number a picture was published under
   * is still this set's object rather than somebody else's handle by now. */
  HANDLE kept[SHARED_PICTURES + 1];
  /* Every handle this set duplicated into the UI's process, in the order they
   * were made, and the process to close what is left of them in. */
  unsigned long long handed[SHARED_PICTURES + 1];
  unsigned ui;
  /* The number of this set, which only grows; see above. */
  unsigned long long set;
  /* The fence value each texture was last written at. */
  unsigned long long written[SHARED_PICTURES];
  ID3D11Fence *fence;
  /* The context's signalling half: a copy is only drawn once the fence says so,
   * and only the GPU can say it. */
  ID3D11DeviceContext4 *signals;
  unsigned long long fence_handle;
  unsigned long long value;
  unsigned long long luid;
  unsigned width, height;
  /* The picture drawn into last, which is the one a test reads back. */
  unsigned drawn;
};

/* Freed where a share is half made, below where it is defined: a share that
 * can't be finished is let go of in the same call that tried to make it. */
void yapfast_d3d11enc_shared_free(struct yapfast_d3d11enc_shared *s);

/* The handle a texture's copy is drawn with, kept until the set goes: see
 * `kept` above. */
static unsigned long long next_set(void) {
  static volatile LONG64 sets;
  return (unsigned long long)InterlockedIncrement64(&sets);
}

/* A handle as another process holds it. A handle's value means nothing outside
 * the process that took it, so what the UI opens is a duplicate made in its
 * table. The duplicate is not closed here: it is the UI's to close as it opens
 * the resource the handle names, and closing that number in this process would
 * close whatever this process has there. */
static int duplicate_into(HANDLE source, unsigned process, unsigned long long *out, char *why,
                          size_t size) {
  HANDLE target = OpenProcess(PROCESS_DUP_HANDLE, FALSE, (DWORD)process), copy = NULL;
  if (!target) {
    if (size)
      snprintf(why, size, "OpenProcess(%u) for the UI's handles failed (%lu)", process,
               (unsigned long)GetLastError());
    return -1;
  }
  if (!DuplicateHandle(GetCurrentProcess(), source, target, &copy, 0, FALSE,
                       DUPLICATE_SAME_ACCESS)) {
    if (size)
      snprintf(why, size, "DuplicateHandle into %u failed (%lu)", process,
               (unsigned long)GetLastError());
    CloseHandle(target);
    return -1;
  }
  CloseHandle(target);
  *out = (unsigned long long)(uintptr_t)copy;
  return 0;
}

/* The handles this set duplicated into the UI's process and nothing opened: the
 * ones a set that couldn't be finished left behind, and the ones of a set that
 * is going that the UI never opened a picture of.
 *
 * The UI closes each handle the moment the resource it names is open, so a value
 * that was closed may name another handle of the UI's by now. That is what
 * `kept` is for: the value is duplicated back here and compared with this
 * process's own handle of the object, and only a value that is still this set's
 * object is closed over there. A set that is going away with the UI process
 * already gone needs nothing: the handles went with the process. */
static void give_back(struct yapfast_d3d11enc_shared *s) {
  HANDLE ui;
  if (!s->ui) return;
  ui = OpenProcess(PROCESS_DUP_HANDLE, FALSE, (DWORD)s->ui);
  if (!ui) return;
  for (unsigned at = 0; at < SHARED_PICTURES + 1; at++) {
    HANDLE copy = NULL;
    if (!s->handed[at] || !s->kept[at]) continue;
    if (!DuplicateHandle(ui, (HANDLE)(uintptr_t)s->handed[at], GetCurrentProcess(), &copy, 0, FALSE,
                         DUPLICATE_SAME_ACCESS)) {
      /* Nothing has that number: the UI closed it, and the set left nothing
       * there to give back. */
      s->handed[at] = 0;
      continue;
    }
    if (CompareObjectHandles(copy, s->kept[at])) {
      /* This process takes a duplicate of it and closes the UI's in the same
       * call: `DUPLICATE_CLOSE_SOURCE` closes the source handle of the process
       * it is read from, and the duplicate it answers with is closed here at
       * once. A NULL out-pointer would leave the UI a handle for every one this
       * closes, which is the leak this whole path exists to end. */
      HANDLE closing = NULL;
      if (DuplicateHandle(ui, (HANDLE)(uintptr_t)s->handed[at], GetCurrentProcess(), &closing, 0,
                          FALSE, DUPLICATE_CLOSE_SOURCE))
        CloseHandle(closing);
    }
    CloseHandle(copy);
    s->handed[at] = 0;
  }
  CloseHandle(ui);
}

/* One of the three textures, and the mutex and handle that go with it. */
static int shared_texture(struct yapfast_d3d11enc *e, struct yapfast_d3d11enc_shared *s,
                          unsigned at, unsigned process, char *why, size_t size) {
  D3D11_TEXTURE2D_DESC desc;
  IDXGIResource1 *resource = NULL;
  IDXGIKeyedMutex *mutex = NULL;
  HANDLE handle = NULL;
  HRESULT status;
  memset(&desc, 0, sizeof desc);
  desc.Width = s->width;
  desc.Height = s->height;
  desc.MipLevels = 1;
  desc.ArraySize = 1;
  desc.Format = DXGI_FORMAT_B8G8R8A8_UNORM;
  desc.SampleDesc.Count = 1;
  desc.Usage = D3D11_USAGE_DEFAULT;
  desc.BindFlags = D3D11_BIND_RENDER_TARGET;
  /* The pair Direct3D 11 requires of a texture another API opens: an NT handle,
   * a real kernel object a process can duplicate, and the keyed mutex that goes
   * beside one. */
  desc.MiscFlags = D3D11_RESOURCE_MISC_SHARED_NTHANDLE | D3D11_RESOURCE_MISC_SHARED_KEYEDMUTEX;
  status = e->device->lpVtbl->CreateTexture2D(e->device, &desc, NULL, &s->pictures[at]);
  if (FAILED(status)) {
    /* A failed call leaves the out-pointer as it was, and this one is the slot's
     * own field: a reference it did answer with is let go of here rather than
     * assumed not to exist, since the caller's cleanup only sees what is NULL. */
    if (s->pictures[at]) s->pictures[at]->lpVtbl->Release(s->pictures[at]);
    s->pictures[at] = NULL;
    hr_failure(why, size, "CreateTexture2D for a shared picture", status);
    return -1;
  }
  status = s->pictures[at]->lpVtbl->QueryInterface(s->pictures[at], &IID_IDXGIResource1,
                                                   (void **)&resource);
  if (FAILED(status) || !resource) {
    if (resource) resource->lpVtbl->Release(resource);
    hr_failure(why, size, "QueryInterface(IDXGIResource1) on a shared picture", status);
    return -1;
  }
  status = resource->lpVtbl->CreateSharedHandle(resource, NULL, GENERIC_ALL, NULL, &handle);
  resource->lpVtbl->Release(resource);
  if (FAILED(status) || !handle) {
    if (handle) CloseHandle(handle);
    hr_failure(why, size, "IDXGIResource1::CreateSharedHandle for a shared picture", status);
    return -1;
  }
  status = s->pictures[at]->lpVtbl->QueryInterface(s->pictures[at], &IID_IDXGIKeyedMutex,
                                                   (void **)&mutex);
  if (FAILED(status) || !mutex) {
    if (mutex) mutex->lpVtbl->Release(mutex);
    CloseHandle(handle);
    hr_failure(why, size, "QueryInterface(IDXGIKeyedMutex) on a shared picture", status);
    return -1;
  }
  s->mutexes[at] = mutex;
  if (duplicate_into(handle, process, &s->textures[at], why, size)) {
    CloseHandle(handle);
    return -1;
  }
  s->handed[at] = s->textures[at];
  s->kept[at] = handle;
  return 0;
}

/* The fence the three textures are drawn behind, and its handle. Shared between
 * the APIs as Direct3D 12's fence because that is where the UI opens it; the
 * same object answers Direct3D 11's fence, which is the half that signals it. */
static int shared_fence(struct yapfast_d3d11enc *e, struct yapfast_d3d11enc_shared *s,
                        unsigned process, char *why, size_t size) {
  ID3D11Device5 *device = NULL;
  HANDLE handle = NULL;
  HRESULT status;
  status = e->device->lpVtbl->QueryInterface(e->device, &IID_ID3D11Device5, (void **)&device);
  if (FAILED(status) || !device) {
    if (device) device->lpVtbl->Release(device);
    hr_failure(why, size, "QueryInterface(ID3D11Device5) for a shared picture's fence", status);
    return -1;
  }
  /* Made as Direct3D 11's fence, which is the only face this object has. Asked
   * for Direct3D 12's instead, a device answers E_NOINTERFACE — measured on an
   * RTX 4090, before and after d3d12.dll is loaded — and every stream fell back
   * to writing pixels. Direct3D 12's half is the UI's, made by opening the
   * handle that is shared below, so which face is asked for here decides
   * nothing across the boundary. */
  status = device->lpVtbl->CreateFence(device, 0, D3D11_FENCE_FLAG_SHARED, &IID_ID3D11Fence,
                                       (void **)&s->fence);
  device->lpVtbl->Release(device);
  if (FAILED(status) || !s->fence) {
    if (s->fence) s->fence->lpVtbl->Release(s->fence);
    s->fence = NULL;
    hr_failure(why, size, "ID3D11Device5::CreateFence", status);
    return -1;
  }
  status = s->fence->lpVtbl->CreateSharedHandle(s->fence, NULL, GENERIC_ALL, NULL, &handle);
  if (FAILED(status) || !handle) {
    if (handle) CloseHandle(handle);
    hr_failure(why, size, "ID3D11Fence::CreateSharedHandle", status);
    return -1;
  }
  if (duplicate_into(handle, process, &s->fence_handle, why, size)) {
    CloseHandle(handle);
    return -1;
  }
  s->handed[SHARED_PICTURES] = s->fence_handle;
  s->kept[SHARED_PICTURES] = handle;
  return 0;
}

/* Three tile-sized pictures and one fence, on the device `e` encodes from, with
 * all four handles duplicated into `process`: the UI's, which is the one that
 * opens them. Answers NULL with `yapfast_d3d11enc_error` saying why, which is
 * the caller's cue to keep writing pixels. */
struct yapfast_d3d11enc_shared *yapfast_d3d11enc_shared_new(
    struct yapfast_d3d11enc *e, unsigned width, unsigned height, unsigned process,
    unsigned long long *fence, unsigned long long *luid, char *why, size_t size) {
  struct yapfast_d3d11enc_shared *s;
  IDXGIAdapter1 *adapter = NULL;
  DXGI_ADAPTER_DESC1 desc;
  char said[192] = "";
  if (why && size) why[0] = 0;
  if (!e) return nothing(e, "there is no encoder to share a picture with");
  if (!width || !height)
    return nothing(e, "%ux%u isn't a size to share a picture at", width, height);
  if (!process) return nothing(e, "there is no UI process to share a picture with");
  s = calloc(1, sizeof *s);
  if (!s) return nothing(e, "no memory for a shared picture");
  s->width = width;
  s->height = height;
  /* The UI's process before anything is duplicated into it: a set that can't be
   * finished gives back what it had already put there, which it can only do by
   * name. */
  s->ui = process;
  s->set = next_set();
  /* The adapter is the UI's test as much as the picture's: Direct3D opens a
   * shared texture on the adapter that made it and on no other, so the LUID is
   * what the UI compares its own window's device against before opening
   * anything. */
  if (adapter_of(e->device, &adapter, &desc, said, sizeof said)) goto failed;
  adapter->lpVtbl->Release(adapter);
  adapter = NULL;
  s->luid = (unsigned long long)desc.AdapterLuid.LowPart |
            ((unsigned long long)(unsigned long)desc.AdapterLuid.HighPart << 32);
  for (unsigned at = 0; at < SHARED_PICTURES; at++)
    if (shared_texture(e, s, at, process, said, sizeof said)) goto failed;
  if (shared_fence(e, s, process, said, sizeof said)) goto failed;
  if (e->context->lpVtbl->QueryInterface(e->context, &IID_ID3D11DeviceContext4,
                                         (void **)&s->signals) != S_OK ||
      !s->signals) {
    if (s->signals) s->signals->lpVtbl->Release(s->signals);
    s->signals = NULL;
    snprintf(said, sizeof said, "this context can't signal a fence (ID3D11DeviceContext4)");
    goto failed;
  }
  *fence = s->fence_handle;
  *luid = s->luid;
  return s;
failed:
  if (adapter) adapter->lpVtbl->Release(adapter);
  yapfast_d3d11enc_shared_free(s);
  if (!said[0]) snprintf(said, sizeof said, "a shared picture couldn't be made");
  if (why && size) snprintf(why, size, "%s", said);
  return nothing(e, "%s", said);
}

/* Scales `texture`'s picture into shared picture `at`, signals the fence with
 * the value that says it is drawn, and answers that texture's handle as the
 * UI's process holds it and the value. Answers 1, or 0 with
 * `yapfast_d3d11enc_error` saying why.
 *
 * The caller chooses the picture because it is the frames file's slot that says
 * one may be drawn into: a slot is only drawn into once its reader has moved
 * past the picture there, so the choice is the reader's pace and not a pairing
 * here. The wait before the copy is the reuse discipline this side still owns:
 * the GPU, not the processor, waits for the value this texture was last written
 * at, so a picture the driver is still filling is never written over. */
int yapfast_d3d11enc_shared_draw(struct yapfast_d3d11enc *e, struct yapfast_d3d11enc_shared *s,
                                 unsigned at, void *texture, unsigned subresource,
                                 unsigned long long *handle, unsigned long long *value) {
  ID3D11Texture2D *source = texture;
  D3D11_TEXTURE2D_DESC desc;
  NV_ENC_BUFFER_FORMAT registered;
  char why[192] = "";
  HRESULT status;
  if (!e) return -1;
  if (!s) return fail(e, -1, "no shared picture to draw into");
  if (at >= SHARED_PICTURES)
    return fail(e, -1, "there is no shared picture %u of %u", at, (unsigned)SHARED_PICTURES);
  if (!source) return fail(e, -1, "the shared picture was given nothing to draw");
  source->lpVtbl->GetDesc(source, &desc);
  if (subresource >= desc.MipLevels * desc.ArraySize)
    return fail(e, -1, "the texture has no subresource %u", subresource);
  if (!texture_format(desc.Format, &registered))
    return fail(e, -1,
                "the texture is DXGI format %u, and a shared picture is drawn from BGRA, RGBA or "
                "NV12",
                (unsigned)desc.Format);
  if (s->mutexes[at]->lpVtbl->AcquireSync(s->mutexes[at], 0, INFINITE) != S_OK)
    return fail(e, -1, "a shared picture couldn't be taken for drawing");
  status = s->signals->lpVtbl->Wait(s->signals, s->fence, s->written[at]);
  if (FAILED(status)) {
    s->mutexes[at]->lpVtbl->ReleaseSync(s->mutexes[at], 0);
    return fail(e, -1, "waiting for a shared picture's last frame failed (0x%08lX)",
                (unsigned long)status);
  }
  if (scale_picture(e, source, subresource, desc.Format, desc.Width, desc.Height, s->width,
                    s->height, why, sizeof why)) {
    s->mutexes[at]->lpVtbl->ReleaseSync(s->mutexes[at], 0);
    return fail(e, -1, "%s", why);
  }
  e->context->lpVtbl->CopyResource(e->context, (ID3D11Resource *)s->pictures[at],
                                   (ID3D11Resource *)e->scaler.picture);
  s->value++;
  status = s->signals->lpVtbl->Signal(s->signals, s->fence, s->value);
  if (FAILED(status)) {
    s->mutexes[at]->lpVtbl->ReleaseSync(s->mutexes[at], 0);
    return fail(e, -1, "signalling a shared picture's fence failed (0x%08lX)",
                (unsigned long)status);
  }
  s->written[at] = s->value;
  s->drawn = at;
  s->mutexes[at]->lpVtbl->ReleaseSync(s->mutexes[at], 0);
  *handle = s->textures[at];
  *value = s->value;
  return 1;
}

/* The fence's handle as the UI's process holds it: one fence for a stream's
 * pictures, waited on by the value each is published with. */
unsigned long long yapfast_d3d11enc_shared_fence(struct yapfast_d3d11enc_shared *s) {
  return s ? s->fence_handle : 0;
}

/* The number of this set of pictures and fence, which only grows: the UI lets go
 * of what it opened for the set before when a picture names this one. */
unsigned long long yapfast_d3d11enc_shared_set(struct yapfast_d3d11enc_shared *s) {
  return s ? s->set : 0;
}

/* The adapter the shared pictures were made on, as the UI compares its own
 * window's device against. */
unsigned long long yapfast_d3d11enc_shared_luid(struct yapfast_d3d11enc_shared *s) {
  return s ? s->luid : 0;
}

/* The size a shared picture is, which is the size it is drawn at and the size the
 * UI opens it at. */
void yapfast_d3d11enc_shared_size(struct yapfast_d3d11enc_shared *s, unsigned *width,
                                  unsigned *height) {
  if (!s) return;
  if (width) *width = s->width;
  if (height) *height = s->height;
}

/* The texture the last draw wrote, as the ID3D11Texture2D it is on this device:
 * what a test reads back to see the picture the UI would show, and nothing the
 * stream path needs, which knows the handle it was answered with. */
void *yapfast_d3d11enc_shared_picture(struct yapfast_d3d11enc_shared *s) {
  return s ? s->pictures[s->drawn] : NULL;
}

/* The textures go with the share, and the UI's handles go with the UI: what the
 * UI opened, it closed as it opened it, and what it never opened is closed here
 * over there before the set is let go, so a set that failed halfway or was
 * stopped leaves no handle of the UI's behind. */
void yapfast_d3d11enc_shared_free(struct yapfast_d3d11enc_shared *s) {
  if (!s) return;
  give_back(s);
  for (unsigned at = 0; at < SHARED_PICTURES; at++) {
    if (s->mutexes[at]) s->mutexes[at]->lpVtbl->Release(s->mutexes[at]);
    if (s->pictures[at]) s->pictures[at]->lpVtbl->Release(s->pictures[at]);
    if (s->kept[at]) CloseHandle(s->kept[at]);
  }
  if (s->signals) s->signals->lpVtbl->Release(s->signals);
  if (s->fence) s->fence->lpVtbl->Release(s->fence);
  if (s->kept[SHARED_PICTURES]) CloseHandle(s->kept[SHARED_PICTURES]);
  free(s);
}
