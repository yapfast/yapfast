/* See vtdec.h. A VTDecompressionSession decodes each access unit while the
 * call waits, so frames arrive in decoding order; those a later frame must
 * precede are held until no later packet can put one before them. */
#include "vtdec.h"

#include <CoreFoundation/CoreFoundation.h>
#include <CoreMedia/CoreMedia.h>
#include <CoreVideo/CoreVideo.h>
#include <VideoToolbox/VideoToolbox.h>
#include <stdlib.h>
#include <string.h>

#include "libyuv/convert.h"

/* NAL unit types of H264. */
enum { NAL_SLICE = 1, NAL_IDR = 5, NAL_SPS = 7, NAL_PPS = 8 };

/* The most pictures held for reordering: H264 allows a decoder 16. */
enum { MAX_HELD = 16 };

/* The widest or tallest picture Yapfast decodes, as in decode.c and video.c. */
enum { MAX_SIDE = 8192 };

/* A parameter set's size is stored in 16 bits in a file, and no real one is near. */
enum { MAX_PARAMETER_SET = 1024 };

/* The length prefix of NAL units in the samples this builds from Annex B. */
enum { ANNEXB_PREFIX = 4 };

/* A decoded picture as I420 in one allocation. */
struct held {
  int width;
  int height;
  int bt709;
  int full_range;
  int64_t pts;
  unsigned char *data;
};

struct yapfast_vt {
  VTDecompressionSessionRef session;
  CMVideoFormatDescriptionRef format;
  unsigned char sps[MAX_PARAMETER_SET];
  size_t sps_size;
  unsigned char pps[MAX_PARAMETER_SET];
  size_t pps_size;
  /* The bytes of NAL unit lengths in the packets: 4 for Annex B input. */
  int length_size;
  /* The packet being decoded, which the callback files its picture under. */
  int64_t decoding_pts;
  int decode_failed;
  struct held held[MAX_HELD];
  int held_count;
  /* Pictures with a time up to this one can't be preceded by a later packet's. */
  int64_t safe_until;
  int draining;
  /* The picture given last, kept until the next call. */
  struct held shown;
};

static void free_held(struct held *held) {
  free(held->data);
  *held = (struct held){0};
}

static void close_session(struct yapfast_vt *vt) {
  if (vt->session) {
    VTDecompressionSessionInvalidate(vt->session);
    CFRelease(vt->session);
    vt->session = NULL;
  }
  if (vt->format) {
    CFRelease(vt->format);
    vt->format = NULL;
  }
}

static void clear_held(struct yapfast_vt *vt) {
  for (int index = 0; index < vt->held_count; index++) free_held(&vt->held[index]);
  vt->held_count = 0;
}

struct yapfast_vt *yapfast_vt_new(void) {
  struct yapfast_vt *vt = calloc(1, sizeof *vt);
  if (vt) {
    vt->length_size = ANNEXB_PREFIX;
    vt->safe_until = INT64_MIN;
  }
  return vt;
}

void yapfast_vt_free(struct yapfast_vt *vt) {
  if (!vt) return;
  close_session(vt);
  clear_held(vt);
  free_held(&vt->shown);
  free(vt);
}

/* ---- Pictures ---- */

/* Whether the pixel buffer's colours are BT.709 (1), BT.601 (0) or unsaid (-1). */
static int matrix_of(CVImageBufferRef image) {
  CFTypeRef matrix = CVBufferCopyAttachment(image, kCVImageBufferYCbCrMatrixKey, NULL);
  if (!matrix) return -1;
  int result = -1;
  if (CFEqual(matrix, kCVImageBufferYCbCrMatrix_ITU_R_709_2)) {
    result = 1;
  } else if (CFEqual(matrix, kCVImageBufferYCbCrMatrix_ITU_R_601_4)) {
    result = 0;
  }
  CFRelease(matrix);
  return result;
}

