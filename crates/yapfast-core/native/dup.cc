/* Desktop Duplication: a display's own compositor output as a D3D11 texture,
 * upright, in BGRA, with the mouse pointer already drawn into it.
 *
 * It is the sibling of native/wgc.cc, and it earns its place by being the path
 * that does not go through WinRT: DXGI hands the compositor's surface straight
 * over, and everything this file does to it afterwards — rotation, HDR, the
 * pointer — is a GPU pass on that surface. WGC gives the caller a BGRA frame
 * already tonemapped and already upright but draws no cursor; Desktop Duplication
 * gives scRGB half-floats on an HDR screen and does not rotate either, so the
 * work WGC does inside Windows happens here instead, and the cursor, which WGC
 * leaves to the caller entirely, happens here too.
 *
 * Ownership, which is the memory story of this file: `yapfast_dup_frame` hands
 * back an `ID3D11Texture2D` that the capture owns, and the caller *borrows* it.
 * It is the same texture for the life of the capture and it is overwritten by the
 * next call that has something to draw, so reading it after that call reads the
 * new picture; it is let go at `yapfast_dup_close` and never by the caller. A
 * caller that has to hand it to something that keeps it past the next call —
 * nothing in this crate does yet — has to hold its own copy. The capture holds
 * two full-screen textures and one cursor shape, all made at open, and allocates
 * nothing per frame.
 *
 * Four things a caller has to know about what comes out:
 *
 *   - the size is the desktop's, not the surface's: a rotated display hands over
 *     a surface in the mode's own orientation, and the frame is turned upright
 *     before it is handed out, so a portrait screen arrives 1080x1920 rather than
 *     1920x1080.
 *   - the pixels are always BGRA, on an HDR screen too: an scRGB half-float
 *     surface is converted on the GPU, and the conversion is this file's, not
 *     Windows', so it is stated and justified where it happens.
 *   - `DXGI_ERROR_WAIT_TIMEOUT` is an answer and not a failure. A desktop nobody
 *     is drawing on produces nothing, and saying so is the difference between a
 *     stream that idles and a stream that reports a broken capture once a second.
 *   - the first frame a new duplication hands over can be an empty image, and
 *     this one is: the RTX 4090 and its HDR desktop in this machine's own test
 *     delivered a black 3440x1440 picture with `AccumulatedFrames` already at 1,
 *     and the first picture with the desktop in it came with the next change.
 *     Nothing in the frame says which it is, so a caller starting a stream drops
 *     the first frame it is given rather than encoding a black one.
 *
 * Nothing here is wired into a stream: see src/dup.rs. */

#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <d3d11.h>
#include <d3dcompiler.h>
#include <dxgi1_2.h>
#include <new>
#include <stdarg.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

/* What asking for a frame found, as Rust reads it. */
enum {
  /* The call failed, or the duplication ended. `why` says which, in words. */
  YAPFAST_DUP_FAILED = -1,
  /* Nothing has been drawn and nothing has moved since the last call: the
   * screen is as it was. This is what AcquireNextFrame's WAIT_TIMEOUT means. */
  YAPFAST_DUP_STILL = 0,
  /* A new desktop image arrived, and the texture holds it with the pointer in
   * it. */
  YAPFAST_DUP_FRAME = 1,
  /* Only the pointer moved, or changed shape, or went away. There is no new
   * desktop image — Desktop Duplication hands over nothing at all in that case —
   * so the last one was redrawn with the pointer where it is now, and the texture
   * is the picture the screen shows. */
  YAPFAST_DUP_POINTER = 2,
};

/* What the surface's pixels are, in the terms the conversion cares about rather
 * than DXGI's format numbers, so the Rust side can name one without depending on
 * a DXGI header. */
enum {
  YAPFAST_DUP_BGRA8 = 0,
  YAPFAST_DUP_RGB10A2 = 1,
  YAPFAST_DUP_SCRGB = 2,
  YAPFAST_DUP_OTHER = 3,
};

/* The pointer shapes Desktop Duplication reports, which are the ones the shape
 * buffer is laid out for. The numbers are DXGI's and are repeated here because
 * this file is the only place that writes the shape texture. */
enum {
  YAPFAST_DUP_POINTER_NONE = 0,
  YAPFAST_DUP_POINTER_MONOCHROME = 1,
  YAPFAST_DUP_POINTER_COLOUR = 2,
};

struct yapfast_dup;

extern "C" void yapfast_dup_close(yapfast_dup *capture);

/* Everything a frame's rendering reads, written into one constant buffer per
 * call: the sizes, how the surface is turned to be upright, whether the pixels
 * need converting, and where the pointer's top-left corner is in the upright
 * picture and how large the shape is. Packed as HLSL packs it — two vectors of
 * four 32-bit values per 16-byte register — so the struct is the shader's own
 * cbuffer and nothing has to be translated at the boundary. */
struct cover {
  float target[2];
  float source[2];
  int rotation;
  int hdr;
  int pointer;
  /* What SDR white is in the surface's linear light: 1.0 on a desktop where SDR
   * content is composited at 80 nits, 3.0 where Windows is set to 240. */
  float white;
  float pointer_rect[4];
};
static_assert(sizeof(cover) == 48, "the constant buffer has to be whole registers");

struct yapfast_dup {
  /* The machine's side: the adapter that drives the output being duplicated, the
   * device made on it, and the duplication itself. These live as long as the
   * capture, and a frame's texture is on this device. */
  IDXGIAdapter1 *adapter;
  IDXGIOutput1 *output;
  IDXGIOutputDuplication *duplication;
  ID3D11Device *device;
  ID3D11DeviceContext *context;

  /* The mode: the surface as DXGI hands it over, and the desktop as a person sees
   * it. They differ only when the display is rotated. */
  unsigned surface_width, surface_height;
  unsigned desktop_width, desktop_height;
  DXGI_MODE_ROTATION rotation;
  DXGI_FORMAT format;
  /* What SDR white is in the surface's linear light, which on an HDR desktop is
   * what Windows composites plain SDR content at: 1.0 when that is 80 nits, and
   * the number the system reports divided by 80 otherwise. */
  float white;
  unsigned output_index;
  LUID luid;
  char card[128];

  /* The last desktop image, in the surface's own orientation and format, which is
   * what `CopyResource` copies the acquired surface into. It is kept because a
   * pointer-only update arrives with no surface at all, and redrawing the last
   * image is the only way to move the cursor in a frame the caller hasn't
   * re-acquired: without it, a mouse move would arrive as a frame with no
   * picture. */
  ID3D11Texture2D *held;
  ID3D11ShaderResourceView *held_view;

  /* What the caller borrows: upright, BGRA, pointer composited. */
  ID3D11Texture2D *image;
  ID3D11RenderTargetView *image_view;

  /* A staging copy of the frame, made the first time a diagnostic asks for one
   * pixel and kept afterwards: reading the picture back to the processor is the
   * one thing this file does that a stream never will, so it costs nothing when
   * nobody asks and is never a per-frame allocation. */
  ID3D11Texture2D *stage;

  /* One triangle and one pixel shader do the whole conversion: they are the same
   * pass for an 8-bit surface and an scRGB one, and the same pass with or without
   * a pointer, which is why there is nothing to choose between per frame. */
  ID3D11VertexShader *vertex;
  ID3D11PixelShader *pixel;
  ID3D11SamplerState *screen_sampler;
  ID3D11SamplerState *shape_sampler;
  ID3D11RasterizerState *raster;
  ID3D11Buffer *constants;

