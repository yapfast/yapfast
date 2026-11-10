/* Decoding a watched stream with oneVPL (MFX) on Intel's card: the third door
 * after native/hwdec_vaapi.c (AMD and Intel through VA-API) and
 * native/hwdec_cuda.c (NVIDIA through NVDEC). It is the one that is only there
 * for the machine VA-API doesn't open: a card whose iHD driver won't
 * initialise, a session where libva isn't on the path the shim needs, a graphics
 * generation whose VA driver is broken while oneVPL's own engine works.
 *
 * The surfaces are VA surfaces regardless — that is what oneVPL allocates when
 * it is handed a VA display — so a decoded picture is exported as a DMA-BUF the
 * same way the VA-API shim exports one (vaExportSurfaceHandle, DRM PRIME 2), and
 * the tile pass imports it where it lies. No pixel comes back to the processor:
 * `yapfast_mfxdec_facts` counts what crossed and answers zero.
 *
 * Nothing here is linked. oneVPL's dispatcher and libva are both dlopened, and
 * every function this file calls is found at run time, so a machine with neither
 * starts Yapfast and answers with a sentence — which is what lets the stream fall
 * to the software decoder rather than lose its picture. */
#include <dlfcn.h>
#include <fcntl.h>
#include <stdarg.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

#include <va/va.h>
#include <va/va_drm.h>
#include <va/va_drmcommon.h>
#include <vpl/mfxvideo.h>

#include "hwdec_mfx.h"

/* DRM's name for the one format this shim exports, and the one a VA surface the
 * decoder writes is made of. */
#define MFX_DRM_FORMAT_NV12 0x3231564eu

/* The most of a frame this hands the parser at once: a whole access unit, a
 * keyframe's included, and grown when one is larger. */
#define MFX_BITSTREAM 4u << 20

/* oneVPL's dispatcher, as the legacy 1.x entry points this file needs. */
struct mfx_api {
  void *library;
  mfxStatus (*MFXInit)(mfxIMPL, mfxVersion *, mfxSession *);
  mfxStatus (*MFXClose)(mfxSession);
  mfxStatus (*MFXQueryVersion)(mfxSession, mfxVersion *);
  mfxStatus (*MFXVideoCORE_SetHandle)(mfxSession, mfxHandleType, mfxHDL);
  mfxStatus (*MFXVideoDECODE_DecodeHeader)(mfxSession, mfxBitstream *, mfxVideoParam *);
  mfxStatus (*MFXVideoDECODE_Init)(mfxSession, mfxVideoParam *);
  mfxStatus (*MFXVideoDECODE_DecodeFrameAsync)(mfxSession, mfxBitstream *, mfxFrameSurface1 *,
                                               mfxFrameSurface1 **, mfxSyncPoint *);
  mfxStatus (*MFXVideoCORE_SyncOperation)(mfxSession, mfxSyncPoint, mfxU32);
};

/* libva, for the display oneVPL decodes on and the export of its surfaces. */
struct va_api {
  void *library;
  VADisplay (*vaGetDisplayDRM)(int);
  VAStatus (*vaInitialize)(VADisplay, int *, int *);
  VAStatus (*vaTerminate)(VADisplay);
  VAStatus (*vaExportSurfaceHandle)(VADisplay, VASurfaceID, uint32_t, uint32_t, void *);
};

struct yapfast_mfxdec {
  struct mfx_api mfx;
  struct va_api va;
  mfxSession session;
  VADisplay display;
  int fd;
  int codec;
  /* The stream's own picture, once its first sequence header has said what it
   * is: nothing is decoded before that. */
  mfxVideoParam param;
  int configured;
  /* The frame being handed over, and the surface it is in: the decoder's pool
   * holds the surface against the card until the picture is released. */
  mfxFrameSurface1 *surface;
  struct yapfast_mfxpicture picture;
  int have_picture;
  int picture_planes;
  int picture_fds[YAPFAST_MFX_PLANES];
  unsigned char *bits;
  size_t bits_size;
  struct yapfast_mfxfacts facts;
  char error[256];
};

