/* Decoding H265 someone streams through Go Live on macOS with Apple's
 * VideoToolbox, matching the interface of hevc.c: whole Annex-B access units
 * in, I420 planes out, valid until the next call on the decoder. */
#include <CoreFoundation/CoreFoundation.h>
#include <CoreMedia/CoreMedia.h>
#include <CoreVideo/CoreVideo.h>
#include <VideoToolbox/VideoToolbox.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

enum { MAX_SIDE = 8192 };

struct yapfast_picture {
  int width;
  int height;
  const unsigned char *y;
  const unsigned char *u;
  const unsigned char *v;
  int y_stride;
  int u_stride;
  int v_stride;
};

struct param_buf {
  unsigned char *data;
  size_t size;
};

struct yapfast_hevc {
  VTDecompressionSessionRef session;
  CMVideoFormatDescriptionRef fmt_desc;
  struct param_buf vps;
  struct param_buf sps;
  struct param_buf pps;
  unsigned char *avcc;
  size_t avcc_cap;
  CVPixelBufferRef pending_frame;
  CVPixelBufferRef locked_frame;
  unsigned char *u_buf;
  unsigned char *v_buf;
  size_t uv_cap;
};

static void release_locked_frame(struct yapfast_hevc *decoder) {
  if (decoder->locked_frame) {
    CVPixelBufferUnlockBaseAddress(decoder->locked_frame, kCVPixelBufferLock_ReadOnly);
    CVPixelBufferRelease(decoder->locked_frame);
    decoder->locked_frame = NULL;
  }
}

static void release_session(struct yapfast_hevc *decoder) {
  if (decoder->session) {
    VTDecompressionSessionWaitForAsynchronousFrames(decoder->session);
    VTDecompressionSessionInvalidate(decoder->session);
    CFRelease(decoder->session);
    decoder->session = NULL;
  }
  if (decoder->fmt_desc) {
    CFRelease(decoder->fmt_desc);
    decoder->fmt_desc = NULL;
  }
}

void yapfast_hevc_free(struct yapfast_hevc *decoder) {
  if (!decoder) return;
  release_locked_frame(decoder);
  if (decoder->pending_frame) {
    CVPixelBufferRelease(decoder->pending_frame);
    decoder->pending_frame = NULL;
  }
  release_session(decoder);
  free(decoder->vps.data);
  free(decoder->sps.data);
  free(decoder->pps.data);
  free(decoder->avcc);
  free(decoder->u_buf);
  free(decoder->v_buf);
  free(decoder);
}

struct yapfast_hevc *yapfast_hevc_new(unsigned threads) {
  (void)threads;
  return calloc(1, sizeof(struct yapfast_hevc));
}

static void vt_output_callback(void *decompressionOutputRefCon, void *sourceFrameRefCon,
                               OSStatus status, VTDecodeInfoFlags infoFlags,
                               CVImageBufferRef imageBuffer, CMTime presentationTimeStamp,
                               CMTime presentationDuration) {
  (void)sourceFrameRefCon;
  (void)infoFlags;
  (void)presentationTimeStamp;
  (void)presentationDuration;
  struct yapfast_hevc *decoder = decompressionOutputRefCon;
  if (status != noErr || !imageBuffer) return;
  if (decoder->pending_frame) {
    CVPixelBufferRelease(decoder->pending_frame);
  }
  decoder->pending_frame = (CVPixelBufferRef)CVPixelBufferRetain(imageBuffer);
}

static int update_param(struct param_buf *dst, const unsigned char *src, size_t size, int *changed) {
  if (dst->size == size && (size == 0 || memcmp(dst->data, src, size) == 0)) {
    return 0;
  }
  unsigned char *copy = malloc(size);
  if (!copy) return -1;
  memcpy(copy, src, size);
  free(dst->data);
  dst->data = copy;
  dst->size = size;
  *changed = 1;
  return 0;
}

