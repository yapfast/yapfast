/* Windows cameras through Media Foundation, each frame staying in the D3D11
 * texture it arrived in.
 *
 * native/camera.cc captures through libwebrtc's module, which hands over I420 in
 * system memory: every frame is converted, then uploaded before an encoder reads
 * it. This is the camera's end of the path native/wgc.cc is the screen's: an
 * IMFSourceReader on a D3D11 device (`MF_SOURCE_READER_D3D_MANAGER`) has the
 * device's NV12 frames written into textures that device can read, and each frame
 * is handed on as the ID3D11Texture2D it lies in, with the subresource index that
 * names the plane of it. Nothing here reads a pixel.
 *
 * Nothing builds it. build.rs compiles native/camera.cc for Windows, and no
 * target, feature or environment variable reaches this file. The Rust side it
 * would be linked into doesn't exist, and pointing a build at it before that
 * side does reports no failure: it hangs a call. The picture the caller reads
 * (`RawPicture` in src/picture.rs) is the eight fields below and nothing more,
 * so it finds no texture: the planes are null and the strides zero,
 * `I420::from_raw` answers None for every frame, the frames are dropped,
 * `yapfast_camera_open` answers a camera, and `Camera::start_filming` waits for
 * a first picture for as long as the camera stays open.
 *
 * What it takes to build it: that picture type (or a picture of its own, and
 * the callback that carries it) declaring the texture, the subresource and the
 * device, and an encoder on the camera's device with a sink that reads a
 * texture rather than I420.
 *
 * It has also never been built against the Windows SDK or run. It was written on
 * Linux, where no Windows SDK is; its C++ was type-checked there against stand-ins
 * for the headers it includes, which says nothing about what Media Foundation does
 * with it. Build it on a machine with a camera, exercise it, and expect to fix what
 * that finds.
 *
 * The exports are camera.cc's, so a caller can be pointed at either:
 * `yapfast_camera_list`, `yapfast_camera_open` and `yapfast_camera_close`, with
 * the same parameters and the same `yapfast_picture` callback. What differs is
 * the picture. Its first fields are camera.cc's layout, field for field, so a
 * caller that reads them reads a frame's width and height and finds no planes;
 * after them come the texture, its subresource and the device it lies on. A caller
 * that wants these frames declares those three fields and gives the texture to an
 * encoder made on that device.
 *
 * A camera here owns its D3D11 device: it makes one, and a frame arrives in that
 * device's memory. `yapfast_camera_open` takes no device to capture on the way
 * `yapfast_wgc_open_on` does, because camera.cc's signature has none and this file
 * keeps the shape it can be swapped into rather than adding a parameter the other
 * one's caller doesn't pass.
 *
 * Ownership: a frame's texture belongs to the source reader's pool and is
 * *borrowed*. It stays valid until the callback returns and not a moment longer,
 * which is why the sample it came in is held across the callback; a caller that
 * has to keep it past that AddRefs it itself. The device belongs to the camera and
 * is valid until `yapfast_camera_close` returns.
 *
 * An id is the device's symbolic link, which is what `yapfast_camera_list` hands
 * out, and a camera is opened by finding the device with that link. camera.cc's
 * list hands out DirectShow's device paths, which are not promised to be the same
 * string, so an id from one list is not an id for the other.
 *
 * Every failure is a sentence, reachable through `yapfast_camera_mf_error`, and
 * none of them is a crash: `yapfast_camera_open` answers NULL with the reason kept
 * for that call, and a capture that ends says what ended it.
 *
 * What it deliberately doesn't do: no I420 conversion, no scaling down to the size
 * asked for (an encoder scales a texture as it encodes it), and no rate limiting
 * beyond the frame rate put on the media type. The size a frame arrives at is the
 * size its picture reports.
 */
#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <windows.h>

#include <d3d11.h>
#include <mfapi.h>
#include <mferror.h>
#include <mfidl.h>
#include <mfreadwrite.h>

#include <atomic>
#include <condition_variable>
#include <cstdarg>
#include <cstdio>
#include <cstring>
#include <exception>
#include <mutex>
#include <new>
#include <thread>

