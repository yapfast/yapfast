/* NVIDIA's card as the way a captured screen reaches its encoder on Linux.
 *
 * Everywhere else this build has a zero-copy rung: Windows hands NVENC a D3D11
 * texture the capture filled (native/d3d11enc.c), and AMD's and Intel's drivers
 * import a compositor's DMA-BUF as a VA-API surface where it lies
 * (native/hwenc_dmabuf.c). NVIDIA has neither: its libva driver decodes only, and
 * NVENC reads its picture from memory. What it does have is the CUDA driver
 * API — the one that ships with the driver, not the toolkit — so this file takes
 * the other door:
 *
 *   DMA-BUF -> EGLImage -> CUDA, NPP-free scale and convert -> NVENC
 *
 * `yapfast_cudaconvert_convert` imports the descriptor the compositor drew into
 * an EGLImage, registers that image with CUDA, and runs the two kernels
 * `native/cuda_convert.cu` was compiled to (BGRA to NV12, BT.601 limited range,
 * bilinear, the same picture libyuv makes on the processor) into an NV12 buffer
 * that lives on the card. `native/cudaconvert` then hands that buffer to the
 * NVIDIA encoder, which reads it where it is. No pixel of the picture is copied
 * by the processor and none crosses the bus: `yapfast_cudaconvert_copied` counts
 * what the processor moved and answers zero.
 *
 * What it will not take, it says and refuses: a format that isn't 32-bit
 * BGRA/BGRX, a picture the driver holds in a shape that is neither rows nor an
 * array, and a device that isn't the one the descriptor came from. A tiled
 * picture, which is what a compositor on NVIDIA's driver shares, is one the
 * driver holds as an array CUDA can't map: GL copies it on the card into a
 * texture of its own, which CUDA can, and the kernels read that through a
 * texture object (or, on a driver without them, from a copy of its rows), so no
 * pixel of it crosses the processor either way. A refusal is
 * a sentence the share's ladder falls through on, to the same processor copy
 * every other path keeps.
 *
 * Nothing here needs a CUDA toolkit at build time or on the machine: the PTX is
 * clang's output (see `build.rs`), the driver loads it, and libcuda is dlopened
 * only when an encoder asks for this path. */

/* readlink is POSIX's, so this file is built as GNU C (see `build.rs`) rather
 * than as strict C, which would leave it undeclared. */
#include <dlfcn.h>
#include <EGL/egl.h>
#include <EGL/eglext.h>
#include <pthread.h>
#include <stdarg.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>

/* The CUDA driver API, as far as this file needs it: the types are the
 * driver's, and the few entry points are looked up when a share asks for the
 * card rather than linked. The declarations match cuda.h's; the structs the
 * driver fills are laid out as cuda_egl_interop.h defines them, which is what
 * makes them usable without the toolkit's headers. */
typedef int CUresult;
typedef int CUdevice;
typedef void *CUcontext;
typedef void *CUmodule;
typedef void *CUfunction;
typedef unsigned long long CUdeviceptr;
typedef void *CUgraphicsResource;
typedef void *CUstream;
typedef unsigned long long CUtexObject;

#define CUDA_SUCCESS 0

typedef struct CUeglFrame_st {
  union {
    void *pArray[3];
    void *pMipmappedArray[3];
    CUdeviceptr pDevPtr[3];
    unsigned long long pSurfObject[3];
  } frame;
  unsigned int width;
  unsigned int height;
  unsigned int depth;
  unsigned int pitch;
  unsigned int planeCount;
  unsigned int numChannels;
  int frameType;
  int eglColorFormat;
  int cuFormat;
} CUeglFrame;

#define CU_EGL_FRAME_TYPE_ARRAY 0
#define CU_EGL_FRAME_TYPE_PITCH 1
#define CU_GRAPHICS_REGISTER_FLAGS_READ_ONLY 0x1

/* A texture object over an array, as cuda.h lays out the two descriptions it
 * is made from: what it reads, and how. The kernels read a tiled picture's
 * array through one, texel by texel, rather than a copy of its rows. */
typedef struct cuda_resource_desc_st {
  int resType;
  union {
    void *hArray;
    int reserved[32];
  } res;
  unsigned flags;
} cuda_resource_desc;

typedef struct cuda_texture_desc_st {
  int addressMode[3];
  int filterMode;
  unsigned flags;
  unsigned maxAnisotropy;
  int mipmapFilterMode;
  float mipmapLevelBias;
  float minMipmapLevelClamp;
  float maxMipmapLevelClamp;
  float borderColor[4];
  int reserved[12];
} cuda_texture_desc;

#define CU_RESOURCE_TYPE_ARRAY 0
#define CU_TR_ADDRESS_MODE_CLAMP 1
#define CU_TR_FILTER_MODE_POINT 0
#define CU_TRSF_READ_AS_INTEGER 0x01

#define CU_MEMORYTYPE_HOST 1
#define CU_MEMORYTYPE_DEVICE 2
#define CU_MEMORYTYPE_ARRAY 3

/* A copy of a rectangle between the places a picture can be: host memory, the
 * card's own memory, or the array a driver imported a tiled picture as. This is
 * the copy a block-linear array takes, where a two-dimensional one is refused.
 * Laid out as cuda.h lays out `CUDA_MEMCPY3D`. */
typedef struct cuda_memcpy3d_st {
  size_t srcXInBytes, srcY, srcZ;
  size_t srcLOD;
  int srcMemoryType;
  const void *srcHost;
  CUdeviceptr srcDevice;
  void *srcArray;
  void *reserved0;
  size_t srcPitch, srcHeight;
  size_t dstXInBytes, dstY, dstZ;
  size_t dstLOD;
  int dstMemoryType;
  void *dstHost;
  CUdeviceptr dstDevice;
  void *dstArray;
  void *reserved1;
  size_t dstPitch, dstHeight;
  size_t WidthInBytes, Height, Depth;
} cuda_memcpy3d;

/* Four bytes a pixel, blue first, which is what a compositor's ARGB8888 and
 * XRGB8888 both are and the only thing the kernels read. */
#define DRM_FORMAT_ARGB8888 0x34325241u
#define DRM_FORMAT_XRGB8888 0x34325258u

/* The driver's entry points, as dlopen finds them. */
struct cuda_api {
  void *library;
  CUresult (*cuInit)(unsigned);
  CUresult (*cuDriverGetVersion)(int *);
  CUresult (*cuDeviceGet)(CUdevice *, int);
  CUresult (*cuDeviceGetCount)(int *);
  CUresult (*cuDeviceGetName)(char *, int, CUdevice);
  /* Which card a device is, by the name Linux gives the same card: the way to
   * the device behind a DRM node when EGL won't say which CUDA device it is. */
  CUresult (*cuDeviceGetPCIBusId)(char *, int, CUdevice);
  CUresult (*cuCtxCreate)(CUcontext *, unsigned, CUdevice);
  CUresult (*cuCtxDestroy)(CUcontext);
  CUresult (*cuCtxPushCurrent)(CUcontext);
  CUresult (*cuCtxPopCurrent)(CUcontext *);
  CUresult (*cuCtxSynchronize)(void);
  CUresult (*cuModuleLoadData)(CUmodule *, const void *);
  CUresult (*cuModuleGetFunction)(CUfunction *, CUmodule, const char *);
  CUresult (*cuMemAlloc)(CUdeviceptr *, size_t);
  CUresult (*cuMemAllocPitch)(CUdeviceptr *, size_t *, size_t, size_t, unsigned);
  CUresult (*cuMemcpy3D)(const cuda_memcpy3d *);
  /* A texture over the array GL's picture lies in, which the kernels read where
   * it is: optional, and a driver without it copies the rows first. */
  CUresult (*cuTexObjectCreate)(CUtexObject *, const cuda_resource_desc *,
                                const cuda_texture_desc *, const void *);
  CUresult (*cuTexObjectDestroy)(CUtexObject);
  CUresult (*cuMemFree)(CUdeviceptr);
  CUresult (*cuLaunchKernel)(CUfunction, unsigned, unsigned, unsigned, unsigned,
                             unsigned, unsigned, unsigned, CUstream, void **,
                             void **);
  CUresult (*cuGetErrorName)(CUresult, const char **);
  CUresult (*cuGraphicsEGLRegisterImage)(CUgraphicsResource *, void *, unsigned);
  /* The same picture as a GL texture, which the driver maps where the resource
   * the EGLImage registered isn't: a block-linear picture is reachable this way. */
  CUresult (*cuGraphicsGLRegisterImage)(CUgraphicsResource *, unsigned, unsigned, unsigned);
  CUresult (*cuGraphicsSubResourceGetMappedArray)(void **, CUgraphicsResource, unsigned,
                                                  unsigned);
  CUresult (*cuGraphicsUnregisterResource)(CUgraphicsResource);
  CUresult (*cuGraphicsMapResources)(unsigned, CUgraphicsResource *, CUstream);
  CUresult (*cuGraphicsUnmapResources)(unsigned, CUgraphicsResource *, CUstream);
  CUresult (*cuGraphicsResourceGetMappedEglFrame)(CUeglFrame *,
                                                  CUgraphicsResource, unsigned,
                                                  unsigned);
};

/* The EGL entry points the extensions bring, which eglGetProcAddress is what
 * finds. */
