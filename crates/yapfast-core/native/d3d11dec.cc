// Shading a watched stream's frames on the graphics card: FFmpeg's own
// Direct3D 11 decoder (native/hwdec.c) leaves every picture in a D3D11
// texture on the device the encoder and the window share, and a video
// processor pass does the colour matrix, the range and the scale to the tile
// in one go, so only the tile is ordered back to the processor. FFmpeg's
// decoder is the card's one decoder here, and the codecs it takes are the
// ones Discord's streams are coded with. See `decode.rs` for how a stream
// falls back to the software decoders when any of this isn't there.
//
// Everything below the decode — the device, the video processor, the tile
// slots and their readback — reads the picture FFmpeg's decoder leaves, and
// none of it wants a COM apartment or Media Foundation started: the decoder
// is handed the device and reads the access unit, as native/hwdec.c does.
//
// C++, because the Direct3D 11 interfaces below are used through their C++
// vtables here while the rest of native/ is C.

// This file is compiled only for Windows, where the SDK headers below
// exist; built anywhere else it is an empty unit rather than a file no
// compiler outside Windows can parse. build.rs defines the macro on the
// one target it builds for.
#if defined(YAPFAST_WINDOWS)
#include <windows.h>
#include <initguid.h>
#include <d3d11.h>
#include <d3d11_1.h>
#include <dxgi1_2.h>

#include <cstdint>
#include <cstdio>
#include <cstring>

// FFmpeg's d3d11va decoder, behind plain C values: hwdec.h carries no language
// guard of its own, so the one C++ file that includes it says which it is.
extern "C" {
#include "hwdec.h"
}

// The video processor's content and colour-space descriptions are made for one
// picture and one tile; a stream that changes either makes them again.
//
// The frame size a decoder is opened with is only a hint: the stream's first
// frame says the real one, and the surfaces are made for that, which is how a
// tile smaller than the stream still decodes it whole and scales it down on
// the card.
namespace {

struct Decoder {
    ID3D11Device *device = nullptr;
    ID3D11DeviceContext *context = nullptr;
    ID3D11VideoDevice *video_device = nullptr;
    ID3D11VideoContext *video_context = nullptr;
    ID3D11VideoContext1 *video_context1 = nullptr;

    // FFmpeg's own Direct3D 11 decoder, where this build has one: it decodes
    // into a texture on `device`, and the pass below turns a slice of it into
    // the tile.
    struct yapfast_ffdec *ffdec = nullptr;

    // The newest picture the decoder made, and what it is: a texture may hold
    // several pictures, one per subresource.
    ID3D11Texture2D *picture = nullptr;
    UINT picture_slice = 0;
    int width = 0;
    int height = 0;
    // The picture inside that surface, which is a macroblock or superblock
    // taller than the stream often is: the video processor reads this rect
    // and nothing outside it.
    RECT visible = {0, 0, 0, 0};

    // The colour the stream says its pictures are, which the video processor
    // is told; 601 limited where the stream says nothing, as the software
    // conversion reads them.
    bool full_range = false;
    bool bt709 = false;
    bool bt2020 = false;
    bool pq = false;

    // The video processor, made for one picture size and one tile size.
    ID3D11VideoProcessorEnumerator *enumerator = nullptr;
    ID3D11VideoProcessor *processor = nullptr;
    // Two tiles and two staging surfaces, because a copy the card is still
    // making is one the processor must not wait on: a frame's tile is copied
    // out while the next frame is decoded, so the only picture read back is
    // one the card finished with a frame ago.
    ID3D11Texture2D *tile[2] = {nullptr, nullptr};
    ID3D11Texture2D *staging[2] = {nullptr, nullptr};
    ID3D11VideoProcessorInputView *input_view = nullptr;
    ID3D11VideoProcessorOutputView *output_view[2] = {nullptr, nullptr};
    // The staging surface whose copy was submitted last, and so the one to
    // read; -1 before the first frame is submitted.
    int pending = -1;
    int next = 0;
    ID3D11Texture2D *input_view_picture = nullptr;
    UINT input_view_slice = 0;
    int processor_width = 0;
    int processor_height = 0;
    int tile_width = 0;
    int tile_height = 0;
    // The tile's pixels come out of the video processor as B, G, R, A: the
    // processor writes its RGB surface that way, and the frames the UI reads
    // are R, G, B, A.
    bool swizzle = false;

