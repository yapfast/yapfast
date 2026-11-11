/* Decoding a watched stream on an NVIDIA card under Linux, the third of the
 * card's decoders after native/hwdec.c's Direct3D 11 and native/hwdec_vaapi.c's
 * VA-API: NVDEC decodes each picture, and it leaves as a DMA-BUF the tile pass
 * imports, so no pixel of it comes back to the processor.
 *
 * Not through FFmpeg, which is what the other two use. The FFmpeg in
 * vendor/ffmpeg-src is configured without NVDEC — CONFIG_NVDEC and CONFIG_CUVID
 * are 0, no cuviddec.c is in the build, and every *_cuvid_decoder and
 * *_nvdec_hwaccel is off — so its decoders can't be asked for the card's. Its
 * libavutil/hwcontext_cuda.c is built, and is what NVENC encodes from, but it
 * has no path to a DMA-BUF at all: no AV_PIX_FMT_DRM_PRIME, no
 * cuMemGetHandleForAddressRange, so even with a CUDA decoder there the picture
 * would have had to cross to the processor before the tile could see it. Fixing
 * that means regenerating that FFmpeg's configure, which is a new build of it
 * for every target, not a change here.
 *
 * The driver is asked directly instead. NVDEC's parser and decoder are in
 * libnvcuvid.so.1, which NVIDIA's driver installs, and the structures they speak
 * are marketplace/ffnvcodec's, vendored in vendor/ffmpeg-src/include for NVENC:
 * declarations only, no toolkit. libcuda.so.1 is dlopened beside them, as
 * native/cudaenc.c and native/cudaconvert.c already do, so a machine without an
 * NVIDIA driver starts and falls back to the software decoders.
 *
 * The parser is what makes this small: cuvidParseVideoData() is handed the
 * stream's own access units and calls back with the decoder's parameters for
 * each picture — Annex B for H264 and HEVC, the OBUs as Discord sends them for
 * AV1 — so no bitstream is parsed here and no slice offset is found here.
 *
 * Two hand-offs, in the order they are tried:
 *
 *  - The driver's own surface, exported where NVDEC decoded it, with
 *    cuMemGetHandleForAddressRange(): no copy at all, which is the point. It
 *    needs the surface's address to be the driver's allocation rounded out to
 *    pages, which is why the fd's window is a page wider than the picture and
 *    each plane's offset carries the difference. A driver that won't export a
 *    surface it allocated itself — the cuvid API hands out a mapped pointer, not
 *    an allocation to set handle types on — refuses here, which is cheap to find
 *    out once per decoder.
 *  - The shim's own buffer on the card, made with cuMemAlloc, exported the same
 *    way and filled with one flat cuMemcpyDtoDAsync from the mapped surface: the
 *    picture is copied, but the copy stays on the card — a frame of memory
 *    bandwidth, no PCIe, no processor. Two of those buffers alternate, so the
 *    picture the tile is sampling is never the one being written, and the
 *    descriptor of each is exported once and kept.
 *
 * Every picture is one buffer of pitch-linear luma and chroma, whose chroma
 * plane starts pitch * height bytes in: that is what both an NVDEC surface and
 * the shim's copy are, and it is what the window imports through a modifier of
 * linear with each plane's offset and pitch given explicitly. Pitch is bytes,
 * so a 10-bit surface's pitch and plane size are twice an 8-bit one's and the
 * same arithmetic places both. A stream whose pictures are neither 8-bit nor
 * 10-bit 4:2:0 — 4:2:2, 4:4:4, 9-bit, 12-bit — is refused before the decoder
 * is made, and one whose 10-bit pictures carry a colour signal the tile pass
 * has no curve for (HLG, or BT.2020's constant luminance) at its sequence
 * header; the stream stays with the software decoder either way. A 10-bit
 * picture comes out as P016, whose 10 bits are the low bits of each 16-bit
 * sample, and it carries the matrix, range and transfer curve its VUI declared
 * out with it for the tile pass to show it in the colour it was coded with.
 *
 * Nothing is read back: the counters count pictures and copies on the card, and
 * the bytes that crossed to the processor, which stay zero. */

/* pthread_once is what loads the driver once for the process, readlink's family
 * gives the page size the export needs, and this file is GNU C, as the rest of
 * native/ is. */
#include <dlfcn.h>
#include <pthread.h>
#include <stdarg.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

/* The vendored ffnvcodec headers, which are declarations of the driver's CUDA
 * and NVDEC APIs and need no toolkit: relative to this file, so that the build
 * and an editor's language server resolve the same headers without an include
 * path either of them has to know. */
#include "../../../vendor/ffmpeg-src/include/ffnvcodec/dynlink_cuda.h"
#include "../../../vendor/ffmpeg-src/include/ffnvcodec/dynlink_nvcuvid.h"

#include "hwdec_cuda.h"

/* cuMemGetHandleForAddressRange, which the vendored dynlink_cuda.h predates: it
 * is what turns a range of the card's memory into a file descriptor the kernel,
 * and so the window, can take as a DMA-BUF. The declaration matches cuda.h's,
 * as the virtual-memory management API below does: cuMemAlloc's memory can't be
 * handed out at all, so a picture the shim copies is in an allocation made with
 * cuMemCreate, whose own shareable handle cuMemExportToShareableHandle gives.
 * Only declarations are borrowed: no CUDA toolkit is needed to build this. */
#define YAPFAST_CU_MEM_RANGE_HANDLE_TYPE_DMA_BUF_FD 1
typedef CUresult (*tcuMemGetHandleForAddressRange)(void *handle, CUdeviceptr dptr, size_t size,
                                                   int handle_type, unsigned long long flags);

/* CUmemGenericAllocationHandle, and the values of the two enumerations the calls
 * below take; the layout of each structure is cuda.h's. */
typedef struct CUmemGenericAllocationHandle_st *CUmemGenericAllocationHandle;
#define YAPFAST_CU_MEM_HANDLE_TYPE_POSIX_FILE_DESCRIPTOR 1
typedef struct {
  int type;
  int id;
} CUmemLocation;
typedef struct {
  int type;
  int requested_handle_types;
  CUmemLocation location;
  void *win32_handle_metadata;
  struct {
    unsigned char compression_type;
    unsigned char gpu_direct_rdma_capable;
    unsigned short usage;
    unsigned char reserved[4];
  } alloc_flags;
} CUmemAllocationProp;
typedef struct {
  CUmemLocation location;
  int flags;
} CUmemAccessDesc;
#define YAPFAST_CU_MEM_ALLOCATION_TYPE_PINNED 1
#define YAPFAST_CU_MEM_LOCATION_TYPE_DEVICE 1
#define YAPFAST_CU_MEM_ACCESS_FLAGS_PROT_READWRITE 3
#define YAPFAST_CU_MEM_ALLOC_GRANULARITY_MINIMUM 0

typedef CUresult (*tcuMemGetAllocationGranularity)(size_t *granularity,
                                                   const CUmemAllocationProp *prop, int option);
typedef CUresult (*tcuMemCreate)(CUmemGenericAllocationHandle *handle, size_t size,
                                 const CUmemAllocationProp *prop, unsigned long long flags);
