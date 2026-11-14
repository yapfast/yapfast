/* Direct3D 11's video processor as the bridge between a capture and an encoder
 * that reads surfaces: BGRA at the capture's size in, NV12 at the stream's size
 * out, both on the card, with no pixel reaching the processor.
 *
 * Quick Sync and AMF read a Direct3D 11 surface through FFmpeg's own encoders
 * (`AV_PIX_FMT_QSV`, `AV_PIX_FMT_D3D11`), and a frames context with no pool hands
 * them the surface the caller names rather than one they allocated. What the
 * caller has is not what they read: Desktop Duplication and Windows Graphics
 * Capture fill a BGRA texture the size of the screen or window, where a session
 * reads NV12 at the size Discord takes. So this runs the card's fixed-function
 * video processor between them once a frame, and the encoder reads the texture it
 * writes — `yapfast_d3d11conv_converted` counts those passes and
 * `yapfast_d3d11conv_copied` answers zero, since no picture crosses a bus.
 *
 * One output texture is made per size and kept: a stream's size changes when the
 * tile showing it does, not per frame. The input view is made per frame, because
 * the capture hands over a different texture each time, and released before the
 * call returns — the blit has already taken what it needs, and Direct3D orders
 * the write before the encoder's read on the same immediate context.
 *
 * A converter opened for an HDR screen does one thing more with the same blit: it
 * tells the processor that its input is scRGB's linear light and that its output
 * is the BT.709 the stream declares, which is what makes the conversion the tone
 * map. That is why this is the interface the HDR path goes through on a card
 * whose encoder reads surfaces — Quick Sync's and AMF's — as well as on NVIDIA's.
 */
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <d3d11.h>
/* D3D11.1's video context, which is the one that names a colour space by its DXGI
 * name rather than by the four flags D3D11's own colour space structure holds. */
#include <d3d11_1.h>
#include <stdarg.h>
#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "d3d11convert.h"

struct yapfast_d3d11conv {
  /* Borrowed: the caller keeps the device alive for the converter's life. */
  ID3D11Device *device;
  /* Made here, from the device's immediate context, and released here. */
  ID3D11DeviceContext *context;
  ID3D11VideoDevice *video;
  ID3D11VideoContext *video_context;
  /* D3D11.1's context, asked for by a converter opened for an HDR screen: the only
   * one that can say what the input and the output really are. */
  ID3D11VideoContext1 *video_context1;
  ID3D11VideoProcessorEnumerator *enumerator;
  ID3D11VideoProcessor *processor;
  /* The picture this wrote last: NV12 at the stream's size, one per size. */
  ID3D11Texture2D *output;
  ID3D11VideoProcessorOutputView *output_view;
  unsigned from_width, from_height, to_width, to_height;
  /* Whether the pictures come in as the half-float scRGB an HDR screen hands over,
   * which the colour spaces below turn this blit into a tone map for. */
  int hdr;
  unsigned long long converted;
  char error[192];
};

static void put(char *buffer, size_t size, const char *format, ...) {
  if (buffer == NULL || size == 0) return;
  va_list arguments;
  va_start(arguments, format);
  vsnprintf(buffer, size, format, arguments);
  va_end(arguments);
}

static void failed(struct yapfast_d3d11conv *conv, const char *what, HRESULT status) {
  put(conv->error, sizeof conv->error, "%s (0x%08lX)", what, (unsigned long)status);
}

/* The NV12 texture the processor writes into, and the view of it the blit names. */
static int make_output(struct yapfast_d3d11conv *conv) {
  D3D11_TEXTURE2D_DESC desc;
  memset(&desc, 0, sizeof desc);
  desc.Width = conv->to_width;
  desc.Height = conv->to_height;
  desc.MipLevels = 1;
  desc.ArraySize = 1;
  /* NV12 is a video format: it is a render target and nothing else. */
  desc.Format = DXGI_FORMAT_NV12;
  desc.SampleDesc.Count = 1;
  desc.Usage = D3D11_USAGE_DEFAULT;
  desc.BindFlags = D3D11_BIND_RENDER_TARGET;
  HRESULT status =
      conv->device->lpVtbl->CreateTexture2D(conv->device, &desc, NULL, &conv->output);
  if (FAILED(status) || conv->output == NULL) {
    conv->output = NULL;
    failed(conv, "the card wouldn't make an NV12 texture for the stream's size", status);
    return 0;
  }
  D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC view;
  memset(&view, 0, sizeof view);
  view.ViewDimension = D3D11_VPOV_DIMENSION_TEXTURE2D;
  view.Texture2D.MipSlice = 0;
  status = conv->video->lpVtbl->CreateVideoProcessorOutputView(
      conv->video, (ID3D11Resource *)conv->output, conv->enumerator, &view, &conv->output_view);
  if (FAILED(status) || conv->output_view == NULL) {
    conv->output_view = NULL;
    failed(conv, "the card wouldn't make a view of the NV12 texture", status);
    return 0;
  }
  return 1;
}