  /* The pointer: its shape as a texture the shader can read — R8G8 for a
   * monochrome pointer, whose two masks are two channels, and BGRA for a colour
   * one — the shape's description, and the bytes it was unpacked from, so that a
   * shape which comes back larger than the last can be taken without a
   * reallocation every time it changes. */
  ID3D11Texture2D *shape;
  ID3D11ShaderResourceView *shape_view;
  DXGI_OUTDUPL_POINTER_SHAPE_INFO shape_info;
  int shape_kind;
  unsigned char *shape_bytes;
  size_t shape_bytes_size;
  unsigned char *shape_pixels;
  size_t shape_pixels_size;

  /* Where the pointer is, as the system reports it — the hot spot's place in
   * desktop coordinates, which is the space the frame is drawn in — and whether it
   * is there. The shape's top-left corner is computed from it at paint time rather
   * than kept, because a shape that changes carries a different hot spot and a
   * corner kept from the last one would put the cursor a few pixels out. */
  int pointer_visible;
  int pointer_seen;
  int pointer_x, pointer_y;

  /* Whether `image` holds a picture at all: a capture that has never had a frame
   * has nothing to redraw for a pointer move, and saying so is not an error. */
  int painted;
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

/* --------------------------------------------------------------- the shader */

/* The whole of the pixel work, in one pass. A full-screen triangle is drawn with
 * no vertex buffer — `SV_VertexID` is the three corners — and the pixel shader
 * decides where in the surface that pixel came from, converts it if it is HDR,
 * and composites the pointer. Doing it in one pass is what keeps the cost at one
 * full-screen pass per frame no matter how many of those steps apply, and what
 * makes a pointer-only update the same cost as a new frame rather than a second
 * kind of work.
 *
 * Rotation is the pixel shader's rather than the vertex shader's because the
 * target is a plain rectangle: turning where a pixel is read from turns the
 * image, and leaving the vertices alone leaves the triangle covering the target. */
static const char *kShader =
    R"hlsl(
cbuffer Cover : register(b0) {
    float2 target_size;
    float2 source_size;
    int rotation;      /* 0 identity, 1 a quarter clockwise, 2 half, 3 three quarters */
    int hdr;           /* the surface is scRGB half-floats */
    int pointer;       /* 0 hidden, 1 monochrome, 2 colour */
    float white;       /* SDR white in the surface's linear light */
    float4 pointer_rect;  /* left, top, width, height in target pixels */
};

Texture2D<float4> desktop : register(t0);
Texture2D<float4> shape : register(t1);
SamplerState screen_sampler : register(s0);
SamplerState shape_sampler : register(s1);

/* scRGB's primaries are BT.709's, so this is the identity — it is written out
 * because it is the step the conversion performs, and a desktop in another space
 * would change this and nothing else in the HDR branch. */
static const float3x3 scrgb_to_bt709 = { 1.0, 0.0, 0.0,
                                         0.0, 1.0, 0.0,
                                         0.0, 0.0, 1.0 };

struct Varying {
    float4 position : SV_Position;
};

Varying vs_main(uint id : SV_VertexID) {
    /* (0,0), (2,0), (0,2) become the corners of a triangle that covers the whole
     * target and then some: (-1,-1), (3,-1), (-1,3) in clip space. Nothing
     * outside the viewport is shaded, so the overshoot costs nothing. */
    float2 corner = float2((id << 1) & 2, id & 2);
    Varying from;
    from.position = float4(corner * 2.0 - 1.0, 0.0, 1.0);
    return from;
}

/* The sRGB transfer function, piecewise as it is defined. A 2.2 power is close
 * and is wrong by up to two code values in the shadows, which is the sort of
 * wrong a caller comparing a capture against a screenshot would notice. */
float3 srgb_encode(float3 light) {
    light = max(light, 0.0);
    float3 low = light * 12.92;
    float3 high = 1.055 * pow(light, 1.0 / 2.4) - 0.055;
    return lerp(low, high, step(0.0031308, light));
}

/* scRGB is linear light with 1.0 at 80 nits, so an SDR white sits at 1.0 and
 * everything above it is a highlight the screen can show and an 8-bit frame
 * cannot. Below the knee this is the identity, which keeps every SDR pixel
 * exactly the value Windows composited; above it, a soft exponential that
 * approaches 1.0 without reaching it, so a 1000-nit specular highlight arrives as
 * white instead of as a flat clipped patch with a hard edge. */
float3 roll_highlights(float3 light) {
    const float knee = 0.9;
    float3 over = max(light - knee, 0.0) / (1.0 - knee);
    return lerp(light, knee + (1.0 - knee) * (1.0 - exp(-over)), step(knee, light));
}

float4 ps_main(Varying pixel) : SV_Target {
    float2 at = pixel.position.xy;

    /* Where in the surface this pixel comes from. A rotated display hands over the
     * mode's own orientation with the desktop turned inside it, so the inverse
     * turn is what makes the picture upright; the corner that looks odd — the
     * source height appearing in an x — is the turn. */
    float2 from;
    if (rotation == 1) {
        from = float2(at.y, source_size.y - at.x);
    } else if (rotation == 2) {
        from = float2(source_size.x - at.x, source_size.y - at.y);
    } else if (rotation == 3) {
        from = float2(source_size.x - at.y, at.x);
    } else {
        from = at;
    }

    float3 colour = desktop.SampleLevel(screen_sampler, from / source_size, 0).rgb;

    if (hdr != 0) {
        /* What actually differs between scRGB and an 8-bit frame is the transfer
         * function — linear light in, sRGB out — and the range, which the shoulder
         * above handles; the primaries are the same, which is why the matrix is the
         * identity.
         *
         * This is not picture.rs's `Tone`. That one is handed 8-bit PQ-coded
         * pixels, where the display-referred signal has already been through the
         * PQ curve and has to be undone before anything can be scaled; an scRGB
         * surface is linear already, and applying an inverse PQ to it would turn
         * a mid-grey into a highlight. */
        float3 light = mul(scrgb_to_bt709, colour) / max(white, 0.001);
        colour = srgb_encode(roll_highlights(light));
    }

    if (pointer != 0) {
        float2 into = at - pointer_rect.xy;
        bool inside = into.x >= 0.0 && into.y >= 0.0 && into.x < pointer_rect.z &&
                      into.y < pointer_rect.w;
        if (inside) {
            float2 where = into / pointer_rect.zw;
            if (pointer == 1) {
                /* A monochrome pointer is two 1-bit masks, and the rule Windows
                 * draws them by is (background AND mask) XOR mask. Read as
                 * colours — AND set means black, XOR set means white — that is
                 * what the standard arrow is: a black outline with a white body.
                 * The two masks do not overlap in a real cursor, so the order of
                 * the two tests decides nothing; it is written AND first because
                 * that is the order the rule applies them in. */
                float2 mask = shape.SampleLevel(shape_sampler, where, 0).rg;
                if (mask.r > 0.5) {
                    colour = float3(0.0, 0.0, 0.0);
                } else if (mask.g > 0.5) {
                    colour = float3(1.0, 1.0, 1.0);
                }
            } else {
                /* A colour pointer carries its own alpha, and Windows blends it
                 * over the desktop with that alpha; the shape is the cursor's own
                 * bitmap, not a pre-composited one, so the blend is un-premultiplied
                 * here and the shadowed edges come out right. A masked colour
                 * pointer arrives through the same path: its mask is what the
                 * system already applied to the alpha it gave us. */
                float4 pointer_colour = shape.SampleLevel(shape_sampler, where, 0);
                colour = lerp(colour, pointer_colour.rgb, saturate(pointer_colour.a));
            }
        }
    }

    return float4(saturate(colour), 1.0);
}
)hlsl";