typedef CUresult (*tcuMemAddressReserve)(CUdeviceptr *ptr, size_t size, size_t alignment,
                                         CUdeviceptr address, unsigned long long flags);
typedef CUresult (*tcuMemMap)(CUdeviceptr ptr, size_t size, size_t offset,
                             CUmemGenericAllocationHandle handle, unsigned long long flags);
typedef CUresult (*tcuMemSetAccess)(CUdeviceptr ptr, size_t size, const CUmemAccessDesc *desc,
                                   size_t count);
typedef CUresult (*tcuMemExportToShareableHandle)(void *shareable_handle,
                                                 CUmemGenericAllocationHandle handle,
                                                 int handle_type, unsigned long long flags);
typedef CUresult (*tcuMemUnmap)(CUdeviceptr ptr, size_t size);
typedef CUresult (*tcuMemRelease)(CUmemGenericAllocationHandle handle);
typedef CUresult (*tcuMemAddressFree)(CUdeviceptr ptr, size_t size);

/* The widest or tallest picture a stream may send, as in native/hwdec_vaapi.c. */
enum { MAX_SIDE = 8192 };

/* The surfaces one stream's decoder holds: H264's sixteen reference pictures at
 * most, the rest giving the card room to decode ahead. A stream whose decoder
 * says it needs more gets what it asks for. */
/* The surfaces the decoder itself keeps: two pictures in flight and room for the
 * codec's reference chain, which is what the parser is allowed to ask for. */
enum { POOL = 20, SURFACES = 32 };

/* How much of the card's memory a picture's DMA-BUF is described over: the
 * surface's address is only page-aligned after rounding down, so the exported
 * window starts before the picture and the plane offsets carry the difference.
 * The page is the machine's own, which is 4 KiB on the x86_64 this decodes on and
 * 16 or 64 KiB elsewhere. */
static size_t page_bytes(void) {
  static size_t page;
  if (!page) {
    /* getpagesize() is the machine's page as the kernel maps one, which is what
     * the export is aligned to. */
    const int asked = getpagesize();
    page = asked > 0 ? (size_t)asked : 4096;
  }
  return page;
}

/* FFmpeg's own padding for the same reason as this one: a bitstream parser reads
 * a little past the end of the frame it was given. */
enum { PADDING = 64 };

/* DRM's four name bytes for NV12 and P016, as native/hwdec_vaapi.c spells
 * them: the two shapes the tile pass shows. Not libdrm's header — this needs
 * the two names and no more — and P016 is the kernel's own name for a 16-bit
 * sample holding 10 bits at its bottom, which is what NVDEC decodes into. */
#define YAPFAST_FOURCC(a, b, c, d)                                                 \
  ((uint32_t)(a) | ((uint32_t)(b) << 8) | ((uint32_t)(c) << 16) | ((uint32_t)(d) << 24))
#define YAPFAST_DRM_FORMAT_NV12 YAPFAST_FOURCC('N', 'V', '1', '2')
#define YAPFAST_DRM_FORMAT_P016 YAPFAST_FOURCC('P', '0', '1', '6')

/* --------------------------------------------------------------- the driver */

/* The driver's entry points, resolved from the two libraries it installs. They
 * are the same for every session — one libcuda, one libnvcuvid, per process —
 * so they are looked up once, and looked up rather than linked so that a machine
 * with no NVIDIA driver is a machine that falls back to the software decoders
 * instead of one that can't start. */
struct driver {
  void *cuda;
  void *cuvid;
  CUresult (*cuInit)(unsigned);
  CUresult (*cuDeviceGet)(CUdevice *, int);
  CUresult (*cuDeviceGetName)(char *, int, CUdevice);
  CUresult (*cuCtxCreate)(CUcontext *, unsigned, CUdevice);
  CUresult (*cuCtxDestroy)(CUcontext);
  CUresult (*cuCtxPushCurrent)(CUcontext);
  CUresult (*cuCtxPopCurrent)(CUcontext *);
  CUresult (*cuMemAlloc)(CUdeviceptr *, size_t);
  CUresult (*cuMemFree)(CUdeviceptr);
  CUresult (*cuMemcpyDtoDAsync)(CUdeviceptr, CUdeviceptr, size_t, CUstream);
  CUresult (*cuStreamCreate)(CUstream *, unsigned);
  CUresult (*cuStreamDestroy)(CUstream);
  CUresult (*cuStreamSynchronize)(CUstream);
  CUresult (*cuGetErrorName)(CUresult, const char **);
  tcuMemGetHandleForAddressRange cuMemGetHandleForAddressRange;
  tcuMemGetAllocationGranularity cuMemGetAllocationGranularity;
  tcuMemCreate cuMemCreate;
  tcuMemAddressReserve cuMemAddressReserve;
  tcuMemMap cuMemMap;
  tcuMemSetAccess cuMemSetAccess;
  tcuMemExportToShareableHandle cuMemExportToShareableHandle;
  tcuMemUnmap cuMemUnmap;
  tcuMemRelease cuMemRelease;
  tcuMemAddressFree cuMemAddressFree;
  tcuvidGetDecoderCaps *cuvidGetDecoderCaps;
  tcuvidCreateVideoParser *cuvidCreateVideoParser;
  tcuvidParseVideoData *cuvidParseVideoData;
  tcuvidDestroyVideoParser *cuvidDestroyVideoParser;
  tcuvidCreateDecoder *cuvidCreateDecoder;
  tcuvidDestroyDecoder *cuvidDestroyDecoder;
  tcuvidDecodePicture *cuvidDecodePicture;
  tcuvidMapVideoFrame64 *cuvidMapVideoFrame;
  tcuvidUnmapVideoFrame64 *cuvidUnmapVideoFrame;
  /* Why the driver couldn't be loaded, kept for the sentence a stream that fell
   * to the software decoder quotes. */
  char why[256];
};

static struct driver g_driver;
static pthread_once_t g_driver_once = PTHREAD_ONCE_INIT;

/* The last thing dlopen or dlsym said went wrong: an absent library's own reason
 * is what tells a missing driver from a broken install. */
static const char *dl_reason(void) {
  const char *reason = dlerror();
  return reason ? reason : "the loader gave no reason";
}

/* One entry point, by the name the driver exports, or by its versioned name where
 * the API was revised: libcuda exports both, and a driver older than the revision
 * exports only the first. */
static int take(void *library, const char *name, const char *versioned, void **into,
                char *why, size_t size) {
  *into = dlsym(library, versioned ? versioned : name);
  if (!*into) *into = dlsym(library, name);
  if (*into) return 0;
  snprintf(why, size, "%s isn't in the driver (%s)", name, dl_reason());
  return -1;
}

#define TAKE(field, name)                                                                       \
  do {                                                                                          \
    if (take(g_driver.cuda, name, name "_v2", (void **)&g_driver.field, g_driver.why,            \
             sizeof g_driver.why) < 0)                                                          \
      return;                                                                                   \
  } while (0)
#define TAKE_PLAIN(field, name)                                                                  \
  do {                                                                                           \
    if (take(g_driver.cuda, name, NULL, (void **)&g_driver.field, g_driver.why,                  \
             sizeof g_driver.why) < 0)                                                           \
      return;                                                                                    \
  } while (0)
