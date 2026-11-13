/* Windows Graphics Capture: a screen, or one of its windows, arriving as a D3D11
 * texture on the device the encoder already reads from.
 *
 * The go-live path captures with libwebrtc's own capturer, which hands over I420
 * in memory: every frame is converted and uploaded before an encoder sees it.
 * native/d3d11enc.c took the encoder off that path, reading D3D11 textures where
 * they lie; this is the other end of it, a capture source that produces those
 * textures instead of pixels. Together a frame goes from the compositor to the
 * driver without crossing the processor, which on this machine is the difference
 * between about 5.4 ms of the processor per 3440x1440 frame and about 0.8
 * (.local/spike/wgc-nvenc/RESULTS.md measured both sides).
 *
 * Three things this does that a caller has to know about:
 *
 *   - the pool is made with `CreateFreeThreaded`, so it has no thread affinity
 *     and no DispatcherQueue or message pump is needed: frames are polled with
 *     `TryGetNextFrame`. That is what makes a capture thread possible at all,
 *     since none of the stream's threads pump anything.
 *   - the pixel format is asked for, not assumed. A screen showing SDR is
 *     captured as BGRA, which is the one format NVENC takes without a
 *     conversion; a screen showing HDR is captured as `R16G16B16A16_FLOAT`,
 *     which is the scRGB picture the compositor really draws it as, so that the
 *     encoder's video processor can tone-map it on the card. Asking for BGRA on
 *     an HDR screen instead gets the compositor's own tone map, already burnt
 *     into 8 bits, and nothing downstream can tell what it was.
 *     Whether the screen shows HDR is the caller's answer, from the system's
 *     display configuration; nothing here guesses it from pixels. A driver that
 *     won't make a pool of the float picture is not a failed capture: the 8-bit
 *     one is asked for instead and the refusal is reported in the line
 *     `yapfast_wgc_describe` writes, which is the 8-bit behaviour this file had
 *     before the float picture existed.
 *   - the capture is of one display, by HMONITOR, or of one window, by HWND, and
 *     not of a desktop: a caller with several screens says which, and a caller
 *     that means a window says which window. The two are different doors into
 *     Windows Graphics Capture (`CreateForMonitor` and `CreateForWindow`), and
 *     nothing here turns one into the other: a window's handle is never read as
 *     a monitor's, and a window that can't be captured fails by name rather
 *     than capturing the screen it sits on.
 *
 * Ownership, which is the whole memory story of this file: `yapfast_wgc_frame`
 * hands back an `ID3D11Texture2D` that belongs to the frame pool, and the caller
 * *borrows* it. It stays valid until the next `yapfast_wgc_frame` or
 * `yapfast_wgc_close` on the same capture, whichever comes first, and the caller
 * must not Release it. A caller that has to hold a frame past the next call
 * AddRefs the texture itself and Releases that reference later. The shim holds
 * exactly one frame and one texture at a time — the ones the last call handed
 * out — so a stopped stream stops holding a compositor buffer, and nothing here
 * allocates per frame.
 *
 * The device follows the same rule, kept with a flag. A capture opened with
 * `yapfast_wgc_open` makes its device and releases it in `yapfast_wgc_close`; a
 * capture opened with `yapfast_wgc_open_on` is on a device its caller made — in a
 * stream, the encoder's, so that the texture a frame arrives in is a texture the
 * encoder reads — and this neither AddRefs nor releases that one. The caller
 * keeps it alive for at least as long as the capture. Only a device that sits on
 * the display's own adapter can be used this way, and `yapfast_wgc_open_on` says
 * so by name when the one handed in doesn't.
 *
 * A window is also three things a display isn't, and `yapfast_wgc_frame` asks
 * about them before it touches the pool: the window can be gone, minimised, or
 * out of every capture by its own display affinity. The first and last end the
 * capture — the pool still holds the moment before, and handing that over again
 * is stale or protected content, which is the one thing a caller must never keep
 * sending — while a minimised window is answered as nothing new, since the
 * compositor has no picture of it to give.
 *
 * What it deliberately doesn't do yet: it opens a capture and hands out frames,
 * and nothing else. No rate limiting (`IGraphicsCaptureSession2`'s
 * `MinUpdateInterval`), and nothing in this crate calls it — the loop that does is
 * the next stage; see src/wgc.rs. The cursor is the one thing it does set: WGC
 * includes it by default, and saying so explicitly is what keeps that true. */

#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <d3d11.h>
#include <dxgi1_2.h>
#include <hstring.h>
#include <inspectable.h>
#include <roapi.h>
#include <new>
#include <stdarg.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* The Windows SDK's own WinRT declarations, not C++/WinRT's: they are C++ classes
 * with virtual methods and no exceptions, which is what a binary built with
 * clang-cl and no exception handling needs. Every call below is a virtual call,
 * every method returns an HRESULT, and nothing can throw across this file's
 * boundary — the same discipline native/d3d11enc.c keeps, in the language the
 * WinRT headers are generated for. */
#include <windows.foundation.h>
#include <windows.graphics.capture.h>
#include <windows.graphics.directx.h>
#include <windows.graphics.directx.direct3d11.h>

/* Reached off activation factories rather than projected by the SDK's headers, so
 * declared here with their documented ids and vtable orders. The interop headers
 * that do carry the first two (`Windows.Graphics.Capture.Interop.h` and the
 * Direct3D11 one) also redeclare ABI::Windows::Graphics::DirectX::Direct3D11,
 * which the headers above already declare, so including them doesn't compile;
 * the four below are everything this file needs from them and no more.
 *
 * The border is the third: `IsBorderRequired` lives on IGraphicsCaptureSession3,
 * which exists from Windows 11 22H2, so it is asked for at runtime and its
 * absence is reported rather than compiled in. `RequestAccessAsync` beside it is
 * the call that actually grants a capture without the yellow indicator:
 * GraphicsCaptureAccessKind has only Borderless and Programmatic, nothing called
 * Border, and the answer comes back as an AppCapability status. */
namespace abi {

MIDL_INTERFACE("3628E81B-3CAC-4C60-B7F4-23CE0E0C3356")
ItemInterop : public IUnknown {
  virtual HRESULT STDMETHODCALLTYPE CreateForWindow(HWND window, REFIID iid, void **out) = 0;
  virtual HRESULT STDMETHODCALLTYPE CreateForMonitor(HMONITOR monitor, REFIID iid, void **out) = 0;
};

MIDL_INTERFACE("A9B3D012-3DF2-4EE3-B8D1-8695F457D3C1")
SurfaceAccess : public IUnknown {
  virtual HRESULT STDMETHODCALLTYPE GetInterface(REFIID iid, void **out) = 0;
};

MIDL_INTERFACE("f2cdd966-22ae-5ea1-9596-3a289344c3be")
CaptureSession3 : public IInspectable {
  virtual HRESULT STDMETHODCALLTYPE get_IsBorderRequired(boolean *value) = 0;
  virtual HRESULT STDMETHODCALLTYPE put_IsBorderRequired(boolean value) = 0;
};

MIDL_INTERFACE("743ed370-06ec-5040-a58a-901f0f757095")
CaptureAccessStatics : public IInspectable {
  virtual HRESULT STDMETHODCALLTYPE RequestAccessAsync(
      ABI::Windows::Graphics::Capture::GraphicsCaptureAccessKind request,
      __FIAsyncOperation_1_Windows__CSecurity__CAuthorization__CAppCapabilityAccess__CAppCapabilityAccessStatus
          **operation) = 0;
};

} /* namespace abi */

/* Exported by d3d11.dll: the WinRT wrapper around a DXGI device that the frame
 * pool is created on. Declared rather than included, for the reason above. */
extern "C" HRESULT __stdcall CreateDirect3D11DeviceFromDXGIDevice(IDXGIDevice *device,
                                                                 IInspectable **out);

namespace capture_abi = ABI::Windows::Graphics::Capture;
namespace directx_abi = ABI::Windows::Graphics::DirectX;
namespace direct3d11_abi = ABI::Windows::Graphics::DirectX::Direct3D11;
namespace capability_abi = ABI::Windows::Security::Authorization::AppCapabilityAccess;