/* Compiles one entry point of the shader above, and puts the compiler's own
 * complaint in `why` when it refuses: a shader that doesn't compile is a build
 * that half-works, and the message is worth more than the status code. */
static ID3DBlob *compile(const char *entry, const char *model, char *why, size_t size) {
  ID3DBlob *code = NULL;
  ID3DBlob *messages = NULL;
  HRESULT status = D3DCompile(kShader, strlen(kShader), "native/dup.cc", NULL, NULL, entry, model,
                              D3DCOMPILE_ENABLE_STRICTNESS | D3DCOMPILE_OPTIMIZATION_LEVEL3, 0,
                              &code, &messages);
  if (FAILED(status)) {
    say(why, size, "the frame shader's %s refused to compile (0x%08lX): %s", entry,
        (unsigned long)status,
        messages ? (const char *)messages->GetBufferPointer() : "no message from the compiler");
    if (code) code->Release();
    code = NULL;
  }
  if (messages) messages->Release();
  return code;
}

/* ------------------------------------------------------------- the pointer */

/* What the system calls each shape, as the shader is told. */
static int pointer_kind(const DXGI_OUTDUPL_POINTER_SHAPE_INFO *info) {
  if (info->Type == DXGI_OUTDUPL_POINTER_SHAPE_TYPE_MONOCHROME)
    return YAPFAST_DUP_POINTER_MONOCHROME;
  if (info->Type == DXGI_OUTDUPL_POINTER_SHAPE_TYPE_COLOR ||
      info->Type == DXGI_OUTDUPL_POINTER_SHAPE_TYPE_MASKED_COLOR)
    return YAPFAST_DUP_POINTER_COLOUR;
  return YAPFAST_DUP_POINTER_NONE;
}

/* Grows a buffer to at least `wanted`, and leaves it alone when it is already
 * that large: a pointer shape changes when the cursor does, which is rare, and
 * the buffers below are therefore allocated a handful of times in a stream's life
 * rather than once per frame. */
static int grow(unsigned char **bytes, size_t *size, size_t wanted) {
  unsigned char *larger;
  if (*size >= wanted) return 1;
  larger = (unsigned char *)realloc(*bytes, wanted);
  if (!larger) return 0;
  *bytes = larger;
  *size = wanted;
  return 1;
}

/* Turns the shape buffer into a texture the shader can read, and says what kind
 * of pointer it is.
 *
 * A monochrome pointer arrives as two 1-bit masks, one above the other, and both
 * are unpacked into one R8G8 texture: the AND mask in red, the XOR mask in green.
 * Reading two channels out of one fetch is what lets the shader keep the whole
 * pointer to one sample per pixel. A colour pointer is already BGRA and is copied
 * across, with its rows repacked only when the system's row pitch is not the
 * width's — which happens for a shape narrower than 4 pixels, and copying it
 * blindly would shear the cursor. */
static int admit_shape(yapfast_dup *capture, const DXGI_OUTDUPL_FRAME_INFO *frame, char *why,
                       size_t size) {
  UINT wanted = frame->PointerShapeBufferSize;
  UINT needed = 0;
  DXGI_OUTDUPL_POINTER_SHAPE_INFO info;
  HRESULT status;

  memset(&info, 0, sizeof info);
  if (!grow(&capture->shape_bytes, &capture->shape_bytes_size, wanted)) {
    say(why, size, "no room for a %u byte pointer shape", (unsigned)wanted);
    return 0;
  }
  status = capture->duplication->GetFramePointerShape((UINT)capture->shape_bytes_size,
                                                     capture->shape_bytes, &needed, &info);
  if (status == DXGI_ERROR_MORE_DATA) {
    /* The shape grew between the frame that announced it and the call that asked
     * for it. The size it wants is `needed`, so it is taken once more and this is
     * not a failure. */
    if (!grow(&capture->shape_bytes, &capture->shape_bytes_size, needed)) {
      say(why, size, "no room for a %u byte pointer shape", (unsigned)needed);
      return 0;
    }
    status = capture->duplication->GetFramePointerShape((UINT)capture->shape_bytes_size,
                                                       capture->shape_bytes, &needed, &info);
  }
  if (FAILED(status)) {
    refused(why, size, "IDXGIOutputDuplication::GetFramePointerShape", status);
    return 0;
  }
  int kind = pointer_kind(&info);
  if (kind == YAPFAST_DUP_POINTER_NONE || info.Width == 0 || info.Height == 0) {
    /* A shape this file has no reading for is a pointer that isn't drawn, not a
     * frame that is refused: the desktop behind it is still the screen. */
    capture->shape_kind = YAPFAST_DUP_POINTER_NONE;
    capture->shape_info = info;
    return 1;
  }

  D3D11_TEXTURE2D_DESC td;
  memset(&td, 0, sizeof td);
  td.Width = info.Width;
  td.Height = info.Height;
  td.MipLevels = 1;
  td.ArraySize = 1;
  td.SampleDesc.Count = 1;
  td.Usage = D3D11_USAGE_DEFAULT;
  td.BindFlags = D3D11_BIND_SHADER_RESOURCE;
  td.Format = kind == YAPFAST_DUP_POINTER_MONOCHROME ? DXGI_FORMAT_R8G8_UNORM
                                                     : DXGI_FORMAT_B8G8R8A8_UNORM;

  D3D11_SUBRESOURCE_DATA start;
  memset(&start, 0, sizeof start);

  if (kind == YAPFAST_DUP_POINTER_MONOCHROME) {
    /* The buffer is twice the height the pointer is: the AND mask's rows first,
     * the XOR mask's after them, each 1 bit per pixel from the most significant
     * bit of each byte, each row padded to a 32-bit boundary. The height in the
     * shape info is already doubled for exactly this reason. */
    unsigned width = info.Width;
    unsigned height = info.Height / 2;
    unsigned pull = info.Pitch ? info.Pitch : ((width + 7) / 8 + 3) & ~3u;
    if (height == 0 || (size_t)pull * info.Height > capture->shape_bytes_size) {
      say(why, size, "a %ux%u monochrome pointer shape doesn't fit its %u byte buffer",
          (unsigned)width, (unsigned)height, (unsigned)capture->shape_bytes_size);
      return 0;
    }
    if (!grow(&capture->shape_pixels, &capture->shape_pixels_size, (size_t)width * height * 2)) {
      say(why, size, "no room for a %ux%u pointer shape", (unsigned)width, (unsigned)height);
      return 0;
    }
    for (unsigned row = 0; row < height; row++) {
      const unsigned char *and_row = capture->shape_bytes + (size_t)row * pull;
      const unsigned char *xor_row = capture->shape_bytes + (size_t)(height + row) * pull;
      unsigned char *out = capture->shape_pixels + (size_t)row * width * 2;
      for (unsigned column = 0; column < width; column++) {
        unsigned char bit = (unsigned char)(0x80u >> (column & 7));
        out[column * 2] = (and_row[column / 8] & bit) ? 0xFF : 0x00;
        out[column * 2 + 1] = (xor_row[column / 8] & bit) ? 0xFF : 0x00;
      }
    }
    td.Height = height;
    start.pSysMem = capture->shape_pixels;
    start.SysMemPitch = width * 2;
  } else {
    unsigned width = info.Width;
    unsigned height = info.Height;
    if (info.Pitch == width * 4) {
      start.pSysMem = capture->shape_bytes;
      start.SysMemPitch = info.Pitch;
    } else {
      unsigned pull = info.Pitch ? info.Pitch : width * 4;
      if ((size_t)pull * height > capture->shape_bytes_size) {
        say(why, size, "a %ux%u pointer shape doesn't fit its %u byte buffer", (unsigned)width,
            (unsigned)height, (unsigned)capture->shape_bytes_size);
        return 0;
      }
      if (!grow(&capture->shape_pixels, &capture->shape_pixels_size, (size_t)width * height * 4)) {
        say(why, size, "no room for a %ux%u pointer shape", (unsigned)width, (unsigned)height);
        return 0;
      }
      for (unsigned row = 0; row < height; row++)
        memcpy(capture->shape_pixels + (size_t)row * width * 4,
               capture->shape_bytes + (size_t)row * pull, (size_t)width * 4);
      start.pSysMem = capture->shape_pixels;
      start.SysMemPitch = width * 4;
    }
  }

  /* A shape is made afresh rather than updated: the two kinds are two formats,
   * and a cursor that turns from an arrow into an I-beam changes both anyway. It
   * is a 256-byte-wide texture at most, made when the pointer changes and never
   * per frame. */
  if (capture->shape_view) {
    capture->shape_view->Release();
    capture->shape_view = NULL;
  }
  if (capture->shape) {
    capture->shape->Release();
    capture->shape = NULL;
  }
  status = capture->device->CreateTexture2D(&td, &start, &capture->shape);
  if (FAILED(status)) {
    refused(why, size, "CreateTexture2D(the pointer's shape)", status);
    capture->shape_kind = YAPFAST_DUP_POINTER_NONE;
    return 0;
  }
  status = capture->device->CreateShaderResourceView(capture->shape, NULL, &capture->shape_view);
  if (FAILED(status)) {
    refused(why, size, "CreateShaderResourceView(the pointer's shape)", status);
    capture->shape_kind = YAPFAST_DUP_POINTER_NONE;
    return 0;
  }
  capture->shape_info = info;
  capture->shape_kind = kind;
  return 1;
}