#define TAKE_CUVID(field, name)                                                                  \
  do {                                                                                           \
    if (take(g_driver.cuvid, name, NULL, (void **)&g_driver.field, g_driver.why,                 \
             sizeof g_driver.why) < 0)                                                           \
      return;                                                                                    \
  } while (0)

/* libcuda.so.1 holds the CUDA driver API, which every NVIDIA machine with a
 * driver has; libnvcuvid.so.1 the NVDEC parser and decoder. Both are opened with
 * RTLD_LOCAL and kept open to the process's end, so that the handles the sessions
 * hold stay valid. Native/hwdec_vaapi.c's sibling, this file is built by a
 * distribution's toolchain as often as by NVIDIA's: a build whose loader searches
 * only its own prefix — a Nix shell's glibc, for one — has to be told where the
 * driver installed itself, so that directory is tried by name as well. */
static void load_driver(void) {
  static const char *const cuda_names[] = {"libcuda.so.1", "libcuda.so", "/usr/lib/libcuda.so.1",
                                           NULL};
  static const char *const cuvid_names[] = {"libnvcuvid.so.1", "libnvcuvid.so",
                                            "/usr/lib/libnvcuvid.so.1", NULL};
  for (int i = 0; cuda_names[i] && !g_driver.cuda; i++)
    g_driver.cuda = dlopen(cuda_names[i], RTLD_NOW | RTLD_LOCAL);
  if (!g_driver.cuda) {
    snprintf(g_driver.why, sizeof g_driver.why,
             "the NVIDIA driver's libcuda.so.1 didn't load (%s)", dl_reason());
    return;
  }
  TAKE_PLAIN(cuInit, "cuInit");
  TAKE_PLAIN(cuDeviceGet, "cuDeviceGet");
  TAKE_PLAIN(cuDeviceGetName, "cuDeviceGetName");
  TAKE(cuCtxCreate, "cuCtxCreate");
  TAKE(cuCtxDestroy, "cuCtxDestroy");
  TAKE(cuCtxPushCurrent, "cuCtxPushCurrent");
  TAKE(cuCtxPopCurrent, "cuCtxPopCurrent");
  TAKE(cuMemAlloc, "cuMemAlloc");
  TAKE(cuMemFree, "cuMemFree");
  /* The copy's versioned entry point, before the one that isn't: libcuda's
   * unversioned cuMemcpyDtoD and cuMemcpyDtoDAsync answer
   * CUDA_ERROR_INVALID_CONTEXT on a driver of 620, where the _v2 symbols — the
   * ones cuda.h's own macros name — copy. */
  TAKE(cuMemcpyDtoDAsync, "cuMemcpyDtoDAsync");
  TAKE_PLAIN(cuStreamCreate, "cuStreamCreate");
  TAKE(cuStreamDestroy, "cuStreamDestroy");
  TAKE_PLAIN(cuStreamSynchronize, "cuStreamSynchronize");
  TAKE_PLAIN(cuGetErrorName, "cuGetErrorName");
  for (int i = 0; cuvid_names[i] && !g_driver.cuvid; i++)
    g_driver.cuvid = dlopen(cuvid_names[i], RTLD_NOW | RTLD_LOCAL);
  if (!g_driver.cuvid) {
    snprintf(g_driver.why, sizeof g_driver.why,
             "NVIDIA's libnvcuvid.so.1 didn't load (%s): the driver has no NVDEC decoder",
             dl_reason());
    return;
  }
  TAKE_CUVID(cuvidGetDecoderCaps, "cuvidGetDecoderCaps");
  TAKE_CUVID(cuvidCreateVideoParser, "cuvidCreateVideoParser");
  TAKE_CUVID(cuvidParseVideoData, "cuvidParseVideoData");
  TAKE_CUVID(cuvidDestroyVideoParser, "cuvidDestroyVideoParser");
  TAKE_CUVID(cuvidCreateDecoder, "cuvidCreateDecoder");
  TAKE_CUVID(cuvidDestroyDecoder, "cuvidDestroyDecoder");
  TAKE_CUVID(cuvidDecodePicture, "cuvidDecodePicture");
  TAKE_CUVID(cuvidMapVideoFrame, "cuvidMapVideoFrame64");
  TAKE_CUVID(cuvidUnmapVideoFrame, "cuvidUnmapVideoFrame64");
  /* The export is not looked up here: a driver too old to have it isn't a
   * machine that can't decode, only one whose picture has to be copied on the
   * card, and the copy route asks for the entry points when it needs them. */
  g_driver.cuMemGetHandleForAddressRange =
      (tcuMemGetHandleForAddressRange)dlsym(g_driver.cuda, "cuMemGetHandleForAddressRange");
  g_driver.cuMemGetAllocationGranularity =
      (tcuMemGetAllocationGranularity)dlsym(g_driver.cuda, "cuMemGetAllocationGranularity");
  g_driver.cuMemCreate = (tcuMemCreate)dlsym(g_driver.cuda, "cuMemCreate");
  g_driver.cuMemAddressReserve =
      (tcuMemAddressReserve)dlsym(g_driver.cuda, "cuMemAddressReserve");
  g_driver.cuMemMap = (tcuMemMap)dlsym(g_driver.cuda, "cuMemMap");
  g_driver.cuMemSetAccess = (tcuMemSetAccess)dlsym(g_driver.cuda, "cuMemSetAccess");
  g_driver.cuMemExportToShareableHandle =
      (tcuMemExportToShareableHandle)dlsym(g_driver.cuda, "cuMemExportToShareableHandle");
  g_driver.cuMemUnmap = (tcuMemUnmap)dlsym(g_driver.cuda, "cuMemUnmap");
  g_driver.cuMemRelease = (tcuMemRelease)dlsym(g_driver.cuda, "cuMemRelease");
  g_driver.cuMemAddressFree = (tcuMemAddressFree)dlsym(g_driver.cuda, "cuMemAddressFree");
}

/* -------------------------------------------------------------- the session */

/* One buffer of the shim's own on the card: an allocation the driver will hand
 * out as a DMA-BUF, the address it is mapped at, the file descriptor that names
 * it, and how much of the card's memory it took. */
struct slot {
  CUmemGenericAllocationHandle handle;
  CUdeviceptr pointer;
  size_t window;
  int fd;
};