    char error[256] = {0};
};

void fail(Decoder *decoder, const char *what, HRESULT result) {
    std::snprintf(
        decoder->error,
        sizeof decoder->error,
        "%s (%08lx)",
        what,
        static_cast<unsigned long>(result)
    );
}

void fail_text(Decoder *decoder, const char *what) {
    std::snprintf(decoder->error, sizeof decoder->error, "%s", what);
}

void release_all(Decoder *decoder) {
    if (decoder->input_view) decoder->input_view->Release();
    for (int slot = 0; slot < 2; ++slot) {
        if (decoder->output_view[slot]) decoder->output_view[slot]->Release();
        if (decoder->staging[slot]) decoder->staging[slot]->Release();
        if (decoder->tile[slot]) decoder->tile[slot]->Release();
    }
    if (decoder->processor) decoder->processor->Release();
    if (decoder->enumerator) decoder->enumerator->Release();
    if (decoder->picture) decoder->picture->Release();
    if (decoder->video_context1) decoder->video_context1->Release();
    if (decoder->video_context) decoder->video_context->Release();
    if (decoder->video_device) decoder->video_device->Release();
    if (decoder->context) decoder->context->Release();
    if (decoder->device) decoder->device->Release();
}

// The reason a decoder that never got made wasn't: there is no decoder to keep
// it in, and the caller asks for it by the null it got back.
thread_local char g_error[256] = {0};

// Lets a decoder that couldn't be made go, keeping its reason for the caller
// that gets the null back.
Decoder *abandon(Decoder *decoder) {
    std::snprintf(g_error, sizeof g_error, "%s", decoder->error);
    if (decoder->ffdec) yapfast_ffdec_free(decoder->ffdec);
    release_all(decoder);
    delete decoder;
    return nullptr;
}

}  // namespace

extern "C" {

typedef Decoder yf_hwdec;

// The machine's decoder for `codec` (0 H.264, 1 HEVC, 2 AV1), on the
// graphics card, through FFmpeg's Direct3D 11 decoder, on `device` where the
// caller brought one — an ID3D11Device it keeps alive, the encoder's and the
// window's, so nothing is copied between them — and on a device made here
// where it didn't. Null when that decoder isn't on this machine, and the
// reason is yapfast_hwdec_error(null).
yf_hwdec *yapfast_hwdec_new(int codec, int hint_width, int hint_height, void *device);
// Feeds one whole frame. 1 says a picture is ready, 0 that the decoder kept
// the frame for the pictures after it, -2 that the stream's pictures are ones
// the tile pass can't show rather than a failure of the decoder, and -1 that it
// failed and must be dropped.
int yapfast_hwdec_decode(yf_hwdec *decoder, const unsigned char *data, size_t size, int keyframe);
// The picture's size, as the decoder made it.
int yapfast_hwdec_size(const yf_hwdec *decoder, int *width, int *height);
// The picture, in the stream's colour and scaled to `width` by `height`, as
// RGBA of `width * 4` bytes a row: the tile submitted on the call before, so
// that the processor never waits on the card's pipeline, and this one on the
// first call. 0 when it was written.
int yapfast_hwdec_rgba(yf_hwdec *decoder, unsigned char *rgba, int width, int height);
// Why the last call failed, for a log to quote.
const char *yapfast_hwdec_error(const yf_hwdec *decoder);
void yapfast_hwdec_free(yf_hwdec *decoder);

}  // extern "C"