/* ------------------------------------------------------------ the rendering */

/* Draws the last desktop image into the caller's texture, turned upright,
 * converted if it is HDR, and with the pointer composited where it is now.
 *
 * Nothing here reads the surface Desktop Duplication handed over: that has been
 * copied and released by the time this runs, which is what makes a pointer-only
 * update possible at all and what keeps a slow caller from holding the
 * compositor's buffer. */
static int paint(yapfast_dup *capture, char *why, size_t size) {
  D3D11_MAPPED_SUBRESOURCE mapped;
  HRESULT status = capture->context->Map(capture->constants, 0, D3D11_MAP_WRITE_DISCARD, 0,
                                         &mapped);
  if (FAILED(status)) {
    refused(why, size, "Map(the frame's constants)", status);
    return 0;
  }
  cover *values = (cover *)mapped.pData;
  memset(values, 0, sizeof *values);
  values->target[0] = (float)capture->desktop_width;
  values->target[1] = (float)capture->desktop_height;
  values->source[0] = (float)capture->surface_width;
  values->source[1] = (float)capture->surface_height;
  switch (capture->rotation) {
    case DXGI_MODE_ROTATION_ROTATE90: values->rotation = 1; break;
    case DXGI_MODE_ROTATION_ROTATE180: values->rotation = 2; break;
    case DXGI_MODE_ROTATION_ROTATE270: values->rotation = 3; break;
    default: values->rotation = 0; break;
  }
  values->hdr = capture->format == DXGI_FORMAT_R16G16B16A16_FLOAT ? 1 : 0;
  values->white = capture->white;
  values->pointer = 0;
  if (capture->pointer_visible && capture->shape_kind != YAPFAST_DUP_POINTER_NONE) {
    /* The shape's own height is doubled for a monochrome pointer, so the size
     * drawn is the one the masks describe rather than the buffer's. */
    unsigned width = capture->shape_info.Width;
    unsigned height = capture->shape_kind == YAPFAST_DUP_POINTER_MONOCHROME
                          ? capture->shape_info.Height / 2
                          : capture->shape_info.Height;
    values->pointer = capture->shape_kind;
    /* The corner, not the hot spot: the shape is drawn from its top-left, and the
     * hot spot is what says where in it the pointer actually is. It can be off the
     * edge of the desktop — a cursor half over the boundary — and the shader's
     * bounds test is what clips it. */
    values->pointer_rect[0] = (float)(capture->pointer_x - (int)capture->shape_info.HotSpot.x);
    values->pointer_rect[1] = (float)(capture->pointer_y - (int)capture->shape_info.HotSpot.y);
    values->pointer_rect[2] = (float)width;
    values->pointer_rect[3] = (float)height;
  }
  capture->context->Unmap(capture->constants, 0);

  D3D11_VIEWPORT viewport;
  memset(&viewport, 0, sizeof viewport);
  viewport.Width = (float)capture->desktop_width;
  viewport.Height = (float)capture->desktop_height;
  viewport.MaxDepth = 1.0f;
  capture->context->RSSetViewports(1, &viewport);
  capture->context->RSSetState(capture->raster);
  capture->context->OMSetRenderTargets(1, &capture->image_view, NULL);
  capture->context->IASetInputLayout(NULL);
  capture->context->IASetPrimitiveTopology(D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
  capture->context->VSSetShader(capture->vertex, NULL, 0);
  capture->context->PSSetShader(capture->pixel, NULL, 0);
  capture->context->PSSetConstantBuffers(0, 1, &capture->constants);
  ID3D11ShaderResourceView *views[2] = {capture->held_view, capture->shape_view};
  capture->context->PSSetShaderResources(0, 2, views);
  ID3D11SamplerState *samplers[2] = {capture->screen_sampler, capture->shape_sampler};
  capture->context->PSSetSamplers(0, 2, samplers);
  capture->context->Draw(3, 0);

  /* Unbound again before the next call copies a new desktop image into the
   * texture they point at: a resource read by the pipeline while it is written to
   * is the one thing the driver is entitled to complain about. */
  ID3D11ShaderResourceView *none[2] = {NULL, NULL};
  capture->context->PSSetShaderResources(0, 2, none);
  capture->context->OMSetRenderTargets(0, NULL, NULL);
  return 1;
}

/* --------------------------------------------------------------- the setup */

/* The adapter with this LUID, which is how a caller says which card drives the
 * display it wants: the LUID is what DXGI's own enumeration reports and what
 * every other part of the stream uses to name a device. */
static IDXGIAdapter1 *adapter_with(IDXGIFactory1 *factory, LUID wanted, char *why, size_t size) {
  IDXGIAdapter1 *adapter = NULL;
  for (unsigned at = 0; factory->EnumAdapters1(at, &adapter) != DXGI_ERROR_NOT_FOUND; at++) {
    DXGI_ADAPTER_DESC1 desc;
    memset(&desc, 0, sizeof desc);
    if (SUCCEEDED(adapter->GetDesc1(&desc)) && desc.AdapterLuid.LowPart == wanted.LowPart &&
        desc.AdapterLuid.HighPart == wanted.HighPart)
      return adapter;
    adapter->Release();
    adapter = NULL;
  }
  say(why, size, "no adapter has LUID %08lX:%08lX", (unsigned long)wanted.HighPart,
      (unsigned long)wanted.LowPart);
  return NULL;
}

/* What SDR white is worth in the duplication surface's linear light.
 *
 * scRGB defines 1.0 as 80 nits, and taking that literally is the mistake this
 * function exists to avoid: Windows composites plain SDR content at the user's
 * "SDR content brightness", which is 240 nits by default, so an SDR white arrives
 * in the surface at 3.0. Converted as if it were 1.0, every window would come out
 * three times too bright and everything above a third of white would clip; with
 * this, a grey reads the same in the frame as it does in a screenshot.
 *
 * DISPLAYCONFIG_SDR_WHITE_LEVEL is where the system says what it used, in
 * thousandths of 80 nits. The display is found by matching the GDI name DXGI
 * gives the output against the desktop configuration's own, since that is the one
 * name the two APIs share. A machine with no HDR, no configuration to ask or an
 * answer of zero gets the standard's 1.0 back, and the frame is then the naive
 * reading rather than a guess. */
static float sdr_white_level(IDXGIOutput *output) {
  DISPLAYCONFIG_PATH_INFO *paths = NULL;
  DISPLAYCONFIG_MODE_INFO *modes = NULL;
  DXGI_OUTPUT_DESC ours;
  UINT32 count = 0;
  UINT32 mode_count = 0;
  float white = 1.0f;

  memset(&ours, 0, sizeof ours);
  if (FAILED(output->GetDesc(&ours))) return white;
  if (GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &count, &mode_count) != ERROR_SUCCESS)
    return white;
  paths = (DISPLAYCONFIG_PATH_INFO *)calloc(count ? count : 1, sizeof *paths);
  modes = (DISPLAYCONFIG_MODE_INFO *)calloc(mode_count ? mode_count : 1, sizeof *modes);
  if (!paths || !modes) goto done;
  if (QueryDisplayConfig(QDC_ONLY_ACTIVE_PATHS, &count, paths, &mode_count, modes, NULL) !=
      ERROR_SUCCESS)
    goto done;
  for (UINT32 at = 0; at < count; at++) {
    DISPLAYCONFIG_SOURCE_DEVICE_NAME source;
    DISPLAYCONFIG_SDR_WHITE_LEVEL level;

    memset(&source, 0, sizeof source);
    source.header.type = DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME;
    source.header.size = (UINT32)sizeof source;
    source.header.adapterId = paths[at].sourceInfo.adapterId;
    source.header.id = paths[at].sourceInfo.id;
    if (DisplayConfigGetDeviceInfo(&source.header) != ERROR_SUCCESS) continue;
    if (wcscmp(source.viewGdiDeviceName, ours.DeviceName) != 0) continue;
    memset(&level, 0, sizeof level);
    level.header.type = DISPLAYCONFIG_DEVICE_INFO_GET_SDR_WHITE_LEVEL;
    level.header.size = (UINT32)sizeof level;
    level.header.adapterId = paths[at].targetInfo.adapterId;
    level.header.id = paths[at].targetInfo.id;
    if (DisplayConfigGetDeviceInfo(&level.header) == ERROR_SUCCESS && level.SDRWhiteLevel > 0)
      white = (float)level.SDRWhiteLevel / 1000.0f;
    break;
  }

done:
  free(paths);
  free(modes);
  return white;
}