static void put(char *buffer, size_t size, const char *format, ...) {
  if (buffer == NULL || size == 0) return;
  va_list arguments;
  va_start(arguments, format);
  vsnprintf(buffer, size, format, arguments);
  va_end(arguments);
}

/* oneVPL's answer, in words, for a log to quote. */
static const char *mfx_name(mfxStatus status) {
  switch (status) {
    case MFX_ERR_NONE: return "no error";
    case MFX_ERR_UNKNOWN: return "an unknown error";
    case MFX_ERR_NULL_PTR: return "a null pointer";
    case MFX_ERR_UNSUPPORTED: return "unsupported";
    case MFX_ERR_MEMORY_ALLOC: return "out of memory";
    case MFX_ERR_NOT_ENOUGH_BUFFER: return "not enough buffer";
    case MFX_ERR_INVALID_HANDLE: return "an invalid handle";
    case MFX_ERR_LOCK_MEMORY: return "the memory wouldn't lock";
    case MFX_ERR_NOT_INITIALIZED: return "not initialised";
    case MFX_ERR_NOT_FOUND: return "not found";
    case MFX_ERR_MORE_DATA: return "more data is needed";
    case MFX_ERR_MORE_SURFACE: return "more surfaces are needed";
    case MFX_ERR_ABORTED: return "aborted";
    case MFX_ERR_DEVICE_LOST: return "the device was lost";
    case MFX_ERR_INCOMPATIBLE_VIDEO_PARAM: return "incompatible parameters";
    case MFX_ERR_INVALID_VIDEO_PARAM: return "invalid parameters";
    case MFX_ERR_UNDEFINED_BEHAVIOR: return "undefined behaviour";
    case MFX_ERR_DEVICE_FAILED: return "the device failed";
    case MFX_ERR_MORE_BITSTREAM: return "more bitstream is needed";
    case MFX_ERR_GPU_HANG: return "the GPU hung";
    case MFX_ERR_REALLOC_SURFACE: return "the surface must be reallocated";
    default: return "an error oneVPL names by number";
  }
}

static void failed(struct yapfast_mfxdec *decoder, mfxStatus status, const char *what) {
  put(decoder->error, sizeof decoder->error, "%s: %s", what, mfx_name(status));
}

static void *symbol(void *library, const char *name) { return dlsym(library, name); }

/* oneVPL's dispatcher and libva: the two libraries this path is made of, and
 * both are only wanted when a watched stream asks for one. */