extern "C" {

/* camera.cc's layout first, the same field for field, so that a caller reads a
 * frame's size here whichever capturer it was linked against. A picture this file
 * hands over has the planes null and the texture set; one camera.cc hands over has
 * the texture null. */
struct yapfast_picture {
  int width;
  int height;
  const unsigned char *y;
  const unsigned char *u;
  const unsigned char *v;
  int y_stride;
  int u_stride;
  int v_stride;
  /* The frame as the picture it already is, borrowed until the callback returns: */
  void *texture;        /* ID3D11Texture2D * */
  unsigned subresource; /* the slice of that texture the frame lies in */
  void *device;         /* the ID3D11Device it lies on, the camera's own */
};

typedef void (*yapfast_camera_frame)(void *user, const struct yapfast_picture *picture);

typedef void (*yapfast_camera_each)(void *user, const char *name, const char *id);

}  // extern "C"

/* ----------------------------------------------------------------- the camera */

/* Everything is made on the capture thread and let go there, in one apartment:
 * Media Foundation's objects belong to the apartment that made them, and frames
 * are pulled on the thread that hands them on. What yapfast_camera_open sets is set
 * before that thread starts; `opened` and `error` are what it then waits for and
 * reads, under `lock`, which guards `source` against yapfast_camera_close shutting
 * it down too. `stopping` is atomic because both sides set it. */
struct yapfast_camera {
  /* Whether the opening came, waited on by yapfast_camera_open: 0 while it runs,
   * 1 when frames may come, -1 when it failed with `error` set. */
  int opened = 0;
  /* The sentence that ended the capture, empty while it runs. */
  char error[512] = {0};
  /* Set by yapfast_camera_close, and by a failure that ends the loop. */
  std::atomic<bool> stopping{false};

  /* Everything below `lock` is the opening's answer and the capture's state. */
  std::mutex lock;
  std::condition_variable ready;

  /* What this camera was asked for, and, once it is open, the size its frames are
   * at, which is what a picture reports. */
  char id[1024] = {0};
  unsigned width = 0;
  unsigned height = 0;
  unsigned fps = 0;
  yapfast_camera_frame frame = NULL;
  void *user = NULL;

  /* The capture thread's, made and released by it; `source` is also shut down
   * from yapfast_camera_close, which is why the lock covers it and `shut`. */
  IMFMediaSource *source = NULL;
  int shut = 0;
  IMFSourceReader *reader = NULL;
  ID3D11Device *device = NULL;

  std::thread thread;
};

/* --------------------------------------------------------------- the failures */

/* The last failure on this thread, empty until one happens: how a yapfast_camera_open
 * that answered NULL says why. */
static thread_local char last_failure[512] = {0};

/* A sentence in the caller's buffer, which is the shape the other shims in this
 * folder report a failure in. */
static void sentence(char *text, size_t size, const char *pattern, va_list arguments) {
  vsnprintf(text, size, pattern, arguments);
}

/* Keeps a failure for this thread to be asked about. */
static void fail(const char *pattern, ...) {
  va_list arguments;
  va_start(arguments, pattern);
  sentence(last_failure, sizeof last_failure, pattern, arguments);
  va_end(arguments);
}

/* A call the system refused, named with the number it gave. */
static void fail_refused(const char *what, HRESULT status) {
  fail("%s refused (0x%08lX)", what, (unsigned long)status);
}

/* Stops the capture with the reason kept in the camera, which is what
 * yapfast_camera_mf_error answers with afterwards. */
static void stop_with(yapfast_camera *camera, const char *pattern, ...) {
  va_list arguments;
  va_start(arguments, pattern);
  {
    std::lock_guard<std::mutex> guard(camera->lock);
    sentence(camera->error, sizeof camera->error, pattern, arguments);
  }
  va_end(arguments);
  camera->stopping = true;
}

/* A call the system refused, named, as the reason a capture ended. */
static void stop_refused(yapfast_camera *camera, const char *what, HRESULT status) {
  stop_with(camera, "%s refused (0x%08lX)", what, (unsigned long)status);
}

/* ---------------------------------------------------------------- the strings */

/* A Windows string as the UTF-8 a name and an id are handed on as, in the caller's
 * buffer. One longer than the buffer is left empty: a link that can't be written
 * whole is no id to open a camera by. */
