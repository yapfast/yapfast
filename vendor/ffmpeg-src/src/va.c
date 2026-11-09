/* libva for FFmpeg's VA-API encoders, loaded on first use, so the binary
 * starts without it and a VA-API encoder fails to open where it's missing.
 * gen/rename.h has FFmpeg call these as yf_va...; each looks up libva's own
 * function, in libva.so.2 or libva-drm.so.2, and calls it. A lookup is a
 * dlsym, which costs nothing next to a frame's encoding. */
#include <dlfcn.h>
#include <pthread.h>
#include <stddef.h>

#include <va/va.h>
#include <va/va_drm.h>
#include <va/va_str.h>

static void *libva, *libva_drm;
static pthread_once_t loaded = PTHREAD_ONCE_INIT;

static void load(void) {
  libva = dlopen("libva.so.2", RTLD_NOW | RTLD_LOCAL);
  if (libva) libva_drm = dlopen("libva-drm.so.2", RTLD_NOW | RTLD_LOCAL);
}

static void *find(const char *name) {
  pthread_once(&loaded, load);
  void *found = libva ? dlsym(libva, name) : NULL;
  if (!found && libva_drm) found = dlsym(libva_drm, name);
  return found;
}

/* yf_NAME, which calls libva's NAME, or returns `missing` without libva. */
#define FORWARD(type, name, params, args, missing)       \
  type yf_##name params {                                 \
    type(*real) params = (type(*) params)find(#name);     \
    return real ? real args : missing;                    \
  }

#define FAILED ((VAStatus)VA_STATUS_ERROR_UNKNOWN)

FORWARD(VAStatus, vaBeginPicture, (VADisplay dpy, VAContextID context, VASurfaceID target),
        (dpy, context, target), FAILED)
FORWARD(VAStatus, vaCreateBuffer,
        (VADisplay dpy, VAContextID context, VABufferType type, unsigned int size,
         unsigned int count, void *data, VABufferID *buffer),
        (dpy, context, type, size, count, data, buffer), FAILED)
FORWARD(VAStatus, vaCreateConfig,
        (VADisplay dpy, VAProfile profile, VAEntrypoint entrypoint, VAConfigAttrib *attribs,
         int count, VAConfigID *config),
        (dpy, profile, entrypoint, attribs, count, config), FAILED)
FORWARD(VAStatus, vaCreateContext,
        (VADisplay dpy, VAConfigID config, int width, int height, int flag,
         VASurfaceID *targets, int count, VAContextID *context),
        (dpy, config, width, height, flag, targets, count, context), FAILED)
FORWARD(VAStatus, vaCreateImage,
        (VADisplay dpy, VAImageFormat *format, int width, int height, VAImage *image),
        (dpy, format, width, height, image), FAILED)
FORWARD(VAStatus, vaCreateSurfaces,
        (VADisplay dpy, unsigned int format, unsigned int width, unsigned int height,
         VASurfaceID *surfaces, unsigned int count, VASurfaceAttrib *attribs,
         unsigned int attrib_count),
        (dpy, format, width, height, surfaces, count, attribs, attrib_count), FAILED)
FORWARD(VAStatus, vaDeriveImage, (VADisplay dpy, VASurfaceID surface, VAImage *image),
        (dpy, surface, image), FAILED)
FORWARD(VAStatus, vaDestroyBuffer, (VADisplay dpy, VABufferID buffer), (dpy, buffer), FAILED)
FORWARD(VAStatus, vaDestroyConfig, (VADisplay dpy, VAConfigID config), (dpy, config), FAILED)
FORWARD(VAStatus, vaDestroyContext, (VADisplay dpy, VAContextID context), (dpy, context),
        FAILED)
FORWARD(VAStatus, vaDestroyImage, (VADisplay dpy, VAImageID image), (dpy, image), FAILED)
FORWARD(VAStatus, vaDestroySurfaces, (VADisplay dpy, VASurfaceID *surfaces, int count),
        (dpy, surfaces, count), FAILED)
FORWARD(VAStatus, vaEndPicture, (VADisplay dpy, VAContextID context), (dpy, context), FAILED)
FORWARD(const char *, vaEntrypointStr, (VAEntrypoint entrypoint), (entrypoint), "unknown")
FORWARD(const char *, vaErrorStr, (VAStatus status), (status), "libva is missing")
FORWARD(VAStatus, vaGetConfigAttributes,
        (VADisplay dpy, VAProfile profile, VAEntrypoint entrypoint, VAConfigAttrib *attribs,
         int count),
        (dpy, profile, entrypoint, attribs, count), FAILED)