/* Says what the picture is and what the frame it becomes is, which is the whole of
 * the tone map: Windows Graphics Capture hands over scRGB for an HDR screen
 * (linear light, BT.709's primaries, 1.0 at 80 nits), and what an encoder reads
 * here is NV12 in BT.709's studio range, which is the claim the stream declares.
 * The output is named as YCbCr rather than as RGB because the texture this blit
 * writes is NV12: a processor told its output is RGB writes the samples a
 * different way, and the encoder would read a picture skewed towards green.
 *
 * Both calls answer nothing, so each is read back: a processor that kept its
 * defaults would convert linear light as if it were sRGB, which is a frame that
 * looks plausible and isn't, and a caller told the converter won't is the better
 * answer. */
static int colour_spaces(struct yapfast_d3d11conv *conv) {
  DXGI_COLOR_SPACE_TYPE wanted = DXGI_COLOR_SPACE_RGB_FULL_G10_NONE_P709;
  DXGI_COLOR_SPACE_TYPE shown = DXGI_COLOR_SPACE_YCBCR_STUDIO_G22_LEFT_P709;
  HRESULT status = conv->context->lpVtbl->QueryInterface(
      conv->context, &IID_ID3D11VideoContext1, (void **)&conv->video_context1);
  if (FAILED(status) || conv->video_context1 == NULL) {
    conv->video_context1 = NULL;
    failed(conv, "the capture's device has no video context that names a colour space", status);
    return 0;
  }
  conv->video_context1->lpVtbl->VideoProcessorSetStreamColorSpace1(
      conv->video_context1, conv->processor, 0, wanted);
  wanted = DXGI_COLOR_SPACE_CUSTOM;
  conv->video_context1->lpVtbl->VideoProcessorGetStreamColorSpace1(
      conv->video_context1, conv->processor, 0, &wanted);
  if (wanted != DXGI_COLOR_SPACE_RGB_FULL_G10_NONE_P709) {
    put(conv->error, sizeof conv->error,
        "the card wouldn't take scRGB as what its input is (VideoProcessorSetStreamColorSpace1, "
        "it says %u)",
        (unsigned)wanted);
    return 0;
  }
  conv->video_context1->lpVtbl->VideoProcessorSetOutputColorSpace1(
      conv->video_context1, conv->processor, shown);
  shown = DXGI_COLOR_SPACE_CUSTOM;
  conv->video_context1->lpVtbl->VideoProcessorGetOutputColorSpace1(conv->video_context1,
                                                                  conv->processor, &shown);
  if (shown != DXGI_COLOR_SPACE_YCBCR_STUDIO_G22_LEFT_P709) {
    put(conv->error, sizeof conv->error,
        "the card wouldn't take BT.709's studio range as what its output is "
        "(VideoProcessorSetOutputColorSpace1, it says %u)",
        (unsigned)shown);
    return 0;
  }
  return 1;
}

struct yapfast_d3d11conv *yapfast_d3d11conv_new(void *device, unsigned from_width,
                                               unsigned from_height, unsigned to_width,
                                               unsigned to_height, int hdr, char *why,
                                               size_t size) {
  put(why, size, "not opened");
  struct yapfast_d3d11conv *conv = calloc(1, sizeof *conv);
  if (conv == NULL) return NULL;
  conv->device = (ID3D11Device *)device;
  conv->from_width = from_width;
  conv->from_height = from_height;
  conv->to_width = to_width & ~1u;
  conv->to_height = to_height & ~1u;
  conv->hdr = hdr ? 1 : 0;
  /* 4:2:0 is coded in pairs of lines and pixels: a size that isn't even has no
   * NV12 texture, and the stream's own size always is. */
  if (conv->to_width < 2 || conv->to_height < 2) {
    put(conv->error, sizeof conv->error, "a stream size of %ux%u has no NV12 texture", to_width,
        to_height);
    goto failed;
  }
  conv->device->lpVtbl->GetImmediateContext(conv->device, &conv->context);
  if (conv->context == NULL) {
    put(conv->error, sizeof conv->error, "the capture's device has no immediate context");
    goto failed;
  }
  if (FAILED(conv->context->lpVtbl->QueryInterface(conv->context, &IID_ID3D11VideoContext,
                                                   (void **)&conv->video_context)) ||
      conv->video_context == NULL) {
    put(conv->error, sizeof conv->error,
        "the capture's device has no video processor to convert a picture with");
    goto failed;
  }
  if (FAILED(conv->device->lpVtbl->QueryInterface(conv->device, &IID_ID3D11VideoDevice,
                                                  (void **)&conv->video)) ||
      conv->video == NULL) {
    put(conv->error, sizeof conv->error,
        "the capture's device has no video processor to convert a picture with");
    goto failed;
  }
  D3D11_VIDEO_PROCESSOR_CONTENT_DESC content;
  memset(&content, 0, sizeof content);
  content.InputFrameRate.Numerator = 30;
  content.InputFrameRate.Denominator = 1;
  content.OutputFrameRate.Numerator = 30;
  content.OutputFrameRate.Denominator = 1;
  content.Usage = D3D11_VIDEO_USAGE_PLAYBACK_NORMAL;
  content.InputWidth = conv->from_width;
  content.InputHeight = conv->from_height;
  content.OutputWidth = conv->to_width;
  content.OutputHeight = conv->to_height;
  HRESULT status = conv->video->lpVtbl->CreateVideoProcessorEnumerator(conv->video, &content,
                                                                      &conv->enumerator);
  if (FAILED(status) || conv->enumerator == NULL) {
    conv->enumerator = NULL;
    failed(conv, "the card wouldn't enumerate a video processor for these sizes", status);
    goto failed;
  }
  status = conv->video->lpVtbl->CreateVideoProcessor(conv->video, conv->enumerator, 0,
                                                     &conv->processor);
  if (FAILED(status) || conv->processor == NULL) {
    conv->processor = NULL;
    failed(conv, "the card wouldn't make a video processor", status);
    goto failed;
  }
  /* Whole pictures, scaled into the stream's frame: what a capture hands over and
   * what an encoder reads are both progressive. */
  conv->video_context->lpVtbl->VideoProcessorSetStreamFrameFormat(
      conv->video_context, conv->processor, 0, D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE);
  RECT whole;
  whole.left = 0;
  whole.top = 0;
  whole.right = (LONG)conv->to_width;
  whole.bottom = (LONG)conv->to_height;
  conv->video_context->lpVtbl->VideoProcessorSetStreamDestRect(conv->video_context,
                                                              conv->processor, 0, TRUE, &whole);
  if (conv->hdr && !colour_spaces(conv)) {
    put(why, size, "%s", conv->error);
    yapfast_d3d11conv_free(conv);
    return NULL;
  }
  if (!make_output(conv)) {
    put(why, size, "%s", conv->error);
    yapfast_d3d11conv_free(conv);
    return NULL;
  }
  return conv;
failed:
  put(why, size, "%s", conv->error[0] ? conv->error : "the converter wasn't opened");
  yapfast_d3d11conv_free(conv);
  return NULL;
}