struct egl_api {
  void *library;
  /* libEGL itself, dlopened as libcuda is: a machine without EGL falls back
   * rather than failing to start Yapfast at all. */
  EGLint (*eglGetError)(void);
  EGLBoolean (*eglInitialize)(EGLDisplay, EGLint *, EGLint *);
  EGLBoolean (*eglTerminate)(EGLDisplay);
  EGLBoolean (*eglChooseConfig)(EGLDisplay, const EGLint *, EGLConfig *, EGLint, EGLint *);
  EGLBoolean (*eglBindAPI)(EGLenum);
  EGLContext (*eglCreateContext)(EGLDisplay, EGLConfig, EGLContext, const EGLint *);
  EGLBoolean (*eglDestroyContext)(EGLDisplay, EGLContext);
  EGLBoolean (*eglMakeCurrent)(EGLDisplay, EGLSurface, EGLSurface, EGLContext);
  __eglMustCastToProperFunctionPointerType (*eglGetProcAddress)(const char *);
  /* The extensions the import comes through, which eglGetProcAddress finds. */
  EGLDisplay (*eglGetPlatformDisplay)(unsigned, void *, const EGLint *);
  EGLBoolean (*eglQueryDevices)(EGLint, void **, EGLint *);
  const char *(*eglQueryDeviceString)(void *, EGLint);
  EGLImageKHR (*eglCreateImage)(EGLDisplay, EGLContext, EGLenum,
                                EGLClientBuffer, const EGLint *);
  EGLBoolean (*eglDestroyImage)(EGLDisplay, EGLImageKHR);
  /* The display of the process's own connection, tried when neither platform
   * above names the card: a gbm device present makes this the driver's own. */
  EGLDisplay (*eglGetDisplay)(void *);
};

/* GL's own entry points, for the one thing this file asks of GL: a texture over
 * an imported picture, which is how a tiled one reaches CUDA. libGL is dlopened
 * as the other two are. */
struct gl_api {
  void *library;
  void (*glGenTextures)(int, unsigned *);
  void (*glDeleteTextures)(int, const unsigned *);
  void (*glBindTexture)(unsigned, unsigned);
  void (*glTexParameteri)(unsigned, unsigned, int);
  void (*glEGLImageTargetTexture2DOES)(unsigned, void *);
  void (*glTexImage2D)(unsigned, int, int, int, int, int, unsigned, unsigned, const void *);
  /* A copy of one texture into another on the card, which is what takes the
   * imported picture into the one CUDA reads. */
  void (*glCopyImageSubData)(unsigned, unsigned, int, int, int, int, unsigned, unsigned, int,
                             int, int, int, int, int, int);
};

/* `EGL_PLATFORM_GBM_KHR` and GL's names come from eglext.h and glext.h; the few
 * this file spells out are the ones glext.h would need the whole header tree for. */
#define GL_TEXTURE_2D 0x0DE1u
#define GL_TEXTURE_MIN_FILTER 0x2801u
#define GL_TEXTURE_MAG_FILTER 0x2800u
#define GL_LINEAR 0x2601u
#define GL_RGBA8 0x8058u
#define GL_RGBA 0x1908u
#define GL_UNSIGNED_BYTE 0x1401u

/* One of the pictures a capture cycles through, as GL and CUDA hold it: a
 * screencast hands over the same few buffers frame after frame, so each is
 * imported, made a texture and registered with CUDA once, and a frame costs a
 * map and an unmap. Known by the buffer itself — the DMA-BUF's inode, which the
 * import's own reference keeps from being reused — and how the picture lies in
 * it. */
#define GL_KEPT 4
/* A kept picture not seen for this many frames is let go of: its capture moved
 * to other buffers, after a resize, and the import holds the old ones. */
#define GL_KEPT_FRAMES 30

struct gl_kept {
  int held;
  dev_t device;
  ino_t inode;
  unsigned offset, stride, width, height;
  unsigned long long modifier;
  EGLImageKHR image;
  unsigned texture;
  unsigned long long seen;
};

struct yapfast_cudaconvert {
  struct cuda_api cuda;
  struct egl_api egl;
  EGLDisplay display;
  EGLContext context;
  CUcontext cuda_context;
  CUdevice device;
  CUmodule module;
  CUfunction to_y, to_uv;
  /* The stream's picture, as NV12 on the card: one allocation, the luma rows and
   * then the interleaved chroma ones, at a pitch NVENC takes. */
  CUdeviceptr nv12;
  size_t nv12_size;
  /* The same rows for a picture the driver holds as an array — a tiled buffer,
   * which is what a compositor on NVIDIA's driver shares — copied on the card
   * before the kernels read them, where the driver has no texture objects to
   * read the array through. Grown as the pictures are, and never made where it
   * has. */
  CUdeviceptr rows;
  size_t rows_pitch;
  unsigned rows_height;
  /* The picture as a GL texture, for a tiled one the EGL resource wouldn't map:
   * the display and context GL is made on, the texture that holds the picture,
   * and whether that was tried and what it said. Nothing here is opened until a
   * picture needs it. */
  struct gl_api gl;
  void *gbm;
  void *gbm_device;
  EGLDisplay gl_display;
  EGLContext gl_context;
  struct gl_kept kept[GL_KEPT];
  /* The texture GL copies each picture into on the card, the one CUDA has
   * registered and reads: made once at the picture's size, and again only when
   * that changes. */
  unsigned copy_texture;
  CUgraphicsResource copy_resource;
  unsigned copy_width, copy_height;
  /* Frames through GL's door, which is what a kept picture's age is counted
   * in. */
  unsigned long long gl_frames;
  int gl_open;
  char gl_why[160];
  /* The layout the EGLImage's own resource refused and GL's texture took: a
   * driver that holds one tiled picture as an array it won't map holds them all
   * so, and every frame after the first goes straight to GL rather than paying
   * for an import, a registration and a refusal first. */
  unsigned long long gl_only_modifier;
  int gl_only;
  /* The card's node, kept open for the gbm device GL's display is made from: a
   * gbm device reads the driver's memory through that descriptor. */
  int drm;
  unsigned width, height, pitch;
  /* The picture's own size, until the kernels are launched at the stream's. */
  unsigned source_width, source_height;
  char device_name[96];
  char driver[96];
  char error[192];
  /* Frames' pixels moved by the processor: this path moves none, and says so. */
  unsigned long long copied;
  unsigned frames;
};

/* Letting go, declared before `yapfast_cudaconvert_new` because it is what an
 * open that can't finish calls. */
void yapfast_cudaconvert_free(struct yapfast_cudaconvert *c);

static void put(char *buffer, size_t size, const char *format, ...) {  if (buffer == NULL || size == 0) return;
  va_list arguments;
  va_start(arguments, format);
  vsnprintf(buffer, size, format, arguments);
  va_end(arguments);
}

static const char *cuda_name(struct yapfast_cudaconvert *c, CUresult result) {
  const char *name = NULL;
  if (c->cuda.cuGetErrorName == NULL) return "an unknown CUDA error";
  if (c->cuda.cuGetErrorName(result, &name) != CUDA_SUCCESS || name == NULL)
    return "an unknown CUDA error";
  return name;
}

/* The context the work runs in, made current for the length of a call: a CUDA
 * context is current per thread, and this handle is handed to the share's
 * encoder thread, so the opening thread's currency says nothing. EGL's current
 * context is per thread for the same reason, and an EGLImage is imported by the
 * context that holds the picture. */
static int enter(struct yapfast_cudaconvert *c) {
  if (c->cuda.cuCtxPushCurrent(c->cuda_context) != CUDA_SUCCESS) {
    put(c->error, sizeof(c->error), "CUDA wouldn't enter the picture's context");
    return 0;
  }
  if (c->display != EGL_NO_DISPLAY && c->context != EGL_NO_CONTEXT &&
      !c->egl.eglMakeCurrent(c->display, EGL_NO_SURFACE, EGL_NO_SURFACE, c->context)) {
    put(c->error, sizeof(c->error), "EGL wouldn't enter the import's context (0x%x)",
        c->egl.eglGetError());
    c->cuda.cuCtxPopCurrent(NULL);
    return 0;
  }
  return 1;
}

static void leave(struct yapfast_cudaconvert *c) {
  if (c->display != EGL_NO_DISPLAY && c->context != EGL_NO_CONTEXT)
    c->egl.eglMakeCurrent(c->display, EGL_NO_SURFACE, EGL_NO_SURFACE, EGL_NO_CONTEXT);
  c->cuda.cuCtxPopCurrent(NULL);
}

/* One CUDA call, with what the driver called the failure when it failed. */
#define CU(c, call, what)                                                      \
  do {                                                                         \
    CUresult result_ = (c)->cuda.call;                                         \
    if (result_ != CUDA_SUCCESS) {                                             \
      put((c)->error, sizeof((c)->error), "%s: %s", what, cuda_name((c), result_)); \
      return 0;                                                                \
    }                                                                          \
  } while (0)

static void *symbol(void *library, const char *name) {
  return library == NULL ? NULL : dlsym(library, name);
}