/* The textures the capture owns, and the pipeline that fills one of them. */
static int make_pipeline(yapfast_dup *capture, char *why, size_t size) {
  D3D11_TEXTURE2D_DESC td;
  D3D11_RENDER_TARGET_VIEW_DESC rt;
  D3D11_SHADER_RESOURCE_VIEW_DESC srv;
  D3D11_SAMPLER_DESC sd;
  D3D11_RASTERIZER_DESC rd;
  D3D11_BUFFER_DESC bd;
  ID3DBlob *vertex = NULL;
  ID3DBlob *pixel = NULL;
  HRESULT status;

  /* The desktop image, in the surface's own orientation and format so that
   * CopyResource can put it there: same format, same size, no CPU access, and
   * readable by the shader and by nothing else. */
  memset(&td, 0, sizeof td);
  td.Width = capture->surface_width;
  td.Height = capture->surface_height;
  td.MipLevels = 1;
  td.ArraySize = 1;
  td.Format = capture->format;
  td.SampleDesc.Count = 1;
  td.Usage = D3D11_USAGE_DEFAULT;
  td.BindFlags = D3D11_BIND_SHADER_RESOURCE;
  status = capture->device->CreateTexture2D(&td, NULL, &capture->held);
  if (FAILED(status)) {
    refused(why, size, "CreateTexture2D(the desktop image)", status);
    goto failed;
  }
  memset(&srv, 0, sizeof srv);
  srv.Format = capture->format;
  srv.ViewDimension = D3D11_SRV_DIMENSION_TEXTURE2D;
  srv.Texture2D.MipLevels = 1;
  status = capture->device->CreateShaderResourceView(capture->held, &srv, &capture->held_view);
  if (FAILED(status)) {
    refused(why, size, "CreateShaderResourceView(the desktop image)", status);
    goto failed;
  }

  /* What the caller borrows: upright, BGRA whatever the surface was, and a render
   * target as well as a shader resource because it is drawn into here and read by
   * the encoder there. */
  memset(&td, 0, sizeof td);
  td.Width = capture->desktop_width;
  td.Height = capture->desktop_height;
  td.MipLevels = 1;
  td.ArraySize = 1;
  td.Format = DXGI_FORMAT_B8G8R8A8_UNORM;
  td.SampleDesc.Count = 1;
  td.Usage = D3D11_USAGE_DEFAULT;
  td.BindFlags = D3D11_BIND_RENDER_TARGET | D3D11_BIND_SHADER_RESOURCE;
  status = capture->device->CreateTexture2D(&td, NULL, &capture->image);
  if (FAILED(status)) {
    refused(why, size, "CreateTexture2D(the frame)", status);
    goto failed;
  }
  memset(&rt, 0, sizeof rt);
  rt.Format = DXGI_FORMAT_B8G8R8A8_UNORM;
  rt.ViewDimension = D3D11_RTV_DIMENSION_TEXTURE2D;
  status = capture->device->CreateRenderTargetView(capture->image, &rt, &capture->image_view);
  if (FAILED(status)) {
    refused(why, size, "CreateRenderTargetView(the frame)", status);
    goto failed;
  }

  vertex = compile("vs_main", "vs_4_0", why, size);
  if (!vertex) goto failed;
  status = capture->device->CreateVertexShader(vertex->GetBufferPointer(), vertex->GetBufferSize(),
                                               NULL, &capture->vertex);
  vertex->Release();
  vertex = NULL;
  if (FAILED(status)) {
    refused(why, size, "CreateVertexShader", status);
    goto failed;
  }
  pixel = compile("ps_main", "ps_4_0", why, size);
  if (!pixel) goto failed;
  status = capture->device->CreatePixelShader(pixel->GetBufferPointer(), pixel->GetBufferSize(),
                                              NULL, &capture->pixel);
  pixel->Release();
  pixel = NULL;
  if (FAILED(status)) {
    refused(why, size, "CreatePixelShader", status);
    goto failed;
  }

  /* The surface is sampled at exactly its own pixel centres, so a filter that
   * blends neighbours would only soften an image that has none to soften; the
   * pointer's shape is sampled point-wise for the same reason, and clamping keeps
   * a cursor half off the edge from wrapping round it. */
  memset(&sd, 0, sizeof sd);
  sd.Filter = D3D11_FILTER_MIN_MAG_MIP_POINT;
  sd.AddressU = D3D11_TEXTURE_ADDRESS_CLAMP;
  sd.AddressV = D3D11_TEXTURE_ADDRESS_CLAMP;
  sd.AddressW = D3D11_TEXTURE_ADDRESS_CLAMP;
  sd.ComparisonFunc = D3D11_COMPARISON_NEVER;
  sd.MaxLOD = D3D11_FLOAT32_MAX;
  status = capture->device->CreateSamplerState(&sd, &capture->screen_sampler);
  if (FAILED(status)) {
    refused(why, size, "CreateSamplerState(the surface)", status);
    goto failed;
  }
  status = capture->device->CreateSamplerState(&sd, &capture->shape_sampler);
  if (FAILED(status)) {
    refused(why, size, "CreateSamplerState(the pointer)", status);
    goto failed;
  }

  /* Culling off rather than a winding argued about: the triangle's corners are
   * chosen to cover the viewport, and which way it is seen to turn is not part of
   * anything this file means. */
  memset(&rd, 0, sizeof rd);
  rd.FillMode = D3D11_FILL_SOLID;
  rd.CullMode = D3D11_CULL_NONE;
  rd.DepthClipEnable = TRUE;
  status = capture->device->CreateRasterizerState(&rd, &capture->raster);
  if (FAILED(status)) {
    refused(why, size, "CreateRasterizerState", status);
    goto failed;
  }

  memset(&bd, 0, sizeof bd);
  bd.ByteWidth = (UINT)sizeof(cover);
  bd.Usage = D3D11_USAGE_DYNAMIC;
  bd.BindFlags = D3D11_BIND_CONSTANT_BUFFER;
  bd.CPUAccessFlags = D3D11_CPU_ACCESS_WRITE;
  status = capture->device->CreateBuffer(&bd, NULL, &capture->constants);
  if (FAILED(status)) {
    refused(why, size, "CreateBuffer(the frame's constants)", status);
    goto failed;
  }
  return 1;

failed:
  if (vertex) vertex->Release();
  if (pixel) pixel->Release();
  return 0;
}