/* What a call to yapfast_wgc_frame found, as Rust reads it. A capture that is
 * asked for a frame has four answers, and the two interesting ones are the
 * unusual ones: a screen that hasn't changed produces no frame at all, and a
 * capture whose size changed has to be rebuilt before its frames are usable. */
enum {
  /* The capture is over: the thing captured closed, or its session ended. Not a
   * failure — a window's owner quitting is what a window capture ends with, so
   * it is answered rather than reported as damage. */
  YAPFAST_WGC_CLOSED = -1,
  /* The call failed. `why` says what refused, in words. */
  YAPFAST_WGC_FAILED = -2,
  /* Nothing has been composited since the last call: ask again later. */
  YAPFAST_WGC_IDLE = 0,
  /* A frame arrived, and its texture is the one `texture` was pointed at. */
  YAPFAST_WGC_FRAME = 1,
  /* The thing captured changed size. The pool has been rebuilt at the new size
   * and the frame that reported it was dropped, since a texture at the old size
   * is not what a caller wants next; `yapfast_wgc_content_size` has the new one,
   * and the next call hands over a frame at it. */
  YAPFAST_WGC_RESIZED = 2,
};

struct yapfast_wgc;

extern "C" void yapfast_wgc_close(yapfast_wgc *capture);

/* The `Closed` event's handler, which WinRT holds a reference to for as long as
 * the subscription lasts. It carries the capture it belongs to and does one thing
 * on the compositor's thread: set a flag. No lock and no call back into the
 * capture, which may be closing at that moment. */
class closed_handler final
    : public ABI::Windows::Foundation::ITypedEventHandler<capture_abi::GraphicsCaptureItem *,
                                                         IInspectable *> {
 public:
  explicit closed_handler(yapfast_wgc *owner) : references_(1), capture_(owner) {}

  HRESULT STDMETHODCALLTYPE QueryInterface(REFIID iid, void **out) override;
  ULONG STDMETHODCALLTYPE AddRef() override;
  ULONG STDMETHODCALLTYPE Release() override;
  HRESULT STDMETHODCALLTYPE Invoke(capture_abi::IGraphicsCaptureItem *sender,
                                   IInspectable *arguments) override;

 private:
  /* Nothing deletes this but Release: the item holds one reference and the
   * capture holds the other, so the object outlives whichever of them finishes
   * first. */
  ~closed_handler() = default;
  LONG references_;
  yapfast_wgc *capture_;
};

struct yapfast_wgc {
  /* The machine's side: the adapter that drives the monitor being captured, the
   * device made on it, and the WinRT wrapper around that device the pool needs.
   * These live as long as the capture and are what a frame's texture is on. */
  IDXGIAdapter1 *adapter;
  ID3D11Device *device;
  /* Whether this capture made the device. It releases only what it made: a
   * device handed to `yapfast_wgc_open_on` is the caller's to release. */
  int owns_device;
  ID3D11DeviceContext *context;
  IInspectable *graphics_device;

  /* WinRT's side, in the order they were made. */
  capture_abi::IGraphicsCaptureItem *item;
  capture_abi::IDirect3D11CaptureFramePool *pool;
  capture_abi::IGraphicsCaptureSession *session;

  /* The frame whose texture was last handed out, and that texture. Both are let
   * go by the next call: see the ownership note at the top of this file. */
  capture_abi::IDirect3D11CaptureFrame *frame;
  ID3D11Texture2D *texture;

  /* The `Closed` subscription and the flag its thread sets. `watching` says the
   * token is one to remove, since a failed subscription leaves none. */
  closed_handler *handler;
  EventRegistrationToken closed_token;
  LONG watching;
  LONG closed;

  /* The size the pool is at, which is the size of the frames it hands out until
   * the thing captured is resized, and the format it holds: BGRA, or the
   * half-float scRGB picture of an HDR screen, which the pool is remade in when
   * the captured thing is resized. */
  unsigned width, height;
  DXGI_FORMAT format;

  /* What the frames are, and which call refused the float picture when one was
   * asked for and didn't take, as one clause of the report: a share that ended
   * up on the 8-bit picture says so here rather than looking like one whose
   * screen never showed HDR. */
  char pixels[192];

  /* Whether the item is of a window rather than a display, and which window: a
   * window capture has an identity of its own to ask about on every frame, where
   * a display has nothing to ask. The name is written once, when the capture is
   * opened, because a report is read once and a retitled window is still the same
   * window. */
  int is_window;
  HWND window;
  char window_title[96];
  char window_class[48];

  /* Why the capture ended, when it did: `yapfast_wgc_frame` answers
   * YAPFAST_WGC_CLOSED from more than one cause, and a caller that stops a share
   * has to be able to say which. Empty while the capture still runs. */
  char ending[192];

  /* The border's answer in the two shapes a caller decides with: `borderless`
   * is 1 only when `IsBorderRequired(false)` was asked and took, and
   * `can_ask_borderless` is 1 when this build of Windows has the interface that
   * can ask at all. A caller that means to show no outline reads the first, and
   * a caller that wants to know whether asking was ever possible reads the
   * second; neither has to read the sentence below. A capture with the yellow
   * indicator is still a capture, so the sentence is reported rather than
   * failed. The pointer is reported the same way and for the same reason. */
  int borderless;
  int can_ask_borderless;
  char border[192];
  char pointer[160];

  /* Which adapter and which of its outputs this capture is on, and what they are
   * called, for the report and for a failure that has to say what was looked at. */
  LUID luid;
  unsigned output;
  char card[128];
  char display[64];
  int primary;
};

/* ------------------------------------------------------------- the failures */

/* A failure in the caller's buffer, when it has one: the shape every shim here
 * reports in, so a stream's log names the call and the number the system gave
 * rather than a bare status. */
static void say(char *why, size_t size, const char *format, ...) {
  va_list arguments;
  if (!why || !size) return;
  va_start(arguments, format);
  vsnprintf(why, size, format, arguments);
  va_end(arguments);
}

/* A call the system refused, named. */
static void refused(char *why, size_t size, const char *what, HRESULT status) {
  say(why, size, "%s refused (0x%08lX)", what, (unsigned long)status);
}

/* --------------------------------------------------------------- the events */

HRESULT STDMETHODCALLTYPE closed_handler::QueryInterface(REFIID iid, void **out) {
  if (!out) return E_POINTER;
  *out = NULL;
  /* The delegate is only ever asked for itself: the item holds it, and nothing
   * else in this file passes it anywhere. It is not an IInspectable — WinRT's
   * delegates derive from IUnknown — so GetIids and its neighbours aren't here
   * to implement. */
  if (IsEqualIID(iid, IID_IUnknown) ||
      IsEqualIID(iid, __uuidof(ABI::Windows::Foundation::ITypedEventHandler<
                              capture_abi::GraphicsCaptureItem *, IInspectable *>))) {
    *out = this;
    AddRef();
    return S_OK;
  }
  return E_NOINTERFACE;
}

ULONG STDMETHODCALLTYPE closed_handler::AddRef() {
  return (ULONG)InterlockedIncrement(&references_);
}

ULONG STDMETHODCALLTYPE closed_handler::Release() {
  LONG left = InterlockedDecrement(&references_);
  if (left == 0) delete this;
  return (ULONG)left;
}

HRESULT STDMETHODCALLTYPE closed_handler::Invoke(capture_abi::IGraphicsCaptureItem *sender,
                                                IInspectable *arguments) {
  (void)sender;
  (void)arguments;
  if (capture_) InterlockedExchange(&capture_->closed, 1);
  return S_OK;
}

/* Whether the Closed event has fired, read atomically: the event's thread writes
 * the flag and the capture's thread reads it. */
static int ended(yapfast_wgc *capture) {
  return InterlockedCompareExchange(&capture->closed, 1, 1) == 1;
}

/* Starts watching the item for its end, and says nothing when it can't: a capture
 * without the event still hands out frames, and the pool going quiet is what a
 * caller sees instead. */