static int load(struct yapfast_mfxdec *decoder) {
  static const char *const vpl[] = {"libvpl.so.2", "libvpl.so", "libmfx.so.1", NULL};
  static const char *const va[] = {"libva.so.2", "libva.so", NULL};
  for (int i = 0; vpl[i] != NULL && decoder->mfx.library == NULL; i++)
    decoder->mfx.library = dlopen(vpl[i], RTLD_NOW | RTLD_LOCAL);
  if (decoder->mfx.library == NULL) {
    put(decoder->error, sizeof decoder->error, "no oneVPL runtime: %s", dlerror());
    return 0;
  }
  for (int i = 0; va[i] != NULL && decoder->va.library == NULL; i++)
    decoder->va.library = dlopen(va[i], RTLD_NOW | RTLD_LOCAL);
  if (decoder->va.library == NULL) {
    put(decoder->error, sizeof decoder->error, "no libva: %s", dlerror());
    return 0;
  }
  void *v = decoder->mfx.library, *a = decoder->va.library;
#define LOAD(api, library, name)                                                 \
  do {                                                                           \
    decoder->api.name = symbol(library, #name);                                  \
    if (decoder->api.name == NULL) {                                             \
      put(decoder->error, sizeof decoder->error, "the runtime has no %s", #name); \
      return 0;                                                                  \
    }                                                                            \
  } while (0)
  LOAD(mfx, v, MFXInit);
  LOAD(mfx, v, MFXClose);
  LOAD(mfx, v, MFXQueryVersion);
  LOAD(mfx, v, MFXVideoCORE_SetHandle);
  LOAD(mfx, v, MFXVideoDECODE_DecodeHeader);
  LOAD(mfx, v, MFXVideoDECODE_Init);
  LOAD(mfx, v, MFXVideoDECODE_DecodeFrameAsync);
  LOAD(mfx, v, MFXVideoCORE_SyncOperation);
  LOAD(va, a, vaGetDisplayDRM);
  LOAD(va, a, vaInitialize);
  LOAD(va, a, vaTerminate);
  LOAD(va, a, vaExportSurfaceHandle);
#undef LOAD
  return 1;
}

/* The render node to decode on: the first the kernel offers, which is the card
 * oneVPL will pick anyway. */
static int open_node(void) {
  static const char *const nodes[] = {"/dev/dri/renderD128", "/dev/dri/renderD129",
                                      "/dev/dri/renderD130", "/dev/dri/renderD131", NULL};
  for (int i = 0; nodes[i] != NULL; i++) {
    int fd = open(nodes[i], O_RDWR | O_CLOEXEC);
    if (fd >= 0) return fd;
  }
  return -1;
}

/* oneVPL on a VA display, which is the shape this shim can export from. */
static int start(struct yapfast_mfxdec *decoder) {
  decoder->fd = open_node();
  if (decoder->fd < 0) {
    put(decoder->error, sizeof decoder->error, "no render node to decode on");
    return 0;
  }
  decoder->display = decoder->va.vaGetDisplayDRM(decoder->fd);
  if (decoder->display == NULL) {
    put(decoder->error, sizeof decoder->error, "libva wouldn't take the render node");
    return 0;
  }
  int major = 0, minor = 0;
  if (decoder->va.vaInitialize(decoder->display, &major, &minor) != VA_STATUS_SUCCESS) {
    put(decoder->error, sizeof decoder->error, "libva wouldn't initialise the display");
    decoder->display = NULL;
    return 0;
  }
  mfxVersion version = {{0, 1}};
  mfxIMPL impl = MFX_IMPL_HARDWARE_ANY | MFX_IMPL_VIA_VAAPI;
  mfxStatus status = decoder->mfx.MFXInit(impl, &version, &decoder->session);
  if (status != MFX_ERR_NONE && status != MFX_ERR_MORE_DATA) {
    failed(decoder, status, "oneVPL wouldn't start");
    return 0;
  }
  status = decoder->mfx.MFXVideoCORE_SetHandle(decoder->session, MFX_HANDLE_VA_DISPLAY,
                                               (mfxHDL)decoder->display);
  if (status != MFX_ERR_NONE) {
    failed(decoder, status, "oneVPL wouldn't decode on the VA display");
    return 0;
  }
  return 1;
}

struct yapfast_mfxdec *yapfast_mfxdec_new(int codec) {
  struct yapfast_mfxdec *decoder = calloc(1, sizeof *decoder);
  if (decoder == NULL) return NULL;
  decoder->codec = codec;
  decoder->picture_planes = 0;
  decoder->picture_fds[0] = -1;
  decoder->picture_fds[1] = -1;
  if (!load(decoder) || !start(decoder)) {
    yapfast_mfxdec_free(decoder);
    return NULL;
  }
  return decoder;
}

/* The stream's shape, from its own first header: the codec, the size and the
 * format oneVPL will make surfaces of. */
static int configure(struct yapfast_mfxdec *decoder, mfxBitstream *bits) {
  mfxVideoParam param;
  memset(&param, 0, sizeof param);
  mfxStatus status = decoder->mfx.MFXVideoDECODE_DecodeHeader(decoder->session, bits, &param);
  if (status < MFX_ERR_NONE) {
    failed(decoder, status, "the stream's header wasn't readable");
    return 0;
  }
  param.IOPattern = MFX_IOPATTERN_OUT_VIDEO_MEMORY;
  if (param.mfx.FrameInfo.FourCC != MFX_FOURCC_NV12) {
    put(decoder->error, sizeof decoder->error,
        "a picture oneVPL makes in a format the tile pass can't show (0x%x)",
        param.mfx.FrameInfo.FourCC);
    return 0;
  }
  param.mfx.FrameInfo.ChromaFormat = MFX_CHROMAFORMAT_YUV420;
  status = decoder->mfx.MFXVideoDECODE_Init(decoder->session, &param);
  if (status != MFX_ERR_NONE) {
    failed(decoder, status, "oneVPL wouldn't open a decoder for the stream");
    return 0;
  }
  decoder->param = param;
  decoder->configured = 1;
  return 1;
}

/* Gives the surface back to the pool the decoder allocates from: oneVPL's own
 * frame interface where it has one, and the legacy unlock where it doesn't. */
static void unlock(struct yapfast_mfxdec *decoder) {
  mfxFrameSurface1 *surface = decoder->surface;
  decoder->surface = NULL;
  if (surface == NULL) return;
  if (surface->FrameInterface != NULL && surface->FrameInterface->Release != NULL)
    surface->FrameInterface->Release(surface);
  else
    surface->Data.Locked = 0;
}

/* The surface as the DMA-BUF the frames channel carries: one object, two planes,
 * at the offsets and pitches DRM PRIME 2 reports. */
static int export_picture(struct yapfast_mfxdec *decoder) {
  mfxFrameSurface1 *surface = decoder->surface;
  VASurfaceID id = (VASurfaceID)(uintptr_t)surface->Data.MemId;
  VADRMPRIMESurfaceDescriptor desc;
  memset(&desc, 0, sizeof desc);
  VAStatus status = decoder->va.vaExportSurfaceHandle(
      decoder->display, id, VA_SURFACE_ATTRIB_MEM_TYPE_DRM_PRIME_2,
      VA_EXPORT_SURFACE_READ_ONLY | VA_EXPORT_SURFACE_SEPARATE_LAYERS, &desc);
  if (status != VA_STATUS_SUCCESS) {
    put(decoder->error, sizeof decoder->error, "libva wouldn't export the decoded surface (0x%x)",
        status);
    return 0;
  }
  if (desc.fourcc != MFX_DRM_FORMAT_NV12 || desc.num_layers < 2) {
    put(decoder->error, sizeof decoder->error,
        "the decoded surface isn't NV12 in two layers (0x%x, %u layers)", desc.fourcc,
        desc.num_layers);
    return 0;
  }
  struct yapfast_mfxpicture *picture = &decoder->picture;
  memset(picture, 0, sizeof *picture);
  picture->width = (int)surface->Info.CropW;
  picture->height = (int)surface->Info.CropH;
  picture->fourcc = MFX_DRM_FORMAT_NV12;
  picture->planes = 2;
  for (unsigned layer = 0; layer < 2; layer++) {
    const uint32_t object = desc.layers[layer].object_index[0];
    picture->plane[layer].fd = desc.objects[object].fd;
    picture->plane[layer].offset = desc.layers[layer].offset[0];
    picture->plane[layer].stride = desc.layers[layer].pitch[0];
    picture->plane[layer].size = desc.objects[object].size;
    picture->plane[layer].modifier = desc.objects[object].drm_format_modifier;
  }
  decoder->picture_fds[0] = picture->plane[0].fd;
  decoder->picture_fds[1] = picture->plane[1].fd;
  decoder->picture_planes = 2;
  decoder->facts.exported++;
  return 1;
}

int yapfast_mfxdec_decode(struct yapfast_mfxdec *decoder, const unsigned char *data, size_t size,
                          int keyframe) {
  (void)keyframe;
  if (decoder == NULL || decoder->session == NULL) return -1;
  decoder->error[0] = 0;
  yapfast_mfxdec_release(decoder);
  if (size > decoder->bits_size) {
    unsigned char *grown = realloc(decoder->bits, size);
    if (grown == NULL) {
      put(decoder->error, sizeof decoder->error, "no room for the stream's own bytes");
      return -1;
    }
    decoder->bits = grown;
    decoder->bits_size = size;
  }
  memcpy(decoder->bits, data, size);
  mfxBitstream bits;
  memset(&bits, 0, sizeof bits);
  bits.Data = decoder->bits;
  bits.DataLength = (mfxU32)size;
  bits.MaxLength = (mfxU32)size;
  if (!decoder->configured && !configure(decoder, &bits)) return -1;
  /* The header was read once; the frame is what the decoder takes now. */
  mfxBitstream frame;
  memset(&frame, 0, sizeof frame);
  frame.Data = decoder->bits;
  frame.DataLength = (mfxU32)size;
  frame.MaxLength = (mfxU32)size;
  mfxFrameSurface1 *surface = NULL;
  mfxSyncPoint sync = NULL;
  mfxStatus status = decoder->mfx.MFXVideoDECODE_DecodeFrameAsync(decoder->session, &frame, NULL,
                                                                  &surface, &sync);
  if (status == MFX_ERR_MORE_DATA || status == MFX_ERR_MORE_SURFACE) return 0;
  if (status != MFX_ERR_NONE) {
    failed(decoder, status, "the card wouldn't decode a frame");
    return -1;
  }
  decoder->facts.decoded++;
  if ((status = decoder->mfx.MFXVideoCORE_SyncOperation(decoder->session, sync, 60000)) !=
      MFX_ERR_NONE) {
    failed(decoder, status, "the card's decode didn't finish");
    return -1;
  }
  decoder->surface = surface;
  if (!export_picture(decoder)) {
    unlock(decoder);
    return -1;
  }
  decoder->have_picture = 1;
  return 1;
}

int yapfast_mfxdec_picture(struct yapfast_mfxdec *decoder, struct yapfast_mfxpicture *picture) {
  if (decoder == NULL || picture == NULL) return -1;
  if (!decoder->have_picture) {
    if (decoder->error[0] == 0)
      put(decoder->error, sizeof decoder->error, "there is no decoded picture to describe");
    return -1;
  }
  *picture = decoder->picture;
  return 0;
}

void yapfast_mfxdec_release(struct yapfast_mfxdec *decoder) {
  if (decoder == NULL) return;
  if (decoder->have_picture) {
    /* The export made its own descriptors; the caller's are closed here. */
    for (unsigned plane = 0; plane < 2; plane++) {
      if (decoder->picture_fds[plane] >= 0) close(decoder->picture_fds[plane]);
      decoder->picture_fds[plane] = -1;
    }
    decoder->have_picture = 0;
    decoder->picture_planes = 0;
  }
  unlock(decoder);
}

void yapfast_mfxdec_facts(const struct yapfast_mfxdec *decoder, struct yapfast_mfxfacts *facts) {
  if (decoder == NULL || facts == NULL) return;
  *facts = decoder->facts;
}

const char *yapfast_mfxdec_error(const struct yapfast_mfxdec *decoder) {
  return decoder == NULL ? "not opened" : decoder->error;
}

void yapfast_mfxdec_free(struct yapfast_mfxdec *decoder) {
  if (decoder == NULL) return;
  yapfast_mfxdec_release(decoder);
  if (decoder->session != NULL && decoder->mfx.MFXClose != NULL)
    decoder->mfx.MFXClose(decoder->session);
  if (decoder->display != NULL && decoder->va.vaTerminate != NULL)
    decoder->va.vaTerminate(decoder->display);
  if (decoder->fd >= 0) close(decoder->fd);
  free(decoder->bits);
  if (decoder->mfx.library != NULL) dlclose(decoder->mfx.library);
  if (decoder->va.library != NULL) dlclose(decoder->va.library);
  free(decoder);
}