static void narrow(char *text, size_t size, const WCHAR *wide) {
  int bytes;
  if (!text || !size) return;
  text[0] = 0;
  if (!wide) return;
  bytes = WideCharToMultiByte(CP_UTF8, 0, wide, -1, NULL, 0, NULL, NULL);
  if (bytes <= 1 || (size_t)bytes > size) return;
  WideCharToMultiByte(CP_UTF8, 0, wide, -1, text, bytes, NULL, NULL);
}

/* Starts COM on this thread, which Media Foundation's objects need, and answers
 * whether this call added a reference for the caller to take away.
 * RPC_E_CHANGED_MODE says the thread is already in an apartment it made itself:
 * not a failure here, and not undone here either. */
static int com_start(void) {
  HRESULT status = CoInitializeEx(NULL, COINIT_MULTITHREADED | COINIT_DISABLE_OLE1DDE);
  return status == S_OK || status == S_FALSE;
}

/* --------------------------------------------------------------- the devices */

/* Every camera the system has, as the activates that start one; the caller
 * releases each of them and frees the array. */
static HRESULT cameras(IMFActivate ***list, UINT32 *count) {
  IMFAttributes *attributes = NULL;
  HRESULT status = MFCreateAttributes(&attributes, 2);
  if (SUCCEEDED(status)) {
    status = attributes->SetGUID(MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE,
                                 MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID);
  }
  if (SUCCEEDED(status)) status = MFEnumDeviceSources(attributes, list, count);
  if (attributes) attributes->Release();
  return status;
}

/* --------------------------------------------------------------- the opening */

/* The NV12 type the reader is set to: the largest size the device offers that fits
 * in the one asked for, or the smallest it offers when none does, so that a caller
 * gets what it asked for or the device's own size to scale itself. A device that
 * offers no NV12 at all is asked for NV12 at the size wanted, and the reader's own
 * processor converts to it. */
static int choose_type(yapfast_camera *camera) {
  IMFMediaType *fits = NULL;
  IMFMediaType *smallest = NULL;
  IMFMediaType *chosen = NULL;
  unsigned long long fits_area = 0;
  unsigned long long smallest_area = 0;
  HRESULT status = S_OK;

  for (DWORD index = 0;; index++) {
    IMFMediaType *type = NULL;
    GUID subtype = GUID_NULL;
    UINT32 width = 0;
    UINT32 height = 0;
    status = camera->reader->GetNativeMediaType(MF_SOURCE_READER_FIRST_VIDEO_STREAM, index, &type);
    if (status == MF_E_NO_MORE_TYPES) {
      status = S_OK;
      break;
    }
    if (FAILED(status)) {
      stop_refused(camera, "IMFSourceReader::GetNativeMediaType", status);
      if (fits) fits->Release();
      if (smallest) smallest->Release();
      return 0;
    }
    if (SUCCEEDED(type->GetGUID(MF_MT_SUBTYPE, &subtype)) && subtype == MFVideoFormat_NV12 &&
        SUCCEEDED(MFGetAttributeSize(type, MF_MT_FRAME_SIZE, &width, &height)) && width &&
        height) {
      unsigned long long area = (unsigned long long)width * height;
      if (width <= camera->width && height <= camera->height) {
        if (!fits || area > fits_area) {
          if (fits) fits->Release();
          fits = type;
          fits_area = area;
          type = NULL;
        }
      } else if (!smallest || area < smallest_area) {
        if (smallest) smallest->Release();
        smallest = type;
        smallest_area = area;
        type = NULL;
      }
    }
    if (type) type->Release();
  }
  chosen = fits ? fits : smallest;
  if (chosen) {
    /* One of the two is kept and the other let go, so that the type set below is
     * held exactly once. */
    IMFMediaType *worse = fits ? smallest : fits;
    if (worse) worse->Release();
    /* No faster than asked for: a device that offers 60 when 30 is wanted is
     * asked for 30, and one that offers less keeps what it offers. */
    UINT32 rate = 0;
    UINT32 scale = 0;
    if (SUCCEEDED(MFGetAttributeRatio(chosen, MF_MT_FRAME_RATE, &rate, &scale)) && rate && scale &&
        (double)rate / (double)scale > (double)camera->fps) {
      MFSetAttributeRatio(chosen, MF_MT_FRAME_RATE, camera->fps, 1);
    }
  } else {
    status = MFCreateMediaType(&chosen);
    if (SUCCEEDED(status)) status = chosen->SetGUID(MF_MT_MAJOR_TYPE, MFMediaType_Video);
    if (SUCCEEDED(status)) status = chosen->SetGUID(MF_MT_SUBTYPE, MFVideoFormat_NV12);
    if (SUCCEEDED(status)) {
      status = MFSetAttributeSize(chosen, MF_MT_FRAME_SIZE, camera->width, camera->height);
    }
    if (SUCCEEDED(status)) {
      status = MFSetAttributeRatio(chosen, MF_MT_FRAME_RATE, camera->fps, 1);
    }
    /* Progressive, because a type asked for from nothing says nothing about
     * interlacing, and a frame of it is read as one picture. */
    if (SUCCEEDED(status)) {
      status = chosen->SetUINT32(MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive);
    }
    if (FAILED(status)) {
      stop_refused(camera, "a media type of NV12 at the size wanted", status);
      if (chosen) chosen->Release();
      return 0;
    }
  }
  status = camera->reader->SetCurrentMediaType(MF_SOURCE_READER_FIRST_VIDEO_STREAM, NULL, chosen);
  chosen->Release();
  if (FAILED(status)) {
    stop_with(camera,
              "SetCurrentMediaType refused NV12 (0x%08lX): this camera offers no type the reader "
              "can convert from",
              (unsigned long)status);
    return 0;
  }
  return 1;
}