/* An EGLDisplay is the process's, not the converter's: every
 * `eglGetPlatformDisplay` for the same device answers the same handle,
 * `eglInitialize` on one already initialised does nothing, and one
 * `eglTerminate` ends it for everyone holding it. A share opens a converter for
 * each codec it could send and keeps one, so the ones it lets go of would
 * terminate the display the kept one imports on, whose next `eglMakeCurrent`
 * then answers EGL_NOT_INITIALIZED. Each display is counted here instead:
 * initialised by the first converter that takes it, terminated by the last that
 * lets go. A display the table has no room for is never terminated, which keeps
 * it initialised rather than ending it under another converter. */
static pthread_mutex_t displays_lock = PTHREAD_MUTEX_INITIALIZER;
static struct {
  EGLDisplay display;
  unsigned users;
} displays[16];

static int display_take(struct yapfast_cudaconvert *c, EGLDisplay display) {
  pthread_mutex_lock(&displays_lock);
  int free_slot = -1;
  for (int i = 0; i < (int)(sizeof(displays) / sizeof(displays[0])); i++) {
    if (displays[i].users > 0 && displays[i].display == display) {
      displays[i].users++;
      pthread_mutex_unlock(&displays_lock);
      return 1;
    }
    if (displays[i].users == 0 && free_slot < 0) free_slot = i;
  }
  EGLint major = 0, minor = 0;
  int ok = c->egl.eglInitialize(display, &major, &minor) == EGL_TRUE;
  if (ok && free_slot >= 0) {
    displays[free_slot].display = display;
    displays[free_slot].users = 1;
  }
  pthread_mutex_unlock(&displays_lock);
  return ok;
}

static void display_let_go(struct yapfast_cudaconvert *c, EGLDisplay display) {
  pthread_mutex_lock(&displays_lock);
  for (int i = 0; i < (int)(sizeof(displays) / sizeof(displays[0])); i++) {
    if (displays[i].users > 0 && displays[i].display == display) {
      if (--displays[i].users == 0) c->egl.eglTerminate(display);
      break;
    }
  }
  pthread_mutex_unlock(&displays_lock);
}

/* libcuda: the driver's own library, which every machine with the NVIDIA driver
 * that NVENC needs already has. */
static int open_cuda(struct yapfast_cudaconvert *c) {
  static const char *const names[] = {"libcuda.so.1", "libcuda.so", NULL};
  for (int i = 0; names[i] != NULL && c->cuda.library == NULL; i++)
    c->cuda.library = dlopen(names[i], RTLD_NOW | RTLD_LOCAL);
  if (c->cuda.library == NULL) {
    put(c->error, sizeof(c->error), "no CUDA driver library: %s", dlerror());
    return 0;
  }
  void *l = c->cuda.library;
#define LOAD(name, as)                                                         \
  do {                                                                         \
    c->cuda.as = symbol(l, name);                                              \
    if (c->cuda.as == NULL) {                                                  \
      put(c->error, sizeof(c->error), "the CUDA driver has no %s", name);      \
      return 0;                                                                \
    }                                                                          \
  } while (0)
  LOAD("cuInit", cuInit);
  LOAD("cuDriverGetVersion", cuDriverGetVersion);
  LOAD("cuDeviceGet", cuDeviceGet);
  LOAD("cuDeviceGetCount", cuDeviceGetCount);
  LOAD("cuDeviceGetName", cuDeviceGetName);
  LOAD("cuDeviceGetPCIBusId", cuDeviceGetPCIBusId);
  LOAD("cuCtxCreate_v2", cuCtxCreate);
  LOAD("cuCtxDestroy_v2", cuCtxDestroy);
  LOAD("cuCtxPushCurrent_v2", cuCtxPushCurrent);
  LOAD("cuCtxPopCurrent_v2", cuCtxPopCurrent);
  LOAD("cuCtxSynchronize", cuCtxSynchronize);
  LOAD("cuModuleLoadData", cuModuleLoadData);
  LOAD("cuModuleGetFunction", cuModuleGetFunction);
  LOAD("cuMemAlloc_v2", cuMemAlloc);
  LOAD("cuMemAllocPitch_v2", cuMemAllocPitch);
  LOAD("cuMemFree_v2", cuMemFree);
  LOAD("cuLaunchKernel", cuLaunchKernel);
  LOAD("cuGetErrorName", cuGetErrorName);
  LOAD("cuGraphicsEGLRegisterImage", cuGraphicsEGLRegisterImage);
  LOAD("cuGraphicsUnregisterResource", cuGraphicsUnregisterResource);
  LOAD("cuGraphicsMapResources", cuGraphicsMapResources);
  LOAD("cuGraphicsUnmapResources", cuGraphicsUnmapResources);
  LOAD("cuGraphicsResourceGetMappedEglFrame", cuGraphicsResourceGetMappedEglFrame);
#undef LOAD
  /* Optional, for the GL door a tiled picture takes: a driver without them still
   * converts every picture whose rows it can read, and the GL route says so. */
  *(void **)(&c->cuda.cuMemcpy3D) = symbol(l, "cuMemcpy3D_v2");
  *(void **)(&c->cuda.cuTexObjectCreate) = symbol(l, "cuTexObjectCreate");
  *(void **)(&c->cuda.cuTexObjectDestroy) = symbol(l, "cuTexObjectDestroy");
  *(void **)(&c->cuda.cuGraphicsGLRegisterImage) = symbol(l, "cuGraphicsGLRegisterImage");
  *(void **)(&c->cuda.cuGraphicsSubResourceGetMappedArray) =
      symbol(l, "cuGraphicsSubResourceGetMappedArray");
  return 1;
}

/* The PCI address of the card a DRM node is, which is how the kernel, EGL and
 * CUDA all name one card: `/sys/class/drm/renderD128/device` and
 * `/sys/class/drm/card1/device` are links to the same PCI device, whose own name
 * is the last part of the link. A node that isn't a DRM one, or a machine
 * without that tree, answers nothing. */
static int drm_node_pci(const char *node, char *pci, size_t size) {
  const char *slash = strrchr(node, '/');
  const char *name = slash ? slash + 1 : node;
  char link[128], target[256];
  snprintf(link, sizeof(link), "/sys/class/drm/%s/device", name);
  ssize_t length = readlink(link, target, sizeof(target) - 1);
  if (length <= 0) return 0;
  target[length] = '\0';
  const char *last = strrchr(target, '/');
  snprintf(pci, size, "%.*s", (int)(size - 1), last ? last + 1 : target);
  return pci[0] != '\0';
}

/* The EGL device that is the descriptor's own. A buffer can only be imported by
 * the device whose memory it is, so the one the DRM node names is looked for
 * rather than the first one offered: a machine with two cards offers several,
 * and Mesa offers one for NVIDIA's node too, which can't import it. */
static void *egl_device_for(struct yapfast_cudaconvert *c, int drm_fd, const char *node) {
  void *devices[16];
  EGLint count = 0;
  if (c->egl.eglQueryDevices == NULL) return NULL;
  if (!c->egl.eglQueryDevices(16, devices, &count)) return NULL;
  if (count > 16) count = 16;
  struct stat ours;
  int have_ours = fstat(drm_fd, &ours) == 0;
  char ours_pci[32] = "";
  drm_node_pci(node, ours_pci, sizeof(ours_pci));
  for (EGLint i = 0; i < count; i++) {
    /* `EGL_EXT_device_drm_render_node` names the render node itself, which is
     * the path the descriptor came from; the extension that predates it names
     * whichever node the driver likes, and NVIDIA names the card. */
    const char *render = c->egl.eglQueryDeviceString(devices[i], EGL_DRM_RENDER_NODE_FILE_EXT);
    if (render != NULL && !strcmp(render, node)) return devices[i];
    const char *file = c->egl.eglQueryDeviceString(devices[i], EGL_DRM_DEVICE_FILE_EXT);
    if (file == NULL) continue;
    struct stat theirs;
    if (have_ours && stat(file, &theirs) == 0 && theirs.st_rdev == ours.st_rdev)
      return devices[i];
    /* The card node of the same card: `/dev/dri/card1` for `/dev/dri/renderD128`
     * on a machine whose numbering isn't the 128 apart of one card alone. */
    char theirs_pci[32] = "";
    if (ours_pci[0] != '\0' && drm_node_pci(file, theirs_pci, sizeof(theirs_pci)) &&
        !strcasecmp(theirs_pci, ours_pci))
      return devices[i];
  }
  /* A driver that names no file leaves one device, which is the only one it
   * could be. */
  return count == 1 ? devices[0] : NULL;
}

/* EGL on that device, and a context to make the import in: an EGLImage needs
 * one current, and this path never draws with it. */
