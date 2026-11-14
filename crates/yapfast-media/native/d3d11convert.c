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
 * A converter opened for an HDR screen does one thing more before the same blit:
 * a shader tone-maps the half-float picture Windows Graphics Capture hands over
 * into BGRA at the stream's size, and the processor turns that into the NV12 the
 * encoder reads. The curve is the one native/d3d11enc.c scales an NVENC picture
 * with — native/tone_hlsl.h, spliced into both shaders — so a share of an HDR
 * screen looks the same whichever of the three encoders sends it. The processor
 * cannot do it alone: its colour-space conversion names the transfer curve on
 * either side and nothing else, with no white point to divide by and no shoulder
 * to roll off, so scRGB's 1.0 at 80 nits would arrive at white and every window
 * would go out three times too bright. With the shader's BGRA in hand, the
 * processor's side of the blit is a conversion and not a tone map, so it is told
 * sRGB codes in and BT.601's studio range out — the matrix the stream declares and
 * the one NVENC converts a BGRA texture with.
 *
 * The HDR path costs one more texture per converter, the BGRA picture at the
 * stream's size, and one shader pass a frame; the SDR path is the blit it always
 * was, byte for byte.
 */
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <d3d11.h>
/* D3D11.1's video context, which is the one that names a colour space by its DXGI
 * name rather than by the four flags D3D11's own colour space structure holds. */
#include <d3d11_1.h>
/* The shader that tone-maps is compiled when the converter opens, so that a card
 * that can't says so in words rather than a frame into a stream. */
#include <d3dcompiler.h>
#include <stdarg.h>
#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "d3d11convert.h"
/* The tone map an HDR screen's picture goes through, in HLSL, shared with
 * native/d3d11enc.c so that the two shims that draw it draw one curve. */
#include "tone_hlsl.h"

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
  /* The tone map's destination, for an HDR screen: BGRA at the stream's size,
   * which the shader below fills and the processor then reads as its input. It
   * is made once per converter, like the NV12 texture it is turned into. */
  ID3D11Texture2D *toned;
  ID3D11RenderTargetView *toned_view;
  ID3D11VideoProcessorInputView *toned_input;
  /* The shaders that tone-map, and what they are told: their sizes and the
   * screen's SDR white. Compiled at the open for an HDR screen and never for
   * an SDR one, whose pictures reach the processor as they are. */
  ID3D11VertexShader *vertex;
  ID3D11PixelShader *tone;
  ID3D11SamplerState *sampler;
  ID3D11Buffer *sizes;
  unsigned from_width, from_height, to_width, to_height;
  /* Whether the pictures come in as the half-float scRGB an HDR screen hands over,
   * which the shader below tone-maps into the BGRA the processor reads. */
  int hdr;
  /* What the screen calls SDR white, as a multiple of scRGB's 1.0: 1.0 at 80
   * nits, 3.0 where Windows composites SDR at 240. The tone map divides by it, and
   * 1.0 is what a converter that wasn't told anything reads a picture through. */
  float white;
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

/* The shader that tone-maps: one triangle over the whole stream-sized
 * destination, four taps of the capture's half-float picture around each pixel
 * the way d3d11enc.c's scale reads one, and the curve both shims share spliced
 * in (tone_hlsl.h). The sizes and the screen's white arrive in one constant
 * buffer, written per frame. */
static const char *tone_shader =
    "Texture2D<float4> picture : register(t0);\n"
    "SamplerState tap : register(s0);\n"
    "cbuffer Sizes : register(b0) { float2 source; float2 target; float4 tone; };\n"
    "struct Varying { float4 position : SV_Position; float2 uv : TEXCOORD0; };\n"
    "Varying VSMain(uint vertex : SV_VertexID) {\n"
    "  float2 corner = float2((vertex << 1) & 2, vertex & 2);\n"
    "  Varying varying;\n"
    "  varying.position = float4(corner * float2(2, -2) + float2(-1, 1), 0, 1);\n"
    "  varying.uv = corner;\n"
    "  return varying;\n"
    "}\n" YAPFAST_TONE_HLSL
    "float4 PSMain(Varying varying) : SV_Target {\n"
    /* A quarter of the footprint, so the four taps sit at the corners of a box
     * half the footprint across, exactly as they do in d3d11enc.c's scale. */
    "  float2 step = 0.25 * max(source / target, 1.0) / source;\n"
    "  static const float2 corners[4] = { float2(-1, -1), float2(1, -1),\n"
    "                                     float2(-1, 1), float2(1, 1) };\n"
    "  float4 colour = 0;\n"
    "  [unroll] for (uint i = 0; i < 4; i++)\n"
    "    colour += picture.Sample(tap, varying.uv + corners[i] * step);\n"
    "  colour *= 0.25;\n"
    "  return float4(yapfast_tone(colour.rgb, tone.x), 1.0);\n"
    "}\n";