static void watch_closed(yapfast_wgc *capture) {
  closed_handler *handler = new (std::nothrow) closed_handler(capture);
  if (!handler) return;
  if (FAILED(capture->item->add_Closed(handler, &capture->closed_token))) {
    handler->Release();
    return;
  }
  /* The item holds its own reference from here, so closing the capture drops the
   * other one and leaves the object to the item. */
  capture->handler = handler;
  InterlockedExchange(&capture->watching, 1);
}

/* -------------------------------------------------------------- the adapter */

/* Which adapter and output show `monitor`, and what they are called.
 *
 * The plan is that a caller names a display by LUID and output index, which is
 * how DXGI identifies one; this turns the monitor handle a caller enumerates
 * (`MonitorFromPoint`, `GetMonitorInfo`, or the UI's own list) into the same pair
 * by asking every adapter's outputs which monitor they drive. A machine with
 * integrated and discrete graphics has adapters on both, and a capture on the
 * wrong one would fill a texture the encoder's session can't read, so the adapter
 * is the one that drives the screen being captured rather than the first one
 * DXGI lists.
 *
 * `monitor` of NULL means the primary display, so a caller that only wants to see
 * the thing work doesn't have to enumerate anything.
 *
 * `what` names the thing in the failure, since the same search answers for a
 * display a caller named and for the display the window it named is on. */
static int adapter_for_monitor(HMONITOR monitor, const char *what, IDXGIAdapter1 **adapter_out,
                               DXGI_OUTPUT_DESC *output_out, LUID *luid_out, unsigned *index_out,
                               char *card, size_t card_size, char *display, size_t display_size,
                               HMONITOR *resolved_out, int *primary, char *why, size_t size) {
  IDXGIFactory1 *factory = NULL;
  IDXGIAdapter1 *chosen = NULL;
  HRESULT status;
  char seen[256] = "";
  size_t used = 0;
  unsigned found_index = 0;
  int found = 0;

  *primary = 0;
  if (!monitor) {
    POINT corner = {0, 0};
    monitor = MonitorFromPoint(corner, MONITOR_DEFAULTTOPRIMARY);
    if (!monitor) {
      say(why, size, "no primary display: MonitorFromPoint(0,0) found none");
      return -1;
    }
    *primary = 1;
  }
  /* The handle the caller named, or the primary's: `CreateForMonitor` with a
   * null handle makes an item of the whole virtual desktop, which is how a share
   * of the primary once came out 4520x1920 on a 3440x1440 screen with a second
   * display beside it. */
  *resolved_out = monitor;
  status = CreateDXGIFactory1(IID_PPV_ARGS(&factory));
  if (FAILED(status)) {
    refused(why, size, "CreateDXGIFactory1", status);
    return -1;
  }
  /* Every adapter is named in the failure with how many outputs it drives, not
   * just the fact that the monitor wasn't found: a machine whose screen is on a
   * card this build doesn't see says which cards it does see. */
  for (UINT i = 0; !found; i++) {
    IDXGIAdapter1 *each = NULL;
    DXGI_ADAPTER_DESC1 described;
    unsigned outputs = 0;
    HRESULT listed = factory->EnumAdapters1(i, &each);
    /* Only NOT_FOUND ends the list: every other failure leaves `each` as it
     * was, which is nothing to ask for a description, and a driver hiccup here
     * must not reach GetDesc1 through a null. */
    if (listed == DXGI_ERROR_NOT_FOUND) break;
    if (FAILED(listed)) {
      refused(why, size, "IDXGIFactory1::EnumAdapters1", listed);
      break;
    }
    if (!each) break;
    if (FAILED(each->GetDesc1(&described))) {
      each->Release();
      continue;
    }
    for (UINT o = 0;; o++) {
      IDXGIOutput *display_output = NULL;
      DXGI_OUTPUT_DESC output;
      HRESULT listed_output = each->EnumOutputs(o, &display_output);
      /* The same rule as the adapters: NOT_FOUND is the end of the list, and
       * anything else is not a display to read. */
      if (listed_output == DXGI_ERROR_NOT_FOUND) break;
      if (FAILED(listed_output) || !display_output) break;
      outputs++;
      if (!found && SUCCEEDED(display_output->GetDesc(&output)) && output.Monitor == monitor) {
        found = 1;
        found_index = o;
        chosen = each;
        *output_out = output;
        *luid_out = described.AdapterLuid;
        if (card_size) snprintf(card, card_size, "%ls", described.Description);
        if (display_size) snprintf(display, display_size, "%ls", output.DeviceName);
      }
      display_output->Release();
    }
    if (used < sizeof seen - 1) {
      int written = snprintf(seen + used, sizeof seen - used, "%s%ls (0x%04lX, %u output%s)",
                             used ? ", " : "", described.Description,
                             (unsigned long)described.VendorId, outputs, outputs == 1 ? "" : "s");
      if (written > 0) used += (size_t)written;
      if (used >= sizeof seen) used = sizeof seen - 1;
    }
    if (!found) each->Release();
  }
  factory->Release();
  if (!found) {
    /* An enumeration that failed above has said what it was; that is more use
     * than the absence of an adapter it could have found. */
    if (!why || !size || !why[0]) {
      say(why, size,
          "no adapter drives %s (HMONITOR %p); the adapters here drive %s",
          what ? what : "the display being captured", (void *)monitor,
          seen[0] ? seen : "nothing");
    }
    return -1;
  }
  *index_out = found_index;
  *adapter_out = chosen;
  return 0;
}

/* --------------------------------------------------------------- the border */

/* The build number of the Windows this process runs on, which is the one fact
 * that says whether Graphics Capture can take its border off before a capture is
 * opened: `IGraphicsCaptureSession3` exists from Windows 11 22H2, build 22621,
 * and a caller that chooses which way to capture has to know that before it
 * chooses. ntdll's own call is the one that answers: the documented
 * `GetVersionExW` is shimmed by the program's manifest, and one without it is
 * told it runs on 6.2. Loaded by hand because ntdll is not among the libraries
 * this file links. 0 when even that can't be asked, which reads as a Windows
 * with no such interface. */
static unsigned windows_build(void) {
  typedef LONG(WINAPI * rtl_get_version)(OSVERSIONINFOW *);
  HMODULE ntdll = GetModuleHandleW(L"ntdll.dll");
  rtl_get_version ask;
  OSVERSIONINFOW version;
  if (!ntdll) return 0;
  ask = (rtl_get_version)(void *)GetProcAddress(ntdll, "RtlGetVersion");
  if (!ask) return 0;
  memset(&version, 0, sizeof version);
  version.dwOSVersionInfoSize = sizeof version;
  if (ask(&version) != 0) return 0;
  return (unsigned)version.dwBuildNumber;
}

/* How long the access request is given before the border is left on and the
 * capture goes ahead: it answers in milliseconds here, and a capture that draws
 * the indicator for a moment at startup is better than one that fails to open
 * because a broker never answered. */
#define ACCESS_WAIT_MS 2000

/* A name for an AppCapability answer, so the report says what happened without
 * its reader needing the enum's numbering. */
static const char *access_name(capability_abi::AppCapabilityAccessStatus status) {
  switch (status) {
  case capability_abi::AppCapabilityAccessStatus_Allowed: return "Allowed";
  case capability_abi::AppCapabilityAccessStatus_DeniedBySystem: return "DeniedBySystem";
  case capability_abi::AppCapabilityAccessStatus_DeniedByUser: return "DeniedByUser";
  case capability_abi::AppCapabilityAccessStatus_UserPromptRequired: return "UserPromptRequired";
  case capability_abi::AppCapabilityAccessStatus_NotDeclaredByApp: return "NotDeclaredByApp";
  }
  return "an answer this file doesn't know";
}

/* Asks for a capture without the yellow indicator, and reports which way it went:
 * an interface this build of Windows doesn't have is a Windows 10 machine, where
 * the border cannot be removed at all; an access request that is refused is a
 * managed machine, where it captures with the indicator; and
 * `put_IsBorderRequired(false)` that takes is what Windows 11 22H2 and later do.
 * None of these is a reason to refuse the capture, so the answer is kept as the
 * two flags a caller decides with and as the sentence the report carries. */