static int ensure_session(struct yapfast_hevc *decoder) {
  if (!decoder->vps.data || !decoder->sps.data || !decoder->pps.data) {
    return 0;
  }
  release_session(decoder);

  const uint8_t *ptrs[3] = {decoder->vps.data, decoder->sps.data, decoder->pps.data};
  size_t sizes[3] = {decoder->vps.size, decoder->sps.size, decoder->pps.size};
  OSStatus status = CMVideoFormatDescriptionCreateFromHEVCParameterSets(
      kCFAllocatorDefault, 3, ptrs, sizes, 4, NULL, &decoder->fmt_desc);
  if (status != noErr || !decoder->fmt_desc) {
    return -1;
  }

  int32_t pix_fmt = kCVPixelFormatType_420YpCbCr8Planar;
  CFNumberRef fmt_num = CFNumberCreate(kCFAllocatorDefault, kCFNumberSInt32Type, &pix_fmt);
  const void *keys[1] = {kCVPixelBufferPixelFormatTypeKey};
  const void *vals[1] = {fmt_num};
  CFDictionaryRef attrs = CFDictionaryCreate(
      kCFAllocatorDefault, keys, vals, 1,
      &kCFTypeDictionaryKeyCallBacks, &kCFTypeDictionaryValueCallBacks);
  CFRelease(fmt_num);

  VTDecompressionOutputCallbackRecord cb = {
      .decompressionOutputCallback = vt_output_callback,
      .decompressionOutputRefCon = decoder,
  };
  status = VTDecompressionSessionCreate(
      kCFAllocatorDefault, decoder->fmt_desc, NULL, attrs, &cb, &decoder->session);
  CFRelease(attrs);
  if (status != noErr || !decoder->session) {
    release_session(decoder);
    return -1;
  }
  return 1;
}

static const unsigned char *find_start_code(const unsigned char *p, const unsigned char *end,
                                            size_t *sc_len) {
  while (p + 3 <= end) {
    if (p[0] == 0 && p[1] == 0) {
      if (p[2] == 1) {
        *sc_len = 3;
        return p;
      }
      if (p + 4 <= end && p[2] == 0 && p[3] == 1) {
        *sc_len = 4;
        return p;
      }
    }
    p++;
  }
  return NULL;
}

static int append_avcc_nal(struct yapfast_hevc *decoder, size_t *avcc_len,
                           const unsigned char *nal, size_t nal_size) {
  if (nal_size > 0x7fffffffU) return -1;
  size_t needed = *avcc_len + 4 + nal_size;
  if (needed > decoder->avcc_cap) {
    size_t new_cap = decoder->avcc_cap ? decoder->avcc_cap : 65536;
    while (new_cap < needed) new_cap *= 2;
    unsigned char *grown = realloc(decoder->avcc, new_cap);
    if (!grown) return -1;
    decoder->avcc = grown;
    decoder->avcc_cap = new_cap;
  }
  uint32_t len = (uint32_t)nal_size;
  unsigned char *dst = decoder->avcc + *avcc_len;
  dst[0] = (unsigned char)((len >> 24) & 0xff);
  dst[1] = (unsigned char)((len >> 16) & 0xff);
  dst[2] = (unsigned char)((len >> 8) & 0xff);
  dst[3] = (unsigned char)(len & 0xff);
  memcpy(dst + 4, nal, nal_size);
  *avcc_len = needed;
  return 0;
}