/* QSV asks the display which Intel device it is, to pick the driver. */
FORWARD(VAStatus, vaGetDisplayAttributes,
        (VADisplay dpy, VADisplayAttribute *attr_list, int num_attributes),
        (dpy, attr_list, num_attributes), FAILED)
FORWARD(VADisplay, vaGetDisplayDRM, (int fd), (fd), NULL)
FORWARD(VAStatus, vaGetImage,
        (VADisplay dpy, VASurfaceID surface, int x, int y, unsigned int width,
         unsigned int height, VAImageID image),
        (dpy, surface, x, y, width, height, image), FAILED)
FORWARD(VAStatus, vaInitialize, (VADisplay dpy, int *major, int *minor), (dpy, major, minor),
        FAILED)
FORWARD(VAStatus, vaMapBuffer, (VADisplay dpy, VABufferID buffer, void **mapped),
        (dpy, buffer, mapped), FAILED)
FORWARD(VAStatus, vaMapBuffer2,
        (VADisplay dpy, VABufferID buffer, void **mapped, uint32_t flags),
        (dpy, buffer, mapped, flags), FAILED)
FORWARD(int, vaMaxNumEntrypoints, (VADisplay dpy), (dpy), 0)
FORWARD(int, vaMaxNumImageFormats, (VADisplay dpy), (dpy), 0)
FORWARD(int, vaMaxNumProfiles, (VADisplay dpy), (dpy), 0)
FORWARD(const char *, vaProfileStr, (VAProfile profile), (profile), "unknown")
FORWARD(VAStatus, vaPutImage,
        (VADisplay dpy, VASurfaceID surface, VAImageID image, int src_x, int src_y,
         unsigned int src_width, unsigned int src_height, int dest_x, int dest_y,
         unsigned int dest_width, unsigned int dest_height),
        (dpy, surface, image, src_x, src_y, src_width, src_height, dest_x, dest_y, dest_width,
         dest_height),
        FAILED)
FORWARD(VAStatus, vaQueryConfigEntrypoints,
        (VADisplay dpy, VAProfile profile, VAEntrypoint *entrypoints, int *count),
        (dpy, profile, entrypoints, count), FAILED)
FORWARD(VAStatus, vaQueryConfigProfiles, (VADisplay dpy, VAProfile *profiles, int *count),
        (dpy, profiles, count), FAILED)
FORWARD(VAStatus, vaQueryImageFormats, (VADisplay dpy, VAImageFormat *formats, int *count),
        (dpy, formats, count), FAILED)
FORWARD(VAStatus, vaQuerySurfaceAttributes,
        (VADisplay dpy, VAConfigID config, VASurfaceAttrib *attribs, unsigned int *count),
        (dpy, config, attribs, count), FAILED)
FORWARD(const char *, vaQueryVendorString, (VADisplay dpy), (dpy), "")
FORWARD(VAStatus, vaRenderPicture,
        (VADisplay dpy, VAContextID context, VABufferID *buffers, int count),
        (dpy, context, buffers, count), FAILED)
FORWARD(VAStatus, vaSetDriverName, (VADisplay dpy, char *driver), (dpy, driver), FAILED)
FORWARD(VAMessageCallback, vaSetErrorCallback,
        (VADisplay dpy, VAMessageCallback callback, void *context), (dpy, callback, context),
        NULL)
FORWARD(VAMessageCallback, vaSetInfoCallback,
        (VADisplay dpy, VAMessageCallback callback, void *context), (dpy, callback, context),
        NULL)
FORWARD(VAStatus, vaSyncBuffer, (VADisplay dpy, VABufferID buffer, uint64_t timeout),
        (dpy, buffer, timeout), FAILED)
FORWARD(VAStatus, vaSyncSurface, (VADisplay dpy, VASurfaceID target), (dpy, target), FAILED)
FORWARD(VAStatus, vaTerminate, (VADisplay dpy), (dpy), FAILED)
FORWARD(VAStatus, vaUnmapBuffer, (VADisplay dpy, VABufferID buffer), (dpy, buffer), FAILED)