/* The size the frames are at, which is what a picture reports: the type after the
 * reader took it, since a camera may take a size near the one asked rather than
 * the one asked. */
static int read_size(yapfast_camera *camera) {
  IMFMediaType *type = NULL;
  UINT32 width = 0;
  UINT32 height = 0;
  HRESULT status = camera->reader->GetCurrentMediaType(MF_SOURCE_READER_FIRST_VIDEO_STREAM, &type);
  if (SUCCEEDED(status)) status = MFGetAttributeSize(type, MF_MT_FRAME_SIZE, &width, &height);
  if (SUCCEEDED(status) && width && height) {
    camera->width = width;
    camera->height = height;
    type->Release();
    return 1;
  }
  stop_with(camera, "the size of the media type the reader took is %ux%u (0x%08lX)",
            (unsigned)width, (unsigned)height, (unsigned long)status);
  if (type) type->Release();
  return 0;
}

/* What the frames are read from: the device with this camera's link, a D3D11
 * device, the manager that puts Media Foundation on it, the reader on that, and
 * the NV12 type it is set to. Answers whether all of it came, having said why when
 * it didn't. */
static int start(yapfast_camera *camera) {
  IMFActivate **devices = NULL;
  UINT32 count = 0;
  UINT32 token = 0;
  IMFDXGIDeviceManager *manager = NULL;
  IMFAttributes *attributes = NULL;
  D3D_FEATURE_LEVEL levels[] = {D3D_FEATURE_LEVEL_11_1, D3D_FEATURE_LEVEL_11_0,
                                D3D_FEATURE_LEVEL_10_1, D3D_FEATURE_LEVEL_10_0};
  int found = 0;
  HRESULT status = cameras(&devices, &count);

  if (FAILED(status)) {
    stop_refused(camera, "MFEnumDeviceSources, looking for this camera", status);
    return 0;
  }
  for (UINT32 index = 0; index < count && !found; index++) {
    WCHAR *wide = NULL;
    UINT32 length = 0;
    if (SUCCEEDED(devices[index]->GetAllocatedString(
            MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_SYMBOLIC_LINK, &wide, &length))) {
      /* The id a list here handed out, matched as the string it is: a device's
       * link names it and nothing else does. */
      char link[1024];
      narrow(link, sizeof link, wide);
      found = strcmp(link, camera->id) == 0;
      if (found) status = MFCreateDeviceSource(devices[index], &camera->source);
      CoTaskMemFree(wide);
    }
  }
  for (UINT32 index = 0; index < count; index++) devices[index]->Release();
  if (devices) CoTaskMemFree(devices);
  if (!found) {
    stop_with(camera, "no camera is %s among the %u this system lists", camera->id,
              (unsigned)count);
    return 0;
  }
  if (FAILED(status) || !camera->source) {
    stop_refused(camera, "MFCreateDeviceSource", status);
    return 0;
  }
  /* A D3D11 device is thread-safe unless asked not to be, which is what Media
   * Foundation needs of a device it processes on with no protection of its own, and
   * VIDEO_SUPPORT is what it asks of one whose textures it writes NV12 into. */
  status = D3D11CreateDevice(NULL, D3D_DRIVER_TYPE_HARDWARE, NULL,
                             D3D11_CREATE_DEVICE_BGRA_SUPPORT | D3D11_CREATE_DEVICE_VIDEO_SUPPORT,
                             levels, ARRAYSIZE(levels), D3D11_SDK_VERSION, &camera->device, NULL,
                             NULL);
  if (FAILED(status)) {
    stop_with(camera,
              "D3D11CreateDevice on a hardware adapter refused (0x%08lX): the frames this captures "
              "live in that card's memory",
              (unsigned long)status);
    goto failed;
  }
  status = MFCreateDXGIDeviceManager(&token, &manager);
  if (FAILED(status)) {
    stop_refused(camera, "MFCreateDXGIDeviceManager", status);
    goto failed;
  }
  status = manager->ResetDevice(camera->device, token);
  if (FAILED(status)) {
    stop_refused(camera, "IMFDXGIDeviceManager::ResetDevice", status);
    goto failed;
  }
  status = MFCreateAttributes(&attributes, 3);
  if (FAILED(status)) {
    stop_refused(camera, "MFCreateAttributes", status);
    goto failed;
  }
  status = attributes->SetUnknown(MF_SOURCE_READER_D3D_MANAGER, manager);
  if (FAILED(status)) {
    stop_refused(camera, "an attributes' D3D manager", status);
    goto failed;
  }
  /* The reader's own video processor converts to NV12 and resizes, so that a camera
   * which offers neither the format nor the size asked for still gives frames; see
   * choose_type. */
  status = attributes->SetUINT32(MF_SOURCE_READER_ENABLE_ADVANCED_VIDEO_PROCESSING, TRUE);
  if (FAILED(status)) {
    stop_refused(camera, "an attributes' advanced video processing", status);
    goto failed;
  }
  status = MFCreateSourceReaderFromMediaSource(camera->source, attributes, &camera->reader);
  if (FAILED(status)) {
    stop_refused(camera, "MFCreateSourceReaderFromMediaSource", status);
    goto failed;
  }
  attributes->Release();
  attributes = NULL;
  manager->Release();
  manager = NULL;
  if (!choose_type(camera)) return 0;
  return read_size(camera);

failed:
  if (attributes) attributes->Release();
  if (manager) manager->Release();
  return 0;
}