/* Copies the decoded picture, which is NV12, into `held` as I420. */
static int copy_picture(CVImageBufferRef image, int64_t pts, struct held *held) {
  OSType format = CVPixelBufferGetPixelFormatType(image);
  int full_range = format == kCVPixelFormatType_420YpCbCr8BiPlanarFullRange;
  if (!full_range && format != kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange) return -1;
  int width = (int)CVPixelBufferGetWidth(image);
  int height = (int)CVPixelBufferGetHeight(image);
  if (width <= 0 || height <= 0 || width > MAX_SIDE || height > MAX_SIDE) return -1;
  size_t chroma_width = ((size_t)width + 1) / 2;
  size_t chroma_height = ((size_t)height + 1) / 2;
  size_t size = (size_t)width * (size_t)height + 2 * chroma_width * chroma_height;
  unsigned char *data = malloc(size);
  if (!data) return -1;
  if (CVPixelBufferLockBaseAddress(image, kCVPixelBufferLock_ReadOnly) != kCVReturnSuccess) {
    free(data);
    return -1;
  }
  unsigned char *y = data;
  unsigned char *u = y + (size_t)width * (size_t)height;
  unsigned char *v = u + chroma_width * chroma_height;
  int converted = NV12ToI420(
      CVPixelBufferGetBaseAddressOfPlane(image, 0), (int)CVPixelBufferGetBytesPerRowOfPlane(image, 0),
      CVPixelBufferGetBaseAddressOfPlane(image, 1), (int)CVPixelBufferGetBytesPerRowOfPlane(image, 1),
      y, width, u, (int)chroma_width, v, (int)chroma_width, width, height);
  CVPixelBufferUnlockBaseAddress(image, kCVPixelBufferLock_ReadOnly);
  if (converted != 0) {
    free(data);
    return -1;
  }
  *held = (struct held){.width = width,
                        .height = height,
                        .bt709 = matrix_of(image),
                        .full_range = full_range,
                        .pts = pts,
                        .data = data};
  return 0;
}

/* VideoToolbox's callback for each decoded frame. */
static void decoded(void *refcon, void *frame_refcon, OSStatus status, VTDecodeInfoFlags flags,
                    CVImageBufferRef image, CMTime pts, CMTime duration) {
  (void)frame_refcon;
  (void)flags;
  (void)pts;
  (void)duration;
  struct yapfast_vt *vt = refcon;
  if (status != noErr || !image) {
    vt->decode_failed = 1;
    return;
  }
  if (vt->held_count == MAX_HELD) {
    /* A stream that reorders deeper than H264 allows: show the earliest now. */
    int earliest = 0;
    for (int index = 1; index < vt->held_count; index++) {
      if (vt->held[index].pts < vt->held[earliest].pts) earliest = index;
    }
    free_held(&vt->held[earliest]);
    vt->held[earliest] = vt->held[--vt->held_count];
  }
  struct held *held = &vt->held[vt->held_count];
  if (copy_picture(image, vt->decoding_pts, held) == 0) {
    vt->held_count++;
  } else {
    vt->decode_failed = 1;
  }
}

int yapfast_vt_next(struct yapfast_vt *vt, struct yapfast_vt_picture *picture) {
  int earliest = -1;
  for (int index = 0; index < vt->held_count; index++) {
    if (earliest < 0 || vt->held[index].pts < vt->held[earliest].pts) earliest = index;
  }
  if (earliest < 0) return 0;
  if (!vt->draining && vt->held[earliest].pts > vt->safe_until) return 0;
  free_held(&vt->shown);
  vt->shown = vt->held[earliest];
  vt->held[earliest] = vt->held[--vt->held_count];
  const struct held *shown = &vt->shown;
  size_t luma = (size_t)shown->width * (size_t)shown->height;
  int chroma_width = (shown->width + 1) / 2;
  int chroma_height = (shown->height + 1) / 2;
  picture->width = shown->width;
  picture->height = shown->height;
  picture->y = shown->data;
  picture->u = shown->data + luma;
  picture->v = picture->u + (size_t)chroma_width * (size_t)chroma_height;
  picture->y_stride = shown->width;
  picture->u_stride = chroma_width;
  picture->v_stride = chroma_width;
  picture->pts = shown->pts;
  picture->bt709 = shown->bt709;
  picture->full_range = shown->full_range;
  return 1;
}

void yapfast_vt_drain(struct yapfast_vt *vt) { vt->draining = 1; }

void yapfast_vt_reset(struct yapfast_vt *vt) {
  clear_held(vt);
  free_held(&vt->shown);
  vt->draining = 0;
  vt->safe_until = INT64_MIN;
  /* The session keeps its parameter sets; it waits for the next keyframe. */
}

