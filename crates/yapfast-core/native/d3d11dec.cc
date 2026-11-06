// Guilding a watched stream's frames on the graphics card: the machine's own
// H.264, HEVC or AV1 decoder, which leaves every picture in a D3D11 texture,
// and a video processor pass that does the colour matrix, the range and the
// scale to the tile in one go, so only the tile ordered back to the
// processor. See `decode.rs` for how a stream falls back to the software
// decoders when any of this isn't there.
//
// C++, because Media Foundation's interfaces are handed over through COM's
// C++ vtables here while the rest of native/ is C.
#include <windows.h>
#include <initguid.h>
#include <d3d11.h>
#include <d3d11_1.h>
#include <dxgi1_2.h>
#include <mfapi.h>
#include <mferror.h>
#include <mfidl.h>

#include <cstdint>
#include <cstdio>
#include <cstring>

// The video processor's content and colour-space descriptions, and the
// decoder's output type, are made for one picture and one tile; a stream that
// changes either makes them again.
//
// The frame size a decoder is opened with is only a hint: the first frame's
// sequence header says the real one, and the decoder answers with a stream
// change, which is how a tile smaller than the stream still decodes it whole
// and scales it down on the card.
namespace {

constexpr int kH264 = 0;
constexpr int kHevc = 1;
constexpr int kAv1 = 2;

// A decoder opened at the size it's likely to be: the tile showing the stream,
// never smaller than a stream is encoded at. Only the first frames are made at
// this size — the stream's own sequence header replaces it before anything is
// shown.
constexpr UINT32 kMinHintWidth = 320;
constexpr UINT32 kMinHintHeight = 180;

struct Decoder {
    int codec = kH264;
    // This decoder initialized COM on the thread that calls it.
    bool com = false;

    ID3D11Device *device = nullptr;
    ID3D11DeviceContext *context = nullptr;
    ID3D11VideoDevice *video_device = nullptr;
    ID3D11VideoContext *video_context = nullptr;
    ID3D11VideoContext1 *video_context1 = nullptr;
    IMFDXGIDeviceManager *manager = nullptr;

    IMFTransform *decoder = nullptr;
    IMFMediaType *output_type = nullptr;

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