static int open_egl(struct yapfast_cudaconvert *c, int drm_fd, const char *node,
                    void **device) {
  static const char *const names[] = {"libEGL.so.1", "libEGL.so", NULL};
  for (int i = 0; names[i] != NULL && c->egl.library == NULL; i++)
    c->egl.library = dlopen(names[i], RTLD_NOW | RTLD_LOCAL);
  if (c->egl.library == NULL) {
    put(c->error, sizeof(c->error), "no EGL library: %s", dlerror());
    return 0;
  }
  void *l = c->egl.library;
#define EGL_LOAD(name)                                                        \
  do {                                                                        \
    c->egl.egl##name = symbol(l, "egl" #name);                                \
    if (c->egl.egl##name == NULL) {                                           \
      put(c->error, sizeof(c->error), "EGL has no egl%s", #name);             \
      return 0;                                                               \
    }                                                                         \
  } while (0)
  c->egl.eglGetProcAddress = symbol(l, "eglGetProcAddress");
  if (c->egl.eglGetProcAddress == NULL) {
    put(c->error, sizeof(c->error), "EGL has no eglGetProcAddress");
    return 0;
  }
  EGL_LOAD(GetError);
  EGL_LOAD(Initialize);
  EGL_LOAD(Terminate);
  EGL_LOAD(ChooseConfig);
  EGL_LOAD(BindAPI);
  EGL_LOAD(CreateContext);
  EGL_LOAD(DestroyContext);
  EGL_LOAD(MakeCurrent);
#undef EGL_LOAD
  c->egl.eglGetPlatformDisplay =
      (EGLDisplay(*)(unsigned, void *, const EGLint *))c->egl.eglGetProcAddress(
          "eglGetPlatformDisplayEXT");
  c->egl.eglQueryDevices =
      (EGLBoolean(*)(EGLint, void **, EGLint *))c->egl.eglGetProcAddress("eglQueryDevicesEXT");
  c->egl.eglQueryDeviceString =
      (const char *(*)(void *, EGLint))c->egl.eglGetProcAddress("eglQueryDeviceStringEXT");
  c->egl.eglCreateImage =
      (EGLImageKHR(*)(EGLDisplay, EGLContext, EGLenum, EGLClientBuffer, const EGLint *))
          c->egl.eglGetProcAddress("eglCreateImageKHR");
  c->egl.eglDestroyImage =
      (EGLBoolean(*)(EGLDisplay, EGLImageKHR))c->egl.eglGetProcAddress("eglDestroyImageKHR");
  /* GL's display is made from the process's own connection when no platform
   * names the card, so this is looked for rather than required. */
  c->egl.eglGetDisplay = symbol(l, "eglGetDisplay");
  if (c->egl.eglGetPlatformDisplay == NULL || c->egl.eglQueryDevices == NULL ||
      c->egl.eglQueryDeviceString == NULL || c->egl.eglCreateImage == NULL ||
      c->egl.eglDestroyImage == NULL) {
    put(c->error, sizeof(c->error), "EGL has no device platform, or no DMA-BUF import");
    return 0;
  }
  *device = egl_device_for(c, drm_fd, node);
  if (*device == NULL) {
    put(c->error, sizeof(c->error), "no EGL device for %s", node);
    return 0;
  }
  c->display = c->egl.eglGetPlatformDisplay(EGL_PLATFORM_DEVICE_EXT, *device, NULL);
  if (c->display == EGL_NO_DISPLAY) {
    put(c->error, sizeof(c->error), "EGL wouldn't open a display for %s", node);
    return 0;
  }
  if (!display_take(c, c->display)) {
    put(c->error, sizeof(c->error), "EGL wouldn't initialize %s (0x%x)", node,
        c->egl.eglGetError());
    c->display = EGL_NO_DISPLAY;
    return 0;
  }
  c->egl.eglBindAPI(EGL_OPENGL_API);
  const EGLint configs[] = {EGL_SURFACE_TYPE, EGL_PBUFFER_BIT, EGL_RENDERABLE_TYPE,
                            EGL_OPENGL_BIT, EGL_NONE};
  EGLConfig config = NULL;
  EGLint found = 0;
  c->egl.eglChooseConfig(c->display, configs, &config, 1, &found);
  const EGLint context_attribs[] = {EGL_CONTEXT_MAJOR_VERSION, 3,
                                    EGL_CONTEXT_MINOR_VERSION, 0, EGL_NONE};
  c->context = c->egl.eglCreateContext(c->display, found > 0 ? config : EGL_NO_CONFIG_KHR,
                                       EGL_NO_CONTEXT, context_attribs);
  if (c->context == EGL_NO_CONTEXT) {
    put(c->error, sizeof(c->error), "EGL wouldn't open a context on %s", node);
    return 0;
  }
  if (!c->egl.eglMakeCurrent(c->display, EGL_NO_SURFACE, EGL_NO_SURFACE, c->context)) {
    put(c->error, sizeof(c->error), "EGL wouldn't make the context current on %s", node);
    return 0;
  }
  return 1;
}

/* Which CUDA device that EGL device is. The driver can answer its ordinal as a
 * string, but a loader in front of it — glvnd, which is what a distribution
 * links — answers nothing for that query, and the card is the same card either
 * way: both EGL and CUDA name it by the PCI address Linux gives it. */
static int cuda_device_for(struct yapfast_cudaconvert *c, void *egl_device, const char *node) {
  const char *named = c->egl.eglQueryDeviceString(egl_device, EGL_CUDA_DEVICE_NV);
  int ordinal = 0;
  if (named != NULL && sscanf(named, "CUDA-%d", &ordinal) == 1) {
    CU(c, cuDeviceGet(&c->device, ordinal), "no such CUDA device");
    return 1;
  }
  char ours[32] = "";
  int count = 0;
  if (c->cuda.cuDeviceGetCount == NULL || c->cuda.cuDeviceGetCount(&count) != CUDA_SUCCESS)
    count = 0;
  if (drm_node_pci(node, ours, sizeof(ours)) && c->cuda.cuDeviceGetPCIBusId != NULL) {
    for (int i = 0; i < count; i++) {
      CUdevice candidate;
      if (c->cuda.cuDeviceGet(&candidate, i) != CUDA_SUCCESS) continue;
      char theirs[32] = "";
      if (c->cuda.cuDeviceGetPCIBusId(theirs, (int)sizeof(theirs), candidate) == CUDA_SUCCESS &&
          !strcasecmp(theirs, ours)) {
        c->device = candidate;
        return 1;
      }
    }
  }
  /* One card whose address nothing would name: it is the only one there is. */
  if (count == 1) {
    CU(c, cuDeviceGet(&c->device, 0), "no CUDA device");
    return 1;
  }
  put(c->error, sizeof(c->error),
      "no CUDA device is the card at %s, where the EGL device is and the picture came from%s",
      ours[0] != '\0' ? ours : node, count > 0 ? "" : ", and the driver has none");
  return 0;
}

/* Opens everything this path needs on the device `drm_fd` is, at the stream's
 * size: the picture's own is known per frame. The PTX is `build.rs`'s. */
struct yapfast_cudaconvert *yapfast_cudaconvert_new(int drm_fd, unsigned width,
                                                    unsigned height, const void *ptx,
                                                    size_t ptx_size, char *error,
                                                    size_t size) {
  put(error, size, "not opened");
  struct yapfast_cudaconvert *c = calloc(1, sizeof(*c));
  if (c == NULL) return NULL;
  /* The card's node, held open: a gbm device made from it later reads through the
   * descriptor, and the caller's own copy is gone by then. */
  c->drm = dup(drm_fd);
  /* The node the descriptor came from, by the path the kernel gave the driver:
   * EGL names its devices the same way. */
  char link[64], node[256];
  snprintf(link, sizeof(link), "/proc/self/fd/%d", drm_fd);
  ssize_t length = readlink(link, node, sizeof(node) - 1);
  if (length <= 0) {
    put(error, size, "the descriptor's device has no name");
    if (c->drm >= 0) close(c->drm);
    free(c);
    return NULL;
  }
  node[length] = '\0';
  if (!open_cuda(c)) {
    put(error, size, "%s", c->error);
    yapfast_cudaconvert_free(c);
    return NULL;
  }
  /* The driver is initialised before anything asks it a question: `cuDeviceGet`
   * and the EGL device's CUDA ordinal both need it, and nothing else in Yapfast
   * initialises the driver. */
  if (c->cuda.cuInit(0) != CUDA_SUCCESS) {
    put(c->error, sizeof(c->error), "the CUDA driver wouldn't start");
    put(error, size, "%s", c->error);
    yapfast_cudaconvert_free(c);
    return NULL;
  }
  void *egl_device = NULL;
  if (!open_egl(c, drm_fd, node, &egl_device) || !cuda_device_for(c, egl_device, node)) {
    put(error, size, "%s", c->error);
    yapfast_cudaconvert_free(c);
    return NULL;
  }
  if (c->cuda.cuCtxCreate(&c->cuda_context, 0, c->device) != CUDA_SUCCESS) {
    put(c->error, sizeof(c->error), "no CUDA context");
    put(error, size, "%s", c->error);
    yapfast_cudaconvert_free(c);
    return NULL;
  }
  int version = 0;
  if (c->cuda.cuDriverGetVersion(&version) == CUDA_SUCCESS)
    put(c->driver, sizeof(c->driver), "CUDA %d.%d", version / 1000, (version % 1000) / 10);
  else
    put(c->driver, sizeof(c->driver), "CUDA");
  if (c->cuda.cuDeviceGetName(c->device_name, sizeof(c->device_name), c->device) != CUDA_SUCCESS)
    put(c->device_name, sizeof(c->device_name), "an NVIDIA device");
  CUresult loaded = c->cuda.cuModuleLoadData(&c->module, ptx);
  if (loaded != CUDA_SUCCESS) {
    put(c->error, sizeof(c->error), "the convert kernels wouldn't load: %s",
        cuda_name(c, loaded));
    put(error, size, "%s", c->error);
    yapfast_cudaconvert_free(c);
    return NULL;
  }
  char name[64];
  if (c->cuda.cuModuleGetFunction(&c->to_y, c->module, "bgra_to_y") != CUDA_SUCCESS) {
    put(c->error, sizeof(c->error), "the PTX has no bgra_to_y");
    put(error, size, "%s", c->error);
    yapfast_cudaconvert_free(c);
    return NULL;
  }
  if (c->cuda.cuModuleGetFunction(&c->to_uv, c->module, "bgra_to_uv") != CUDA_SUCCESS) {
    put(c->error, sizeof(c->error), "the PTX has no bgra_to_uv");
    put(error, size, "%s", c->error);
    yapfast_cudaconvert_free(c);
    return NULL;
  }
  (void)ptx_size;
  (void)name;
  /* An NV12 picture at a pitch NVENC takes: 256 bytes a row, as its own encoder
   * asks of a device pointer it wasn't told the geometry of. */
  c->width = width & ~1u;
  c->height = height & ~1u;
  c->pitch = (c->width + 255u) & ~255u;
  /* A whole number of macroblocks' rows is allocated, though only the stream's
   * own are written: a driver that rounds a surface's height up to sixteen reads
   * the padding for the last chroma row rather than past the allocation. */
  c->nv12_size = (size_t)c->pitch * ((c->height + 15u) & ~15u) * 3 / 2;
  if (c->cuda.cuMemAlloc(&c->nv12, c->nv12_size) != CUDA_SUCCESS) {
    put(c->error, sizeof(c->error), "no room on the card for the picture");
    put(error, size, "%s", c->error);
    yapfast_cudaconvert_free(c);
    return NULL;
  }
  /* Nothing is current on the thread that opened this: the work enters the two
   * contexts per call, and a share that never encodes leaves no binding behind. */
  if (c->display != EGL_NO_DISPLAY && c->context != EGL_NO_CONTEXT)
    c->egl.eglMakeCurrent(c->display, EGL_NO_SURFACE, EGL_NO_SURFACE, EGL_NO_CONTEXT);
  /* The picture the kernels are given, until a frame says otherwise. */
  c->source_width = c->width;
  c->source_height = c->height;
  return c;
}

unsigned yapfast_cudaconvert_pitch(struct yapfast_cudaconvert *c) { return c->pitch; }

void *yapfast_cudaconvert_nv12(struct yapfast_cudaconvert *c) {
  return c == NULL ? NULL : (void *)(uintptr_t)c->nv12;
}

/* The CUDA context the picture lives in. An encoder handed the buffer has to
 * register it in this context and no other: a pointer `cuMemAlloc` made belongs
 * to the context that made it. */
void *yapfast_cudaconvert_context(struct yapfast_cudaconvert *c) {
  return c == NULL ? NULL : (void *)c->cuda_context;
}

const char *yapfast_cudaconvert_device(struct yapfast_cudaconvert *c) {
  return c == NULL ? "" : c->device_name;
}

const char *yapfast_cudaconvert_driver(struct yapfast_cudaconvert *c) {
  return c == NULL ? "" : c->driver;
}

unsigned long long yapfast_cudaconvert_copied(struct yapfast_cudaconvert *c) {
  return c == NULL ? 0 : c->copied;
}

const char *yapfast_cudaconvert_error(struct yapfast_cudaconvert *c) {
  return c == NULL ? "not opened" : c->error;
}

/* Runs both kernels: the stream's picture comes out as NV12 in `c->nv12`, and
 * the caller hands that buffer to its encoder. The picture is `pitch`-byte rows
 * at `pixels`, or, where `texture` isn't zero, the array that texture reads. */
static int launch(struct yapfast_cudaconvert *c, const void *pixels, unsigned pitch,
                  int width, int height, int red_first, CUtexObject texture) {
  unsigned char *frames = (unsigned char *)(uintptr_t)c->nv12;
  /* Laid out as `native/cuda_convert.cu`'s `bgra_frame`. */
  struct {
    const unsigned char *pixels;
    unsigned pitch;
    int width, height;
    int red_first;
    unsigned long long texture;
  } source = {pixels, pitch, width, height, red_first, texture};
  struct {
    unsigned char *pixels;
    unsigned pitch;
    int width, height;
  } nv12 = {frames, c->pitch, (int)c->width, (int)c->height};
  void *arguments[] = {&source, &nv12};
  unsigned block = 16;
  unsigned blocks_x = (c->width + block - 1) / block;
  unsigned blocks_y = (c->height + block - 1) / block;
  CU(c, cuLaunchKernel(c->to_y, blocks_x, blocks_y, 1, block, block, 1, 0, NULL, arguments, NULL),
     "the luma kernel wouldn't start");
  blocks_x = ((c->width / 2) + block - 1) / block;
  blocks_y = ((c->height / 2) + block - 1) / block;
  CU(c, cuLaunchKernel(c->to_uv, blocks_x, blocks_y, 1, block, block, 1, 0, NULL, arguments, NULL),
     "the chroma kernel wouldn't start");
  CU(c, cuCtxSynchronize(), "the convert didn't finish");
  c->frames++;
  return 1;
}

/* One captured picture: `fd`, `offset`, `stride` and `modifier` describe the
 * memory the compositor drew, `width` by `height` of BGRA or BGRX pixels in it.
 * The picture lands in the NV12 buffer this was opened with, and needs nothing
 * of the processor. */
/* Room on the card for a tiled picture's rows, which are copied out of the array
 * before the kernels read them: allocated at the picture's own size, and grown
 * when a picture is bigger than the one before it. */
static int rows_for(struct yapfast_cudaconvert *c, unsigned width, unsigned height) {
  size_t needed = (size_t)width * 4;
  if (c->rows != 0 && c->rows_pitch >= needed && c->rows_height >= height) return 1;
  if (c->rows != 0) {
    c->cuda.cuMemFree(c->rows);
    c->rows = 0;
    c->rows_pitch = 0;
    c->rows_height = 0;
  }
  CUdeviceptr rows = 0;
  size_t pitch = 0;
  if (c->cuda.cuMemAllocPitch(&rows, &pitch, needed, height, 4) != CUDA_SUCCESS) return 0;
  c->rows = rows;
  c->rows_pitch = pitch;
  c->rows_height = height;
  return 1;
}

/* The picture as a GL texture, which is the door a tiled one takes: the driver
 * answers an error for the resource an EGLImage registered, and GL copies the
 * picture into a texture of its own that CUDA maps, whose array the kernels read
 * through a texture object, or, without those, from rows `cuMemcpy3D` copies it
 * into, where `cuMemcpy2D` refuses a block-linear array.
 *
 * GL and gbm are dlopened as libEGL and libcuda are, and nothing is opened until
 * a picture needs them: every picture the first door takes costs nothing here. */
static int gl_ready(struct yapfast_cudaconvert *c) {
  if (c->gl_open > 0) return 1;
  if (c->gl_open < 0) return 0;
  c->gl_open = -1;
  const int reads_arrays = c->cuda.cuMemcpy3D != NULL ||
                           (c->cuda.cuTexObjectCreate != NULL && c->cuda.cuTexObjectDestroy != NULL);
  if (c->cuda.cuGraphicsGLRegisterImage == NULL ||
      c->cuda.cuGraphicsSubResourceGetMappedArray == NULL || !reads_arrays) {
    put(c->gl_why, sizeof(c->gl_why), "the CUDA driver has no GL interop");
    return 0;
  }
  static const char *const gl_names[] = {"libGL.so.1", "libGL.so", NULL};
  for (int i = 0; gl_names[i] != NULL && c->gl.library == NULL; i++)
    c->gl.library = dlopen(gl_names[i], RTLD_NOW | RTLD_LOCAL);
  if (c->gl.library == NULL) {
    put(c->gl_why, sizeof(c->gl_why), "no GL library: %s", dlerror());
    return 0;
  }
  void *l = c->gl.library;
#define GL_LOAD(field, name)                                                  \
  do {                                                                       \
    *(void **)(&c->gl.field) = symbol(l, name);                               \
    if (c->gl.field == NULL) {                                                \
      put(c->gl_why, sizeof(c->gl_why), "GL has no %s", name);                \
      return 0;                                                              \
    }                                                                        \
  } while (0)
  GL_LOAD(glGenTextures, "glGenTextures");
  GL_LOAD(glDeleteTextures, "glDeleteTextures");
  GL_LOAD(glTexImage2D, "glTexImage2D");
  GL_LOAD(glBindTexture, "glBindTexture");
  GL_LOAD(glTexParameteri, "glTexParameteri");
  /* The one entry point that makes a texture of an imported picture, which only
   * eglGetProcAddress finds. */
  *(void **)(&c->gl.glEGLImageTargetTexture2DOES) =
      (void *)c->egl.eglGetProcAddress("glEGLImageTargetTexture2DOES");
  *(void **)(&c->gl.glCopyImageSubData) = (void *)c->egl.eglGetProcAddress("glCopyImageSubData");
  if (c->gl.glCopyImageSubData == NULL) {
    put(c->gl_why, sizeof(c->gl_why), "GL has no glCopyImageSubData");
    return 0;
  }
  if (c->gl.glEGLImageTargetTexture2DOES == NULL) {
    put(c->gl_why, sizeof(c->gl_why), "GL has no glEGLImageTargetTexture2DOES");
    return 0;
  }
#undef GL_LOAD
  /* A gbm device on the picture's card, which is what makes the display GL binds
   * to the card's own: the display the EGL device platform makes refuses GL
   * interop on this driver. */
  static const char *const gbm_names[] = {"libgbm.so.1", "libgbm.so", NULL};
  for (int i = 0; gbm_names[i] != NULL && c->gbm == NULL; i++)
    c->gbm = dlopen(gbm_names[i], RTLD_NOW | RTLD_LOCAL);
  if (c->gbm != NULL) {
    void *(*create)(int) = symbol(c->gbm, "gbm_create_device");
    if (create != NULL && c->drm >= 0) c->gbm_device = create(c->drm);
  }
  if (c->gbm_device != NULL)
    c->gl_display =
        c->egl.eglGetPlatformDisplay(EGL_PLATFORM_GBM_KHR, c->gbm_device, NULL);
  if (c->gl_display == EGL_NO_DISPLAY && c->egl.eglGetDisplay != NULL)
    c->gl_display = c->egl.eglGetDisplay(EGL_DEFAULT_DISPLAY);
  if (c->gl_display == EGL_NO_DISPLAY) {
    put(c->gl_why, sizeof(c->gl_why), "no EGL display GL can be made on");
    return 0;
  }
  if (!display_take(c, c->gl_display)) {
    put(c->gl_why, sizeof(c->gl_why), "EGL wouldn't initialize GL's display (0x%x)",
        c->egl.eglGetError());
    c->gl_display = EGL_NO_DISPLAY;
    return 0;
  }
  c->egl.eglBindAPI(EGL_OPENGL_API);
  const EGLint configs[] = {EGL_SURFACE_TYPE, EGL_PBUFFER_BIT, EGL_RENDERABLE_TYPE,
                            EGL_OPENGL_BIT, EGL_NONE};
  EGLConfig config = NULL;
  EGLint found = 0;
  c->egl.eglChooseConfig(c->gl_display, configs, &config, 1, &found);
  const EGLint context_attribs[] = {EGL_CONTEXT_MAJOR_VERSION, 3,
                                    EGL_CONTEXT_MINOR_VERSION, 0, EGL_NONE};
  c->gl_context = c->egl.eglCreateContext(c->gl_display,
                                          found > 0 ? config : EGL_NO_CONFIG_KHR,
                                          EGL_NO_CONTEXT, context_attribs);
  if (c->gl_context == EGL_NO_CONTEXT) {
    put(c->gl_why, sizeof(c->gl_why), "EGL wouldn't open GL's context (0x%x)",
        c->egl.eglGetError());
    return 0;
  }
  if (!c->egl.eglMakeCurrent(c->gl_display, EGL_NO_SURFACE, EGL_NO_SURFACE, c->gl_context)) {
    put(c->gl_why, sizeof(c->gl_why), "EGL wouldn't make GL's context current (0x%x)",
        c->egl.eglGetError());
    return 0;
  }
  c->egl.eglMakeCurrent(c->gl_display, EGL_NO_SURFACE, EGL_NO_SURFACE, EGL_NO_CONTEXT);
  c->gl_open = 1;
  return 1;
}

/* The array's rows on the card, in `c->rows`: the copy a block-linear picture
 * takes, since `cuMemcpy2D` refuses one. */
static int copy_rows(struct yapfast_cudaconvert *c, void *array, unsigned width, unsigned height) {
  if (!rows_for(c, width, height)) {
    put(c->error, sizeof(c->error), "no room on the card for the picture's rows");
    return 0;
  }
  cuda_memcpy3d copy;
  memset(&copy, 0, sizeof(copy));
  copy.srcMemoryType = CU_MEMORYTYPE_ARRAY;
  copy.srcArray = array;
  copy.srcHeight = height;
  copy.dstMemoryType = CU_MEMORYTYPE_DEVICE;
  copy.dstDevice = c->rows;
  copy.dstPitch = c->rows_pitch;
  copy.dstHeight = height;
  copy.WidthInBytes = (size_t)width * 4;
  copy.Height = height;
  copy.Depth = 1;
  CUresult copied = c->cuda.cuMemcpy3D(&copy);
  if (copied != CUDA_SUCCESS) {
    put(c->error, sizeof(c->error), "the card wouldn't copy the picture's rows: %s",
        cuda_name(c, copied));
    return 0;
  }
  return 1;
}

/* The array behind the texture GL copied the picture into, read by the kernels:
 * through a texture object where the driver has them, and copied on the card
 * into `c->rows` first where it hasn't. The array holds the picture as GL does,
 * red first, where the descriptor's own memory is blue first. */
static int convert_array(struct yapfast_cudaconvert *c, void *array, unsigned width,
                         unsigned height) {
  if (c->cuda.cuTexObjectCreate != NULL && c->cuda.cuTexObjectDestroy != NULL) {
    cuda_resource_desc resource;
    memset(&resource, 0, sizeof(resource));
    resource.resType = CU_RESOURCE_TYPE_ARRAY;
    resource.res.hArray = array;
    cuda_texture_desc texture;
    memset(&texture, 0, sizeof(texture));
    texture.addressMode[0] = texture.addressMode[1] = texture.addressMode[2] =
        CU_TR_ADDRESS_MODE_CLAMP;
    texture.filterMode = CU_TR_FILTER_MODE_POINT;
    /* Each channel as the byte it is, at whole texel coordinates: the kernels
     * filter on their own, as they do rows. */
    texture.flags = CU_TRSF_READ_AS_INTEGER;
    CUtexObject object = 0;
    if (c->cuda.cuTexObjectCreate(&object, &resource, &texture, NULL) == CUDA_SUCCESS) {
      int ok = launch(c, NULL, 0, (int)width, (int)height, 1, object);
      c->cuda.cuTexObjectDestroy(object);
      return ok;
    }
  }
  if (!copy_rows(c, array, width, height)) return 0;
  return launch(c, (const void *)(uintptr_t)c->rows, (unsigned)c->rows_pitch, (int)width,
                (int)height, 1, 0);
}

/* Lets go of a kept picture: GL's texture and the import, in that order. GL's
 * context is current. */
static void let_go_of_kept(struct yapfast_cudaconvert *c, struct gl_kept *kept) {
  if (!kept->held) return;
  if (kept->texture != 0) c->gl.glDeleteTextures(1, &kept->texture);
  if (kept->image != EGL_NO_IMAGE_KHR) c->egl.eglDestroyImage(c->gl_display, kept->image);
  memset(kept, 0, sizeof(*kept));
}

/* Imports the picture into `kept`: the EGLImage and a texture over it. 1 when
 * both are made; GL's context is current. */
static int keep_picture(struct yapfast_cudaconvert *c, struct gl_kept *kept, int fd,
                        unsigned offset, unsigned stride, unsigned long long modifier,
                        unsigned width, unsigned height) {
  /* A picture without alpha is imported as one with it: the memory is laid out
   * the same, the kernels never read the fourth byte, and a texture of three
   * channels — what the driver makes of XRGB8888 — is one CUDA refuses to
   * register (CUDA_ERROR_INVALID_VALUE). */
  const EGLint attribs[] = {
      EGL_WIDTH,                          (EGLint)width,
      EGL_HEIGHT,                         (EGLint)height,
      EGL_LINUX_DRM_FOURCC_EXT,           (EGLint)DRM_FORMAT_ARGB8888,
      EGL_DMA_BUF_PLANE0_FD_EXT,          fd,
      EGL_DMA_BUF_PLANE0_OFFSET_EXT,      (EGLint)offset,
      EGL_DMA_BUF_PLANE0_PITCH_EXT,       (EGLint)stride,
      EGL_DMA_BUF_PLANE0_MODIFIER_LO_EXT, (EGLint)(modifier & 0xffffffffull),
      EGL_DMA_BUF_PLANE0_MODIFIER_HI_EXT, (EGLint)(modifier >> 32),
      EGL_NONE,
  };
  kept->held = 1;
  kept->image = c->egl.eglCreateImage(c->gl_display, EGL_NO_CONTEXT, EGL_LINUX_DMA_BUF_EXT,
                                      NULL, attribs);
  if (kept->image == EGL_NO_IMAGE_KHR) {
    put(c->gl_why, sizeof(c->gl_why), "GL's display wouldn't import the picture (0x%x)",
        c->egl.eglGetError());
    let_go_of_kept(c, kept);
    return 0;
  }
  c->gl.glGenTextures(1, &kept->texture);
  c->gl.glBindTexture(GL_TEXTURE_2D, kept->texture);
  c->gl.glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, (int)GL_LINEAR);
  c->gl.glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MAG_FILTER, (int)GL_LINEAR);
  c->gl.glEGLImageTargetTexture2DOES(GL_TEXTURE_2D, kept->image);
  return 1;
}