/* ---- Session ---- */

/* Makes the session for the parameter sets held, which a new picture size or
 * profile needs again. */
static int open_session(struct yapfast_vt *vt) {
  close_session(vt);
  const uint8_t *sets[2] = {vt->sps, vt->pps};
  const size_t sizes[2] = {vt->sps_size, vt->pps_size};
  if (CMVideoFormatDescriptionCreateFromH264ParameterSets(
          kCFAllocatorDefault, 2, sets, sizes, vt->length_size, &vt->format) != noErr) {
    return -1;
  }
  /* Pictures as NV12, which every VideoToolbox decoder makes without converting. */
  int pixel_format = kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange;
  CFNumberRef format_number = CFNumberCreate(NULL, kCFNumberIntType, &pixel_format);
  const void *keys[] = {kCVPixelBufferPixelFormatTypeKey};
  const void *values[] = {format_number};
  CFDictionaryRef attributes = CFDictionaryCreate(
      NULL, keys, values, 1, &kCFTypeDictionaryKeyCallBacks, &kCFTypeDictionaryValueCallBacks);
  CFRelease(format_number);
  const void *spec_keys[] = {kVTVideoDecoderSpecification_EnableHardwareAcceleratedVideoDecoder};
  const void *spec_values[] = {kCFBooleanTrue};
  CFDictionaryRef specification = CFDictionaryCreate(
      NULL, spec_keys, spec_values, 1, &kCFTypeDictionaryKeyCallBacks,
      &kCFTypeDictionaryValueCallBacks);
  VTDecompressionOutputCallbackRecord callback = {.decompressionOutputCallback = decoded,
                                                  .decompressionOutputRefCon = vt};
  OSStatus status = VTDecompressionSessionCreate(kCFAllocatorDefault, vt->format, specification,
                                                 attributes, &callback, &vt->session);
  CFRelease(attributes);
  CFRelease(specification);
  if (status != noErr) {
    close_session(vt);
    return -1;
  }
  return 0;
}

/* Keeps a parameter set, and says whether it is new. */
static int keep_set(unsigned char *kept, size_t *kept_size, const unsigned char *set, size_t size) {
  if (size == 0 || size > MAX_PARAMETER_SET) return 0;
  if (*kept_size == size && memcmp(kept, set, size) == 0) return 0;
  memcpy(kept, set, size);
  *kept_size = size;
  return 1;
}

int yapfast_vt_set_avcc(struct yapfast_vt *vt, const unsigned char *avcc, size_t size) {
  /* configurationVersion, profile, compatibility, level, length size, SPS count. */
  if (size < 7 || avcc[0] != 1) return -1;
  int length_size = (avcc[4] & 3) + 1;
  size_t at = 6;
  if ((avcc[5] & 0x1f) < 1 || at + 2 > size) return -1;
  size_t sps_size = ((size_t)avcc[at] << 8) | avcc[at + 1];
  at += 2;
  if (at + sps_size + 1 > size) return -1;
  const unsigned char *sps = avcc + at;
  at += sps_size;
  if (avcc[at] < 1 || at + 3 > size) return -1;
  at += 1;
  size_t pps_size = ((size_t)avcc[at] << 8) | avcc[at + 1];
  at += 2;
  if (at + pps_size > size) return -1;
  keep_set(vt->sps, &vt->sps_size, sps, sps_size);
  keep_set(vt->pps, &vt->pps_size, avcc + at, pps_size);
  if (vt->sps_size == 0 || vt->pps_size == 0) return -1;
  vt->length_size = length_size;
  return open_session(vt);
}

/* ---- Decoding ---- */

/* Hands VideoToolbox `data`, `size` bytes of length prefixed NAL units, as one
 * sample, and waits for it to be decoded. */
