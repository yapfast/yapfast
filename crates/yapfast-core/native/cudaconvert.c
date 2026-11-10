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
 * What it will not take, it says and refuses: a picture the driver hands back as
 * a CUDA array rather than a pitch pointer (a tiled buffer it imported as an
 * array), a format that isn't 32-bit BGRA/BGRX, a device that isn't the one the
 * descriptor came from. A refusal is a sentence the share's ladder falls through
 * on, to the same processor copy every other path keeps.
 *
 * Nothing here needs a CUDA toolkit at build time or on the machine: the PTX is
 * clang's output (see `build.rs`), the driver loads it, and libcuda is dlopened
 * only when an encoder asks for this path. */

/* readlink is POSIX's, so this file is built as GNU C (see `build.rs`) rather
 * than as strict C, which would leave it undeclared. */
#include <dlfcn.h>
#include <EGL/egl.h>
#include <EGL/eglext.h>
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
  CUresult (*cuDeviceGetName)(char *, int, CUdevice);
  CUresult (*cuCtxCreate)(CUcontext *, unsigned, CUdevice);
  CUresult (*cuCtxDestroy)(CUcontext);
  CUresult (*cuCtxPushCurrent)(CUcontext);
  CUresult (*cuCtxPopCurrent)(CUcontext *);
  CUresult (*cuCtxSynchronize)(void);
  CUresult (*cuModuleLoadData)(CUmodule *, const void *);
  CUresult (*cuModuleGetFunction)(CUfunction *, CUmodule, const char *);
  CUresult (*cuMemAlloc)(CUdeviceptr *, size_t);
  CUresult (*cuMemFree)(CUdeviceptr);
  CUresult (*cuLaunchKernel)(CUfunction, unsigned, unsigned, unsigned, unsigned,
                             unsigned, unsigned, unsigned, CUstream, void **,
                             void **);
  CUresult (*cuGetErrorName)(CUresult, const char **);
  CUresult (*cuGraphicsEGLRegisterImage)(CUgraphicsResource *, void *, unsigned);
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
    put(c->error, sizeof(c->error), "EGL wouldn't enter the import's context");
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
  LOAD("cuDeviceGetName", cuDeviceGetName);
  LOAD("cuCtxCreate_v2", cuCtxCreate);
  LOAD("cuCtxDestroy_v2", cuCtxDestroy);
  LOAD("cuCtxPushCurrent_v2", cuCtxPushCurrent);
  LOAD("cuCtxPopCurrent_v2", cuCtxPopCurrent);
  LOAD("cuCtxSynchronize", cuCtxSynchronize);
  LOAD("cuModuleLoadData", cuModuleLoadData);
  LOAD("cuModuleGetFunction", cuModuleGetFunction);
  LOAD("cuMemAlloc_v2", cuMemAlloc);
  LOAD("cuMemFree_v2", cuMemFree);
  LOAD("cuLaunchKernel", cuLaunchKernel);
  LOAD("cuGetErrorName", cuGetErrorName);
  LOAD("cuGraphicsEGLRegisterImage", cuGraphicsEGLRegisterImage);
  LOAD("cuGraphicsUnregisterResource", cuGraphicsUnregisterResource);
  LOAD("cuGraphicsMapResources", cuGraphicsMapResources);
  LOAD("cuGraphicsUnmapResources", cuGraphicsUnmapResources);
  LOAD("cuGraphicsResourceGetMappedEglFrame", cuGraphicsResourceGetMappedEglFrame);
#undef LOAD
  return 1;
}

/* The EGL device that is the descriptor's own. A buffer can only be imported by
 * the device whose memory it is, so the one matching the DRM node is looked for
 * rather than the first one offered. */