static void ask_borderless(yapfast_wgc *capture) {
  abi::CaptureSession3 *session3 = NULL;
  abi::CaptureAccessStatics *access = NULL;
  HSTRING_HEADER header;
  HSTRING name;
  boolean required = TRUE;
  HRESULT status;

  capture->borderless = 0;
  capture->can_ask_borderless = 0;
  if (!capture->session) {
    snprintf(capture->border, sizeof capture->border,
             "the border is on: there is no session to ask");
    return;
  }
  status = capture->session->QueryInterface(IID_PPV_ARGS(&session3));
  if (FAILED(status)) {
    snprintf(capture->border, sizeof capture->border,
             "the border is on: this build of Windows has no IGraphicsCaptureSession3 (0x%08lX), "
             "so the capture draws the indicator",
             (unsigned long)status);
    return;
  }
  /* From here the interface that can ask is there, whatever it answers: this is
   * the difference between a Windows that has no way to remove the border and a
   * Windows that has one and refused. */
  capture->can_ask_borderless = 1;
  session3->get_IsBorderRequired(&required);
  status = WindowsCreateStringReference(
      L"Windows.Graphics.Capture.GraphicsCaptureAccess",
      (UINT32)(ARRAYSIZE(L"Windows.Graphics.Capture.GraphicsCaptureAccess") - 1), &header, &name);
  if (SUCCEEDED(status))
    status = RoGetActivationFactory(name, __uuidof(abi::CaptureAccessStatics), (void **)&access);
  if (FAILED(status)) {
    snprintf(capture->border, sizeof capture->border,
             "the border is on: there is no GraphicsCaptureAccess to ask with (0x%08lX), so "
             "IsBorderRequired stays as it was",
             (unsigned long)status);
    session3->Release();
    return;
  }
  {
    __FIAsyncOperation_1_Windows__CSecurity__CAuthorization__CAppCapabilityAccess__CAppCapabilityAccessStatus
        *operation = NULL;
    status = access->RequestAccessAsync(
        capture_abi::GraphicsCaptureAccessKind_Borderless, &operation);
    if (FAILED(status)) {
      snprintf(capture->border, sizeof capture->border,
               "the border is on: RequestAccessAsync(Borderless) refused (0x%08lX)",
               (unsigned long)status);
    } else {
      /* Waited on by asking its status rather than by blocking on a completion
       * handler: the operation is run by a broker in another process, a handler
       * would be another COM object, and polling costs a couple of hundred
       * microseconds once per capture. */
      ABI::Windows::Foundation::IAsyncInfo *info = NULL;
      capability_abi::AppCapabilityAccessStatus answer =
          capability_abi::AppCapabilityAccessStatus_NotDeclaredByApp;
      if (SUCCEEDED(operation->QueryInterface(IID_PPV_ARGS(&info))) && info) {
        for (int waited = 0; waited < ACCESS_WAIT_MS; waited += 2) {
          ABI::Windows::Foundation::AsyncStatus state =
              ABI::Windows::Foundation::AsyncStatus::Started;
          if (FAILED(info->get_Status(&state))) break;
          if (state != ABI::Windows::Foundation::AsyncStatus::Started) break;
          Sleep(2);
        }
        info->Release();
      }
      if (SUCCEEDED(operation->GetResults(&answer)) &&
          answer == capability_abi::AppCapabilityAccessStatus_Allowed) {
        session3->put_IsBorderRequired(FALSE);
        session3->get_IsBorderRequired(&required);
        if (required == FALSE) {
          capture->borderless = 1;
          snprintf(capture->border, sizeof capture->border,
                   "the border is off: RequestAccessAsync(Borderless) = Allowed and "
                   "IsBorderRequired(false) took");
        } else {
          snprintf(capture->border, sizeof capture->border,
                   "the border is on: RequestAccessAsync(Borderless) = Allowed but "
                   "IsBorderRequired(false) was refused");
        }
      } else {
        snprintf(capture->border, sizeof capture->border,
                 "the border is on: RequestAccessAsync(Borderless) = %s", access_name(answer));
      }
      operation->Release();
    }
  }
  access->Release();
  session3->Release();
}

/* Makes sure the mouse pointer is in the frames, and says which way that went.
 *
 * WGC includes the cursor unless it is told not to, so a capture built without
 * this line already had one — the frames this file handed out before it did. It
 * is set anyway because a default is not a contract: `IsCursorCaptureEnabled` is
 * the thing that means "the pointer is part of the picture", and a later Windows
 * that changed the default would turn every stream's mouse into a hole. The
 * property lives on IGraphicsCaptureSession2, which exists from Windows 10 1803,
 * so on anything older the question can't be asked and the default is what a
 * capture keeps — said in the report rather than failed. */
static void include_pointer(yapfast_wgc *capture) {
  capture_abi::IGraphicsCaptureSession2 *session2 = NULL;
  boolean enabled = FALSE;
  HRESULT status;

  if (!capture->session) {
    snprintf(capture->pointer, sizeof capture->pointer,
             "the pointer is in the capture only by Windows' default: there is no session to ask");
    return;
  }
  status = capture->session->QueryInterface(IID_PPV_ARGS(&session2));
  if (FAILED(status)) {
    snprintf(capture->pointer, sizeof capture->pointer,
             "the pointer is in the capture only by Windows' default: this build of Windows has "
             "no IGraphicsCaptureSession2 (0x%08lX)",
             (unsigned long)status);
    return;
  }
  status = session2->put_IsCursorCaptureEnabled(TRUE);
  if (FAILED(status)) {
    snprintf(capture->pointer, sizeof capture->pointer,
             "the pointer is in the capture only by Windows' default: "
             "IsCursorCaptureEnabled(true) was refused (0x%08lX)",
             (unsigned long)status);
  } else {
    /* Read back rather than assumed: a refused-but-S_OK put is the sort of thing
     * that would leave a stream quietly without a cursor. */
    session2->get_IsCursorCaptureEnabled(&enabled);
    snprintf(capture->pointer, sizeof capture->pointer,
             "the pointer is captured (IsCursorCaptureEnabled = %s)",
             enabled ? "true" : "false");
  }
  session2->Release();
}

/* ---------------------------------------------------------------- the capture */

/* The format a pool asks Windows for, from the DXGI format the capture is in: the
 * two are the same picture under two names, one DXGI's and one WinRT's. */
static directx_abi::DirectXPixelFormat pixel_format(DXGI_FORMAT format) {
  if (format == DXGI_FORMAT_R16G16B16A16_FLOAT)
    return directx_abi::DirectXPixelFormat_R16G16B16A16Float;
  return directx_abi::DirectXPixelFormat_B8G8R8A8UIntNormalized;
}

/* The pool a capture's frames arrive in: free-threaded so that no thread owns it
 * and TryGetNextFrame can be called from the capture thread, two buffers so that
 * one can be read while the compositor fills the other, and in the format the
 * capture was opened for: BGRA, which is what NVENC takes without a conversion,
 * or the half-float scRGB picture of an HDR screen, which the encoder's video
 * processor tone-maps on the card.
 *
 * A pool that won't hold the float picture is answered by asking for the 8-bit
 * one, which is what this file delivered before it asked for anything else: an
 * HDR share whose driver refuses the float picture is a share on the same
 * picture as before with a sentence saying which call refused, not a capture
 * that fails. */
static HRESULT make_pool(yapfast_wgc *capture, ABI::Windows::Graphics::SizeInt32 size) {
  capture_abi::IDirect3D11CaptureFramePoolStatics2 *statics = NULL;
  HSTRING_HEADER header;
  HSTRING name;
  HRESULT status;

  status = WindowsCreateStringReference(
      L"Windows.Graphics.Capture.Direct3D11CaptureFramePool",
      (UINT32)(ARRAYSIZE(L"Windows.Graphics.Capture.Direct3D11CaptureFramePool") - 1), &header,
      &name);
  if (FAILED(status)) return status;
  status = RoGetActivationFactory(name, IID_PPV_ARGS(&statics));
  if (FAILED(status)) return status;
  status = statics->CreateFreeThreaded((direct3d11_abi::IDirect3DDevice *)capture->graphics_device,
                                      pixel_format(capture->format), 2, size, &capture->pool);
  if (FAILED(status) && capture->format == DXGI_FORMAT_R16G16B16A16_FLOAT) {
    snprintf(capture->pixels, sizeof capture->pixels,
             "the screen shows HDR, and the frame pool refused the 16-bit float picture "
             "(CreateFreeThreaded, 0x%08lX): the frames are BGRA8, which Windows tone-maps "
             "itself",
             (unsigned long)status);
    capture->format = DXGI_FORMAT_B8G8R8A8_UNORM;
    status = statics->CreateFreeThreaded((direct3d11_abi::IDirect3DDevice *)capture->graphics_device,
                                        pixel_format(capture->format), 2, size, &capture->pool);
  }
  statics->Release();
  return status;
}