/* Lets go of the texture CUDA reads, and its registration. Both contexts are
 * current. */
static void let_go_of_copy(struct yapfast_cudaconvert *c) {
  if (c->copy_resource != NULL) c->cuda.cuGraphicsUnregisterResource(c->copy_resource);
  if (c->copy_texture != 0) c->gl.glDeleteTextures(1, &c->copy_texture);
  c->copy_resource = NULL;
  c->copy_texture = 0;
  c->copy_width = c->copy_height = 0;
}

/* The texture CUDA reads, at `width` by `height`: made and registered once, and
 * again when the pictures change size. Mapping a texture over an imported
 * picture copies the whole of it every time, decompressing it, where mapping
 * one of GL's own is a wait for GL's work: so the picture is copied into this
 * one by GL, on the card, and CUDA reads that. */
static int copy_ready(struct yapfast_cudaconvert *c, unsigned width, unsigned height) {
  if (c->copy_resource != NULL && c->copy_width == width && c->copy_height == height) return 1;
  let_go_of_copy(c);
  c->gl.glGenTextures(1, &c->copy_texture);
  c->gl.glBindTexture(GL_TEXTURE_2D, c->copy_texture);
  c->gl.glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, (int)GL_LINEAR);
  c->gl.glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MAG_FILTER, (int)GL_LINEAR);
  c->gl.glTexImage2D(GL_TEXTURE_2D, 0, (int)GL_RGBA8, (int)width, (int)height, 0, GL_RGBA,
                     GL_UNSIGNED_BYTE, NULL);
  CUresult r = c->cuda.cuGraphicsGLRegisterImage(&c->copy_resource, c->copy_texture,
                                                GL_TEXTURE_2D,
                                                CU_GRAPHICS_REGISTER_FLAGS_READ_ONLY);
  if (r != CUDA_SUCCESS) {
    c->copy_resource = NULL;
    put(c->gl_why, sizeof(c->gl_why), "CUDA wouldn't take GL's texture for the picture: %s",
        cuda_name(c, r));
    let_go_of_copy(c);
    return 0;
  }
  c->copy_width = width;
  c->copy_height = height;
  return 1;
}