/* Opens a duplication of one output on one adapter: the device, the duplication
 * and everything a frame is drawn with. */
extern "C" yapfast_dup *yapfast_dup_open(unsigned luid_high, unsigned luid_low, unsigned output,
                                        char *why, size_t size) {
  yapfast_dup *capture = NULL;
  IDXGIFactory1 *factory = NULL;
  DXGI_OUTDUPL_DESC desc;
  DXGI_ADAPTER_DESC1 card;
  HRESULT status;
  LUID wanted;

  if (why && size) why[0] = 0;
  /* Value-initialised, so every pointer and every count starts at nothing and a
   * failed open can let go of whatever was made before it failed. */
  capture = new (std::nothrow) yapfast_dup();
  if (!capture) {
    say(why, size, "no room for a capture");
    return NULL;
  }
  wanted.HighPart = (LONG)luid_high;
  wanted.LowPart = luid_low;
  capture->luid = wanted;
  capture->output_index = output;

  status = CreateDXGIFactory1(IID_PPV_ARGS(&factory));
  if (FAILED(status)) {
    refused(why, size, "CreateDXGIFactory1", status);
    goto failed;
  }
  capture->adapter = adapter_with(factory, wanted, why, size);
  factory->Release();
  factory = NULL;
  if (!capture->adapter) goto failed;
  memset(&card, 0, sizeof card);
  capture->adapter->GetDesc1(&card);
  snprintf(capture->card, sizeof capture->card, "%ls", card.Description);

  {
    IDXGIOutput *found = NULL;
    status = capture->adapter->EnumOutputs(output, &found);
    if (FAILED(status)) {
      if (status == DXGI_ERROR_NOT_FOUND)
        say(why, size, "%s has no output %u", capture->card, output);
      else
        refused(why, size, "IDXGIAdapter1::EnumOutputs", status);
      goto failed;
    }
    status = found->QueryInterface(IID_PPV_ARGS(&capture->output));
    found->Release();
    if (FAILED(status)) {
      refused(why, size, "QueryInterface(IDXGIOutput1)", status);
      goto failed;
    }
  }

  /* BGRA is what a duplication surface is and what the frame handed out is; the
   * device is made on this adapter because the surface only ever exists there. */
  static const D3D_FEATURE_LEVEL levels[] = {D3D_FEATURE_LEVEL_11_1, D3D_FEATURE_LEVEL_11_0,
                                             D3D_FEATURE_LEVEL_10_0};
  D3D_FEATURE_LEVEL reached;
  status = D3D11CreateDevice((IDXGIAdapter *)capture->adapter, D3D_DRIVER_TYPE_UNKNOWN, NULL,
                             D3D11_CREATE_DEVICE_BGRA_SUPPORT, levels, ARRAYSIZE(levels),
                             D3D11_SDK_VERSION, &capture->device, &reached, &capture->context);
  if (FAILED(status)) {
    say(why, size, "D3D11CreateDevice on %s refused (0x%08lX)", capture->card,
        (unsigned long)status);
    goto failed;
  }

  status = capture->output->DuplicateOutput(capture->device, &capture->duplication);
  if (FAILED(status)) {
    /* E_ACCESSDENIED is the desktop being held by another duplicator or showing a
     * secure one — a lock screen, a UAC prompt — and DXGI_ERROR_UNSUPPORTED is an
     * output or a driver that cannot duplicate at all. Both are worth naming,
     * because the first goes away and the second never does. */
    if (status == E_ACCESSDENIED)
      say(why, size,
          "the display is being duplicated by something else, or is showing a secure desktop "
          "(0x%08lX)",
          (unsigned long)status);
    else if (status == DXGI_ERROR_UNSUPPORTED)
      say(why, size, "output %u of %s cannot be duplicated (0x%08lX)", output, capture->card,
          (unsigned long)status);
    else
      refused(why, size, "IDXGIOutput1::DuplicateOutput", status);
    goto failed;
  }

  memset(&desc, 0, sizeof desc);
  capture->duplication->GetDesc(&desc);
  capture->surface_width = desc.ModeDesc.Width;
  capture->surface_height = desc.ModeDesc.Height;
  capture->format = desc.ModeDesc.Format;
  capture->rotation = desc.Rotation;
  capture->white = sdr_white_level(capture->output);
  if (capture->rotation == DXGI_MODE_ROTATION_ROTATE90 ||
      capture->rotation == DXGI_MODE_ROTATION_ROTATE270) {
    capture->desktop_width = capture->surface_height;
    capture->desktop_height = capture->surface_width;
  } else {
    capture->desktop_width = capture->surface_width;
    capture->desktop_height = capture->surface_height;
  }
  if (capture->desktop_width == 0 || capture->desktop_height == 0) {
    say(why, size, "output %u of %s duplicates at %ux%u", output, capture->card,
        capture->desktop_width, capture->desktop_height);
    goto failed;
  }
  switch (capture->format) {
    case DXGI_FORMAT_B8G8R8A8_UNORM:
    case DXGI_FORMAT_R8G8B8A8_UNORM:
    case DXGI_FORMAT_R10G10B10A2_UNORM:
    case DXGI_FORMAT_R16G16B16A16_FLOAT:
      break;
    default:
      say(why, size, "the surface of output %u of %s is in format %u, which has no conversion here",
          output, capture->card, (unsigned)capture->format);
      goto failed;
  }

  if (!make_pipeline(capture, why, size)) goto failed;
  return capture;

failed:
  if (factory) factory->Release();
  yapfast_dup_close(capture);
  return NULL;
}