void *yapfast_d3d11conv_run(struct yapfast_d3d11conv *conv, void *texture, unsigned subresource,
                            char *why, size_t size) {
  if (conv == NULL || texture == NULL) {
    put(why, size, "no converter");
    return NULL;
  }
  conv->error[0] = 0;
  D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC desc;
  memset(&desc, 0, sizeof desc);
  desc.FourCC = 0;
  desc.ViewDimension = D3D11_VPIV_DIMENSION_TEXTURE2D;
  desc.Texture2D.MipSlice = 0;
  desc.Texture2D.ArraySlice = subresource;
  ID3D11VideoProcessorInputView *input = NULL;
  HRESULT status = conv->video->lpVtbl->CreateVideoProcessorInputView(
      conv->video, (ID3D11Resource *)texture, conv->enumerator, &desc, &input);
  if (FAILED(status) || input == NULL) {
    failed(conv, "the card wouldn't read the capture's texture as a video processor's input",
           status);
    put(why, size, "%s", conv->error);
    return NULL;
  }
  D3D11_VIDEO_PROCESSOR_STREAM stream;
  memset(&stream, 0, sizeof stream);
  stream.Enable = TRUE;
  stream.OutputIndex = 0;
  stream.InputFrameOrField = 0;
  stream.pInputSurface = input;
  /* One output frame, one stream, no history: what a capture hands over and what
   * an encoder reads are both whole pictures. */
  status = conv->video_context->lpVtbl->VideoProcessorBlt(conv->video_context, conv->processor,
                                                          conv->output_view, 0, 1, &stream);
  input->lpVtbl->Release(input);
  if (FAILED(status)) {
    failed(conv, "the card's video processor wouldn't convert the picture", status);
    put(why, size, "%s", conv->error);
    return NULL;
  }
  conv->converted++;
  return conv->output;
}

unsigned long long yapfast_d3d11conv_converted(struct yapfast_d3d11conv *conv) {
  return conv == NULL ? 0 : conv->converted;
}

/* No picture is copied: the one buffer is the card's, written by its processor. */
unsigned long long yapfast_d3d11conv_copied(struct yapfast_d3d11conv *conv) {
  (void)conv;
  return 0;
}

const char *yapfast_d3d11conv_error(struct yapfast_d3d11conv *conv) {
  return conv == NULL ? "not opened" : conv->error;
}

void yapfast_d3d11conv_free(struct yapfast_d3d11conv *conv) {
  if (conv == NULL) return;
  if (conv->video_context1 != NULL) conv->video_context1->lpVtbl->Release(conv->video_context1);
  if (conv->output_view != NULL) conv->output_view->lpVtbl->Release(conv->output_view);
  if (conv->output != NULL) conv->output->lpVtbl->Release(conv->output);
  if (conv->processor != NULL) conv->processor->lpVtbl->Release(conv->processor);
  if (conv->enumerator != NULL) conv->enumerator->lpVtbl->Release(conv->enumerator);
  if (conv->video_context != NULL) conv->video_context->lpVtbl->Release(conv->video_context);
  if (conv->video != NULL) conv->video->lpVtbl->Release(conv->video);
  /* The immediate context was had from the device, so it is given back to it. */
  if (conv->context != NULL) conv->context->lpVtbl->Release(conv->context);
  free(conv);
}