static void *egl_device_for(struct yapfast_cudaconvert *c, int drm_fd, const char *node) {
  void *devices[16];
  EGLint count = 0;
  if (c->egl.eglQueryDevices == NULL) return NULL;
  if (!c->egl.eglQueryDevices(16, devices, &count)) return NULL;
  if (count > 16) count = 16;
  struct stat ours;
  int have_ours = fstat(drm_fd, &ours) == 0;
  /* The descriptor's own render node, and the card node a driver may name for
   * the same device: NVIDIA reports `EGL_DRM_DEVICE_FILE_EXT` as
   * `/dev/dri/card0` where the picture came from `/dev/dri/renderD128`, and
   * Linux numbers the two up to 128 apart. */
  char card[64];
  const char *slash = strrchr(node, '/');
  const char *render = slash ? slash + 1 : node;
  snprintf(card, sizeof(card), "/dev/dri/card%d", render[0] ? atoi(render + 6) - 128 : -1);
  for (EGLint i = 0; i < count; i++) {
    const char *file = c->egl.eglQueryDeviceString(devices[i], EGL_DRM_DEVICE_FILE_EXT);
    if (file == NULL) continue;
    struct stat theirs;
    if (have_ours && stat(file, &theirs) == 0 && theirs.st_rdev == ours.st_rdev)
      return devices[i];
    if (!strcmp(file, node) || (atoi(render + 6) >= 128 && !strcmp(file, card)))
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
  EGLint major = 0, minor = 0;
  if (!c->egl.eglInitialize(c->display, &major, &minor)) {
    put(c->error, sizeof(c->error), "EGL wouldn't initialize %s", node);
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

/* Which CUDA device that EGL device is: the driver answers its ordinal as a
 * string, so no guess about their order is made. */
static int cuda_device_for(struct yapfast_cudaconvert *c, void *egl_device) {
  const char *named = c->egl.eglQueryDeviceString(egl_device, EGL_CUDA_DEVICE_NV);
  int ordinal = 0;
  if (named == NULL || sscanf(named, "CUDA-%d", &ordinal) != 1) {
    put(c->error, sizeof(c->error), "EGL device %p names no CUDA device", egl_device);
    return 0;
  }
  CU(c, cuDeviceGet(&c->device, ordinal), "no such CUDA device");
  return 1;
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
  /* The node the descriptor came from, by the path the kernel gave the driver:
   * EGL names its devices the same way. */
  char link[64], node[256];
  snprintf(link, sizeof(link), "/proc/self/fd/%d", drm_fd);
  ssize_t length = readlink(link, node, sizeof(node) - 1);
  if (length <= 0) {
    put(error, size, "the descriptor's device has no name");
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
  if (!open_egl(c, drm_fd, node, &egl_device) || !cuda_device_for(c, egl_device)) {
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
  CU(c, cuModuleLoadData(&c->module, ptx), "the convert kernels wouldn't load");
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
  c->nv12_size = (size_t)c->pitch * c->height * 3 / 2;
  CU(c, cuMemAlloc(&c->nv12, c->nv12_size), "no room on the card for the picture");
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
 * the caller hands that buffer to its encoder. */
static int launch(struct yapfast_cudaconvert *c, const void *pixels, unsigned pitch,
                  int width, int height, const char *node) {
  unsigned char *frames = (unsigned char *)(uintptr_t)c->nv12;
  struct {
    const unsigned char *pixels;
    unsigned pitch;
    int width, height;
  } source = {pixels, pitch, width, height};
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
  (void)node;
  c->frames++;
  return 1;
}

/* One captured picture: `fd`, `offset`, `stride` and `modifier` describe the
 * memory the compositor drew, `width` by `height` of BGRA or BGRX pixels in it.
 * The picture lands in the NV12 buffer this was opened with, and needs nothing
 * of the processor. */
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
  if (image == EGL_NO_IMAGE_KHR) {
    put(c->error, sizeof(c->error), "EGL wouldn't import the picture (0x%x)",
        c->egl.eglGetError());
    put(error, size, "%s", c->error);
    leave(c);
    return 0;
  }
  CUgraphicsResource resource = NULL;
  int ok = 0, registered = 0, mapped = 0;
  if (c->cuda.cuGraphicsEGLRegisterImage(&resource, image, CU_GRAPHICS_REGISTER_FLAGS_READ_ONLY) !=
      CUDA_SUCCESS) {
    put(c->error, sizeof(c->error), "CUDA wouldn't take the imported picture");
    goto done;
  }
  registered = 1;
  if (c->cuda.cuGraphicsMapResources(1, &resource, NULL) != CUDA_SUCCESS) {
    put(c->error, sizeof(c->error), "CUDA wouldn't map the imported picture");
    goto done;
  }
  mapped = 1;
  CUeglFrame frame;
  memset(&frame, 0, sizeof(frame));
  if (c->cuda.cuGraphicsResourceGetMappedEglFrame(&frame, resource, 0, 0) != CUDA_SUCCESS) {
    put(c->error, sizeof(c->error), "CUDA wouldn't say where the imported picture lies");
  } else if (frame.frameType != CU_EGL_FRAME_TYPE_PITCH || frame.frame.pDevPtr[0] == 0) {
    /* A tiled buffer the driver imported as an array: the kernels read rows, and
     * refuse rather than read them wrongly. The share falls to the next rung. */
    put(c->error, sizeof(c->error),
        "a picture the driver holds as an array rather than rows (a tiled buffer)");
  } else {
    c->source_width = width;
    c->source_height = height;
    ok = launch(c, (const void *)(uintptr_t)frame.frame.pDevPtr[0], frame.pitch, (int)width,
                (int)height, NULL);
  }
done:
  if (mapped) c->cuda.cuGraphicsUnmapResources(1, &resource, NULL);
  if (registered) c->cuda.cuGraphicsUnregisterResource(resource);
  c->egl.eglDestroyImage(c->display, image);
  leave(c);
  if (!ok) put(error, size, "%s", c->error);
  return ok;
}

void yapfast_cudaconvert_free(struct yapfast_cudaconvert *c) {
  if (c == NULL) return;
  if (c->cuda_context != NULL && c->cuda.cuCtxDestroy != NULL) {
    if (c->nv12 != 0 && c->cuda.cuMemFree != NULL) c->cuda.cuMemFree(c->nv12);
    c->cuda.cuCtxDestroy(c->cuda_context);
  }
  if (c->display != EGL_NO_DISPLAY) {
    c->egl.eglMakeCurrent(c->display, EGL_NO_SURFACE, EGL_NO_SURFACE, EGL_NO_CONTEXT);
    if (c->context != EGL_NO_CONTEXT) c->egl.eglDestroyContext(c->display, c->context);
    c->egl.eglTerminate(c->display);
  }
  if (c->egl.library != NULL) dlclose(c->egl.library);
  if (c->cuda.library != NULL) dlclose(c->cuda.library);
  free(c);
}