struct yapfast_cudec {
  CUcontext context;
  CUvideoparser parser;
  CUvideodecoder decoder;
  CUstream stream;
  /* The codec, as decode.rs numbers it, and the driver's own name for it. */
  int codec;
  cudaVideoCodec driver_codec;
  /* The device the context is on, which the memory a picture is copied into is
   * allocated for. */
  CUdevice device;
  /* What the stream's pictures are, from the sequence callback: the visible
   * picture the tile shows, and the coded height and pitch its surface's chroma
   * plane is placed by. `described` says a sequence header has been seen, and
   * `ten_bit` that the samples are 16-bit with 10 bits in them, which is what
   * makes the picture P016 rather than NV12. */
  int width;
  int height;
  int surface_height;
  unsigned int pitch;
  int described;
  int ten_bit;
  /* How many frames the parser has read without ever saying what the stream's
   * pictures are, which is how it answers a profile it has no decoder for: a
   * H264 High 10 stream on a GeForce is parsed and dropped without a word. */
  int undescribed;
  /* The colour signal the newest picture carries, as the numbers
   * hwdec_cuda.h lists: the range, matrix and curve its samples are in, taken
   * from the sequence header's own VUI. */
  struct yapfast_cucolour colour;
  /* Set where the pictures are ones the tile pass can't show, which
   * yapfast_cuvadec_decode answers with -2 rather than as a plain failure. */
  int refused;
  /* A picture the parser said to display, mapped and not yet exported: the
   * pointer is held until it is exported where it lies or copied, and a mapped
   * picture is one the decoder may not reuse. */
  int mapped;
  CUdeviceptr mapped_pointer;
  unsigned int mapped_pitch;
  int fresh;
  /* The picture the caller has: the descriptor of a surface the driver exported,
   * which is closed at release, or one of the two buffers below, whose
   * descriptors outlive the picture they hold. `exported_size` and
   * `exported_shift` are the window the descriptor is over and where the picture
   * starts in it. */
  int exported;
  int direct;
  int direct_refused;
  int fd;
  size_t exported_size;
  uint32_t exported_shift;
  /* The two buffers of the shim's own the picture is copied into, and which the
   * descriptor the caller has names. `window` is how much memory one took, which
   * is the picture rounded up to the driver's allocation granularity. */
  struct slot slot[2];
  size_t slot_window;
  int slot_last;
  unsigned char *padded;
  size_t padded_size;
  struct yapfast_cucounts counts;
  char error[256];
};

/* The reason a decoder that couldn't be made wasn't, kept for the caller that
 * gets the null back: hwdec_cuda.h promises it from yapfast_cuvadec_error(NULL). */
static _Thread_local char g_error[256];

static void fail_text(struct yapfast_cudec *decoder, const char *what) {
  snprintf(decoder->error, sizeof decoder->error, "%s", what);
}

/* What the driver called a failure, as its own name for it: an absent value means
 * a driver too old to name one. */
static const char *status_name(CUresult status) {
  const char *name = NULL;
  if (g_driver.cuGetErrorName && g_driver.cuGetErrorName(status, &name) == CUDA_SUCCESS && name)
    return name;
  return "an error the driver has no name for";
}

/* `what`, and the name the driver gave the failure: a stream that fell to the
 * software decoder says why the card's decoder didn't take it. */
static void fail_status(struct yapfast_cudec *decoder, const char *what, CUresult status) {
  snprintf(decoder->error, sizeof decoder->error, "%s (%s)", what, status_name(status));
}

static const char *codec_name(int codec) {
  switch (codec) {
    case YAPFAST_CU_H264:
      return "H264";
    case YAPFAST_CU_HEVC:
      return "H265";
    case YAPFAST_CU_AV1:
      return "AV1";
    default:
      return "this codec";
  }
}

/* The driver's own number for the codec, or -1 for one NVDEC has none for. */
static int driver_codec_of(int codec, cudaVideoCodec *out) {
  switch (codec) {
    case YAPFAST_CU_H264:
      *out = cudaVideoCodec_H264;
      return 0;
    case YAPFAST_CU_HEVC:
      *out = cudaVideoCodec_HEVC;
      return 0;
    case YAPFAST_CU_AV1:
      *out = cudaVideoCodec_AV1;
      return 0;
    default:
      return -1;
  }
}

/* The context is the decoder's own, and a stream's worker thread is the one place
 * it decodes; pushing it per call keeps a decoder that is passed to another
 * thread working rather than failing in the driver with a status nobody could
 * act on. Everything the parser's callbacks run is inside a push. */
static int push_context(struct yapfast_cudec *decoder) {
  const CUresult status = g_driver.cuCtxPushCurrent(decoder->context);
  if (status != CUDA_SUCCESS) {
    fail_status(decoder, "the card's context wasn't made current", status);
    return -1;
  }
  return 0;
}

static void pop_context(struct yapfast_cudec *decoder) {
  CUcontext popped = NULL;
  (void)decoder;
  g_driver.cuCtxPopCurrent(&popped);
}

/* Lets go of the picture the caller has: a surface the driver exported goes back
 * to the decoder, since a mapped picture is one it may not decode into, and its
 * descriptor is closed. A picture in one of the shim's own buffers needs neither
 * — its descriptor and its memory are the decoder's. */
void yapfast_cuvadec_release(struct yapfast_cudec *decoder) {
  if (!decoder) return;
  decoder->exported = 0;
  decoder->exported_size = 0;
  decoder->exported_shift = 0;
  if (decoder->mapped && decoder->decoder) {
    /* Mapping belongs to the context that made it: a picture is handed back
     * whether or not this thread had that context current, since a surface the
     * driver believes is still mapped is one it can't decode into again. */
    const int current = push_context(decoder);
    g_driver.cuvidUnmapVideoFrame(decoder->decoder, (unsigned long long)decoder->mapped_pointer);
    if (current) pop_context(decoder);
    decoder->mapped = 0;
    decoder->mapped_pointer = 0;
  }
  /* A descriptor the shim opened for a picture the driver exported where it lay.
   * A slot's is drop_slots's to close: it names the same buffer as the picture
   * that used it and outlives the picture. */
  if (decoder->direct && decoder->fd >= 0) {
    close(decoder->fd);
    decoder->fd = -1;
  }
  decoder->fd = -1;
  decoder->direct = 0;
}

/* Frees what the shim's own buffers were, when the stream's size changed or the
 * decoder is being dropped: their descriptors close with their memory. */
static void drop_slots(struct yapfast_cudec *decoder) {
  for (int which = 0; which < 2; which++) {
    struct slot *slot = &decoder->slot[which];
    if (slot->fd >= 0) {
      close(slot->fd);
      slot->fd = -1;
    }
    if (slot->pointer) {
      /* The descriptor names the memory as long as its own reference to it is
       * open, so the mapping goes back even while a window still holds it. */
      if (g_driver.cuMemUnmap) g_driver.cuMemUnmap(slot->pointer, slot->window);
      if (g_driver.cuMemRelease) g_driver.cuMemRelease(slot->handle);
      if (g_driver.cuMemAddressFree) g_driver.cuMemAddressFree(slot->pointer, slot->window);
      slot->pointer = 0;
      slot->handle = NULL;
      slot->window = 0;
    }
  }
  decoder->slot_window = 0;
  decoder->slot_last = 0;
}

/* A buffer of the shim's own on the card, at least `size` bytes, and its
 * DMA-BUF: the inside of the fallback hand-off.
 *
 * Not with cuMemAlloc, whose memory the driver won't hand out at all: a range is
 * exported only from an allocation made with cuMemCreate for exactly that,
 * which is why the picture is in a shareable allocation of its own, mapped into
 * the context and given to the window as the descriptor cuMemExportToShareableHandle
 * answers. The allocation is the driver's granularity — 2 MiB of the card's
 * memory here — so a picture of another size takes that much and no more, and
 * the descriptor's planes are at their own offsets from its start. */