/* Copies a kept picture into the texture CUDA reads, and runs the kernels over
 * that while it is mapped. */
static int convert_kept(struct yapfast_cudaconvert *c, struct gl_kept *kept) {
  if (!copy_ready(c, kept->width, kept->height)) return 0;
  c->gl.glCopyImageSubData(kept->texture, GL_TEXTURE_2D, 0, 0, 0, 0, c->copy_texture,
                           GL_TEXTURE_2D, 0, 0, 0, 0, (int)kept->width, (int)kept->height, 1);
  CUresult r = c->cuda.cuGraphicsMapResources(1, &c->copy_resource, NULL);
  if (r != CUDA_SUCCESS) {
    put(c->gl_why, sizeof(c->gl_why), "CUDA wouldn't map GL's texture over the picture: %s",
        cuda_name(c, r));
    return 0;
  }
  int ok = 0;
  void *array = NULL;
  r = c->cuda.cuGraphicsSubResourceGetMappedArray(&array, c->copy_resource, 0, 0);
  if (r != CUDA_SUCCESS || array == NULL) {
    put(c->gl_why, sizeof(c->gl_why), "CUDA wouldn't say where GL's picture lies: %s",
        cuda_name(c, r));
  } else {
    ok = convert_array(c, array, kept->width, kept->height);
    if (!ok) put(c->gl_why, sizeof(c->gl_why), "%s", c->error);
  }
  c->cuda.cuGraphicsUnmapResources(1, &c->copy_resource, NULL);
  return ok;
}