namespace {

// The color space the video processor is told the pictures are in, from the
// matrix, range and transfer the stream announced.
DXGI_COLOR_SPACE_TYPE stream_colour_space(const Decoder *decoder) {
    if (decoder->bt2020) {
        if (decoder->pq) return DXGI_COLOR_SPACE_YCBCR_STUDIO_G2084_TOPLEFT_P2020;
        return decoder->full_range ? DXGI_COLOR_SPACE_YCBCR_FULL_G22_LEFT_P2020
                                   : DXGI_COLOR_SPACE_YCBCR_STUDIO_G22_TOPLEFT_P2020;
    }
    if (decoder->bt709)
        return decoder->full_range ? DXGI_COLOR_SPACE_YCBCR_FULL_G22_LEFT_P709
                                   : DXGI_COLOR_SPACE_YCBCR_STUDIO_G22_LEFT_P709;
    return decoder->full_range ? DXGI_COLOR_SPACE_YCBCR_FULL_G22_LEFT_P601
                               : DXGI_COLOR_SPACE_YCBCR_STUDIO_G22_LEFT_P601;
}

// Makes the video processor and the surfaces it needs for one picture size and
// one tile size, and says whether the tile comes out of it as B, G, R, A.
bool build_processor(Decoder *decoder, int width, int height, int tile_width, int tile_height) {
    if (decoder->input_view) {
        decoder->input_view->Release();
        decoder->input_view = nullptr;
        decoder->input_view_picture = nullptr;
    }
    for (int slot = 0; slot < 2; ++slot) {
        if (decoder->output_view[slot]) {
            decoder->output_view[slot]->Release();
            decoder->output_view[slot] = nullptr;
        }
        if (decoder->staging[slot]) {
            decoder->staging[slot]->Release();
            decoder->staging[slot] = nullptr;
        }
        if (decoder->tile[slot]) {
            decoder->tile[slot]->Release();
            decoder->tile[slot] = nullptr;
        }
    }
    decoder->pending = -1;
    decoder->next = 0;
    if (decoder->processor) {
        decoder->processor->Release();
        decoder->processor = nullptr;
    }
    if (decoder->enumerator) {
        decoder->enumerator->Release();
        decoder->enumerator = nullptr;
    }
    D3D11_VIDEO_PROCESSOR_CONTENT_DESC content = {};
    content.InputFrameFormat = D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE;
    content.InputWidth = static_cast<UINT>(width);
    content.InputHeight = static_cast<UINT>(height);
    content.OutputWidth = static_cast<UINT>(tile_width);
    content.OutputHeight = static_cast<UINT>(tile_height);
    content.Usage = D3D11_VIDEO_USAGE_PLAYBACK_NORMAL;
    HRESULT result = decoder->video_device->CreateVideoProcessorEnumerator(&content, &decoder->enumerator);
    if (FAILED(result)) {
        fail(decoder, "the video processor can't take this picture", result);
        return false;
    }
    result = decoder->video_device->CreateVideoProcessor(decoder->enumerator, 0, &decoder->processor);
    if (FAILED(result)) {
        fail(decoder, "the video processor wouldn't start", result);
        return false;
    }

    // R, G, B, A where the processor can write it, which saves turning every
    // pixel round on the way to the frames; B, G, R, A where it can't.
    UINT support = 0;
    decoder->enumerator->CheckVideoProcessorFormat(DXGI_FORMAT_R8G8B8A8_UNORM, &support);
    const DXGI_FORMAT format =
        (support & D3D11_VIDEO_PROCESSOR_FORMAT_SUPPORT_OUTPUT) ? DXGI_FORMAT_R8G8B8A8_UNORM : DXGI_FORMAT_B8G8R8A8_UNORM;
    decoder->swizzle = format == DXGI_FORMAT_B8G8R8A8_UNORM;
    D3D11_TEXTURE2D_DESC description = {};
    description.Width = static_cast<UINT>(tile_width);
    description.Height = static_cast<UINT>(tile_height);
    description.MipLevels = 1;
    description.ArraySize = 1;
    description.Format = format;
    description.SampleDesc.Count = 1;
    description.Usage = D3D11_USAGE_DEFAULT;
    description.BindFlags = D3D11_BIND_RENDER_TARGET;
    D3D11_TEXTURE2D_DESC staging = description;
    staging.Usage = D3D11_USAGE_STAGING;
    staging.BindFlags = 0;
    staging.CPUAccessFlags = D3D11_CPU_ACCESS_READ;
    D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC output = {};
    output.ViewDimension = D3D11_VPOV_DIMENSION_TEXTURE2D;
    output.Texture2D.MipSlice = 0;
    for (int slot = 0; slot < 2; ++slot) {
        result = decoder->device->CreateTexture2D(&description, nullptr, &decoder->tile[slot]);
        if (FAILED(result)) {
            fail(decoder, "the tile the picture is scaled into wasn't made", result);
            return false;
        }
        result = decoder->device->CreateTexture2D(&staging, nullptr, &decoder->staging[slot]);
        if (FAILED(result)) {
            fail(decoder, "the staging surface the tile comes back through wasn't made", result);
            return false;
        }
        result = decoder->video_device->CreateVideoProcessorOutputView(decoder->tile[slot], decoder->enumerator, &output,
                                                                      &decoder->output_view[slot]);
        if (FAILED(result)) {
            fail(decoder, "the tile isn't a video processor output", result);
            return false;
        }
    }

    // The matrix, the range and the transfer from the stream; RGB out, full
    // range, with the processor doing the scale in the same pass.
    if (decoder->video_context1) {
        decoder->video_context1->VideoProcessorSetStreamColorSpace1(decoder->processor, 0, stream_colour_space(decoder));
        decoder->video_context1->VideoProcessorSetOutputColorSpace1(decoder->processor, DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709);
    } else {
        D3D11_VIDEO_PROCESSOR_COLOR_SPACE in = {};
        in.Usage = 0;
        in.RGB_Range = 0;
        in.YCbCr_Matrix = decoder->bt709 || decoder->bt2020;
        in.Nominal_Range = decoder->full_range ? D3D11_VIDEO_PROCESSOR_NOMINAL_RANGE_0_255
                                               : D3D11_VIDEO_PROCESSOR_NOMINAL_RANGE_16_235;
        decoder->video_context->VideoProcessorSetStreamColorSpace(decoder->processor, 0, &in);
        D3D11_VIDEO_PROCESSOR_COLOR_SPACE out = {};
        out.Usage = 0;
        out.RGB_Range = 0;
        out.Nominal_Range = D3D11_VIDEO_PROCESSOR_NOMINAL_RANGE_0_255;
        decoder->video_context->VideoProcessorSetOutputColorSpace(decoder->processor, &out);
    }
    decoder->video_context->VideoProcessorSetStreamFrameFormat(decoder->processor, 0, D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE);
    RECT source = decoder->visible;
    decoder->video_context->VideoProcessorSetStreamSourceRect(decoder->processor, 0, TRUE, &source);
    RECT target = {0, 0, tile_width, tile_height};
    decoder->video_context->VideoProcessorSetStreamDestRect(decoder->processor, 0, TRUE, &target);
    decoder->video_context->VideoProcessorSetOutputTargetRect(decoder->processor, FALSE, nullptr);
    decoder->processor_width = width;
    decoder->processor_height = height;
    decoder->tile_width = tile_width;
    decoder->tile_height = tile_height;
    return true;
}

}  // namespace