/* One stage of that shader, compiled and made on the device: D3DCompile either
 * writes the program or says, in its own words, which line it didn't like, and
 * `profile` is what it is asked to be. */
static int tone_stage(struct yapfast_d3d11conv *conv, const char *entry, const char *profile,
                      void **out, char *why, size_t size) {
  ID3DBlob *code = NULL;
  ID3DBlob *errors = NULL;
  HRESULT status = D3DCompile(tone_shader, strlen(tone_shader), "yapfast-tone.hlsl", NULL, NULL,
                              entry, profile, D3DCOMPILE_OPTIMIZATION_LEVEL3, 0, &code, &errors);
  if (FAILED(status)) {
    put(why, size, "D3DCompile(%s) failed (0x%08lX) %.96s", entry, (unsigned long)status,
        errors ? (const char *)errors->lpVtbl->GetBufferPointer(errors) : "");
    if (errors) errors->lpVtbl->Release(errors);
    return 0;
  }
  if (errors) errors->lpVtbl->Release(errors);
  if (profile[0] == 'v')
    status = conv->device->lpVtbl->CreateVertexShader(conv->device,
                                                      code->lpVtbl->GetBufferPointer(code),
                                                      code->lpVtbl->GetBufferSize(code), NULL,
                                                      (ID3D11VertexShader **)out);
  else
    status = conv->device->lpVtbl->CreatePixelShader(conv->device,
                                                     code->lpVtbl->GetBufferPointer(code),
                                                     code->lpVtbl->GetBufferSize(code), NULL,
                                                     (ID3D11PixelShader **)out);
  code->lpVtbl->Release(code);
  if (FAILED(status)) {
    *out = NULL;
    failed(conv, profile[0] == 'v' ? "CreateVertexShader for the tone map"
                                   : "CreatePixelShader for the tone map",
           status);
    put(why, size, "%s", conv->error);
    return 0;
  }
  return 1;
}

/* Everything the tone map needs before a frame is drawn: the BGRA picture the
 * shader fills and the processor then reads, a view of it as each, the programs,
 * a sampler and the constant buffer. Asked for at the open, so a card that can't
 * says so in words rather than three frames into a stream. */