    int64_t time = 0;
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
    if (decoder->output_type) decoder->output_type->Release();
    if (decoder->decoder) decoder->decoder->Release();
    if (decoder->manager) decoder->manager->Release();
    if (decoder->video_context1) decoder->video_context1->Release();
    if (decoder->video_context) decoder->video_context->Release();
    if (decoder->video_device) decoder->video_device->Release();
    if (decoder->context) decoder->context->Release();
    if (decoder->device) decoder->device->Release();
}

const GUID &subtype_of(int codec) {
    switch (codec) {
        case kHevc:
            return MFVideoFormat_HEVC;
        case kAv1:
            return MFVideoFormat_AV1;
        default:
            return MFVideoFormat_H264;
    }
}

// The machine's decoder for this codec, chosen by what it says it takes: a
// decoder that runs on the machine's own thread of events rather than
// answering when asked is passed over, since a watched stream asks for its
// picture as soon as the frame arrives and waits for none.
IMFTransform *find_decoder(int codec) {
    IMFActivate **activates = nullptr;
    UINT32 count = 0;
    if (FAILED(MFTEnumEx(MFT_CATEGORY_VIDEO_DECODER, MFT_ENUM_FLAG_ALL, nullptr, nullptr, &activates, &count)))
        return nullptr;
    const GUID &wanted = subtype_of(codec);
    IMFTransform *chosen = nullptr;
    for (UINT32 i = 0; i < count && !chosen; ++i) {
        UINT32 is_async = 0;
        activates[i]->GetUINT32(MF_TRANSFORM_ASYNC, &is_async);
        if (is_async) continue;
        activates[i]->SetUINT32(MF_TRANSFORM_ASYNC_UNLOCK, TRUE);
        IMFTransform *transform = nullptr;
        if (FAILED(activates[i]->ActivateObject(IID_IMFTransform, reinterpret_cast<void **>(&transform))) || !transform)
            continue;
        for (DWORD k = 0;; ++k) {
            IMFMediaType *type = nullptr;
            if (FAILED(transform->GetInputAvailableType(0, k, &type))) break;
            GUID subtype = {};
            type->GetGUID(MF_MT_SUBTYPE, &subtype);
            type->Release();
            if (IsEqualGUID(subtype, wanted)) {
                chosen = transform;
                break;
            }
        }
        if (!chosen) transform->Release();
    }
    for (UINT32 i = 0; i < count; ++i) activates[i]->Release();
    CoTaskMemFree(activates);
    return chosen;
}

// What the stream's own metadata says its pictures are, read off the type the
// decoder settled on. Untagged 8-bit content is BT.601 limited, which is what
// libyuv's conversion of the software path assumes and what the stream is
// usually shown with today.
void read_colour(Decoder *decoder, IMFMediaType *type) {
    UINT32 matrix = 0;
    if (SUCCEEDED(type->GetUINT32(MF_MT_YUV_MATRIX, &matrix))) {
        decoder->bt709 = matrix == MFVideoTransferMatrix_BT709;
        decoder->bt2020 = matrix == MFVideoTransferMatrix_BT2020_10 || matrix == MFVideoTransferMatrix_BT2020_12;
    }
    UINT32 range = 0;
    if (SUCCEEDED(type->GetUINT32(MF_MT_VIDEO_NOMINAL_RANGE, &range)))
        decoder->full_range = range == MFNominalRange_0_255;
    UINT32 transfer = 0;
    if (SUCCEEDED(type->GetUINT32(MF_MT_TRANSFER_FUNCTION, &transfer)))
        decoder->pq = transfer == MFVideoTransFunc_2084;
}

// Takes the type the decoder offers for a picture, which must be 8-bit NV12:
// a 10- or 12-bit picture (P010) would have to be tonemapped to 8-bit SDR
// before it could be shown, and this pass has no curve, so such a stream stays
// on the software decoders, which already hand back SDR.
bool settle_output(Decoder *decoder) {
    IMFMediaType *type = nullptr;
    HRESULT result = decoder->decoder->GetOutputAvailableType(0, 0, &type);
    if (FAILED(result)) {
        fail(decoder, "the decoder offered no picture type", result);
        return false;
    }
    GUID subtype = {};
    type->GetGUID(MF_MT_SUBTYPE, &subtype);
    if (!IsEqualGUID(subtype, MFVideoFormat_NV12)) {
        type->Release();
        fail_text(decoder, "the decoder's picture is not 8-bit NV12");
        return false;
    }
    result = decoder->decoder->SetOutputType(0, type, 0);
    if (FAILED(result)) {
        type->Release();
        fail(decoder, "the decoder wouldn't take its own picture type", result);
        return false;
    }
    if (decoder->output_type) decoder->output_type->Release();
    decoder->output_type = type;
    read_colour(decoder, type);
    UINT32 width = 0;
    UINT32 height = 0;
    MFGetAttributeSize(type, MF_MT_FRAME_SIZE, &width, &height);
    decoder->width = static_cast<int>(width);
    decoder->height = static_cast<int>(height);
    // What of that surface is the picture: a decoder codes 180 lines in a 192
    // line surface, and the rest is padding the picture is not. The decoder
    // says which part, in the aperture its output type carries; a type that
    // says nothing is taken whole.
    decoder->visible = {0, 0, static_cast<LONG>(width), static_cast<LONG>(height)};
    MFVideoArea area = {};
    UINT32 got = 0;
    if (SUCCEEDED(type->GetBlob(MF_MT_MINIMUM_DISPLAY_APERTURE, reinterpret_cast<UINT8 *>(&area), sizeof area, &got)) &&
        area.Area.cx > 0 && area.Area.cy > 0) {
        decoder->visible = {static_cast<LONG>(area.OffsetX.value), static_cast<LONG>(area.OffsetY.value),
                            static_cast<LONG>(area.OffsetX.value) + static_cast<LONG>(area.Area.cx),
                            static_cast<LONG>(area.OffsetY.value) + static_cast<LONG>(area.Area.cy)};
    }
    return true;
}

// The reason a decoder that never got made wasn't: there is no decoder to keep
// it in, and the caller asks for it by the null it got back.
thread_local char g_error[256] = {0};

// Lets a decoder that couldn't be made go, keeping its reason for the caller
// that gets the null back.
Decoder *abandon(Decoder *decoder) {
    std::snprintf(g_error, sizeof g_error, "%s", decoder->error);
    release_all(decoder);
    MFShutdown();
    if (decoder->com) CoUninitialize();
    delete decoder;
    return nullptr;
}

}  // namespace