namespace {

// The card the pictures are decoded and shown on: the caller's device where it
// brought one — the encoder's and the window's, so a decoded picture is never
// copied between devices — and one made here where it didn't, with the video
// features the decoder and the processor both need. A build with no FFmpeg
// decoder still compiles the pass and has no decoder to hand a device to, so
// this one helper goes uncalled there.
[[maybe_unused]] bool take_device(Decoder *decoder, void *given) {
    HRESULT result;
    if (given) {
        // Borrowed for this decoder's life, which the caller keeps it for; the
        // reference taken here is the one release_all gives back.
        decoder->device = static_cast<ID3D11Device *>(given);
        decoder->device->AddRef();
    } else {
        const UINT flags = D3D11_CREATE_DEVICE_BGRA_SUPPORT | D3D11_CREATE_DEVICE_VIDEO_SUPPORT;
        result = D3D11CreateDevice(nullptr, D3D_DRIVER_TYPE_HARDWARE, nullptr, flags, nullptr, 0,
                                   D3D11_SDK_VERSION, &decoder->device, nullptr, &decoder->context);
        if (FAILED(result)) {
            fail(decoder, "the graphics card was refused", result);
            return false;
        }
    }
    if (!decoder->context) decoder->device->GetImmediateContext(&decoder->context);
    decoder->device->QueryInterface(IID_PPV_ARGS(&decoder->video_device));
    decoder->context->QueryInterface(IID_PPV_ARGS(&decoder->video_context));
    decoder->context->QueryInterface(IID_PPV_ARGS(&decoder->video_context1));
    if (!decoder->video_device || !decoder->video_context) {
        fail_text(decoder, "the graphics card has no video processor");
        return false;
    }
    return true;
}

// FFmpeg's decoder's newest picture, adopted as this file's: the texture is one
// slice of the array texture FFmpeg's surfaces live in, and the tile pass below
// reads that slice and nothing else.
int adopt_ffmpeg(Decoder *decoder, const unsigned char *data, size_t size, int keyframe) {
    decoder->error[0] = 0;
    const int decoded = yapfast_ffdec_decode(decoder->ffdec, data, size, keyframe);
    if (decoded < 0) {
        std::snprintf(decoder->error, sizeof decoder->error, "%s", yapfast_ffdec_error(decoder->ffdec));
        return decoded;
    }
    if (decoded == 0) return 0;
    yapfast_ffpicture picture = {};
    if (yapfast_ffdec_picture(decoder->ffdec, &picture) != 0) {
        fail_text(decoder, "the card gave a picture with no texture");
        return -1;
    }
    auto *texture = static_cast<ID3D11Texture2D *>(picture.texture);
    // This keeps the array texture alive past the frames context that made it,
    // which a stream that changes size and format frees. The slice is no
    // reference to hold: the frame's second plane is only its index, and
    // av_frame_unref gives it back to the pool for the next decode to write.
    // What keeps the tile pass's read of it honest is the immediate context:
    // FFmpeg's decoder writes the slice and this file's video processor reads
    // it on the same one, so the blit is ordered before the next decode's
    // writes.
    texture->AddRef();
    if (decoder->picture) decoder->picture->Release();
    decoder->picture = texture;
    decoder->picture_slice = picture.index;
    D3D11_TEXTURE2D_DESC description = {};
    texture->GetDesc(&description);
    decoder->width = static_cast<int>(description.Width);
    decoder->height = static_cast<int>(description.Height);
    // The stream's own size, not the padded surface the decoder made it in:
    // this is the size its tile is fitted to.
    decoder->visible = {0, 0, picture.width, picture.height};
    decoder->full_range = picture.full_range != 0;
    decoder->bt709 = picture.bt709 != 0;
    decoder->bt2020 = picture.bt2020 != 0;
    decoder->pq = picture.pq != 0;
    return 1;
}

}  // namespace