static int make_slot(struct yapfast_cudec *decoder, int which, size_t size) {
  if (!g_driver.cuMemCreate || !g_driver.cuMemAddressReserve || !g_driver.cuMemMap ||
      !g_driver.cuMemSetAccess || !g_driver.cuMemExportToShareableHandle ||
      !g_driver.cuMemGetAllocationGranularity) {
    fail_text(decoder,
              "this driver can't hand a picture out as a DMA-BUF (no cuMemCreate and "
              "cuMemExportToShareableHandle: it is older than 520)");
    return -1;
  }
  CUmemAllocationProp prop;
  memset(&prop, 0, sizeof prop);
  prop.type = YAPFAST_CU_MEM_ALLOCATION_TYPE_PINNED;
  prop.requested_handle_types = YAPFAST_CU_MEM_HANDLE_TYPE_POSIX_FILE_DESCRIPTOR;
  prop.location.type = YAPFAST_CU_MEM_LOCATION_TYPE_DEVICE;
  prop.location.id = decoder->device;
  size_t granularity = 0;
  CUresult status =
      g_driver.cuMemGetAllocationGranularity(&granularity, &prop, YAPFAST_CU_MEM_ALLOC_GRANULARITY_MINIMUM);
  if (status != CUDA_SUCCESS || !granularity) {
    fail_status(decoder, "the card wouldn't say how it holds a picture", status);
    return -1;
  }
  const size_t window = (size + granularity - 1) / granularity * granularity;
  CUmemGenericAllocationHandle handle = NULL;
  status = g_driver.cuMemCreate(&handle, window, &prop, 0);
  if (status != CUDA_SUCCESS) {
    fail_status(decoder, "the card wouldn't hold a picture of the stream", status);
    return -1;
  }
  CUdeviceptr pointer = 0;
  status = g_driver.cuMemAddressReserve(&pointer, window, 0, 0, 0);
  if (status == CUDA_SUCCESS) status = g_driver.cuMemMap(pointer, window, 0, handle, 0);
  CUmemAccessDesc access;
  memset(&access, 0, sizeof access);
  access.location = prop.location;
  access.flags = YAPFAST_CU_MEM_ACCESS_FLAGS_PROT_READWRITE;
  if (status == CUDA_SUCCESS) status = g_driver.cuMemSetAccess(pointer, window, &access, 1);
  int fd = -1;
  if (status == CUDA_SUCCESS) {
    status = g_driver.cuMemExportToShareableHandle(
        &fd, handle, YAPFAST_CU_MEM_HANDLE_TYPE_POSIX_FILE_DESCRIPTOR, 0);
  }
  if (status != CUDA_SUCCESS) {
    if (pointer) {
      g_driver.cuMemUnmap(pointer, window);
      g_driver.cuMemAddressFree(pointer, window);
    }
    g_driver.cuMemRelease(handle);
    fail_status(decoder, "the card's memory wasn't handed out as a DMA-BUF", status);
    return -1;
  }
  struct slot *slot = &decoder->slot[which];
  slot->handle = handle;
  slot->pointer = pointer;
  slot->window = window;
  slot->fd = fd;
  return 0;
}

/* Makes the picture yapfast_cuvadec_picture describes out of the geometry the
 * sequence header gave: two planes of one buffer, chroma where a pitch-linear
 * surface puts it, linear layout. A 16-bit surface's pitch is bytes, so the
 * plane's offset and size follow the same arithmetic. */
static void describe_picture(const struct yapfast_cudec *decoder, struct yapfast_cupicture *picture,
                             int fd, size_t size, uint32_t shift) {
  memset(picture, 0, sizeof *picture);
  picture->width = decoder->width;
  picture->height = decoder->height;
  picture->fourcc = decoder->ten_bit ? YAPFAST_DRM_FORMAT_P016 : YAPFAST_DRM_FORMAT_NV12;
  picture->planes = YAPFAST_CU_PLANES;
  const uint32_t luma = decoder->pitch * (uint32_t)decoder->surface_height;
  for (int plane = 0; plane < YAPFAST_CU_PLANES; plane++) {
    picture->plane[plane].fd = fd;
    picture->plane[plane].offset = (plane == 0 ? 0 : luma) + shift;
    picture->plane[plane].stride = decoder->pitch;
    picture->plane[plane].size = size;
    picture->plane[plane].modifier = 0;
  }
}

/* Exports the mapped picture: where the driver decoded it if it will hand that
 * out, and otherwise from a copy of it on the card. 0 when the caller has a
 * picture, -1 with the reason when it hasn't. */
static int export_picture(struct yapfast_cudec *decoder) {
  if (!decoder->mapped) {
    fail_text(decoder, "the card gave no picture to export");
    return -1;
  }
  const unsigned int pitch = decoder->mapped_pitch;
  if (!pitch || !decoder->surface_height) {
    fail_text(decoder, "the card's picture has no pitch");
    return -1;
  }
  decoder->pitch = pitch;
  const size_t page = page_bytes();
  const size_t luma = (size_t)pitch * (size_t)decoder->surface_height;
  /* NV12's chroma plane is half as tall as the picture and starts where the
   * luma plane ends. A coded height the codec aligned is even, so this is the
   * whole surface, which is what the copy below moves in one go. */
  const size_t total = luma + luma / 2;
  if (push_context(decoder) < 0) return -1;
  if (!decoder->direct_refused && g_driver.cuMemGetHandleForAddressRange) {
    /* The surface's address is the driver's allocation's, which is page
     * aligned: rounding the window down to a page is what the export asks for,
     * and the picture's planes then start `shift` bytes into it. */
    const CUdeviceptr base = decoder->mapped_pointer & ~(CUdeviceptr)(page - 1);
    const uint32_t shift = (uint32_t)(decoder->mapped_pointer - base);
    const size_t window = (shift + total + page - 1) & ~(page - 1);
    int fd = -1;
    const CUresult status = g_driver.cuMemGetHandleForAddressRange(
        &fd, base, window, YAPFAST_CU_MEM_RANGE_HANDLE_TYPE_DMA_BUF_FD, 0);
    if (status == CUDA_SUCCESS) {
      decoder->fd = fd;
      decoder->direct = 1;
      decoder->exported = 1;
      decoder->exported_size = window;
      decoder->exported_shift = shift;
      decoder->counts.exported++;
      pop_context(decoder);
      return 0;
    }
    /* Once per decoder: the driver that won't export the surface it decoded
     * into says so the same way for every picture, and the copy route is the
     * one that works. */
    decoder->direct_refused = 1;
  }
  /* The copy route: the newest slot, so that the one the tile may still be
   * sampling is not the one being written. */
  if (decoder->slot_window < total) {
    drop_slots(decoder);
    for (int which = 0; which < 2; which++) {
      if (make_slot(decoder, which, total) < 0) {
        drop_slots(decoder);
        pop_context(decoder);
        return -1;
      }
    }
    decoder->slot_window = decoder->slot[0].window;
  }
  const int which = decoder->slot_last ^ 1;
  decoder->slot_last = which;
  const struct slot *slot = &decoder->slot[which];
  /* One flat move: both the surface and the buffer are the same pitch-linear
   * picture, so the copy is the whole plane pair at once rather than one line at
   * a time. */
  const CUresult status = g_driver.cuMemcpyDtoDAsync(slot->pointer, decoder->mapped_pointer,
                                                     total, decoder->stream);
  if (status != CUDA_SUCCESS) {
    fail_status(decoder, "the card wouldn't copy the stream's picture", status);
    pop_context(decoder);
    return -1;
  }
  /* The picture is the buffer's now, so the surface goes back to the decoder
   * for the pictures after it, while the context is still current. */
  g_driver.cuStreamSynchronize(decoder->stream);
  g_driver.cuvidUnmapVideoFrame(decoder->decoder, (unsigned long long)decoder->mapped_pointer);
  decoder->mapped = 0;
  decoder->mapped_pointer = 0;
  pop_context(decoder);
  decoder->fd = slot->fd;
  decoder->direct = 0;
  decoder->exported = 1;
  decoder->exported_size = slot->window;
  decoder->exported_shift = 0;
  decoder->counts.copied++;
  return 0;
}