extern "C" {

typedef Decoder yf_hwdec;

// The machine's decoder for `codec` (0 H.264, 1 HEVC, 2 AV1), on the graphics
// card, opening no larger than `hint_width` by `hint_height`; null when the
// machine has none for this codec, or no adapter to make one on.
yf_hwdec *yapfast_hwdec_new(int codec, int hint_width, int hint_height);
// Feeds one whole frame. 1 says a picture is ready, 0 that the decoder kept
// the frame for the pictures after it, -1 that it failed and must be dropped.
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

// Empty the decoder's queue, keeping the newest picture and dropping the ones
// a live stream has no time to show.
bool drain(Decoder *decoder, bool *produced) {
    for (int guard = 0; guard < 32; ++guard) {
        MFT_OUTPUT_DATA_BUFFER buffer = {};
        DWORD status = 0;
        HRESULT result = decoder->decoder->ProcessOutput(0, 1, &buffer, &status);
        // The output buffer's event collection, if the decoder set one, is the
        // caller's to release however the call answered: a stream change or a
        // picture-less answer must not leave it behind, and the need-more-input
        // return must not either.
        if (buffer.pEvents) buffer.pEvents->Release();
        if (result == MF_E_TRANSFORM_NEED_MORE_INPUT) return true;
        if (result == MF_E_TRANSFORM_STREAM_CHANGE || result == MF_E_TRANSFORM_TYPE_NOT_SET) {
            // The stream's sequence header says a size or a kind of picture
            // other than the one it was opened with.
            if (!settle_output(decoder)) return false;
            if (decoder->processor) {
                decoder->processor_width = 0;
                decoder->processor_height = 0;
            }
            continue;
        }
        if (FAILED(result)) {
            fail(decoder, "the decoder couldn't give its picture", result);
            return false;
        }
        if (!buffer.pSample) continue;
        IMFMediaBuffer *media = nullptr;
        IMFDXGIBuffer *surface = nullptr;
        if (SUCCEEDED(buffer.pSample->GetBufferByIndex(0, &media))) media->QueryInterface(IID_PPV_ARGS(&surface));
        if (surface) {
            IUnknown *resource = nullptr;
            if (SUCCEEDED(surface->GetResource(__uuidof(ID3D11Texture2D), reinterpret_cast<void **>(&resource)))) {
                ID3D11Texture2D *texture = static_cast<ID3D11Texture2D *>(resource);
                UINT slice = 0;
                surface->GetSubresourceIndex(&slice);
                if (decoder->picture != texture || decoder->picture_slice != slice) {
                    if (decoder->picture) decoder->picture->Release();
                    decoder->picture = texture;
                    decoder->picture_slice = slice;
                } else {
                    texture->Release();
                }
                D3D11_TEXTURE2D_DESC description = {};
                decoder->picture->GetDesc(&description);
                decoder->width = static_cast<int>(description.Width);
                decoder->height = static_cast<int>(description.Height);
                *produced = true;
            }
            surface->Release();
        }
        if (media) media->Release();
        buffer.pSample->Release();
    }
    return true;
}

}  // namespace

extern "C" yf_hwdec *yapfast_hwdec_new(int codec, int hint_width, int hint_height) {
    auto *decoder = new Decoder();
    decoder->codec = codec;
    HRESULT result = CoInitializeEx(nullptr, COINIT_MULTITHREADED);
    if (result == RPC_E_CHANGED_MODE) {
        fail_text(decoder, "this thread's COM apartment isn't the one Media Foundation needs");
        return abandon(decoder);
    }
    decoder->com = true;
    if (FAILED(MFStartup(MF_VERSION, MFSTARTUP_LITE))) {
        fail_text(decoder, "Media Foundation wouldn't start");
        return abandon(decoder);
    }
    const UINT flags = D3D11_CREATE_DEVICE_BGRA_SUPPORT | D3D11_CREATE_DEVICE_VIDEO_SUPPORT;
    result = D3D11CreateDevice(nullptr, D3D_DRIVER_TYPE_HARDWARE, nullptr, flags, nullptr, 0, D3D11_SDK_VERSION,
                               &decoder->device, nullptr, &decoder->context);
    if (FAILED(result)) {
        fail(decoder, "the graphics card was refused", result);
        return abandon(decoder);
    }
    decoder->device->QueryInterface(IID_PPV_ARGS(&decoder->video_device));
    decoder->context->QueryInterface(IID_PPV_ARGS(&decoder->video_context));
    decoder->context->QueryInterface(IID_PPV_ARGS(&decoder->video_context1));
    if (!decoder->video_device || !decoder->video_context) {
        fail_text(decoder, "the graphics card has no video processor");
        return abandon(decoder);
    }
    UINT token = 0;
    result = MFCreateDXGIDeviceManager(&token, &decoder->manager);
    if (SUCCEEDED(result)) result = decoder->manager->ResetDevice(decoder->device, token);
    if (FAILED(result)) {
        fail(decoder, "the card's device manager wasn't made", result);
        return abandon(decoder);
    }

    decoder->decoder = find_decoder(codec);
    if (!decoder->decoder) {
        fail_text(decoder, "this machine has no hardware decoder for the codec");
        return abandon(decoder);
    }
    IMFAttributes *attributes = nullptr;
    if (SUCCEEDED(decoder->decoder->GetAttributes(&attributes)) && attributes) {
        // The picture stays on the card, and the decoder doesn't hold frames
        // back to reorder them: a watched stream shows the newest, now.
        attributes->SetUINT32(MF_SA_D3D11_AWARE, TRUE);
        attributes->SetUINT32(MF_LOW_LATENCY, TRUE);
        attributes->Release();
    }
    result = decoder->decoder->ProcessMessage(MFT_MESSAGE_SET_D3D_MANAGER, reinterpret_cast<ULONG_PTR>(decoder->manager));
    if (FAILED(result)) {
        fail(decoder, "the decoder wouldn't take the graphics card", result);
        return abandon(decoder);
    }

    // The size the stream is likely to be only shapes the first surfaces: the
    // first frame's sequence header settles the real one, and a change
    // renegotiates through the stream change `drain` answers.
    UINT32 width = static_cast<UINT32>(hint_width) < kMinHintWidth ? kMinHintWidth : static_cast<UINT32>(hint_width);
    UINT32 height = static_cast<UINT32>(hint_height) < kMinHintHeight ? kMinHintHeight : static_cast<UINT32>(hint_height);
    IMFMediaType *input = nullptr;
    result = MFCreateMediaType(&input);
    if (SUCCEEDED(result)) result = input->SetGUID(MF_MT_MAJOR_TYPE, MFMediaType_Video);
    if (SUCCEEDED(result)) result = input->SetGUID(MF_MT_SUBTYPE, subtype_of(codec));
    if (SUCCEEDED(result)) result = MFSetAttributeSize(input, MF_MT_FRAME_SIZE, width, height);
    if (SUCCEEDED(result)) result = MFSetAttributeRatio(input, MF_MT_FRAME_RATE, 30, 1);
    if (SUCCEEDED(result)) result = MFSetAttributeRatio(input, MF_MT_PIXEL_ASPECT_RATIO, 1, 1);
    if (SUCCEEDED(result)) result = input->SetUINT32(MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive);
    if (SUCCEEDED(result)) result = decoder->decoder->SetInputType(0, input, 0);
    if (input) input->Release();
    if (FAILED(result)) {
        fail(decoder, "the decoder wouldn't take the stream's format", result);
        return abandon(decoder);
    }
    // The picture type is settled before the first frame: the machine's decoders
    // answer what they would produce at this size, and the first frame that
    // says otherwise comes back as a stream change.
    if (!settle_output(decoder)) {
        return abandon(decoder);
    }
    decoder->decoder->ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0);
    decoder->decoder->ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM, 0);
    return decoder;
}