extern "C" yf_hwdec *yapfast_hwdec_new(int codec, int hint_width, int hint_height, void *device) {
    auto *decoder = new Decoder();
#ifdef YAPFAST_FFDEC
    if (!take_device(decoder, device)) return abandon(decoder);
    decoder->ffdec = yapfast_ffdec_new(codec, hint_width, hint_height, decoder->device);
    if (!decoder->ffdec) {
        fail_text(decoder, yapfast_ffdec_error(nullptr));
        return abandon(decoder);
    }
    return decoder;
#else
    (void)codec;
    (void)hint_width;
    (void)hint_height;
    (void)device;
    fail_text(decoder, "this build has no FFmpeg hardware decoder");
    return abandon(decoder);
#endif
}

extern "C" int yapfast_hwdec_decode(yf_hwdec *decoder, const unsigned char *data, size_t size, int keyframe) {
    if (!decoder || !decoder->ffdec) return -1;
    return adopt_ffmpeg(decoder, data, size, keyframe);
}

extern "C" int yapfast_hwdec_size(const yf_hwdec *decoder, int *width, int *height) {
    if (!decoder || !decoder->picture) return 0;
    // The stream's own size, not the padded surface the decoder made it in:
    // this is the size its tile is fitted to.
    *width = decoder->visible.right - decoder->visible.left;
    *height = decoder->visible.bottom - decoder->visible.top;
    return 1;
}