int yapfast_cuvadec_picture(struct yapfast_cudec *decoder, struct yapfast_cupicture *picture) {
  if (!decoder || !picture) return -1;
  if (!decoder->exported) {
    fail_text(decoder, "no picture was exported");
    return -1;
  }
  describe_picture(decoder, picture, decoder->fd, decoder->exported_size, decoder->exported_shift);
  picture->colour = decoder->colour;
  return 0;
}

/* ------------------------------------------------------------- the callbacks */

/* H.273's code points for the matrix and transfer curve a stream's VUI
 * declares, which is what NVDEC hands over: the three the tile pass reads for
 * at 10 bits, and the ones it refuses by name. */
enum {
  YAPFAST_VUI_MATRIX_BT709 = 1,
  YAPFAST_VUI_MATRIX_BT2020_NCL = 9,
  YAPFAST_VUI_MATRIX_BT2020_CL = 10,
  YAPFAST_VUI_TRANSFER_BT709 = 1,
  YAPFAST_VUI_TRANSFER_BT2020_10 = 14,
  YAPFAST_VUI_TRANSFER_BT2020_12 = 15,
  YAPFAST_VUI_TRANSFER_PQ = 16,
  YAPFAST_VUI_TRANSFER_HLG = 18,
};

/* What the card says it decodes of this kind of picture, which is the driver's
 * own answer rather than a guess: a GeForce's NVDEC takes H264 at 8 bits and no
 * more, so a 10-bit H264 stream gets nothing from it however the decoder is
 * asked, and a stream that would decode into nothing must stay with the software
 * decoders instead. 1 says the card decodes it and whether it can give 16-bit
 * surfaces, 0 that no answer could be had and the decoder is worth trying, -1
 * with the reason that it can't. */
static int card_decodes(struct yapfast_cudec *decoder, unsigned int depth, unsigned int width,
                        unsigned int height, int *sixteen) {
  *sixteen = 0;
  if (!g_driver.cuvidGetDecoderCaps) return 0;
  CUVIDDECODECAPS caps;
  memset(&caps, 0, sizeof caps);
  caps.eCodecType = decoder->driver_codec;
  caps.eChromaFormat = cudaVideoChromaFormat_420;
  caps.nBitDepthMinus8 = depth;
  if (g_driver.cuvidGetDecoderCaps(&caps) != CUDA_SUCCESS) return 0;
  if (!caps.bIsSupported) {
    decoder->refused = 1;
    snprintf(decoder->error, sizeof decoder->error,
             "the card has no %s decoder for %u-bit pictures", codec_name(decoder->codec),
             depth + 8);
    return -1;
  }
  if (width > caps.nMaxWidth || height > caps.nMaxHeight) {
    decoder->refused = 1;
    snprintf(decoder->error, sizeof decoder->error,
             "the card's %s decoder takes pictures up to %ux%u, and this one is %ux%u",
             codec_name(decoder->codec), caps.nMaxWidth, caps.nMaxHeight, width, height);
    return -1;
  }
  *sixteen = (caps.nOutputFormatMask >> cudaVideoSurfaceFormat_P016) & 1;
  if (depth == 2 && !*sixteen) {
    decoder->refused = 1;
    snprintf(decoder->error, sizeof decoder->error,
             "the card's %s decoder has no 16-bit surface for a 10-bit picture",
             codec_name(decoder->codec));
    return -1;
  }
  return 1;
}

/* Takes the colour signal the stream's sequence header declared: the range its
 * codes use, the matrix luma and chroma are combined with, and the curve they
 * are in, which the tile pass needs to show a 10-bit picture in its own colour.
 * An 8-bit picture keeps the pass's one fixed matrix, so only a 10-bit one is
 * refused here, by the name of what it carried, when the pass has no
 * conversion or curve for it. 0 when the stream can be shown, -1 with the
 * reason when it can't. */
static int take_colour(struct yapfast_cudec *decoder, const CUVIDEOFORMAT *format, int ten_bit) {
  const unsigned int matrix = format->video_signal_description.matrix_coefficients;
  const unsigned int transfer = format->video_signal_description.transfer_characteristics;
  decoder->colour.range = format->video_signal_description.video_full_range_flag
                              ? YAPFAST_CU_FULL
                              : YAPFAST_CU_LIMITED;
  decoder->colour.matrix = YAPFAST_CU_BT709;
  decoder->colour.transfer = YAPFAST_CU_SDR;
  if (!ten_bit) return 0;
  switch (matrix) {
    case 0:
    case YAPFAST_VUI_MATRIX_BT709:
      break;
    case YAPFAST_VUI_MATRIX_BT2020_NCL:
      decoder->colour.matrix = YAPFAST_CU_BT2020_NCL;
      break;
    default:
      decoder->refused = 1;
      snprintf(decoder->error, sizeof decoder->error,
               "the 10-bit picture is coded with %s, which the tile pass has no conversion for",
               matrix == YAPFAST_VUI_MATRIX_BT2020_CL ? "BT.2020's constant-luminance matrix"
                                                      : "a matrix");
      return -1;
  }
  switch (transfer) {
    case 0:
    case YAPFAST_VUI_TRANSFER_BT709:
    case YAPFAST_VUI_TRANSFER_BT2020_10:
    case YAPFAST_VUI_TRANSFER_BT2020_12:
      return 0;
    case YAPFAST_VUI_TRANSFER_PQ:
      decoder->colour.transfer = YAPFAST_CU_PQ;
      return 0;
    default:
      decoder->refused = 1;
      snprintf(decoder->error, sizeof decoder->error,
               "the 10-bit picture is coded in %s, which the tile pass has no tone map for",
               transfer == YAPFAST_VUI_TRANSFER_HLG ? "HLG" : "a transfer curve");
      return -1;
  }
}

/* The stream's sequence header: what its pictures are, and the decoder for them.
 * 0 fails the parse, which is how a stream whose pictures the tile pass can't
 * show stays with the software decoder. */