static int make_tone(struct yapfast_d3d11conv *conv, char *why, size_t size) {
  D3D11_TEXTURE2D_DESC desc;
  D3D11_RENDER_TARGET_VIEW_DESC target;
  D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC input;
  D3D11_SAMPLER_DESC sampler;
  D3D11_BUFFER_DESC sizes;
  HRESULT status;
  memset(&desc, 0, sizeof desc);
  desc.Width = conv->to_width;
  desc.Height = conv->to_height;
  desc.MipLevels = 1;
  desc.ArraySize = 1;
  desc.Format = DXGI_FORMAT_B8G8R8A8_UNORM;
  desc.SampleDesc.Count = 1;
  desc.Usage = D3D11_USAGE_DEFAULT;
  /* A render target the shader writes and a surface the video processor reads:
   * the two things the capture's own textures are made for. */
  desc.BindFlags = D3D11_BIND_RENDER_TARGET | D3D11_BIND_SHADER_RESOURCE;
  status = conv->device->lpVtbl->CreateTexture2D(conv->device, &desc, NULL, &conv->toned);
  if (FAILED(status) || conv->toned == NULL) {
    conv->toned = NULL;
    failed(conv, "the card wouldn't make a picture for the tone map at the stream's size", status);
    put(why, size, "%s", conv->error);
    return 0;
  }
  memset(&target, 0, sizeof target);
  target.ViewDimension = D3D11_RTV_DIMENSION_TEXTURE2D;
  target.Texture2D.MipSlice = 0;
  status = conv->device->lpVtbl->CreateRenderTargetView(conv->device, (ID3D11Resource *)conv->toned,
                                                       &target, &conv->toned_view);
  if (FAILED(status) || conv->toned_view == NULL) {
    conv->toned_view = NULL;
    failed(conv, "the card wouldn't make a view of the tone map's picture", status);
    put(why, size, "%s", conv->error);
    return 0;
  }
  memset(&input, 0, sizeof input);
  input.FourCC = 0;
  input.ViewDimension = D3D11_VPIV_DIMENSION_TEXTURE2D;
  input.Texture2D.MipSlice = 0;
  input.Texture2D.ArraySlice = 0;
  status = conv->video->lpVtbl->CreateVideoProcessorInputView(
      conv->video, (ID3D11Resource *)conv->toned, conv->enumerator, &input, &conv->toned_input);
  if (FAILED(status) || conv->toned_input == NULL) {
    conv->toned_input = NULL;
    failed(conv, "the card wouldn't read the tone map's picture as a video processor's input",
           status);
    put(why, size, "%s", conv->error);
    return 0;
  }
  if (!tone_stage(conv, "VSMain", "vs_5_0", (void **)&conv->vertex, why, size)) return 0;
  if (!tone_stage(conv, "PSMain", "ps_5_0", (void **)&conv->tone, why, size)) return 0;
  /* Bilinear and clamped: the taps decide what is read, the filter between them
   * is the hardware's, and a texel outside the picture is the picture's edge. */
  memset(&sampler, 0, sizeof sampler);
  sampler.Filter = D3D11_FILTER_MIN_MAG_LINEAR_MIP_POINT;
  sampler.AddressU = D3D11_TEXTURE_ADDRESS_CLAMP;
  sampler.AddressV = D3D11_TEXTURE_ADDRESS_CLAMP;
  sampler.AddressW = D3D11_TEXTURE_ADDRESS_CLAMP;
  sampler.ComparisonFunc = D3D11_COMPARISON_NEVER;
  sampler.MaxLOD = D3D11_FLOAT32_MAX;
  status = conv->device->lpVtbl->CreateSamplerState(conv->device, &sampler, &conv->sampler);
  if (FAILED(status) || conv->sampler == NULL) {
    conv->sampler = NULL;
    failed(conv, "the card wouldn't make a sampler for the tone map", status);
    put(why, size, "%s", conv->error);
    return 0;
  }
  /* Thirty-two bytes: the source's size, the destination's, and what the tone map
   * divides the light by, written per frame. */
  memset(&sizes, 0, sizeof sizes);
  sizes.ByteWidth = 32;
  sizes.Usage = D3D11_USAGE_DEFAULT;
  sizes.BindFlags = D3D11_BIND_CONSTANT_BUFFER;
  status = conv->device->lpVtbl->CreateBuffer(conv->device, &sizes, NULL, &conv->sizes);
  if (FAILED(status) || conv->sizes == NULL) {
    conv->sizes = NULL;
    failed(conv, "the card wouldn't make the tone map's constants", status);
    put(why, size, "%s", conv->error);
    return 0;
  }
  return 1;
}

/* Tone-maps the picture in `texture` into the BGRA picture the processor then
 * reads. The state this borrows is given back: the capture that filled the texture
 * draws on this context too, and finding its render target or viewport changed is
 * not its fault. A colour texture's picture is the whole surface, as it is in
 * native/d3d11enc.c's scale; a capture hands over one texture per frame. */