extern "C" int yapfast_hwdec_rgba(yf_hwdec *decoder, unsigned char *rgba, int width, int height) {
    if (!decoder || !decoder->picture) {
        if (decoder) fail_text(decoder, "no picture to convert");
        return -1;
    }
    decoder->error[0] = 0;
    if (!decoder->processor || decoder->processor_width != decoder->width || decoder->processor_height != decoder->height ||
        decoder->tile_width != width || decoder->tile_height != height) {
        if (!build_processor(decoder, decoder->width, decoder->height, width, height)) return -1;
    }
    if (decoder->input_view_picture != decoder->picture || decoder->input_view_slice != decoder->picture_slice) {
        if (decoder->input_view) decoder->input_view->Release();
        D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC view = {};
        view.FourCC = 0;
        view.ViewDimension = D3D11_VPIV_DIMENSION_TEXTURE2D;
        view.Texture2D.MipSlice = 0;
        view.Texture2D.ArraySlice = decoder->picture_slice;
        HRESULT result =
            decoder->video_device->CreateVideoProcessorInputView(decoder->picture, decoder->enumerator, &view, &decoder->input_view);
        if (FAILED(result)) {
            decoder->input_view = nullptr;
            fail(decoder, "the picture isn't a video processor input", result);
            return -1;
        }
        decoder->input_view_picture = decoder->picture;
        decoder->input_view_slice = decoder->picture_slice;
    }
    const int slot = decoder->next;
    decoder->next ^= 1;
    D3D11_VIDEO_PROCESSOR_STREAM stream = {};
    stream.Enable = TRUE;
    stream.pInputSurface = decoder->input_view;
    HRESULT result =
        decoder->video_context->VideoProcessorBlt(decoder->processor, decoder->output_view[slot], 0, 1, &stream);
    if (FAILED(result)) {
        fail(decoder, "the picture wasn't converted", result);
        return -1;
    }
    decoder->context->CopyResource(decoder->staging[slot], decoder->tile[slot]);
    // The surface copied on the call before this one, which the card has had a
    // frame to finish; the first picture has no earlier copy and is waited on.
    const int read = decoder->pending >= 0 ? decoder->pending : slot;
    decoder->pending = slot;
    D3D11_MAPPED_SUBRESOURCE mapped = {};
    result = decoder->context->Map(decoder->staging[read], 0, D3D11_MAP_READ, 0, &mapped);
    if (FAILED(result)) {
        fail(decoder, "the tile wasn't read back", result);
        return -1;
    }
    const auto *source = static_cast<const unsigned char *>(mapped.pData);
    const size_t row = static_cast<size_t>(width) * 4;
    for (int y = 0; y < height; ++y) {
        const unsigned char *line = source + static_cast<size_t>(y) * mapped.RowPitch;
        unsigned char *out = rgba + static_cast<size_t>(y) * row;
        if (!decoder->swizzle) {
            std::memcpy(out, line, row);
            continue;
        }
        for (int x = 0; x < width; ++x) {
            out[x * 4 + 0] = line[x * 4 + 2];
            out[x * 4 + 1] = line[x * 4 + 1];
            out[x * 4 + 2] = line[x * 4 + 0];
            out[x * 4 + 3] = line[x * 4 + 3];
        }
    }
    decoder->context->Unmap(decoder->staging[read], 0);
    return 0;
}