/* --------------------------------------------------------------- the frames */

/* Hands the frame in `sample` over as the texture it lies in, borrowed for the
 * length of the callback: the sample is held until the callback returns, so the
 * pool can't reuse what it is being read from. */
static void deliver(yapfast_camera *camera, IMFSample *sample) {
  IMFMediaBuffer *buffer = NULL;
  IMFDXGIBuffer *surface = NULL;
  ID3D11Texture2D *texture = NULL;
  UINT32 subresource = 0;
  yapfast_picture picture;
  HRESULT status = sample->GetBufferByIndex(0, &buffer);
  if (SUCCEEDED(status)) status = buffer->QueryInterface(IID_PPV_ARGS(&surface));
  if (SUCCEEDED(status)) status = surface->GetResource(IID_PPV_ARGS(&texture));
  if (SUCCEEDED(status)) status = surface->GetSubresourceIndex(&subresource);
  if (FAILED(status)) {
    /* A buffer that isn't a D3D11 texture is a reader that isn't on the manager this
     * asked it for: worth ending the capture over rather than skipping a frame. */
    stop_with(camera, "a frame's buffer isn't a D3D11 texture (0x%08lX)",
              (unsigned long)status);
  } else {
    memset(&picture, 0, sizeof picture);
    picture.width = (int)camera->width;
    picture.height = (int)camera->height;
    picture.texture = texture;
    picture.subresource = subresource;
    picture.device = camera->device;
    camera->frame(camera->user, &picture);
  }
  if (texture) texture->Release();
  if (surface) surface->Release();
  if (buffer) buffer->Release();
}