/* The card a device was made on, as DXGI reports it. LUIDs of two adapters are
 * what this compares, since that is a fact about the hardware; the devices
 * themselves are never compared, because two devices made on one adapter can't
 * read each other's textures and matching LUIDs would say they could. */
static HRESULT device_adapter(ID3D11Device *device, IDXGIAdapter1 **adapter_out,
                              DXGI_ADAPTER_DESC1 *desc) {
  IDXGIDevice *dxgi = NULL;
  IDXGIAdapter *adapter = NULL;
  IDXGIAdapter1 *found = NULL;
  HRESULT status = device->QueryInterface(IID_PPV_ARGS(&dxgi));
  if (FAILED(status)) return status;
  status = dxgi->GetAdapter(&adapter);
  dxgi->Release();
  if (FAILED(status)) return status;
  status = adapter->QueryInterface(IID_PPV_ARGS(&found));
  adapter->Release();
  if (FAILED(status)) return status;
  status = found->GetDesc1(desc);
  if (FAILED(status)) {
    found->Release();
    return status;
  }
  *adapter_out = found;
  return S_OK;
}

/* ---------------------------------------------------------------- the window */

/* The affinity a window is hidden from every application's capture with. Windows
 * 10 2004 added it, and a protected window — a password manager, a DRM player —
 * is what sets it. The number is spelled out because an SDK older than the
 * Windows it builds for has no name for it. */
#ifndef WDA_EXCLUDEFROMCAPTURE
#define WDA_EXCLUDEFROMCAPTURE 0x00000011
#endif

/* Whether `window` has taken itself out of every capture. Asked before each
 * frame rather than once at the open, because a window can exclude itself while
 * it is being captured, and that is exactly the case a caller has to stop for:
 * the compositor keeps its last picture of the window and refreshes it with
 * nothing, so continuing to ask would send the same stale content forever. A
 * window whose affinity the system won't report is not evidence of exclusion, so
 * it reads as no — `WDA_MONITOR` is left alone too, since that one still
 * composites a picture, black or not, which the caller can see is black. */
static int excluded_from_capture(HWND window) {
  DWORD affinity = 0;
  if (!GetWindowDisplayAffinity(window, &affinity)) return 0;
  return affinity == WDA_EXCLUDEFROMCAPTURE;
}

/* Refuses a window that Windows can't make a capture item of, in a sentence
 * rather than in the status `CreateForWindow` would answer with, because the
 * caller chose this window and being given a different picture instead is worse
 * than being told: a handle for nothing, a handle whose window has closed, a
 * child window (whose item would be of its top-level window, not of it), and the
 * desktop, which is a display and is captured as one. */
static int validate_window(HWND window, char *why, size_t size) {
  HWND root;

  if (!window) {
    say(why, size,
        "no window to capture: a window capture needs that window's own handle, which is not the "
        "desktop and not the primary display");
    return -1;
  }
  if (!IsWindow(window)) {
    say(why, size,
        "window %p is not a window any more: it has closed, or that handle is not one this "
        "desktop hands out (IsWindow said no)",
        (void *)window);
    return -1;
  }
  if (window == GetDesktopWindow() || window == GetShellWindow()) {
    say(why, size,
        "window %p is the desktop, which is captured as a display rather than as a window",
        (void *)window);
    return -1;
  }
  /* The item `CreateForWindow` makes of a child window is its top-level window's,
   * so a child asked for by handle would arrive as somebody's whole application:
   * refused by name instead, with the window that would have been captured. */
  root = GetAncestor(window, GA_ROOT);
  if (root && root != window) {
    say(why, size,
        "window %p is not a top-level window: its top-level window is %p, and that is what a "
        "capture of it would be of",
        (void *)window, (void *)root);
    return -1;
  }
  return 0;
}

/* What the window is called, for the one line a share logs: asked once, when the
 * capture is opened, since that line is written once and a retitled window is
 * still the same window. */
static void name_window(yapfast_wgc *capture) {
  WCHAR title[128];
  WCHAR name[64];

  title[0] = 0;
  name[0] = 0;
  if (GetWindowTextW(capture->window, title, ARRAYSIZE(title)) > 0)
    snprintf(capture->window_title, sizeof capture->window_title, "%ls", title);
  if (GetClassNameW(capture->window, name, ARRAYSIZE(name)) > 0)
    snprintf(capture->window_class, sizeof capture->window_class, "%ls", name);
}

/* Opens a capture of `monitor`, an HMONITOR, or of the primary display when it is
 * NULL, or of `target` as a window when `want_window` is set, and starts it: the
 * device (made here, or the one handed in), the frame pool on it, the session,
 * the request that it draw no border, and the subscription to the item ending.
 * `hdr` is the caller's answer to whether the display shows HDR, which decides
 * the format the frames arrive in: the half-float scRGB picture where it does,
 * and BGRA where it doesn't. NULL with `why` filled in when anything refused. */