static int CUDAAPI sequence_callback(void *user, CUVIDEOFORMAT *format) {
  struct yapfast_cudec *decoder = user;
  if (!format) return 0;
  if (format->codec != decoder->driver_codec) {
    fail_text(decoder, "the card's parser read another codec's stream");
    return 0;
  }
  /* 4:2:0 at 8 or 10 bits, and nothing else: a 10-bit stream decodes into
   * P016, whose samples are 16 bits, and 9- and 12-bit pictures, 4:2:2 and
   * 4:4:4 have no layout the tile pass can read. */
  const int depth = format->bit_depth_luma_minus8;
  if ((depth != 0 && depth != 2) || format->bit_depth_chroma_minus8 != depth ||
      format->chroma_format != cudaVideoChromaFormat_420) {
    decoder->refused = 1;
    fail_text(decoder, "the card's picture isn't 8-bit or 10-bit 4:2:0, which the tile pass shows");
    return 0;
  }
  /* The colour it declared, taken before a decoder is made for it: 10 bits of
   * a signal the pass has no curve for is a picture shown wrongly, so the
   * stream ends on the card here and stays with the software decoder. */
  if (take_colour(decoder, format, depth == 2) < 0) return 0;
  decoder->ten_bit = depth == 2;
  const unsigned int width = format->coded_width;
  const unsigned int height = format->coded_height;
  if (!width || !height || width > MAX_SIDE || height > MAX_SIDE) {
    fail_text(decoder, "the stream never said how large its pictures are");
    return 0;
  }
  /* The card's own answer on whether it decodes pictures of this depth and size
   * at all, asked before a decoder is made for them. */
  int sixteen = 0;
  if (card_decodes(decoder, (unsigned int)depth, width, height, &sixteen) < 0) return 0;
  /* A stream that changed size gets a decoder for its new pictures, made before
   * the old one is dropped so that a driver that won't make it leaves the old
   * one in place rather than the stream with none. */
  CUVIDDECODECREATEINFO info;
  memset(&info, 0, sizeof info);
  info.ulWidth = width;
  info.ulHeight = height;
  info.ulNumDecodeSurfaces = format->min_num_decode_surfaces > POOL
                                 ? format->min_num_decode_surfaces
                                 : POOL;
  if (info.ulNumDecodeSurfaces > SURFACES) info.ulNumDecodeSurfaces = SURFACES;
  info.CodecType = decoder->driver_codec;
  info.ChromaFormat = cudaVideoChromaFormat_420;
  /* The dedicated video engines, not the CUDA cores standing in for them. */
  info.ulCreationFlags = cudaVideoCreate_PreferCUVID;
  info.bitDepthMinus8 = (unsigned int)depth;
  info.ulMaxWidth = width;
  info.ulMaxHeight = height;
  info.display_area.left = (short)format->display_area.left;
  info.display_area.top = (short)format->display_area.top;
  info.display_area.right = (short)format->display_area.right;
  info.display_area.bottom = (short)format->display_area.bottom;
  /* 16-bit samples for a 10-bit stream, which is the surface the tile pass
   * imports as P016: NVDEC keeps the 10 bits in the low bits of the 16. */
  info.OutputFormat =
      depth ? cudaVideoSurfaceFormat_P016 : cudaVideoSurfaceFormat_NV12;
  info.DeinterlaceMode = cudaVideoDeinterlaceMode_Weave;
  info.ulTargetWidth = width;
  info.ulTargetHeight = height;
  /* Two: the one the caller's picture is mapped from while the next decodes. */
  info.ulNumOutputSurfaces = 2;
  info.target_rect.left = 0;
  info.target_rect.top = 0;
  info.target_rect.right = (short)width;
  info.target_rect.bottom = (short)height;
  /* A picture still mapped from the decoder about to be dropped goes back to it
   * first: the new decoder has no way to unmap it, and the old one is gone. */
  yapfast_cuvadec_release(decoder);
  CUvideodecoder made = NULL;
  const CUresult status = g_driver.cuvidCreateDecoder(&made, &info);
  if (status != CUDA_SUCCESS) {
    fail_status(decoder, "the card wouldn't start the decoder", status);
    return 0;
  }
  if (decoder->decoder) g_driver.cuvidDestroyDecoder(decoder->decoder);
  decoder->decoder = made;
  /* The visible picture, which is smaller than the coded one where the codec
   * codes in whole macroblocks, and the coded height the surface's chroma plane
   * is placed by. */
  decoder->width = format->display_area.right - format->display_area.left > 0
                       ? format->display_area.right - format->display_area.left
                       : (int)width;
  decoder->height = format->display_area.bottom - format->display_area.top > 0
                        ? format->display_area.bottom - format->display_area.top
                        : (int)height;
  decoder->surface_height = (int)height;
  decoder->described = 1;
  decoder->direct_refused = 0;
  /* The parser's own queue is as deep as the decoder's surfaces: a picture the
   * parser thinks it may recycle while the decoder still holds it is the kind of
   * bug that shows as one torn picture in a hundred. */
  return (int)info.ulNumDecodeSurfaces;
}

/* A picture to decode: the parser has already filled in every parameter the
 * decoder needs, slices and all, so this is one call. */
static int CUDAAPI decode_callback(void *user, CUVIDPICPARAMS *params) {
  struct yapfast_cudec *decoder = user;
  if (!decoder->decoder || !params) {
    fail_text(decoder, "the card had no decoder for the stream's picture");
    return 0;
  }
  const CUresult status = g_driver.cuvidDecodePicture(decoder->decoder, params);
  if (status != CUDA_SUCCESS) {
    fail_status(decoder, "the card couldn't decode a frame", status);
    return 0;
  }
  return 1;
}

/* A picture to show: mapped where it lies, and kept as the newest one. Only the
 * newest picture is shown, here as everywhere, so an earlier one from the same
 * frame goes back to the decoder. */
static int CUDAAPI display_callback(void *user, CUVIDPARSERDISPINFO *info) {
  struct yapfast_cudec *decoder = user;
  if (!info) return 1;
  if (!decoder->decoder) return 0;
  if (decoder->mapped) yapfast_cuvadec_release(decoder);
  CUVIDPROCPARAMS params;
  memset(&params, 0, sizeof params);
  params.progressive_frame = info->progressive_frame;
  params.top_field_first = info->top_field_first;
  params.second_field = 0;
  params.output_stream = NULL;
  unsigned long long pointer = 0;
  unsigned int pitch = 0;
  const CUresult status = g_driver.cuvidMapVideoFrame(decoder->decoder, info->picture_index,
                                                      &pointer, &pitch, &params);
  if (status != CUDA_SUCCESS) {
    fail_status(decoder, "the card's picture wasn't mapped", status);
    return 0;
  }
  decoder->mapped = 1;
  decoder->mapped_pointer = (CUdeviceptr)pointer;
  decoder->mapped_pitch = pitch;
  decoder->fresh = 1;
  return 1;
}

/* ----------------------------------------------------------------- the API */