/* ----------------------------------------------------------- the frames */

/* Asks for the next frame and draws it, and says which of the three answers this
 * was.
 *
 * The order matters and is the whole of the file: the compositor's surface is
 * copied into a texture this capture owns and released *before* anything else
 * happens, so the pointer and the conversion work on a picture that no longer
 * belongs to the compositor. A pointer-only update arrives with no surface at
 * all, which is why the last one has to be kept. */
extern "C" int yapfast_dup_frame(yapfast_dup *capture, unsigned timeout_ms, void **texture,
                                 char *why, size_t size) {
  DXGI_OUTDUPL_FRAME_INFO info;
  IDXGIResource *resource = NULL;
  ID3D11Texture2D *surface = NULL;
  HRESULT status;
  int fresh = 0;
  int moved = 0;

  if (why && size) why[0] = 0;
  if (!capture || !texture) {
    say(why, size, "no capture to ask for a frame");
    return YAPFAST_DUP_FAILED;
  }
  *texture = capture->painted ? (void *)capture->image : NULL;

  memset(&info, 0, sizeof info);
  status = capture->duplication->AcquireNextFrame(timeout_ms, &info, &resource);
  if (status == DXGI_ERROR_WAIT_TIMEOUT) {
    /* The screen didn't change. Not a failure, and not an empty answer either: a
     * capture that is still working and has nothing to show is what a static
     * desktop is, and a caller told otherwise sleeps on a broken stream. */
    return YAPFAST_DUP_STILL;
  }
  if (status == DXGI_ERROR_ACCESS_LOST) {
    /* The desktop went away from under the duplication: a secure desktop, a
     * display change, or the output being switched off. There is nothing to
     * recover inside this object; the duplication has to be opened again. */
    say(why, size,
        "the duplication was lost (0x%08lX), which is a secure desktop or a display change, so it "
        "has to be opened again",
        (unsigned long)status);
    return YAPFAST_DUP_FAILED;
  }
  if (FAILED(status)) {
    refused(why, size, "IDXGIOutputDuplication::AcquireNextFrame", status);
    return YAPFAST_DUP_FAILED;
  }

  if (resource) {
    status = resource->QueryInterface(IID_PPV_ARGS(&surface));
    resource->Release();
    resource = NULL;
    if (FAILED(status)) {
      capture->duplication->ReleaseFrame();
      refused(why, size, "QueryInterface(ID3D11Texture2D)", status);
      return YAPFAST_DUP_FAILED;
    }
    capture->context->CopyResource(capture->held, surface);
    surface->Release();
    surface = NULL;
    fresh = 1;
  }

  /* The shape is asked for while the frame is still held: `GetFramePointerShape`
   * only answers between an acquire and its release, and answers with
   * DXGI_ERROR_INVALID_CALL outside that window. It comes first because the hot
   * spot that turns the pointer's later position into its corner is the shape's,
   * and a shape that changed is a redraw even when nothing else did — the pointer
   * is in the picture, so a new shape is a new picture. */
  if (info.PointerShapeBufferSize != 0) {
    int admitted = admit_shape(capture, &info, why, size);
    if (!admitted) {
      capture->duplication->ReleaseFrame();
      return YAPFAST_DUP_FAILED;
    }
    moved = 1;
  }

  /* The surface belongs to the compositor until this line and to nothing after
   * it, and a caller that took twenty milliseconds to be asked again would have
   * held the desktop's own buffer for that long. */
  capture->duplication->ReleaseFrame();

  if (info.LastMouseUpdateTime.QuadPart != 0) {
    /* The pointer's position is in desktop coordinates — the upright ones, which
     * is the space the frame is drawn in — so no turn is applied to it. */
    capture->pointer_visible = info.PointerPosition.Visible ? 1 : 0;
    if (info.PointerPosition.Visible) {
      capture->pointer_x = info.PointerPosition.Position.x;
      capture->pointer_y = info.PointerPosition.Position.y;
      capture->pointer_seen = 1;
    }
    moved = 1;
  }

  if (!fresh && !moved) return YAPFAST_DUP_STILL;
  if (!fresh && !capture->painted) {
    /* A pointer that moved before any desktop image has been composited: there is
     * nothing to draw it over, and the next real frame will carry it. */
    return YAPFAST_DUP_STILL;
  }
  if (!paint(capture, why, size)) return YAPFAST_DUP_FAILED;
  capture->painted = 1;
  *texture = (void *)capture->image;
  return fresh ? YAPFAST_DUP_FRAME : YAPFAST_DUP_POINTER;
}

extern "C" void yapfast_dup_content_size(yapfast_dup *capture, unsigned *width, unsigned *height) {
  if (width) *width = capture ? capture->desktop_width : 0;
  if (height) *height = capture ? capture->desktop_height : 0;
}

/* Which way the display is turned, as DXGI numbers it: 1 identity, 2 a quarter
 * clockwise, 3 half, 4 three quarters. The frame is already upright, so this is
 * for a log line rather than for a caller to act on. */
extern "C" unsigned yapfast_dup_rotation(yapfast_dup *capture) {
  return capture ? (unsigned)capture->rotation : 0;
}

/* What the surface was, as the three cases the conversion has: 0 BGRA8, 1
 * RGB10A2, 2 scRGB half-floats, 3 something this file only passes through. */
extern "C" unsigned yapfast_dup_format(yapfast_dup *capture) {
  if (!capture) return YAPFAST_DUP_OTHER;
  switch (capture->format) {
    case DXGI_FORMAT_B8G8R8A8_UNORM:
    case DXGI_FORMAT_R8G8B8A8_UNORM: return YAPFAST_DUP_BGRA8;
    case DXGI_FORMAT_R10G10B10A2_UNORM: return YAPFAST_DUP_RGB10A2;
    case DXGI_FORMAT_R16G16B16A16_FLOAT: return YAPFAST_DUP_SCRGB;
    default: return YAPFAST_DUP_OTHER;
  }
}

/* Whether a visible pointer has been seen at all, which is what a test can check
 * on a machine whose mouse nobody touches: the shape arriving and being drawn is
 * separate from it being there in the frame that happens to be asked for. */