static yapfast_wgc *open_capture(void *given, int want_window, void *target, int hdr, char *why,
                                 size_t size) {
  yapfast_wgc *capture = NULL;
  DXGI_OUTPUT_DESC output;
  HMONITOR monitor = (HMONITOR)target;
  HMONITOR resolved = NULL;
  ABI::Windows::Graphics::SizeInt32 content;
  IDXGIDevice *dxgi_device = NULL;
  abi::ItemInterop *interop = NULL;
  HSTRING_HEADER header;
  HSTRING name;
  D3D_FEATURE_LEVEL levels[] = {D3D_FEATURE_LEVEL_12_1, D3D_FEATURE_LEVEL_12_0,
                                D3D_FEATURE_LEVEL_11_1, D3D_FEATURE_LEVEL_11_0};
  D3D_FEATURE_LEVEL reached = D3D_FEATURE_LEVEL_11_0;
  HRESULT status;

  if (why && size) why[0] = 0;
  /* The window is checked before Windows Graphics Capture is started, so that a
   * handle for nothing costs a sentence rather than a device and a pool. */
  if (want_window && validate_window((HWND)target, why, size)) return NULL;
  /* WinRT has to be started in this process before any of its classes can be
   * activated. Multi-threaded, because the pool is free-threaded and frames are
   * polled from whichever thread the capture lives on; RPC_E_CHANGED_MODE means
   * the process already started WinRT another way, which is not a failure here,
   * and is not undone on the way out either, since other code is using it. */
  status = RoInitialize(RO_INIT_MULTITHREADED);
  if (FAILED(status) && status != RPC_E_CHANGED_MODE) {
    refused(why, size, "RoInitialize", status);
    return NULL;
  }
  capture = (yapfast_wgc *)calloc(1, sizeof *capture);
  if (!capture) {
    say(why, size, "no memory for a capture");
    return NULL;
  }
  capture->is_window = want_window;
  capture->window = want_window ? (HWND)target : NULL;
  /* The format is the caller's answer to what the display shows, and it is what
   * every pool this capture makes holds: the scRGB picture an HDR screen
   * composes in, or the 8-bit one everything reads. */
  capture->format = hdr ? DXGI_FORMAT_R16G16B16A16_FLOAT : DXGI_FORMAT_B8G8R8A8_UNORM;
  snprintf(capture->pixels, sizeof capture->pixels, "%s",
           capture->format == DXGI_FORMAT_R16G16B16A16_FLOAT
               ? "the frames are the 16-bit float scRGB picture an HDR screen hands over"
               : "the frames are BGRA8, which the encoder reads as they are");
  if (capture->is_window) {
    /* A window is composited on the adapter that drives the display it mostly
     * sits on, so that display answers which card to make the device on — the
     * same question a display's own capture asks, which is why the search is the
     * same one. `MONITOR_DEFAULTTOPRIMARY` is what Windows answers for a window
     * that is over no screen at all, a move in progress say: the composition is
     * still the primary's card's, and asking again per frame would only turn a
     * moment into an error. */
    name_window(capture);
    monitor = MonitorFromWindow(capture->window, MONITOR_DEFAULTTOPRIMARY);
    if (!monitor) {
      say(why, size, "the display the window is on: MonitorFromWindow(%p) found none",
          (void *)capture->window);
      goto failed;
    }
  }
  if (adapter_for_monitor(
          monitor,
          want_window ? "the display the window is on" : "the display being captured",
          &capture->adapter, &output, &capture->luid, &capture->output, capture->card,
          sizeof capture->card, capture->display, sizeof capture->display, &resolved,
          &capture->primary, why, size))
    goto failed;
  /* BGRA_SUPPORT is what Windows Graphics Capture requires of the device it hands
   * BGRA frames to, so it is asked for rather than fallen back from. A device the
   * caller brought can't be made again, so what it was made with is checked
   * instead: either it takes BGRA as a texture, or this refuses in a sentence
   * rather than letting the pool's creation fail later with a status number. */
  if (given) {
    IDXGIAdapter1 *actual = NULL;
    DXGI_ADAPTER_DESC1 desc;
    UINT support = 0;

    capture->device = (ID3D11Device *)given;
    /* Borrowed: see the ownership note at the top of this file. */
    capture->owns_device = 0;
    memset(&desc, 0, sizeof desc);
    status = device_adapter(capture->device, &actual, &desc);
    if (FAILED(status)) {
      refused(why, size, "IDXGIDevice::GetAdapter on the device handed in", status);
      goto failed;
    }
    /* The pool's textures are filled by the compositor from the display's own
     * adapter, so a device elsewhere means every frame is copied across cards:
     * a capture that works and a stream that pays for it on every frame. It is
     * refused with the two cards named instead, which is a call the caller can
     * act on — open on a device made where the display is. */
    if (desc.AdapterLuid.HighPart != capture->luid.HighPart ||
        desc.AdapterLuid.LowPart != capture->luid.LowPart) {
      say(why, size,
          "the device handed in was made on %ls (LUID %08lX:%08lX), and this display is on %s "
          "(LUID %08lX:%08lX): a capture has to be opened on a device made where the display is, "
          "or every frame is copied between cards",
          desc.Description, (unsigned long)desc.AdapterLuid.HighPart,
          (unsigned long)desc.AdapterLuid.LowPart, capture->card,
          (unsigned long)capture->luid.HighPart, (unsigned long)capture->luid.LowPart);
      actual->Release();
      goto failed;
    }
    actual->Release();
    status = capture->device->CheckFormatSupport(DXGI_FORMAT_B8G8R8A8_UNORM, &support);
    if (FAILED(status) || !(support & D3D11_FORMAT_SUPPORT_TEXTURE2D)) {
      say(why, size,
          "the device handed in can't hold BGRA textures (0x%08lX, formats 0x%08lX), which is "
          "the only format Windows Graphics Capture delivers; a device made with "
          "D3D11_CREATE_DEVICE_BGRA_SUPPORT can",
          (unsigned long)status, (unsigned long)support);
      goto failed;
    }
    /* The float picture is held by the same device, and a device that can't hold
     * it is not a reason to refuse the capture: the 8-bit picture is asked for
     * instead, and the sentence the report carries names what refused. */
    if (capture->format == DXGI_FORMAT_R16G16B16A16_FLOAT) {
      status = capture->device->CheckFormatSupport(DXGI_FORMAT_R16G16B16A16_FLOAT, &support);
      if (FAILED(status) || !(support & D3D11_FORMAT_SUPPORT_TEXTURE2D)) {
        snprintf(capture->pixels, sizeof capture->pixels,
                 "the screen shows HDR, and the device handed in can't hold the 16-bit float "
                 "picture (CheckFormatSupport, 0x%08lX, formats 0x%08lX): the frames are BGRA8, "
                 "which Windows tone-maps itself",
                 (unsigned long)status, (unsigned long)support);
        capture->format = DXGI_FORMAT_B8G8R8A8_UNORM;
      }
    }
  } else {
    status = D3D11CreateDevice((IDXGIAdapter *)capture->adapter, D3D_DRIVER_TYPE_UNKNOWN, NULL,
                               D3D11_CREATE_DEVICE_BGRA_SUPPORT, levels, ARRAYSIZE(levels),
                               D3D11_SDK_VERSION, &capture->device, &reached, &capture->context);
    if (FAILED(status)) {
      say(why, size, "D3D11CreateDevice on %s refused (0x%08lX)", capture->card,
          (unsigned long)status);
      goto failed;
    }
    /* Made here, so released by `yapfast_wgc_close` and nowhere else. */
    capture->owns_device = 1;
  }
  if (!capture->context) capture->device->GetImmediateContext(&capture->context);
  status = capture->device->QueryInterface(IID_PPV_ARGS(&dxgi_device));
  if (FAILED(status)) {
    refused(why, size, "QueryInterface(IDXGIDevice)", status);
    goto failed;
  }
  status = CreateDirect3D11DeviceFromDXGIDevice(dxgi_device, &capture->graphics_device);
  dxgi_device->Release();
  if (FAILED(status)) {
    refused(why, size, "CreateDirect3D11DeviceFromDXGIDevice", status);
    goto failed;
  }
  status = WindowsCreateStringReference(
      L"Windows.Graphics.Capture.GraphicsCaptureItem",
      (UINT32)(ARRAYSIZE(L"Windows.Graphics.Capture.GraphicsCaptureItem") - 1), &header, &name);
  if (FAILED(status)) {
    refused(why, size, "WindowsCreateStringReference(GraphicsCaptureItem)", status);
    goto failed;
  }
  /* The activation factory itself carries the interop interface: this is the door
   * that turns a handle into a capture item, and it is not part of the WinRT
   * projection. */
  status = RoGetActivationFactory(name, __uuidof(abi::ItemInterop), (void **)&interop);
  if (FAILED(status)) {
    say(why, size,
        "this build of Windows has no Windows Graphics Capture: its activation factory refused "
        "IGraphicsCaptureItemInterop (0x%08lX)",
        (unsigned long)status);
    goto failed;
  }
  /* The door the target goes through, and the only place the two kinds are told
   * apart: a display's HMONITOR through CreateForMonitor, a window's HWND through
   * CreateForWindow. Nothing here tries the other door when one refuses, since a
   * window is not a display and the screen behind it is not what was asked for. */
  if (want_window)
    status = interop->CreateForWindow(capture->window, IID_PPV_ARGS(&capture->item));
  else
    status = interop->CreateForMonitor(resolved, IID_PPV_ARGS(&capture->item));
  interop->Release();
  if (FAILED(status)) {
    refused(why, size,
            want_window ? "IGraphicsCaptureItemInterop::CreateForWindow"
                        : "IGraphicsCaptureItemInterop::CreateForMonitor",
            status);
    goto failed;
  }
  capture->item->get_Size(&content);
  if (content.Width <= 0 || content.Height <= 0) {
    say(why, size, "the capture item reports no size (%ldx%ld)", (long)content.Width,
        (long)content.Height);
    goto failed;
  }
  capture->width = (unsigned)content.Width;
  capture->height = (unsigned)content.Height;
  status = make_pool(capture, content);
  if (FAILED(status)) {
    refused(why, size, "Direct3D11CaptureFramePool::CreateFreeThreaded", status);
    goto failed;
  }
  status = capture->pool->CreateCaptureSession(capture->item, &capture->session);
  if (FAILED(status)) {
    refused(why, size, "CreateCaptureSession", status);
    goto failed;
  }
  ask_borderless(capture);
  include_pointer(capture);
  /* Watched before it starts, so that an item which closes on its first frame is
   * reported as closed rather than as a capture that never produced one. */
  watch_closed(capture);
  status = capture->session->StartCapture();
  if (FAILED(status)) {
    say(why, size, "StartCapture refused (0x%08lX)%s", (unsigned long)status,
        ended(capture) ? " after the item closed" : "");
    goto failed;
  }
  return capture;

failed:
  /* Whatever was opened is let go here, and `why` already says what refused: a
   * caller that gets NULL has the reason in the buffer it passed, which is the
   * shape of every other shim's failure in native/. */
  yapfast_wgc_close(capture);
  return NULL;
}