struct yapfast_cudec *yapfast_cuvadec_new(int codec) {
  pthread_once(&g_driver_once, load_driver);
  struct yapfast_cudec *decoder = calloc(1, sizeof *decoder);
  if (!decoder) return NULL;
  decoder->fd = -1;
  decoder->slot[0].fd = -1;
  decoder->slot[1].fd = -1;
  const char *what = codec_name(codec);
  if (g_driver.why[0]) {
    /* Both are bounded as they are printed: the error is one sentence a report
     * shows, and a driver's own words can be longer than it. */
    snprintf(decoder->error, sizeof decoder->error,
             "%.96s, so there is no %.16s decoder on the card", g_driver.why, what);
    goto failed;
  }
  if (driver_codec_of(codec, &decoder->driver_codec) < 0) {
    snprintf(decoder->error, sizeof decoder->error, "the graphics card has no %s decoder", what);
    goto failed;
  }
  decoder->codec = codec;
  const CUresult init = g_driver.cuInit(0);
  if (init != CUDA_SUCCESS) {
    snprintf(decoder->error, sizeof decoder->error,
             "the NVIDIA driver wouldn't start (%s), so there is no %s decoder on the card",
             status_name(init), what);
    goto failed;
  }
  CUdevice device = 0;
  CUresult status = g_driver.cuDeviceGet(&device, 0);
  if (status != CUDA_SUCCESS) {
    snprintf(decoder->error, sizeof decoder->error,
             "the machine has no NVIDIA card for the %s decoder (%s)", what, status_name(status));
    goto failed;
  }
  status = g_driver.cuCtxCreate(&decoder->context, 0, device);
  if (status != CUDA_SUCCESS) {
    decoder->context = NULL;
    snprintf(decoder->error, sizeof decoder->error, "the card wouldn't make a context (%s)",
             status_name(status));
    goto failed;
  }
  decoder->device = device;
  /* Created once, reused for every copy: a stream makes a stream, not one per
   * picture. */
  const unsigned int flags = 0;
  status = g_driver.cuStreamCreate(&decoder->stream, flags);
  if (status != CUDA_SUCCESS) {
    decoder->stream = NULL;
    snprintf(decoder->error, sizeof decoder->error, "the card wouldn't make a stream (%s)",
             status_name(status));
    goto failed;
  }
  CUVIDPARSERPARAMS params;
  memset(&params, 0, sizeof params);
  params.CodecType = decoder->driver_codec;
  /* The parser's queue, which the sequence callback raises if the stream's own
   * decoder needs more. */
  /* What the sequence callback may ask for, which is what the parser will hand
   * over: a stream that needs more than this is refused rather than silently
   * giving up the pictures the decoder can't get a surface for. */
  params.ulMaxNumDecodeSurfaces = SURFACES;
  /* A microsecond clock, since no timestamp is passed: it only has to be a
   * clock. */
  params.ulClockRate = 1000000;
  /* A whole picture decoded before it is shown: a watched stream is shown as it
   * arrives, and a corrupt one is not worth a picture the viewer has passed. */
  params.ulErrorThreshold = 0;
  params.ulMaxDisplayDelay = 0;
  /* NVDEC's H264 and HEVC parsers read Annex B, which is what Discord's streams
   * carry; AV1's reads the OBUs, which have no start codes. */
  params.bAnnexb = codec == YAPFAST_CU_AV1 ? 0 : 1;
  params.pUserData = decoder;
  params.pfnSequenceCallback = sequence_callback;
  params.pfnDecodePicture = decode_callback;
  params.pfnDisplayPicture = display_callback;
  if (push_context(decoder) < 0) goto failed;
  status = g_driver.cuvidCreateVideoParser(&decoder->parser, &params);
  pop_context(decoder);
  if (status != CUDA_SUCCESS) {
    decoder->parser = NULL;
    snprintf(decoder->error, sizeof decoder->error,
             "the card's %s parser wouldn't start (%s)", what, status_name(status));
    goto failed;
  }
  return decoder;
failed:
  snprintf(g_error, sizeof g_error, "%s", decoder->error);
  yapfast_cuvadec_free(decoder);
  return NULL;
}

int yapfast_cuvadec_decode(struct yapfast_cudec *decoder, const unsigned char *data, size_t size,
                           int keyframe) {
  (void)keyframe;
  if (!decoder || !decoder->parser) return -1;
  decoder->error[0] = 0;
  /* The caller had its picture: a surface goes back to the decoder here, and a
   * descriptor the driver gave is closed. */
  yapfast_cuvadec_release(decoder);
  decoder->fresh = 0;
  if (!size) return 0;
  /* The parser reads a little past the end of a frame. */
  const size_t needed = size + PADDING;
  if (decoder->padded_size < needed) {
    unsigned char *grown = realloc(decoder->padded, needed);
    if (!grown) {
      fail_text(decoder, "the frame didn't fit");
      return -3;
    }
    decoder->padded = grown;
    decoder->padded_size = needed;
  }
  memcpy(decoder->padded, data, size);
  memset(decoder->padded + size, 0, PADDING);
  if (push_context(decoder) < 0) return -1;
  CUVIDSOURCEDATAPACKET packet;
  memset(&packet, 0, sizeof packet);
  packet.payload = decoder->padded;
  packet.payload_size = size;
  const CUresult status = g_driver.cuvidParseVideoData(decoder->parser, &packet);
  pop_context(decoder);
  if (status != CUDA_SUCCESS) {
    /* The callbacks run inside the parse and are where a stream the card can't
     * take says why, so their sentence is the one quoted. */
    if (!decoder->error[0]) fail_status(decoder, "the card's parser refused a frame", status);
    return decoder->refused ? -2 : -1;
  }
  if (!decoder->fresh) {
    /* A stream's first frame carries its sequence header, so a parser that has
     * read this many without one has nothing to decode the rest of it with:
     * the stream ends on the card here rather than showing nothing for as long
     * as it runs, and the software decoder reads it from the next keyframe. */
    if (!decoder->described && ++decoder->undescribed >= 4) {
      decoder->refused = 1;
      snprintf(decoder->error, sizeof decoder->error,
               "the card's %s parser read %d frames without saying what their pictures are, "
               "which is how it answers a profile it has no decoder for",
               codec_name(decoder->codec), decoder->undescribed);
      return -2;
    }
    return 0;
  }
  decoder->undescribed = 0;
  if (export_picture(decoder) < 0) return decoder->refused ? -2 : -1;
  decoder->counts.pictures++;
  return 1;
}

void yapfast_cuvadec_counts(const struct yapfast_cudec *decoder,
                            struct yapfast_cucounts *counts) {
  if (!counts) return;
  if (!decoder) {
    memset(counts, 0, sizeof *counts);
    return;
  }
  *counts = decoder->counts;
}

const char *yapfast_cuvadec_error(const struct yapfast_cudec *decoder) {
  return decoder ? decoder->error : g_error;
}

void yapfast_cuvadec_free(struct yapfast_cudec *decoder) {
  if (!decoder) return;
  yapfast_cuvadec_release(decoder);
  /* Anything that needs the driver needs the context current. */
  const int pushed = decoder->context && push_context(decoder) == 0;
  if (pushed) drop_slots(decoder);
  if (decoder->decoder) {
    g_driver.cuvidDestroyDecoder(decoder->decoder);
    decoder->decoder = NULL;
  }
  if (decoder->parser) {
    g_driver.cuvidDestroyVideoParser(decoder->parser);
    decoder->parser = NULL;
  }
  if (pushed) pop_context(decoder);
  if (decoder->stream) g_driver.cuStreamDestroy(decoder->stream);
  if (decoder->context) g_driver.cuCtxDestroy(decoder->context);
  free(decoder->padded);
  free(decoder);
}