extern "C" int yapfast_dup_pointer_seen(yapfast_dup *capture) {
  return capture ? capture->pointer_seen : 0;
}

/* One pixel of the frame the last call drew, as 0x00RRGGBB, for a test that has
 * to look at the colour rather than at the shape of the answer: a conversion and
 * a rotation can both be wrong in a way a non-null texture doesn't show. It is
 * the only read-back in this file, it copies the frame to a staging texture once
 * and keeps it, and no stream will call it. */
extern "C" int yapfast_dup_sample(yapfast_dup *capture, unsigned x, unsigned y, unsigned *bgra,
                                  char *why, size_t size) {
  D3D11_MAPPED_SUBRESOURCE mapped;
  D3D11_TEXTURE2D_DESC td;
  HRESULT status;

  if (why && size) why[0] = 0;
  if (!capture || !bgra) {
    say(why, size, "nowhere to put a sample");
    return 0;
  }
  if (!capture->painted || !capture->image) {
    say(why, size, "no frame has been drawn to sample");
    return 0;
  }
  if (x >= capture->desktop_width || y >= capture->desktop_height) {
    say(why, size, "(%u,%u) is outside the %ux%u frame", x, y, capture->desktop_width,
        capture->desktop_height);
    return 0;
  }
  if (!capture->stage) {
    capture->image->GetDesc(&td);
    td.Usage = D3D11_USAGE_STAGING;
    td.BindFlags = 0;
    td.CPUAccessFlags = D3D11_CPU_ACCESS_READ;
    td.MiscFlags = 0;
    status = capture->device->CreateTexture2D(&td, NULL, &capture->stage);
    if (FAILED(status)) {
      refused(why, size, "CreateTexture2D(the sample's staging copy)", status);
      return 0;
    }
  }
  capture->context->CopyResource(capture->stage, capture->image);
  status = capture->context->Map(capture->stage, 0, D3D11_MAP_READ, 0, &mapped);
  if (FAILED(status)) {
    refused(why, size, "Map(the sample's staging copy)", status);
    return 0;
  }
  const unsigned char *pixel = (const unsigned char *)mapped.pData + (size_t)y * mapped.RowPitch +
                               (size_t)x * 4;
  *bgra = (unsigned)pixel[0] | ((unsigned)pixel[1] << 8) | ((unsigned)pixel[2] << 16);
  capture->context->Unmap(capture->stage, 0);
  return 1;
}
/* The ID3D11Device the frame's texture is on, for an encoder that has to open its
 * session on that same device. */
extern "C" void *yapfast_dup_device(yapfast_dup *capture) {
  return capture ? (void *)capture->device : NULL;
}

/* What this capture is, in one line: the card, the output, both sizes, the
 * rotation and what the surface's pixels are. */
extern "C" void yapfast_dup_describe(yapfast_dup *capture, char *text, size_t size) {
  static const char *const rotations[] = {"unspecified", "identity", "a quarter clockwise", "half",
                                          "three quarters clockwise"};
  if (!text || !size) return;
  if (!capture) {
    snprintf(text, size, "no capture");
    return;
  }
  unsigned rotation = (unsigned)capture->rotation;
  char pixels[96];
  if (capture->format == DXGI_FORMAT_R16G16B16A16_FLOAT)
    snprintf(pixels, sizeof pixels, "scRGB half-floats, SDR white at %.2f of them",
             capture->white);
  else if (capture->format == DXGI_FORMAT_R10G10B10A2_UNORM)
    snprintf(pixels, sizeof pixels, "10-bit RGB, passed through");
  else
    snprintf(pixels, sizeof pixels, "8-bit BGRA, passed through");
  snprintf(text, size, "output %u of %s, %ux%u from a %ux%u surface, rotation %s, %s",
           capture->output_index, capture->card[0] ? capture->card : "(an unnamed adapter)",
           capture->desktop_width, capture->desktop_height, capture->surface_width,
           capture->surface_height, rotation < ARRAYSIZE(rotations) ? rotations[rotation] : "a mode",
           pixels);
}

/* The primary display's adapter and output, for a caller that hasn't been told
 * which screen to capture. The primary display is the one the desktop starts on —
 * its rectangle's origin is the desktop's — and that is what a test on a machine
 * with one screen, or several, can rely on. */
extern "C" int yapfast_dup_primary(unsigned *luid_high, unsigned *luid_low, unsigned *output,
                                   char *why, size_t size) {
  IDXGIFactory1 *factory = NULL;
  IDXGIAdapter1 *adapter = NULL;
  HRESULT status;
  int found = 0;

  if (why && size) why[0] = 0;
  if (!luid_high || !luid_low || !output) {
    say(why, size, "nowhere to put the primary display's name");
    return 0;
  }
  status = CreateDXGIFactory1(IID_PPV_ARGS(&factory));
  if (FAILED(status)) {
    refused(why, size, "CreateDXGIFactory1", status);
    return 0;
  }
  for (unsigned at = 0; !found; at++) {
    DXGI_ADAPTER_DESC1 card;
    if (factory->EnumAdapters1(at, &adapter) == DXGI_ERROR_NOT_FOUND) break;
    memset(&card, 0, sizeof card);
    adapter->GetDesc1(&card);
    for (unsigned index = 0; !found; index++) {
      IDXGIOutput *screen = NULL;
      DXGI_OUTPUT_DESC desc;
      if (adapter->EnumOutputs(index, &screen) == DXGI_ERROR_NOT_FOUND) break;
      memset(&desc, 0, sizeof desc);
      if (SUCCEEDED(screen->GetDesc(&desc)) && desc.AttachedToDesktop &&
          desc.DesktopCoordinates.left == 0 && desc.DesktopCoordinates.top == 0) {
        *luid_high = (unsigned)card.AdapterLuid.HighPart;
        *luid_low = (unsigned)card.AdapterLuid.LowPart;
        *output = index;
        found = 1;
      }
      screen->Release();
    }
    adapter->Release();
    adapter = NULL;
  }
  factory->Release();
  if (!found) say(why, size, "no adapter has an output at the origin of the desktop");
  return found;
}

/* Lets everything go. Called once by Rust's wrapper, and by a failed open on its
 * way out, so every pointer here may be null. */
extern "C" void yapfast_dup_close(yapfast_dup *capture) {
  if (!capture) return;
  if (capture->duplication) capture->duplication->Release();
  if (capture->shape_view) capture->shape_view->Release();
  if (capture->shape) capture->shape->Release();
  if (capture->constants) capture->constants->Release();
  if (capture->raster) capture->raster->Release();
  if (capture->screen_sampler) capture->screen_sampler->Release();
  if (capture->shape_sampler) capture->shape_sampler->Release();
  if (capture->pixel) capture->pixel->Release();
  if (capture->vertex) capture->vertex->Release();
  if (capture->image_view) capture->image_view->Release();
  if (capture->stage) capture->stage->Release();
  if (capture->image) capture->image->Release();
  if (capture->held_view) capture->held_view->Release();
  if (capture->held) capture->held->Release();
  if (capture->context) capture->context->Release();
  if (capture->device) capture->device->Release();
  if (capture->output) capture->output->Release();
  if (capture->adapter) capture->adapter->Release();
  free(capture->shape_bytes);
  free(capture->shape_pixels);
  delete capture;
}