static int decode_sample(struct yapfast_vt *vt, const unsigned char *data, size_t size,
                         int64_t pts, int64_t dts) {
  if (!vt->session || size == 0) return 0;
  CMBlockBufferRef block = NULL;
  if (CMBlockBufferCreateWithMemoryBlock(kCFAllocatorDefault, NULL, size, kCFAllocatorDefault,
                                         NULL, 0, size, kCMBlockBufferAssureMemoryNowFlag,
                                         &block) != noErr) {
    return -1;
  }
  if (CMBlockBufferReplaceDataBytes(data, block, 0, size) != noErr) {
    CFRelease(block);
    return -1;
  }
  CMSampleBufferRef sample = NULL;
  OSStatus status = CMSampleBufferCreateReady(kCFAllocatorDefault, block, vt->format, 1, 0, NULL,
                                              1, &size, &sample);
  CFRelease(block);
  if (status != noErr) return -1;
  vt->decoding_pts = pts;
  vt->decode_failed = 0;
  status = VTDecompressionSessionDecodeFrame(vt->session, sample, 0, NULL, NULL);
  CFRelease(sample);
  /* Frames before this packet's decoding time can't be preceded by a later one. */
  vt->safe_until = dts == YAPFAST_VT_NO_TIME ? pts : dts;
  if (status != noErr || vt->decode_failed) {
    /* A session the system ended, or a stream it can't follow, starts again
     * with the next parameter sets. */
    if (status == kVTInvalidSessionErr || status == kVTVideoDecoderMalfunctionErr) {
      open_session(vt);
    }
    return -1;
  }
  return 0;
}

int yapfast_vt_decode_avcc(struct yapfast_vt *vt, const unsigned char *data, size_t size,
                           int64_t pts, int64_t dts) {
  return decode_sample(vt, data, size, pts, dts);
}

/* The length of the start code at `at`, 3 or 4, or 0. */
static size_t start_code(const unsigned char *data, size_t size, size_t at) {
  if (at + 3 <= size && data[at] == 0 && data[at + 1] == 0 && data[at + 2] == 1) return 3;
  if (at + 4 <= size && data[at] == 0 && data[at + 1] == 0 && data[at + 2] == 0 &&
      data[at + 3] == 1) {
    return 4;
  }
  return 0;
}

/* Where the NAL unit after the start code at `at` ends. */
static size_t nal_end(const unsigned char *data, size_t size, size_t from) {
  for (size_t at = from; at + 3 <= size; at++) {
    if (start_code(data, size, at)) return at;
  }
  return size;
}

int yapfast_vt_decode_annexb(struct yapfast_vt *vt, const unsigned char *data, size_t size,
                             int64_t pts, int64_t dts) {
  /* Each NAL unit's 4 byte length replaces its start code of 3 or 4 bytes. A
   * NAL unit with its start code takes at least 4 bytes, so a third more
   * room is plenty. */
  unsigned char *sample = malloc(size + size / 3 + ANNEXB_PREFIX);
  if (!sample) return -1;
  size_t used = 0;
  int new_sets = 0;
  size_t at = 0;
  while (at < size) {
    size_t code = start_code(data, size, at);
    if (!code) {
      at++;
      continue;
    }
    size_t begin = at + code;
    size_t end = nal_end(data, size, begin);
    /* Trailing zeros of a 4 byte start code belong to the next one. */
    while (end > begin && data[end - 1] == 0) end--;
    if (end > begin) {
      const unsigned char *nal = data + begin;
      size_t length = end - begin;
      int type = nal[0] & 0x1f;
      if (type == NAL_SPS) {
        new_sets |= keep_set(vt->sps, &vt->sps_size, nal, length);
      } else if (type == NAL_PPS) {
        new_sets |= keep_set(vt->pps, &vt->pps_size, nal, length);
      } else if (type == NAL_SLICE || type == NAL_IDR) {
        sample[used] = (unsigned char)(length >> 24);
        sample[used + 1] = (unsigned char)(length >> 16);
        sample[used + 2] = (unsigned char)(length >> 8);
        sample[used + 3] = (unsigned char)length;
        memcpy(sample + used + ANNEXB_PREFIX, nal, length);
        used += ANNEXB_PREFIX + length;
      }
    }
    at = end > begin ? end : begin;
  }
  int result = 0;
  if ((new_sets || !vt->session) && vt->sps_size > 0 && vt->pps_size > 0) {
    vt->length_size = ANNEXB_PREFIX;
    result = open_session(vt);
  }
  if (result == 0) result = decode_sample(vt, sample, used, pts, dts);
  free(sample);
  return result;
}
