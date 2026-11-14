/* libvpl (oneVPL) for FFmpeg's QSV encoders, loaded on first use, so the
 * binary starts without it and a QSV encoder fails to open where it's missing.
 * gen/rename.h has FFmpeg call these as yf_MFX...; each looks up oneVPL's own
 * function, in libvpl.so.2 or vpl.dll, and calls it; the dispatcher in turn
 * loads the driver that runs the encoder. A lookup is a dlsym, which costs
 * nothing next to a frame's encoding. */
#ifdef _WIN32
#include <windows.h>
#else
#include <dlfcn.h>
#include <pthread.h>
#endif
#include <stddef.h>

#include <vpl/mfxdispatcher.h>
#include <vpl/mfxvideo.h>

#ifdef _WIN32
typedef HMODULE library;
#define OPEN(name) LoadLibraryA(name)
#define FIND(library, name) ((void *)GetProcAddress(library, name))
#define ONCE(callback) InitOnceExecuteOnce(&once, callback, NULL, NULL)
#else
typedef void *library;
#define OPEN(name) dlopen(name, RTLD_NOW | RTLD_LOCAL)
#define FIND(library, name) dlsym(library, name)
#define ONCE(callback) pthread_once(&once, callback)
#endif

static library vpl;
#ifdef _WIN32
static INIT_ONCE once = INIT_ONCE_STATIC_INIT;

static BOOL CALLBACK load(PINIT_ONCE initiate, PVOID parameter, PVOID *context) {
  (void)initiate;
  (void)parameter;
  (void)context;
  /* Intel's driver installs libvpl.dll; oneVPL's own build names it vpl.dll. */
  vpl = OPEN("vpl.dll");
  if (!vpl) vpl = OPEN("libvpl.dll");
  return TRUE;
}
#else
static pthread_once_t once = PTHREAD_ONCE_INIT;

static void load(void) { vpl = OPEN("libvpl.so.2"); }
#endif

static void *find(const char *name) {
  ONCE(load);
  return vpl ? FIND(vpl, name) : NULL;
}

/* yf_NAME, which calls oneVPL's NAME, or returns `missing` without oneVPL. */
#define FORWARD(type, name, params, args, missing)       \
  type yf_##name params {                                \
    type(*real) params = (type(*) params)find(#name);     \
    return real ? real args : missing;                   \
  }

#define FORWARD_VOID(name, params, args)                  \
  void yf_##name params {                                 \
    void (*real) params = (void (*)params)find(#name);    \
    if (real) real args;                                  \
  }

#define FAILED ((mfxStatus)MFX_ERR_NOT_FOUND)

/* The dispatcher: what FFmpeg asks for a session, and it loads the driver. */
FORWARD(mfxLoader, MFXLoad, (void), (), NULL)
FORWARD_VOID(MFXUnload, (mfxLoader loader), (loader))
FORWARD(mfxConfig, MFXCreateConfig, (mfxLoader loader), (loader), NULL)
FORWARD(mfxStatus, MFXSetConfigFilterProperty,
        (mfxConfig config, const mfxU8 *name, mfxVariant value), (config, name, value), FAILED)
FORWARD(mfxStatus, MFXEnumImplementations,
        (mfxLoader loader, mfxU32 i, mfxImplCapsDeliveryFormat format, mfxHDL *idesc),
        (loader, i, format, idesc), FAILED)
FORWARD(mfxStatus, MFXDispReleaseImplDescription, (mfxLoader loader, mfxHDL hdl), (loader, hdl),
        FAILED)
FORWARD(mfxStatus, MFXCreateSession, (mfxLoader loader, mfxU32 i, mfxSession *session),
        (loader, i, session), FAILED)

/* The session, which the dispatcher made out of one of those drivers. */
FORWARD(mfxStatus, MFXClose, (mfxSession session), (session), FAILED)
FORWARD(mfxStatus, MFXJoinSession, (mfxSession session, mfxSession child), (session, child),
        FAILED)
FORWARD(mfxStatus, MFXQueryVersion, (mfxSession session, mfxVersion *version),
        (session, version), FAILED)
FORWARD(mfxStatus, MFXQueryIMPL, (mfxSession session, mfxIMPL *impl), (session, impl), FAILED)

/* The device the session encodes on, which on Windows is the Direct3D 11 one
 * the window and the decoder already hold. */
FORWARD(mfxStatus, MFXVideoCORE_SetHandle, (mfxSession session, mfxHandleType type, mfxHDL hdl),
        (session, type, hdl), FAILED)
FORWARD(mfxStatus, MFXVideoCORE_GetHandle, (mfxSession session, mfxHandleType type, mfxHDL *hdl),
        (session, type, hdl), FAILED)
FORWARD(mfxStatus, MFXVideoCORE_SetFrameAllocator,
        (mfxSession session, mfxFrameAllocator *allocator), (session, allocator), FAILED)
FORWARD(mfxStatus, MFXVideoCORE_SyncOperation,
        (mfxSession session, mfxSyncPoint syncp, mfxU32 wait), (session, syncp, wait), FAILED)

FORWARD(mfxStatus, MFXVideoENCODE_Query,
        (mfxSession session, mfxVideoParam *in, mfxVideoParam *out), (session, in, out), FAILED)
FORWARD(mfxStatus, MFXVideoENCODE_QueryIOSurf,
        (mfxSession session, mfxVideoParam *par, mfxFrameAllocRequest *request),
        (session, par, request), FAILED)
FORWARD(mfxStatus, MFXVideoENCODE_Init, (mfxSession session, mfxVideoParam *par), (session, par),
        FAILED)
FORWARD(mfxStatus, MFXVideoENCODE_Reset, (mfxSession session, mfxVideoParam *par), (session, par),
        FAILED)
FORWARD(mfxStatus, MFXVideoENCODE_Close, (mfxSession session), (session), FAILED)
FORWARD(mfxStatus, MFXVideoENCODE_GetVideoParam, (mfxSession session, mfxVideoParam *par),
        (session, par), FAILED)
FORWARD(mfxStatus, MFXVideoENCODE_EncodeFrameAsync,
        (mfxSession session, mfxEncodeCtrl *ctrl, mfxFrameSurface1 *surface, mfxBitstream *bs,
         mfxSyncPoint *syncp),
        (session, ctrl, surface, bs, syncp), FAILED)

/* hwcontext_qsv copies between the device's frames with VPP. */
FORWARD(mfxStatus, MFXVideoVPP_Init, (mfxSession session, mfxVideoParam *par), (session, par),
        FAILED)
FORWARD(mfxStatus, MFXVideoVPP_Query, (mfxSession session, mfxVideoParam *in, mfxVideoParam *out),
        (session, in, out), FAILED)
FORWARD(mfxStatus, MFXVideoVPP_Reset, (mfxSession session, mfxVideoParam *par), (session, par),
        FAILED)
FORWARD(mfxStatus, MFXVideoVPP_RunFrameVPPAsync,
        (mfxSession session, mfxFrameSurface1 *in, mfxFrameSurface1 *out, mfxExtVppAuxData *aux,
         mfxSyncPoint *syncp),
        (session, in, out, aux, syncp), FAILED)
FORWARD(mfxStatus, MFXVideoVPP_Close, (mfxSession session), (session), FAILED)