/* Opens a capture of `monitor` with a device of its own, made on the adapter
 * that drives that display, `hdr` asking for the display's own HDR picture rather
 * than SDR's. NULL with `why` filled in when anything refused. */
extern "C" yapfast_wgc *yapfast_wgc_open(void *monitor, int hdr, char *why, size_t size) {
  return open_capture(NULL, 0, monitor, hdr, why, size);
}

/* Opens a capture of `monitor` on `device`, a D3D11 device the caller already
 * has, so that the pool's textures are textures that device reads — the one
 * thing that lets a captured frame reach an encoder with no copy between. The
 * device is borrowed: this does not AddRef it and `yapfast_wgc_close` does not
 * release it, so the caller keeps it alive for at least as long as the capture.
 *
 * It has to be made on the adapter that drives the display being captured, and
 * with `D3D11_CREATE_DEVICE_BGRA_SUPPORT`; both are checked here and refused by
 * name, since either would otherwise show up as a pool that wouldn't be created
 * or as a frame that encodes as noise. NULL with `why` filled in when anything
 * refused. */
extern "C" yapfast_wgc *yapfast_wgc_open_on(void *device, void *monitor, int hdr, char *why,
                                            size_t size) {
  return open_capture(device, 0, monitor, hdr, why, size);
}

/* Opens a capture of one window, `window` being that window's own HWND, with a
 * device made on the adapter that drives the display the window is composited
 * on. The window is checked first: one that has gone, one that isn't a top-level
 * window, and the desktop itself are each refused with the reason in `why`, since
 * a caller that asked for one window must never be handed a picture of its
 * screen. NULL with `why` filled in when anything refused. */
extern "C" yapfast_wgc *yapfast_wgc_open_window(void *window, int hdr, char *why, size_t size) {
  return open_capture(NULL, 1, window, hdr, why, size);
}

/* Opens a capture of one window on `device`, borrowed exactly as
 * `yapfast_wgc_open_on` borrows it and checked the same way: a device made on
 * another card than the window's display is refused by name rather than copying
 * every frame between cards. The window is checked the same way as above. */
extern "C" yapfast_wgc *yapfast_wgc_open_window_on(void *device, void *window, int hdr, char *why,
                                                   size_t size) {
  return open_capture(device, 1, window, hdr, why, size);
}

/* The next frame, if the compositor has made one. */
extern "C" int yapfast_wgc_frame(yapfast_wgc *capture, void **texture, char *why, size_t size) {
  capture_abi::IDirect3D11CaptureFrame *frame = NULL;
  direct3d11_abi::IDirect3DSurface *surface = NULL;
  abi::SurfaceAccess *access = NULL;
  ID3D11Texture2D *found = NULL;
  ABI::Windows::Graphics::SizeInt32 content;
  HRESULT status;

  if (why && size) why[0] = 0;
  if (!capture || !texture) {
    say(why, size, "no capture to ask for a frame");
    return YAPFAST_WGC_FAILED;
  }
  *texture = NULL;
  /* The frame handed out last time stays the caller's until a new one comes, not
   * merely until this call: a desktop that hasn't changed has no frame to hand
   * over, and a caller with a viewer joining, a decoder that lost its place or a
   * preview coming back still has to send that picture. Its buffer stays out of
   * the pool while the frame is held, which is one of the two the pool was asked
   * for. It is let go below, when `TryGetNextFrame` has answered with a frame of
   * its own. */
  if (ended(capture)) {
    snprintf(capture->ending, sizeof capture->ending, "%s",
             capture->is_window ? "the window closed" : "the display capture ended");
    say(why, size, "the capture is over: %s", capture->ending);
    return YAPFAST_WGC_CLOSED;
  }
  if (capture->is_window) {
    /* Three questions a display has no answer to, asked before the pool is
     * touched. A window that is gone and a window that has excluded itself both
     * end the capture, because the pool still holds the last picture from before
     * and handing that over again is the stale or protected content this exists to
     * stop refreshing; a minimised window composites nothing, so it is answered as
     * nothing new, which the caller sleeps on. These are three cheap window calls
     * per frame and only on this path; the display path asks nothing.
     */
    if (!IsWindow(capture->window)) {
      snprintf(capture->ending, sizeof capture->ending,
               "the window closed: window %p is not a window any more",
               (void *)capture->window);
      say(why, size, "the capture is over: %s", capture->ending);
      return YAPFAST_WGC_CLOSED;
    }
    if (excluded_from_capture(capture->window)) {
      snprintf(capture->ending, sizeof capture->ending,
               "the window is out of every capture (WDA_EXCLUDEFROMCAPTURE), so no picture of "
               "it can be had while it stays set that way");
      say(why, size, "the capture is over: %s", capture->ending);
      return YAPFAST_WGC_CLOSED;
    }
    /* Asked before the pool, not after: a compositor that thinks a minimised
     * window is still worth a frame hands over a thumbnail at the window's own
     * size, which reads as a resize per frame and rebuilds the pool for it. */
    if (IsIconic(capture->window)) return YAPFAST_WGC_IDLE;
  }
  status = capture->pool->TryGetNextFrame(&frame);
  if (FAILED(status)) {
    refused(why, size, "TryGetNextFrame", status);
    return YAPFAST_WGC_FAILED;
  }
  if (!frame) return YAPFAST_WGC_IDLE;
  /* A frame came, so the one handed out last time goes back: the pool reuses its
   * buffer for the frame after this one, and this is the only place the shim
   * lets go of one. The texture goes with it: `GetInterface` hands out a
   * reference of its own, so the frame holding the surface does not cover it — a
   * release per frame never given back left the pool's buffer climbing and
   * stranded a texture set on every resize. */
  if (capture->texture) {
    capture->texture->Release();
    capture->texture = NULL;
  }
  if (capture->frame) {
    capture->frame->Release();
    capture->frame = NULL;
  }
  status = frame->get_ContentSize(&content);
  if (FAILED(status)) {
    frame->Release();
    refused(why, size, "a frame's ContentSize", status);
    return YAPFAST_WGC_FAILED;
  }
  if (content.Width <= 0 || content.Height <= 0) {
    frame->Release();
    /* A window caught between sizes, or on its way down, reports an empty picture
     * for a moment: nothing to hand over, and no damage either. A display has no
     * such moment, so an empty frame there is still worth saying. */
    if (capture->is_window) return YAPFAST_WGC_IDLE;
    say(why, size, "a frame's ContentSize is %ldx%ld", (long)content.Width,
        (long)content.Height);
    return YAPFAST_WGC_FAILED;
  }
  if ((unsigned)content.Width != capture->width || (unsigned)content.Height != capture->height) {
    /* The thing captured was resized — a window dragged to a new size, or a
     * display whose mode changed — and the pool still holds buffers at the old
     * size: it is recreated at the new one, and the frame that reported the change
     * is dropped, since a caller reading it would be reading a texture at
     * dimensions its encoder has already been given. Recreate rather than a new
     * pool, because the session is bound to this one. */
    capture->width = (unsigned)content.Width;
    capture->height = (unsigned)content.Height;
    status = capture->pool->Recreate((direct3d11_abi::IDirect3DDevice *)capture->graphics_device,
                                     pixel_format(capture->format), 2, content);
    frame->Release();
    if (FAILED(status)) {
      refused(why, size, "Direct3D11CaptureFramePool::Recreate", status);
      return YAPFAST_WGC_FAILED;
    }
    return YAPFAST_WGC_RESIZED;
  }
  status = frame->get_Surface(&surface);
  if (FAILED(status)) {
    frame->Release();
    refused(why, size, "a frame's Surface", status);
    return YAPFAST_WGC_FAILED;
  }
  /* The surface is a WinRT object around the frame's texture; the interface that
   * gives the texture itself is the one the SDK doesn't project, and is why
   * nothing has to be copied on the way out of the compositor. */
  status = surface->QueryInterface(IID_PPV_ARGS(&access));
  surface->Release();
  if (FAILED(status)) {
    frame->Release();
    refused(why, size, "QueryInterface(IDirect3DDxgiInterfaceAccess)", status);
    return YAPFAST_WGC_FAILED;
  }
  status = access->GetInterface(IID_PPV_ARGS(&found));
  access->Release();
  if (FAILED(status)) {
    frame->Release();
    say(why, size, "a frame's surface isn't an ID3D11Texture2D this device can read (0x%08lX)",
        (unsigned long)status);
    return YAPFAST_WGC_FAILED;
  }
  /* Kept together: the frame owns the surface, which owns the texture, so the
   * texture is only valid while both are held — and the caller borrows it until
   * the next frame. The reference `GetInterface` gave here is the one released
   * when the next frame comes and in `yapfast_wgc_close`. */
  capture->frame = frame;
  capture->texture = found;
  *texture = found;
  return YAPFAST_WGC_FRAME;
}