// The newest picture's 8-bit planes, the visible part of them, as three
// separate planes: what a test needs to convert the decoder's own samples the
// way the software path does and compare the two conversions of one picture.
// Nothing in the watch path calls this.
extern "C" int yapfast_hwdec_nv12(
    yf_hwdec *decoder,
    unsigned char *y,
    int y_stride,
    unsigned char *u,
    int u_stride,
    unsigned char *v,
    int v_stride
) {
    if (!decoder || !decoder->picture) return -1;
    const int width = decoder->visible.right - decoder->visible.left;
    const int height = decoder->visible.bottom - decoder->visible.top;
    if (width <= 0 || height <= 0 || y_stride < width || u_stride < (width + 1) / 2 || v_stride < (width + 1) / 2)
        return -1;
    D3D11_TEXTURE2D_DESC description = {};
    decoder->picture->GetDesc(&description);
    D3D11_TEXTURE2D_DESC staging = description;
    staging.Usage = D3D11_USAGE_STAGING;
    staging.BindFlags = 0;
    staging.CPUAccessFlags = D3D11_CPU_ACCESS_READ;
    staging.MiscFlags = 0;
    staging.ArraySize = 1;
    staging.MipLevels = 1;
    ID3D11Texture2D *readable = nullptr;
    HRESULT result = decoder->device->CreateTexture2D(&staging, nullptr, &readable);
    if (FAILED(result)) {
        fail(decoder, "the picture's planes weren't readable", result);
        return -1;
    }
    D3D11_BOX box = {};
    box.right = description.Width;
    box.bottom = description.Height;
    box.back = 1;
    decoder->context->CopySubresourceRegion(readable, 0, 0, 0, 0, decoder->picture, decoder->picture_slice, &box);
    D3D11_MAPPED_SUBRESOURCE mapped = {};
    result = decoder->context->Map(readable, 0, D3D11_MAP_READ, 0, &mapped);
    if (FAILED(result)) {
        readable->Release();
        fail(decoder, "the picture's planes weren't readable", result);
        return -1;
    }
    const auto *base = static_cast<const unsigned char *>(mapped.pData);
    for (int row = 0; row < height; ++row)
        std::memcpy(y + static_cast<size_t>(row) * y_stride, base + static_cast<size_t>(row) * mapped.RowPitch, width);
    const auto *chroma = base + static_cast<size_t>(description.Height) * mapped.RowPitch;
    for (int row = 0; row < height / 2; ++row) {
        const unsigned char *line = chroma + static_cast<size_t>(row) * mapped.RowPitch;
        for (int column = 0; column < width / 2; ++column) {
            u[static_cast<size_t>(row) * u_stride + column] = line[column * 2];
            v[static_cast<size_t>(row) * v_stride + column] = line[column * 2 + 1];
        }
    }
    decoder->context->Unmap(readable, 0);
    readable->Release();
    return 0;
}

extern "C" const char *yapfast_hwdec_error(const yf_hwdec *decoder) {
    return decoder ? decoder->error : g_error;
}

extern "C" void yapfast_hwdec_free(yf_hwdec *decoder) {
    if (!decoder) return;
    if (decoder->ffdec) yapfast_ffdec_free(decoder->ffdec);
    release_all(decoder);
    delete decoder;
}

#endif  // YAPFAST_WINDOWS