int yapfast_hevc_decode(struct yapfast_hevc *decoder, const unsigned char *data, size_t size,
                        struct yapfast_picture *picture) {
  if (!decoder || !data || !size || !picture) return -1;
  release_locked_frame(decoder);
  if (decoder->pending_frame) {
    CVPixelBufferRelease(decoder->pending_frame);
    decoder->pending_frame = NULL;
  }

  const unsigned char *cur = data;
  const unsigned char *end = data + size;
  size_t sc_len = 0;
  const unsigned char *nal_start = find_start_code(cur, end, &sc_len);
  if (!nal_start) return -1;

  int params_changed = 0;
  size_t avcc_len = 0;
  int has_vcl = 0;

  while (nal_start) {
    const unsigned char *payload = nal_start + sc_len;
    size_t next_sc_len = 0;
    const unsigned char *next_sc = find_start_code(payload, end, &next_sc_len);
    const unsigned char *nal_end = next_sc ? next_sc : end;
    while (nal_end > payload && nal_end[-1] == 0) {
      /* Strip trailing zero bytes only if followed by another start code;
       * keeping it simple: Annex-B zero padding before next start code. */
      if (!next_sc) break;
      nal_end--;
    }
    size_t nal_size = (size_t)(nal_end - payload);
    if (nal_size >= 2) {
      int nal_type = (payload[0] >> 1) & 0x3f;
      if (nal_type == 32) {
        if (update_param(&decoder->vps, payload, nal_size, &params_changed) < 0) return -3;
      } else if (nal_type == 33) {
        if (update_param(&decoder->sps, payload, nal_size, &params_changed) < 0) return -3;
      } else if (nal_type == 34) {
        if (update_param(&decoder->pps, payload, nal_size, &params_changed) < 0) return -3;
      } else {
        if (nal_type < 32) has_vcl = 1;
        if (append_avcc_nal(decoder, &avcc_len, payload, nal_size) < 0) return -3;
      }
    }
    nal_start = next_sc;
    sc_len = next_sc_len;
  }

  if (params_changed || !decoder->session) {
    int ready = ensure_session(decoder);
    if (ready < 0) return -1;
  }
  if (!decoder->session || !has_vcl || avcc_len == 0) {
    return 0;
  }

  CMBlockBufferRef block = NULL;
  OSStatus status = CMBlockBufferCreateWithMemoryBlock(
      kCFAllocatorDefault, decoder->avcc, avcc_len, kCFAllocatorNull,
      NULL, 0, avcc_len, 0, &block);
  if (status != noErr || !block) return -1;

  CMSampleBufferRef sample = NULL;
  size_t sample_size = avcc_len;
  status = CMSampleBufferCreateReady(
      kCFAllocatorDefault, block, decoder->fmt_desc, 1, 0, NULL, 1, &sample_size, &sample);
  CFRelease(block);
  if (status != noErr || !sample) return -1;

  status = VTDecompressionSessionDecodeFrame(decoder->session, sample, 0, NULL, NULL);
  CFRelease(sample);
  if (status != noErr) return -1;

  if (!decoder->pending_frame) return 0;

  CVPixelBufferRef pb = decoder->pending_frame;
  decoder->pending_frame = NULL;

  int width = (int)CVPixelBufferGetWidth(pb);
  int height = (int)CVPixelBufferGetHeight(pb);
  if (width <= 0 || height <= 0 || width > MAX_SIDE || height > MAX_SIDE) {
    CVPixelBufferRelease(pb);
    return -2;
  }

  if (CVPixelBufferLockBaseAddress(pb, kCVPixelBufferLock_ReadOnly) != kCVReturnSuccess) {
    CVPixelBufferRelease(pb);
    return -1;
  }

  size_t planes = CVPixelBufferGetPlaneCount(pb);
  if (planes >= 3) {
    decoder->locked_frame = pb;
    picture->width = width;
    picture->height = height;
    picture->y = CVPixelBufferGetBaseAddressOfPlane(pb, 0);
    picture->u = CVPixelBufferGetBaseAddressOfPlane(pb, 1);
    picture->v = CVPixelBufferGetBaseAddressOfPlane(pb, 2);
    picture->y_stride = (int)CVPixelBufferGetBytesPerRowOfPlane(pb, 0);
    picture->u_stride = (int)CVPixelBufferGetBytesPerRowOfPlane(pb, 1);
    picture->v_stride = (int)CVPixelBufferGetBytesPerRowOfPlane(pb, 2);
    return 1;
  }

  if (planes == 2) {
    int chroma_w = (width + 1) / 2;
    int chroma_h = (height + 1) / 2;
    size_t needed_uv = (size_t)chroma_w * (size_t)chroma_h;
    if (decoder->uv_cap < needed_uv) {
      unsigned char *u_new = realloc(decoder->u_buf, needed_uv);
      unsigned char *v_new = realloc(decoder->v_buf, needed_uv);
      if (!u_new || !v_new) {
        free(u_new ? u_new : decoder->u_buf);
        free(v_new ? v_new : decoder->v_buf);
        decoder->u_buf = NULL;
        decoder->v_buf = NULL;
        decoder->uv_cap = 0;
        CVPixelBufferUnlockBaseAddress(pb, kCVPixelBufferLock_ReadOnly);
        CVPixelBufferRelease(pb);
        return -3;
      }
      decoder->u_buf = u_new;
      decoder->v_buf = v_new;
      decoder->uv_cap = needed_uv;
    }
    const unsigned char *uv = CVPixelBufferGetBaseAddressOfPlane(pb, 1);
    size_t uv_stride = CVPixelBufferGetBytesPerRowOfPlane(pb, 1);
    for (int row = 0; row < chroma_h; row++) {
      const unsigned char *src = uv + (size_t)row * uv_stride;
      unsigned char *du = decoder->u_buf + (size_t)row * (size_t)chroma_w;
      unsigned char *dv = decoder->v_buf + (size_t)row * (size_t)chroma_w;
      for (int col = 0; col < chroma_w; col++) {
        du[col] = src[col * 2];
        dv[col] = src[col * 2 + 1];
      }
    }
    decoder->locked_frame = pb;
    picture->width = width;
    picture->height = height;
    picture->y = CVPixelBufferGetBaseAddressOfPlane(pb, 0);
    picture->u = decoder->u_buf;
    picture->v = decoder->v_buf;
    picture->y_stride = (int)CVPixelBufferGetBytesPerRowOfPlane(pb, 0);
    picture->u_stride = chroma_w;
    picture->v_stride = chroma_w;
    return 1;
  }

  CVPixelBufferUnlockBaseAddress(pb, kCVPixelBufferLock_ReadOnly);
  CVPixelBufferRelease(pb);
  return -2;
}