/* The size the capture is at now, which changes when what is being captured is
 * resized: the shim has already recreated the pool at the new size by the time
 * `yapfast_wgc_frame` answers YAPFAST_WGC_RESIZED, so this is what a caller's
 * encoder should be built for. */
extern "C" void yapfast_wgc_content_size(yapfast_wgc *capture, unsigned *width, unsigned *height) {
  if (width) *width = capture ? capture->width : 0;
  if (height) *height = capture ? capture->height : 0;
}

/* The ID3D11Device the frames' textures are on, as an `ID3D11Device *`: the one
 * this capture made, or the one it was opened on, which is what an encoder
 * opens its session on so that a frame's texture is a texture it can read. It
 * belongs to this capture when the capture made it and to whoever made it
 * otherwise; releasing it is that owner's business, and never a reader's. */
extern "C" void *yapfast_wgc_device(yapfast_wgc *capture) {
  return capture ? (void *)capture->device : NULL;
}

/* Whether this capture's frames are the 16-bit float scRGB picture an HDR screen
 * hands over, rather than BGRA8: what a caller that encodes them has to know to
 * tone-map them on the card. It is not what was asked for — it is what the pool
 * took, so an HDR screen whose driver refused the float picture answers 0 here
 * and says in `yapfast_wgc_describe` what refused it. */
extern "C" int yapfast_wgc_hdr(yapfast_wgc *capture) {
  return capture && capture->format == DXGI_FORMAT_R16G16B16A16_FLOAT;
}

/* Whether this capture's yellow border is actually off: 1 only when
 * IsBorderRequired(false) was asked and took, and 0 for every other answer — a
 * Windows 10 whose session has no such interface, a managed machine that
 * refused the request, and a capture still keeping the interface while it said
 * no. This is what a caller decides with; the sentence `yapfast_wgc_describe`
 * carries is for the person reading the log. */
extern "C" int yapfast_wgc_borderless(yapfast_wgc *capture) {
  return capture ? capture->borderless : 0;
}

/* Whether this build of Windows has the interface that can ask at all:
 * IGraphicsCaptureSession3, from Windows 11 22H2. A capture on a build without
 * it draws the indicator and nothing can change that, which is a difference
 * from a machine that could ask and was refused. */
extern "C" int yapfast_wgc_can_ask_border(yapfast_wgc *capture) {
  return capture ? capture->can_ask_borderless : 0;
}

/* The border's answer as the clause `yapfast_wgc_describe` includes, so that a
 * share which lets this capture go for another way can quote the reason in its
 * own words rather than in a sentence of its own. */
extern "C" void yapfast_wgc_border(yapfast_wgc *capture, char *text, size_t size) {
  if (!text || !size) return;
  if (!capture) {
    snprintf(text, size, "no capture");
    return;
  }
  snprintf(text, size, "%s", capture->border);
}

/* The build number of the Windows this process runs on, which is whether
 * Graphics Capture has the interface that can take its border off; see
 * `windows_build`. */
extern "C" unsigned yapfast_wgc_build(void) { return windows_build(); }

/* Why the capture ended, when it did, as the sentence `yapfast_wgc_frame` wrote
 * for the YAPFAST_WGC_CLOSED it answered. Both answers mean the same thing to a
 * caller — stop, drop this capture — but a share that says why it stopped has to
 * tell a window its owner closed from a window that left every capture on
 * purpose, and they are different things to a person reading the log. Empty while
 * the capture still runs. */
extern "C" void yapfast_wgc_ending(yapfast_wgc *capture, char *text, size_t size) {
  if (!text || !size) return;
  if (!capture) {
    snprintf(text, size, "no capture");
    return;
  }
  snprintf(text, size, "%s", capture->ending);
}

/* What this capture is and how it went, in one line: which display or which
 * window, the card and output it is on, the size, and whether the border is off
 * and what it took. A capture that draws the indicator is still a capture, so the
 * border's answer is reported here rather than failed, and a window's item is
 * exactly as subject to it as a display's. */
extern "C" void yapfast_wgc_describe(yapfast_wgc *capture, char *text, size_t size) {
  if (!text || !size) return;
  if (!capture) {
    snprintf(text, size, "no capture");
    return;
  }
  if (capture->is_window) {
    snprintf(text, size,
             "the window \"%s\" (class %s, window %p) on %s, %ux%u, through %s (LUID "
             "%08lX:%08lX, output %u); %s; %s; %s",
             capture->window_title[0] ? capture->window_title : "(untitled)",
             capture->window_class[0] ? capture->window_class : "(an unnamed class)",
             (void *)capture->window,
             capture->display[0] ? capture->display : "(an unnamed display)", capture->width,
             capture->height, capture->card[0] ? capture->card : "(an unnamed adapter)",
             (unsigned long)capture->luid.HighPart, (unsigned long)capture->luid.LowPart,
             capture->output, capture->border, capture->pointer, capture->pixels);
    return;
  }
  snprintf(text, size, "%s %s, %ux%u, through %s (LUID %08lX:%08lX, output %u); %s; %s; %s",
           capture->primary ? "the primary display" : "a display",
           capture->display[0] ? capture->display : "(unnamed)", capture->width, capture->height,
           capture->card[0] ? capture->card : "(an unnamed adapter)",
           (unsigned long)capture->luid.HighPart, (unsigned long)capture->luid.LowPart,
           capture->output, capture->border, capture->pointer, capture->pixels);
}

/* Stops the capture and lets everything go. A capture whose end has already come
 * is the ordinary way a window capture finishes, so an item that has closed, a
 * session that has stopped and a frame that was already released are all what
 * this expects to find; being called twice is the one thing it doesn't survive,
 * which Rust's wrapper is what prevents. */
extern "C" void yapfast_wgc_close(yapfast_wgc *capture) {
  if (!capture) return;
  if (capture->texture) {
    capture->texture->Release();
    capture->texture = NULL;
  }
  if (capture->frame) {
    capture->frame->Release();
    capture->frame = NULL;
  }
  if (InterlockedExchange(&capture->watching, 0) == 1 && capture->item)
    capture->item->remove_Closed(capture->closed_token);
  if (capture->handler) {
    capture->handler->Release();
    capture->handler = NULL;
  }
  if (capture->session) {
    ABI::Windows::Foundation::IClosable *closable = NULL;
    /* The session's own end: a capture that stopped because its item closed
     * refuses this, which is what "tolerating a session that already ended"
     * means and not a reason to say anything. */
    if (SUCCEEDED(capture->session->QueryInterface(IID_PPV_ARGS(&closable)))) {
      closable->Close();
      closable->Release();
    }
    capture->session->Release();
  }
  if (capture->pool) capture->pool->Release();
  if (capture->item) capture->item->Release();
  if (capture->graphics_device) capture->graphics_device->Release();
  if (capture->context) capture->context->Release();
  /* Only what this capture made: a device handed to `yapfast_wgc_open_on` is the
   * caller's, and releasing it here would take it out from under the encoder
   * sharing it. */
  if (capture->owns_device && capture->device) capture->device->Release();
  if (capture->adapter) capture->adapter->Release();
  free(capture);
}