/* One frame, or the wait for one: ReadSample blocks until the device has something,
 * which is where the capture thread spends its time. */
static void read_one(yapfast_camera *camera) {
  DWORD stream = 0;
  DWORD flags = 0;
  LONGLONG time = 0;
  IMFSample *sample = NULL;
  HRESULT status = camera->reader->ReadSample(MF_SOURCE_READER_FIRST_VIDEO_STREAM, 0, &stream,
                                              &flags, &time, &sample);
  if (FAILED(status)) {
    /* A source shut down by yapfast_camera_close answers here, which is this file's
     * own doing rather than a failure to report. */
    if (!camera->stopping.load()) stop_refused(camera, "IMFSourceReader::ReadSample", status);
  } else if (flags & MF_SOURCE_READERF_ERROR) {
    /* The reader found the stream broken and carries nothing more about why, so
     * the sentence is this file's: a stream left alone here would read nothing
     * for ever. */
    stop_with(camera, "the camera's stream broke");
  } else if (flags & MF_SOURCE_READERF_ENDOFSTREAM) {
    /* A camera unplugged, or one whose own controls stopped it: the next read would
     * answer this again. */
    stop_with(camera, "the camera stopped sending frames");
  } else if (flags & MF_SOURCE_READERF_STREAMTICK) {
    /* A gap with no frame in it, which is what a device with nothing new answers. */
  } else if (sample) {
    /* A device that renegotiated, from its own controls say, hands over frames at a
     * new size, which a picture has to report: a size that can't be read is a frame
     * that can't be described, and the capture ends over it. */
    if (!(flags & MF_SOURCE_READERF_CURRENTMEDIATYPECHANGED) || read_size(camera)) {
      deliver(camera, sample);
    }
  }
  if (sample) sample->Release();
}

/* Shuts the source down, once, under `lock`: yapfast_camera_close asks for it to
 * wake a read that is waiting, and the capture thread asks for it on its way out,
 * and only the first of them has anything to do. */
static void shut_source(yapfast_camera *camera) {
  if (!camera->source || camera->shut) return;
  camera->source->Shutdown();
  camera->shut = 1;
}

/* Lets go of what the frames were read through, in the order it was made, under the
 * lock because yapfast_camera_close shuts the source down from another thread and
 * must not find it gone. */
static void let_go(yapfast_camera *camera) {
  std::lock_guard<std::mutex> guard(camera->lock);
  if (camera->reader) {
    camera->reader->Release();
    camera->reader = NULL;
  }
  if (camera->source) {
    shut_source(camera);
    camera->source->Release();
    camera->source = NULL;
  }
  if (camera->device) {
    camera->device->Release();
    camera->device = NULL;
  }
}

/* Says whether the opening came, which is what yapfast_camera_open waits for. */
static void publish(yapfast_camera *camera, int opened) {
  {
    std::lock_guard<std::mutex> guard(camera->lock);
    camera->opened = opened;
  }
  camera->ready.notify_one();
}

/* The capture thread: everything is made here and let go here, in one apartment. */
static void capture(yapfast_camera *camera) {
  int com = com_start();
  HRESULT status = MFStartup(MF_VERSION, MFSTARTUP_FULL);
  if (FAILED(status)) {
    stop_refused(camera, "MFStartup", status);
    publish(camera, -1);
    if (com) CoUninitialize();
    return;
  }
  if (!start(camera)) {
    publish(camera, -1);
    let_go(camera);
    MFShutdown();
    if (com) CoUninitialize();
    return;
  }
  publish(camera, 1);
  while (!camera->stopping.load()) read_one(camera);
  let_go(camera);
  MFShutdown();
  if (com) CoUninitialize();
}

/* --------------------------------------------------------------------- the API */