/* The picture through GL's texture, scaled and converted into `c->nv12`: 1 when
 * it is there. GL's context takes the thread for the length of the call and
 * leaves it to the import's own. */
static int take_through_gl(struct yapfast_cudaconvert *c, int fd, unsigned offset,
                           unsigned stride, unsigned fourcc, unsigned long long modifier,
                           unsigned width, unsigned height) {
  (void)fourcc;
  c->gl_why[0] = '\0';
  if (!gl_ready(c)) return 0;
  struct stat buffer;
  if (fstat(fd, &buffer) != 0) {
    put(c->gl_why, sizeof(c->gl_why), "the picture's memory can't be told apart from another's");
    return 0;
  }
  c->egl.eglMakeCurrent(c->display, EGL_NO_SURFACE, EGL_NO_SURFACE, EGL_NO_CONTEXT);
  if (!c->egl.eglMakeCurrent(c->gl_display, EGL_NO_SURFACE, EGL_NO_SURFACE, c->gl_context)) {
    put(c->gl_why, sizeof(c->gl_why), "EGL wouldn't make GL's context current (0x%x)",
        c->egl.eglGetError());
    c->egl.eglMakeCurrent(c->display, EGL_NO_SURFACE, EGL_NO_SURFACE, c->context);
    return 0;
  }
  c->gl_frames++;
  struct gl_kept *found = NULL, *spare = NULL;
  for (int i = 0; i < GL_KEPT; i++) {
    struct gl_kept *kept = &c->kept[i];
    if (kept->held && kept->device == buffer.st_dev && kept->inode == buffer.st_ino &&
        kept->offset == offset && kept->stride == stride && kept->modifier == modifier &&
        kept->width == width && kept->height == height) {
      found = kept;
      continue;
    }
    /* Buffers the capture no longer hands over are let go of here, with their
     * imports and the memory those keep. */
    if (kept->held && c->gl_frames - kept->seen > GL_KEPT_FRAMES) let_go_of_kept(c, kept);
    if (!kept->held) {
      if (spare == NULL) spare = kept;
    } else if (spare == NULL || (spare->held && kept->seen < spare->seen)) {
      spare = kept;
    }
  }
  int ok = 0;
  if (found != NULL) {
    found->seen = c->gl_frames;
    ok = convert_kept(c, found);
    /* A registration that stopped mapping is made again, once, from the
     * picture as it is now. */
    if (!ok) let_go_of_kept(c, found);
  }
  if (!ok) {
    struct gl_kept *kept = found != NULL ? found : spare;
    let_go_of_kept(c, kept);
    if (keep_picture(c, kept, fd, offset, stride, modifier, width, height)) {
      kept->device = buffer.st_dev;
      kept->inode = buffer.st_ino;
      kept->offset = offset;
      kept->stride = stride;
      kept->modifier = modifier;
      kept->width = width;
      kept->height = height;
      kept->seen = c->gl_frames;
      ok = convert_kept(c, kept);
      if (!ok) let_go_of_kept(c, kept);
    }
  }
  if (ok) {
    c->source_width = width;
    c->source_height = height;
  }
  /* Back to the import's own display, which `leave` lets go of. */
  c->egl.eglMakeCurrent(c->gl_display, EGL_NO_SURFACE, EGL_NO_SURFACE, EGL_NO_CONTEXT);
  c->egl.eglMakeCurrent(c->display, EGL_NO_SURFACE, EGL_NO_SURFACE, c->context);
  return ok;
}