static int tone_draw(struct yapfast_d3d11conv *conv, void *texture, char *why, size_t size) {
  ID3D11ShaderResourceView *source = NULL;
  ID3D11ShaderResourceView *unbound = NULL;
  ID3D11RenderTargetView *was_target = NULL;
  ID3D11DepthStencilView *was_depth = NULL;
  D3D11_VIEWPORT viewport;
  D3D11_VIEWPORT was;
  UINT viewports = 1;
  float constants[8] = {(float)conv->from_width,
                        (float)conv->from_height,
                        (float)conv->to_width,
                        (float)conv->to_height,
                        conv->white > 0.f ? conv->white : 1.f,
                        0.f,
                        0.f,
                        0.f};
  HRESULT status = conv->device->lpVtbl->CreateShaderResourceView(
      conv->device, (ID3D11Resource *)texture, NULL, &source);
  if (FAILED(status) || source == NULL) {
    failed(conv, "the card wouldn't read the capture's picture in a shader", status);
    put(why, size, "%s", conv->error);
    return 0;
  }
  conv->context->lpVtbl->UpdateSubresource(conv->context, (ID3D11Resource *)conv->sizes, 0, NULL,
                                           constants, 0, 0);
  conv->context->lpVtbl->OMGetRenderTargets(conv->context, 1, &was_target, &was_depth);
  conv->context->lpVtbl->RSGetViewports(conv->context, &viewports, &was);
  memset(&viewport, 0, sizeof viewport);
  viewport.Width = (float)conv->to_width;
  viewport.Height = (float)conv->to_height;
  viewport.MaxDepth = 1.f;
  conv->context->lpVtbl->OMSetRenderTargets(conv->context, 1, &conv->toned_view, NULL);
  conv->context->lpVtbl->RSSetViewports(conv->context, 1, &viewport);
  conv->context->lpVtbl->IASetPrimitiveTopology(conv->context,
                                                D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
  conv->context->lpVtbl->IASetInputLayout(conv->context, NULL);
  conv->context->lpVtbl->VSSetShader(conv->context, conv->vertex, NULL, 0);
  conv->context->lpVtbl->PSSetShader(conv->context, conv->tone, NULL, 0);
  conv->context->lpVtbl->VSSetConstantBuffers(conv->context, 0, 1, &conv->sizes);
  conv->context->lpVtbl->PSSetConstantBuffers(conv->context, 0, 1, &conv->sizes);
  conv->context->lpVtbl->PSSetShaderResources(conv->context, 0, 1, &source);
  conv->context->lpVtbl->PSSetSamplers(conv->context, 0, 1, &conv->sampler);
  conv->context->lpVtbl->Draw(conv->context, 3, 0);
  /* Both ends unbound before the processor reads the picture it just wrote: a
   * texture left as a render target or a shader's input is one the driver would
   * rather not be asked to convert. */
  conv->context->lpVtbl->PSSetShaderResources(conv->context, 0, 1, &unbound);
  conv->context->lpVtbl->OMSetRenderTargets(conv->context, 0, NULL, NULL);
  if (was_target != NULL)
    conv->context->lpVtbl->OMSetRenderTargets(conv->context, 1, &was_target, was_depth);
  if (was_target != NULL) was_target->lpVtbl->Release(was_target);
  if (was_depth != NULL) was_depth->lpVtbl->Release(was_depth);
  if (viewports > 0)
    conv->context->lpVtbl->RSSetViewports(conv->context, 1, &was);
  else
    conv->context->lpVtbl->RSSetViewports(conv->context, 0, NULL);
  source->lpVtbl->Release(source);
  return 1;
}

/* Says what the picture is and what the frame it becomes is, once the shader has
 * tone-mapped it: the BGRA it wrote is the sRGB codes an SDR screen shows, and
 * what an encoder reads here is NV12 in BT.601's studio range, which is the claim
 * the stream declares — the same matrix NVENC converts a BGRA texture with. The
 * output is named as YCbCr rather than as RGB because the texture this blit
 * writes is NV12: a processor told its output is RGB writes the samples a
 * different way, and the encoder would read a picture skewed towards green.
 *
 * Both calls answer nothing, so each is read back: a processor that kept its
 * defaults would read sRGB codes as linear light, which is a frame that looks
 * plausible and isn't, and a caller told the converter won't is the better
 * answer. */
static int colour_spaces(struct yapfast_d3d11conv *conv) {
  DXGI_COLOR_SPACE_TYPE wanted = DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709;
  DXGI_COLOR_SPACE_TYPE shown = DXGI_COLOR_SPACE_YCBCR_STUDIO_G22_LEFT_P601;
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
  if (wanted != DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709) {
    put(conv->error, sizeof conv->error,
        "the card wouldn't take sRGB as what the tone map's picture is "
        "(VideoProcessorSetStreamColorSpace1, it says %u)",
        (unsigned)wanted);
    return 0;
  }
  conv->video_context1->lpVtbl->VideoProcessorSetOutputColorSpace1(
      conv->video_context1, conv->processor, shown);
  shown = DXGI_COLOR_SPACE_CUSTOM;
  conv->video_context1->lpVtbl->VideoProcessorGetOutputColorSpace1(conv->video_context1,
                                                                  conv->processor, &shown);
  if (shown != DXGI_COLOR_SPACE_YCBCR_STUDIO_G22_LEFT_P601) {
    put(conv->error, sizeof conv->error,
        "the card wouldn't take BT.601's studio range as what its output is "
        "(VideoProcessorSetOutputColorSpace1, it says %u)",
        (unsigned)shown);
    return 0;
  }
  return 1;
}

struct yapfast_d3d11conv *yapfast_d3d11conv_new(void *device, unsigned from_width,
                                               unsigned from_height, unsigned to_width,
                                               unsigned to_height, int hdr, unsigned white,
                                               char *why, size_t size) {
  put(why, size, "not opened");
  struct yapfast_d3d11conv *conv = calloc(1, sizeof *conv);
  if (conv == NULL) return NULL;
  conv->device = (ID3D11Device *)device;
  conv->from_width = from_width;
  conv->from_height = from_height;
  conv->to_width = to_width & ~1u;
  conv->to_height = to_height & ~1u;
  conv->hdr = hdr ? 1 : 0;
  conv->white = white ? (float)white / 1000.0f : 1.0f;
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
  if (conv->hdr) {
    /* The shader that tone-maps, and the BGRA picture it fills for the processor
     * to read, made and checked before the converter answers: a card that can't is
     * named here rather than a frame into a stream. */
    if (!make_tone(conv, why, size) || !colour_spaces(conv)) {
      put(why, size, "%s", conv->error);
      yapfast_d3d11conv_free(conv);
      return NULL;
    }
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
  /* The picture the processor reads: the capture's own texture where it is
   * BGRA already, and the BGRA the shader tone-mapped it into where it isn't. A
   * view this call made is let go after the blit; the tone map's is the
   * converter's and outlives it. */
  ID3D11VideoProcessorInputView *input = NULL;
  int made = 0;
  HRESULT status;
  if (conv->hdr) {
    if (!tone_draw(conv, texture, why, size)) return NULL;
    input = conv->toned_input;
  } else {
    status = conv->video->lpVtbl->CreateVideoProcessorInputView(
        conv->video, (ID3D11Resource *)texture, conv->enumerator, &desc, &input);
    if (FAILED(status) || input == NULL) {
      failed(conv, "the card wouldn't read the capture's texture as a video processor's input",
             status);
      put(why, size, "%s", conv->error);
      return NULL;
    }
    made = 1;
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
  if (made) input->lpVtbl->Release(input);
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
  /* The tone map's views and picture go before the enumerator they were made
   * against, and its programs and constants with them. */
  if (conv->toned_input != NULL) conv->toned_input->lpVtbl->Release(conv->toned_input);
  if (conv->toned_view != NULL) conv->toned_view->lpVtbl->Release(conv->toned_view);
  if (conv->toned != NULL) conv->toned->lpVtbl->Release(conv->toned);
  if (conv->tone != NULL) conv->tone->lpVtbl->Release(conv->tone);
  if (conv->vertex != NULL) conv->vertex->lpVtbl->Release(conv->vertex);
  if (conv->sampler != NULL) conv->sampler->lpVtbl->Release(conv->sampler);
  if (conv->sizes != NULL) conv->sizes->lpVtbl->Release(conv->sizes);
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