extern "C" {

/* Calls `each` with the name and the id that opens it of each camera on this
 * system, from one look at the devices. */
void yapfast_camera_list(yapfast_camera_each each, void *user) {
  IMFActivate **devices = NULL;
  UINT32 count = 0;
  int com;
  HRESULT status;

  if (!each) return;
  com = com_start();
  status = MFStartup(MF_VERSION, MFSTARTUP_FULL);
  if (FAILED(status)) {
    fail_refused("MFStartup", status);
    if (com) CoUninitialize();
    return;
  }
  status = cameras(&devices, &count);
  if (FAILED(status)) fail_refused("MFEnumDeviceSources", status);
  for (UINT32 index = 0; SUCCEEDED(status) && index < count; index++) {
    WCHAR *name = NULL;
    WCHAR *wide = NULL;
    UINT32 length = 0;
    char named[256] = {0};
    char link[1024] = {0};
    if (SUCCEEDED(devices[index]->GetAllocatedString(MF_DEVSOURCE_ATTRIBUTE_FRIENDLY_NAME, &name,
                                                     &length))) {
      narrow(named, sizeof named, name);
      CoTaskMemFree(name);
    }
    if (SUCCEEDED(devices[index]->GetAllocatedString(
            MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_SYMBOLIC_LINK, &wide, &length))) {
      narrow(link, sizeof link, wide);
      CoTaskMemFree(wide);
    }
    /* A device with no link has no id to open it by, so it isn't offered: what this
     * hands out has to be what yapfast_camera_open takes. */
    if (link[0]) each(user, named, link);
    devices[index]->Release();
  }
  if (devices) CoTaskMemFree(devices);
  MFShutdown();
  if (com) CoUninitialize();
}

/* Opens the camera with `id` and calls `frame` with each picture it sends, at most
 * `width` by `height` and `fps` a second, until yapfast_camera_close. NULL when it
 * couldn't open, with the reason in yapfast_camera_mf_error. */
struct yapfast_camera *yapfast_camera_open(const char *id, int width, int height, int fps,
                                           yapfast_camera_frame frame, void *user) {
  yapfast_camera *camera = NULL;

  if (!id || !*id) {
    fail("a camera needs an id: the one yapfast_camera_list gives");
    return NULL;
  }
  if (!frame) {
    fail("a camera needs somewhere to send its frames");
    return NULL;
  }
  if (width < 2 || height < 2 || fps < 1) {
    fail("%dx%d at %d a second is not something to capture", width, height, fps);
    return NULL;
  }
  camera = new (std::nothrow) yapfast_camera();
  if (!camera) {
    fail("no memory for a camera");
    return NULL;
  }
  if (snprintf(camera->id, sizeof camera->id, "%s", id) >= (int)sizeof camera->id) {
    fail("that is not a camera id: it is longer than any link a device has");
    delete camera;
    return NULL;
  }
  /* Even, as the I420 planes and the encoders that take them want. */
  camera->width = (unsigned)(width & ~1);
  camera->height = (unsigned)(height & ~1);
  camera->fps = (unsigned)fps;
  camera->frame = frame;
  camera->user = user;
  try {
    camera->thread = std::thread(capture, camera);
  } catch (const std::exception &error) {
    fail("no thread to capture on: %s", error.what());
    delete camera;
    return NULL;
  }
  {
    std::unique_lock<std::mutex> guard(camera->lock);
    camera->ready.wait(guard, [camera] { return camera->opened != 0; });
    if (camera->opened < 0) {
      char why[512];
      snprintf(why, sizeof why, "%s", camera->error);
      guard.unlock();
      camera->thread.join();
      delete camera;
      fail("%s", why);
      return NULL;
    }
  }
  return camera;
}

/* Stops capturing: no frame arrives once this returns. */
void yapfast_camera_close(struct yapfast_camera *camera) {
  if (!camera) return;
  camera->stopping = true;
  {
    /* A read waiting on a device with nothing to give has nothing else to wake it:
     * shutting the source down makes that read answer MF_E_SHUTDOWN, which is what
     * ends the thread. */
    std::lock_guard<std::mutex> guard(camera->lock);
    shut_source(camera);
  }
  if (camera->thread.joinable()) camera->thread.join();
  delete camera;
}

/* The message for the last failure: the one that ended `camera`, or, with no camera
 * to ask, the last one on this thread — which is how an open that answered NULL
 * says why. */
void yapfast_camera_mf_error(struct yapfast_camera *camera, char *text, size_t size) {
  if (!text || !size) return;
  text[0] = 0;
  if (!camera) {
    snprintf(text, size, "%s", last_failure);
    return;
  }
  std::lock_guard<std::mutex> guard(camera->lock);
  snprintf(text, size, "%s", camera->error);
}

}  // extern "C"