extern "C" int yapfast_hwdec_decode(yf_hwdec *decoder, const unsigned char *data, size_t size, int keyframe) {
    if (!decoder || !decoder->decoder) return -1;
    decoder->error[0] = 0;
    IMFSample *sample = nullptr;
    IMFMediaBuffer *buffer = nullptr;
    HRESULT result = MFCreateSample(&sample);
    if (SUCCEEDED(result)) result = MFCreateMemoryBuffer(static_cast<DWORD>(size), &buffer);
    if (SUCCEEDED(result)) {
        BYTE *pixels = nullptr;
        result = buffer->Lock(&pixels, nullptr, nullptr);
        if (SUCCEEDED(result)) {
            std::memcpy(pixels, data, size);
            buffer->Unlock();
            result = buffer->SetCurrentLength(static_cast<DWORD>(size));
        }
    }
    if (SUCCEEDED(result)) result = sample->AddBuffer(buffer);
    if (buffer) buffer->Release();
    if (FAILED(result)) {
        if (sample) sample->Release();
        fail(decoder, "the frame wouldn't be handed over", result);
        return -1;
    }
    // A time for every sample, uneven by a frame's worth, since a decoder
    // ordering pictures needs them in time order; the value itself is never
    // shown, and a live stream has no clock to keep.
    sample->SetSampleTime(decoder->time);
    sample->SetSampleDuration(333333);
    decoder->time += 333333;
    if (keyframe) sample->SetUINT32(MFSampleExtension_CleanPoint, TRUE);

    bool produced = false;
    // Nothing waits for a picture here: a decoder that wants room first gets
    // what it has taken so far taken off it.
    if (!drain(decoder, &produced)) {
        sample->Release();
        return -1;
    }
    result = decoder->decoder->ProcessInput(0, sample, 0);
    if (result == MF_E_NOTACCEPTING) {
        if (!drain(decoder, &produced)) {
            sample->Release();
            return -1;
        }
        result = decoder->decoder->ProcessInput(0, sample, 0);
    }
    sample->Release();
    if (FAILED(result)) {
        fail(decoder, "the decoder refused a frame", result);
        return -1;
    }
    if (!drain(decoder, &produced)) return -1;
    return produced ? 1 : 0;
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
    if (decoder->decoder) decoder->decoder->ProcessMessage(MFT_MESSAGE_NOTIFY_END_OF_STREAM, 0);
    release_all(decoder);
    MFShutdown();
    if (decoder->com) CoUninitialize();
    delete decoder;
}