int yapfast_cudaconvert_convert(struct yapfast_cudaconvert *c, int fd, unsigned offset,
                                unsigned stride, unsigned long long modifier,
                                unsigned fourcc, unsigned width, unsigned height,
                                char *error, size_t size) {
  if (c == NULL) {
    put(error, size, "no converter");
    return 0;
  }
  if (fourcc != DRM_FORMAT_ARGB8888 && fourcc != DRM_FORMAT_XRGB8888) {
    put(c->error, sizeof(c->error), "a picture of format 0x%08x, which this convert doesn't read",
        fourcc);
    put(error, size, "%s", c->error);
    return 0;
  }
  if (width == 0 || height == 0 || stride < width * 4) {
    put(c->error, sizeof(c->error), "a picture of %ux%u at %u bytes a row", width, height, stride);
    put(error, size, "%s", c->error);
    return 0;
  }
  if (!enter(c)) {
    put(error, size, "%s", c->error);
    return 0;
  }
  if (c->gl_only && c->gl_only_modifier == modifier) {
    int ok = take_through_gl(c, fd, offset, stride, fourcc, modifier, width, height);
    if (!ok) put(c->error, sizeof(c->error), "through GL's texture: %s", c->gl_why);
    leave(c);
    if (!ok) put(error, size, "%s", c->error);
    return ok;
  }
  /* The descriptor's own memory, as an EGLImage: with the modifier, since a
   * compositor draws tiled buffers and the driver has to be told which layout
   * the bytes are in. */
  const EGLint attribs[] = {
      EGL_WIDTH,                          (EGLint)width,
      EGL_HEIGHT,                         (EGLint)height,
      EGL_LINUX_DRM_FOURCC_EXT,           (EGLint)fourcc,
      EGL_DMA_BUF_PLANE0_FD_EXT,          fd,
      EGL_DMA_BUF_PLANE0_OFFSET_EXT,      (EGLint)offset,
      EGL_DMA_BUF_PLANE0_PITCH_EXT,       (EGLint)stride,
      EGL_DMA_BUF_PLANE0_MODIFIER_LO_EXT, (EGLint)(modifier & 0xffffffffull),
      EGL_DMA_BUF_PLANE0_MODIFIER_HI_EXT, (EGLint)(modifier >> 32),
      EGL_NONE,
  };
  EGLImageKHR image = c->egl.eglCreateImage(c->display, EGL_NO_CONTEXT, EGL_LINUX_DMA_BUF_EXT,
                                            NULL, attribs);
  CUgraphicsResource resource = NULL;
  int ok = 0, registered = 0, mapped = 0;
  if (image == EGL_NO_IMAGE_KHR) {
    put(c->error, sizeof(c->error), "EGL wouldn't import the picture (0x%x)",
        c->egl.eglGetError());
    goto done;
  }
  if (c->cuda.cuGraphicsEGLRegisterImage(&resource, image, CU_GRAPHICS_REGISTER_FLAGS_READ_ONLY) !=
      CUDA_SUCCESS) {
    put(c->error, sizeof(c->error), "CUDA wouldn't take the imported picture");
    goto done;
  }
  registered = 1;
  /* The driver gives a picture back without the mapping being made: NVIDIA's
   * 620 driver answers CUDA_ERROR_UNKNOWN here for every imported DMA-BUF, and
   * hands the frame over anyway, whose rows read correctly. So the frame decides
   * whether the picture can be used, and only a mapping that was made is unmade. */
  mapped = c->cuda.cuGraphicsMapResources(1, &resource, NULL) == CUDA_SUCCESS;
  CUeglFrame frame;
  memset(&frame, 0, sizeof(frame));
  if (c->cuda.cuGraphicsResourceGetMappedEglFrame(&frame, resource, 0, 0) != CUDA_SUCCESS) {
    put(c->error, sizeof(c->error), "CUDA wouldn't say where the imported picture lies%s",
        mapped ? "" : ", and wouldn't map it");
  } else if (frame.frameType == CU_EGL_FRAME_TYPE_PITCH && frame.frame.pDevPtr[0] != 0) {
    if (frame.pitch < stride) {
      /* The driver imported the picture at a narrower pitch than the descriptor's
       * own: the kernels read rows at the pitch they are given, so this one would
       * read past every row's real extent. */
      put(c->error, sizeof(c->error),
          "a picture the driver holds at %u bytes a row where its descriptor says %u",
          frame.pitch, stride);
    } else {
      c->source_width = width;
      c->source_height = height;
      ok = launch(c, (const void *)(uintptr_t)frame.frame.pDevPtr[0], frame.pitch, (int)width,
                  (int)height, 0, 0);
    }
  } else if (frame.frameType == CU_EGL_FRAME_TYPE_ARRAY && frame.frame.pArray[0] != NULL) {
    /* A picture the driver holds as an array rather than rows: its rows are
     * copied on the card, and the kernels read those. Nothing crosses the
     * processor. */
    if (!mapped) {
      put(c->error, sizeof(c->error),
          "a picture the driver holds as a tiled array and wouldn't map");
    } else if (copy_rows(c, frame.frame.pArray[0], width, height)) {
      c->source_width = width;
      c->source_height = height;
      ok = launch(c, (const void *)(uintptr_t)c->rows, (unsigned)c->rows_pitch, (int)width,
                  (int)height, 0, 0);
    }
  } else {
    put(c->error, sizeof(c->error),
        "a picture the driver holds in neither rows nor an array, so nothing here can read it");
  }
done:
  if (mapped) c->cuda.cuGraphicsUnmapResources(1, &resource, NULL);
  if (registered) c->cuda.cuGraphicsUnregisterResource(resource);
  if (image != EGL_NO_IMAGE_KHR) c->egl.eglDestroyImage(c->display, image);
  if (!ok) {
    /* The import's own resource didn't give the picture, which is what NVIDIA's
     * driver does with a tiled one: it answers an error for the mapping and then
     * hands back an array nothing can read. GL's texture over the same picture is
     * the door that takes it: GL copies it into a texture CUDA maps. Whatever the
     * second door says is added to what the first said. */
    char first[sizeof(c->error)];
    put(first, sizeof(first), "%s", c->error);
    ok = take_through_gl(c, fd, offset, stride, fourcc, modifier, width, height);
    if (ok) {
      c->gl_only = 1;
      c->gl_only_modifier = modifier;
    } else if (c->gl_why[0] != '\0') {
      put(c->error, sizeof(c->error), "%s; and through GL's texture: %s", first, c->gl_why);
    }
  }
  leave(c);
  if (!ok) put(error, size, "%s", c->error);
  return ok;
}

/* Copies the picture the last convert wrote, at the stream's size, back into
 * the processor's memory as I420: Discord's still is made from it every few
 * minutes, and a tiled picture has no rows the processor could read it from
 * instead. The luma rows go straight into `y`; the chroma, interleaved on the
 * card, is copied once into memory here and split into `u` and `v`. Returns 1,
 * or 0 with the reason in `yapfast_cudaconvert_error`. */
int yapfast_cudaconvert_still(struct yapfast_cudaconvert *c, unsigned char *y, unsigned y_stride,
                              unsigned char *u, unsigned u_stride, unsigned char *v,
                              unsigned v_stride) {
  if (c == NULL || c->frames == 0) return 0;
  if (c->cuda.cuMemcpy3D == NULL) {
    put(c->error, sizeof(c->error), "the CUDA driver can't copy the picture back");
    return 0;
  }
  const unsigned width = c->width, height = c->height;
  const unsigned chroma_width = width / 2, chroma_height = height / 2;
  unsigned char *interleaved = malloc((size_t)chroma_width * 2 * chroma_height);
  if (interleaved == NULL) {
    put(c->error, sizeof(c->error), "no memory for the picture's colour");
    return 0;
  }
  if (!enter(c)) {
    free(interleaved);
    return 0;
  }
  cuda_memcpy3d copy;
  memset(&copy, 0, sizeof(copy));
  copy.srcMemoryType = CU_MEMORYTYPE_DEVICE;
  copy.srcDevice = c->nv12;
  copy.srcPitch = c->pitch;
  copy.dstMemoryType = CU_MEMORYTYPE_HOST;
  copy.dstHost = y;
  copy.dstPitch = y_stride;
  copy.WidthInBytes = width;
  copy.Height = height;
  copy.Depth = 1;
  CUresult luma = c->cuda.cuMemcpy3D(&copy);
  copy.srcDevice = c->nv12 + (CUdeviceptr)c->pitch * height;
  copy.dstHost = interleaved;
  copy.dstPitch = chroma_width * 2;
  copy.WidthInBytes = chroma_width * 2;
  copy.Height = chroma_height;
  CUresult chroma = luma == CUDA_SUCCESS ? c->cuda.cuMemcpy3D(&copy) : luma;
  leave(c);
  if (chroma != CUDA_SUCCESS) {
    put(c->error, sizeof(c->error), "the card wouldn't copy the picture back: %s",
        cuda_name(c, chroma));
    free(interleaved);
    return 0;
  }
  for (unsigned row = 0; row < chroma_height; row++) {
    const unsigned char *from = interleaved + (size_t)row * chroma_width * 2;
    unsigned char *to_u = u + (size_t)row * u_stride, *to_v = v + (size_t)row * v_stride;
    for (unsigned x = 0; x < chroma_width; x++) {
      to_u[x] = from[2 * x];
      to_v[x] = from[2 * x + 1];
    }
  }
  free(interleaved);
  return 1;
}

void yapfast_cudaconvert_free(struct yapfast_cudaconvert *c) {
  if (c == NULL) return;
  /* The kept pictures first, while both contexts they were made in still are:
   * GL's textures and the imports that hold the capture's buffers, and the
   * texture CUDA reads with its registration. */
  if (c->gl_open > 0 && c->cuda_context != NULL &&
      c->cuda.cuCtxPushCurrent(c->cuda_context) == CUDA_SUCCESS) {
    if (c->egl.eglMakeCurrent(c->gl_display, EGL_NO_SURFACE, EGL_NO_SURFACE, c->gl_context)) {
      for (int i = 0; i < GL_KEPT; i++) let_go_of_kept(c, &c->kept[i]);
      let_go_of_copy(c);
      c->egl.eglMakeCurrent(c->gl_display, EGL_NO_SURFACE, EGL_NO_SURFACE, EGL_NO_CONTEXT);
    }
    c->cuda.cuCtxPopCurrent(NULL);
  }
  if (c->cuda_context != NULL && c->cuda.cuCtxDestroy != NULL) {
    /* The block belongs to the context that made it, so it is let go of inside
     * that context whether or not this thread had it current. */
    c->cuda.cuCtxPushCurrent(c->cuda_context);
    if (c->nv12 != 0 && c->cuda.cuMemFree != NULL) c->cuda.cuMemFree(c->nv12);
    if (c->rows != 0 && c->cuda.cuMemFree != NULL) c->cuda.cuMemFree(c->rows);
    c->cuda.cuCtxPopCurrent(NULL);
    c->cuda.cuCtxDestroy(c->cuda_context);
  }
  if (c->display != EGL_NO_DISPLAY) {
    c->egl.eglMakeCurrent(c->display, EGL_NO_SURFACE, EGL_NO_SURFACE, EGL_NO_CONTEXT);
    if (c->context != EGL_NO_CONTEXT) c->egl.eglDestroyContext(c->display, c->context);
    display_let_go(c, c->display);
  }
  if (c->gl_display != EGL_NO_DISPLAY) {
    if (c->gl_context != EGL_NO_CONTEXT) c->egl.eglDestroyContext(c->gl_display, c->gl_context);
    display_let_go(c, c->gl_display);
  }
  if (c->gbm_device != NULL && c->gbm != NULL) {
    void (*destroy)(void *) = symbol(c->gbm, "gbm_device_destroy");
    if (destroy != NULL) destroy(c->gbm_device);
  }
  if (c->gbm != NULL) dlclose(c->gbm);
  if (c->gl.library != NULL) dlclose(c->gl.library);
  if (c->drm >= 0) close(c->drm);
  if (c->egl.library != NULL) dlclose(c->egl.library);
  if (c->cuda.library != NULL) dlclose(c->cuda.library);
  free(c);
}
